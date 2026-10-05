//! Inventory & Stock Mutations API Endpoints (Features 3, 4, 5, 6, 8, 9, 17, Requirement R1, R2)
//!
//! Provides endpoints for stock levels, atomic stock movements, inter-warehouse transfers,
//! and physical cycle count adjustments with idempotency caching.

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::AppState;
use crate::domain::inventory::{
    AdjustStockRequest, CreateMovementRequest, StockItemsListResponse,
    StockMovementsListResponse, TransferStockRequest,
};
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::service::inventory_service::InventoryService;

pub fn inventory_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_stock_handler))
        .route("/stock", get(list_stock_handler))
        .route(
            "/movements",
            get(list_movements_handler).post(create_movement_handler),
        )
        .route("/transfer", post(transfer_handler))
        .route("/transfers", post(transfer_handler))
        .route("/adjust", post(adjust_handler))
        .route("/adjustments", post(adjust_handler))
}

#[derive(Debug, Deserialize)]
pub struct StockQuery {
    pub warehouse_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    #[serde(default)]
    pub low_stock: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct MovementsQuery {
    pub warehouse_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list_stock_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Query(query): Query<StockQuery>,
) -> Result<impl IntoResponse, AppError> {
    let service = InventoryService::new(state.pool);
    let stock_items = service
        .list_stock(
            &ctx,
            query.warehouse_id,
            query.product_id,
            query.low_stock.unwrap_or(false),
        )
        .await?;
    let count = stock_items.len();
    Ok((
        StatusCode::OK,
        Json(StockItemsListResponse { stock_items, count }),
    ))
}

async fn create_movement_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let req: CreateMovementRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid movement request JSON: {}", e),
            "INVALID_JSON",
        )
    })?;

    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let service = InventoryService::new(state.pool);
    let (res, is_replay) = service
        .create_movement(&ctx, req, idempotency_key, Some(&body_bytes))
        .await?;

    let mut resp_headers = HeaderMap::new();
    if is_replay {
        resp_headers.insert("x-cache-replay", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::CREATED, resp_headers, Json(res)))
}

async fn list_movements_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Query(query): Query<MovementsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let service = InventoryService::new(state.pool);
    let movements = service
        .list_movements(
            &ctx,
            query.product_id,
            query.warehouse_id,
            query.limit,
            query.offset,
        )
        .await?;
    let count = movements.len();
    Ok((
        StatusCode::OK,
        Json(StockMovementsListResponse { movements, count }),
    ))
}

async fn transfer_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let req: TransferStockRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid transfer request JSON: {}", e),
            "INVALID_JSON",
        )
    })?;

    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let service = InventoryService::new(state.pool);
    let (res, is_replay) = service
        .transfer_stock(&ctx, req, idempotency_key, Some(&body_bytes))
        .await?;

    let mut resp_headers = HeaderMap::new();
    if is_replay {
        resp_headers.insert("x-cache-replay", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, resp_headers, Json(res)))
}

async fn adjust_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let req: AdjustStockRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid adjustment request JSON: {}", e),
            "INVALID_JSON",
        )
    })?;

    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let service = InventoryService::new(state.pool);
    let (res, is_replay) = service
        .adjust_stock(&ctx, req, idempotency_key, Some(&body_bytes))
        .await?;

    let mut resp_headers = HeaderMap::new();
    if is_replay {
        resp_headers.insert("x-cache-replay", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, resp_headers, Json(res)))
}
