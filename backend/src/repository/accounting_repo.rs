use async_trait::async_trait;
use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::domain::accounting::{
    Account, AccountType, JournalEntry, JournalEntryWithLines, JournalLine, JournalLineDto,
};
use crate::domain::tenant::TenantContext;
use crate::repository::error::DbError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrialBalanceAccountDto {
    pub code: String,
    pub name: String,
    pub account_type: String,
    pub debit: i64,
    pub credit: i64,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrialBalanceDto {
    pub accounts: Vec<TrialBalanceAccountDto>,
    pub total_debit: i64,
    pub total_credit: i64,
    pub net_balance: i64,
    pub is_balanced: bool,
}

#[async_trait]
pub trait ChartOfAccountsRepository: Send + Sync {
    async fn list_accounts(&self, ctx: &TenantContext) -> Result<Vec<Account>, DbError>;
    async fn get_account_by_code(
        &self,
        ctx: &TenantContext,
        code: &str,
    ) -> Result<Option<Account>, DbError>;
    async fn create_account(&self, ctx: &TenantContext, account: &Account) -> Result<(), DbError>;
    async fn delete_account(&self, ctx: &TenantContext, code: &str) -> Result<bool, DbError>;
}

#[async_trait]
pub trait JournalRepository: Send + Sync {
    async fn get_journal_count(&self, ctx: &TenantContext) -> Result<i64, DbError>;
    async fn post_journal(
        &self,
        ctx: &TenantContext,
        entry: &JournalEntry,
        lines: &[JournalLine],
    ) -> Result<JournalEntryWithLines, DbError>;
    async fn list_journals(&self, ctx: &TenantContext)
        -> Result<Vec<JournalEntryWithLines>, DbError>;
    async fn get_journal_by_id(
        &self,
        ctx: &TenantContext,
        id: &str,
    ) -> Result<Option<JournalEntryWithLines>, DbError>;
    async fn reverse_journal(
        &self,
        ctx: &TenantContext,
        orig_id: &str,
        reversal_entry: &JournalEntry,
        reversal_lines: &[JournalLine],
    ) -> Result<JournalEntryWithLines, DbError>;
    async fn get_trial_balance(&self, ctx: &TenantContext) -> Result<TrialBalanceDto, DbError>;
}

pub trait AccountingRepository: ChartOfAccountsRepository + JournalRepository + Send + Sync {}
impl<T: ChartOfAccountsRepository + JournalRepository + Send + Sync> AccountingRepository for T {}

#[derive(Clone)]
pub struct SqlxAccountingRepository {
    pool: SqlitePool,
}

impl SqlxAccountingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Seeds the canonical 8 Chart of Accounts and default tax rules inside an existing transaction.
    /// Used when workspaces are created or provisioned.
    pub async fn seed_default_accounts_tx(
        tx: &mut Transaction<'_, Sqlite>,
        tenant_id: &str,
    ) -> Result<(), DbError> {
        let now_iso = Utc::now().to_rfc3339();

        for (code, name, acc_type) in Account::canonical_system_accounts() {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO chart_of_accounts 
                    (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)
                "#,
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(code)
            .bind(name)
            .bind(acc_type.as_str())
            .bind(&now_iso)
            .bind(&now_iso)
            .execute(&mut **tx)
            .await
            .map_err(DbError::from_sqlx)?;
        }

        let default_tax_rules = [
            ("PPN_11_EXCL", "PPN 11% Eksklusif", 1100, 0),
            ("PPN_11_INCL", "PPN 11% Inklusif", 1100, 1),
            ("PPN_12_EXCL", "PPN 12% Eksklusif", 1200, 0),
            ("PPN_12_INCL", "PPN 12% Inklusif", 1200, 1),
            ("UMKM_05", "PPh Final UMKM 0.5%", 50, 0),
            ("EXEMPT", "Bebas Pajak (0%)", 0, 0),
        ];

        for (tax_type, name, rate_bps, is_incl) in default_tax_rules {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO tax_rules
                    (id, tenant_id, tax_type, name, rate_basis_points, is_inclusive, is_system, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)
                "#,
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(tax_type)
            .bind(name)
            .bind(rate_bps)
            .bind(is_incl)
            .bind(&now_iso)
            .bind(&now_iso)
            .execute(&mut **tx)
            .await
            .map_err(DbError::from_sqlx)?;
        }

        Ok(())
    }
}

