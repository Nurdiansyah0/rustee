//! Transactional Outbox Processor Worker (Features 23, 24, 25)
//! At-least-once event delivery, exponential backoff, DLQ, and sidecar isolation.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;

use crate::domain::outbox::{format_utc_iso_z, now_utc_iso_z, OutboxEvent};
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct OutboxProcessorConfig {
    pub poll_interval: std::time::Duration,
    pub batch_size: u32,
    pub max_retries: u32,
    pub base_backoff_secs: i64,
}

impl Default for OutboxProcessorConfig {
    fn default() -> Self {
        Self {
            poll_interval: std::time::Duration::from_millis(500),
            batch_size: 50,
            max_retries: 5,
            base_backoff_secs: 2,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("Sidecar temporary failure (HTTP {0}): {1}")]
    Transient(u16, String),
    #[error("Permanent failure: {0}")]
    Permanent(String),
    #[error("Timeout: {0}")]
    Timeout(String),
}

#[async_trait::async_trait]
pub trait EventDispatcher: Send + Sync {
    async fn dispatch(&self, event: &OutboxEvent) -> Result<(), DispatchError>;
}

pub struct DefaultEventDispatcher;

#[async_trait::async_trait]
impl EventDispatcher for DefaultEventDispatcher {
    async fn dispatch(&self, _event: &OutboxEvent) -> Result<(), DispatchError> {
        // Standard delivery dispatcher (simulates successful sidecar webhook/message dispatch)
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessBatchResult {
    pub processed: usize,
    pub failed: usize,
    pub dead_lettered: usize,
}

pub struct OutboxProcessor {
    pool: SqlitePool,
    dispatcher: Arc<dyn EventDispatcher>,
    config: OutboxProcessorConfig,
}

impl OutboxProcessor {
    pub fn new(
        pool: SqlitePool,
        dispatcher: Arc<dyn EventDispatcher>,
        config: OutboxProcessorConfig,
    ) -> Self {
        Self {
            pool,
            dispatcher,
            config,
        }
    }

    pub fn with_default_config(pool: SqlitePool) -> Self {
        Self::new(
            pool,
            Arc::new(DefaultEventDispatcher),
            OutboxProcessorConfig::default(),
        )
    }

    /// Process batch of pending events for a specific tenant
    pub async fn process_tenant_batch(
        &self,
        tenant_id: &str,
        simulate_failure: bool,
    ) -> Result<ProcessBatchResult, AppError> {
        let now_iso = now_utc_iso_z();

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, 
                   status, retry_count, attempt_count, max_retries, next_retry_at, 
                   last_error, error_message, created_at, published_at
            FROM outbox_events
            WHERE tenant_id = ?1 
              AND status IN ('PENDING', 'FAILED')
              AND next_retry_at <= ?2
            ORDER BY created_at ASC
            LIMIT ?3
            "#,
        )
        .bind(tenant_id)
        .bind(&now_iso)
        .bind(self.config.batch_size as i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::from)?;

        let mut claimed_events = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r.get("id");
            let claim_res = sqlx::query(
                "UPDATE outbox_events SET status = 'PROCESSING' WHERE id = ?1 AND status IN ('PENDING', 'FAILED')"
            )
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

            if claim_res.rows_affected() > 0 {
                claimed_events.push(OutboxEvent {
                    id,
                    tenant_id: r.get("tenant_id"),
                    event_type: r.get("event_type"),
                    aggregate_type: r.get("aggregate_type"),
                    aggregate_id: r.get("aggregate_id"),
                    payload_json: r.get("payload_json"),
                    status: "PROCESSING".to_string(),
                    retry_count: r.get("retry_count"),
                    attempt_count: r.get("attempt_count"),
                    max_retries: r.get("max_retries"),
                    next_retry_at: r.get("next_retry_at"),
                    last_error: r.get("last_error"),
                    error_message: r.get("error_message"),
                    created_at: r.get("created_at"),
                    published_at: r.get("published_at"),
                });
            }
        }
        tx.commit().await.map_err(AppError::from)?;

        self.dispatch_claimed_events(claimed_events, simulate_failure).await
    }

    /// Process batch of pending events across all tenants
    pub async fn process_all_pending(
        &self,
        simulate_failure: bool,
    ) -> Result<ProcessBatchResult, AppError> {
        let now_iso = now_utc_iso_z();

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, 
                   status, retry_count, attempt_count, max_retries, next_retry_at, 
                   last_error, error_message, created_at, published_at
            FROM outbox_events
            WHERE status IN ('PENDING', 'FAILED')
              AND next_retry_at <= ?1
            ORDER BY created_at ASC
            LIMIT ?2
            "#,
        )
        .bind(&now_iso)
        .bind(self.config.batch_size as i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::from)?;

