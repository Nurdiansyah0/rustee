use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::AppError;

// --- Identifiers ---
pub type TenantId = Uuid;
pub type UserId = Uuid;
pub type MembershipId = Uuid;

// --- Tenant Status ---
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TenantStatus {
    Active,
    Suspended,
    Archived,
}

impl TenantStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Suspended => "SUSPENDED",
            Self::Archived => "ARCHIVED",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for TenantStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "ACTIVE" => Ok(Self::Active),
            "SUSPENDED" => Ok(Self::Suspended),
            "ARCHIVED" => Ok(Self::Archived),
            _ => Err(()),
        }
    }
}

// --- Membership Status ---
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MembershipStatus {
    Active,
    Invited,
    Suspended,
}

impl MembershipStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Invited => "INVITED",
            Self::Suspended => "SUSPENDED",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for MembershipStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "ACTIVE" => Ok(Self::Active),
            "INVITED" => Ok(Self::Invited),
            "SUSPENDED" => Ok(Self::Suspended),
            _ => Err(()),
        }
    }
}

// --- Roles & Permissions ---
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "owner", alias = "OWNER")]
    Owner,
    #[serde(rename = "administrator", alias = "admin", alias = "ADMINISTRATOR")]
    Administrator,
    #[serde(rename = "manager", alias = "MANAGER")]
    Manager,
    #[serde(rename = "staff", alias = "STAFF")]
    Staff,
    #[serde(rename = "accountant", alias = "ACCOUNTANT")]
    Accountant,
    #[serde(rename = "custom")]
    Custom(String),
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Owner => "owner",
            Self::Administrator => "administrator",
            Self::Manager => "manager",
            Self::Staff => "staff",
            Self::Accountant => "accountant",
            Self::Custom(name) => name.as_str(),
        }
    }

    pub fn to_db_string(&self) -> String {
        match self {
            Self::Owner => "owner".to_string(),
            Self::Administrator => "administrator".to_string(),
            Self::Manager => "manager".to_string(),
            Self::Staff => "staff".to_string(),
            Self::Accountant => "accountant".to_string(),
            Self::Custom(name) => format!("custom:{}", name),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn from_str_or_custom(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "owner" => Self::Owner,
            "administrator" | "admin" => Self::Administrator,
            "manager" => Self::Manager,
            "staff" => Self::Staff,
            "accountant" => Self::Accountant,
            other => {
                if let Some(custom_name) = other.strip_prefix("custom:") {
                    Self::Custom(custom_name.to_string())
                } else {
                    Self::Custom(other.to_string())
                }
            }
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        Self::from_str_or_custom(s)
    }

    pub fn is_owner(&self) -> bool {
        matches!(self, Self::Owner)
    }

    pub fn can_manage_members(&self) -> bool {
        matches!(self, Self::Owner | Self::Administrator)
    }

    pub fn can_manage_settings(&self) -> bool {
        matches!(self, Self::Owner | Self::Administrator)
    }

    pub fn can_edit_profile(&self) -> bool {
        matches!(self, Self::Owner | Self::Administrator)
    }

    pub fn can_manage_billing(&self) -> bool {
        matches!(self, Self::Owner)
    }

    pub fn can_post_ledger(&self) -> bool {
        matches!(self, Self::Owner | Self::Administrator | Self::Accountant)
    }

    pub fn can_issue_invoice(&self) -> bool {
        matches!(
            self,
            Self::Owner | Self::Administrator | Self::Manager | Self::Staff
        )
    }

    pub fn can_reverse_journal(&self) -> bool {
        matches!(self, Self::Owner | Self::Accountant)
    }
}

impl std::str::FromStr for Role {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "owner" => Ok(Self::Owner),
            "administrator" | "admin" => Ok(Self::Administrator),
            "manager" => Ok(Self::Manager),
            "staff" => Ok(Self::Staff),
            "accountant" => Ok(Self::Accountant),
            _ => Err(()),
        }
    }
}