#[async_trait]
impl ChartOfAccountsRepository for SqlxAccountingRepository {
    async fn list_accounts(&self, ctx: &TenantContext) -> Result<Vec<Account>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, code, name, account_type, is_system, created_at, updated_at
            FROM chart_of_accounts
            WHERE tenant_id = ?1
            ORDER BY code ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let mut accounts = Vec::with_capacity(rows.len());
        for row in rows {
            let acc_type_str: String = row.get("account_type");
            let account_type = AccountType::from_str_case_insensitive(&acc_type_str)
                .unwrap_or(AccountType::Asset);
            let is_system_int: i64 = row.get("is_system");

            accounts.push(Account {
                id: row.get("id"),
                tenant_id: row.get("tenant_id"),
                code: row.get("code"),
                name: row.get("name"),
                account_type,
                is_system: is_system_int == 1,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(accounts)
    }

    async fn get_account_by_code(
        &self,
        ctx: &TenantContext,
        code: &str,
    ) -> Result<Option<Account>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, code, name, account_type, is_system, created_at, updated_at
            FROM chart_of_accounts
            WHERE tenant_id = ?1 AND code = ?2
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        match row {
            Some(row) => {
                let acc_type_str: String = row.get("account_type");
                let account_type = AccountType::from_str_case_insensitive(&acc_type_str)
                    .unwrap_or(AccountType::Asset);
                let is_system_int: i64 = row.get("is_system");

                Ok(Some(Account {
                    id: row.get("id"),
                    tenant_id: row.get("tenant_id"),
                    code: row.get("code"),
                    name: row.get("name"),
                    account_type,
                    is_system: is_system_int == 1,
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                }))
            }
            None => Ok(None),
        }
    }

    async fn create_account(&self, ctx: &TenantContext, account: &Account) -> Result<(), DbError> {
        let is_sys_int: i64 = if account.is_system { 1 } else { 0 };
        let now = Utc::now().to_rfc3339();
        let updated_at = account.updated_at.as_deref().unwrap_or(&now);

        sqlx::query(
            r#"
            INSERT INTO chart_of_accounts 
                (id, tenant_id, code, name, account_type, is_system, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&account.id)
        .bind(ctx.tenant_id_str())
        .bind(&account.code)
        .bind(&account.name)
        .bind(account.account_type.as_str())
        .bind(is_sys_int)
        .bind(&account.created_at)
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn delete_account(&self, ctx: &TenantContext, code: &str) -> Result<bool, DbError> {
        let res = sqlx::query(
            r#"
            DELETE FROM chart_of_accounts
            WHERE tenant_id = ?1 AND code = ?2 AND is_system = 0
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(code)
        .execute(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(res.rows_affected() > 0)
    }
}

impl SqlxAccountingRepository {
    /// Queries journal entry count inside an active transaction.
    pub async fn get_journal_count_tx(
        tx: &mut Transaction<'_, Sqlite>,
        tenant_id: &str,
        year: i32,
    ) -> Result<i64, DbError> {
        let prefix = format!("JRN-{}-", year);
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?1 AND entry_number LIKE ?2",
        )
        .bind(tenant_id)
        .bind(format!("{}%", prefix))
        .fetch_one(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(count.0)
    }

    /// Queries an account by code inside an active transaction.
    pub async fn get_account_by_code_tx(
        tx: &mut Transaction<'_, Sqlite>,
        tenant_id: &str,
        code: &str,
    ) -> Result<Option<Account>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, code, name, account_type, is_system, created_at, updated_at
            FROM chart_of_accounts
            WHERE tenant_id = ?1 AND code = ?2
            "#,
        )
        .bind(tenant_id)
        .bind(code)
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        match row {
            Some(row) => {
                let acc_type_str: String = row.get("account_type");
                let account_type = AccountType::from_str_case_insensitive(&acc_type_str)
                    .unwrap_or(AccountType::Asset);
                let is_system_int: i64 = row.get("is_system");

                Ok(Some(Account {
                    id: row.get("id"),
                    tenant_id: row.get("tenant_id"),
                    code: row.get("code"),
                    name: row.get("name"),
                    account_type,
                    is_system: is_system_int == 1,
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                }))
            }
            None => Ok(None),
        }
    }

    /// Posts journal entry and its lines inside an active transaction without committing.
    pub async fn post_journal_tx(
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        entry: &JournalEntry,
        lines: &[JournalLine],
    ) -> Result<JournalEntryWithLines, DbError> {
        let now_iso = Utc::now().to_rfc3339();
        let updated_at = entry.updated_at.as_deref().unwrap_or(&now_iso);

        sqlx::query(
            r#"
            INSERT INTO journal_entries 
                (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, reversal_entry_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        )
        .bind(&entry.id)
        .bind(ctx.tenant_id_str())
        .bind(&entry.entry_number)
        .bind(&entry.entry_date)
        .bind(&entry.description)
        .bind(&entry.source_type)
        .bind(&entry.source_id)
        .bind(&entry.status)
        .bind(entry.is_reversed)
        .bind(&entry.reversal_entry_id)
        .bind(&entry.created_at)
        .bind(updated_at)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        let mut saved_lines = Vec::with_capacity(lines.len());
        let mut total_debit = 0i64;
        let mut total_credit = 0i64;

        for line in lines {
            let debit_val = line.debit.as_i64();
            let credit_val = line.credit.as_i64();
            total_debit += debit_val;
            total_credit += credit_val;

            sqlx::query(
                r#"
                INSERT INTO journal_lines 
                    (id, journal_id, tenant_id, account_code, debit, credit, memo, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )
            .bind(&line.id)
            .bind(&entry.id)
            .bind(ctx.tenant_id_str())
            .bind(&line.account_code)
            .bind(debit_val)
            .bind(credit_val)
            .bind(&line.memo)
            .bind(&now_iso)
            .execute(&mut **tx)
            .await
            .map_err(DbError::from_sqlx)?;

            saved_lines.push(JournalLineDto {
                id: line.id.clone(),
                account_code: line.account_code.clone(),
                debit: debit_val,
                credit: credit_val,
                memo: line.memo.clone(),
            });
        }

        Ok(JournalEntryWithLines {
            id: entry.id.clone(),
            tenant_id: ctx.tenant_id_str(),
            entry_number: entry.entry_number.clone(),
            entry_date: entry.entry_date.clone(),
            description: entry.description.clone(),
            source_type: entry.source_type.clone(),
            source_id: entry.source_id.clone(),
            status: entry.status.clone(),
            is_reversed: entry.is_reversed,
            reversal_entry_id: entry.reversal_entry_id.clone(),
            total_debit,
            total_credit,
            lines: saved_lines,
            created_at: entry.created_at.clone(),
        })
    }
}

#[async_trait]
impl JournalRepository for SqlxAccountingRepository {
    async fn get_journal_count(&self, ctx: &TenantContext) -> Result<i64, DbError> {
        let year = Utc::now().year();
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?1 AND entry_number LIKE ?2",
        )
        .bind(ctx.tenant_id_str())
        .bind(format!("JRN-{}-", year) + "%")
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(count.0)
    }

    async fn post_journal(
        &self,
        ctx: &TenantContext,
        entry: &JournalEntry,
        lines: &[JournalLine],
    ) -> Result<JournalEntryWithLines, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from_sqlx)?;
        let res = Self::post_journal_tx(&mut tx, ctx, entry, lines).await?;
        tx.commit().await.map_err(DbError::from_sqlx)?;
        Ok(res)
    }

    async fn list_journals(
        &self,
        ctx: &TenantContext,
    ) -> Result<Vec<JournalEntryWithLines>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, reversal_entry_id, created_at
            FROM journal_entries
            WHERE tenant_id = ?1
            ORDER BY entry_date DESC, entry_number DESC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch all lines for all tenant journals in a single query (O(N+M) instead of O(N*M) N+1 query)
        let line_rows = sqlx::query(
            r#"
            SELECT id, journal_id, account_code, debit, credit, memo
            FROM journal_lines
            WHERE tenant_id = ?1
            ORDER BY id ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        use std::collections::HashMap;
        let mut lines_by_journal: HashMap<String, Vec<JournalLineDto>> = HashMap::new();
        for lr in line_rows {
            let journal_id: String = lr.get("journal_id");
            let debit: i64 = lr.get("debit");
            let credit: i64 = lr.get("credit");

            lines_by_journal
                .entry(journal_id)
                .or_default()
                .push(JournalLineDto {
                    id: lr.get("id"),
                    account_code: lr.get("account_code"),
                    debit,
                    credit,
                    memo: lr.get("memo"),
                });
        }

        let mut journals = Vec::with_capacity(rows.len());
        for row in rows {
            let j_id: String = row.get("id");
            let lines = lines_by_journal.remove(&j_id).unwrap_or_default();

            let mut total_debit = 0i64;
            let mut total_credit = 0i64;
            for l in &lines {
                total_debit += l.debit;
                total_credit += l.credit;
            }

            journals.push(JournalEntryWithLines {
                id: j_id,
                tenant_id: row.get("tenant_id"),
                entry_number: row.get("entry_number"),
                entry_date: row.get("entry_date"),
                description: row.get("description"),
                source_type: row.get("source_type"),
                source_id: row.get("source_id"),
                status: row.get("status"),
                is_reversed: row.get("is_reversed"),
                reversal_entry_id: row.get("reversal_entry_id"),
                total_debit,
                total_credit,
                lines,
                created_at: row.get("created_at"),
            });
        }

        Ok(journals)
    }

    async fn get_journal_by_id(
        &self,
        ctx: &TenantContext,
        id: &str,
    ) -> Result<Option<JournalEntryWithLines>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, reversal_entry_id, created_at
            FROM journal_entries
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let line_rows = sqlx::query(
            r#"
            SELECT id, account_code, debit, credit, memo
            FROM journal_lines
            WHERE journal_id = ?1 AND tenant_id = ?2
            ORDER BY id ASC
            "#,
        )
        .bind(id)
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let mut lines = Vec::with_capacity(line_rows.len());
        let mut total_debit = 0i64;
        let mut total_credit = 0i64;

        for lr in line_rows {
            let debit: i64 = lr.get("debit");
            let credit: i64 = lr.get("credit");
            total_debit += debit;
            total_credit += credit;

            lines.push(JournalLineDto {
                id: lr.get("id"),
                account_code: lr.get("account_code"),
                debit,
                credit,
                memo: lr.get("memo"),
            });
        }

        Ok(Some(JournalEntryWithLines {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            entry_number: row.get("entry_number"),
            entry_date: row.get("entry_date"),
            description: row.get("description"),
            source_type: row.get("source_type"),
            source_id: row.get("source_id"),
            status: row.get("status"),
            is_reversed: row.get("is_reversed"),
            reversal_entry_id: row.get("reversal_entry_id"),
            total_debit,
            total_credit,
            lines,
            created_at: row.get("created_at"),
        }))
    }

    async fn reverse_journal(
        &self,
        ctx: &TenantContext,
        orig_id: &str,
        reversal_entry: &JournalEntry,
        reversal_lines: &[JournalLine],
    ) -> Result<JournalEntryWithLines, DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from_sqlx)?;

