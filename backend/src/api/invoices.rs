//! Commercial Invoices HTTP API Endpoints (Features 16, 17, 18, 21)

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sha2::{Digest, Sha256};

use crate::api::AppState;
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::repository::idempotency_repo::{IdempotencyLockResult, SqlxIdempotencyRepository};
use crate::repository::IdempotencyRepository;
use crate::service::invoice_service::{
    CreateInvoiceRequest, InvoiceResponse, InvoicesListResponse, UpdateInvoiceRequest,
    VoidInvoiceRequest,
};

pub fn invoices_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_invoices_handler).post(create_invoice_handler))
        .route("/{id}", get(get_invoice_handler).put(update_invoice_handler))
        .route("/{id}/issue", post(issue_invoice_handler))
        .route("/{id}/void", post(void_invoice_handler))
}

async fn list_invoices_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<Json<InvoicesListResponse>, AppError> {
    let service = state.invoice_service();
    let res = service.list_invoices(&ctx).await?;
    Ok(Json(res))
}

async fn get_invoice_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<String>,
) -> Result<Json<InvoiceResponse>, AppError> {
    let service = state.invoice_service();
    let res = service.get_invoice(&ctx, &id).await?;
    Ok(Json(res))
}

async fn create_invoice_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let req: CreateInvoiceRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid invoice request JSON: {}", e),
            "INVALID_JSON",
        )
    })?;

    let key_opt = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let service = state.invoice_service();
    let idempotency_repo = SqlxIdempotencyRepository::new(state.pool.clone());
    let user_id = ctx.actor_id_str();

    match key_opt {
        None => {
            let res = service.create_invoice(&ctx, req).await?;
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
                    let res: InvoiceResponse = serde_json::from_str(&response_body).map_err(|e| {
                        AppError::Internal(format!("Failed to parse cached response: {}", e))
                    })?;
                    let mut resp_headers = HeaderMap::new();
                    resp_headers.insert("x-cache-replay", HeaderValue::from_static("true"));
                    let status =
                        StatusCode::from_u16(response_code as u16).unwrap_or(StatusCode::CREATED);
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
                    let res = match service.create_invoice(&ctx, req).await {
                        Ok(res) => res,
                        Err(err) => {
                            let _ = idempotency_repo.release_lock_on_failure(&user_id, key).await;
                            return Err(err);
                        }
                    };

                    let response_body = serde_json::to_string(&res).unwrap_or_default();
                    let _ = idempotency_repo
                        .save_response(&user_id, key, StatusCode::CREATED.as_u16() as i32, &response_body)
                        .await;

                    Ok((StatusCode::CREATED, HeaderMap::new(), Json(res)))
                }
            }
        }
    }
}

async fn update_invoice_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<String>,
    Json(req): Json<UpdateInvoiceRequest>,
) -> Result<Json<InvoiceResponse>, AppError> {
    let service = state.invoice_service();
    let res = service.update_invoice(&ctx, &id, req).await?;
    Ok(Json(res))
}

async fn issue_invoice_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<String>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let key_opt = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let service = state.invoice_service();
    let idempotency_repo = SqlxIdempotencyRepository::new(state.pool.clone());
    let user_id = ctx.actor_id_str();

    match key_opt {
        None => {
            let res = service.issue_invoice(&ctx, &id).await?;
            Ok((StatusCode::OK, HeaderMap::new(), Json(res)))
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
                    let res: InvoiceResponse = serde_json::from_str(&response_body).map_err(|e| {
                        AppError::Internal(format!("Failed to parse cached response: {}", e))
                    })?;
                    let mut resp_headers = HeaderMap::new();
                    resp_headers.insert("x-cache-replay", HeaderValue::from_static("true"));
                    let status =
                        StatusCode::from_u16(response_code as u16).unwrap_or(StatusCode::OK);
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
                    let res = match service.issue_invoice(&ctx, &id).await {
                        Ok(res) => res,
                        Err(err) => {
                            let _ = idempotency_repo.release_lock_on_failure(&user_id, key).await;
                            return Err(err);
                        }
                    };

                    let response_body = serde_json::to_string(&res).unwrap_or_default();
                    let _ = idempotency_repo
                        .save_response(&user_id, key, StatusCode::OK.as_u16() as i32, &response_body)
                        .await;

                    Ok((StatusCode::OK, HeaderMap::new(), Json(res)))
                }
            }
        }
    }
}

async fn void_invoice_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id): Path<String>,
    Json(req): Json<Option<VoidInvoiceRequest>>,
) -> Result<Json<InvoiceResponse>, AppError> {
    let service = state.invoice_service();
    let void_req = req.unwrap_or(VoidInvoiceRequest {
        reason: "Voided".to_string(),
    });
    let res = service.void_invoice(&ctx, &id, void_req).await?;
    Ok(Json(res))
}
