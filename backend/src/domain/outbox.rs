//! Transactional Outbox Domain Models (Features 22, 23, 24, 25)
//! Reliable asynchronous event delivery, deduplication, and sidecar isolation.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Lifecycle state for transactional outbox events (§20)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutboxStatus {
    Pending,
    Processing,
    Published,
    Failed,
    DeadLetter,
}

impl OutboxStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Processing => "PROCESSING",
            Self::Published => "PUBLISHED",
            Self::Failed => "FAILED",
            Self::DeadLetter => "DEAD_LETTER",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PENDING" => Some(Self::Pending),
            "PROCESSING" => Some(Self::Processing),
            "PUBLISHED" => Some(Self::Published),
            "FAILED" => Some(Self::Failed),
            "DEAD_LETTER" => Some(Self::DeadLetter),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Published | Self::DeadLetter)
    }

    pub fn can_claim(&self) -> bool {
        matches!(self, Self::Pending | Self::Failed)
    }

    /// Calculate next retry backoff using exponential schedule: base_secs * 2^(retry_count - 1)
    pub fn calculate_backoff(retry_count: u32, base_secs: u64, max_secs: u64) -> Duration {
        let shift = retry_count.saturating_sub(1).min(30);
        let factor = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
        let delay_secs = base_secs.saturating_mul(factor).min(max_secs);
        Duration::seconds(delay_secs as i64)
    }
}

/// Outbox Event Database Entity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct OutboxEvent {
    pub id: String,
    pub tenant_id: String,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub payload_json: String,
    pub status: String,
    pub retry_count: i64,
    pub attempt_count: i64,
    pub max_retries: i64,
    pub next_retry_at: String,
    pub last_error: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub published_at: Option<String>,
}

impl OutboxEvent {
    pub fn parsed_status(&self) -> Option<OutboxStatus> {
        OutboxStatus::from_str(&self.status)
    }

    pub fn id_uuid(&self) -> Result<Uuid, uuid::Error> {
        Uuid::parse_str(&self.id)
    }

    pub fn tenant_id_uuid(&self) -> Result<Uuid, uuid::Error> {
        Uuid::parse_str(&self.tenant_id)
    }

    pub fn parse_payload<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.payload_json)
    }

    pub fn payload_value(&self) -> serde_json::Value {
        serde_json::from_str(&self.payload_json).unwrap_or(serde_json::Value::Null)
    }

    /// Convert to API DTO where `payload` is delivered as a JSON object
    pub fn to_dto(&self) -> OutboxEventDto {
        let err = self.last_error.clone().or_else(|| self.error_message.clone());
        OutboxEventDto {
            id: self.id.clone(),
            tenant_id: self.tenant_id.clone(),
            event_type: self.event_type.clone(),
            aggregate_type: self.aggregate_type.clone(),
            aggregate_id: self.aggregate_id.clone(),
            payload: self.payload_value(),
            payload_json: self.payload_json.clone(),
            status: self.status.clone(),
            retry_count: self.retry_count,
            attempt_count: self.attempt_count,
            max_retries: self.max_retries,
            next_retry_at: self.next_retry_at.clone(),
            last_error: err.clone(),
            error_message: err,
            created_at: self.created_at.clone(),
            published_at: self.published_at.clone(),
        }
    }
}

/// Outbox Event API Response DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEventDto {
    pub id: String,
    pub tenant_id: String,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub payload: serde_json::Value,
    pub payload_json: String,
    pub status: String,
    pub retry_count: i64,
    pub attempt_count: i64,
    pub max_retries: i64,
    pub next_retry_at: String,
    pub last_error: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub published_at: Option<String>,
}

/// Outbox Event Draft for Atomic Insertion
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEventDraft {
    pub tenant_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub payload_json: serde_json::Value,
    pub max_retries: i64,
}

impl OutboxEventDraft {
    pub fn new(
        tenant_id: Uuid,
        event_type: impl Into<String>,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            tenant_id,
            event_type: event_type.into(),
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
            payload_json: payload,
            max_retries: 5,
        }
    }

    pub fn invoice_issued(
        tenant_id: Uuid,
        invoice_id: &str,
        invoice_number: &str,
        total_amount: i64,
        customer_name: &str,
    ) -> Self {
        Self::new(
            tenant_id,
            "InvoiceIssued",
            "Invoice",
            invoice_id,
            serde_json::json!({
                "invoice_id": invoice_id,
                "invoice_number": invoice_number,
                "total_amount": total_amount,
                "customer_name": customer_name,
            }),
        )
    }

    pub fn invoice_voided(
        tenant_id: Uuid,
        invoice_id: &str,
        invoice_number: &str,
        reason: &str,
    ) -> Self {
        Self::new(
            tenant_id,
            "InvoiceVoided",
            "Invoice",
            invoice_id,
            serde_json::json!({
                "invoice_id": invoice_id,
                "invoice_number": invoice_number,
                "reason": reason,
            }),
        )
    }

    pub fn payment_confirmed(
        tenant_id: Uuid,
        payment_id: &str,
        invoice_id: &str,
        amount: i64,
        reference: Option<&str>,
    ) -> Self {
        Self::new(
            tenant_id,
            "PaymentConfirmed",
            "Payment",
            payment_id,
            serde_json::json!({
                "payment_id": payment_id,
                "invoice_id": invoice_id,
                "amount": amount,
                "reference": reference,
            }),
        )
    }

    pub fn journal_posted(
        tenant_id: Uuid,
        journal_id: &str,
        entry_number: &str,
        total_amount: i64,
    ) -> Self {
        Self::new(
            tenant_id,
            "JournalPosted",
            "Journal",
            journal_id,
            serde_json::json!({
                "journal_id": journal_id,
                "entry_number": entry_number,
                "total_amount": total_amount,
            }),
        )
    }

    pub fn journal_reversed(
        tenant_id: Uuid,
        original_journal_id: &str,
        reversal_journal_id: &str,
    ) -> Self {
        Self::new(
            tenant_id,
            "JournalReversed",
            "Journal",
            reversal_journal_id,
            serde_json::json!({
                "original_journal_id": original_journal_id,
                "reversal_journal_id": reversal_journal_id,
            }),
        )
    }

    pub fn stock_received(
        tenant_id: Uuid,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(tenant_id, "StockReceived", aggregate_type, aggregate_id, payload)
    }

    pub fn stock_deducted(
        tenant_id: Uuid,
        aggregate_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(tenant_id, "StockDeducted", "Inventory", aggregate_id, payload)
    }

    pub fn stock_adjusted(
        tenant_id: Uuid,
        aggregate_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(tenant_id, "StockAdjusted", "Inventory", aggregate_id, payload)
    }

    pub fn stock_transferred(
        tenant_id: Uuid,
        aggregate_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(tenant_id, "StockTransferred", "Inventory", aggregate_id, payload)
    }
}


/// Helper function to format DateTime<Utc> into RFC3339 with 'Z' suffix
pub fn format_utc_iso_z(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

/// Helper function to get current UTC timestamp with 'Z' suffix
pub fn now_utc_iso_z() -> String {
    format_utc_iso_z(Utc::now())
}
