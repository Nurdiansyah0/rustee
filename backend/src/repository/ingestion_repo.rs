use crate::domain::money::Rupiah;
use crate::repository::account_repo::SqlxAccountRepository;
use crate::repository::transaction_repo::TransactionRecord;
use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct IngestionEventRecord {
    pub id: String,
    pub user_id: String,
    pub source: String,
    pub raw_payload_hash: String,
    pub status: String,
    pub confidence: String,
    pub parsed_candidate: Option<String>,
    pub created_at: String,
    pub processed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewIngestionEvent {
    pub id: String,
    pub user_id: String,
    pub source: String,
    pub raw_payload_hash: String,
    pub status: String,
    pub confidence: String,
    pub parsed_candidate: Option<String>,
    pub created_at: String,
    pub processed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateIngestedTxParams {
    pub transaction_id: String,
    pub user_id: String,
    pub account_id: String,
    pub category_id: Option<String>,
    pub transaction_type: String, // "income" or "expense"
    pub amount: Rupiah,
    pub date: String,
    pub description: String,
    pub notes: Option<String>,
    pub source: String,
    pub external_reference: Option<String>,
    pub merchant: Option<String>,
    pub confidence: String,
    pub ingestion_id: String,
}

#[async_trait]
pub trait IngestionRepository: Send + Sync {
    async fn create_event(
        &self,
        event: &NewIngestionEvent,
    ) -> Result<IngestionEventRecord, DbError>;

    async fn find_by_id(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<Option<IngestionEventRecord>, DbError>;

    async fn find_by_hash(
        &self,
        user_id: &str,
        raw_payload_hash: &str,
    ) -> Result<Option<IngestionEventRecord>, DbError>;

    async fn update_status(
        &self,
        user_id: &str,
        id: &str,
        status: &str,
        processed_at: Option<&str>,
        parsed_candidate: Option<&str>,
    ) -> Result<(), DbError>;

    async fn list_pending_candidates(
        &self,
        user_id: &str,
    ) -> Result<Vec<IngestionEventRecord>, DbError>;

    async fn list_recent_events(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionEventRecord>, DbError>;

    /// Atomically insert transaction, update account balance, and update ingestion event status.
    async fn commit_transaction_atomic(
        &self,
        params: &CreateIngestedTxParams,
    ) -> Result<(TransactionRecord, Rupiah), DbError>;
}

pub struct SqlxIngestionRepository {
    pool: SqlitePool,
}

impl SqlxIngestionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IngestionRepository for SqlxIngestionRepository {
    async fn create_event(
        &self,
        event: &NewIngestionEvent,
    ) -> Result<IngestionEventRecord, DbError> {
        sqlx::query(
            r#"
            INSERT INTO ingestion_events
                (id, user_id, source, raw_payload_hash, status, confidence, parsed_candidate, created_at, processed_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
        )
        .bind(&event.id)
        .bind(&event.user_id)
        .bind(&event.source)
        .bind(&event.raw_payload_hash)
        .bind(&event.status)
        .bind(&event.confidence)
        .bind(&event.parsed_candidate)
        .bind(&event.created_at)
        .bind(&event.processed_at)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(IngestionEventRecord {
            id: event.id.clone(),
            user_id: event.user_id.clone(),
            source: event.source.clone(),
            raw_payload_hash: event.raw_payload_hash.clone(),
            status: event.status.clone(),
            confidence: event.confidence.clone(),
            parsed_candidate: event.parsed_candidate.clone(),
            created_at: event.created_at.clone(),
            processed_at: event.processed_at.clone(),
        })
    }

    async fn find_by_id(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<Option<IngestionEventRecord>, DbError> {
        sqlx::query_as::<_, IngestionEventRecord>(
            r#"
            SELECT id, user_id, source, raw_payload_hash, status, confidence, parsed_candidate, created_at, processed_at
            FROM ingestion_events
            WHERE id = ?1 AND user_id = ?2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn find_by_hash(
        &self,
        user_id: &str,
        raw_payload_hash: &str,
    ) -> Result<Option<IngestionEventRecord>, DbError> {
        sqlx::query_as::<_, IngestionEventRecord>(
            r#"
            SELECT id, user_id, source, raw_payload_hash, status, confidence, parsed_candidate, created_at, processed_at
            FROM ingestion_events
            WHERE user_id = ?1 AND raw_payload_hash = ?2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(raw_payload_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn update_status(
        &self,
        user_id: &str,
        id: &str,
        status: &str,
        processed_at: Option<&str>,
        parsed_candidate: Option<&str>,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE ingestion_events
            SET status = ?1,
                processed_at = COALESCE(?2, processed_at),
                parsed_candidate = COALESCE(?3, parsed_candidate)
            WHERE id = ?4 AND user_id = ?5
            "#,
        )
        .bind(status)
        .bind(processed_at)
        .bind(parsed_candidate)
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn list_pending_candidates(
        &self,
        user_id: &str,
    ) -> Result<Vec<IngestionEventRecord>, DbError> {
        sqlx::query_as::<_, IngestionEventRecord>(
            r#"
            SELECT id, user_id, source, raw_payload_hash, status, confidence, parsed_candidate, created_at, processed_at
            FROM ingestion_events
            WHERE user_id = ?1 AND status = 'requires_confirmation'
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn list_recent_events(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<IngestionEventRecord>, DbError> {
        sqlx::query_as::<_, IngestionEventRecord>(
            r#"
            SELECT id, user_id, source, raw_payload_hash, status, confidence, parsed_candidate, created_at, processed_at
            FROM ingestion_events
            WHERE user_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn commit_transaction_atomic(
        &self,
        params: &CreateIngestedTxParams,
    ) -> Result<(TransactionRecord, Rupiah), DbError> {
        let mut tx = self.pool.begin().await.map_err(DbError::from_sqlx)?;
        let now = Utc::now().to_rfc3339();

        // 1. Insert transaction row with source, external_reference, merchant, confidence, ingestion_id
        sqlx::query(
            r#"
            INSERT INTO transactions
                (id, user_id, account_id, to_account_id, category_id, transaction_type,
                 amount, date, description, notes, is_recurring, created_at, updated_at,
                 source, external_reference, merchant, confidence, ingestion_id)
            VALUES
                (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
        )
        .bind(&params.transaction_id)
        .bind(&params.user_id)
        .bind(&params.account_id)
        .bind(&params.category_id)
        .bind(&params.transaction_type)
        .bind(params.amount.0)
        .bind(&params.date)
        .bind(&params.description)
        .bind(&params.notes)
        .bind(&now)
        .bind(&now)
        .bind(&params.source)
        .bind(&params.external_reference)
        .bind(&params.merchant)
        .bind(&params.confidence)
        .bind(&params.ingestion_id)
        .execute(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        // 2. Adjust account balance atomically
        let delta = match params.transaction_type.as_str() {
            "income" => params.amount.0,
            "expense" => -params.amount.0,
            _ => {
                let _ = tx.rollback().await;
                return Err(DbError::Validation(
                    "Invalid transaction direction".to_string(),
                ));
            }
        };

        let new_balance = SqlxAccountRepository::adjust_balance_atomic_tx(
            &mut tx,
            &params.user_id,
            &params.account_id,
            delta,
        )
        .await?;

        // 3. Update ingestion event status to auto_created
        sqlx::query(
            r#"
            UPDATE ingestion_events
            SET status = 'auto_created',
                processed_at = ?1
            WHERE id = ?2 AND user_id = ?3
            "#,
        )
        .bind(&now)
        .bind(&params.ingestion_id)
        .bind(&params.user_id)
        .execute(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        tx.commit().await.map_err(DbError::from_sqlx)?;

        let record = TransactionRecord {
            id: params.transaction_id.clone(),
            user_id: params.user_id.clone(),
            account_id: params.account_id.clone(),
            to_account_id: None,
            category_id: params.category_id.clone(),
            transaction_type: params.transaction_type.clone(),
            amount: params.amount,
            date: params.date.clone(),
            description: params.description.clone(),
            notes: params.notes.clone(),
            is_recurring: false,
            created_at: now.clone(),
            updated_at: now,
        };

        Ok((record, new_balance))
    }
}
