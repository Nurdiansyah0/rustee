use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::sync::Arc;
use uuid::Uuid;

use crate::api::middleware::tenant_extractor::parse_user_uuid;
use crate::domain::tenant::{BusinessProfile, Role, Tenant, TenantContext, TenantStatus};
use crate::error::AppError;
use crate::repository::accounting_repo::SqlxAccountingRepository;
use crate::repository::error::DbError;
use crate::repository::tenant_repo::{
    BusinessProfileRepository, MembershipRepository, NewMembership, NewTenant,
    SqlxBusinessProfileRepository, SqlxMembershipRepository, SqlxTenantRepository,
    TenantRepository, UpsertBusinessProfile,
};
use crate::repository::user_repo::{SqlxUserRepository, UserRepository};

// --- DTOs ---

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: Option<String>,
    pub business_type: Option<String>,
    pub timezone: Option<String>,
    pub currency: Option<String>,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBusinessProfileRequest {
    pub business_name: Option<String>,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub timezone: Option<String>,
    pub currency: Option<String>,
    pub invoice_prefix: Option<String>,
    pub business_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InviteMemberRequest {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantDetailDto {
    pub tenant: Tenant,
    pub profile: BusinessProfile,
    pub caller_role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSummaryDto {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub role: Role,
    pub is_personal: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberDto {
    pub membership_id: String,
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: Role,
    pub status: String,
    pub joined_at: String,
}

// --- TenantService ---

pub struct TenantService {
    pool: SqlitePool,
    tenant_repo: Arc<dyn TenantRepository>,
    membership_repo: Arc<dyn MembershipRepository>,
    profile_repo: Arc<dyn BusinessProfileRepository>,
    user_repo: Arc<dyn UserRepository>,
}

impl TenantService {
    pub fn new(
        pool: SqlitePool,
        tenant_repo: Arc<dyn TenantRepository>,
        membership_repo: Arc<dyn MembershipRepository>,
        profile_repo: Arc<dyn BusinessProfileRepository>,
        user_repo: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            pool,
            tenant_repo,
            membership_repo,
            profile_repo,
            user_repo,
        }
    }

    pub fn new_with_pool(pool: SqlitePool) -> Self {
        let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
        let tenant_repo = Arc::new(SqlxTenantRepository::new(pool.clone()));
        let membership_repo = Arc::new(SqlxMembershipRepository::new(pool.clone()));
        let profile_repo = Arc::new(SqlxBusinessProfileRepository::new(pool.clone()));
        Self::new(pool, tenant_repo, membership_repo, profile_repo, user_repo)
    }

    pub fn generate_slug_from_name(name: &str) -> String {
        let mut slug = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();

        while slug.contains("--") {
            slug = slug.replace("--", "-");
        }
        let trimmed = slug.trim_matches('-');
        if trimmed.is_empty() {
            format!("workspace-{}", &Uuid::new_v4().to_string()[..8])
        } else {
            trimmed.to_string()
        }
    }

    /// Creates a new workspace atomically with owner membership and default Indonesian business profile
    pub async fn create_tenant_workspace(
        &self,
        user_id: &str,
        req: CreateTenantRequest,
    ) -> Result<TenantDetailDto, AppError> {
        let trimmed_name = req.name.trim();
        if trimmed_name.is_empty() {
            return Err(AppError::BadRequest(
                "Workspace name cannot be empty".to_string(),
                "INVALID_NAME",
            ));
        }

        let raw_slug = match req.slug {
            Some(ref s) if !s.trim().is_empty() => s.clone(),
            _ => Self::generate_slug_from_name(trimmed_name),
        };
        let slug = Tenant::validate_slug(&raw_slug)?;

        // Check slug uniqueness
        if self
            .tenant_repo
            .get_by_slug(&slug)
            .await
            .map_err(AppError::from)?
            .is_some()
        {
            return Err(AppError::Conflict(
                format!("A workspace with the slug '{}' already exists", slug),
                "SLUG_ALREADY_EXISTS",
            ));
        }

        // Validate timezone
        let timezone = match req.timezone.as_deref() {
            Some(tz) => BusinessProfile::validate_timezone(tz)?,
            None => BusinessProfile::DEFAULT_TIMEZONE.to_string(),
        };

        let currency = req
            .currency
            .unwrap_or_else(|| BusinessProfile::DEFAULT_CURRENCY.to_string());
        let tenant_id = Uuid::new_v4().to_string();
        let membership_id = format!("mem_{}", Uuid::new_v4());

        // ATOMIC TRANSACTION
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;

        // 1. Insert Tenant
        let new_tenant = NewTenant {
            id: tenant_id.clone(),
            name: trimmed_name.to_string(),
            slug: slug.clone(),
            status: Some(TenantStatus::Active),
            is_personal: false,
        };
        let tenant = SqlxTenantRepository::create_tenant_tx(&mut tx, &new_tenant)
            .await
            .map_err(|e| {
                if let DbError::UniqueViolation { ref constraint } = e {
                    if constraint.contains("slug") {
                        return AppError::Conflict(
                            format!("A workspace with the slug '{}' already exists", slug),
                            "SLUG_ALREADY_EXISTS",
                        );
                    }
                }
                AppError::from(e)
            })?;

        // 2. Insert Membership (Creator is Owner)
        let new_membership = NewMembership {
            id: membership_id,
            tenant_id: tenant_id.clone(),
            user_id: user_id.to_string(),
            role: Role::Owner,
        };
        SqlxMembershipRepository::create_membership_tx(&mut tx, &new_membership)
            .await
            .map_err(AppError::from)?;

        // 3. Insert BusinessProfile with Indonesian Defaults
        let profile_cmd = UpsertBusinessProfile {
            business_name: trimmed_name.to_string(),
            legal_name: req.legal_name,
            tax_id: req.tax_id,
            address: None,
            phone: None,
            email: None,
            timezone: Some(timezone),
            currency: Some(currency),
            locale: Some(BusinessProfile::DEFAULT_LOCALE.to_string()),
            invoice_prefix: Some(BusinessProfile::DEFAULT_INVOICE_PREFIX.to_string()),
            business_type: req.business_type.or(Some("general".to_string())),
        };
        let profile =
            SqlxBusinessProfileRepository::upsert_profile_tx(&mut tx, &tenant_id, &profile_cmd)
                .await
                .map_err(AppError::from)?;

        // Seed Canonical 8 Chart of Accounts and Default Tax Rules
        SqlxAccountingRepository::seed_default_accounts_tx(&mut tx, &tenant_id)
            .await
            .map_err(AppError::from)?;

        tx.commit().await?;

        Ok(TenantDetailDto {
            tenant,
            profile,
            caller_role: Role::Owner,
        })
    }

    /// Auto-provisions a default personal workspace for backwards compatibility.
    /// Executes inside an existing caller transaction (e.g. during user registration).
    pub async fn provision_personal_workspace(
        tx: &mut Transaction<'_, Sqlite>,
        user_id: &str,
        display_name: &str,
    ) -> Result<Tenant, AppError> {
        let tenant_id = Uuid::new_v4().to_string();
        let membership_id = format!("mem_{}", Uuid::new_v4());
        let short_id = if user_id.len() >= 8 {
            &user_id[..8]
        } else {
            user_id
        };
        let clean_short_id = short_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>();
        let sanitized_slug = if clean_short_id.is_empty() {
            format!("personal-{}", &Uuid::new_v4().to_string()[..8])
        } else {
            format!("personal-{}", clean_short_id.to_lowercase())
        };

        let workspace_name = format!("{}'s Workspace", display_name.trim());

        let new_tenant = NewTenant {
            id: tenant_id.clone(),
            name: workspace_name.clone(),
            slug: sanitized_slug,
            status: Some(TenantStatus::Active),
            is_personal: true,
        };
        let tenant = SqlxTenantRepository::create_tenant_tx(tx, &new_tenant)
            .await
            .map_err(AppError::from)?;

        let new_membership = NewMembership {
            id: membership_id,
            tenant_id: tenant_id.clone(),
            user_id: user_id.to_string(),
            role: Role::Owner,
        };
        SqlxMembershipRepository::create_membership_tx(tx, &new_membership)
            .await
            .map_err(AppError::from)?;

        let profile_cmd = UpsertBusinessProfile {
            business_name: workspace_name,
            legal_name: None,
            tax_id: None,
            address: None,
            phone: None,
            email: None,
            timezone: Some(BusinessProfile::DEFAULT_TIMEZONE.to_string()),
            currency: Some(BusinessProfile::DEFAULT_CURRENCY.to_string()),
            locale: Some(BusinessProfile::DEFAULT_LOCALE.to_string()),
            invoice_prefix: Some(BusinessProfile::DEFAULT_INVOICE_PREFIX.to_string()),
            business_type: Some("personal".to_string()),
        };
        SqlxBusinessProfileRepository::upsert_profile_tx(tx, &tenant_id, &profile_cmd)
            .await
            .map_err(AppError::from)?;

        // Seed Canonical 8 Chart of Accounts and Default Tax Rules
        SqlxAccountingRepository::seed_default_accounts_tx(tx, &tenant_id)
            .await
            .map_err(AppError::from)?;

        Ok(tenant)
    }

    /// Lists all workspaces the user has an active membership in
    pub async fn list_user_workspaces(
        &self,
        user_id: &str,
    ) -> Result<Vec<TenantSummaryDto>, AppError> {
        let list = self
            .tenant_repo
            .list_user_tenants(user_id)
            .await
            .map_err(AppError::from)?;
        let result = list
            .into_iter()
            .map(|item| TenantSummaryDto {
                id: item.tenant.id,
                name: item.tenant.name,
                slug: item.tenant.slug,
                status: item.tenant.status,
                role: item.role,
                is_personal: item.tenant.is_personal,
                created_at: item.tenant.created_at,
                updated_at: item.tenant.updated_at,
            })
            .collect();
        Ok(result)
    }

    /// Gets workspace details with strict anti-enumeration enforcement
    pub async fn get_workspace_detail(
        &self,
        user_id: &str,
        tenant_id_or_slug: &str,
    ) -> Result<TenantDetailDto, AppError> {
        let tenant = if let Ok(uuid) = Uuid::parse_str(tenant_id_or_slug) {
            self.tenant_repo
                .get_by_id(&uuid.to_string())
                .await
                .map_err(AppError::from)?
        } else {
            self.tenant_repo
                .get_by_slug(tenant_id_or_slug)
                .await
                .map_err(AppError::from)?
        };

        let tenant = match tenant {
            Some(t) => t,
            None => {
                return Err(AppError::NotFound(
                    "Workspace not found".to_string(),
                    "NOT_FOUND",
                ));
            }
        };

        // ANTI-ENUMERATION INVARIANT:
        // Must return 404 (NEVER 403) if caller is not an ACTIVE member!
        let membership = self
            .membership_repo
            .get_membership(&tenant.id, user_id)
            .await
            .map_err(AppError::from)?;

        let membership = match membership {
            Some(m) if m.status == "ACTIVE" => m,
            _ => {
                return Err(AppError::NotFound(
                    "Workspace not found".to_string(),
                    "NOT_FOUND",
                ));
            }
        };

        let caller_role = Role::from_str_or_custom(&membership.role);

        let tenant_uuid = Uuid::parse_str(&tenant.id).unwrap_or_default();
        let actor_uuid = parse_user_uuid(user_id);
        let ctx = TenantContext::new(tenant_uuid, actor_uuid, caller_role.clone());

        let profile = self
            .profile_repo
            .get_profile(&ctx)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Business profile not found".to_string(), "NOT_FOUND")
            })?;

        Ok(TenantDetailDto {
            tenant,
            profile,
            caller_role,
        })
    }

    /// Gets business profile for active tenant context
    pub async fn get_business_profile(
        &self,
        ctx: &TenantContext,
    ) -> Result<BusinessProfile, AppError> {
        self.profile_repo
            .get_profile(ctx)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Business profile not found".to_string(), "NOT_FOUND")
            })
    }

    /// Updates business profile (strictly requires Owner or Administrator role)
    pub async fn update_business_profile(
        &self,
        ctx: &TenantContext,
        req: UpdateBusinessProfileRequest,
    ) -> Result<BusinessProfile, AppError> {
        // RBAC INVARIANT: Only Owner or Administrator can update profile
        if !ctx.role.can_edit_profile() {
            return Err(AppError::Forbidden(
                "Insufficient role permissions to update business profile".to_string(),
                "FORBIDDEN",
            ));
        }

        let timezone = if let Some(ref tz) = req.timezone {
            Some(BusinessProfile::validate_timezone(tz)?)
        } else {
            None
        };

        let existing = self.get_business_profile(ctx).await?;

        let business_name = req
            .business_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(existing.business_name);

        let cmd = UpsertBusinessProfile {
            business_name,
            legal_name: req.legal_name.or(existing.legal_name),
            tax_id: req.tax_id.or(existing.tax_id),
            address: req.address.or(existing.address),
            phone: req.phone.or(existing.phone),
            email: req.email.or(existing.email),
            timezone: timezone.or(Some(existing.timezone)),
            currency: req.currency.or(Some(existing.currency)),
            locale: Some(existing.locale),
            invoice_prefix: req.invoice_prefix.or(Some(existing.invoice_prefix)),
            business_type: req.business_type.or(Some(existing.business_type)),
        };

        let updated = self
            .profile_repo
            .upsert_profile(ctx, &cmd)
            .await
            .map_err(AppError::from)?;
        Ok(updated)
    }

    /// Lists all members within active workspace
    pub async fn list_members(&self, ctx: &TenantContext) -> Result<Vec<MemberDto>, AppError> {
        let members = self
            .membership_repo
            .list_members(ctx)
            .await
            .map_err(AppError::from)?;
        let result = members
            .into_iter()
            .map(|m| MemberDto {
                membership_id: m.membership_id,
                user_id: m.user_id,
                email: m.email,
                display_name: m.display_name,
                role: m.role,
                status: m.status,
                joined_at: m.joined_at,
            })
            .collect();
        Ok(result)
    }

    /// Invites a member to the active workspace (Owner or Administrator only)
    pub async fn invite_member(
        &self,
        ctx: &TenantContext,
        req: InviteMemberRequest,
    ) -> Result<MemberDto, AppError> {
        // RBAC INVARIANT: Only Owner or Administrator can invite members
        if !ctx.role.can_manage_members() {
            return Err(AppError::Forbidden(
                "Insufficient role permissions to invite workspace members".to_string(),
                "FORBIDDEN",
            ));
        }

        let role_str = req.role.trim().to_lowercase();
        if role_str == "owner" {
            return Err(AppError::BadRequest(
                "Cannot invite a member with owner role".to_string(),
                "INVALID_ROLE",
            ));
        }

        let target_role = Role::from_str(&role_str).ok_or_else(|| {
            AppError::BadRequest(
                format!(
                    "Invalid role '{}'. Allowed roles: administrator, manager, staff, accountant",
                    role_str
                ),
                "INVALID_ROLE",
            )
        })?;

        let normalized_email = req.email.trim().to_lowercase();
        let target_user = self
            .user_repo
            .find_by_email(&normalized_email)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(
                    "User with the specified email does not exist".to_string(),
                    "USER_NOT_FOUND",
                )
            })?;

        // Check if already a member
        if let Some(existing) = self
            .membership_repo
            .get_membership(&ctx.tenant_id_str(), &target_user.id)
            .await
            .map_err(AppError::from)?
        {
            if existing.status == "ACTIVE" {
                return Err(AppError::Conflict(
                    "User is already an active member of this workspace".to_string(),
                    "MEMBER_ALREADY_EXISTS",
                ));
            }
        }

        let new_membership = NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: ctx.tenant_id_str(),
            user_id: target_user.id.clone(),
            role: target_role.clone(),
        };

        let created = self
            .membership_repo
            .create_membership(&new_membership)
            .await
            .map_err(AppError::from)?;

        Ok(MemberDto {
            membership_id: created.id,
            user_id: target_user.id,
            email: target_user.email,
            display_name: target_user.display_name,
            role: target_role,
            status: created.status,
            joined_at: created.created_at,
        })
    }

    /// Updates a member's role (Owner only)
    pub async fn update_member_role(
        &self,
        ctx: &TenantContext,
        target_user_id: &str,
        new_role: Role,
    ) -> Result<(), AppError> {
        // RBAC INVARIANT: Only Owner can modify member roles
        if ctx.role != Role::Owner {
            return Err(AppError::Forbidden(
                "Only the workspace owner can modify member roles".to_string(),
                "FORBIDDEN",
            ));
        }

        let _target_membership = self
            .membership_repo
            .get_membership(&ctx.tenant_id_str(), target_user_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(
                    "Member not found in this workspace".to_string(),
                    "NOT_FOUND",
                )
            })?;

        // Guard against demoting self if sole owner
        if target_user_id == ctx.actor_id_str() && new_role != Role::Owner {
            let all_members = self
                .membership_repo
                .list_members(ctx)
                .await
                .map_err(AppError::from)?;
            let owner_count = all_members.iter().filter(|m| m.role == Role::Owner).count();
            if owner_count <= 1 {
                return Err(AppError::Conflict(
                    "Cannot demote the sole workspace owner".to_string(),
                    "SOLE_OWNER_DEMOTION",
                ));
            }
        }

        self.membership_repo
            .update_role(ctx, target_user_id, new_role)
            .await
            .map_err(AppError::from)?;
        Ok(())
    }

    /// Removes a member from the workspace (Owner/Admin or self-leave)
    pub async fn remove_member(
        &self,
        ctx: &TenantContext,
        target_user_id: &str,
    ) -> Result<(), AppError> {
        let is_self = target_user_id == ctx.actor_id_str();
        if !is_self && !ctx.role.can_manage_members() {
            return Err(AppError::Forbidden(
                "Insufficient role permissions to remove member".to_string(),
                "FORBIDDEN",
            ));
        }

        // Query target membership to check existence and role
        let target_membership = self
            .membership_repo
            .get_membership(&ctx.tenant_id_str(), target_user_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound(
                    "Member not found in this workspace".to_string(),
                    "NOT_FOUND",
                )
            })?;

        // Invariant: Non-owner cannot remove owner
        if target_membership.role.to_lowercase() == "owner" && !is_self {
            return Err(AppError::Forbidden(
                "Cannot remove workspace owner".to_string(),
                "FORBIDDEN",
            ));
        }

        // Sole owner cannot leave without transferring ownership
        if is_self && ctx.role == Role::Owner {
            let all_members = self
                .membership_repo
                .list_members(ctx)
                .await
                .map_err(AppError::from)?;
            let owner_count = all_members.iter().filter(|m| m.role == Role::Owner).count();
            if owner_count <= 1 {
                return Err(AppError::Conflict(
                    "Sole owner cannot leave workspace without transferring ownership".to_string(),
                    "SOLE_OWNER_LEAVE",
                ));
            }
        }

        self.membership_repo
            .delete_membership(ctx, target_user_id)
            .await
            .map_err(AppError::from)?;
        Ok(())
    }
}
