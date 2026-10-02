//! Commercial Invoicing Domain Model (Features 16, 17, 18)
//! Strict integer Rupiah arithmetic, lifecycle state machine, and document snapshots.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::domain::money::Rupiah;

/// Lifecycle state for commercial invoices (§12: DRAFT -> ISSUED -> PARTIALLY_PAID -> PAID / VOIDED)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvoiceStatus {
    Draft,
    Issued,
    PartiallyPaid,
    Paid,
    Voided,
}

impl InvoiceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Issued => "ISSUED",
            Self::PartiallyPaid => "PARTIALLY_PAID",
            Self::Paid => "PAID",
            Self::Voided => "VOIDED",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "DRAFT" => Some(Self::Draft),
            "ISSUED" => Some(Self::Issued),
            "PARTIALLY_PAID" => Some(Self::PartiallyPaid),
            "PAID" => Some(Self::Paid),
            "VOIDED" => Some(Self::Voided),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Paid | Self::Voided)
    }

    pub fn can_modify(&self) -> bool {
        matches!(self, Self::Draft)
    }

    pub fn can_issue(&self) -> bool {
        matches!(self, Self::Draft)
    }

    pub fn can_allocate_payment(&self) -> bool {
        matches!(self, Self::Issued | Self::PartiallyPaid)
    }

    pub fn can_void(&self) -> bool {
        matches!(self, Self::Draft | Self::Issued)
    }
}

/// Commercial Invoice Domain Entity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Invoice {
    pub id: String,
    pub tenant_id: String,
    pub invoice_number: Option<String>,
    pub status: String,
    pub customer_id: Option<String>,
    pub customer_name: String,
    pub customer_address: Option<String>,
    pub customer_email: Option<String>,
    pub issue_date: Option<String>,
    pub due_date: String,
    pub currency: String,
    pub subtotal: i64,
    pub discount: i64,
    pub tax_type: String,
    pub tax_amount: i64,
    pub total_amount: i64,
    pub balance_due: i64,
    pub snapshot_json: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Invoice {
    pub fn parsed_status(&self) -> Option<InvoiceStatus> {
        InvoiceStatus::from_str(&self.status)
    }

    pub fn subtotal_rupiah(&self) -> Rupiah {
        Rupiah::new(self.subtotal)
    }

    pub fn discount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.discount)
    }

    pub fn tax_amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.tax_amount)
    }

    pub fn total_amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.total_amount)
    }

    pub fn balance_due_rupiah(&self) -> Rupiah {
        Rupiah::new(self.balance_due)
    }
}

/// Commercial Invoice Line Item
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct InvoiceItem {
    pub id: String,
    pub invoice_id: String,
    pub tenant_id: String,
    pub description: String,
    pub quantity: i64,
    pub unit_price: i64,
    pub discount: i64,
    pub tax_amount: i64,
    pub line_total: i64,
    pub created_at: String,
}

impl InvoiceItem {
    pub fn unit_price_rupiah(&self) -> Rupiah {
        Rupiah::new(self.unit_price)
    }

    pub fn discount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.discount)
    }

    pub fn tax_amount_rupiah(&self) -> Rupiah {
        Rupiah::new(self.tax_amount)
    }

    pub fn line_total_rupiah(&self) -> Rupiah {
        Rupiah::new(self.line_total)
    }
}

/// Snapshot item persisted immutably at invoice issue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceSnapshotItem {
    pub id: String,
    pub description: String,
    pub quantity: i64,
    pub unit_price: i64,
    pub discount: i64,
    pub tax_amount: i64,
    pub line_total: i64,
}

/// Frozen snapshot captured at invoice issuance (§15)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceSnapshot {
    pub customer_name: String,
    pub customer_address: Option<String>,
    pub customer_email: Option<String>,
    pub items: Vec<InvoiceSnapshotItem>,
    pub tax_type: String,
    pub subtotal: i64,
    pub tax_amount: i64,
    pub total_amount: i64,
    pub issued_at: String,
}
