use crate::error::AppError;
use jsonwebtoken::{
    decode, encode, errors::Error as JwtError, errors::ErrorKind, Algorithm, DecodingKey,
    EncodingKey, Header, Validation,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthTokenError {
    #[error("Authentication token has expired")]
    Expired,

    #[error("Invalid token or signature: {0}")]
    Invalid(String),

    #[error("System clock error: {0}")]
    ClockError(String),
}

impl From<JwtError> for AuthTokenError {
    fn from(err: JwtError) -> Self {
        match err.kind() {
            ErrorKind::ExpiredSignature => AuthTokenError::Expired,
            _ => AuthTokenError::Invalid(err.to_string()),
        }
    }
}

impl From<AuthTokenError> for AppError {
    fn from(err: AuthTokenError) -> Self {
        match err {
            AuthTokenError::Expired => AppError::Unauthorized(
                "Authentication token has expired".to_string(),
                "AUTH_TOKEN_EXPIRED",
            ),
            AuthTokenError::Invalid(msg) => AppError::Unauthorized(
                format!("Invalid authentication token: {}", msg),
                "AUTH_TOKEN_INVALID",
            ),
            AuthTokenError::ClockError(msg) => {
                AppError::Internal(format!("System clock error: {}", msg))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub sub: String,   // Subject: User ID
    pub email: String, // User email
    pub role: String,  // "user" | "admin"
    pub tier: String,  // "free" | "premium"
    pub exp: usize,    // Expiration timestamp (seconds since UNIX epoch)
    pub iat: usize,    // Issued at timestamp (seconds since UNIX epoch)
}

#[derive(Clone)]
pub struct JwtEngine {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    ttl_seconds: u64,
}

impl JwtEngine {
    pub fn new(secret: &str, ttl_seconds: u64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            ttl_seconds,
        }
    }

    pub fn ttl_seconds(&self) -> u64 {
        self.ttl_seconds
    }

    pub fn generate_token(
        &self,
        user_id: &str,
        email: &str,
        role: &str,
        tier: &str,
    ) -> Result<(String, Claims), AuthTokenError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AuthTokenError::ClockError(e.to_string()))?
            .as_secs();

        let exp = (now + self.ttl_seconds) as usize;
        let iat = now as usize;

        self.generate_token_with_exp(user_id, email, role, tier, exp, iat)
    }

    /// Generates a signed JWT token with custom expiration (`exp`) and issued-at (`iat`) timestamps.
    /// Enables testing explicit expiration boundaries, clock skew leeways, and epoch timestamps.
    pub fn generate_token_with_exp(
        &self,
        user_id: &str,
        email: &str,
        role: &str,
        tier: &str,
        exp: usize,
        iat: usize,
    ) -> Result<(String, Claims), AuthTokenError> {
        let claims = Claims {
            sub: user_id.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            tier: tier.to_string(),
            exp,
            iat,
        };

        let header = Header::new(Algorithm::HS256);
        let token = encode(&header, &claims, &self.encoding_key)?;
        Ok((token, claims))
    }

    /// Generates a token with an expiration timestamp set 60 seconds in the past.
    /// This strictly exceeds the 10-second validation clock-skew leeway, guaranteeing deterministic rejection as `AuthTokenError::Expired`.
    pub fn generate_expired_token(
        &self,
        user_id: &str,
        email: &str,
        role: &str,
        tier: &str,
    ) -> Result<(String, Claims), AuthTokenError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AuthTokenError::ClockError(e.to_string()))?
            .as_secs();

        let exp = now.saturating_sub(60) as usize;
        let iat = now.saturating_sub(120) as usize;

        self.generate_token_with_exp(user_id, email, role, tier, exp, iat)
    }

    pub fn verify_token(&self, token: &str) -> Result<Claims, AuthTokenError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 10; // 10-second clock skew leeway
        validation.validate_exp = true;

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;
        Ok(token_data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_generation_and_verification() {
        let secret = "my_super_secret_jwt_key_at_least_32_bytes_long!";
        let engine = JwtEngine::new(secret, 900);

        let (token, claims) = engine
            .generate_token("usr_123", "alice@example.com", "user", "premium")
            .expect("token generation");

        assert_eq!(claims.sub, "usr_123");
        assert_eq!(claims.email, "alice@example.com");
        assert_eq!(claims.role, "user");
        assert_eq!(claims.tier, "premium");
        assert_eq!(claims.exp - claims.iat, 900);

        let verified = engine.verify_token(&token).expect("verify failed");
        assert_eq!(claims, verified);
    }

    #[test]
    fn test_jwt_expired_token_rejection() {
        let secret = "my_super_secret_jwt_key_at_least_32_bytes_long!";
        let engine = JwtEngine::new(secret, 900);

        // 1. Helper-generated token expired 60 seconds ago (> 10s leeway)
        let (token, _) = engine
            .generate_expired_token("usr_expired", "bob@example.com", "user", "free")
            .expect("token generation");

        let res = engine.verify_token(&token);
        assert!(
            matches!(res, Err(AuthTokenError::Expired)),
            "Expired token must be rejected with AuthTokenError::Expired"
        );

        // 2. Explicit past timestamp construction (UNIX timestamp 1_000, year 1970)
        let (token_past, _) = engine
            .generate_token_with_exp("usr_past", "bob@example.com", "user", "free", 1_000, 500)
            .expect("token generation");

        let res_past = engine.verify_token(&token_past);
        assert!(
            matches!(res_past, Err(AuthTokenError::Expired)),
            "Token with past exp must be rejected with AuthTokenError::Expired"
        );
    }

    #[test]
    fn test_jwt_tamper_signature_rejection() {
        let secret = "my_super_secret_jwt_key_at_least_32_bytes_long!";
        let engine = JwtEngine::new(secret, 900);

        let (token, _) = engine
            .generate_token("usr_123", "alice@example.com", "user", "free")
            .unwrap();

        let mut tampered = token.clone();
        tampered.push('x');

        let res = engine.verify_token(&tampered);
        assert!(matches!(res, Err(AuthTokenError::Invalid(_))));
    }

    #[test]
    fn test_jwt_wrong_secret_rejection() {
        let engine1 = JwtEngine::new("secret_key_one_at_least_32_bytes_long!", 900);
        let engine2 = JwtEngine::new("secret_key_two_at_least_32_bytes_long!", 900);

        let (token, _) = engine1
            .generate_token("usr_123", "alice@example.com", "user", "free")
            .unwrap();

        let res = engine2.verify_token(&token);
        assert!(matches!(res, Err(AuthTokenError::Invalid(_))));
    }
}
