//! Commercial Receivable & Payment Repository with Strict Multi-Tenant Isolation
//! Manages open receivables, payment allocation, and aging records.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::domain::receivable::{Payment, PaymentAllocation, Receivable};
use crate::domain::tenant::TenantContext;
use crate::repository::DbError;

#[async_trait]
pub trait ReceivableRepository: Send + Sync {
    async fn create_receivable(
        &self,
        ctx: &TenantContext,
        receivable: &Receivable,
    ) -> Result<(), DbError>;

    async fn get_by_id(
        &self,
        ctx: &TenantContext,
        id: &str,
    ) -> Result<Option<Receivable>, DbError>;

    async fn get_by_invoice_id(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Option<Receivable>, DbError>;

    async fn list_receivables(&self, ctx: &TenantContext) -> Result<Vec<Receivable>, DbError>;

    async fn list_active_for_aging(&self, ctx: &TenantContext)
        -> Result<Vec<Receivable>, DbError>;

    async fn update_balances_and_status(
        &self,
        ctx: &TenantContext,
        id: &str,
        allocated: i64,
        outstanding: i64,
        status: &str,
    ) -> Result<bool, DbError>;

    async fn void_receivable_by_invoice_id(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<bool, DbError>;

    async fn record_payment(
        &self,
        ctx: &TenantContext,
        payment: &Payment,
        allocation: &PaymentAllocation,
    ) -> Result<(), DbError>;

    async fn list_payments_by_invoice(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Vec<Payment>, DbError>;

    async fn get_next_payment_number(
        &self,
        ctx: &TenantContext,
        year: i32,
    ) -> Result<String, DbError>;
}

pub struct SqlxReceivableRepository {
    pool: SqlitePool,
}

impl SqlxReceivableRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReceivableRepository for SqlxReceivableRepository {
    async fn create_receivable(
        &self,
        ctx: &TenantContext,
        receivable: &Receivable,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO receivables (
                id, tenant_id, invoice_id, total_amount, allocated_amount,
                outstanding_amount, due_date, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(&receivable.id)
        .bind(ctx.tenant_id_str())
        .bind(&receivable.invoice_id)
        .bind(receivable.total_amount)
        .bind(receivable.allocated_amount)
        .bind(receivable.outstanding_amount)
        .bind(&receivable.due_date)
        .bind(&receivable.status)
        .bind(&receivable.created_at)
        .bind(&receivable.updated_at)
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(())
    }

    async fn get_by_id(
        &self,
        ctx: &TenantContext,
        id: &str,
    ) -> Result<Option<Receivable>, DbError> {
        let rec = sqlx::query_as::<_, Receivable>(
            r#"
            SELECT id, tenant_id, invoice_id, total_amount, allocated_amount,
                   outstanding_amount, due_date, status, created_at, updated_at
            FROM receivables
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(rec)
    }

    async fn get_by_invoice_id(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Option<Receivable>, DbError> {
        let rec = sqlx::query_as::<_, Receivable>(
            r#"
            SELECT id, tenant_id, invoice_id, total_amount, allocated_amount,
                   outstanding_amount, due_date, status, created_at, updated_at
            FROM receivables
            WHERE invoice_id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(rec)
    }

    async fn list_receivables(&self, ctx: &TenantContext) -> Result<Vec<Receivable>, DbError> {
        let recs = sqlx::query_as::<_, Receivable>(
            r#"
            SELECT id, tenant_id, invoice_id, total_amount, allocated_amount,
                   outstanding_amount, due_date, status, created_at, updated_at
            FROM receivables
            WHERE tenant_id = ?1
            ORDER BY due_date ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(recs)
    }

    async fn list_active_for_aging(
        &self,
        ctx: &TenantContext,
    ) -> Result<Vec<Receivable>, DbError> {
        let recs = sqlx::query_as::<_, Receivable>(
            r#"
            SELECT id, tenant_id, invoice_id, total_amount, allocated_amount,
                   outstanding_amount, due_date, status, created_at, updated_at
            FROM receivables
            WHERE tenant_id = ?1 AND outstanding_amount > 0 AND status != 'VOIDED'
            ORDER BY due_date ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(recs)
    }

    async fn update_balances_and_status(
        &self,
        ctx: &TenantContext,
        id: &str,
        allocated: i64,
        outstanding: i64,
        status: &str,
    ) -> Result<bool, DbError> {
        let now_iso = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            UPDATE receivables
            SET allocated_amount = ?1,
                outstanding_amount = ?2,
                status = ?3,
                updated_at = ?4
            WHERE id = ?5 AND tenant_id = ?6
            "#,
        )
        .bind(allocated)
        .bind(outstanding)
        .bind(status)
        .bind(&now_iso)
        .bind(id)
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    async fn void_receivable_by_invoice_id(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<bool, DbError> {
        let now_iso = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query(
            r#"
            UPDATE receivables
            SET status = 'VOIDED',
                outstanding_amount = 0,
                updated_at = ?1
            WHERE invoice_id = ?2 AND tenant_id = ?3
            "#,
        )
        .bind(&now_iso)
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .execute(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(res.rows_affected() > 0)
    }

    async fn record_payment(
        &self,
        ctx: &TenantContext,
        payment: &Payment,
        allocation: &PaymentAllocation,
    ) -> Result<(), DbError> {
        let mut tx = self.pool.begin().await.map_err(DbError::from)?;

        sqlx::query(
            r#"
            INSERT INTO payments (
                id, tenant_id, invoice_id, receivable_id, payment_number,
                payment_date, amount, payment_method, reference, status, notes,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
        )
        .bind(&payment.id)
        .bind(ctx.tenant_id_str())
        .bind(&payment.invoice_id)
        .bind(&payment.receivable_id)
        .bind(&payment.payment_number)
        .bind(&payment.payment_date)
        .bind(payment.amount)
        .bind(&payment.payment_method)
        .bind(&payment.reference)
        .bind(&payment.status)
        .bind(&payment.notes)
        .bind(&payment.created_at)
        .bind(&payment.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(DbError::from)?;

        sqlx::query(
            r#"
            INSERT INTO payment_allocations (
                id, payment_id, invoice_id, tenant_id, amount, allocated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&allocation.id)
        .bind(&allocation.payment_id)
        .bind(&allocation.invoice_id)
        .bind(ctx.tenant_id_str())
        .bind(allocation.amount)
        .bind(&allocation.allocated_at)
        .execute(&mut *tx)
        .await
        .map_err(DbError::from)?;

        tx.commit().await.map_err(DbError::from)?;
        Ok(())
    }

    async fn list_payments_by_invoice(
        &self,
        ctx: &TenantContext,
        invoice_id: &str,
    ) -> Result<Vec<Payment>, DbError> {
        let payments = sqlx::query_as::<_, Payment>(
            r#"
            SELECT id, tenant_id, invoice_id, receivable_id, payment_number,
                   payment_date, amount, payment_method, reference, status, notes,
                   created_at, updated_at
            FROM payments
            WHERE invoice_id = ?1 AND tenant_id = ?2
            ORDER BY created_at ASC
            "#,
        )
        .bind(invoice_id)
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(payments)
    }

    async fn get_next_payment_number(
        &self,
        ctx: &TenantContext,
        year: i32,
    ) -> Result<String, DbError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM payments WHERE tenant_id = ?1",
        )
        .bind(ctx.tenant_id_str())
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::from)?;

        Ok(format!("PAY-{}-{:06}", year, count + 1))
    }
}
