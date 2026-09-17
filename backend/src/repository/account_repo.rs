use crate::domain::money::Rupiah;
use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub account_type: String,
    pub currency: String,
    pub initial_balance: Rupiah,
    pub current_balance: Rupiah,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewAccount {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub account_type: String,
    pub currency: Option<String>,
    pub initial_balance: Rupiah,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn create(&self, account: &NewAccount) -> Result<Account, DbError>;
    async fn find_by_id(&self, user_id: &str, id: &str) -> Result<Option<Account>, DbError>;
    async fn list_by_user(
        &self,
        user_id: &str,
        include_archived: bool,
    ) -> Result<Vec<Account>, DbError>;
    async fn update_balance(
        &self,
        user_id: &str,
        id: &str,
        new_balance: Rupiah,
    ) -> Result<(), DbError>;
    async fn adjust_balance_atomic(
        &self,
        user_id: &str,
        id: &str,
        delta: i64,
    ) -> Result<Rupiah, DbError>;
    async fn archive(&self, user_id: &str, id: &str) -> Result<(), DbError>;
}

pub struct SqlxAccountRepository {
    pool: SqlitePool,
}

impl SqlxAccountRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Transactional multi-table atomic balance adjustment helper
    pub async fn adjust_balance_atomic_tx(
        tx: &mut Transaction<'_, Sqlite>,
        user_id: &str,
        id: &str,
        delta: i64,
    ) -> Result<Rupiah, DbError> {
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query_scalar::<_, i64>(
            r#"
            UPDATE accounts
            SET current_balance = current_balance + ?1, updated_at = ?2
            WHERE id = ?3 AND user_id = ?4
            RETURNING current_balance
            "#,
        )
        .bind(delta)
        .bind(&now)
        .bind(id)
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        match row {
            Some(balance) => Ok(Rupiah(balance)),
            None => Err(DbError::NotFound),
        }
    }
}

#[async_trait]
impl AccountRepository for SqlxAccountRepository {
    async fn create(&self, account: &NewAccount) -> Result<Account, DbError> {
        let now = Utc::now().to_rfc3339();
        let currency = account.currency.as_deref().unwrap_or("IDR");

        sqlx::query(
            r#"
            INSERT INTO accounts (id, user_id, name, account_type, currency, initial_balance, current_balance, color, icon, is_archived, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11)
            "#
        )
        .bind(&account.id)
        .bind(&account.user_id)
        .bind(&account.name)
        .bind(&account.account_type)
        .bind(currency)
        .bind(account.initial_balance.0)
        .bind(account.initial_balance.0)
        .bind(&account.color)
        .bind(&account.icon)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Account {
            id: account.id.clone(),
            user_id: account.user_id.clone(),
            name: account.name.clone(),
            account_type: account.account_type.clone(),
            currency: currency.to_string(),
            initial_balance: account.initial_balance,
            current_balance: account.initial_balance,
            color: account.color.clone(),
            icon: account.icon.clone(),
            is_archived: false,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find_by_id(&self, user_id: &str, id: &str) -> Result<Option<Account>, DbError> {
        let account = sqlx::query_as::<_, Account>(
            r#"
            SELECT id, user_id, name, account_type, currency, initial_balance, current_balance, color, icon, is_archived, created_at, updated_at
            FROM accounts
            WHERE id = ?1 AND user_id = ?2
            "#
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(account)
    }

    async fn list_by_user(
        &self,
        user_id: &str,
        include_archived: bool,
    ) -> Result<Vec<Account>, DbError> {
        let accounts = sqlx::query_as::<_, Account>(
            r#"
            SELECT id, user_id, name, account_type, currency, initial_balance, current_balance, color, icon, is_archived, created_at, updated_at
            FROM accounts
            WHERE user_id = ?1 AND (is_archived = 0 OR ?2 = 1)
            ORDER BY created_at ASC
            "#
        )
        .bind(user_id)
        .bind(if include_archived { 1 } else { 0 })
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(accounts)
    }

    async fn update_balance(
        &self,
        user_id: &str,
        id: &str,
        new_balance: Rupiah,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE accounts
            SET current_balance = ?1, updated_at = ?2
            WHERE id = ?3 AND user_id = ?4
            "#,
        )
        .bind(new_balance.0)
        .bind(&now)
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn adjust_balance_atomic(
        &self,
        user_id: &str,
        id: &str,
        delta: i64,
    ) -> Result<Rupiah, DbError> {
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query_scalar::<_, i64>(
            r#"
            UPDATE accounts
            SET current_balance = current_balance + ?1, updated_at = ?2
            WHERE id = ?3 AND user_id = ?4
            RETURNING current_balance
            "#,
        )
        .bind(delta)
        .bind(&now)
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        match row {
            Some(balance) => Ok(Rupiah(balance)),
            None => Err(DbError::NotFound),
        }
    }

    async fn archive(&self, user_id: &str, id: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE accounts
            SET is_archived = 1, updated_at = ?1
            WHERE id = ?2 AND user_id = ?3
            "#,
        )
        .bind(&now)
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }
}
