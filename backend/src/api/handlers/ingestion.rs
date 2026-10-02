use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::api::AppState;
use crate::domain::ingestion::CanonicalTransactionCandidate;
use crate::error::AppError;
use crate::service::ingestion_service::{
    ConfirmCandidateRequest, ConfirmCandidateResponse, GmailPayload, IngestionService,
    NotificationPayload, RejectCandidateResponse, SmsPayload,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

fn require_ingestion_permission(user: &AuthenticatedUser) -> Result<(), AppError> {
    if user.tier.eq_ignore_ascii_case("premium")
        || user.tier.eq_ignore_ascii_case("trialing")
        || user.tier.eq_ignore_ascii_case("active")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Fitur ini memerlukan langganan Premium aktif atau uji coba 3 bulan.".to_string(),
            "FEATURE_LOCKED",
        ))
    }
}

pub async fn ingest_notification(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    Json(payload): Json<NotificationPayload>,
) -> Result<impl IntoResponse, AppError> {
    require_ingestion_permission(&user)?;

    let idempotency_key = headers.get("Idempotency-Key").and_then(|h| h.to_str().ok());

    let service = IngestionService::new(state.pool.clone());
    let response = service
        .process_notification(&user.user_id, idempotency_key, payload)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::OK, Json(response)))
}

pub async fn ingest_sms(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    Json(payload): Json<SmsPayload>,
) -> Result<impl IntoResponse, AppError> {
    require_ingestion_permission(&user)?;

    let idempotency_key = headers.get("Idempotency-Key").and_then(|h| h.to_str().ok());

    let service = IngestionService::new(state.pool.clone());
    let response = service
        .process_sms(&user.user_id, idempotency_key, payload)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::OK, Json(response)))
}

pub async fn ingest_gmail(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    Json(payload): Json<GmailPayload>,
) -> Result<impl IntoResponse, AppError> {
    require_ingestion_permission(&user)?;

    let idempotency_key = headers.get("Idempotency-Key").and_then(|h| h.to_str().ok());

    let service = IngestionService::new(state.pool.clone());
    let response = service
        .process_gmail(&user.user_id, idempotency_key, payload)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::OK, Json(response)))
}

pub async fn list_candidates(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<Vec<CanonicalTransactionCandidate>>, AppError> {
    require_ingestion_permission(&user)?;

    let service = IngestionService::new(state.pool.clone());
    let candidates = service
        .list_pending_candidates(&user.user_id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(candidates))
}

pub async fn confirm_candidate(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<ConfirmCandidateRequest>,
) -> Result<Json<ConfirmCandidateResponse>, AppError> {
    require_ingestion_permission(&user)?;

    let service = IngestionService::new(state.pool.clone());
    let response = service
        .confirm_candidate(&user.user_id, &id, req)
        .await
        .map_err(AppError::from)?;

    Ok(Json(response))
}

pub async fn reject_candidate(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<RejectCandidateResponse>, AppError> {
    require_ingestion_permission(&user)?;

    let service = IngestionService::new(state.pool.clone());
    let response = service
        .reject_candidate(&user.user_id, &id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(response))
}

pub fn ingestion_router() -> Router<AppState> {
    Router::new()
        .route("/notification", post(ingest_notification))
        .route("/sms", post(ingest_sms))
        .route("/gmail", post(ingest_gmail))
        .route("/candidates", get(list_candidates))
        .route("/candidates/{id}/confirm", post(confirm_candidate))
        .route("/candidates/{id}/reject", post(reject_candidate))
}
