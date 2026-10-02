use chrono::{Datelike, DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::accounting::{
    calculate_tax_integer, Account, AccountType, JournalEntry, JournalEntryWithLines, JournalLine,
    PostJournalEntryCommand, PostJournalLineCommand, TaxCalculationResult,
};
use crate::domain::money::Rupiah;
use crate::domain::tenant::TenantContext;
use crate::error::AppError;
use crate::repository::accounting_repo::{
    AccountingRepository, SqlxAccountingRepository, TrialBalanceDto,
};
use crate::repository::error::DbError;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAccountRequest {
    pub code: String,
    pub name: String,
    pub account_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostJournalLineRequest {
    pub account_code: String,
    pub debit: i64,
    pub credit: i64,
    pub memo: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostJournalRequest {
    pub entry_date: Option<String>,
    pub description: Option<String>,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub lines: Vec<PostJournalLineRequest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReverseJournalRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsListResponse {
    pub accounts: Vec<Account>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalsListResponse {
    pub journals: Vec<JournalEntryWithLines>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAccountResponse {
    pub status: String,
    pub code: String,
}

pub struct AccountingService {
    _pool: SqlitePool,
    repo: Arc<dyn AccountingRepository>,
}

impl AccountingService {
    pub fn new(pool: SqlitePool, repo: Arc<dyn AccountingRepository>) -> Self {
        Self { _pool: pool, repo }
    }

    pub fn new_with_pool(pool: SqlitePool) -> Self {
        let repo = Arc::new(SqlxAccountingRepository::new(pool.clone()));
        Self::new(pool, repo)
    }

    pub async fn list_accounts(&self, ctx: &TenantContext) -> Result<AccountsListResponse, AppError> {
        let accounts = self.repo.list_accounts(ctx).await.map_err(AppError::from)?;
        let count = accounts.len();
        Ok(AccountsListResponse { accounts, count })
    }

    pub async fn create_account(
        &self,
        ctx: &TenantContext,
        req: CreateAccountRequest,
    ) -> Result<Account, AppError> {
        if !ctx.role.can_post_ledger() {
            return Err(AppError::Forbidden(
                "Current role is not permitted to manage chart of accounts".to_string(),
                "FORBIDDEN",
            ));
        }

        let code = req.code.trim().to_string();
        let name = req.name.trim().to_string();

        if code.is_empty() || name.is_empty() {
            return Err(AppError::BadRequest(
                "Account code and name are required".to_string(),
                "INVALID_ACCOUNT",
            ));
        }

        // Check for existing account code within tenant
        if self
            .repo
            .get_account_by_code(ctx, &code)
            .await
            .map_err(AppError::from)?
            .is_some()
        {
            return Err(AppError::Conflict(
                format!("Account code '{}' already exists", code),
                "ACCOUNT_ALREADY_EXISTS",
            ));
        }

        let account_type = req
            .account_type
            .as_deref()
            .and_then(AccountType::from_str_case_insensitive)
            .unwrap_or(AccountType::Asset);

        let now_iso = Utc::now().to_rfc3339();
        let account = Account {
            id: Uuid::new_v4().to_string(),
            tenant_id: ctx.tenant_id_str(),
            code,
            name,
            account_type,
            is_system: false,
            created_at: now_iso.clone(),
            updated_at: Some(now_iso),
        };

        self.repo
            .create_account(ctx, &account)
            .await
            .map_err(AppError::from)?;

        Ok(account)
    }

    pub async fn delete_account(
        &self,
        ctx: &TenantContext,
        code: &str,
    ) -> Result<DeleteAccountResponse, AppError> {
        if !ctx.role.can_post_ledger() {
            return Err(AppError::Forbidden(
                "Current role is not permitted to manage chart of accounts".to_string(),
                "FORBIDDEN",
            ));
        }

        let trimmed_code = code.trim();

        // 1. System account guard
        if Account::is_system_code(trimmed_code) {
            return Err(AppError::Forbidden(
                format!("System account '{}' is protected from deletion", trimmed_code),
                "SYSTEM_ACCOUNT_PROTECTED",
            ));
        }

        // 2. Query account existence
        let existing = self
            .repo
            .get_account_by_code(ctx, trimmed_code)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Account not found".to_string(), "NOT_FOUND")
            })?;

        if existing.is_system {
            return Err(AppError::Forbidden(
                format!("System account '{}' is protected from deletion", trimmed_code),
                "SYSTEM_ACCOUNT_PROTECTED",
            ));
        }

        // 3. Delete non-system account
        let deleted = self
            .repo
            .delete_account(ctx, trimmed_code)
            .await
            .map_err(AppError::from)?;

        if !deleted {
            return Err(AppError::NotFound(
                "Account not found".to_string(),
                "NOT_FOUND",
            ));
        }

        Ok(DeleteAccountResponse {
            status: "deleted".to_string(),
            code: trimmed_code.to_string(),
        })
    }

    /// Validates, sequences, and inserts a journal entry within an external transaction.
    /// Does NOT commit the transaction, guaranteeing atomicity with the caller's mutations.
    pub async fn post_journal_command_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        cmd: PostJournalEntryCommand,
    ) -> Result<JournalEntryWithLines, AppError> {
        // 1. Multi-Tenant Boundary Invariant
        if cmd.tenant_id != ctx.tenant_id {
            return Err(AppError::Forbidden(
                "Tenant ID mismatch in journal command".to_string(),
                "FORBIDDEN",
            ));
        }

        // 2. RBAC Enforcement Invariant
        if !ctx.role.can_post_ledger() {
            return Err(AppError::Forbidden(
                format!("Role {:?} is not authorized to post journals", ctx.role),
                "FORBIDDEN",
            ));
        }

        // 3. Validate lines presence
        if cmd.lines.is_empty() {
            return Err(AppError::BadRequest(
                "Journal lines cannot be empty".to_string(),
                "INVALID_JOURNAL_LINES",
            ));
        }

        // 4. Validate at least 2 lines
        if cmd.lines.len() < 2 {
            return Err(AppError::UnprocessableEntity(
                "Single line journal is unbalanced".to_string(),
                "UNBALANCED_JOURNAL_ENTRY",
            ));
        }

        // 5. Validate non-negative and mutually exclusive amounts
        for line in &cmd.lines {
            let debit_val = line.debit.as_i64();
            let credit_val = line.credit.as_i64();

            if debit_val < 0 || credit_val < 0 {
                return Err(AppError::BadRequest(
                    "Journal amounts must be non-negative integers".to_string(),
                    "INVALID_AMOUNT",
                ));
            }

            if debit_val > 0 && credit_val > 0 {
                return Err(AppError::BadRequest(
                    "A journal line cannot have both debit and credit".to_string(),
                    "INVALID_JOURNAL_LINES",
                ));
            }
        }

        // 6. Account Code Existence Validation inside transaction
        let mut validated_accounts = std::collections::HashSet::new();
        for line in &cmd.lines {
            if validated_accounts.insert(&line.account_code) {
                let account = SqlxAccountingRepository::get_account_by_code_tx(
                    tx,
                    &ctx.tenant_id_str(),
                    &line.account_code,
                )
                .await
                .map_err(AppError::from)?;

                if account.is_none() {
                    return Err(AppError::UnprocessableEntity(
                        format!("Account code '{}' not found in chart of accounts", line.account_code),
                        "ACCOUNT_NOT_FOUND",
                    ));
                }
            }
        }

        // 7. Validate double-entry sum(debit) == sum(credit)
        let total_debit: i64 = cmd.lines.iter().map(|l| l.debit.as_i64()).sum();
        let total_credit: i64 = cmd.lines.iter().map(|l| l.credit.as_i64()).sum();

        if total_debit != total_credit {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "SUM(debit)={} must equal SUM(credit)={}",
                    total_debit, total_credit
                ),
                "UNBALANCED_JOURNAL_ENTRY",
            ));
        }

        // 8. Validate strictly positive total amount
        if total_debit <= 0 {
            return Err(AppError::UnprocessableEntity(
                "Journal total amount must be strictly positive".to_string(),
                "UNBALANCED_JOURNAL_ENTRY",
            ));
        }

        // 9. Source Type Validation
        const VALID_SOURCE_TYPES: &[&str] = &["MANUAL", "INVOICE", "PAYMENT", "REVERSAL", "SYSTEM"];
        if !VALID_SOURCE_TYPES.contains(&cmd.source_type.as_str()) {
            return Err(AppError::BadRequest(
                format!("Invalid source type '{}'. Allowed: MANUAL, INVOICE, PAYMENT, REVERSAL, SYSTEM", cmd.source_type),
                "INVALID_SOURCE_TYPE",
            ));
        }

        // 10. Generate sequential gapless entry number inside transaction: JRN-YYYY-XXXXXX
        let count = SqlxAccountingRepository::get_journal_count_tx(tx, &ctx.tenant_id_str())
            .await
            .map_err(AppError::from)?;
        let now = Utc::now();
        let entry_date_str = cmd.entry_date.to_rfc3339();
        let year = cmd.entry_date.year();
        let entry_number = format!("JRN-{}-{:06}", year, count + 1);

        let journal_id = Uuid::new_v4().to_string();
        let now_iso = now.to_rfc3339();

        let entry = JournalEntry {
            id: journal_id.clone(),
            tenant_id: ctx.tenant_id_str(),
            entry_number,
            entry_date: entry_date_str,
            description: cmd.description,
            source_type: cmd.source_type,
            source_id: cmd.source_id.map(|u| u.to_string()),
            status: "POSTED".to_string(),
            is_reversed: 0,
            reversal_entry_id: None,
            created_at: now_iso.clone(),
            updated_at: Some(now_iso),
        };

        let lines: Vec<JournalLine> = cmd
            .lines
            .into_iter()
            .map(|l| JournalLine {
                id: Uuid::new_v4().to_string(),
                journal_id: journal_id.clone(),
                tenant_id: ctx.tenant_id_str(),
                account_code: l.account_code,
                debit: l.debit,
                credit: l.credit,
                memo: l.memo,
            })
            .collect();

        let created = SqlxAccountingRepository::post_journal_tx(tx, ctx, &entry, &lines)
            .await
            .map_err(AppError::from)?;

        if entry.source_type == "MANUAL" {
            let outbox_draft = crate::domain::outbox::OutboxEventDraft::journal_posted(
                ctx.tenant_id,
                &entry.id,
                &entry.entry_number,
                total_debit,
            );
            crate::repository::outbox_repo::SqlxOutboxRepository::insert_tx_static(tx, &outbox_draft)
                .await
                .map_err(AppError::from)?;
        }

        Ok(created)
    }

    pub async fn post_journal_command(
        &self,
        ctx: &TenantContext,
        cmd: PostJournalEntryCommand,
    ) -> Result<JournalEntryWithLines, AppError> {
        let mut tx = self._pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;
        let created = self.post_journal_command_tx(&mut tx, ctx, cmd).await?;
        tx.commit().await.map_err(AppError::from)?;
        Ok(created)
    }

    pub async fn post_journal(
        &self,
        ctx: &TenantContext,
        req: PostJournalRequest,
    ) -> Result<JournalEntryWithLines, AppError> {
        let now = Utc::now();
        let entry_date = match req.entry_date {
            Some(ref d) => d.parse::<DateTime<Utc>>().unwrap_or(now),
            None => now,
        };
        let source_id = req.source_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());

        let cmd = PostJournalEntryCommand {
            tenant_id: ctx.tenant_id,
            entry_date,
            description: req
                .description
                .unwrap_or_else(|| "Manual Journal Entry".to_string()),
            source_type: req.source_type.unwrap_or_else(|| "MANUAL".to_string()),
            source_id,
            lines: req
                .lines
                .into_iter()
                .map(|l| PostJournalLineCommand {
                    account_code: l.account_code,
                    debit: Rupiah::new(l.debit),
                    credit: Rupiah::new(l.credit),
                    memo: l.memo,
                })
                .collect(),
        };

        self.post_journal_command(ctx, cmd).await
    }

    pub async fn get_journal(
        &self,
        ctx: &TenantContext,
        id: &str,
    ) -> Result<JournalEntryWithLines, AppError> {
        self.repo
            .get_journal_by_id(ctx, id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Journal entry not found".to_string(), "NOT_FOUND")
            })
    }

    pub async fn list_journals(
        &self,
        ctx: &TenantContext,
    ) -> Result<JournalsListResponse, AppError> {
        let journals = self
            .repo
            .list_journals(ctx)
            .await
            .map_err(AppError::from)?;
        let count = journals.len();
        Ok(JournalsListResponse { journals, count })
    }

    pub async fn reverse_journal(
        &self,
        ctx: &TenantContext,
        id: &str,
        req: ReverseJournalRequest,
    ) -> Result<JournalEntryWithLines, AppError> {
        // RBAC INVARIANT: Only Owner and Accountant can reverse journals
        if !ctx.role.can_reverse_journal() {
            return Err(AppError::Forbidden(
                format!("Role {:?} is not authorized to reverse journals", ctx.role),
                "FORBIDDEN",
            ));
        }

        // 1. Query original journal
        let orig = self
            .repo
            .get_journal_by_id(ctx, id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::NotFound("Journal entry not found".to_string(), "NOT_FOUND")
            })?;

        // 2. Check already reversed
        if orig.is_reversed == 1 {
            return Err(AppError::Conflict(
                "Journal has already been reversed".to_string(),
                "ALREADY_REVERSED",
            ));
        }

        // 3. Generate sequential reversal entry template
        let now = Utc::now();
        let rev_id = Uuid::new_v4().to_string();
        let reason_text = req.reason.unwrap_or_else(|| "Correction".to_string());
        let rev_desc = format!("Reversal of {}: {}", orig.entry_number, reason_text);
        let now_iso = now.to_rfc3339();

        let reversal_entry = JournalEntry {
            id: rev_id.clone(),
            tenant_id: ctx.tenant_id_str(),
            entry_number: String::new(),
            entry_date: now_iso.clone(),
            description: rev_desc,
            source_type: "REVERSAL".to_string(),
            source_id: Some(orig.id.clone()),
            status: "POSTED".to_string(),
            is_reversed: 0,
            reversal_entry_id: None,
            created_at: now_iso.clone(),
            updated_at: Some(now_iso),
        };

        // 4. Invert debits and credits
        let reversal_lines: Vec<JournalLine> = orig
            .lines
            .into_iter()
            .map(|ol| {
                let memo = format!(
                    "Reversal: {}",
                    ol.memo.as_deref().unwrap_or("")
                )
                .trim()
                .to_string();

                JournalLine {
                    id: Uuid::new_v4().to_string(),
                    journal_id: rev_id.clone(),
                    tenant_id: ctx.tenant_id_str(),
                    account_code: ol.account_code,
                    debit: Rupiah::new(ol.credit),  // swapped
                    credit: Rupiah::new(ol.debit),  // swapped
                    memo: if memo == "Reversal:" || memo.is_empty() {
                        None
                    } else {
                        Some(memo)
                    },
                }
            })
            .collect();

        let reversal_res = self
            .repo
            .reverse_journal(ctx, id, &reversal_entry, &reversal_lines)
            .await;

        let created_reversal = match reversal_res {
            Ok(rev) => rev,
            Err(DbError::NotFound) => {
                return Err(AppError::NotFound("Journal entry not found".to_string(), "NOT_FOUND"));
            }
            Err(DbError::Validation(ref msg)) if msg.contains("already been reversed") => {
                return Err(AppError::Conflict("Journal has already been reversed".to_string(), "ALREADY_REVERSED"));
            }
            Err(DbError::UniqueViolation { .. }) => {
                return Err(AppError::Conflict("Journal has already been reversed".to_string(), "ALREADY_REVERSED"));
            }
            Err(other) => {
                tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                if let Ok(Some(current)) = self.repo.get_journal_by_id(ctx, id).await {
                    if current.is_reversed == 1 {
                        return Err(AppError::Conflict(
                            "Journal has already been reversed".to_string(),
                            "ALREADY_REVERSED",
                        ));
                    }
                }
                return Err(AppError::from(other));
            }
        };

        Ok(created_reversal)
    }

    pub async fn get_trial_balance(&self, ctx: &TenantContext) -> Result<TrialBalanceDto, AppError> {
        self.repo
            .get_trial_balance(ctx)
            .await
            .map_err(AppError::from)
    }

    pub fn calculate_tax(
        &self,
        amount: i64,
        tax_type: &str,
        is_inclusive: bool,
    ) -> Result<TaxCalculationResult, AppError> {
        calculate_tax_integer(amount, tax_type, is_inclusive)
    }
}
