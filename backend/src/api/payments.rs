//! Commercial Payments HTTP API Endpoints (Features 20, 21)

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use sha2::{Digest, Sha256};

use crate::api::AppState;
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::repository::idempotency_repo::{IdempotencyLockResult, SqlxIdempotencyRepository};
use crate::repository::IdempotencyRepository;
use crate::service::payment_service::{AllocatePaymentRequest, PaymentAllocationResponse};

pub fn payments_router() -> Router<AppState> {
    Router::new().route("/", post(allocate_payment_handler))
}

async fn allocate_payment_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let req: AllocatePaymentRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid payment request JSON: {}", e),
            "INVALID_JSON",
        )
    })?;

    let key_opt = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let idempotency_repo = SqlxIdempotencyRepository::new(state.pool.clone());
    let user_id = ctx.actor_id_str();

    match key_opt {
        None => {
            let res = state.payment_service.allocate_payment(&ctx, req).await?;
            Ok((StatusCode::CREATED, HeaderMap::new(), Json(res)))
        }
        Some(key) => {
            let mut hasher = Sha256::new();
            hasher.update(&body_bytes);
            let request_hash = format!("{:x}", hasher.finalize());

            match idempotency_repo
                .acquire_lock(&user_id, key, &request_hash, 86_400)
                .await?
            {
                IdempotencyLockResult::Cached {
                    response_code,
                    response_body,
                } => {
                    let res: PaymentAllocationResponse = serde_json::from_str(&response_body)
                        .map_err(|e| {
                            AppError::Internal(format!("Failed to parse cached response: {}", e))
                        })?;
                    let mut resp_headers = HeaderMap::new();
                    resp_headers.insert("x-cache-replay", HeaderValue::from_static("true"));
                    let status = StatusCode::from_u16(response_code as u16)
                        .unwrap_or(StatusCode::CREATED);
                    Ok((status, resp_headers, Json(res)))
                }
                IdempotencyLockResult::MismatchedPayload => Err(AppError::Conflict(
                    "Idempotency key reused with different request payload".to_string(),
                    "IDEMPOTENCY_KEY_MISMATCH",
                )),
                IdempotencyLockResult::InProgress => Err(AppError::Conflict(
                    "Operation with this idempotency key is already in progress".to_string(),
                    "IDEMPOTENCY_IN_PROGRESS",
                )),
                IdempotencyLockResult::Acquired => {
                    let res = match state.payment_service.allocate_payment(&ctx, req).await {
                        Ok(res) => res,
                        Err(err) => {
                            let _ = idempotency_repo.release_lock_on_failure(&user_id, key).await;
                            return Err(err);
                        }
                    };

                    let response_body = serde_json::to_string(&res).unwrap_or_default();
                    let _ = idempotency_repo
                        .save_response(
                            &user_id,
                            key,
                            StatusCode::CREATED.as_u16() as i32,
                            &response_body,
                        )
                        .await;

                    Ok((StatusCode::CREATED, HeaderMap::new(), Json(res)))
                }
            }
        }
    }
}
