use crate::domain::money::Rupiah;
use crate::error::AppError;
use crate::repository::{
    account_repo::SqlxAccountRepository,
    idempotency_repo::{IdempotencyLockResult, IdempotencyRepository},
    transaction_repo::{
        CashFlowSummary, NewTransaction, TransactionFilter, TransactionRecord,
        TransactionRepository,
    },
    DbError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("Account not found or does not belong to user")]
    AccountNotFound,

    #[error("Transaction not found")]
    TransactionNotFound,

    #[error("Transfer requires a distinct destination account")]
    InvalidTransferTarget,

    #[error("Idempotent duplicate: {0}")]
    DuplicateIdempotentKey(String),

    #[error("Database error: {0}")]
    Db(#[from] DbError),
}

impl From<LedgerError> for AppError {
    fn from(err: LedgerError) -> Self {
        match err {
            LedgerError::AccountNotFound | LedgerError::TransactionNotFound => {
                AppError::NotFound(err.to_string(), "NOT_FOUND")
            }
            LedgerError::InvalidTransferTarget => {
                AppError::BadRequest(err.to_string(), "INVALID_TRANSFER")
            }
            LedgerError::DuplicateIdempotentKey(msg) => {
                AppError::Conflict(msg, "IDEMPOTENCY_DUPLICATE")
            }
            LedgerError::Db(e) => AppError::from(e),
        }
    }
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTransactionRequest {
    pub idempotency_key: String,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub category_id: Option<String>,
    pub transaction_type: String,
    /// Strictly positive integer Rupiah.
    pub amount: Rupiah,
    pub date: String,
    pub description: String,
    pub notes: Option<String>,
    pub is_recurring: Option<bool>,
}

impl CreateTransactionRequest {
    /// Computes a deterministic SHA-256 hash of the transaction business payload.
    /// Excludes the idempotency key to allow verifying if the same key is reused
    /// with a conflicting or mutated payload.
    pub fn compute_payload_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.account_id.as_bytes());
        hasher.update(b"|");
        hasher.update(self.to_account_id.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(self.category_id.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(self.transaction_type.as_bytes());
        hasher.update(b"|");
        hasher.update(self.amount.as_i64().to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(self.date.as_bytes());
        hasher.update(b"|");
        hasher.update(self.description.as_bytes());
        hasher.update(b"|");
        hasher.update(self.notes.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(if self.is_recurring.unwrap_or(false) {
            b"1"
        } else {
            b"0"
        });
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub transaction: TransactionRecord,
    pub account_balance: Rupiah,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionListResponse {
    pub data: Vec<TransactionRecord>,
    pub meta: PaginationMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct PaginationMeta {
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub total_pages: i64,
}

// ---------------------------------------------------------------------------
// Ledger Service
// ---------------------------------------------------------------------------

pub struct LedgerService {
    pool: SqlitePool,
    transaction_repo: Arc<dyn TransactionRepository>,
    idempotency_repo: Arc<dyn IdempotencyRepository>,
}

impl LedgerService {
    pub fn new(
        pool: SqlitePool,
        transaction_repo: Arc<dyn TransactionRepository>,
        idempotency_repo: Arc<dyn IdempotencyRepository>,
    ) -> Self {
        Self {
            pool,
            transaction_repo,
            idempotency_repo,
        }
    }

    /// Create a transaction with idempotency guarantee and atomic balance update.
    ///
    /// Flow:
    ///   1. Acquire idempotency lock (user_id, idempotency_key).
    ///   2. If Cached → return deserialized cached response (exact replay).
    ///   3. If InProgress → conflict error.
    ///   4. Open SQLite write transaction.
    ///   5. Insert transaction row.
    ///   6. Atomically adjust account balance(s) via UPDATE … RETURNING.
    ///   7. Mark idempotency key completed with cached HTTP 201 body.
    ///   8. Commit.
    pub async fn create_transaction(
        &self,
        user_id: &str,
        request: CreateTransactionRequest,
    ) -> Result<TransactionResponse, LedgerError> {
        // Validate amount is strictly positive.
        if !request.amount.is_positive() {
            return Err(LedgerError::Db(DbError::Validation(
                "Transaction amount must be strictly positive".to_string(),
            )));
        }

        // Validate transfer has a distinct target account.
        if request.transaction_type == "transfer" {
            match &request.to_account_id {
                None => return Err(LedgerError::InvalidTransferTarget),
                Some(to_id) if to_id == &request.account_id => {
                    return Err(LedgerError::InvalidTransferTarget)
                }
                _ => {}
            }
        }

        let request_hash = request.compute_payload_hash();

        // Idempotency lock — 24-hour TTL is sufficient for financial transactions.
        match self
            .idempotency_repo
            .acquire_lock(user_id, &request.idempotency_key, &request_hash, 86_400)
            .await?
        {
            IdempotencyLockResult::Cached {
                response_code: _,
                response_body,
            } => {
                if let Ok(resp) = serde_json::from_str::<TransactionResponse>(&response_body) {
                    return Ok(resp);
                }
                return Err(LedgerError::DuplicateIdempotentKey(
                    request.idempotency_key.clone(),
                ));
            }
            IdempotencyLockResult::InProgress => {
                return Err(LedgerError::DuplicateIdempotentKey(format!(
                    "Request '{}' is already in progress",
                    request.idempotency_key
                )));
            }
            IdempotencyLockResult::MismatchedPayload => {
                return Err(LedgerError::DuplicateIdempotentKey(format!(
                    "Idempotency key '{}' was used with a different payload",
                    request.idempotency_key
                )));
            }
            IdempotencyLockResult::Acquired => {}
        }

        let mut db_tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from_sqlx)?;

        let transaction_id = uuid::Uuid::new_v4().to_string();
        let new_tx = NewTransaction {
            id: transaction_id,
            user_id: user_id.to_string(),
            account_id: request.account_id.clone(),
            to_account_id: request.to_account_id.clone(),
            category_id: request.category_id.clone(),
            transaction_type: request.transaction_type.clone(),
            amount: request.amount,
            date: request.date.clone(),
            description: request.description.clone(),
            notes: request.notes.clone(),
            is_recurring: request.is_recurring.unwrap_or(false),
        };

        let record = match self
            .transaction_repo
            .create_in_tx(&mut db_tx, &new_tx)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = db_tx.rollback().await;
                let _ = self
                    .idempotency_repo
                    .release_lock_on_failure(user_id, &request.idempotency_key)
                    .await;
                return Err(LedgerError::Db(e));
            }
        };

        // Compute signed deltas.
        let (source_delta, dest_delta): (i64, Option<i64>) = match request.transaction_type.as_str()
        {
            "income" => (request.amount.0, None),
            "expense" => (-request.amount.0, None),
            "transfer" => (-request.amount.0, Some(request.amount.0)),
            _ => {
                let _ = db_tx.rollback().await;
                let _ = self
                    .idempotency_repo
                    .release_lock_on_failure(user_id, &request.idempotency_key)
                    .await;
                return Err(LedgerError::Db(DbError::Validation(format!(
                    "Unknown transaction_type: {}",
                    request.transaction_type
                ))));
            }
        };

        let account_balance = match SqlxAccountRepository::adjust_balance_atomic_tx(
            &mut db_tx,
            user_id,
            &request.account_id,
            source_delta,
        )
        .await
        {
            Ok(b) => b,
            Err(e) => {
                let _ = db_tx.rollback().await;
                let _ = self
                    .idempotency_repo
                    .release_lock_on_failure(user_id, &request.idempotency_key)
                    .await;
                return Err(match e {
                    DbError::NotFound => LedgerError::AccountNotFound,
                    other => LedgerError::Db(other),
                });
            }
        };

        // Transfer: adjust destination account.
        if let (Some(to_id), Some(delta)) = (&request.to_account_id, dest_delta) {
            if let Err(e) =
                SqlxAccountRepository::adjust_balance_atomic_tx(&mut db_tx, user_id, to_id, delta)
                    .await
            {
                let _ = db_tx.rollback().await;
                let _ = self
                    .idempotency_repo
                    .release_lock_on_failure(user_id, &request.idempotency_key)
                    .await;
                return Err(match e {
                    DbError::NotFound => LedgerError::AccountNotFound,
                    other => LedgerError::Db(other),
                });
            }
        }

        let response = TransactionResponse {
            transaction: record,
            account_balance,
        };
        let cached_body = serde_json::to_string(&response).unwrap_or_default();

        // Persist idempotency result inside the transaction before commit.
        if let Err(e) = self
            .idempotency_repo
            .save_response_tx(
                &mut db_tx,
                user_id,
                &request.idempotency_key,
                201,
                &cached_body,
            )
            .await
        {
            let _ = db_tx.rollback().await;
            return Err(LedgerError::Db(e));
        }

        db_tx.commit().await.map_err(DbError::from_sqlx)?;
        Ok(response)
    }

    /// Delete a transaction and atomically reverse its balance impact.
    pub async fn delete_transaction(
        &self,
        user_id: &str,
        transaction_id: &str,
    ) -> Result<(), LedgerError> {
        let mut db_tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from_sqlx)?;

        let record = self
            .transaction_repo
            .delete_in_tx(&mut db_tx, user_id, transaction_id)
            .await
            .map_err(|e| match e {
                DbError::NotFound => LedgerError::TransactionNotFound,
                other => LedgerError::Db(other),
            })?;

        let reversal_delta = match record.transaction_type.as_str() {
            "income" => -record.amount.0,
            "expense" => record.amount.0,
            "transfer" => record.amount.0,
            _ => 0,
        };

        SqlxAccountRepository::adjust_balance_atomic_tx(
            &mut db_tx,
            user_id,
            &record.account_id,
            reversal_delta,
        )
        .await
        .map_err(|e| match e {
            DbError::NotFound => LedgerError::AccountNotFound,
            other => LedgerError::Db(other),
        })?;

        if record.transaction_type == "transfer" {
            if let Some(to_id) = &record.to_account_id {
                SqlxAccountRepository::adjust_balance_atomic_tx(
                    &mut db_tx,
                    user_id,
                    to_id,
                    -record.amount.0,
                )
                .await
                .map_err(|e| match e {
                    DbError::NotFound => LedgerError::AccountNotFound,
                    other => LedgerError::Db(other),
                })?;
            }
        }

        db_tx.commit().await.map_err(DbError::from_sqlx)?;
        Ok(())
    }

    /// List transactions with pagination and optional filters.
    pub async fn list_transactions(
        &self,
        user_id: &str,
        filter: TransactionFilter,
    ) -> Result<TransactionListResponse, LedgerError> {
        let per_page = filter.per_page.clamp(1, 100);
        let page = filter.page.max(1);
        let filter = TransactionFilter {
            page,
            per_page,
            ..filter
        };

        let (data, total) = self.transaction_repo.list(user_id, &filter).await?;
        let total_pages = (total + per_page - 1) / per_page;

        Ok(TransactionListResponse {
            data,
            meta: PaginationMeta {
                page,
                per_page,
                total,
                total_pages,
            },
        })
    }

    /// Get a single transaction by ID, enforcing user ownership.
    pub async fn get_transaction(
        &self,
        user_id: &str,
        transaction_id: &str,
    ) -> Result<TransactionRecord, LedgerError> {
        self.transaction_repo
            .find_by_id(user_id, transaction_id)
            .await?
            .ok_or(LedgerError::TransactionNotFound)
    }

    /// Compute net cash flow for a user over an optional date range.
    ///
    /// This is the **single shared calculation** — used by dashboard, analytics, and reports.
    /// Never re-implement this aggregation in callers; always call this method.
    pub async fn cash_flow_summary(
        &self,
        user_id: &str,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<CashFlowSummary, LedgerError> {
        self.transaction_repo
            .cash_flow_summary(user_id, date_from, date_to)
            .await
            .map_err(LedgerError::Db)
    }
}
