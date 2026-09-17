use crate::repository::DbError;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, FromRow, serde::Serialize, serde::Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub user_id: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub details: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewAuditLog {
    pub user_id: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub details: Option<String>,
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn log_event(&self, event: &NewAuditLog) -> Result<(), DbError>;
    async fn list_by_user(
        &self,
        user_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AuditLog>, DbError>;
}

pub struct SqlxAuditRepository {
    pool: SqlitePool,
}

impl SqlxAuditRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditRepository for SqlxAuditRepository {
    async fn log_event(&self, event: &NewAuditLog) -> Result<(), DbError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO audit_logs (id, user_id, action, entity_type, entity_id, ip_address, user_agent, details, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#
        )
        .bind(&id)
        .bind(&event.user_id)
        .bind(&event.action)
        .bind(&event.entity_type)
        .bind(&event.entity_id)
        .bind(&event.ip_address)
        .bind(&event.user_agent)
        .bind(&event.details)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn list_by_user(
        &self,
        user_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AuditLog>, DbError> {
        let logs = sqlx::query_as::<_, AuditLog>(
            r#"
            SELECT id, user_id, action, entity_type, entity_id, ip_address, user_agent, details, created_at
            FROM audit_logs
            WHERE user_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2 OFFSET ?3
            "#
        )
        .bind(user_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(logs)
    }
}
