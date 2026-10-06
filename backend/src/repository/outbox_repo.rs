//! Transactional Outbox Repository Implementation
//! Enforces atomic event commits, batch polling, and status transitions.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::domain::outbox::{format_utc_iso_z, now_utc_iso_z, OutboxEvent, OutboxEventDraft};
use crate::domain::tenant::TenantContext;
use crate::repository::DbError;

#[async_trait]
pub trait OutboxRepository: Send + Sync {
    /// Inserts an outbox event atomically within an ongoing transaction
    async fn insert_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        draft: &OutboxEventDraft,
    ) -> Result<OutboxEvent, DbError>;

    /// Standalone insertion outside of external transaction
    async fn insert(&self, draft: &OutboxEventDraft) -> Result<OutboxEvent, DbError>;

    /// Retrieves an event by ID scoped to tenant
    async fn get_by_id(&self, ctx: &TenantContext, id: &str) -> Result<Option<OutboxEvent>, DbError>;

    /// Lists events for a tenant with optional status filter
    async fn list_by_tenant(
        &self,
        ctx: &TenantContext,
        status_filter: Option<&str>,
        limit: u32,
    ) -> Result<Vec<OutboxEvent>, DbError>;

    /// Queries events by aggregate type and aggregate ID
    async fn list_by_aggregate(
        &self,
        ctx: &TenantContext,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> Result<Vec<OutboxEvent>, DbError>;

    /// Polls a batch of pending/failed events due for dispatch
    async fn fetch_pending_batch(
        &self,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<Vec<OutboxEvent>, DbError>;

    /// Atomically transitions selected events from PENDING/FAILED to PROCESSING
    async fn claim_batch_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ids: &[String],
    ) -> Result<Vec<OutboxEvent>, DbError>;

    /// Marks an event as published
    async fn mark_published_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        published_at: DateTime<Utc>,
    ) -> Result<(), DbError>;

    /// Records failure, increments retry/attempt counters, and schedules next retry
    async fn mark_failed_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        error_msg: &str,
        next_retry: DateTime<Utc>,
    ) -> Result<(), DbError>;

    /// Transitions an event to DEAD_LETTER after exhausting max retries
    async fn mark_dead_letter_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        error_msg: &str,
    ) -> Result<(), DbError>;
}

pub struct SqlxOutboxRepository {
    pool: SqlitePool,
}

impl SqlxOutboxRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Static helper to insert inside an active transaction without an instance
    pub async fn insert_tx_static(
        tx: &mut Transaction<'_, Sqlite>,
        draft: &OutboxEventDraft,
    ) -> Result<OutboxEvent, DbError> {
        let event_id = Uuid::new_v4().to_string();
        let now_iso = now_utc_iso_z();
        let payload_str = serde_json::to_string(&draft.payload_json)
            .map_err(|e| DbError::Serialization(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO outbox_events (
                id, tenant_id, event_type, aggregate_type, aggregate_id,
                payload_json, status, retry_count, attempt_count, max_retries,
                next_retry_at, last_error, error_message, created_at, published_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, 'PENDING', 0, 0, ?7,
                ?8, NULL, NULL, ?8, NULL
            )
            "#,
        )
        .bind(&event_id)
        .bind(draft.tenant_id.to_string())
        .bind(&draft.event_type)
        .bind(&draft.aggregate_type)
        .bind(&draft.aggregate_id)
        .bind(&payload_str)
        .bind(draft.max_retries)
        .bind(&now_iso)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(OutboxEvent {
            id: event_id,
            tenant_id: draft.tenant_id.to_string(),
            event_type: draft.event_type.clone(),
            aggregate_type: draft.aggregate_type.clone(),
            aggregate_id: draft.aggregate_id.clone(),
            payload_json: payload_str,
            status: "PENDING".to_string(),
            retry_count: 0,
            attempt_count: 0,
            max_retries: draft.max_retries,
            next_retry_at: now_iso.clone(),
            last_error: None,
            error_message: None,
            created_at: now_iso,
            published_at: None,
        })
    }
}