        let now_iso = Utc::now().to_rfc3339();

        // Atomic compare-and-swap guard: atomically claim the right to reverse
        let update_result = sqlx::query(
            r#"
            UPDATE journal_entries
            SET is_reversed = 1, updated_at = ?1
            WHERE id = ?2 AND tenant_id = ?3 AND is_reversed = 0
            "#,
        )
        .bind(&now_iso)
        .bind(orig_id)
        .bind(ctx.tenant_id_str())
        .execute(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        if update_result.rows_affected() == 0 {
            // Determine whether entry does not exist or was already reversed
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM journal_entries WHERE id = ?1 AND tenant_id = ?2",
            )
            .bind(orig_id)
            .bind(ctx.tenant_id_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(DbError::from_sqlx)?;

            if exists == 0 {
                return Err(DbError::NotFound);
            } else {
                return Err(DbError::Validation("Journal has already been reversed".to_string()));
            }
        }

        // Generate sequential reversal number safely inside the write transaction
        let now = Utc::now();
        let rev_year = now.year();
        let count_row = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?1 AND entry_number LIKE ?2",
        )
        .bind(ctx.tenant_id_str())
        .bind(format!("REV-{}-", rev_year) + "%")
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        let rev_num = format!("REV-{}-{:06}", rev_year, count_row + 1);