// --- Tenant Entity ---
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub is_personal: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Tenant {
    pub const RESERVED_SLUGS: &'static [&'static str] = &[
        "admin",
        "administrator",
        "api",
        "app",
        "auth",
        "billing",
        "dashboard",
        "docs",
        "health",
        "payment",
        "ready",
        "root",
        "status",
        "support",
        "system",
        "user",
        "users",
        "ws",
        "www",
        "settings",
        "help",
        "legal",
        "terms",
        "privacy",
        "static",
        "assets",
        "login",
        "register",
        "logout",
    ];

    pub fn validate_slug(slug: &str) -> Result<String, AppError> {
        let clean = slug.trim().to_lowercase();
        if clean.len() < 3 || clean.len() > 63 {
            return Err(AppError::BadRequest(
                "Slug must be between 3 and 63 characters long".to_string(),
                "INVALID_SLUG",
            ));
        }

        let valid_chars = clean
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid_chars {
            return Err(AppError::BadRequest(
                "Slug can only contain lowercase letters, numbers, and hyphens".to_string(),
                "INVALID_SLUG",
            ));
        }

        if clean.starts_with('-') || clean.ends_with('-') {
            return Err(AppError::BadRequest(
                "Slug cannot start or end with a hyphen".to_string(),
                "INVALID_SLUG",
            ));
        }

        if clean.contains("--") {
            return Err(AppError::BadRequest(
                "Slug cannot contain consecutive hyphens".to_string(),
                "INVALID_SLUG",
            ));
        }

        if Self::RESERVED_SLUGS.contains(&clean.as_str()) {
            return Err(AppError::BadRequest(
                format!("The slug '{}' is reserved by the system", clean),
                "RESERVED_SLUG",
            ));
        }

        Ok(clean)
    }
}

// --- Business Profile Entity ---
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct BusinessProfile {
    pub id: String,
    pub tenant_id: String,
    pub business_name: String,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub timezone: String,       // Default: "Asia/Jakarta"
    pub currency: String,       // Default: "IDR"
    pub locale: String,         // Default: "id-ID"
    pub invoice_prefix: String, // Default: "INV"
    pub business_type: String,  // Default: "general"
    pub created_at: String,
    pub updated_at: String,
}

impl BusinessProfile {
    pub const DEFAULT_TIMEZONE: &'static str = "Asia/Jakarta";
    pub const DEFAULT_CURRENCY: &'static str = "IDR";
    pub const DEFAULT_INVOICE_PREFIX: &'static str = "INV";
    pub const DEFAULT_LOCALE: &'static str = "id-ID";

    pub const VALID_INDONESIAN_TIMEZONES: &'static [&'static str] = &[
        "Asia/Jakarta",   // WIB (UTC+7)
        "Asia/Pontianak", // WIB (UTC+7)
        "Asia/Makassar",  // WITA (UTC+8)
        "Asia/Jayapura",  // WIT (UTC+9)
        "UTC",
    ];

    pub fn validate_timezone(tz: &str) -> Result<String, AppError> {
        let clean = tz.trim();
        for &valid in Self::VALID_INDONESIAN_TIMEZONES {
            if clean.eq_ignore_ascii_case(valid) {
                return Ok(valid.to_string());
            }
        }
        Err(AppError::BadRequest(
            format!(
                "Invalid timezone '{}'. Must be one of: {}",
                clean,
                Self::VALID_INDONESIAN_TIMEZONES.join(", ")
            ),
            "INVALID_TIMEZONE",
        ))
    }
}

// --- Membership Entity ---
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct Membership {
    pub id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub role: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

// --- User Domain Representation ---
#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

// --- TenantContext Invariant ---
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub actor_id: Uuid,
    pub role: Role,
}

impl TenantContext {
    pub fn new(tenant_id: Uuid, actor_id: Uuid, role: Role) -> Self {
        Self {
            tenant_id,
            actor_id,
            role,
        }
    }

    pub fn tenant_id_str(&self) -> String {
        self.tenant_id.to_string()
    }

    pub fn actor_id_str(&self) -> String {
        self.actor_id.to_string()
    }

    pub fn is_owner(&self) -> bool {
        self.role.is_owner()
    }

    pub fn can_manage_members(&self) -> bool {
        self.role.can_manage_members()
    }

    pub fn can_edit_profile(&self) -> bool {
        self.role.can_edit_profile()
    }

    pub fn require_role(&self, allowed: &[Role]) -> Result<(), AppError> {
        if allowed.contains(&self.role) {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Insufficient role permissions for this operation".to_string(),
                "FORBIDDEN",
            ))
        }
    }

    pub fn require_admin_or_owner(&self) -> Result<(), AppError> {
        if self.role.can_manage_settings() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "Only workspace owners and administrators can perform this operation".to_string(),
                "FORBIDDEN",
            ))
        }
    }
}