        let mut claimed_events = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r.get("id");
            let claim_res = sqlx::query(
                "UPDATE outbox_events SET status = 'PROCESSING' WHERE id = ?1 AND status IN ('PENDING', 'FAILED')"
            )
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

            if claim_res.rows_affected() > 0 {
                claimed_events.push(OutboxEvent {
                    id,
                    tenant_id: r.get("tenant_id"),
                    event_type: r.get("event_type"),
                    aggregate_type: r.get("aggregate_type"),
                    aggregate_id: r.get("aggregate_id"),
                    payload_json: r.get("payload_json"),
                    status: "PROCESSING".to_string(),
                    retry_count: r.get("retry_count"),
                    attempt_count: r.get("attempt_count"),
                    max_retries: r.get("max_retries"),
                    next_retry_at: r.get("next_retry_at"),
                    last_error: r.get("last_error"),
                    error_message: r.get("error_message"),
                    created_at: r.get("created_at"),
                    published_at: r.get("published_at"),
                });
            }
        }
        tx.commit().await.map_err(AppError::from)?;

        self.dispatch_claimed_events(claimed_events, simulate_failure).await
    }

    /// Helper to dispatch claimed events, record success or compute exponential backoff / DLQ
    async fn dispatch_claimed_events(
        &self,
        claimed_events: Vec<OutboxEvent>,
        simulate_failure: bool,
    ) -> Result<ProcessBatchResult, AppError> {
        let mut processed = 0;
        let mut failed = 0;
        let mut dead_lettered = 0;

        for event in claimed_events {
            let dispatch_result = if simulate_failure {
                Err(DispatchError::Transient(
                    503,
                    "SIMULATED_SIDECAR_ERROR: Sidecar provider temporary failure (HTTP 503)".to_string(),
                ))
            } else {
                self.dispatcher.dispatch(&event).await
            };

            let now = Utc::now();
            let now_iso = format_utc_iso_z(now);

            match dispatch_result {
                Ok(()) => {
                    let mut up_tx = self.pool.begin().await.map_err(AppError::from)?;
                    sqlx::query(
                        r#"
                        UPDATE outbox_events 
                        SET status = 'PUBLISHED', published_at = ?1, 
                            attempt_count = attempt_count + 1, retry_count = retry_count + 1
                        WHERE id = ?2
                        "#,
                    )
                    .bind(&now_iso)
                    .bind(&event.id)
                    .execute(&mut *up_tx)
                    .await
                    .map_err(AppError::from)?;
                    up_tx.commit().await.map_err(AppError::from)?;
                    processed += 1;
                }
                Err(err) => {
                    let new_attempts = event.attempt_count + 1;
                    let err_msg = err.to_string();

                    let mut up_tx = self.pool.begin().await.map_err(AppError::from)?;
                    if new_attempts >= event.max_retries {
                        // Exhausted max attempts: transition to DEAD_LETTER
                        sqlx::query(
                            r#"
                            UPDATE outbox_events 
                            SET status = 'DEAD_LETTER',
                                attempt_count = ?1, retry_count = ?1, 
                                last_error = ?2, error_message = ?2
                            WHERE id = ?3
                            "#,
                        )
                        .bind(new_attempts)
                        .bind(&err_msg)
                        .bind(&event.id)
                        .execute(&mut *up_tx)
                        .await
                        .map_err(AppError::from)?;
                        dead_lettered += 1;
                    } else {
                        // Exponential backoff: base_delay * 2^(new_attempts - 1)
                        let shift = (new_attempts.saturating_sub(1)).min(30) as u32;
                        let factor = 1i64.checked_shl(shift).unwrap_or(1024);
                        let delay_secs = self.config.base_backoff_secs.saturating_mul(factor);
                        let next_retry = format_utc_iso_z(now + Duration::seconds(delay_secs));

                        sqlx::query(
                            r#"
                            UPDATE outbox_events 
                            SET status = 'FAILED',
                                attempt_count = ?1, retry_count = ?1, 
                                last_error = ?2, error_message = ?2,
                                next_retry_at = ?3
                            WHERE id = ?4
                            "#,
                        )
                        .bind(new_attempts)
                        .bind(&err_msg)
                        .bind(&next_retry)
                        .bind(&event.id)
                        .execute(&mut *up_tx)
                        .await
                        .map_err(AppError::from)?;
                        failed += 1;
                    }
                    up_tx.commit().await.map_err(AppError::from)?;
                }
            }
        }

        Ok(ProcessBatchResult {
            processed,
            failed,
            dead_lettered,
        })
    }
}
