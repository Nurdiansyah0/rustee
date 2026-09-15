use axum::{
    extract::{Path, State},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;

use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::api::AppState;
use crate::error::AppError;
use crate::repository::category_repo::{Category, NewCategory};

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Deserialize)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub category_type: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}

pub async fn list_categories(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let categories: Vec<Category> = state.category_repo.list_by_user(&user.user_id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(categories)))
}

pub async fn create_category(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<CreateCategoryRequest>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Category name cannot be empty".to_string(),
            "INVALID_NAME",
        ));
    }

    if payload.category_type != "income" && payload.category_type != "expense" {
        return Err(AppError::BadRequest(
            "Category type must be 'income' or 'expense'".to_string(),
            "INVALID_TYPE",
        ));
    }

    let new_cat = NewCategory {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: Some(user.user_id),
        name: payload.name.trim().to_string(),
        category_type: payload.category_type,
        icon: payload.icon,
        color: payload.color,
        is_system: false,
    };

    let cat = state.category_repo.create(&new_cat).await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::CREATED, headers, Json(cat)))
}

pub async fn delete_category(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.category_repo.soft_delete(&user.user_id, &id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::NO_CONTENT, headers))
}

pub fn categories_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_categories).post(create_category))
        .route("/{id}", delete(delete_category))
}
