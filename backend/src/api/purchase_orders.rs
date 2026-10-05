//! Purchase Orders API Endpoints (Features 10, 11, 12, Requirements R1, R3, R4)
//!
//! Exposes REST endpoints for the Purchase Order lifecycle state machine,
//! inbound goods receipt, and order cancellation with strict TenantContext isolation.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::api::AppState;
use crate::domain::inventory::{
    CancelPurchaseOrderRequest, CreatePurchaseOrderRequest, PurchaseOrdersListResponse,
    ReceivePurchaseOrderRequest,
};
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::service::inventory_service::InventoryService;

pub fn purchase_orders_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_purchase_orders_handler).post(create_purchase_order_handler))
        .route("/{id}", get(get_purchase_order_handler))
        .route("/{id}/order", post(order_purchase_order_handler))
        .route("/{id}/receive", post(receive_purchase_order_handler))
        .route("/{id}/cancel", post(cancel_purchase_order_handler))
}

async fn list_purchase_orders_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
) -> Result<impl IntoResponse, AppError> {
    let service = InventoryService::new(state.pool);
    let purchase_orders = service.list_purchase_orders(&ctx).await?;
    let count = purchase_orders.len();
    Ok((
        StatusCode::OK,
        Json(PurchaseOrdersListResponse {
            purchase_orders,
            count,
        }),
    ))
}

async fn create_purchase_order_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let req: CreatePurchaseOrderRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid purchase order request JSON: {}", e),
            "MISSING_REQUIRED_FIELDS",
        )
    })?;

    let service = InventoryService::new(state.pool);
    let po = service.create_purchase_order(&ctx, req).await?;
    Ok((StatusCode::CREATED, Json(po)))
}

async fn get_purchase_order_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(|_| {
        AppError::NotFound(
            format!("Purchase order '{}' not found", id_str),
            "NOT_FOUND",
        )
    })?;

    let service = InventoryService::new(state.pool);
    let po = service.get_purchase_order(&ctx, id).await?;
    Ok((StatusCode::OK, Json(po)))
}

async fn order_purchase_order_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(|_| {
        AppError::NotFound(
            format!("Purchase order '{}' not found", id_str),
            "NOT_FOUND",
        )
    })?;

    let service = InventoryService::new(state.pool);
    let res = service.order_purchase_order(&ctx, id).await?;
    Ok((StatusCode::OK, Json(res)))
}

async fn receive_purchase_order_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(|_| {
        AppError::NotFound(
            format!("Purchase order '{}' not found", id_str),
            "NOT_FOUND",
        )
    })?;

    let req: ReceivePurchaseOrderRequest = serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid receive purchase order request JSON: {}", e),
            "MISSING_REQUIRED_FIELDS",
        )
    })?;

    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let service = InventoryService::new(state.pool);
    let (res, is_replay) = service
        .receive_purchase_order(&ctx, id, req, idempotency_key, Some(&body_bytes))
        .await?;

    let mut resp_headers = HeaderMap::new();
    if is_replay {
        resp_headers.insert("x-cache-replay", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, resp_headers, Json(res)))
}

async fn cancel_purchase_order_handler(
    State(state): State<AppState>,
    ctx: TenantContext,
    Path(id_str): Path<String>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(|_| {
        AppError::NotFound(
            format!("Purchase order '{}' not found", id_str),
            "NOT_FOUND",
        )
    })?;

    let req: CancelPurchaseOrderRequest = if body_bytes.is_empty() {
        CancelPurchaseOrderRequest { reason: None }
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(CancelPurchaseOrderRequest { reason: None })
    };

    let service = InventoryService::new(state.pool);
    let res = service.cancel_purchase_order(&ctx, id, req).await?;
    Ok((StatusCode::OK, Json(res)))
}
