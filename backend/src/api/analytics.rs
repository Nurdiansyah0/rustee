use axum::{
    extract::State,
    http::{
        header::{HeaderMap, HeaderValue, CACHE_CONTROL, SET_COOKIE},
        StatusCode,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::api::middleware::auth_extractor::{AuthenticatedUser, HasJwtEngine};
use crate::api::middleware::feature_gate::is_permission_granted;
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

#[derive(Debug, Serialize, Clone)]
pub struct CategorySpendingSummary {
    pub category_id: Option<String>,
    pub category_name: String,
    pub category_color: Option<String>,
    pub category_icon: Option<String>,
    pub total_amount: Rupiah,
    pub percentage: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct Rule503020Summary {
    pub needs_amount: Rupiah,
    pub needs_percent: f64,
    pub wants_amount: Rupiah,
    pub wants_percent: f64,
    pub savings_amount: Rupiah,
    pub savings_percent: f64,
    pub target_needs_percent: f64,
    pub target_wants_percent: f64,
    pub target_savings_percent: f64,
}

#[derive(Debug, Serialize)]
pub struct AdvancedAnalyticsResponse {
    pub cash_flow: CashFlowSummary,
    pub savings_rate_percent: i64,
    pub financial_health_score: i64,
    pub health_grade: &'static str,
    pub monthly_trend: &'static str,
    pub total_balance: Rupiah,
    pub runway_months: f64,
    pub runway_status: &'static str,
    pub category_spending: Vec<CategorySpendingSummary>,
    pub rule_50_30_20: Rule503020Summary,
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
    // Check permission granted by token tier first
    let mut allowed = is_permission_granted(&user.tier, "analytics.advanced");
    let mut refreshed_cookie = None;

    if !allowed {
        // Fallback: Check database for active trial or paid subscription
        let sub = state
            .payment_service
            .get_subscription(&user.user_id)
            .await
            .ok()
            .flatten();
        let now = chrono::Utc::now();
        if let Some(s) = sub {
            if s.status == "active" {
                allowed = true;
            } else if s.status == "trialing" {
                if let Ok(end_dt) = chrono::DateTime::parse_from_rfc3339(&s.current_period_end) {
                    if end_dt.with_timezone(&chrono::Utc) > now {
                        allowed = true;
                    }
                }
            }
        }

        if !allowed {
            if let Ok(Some((tier,))) =
                sqlx::query_as::<_, (String,)>("SELECT subscription_tier FROM users WHERE id = ?1")
                    .bind(&user.user_id)
                    .fetch_optional(&state.pool)
                    .await
            {
                if tier.eq_ignore_ascii_case("premium") || tier.eq_ignore_ascii_case("trialing") {
                    allowed = true;
                }
            }
        }

        if allowed {
            if let Ok((refreshed_token, _)) =
                state
                    .jwt_engine()
                    .generate_token(&user.user_id, &user.email, &user.role, "premium")
            {
                let cookie_val = crate::api::auth::make_auth_cookie(
                    &refreshed_token,
                    86400,
                    state.auth_state.secure_cookie,
                );
                if let Ok(hv) = HeaderValue::from_str(&cookie_val) {
                    refreshed_cookie = Some(hv);
                }
            }
        }
    }

    if !allowed {
        return Err(AppError::Forbidden(
            "Subscription feature 'analytics.advanced' required.".to_string(),
            "FEATURE_LOCKED",
        ));
    }

    let cash_flow = state
        .ledger_service
        .cash_flow_summary(&user.user_id, None, None)
        .await?;

    let savings_rate = if cash_flow.total_income.is_positive() {
        (cash_flow.net_cash_flow.0 * 100) / cash_flow.total_income.0
    } else {
        0
    };

    let accounts = state
        .account_repo
        .list_by_user(&user.user_id, false)
        .await?;
    let total_balance: Rupiah = accounts.iter().map(|a| a.current_balance).sum();

    let monthly_expenses = cash_flow.total_expenses.0;
    let runway_months = if monthly_expenses > 0 {
        let m = (total_balance.0 as f64) / (monthly_expenses as f64);
        (m * 10.0).round() / 10.0
    } else {
        12.0
    };

    let runway_status = if runway_months >= 6.0 {
        "safe"
    } else if runway_months >= 3.0 {
        "warning"
    } else {
        "critical"
    };

    // Calculate dynamic health score
    let savings_pts = ((savings_rate.clamp(0, 50) as f64 / 50.0) * 40.0) as i64;
    let runway_pts = ((runway_months.clamp(0.0, 12.0) / 12.0) * 35.0) as i64;
    let cashflow_pts = if cash_flow.net_cash_flow.0 > 0 {
        25
    } else if cash_flow.net_cash_flow.0 == 0 {
        15
    } else {
        5
    };
    let health_score = (savings_pts + runway_pts + cashflow_pts).clamp(10, 100);

    let health_grade = if health_score >= 80 {
        "Sangat Baik (A)"
    } else if health_score >= 60 {
        "Baik (B)"
    } else {
        "Perlu Optimasi (C)"
    };

    // Query spending per category
    let rows = sqlx::query_as::<
        _,
        (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
        ),
    >(
        r#"
        SELECT 
            t.category_id,
            COALESCE(c.display_name, c.name, 'Lainnya / Umum') as category_name,
            c.color as category_color,
            c.icon as category_icon,
            SUM(t.amount) as total_amount
        FROM transactions t
        LEFT JOIN categories c ON t.category_id = c.id
        WHERE t.user_id = ?1
          AND t.transaction_type = 'expense'
        GROUP BY t.category_id, c.display_name, c.name, c.color, c.icon
        ORDER BY total_amount DESC
        LIMIT 6
        "#,
    )
    .bind(&user.user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut category_spending = Vec::new();
    let total_exp_float = if monthly_expenses > 0 {
        monthly_expenses as f64
    } else {
        1.0
    };
    for (cat_id, cat_name, cat_color, cat_icon, amount) in rows {
        let pct = ((amount as f64 / total_exp_float) * 1000.0).round() / 10.0;
        category_spending.push(CategorySpendingSummary {
            category_id: cat_id,
            category_name: cat_name.unwrap_or_else(|| "Lainnya / Umum".to_string()),
            category_color: cat_color,
            category_icon: cat_icon,
            total_amount: Rupiah::new(amount),
            percentage: pct,
        });
    }

    // 50/30/20 rule calculation
    let total_income = cash_flow.total_income.0;
    let total_inc_float = if total_income > 0 {
        total_income as f64
    } else {
        1.0
    };
    let needs_amount = (monthly_expenses * 60) / 100;
    let wants_amount = monthly_expenses - needs_amount;
    let savings_amount = if cash_flow.net_cash_flow.0 > 0 {
        cash_flow.net_cash_flow.0
    } else {
        0
    };

    let rule_50_30_20 = Rule503020Summary {
        needs_amount: Rupiah::new(needs_amount),
        needs_percent: if total_income > 0 {
            ((needs_amount as f64 / total_inc_float) * 1000.0).round() / 10.0
        } else {
            0.0
        },
        wants_amount: Rupiah::new(wants_amount),
        wants_percent: if total_income > 0 {
            ((wants_amount as f64 / total_inc_float) * 1000.0).round() / 10.0
        } else {
            0.0
        },
        savings_amount: Rupiah::new(savings_amount),
        savings_percent: if total_income > 0 {
            ((savings_amount as f64 / total_inc_float) * 1000.0).round() / 10.0
        } else {
            0.0
        },
        target_needs_percent: 50.0,
        target_wants_percent: 30.0,
        target_savings_percent: 20.0,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    if let Some(cookie) = refreshed_cookie {
        headers.insert(SET_COOKIE, cookie);
    }

    Ok((
        StatusCode::OK,
        headers,
        Json(AdvancedAnalyticsResponse {
            cash_flow,
            savings_rate_percent: savings_rate,
            financial_health_score: health_score,
            health_grade,
            monthly_trend: "positive",
            total_balance,
            runway_months,
            runway_status,
            category_spending,
            rule_50_30_20,
            tier: if !user.tier.eq_ignore_ascii_case("free") {
                user.tier
            } else if allowed {
                "premium".to_string()
            } else {
                user.tier
            },
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
