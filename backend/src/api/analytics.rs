use axum::{
    extract::State,
    http::{
        header::{HeaderMap, HeaderValue, CACHE_CONTROL},
        StatusCode,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::api::middleware::feature_gate::require_permission;
use crate::api::AppState;
use crate::domain::money::Rupiah;
use crate::error::AppError;
use crate::repository::{
    account_repo::Account,
    transaction_repo::{CashFlowSummary, TransactionFilter, TransactionRecord},
};

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    pub total_balance: Rupiah,
    pub cash_flow: CashFlowSummary,
    pub accounts: Vec<Account>,
    pub recent_transactions: Vec<TransactionRecord>,
    pub tier: String,
}

#[derive(Debug, Serialize)]
pub struct BasicAnalyticsResponse {
    pub cash_flow: CashFlowSummary,
    pub savings_rate_percent: i64,
    pub tier: String,
}

#[derive(Debug, Serialize)]
pub struct AdvancedAnalyticsResponse {
    pub cash_flow: CashFlowSummary,
    pub savings_rate_percent: i64,
    pub financial_health_score: i64,
    pub monthly_trend: &'static str,
    pub tier: String,
    pub message: &'static str,
}

pub async fn get_dashboard(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let accounts = state
        .account_repo
        .list_by_user(&user.user_id, false)
        .await?;
    let total_balance: Rupiah = accounts.iter().map(|a| a.current_balance).sum();

    let cash_flow = state
        .ledger_service
        .cash_flow_summary(&user.user_id, None, None)
        .await?;

    let filter = TransactionFilter {
        page: 1,
        per_page: 5,
        ..Default::default()
    };
    let (recent_transactions, _) = state
        .ledger_service
        .list_transactions(&user.user_id, filter)
        .await
        .map(|r| (r.data, r.meta))?;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    Ok((
        StatusCode::OK,
        headers,
        Json(DashboardResponse {
            total_balance,
            cash_flow,
            accounts,
            recent_transactions,
            tier: user.tier,
        }),
    ))
}

pub async fn get_basic_analytics(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let cash_flow = state
        .ledger_service
        .cash_flow_summary(&user.user_id, None, None)
        .await?;

    let savings_rate = if cash_flow.total_income.is_positive() {
        (cash_flow.net_cash_flow.0 * 100) / cash_flow.total_income.0
    } else {
        0
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    Ok((
        StatusCode::OK,
        headers,
        Json(BasicAnalyticsResponse {
            cash_flow,
            savings_rate_percent: savings_rate,
            tier: user.tier,
        }),
    ))
}

pub async fn get_advanced_analytics(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // Strict server-side feature gate: returns HTTP 403 Forbidden for Free tier
    require_permission(&user, "analytics.advanced")?;

    let cash_flow = state
        .ledger_service
        .cash_flow_summary(&user.user_id, None, None)
        .await?;

    let savings_rate = if cash_flow.total_income.is_positive() {
        (cash_flow.net_cash_flow.0 * 100) / cash_flow.total_income.0
    } else {
        0
    };

    let health_score = (savings_rate.clamp(0, 100) * 8 / 10) + 20;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    Ok((
        StatusCode::OK,
        headers,
        Json(AdvancedAnalyticsResponse {
            cash_flow,
            savings_rate_percent: savings_rate,
            financial_health_score: health_score,
            monthly_trend: "positive",
            tier: user.tier,
            message: "Premium analytics unlocked",
        }),
    ))
}

pub fn analytics_router() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(get_dashboard))
        .route("/analytics/basic", get(get_basic_analytics))
        .route("/analytics/advanced", get(get_advanced_analytics))
}
