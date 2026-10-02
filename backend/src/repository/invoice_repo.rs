//! Commercial Invoice Repository with Strict Multi-Tenant Isolation
//! Handles persistence of invoices, line items, and document snapshots.

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::domain::invoice::{Invoice, InvoiceItem};
use crate::domain::tenant::TenantContext;
use crate::repository::DbError;

#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn create_invoice(
        &self,
        ctx: &TenantContext,
        invoice: &Invoice,
        items: &[InvoiceItem],
    ) -> Result<(), DbError>;

    async fn get_invoice_by_id(
        &self,
        ctx: &TenantContext,
        id: &str,
    ) -> Result<Option<Invoice>, DbError>;

    async fn get_invoice_items(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Vec<InvoiceItem>, DbError>;

    async fn list_invoices(&self, ctx: &TenantContext) -> Result<Vec<Invoice>, DbError>;

    async fn update_invoice_draft(
        &self,
        ctx: &TenantContext,
        id: &str,
        customer_name: Option<&str>,
        customer_address: Option<&str>,
        customer_email: Option<&str>,
        due_date: Option<&str>,
    ) -> Result<bool, DbError>;

    async fn issue_invoice(
        &self,
        ctx: &TenantContext,
        id: &str,
        invoice_number: &str,
        issue_date: &str,
        snapshot_json: &str,
    ) -> Result<(), DbError>;

    async fn void_invoice(&self, ctx: &TenantContext, id: &str) -> Result<bool, DbError>;

    async fn update_invoice_status_and_balance(
        &self,
        ctx: &TenantContext,
        id: &str,
        status: &str,
        balance_due: i64,
    ) -> Result<bool, DbError>;

    async fn get_next_invoice_number(
        &self,
        ctx: &TenantContext,
        prefix: &str,
        year: i32,
    ) -> Result<String, DbError>;

    async fn save_snapshot(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
        snapshot_json: &str,
    ) -> Result<(), DbError>;

    async fn get_snapshot(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Option<String>, DbError>;
}

pub struct SqlxInvoiceRepository {
    pool: SqlitePool,
}

impl SqlxInvoiceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InvoiceRepository for SqlxInvoiceRepository {
    async fn create_invoice(
        &self,
        ctx: &TenantContext,
        invoice: &Invoice,
        items: &[InvoiceItem],
    ) -> Result<(), DbError> {
        let mut tx = self.pool.begin().await.map_err(DbError::from)?;

        sqlx::query(
            r#"
            INSERT INTO invoices (
                id, tenant_id, invoice_number, status, customer_id, customer_name,
                customer_address, customer_email, issue_date, due_date, currency,
                subtotal, discount, tax_type, tax_amount, total_amount, balance_due,
                snapshot_json, notes, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
            )
            "#,
        )
        .bind(&invoice.id)
        .bind(ctx.tenant_id_str())
        .bind(&invoice.invoice_number)
        .bind(&invoice.status)
        .bind(&invoice.customer_id)
        .bind(&invoice.customer_name)
        .bind(&invoice.customer_address)
        .bind(&invoice.customer_email)
        .bind(&invoice.issue_date)
        .bind(&invoice.due_date)
        .bind(&invoice.currency)
        .bind(invoice.subtotal)
        .bind(invoice.discount)
        .bind(&invoice.tax_type)
        .bind(invoice.tax_amount)
        .bind(invoice.total_amount)
        .bind(invoice.balance_due)
        .bind(&invoice.snapshot_json)
        .bind(&invoice.notes)
        .bind(&invoice.created_at)
        .bind(&invoice.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(DbError::from)?;

        for item in items {
            sqlx::query(
                r#"
                INSERT INTO invoice_items (
                    id, invoice_id, tenant_id, description, quantity, unit_price,
                    discount, tax_amount, line_total, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
            )
            .bind(&item.id)
            .bind(&invoice.id)
            .bind(ctx.tenant_id_str())
            .bind(&item.description)
            .bind(item.quantity)
            .bind(item.unit_price)
            .bind(item.discount)
            .bind(item.tax_amount)
            .bind(item.line_total)
            .bind(&item.created_at)
            .execute(&mut *tx)
            .await
            .map_err(DbError::from)?;
        }

        tx.commit().await.map_err(DbError::from)?;
        Ok(())
    }

    async fn get_invoice_by_id(
        &self,
        ctx: &TenantContext,
        id: &str,
    ) -> Result<Option<Invoice>, DbError> {
        let invoice = sqlx::query_as::<_, Invoice>(
            r#"
            SELECT id, tenant_id, invoice_number, status, customer_id, customer_name,
                   customer_address, customer_email, issue_date, due_date, currency,
                   subtotal, discount, tax_type, tax_amount, total_amount, balance_due,
                   snapshot_json, notes, created_at, updated_at
            FROM invoices
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(invoice)
    }

    async fn get_invoice_items(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Vec<InvoiceItem>, DbError> {
        let items = sqlx::query_as::<_, InvoiceItem>(
            r#"
            SELECT id, invoice_id, tenant_id, description, quantity, unit_price,
                   discount, tax_amount, line_total, created_at
            FROM invoice_items
            WHERE invoice_id = ?1 AND tenant_id = ?2
            ORDER BY rowid ASC
            "#,
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(items)
    }

    async fn list_invoices(&self, ctx: &TenantContext) -> Result<Vec<Invoice>, DbError> {
        let invoices = sqlx::query_as::<_, Invoice>(
            r#"
            SELECT id, tenant_id, invoice_number, status, customer_id, customer_name,
                   customer_address, customer_email, issue_date, due_date, currency,
                   subtotal, discount, tax_type, tax_amount, total_amount, balance_due,
                   snapshot_json, notes, created_at, updated_at
            FROM invoices
            WHERE tenant_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(invoices)
    }

    async fn update_invoice_draft(
        &self,
        ctx: &TenantContext,
        id: &str,
        customer_name: Option<&str>,
        customer_address: Option<&str>,
        customer_email: Option<&str>,
        due_date: Option<&str>,
    ) -> Result<bool, DbError> {
        let now_iso = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            UPDATE invoices
            SET customer_name = COALESCE(?1, customer_name),
                customer_address = COALESCE(?2, customer_address),
                customer_email = COALESCE(?3, customer_email),
                due_date = COALESCE(?4, due_date),
                updated_at = ?5
            WHERE id = ?6 AND tenant_id = ?7 AND status = 'DRAFT'
            "#,
        )
        .bind(customer_name)
        .bind(customer_address)
        .bind(customer_email)
        .bind(due_date)
        .bind(&now_iso)
        .bind(id)
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    async fn issue_invoice(
        &self,
        ctx: &TenantContext,
        id: &str,
        invoice_number: &str,
        issue_date: &str,
        snapshot_json: &str,
    ) -> Result<(), DbError> {
        let res = sqlx::query(
            r#"
            UPDATE invoices
            SET invoice_number = ?1,
                status = 'ISSUED',
                issue_date = ?2,
                snapshot_json = ?3,
                updated_at = ?2
            WHERE id = ?4 AND tenant_id = ?5 AND status = 'DRAFT'
            "#,
        )
        .bind(invoice_number)
        .bind(issue_date)
        .bind(snapshot_json)
        .bind(id)
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        if res.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }

        Ok(())
    }

    async fn void_invoice(&self, ctx: &TenantContext, id: &str) -> Result<bool, DbError> {
        let now_iso = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            UPDATE invoices
            SET status = 'VOIDED',
                updated_at = ?1
            WHERE id = ?2 AND tenant_id = ?3 AND status IN ('DRAFT', 'ISSUED')
            "#,
        )
        .bind(&now_iso)
        .bind(id)
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    async fn update_invoice_status_and_balance(
        &self,
        ctx: &TenantContext,
        id: &str,
        status: &str,
        balance_due: i64,
    ) -> Result<bool, DbError> {
        let now_iso = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            UPDATE invoices
            SET status = ?1,
                balance_due = ?2,
                updated_at = ?3
            WHERE id = ?4 AND tenant_id = ?5
            "#,
        )
        .bind(status)
        .bind(balance_due)
        .bind(&now_iso)
        .bind(id)
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    async fn get_next_invoice_number(
        &self,
        ctx: &TenantContext,
        prefix: &str,
        year: i32,
    ) -> Result<String, DbError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices WHERE tenant_id = ?1 AND invoice_number IS NOT NULL",
        )
        .bind(ctx.tenant_id_str())
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(format!("{}-{}-{:06}", prefix, year, count + 1))
    }

    async fn save_snapshot(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
        snapshot_json: &str,
    ) -> Result<(), DbError> {
        let id = Uuid::new_v4().to_string();
        let now_iso = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO invoice_snapshots (id, invoice_id, tenant_id, snapshot_json, captured_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(&id)
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .bind(snapshot_json)
        .bind(&now_iso)
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    async fn get_snapshot(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Option<String>, DbError> {
        let snap: Option<String> = sqlx::query_scalar(
            "SELECT snapshot_json FROM invoice_snapshots WHERE invoice_id = ?1 AND tenant_id = ?2",
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(snap)
    }
}
