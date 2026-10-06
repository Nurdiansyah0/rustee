use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::tenant::{Role, TenantContext};
use crate::error::AppError;
use crate::repository::invitation_repo::{
    InvitationRepository, NewInvitation, SqlxInvitationRepository, WorkspaceInvitation,
};
use crate::repository::tenant_repo::{
    MembershipRepository, NewMembership, SqlxMembershipRepository, SqlxTenantRepository,
    TenantRepository,
};
use crate::repository::user_repo::{SqlxUserRepository, UserRepository};
use crate::service::crypto::CryptoService;
use crate::service::jwt::JwtEngine;

// --- DTOs ---

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvitationRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvitationDto {
    pub id: String,
    pub tenant_id: String,
    pub email: String,
    pub role: String,
    pub token: String,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AcceptInvitationRequest {
    pub token: String,
    pub display_name: String,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcceptInvitationResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub username: Option<String>,
    pub phone: Option<String>,
    pub token: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub role: String,
    pub account_type: String,
    pub permissions: Vec<String>,
}

/// Preview info shown to the invited user before they accept (public endpoint, no auth)
#[derive(Debug, Clone, Serialize)]
pub struct InvitationPreviewDto {
    pub workspace_name: String,
    pub workspace_slug: String,
    pub invited_email: String,
    pub role: String,
    pub expires_at: String,
}

// --- InvitationService ---

pub struct InvitationService {
    pool: SqlitePool,
    invitation_repo: Arc<dyn InvitationRepository>,
    tenant_repo: Arc<dyn TenantRepository>,
    membership_repo: Arc<dyn MembershipRepository>,
    user_repo: Arc<dyn UserRepository>,
    crypto_service: Arc<CryptoService>,
    jwt_engine: Arc<JwtEngine>,
}

impl InvitationService {
    pub fn new_with_pool(
        pool: SqlitePool,
        crypto_service: Arc<CryptoService>,
        jwt_engine: Arc<JwtEngine>,
    ) -> Self {
        Self {
            invitation_repo: Arc::new(SqlxInvitationRepository::new(pool.clone())),
            tenant_repo: Arc::new(SqlxTenantRepository::new(pool.clone())),
            membership_repo: Arc::new(SqlxMembershipRepository::new(pool.clone())),
            user_repo: Arc::new(SqlxUserRepository::new(pool.clone())),
            crypto_service,
            jwt_engine,
            pool,
        }
    }

    fn generate_token() -> String {
        // 32-byte (64-char hex) secure random token
        (0..64)
            .map(|_| format!("{:x}", rand_byte()))
            .collect::<String>()
    }

    /// Creates an invitation link for a given email + role.
    /// Only Owner or Administrator can call this (enforced via TenantContext).
    pub async fn create_invitation(
        &self,
        ctx: &TenantContext,
        req: CreateInvitationRequest,
    ) -> Result<InvitationDto, AppError> {
        if !ctx.role.can_manage_members() {
            return Err(AppError::Forbidden(
                "Only Owner or Administrator can invite members".to_string(),
                "FORBIDDEN",
            ));
        }

        let role_str = req.role.trim().to_lowercase();
        if role_str == "owner" {
            return Err(AppError::BadRequest(
                "Cannot invite a member with the owner role".to_string(),
                "INVALID_ROLE",
            ));
        }

        // Validate role is a known value
        Role::from_str(&role_str).ok_or_else(|| {
            AppError::BadRequest(
                format!(
                    "Invalid role '{}'. Allowed: administrator, manager, staff, accountant",
                    role_str
                ),
                "INVALID_ROLE",
            )
        })?;

        let email = req.email.trim().to_lowercase();

        // Revoke any existing pending invitation for the same email+tenant
        if let Some(existing) = self
            .invitation_repo
            .find_pending_by_email_and_tenant(&email, &ctx.tenant_id_str())
            .await
            .map_err(AppError::from)?
        {
            self.invitation_repo
                .revoke(&existing.id)
                .await
                .map_err(AppError::from)?;
        }

        let expires_at = (Utc::now() + Duration::days(7)).to_rfc3339();
        let token = Self::generate_token();

        let new_inv = NewInvitation {
            id: format!("inv_{}", Uuid::new_v4()),
            tenant_id: ctx.tenant_id_str(),
            invited_by: ctx.actor_id_str(),
            email: email.clone(),
            role: role_str.clone(),
            token: token.clone(),
            expires_at: expires_at.clone(),
        };

        let inv = self
            .invitation_repo
            .create(&new_inv)
            .await
            .map_err(AppError::from)?;

        Ok(InvitationDto {
            id: inv.id,
            tenant_id: inv.tenant_id,
            email: inv.email,
            role: inv.role,
            token: inv.token,
            status: inv.status,
            expires_at: inv.expires_at,
            created_at: inv.created_at,
        })
    }

    /// Public preview: returns workspace name and invited email for the join page.
    /// No authentication required — token-gated.
    pub async fn preview_invitation(
        &self,
        token: &str,
    ) -> Result<InvitationPreviewDto, AppError> {
        let inv = self.find_valid_invitation(token).await?;

        let tenant = self
            .tenant_repo
            .get_by_id(&inv.tenant_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound("Workspace not found".to_string(), "NOT_FOUND"))?;

        Ok(InvitationPreviewDto {
            workspace_name: tenant.name,
            workspace_slug: tenant.slug,
            invited_email: inv.email,
            role: inv.role,
            expires_at: inv.expires_at,
        })
    }

    /// Accepts an invitation: registers the user as staff (account_type = 'staff'),
    /// creates the workspace membership, and returns a JWT + workspace context.
    pub async fn accept_invitation(
        &self,
        req: AcceptInvitationRequest,
    ) -> Result<AcceptInvitationResponse, AppError> {
        let inv = self.find_valid_invitation(&req.token).await?;

        // Validate inputs
        CryptoService::validate_password_strength(&req.password)?;
        let display_name = req.display_name.trim().to_string();
        if display_name.is_empty() {
            return Err(AppError::BadRequest(
                "Display name cannot be blank".to_string(),
                "DISPLAY_NAME_REQUIRED",
            ));
        }

        // Email must match invitation
        let normalized_email = inv.email.to_lowercase();

        // Check user does not already exist (must not have registered before)
        if let Some(existing_user) = self
            .user_repo
            .find_by_email(&normalized_email)
            .await
            .map_err(AppError::from)?
        {
            // User already registered — just add them to the workspace directly
            return self
                .add_existing_user_to_workspace(&inv, existing_user.id)
                .await;
        }

        let clean_username = req.username
            .map(|u| u.trim().to_lowercase())
            .filter(|u| !u.is_empty());

        if let Some(ref uname) = clean_username {
            if uname.len() < 3 || uname.len() > 30 {
                return Err(AppError::BadRequest(
                    "Username harus antara 3 hingga 30 karakter".to_string(),
                    "INVALID_USERNAME_LENGTH",
                ));
            }
            if !uname.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err(AppError::BadRequest(
                    "Username hanya boleh berisi huruf, angka, garis bawah (_), atau strip (-)".to_string(),
                    "INVALID_USERNAME_FORMAT",
                ));
            }
            if self.user_repo.find_by_identifier(uname).await.map_err(AppError::from)?.is_some() {
                return Err(AppError::Conflict(
                    "Username ini sudah digunakan, silakan pilih username lain".to_string(),
                    "USERNAME_ALREADY_EXISTS",
                ));
            }
        }

        let clean_phone = req.phone
            .map(|p| p.trim().chars().filter(|c| c.is_ascii_digit() || *c == '+').collect::<String>())
            .filter(|p| !p.is_empty());

        let password_hash = self
            .crypto_service
            .hash_password(req.password)
            .await
            .map_err(AppError::from)?;

        let user_id = Uuid::new_v4().to_string();
        let role_str = inv.role.clone();

        // ATOMIC TRANSACTION: create user (staff) + membership + mark invitation accepted
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;

        // 1. Insert user with account_type = 'staff' (no business onboarding, no personal workspace seed)
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO users
                (id, email, password_hash, display_name, currency, role, subscription_tier,
                 account_type, username, phone, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 'IDR', 'user', 'free', 'staff', ?5, ?6, ?7, ?7)
            "#,
        )
        .bind(&user_id)
        .bind(&normalized_email)
        .bind(&password_hash)
        .bind(&display_name)
        .bind(&clean_username)
        .bind(&clean_phone)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        // 2. Create workspace membership
        let membership_id = format!("mem_{}", Uuid::new_v4());
        let new_membership = NewMembership {
            id: membership_id,
            tenant_id: inv.tenant_id.clone(),
            user_id: user_id.clone(),
            role: Role::from_str_or_custom(&role_str),
        };
        SqlxMembershipRepository::create_membership_tx(&mut tx, &new_membership)
            .await
            .map_err(AppError::from)?;

        // 3. Mark invitation as accepted
        sqlx::query(
            r#"
            UPDATE workspace_invitations
            SET status = 'accepted', accepted_at = ?1, updated_at = ?1
            WHERE token = ?2
            "#,
        )
        .bind(&now)
        .bind(&inv.token)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Fetch workspace name for response
        let tenant = self
            .tenant_repo
            .get_by_id(&inv.tenant_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound("Workspace not found".to_string(), "NOT_FOUND"))?;

        let (token, _) = self
            .jwt_engine
            .generate_token(&user_id, &normalized_email, "user", "free")
            .map_err(AppError::from)?;

        Ok(AcceptInvitationResponse {
            user_id,
            email: normalized_email,
            display_name,
            username: clean_username,
            phone: clean_phone,
            token,
            workspace_id: inv.tenant_id,
            workspace_name: tenant.name,
            role: role_str,
            account_type: "staff".to_string(),
            permissions: vec!["transactions.basic".to_string()],
        })
    }

    /// Lists all invitations for a workspace (Owner/Admin only)
    pub async fn list_invitations(
        &self,
        ctx: &TenantContext,
    ) -> Result<Vec<InvitationDto>, AppError> {
        if !ctx.role.can_manage_members() {
            return Err(AppError::Forbidden(
                "Only Owner or Administrator can view invitations".to_string(),
                "FORBIDDEN",
            ));
        }

        let invitations = self
            .invitation_repo
            .list_by_tenant(&ctx.tenant_id_str())
            .await
            .map_err(AppError::from)?;

        Ok(invitations
            .into_iter()
            .map(|inv| InvitationDto {
                id: inv.id,
                tenant_id: inv.tenant_id,
                email: inv.email,
                role: inv.role,
                token: inv.token,
                status: inv.status,
                expires_at: inv.expires_at,
                created_at: inv.created_at,
            })
            .collect())
    }

    /// Revokes a pending invitation (Owner/Admin only)
    pub async fn revoke_invitation(
        &self,
        ctx: &TenantContext,
        invitation_id: &str,
    ) -> Result<(), AppError> {
        if !ctx.role.can_manage_members() {
            return Err(AppError::Forbidden(
                "Only Owner or Administrator can revoke invitations".to_string(),
                "FORBIDDEN",
            ));
        }

        self.invitation_repo
            .revoke(invitation_id)
            .await
            .map_err(AppError::from)?;

        Ok(())
    }

    // --- Private Helpers ---

    async fn find_valid_invitation(&self, token: &str) -> Result<WorkspaceInvitation, AppError> {
        let inv = self
            .invitation_repo
            .find_by_token(token)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Invitation not found or already used".to_string(), "NOT_FOUND")
            })?;

        if inv.status != "pending" {
            return Err(AppError::BadRequest(
                "This invitation has already been used or revoked".to_string(),
                "INVITATION_INVALID",
            ));
        }

        let expires = chrono::DateTime::parse_from_rfc3339(&inv.expires_at)
            .map_err(|_| AppError::Internal("Invalid expiry date".to_string()))?;

        if Utc::now() > expires {
            return Err(AppError::BadRequest(
                "This invitation link has expired".to_string(),
                "INVITATION_EXPIRED",
            ));
        }

        Ok(inv)
    }

    async fn add_existing_user_to_workspace(
        &self,
        inv: &WorkspaceInvitation,
        user_id: String,
    ) -> Result<AcceptInvitationResponse, AppError> {
        // Check if already a member
        if let Some(existing_mem) = self
            .membership_repo
            .get_membership(&inv.tenant_id, &user_id)
            .await
            .map_err(AppError::from)?
        {
            if existing_mem.status == "ACTIVE" {
                return Err(AppError::Conflict(
                    "You are already a member of this workspace".to_string(),
                    "ALREADY_MEMBER",
                ));
            }
        }

        let membership_id = format!("mem_{}", Uuid::new_v4());
        let new_membership = NewMembership {
            id: membership_id,
            tenant_id: inv.tenant_id.clone(),
            user_id: user_id.clone(),
            role: Role::from_str_or_custom(&inv.role),
        };
        self.membership_repo
            .create_membership(&new_membership)
            .await
            .map_err(AppError::from)?;

        self.invitation_repo
            .mark_accepted(&inv.token)
            .await
            .map_err(AppError::from)?;

        let user = self
            .user_repo
            .find_by_id(&user_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound("User not found".to_string(), "NOT_FOUND"))?;

        let tenant = self
            .tenant_repo
            .get_by_id(&inv.tenant_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::NotFound("Workspace not found".to_string(), "NOT_FOUND"))?;

        let (token, _) = self
            .jwt_engine
            .generate_token(&user.id, &user.email, &user.role, &user.subscription_tier)
            .map_err(AppError::from)?;

        Ok(AcceptInvitationResponse {
            user_id: user.id,
            email: user.email,
            display_name: user.display_name,
            username: user.username,
            phone: user.phone,
            token,
            workspace_id: inv.tenant_id.clone(),
            workspace_name: tenant.name,
            role: inv.role.clone(),
            account_type: user.role, // existing user keeps their account_type
            permissions: vec!["transactions.basic".to_string()],
        })
    }
}

/// Simple pseudo-random byte using std (no extra dep needed since we use uuid already)
fn rand_byte() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // Mix with a uuid-derived entropy
    let u = Uuid::new_v4();
    let bytes = u.as_bytes();
    bytes[0] ^ (nanos as u8)
}
