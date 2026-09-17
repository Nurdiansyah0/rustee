use axum::{
    body::Bytes,
    extract::State,
    http::{
        header::{HeaderMap, HeaderValue, CACHE_CONTROL, SET_COOKIE},
        StatusCode,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;

use crate::api::auth::make_auth_cookie;
use crate::api::middleware::auth_extractor::{AuthenticatedUser, HasJwtEngine};
use crate::api::AppState;
use crate::error::AppError;
use crate::service::payment_service::{MidtransNotification, XenditNotification};

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    pub provider: Option<String>,
    pub plan_id: Option<String>,
}

pub async fn midtrans_webhook_handler(
    State(state): State<AppState>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let payload_str = std::str::from_utf8(&body_bytes).map_err(|e| {
        AppError::BadRequest(format!("Invalid UTF-8 payload: {}", e), "INVALID_PAYLOAD")
    })?;

    let notification: MidtransNotification = serde_json::from_str(payload_str).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid Midtrans JSON payload: {}", e),
            "INVALID_PAYLOAD",
        )
    })?;

    let result = state
        .payment_service
        .handle_midtrans_webhook(notification, payload_str)
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, headers, Json(result)))
}

pub async fn xendit_webhook_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let token = headers
        .get("x-callback-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let payload_str = std::str::from_utf8(&body_bytes).map_err(|e| {
        AppError::BadRequest(format!("Invalid UTF-8 payload: {}", e), "INVALID_PAYLOAD")
    })?;

    let notification: XenditNotification = serde_json::from_str(payload_str).map_err(|e| {
        AppError::BadRequest(
            format!("Invalid Xendit JSON payload: {}", e),
            "INVALID_PAYLOAD",
        )
    })?;

    let result = state
        .payment_service
        .handle_xendit_webhook(token, notification, payload_str)
        .await?;

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    Ok((StatusCode::OK, resp_headers, Json(result)))
}

pub async fn get_subscription_status(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let mut sub = state
        .payment_service
        .get_subscription(&user.user_id)
        .await?;

    let user_trial = state
        .payment_service
        .get_user_trial_info(&state.pool, &user.user_id)
        .await
        .unwrap_or(None);
    let mut trial_ends_at = user_trial.as_ref().and_then(|u| u.trial_ends_at.clone());

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    let now = Utc::now();
    let mut is_pro = user.tier == "premium";
    let mut status = if is_pro { "active" } else { "free" };
    let mut days_remaining: Option<i64> = None;
    let mut current_period_end: Option<String> = None;
    let mut plan_id: String = "free".to_string();
    let mut amount: i64 = 0;

    if let Some(ref mut s) = sub {
        current_period_end = Some(s.current_period_end.clone());
        plan_id = s.plan_id.clone();
        amount = s.amount.0;

        if s.status == "trialing" {
            let end_dt = chrono::DateTime::parse_from_rfc3339(&s.current_period_end)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(now);

            trial_ends_at = Some(s.current_period_end.clone());

            if end_dt > now {
                let diff_secs = (end_dt - now).num_seconds();
                let days = if diff_secs > 0 {
                    (diff_secs + 86399) / 86400
                } else {
                    0
                };
                is_pro = true;
                status = "trialing";
                days_remaining = Some(days);
            } else {
                // Trial has expired: lazily update user to tier 'free' and status 'expired', preserving all user data
                let now_str = now.to_rfc3339();
                let _ = sqlx::query(
                    "UPDATE users SET subscription_tier = 'free', updated_at = ?1 WHERE id = ?2",
                )
                .bind(&now_str)
                .bind(&user.user_id)
                .execute(&state.pool)
                .await;

                let _ = sqlx::query(
                    "UPDATE subscriptions SET status = 'expired', updated_at = ?1 WHERE user_id = ?2 AND status = 'trialing'",
                )
                .bind(&now_str)
                .bind(&user.user_id)
                .execute(&state.pool)
                .await;

                let _ = state
                    .payment_service
                    .subscription_repo
                    .record_subscription_event(
                        &user.user_id,
                        "subscription_expired",
                        "trial",
                        &s.id,
                        "{\"reason\": \"trial_period_ended\", \"tier\": \"free\"}",
                    )
                    .await;

                let _ = state
                    .payment_service
                    .audit_repo
                    .log_event(&crate::repository::audit_repo::NewAuditLog {
                        user_id: Some(user.user_id.clone()),
                        action: "subscription_expired".to_string(),
                        entity_type: "subscription".to_string(),
                        entity_id: s.id.clone(),
                        ip_address: None,
                        user_agent: Some("System-Lazy-Expiry".to_string()),
                        details: Some("3-month trial expired; reverted to free tier without data loss".to_string()),
                    })
                    .await;

                s.status = "expired".to_string();
                s.updated_at = now_str;
                status = "expired";
                is_pro = false;
                days_remaining = Some(0);
            }
        } else if s.status == "active" {
            is_pro = true;
            status = "active";
            amount = if s.amount.0 > 0 { s.amount.0 } else { 10000 };
        } else {
            status = s.status.as_str();
            is_pro = false;
        }
    }

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "tier": if is_pro { "premium" } else { "free" },
            "status": status,
            "is_premium": is_pro,
            "days_remaining": days_remaining,
            "remaining_days": days_remaining,
            "trial_ends_at": trial_ends_at,
            "current_period_end": current_period_end,
            "plan_id": plan_id,
            "amount": amount,
            "price_monthly": if status == "trialing" { 0 } else if is_pro { 10000 } else { 0 },
            "price_annual": 110000,
            "subscription": sub,
            "features": if is_pro {
                vec!["transactions.basic", "analytics.basic", "analytics.advanced", "budgeting", "reports.advanced"]
            } else {
                vec!["transactions.basic", "analytics.basic"]
            }
        })),
    ))
}