#[async_trait]
impl OutboxRepository for SqlxOutboxRepository {
    async fn insert_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        draft: &OutboxEventDraft,
    ) -> Result<OutboxEvent, DbError> {
        Self::insert_tx_static(tx, draft).await
    }

    async fn insert(&self, draft: &OutboxEventDraft) -> Result<OutboxEvent, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from_sqlx)?;
        let event = self.insert_tx(&mut tx, draft).await?;
        tx.commit().await.map_err(DbError::from_sqlx)?;
        Ok(event)
    }

    async fn get_by_id(&self, ctx: &TenantContext, id: &str) -> Result<Option<OutboxEvent>, DbError> {
        sqlx::query_as::<_, OutboxEvent>(
            "SELECT * FROM outbox_events WHERE id = ?1 AND tenant_id = ?2",
        )
        .bind(id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn list_by_tenant(
        &self,
        ctx: &TenantContext,
        status_filter: Option<&str>,
        limit: u32,
    ) -> Result<Vec<OutboxEvent>, DbError> {
        if let Some(status) = status_filter {
            sqlx::query_as::<_, OutboxEvent>(
                "SELECT * FROM outbox_events WHERE tenant_id = ?1 AND status = ?2 ORDER BY created_at ASC LIMIT ?3",
            )
            .bind(ctx.tenant_id_str())
            .bind(status)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::from_sqlx)
        } else {
            sqlx::query_as::<_, OutboxEvent>(
                "SELECT * FROM outbox_events WHERE tenant_id = ?1 ORDER BY created_at ASC LIMIT ?2",
            )
            .bind(ctx.tenant_id_str())
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::from_sqlx)
        }
    }

    async fn list_by_aggregate(
        &self,
        ctx: &TenantContext,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> Result<Vec<OutboxEvent>, DbError> {
        sqlx::query_as::<_, OutboxEvent>(
            "SELECT * FROM outbox_events WHERE tenant_id = ?1 AND aggregate_type = ?2 AND aggregate_id = ?3 ORDER BY created_at ASC",
        )
        .bind(ctx.tenant_id_str())
        .bind(aggregate_type)
        .bind(aggregate_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn fetch_pending_batch(
        &self,
        limit: u32,
        now: DateTime<Utc>,
    ) -> Result<Vec<OutboxEvent>, DbError> {
        let now_iso = format_utc_iso_z(now);
        sqlx::query_as::<_, OutboxEvent>(
            r#"
            SELECT * FROM outbox_events
            WHERE status IN ('PENDING', 'FAILED') AND next_retry_at <= ?1
            ORDER BY created_at ASC
            LIMIT ?2
            "#,
        )
        .bind(&now_iso)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn claim_batch_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ids: &[String],
    ) -> Result<Vec<OutboxEvent>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut claimed = Vec::new();
        for id in ids {
            let res = sqlx::query(
                "UPDATE outbox_events SET status = 'PROCESSING' WHERE id = ?1 AND status IN ('PENDING', 'FAILED')",
            )
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(DbError::from_sqlx)?;

            if res.rows_affected() > 0 {
                if let Some(event) = sqlx::query_as::<_, OutboxEvent>(
                    "SELECT * FROM outbox_events WHERE id = ?1 AND status = 'PROCESSING'",
                )
                .bind(id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(DbError::from_sqlx)? {
                    claimed.push(event);
                }
            }
        }

        Ok(claimed)
    }

    async fn mark_published_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        published_at: DateTime<Utc>,
    ) -> Result<(), DbError> {
        let pub_iso = format_utc_iso_z(published_at);
        sqlx::query(
            r#"
            UPDATE outbox_events 
            SET status = 'PUBLISHED', published_at = ?1,
                attempt_count = attempt_count + 1, retry_count = retry_count + 1
            WHERE id = ?2
            "#,
        )
        .bind(&pub_iso)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn mark_failed_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        error_msg: &str,
        next_retry: DateTime<Utc>,
    ) -> Result<(), DbError> {
        let retry_iso = format_utc_iso_z(next_retry);
        sqlx::query(
            r#"
            UPDATE outbox_events 
            SET status = 'FAILED',
                retry_count = retry_count + 1,
                attempt_count = attempt_count + 1,
                next_retry_at = ?1,
                last_error = ?2,
                error_message = ?2
            WHERE id = ?3
            "#,
        )
        .bind(&retry_iso)
        .bind(error_msg)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn mark_dead_letter_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        error_msg: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE outbox_events 
            SET status = 'DEAD_LETTER',
                retry_count = retry_count + 1,
                attempt_count = attempt_count + 1,
                last_error = ?1,
                error_message = ?1
            WHERE id = ?2
            "#,
        )
        .bind(error_msg)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }
}
