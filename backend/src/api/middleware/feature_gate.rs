use crate::api::middleware::auth_extractor::AuthenticatedUser;
use crate::error::AppError;
use axum::{extract::Request, middleware::Next, response::Response};

/// Centralized permission checker for SaaS feature gating.
///
/// Permissions:
/// - "transactions.basic" -> free, premium
/// - "analytics.basic"    -> free, premium
/// - "analytics.advanced" -> premium only
/// - "budgeting"          -> premium only
/// - "reports.advanced"   -> premium only
/// - "export"             -> premium only
/// - "financial_insights" -> premium only
pub fn is_permission_granted(user_tier: &str, permission: &str) -> bool {
    match permission {
        "transactions.basic" | "analytics.basic" => true,
        "analytics.advanced" | "budgeting" | "reports.advanced" | "export"
        | "financial_insights" => {
            user_tier.eq_ignore_ascii_case("premium")
                || user_tier.eq_ignore_ascii_case("trialing")
                || user_tier.eq_ignore_ascii_case("active")
        }
        _ => false,
    }
}

/// Helper to guard a route with a required permission.
/// Returns HTTP 403 Forbidden with RFC 7807 compatible error if user is not authorized.
pub fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), AppError> {
    if is_permission_granted(&user.tier, permission) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            format!("Subscription feature '{}' required.", permission),
            "FEATURE_LOCKED",
        ))
    }
}

/// Macro / Middleware helper to enforce premium tier on handlers
pub async fn require_premium_middleware(
    user: AuthenticatedUser,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    require_permission(&user, "analytics.advanced")?;
    Ok(next.run(request).await)
}
