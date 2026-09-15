use axum::{
    extract::{Path, State},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::api::AppState;
use crate::domain::money::Rupiah;
use crate::error::AppError;
use crate::repository::account_repo::NewAccount;

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub name: String,
    pub account_type: String,
    pub currency: Option<String>,
    pub initial_balance: Option<Rupiah>,
    pub color: Option<String>,
    pub icon: Option<String>,
}

pub async fn list_accounts(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let accounts = state
        .account_repo
        .list_by_user(&user.user_id, false)
        .await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(accounts)))
}

pub async fn create_account(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<CreateAccountRequest>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Account name cannot be empty".to_string(),
            "INVALID_NAME",
        ));
    }

    let new_acc = NewAccount {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.user_id,
        name: payload.name.trim().to_string(),
        account_type: payload.account_type,
        currency: payload.currency,
        initial_balance: payload.initial_balance.unwrap_or(Rupiah::ZERO),
        color: payload.color,
        icon: payload.icon,
    };

    let acc = state.account_repo.create(&new_acc).await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::CREATED, headers, Json(acc)))
}

pub async fn get_account(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let account = state
        .account_repo
        .find_by_id(&user.user_id, &id)
        .await?
        .ok_or_else(|| AppError::NotFound("Account not found".to_string(), "NOT_FOUND"))?;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(account)))
}

pub async fn archive_account(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.account_repo.archive(&user.user_id, &id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::NO_CONTENT, headers))
}

pub fn accounts_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_accounts).post(create_account))
        .route("/{id}", get(get_account))
        .route("/{id}/archive", post(archive_account))
}
