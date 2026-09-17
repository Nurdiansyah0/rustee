use crate::domain::money::Rupiah;
use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction as DbTransaction};

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct TransactionRecord {
    pub id: String,
    pub user_id: String,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub category_id: Option<String>,
    pub transaction_type: String,
    pub amount: Rupiah,
    pub date: String,
    pub description: String,
    pub notes: Option<String>,
    pub is_recurring: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewTransaction {
    pub id: String,
    pub user_id: String,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub category_id: Option<String>,
    pub transaction_type: String,
    /// Strictly positive. Service layer applies sign for balance adjustments.
    pub amount: Rupiah,
    pub date: String,
    pub description: String,
    pub notes: Option<String>,
    pub is_recurring: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CashFlowSummary {
    pub total_income: Rupiah,
    pub total_expenses: Rupiah,
    pub net_cash_flow: Rupiah,
}

impl CashFlowSummary {
    /// Compute net cash flow from income and expenses.
    /// This is the SINGLE shared calculation — called by dashboard, analytics, and reports.
    pub fn compute(total_income: Rupiah, total_expenses: Rupiah) -> Self {
        let net_cash_flow = total_income
            .checked_sub(total_expenses)
            .expect("CashFlowSummary: net cash flow overflow — income and expense values exceeded i64 range");
        Self {
            total_income,
            total_expenses,
            net_cash_flow,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransactionFilter {
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub transaction_type: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub page: i64,
    pub per_page: i64,
}

impl Default for TransactionFilter {
    fn default() -> Self {
        Self {
            account_id: None,
            category_id: None,
            transaction_type: None,
            date_from: None,
            date_to: None,
            page: 1,
            per_page: 25,
        }
    }
}

#[async_trait]
pub trait TransactionRepository: Send + Sync {
    async fn create_in_tx(
        &self,
        tx: &mut DbTransaction<'_, Sqlite>,
        record: &NewTransaction,
    ) -> Result<TransactionRecord, DbError>;

    async fn find_by_id(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<Option<TransactionRecord>, DbError>;

    async fn list(
        &self,
        user_id: &str,
        filter: &TransactionFilter,
    ) -> Result<(Vec<TransactionRecord>, i64), DbError>;

    async fn delete_in_tx(
        &self,
        tx: &mut DbTransaction<'_, Sqlite>,
        user_id: &str,
        id: &str,
    ) -> Result<TransactionRecord, DbError>;

    async fn cash_flow_summary(
        &self,
        user_id: &str,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<CashFlowSummary, DbError>;
}

pub struct SqlxTransactionRepository {
    pool: SqlitePool,
}

impl SqlxTransactionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TransactionRepository for SqlxTransactionRepository {
    async fn create_in_tx(
        &self,
        tx: &mut DbTransaction<'_, Sqlite>,
        record: &NewTransaction,
    ) -> Result<TransactionRecord, DbError> {
        let now = Utc::now().to_rfc3339();
        let is_recurring = record.is_recurring as i32;

        sqlx::query(
            r#"
            INSERT INTO transactions
                (id, user_id, account_id, to_account_id, category_id, transaction_type,
                 amount, date, description, notes, is_recurring, created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
        )
        .bind(&record.id)
        .bind(&record.user_id)
        .bind(&record.account_id)
        .bind(&record.to_account_id)
        .bind(&record.category_id)
        .bind(&record.transaction_type)
        .bind(record.amount.0)
        .bind(&record.date)
        .bind(&record.description)
        .bind(&record.notes)
        .bind(is_recurring)
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(TransactionRecord {
            id: record.id.clone(),
            user_id: record.user_id.clone(),
            account_id: record.account_id.clone(),
            to_account_id: record.to_account_id.clone(),
            category_id: record.category_id.clone(),
            transaction_type: record.transaction_type.clone(),
            amount: record.amount,
            date: record.date.clone(),
            description: record.description.clone(),
            notes: record.notes.clone(),
            is_recurring: record.is_recurring,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find_by_id(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<Option<TransactionRecord>, DbError> {
        sqlx::query_as::<_, TransactionRecord>(
            r#"
            SELECT id, user_id, account_id, to_account_id, category_id, transaction_type,
                   amount, date, description, notes, is_recurring, created_at, updated_at
            FROM transactions
            WHERE id = ?1 AND user_id = ?2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn list(
        &self,
        user_id: &str,
        filter: &TransactionFilter,
    ) -> Result<(Vec<TransactionRecord>, i64), DbError> {
        let offset = (filter.page.saturating_sub(1)) * filter.per_page;
        let per_page = filter.per_page.min(100);

        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM transactions
            WHERE user_id = ?1
              AND (?2 IS NULL OR account_id = ?2)
              AND (?3 IS NULL OR category_id = ?3)
              AND (?4 IS NULL OR transaction_type = ?4)
              AND (?5 IS NULL OR date >= ?5)
              AND (?6 IS NULL OR date <= ?6)
            "#,
        )
        .bind(user_id)
        .bind(&filter.account_id)
        .bind(&filter.category_id)
        .bind(&filter.transaction_type)
        .bind(&filter.date_from)
        .bind(&filter.date_to)
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let records = sqlx::query_as::<_, TransactionRecord>(
            r#"
            SELECT id, user_id, account_id, to_account_id, category_id, transaction_type,
                   amount, date, description, notes, is_recurring, created_at, updated_at
            FROM transactions
            WHERE user_id = ?1
              AND (?2 IS NULL OR account_id = ?2)
              AND (?3 IS NULL OR category_id = ?3)
              AND (?4 IS NULL OR transaction_type = ?4)
              AND (?5 IS NULL OR date >= ?5)
              AND (?6 IS NULL OR date <= ?6)
            ORDER BY date DESC, created_at DESC
            LIMIT ?7 OFFSET ?8
            "#,
        )
        .bind(user_id)
        .bind(&filter.account_id)
        .bind(&filter.category_id)
        .bind(&filter.transaction_type)
        .bind(&filter.date_from)
        .bind(&filter.date_to)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok((records, total))
    }

    async fn delete_in_tx(
        &self,
        tx: &mut DbTransaction<'_, Sqlite>,
        user_id: &str,
        id: &str,
    ) -> Result<TransactionRecord, DbError> {
        let record = sqlx::query_as::<_, TransactionRecord>(
            r#"
            SELECT id, user_id, account_id, to_account_id, category_id, transaction_type,
                   amount, date, description, notes, is_recurring, created_at, updated_at
            FROM transactions
            WHERE id = ?1 AND user_id = ?2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?
        .ok_or(DbError::NotFound)?;

        sqlx::query("DELETE FROM transactions WHERE id = ?1 AND user_id = ?2")
            .bind(id)
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(DbError::from_sqlx)?;

        Ok(record)
    }

    async fn cash_flow_summary(
        &self,
        user_id: &str,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<CashFlowSummary, DbError> {
        let (income_raw, expense_raw): (Option<i64>, Option<i64>) = sqlx::query_as(
            r#"
            SELECT
                SUM(CASE WHEN transaction_type = 'income' THEN amount ELSE 0 END),
                SUM(CASE WHEN transaction_type = 'expense' THEN amount ELSE 0 END)
            FROM transactions
            WHERE user_id = ?1
              AND (?2 IS NULL OR date >= ?2)
              AND (?3 IS NULL OR date <= ?3)
            "#,
        )
        .bind(user_id)
        .bind(date_from)
        .bind(date_to)
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let total_income = Rupiah(income_raw.unwrap_or(0));
        let total_expenses = Rupiah(expense_raw.unwrap_or(0));
        Ok(CashFlowSummary::compute(total_income, total_expenses))
    }
}
