use crate::domain::money::Rupiah;
use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct Subscription {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub provider_subscription_id: Option<String>,
    pub plan_id: String,
    pub status: String,
    pub amount: Rupiah,
    pub current_period_start: String,
    pub current_period_end: String,
    pub cancel_at_period_end: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewSubscription {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub provider_subscription_id: Option<String>,
    pub plan_id: String,
    pub status: String,
    pub amount: Rupiah,
    pub current_period_start: String,
    pub current_period_end: String,
    pub cancel_at_period_end: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct WebhookEvent {
    pub id: String,
    pub provider: String,
    pub event_id: String,
    pub event_type: String,
    pub payload: String,
    pub status: String,
    pub processed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewWebhookEvent {
    pub id: String,
    pub provider: String,
    pub event_id: String,
    pub event_type: String,
    pub payload: String,
}

#[async_trait]
pub trait SubscriptionRepository: Send + Sync {
    async fn upsert_subscription(&self, sub: &NewSubscription) -> Result<Subscription, DbError>;
    async fn find_by_user_id(&self, user_id: &str) -> Result<Option<Subscription>, DbError>;
    async fn update_status(&self, user_id: &str, status: &str) -> Result<(), DbError>;
    /// Returns Ok(true) if event was newly recorded, Ok(false) if it was a duplicate (idempotent ignore).
    async fn record_webhook_event(&self, event: &NewWebhookEvent) -> Result<bool, DbError>;
    async fn mark_webhook_processed(
        &self,
        provider: &str,
        event_id: &str,
        status: &str,
    ) -> Result<(), DbError>;
    async fn get_webhook_event(
        &self,
        provider: &str,
        event_id: &str,
    ) -> Result<Option<WebhookEvent>, DbError>;
}

pub struct SqlxSubscriptionRepository {
    pool: SqlitePool,
}

impl SqlxSubscriptionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SubscriptionRepository for SqlxSubscriptionRepository {
    async fn upsert_subscription(&self, sub: &NewSubscription) -> Result<Subscription, DbError> {
        let now = Utc::now().to_rfc3339();
        let cancel_flag = sub.cancel_at_period_end as i32;

        sqlx::query(
            r#"
            INSERT INTO subscriptions
                (id, user_id, provider, provider_subscription_id, plan_id, status, amount,
                 current_period_start, current_period_end, cancel_at_period_end, created_at, updated_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                provider_subscription_id = excluded.provider_subscription_id,
                plan_id = excluded.plan_id,
                status = excluded.status,
                amount = excluded.amount,
                current_period_start = excluded.current_period_start,
                current_period_end = excluded.current_period_end,
                cancel_at_period_end = excluded.cancel_at_period_end,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&sub.id)
        .bind(&sub.user_id)
        .bind(&sub.provider)
        .bind(&sub.provider_subscription_id)
        .bind(&sub.plan_id)
        .bind(&sub.status)
        .bind(sub.amount.0)
        .bind(&sub.current_period_start)
        .bind(&sub.current_period_end)
        .bind(cancel_flag)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Subscription {
            id: sub.id.clone(),
            user_id: sub.user_id.clone(),
            provider: sub.provider.clone(),
            provider_subscription_id: sub.provider_subscription_id.clone(),
            plan_id: sub.plan_id.clone(),
            status: sub.status.clone(),
            amount: sub.amount,
            current_period_start: sub.current_period_start.clone(),
            current_period_end: sub.current_period_end.clone(),
            cancel_at_period_end: sub.cancel_at_period_end,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find_by_user_id(&self, user_id: &str) -> Result<Option<Subscription>, DbError> {
        sqlx::query_as::<_, Subscription>(
            r#"
            SELECT id, user_id, provider, provider_subscription_id, plan_id, status, amount,
                   current_period_start, current_period_end, cancel_at_period_end, created_at, updated_at
            FROM subscriptions
            WHERE user_id = ?1
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }

    async fn update_status(&self, user_id: &str, status: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            UPDATE subscriptions
            SET status = ?1, updated_at = ?2
            WHERE user_id = ?3
            "#,
        )
        .bind(status)
        .bind(&now)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if res.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn record_webhook_event(&self, event: &NewWebhookEvent) -> Result<bool, DbError> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            INSERT INTO webhook_events
                (id, provider, event_id, event_type, payload, status, created_at)
            VALUES
                (?1, ?2, ?3, ?4, ?5, 'received', ?6)
            ON CONFLICT(provider, event_id) DO NOTHING
            "#,
        )
        .bind(&event.id)
        .bind(&event.provider)
        .bind(&event.event_id)
        .bind(&event.event_type)
        .bind(&event.payload)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(res.rows_affected() > 0)
    }

    async fn mark_webhook_processed(
        &self,
        provider: &str,
        event_id: &str,
        status: &str,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            UPDATE webhook_events
            SET status = ?1, processed_at = ?2
            WHERE provider = ?3 AND event_id = ?4
            "#,
        )
        .bind(status)
        .bind(&now)
        .bind(provider)
        .bind(event_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if res.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn get_webhook_event(
        &self,
        provider: &str,
        event_id: &str,
    ) -> Result<Option<WebhookEvent>, DbError> {
        sqlx::query_as::<_, WebhookEvent>(
            r#"
            SELECT id, provider, event_id, event_type, payload, status, processed_at, created_at
            FROM webhook_events
            WHERE provider = ?1 AND event_id = ?2
            "#,
        )
        .bind(provider)
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)
    }
}