        // Insert reversal header
        sqlx::query(
            r#"
            INSERT INTO journal_entries 
                (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10)
            "#,
        )
        .bind(&reversal_entry.id)
        .bind(ctx.tenant_id_str())
        .bind(&rev_num)
        .bind(&reversal_entry.entry_date)
        .bind(&reversal_entry.description)
        .bind("REVERSAL")
        .bind(orig_id)
        .bind("POSTED")
        .bind(&reversal_entry.created_at)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        // Insert swapped reversal lines
        let mut saved_lines = Vec::with_capacity(reversal_lines.len());
        let mut total_debit = 0i64;
        let mut total_credit = 0i64;

        for line in reversal_lines {
            let debit_val = line.debit.as_i64();
            let credit_val = line.credit.as_i64();
            total_debit += debit_val;
            total_credit += credit_val;

            sqlx::query(
                r#"
                INSERT INTO journal_lines 
                    (id, journal_id, tenant_id, account_code, debit, credit, memo, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )
            .bind(&line.id)
            .bind(&reversal_entry.id)
            .bind(ctx.tenant_id_str())
            .bind(&line.account_code)
            .bind(debit_val)
            .bind(credit_val)
            .bind(&line.memo)
            .bind(&now_iso)
            .execute(&mut *tx)
            .await
            .map_err(DbError::from_sqlx)?;

            saved_lines.push(JournalLineDto {
                id: line.id.clone(),
                account_code: line.account_code.clone(),
                debit: debit_val,
                credit: credit_val,
                memo: line.memo.clone(),
            });
        }

        // Set reversal_entry_id on original journal
        sqlx::query(
            r#"
            UPDATE journal_entries
            SET reversal_entry_id = ?1
            WHERE id = ?2 AND tenant_id = ?3
            "#,
        )
        .bind(&reversal_entry.id)
        .bind(orig_id)
        .bind(ctx.tenant_id_str())
        .execute(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        tx.commit().await.map_err(DbError::from_sqlx)?;

        Ok(JournalEntryWithLines {
            id: reversal_entry.id.clone(),
            tenant_id: ctx.tenant_id_str(),
            entry_number: rev_num,
            entry_date: reversal_entry.entry_date.clone(),
            description: reversal_entry.description.clone(),
            source_type: "REVERSAL".to_string(),
            source_id: Some(orig_id.to_string()),
            status: "POSTED".to_string(),
            is_reversed: 0,
            reversal_entry_id: None,
            total_debit,
            total_credit,
            lines: saved_lines,
            created_at: reversal_entry.created_at.clone(),
        })
    }

    async fn get_trial_balance(&self, ctx: &TenantContext) -> Result<TrialBalanceDto, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT 
                coa.code,
                coa.name,
                coa.account_type,
                COALESCE(SUM(jl.debit), 0) AS total_debit,
                COALESCE(SUM(jl.credit), 0) AS total_credit
            FROM chart_of_accounts coa
            LEFT JOIN (
                SELECT jl.tenant_id, jl.account_code, jl.debit, jl.credit
                FROM journal_lines jl
                JOIN journal_entries je ON je.id = jl.journal_id AND je.tenant_id = jl.tenant_id
                WHERE je.status = 'POSTED'
            ) jl ON jl.tenant_id = coa.tenant_id AND jl.account_code = coa.code
            WHERE coa.tenant_id = ?1
            GROUP BY coa.code, coa.name, coa.account_type
            ORDER BY coa.code ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let mut accounts = Vec::with_capacity(rows.len());
        let mut sum_debit = 0i64;
        let mut sum_credit = 0i64;

        for r in rows {
            let debit: i64 = r.get("total_debit");
            let credit: i64 = r.get("total_credit");
            sum_debit += debit;
            sum_credit += credit;
            let balance = debit - credit;

            accounts.push(TrialBalanceAccountDto {
                code: r.get("code"),
                name: r.get("name"),
                account_type: r.get("account_type"),
                debit,
                credit,
                balance,
            });
        }

        let net_balance = sum_debit - sum_credit;
        Ok(TrialBalanceDto {
            accounts,
            total_debit: sum_debit,
            total_credit: sum_credit,
            net_balance,
            is_balanced: net_balance == 0,
        })
    }
}
