use axum::{
    extract::{Path, Query, State},
    http::{
        header::{HeaderMap, HeaderValue, CACHE_CONTROL},
        StatusCode,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::api::AppState;
use crate::domain::money::Rupiah;
use crate::error::AppError;
use crate::repository::transaction_repo::TransactionFilter;
use crate::service::ledger_service::CreateTransactionRequest;

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Deserialize)]
pub struct CreateTxPayload {
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub category_id: Option<String>,
    pub transaction_type: String,
    pub amount: Rupiah,
    pub date: String,
    pub description: String,
    pub notes: Option<String>,
    pub is_recurring: Option<bool>,
    pub idempotency_key: Option<String>,
}

pub async fn list_transactions(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Query(filter): Query<TransactionFilter>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .ledger_service
        .list_transactions(&user.user_id, filter)
        .await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(result)))
}

pub async fn create_transaction(
    user: AuthenticatedUser,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<CreateTxPayload>,
) -> Result<impl IntoResponse, AppError> {
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or(payload.idempotency_key)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let req = CreateTransactionRequest {
        idempotency_key,
        account_id: payload.account_id,
        to_account_id: payload.to_account_id,
        category_id: payload.category_id,
        transaction_type: payload.transaction_type,
        amount: payload.amount,
        date: payload.date,
        description: payload.description,
        notes: payload.notes,
        is_recurring: payload.is_recurring,
    };

    let result = state
        .ledger_service
        .create_transaction(&user.user_id, req)
        .await?;
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::CREATED, resp_headers, Json(result)))
}

pub async fn get_transaction(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let tx = state
        .ledger_service
        .get_transaction(&user.user_id, &id)
        .await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(tx)))
}

pub async fn delete_transaction(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state
        .ledger_service
        .delete_transaction(&user.user_id, &id)
        .await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::NO_CONTENT, headers))
}

pub fn transactions_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_transactions).post(create_transaction))
        .route("/{id}", get(get_transaction).delete(delete_transaction))
}
