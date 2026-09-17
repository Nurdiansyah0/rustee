use crate::domain::ingestion::*;
use crate::domain::money::Rupiah;
use crate::error::AppError;
use crate::repository::{
    account_repo::{AccountRepository, SqlxAccountRepository},
    category_repo::{CategoryRepository, SqlxCategoryRepository},
    idempotency_repo::{IdempotencyLockResult, IdempotencyRepository, SqlxIdempotencyRepository},
    ingestion_repo::{
        CreateIngestedTxParams, IngestionRepository, NewIngestionEvent,
        SqlxIngestionRepository,
    },
    transaction_repo::TransactionRecord,
    DbError,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DTOs & Payload Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationPayload {
    pub package_name: String,
    pub title: String,
    pub text: String,
    pub sub_text: Option<String>,
    pub posted_at: Option<i64>, // epoch milliseconds or unix seconds
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmsPayload {
    pub sender: String,
    pub body: String,
    pub received_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GmailPayload {
    pub message_id: String,
    pub snippet: String,
    pub body: String,
    pub internal_date: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResponse {
    pub event_id: String,
    pub status: String,
    pub confidence: String,
    pub transaction_id: Option<String>,
    pub candidate: Option<CanonicalTransactionCandidate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfirmCandidateRequest {
    pub account_id: String,
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmCandidateResponse {
    pub transaction: TransactionRecord,
    pub account_balance: Rupiah,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectCandidateResponse {
    pub event_id: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum IngestionError {
    #[error("Candidate event not found")]
    NotFound,

    #[error("Candidate cannot be confirmed: current status is '{0}'")]
    InvalidCandidateState(String),

    #[error("Target account not found or inaccessible: {0}")]
    AccountNotFound(String),

    #[error("Target category not found: {0}")]
    CategoryNotFound(String),

    #[error("Idempotency conflict: {0}")]
    IdempotencyConflict(String),

    #[error("Database error: {0}")]
    Db(#[from] DbError),
}

impl From<IngestionError> for AppError {
    fn from(err: IngestionError) -> Self {
        match err {
            IngestionError::NotFound => AppError::NotFound(err.to_string(), "NOT_FOUND"),
            IngestionError::InvalidCandidateState(s) => {
                AppError::BadRequest(format!("Invalid candidate state: {}", s), "INVALID_STATE")
            }
            IngestionError::AccountNotFound(msg) => AppError::NotFound(msg, "ACCOUNT_NOT_FOUND"),
            IngestionError::CategoryNotFound(msg) => AppError::NotFound(msg, "CATEGORY_NOT_FOUND"),
            IngestionError::IdempotencyConflict(msg) => {
                AppError::Conflict(msg, "IDEMPOTENCY_CONFLICT")
            }
            IngestionError::Db(e) => AppError::from(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Ingestion Service
// ---------------------------------------------------------------------------

pub struct IngestionService {
    pool: SqlitePool,
    ingestion_repo: Arc<dyn IngestionRepository>,
    account_repo: Arc<dyn AccountRepository>,
    category_repo: Arc<dyn CategoryRepository>,
    idempotency_repo: Arc<dyn IdempotencyRepository>,
}

impl IngestionService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            ingestion_repo: Arc::new(SqlxIngestionRepository::new(pool.clone())),
            account_repo: Arc::new(SqlxAccountRepository::new(pool.clone())),
            category_repo: Arc::new(SqlxCategoryRepository::new(pool.clone())),
            idempotency_repo: Arc::new(SqlxIdempotencyRepository::new(pool.clone())),
            pool,
        }
    }

    pub fn with_repos(
        pool: SqlitePool,
        ingestion_repo: Arc<dyn IngestionRepository>,
        account_repo: Arc<dyn AccountRepository>,
        category_repo: Arc<dyn CategoryRepository>,
        idempotency_repo: Arc<dyn IdempotencyRepository>,
    ) -> Self {
        Self {
            pool,
            ingestion_repo,
            account_repo,
            category_repo,
            idempotency_repo,
        }
    }

    // -----------------------------------------------------------------------
    // Stage 1-9 Ingestion Pipeline
    // -----------------------------------------------------------------------

    /// Executes the 9-Stage Ingestion Pipeline for an Android Notification:
    /// 1. Source Adapter
    /// 2. Provider Parser
    /// 3. Normalizer
    /// 4. Validator
    /// 5. Confidence Engine
    /// 6. Cross-Source Deduplication
    /// 7. Transaction Candidate
    /// 8. Domain Validation
    /// 9. Ledger Ingestion
    pub async fn process_notification(
        &self,
        user_id: &str,
        idempotency_key: Option<&str>,
        payload: NotificationPayload,
    ) -> Result<IngestionResponse, IngestionError> {
        let raw_repr = format!(
            "{}:{}:{}:{:?}:{:?}",
            payload.package_name, payload.title, payload.text, payload.sub_text, payload.posted_at
        );
        let raw_hash = compute_payload_hash(&raw_repr);

        // Optional Idempotency-Key handling
        if let Some(key) = idempotency_key {
            match self
                .idempotency_repo
                .acquire_lock(user_id, key, &raw_hash, 86_400)
                .await?
            {
                IdempotencyLockResult::Cached { response_body, .. } => {
                    if let Ok(cached) = serde_json::from_str::<IngestionResponse>(&response_body) {
                        return Ok(cached);
                    }
                }
                IdempotencyLockResult::InProgress => {
                    return Err(IngestionError::IdempotencyConflict(format!(
                        "Request with idempotency key '{}' is in progress",
                        key
                    )));
                }
                IdempotencyLockResult::MismatchedPayload => {
                    return Err(IngestionError::IdempotencyConflict(format!(
                        "Idempotency key '{}' was used with a different payload",
                        key
                    )));
                }
                IdempotencyLockResult::Acquired => {}
            }
        }

        // Fast path: Exact raw payload hash duplicate check (Privacy REQ-INGEST-08)
        if let Some(existing) = self.ingestion_repo.find_by_hash(user_id, &raw_hash).await? {
            let candidate: Option<CanonicalTransactionCandidate> = existing
                .parsed_candidate
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok());

            let resp = IngestionResponse {
                event_id: existing.id,
                status: "duplicate".to_string(),
                confidence: existing.confidence,
                transaction_id: None,
                candidate,
            };

            if let Some(key) = idempotency_key {
                if let Ok(body) = serde_json::to_string(&resp) {
                    let _ = self
                        .idempotency_repo
                        .save_response(user_id, key, 200, &body)
                        .await;
                }
            }

            return Ok(resp);
        }

        // Stage 2: Provider Parser
        let detected_provider = detect_provider(&payload.package_name, &payload.text);
        let mut signal = match detected_provider.as_str() {
            "bca" => parse_bca_notification(&payload.title, &payload.text),
            "mandiri" => parse_mandiri_notification(&payload.title, &payload.text),
            "bri" => parse_bri_notification(&payload.title, &payload.text),
            "bni" => parse_bni_notification(&payload.title, &payload.text),
            "dana" => parse_dana_notification(&payload.title, &payload.text),
            "gopay" => parse_gopay_notification(&payload.title, &payload.text),
            "ovo" => parse_ovo_notification(&payload.title, &payload.text),
            "shopeepay" => parse_shopeepay_notification(&payload.title, &payload.text),
            _ => {
                // Generic fallback
                let combined = format!("{} {}", payload.title, payload.text);
                let is_financial = combined.to_lowercase().contains("berhasil")
                    || combined.to_lowercase().contains("transfer")
                    || combined.to_lowercase().contains("bayar")
                    || combined.to_lowercase().contains("rp");
                ParsedSignal {
                    provider: detected_provider.clone(),
                    amount: parse_idr_amount(&combined),
                    direction: if combined.to_lowercase().contains("masuk") || combined.to_lowercase().contains("terima") {
                        Some(TransactionDirection::Income)
                    } else if combined.to_lowercase().contains("keluar") || combined.to_lowercase().contains("bayar") {
                        Some(TransactionDirection::Expense)
                    } else {
                        None
                    },
                    merchant: parse_merchant_candidate(&combined),
                    external_reference: parse_external_reference(&combined),
                    occurred_at: None,
                    is_financial,
                }
            }
        };

        // Timestamp normalization
        let occurred_at = if let Some(ts) = payload.posted_at {
            // ts could be seconds or milliseconds
            let epoch_secs = if ts > 100_000_000_000 { ts / 1000 } else { ts };
            DateTime::from_timestamp(epoch_secs, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339())
        } else {
            Utc::now().to_rfc3339()
        };
        signal.occurred_at = Some(occurred_at.clone());

        let res = self
            .execute_pipeline_core(
                user_id,
                IngestionSource::Notification,
                &raw_hash,
                signal,
                occurred_at,
            )
            .await;

        match res {
            Ok(resp) => {
                if let Some(key) = idempotency_key {
                    if let Ok(body) = serde_json::to_string(&resp) {
                        let _ = self
                            .idempotency_repo
                            .save_response(user_id, key, 200, &body)
                            .await;
                    }
                }
                Ok(resp)
            }
            Err(e) => {
                if let Some(key) = idempotency_key {
                    let _ = self
                        .idempotency_repo
                        .release_lock_on_failure(user_id, key)
                        .await;
                }
                Err(e)
            }
        }
    }

    /// Process SMS Ingestion payload
    pub async fn process_sms(
        &self,
        user_id: &str,
        idempotency_key: Option<&str>,
        payload: SmsPayload,
    ) -> Result<IngestionResponse, IngestionError> {
        let raw_repr = format!("{}:{}:{:?}", payload.sender, payload.body, payload.received_at);
        let raw_hash = compute_payload_hash(&raw_repr);

        if let Some(key) = idempotency_key {
            match self
                .idempotency_repo
                .acquire_lock(user_id, key, &raw_hash, 86_400)
                .await?
            {
                IdempotencyLockResult::Cached { response_body, .. } => {
                    if let Ok(cached) = serde_json::from_str::<IngestionResponse>(&response_body) {
                        return Ok(cached);
                    }
                }
                IdempotencyLockResult::InProgress => {
                    return Err(IngestionError::IdempotencyConflict(format!(
                        "Request with idempotency key '{}' is in progress",
                        key
                    )));
                }
                IdempotencyLockResult::MismatchedPayload => {
                    return Err(IngestionError::IdempotencyConflict(format!(
                        "Idempotency key '{}' was used with a different payload",
                        key
                    )));
                }
                IdempotencyLockResult::Acquired => {}
            }
        }

        let signal = parse_sms_payload(&payload.sender, &payload.body);
        let occurred_at = if let Some(ts) = payload.received_at {
            let epoch_secs = if ts > 100_000_000_000 { ts / 1000 } else { ts };
            DateTime::from_timestamp(epoch_secs, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339())
        } else {
            Utc::now().to_rfc3339()
        };

        let res = self
            .execute_pipeline_core(user_id, IngestionSource::Sms, &raw_hash, signal, occurred_at)
            .await;

        match res {
            Ok(resp) => {
                if let Some(key) = idempotency_key {
                    if let Ok(body) = serde_json::to_string(&resp) {
                        let _ = self
                            .idempotency_repo
                            .save_response(user_id, key, 200, &body)
                            .await;
                    }
                }
                Ok(resp)
            }
            Err(e) => {
                if let Some(key) = idempotency_key {
                    let _ = self
                        .idempotency_repo
                        .release_lock_on_failure(user_id, key)
                        .await;
                }
                Err(e)
            }
        }
    }

    /// Process Gmail OAuth Ingestion payload
    pub async fn process_gmail(
        &self,
        user_id: &str,
        idempotency_key: Option<&str>,
        payload: GmailPayload,
    ) -> Result<IngestionResponse, IngestionError> {
        let raw_repr = format!(
            "{}:{}:{}:{:?}",
            payload.message_id, payload.snippet, payload.body, payload.internal_date
        );
        let raw_hash = compute_payload_hash(&raw_repr);

        if let Some(key) = idempotency_key {
            match self
                .idempotency_repo
                .acquire_lock(user_id, key, &raw_hash, 86_400)
                .await?
            {
                IdempotencyLockResult::Cached { response_body, .. } => {
                    if let Ok(cached) = serde_json::from_str::<IngestionResponse>(&response_body) {
                        return Ok(cached);
                    }
                }
                IdempotencyLockResult::InProgress => {
                    return Err(IngestionError::IdempotencyConflict(format!(
                        "Request with idempotency key '{}' is in progress",
                        key
                    )));
                }
                IdempotencyLockResult::MismatchedPayload => {
                    return Err(IngestionError::IdempotencyConflict(format!(
                        "Idempotency key '{}' was used with a different payload",
                        key
                    )));
                }
                IdempotencyLockResult::Acquired => {}
            }
        }

        let signal = parse_gmail_payload(&payload.snippet, &payload.body);
        let occurred_at = if let Some(ts) = payload.internal_date {
            let epoch_secs = if ts > 100_000_000_000 { ts / 1000 } else { ts };
            DateTime::from_timestamp(epoch_secs, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339())
        } else {
            Utc::now().to_rfc3339()
        };

        let res = self
            .execute_pipeline_core(
                user_id,
                IngestionSource::Gmail,
                &raw_hash,
                signal,
                occurred_at,
            )
            .await;

        match res {
            Ok(resp) => {
                if let Some(key) = idempotency_key {
                    if let Ok(body) = serde_json::to_string(&resp) {
                        let _ = self
                            .idempotency_repo
                            .save_response(user_id, key, 200, &body)
                            .await;
                    }
                }
                Ok(resp)
            }
            Err(e) => {
                if let Some(key) = idempotency_key {
                    let _ = self
                        .idempotency_repo
                        .release_lock_on_failure(user_id, key)
                        .await;
                }
                Err(e)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Core Pipeline Stages 4-9
    // -----------------------------------------------------------------------

    async fn execute_pipeline_core(
        &self,
        user_id: &str,
        source: IngestionSource,
        raw_hash: &str,
        signal: ParsedSignal,
        occurred_at: String,
    ) -> Result<IngestionResponse, IngestionError> {
        let event_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // Stage 4: Validator
        let amount_valid = signal.amount.map(|a| a.is_positive()).unwrap_or(false);
        if !signal.is_financial || !amount_valid || signal.direction.is_none() {
            // Low confidence / unparseable / non-financial
            let new_event = NewIngestionEvent {
                id: event_id.clone(),
                user_id: user_id.to_string(),
                source: source.to_string(),
                raw_payload_hash: raw_hash.to_string(),
                status: IngestionStatus::Rejected.to_string(),
                confidence: ConfidenceLevel::LOW.to_string(),
                parsed_candidate: None,
                created_at: now.clone(),
                processed_at: Some(now),
            };
            self.ingestion_repo.create_event(&new_event).await?;

            return Ok(IngestionResponse {
                event_id,
                status: IngestionStatus::Rejected.to_string(),
                confidence: ConfidenceLevel::LOW.to_string(),
                transaction_id: None,
                candidate: None,
            });
        }

        let amount = signal.amount.unwrap();
        let direction = signal.direction.unwrap();

        // Stage 5: Confidence Engine & Account Resolution
        // Attempt to match an active user wallet/account for this provider
        let user_accounts = self.account_repo.list_by_user(user_id, false).await?;
        let active_accounts: Vec<_> = user_accounts.into_iter().filter(|a| !a.is_archived).collect();

        let resolved_account = active_accounts.iter().find(|acc| {
            let name_lower = acc.name.to_lowercase();
            let provider_lower = signal.provider.to_lowercase();
            name_lower.contains(&provider_lower) || provider_lower.contains(&name_lower)
        }).or_else(|| {
            // If only 1 account exists, default to it
            if active_accounts.len() == 1 {
                active_accounts.first()
            } else {
                None
            }
        });

        let account_resolved = resolved_account.is_some();
        let (confidence, requires_confirmation) =
            evaluate_confidence(&signal, account_resolved);

        // Stage 7: Build Candidate
        let candidate = CanonicalTransactionCandidate {
            candidate_id: Uuid::new_v4().to_string(),
            event_id: event_id.clone(),
            transaction_id: None,
            amount,
            currency: "IDR".to_string(),
            direction,
            occurred_at: occurred_at.clone(),
            provider: signal.provider.clone(),
            merchant: signal.merchant.clone(),
            account_id: resolved_account.map(|a| a.id.clone()),
            category_id: None,
            source,
            external_reference: signal.external_reference.clone(),
            confidence,
            requires_confirmation,
        };

        // Stage 6: Cross-Source Signal Deduplication (±300s window)
        // Check recent events and transactions for cross-source duplicates
        let recent_events = self.ingestion_repo.list_recent_events(user_id, 20).await?;
        for ev in recent_events {
            if let Some(c_json) = &ev.parsed_candidate {
                if let Ok(other_candidate) =
                    serde_json::from_str::<CanonicalTransactionCandidate>(c_json)
                {
                    if candidate.is_duplicate_of(&other_candidate, 300) {
                        // Mark duplicate
                        let candidate_json = serde_json::to_string(&candidate).ok();
                        let new_event = NewIngestionEvent {
                            id: event_id.clone(),
                            user_id: user_id.to_string(),
                            source: source.to_string(),
                            raw_payload_hash: raw_hash.to_string(),
                            status: IngestionStatus::Duplicate.to_string(),
                            confidence: confidence.to_string(),
                            parsed_candidate: candidate_json,
                            created_at: now.clone(),
                            processed_at: Some(now.clone()),
                        };
                        self.ingestion_repo.create_event(&new_event).await?;

                        return Ok(IngestionResponse {
                            event_id,
                            status: IngestionStatus::Duplicate.to_string(),
                            confidence: confidence.to_string(),
                            transaction_id: other_candidate.transaction_id,
                            candidate: Some(candidate),
                        });
                    }
                }
            }
        }

        // Also check committed transactions table within ±300s
        if let Ok(Some(existing_tx)) = self
            .find_duplicate_in_transactions(user_id, &candidate, 300)
            .await
        {
            let candidate_json = serde_json::to_string(&candidate).ok();
            let new_event = NewIngestionEvent {
                id: event_id.clone(),
                user_id: user_id.to_string(),
                source: source.to_string(),
                raw_payload_hash: raw_hash.to_string(),
                status: IngestionStatus::Duplicate.to_string(),
                confidence: confidence.to_string(),
                parsed_candidate: candidate_json,
                created_at: now.clone(),
                processed_at: Some(now.clone()),
            };
            self.ingestion_repo.create_event(&new_event).await?;

            return Ok(IngestionResponse {
                event_id,
                status: IngestionStatus::Duplicate.to_string(),
                confidence: confidence.to_string(),
                transaction_id: Some(existing_tx.id),
                candidate: Some(candidate),
            });
        }

        // Stage 8 & 9: Candidate action based on Confidence
        if confidence == ConfidenceLevel::HIGH && account_resolved {
            // HIGH confidence & domain validated -> Auto-create transaction in ledger
            let account_id = candidate.account_id.as_ref().unwrap();

            // Find default category
            let categories = self.category_repo.list_by_user(user_id).await?;
            let category_id = categories
                .into_iter()
                .find(|c| c.category_type.eq_ignore_ascii_case(&direction.to_string()))
                .map(|c| c.id);

            let tx_id = Uuid::new_v4().to_string();
            let merchant_str = candidate.merchant.clone().unwrap_or_else(|| signal.provider.clone());
            let description = format!("{} via {}", merchant_str, signal.provider);

            let mut final_candidate = candidate.clone();
            final_candidate.transaction_id = Some(tx_id.clone());
            final_candidate.category_id = category_id.clone();
            let candidate_json = serde_json::to_string(&final_candidate).ok();

            // Create ingestion event record first with status pending
            let new_event = NewIngestionEvent {
                id: event_id.clone(),
                user_id: user_id.to_string(),
                source: source.to_string(),
                raw_payload_hash: raw_hash.to_string(),
                status: IngestionStatus::Pending.to_string(),
                confidence: ConfidenceLevel::HIGH.to_string(),
                parsed_candidate: candidate_json,
                created_at: now.clone(),
                processed_at: None,
            };
            self.ingestion_repo.create_event(&new_event).await?;

            // Stage 9: Ledger commit
            let params = CreateIngestedTxParams {
                transaction_id: tx_id.clone(),
                user_id: user_id.to_string(),
                account_id: account_id.clone(),
                category_id,
                transaction_type: direction.to_string(),
                amount,
                date: occurred_at,
                description,
                notes: signal.external_reference.as_ref().map(|r| format!("Ref: {}", r)),
                source: source.to_string(),
                external_reference: signal.external_reference,
                merchant: signal.merchant,
                confidence: "HIGH".to_string(),
                ingestion_id: event_id.clone(),
            };

            let (tx_record, _) = self.ingestion_repo.commit_transaction_atomic(&params).await?;

            Ok(IngestionResponse {
                event_id,
                status: IngestionStatus::AutoCreated.to_string(),
                confidence: ConfidenceLevel::HIGH.to_string(),
                transaction_id: Some(tx_record.id),
                candidate: Some(final_candidate),
            })
        } else {
            // MEDIUM confidence: Ambiguity in account/category -> Store as candidate requiring user confirmation
            let candidate_json = serde_json::to_string(&candidate).ok();
            let new_event = NewIngestionEvent {
                id: event_id.clone(),
                user_id: user_id.to_string(),
                source: source.to_string(),
                raw_payload_hash: raw_hash.to_string(),
                status: IngestionStatus::RequiresConfirmation.to_string(),
                confidence: ConfidenceLevel::MEDIUM.to_string(),
                parsed_candidate: candidate_json,
                created_at: now.clone(),
                processed_at: None,
            };
            self.ingestion_repo.create_event(&new_event).await?;

            Ok(IngestionResponse {
                event_id,
                status: IngestionStatus::RequiresConfirmation.to_string(),
                confidence: ConfidenceLevel::MEDIUM.to_string(),
                transaction_id: None,
                candidate: Some(candidate),
            })
        }
    }

    async fn find_duplicate_in_transactions(
        &self,
        user_id: &str,
        candidate: &CanonicalTransactionCandidate,
        window_secs: i64,
    ) -> Result<Option<TransactionRecord>, DbError> {
        let rows = sqlx::query_as::<_, TransactionRecord>(
            r#"
            SELECT id, user_id, account_id, to_account_id, category_id, transaction_type,
                   amount, date, description, notes, is_recurring, created_at, updated_at
            FROM transactions
            WHERE user_id = ?1 AND amount = ?2 AND transaction_type = ?3
            ORDER BY date DESC
            LIMIT 10
            "#,
        )
        .bind(user_id)
        .bind(candidate.amount.0)
        .bind(candidate.direction.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        for tx in rows {
            if within_time_window(&tx.date, &candidate.occurred_at, window_secs) {
                return Ok(Some(tx));
            }
        }

        Ok(None)
    }

    // -----------------------------------------------------------------------
    // Candidate Review API Service Methods
    // -----------------------------------------------------------------------

    pub async fn list_pending_candidates(
        &self,
        user_id: &str,
    ) -> Result<Vec<CanonicalTransactionCandidate>, IngestionError> {
        let records = self.ingestion_repo.list_pending_candidates(user_id).await?;
        let mut result = Vec::new();

        for rec in records {
            if let Some(json) = rec.parsed_candidate {
                if let Ok(candidate) = serde_json::from_str::<CanonicalTransactionCandidate>(&json) {
                    result.push(candidate);
                }
            }
        }

        Ok(result)
    }

    pub async fn confirm_candidate(
        &self,
        user_id: &str,
        candidate_id: &str,
        req: ConfirmCandidateRequest,
    ) -> Result<ConfirmCandidateResponse, IngestionError> {
        // Find the event
        let event = self
            .ingestion_repo
            .find_by_id(user_id, candidate_id)
            .await?
            .ok_or(IngestionError::NotFound)?;

        if event.status != "requires_confirmation" && event.status != "pending" {
            return Err(IngestionError::InvalidCandidateState(event.status));
        }

        // Deserialize candidate
        let candidate_json = event
            .parsed_candidate
            .ok_or_else(|| IngestionError::InvalidCandidateState("Missing parsed candidate".to_string()))?;
        let mut candidate: CanonicalTransactionCandidate = serde_json::from_str(&candidate_json)
            .map_err(|e| IngestionError::InvalidCandidateState(e.to_string()))?;

        // Domain validation: Verify account
        let account = self
            .account_repo
            .find_by_id(user_id, &req.account_id)
            .await?
            .ok_or_else(|| IngestionError::AccountNotFound(req.account_id.clone()))?;

        if account.is_archived {
            return Err(IngestionError::AccountNotFound(
                "Target account is archived".to_string(),
            ));
        }

        // Verify category if provided
        if let Some(cat_id) = &req.category_id {
            let cat = self
                .category_repo
                .find_by_id(user_id, cat_id)
                .await?
                .ok_or_else(|| IngestionError::CategoryNotFound(cat_id.clone()))?;
            if cat.deleted_at.is_some() {
                return Err(IngestionError::CategoryNotFound(
                    "Target category is deleted".to_string(),
                ));
            }
        }

        let tx_id = Uuid::new_v4().to_string();
        let merchant_str = candidate.merchant.clone().unwrap_or_else(|| candidate.provider.clone());
        let description = format!("{} via {}", merchant_str, candidate.provider);

        let params = CreateIngestedTxParams {
            transaction_id: tx_id.clone(),
            user_id: user_id.to_string(),
            account_id: req.account_id.clone(),
            category_id: req.category_id.clone(),
            transaction_type: candidate.direction.to_string(),
            amount: candidate.amount,
            date: candidate.occurred_at.clone(),
            description,
            notes: candidate
                .external_reference
                .as_ref()
                .map(|r| format!("Ref: {}", r)),
            source: candidate.source.to_string(),
            external_reference: candidate.external_reference.clone(),
            merchant: candidate.merchant.clone(),
            confidence: "HIGH".to_string(), // Confirmed by user -> becomes HIGH
            ingestion_id: candidate_id.to_string(),
        };

        // Atomically commit to ledger & update event status
        let (transaction, account_balance) = self
            .ingestion_repo
            .commit_transaction_atomic(&params)
            .await?;

        // Update candidate in record
        candidate.transaction_id = Some(tx_id);
        candidate.account_id = Some(req.account_id);
        candidate.category_id = req.category_id;
        candidate.requires_confirmation = false;
        candidate.confidence = ConfidenceLevel::HIGH;

        let updated_json = serde_json::to_string(&candidate).ok();
        let now = Utc::now().to_rfc3339();
        self.ingestion_repo
            .update_status(
                user_id,
                candidate_id,
                "auto_created",
                Some(&now),
                updated_json.as_deref(),
            )
            .await?;

        Ok(ConfirmCandidateResponse {
            transaction,
            account_balance,
            status: "confirmed".to_string(),
        })
    }

    pub async fn reject_candidate(
        &self,
        user_id: &str,
        candidate_id: &str,
    ) -> Result<RejectCandidateResponse, IngestionError> {
        let event = self
            .ingestion_repo
            .find_by_id(user_id, candidate_id)
            .await?
            .ok_or(IngestionError::NotFound)?;

        let now = Utc::now().to_rfc3339();
        self.ingestion_repo
            .update_status(user_id, candidate_id, "rejected", Some(&now), None)
            .await?;

        Ok(RejectCandidateResponse {
            event_id: event.id,
            status: "rejected".to_string(),
        })
    }
}
