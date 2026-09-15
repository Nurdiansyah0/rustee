use axum::extract::FromRequestParts;
use axum::http::header::{AUTHORIZATION, COOKIE};
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::AppError;
use crate::service::jwt::JwtEngine;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub tier: String,
}

pub trait HasJwtEngine {
    fn jwt_engine(&self) -> &JwtEngine;
}

impl HasJwtEngine for JwtEngine {
    fn jwt_engine(&self) -> &JwtEngine {
        self
    }
}

impl HasJwtEngine for Arc<JwtEngine> {
    fn jwt_engine(&self) -> &JwtEngine {
        self
    }
}

/// Helper to extract cookie value from Cookie header(s).
/// Complies with HTTP/2 multi-header splitting by using `headers.get_all(COOKIE)`.
pub fn extract_cookie_token(parts: &Parts, cookie_name: &str) -> Option<String> {
    parts
        .headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|header_val| header_val.to_str().ok())
        .flat_map(|cookie_str| cookie_str.split(';'))
        .find_map(|pair| {
            let (name, val) = pair.trim().split_once('=')?;
            if name.trim() == cookie_name {
                let trimmed_val = val.trim().trim_matches('"');
                if !trimmed_val.is_empty() {
                    Some(trimmed_val.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        })
}

/// Helper to extract token from Authorization: Bearer <token> header
pub fn extract_bearer_token(parts: &Parts) -> Option<String> {
    let auth_header = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: HasJwtEngine + Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Check Cookie header first
        let raw_token = extract_cookie_token(parts, "auth_token")
            // 2. Fall back to Authorization: Bearer <token>
            .or_else(|| extract_bearer_token(parts))
            .ok_or_else(|| {
                AppError::Unauthorized(
                    "Missing authentication token in cookie or Authorization header".to_string(),
                    "AUTH_TOKEN_MISSING",
                )
            })?;

        // 3. Cryptographic signature and expiration check
        let claims = state
            .jwt_engine()
            .verify_token(&raw_token)
            .map_err(AppError::from)?;

        Ok(AuthenticatedUser {
            user_id: claims.sub,
            email: claims.email,
            role: claims.role,
            tier: claims.tier,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header::COOKIE, HeaderMap, Request};

    #[test]
    fn test_extract_cookie_single_header_multiple_cookies() {
        let req = Request::builder()
            .header(
                COOKIE,
                "session_id=abc; auth_token=token_single_123; pref=dark",
            )
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        assert_eq!(
            extract_cookie_token(&parts, "auth_token"),
            Some("token_single_123".to_string())
        );
        assert_eq!(
            extract_cookie_token(&parts, "session_id"),
            Some("abc".to_string())
        );
        assert_eq!(
            extract_cookie_token(&parts, "pref"),
            Some("dark".to_string())
        );
        assert_eq!(extract_cookie_token(&parts, "nonexistent"), None);
    }

    #[test]
    fn test_extract_cookie_http2_multiple_headers() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, "pref=dark".parse().unwrap());
        headers.append(COOKIE, "auth_token=http2_multi_token_456".parse().unwrap());
        headers.append(COOKIE, "lang=id-ID; country=ID".parse().unwrap());

        let mut req = Request::builder().body(()).unwrap();
        *req.headers_mut() = headers;
        let (parts, _) = req.into_parts();

        assert_eq!(
            extract_cookie_token(&parts, "auth_token"),
            Some("http2_multi_token_456".to_string())
        );
        assert_eq!(
            extract_cookie_token(&parts, "lang"),
            Some("id-ID".to_string())
        );
        assert_eq!(
            extract_cookie_token(&parts, "country"),
            Some("ID".to_string())
        );
        assert_eq!(extract_cookie_token(&parts, "other"), None);
    }

    #[test]
    fn test_extract_cookie_whitespace_and_enclosing_quotes() {
        let req = Request::builder()
            .header(
                COOKIE,
                "  auth_token  =  \"quoted_jwt_payload_value\"  ;  flag = 1 ",
            )
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        assert_eq!(
            extract_cookie_token(&parts, "auth_token"),
            Some("quoted_jwt_payload_value".to_string())
        );
        assert_eq!(extract_cookie_token(&parts, "flag"), Some("1".to_string()));
    }

    #[test]
    fn test_extract_cookie_skips_empty_val_to_find_valid_token() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, "auth_token=; other=1".parse().unwrap());
        headers.append(COOKIE, "auth_token=recovered_valid_token".parse().unwrap());

        let mut req = Request::builder().body(()).unwrap();
        *req.headers_mut() = headers;
        let (parts, _) = req.into_parts();

        assert_eq!(
            extract_cookie_token(&parts, "auth_token"),
            Some("recovered_valid_token".to_string())
        );
    }

    #[test]
    fn test_extract_cookie_empty_headers_returns_none() {
        let req = Request::builder().body(()).unwrap();
        let (parts, _) = req.into_parts();
        assert_eq!(extract_cookie_token(&parts, "auth_token"), None);
    }
}
