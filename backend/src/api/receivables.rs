//! Commercial Receivables HTTP API Endpoints (Feature 19)

use axum::{
    extract::State,
    routing::get,
    Json, Router,
};

use crate::api::AppState;
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::service::invoice_service::{AgingReportResponse, ReceivablesListResponse};

pub fn receivables_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_receivables_handler))
        .route("/aging", get(get_aging_report_handler))
}

async fn list_receivables_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<Json<ReceivablesListResponse>, AppError> {
    let service = state.invoice_service();
    let res = service.list_receivables(&ctx).await?;
    Ok(Json(res))
}

async fn get_aging_report_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<Json<AgingReportResponse>, AppError> {
    let service = state.invoice_service();
    let res = service.get_aging_report(&ctx).await?;
    Ok(Json(res))
}
