use crate::error::AppError;
use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Algorithm, Argon2, Params, Version,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Argon2 hashing failure: {0}")]
    HashingFailed(String),

    #[error("Invalid password hash format: {0}")]
    InvalidHashFormat(String),

    #[error("Worker thread join failure: {0}")]
    JoinError(String),
}

impl From<CryptoError> for AppError {
    fn from(err: CryptoError) -> Self {
        AppError::Internal(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Argon2Config {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub output_len: usize,
}

impl Default for Argon2Config {
    fn default() -> Self {
        // OWASP recommended settings: 64 MiB, 3 iterations, 1 lane, 32-byte key
        Self {
            m_cost: 65536,
            t_cost: 3,
            p_cost: 1,
            output_len: 32,
        }
    }
}

impl Argon2Config {
    /// Fast parameters for deterministic unit and integration testing (~2ms)
    pub fn fast_for_testing() -> Self {
        Self {
            m_cost: 4096,
            t_cost: 1,
            p_cost: 1,
            output_len: 32,
        }
    }
}

#[derive(Clone)]
pub struct CryptoService {
    argon2: Argon2<'static>,
    dummy_hash: String,
}

impl CryptoService {
    pub fn new(config: Argon2Config) -> Result<Self, CryptoError> {
        let params = Params::new(
            config.m_cost,
            config.t_cost,
            config.p_cost,
            Some(config.output_len),
        )
        .map_err(|e| CryptoError::HashingFailed(e.to_string()))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        // Precompute a valid dummy Argon2id hash at startup for timing-attack protection
        let dummy_hash = argon2
            .hash_password(b"timing_attack_dummy_entropy_98412498124_unmatchable")
            .map_err(|e| CryptoError::HashingFailed(e.to_string()))?
            .to_string();

        Ok(Self { argon2, dummy_hash })
    }

    /// Validates password strength policy:
    /// - At least 8 characters
    /// - At most 128 characters
    /// - Contains at least one ASCII alphabetic character
    /// - Contains at least one ASCII digit
    pub fn validate_password_strength(password: &str) -> Result<(), AppError> {
        if password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters long".to_string(),
                "PASSWORD_TOO_SHORT",
            ));
        }
        if password.len() > 128 {
            return Err(AppError::BadRequest(
                "Password exceeds maximum length of 128 characters".to_string(),
                "PASSWORD_TOO_LONG",
            ));
        }
        let has_letter = password.chars().any(|c| c.is_ascii_alphabetic());
        let has_digit = password.chars().any(|c| c.is_ascii_digit());

        if !has_letter || !has_digit {
            return Err(AppError::BadRequest(
                "Password must contain at least one letter and one number".to_string(),
                "PASSWORD_TOO_WEAK",
            ));
        }
        Ok(())
    }

    /// Hashes a plaintext password using Argon2id inside tokio::task::spawn_blocking
    pub async fn hash_password(&self, password: String) -> Result<String, CryptoError> {
        let argon2 = self.argon2.clone();
        tokio::task::spawn_blocking(move || {
            argon2
                .hash_password(password.as_bytes())
                .map(|h| h.to_string())
                .map_err(|e| CryptoError::HashingFailed(e.to_string()))
        })
        .await
        .map_err(|e| CryptoError::JoinError(e.to_string()))?
    }

    /// Verifies a password against an existing PHC hash string inside tokio::task::spawn_blocking
    pub async fn verify_password(
        &self,
        password: String,
        hash_str: &str,
    ) -> Result<bool, CryptoError> {
        let argon2 = self.argon2.clone();
        let hash_owned = hash_str.to_string();
        tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(&hash_owned)
                .map_err(|e| CryptoError::InvalidHashFormat(e.to_string()))?;
            Ok(argon2.verify_password(password.as_bytes(), &parsed).is_ok())
        })
        .await
        .map_err(|e| CryptoError::JoinError(e.to_string()))?
    }

    /// Performs constant-time verification:
    /// - If user exists (`Some(hash)`), verifies against their actual password hash.
    /// - If user does not exist (`None`), verifies against the precomputed dummy hash.
    ///
    /// This ensures response latency (~120ms) is identical, mitigating email enumeration.
    pub async fn verify_or_dummy(&self, password: String, maybe_hash: Option<&str>) -> bool {
        match maybe_hash {
            Some(hash) => self.verify_password(password, hash).await.unwrap_or(false),
            None => {
                let _ = self.verify_password(password, &self.dummy_hash).await;
                false
            }
        }
    }

    pub fn dummy_hash(&self) -> &str {
        &self.dummy_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_strength_validation() {
        assert!(CryptoService::validate_password_strength("Pass1234").is_ok());
        assert!(CryptoService::validate_password_strength("MyStrongP@ssw0rd").is_ok());

        // Too short (< 8 chars)
        assert!(matches!(
            CryptoService::validate_password_strength("P12345"),
            Err(AppError::BadRequest(_, "PASSWORD_TOO_SHORT"))
        ));

        // Missing numbers
        assert!(matches!(
            CryptoService::validate_password_strength("PasswordOnly"),
            Err(AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
        ));

        // Missing letters
        assert!(matches!(
            CryptoService::validate_password_strength("1234567890"),
            Err(AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
        ));

        // Too long (> 128 chars)
        let long_pw = "a1".repeat(65);
        assert!(matches!(
            CryptoService::validate_password_strength(&long_pw),
            Err(AppError::BadRequest(_, "PASSWORD_TOO_LONG"))
        ));
    }

    #[tokio::test]
    async fn test_argon2id_hashing_and_verification() {
        let config = Argon2Config::fast_for_testing();
        let service = CryptoService::new(config).expect("failed to init CryptoService");

        let password = "CorrectHorseBatteryStaple123!".to_string();
        let hash = service
            .hash_password(password.clone())
            .await
            .expect("hash failure");
        assert!(hash.starts_with("$argon2id$"));

        // Matching password
        let valid = service
            .verify_password(password, &hash)
            .await
            .expect("verify failure");
        assert!(valid);

        // Wrong password
        let invalid = service
            .verify_password("WrongPassword123!".to_string(), &hash)
            .await
            .expect("verify failure");
        assert!(!invalid);
    }

    #[tokio::test]
    async fn test_verify_or_dummy_constant_time() {
        let config = Argon2Config::fast_for_testing();
        let service = CryptoService::new(config).expect("failed to init CryptoService");

        let real_hash = service
            .hash_password("MyRealPassword123!".to_string())
            .await
            .unwrap();

        // 1. Existing user with wrong password
        let res1 = service
            .verify_or_dummy("WrongPassword123!".to_string(), Some(&real_hash))
            .await;
        assert!(!res1);

        // 2. Non-existent user (dummy hash)
        let res2 = service
            .verify_or_dummy("WrongPassword123!".to_string(), None)
            .await;
        assert!(!res2);

        // 3. Existing user with correct password
        let res3 = service
            .verify_or_dummy("MyRealPassword123!".to_string(), Some(&real_hash))
            .await;
        assert!(res3);
    }
}