pub async fn trial_handler(
    user: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let res = state
        .payment_service
        .activate_trial_with_pool(&state.pool, &user.user_id)
        .await?;

    // Crucial: Re-issue JWT cookie auth_token with tier: "premium" in the response header (Set-Cookie)
    // so subsequent client requests immediately have Pro permissions without requiring re-login.
    let (refreshed_token, _) = state
        .jwt_engine()
        .generate_token(&user.user_id, &user.email, &user.role, "premium")
        .map_err(AppError::from)?;

    let cookie_val = make_auth_cookie(&refreshed_token, 86400, state.auth_state.secure_cookie);

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie_val)
            .map_err(|e| AppError::Internal(format!("Invalid cookie header: {}", e)))?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "status": res.status,
            "tier": res.tier,
            "is_premium": res.is_premium,
            "trial_started_at": res.trial_started_at,
            "trial_ends_at": res.trial_ends_at,
            "days_remaining": res.days_remaining,
            "remaining_days": res.remaining_days,
            "message": res.message,
            "subscription": res.subscription,
        })),
    ))
}

pub async fn checkout_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CheckoutRequest>,
) -> Result<impl IntoResponse, AppError> {
    let provider = payload.provider.as_deref().unwrap_or("");
    let plan = payload.plan_id.as_deref();
    let session = state
        .payment_service
        .create_checkout_session_async(&user.user_id, provider, plan)
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    Ok((
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "order_id": session.order_id,
            "provider": session.provider,
            "plan_id": session.plan_id,
            "amount": session.amount,
            "currency": session.currency,
            "checkout_url": session.checkout_url,
            "reference_no": session.reference_no,
        })),
    ))
}

pub async fn dana_webhook_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    let signature = headers
        .get("x-signature")
        .or_else(|| headers.get("X-SIGNATURE"))
        .and_then(|v| v.to_str().ok());
    let timestamp = headers
        .get("x-timestamp")
        .or_else(|| headers.get("X-TIMESTAMP"))
        .and_then(|v| v.to_str().ok());

    let payload_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(e) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                resp_headers,
                Json(serde_json::json!({
                    "responseCode": "4005600",
                    "responseMessage": format!("Invalid UTF-8 payload: {}", e)
                })),
            ));
        }
    };

    match state
        .payment_service
        .handle_dana_webhook(
            "POST",
            "/api/v1/webhooks/dana",
            timestamp,
            signature,
            payload_str,
        )
        .await
    {
        Ok(_) => Ok((
            StatusCode::OK,
            resp_headers,
            Json(serde_json::json!({
                "responseCode": "2005600",
                "responseMessage": "Successful"
            })),
        )),
        Err(crate::service::payment_service::PaymentError::InvalidSignature) => Ok((
            StatusCode::UNAUTHORIZED,
            resp_headers,
            Json(serde_json::json!({
                "responseCode": "4015600",
                "responseMessage": "Unauthorized: Invalid Signature"
            })),
        )),
        Err(crate::service::payment_service::PaymentError::InvalidPayload(msg)) => Ok((
            StatusCode::BAD_REQUEST,
            resp_headers,
            Json(serde_json::json!({
                "responseCode": "4005600",
                "responseMessage": format!("Bad Request: {}", msg)
            })),
        )),
        Err(e) => Err(AppError::from(e)),
    }
}

/// Stub for DANA Disburse-to-Bank notification.
/// We only collect payments — disbursements are not used.
/// DANA dashboard requires this URL; we acknowledge with success.
pub async fn dana_disburse_notify_handler(
    _headers: HeaderMap,
    _body_bytes: Bytes,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    (
        StatusCode::OK,
        headers,
        Json(serde_json::json!({
            "responseCode": "2005600",
            "responseMessage": "Successful"
        })),
    )
}

pub fn webhooks_router() -> Router<AppState> {
    Router::new()
        .route("/webhooks/midtrans", post(midtrans_webhook_handler))
        .route("/webhooks/xendit", post(xendit_webhook_handler))
        .route("/webhooks/dana", post(dana_webhook_handler))
        .route("/webhooks/dana/disburse", post(dana_disburse_notify_handler))
        .route("/subscription", get(get_subscription_status))
        .route("/subscriptions/status", get(get_subscription_status))
        .route("/subscriptions/checkout", post(checkout_handler))
        .route("/subscriptions/trial", post(trial_handler))
        .route("/subscription/trial", post(trial_handler))
}
