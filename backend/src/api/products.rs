//! Products API Endpoints (Feature 2, Requirement R1)
//!
//! Provides endpoints for product catalog and sequential SKU generation.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::api::AppState;
use crate::domain::inventory::{CreateProductRequest, ProductsListResponse};
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::service::inventory_service::InventoryService;

pub fn products_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_products_handler).post(create_product_handler))
        .route("/{id}", get(get_product_handler))
}

async fn list_products_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    let service = InventoryService::new(state.pool);
    let products = service.list_products(&ctx).await?;
    let count = products.len();
    Ok((
        StatusCode::OK,
        Json(ProductsListResponse { products, count }),
    ))
}

async fn get_product_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::NotFound(format!("Product '{}' not found", id_str), "NOT_FOUND"))?;

    let service = InventoryService::new(state.pool);
    let product = service.get_product(&ctx, id).await?;
    Ok((StatusCode::OK, Json(product)))
}

async fn create_product_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Json(payload): Json<CreateProductRequest>,
) -> Result<impl IntoResponse, AppError> {
    let service = InventoryService::new(state.pool);
    let product = service.create_product(&ctx, payload).await?;
    Ok((StatusCode::CREATED, Json(product)))
}
