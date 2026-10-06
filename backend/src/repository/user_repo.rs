use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub currency: String,
    pub role: String,
    pub subscription_tier: String,
    pub account_type: String,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub currency: Option<String>,
    pub role: Option<String>,
    pub subscription_tier: Option<String>,
    pub account_type: Option<String>,
    pub username: Option<String>,
    pub phone: Option<String>,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, user: &NewUser) -> Result<User, DbError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<User>, DbError>;
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbError>;
    async fn find_by_identifier(&self, identifier: &str) -> Result<Option<User>, DbError>;
    async fn update_tier(&self, id: &str, tier: &str) -> Result<(), DbError>;
    async fn update_password_hash(&self, id: &str, password_hash: &str) -> Result<(), DbError>;
}

pub struct SqlxUserRepository {
    pool: SqlitePool,
}

impl SqlxUserRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for SqlxUserRepository {
    async fn create(&self, user: &NewUser) -> Result<User, DbError> {
        let now = Utc::now().to_rfc3339();
        let currency = user.currency.as_deref().unwrap_or("IDR");
        let role = user.role.as_deref().unwrap_or("user");
        let tier = user.subscription_tier.as_deref().unwrap_or("free");
        let acc_type = user.account_type.as_deref().unwrap_or("owner");

        sqlx::query(
            r#"
            INSERT INTO users (id, email, password_hash, display_name, currency, role, subscription_tier, account_type, username, phone, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
            "#
        )
        .bind(&user.id)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.display_name)
        .bind(currency)
        .bind(role)
        .bind(tier)
        .bind(acc_type)
        .bind(&user.username)
        .bind(&user.phone)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(User {
            id: user.id.clone(),
            email: user.email.clone(),
            password_hash: user.password_hash.clone(),
            display_name: user.display_name.clone(),
            currency: currency.to_string(),
            role: role.to_string(),
            subscription_tier: tier.to_string(),
            account_type: acc_type.to_string(),
            username: user.username.clone(),
            phone: user.phone.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<User>, DbError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, display_name, currency, role, subscription_tier,
                   COALESCE(account_type, 'owner') as account_type,
                   username, phone, created_at, updated_at
            FROM users
            WHERE id = ?1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(user)
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbError> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, display_name, currency, role, subscription_tier,
                   COALESCE(account_type, 'owner') as account_type,
                   username, phone, created_at, updated_at
            FROM users
            WHERE LOWER(email) = LOWER(?1)
            "#
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(user)
    }

    async fn find_by_identifier(&self, identifier: &str) -> Result<Option<User>, DbError> {
        let clean = identifier.trim().to_lowercase();
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, password_hash, display_name, currency, role, subscription_tier,
                   COALESCE(account_type, 'owner') as account_type,
                   username, phone, created_at, updated_at
            FROM users
            WHERE LOWER(email) = ?1
               OR (username IS NOT NULL AND LOWER(username) = ?1)
               OR (phone IS NOT NULL AND phone = ?1)
            LIMIT 1
            "#
        )
        .bind(&clean)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(user)
    }

    async fn update_tier(&self, id: &str, tier: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE users
            SET subscription_tier = ?1, updated_at = ?2
            WHERE id = ?3
            "#,
        )
        .bind(tier)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn update_password_hash(&self, id: &str, password_hash: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = ?1, updated_at = ?2
            WHERE id = ?3
            "#,
        )
        .bind(password_hash)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }
}
