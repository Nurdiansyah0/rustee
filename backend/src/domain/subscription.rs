use crate::domain::money::Rupiah;
use chrono::Duration;
use serde::{Deserialize, Serialize};

pub const PREMIUM_MONTHLY_PRICE: i64 = 10_000;
pub const PREMIUM_ANNUAL_PRICE: i64 = 110_000;
pub const TRIAL_DURATION_DAYS: i64 = 90;

/// Authoritative Subscription Lifecycle States strictly conforming to Master Specification v3.1.0:
/// FREE, TRIALING, ACTIVE, GRACE, CANCELLED, EXPIRED, PENDING, UNVERIFIED.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionStatus {
    #[serde(rename = "free", alias = "FREE")]
    Free,
    #[serde(rename = "trialing", alias = "TRIALING")]
    Trialing,
    #[serde(rename = "active", alias = "ACTIVE")]
    Active,
    #[serde(rename = "grace", alias = "GRACE")]
    Grace,
    #[serde(rename = "cancelled", alias = "CANCELLED", alias = "canceled", alias = "CANCELED")]
    Cancelled,
    #[serde(rename = "expired", alias = "EXPIRED")]
    Expired,
    #[serde(rename = "pending", alias = "PENDING")]
    Pending,
    #[serde(rename = "unverified", alias = "UNVERIFIED")]
    Unverified,
}

pub type SubscriptionState = SubscriptionStatus;

impl SubscriptionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Trialing => "trialing",
            Self::Active => "active",
            Self::Grace => "grace",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Pending => "pending",
            Self::Unverified => "unverified",
        }
    }

    pub fn as_upper_str(&self) -> &'static str {
        match self {
            Self::Free => "FREE",
            Self::Trialing => "TRIALING",
            Self::Active => "ACTIVE",
            Self::Grace => "GRACE",
            Self::Cancelled => "CANCELLED",
            Self::Expired => "EXPIRED",
            Self::Pending => "PENDING",
            Self::Unverified => "UNVERIFIED",
        }
    }

    pub fn is_pro(&self) -> bool {
        matches!(self, Self::Active | Self::Trialing | Self::Grace)
    }

    pub fn can_access_premium(&self) -> bool {
        matches!(self, Self::Active | Self::Trialing)
    }

    pub fn from_str_relaxed(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "free" => Some(Self::Free),
            "trialing" => Some(Self::Trialing),
            "active" => Some(Self::Active),
            "grace" => Some(Self::Grace),
            "cancelled" | "canceled" => Some(Self::Cancelled),
            "expired" => Some(Self::Expired),
            "pending" => Some(Self::Pending),
            "unverified" => Some(Self::Unverified),
            _ => None,
        }
    }
}

/// Authoritative Subscription Plans and pricing according to Master Specification v3.1.0:
/// - premium_monthly: Rp 10.000 / month
/// - premium_annual: Rp 110.000 / year
/// - premium_trial_90d: 3 months (90 days) free trial
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionPlan {
    #[serde(rename = "premium_monthly", alias = "monthly")]
    PremiumMonthly,
    #[serde(rename = "premium_annual", alias = "annual", alias = "yearly")]
    PremiumAnnual,
    #[serde(
        rename = "premium_trial_90d",
        alias = "premium_trial",
        alias = "premium_trial_3m",
        alias = "premium_trial_7d",
        alias = "trial"
    )]
    PremiumTrial,
}

impl SubscriptionPlan {
    pub const MONTHLY_PRICE: i64 = PREMIUM_MONTHLY_PRICE;
    pub const ANNUAL_PRICE: i64 = PREMIUM_ANNUAL_PRICE;

    pub fn plan_id(&self) -> &'static str {
        match self {
            Self::PremiumMonthly => "premium_monthly",
            Self::PremiumAnnual => "premium_annual",
            Self::PremiumTrial => "premium_trial_90d",
        }
    }

    pub fn amount(&self) -> Rupiah {
        match self {
            Self::PremiumMonthly => Rupiah::new(PREMIUM_MONTHLY_PRICE),
            Self::PremiumAnnual => Rupiah::new(PREMIUM_ANNUAL_PRICE),
            Self::PremiumTrial => Rupiah::new(0),
        }
    }

    pub fn duration_days(&self) -> i64 {
        match self {
            Self::PremiumMonthly => 30,
            Self::PremiumAnnual => 365,
            Self::PremiumTrial => TRIAL_DURATION_DAYS,
        }
    }

    pub fn duration(&self) -> Duration {
        Duration::days(self.duration_days())
    }

    pub fn from_id(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "premium_monthly" | "monthly" => Some(Self::PremiumMonthly),
            "premium_annual" | "annual" | "yearly" => Some(Self::PremiumAnnual),
            "premium_trial" | "premium_trial_90d" | "premium_trial_3m" | "premium_trial_7d" | "trial" => {
                Some(Self::PremiumTrial)
            }
            _ => None,
        }
    }
}
