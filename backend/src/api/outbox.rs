//! Transactional Outbox HTTP API Endpoints (Features 22, 23, 24, 25)
//! Scoped event streams, consumer deduplication, and on-demand dispatch processing.

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::api::AppState;
use crate::domain::outbox::OutboxEventDto;
use crate::domain::tenant::TenantContext;
use crate::error::AppError;

pub fn outbox_router() -> Router<AppState> {
    Router::new()
        .route("/events", get(list_outbox_events_handler))
        .route("/process", post(process_outbox_handler))
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutboxQueryFilter {
    pub aggregate_type: Option<String>,
    pub aggregate_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEventsListResponse {
    pub events: Vec<OutboxEventDto>,
    pub count: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProcessOutboxRequest {
    #[serde(default)]
    pub simulate_sidecar_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessOutboxResponse {
    pub processed: usize,
    pub failed: usize,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

async fn list_outbox_events_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Query(filter): Query<OutboxQueryFilter>,
) -> Result<Json<OutboxEventsListResponse>, AppError> {
    let mut query_str = String::from(
        r#"
        SELECT id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, 
               status, retry_count, attempt_count, max_retries, next_retry_at, 
               last_error, error_message, created_at, published_at
        FROM outbox_events
        WHERE tenant_id = ?1
        "#,
    );

    if filter.aggregate_type.is_some() {
        query_str.push_str(" AND aggregate_type = ?2");
    }
    if filter.aggregate_id.is_some() {
        if filter.aggregate_type.is_some() {
            query_str.push_str(" AND aggregate_id = ?3");
        } else {
            query_str.push_str(" AND aggregate_id = ?2");
        }
    }
    if filter.status.is_some() {
        let param_idx = 1
            + filter.aggregate_type.is_some() as usize
            + filter.aggregate_id.is_some() as usize
            + 1;
        query_str.push_str(&format!(" AND status = ?{}", param_idx));
    }

    query_str.push_str(" ORDER BY created_at ASC");

    if let Some(limit) = filter.limit {
        query_str.push_str(&format!(" LIMIT {}", limit));
    }

    let mut query = sqlx::query(&query_str).bind(ctx.tenant_id_str());

    if let Some(ref agg_type) = filter.aggregate_type {
        query = query.bind(agg_type);
    }
    if let Some(ref agg_id) = filter.aggregate_id {
        query = query.bind(agg_id);
    }
    if let Some(ref st) = filter.status {
        query = query.bind(st);
    }

    let rows = query.fetch_all(&state.pool).await.map_err(AppError::from)?;

    let mut events = Vec::with_capacity(rows.len());
    for r in rows {
        let payload_json: String = r.get("payload_json");
        let payload = serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
        let last_err: Option<String> = r.get("last_error");
        let err_msg: Option<String> = r.get("error_message");
        let effective_err = last_err.or(err_msg);

        events.push(OutboxEventDto {
            id: r.get("id"),
            tenant_id: r.get("tenant_id"),
            event_type: r.get("event_type"),
            aggregate_type: r.get("aggregate_type"),
            aggregate_id: r.get("aggregate_id"),
            payload,
            payload_json,
            status: r.get("status"),
            retry_count: r.get("retry_count"),
            attempt_count: r.get("attempt_count"),
            max_retries: r.get("max_retries"),
            next_retry_at: r.get("next_retry_at"),
            last_error: effective_err.clone(),
            error_message: effective_err,
            created_at: r.get("created_at"),
            published_at: r.get("published_at"),
        });
    }

    let count = events.len();
    Ok(Json(OutboxEventsListResponse { events, count }))
}

async fn process_outbox_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    req_opt: Option<Json<ProcessOutboxRequest>>,
) -> Result<Json<ProcessOutboxResponse>, AppError> {
    let sim_fail = req_opt.map(|j| j.simulate_sidecar_failure).unwrap_or(false);
    let processor = state.outbox_processor();
    let res = processor.process_tenant_batch(&ctx.tenant_id_str(), sim_fail).await?;

    if sim_fail {
        Ok(Json(ProcessOutboxResponse {
            processed: 0,
            failed: res.failed,
            status: "retry_scheduled".to_string(),
            message: Some("Sidecar failure isolated; core transaction unaffected".to_string()),
        }))
    } else {
        Ok(Json(ProcessOutboxResponse {
            processed: res.processed,
            failed: res.failed,
            status: "completed".to_string(),
            message: None,
        }))
    }
}
