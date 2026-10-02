//! Commercial Receivables & Payment Allocation Domain Model (Features 19, 20)
//! Tracks open commercial credit, aging buckets, and atomic payments.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::domain::money::Rupiah;

/// Status for commercial receivables (§17: OPEN, PARTIALLY_PAID, PAID, VOIDED, OVERDUE, WRITTEN_OFF)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceivableStatus {
    Open,
    PartiallyPaid,
    Paid,
    Voided,
    Overdue,
    WrittenOff,
}

impl ReceivableStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::PartiallyPaid => "PARTIALLY_PAID",
            Self::Paid => "PAID",
            Self::Voided => "VOIDED",
            Self::Overdue => "OVERDUE",
            Self::WrittenOff => "WRITTEN_OFF",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "OPEN" => Some(Self::Open),
            "PARTIALLY_PAID" => Some(Self::PartiallyPaid),
            "PAID" => Some(Self::Paid),
            "VOIDED" => Some(Self::Voided),
            "OVERDUE" => Some(Self::Overdue),
            "WRITTEN_OFF" => Some(Self::WrittenOff),
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Open | Self::PartiallyPaid | Self::Overdue)
    }
}

/// Commercial Receivable Entity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Receivable {
    pub id: String,
    pub tenant_id: String,
    pub invoice_id: String,
    pub total_amount: i64,
    pub allocated_amount: i64,
    pub outstanding_amount: i64,
    pub due_date: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Receivable {
    pub fn parsed_status(&self) -> Option<ReceivableStatus> {
        ReceivableStatus::from_str(&self.status)
    }

    pub fn total_amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.total_amount)
    }

    pub fn allocated_amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.allocated_amount)
    }

    pub fn outstanding_amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.outstanding_amount)
    }
}

/// Supported payment methods for commercial settlement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentMethod {
    Cash,
    BankTransfer,
    Qris,
    PaymentGateway,
    Manual,
}

impl PaymentMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cash => "CASH",
            Self::BankTransfer => "BANK_TRANSFER",
            Self::Qris => "QRIS",
            Self::PaymentGateway => "PAYMENT_GATEWAY",
            Self::Manual => "MANUAL",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "CASH" => Some(Self::Cash),
            "BANK_TRANSFER" => Some(Self::BankTransfer),
            "QRIS" => Some(Self::Qris),
            "PAYMENT_GATEWAY" => Some(Self::PaymentGateway),
            "MANUAL" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// Lifecycle state for payment records
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentStatus {
    Pending,
    Confirmed,
    Failed,
}

impl PaymentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Confirmed => "CONFIRMED",
            Self::Failed => "FAILED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PENDING" => Some(Self::Pending),
            "CONFIRMED" => Some(Self::Confirmed),
            "FAILED" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Commercial Payment Record
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Payment {
    pub id: String,
    pub tenant_id: String,
    pub invoice_id: String,
    pub receivable_id: String,
    pub payment_number: String,
    pub payment_date: String,
    pub amount: i64,
    pub payment_method: String,
    pub reference: Option<String>,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Payment {
    pub fn amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.amount)
    }

    pub fn parsed_method(&self) -> Option<PaymentMethod> {
        PaymentMethod::from_str(&self.payment_method)
    }

    pub fn parsed_status(&self) -> Option<PaymentStatus> {
        PaymentStatus::from_str(&self.status)
    }
}

/// Payment Allocation Bridge Record
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct PaymentAllocation {
    pub id: String,
    pub payment_id: String,
    pub invoice_id: String,
    pub tenant_id: String,
    pub amount: i64,
    pub allocated_at: String,
}

impl PaymentAllocation {
    pub fn amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.amount)
    }
}

/// Aging buckets report (§17)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgingBuckets {
    pub current_0_30: Rupiah,
    pub overdue_31_60: Rupiah,
    pub overdue_61_90: Rupiah,
    pub overdue_90_plus: Rupiah,
    pub total_outstanding: Rupiah,
}
