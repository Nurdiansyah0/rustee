use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::AppError;
use crate::repository::UserRepository;
use crate::service::crypto::CryptoService;
use crate::service::jwt::JwtEngine;
use crate::service::tenant_service::TenantService;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    #[serde(alias = "name")]
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    #[serde(alias = "identifier", alias = "username")]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDto {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub currency: String,
    pub role: String,
    pub subscription_tier: String,
    pub account_type: String,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthResponse {
    pub user: UserDto,
    pub token: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserProfileResponse {
    pub user: UserDto,
    pub permissions: Vec<String>,
}

pub struct AuthService {
    pool: SqlitePool,
    user_repo: Arc<dyn UserRepository>,
    crypto_service: Arc<CryptoService>,
    jwt_engine: Arc<JwtEngine>,
}

impl AuthService {
    pub fn new(
        pool: SqlitePool,
        user_repo: Arc<dyn UserRepository>,
        crypto_service: Arc<CryptoService>,
        jwt_engine: Arc<JwtEngine>,
    ) -> Self {
        Self {
            pool,
            user_repo,
            crypto_service,
            jwt_engine,
        }
    }

    pub fn jwt_engine(&self) -> &JwtEngine {
        &self.jwt_engine
    }

    pub fn jwt_engine_arc(&self) -> Arc<JwtEngine> {
        Arc::clone(&self.jwt_engine)
    }

    pub fn crypto_service(&self) -> Arc<CryptoService> {
        Arc::clone(&self.crypto_service)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Normalizes and validates email format
    pub fn normalize_email(email: &str) -> Result<String, AppError> {
        let trimmed = email.trim().to_lowercase();
        if trimmed.is_empty() || !trimmed.contains('@') || !trimmed.contains('.') {
            return Err(AppError::BadRequest(
                "A valid email address is required".to_string(),
                "INVALID_EMAIL_FORMAT",
            ));
        }
        Ok(trimmed)
    }

    /// Computes active permissions based on user subscription tier
    pub fn permissions_for_tier(tier: &str) -> Vec<String> {
        match tier.to_lowercase().as_str() {
            "premium" => vec![
                "transactions.basic".to_string(),
                "analytics.advanced".to_string(),
                "budgeting".to_string(),
                "reports.advanced".to_string(),
            ],
            _ => vec!["transactions.basic".to_string()],
        }
    }

    /// Registers a new user, atomically seeding a default Cash wallet and starter categories
    pub async fn register(&self, req: RegisterRequest) -> Result<AuthResponse, AppError> {
        let normalized_email = Self::normalize_email(&req.email)?;
        CryptoService::validate_password_strength(&req.password)?;

        let display_name = req.display_name.trim();
        if display_name.is_empty() {
            return Err(AppError::BadRequest(
                "Display name cannot be blank".to_string(),
                "DISPLAY_NAME_REQUIRED",
            ));
        }

        // Check if email is already registered (case-insensitive)
        if self
            .user_repo
            .find_by_email(&normalized_email)
            .await
            .map_err(AppError::from)?
            .is_some()
        {
            return Err(AppError::Conflict(
                "An account with this email address already exists".to_string(),
                "EMAIL_ALREADY_EXISTS",
            ));
        }

        // Hash password offloaded to worker thread via spawn_blocking
        let password_hash = self
            .crypto_service
            .hash_password(req.password)
            .await
            .map_err(AppError::from)?;

        let user_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // Transactional execution: User + Default Cash Account + Starter Categories
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;

        // 1. Insert User
        sqlx::query(
            r#"
            INSERT INTO users (id, email, password_hash, display_name, currency, role, subscription_tier, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 'IDR', 'user', 'free', ?5, ?5)
            "#,
        )
        .bind(&user_id)
        .bind(&normalized_email)
        .bind(&password_hash)
        .bind(display_name)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        // 2. Seed Default Wallet Account ("Cash")
        let account_id = format!("acc_{}", Uuid::new_v4());
        sqlx::query(
            r#"
            INSERT INTO accounts (id, user_id, name, account_type, currency, initial_balance, current_balance, color, icon, is_archived, created_at, updated_at)
            VALUES (?1, ?2, 'Cash', 'cash', 'IDR', 0, 0, '#10B981', 'banknotes', 0, ?3, ?3)
            "#,
        )
        .bind(&account_id)
        .bind(&user_id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        // 3. Seed Default Starter Categories (User-personalized with is_system = 0)
        let starter_categories = [
            ("Salary", "income", "#3B82F6", "briefcase"),
            ("Freelance", "income", "#8B5CF6", "laptop"),
            ("Investments", "income", "#10B981", "trending-up"),
            ("Food & Beverage", "expense", "#EF4444", "utensils"),
            ("Transportation", "expense", "#F59E0B", "car"),
            ("Housing & Utilities", "expense", "#6366F1", "home"),
            ("Health & Medical", "expense", "#EC4899", "heart"),
            ("Entertainment", "expense", "#14B8A6", "film"),
            ("Shopping", "expense", "#F97316", "shopping-bag"),
            ("Other Expense", "expense", "#6B7280", "tag"),
        ];

        for (name, cat_type, color, icon) in starter_categories {
            let cat_id = format!("cat_{}", Uuid::new_v4());
            let norm_name = name.to_lowercase();
            sqlx::query(
                r#"
                INSERT INTO categories (id, user_id, name, category_type, icon, color, is_system, display_name, normalized_name, metadata, deleted_at, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, NULL, NULL, ?9, ?9)
                "#,
            )
            .bind(&cat_id)
            .bind(&user_id)
            .bind(name)
            .bind(cat_type)
            .bind(icon)
            .bind(color)
            .bind(name)
            .bind(&norm_name)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        // 4. Auto-provision default personal workspace (multi-tenant core foundation)
        TenantService::provision_personal_workspace(&mut tx, &user_id, display_name).await?;

        // Commit transaction
        tx.commit().await?;

        // Generate JWT token
        let (token, _) = self
            .jwt_engine
            .generate_token(&user_id, &normalized_email, "user", "free")
            .map_err(AppError::from)?;

        let user_dto = UserDto {
            id: user_id,
            email: normalized_email,
            display_name: display_name.to_string(),
            currency: "IDR".to_string(),
            role: "user".to_string(),
            subscription_tier: "free".to_string(),
            account_type: "owner".to_string(),
            username: None,
            phone: None,
            created_at: now,
        };

        let permissions = Self::permissions_for_tier("free");

        Ok(AuthResponse {
            user: user_dto,
            token,
            permissions,
        })
    }

    /// Authenticates user credentials via email, username, or phone number
    pub async fn login(&self, req: LoginRequest) -> Result<AuthResponse, AppError> {
        let identifier = req.email.trim();
        if identifier.is_empty() {
            return Err(AppError::BadRequest(
                "Email, username, or phone number is required".to_string(),
                "IDENTIFIER_REQUIRED",
            ));
        }

        let user_opt = self
            .user_repo
            .find_by_identifier(identifier)
            .await
            .map_err(AppError::from)?;

        match user_opt {
            Some(user) => {
                let valid = self
                    .crypto_service
                    .verify_password(req.password, &user.password_hash)
                    .await
                    .unwrap_or(false);

                if !valid {
                    return Err(AppError::Unauthorized(
                        "Invalid email, username, or password".to_string(),
                        "INVALID_CREDENTIALS",
                    ));
                }

                let (token, _) = self
                    .jwt_engine
                    .generate_token(&user.id, &user.email, &user.role, &user.subscription_tier)
                    .map_err(AppError::from)?;

                let permissions = Self::permissions_for_tier(&user.subscription_tier);

                let user_dto = UserDto {
                    id: user.id,
                    email: user.email,
                    display_name: user.display_name,
                    currency: user.currency,
                    role: user.role,
                    subscription_tier: user.subscription_tier,
                    account_type: user.account_type,
                    username: user.username,
                    phone: user.phone,
                    created_at: user.created_at,
                };

                Ok(AuthResponse {
                    user: user_dto,
                    token,
                    permissions,
                })
            }
            None => {
                // Constant-time execution defense: execute dummy verification
                self.crypto_service
                    .verify_or_dummy(req.password, None)
                    .await;

                Err(AppError::Unauthorized(
                    "Invalid email, username, or password".to_string(),
                    "INVALID_CREDENTIALS",
                ))
            }
        }
    }

    /// Fetches user profile and real-time active permissions
    pub async fn get_me(&self, user_id: &str) -> Result<UserProfileResponse, AppError> {
        let user = self
            .user_repo
            .find_by_id(user_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("User account not found".to_string(), "USER_NOT_FOUND")
            })?;

        let permissions = Self::permissions_for_tier(&user.subscription_tier);

        let user_dto = UserDto {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            currency: user.currency,
            role: user.role,
            subscription_tier: user.subscription_tier,
            account_type: user.account_type,
            username: user.username,
            phone: user.phone,
            created_at: user.created_at,
        };

        Ok(UserProfileResponse {
            user: user_dto,
            permissions,
        })
    }

    /// Initiates password recovery process (timing-safe against email enumeration)
    pub async fn forgot_password(&self, email: &str) -> Result<(), AppError> {
        let normalized_email = email.trim().to_lowercase();
        if normalized_email.is_empty() || !normalized_email.contains('@') {
            return Err(AppError::BadRequest(
                "Format alamat email tidak valid".to_string(),
                "INVALID_EMAIL",
            ));
        }

        let user_opt = self.user_repo.find_by_email(&normalized_email).await.map_err(AppError::from)?;

        match user_opt {
            Some(user) => {
                let raw_token = format!(
                    "{}{}",
                    Uuid::new_v4().simple(),
                    Uuid::new_v4().simple()
                );
                let mut hasher = Sha256::new();
                hasher.update(raw_token.as_bytes());
                let token_hash = format!("{:x}", hasher.finalize());

                let now = Utc::now();
                let expires_at = (now + chrono::Duration::hours(1)).to_rfc3339();
                let now_str = now.to_rfc3339();
                let token_id = format!("rst_{}", &raw_token[..12]);

                // Ensure table exists and insert token
                let _ = sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS password_reset_tokens (
                        id TEXT PRIMARY KEY NOT NULL,
                        user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                        token_hash TEXT NOT NULL UNIQUE,
                        expires_at TEXT NOT NULL,
                        used_at TEXT,
                        created_at TEXT NOT NULL
                    )
                    "#,
                )
                .execute(&self.pool)
                .await;

                let _ = sqlx::query(
                    r#"
                    INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&token_id)
                .bind(&user.id)
                .bind(&token_hash)
                .bind(&expires_at)
                .bind(&now_str)
                .execute(&self.pool)
                .await;

                // Send email via EmailService
                let email_service = crate::service::email_service::EmailService::with_default_config();
                let _ = email_service
                    .send_password_recovery(&user.email, &user.display_name, &raw_token)
                    .await;
            }
            None => {
                // Constant-time dummy verification against timing attacks
                self.crypto_service
                    .verify_or_dummy("dummy_timing_safe_pass".to_string(), None)
                    .await;
            }
        }

        Ok(())
    }

    /// Completes password recovery using a single-use token
    pub async fn reset_password(&self, raw_token: &str, new_password: &str) -> Result<(), AppError> {
        let trimmed_token = raw_token.trim();
        if trimmed_token.is_empty() {
            return Err(AppError::BadRequest(
                "Tautan pemulihan tidak valid".to_string(),
                "INVALID_TOKEN",
            ));
        }

        CryptoService::validate_password_strength(new_password)?;

        let mut hasher = Sha256::new();
        hasher.update(trimmed_token.as_bytes());
        let token_hash = format!("{:x}", hasher.finalize());

        #[derive(sqlx::FromRow)]
        struct TokenRow {
            id: String,
            user_id: String,
            expires_at: String,
            used_at: Option<String>,
        }

        let token_row = sqlx::query_as::<_, TokenRow>(
            "SELECT id, user_id, expires_at, used_at FROM password_reset_tokens WHERE token_hash = ?",
        )
        .bind(&token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Database query failure: {}", e)))?;

        let row = match token_row {
            Some(r) => r,
            None => {
                return Err(AppError::BadRequest(
                    "Tautan pemulihan tidak valid atau telah kedaluwarsa".to_string(),
                    "INVALID_OR_EXPIRED_TOKEN",
                ))
            }
        };

        if row.used_at.is_some() {
            return Err(AppError::BadRequest(
                "Tautan pemulihan ini telah digunakan".to_string(),
                "TOKEN_ALREADY_USED",
            ));
        }

        let expires_at_dt = chrono::DateTime::parse_from_rfc3339(&row.expires_at)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| AppError::Internal("Invalid expiration timestamp".to_string()))?;

        if Utc::now() > expires_at_dt {
            return Err(AppError::BadRequest(
                "Tautan pemulihan telah kedaluwarsa (maksimal 1 jam)".to_string(),
                "TOKEN_EXPIRED",
            ));
        }

        let password_hash = self
            .crypto_service
            .hash_password(new_password.to_string())
            .await
            .map_err(AppError::from)?;

        let now_str = Utc::now().to_rfc3339();

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(|e| AppError::Internal(e.to_string()))?;

        sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
            .bind(&password_hash)
            .bind(&now_str)
            .bind(&row.user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        sqlx::query("UPDATE password_reset_tokens SET used_at = ? WHERE id = ?")
            .bind(&now_str)
            .bind(&row.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        tx.commit().await.map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }
}
