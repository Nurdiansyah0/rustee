use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, SqlitePool};

use crate::repository::DbError;

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceInvitation {
    pub id: String,
    pub tenant_id: String,
    pub invited_by: String,
    pub email: String,
    pub role: String,
    pub token: String,
    pub status: String,
    pub expires_at: String,
    pub accepted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewInvitation {
    pub id: String,
    pub tenant_id: String,
    pub invited_by: String,
    pub email: String,
    pub role: String,
    pub token: String,
    pub expires_at: String,
}

#[async_trait]
pub trait InvitationRepository: Send + Sync {
    async fn create(&self, inv: &NewInvitation) -> Result<WorkspaceInvitation, DbError>;
    async fn find_by_token(&self, token: &str) -> Result<Option<WorkspaceInvitation>, DbError>;
    async fn find_pending_by_email_and_tenant(
        &self,
        email: &str,
        tenant_id: &str,
    ) -> Result<Option<WorkspaceInvitation>, DbError>;
    async fn mark_accepted(&self, token: &str) -> Result<(), DbError>;
    async fn revoke(&self, id: &str) -> Result<(), DbError>;
    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<WorkspaceInvitation>, DbError>;
}

pub struct SqlxInvitationRepository {
    pool: SqlitePool,
}

impl SqlxInvitationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InvitationRepository for SqlxInvitationRepository {
    async fn create(&self, inv: &NewInvitation) -> Result<WorkspaceInvitation, DbError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO workspace_invitations
                (id, tenant_id, invited_by, email, role, token, status, expires_at, created_at, updated_at)
            VALUES (?1, ?2, ?3, LOWER(?4), ?5, ?6, 'pending', ?7, ?8, ?8)
            "#,
        )
        .bind(&inv.id)
        .bind(&inv.tenant_id)
        .bind(&inv.invited_by)
        .bind(&inv.email)
        .bind(&inv.role)
        .bind(&inv.token)
        .bind(&inv.expires_at)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(WorkspaceInvitation {
            id: inv.id.clone(),
            tenant_id: inv.tenant_id.clone(),
            invited_by: inv.invited_by.clone(),
            email: inv.email.to_lowercase(),
            role: inv.role.clone(),
            token: inv.token.clone(),
            status: "pending".to_string(),
            expires_at: inv.expires_at.clone(),
            accepted_at: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find_by_token(&self, token: &str) -> Result<Option<WorkspaceInvitation>, DbError> {
        let row = sqlx::query_as::<_, WorkspaceInvitation>(
            r#"
            SELECT id, tenant_id, invited_by, email, role, token, status,
                   expires_at, accepted_at, created_at, updated_at
            FROM workspace_invitations
            WHERE token = ?1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(row)
    }

    async fn find_pending_by_email_and_tenant(
        &self,
        email: &str,
        tenant_id: &str,
    ) -> Result<Option<WorkspaceInvitation>, DbError> {
        let row = sqlx::query_as::<_, WorkspaceInvitation>(
            r#"
            SELECT id, tenant_id, invited_by, email, role, token, status,
                   expires_at, accepted_at, created_at, updated_at
            FROM workspace_invitations
            WHERE LOWER(email) = LOWER(?1)
              AND tenant_id = ?2
              AND status = 'pending'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(email)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(row)
    }

    async fn mark_accepted(&self, token: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE workspace_invitations
            SET status = 'accepted', accepted_at = ?1, updated_at = ?1
            WHERE token = ?2
            "#,
        )
        .bind(&now)
        .bind(token)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn revoke(&self, id: &str) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE workspace_invitations
            SET status = 'revoked', updated_at = ?1
            WHERE id = ?2 AND status = 'pending'
            "#,
        )
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn list_by_tenant(&self, tenant_id: &str) -> Result<Vec<WorkspaceInvitation>, DbError> {
        let rows = sqlx::query_as::<_, WorkspaceInvitation>(
            r#"
            SELECT id, tenant_id, invited_by, email, role, token, status,
                   expires_at, accepted_at, created_at, updated_at
            FROM workspace_invitations
            WHERE tenant_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(rows)
    }
}
