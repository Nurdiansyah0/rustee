use async_trait::async_trait;
use chrono::Utc;
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::domain::tenant::{
    BusinessProfile, Membership, Role, Tenant, TenantContext, TenantStatus,
};
use crate::repository::DbError;

// --- DTOs for Creation and Queries ---

#[derive(Debug, Clone)]
pub struct NewTenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: Option<TenantStatus>,
    pub is_personal: bool,
}

#[derive(Debug, Clone)]
pub struct NewMembership {
    pub id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct UpsertBusinessProfile {
    pub business_name: String,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub timezone: Option<String>,
    pub currency: Option<String>,
    pub locale: Option<String>,
    pub invoice_prefix: Option<String>,
    pub business_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TenantWithRole {
    pub tenant: Tenant,
    pub role: Role,
    pub membership_status: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MembershipWithUser {
    pub membership_id: String,
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: Role,
    pub status: String,
    pub joined_at: String,
}

// --- 1. TenantRepository Trait ---

#[async_trait]
pub trait TenantRepository: Send + Sync {
    /// Inserts a new tenant record
    async fn create_tenant(&self, tenant: &NewTenant) -> Result<Tenant, DbError>;

    /// Finds a tenant by its primary ID
    async fn get_by_id(&self, id: &str) -> Result<Option<Tenant>, DbError>;

    /// Finds a tenant by unique URL slug
    async fn get_by_slug(&self, slug: &str) -> Result<Option<Tenant>, DbError>;

    /// Lists all workspaces an authenticated user has active membership in
    async fn list_user_tenants(&self, user_id: &str) -> Result<Vec<TenantWithRole>, DbError>;

    /// Updates tenant lifecycle status (ACTIVE, SUSPENDED, ARCHIVED)
    async fn update_status(&self, id: &str, status: TenantStatus) -> Result<(), DbError>;
}

// --- 2. MembershipRepository Trait ---

#[async_trait]
pub trait MembershipRepository: Send + Sync {
    /// Inserts a new membership record
    async fn create_membership(&self, membership: &NewMembership) -> Result<Membership, DbError>;

    /// Looks up membership by tenant_id and user_id
    async fn get_membership(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<Option<Membership>, DbError>;

    /// Lists all members within a tenant (Mandatory TenantContext isolation)
    async fn list_members(&self, ctx: &TenantContext) -> Result<Vec<MembershipWithUser>, DbError>;

    /// Updates a user's role within the tenant (Mandatory TenantContext isolation)
    async fn update_role(
        &self,
        ctx: &TenantContext,
        user_id: &str,
        new_role: Role,
    ) -> Result<(), DbError>;

    /// Removes a user's membership from the tenant (Mandatory TenantContext isolation)
    async fn delete_membership(&self, ctx: &TenantContext, user_id: &str) -> Result<(), DbError>;
}

// --- 3. BusinessProfileRepository Trait ---

#[async_trait]
pub trait BusinessProfileRepository: Send + Sync {
    /// Retrieves business profile for the active tenant (Mandatory TenantContext isolation)
    async fn get_profile(&self, ctx: &TenantContext) -> Result<Option<BusinessProfile>, DbError>;

    /// Upserts business profile for the active tenant (Mandatory TenantContext isolation)
    async fn upsert_profile(
        &self,
        ctx: &TenantContext,
        profile: &UpsertBusinessProfile,
    ) -> Result<BusinessProfile, DbError>;
}

// --- SQLx Implementations ---

pub struct SqlxTenantRepository {
    pool: SqlitePool,
}

impl SqlxTenantRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Transactional helper: create tenant within an open transaction
    pub async fn create_tenant_tx(
        tx: &mut Transaction<'_, Sqlite>,
        tenant: &NewTenant,
    ) -> Result<Tenant, DbError> {
        let now = Utc::now().to_rfc3339();
        let status = tenant.status.unwrap_or(TenantStatus::Active).as_str();

        sqlx::query(
            r#"
            INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            "#,
        )
        .bind(&tenant.id)
        .bind(&tenant.name)
        .bind(&tenant.slug)
        .bind(status)
        .bind(tenant.is_personal)
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Tenant {
            id: tenant.id.clone(),
            name: tenant.name.clone(),
            slug: tenant.slug.clone(),
            status: status.to_string(),
            is_personal: tenant.is_personal,
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

#[async_trait]
impl TenantRepository for SqlxTenantRepository {
    async fn create_tenant(&self, tenant: &NewTenant) -> Result<Tenant, DbError> {
        let now = Utc::now().to_rfc3339();
        let status = tenant.status.unwrap_or(TenantStatus::Active).as_str();

        sqlx::query(
            r#"
            INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
            "#,
        )
        .bind(&tenant.id)
        .bind(&tenant.name)
        .bind(&tenant.slug)
        .bind(status)
        .bind(tenant.is_personal)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Tenant {
            id: tenant.id.clone(),
            name: tenant.name.clone(),
            slug: tenant.slug.clone(),
            status: status.to_string(),
            is_personal: tenant.is_personal,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<Tenant>, DbError> {
        let tenant = sqlx::query_as::<_, Tenant>(
            r#"
            SELECT id, name, slug, status, is_personal, created_at, updated_at
            FROM tenants
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(tenant)
    }

    async fn get_by_slug(&self, slug: &str) -> Result<Option<Tenant>, DbError> {
        let tenant = sqlx::query_as::<_, Tenant>(
            r#"
            SELECT id, name, slug, status, is_personal, created_at, updated_at
            FROM tenants
            WHERE LOWER(slug) = LOWER(?1)
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(tenant)
    }

    async fn list_user_tenants(&self, user_id: &str) -> Result<Vec<TenantWithRole>, DbError> {
        #[derive(FromRow)]
        struct TenantRoleRow {
            id: String,
            name: String,
            slug: String,
            status: String,
            is_personal: bool,
            created_at: String,
            updated_at: String,
            role: String,
            membership_status: String,
        }

        let rows = sqlx::query_as::<_, TenantRoleRow>(
            r#"
            SELECT 
                t.id, t.name, t.slug, t.status, t.is_personal, t.created_at, t.updated_at,
                m.role, m.status as membership_status
            FROM tenants t
            INNER JOIN memberships m ON m.tenant_id = t.id
            WHERE m.user_id = ?1 AND m.status = 'ACTIVE' AND t.status != 'ARCHIVED'
            ORDER BY t.created_at ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let result = rows
            .into_iter()
            .map(|r| TenantWithRole {
                tenant: Tenant {
                    id: r.id,
                    name: r.name,
                    slug: r.slug,
                    status: r.status,
                    is_personal: r.is_personal,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                },
                role: Role::from_str_or_custom(&r.role),
                membership_status: r.membership_status,
            })
            .collect();

        Ok(result)
    }

    async fn update_status(&self, id: &str, status: TenantStatus) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE tenants
            SET status = ?1, updated_at = ?2
            WHERE id = ?3
            "#,
        )
        .bind(status.as_str())
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }
}

pub struct SqlxMembershipRepository {
    pool: SqlitePool,
}

impl SqlxMembershipRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Transactional helper: create membership within an open transaction
    pub async fn create_membership_tx(
        tx: &mut Transaction<'_, Sqlite>,
        membership: &NewMembership,
    ) -> Result<Membership, DbError> {
        let now = Utc::now().to_rfc3339();
        let role = membership.role.to_db_string();

        sqlx::query(
            r#"
            INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 'ACTIVE', ?5, ?5)
            "#,
        )
        .bind(&membership.id)
        .bind(&membership.tenant_id)
        .bind(&membership.user_id)
        .bind(&role)
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Membership {
            id: membership.id.clone(),
            tenant_id: membership.tenant_id.clone(),
            user_id: membership.user_id.clone(),
            role,
            status: "ACTIVE".to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

#[async_trait]
impl MembershipRepository for SqlxMembershipRepository {
    async fn create_membership(&self, membership: &NewMembership) -> Result<Membership, DbError> {
        let now = Utc::now().to_rfc3339();
        let role = membership.role.to_db_string();

        sqlx::query(
            r#"
            INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 'ACTIVE', ?5, ?5)
            "#,
        )
        .bind(&membership.id)
        .bind(&membership.tenant_id)
        .bind(&membership.user_id)
        .bind(&role)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(Membership {
            id: membership.id.clone(),
            tenant_id: membership.tenant_id.clone(),
            user_id: membership.user_id.clone(),
            role,
            status: "ACTIVE".to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn get_membership(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<Option<Membership>, DbError> {
        let membership = sqlx::query_as::<_, Membership>(
            r#"
            SELECT id, tenant_id, user_id, role, status, created_at, updated_at
            FROM memberships
            WHERE tenant_id = ?1 AND user_id = ?2
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(membership)
    }

    async fn list_members(&self, ctx: &TenantContext) -> Result<Vec<MembershipWithUser>, DbError> {
        #[derive(FromRow)]
        struct MemberRow {
            membership_id: String,
            user_id: String,
            email: String,
            display_name: String,
            role: String,
            status: String,
            joined_at: String,
        }

        // STRICT ISOLATION: WHERE m.tenant_id = ?1
        let rows = sqlx::query_as::<_, MemberRow>(
            r#"
            SELECT 
                m.id as membership_id,
                u.id as user_id,
                u.email,
                u.display_name,
                m.role,
                m.status,
                m.created_at as joined_at
            FROM memberships m
            INNER JOIN users u ON u.id = m.user_id
            WHERE m.tenant_id = ?1
            ORDER BY m.created_at ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let result = rows
            .into_iter()
            .map(|r| MembershipWithUser {
                membership_id: r.membership_id,
                user_id: r.user_id,
                email: r.email,
                display_name: r.display_name,
                role: Role::from_str_or_custom(&r.role),
                status: r.status,
                joined_at: r.joined_at,
            })
            .collect();

        Ok(result)
    }

    async fn update_role(
        &self,
        ctx: &TenantContext,
        user_id: &str,
        new_role: Role,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();

        // STRICT ISOLATION: WHERE tenant_id = ?3 AND user_id = ?4
        let result = sqlx::query(
            r#"
            UPDATE memberships
            SET role = ?1, updated_at = ?2
            WHERE tenant_id = ?3 AND user_id = ?4
            "#,
        )
        .bind(new_role.to_db_string())
        .bind(&now)
        .bind(ctx.tenant_id_str())
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    async fn delete_membership(&self, ctx: &TenantContext, user_id: &str) -> Result<(), DbError> {
        // STRICT ISOLATION: WHERE tenant_id = ?1 AND user_id = ?2
        let result = sqlx::query(
            r#"
            DELETE FROM memberships
            WHERE tenant_id = ?1 AND user_id = ?2
            "#,
        )
        .bind(ctx.tenant_id_str())
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

pub struct SqlxBusinessProfileRepository {
    pool: SqlitePool,
}

impl SqlxBusinessProfileRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Transactional helper: upsert profile within an open transaction
    pub async fn upsert_profile_tx(
        tx: &mut Transaction<'_, Sqlite>,
        tenant_id: &str,
        profile: &UpsertBusinessProfile,
    ) -> Result<BusinessProfile, DbError> {
        let now = Utc::now().to_rfc3339();
        let id = format!("prof_{}", Uuid::new_v4());
        let currency = profile
            .currency
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_CURRENCY);
        let locale = profile
            .locale
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_LOCALE);
        let timezone = profile
            .timezone
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_TIMEZONE);
        let invoice_prefix = profile
            .invoice_prefix
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_INVOICE_PREFIX);
        let business_type = profile.business_type.as_deref().unwrap_or("general");

        sqlx::query(
            r#"
            INSERT INTO business_profiles (
                id, tenant_id, business_name, legal_name, tax_id, address, phone,
                email, timezone, currency, locale, invoice_prefix, business_type, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
            ON CONFLICT(tenant_id) DO UPDATE SET
                business_name = excluded.business_name,
                legal_name = excluded.legal_name,
                tax_id = excluded.tax_id,
                address = excluded.address,
                phone = excluded.phone,
                email = excluded.email,
                timezone = excluded.timezone,
                currency = excluded.currency,
                locale = excluded.locale,
                invoice_prefix = excluded.invoice_prefix,
                business_type = excluded.business_type,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(&profile.business_name)
        .bind(&profile.legal_name)
        .bind(&profile.tax_id)
        .bind(&profile.address)
        .bind(&profile.phone)
        .bind(&profile.email)
        .bind(timezone)
        .bind(currency)
        .bind(locale)
        .bind(invoice_prefix)
        .bind(business_type)
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(BusinessProfile {
            id,
            tenant_id: tenant_id.to_string(),
            business_name: profile.business_name.clone(),
            legal_name: profile.legal_name.clone(),
            tax_id: profile.tax_id.clone(),
            address: profile.address.clone(),
            phone: profile.phone.clone(),
            email: profile.email.clone(),
            timezone: timezone.to_string(),
            currency: currency.to_string(),
            locale: locale.to_string(),
            invoice_prefix: invoice_prefix.to_string(),
            business_type: business_type.to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

#[async_trait]
impl BusinessProfileRepository for SqlxBusinessProfileRepository {
    async fn get_profile(&self, ctx: &TenantContext) -> Result<Option<BusinessProfile>, DbError> {
        // STRICT ISOLATION: WHERE tenant_id = ?1
        let profile = sqlx::query_as::<_, BusinessProfile>(
            r#"
            SELECT 
                id, tenant_id, business_name, legal_name, tax_id, address, phone,
                email, timezone, currency, locale, invoice_prefix, business_type, created_at, updated_at
            FROM business_profiles
            WHERE tenant_id = ?1
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(profile)
    }

    async fn upsert_profile(
        &self,
        ctx: &TenantContext,
        profile: &UpsertBusinessProfile,
    ) -> Result<BusinessProfile, DbError> {
        let now = Utc::now().to_rfc3339();
        let id = format!("prof_{}", Uuid::new_v4());
        let currency = profile
            .currency
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_CURRENCY);
        let locale = profile
            .locale
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_LOCALE);
        let timezone = profile
            .timezone
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_TIMEZONE);
        let invoice_prefix = profile
            .invoice_prefix
            .as_deref()
            .unwrap_or(BusinessProfile::DEFAULT_INVOICE_PREFIX);
        let business_type = profile.business_type.as_deref().unwrap_or("general");

        // STRICT ISOLATION: bound to ctx.tenant_id_str()
        sqlx::query(
            r#"
            INSERT INTO business_profiles (
                id, tenant_id, business_name, legal_name, tax_id, address, phone,
                email, timezone, currency, locale, invoice_prefix, business_type, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
            ON CONFLICT(tenant_id) DO UPDATE SET
                business_name = excluded.business_name,
                legal_name = excluded.legal_name,
                tax_id = excluded.tax_id,
                address = excluded.address,
                phone = excluded.phone,
                email = excluded.email,
                timezone = excluded.timezone,
                currency = excluded.currency,
                locale = excluded.locale,
                invoice_prefix = excluded.invoice_prefix,
                business_type = excluded.business_type,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&id)
        .bind(ctx.tenant_id_str())
        .bind(&profile.business_name)
        .bind(&profile.legal_name)
        .bind(&profile.tax_id)
        .bind(&profile.address)
        .bind(&profile.phone)
        .bind(&profile.email)
        .bind(timezone)
        .bind(currency)
        .bind(locale)
        .bind(invoice_prefix)
        .bind(business_type)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(BusinessProfile {
            id,
            tenant_id: ctx.tenant_id_str(),
            business_name: profile.business_name.clone(),
            legal_name: profile.legal_name.clone(),
            tax_id: profile.tax_id.clone(),
            address: profile.address.clone(),
            phone: profile.phone.clone(),
            email: profile.email.clone(),
            timezone: timezone.to_string(),
            currency: currency.to_string(),
            locale: locale.to_string(),
            invoice_prefix: invoice_prefix.to_string(),
            business_type: business_type.to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }
}
