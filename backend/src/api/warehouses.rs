//! Warehouses API Endpoints (Feature 1, Requirement R1)
//!
//! Provides endpoints for multi-location warehouse management with strict TenantContext isolation.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::api::AppState;
use crate::domain::inventory::{CreateWarehouseRequest, WarehousesListResponse};
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::service::inventory_service::InventoryService;

pub fn warehouses_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_warehouses_handler).post(create_warehouse_handler))
        .route("/{id}", get(get_warehouse_handler))
}

async fn list_warehouses_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    let service = InventoryService::new(state.pool);
    let warehouses = service.list_warehouses(&ctx).await?;
    let count = warehouses.len();
    Ok((
        StatusCode::OK,
        Json(WarehousesListResponse { warehouses, count }),
    ))
}

async fn get_warehouse_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Warehouse '{}' not found", id_str), "NOT_FOUND"))?;

    let service = InventoryService::new(state.pool);
    let warehouse = service.get_warehouse(&ctx, id).await?;
    Ok((StatusCode::OK, Json(warehouse)))
}

async fn create_warehouse_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(payload): Json<CreateWarehouseRequest>,
) -> Result<impl IntoResponse, AppError> {
    let service = InventoryService::new(state.pool);
    let warehouse = service.create_warehouse(&ctx, payload).await?;
    Ok((StatusCode::CREATED, Json(warehouse)))
}
