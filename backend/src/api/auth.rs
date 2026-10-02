use axum::{
    extract::{Request, State},
    http::{
        header::{HeaderMap, HeaderValue, CACHE_CONTROL, SET_COOKIE},
        StatusCode,
    },
    middleware::{from_fn, from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::middleware::auth_extractor::{AuthenticatedUser, HasJwtEngine};
use crate::api::middleware::rate_limiter::{rate_limit_middleware, SlidingWindowRateLimiter};
use crate::error::AppError;
use crate::service::auth_service::{AuthService, LoginRequest, RegisterRequest};
use crate::service::jwt::JwtEngine;

/// RFC 9111 compliant Cache-Control header value for financial and auth data
pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Clone)]
pub struct AuthState {
    pub auth_service: Arc<AuthService>,
    pub secure_cookie: bool,
}

impl HasJwtEngine for AuthState {
    fn jwt_engine(&self) -> &JwtEngine {
        self.auth_service.jwt_engine()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutResponse {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleSuccessResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub password: String,
}

/// Formats the Set-Cookie header for auth_token
pub fn make_auth_cookie(token: &str, max_age_secs: i64, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "auth_token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{}",
        token, max_age_secs, secure_flag
    )
}

/// Formats the clearing Set-Cookie header for logout
pub fn make_logout_cookie(secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "auth_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT{}",
        secure_flag
    )
}

/// POST /api/v1/auth/register
pub async fn register_handler(
    State(state): State<AuthState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.auth_service.register(payload).await?;

    let cookie_val = make_auth_cookie(&result.token, 900, state.secure_cookie);
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie_val)
            .map_err(|e| AppError::Internal(format!("Invalid cookie header: {}", e)))?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    Ok((StatusCode::CREATED, headers, Json(result)))
}

/// POST /api/v1/auth/login
pub async fn login_handler(
    State(state): State<AuthState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.auth_service.login(payload).await?;

    let cookie_val = make_auth_cookie(&result.token, 900, state.secure_cookie);
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie_val)
            .map_err(|e| AppError::Internal(format!("Invalid cookie header: {}", e)))?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    Ok((StatusCode::OK, headers, Json(result)))
}

/// POST /api/v1/auth/logout
pub async fn logout_handler(State(state): State<AuthState>) -> Result<impl IntoResponse, AppError> {
    let cookie_val = make_logout_cookie(state.secure_cookie);
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie_val)
            .map_err(|e| AppError::Internal(format!("Invalid cookie header: {}", e)))?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    let response = LogoutResponse {
        status: "success".to_string(),
        message: "Logged out successfully".to_string(),
    };

    Ok((StatusCode::OK, headers, Json(response)))
}

/// GET /api/v1/auth/me
pub async fn me_handler(
    auth_user: AuthenticatedUser,
    State(state): State<AuthState>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.auth_service.get_me(&auth_user.user_id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    // If database subscription tier is premium/trialing but incoming token was free, refresh cookie
    if (result.user.subscription_tier == "premium" || result.user.subscription_tier == "trialing")
        && auth_user.tier != "premium"
    {
        if let Ok((refreshed_token, _)) = state.jwt_engine().generate_token(
            &auth_user.user_id,
            &auth_user.email,
            &auth_user.role,
            "premium",
        ) {
            let cookie_val = make_auth_cookie(&refreshed_token, 86400, state.secure_cookie);
            if let Ok(hv) = HeaderValue::from_str(&cookie_val) {
                headers.insert(SET_COOKIE, hv);
            }
        }
    }

    Ok((StatusCode::OK, headers, Json(result)))
}

/// Axum middleware ensuring Cache-Control: private, no-store, must-revalidate is present on all responses
pub async fn cache_control_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    response
}

/// POST /api/v1/auth/forgot-password & /api/v1/auth/recovery
pub async fn forgot_password_handler(
    State(state): State<AuthState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.auth_service.forgot_password(&payload.email).await?;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    let res = SimpleSuccessResponse {
        success: true,
        message: "Jika alamat email terdaftar, tautan pemulihan kata sandi telah dikirimkan ke kotak masuk Anda.".to_string(),
    };

    Ok((StatusCode::OK, headers, Json(res)))
}

/// POST /api/v1/auth/reset-password
pub async fn reset_password_handler(
    State(state): State<AuthState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.auth_service.reset_password(&payload.token, &payload.password).await?;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    let res = SimpleSuccessResponse {
        success: true,
        message: "Kata sandi Anda berhasil diperbarui. Silakan masuk menggunakan kata sandi baru.".to_string(),
    };

    Ok((StatusCode::OK, headers, Json(res)))
}

/// Builds the basic Auth sub-router without rate limiter
pub fn auth_routes(state: AuthState) -> Router {
    Router::new()
        .route("/register", post(register_handler))
        .route("/login", post(login_handler))
        .route("/forgot-password", post(forgot_password_handler))
        .route("/recovery", post(forgot_password_handler))
        .route("/reset-password", post(reset_password_handler))
        .route("/logout", post(logout_handler))
        .route("/me", get(me_handler))
        .layer(from_fn(cache_control_middleware))
        .with_state(state)
}

/// Builds the Auth sub-router with rate limiting on sensitive routes
pub fn auth_routes_with_rate_limiter(
    state: AuthState,
    rate_limiter: Arc<SlidingWindowRateLimiter>,
) -> Router {
    let sensitive = Router::new()
        .route("/register", post(register_handler))
        .route("/login", post(login_handler))
        .route("/forgot-password", post(forgot_password_handler))
        .route("/recovery", post(forgot_password_handler))
        .route("/reset-password", post(reset_password_handler))
        .route_layer(from_fn_with_state(rate_limiter, rate_limit_middleware))
        .with_state(state.clone());

    let standard = Router::new()
        .route("/logout", post(logout_handler))
        .route("/me", get(me_handler))
        .with_state(state);

    Router::new()
        .merge(sensitive)
        .merge(standard)
        .layer(from_fn(cache_control_middleware))
}
