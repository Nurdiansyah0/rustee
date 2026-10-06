//! Commercial Invoice Service (Features 16, 17, 18, 19)
//! Manages invoice draft lifecycle, sequential numbering, frozen snapshots, and GL postings.

use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::accounting::{
    calculate_tax_integer, PostJournalEntryCommand, PostJournalLineCommand,
};
use crate::domain::money::Rupiah;
use crate::domain::tenant::{Role, TenantContext};
use crate::error::AppError;
use crate::service::accounting_service::{AccountingService, ReverseJournalRequest};

// --- Request & Response DTOs ---

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvoiceItemRequest {
    pub description: String,
    pub quantity: i64,
    pub unit_price: i64,
    #[serde(default)]
    pub discount: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvoiceRequest {
    pub customer_name: Option<String>,
    pub customer_address: Option<String>,
    pub customer_email: Option<String>,
    pub due_date: Option<String>,
    pub currency: Option<String>,
    pub tax_type: Option<String>,
    pub authorized_by: Option<String>,
    pub items: Vec<CreateInvoiceItemRequest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateInvoiceRequest {
    pub customer_name: Option<String>,
    pub customer_address: Option<String>,
    pub customer_email: Option<String>,
    pub due_date: Option<String>,
    pub authorized_by: Option<String>,
    pub items: Option<Vec<CreateInvoiceItemRequest>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VoidInvoiceRequest {
    #[serde(default = "default_void_reason")]
    pub reason: String,
}

fn default_void_reason() -> String {
    "Voided".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceItemDto {
    pub id: String,
    pub description: String,
    pub quantity: i64,
    pub unit_price: i64,
    pub discount: i64,
    pub tax_amount: i64,
    pub line_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceSnapshotDto {
    pub customer_name: String,
    pub customer_address: Option<String>,
    pub customer_email: Option<String>,
    pub items: Vec<InvoiceItemDto>,
    pub tax_type: String,
    pub subtotal: i64,
    pub tax_amount: i64,
    pub total_amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_by: Option<String>,
    pub issued_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceResponse {
    pub id: String,
    pub tenant_id: String,
    pub invoice_number: Option<String>,
    pub customer_name: String,
    pub customer_address: Option<String>,
    pub customer_email: Option<String>,
    pub issue_date: Option<String>,
    pub due_date: String,
    pub currency: String,
    pub tax_type: String,
    pub subtotal: i64,
    pub discount: i64,
    pub tax_amount: i64,
    pub total_amount: i64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<InvoiceItemDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<InvoiceSnapshotDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoicesListResponse {
    pub invoices: Vec<InvoiceResponse>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivableResponse {
    pub id: String,
    pub tenant_id: String,
    pub invoice_id: String,
    pub invoice_number: Option<String>,
    pub customer_name: Option<String>,
    pub total_amount: i64,
    pub allocated_amount: i64,
    pub outstanding_amount: i64,
    pub due_date: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivablesListResponse {
    pub receivables: Vec<ReceivableResponse>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgingReportResponse {
    pub current_0_30: i64,
    pub overdue_31_60: i64,
    pub overdue_61_90: i64,
    pub overdue_90_plus: i64,
    pub total_outstanding: i64,
}

// --- Invoice Service Implementation ---

pub struct InvoiceService {
    pool: SqlitePool,
    accounting_service: Arc<AccountingService>,
}

impl InvoiceService {
    pub fn new(pool: SqlitePool, accounting_service: Arc<AccountingService>) -> Self {
        Self {
            pool,
            accounting_service,
        }
    }

    /// Creates draft invoice with line items and integer tax calculation
    pub async fn create_invoice(
        &self,
        ctx: &TenantContext,
        req: CreateInvoiceRequest,
    ) -> Result<InvoiceResponse, AppError> {
        if req.items.is_empty() {
            return Err(AppError::BadRequest(
                "Invoice must contain at least one item".to_string(),
                "INVALID_ITEMS",
            ));
        }

        let customer_name = req
            .customer_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Pelanggan Umum".to_string());
        let customer_address = req
            .customer_address
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let customer_email = req
            .customer_email
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let now_utc = Utc::now();
        let now_iso = now_utc.to_rfc3339();
        let due_date = req.due_date.unwrap_or_else(|| now_iso.clone());
        let currency = req.currency.unwrap_or_else(|| "IDR".to_string());
        let tax_type_str = req.tax_type.unwrap_or_else(|| "PPN_11_EXCL".to_string());

        let mut subtotal: i64 = 0;
        let mut computed_items = Vec::with_capacity(req.items.len());

        for item in req.items {
            if item.quantity <= 0 {
                return Err(AppError::BadRequest(
                    "Item quantity must be a positive integer".to_string(),
                    "INVALID_QUANTITY",
                ));
            }
            if item.unit_price < 0 {
                return Err(AppError::BadRequest(
                    "Item unit price must be a non-negative integer".to_string(),
                    "INVALID_PRICE",
                ));
            }
            if item.discount < 0 {
                return Err(AppError::BadRequest(
                    "Discount must be a non-negative integer".to_string(),
                    "INVALID_DISCOUNT",
                ));
            }

            let base_total = item
                .quantity
                .checked_mul(item.unit_price)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "Arithmetic overflow in line total calculation".to_string(),
                        "ARITHMETIC_OVERFLOW",
                    )
                })?;

            let line_total = base_total
                .checked_sub(item.discount)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "Arithmetic overflow in line total discount".to_string(),
                        "ARITHMETIC_OVERFLOW",
                    )
                })?;

            if line_total < 0 {
                return Err(AppError::BadRequest(
                    "Discount cannot exceed item gross total".to_string(),
                    "INVALID_DISCOUNT",
                ));
            }

            subtotal = subtotal.checked_add(line_total).ok_or_else(|| {
                AppError::BadRequest(
                    "Arithmetic overflow in subtotal calculation".to_string(),
                    "ARITHMETIC_OVERFLOW",
                )
            })?;

            computed_items.push(InvoiceItemDto {
                id: Uuid::new_v4().to_string(),
                description: item.description.trim().to_string(),
                quantity: item.quantity,
                unit_price: item.unit_price,
                discount: item.discount,
                tax_amount: 0,
                line_total,
            });
        }

        // Indonesian Tax Calculation
        let is_inclusive = tax_type_str.ends_with("_INCL");
        let tax_res = calculate_tax_integer(subtotal, &tax_type_str, is_inclusive)?;
        let tax_amount = tax_res.tax_amount;
        let total_amount = if is_inclusive {
            subtotal
        } else {
            subtotal.checked_add(tax_amount).ok_or_else(|| {
                AppError::BadRequest(
                    "Arithmetic overflow in total amount calculation".to_string(),
                    "ARITHMETIC_OVERFLOW",
                )
            })?
        };

        let authorized_by = req.authorized_by.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let created_by = Some(ctx.actor_id_str());

        let invoice_id = Uuid::new_v4().to_string();

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        sqlx::query(
            r#"
            INSERT INTO invoices 
                (id, tenant_id, invoice_number, customer_name, customer_address, customer_email,
                 due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, balance_due,
                 status, created_by, authorized_by, created_at, updated_at)
            VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?11, 'DRAFT', ?12, ?13, ?14, ?14)
            "#,
        )
        .bind(&invoice_id)
        .bind(ctx.tenant_id_str())
        .bind(&customer_name)
        .bind(&customer_address)
        .bind(&customer_email)
        .bind(&due_date)
        .bind(&currency)
        .bind(&tax_type_str)
        .bind(subtotal)
        .bind(tax_amount)
        .bind(total_amount)
        .bind(&created_by)
        .bind(&authorized_by)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        for item in &computed_items {
            sqlx::query(
                r#"
                INSERT INTO invoice_items 
                    (id, invoice_id, tenant_id, description, quantity, unit_price, discount, tax_amount, line_total, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
            )
            .bind(&item.id)
            .bind(&invoice_id)
            .bind(ctx.tenant_id_str())
            .bind(&item.description)
            .bind(item.quantity)
            .bind(item.unit_price)
            .bind(item.discount)
            .bind(item.tax_amount)
            .bind(item.line_total)
            .bind(&now_iso)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }

        tx.commit().await.map_err(AppError::from)?;

        Ok(InvoiceResponse {
            id: invoice_id,
            tenant_id: ctx.tenant_id_str(),
            invoice_number: None,
            customer_name,
            customer_address,
            customer_email,
            issue_date: None,
            due_date,
            currency,
            tax_type: tax_type_str,
            subtotal,
            discount: 0,
            tax_amount,
            total_amount,
            status: "DRAFT".to_string(),
            items: Some(computed_items),
            snapshot: None,
            created_by,
            issued_by: None,
            authorized_by,
            created_at: now_iso.clone(),
            updated_at: now_iso,
        })
    }

    /// Issues draft invoice: assigns gapless INV-YYYY-XXXXXX, captures snapshot, creates receivable, posts AR journal
    pub async fn issue_invoice(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<InvoiceResponse, AppError> {
        if !ctx.role.can_issue_invoice() {
            return Err(AppError::Forbidden(
                format!("Role {:?} is not authorized to issue invoices", ctx.role),
                "FORBIDDEN",
            ));
        }

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // 1. Fetch invoice with lock within transaction
        let inv_row = sqlx::query(
            "SELECT id, tenant_id, customer_name, customer_address, customer_email, due_date, currency, tax_type, subtotal, tax_amount, total_amount, status, created_by, authorized_by, created_at FROM invoices WHERE id = ?1 AND tenant_id = ?2",
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("Invoice not found".to_string(), "NOT_FOUND"))?;

        use sqlx::Row;
        let status: String = inv_row.get("status");
        if status != "DRAFT" {
            return Err(AppError::Conflict(
                "Invoice is already issued or finalized".to_string(),
                "ALREADY_ISSUED",
            ));
        }

        let cust_name: String = inv_row.get("customer_name");
        let cust_addr: Option<String> = inv_row.get("customer_address");
        let cust_email: Option<String> = inv_row.get("customer_email");
        let due_date: String = inv_row.get("due_date");
        let currency: String = inv_row.get("currency");
        let tax_type: String = inv_row.get("tax_type");
        let subtotal: i64 = inv_row.get("subtotal");
        let tax_amount: i64 = inv_row.get("tax_amount");
        let total_amount: i64 = inv_row.get("total_amount");
        let created_by: Option<String> = inv_row.get("created_by");
        let authorized_by: Option<String> = inv_row.get("authorized_by");
        let created_at: String = inv_row.get("created_at");

        // 2. Fetch line items
        let item_rows = sqlx::query(
            "SELECT id, description, quantity, unit_price, discount, tax_amount, line_total FROM invoice_items WHERE invoice_id = ?1",
        )
        .bind(invoice_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::from)?;

        let items_list: Vec<InvoiceItemDto> = item_rows
            .into_iter()
            .map(|r| InvoiceItemDto {
                id: r.get("id"),
                description: r.get("description"),
                quantity: r.get("quantity"),
                unit_price: r.get("unit_price"),
                discount: r.get("discount"),
                tax_amount: r.get("tax_amount"),
                line_total: r.get("line_total"),
            })
            .collect();

        // 3. Server-authoritative sequential numbering: [PREFIX]-YYYY-XXXXXX
        let now_utc = Utc::now();
        let year = now_utc.year();
        let now_iso = now_utc.to_rfc3339();

        let prefix: Option<String> = sqlx::query_scalar(
            "SELECT invoice_prefix FROM business_profiles WHERE tenant_id = ?1",
        )
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .flatten();

        let pfx = prefix
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| "INV".to_string());

        let year_pattern = format!("{}-{}-%", pfx, year);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE tenant_id = ?1 AND invoice_number IS NOT NULL AND invoice_number LIKE ?2",
        )
        .bind(ctx.tenant_id_str())
        .bind(&year_pattern)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::from)?;

        let inv_number = format!("{}-{}-{:06}", pfx, year, count + 1);
        let issued_by = Some(ctx.actor_id_str());

        // 4. Capture frozen snapshot
        let snapshot = InvoiceSnapshotDto {
            customer_name: cust_name.clone(),
            customer_address: cust_addr.clone(),
            customer_email: cust_email.clone(),
            items: items_list.clone(),
            tax_type: tax_type.clone(),
            subtotal,
            tax_amount,
            total_amount,
            created_by: created_by.clone(),
            issued_by: issued_by.clone(),
            authorized_by: authorized_by.clone(),
            issued_at: now_iso.clone(),
        };
        let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();

        // 5. Update invoice record
        let update_res = sqlx::query(
            r#"
            UPDATE invoices 
            SET invoice_number = ?1, status = 'ISSUED', issue_date = ?2, snapshot_json = ?3, issued_by = ?4, updated_at = ?2
            WHERE id = ?5 AND status = 'DRAFT'
            "#,
        )
        .bind(&inv_number)
        .bind(&now_iso)
        .bind(&snapshot_json)
        .bind(&issued_by)
        .bind(invoice_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        if update_res.rows_affected() == 0 {
            return Err(AppError::Conflict(
                "Invoice is already issued or finalized".to_string(),
                "ALREADY_ISSUED",
            ));
        }

        // 6. Insert into invoice_snapshots table
        let snap_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO invoice_snapshots (id, invoice_id, tenant_id, snapshot_json, captured_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(&snap_id)
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .bind(&snapshot_json)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // 7. Create matching Receivable record
        let rec_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO receivables 
                (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 0, ?4, ?5, 'OPEN', ?6, ?6)
            "#,
        )
        .bind(&rec_id)
        .bind(ctx.tenant_id_str())
        .bind(invoice_id)
        .bind(total_amount)
        .bind(&due_date)
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // 8. Automatic General Ledger posting via AccountingService WITHIN SAME TX
        let net_revenue = if tax_type.ends_with("_INCL") {
            subtotal - tax_amount
        } else {
            subtotal
        };

        let mut journal_lines = vec![
            PostJournalLineCommand {
                account_code: "1200".to_string(), // Piutang Usaha
                debit: Rupiah::new(total_amount),
                credit: Rupiah::ZERO,
                memo: Some(format!("Piutang Invoice {}", inv_number)),
            },
            PostJournalLineCommand {
                account_code: "4000".to_string(), // Pendapatan Usaha
                debit: Rupiah::ZERO,
                credit: Rupiah::new(net_revenue),
                memo: Some(format!("Pendapatan Invoice {}", inv_number)),
            },
        ];

        if tax_amount > 0 {
            journal_lines.push(PostJournalLineCommand {
                account_code: "2100".to_string(), // Utang Pajak
                debit: Rupiah::ZERO,
                credit: Rupiah::new(tax_amount),
                memo: Some(format!("Utang Pajak Invoice {}", inv_number)),
            });
        }

        let cmd = PostJournalEntryCommand {
            tenant_id: ctx.tenant_id,
            entry_date: now_utc,
            description: format!("Invoice {} issued to {}", inv_number, cust_name),
            source_type: "INVOICE".to_string(),
            source_id: Uuid::parse_str(invoice_id).ok(),
            lines: journal_lines,
        };

        // System posting context: allows invoice issuance by Staff/Manager to post ledger automatically
        let post_ctx = if ctx.role.can_post_ledger() {
            ctx.clone()
        } else {
            TenantContext {
                tenant_id: ctx.tenant_id,
                actor_id: ctx.actor_id,
                role: Role::Owner,
            }
        };

        self.accounting_service.post_journal_command_tx(&mut tx, &post_ctx, cmd).await?;

        // 9. Transactional Outbox Event: InvoiceIssued (Feature 22)
        let outbox_draft = crate::domain::outbox::OutboxEventDraft::invoice_issued(
            ctx.tenant_id,
            invoice_id,
            &inv_number,
            total_amount,
            &cust_name,
        );
        crate::repository::outbox_repo::SqlxOutboxRepository::insert_tx_static(&mut tx, &outbox_draft)
            .await
            .map_err(AppError::from)?;

        // 10. Atomic commit of domain mutations, journal lines, and outbox event
        tx.commit().await.map_err(AppError::from)?;

        Ok(InvoiceResponse {
            id: invoice_id.to_string(),
            tenant_id: ctx.tenant_id_str(),
            invoice_number: Some(inv_number),
            customer_name: cust_name,
            customer_address: cust_addr,
            customer_email: cust_email,
            issue_date: Some(now_iso.clone()),
            due_date,
            currency,
            tax_type,
            subtotal,
            discount: 0,
            tax_amount,
            total_amount,
            status: "ISSUED".to_string(),
            items: Some(items_list),
            snapshot: Some(snapshot),
            created_by,
            issued_by,
            authorized_by,
            created_at,
            updated_at: now_iso,
        })
    }

    /// Creates and immediately issues an invoice within an existing SQLite transaction.
    /// Used by project progress billing to avoid SQLite busy deadlock while ensuring full atomicity.
    pub async fn create_and_issue_invoice_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        ctx: &TenantContext,
        req: CreateInvoiceRequest,
    ) -> Result<InvoiceResponse, AppError> {
        if req.items.is_empty() {
            return Err(AppError::BadRequest(
                "Invoice must contain at least one item".to_string(),
                "INVALID_ITEMS",
            ));
        }

        let customer_name = req
            .customer_name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Pelanggan Umum".to_string());
        let customer_address = req
            .customer_address
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let customer_email = req
            .customer_email
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let now_utc = Utc::now();
        let now_iso = now_utc.to_rfc3339();
        let due_date = req.due_date.unwrap_or_else(|| now_iso.clone());
        let currency = req.currency.unwrap_or_else(|| "IDR".to_string());
        let tax_type_str = req.tax_type.unwrap_or_else(|| "PPN_11_EXCL".to_string());

        let mut subtotal: i64 = 0;
        let mut computed_items = Vec::with_capacity(req.items.len());

        for item in req.items {
            if item.quantity <= 0 {
                return Err(AppError::BadRequest(
                    "Item quantity must be a positive integer".to_string(),
                    "INVALID_QUANTITY",
                ));
            }
            if item.unit_price < 0 {
                return Err(AppError::BadRequest(
                    "Item unit price must be a non-negative integer".to_string(),
                    "INVALID_PRICE",
                ));
            }
            if item.discount < 0 {
                return Err(AppError::BadRequest(
                    "Discount must be a non-negative integer".to_string(),
                    "INVALID_DISCOUNT",
                ));
            }

            let base_total = item
                .quantity
                .checked_mul(item.unit_price)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "Arithmetic overflow in line total calculation".to_string(),
                        "ARITHMETIC_OVERFLOW",
                    )
                })?;

            let line_total = base_total
                .checked_sub(item.discount)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "Arithmetic overflow in line total discount".to_string(),
                        "ARITHMETIC_OVERFLOW",
                    )
                })?;

            if line_total < 0 {
                return Err(AppError::BadRequest(
                    "Discount cannot exceed item gross total".to_string(),
                    "INVALID_DISCOUNT",
                ));
            }

            subtotal = subtotal.checked_add(line_total).ok_or_else(|| {
                AppError::BadRequest(
                    "Arithmetic overflow in subtotal calculation".to_string(),
                    "ARITHMETIC_OVERFLOW",
                )
            })?;

            computed_items.push(InvoiceItemDto {
                id: Uuid::new_v4().to_string(),
                description: item.description.trim().to_string(),
                quantity: item.quantity,
                unit_price: item.unit_price,
                discount: item.discount,
                tax_amount: 0,
                line_total,
            });
        }

        // Indonesian Tax Calculation
        let is_inclusive = tax_type_str.ends_with("_INCL");
        let tax_res = calculate_tax_integer(subtotal, &tax_type_str, is_inclusive)?;
        let tax_amount = tax_res.tax_amount;
        let total_amount = if is_inclusive {
            subtotal
        } else {
            subtotal.checked_add(tax_amount).ok_or_else(|| {
                AppError::BadRequest(
                    "Arithmetic overflow in total amount calculation".to_string(),
                    "ARITHMETIC_OVERFLOW",
                )
            })?
        };

        let authorized_by = req.authorized_by.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let created_by = Some(ctx.actor_id_str());
        let issued_by = Some(ctx.actor_id_str());

        let invoice_id = Uuid::new_v4().to_string();

        // Server-authoritative sequential numbering: [PREFIX]-YYYY-XXXXXX
        let year = now_utc.year();

        let prefix: Option<String> = sqlx::query_scalar(
            "SELECT invoice_prefix FROM business_profiles WHERE tenant_id = ?1",
        )
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(AppError::from)?
        .flatten();

        let pfx = prefix
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| "INV".to_string());

        let year_pattern = format!("{}-{}-%", pfx, year);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE tenant_id = ?1 AND invoice_number IS NOT NULL AND invoice_number LIKE ?2",
        )
        .bind(ctx.tenant_id_str())
        .bind(&year_pattern)
        .fetch_one(&mut **tx)
        .await
        .map_err(AppError::from)?;

        let inv_number = format!("{}-{}-{:06}", pfx, year, count + 1);

        // Capture frozen snapshot
        let snapshot = InvoiceSnapshotDto {
            customer_name: customer_name.clone(),
            customer_address: customer_address.clone(),
            customer_email: customer_email.clone(),
            items: computed_items.clone(),
            tax_type: tax_type_str.clone(),
            subtotal,
            tax_amount,
            total_amount,
            created_by: created_by.clone(),
            issued_by: issued_by.clone(),
            authorized_by: authorized_by.clone(),
            issued_at: now_iso.clone(),
        };
        let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();

        // 1. Insert invoice record initially in DRAFT status
        sqlx::query(
            r#"
            INSERT INTO invoices 
                (id, tenant_id, invoice_number, customer_name, customer_address, customer_email,
                 due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, balance_due,
                 status, created_by, issued_by, authorized_by, created_at, updated_at)
            VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?11, 'DRAFT', ?12, ?13, ?14, ?15, ?15)
            "#,
        )
        .bind(&invoice_id)
        .bind(ctx.tenant_id_str())
        .bind(&customer_name)
        .bind(&customer_address)
        .bind(&customer_email)
        .bind(&due_date)
        .bind(&currency)
        .bind(&tax_type_str)
        .bind(subtotal)
        .bind(tax_amount)
        .bind(total_amount)
        .bind(&created_by)
        .bind(&issued_by)
        .bind(&authorized_by)
        .bind(&now_iso)
        .execute(&mut **tx)
        .await
        .map_err(AppError::from)?;

        // 2. Insert line items while invoice is in DRAFT status
        for item in &computed_items {
            sqlx::query(
                r#"
                INSERT INTO invoice_items 
                    (id, invoice_id, tenant_id, description, quantity, unit_price, discount, tax_amount, line_total, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
            )
            .bind(&item.id)
            .bind(&invoice_id)
            .bind(ctx.tenant_id_str())
            .bind(&item.description)
            .bind(item.quantity)
            .bind(item.unit_price)
            .bind(item.discount)
            .bind(item.tax_amount)
            .bind(item.line_total)
            .bind(&now_iso)
            .execute(&mut **tx)
            .await
            .map_err(AppError::from)?;
        }

        // 3. Update invoice record to ISSUED status with gapless number and frozen snapshot
        sqlx::query(
            r#"
            UPDATE invoices 
            SET invoice_number = ?1, status = 'ISSUED', issue_date = ?2, snapshot_json = ?3, issued_by = ?4, updated_at = ?2
            WHERE id = ?5 AND status = 'DRAFT'
            "#,
        )
        .bind(&inv_number)
        .bind(&now_iso)
        .bind(&snapshot_json)
        .bind(&issued_by)
        .bind(&invoice_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::from)?;

        // Insert into invoice_snapshots table
        let snap_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO invoice_snapshots (id, invoice_id, tenant_id, snapshot_json, captured_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(&snap_id)
        .bind(&invoice_id)
        .bind(ctx.tenant_id_str())
        .bind(&snapshot_json)
        .bind(&now_iso)
        .execute(&mut **tx)
        .await
        .map_err(AppError::from)?;

        // Create matching Receivable record
        let rec_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO receivables 
                (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 0, ?4, ?5, 'OPEN', ?6, ?6)
            "#,
        )
        .bind(&rec_id)
        .bind(ctx.tenant_id_str())
        .bind(&invoice_id)
        .bind(total_amount)
        .bind(&due_date)
        .bind(&now_iso)
        .execute(&mut **tx)
        .await
        .map_err(AppError::from)?;

        // Automatic General Ledger posting via AccountingService WITHIN SAME TX
        let net_revenue = if is_inclusive {
            subtotal - tax_amount
        } else {
            subtotal
        };

        let mut journal_lines = vec![
            PostJournalLineCommand {
                account_code: "1200".to_string(), // Piutang Usaha
                debit: Rupiah::new(total_amount),
                credit: Rupiah::ZERO,
                memo: Some(format!("Piutang Invoice {}", inv_number)),
            },
            PostJournalLineCommand {
                account_code: "4000".to_string(), // Pendapatan Usaha
                debit: Rupiah::ZERO,
                credit: Rupiah::new(net_revenue),
                memo: Some(format!("Pendapatan Invoice {}", inv_number)),
            },
        ];

        if tax_amount > 0 {
            journal_lines.push(PostJournalLineCommand {
                account_code: "2100".to_string(), // Utang Pajak
                debit: Rupiah::ZERO,
                credit: Rupiah::new(tax_amount),
                memo: Some(format!("Utang Pajak Invoice {}", inv_number)),
            });
        }

        let cmd = PostJournalEntryCommand {
            tenant_id: ctx.tenant_id,
            entry_date: now_utc,
            description: format!("Invoice {} issued to {}", inv_number, customer_name),
            source_type: "INVOICE".to_string(),
            source_id: Uuid::parse_str(&invoice_id).ok(),
            lines: journal_lines,
        };

        // System posting context: allows invoice issuance by Staff/Manager to post ledger automatically
        let post_ctx = if ctx.role.can_post_ledger() {
            ctx.clone()
        } else {
            TenantContext {
                tenant_id: ctx.tenant_id,
                actor_id: ctx.actor_id,
                role: Role::Owner,
            }
        };

        self.accounting_service.post_journal_command_tx(tx, &post_ctx, cmd).await?;

        // Transactional Outbox Event: InvoiceIssued
        let outbox_draft = crate::domain::outbox::OutboxEventDraft::invoice_issued(
            ctx.tenant_id,
            &invoice_id,
            &inv_number,
            total_amount,
            &customer_name,
        );
        crate::repository::outbox_repo::SqlxOutboxRepository::insert_tx_static(tx, &outbox_draft)
            .await
            .map_err(AppError::from)?;

        Ok(InvoiceResponse {
            id: invoice_id,
            tenant_id: ctx.tenant_id_str(),
            invoice_number: Some(inv_number),
            customer_name,
            customer_address,
            customer_email,
            issue_date: Some(now_iso.clone()),
            due_date,
            currency,
            tax_type: tax_type_str,
            subtotal,
            discount: 0,
            tax_amount,
            total_amount,
            status: "ISSUED".to_string(),
            items: Some(computed_items),
            snapshot: Some(snapshot),
            created_by,
            issued_by,
            authorized_by,
            created_at: now_iso.clone(),
            updated_at: now_iso,
        })
    }

    /// Voids draft or issued invoice; performs reversal journal if invoice was issued
    pub async fn void_invoice(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
        req: VoidInvoiceRequest,
    ) -> Result<InvoiceResponse, AppError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        let inv_row = sqlx::query(
            "SELECT id, tenant_id, invoice_number, status FROM invoices WHERE id = ?1 AND tenant_id = ?2",
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("Invoice not found".to_string(), "NOT_FOUND"))?;

        use sqlx::Row;
        let status: String = inv_row.get("status");
        if status == "PAID" || status == "PARTIALLY_PAID" {
            return Err(AppError::Conflict(
                "Cannot void invoice with allocated payments".to_string(),
                "CANNOT_VOID_PAID_INVOICE",
            ));
        }

        let now_iso = Utc::now().to_rfc3339();
        let inv_number: Option<String> = inv_row.get("invoice_number");

        sqlx::query("UPDATE invoices SET status = 'VOIDED', updated_at = ?1 WHERE id = ?2")
            .bind(&now_iso)
            .bind(invoice_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        sqlx::query(
            "UPDATE receivables SET status = 'VOIDED', outstanding_amount = 0, updated_at = ?1 WHERE invoice_id = ?2",
        )
        .bind(&now_iso)
        .bind(invoice_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

        // Transactional Outbox Event: InvoiceVoided (Feature 22)
        let outbox_draft = crate::domain::outbox::OutboxEventDraft::invoice_voided(
            ctx.tenant_id,
            invoice_id,
            inv_number.as_deref().unwrap_or(invoice_id),
            &req.reason,
        );
        crate::repository::outbox_repo::SqlxOutboxRepository::insert_tx_static(&mut tx, &outbox_draft)
            .await
            .map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;

        // If invoice was issued, reverse posted journal entry
        if status == "ISSUED" {
            let jrn_id: Option<String> = sqlx::query_scalar(
                "SELECT id FROM journal_entries WHERE tenant_id = ?1 AND source_type = 'INVOICE' AND source_id = ?2 AND is_reversed = 0",
            )
            .bind(ctx.tenant_id_str())
            .bind(invoice_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)?;

            if let Some(orig_jid) = jrn_id {
                let rev_req = ReverseJournalRequest {
                    reason: Some(format!(
                        "Void invoice {}: {}",
                        inv_number.as_deref().unwrap_or(invoice_id),
                        req.reason
                    )),
                };
                let rev_ctx = if ctx.role.can_reverse_journal() {
                    ctx.clone()
                } else {
                    TenantContext {
                        tenant_id: ctx.tenant_id,
                        actor_id: ctx.actor_id,
                        role: Role::Owner,
                    }
                };
                self.accounting_service.reverse_journal(&rev_ctx, &orig_jid, rev_req).await?;
            }
        }

        self.get_invoice(ctx, invoice_id).await
    }

    /// Gets invoice by ID with items and parsed snapshot
    pub async fn get_invoice(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<InvoiceResponse, AppError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, invoice_number, customer_name, customer_address, customer_email,
                   issue_date, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount,
                   status, snapshot_json, created_by, issued_by, authorized_by, created_at, updated_at
            FROM invoices
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("Invoice not found".to_string(), "NOT_FOUND"))?;

        use sqlx::Row;
        let snap_raw: Option<String> = row.get("snapshot_json");
        let snapshot: Option<InvoiceSnapshotDto> =
            snap_raw.as_deref().and_then(|s| serde_json::from_str(s).ok());

        let item_rows = sqlx::query(
            "SELECT id, description, quantity, unit_price, discount, tax_amount, line_total FROM invoice_items WHERE invoice_id = ?1",
        )
        .bind(invoice_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        let items: Vec<InvoiceItemDto> = item_rows
            .into_iter()
            .map(|r| InvoiceItemDto {
                id: r.get("id"),
                description: r.get("description"),
                quantity: r.get("quantity"),
                unit_price: r.get("unit_price"),
                discount: r.get("discount"),
                tax_amount: r.get("tax_amount"),
                line_total: r.get("line_total"),
            })
            .collect();

        Ok(InvoiceResponse {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            invoice_number: row.get("invoice_number"),
            customer_name: row.get("customer_name"),
            customer_address: row.get("customer_address"),
            customer_email: row.get("customer_email"),
            issue_date: row.get("issue_date"),
            due_date: row.get("due_date"),
            currency: row.get("currency"),
            tax_type: row.get("tax_type"),
            subtotal: row.get("subtotal"),
            discount: row.get("discount"),
            tax_amount: row.get("tax_amount"),
            total_amount: row.get("total_amount"),
            status: row.get("status"),
            items: Some(items),
            snapshot,
            created_by: row.get("created_by"),
            issued_by: row.get("issued_by"),
            authorized_by: row.get("authorized_by"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Lists all invoices for workspace ordered by created_at DESC
    pub async fn list_invoices(
        &self,
        ctx: &TenantContext,
    ) -> Result<InvoicesListResponse, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, invoice_number, customer_name, customer_address, customer_email,
                   issue_date, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount,
                   status, snapshot_json, created_by, issued_by, authorized_by, created_at, updated_at
            FROM invoices
            WHERE tenant_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        use sqlx::Row;
        let invoices: Vec<InvoiceResponse> = rows
            .into_iter()
            .map(|r| {
                let snap_raw: Option<String> = r.get("snapshot_json");
                let snapshot = snap_raw
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok());
                InvoiceResponse {
                    id: r.get("id"),
                    tenant_id: r.get("tenant_id"),
                    invoice_number: r.get("invoice_number"),
                    customer_name: r.get("customer_name"),
                    customer_address: r.get("customer_address"),
                    customer_email: r.get("customer_email"),
                    issue_date: r.get("issue_date"),
                    due_date: r.get("due_date"),
                    currency: r.get("currency"),
                    tax_type: r.get("tax_type"),
                    subtotal: r.get("subtotal"),
                    discount: r.get("discount"),
                    tax_amount: r.get("tax_amount"),
                    total_amount: r.get("total_amount"),
                    status: r.get("status"),
                    items: None,
                    snapshot,
                    created_by: r.get("created_by"),
                    issued_by: r.get("issued_by"),
                    authorized_by: r.get("authorized_by"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                }
            })
            .collect();

        let count = invoices.len();
        Ok(InvoicesListResponse { invoices, count })
    }

    /// Updates DRAFT invoice; rejects update if status != DRAFT with HTTP 409 INVOICE_LOCKED
    pub async fn update_invoice(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
        req: UpdateInvoiceRequest,
    ) -> Result<InvoiceResponse, AppError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        let current_status: String = sqlx::query_scalar(
            "SELECT status FROM invoices WHERE id = ?1 AND tenant_id = ?2",
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("Invoice not found".to_string(), "NOT_FOUND"))?;

        if current_status != "DRAFT" {
            return Err(AppError::Conflict(
                "Issued or paid invoices cannot be modified".to_string(),
                "INVOICE_LOCKED",
            ));
        }

        let now_iso = Utc::now().to_rfc3339();

        if req.customer_name.is_some()
            || req.customer_address.is_some()
            || req.customer_email.is_some()
            || req.due_date.is_some()
            || req.authorized_by.is_some()
        {
            sqlx::query(
                r#"
                UPDATE invoices
                SET customer_name = COALESCE(?1, customer_name),
                    customer_address = COALESCE(?2, customer_address),
                    customer_email = COALESCE(?3, customer_email),
                    due_date = COALESCE(?4, due_date),
                    authorized_by = COALESCE(?5, authorized_by),
                    updated_at = ?6
                WHERE id = ?7
                "#,
            )
            .bind(req.customer_name)
            .bind(req.customer_address)
            .bind(req.customer_email)
            .bind(req.due_date)
            .bind(req.authorized_by)
            .bind(&now_iso)
            .bind(invoice_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
        }

        tx.commit().await.map_err(AppError::from)?;
        self.get_invoice(ctx, invoice_id).await
    }

    /// Lists receivables for workspace ordered by due_date ASC
    pub async fn list_receivables(
        &self,
        ctx: &TenantContext,
    ) -> Result<ReceivablesListResponse, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT r.id, r.tenant_id, r.invoice_id, i.invoice_number, i.customer_name,
                   r.total_amount, r.allocated_amount, r.outstanding_amount, r.due_date,
                   r.status, r.created_at, r.updated_at
            FROM receivables r
            JOIN invoices i ON i.id = r.invoice_id
            WHERE r.tenant_id = ?1
            ORDER BY r.due_date ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        use sqlx::Row;
        let receivables: Vec<ReceivableResponse> = rows
            .into_iter()
            .map(|r| ReceivableResponse {
                id: r.get("id"),
                tenant_id: r.get("tenant_id"),
                invoice_id: r.get("invoice_id"),
                invoice_number: r.get("invoice_number"),
                customer_name: r.get("customer_name"),
                total_amount: r.get("total_amount"),
                allocated_amount: r.get("allocated_amount"),
                outstanding_amount: r.get("outstanding_amount"),
                due_date: r.get("due_date"),
                status: r.get("status"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect();

        let count = receivables.len();
        Ok(ReceivablesListResponse { receivables, count })
    }

    /// Computes aging buckets across active open receivables
    pub async fn get_aging_report(
        &self,
        ctx: &TenantContext,
    ) -> Result<AgingReportResponse, AppError> {
        let rows = sqlx::query(
            "SELECT due_date, outstanding_amount FROM receivables WHERE tenant_id = ?1 AND outstanding_amount > 0 AND status != 'VOIDED'",
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        use sqlx::Row;
        let today = Utc::now().date_naive();
        let mut b_0_30: i64 = 0;
        let mut b_31_60: i64 = 0;
        let mut b_61_90: i64 = 0;
        let mut b_90_plus: i64 = 0;
        let mut total_out: i64 = 0;

        for r in rows {
            let out: i64 = r.get("outstanding_amount");
            total_out += out;

            let due_str: String = r.get("due_date");
            let due_date_part = due_str.split('T').next().unwrap_or(&due_str);
            let days_overdue = match chrono::NaiveDate::parse_from_str(due_date_part, "%Y-%m-%d") {
                Ok(due_date) => (today - due_date).num_days(),
                Err(_) => 0,
            };

            if days_overdue <= 30 {
                b_0_30 += out;
            } else if days_overdue <= 60 {
                b_31_60 += out;
            } else if days_overdue <= 90 {
                b_61_90 += out;
            } else {
                b_90_plus += out;
            }
        }

        Ok(AgingReportResponse {
            current_0_30: b_0_30,
            overdue_31_60: b_31_60,
            overdue_61_90: b_61_90,
            overdue_90_plus: b_90_plus,
            total_outstanding: total_out,
        })
    }
}
