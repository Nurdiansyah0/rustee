use axum::{
    extract::State,
    http::{
        header::{CACHE_CONTROL, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::middleware::auth_extractor::{AuthenticatedUser, HasJwtEngine};
use crate::api::AppState;
use crate::error::AppError;
use crate::repository::{
    account_repo::NewAccount,
    category_repo::NewCategory,
    user_preferences_repo::{UpdateUserPreferences, UserPreferences},
};

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Serialize, Deserialize)]
pub struct PersonalizationResponse {
    pub user_id: String,
    pub display_name: Option<String>,
    pub income_title: Option<String>,
    pub expense_title: Option<String>,
    pub financial_goals: Vec<String>,
    pub onboarding_completed: bool,
    pub updated_at: Option<String>,
}

impl From<Option<UserPreferences>> for PersonalizationResponse {
    fn from(opt: Option<UserPreferences>) -> Self {
        match opt {
            Some(p) => {
                let goals: Vec<String> = p
                    .financial_goals
                    .as_deref()
                    .and_then(|g| serde_json::from_str(g).ok())
                    .unwrap_or_default();
                Self {
                    user_id: p.user_id,
                    display_name: p.display_name,
                    income_title: p.income_title,
                    expense_title: p.expense_title,
                    financial_goals: goals,
                    onboarding_completed: p.onboarding_completed,
                    updated_at: Some(p.updated_at),
                }
            }
            None => Self {
                user_id: String::new(),
                display_name: None,
                income_title: None,
                expense_title: None,
                financial_goals: Vec::new(),
                onboarding_completed: false,
                updated_at: None,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct OnboardingWallet {
    pub name: String,
    pub account_type: Option<String>,
    pub initial_balance: Option<i64>,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OnboardingCategory {
    pub name: String,
    pub category_type: String, // 'income' | 'expense'
    pub display_name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OnboardingRequest {
    pub display_name: Option<String>,
    pub income_title: Option<String>,
    pub expense_title: Option<String>,
    pub financial_goals: Option<Vec<String>>,
    pub wallets: Option<Vec<OnboardingWallet>>,
    pub categories: Option<Vec<OnboardingCategory>>,
    pub activate_trial: Option<bool>,
}

pub async fn get_personalization(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let prefs = state
        .user_preferences_repo
        .get_by_user_id(&user.user_id)
        .await?;

    let mut resp: PersonalizationResponse = prefs.into();
    resp.user_id = user.user_id.clone();

    // If display_name is not yet set in preferences, fall back to registered display_name
    if resp.display_name.is_none() {
        if let Ok(u) = state.auth_state.auth_service.get_me(&user.user_id).await {
            resp.display_name = Some(u.user.display_name);
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(resp)))
}

pub async fn update_personalization(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<UpdateUserPreferences>,
) -> Result<impl IntoResponse, AppError> {
    let updated = state
        .user_preferences_repo
        .upsert(&user.user_id, &payload)
        .await?;

    let resp: PersonalizationResponse = Some(updated).into();
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(resp)))
}

pub async fn onboarding_handler(
    user: AuthenticatedUser,
    State(state): State<AppState>,
    Json(payload): Json<OnboardingRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Upsert user preferences with onboarding_completed: true
    let update = UpdateUserPreferences {
        display_name: payload.display_name,
        income_title: payload.income_title,
        expense_title: payload.expense_title,
        financial_goals: payload.financial_goals,
        onboarding_completed: Some(true),
    };
    let prefs = state
        .user_preferences_repo
        .upsert(&user.user_id, &update)
        .await?;

    // 2. Create custom wallets/accounts if supplied
    if let Some(wallets) = payload.wallets {
        for w in wallets {
            let name_trim = w.name.trim();
            if !name_trim.is_empty() {
                let new_acc = NewAccount {
                    id: uuid::Uuid::new_v4().to_string(),
                    user_id: user.user_id.clone(),
                    name: name_trim.to_string(),
                    account_type: w.account_type.unwrap_or_else(|| "checking".to_string()),
                    currency: Some("IDR".to_string()),
                    initial_balance: crate::domain::Rupiah::new(w.initial_balance.unwrap_or(0)),
                    color: w.color,
                    icon: w.icon,
                };
                let _ = state.account_repo.create(&new_acc).await;
            }
        }
    }

    // 3. Create custom categories if supplied
    if let Some(cats) = payload.categories {
        for c in cats {
            let name_trim = c.name.trim();
            if !name_trim.is_empty() && (c.category_type == "income" || c.category_type == "expense") {
                let disp = c.display_name.unwrap_or_else(|| name_trim.to_string());
                let new_cat = NewCategory {
                    id: uuid::Uuid::new_v4().to_string(),
                    user_id: Some(user.user_id.clone()),
                    name: name_trim.to_string(),
                    category_type: c.category_type,
                    icon: c.icon,
                    color: c.color,
                    is_system: false,
                    display_name: Some(disp),
                    normalized_name: Some(name_trim.to_lowercase()),
                    metadata: c.metadata,
                };
                let _ = state.category_repo.create(&new_cat).await;
            }
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    // 4. Activate trial if user opted in
    if payload.activate_trial.unwrap_or(false) {
        let _ = state.payment_service.activate_trial_with_pool(&state.pool, &user.user_id).await;
        if let Ok((refreshed_token, _)) = state
            .jwt_engine()
            .generate_token(&user.user_id, &user.email, &user.role, "premium")
        {
            let cookie_val = crate::api::auth::make_auth_cookie(
                &refreshed_token,
                86400,
                state.auth_state.secure_cookie,
            );
            if let Ok(hv) = HeaderValue::from_str(&cookie_val) {
                headers.insert(SET_COOKIE, hv);
            }
        }
    }

    let resp: PersonalizationResponse = Some(prefs).into();
    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "status": "success",
            "message": "Onboarding completed successfully",
            "personalization": resp,
        })),
    ))
}

pub fn users_router() -> Router<AppState> {
    Router::new()
        .route(
            "/personalization",
            get(get_personalization).put(update_personalization),
        )
        .route("/onboarding", post(onboarding_handler))
}
