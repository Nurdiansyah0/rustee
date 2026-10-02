use axum::{
    extract::{Query, State},
    http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{middleware::auth_extractor::AuthenticatedUser, AppState},
    error::AppError,
    repository::{
        account_repo::Account,
        category_repo::Category,
        transaction_repo::{TransactionFilter, TransactionRecord},
    },
};

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Deserialize)]
pub struct SyncQuery {
    pub cursor: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub cursor: i64,
    pub deltas_count: usize,
    pub transactions: Vec<TransactionRecord>,
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub has_more: bool,
    pub timestamp: String,
}

pub async fn sync_handler(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> Result<impl IntoResponse, AppError> {
    let current_cursor = query.cursor.unwrap_or(0);
    let new_cursor = current_cursor + 1;

    let accounts = state
        .account_repo
        .list_by_user(&user.user_id, false)
        .await?;

    let categories = state.category_repo.list_by_user(&user.user_id).await?;

    let filter = TransactionFilter {
        per_page: 50,
        ..Default::default()
    };
    let tx_resp = state
        .ledger_service
        .list_transactions(&user.user_id, filter)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Update sync_cursors table for user (§16, §25)
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO sync_cursors (user_id, last_cursor, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET last_cursor = excluded.last_cursor, updated_at = excluded.updated_at",
    )
    .bind(&user.user_id)
    .bind(new_cursor)
    .bind(&now)
    .execute(&state.pool)
    .await;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    let body = SyncResponse {
        cursor: new_cursor,
        deltas_count: 0,
        transactions: tx_resp.data,
        accounts,
        categories,
        has_more: false,
        timestamp: now,
    };

    Ok((StatusCode::OK, headers, Json(body)))
}
