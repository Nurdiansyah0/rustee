//! Inventory Repository with Strict Multi-Tenant Isolation (Features 1, 2, 3, 5, 6, 8, 17)
//!
//! Provides SQL persistence for warehouses, products, stock items, immutable stock movements,
//! and stock adjustments. All operations enforce strict TenantContext isolation (WHERE tenant_id = ?).

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::domain::inventory::{
    Product, PurchaseOrder, PurchaseOrderItem, PurchaseOrderStatus, PurchaseOrderWithItems,
    StockAdjustment, StockItem, StockItemWithDetails, StockMovement, StockMovementType, Warehouse,
};
use crate::domain::money::Rupiah;
use crate::domain::tenant::TenantContext;
use crate::repository::DbError;

#[async_trait]
pub trait InventoryRepository: Send + Sync {
    // --- Warehouse Operations ---
    async fn create_warehouse(&self, ctx: &TenantContext, warehouse: &Warehouse) -> Result<(), DbError>;
    async fn create_warehouse_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        warehouse: &Warehouse,
    ) -> Result<(), DbError>;
    async fn find_warehouse_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<Warehouse>, DbError>;
    async fn find_warehouse_by_code(&self, ctx: &TenantContext, code: &str) -> Result<Option<Warehouse>, DbError>;
    async fn list_warehouses(&self, ctx: &TenantContext) -> Result<Vec<Warehouse>, DbError>;
    async fn find_default_warehouse(&self, ctx: &TenantContext) -> Result<Option<Warehouse>, DbError>;
    async fn unset_default_warehouses_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<(), DbError>;

    // --- Product Operations ---
    async fn create_product(&self, ctx: &TenantContext, product: &Product) -> Result<(), DbError>;
    async fn create_product_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        product: &Product,
    ) -> Result<(), DbError>;
    async fn find_product_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<Product>, DbError>;
    async fn find_product_by_sku(&self, ctx: &TenantContext, sku: &str) -> Result<Option<Product>, DbError>;
    async fn list_products(&self, ctx: &TenantContext) -> Result<Vec<Product>, DbError>;
    async fn get_next_sku_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError>;

    // --- Stock Item Operations ---
    async fn find_stock_item(
        &self,
        ctx: &TenantContext,
        warehouse_id: Uuid,
        product_id: Uuid,
    ) -> Result<Option<StockItem>, DbError>;
    async fn find_stock_item_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        warehouse_id: Uuid,
        product_id: Uuid,
    ) -> Result<Option<StockItem>, DbError>;
    async fn find_stock_item_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<StockItem>, DbError>;
    async fn get_or_create_stock_item_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        warehouse_id: Uuid,
        product_id: Uuid,
        default_threshold: i64,
        default_cost: Rupiah,
    ) -> Result<StockItem, DbError>;
    async fn update_stock_item_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        item_id: Uuid,
        quantity_on_hand: i64,
        average_cost: Rupiah,
    ) -> Result<(), DbError>;
    async fn list_stock_items_with_details(
        &self,
        ctx: &TenantContext,
        warehouse_id: Option<Uuid>,
        product_id: Option<Uuid>,
        low_stock_only: bool,
    ) -> Result<Vec<StockItemWithDetails>, DbError>;
    async fn list_stock_items_by_warehouse(
        &self,
        ctx: &TenantContext,
        warehouse_id: Uuid,
    ) -> Result<Vec<StockItem>, DbError>;
    async fn list_low_stock_items(
        &self,
        ctx: &TenantContext,
        warehouse_id: Option<Uuid>,
    ) -> Result<Vec<StockItem>, DbError>;

    // --- Stock Movement Operations ---
    async fn insert_stock_movement_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        movement: &StockMovement,
    ) -> Result<(), DbError>;
    async fn list_stock_movements(
        &self,
        ctx: &TenantContext,
        product_id: Option<Uuid>,
        warehouse_id: Option<Uuid>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<StockMovement>, DbError>;

    // --- Stock Adjustment Operations ---
    async fn insert_stock_adjustment_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        adjustment: &StockAdjustment,
    ) -> Result<(), DbError>;
    async fn list_stock_adjustments(
        &self,
        ctx: &TenantContext,
        warehouse_id: Option<Uuid>,
        product_id: Option<Uuid>,
    ) -> Result<Vec<StockAdjustment>, DbError>;
    async fn get_next_adjustment_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError>;

    // --- Purchase Order Operations ---
    async fn create_purchase_order_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        po: &PurchaseOrder,
        items: &[PurchaseOrderItem],
    ) -> Result<(), DbError>;
    async fn get_purchase_order(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<PurchaseOrderWithItems>, DbError>;
    async fn get_purchase_order_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<PurchaseOrderWithItems>, DbError>;
    async fn list_purchase_orders(
        &self,
        ctx: &TenantContext,
    ) -> Result<Vec<PurchaseOrderWithItems>, DbError>;
    async fn update_purchase_order_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        status: PurchaseOrderStatus,
        updated_at: DateTime<Utc>,
    ) -> Result<(), DbError>;
    async fn update_purchase_order_item_received_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        item_id: Uuid,
        quantity_received: i64,
    ) -> Result<(), DbError>;
    async fn get_next_po_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError>;
}

pub struct SqlxInventoryRepository {
    pool: SqlitePool,
}

impl SqlxInventoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn row_to_warehouse(row: &sqlx::sqlite::SqliteRow) -> Result<Warehouse, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let created_at_str: String = row.get("created_at");
        let is_default_int: i64 = row.get("is_default");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid warehouse id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid created_at timestamp: {}", e)))?;

        Ok(Warehouse {
            id,
            tenant_id,
            code: row.get("code"),
            name: row.get("name"),
            address: row.get("address"),
            is_default: is_default_int == 1,
            created_at,
        })
    }

    fn row_to_product(row: &sqlx::sqlite::SqliteRow) -> Result<Product, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let created_at_str: String = row.get("created_at");
        let is_active_int: i64 = row.get("is_active");
        let cost_price_raw: i64 = row.get("cost_price");
        let sale_price_raw: i64 = row.get("sale_price");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid product id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid created_at timestamp: {}", e)))?;

        Ok(Product {
            id,
            tenant_id,
            sku: row.get("sku"),
            name: row.get("name"),
            unit: row.get("unit"),
            cost_price: Rupiah::new(cost_price_raw),
            sale_price: Rupiah::new(sale_price_raw),
            reorder_threshold: row.get("reorder_threshold"),
            is_active: is_active_int == 1,
            created_at,
        })
    }

    fn row_to_stock_item(row: &sqlx::sqlite::SqliteRow) -> Result<StockItem, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let warehouse_id_str: String = row.get("warehouse_id");
        let product_id_str: String = row.get("product_id");
        let updated_at_str: String = row.get("updated_at");
        let average_cost_raw: i64 = row.get("average_cost");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid stock item id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let warehouse_id = Uuid::parse_str(&warehouse_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid warehouse id: {}", e)))?;
        let product_id = Uuid::parse_str(&product_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid product id: {}", e)))?;
        let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid updated_at timestamp: {}", e)))?;

        Ok(StockItem {
            id,
            tenant_id,
            warehouse_id,
            product_id,
            quantity_on_hand: row.get("quantity_on_hand"),
            quantity_reserved: row.get("quantity_reserved"),
            reorder_threshold: row.get("reorder_threshold"),
            bin_location: row.get("bin_location"),
            average_cost: Rupiah::new(average_cost_raw),
            updated_at,
        })
    }

    fn row_to_stock_movement(row: &sqlx::sqlite::SqliteRow) -> Result<StockMovement, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let product_id_str: String = row.get("product_id");
        let m_type_str: String = row.get("movement_type");
        let source_wh_str: Option<String> = row.get("source_warehouse_id");
        let dest_wh_str: Option<String> = row.get("destination_warehouse_id");
        let unit_cost_raw: Option<i64> = row.get("unit_cost");
        let ref_id_str: Option<String> = row.get("reference_id");
        let actor_id_str: Option<String> = row.get("actor_id");
        let created_at_str: String = row.get("created_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid movement id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let product_id = Uuid::parse_str(&product_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid product id: {}", e)))?;
        let movement_type = StockMovementType::from_str(&m_type_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid movement type: {}", m_type_str)))?;

        let source_warehouse_id = source_wh_str.and_then(|s| Uuid::parse_str(&s).ok());
        let destination_warehouse_id = dest_wh_str.and_then(|s| Uuid::parse_str(&s).ok());
        let reference_id = ref_id_str.and_then(|s| Uuid::parse_str(&s).ok());
        let actor_id = actor_id_str.and_then(|s| Uuid::parse_str(&s).ok());

        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid created_at timestamp: {}", e)))?;

        Ok(StockMovement {
            id,
            tenant_id,
            movement_type,
            product_id,
            source_warehouse_id,
            destination_warehouse_id,
            quantity: row.get("quantity"),
            unit_cost: unit_cost_raw.map(Rupiah::new),
            reference_type: row.get("reference_type"),
            reference_id,
            batch_number: row.get("batch_number"),
            notes: row.get("notes"),
            created_at,
            actor_id,
        })
    }

    fn row_to_stock_adjustment(row: &sqlx::sqlite::SqliteRow) -> Result<StockAdjustment, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let warehouse_id_str: String = row.get("warehouse_id");
        let product_id_str: String = row.get("product_id");
        let journal_entry_id_str: Option<String> = row.get("journal_entry_id");
        let actor_id_str: Option<String> = row.get("actor_id");
        let created_at_str: String = row.get("created_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid adjustment id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let warehouse_id = Uuid::parse_str(&warehouse_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid warehouse id: {}", e)))?;
        let product_id = Uuid::parse_str(&product_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid product id: {}", e)))?;
        let journal_entry_id = journal_entry_id_str.and_then(|s| Uuid::parse_str(&s).ok());
        let actor_id = actor_id_str.and_then(|s| Uuid::parse_str(&s).ok());

        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid created_at timestamp: {}", e)))?;

        Ok(StockAdjustment {
            id,
            tenant_id,
            adjustment_number: row.get("adjustment_number"),
            warehouse_id,
            product_id,
            variance_quantity: row.get("variance_quantity"),
            previous_quantity: row.get("previous_quantity"),
            new_quantity: row.get("new_quantity"),
            reason: row.get("reason"),
            journal_entry_id,
            created_at,
            actor_id,
        })
    }

    fn row_to_purchase_order_header(row: &sqlx::sqlite::SqliteRow) -> Result<PurchaseOrder, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let dst_wh_str: String = row.get("destination_warehouse_id");
        let status_str: String = row.get("status");
        let total_amount_raw: i64 = row.get("total_amount");
        let created_at_str: String = row.get("created_at");
        let updated_at_str: String = row.get("updated_at");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid purchase order id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let destination_warehouse_id = Uuid::parse_str(&dst_wh_str)
            .map_err(|e| DbError::Serialization(format!("Invalid destination warehouse id: {}", e)))?;
        let status = PurchaseOrderStatus::from_str(&status_str)
            .ok_or_else(|| DbError::Serialization(format!("Invalid purchase order status: {}", status_str)))?;

        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid created_at timestamp: {}", e)))?;
        let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| DbError::Serialization(format!("Invalid updated_at timestamp: {}", e)))?;

        Ok(PurchaseOrder {
            id,
            tenant_id,
            po_number: row.get("po_number"),
            supplier_name: row.get("supplier_name"),
            destination_warehouse_id,
            status,
            total_amount: Rupiah::new(total_amount_raw),
            notes: row.get("notes"),
            created_at,
            updated_at,
        })
    }

    fn row_to_purchase_order_item(row: &sqlx::sqlite::SqliteRow) -> Result<PurchaseOrderItem, DbError> {
        let id_str: String = row.get("id");
        let tenant_id_str: String = row.get("tenant_id");
        let po_id_str: String = row.get("purchase_order_id");
        let prod_id_str: String = row.get("product_id");
        let unit_cost_raw: i64 = row.get("unit_cost");
        let total_cost_raw: i64 = row.get("total_cost");

        let id = Uuid::parse_str(&id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid purchase order item id: {}", e)))?;
        let tenant_id = Uuid::parse_str(&tenant_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
        let purchase_order_id = Uuid::parse_str(&po_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid purchase order id: {}", e)))?;
        let product_id = Uuid::parse_str(&prod_id_str)
            .map_err(|e| DbError::Serialization(format!("Invalid product id: {}", e)))?;

        Ok(PurchaseOrderItem {
            id,
            tenant_id,
            purchase_order_id,
            product_id,
            quantity_ordered: row.get("quantity_ordered"),
            quantity_received: row.get("quantity_received"),
            unit_cost: Rupiah::new(unit_cost_raw),
            total_cost: Rupiah::new(total_cost_raw),
        })
    }
}

#[async_trait]
impl InventoryRepository for SqlxInventoryRepository {
    // --- Warehouse Operations ---

    async fn create_warehouse(&self, ctx: &TenantContext, warehouse: &Warehouse) -> Result<(), DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from_sqlx)?;
        self.create_warehouse_tx(&mut tx, ctx, warehouse).await?;
        tx.commit().await.map_err(DbError::from_sqlx)?;
        Ok(())
    }

    async fn create_warehouse_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        warehouse: &Warehouse,
    ) -> Result<(), DbError> {
        let is_def_int = if warehouse.is_default { 1 } else { 0 };
        let now = Utc::now().to_rfc3339();

        if warehouse.is_default {
            self.unset_default_warehouses_tx(tx, ctx).await?;
        }

        sqlx::query(
            r#"
            INSERT INTO warehouses (id, tenant_id, code, name, address, is_default, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(warehouse.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(&warehouse.code)
        .bind(&warehouse.name)
        .bind(&warehouse.address)
        .bind(is_def_int)
        .bind(warehouse.created_at.to_rfc3339())
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn find_warehouse_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<Warehouse>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, code, name, address, is_default, created_at, updated_at
            FROM warehouses
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_warehouse).transpose()
    }

    async fn find_warehouse_by_code(&self, ctx: &TenantContext, code: &str) -> Result<Option<Warehouse>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, code, name, address, is_default, created_at, updated_at
            FROM warehouses
            WHERE code = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(code)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_warehouse).transpose()
    }

    async fn list_warehouses(&self, ctx: &TenantContext) -> Result<Vec<Warehouse>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, code, name, address, is_default, created_at, updated_at
            FROM warehouses
            WHERE tenant_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        rows.iter().map(Self::row_to_warehouse).collect()
    }

    async fn find_default_warehouse(&self, ctx: &TenantContext) -> Result<Option<Warehouse>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, code, name, address, is_default, created_at, updated_at
            FROM warehouses
            WHERE tenant_id = ?1 AND is_default = 1
            LIMIT 1
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_warehouse).transpose()
    }

    async fn unset_default_warehouses_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE warehouses
            SET is_default = 0, updated_at = ?2
            WHERE tenant_id = ?1 AND is_default = 1
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    // --- Product Operations ---

    async fn create_product(&self, ctx: &TenantContext, product: &Product) -> Result<(), DbError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(DbError::from_sqlx)?;
        self.create_product_tx(&mut tx, ctx, product).await?;
        tx.commit().await.map_err(DbError::from_sqlx)?;
        Ok(())
    }

    async fn create_product_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        product: &Product,
    ) -> Result<(), DbError> {
        let is_active_int = if product.is_active { 1 } else { 0 };
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
        )
        .bind(product.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(&product.sku)
        .bind(&product.name)
        .bind(&product.unit)
        .bind(product.cost_price.as_i64())
        .bind(product.sale_price.as_i64())
        .bind(product.reorder_threshold)
        .bind(is_active_int)
        .bind(product.created_at.to_rfc3339())
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn find_product_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<Product>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at
            FROM products
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_product).transpose()
    }

    async fn find_product_by_sku(&self, ctx: &TenantContext, sku: &str) -> Result<Option<Product>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at
            FROM products
            WHERE sku = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(sku)
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_product).transpose()
    }

    async fn list_products(&self, ctx: &TenantContext) -> Result<Vec<Product>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at
            FROM products
            WHERE tenant_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        rows.iter().map(Self::row_to_product).collect()
    }

    async fn get_next_sku_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError> {
        let max_val: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT MAX(CAST(SUBSTR(sku, 5) AS INTEGER))
            FROM products
            WHERE tenant_id = ?1 AND sku LIKE 'SKU-%'
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_one(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        let next_num = max_val.unwrap_or(0) + 1;
        Ok(format!("SKU-{:06}", next_num))
    }

    // --- Stock Item Operations ---

    async fn find_stock_item(
        &self,
        ctx: &TenantContext,
        warehouse_id: Uuid,
        product_id: Uuid,
    ) -> Result<Option<StockItem>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at
            FROM stock_items
            WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(warehouse_id.to_string())
        .bind(product_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_stock_item).transpose()
    }

    async fn find_stock_item_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        warehouse_id: Uuid,
        product_id: Uuid,
    ) -> Result<Option<StockItem>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at
            FROM stock_items
            WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(warehouse_id.to_string())
        .bind(product_id.to_string())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_stock_item).transpose()
    }

    async fn find_stock_item_by_id(&self, ctx: &TenantContext, id: Uuid) -> Result<Option<StockItem>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at
            FROM stock_items
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        row.as_ref().map(Self::row_to_stock_item).transpose()
    }

    async fn get_or_create_stock_item_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        warehouse_id: Uuid,
        product_id: Uuid,
        default_threshold: i64,
        default_cost: Rupiah,
    ) -> Result<StockItem, DbError> {
        if let Some(existing) = self.find_stock_item_tx(tx, ctx, warehouse_id, product_id).await? {
            return Ok(existing);
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
            VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, NULL, ?6, ?7)
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(warehouse_id.to_string())
        .bind(product_id.to_string())
        .bind(default_threshold)
        .bind(default_cost.as_i64())
        .bind(&now_str)
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(StockItem {
            id,
            tenant_id: ctx.tenant_id,
            warehouse_id,
            product_id,
            quantity_on_hand: 0,
            quantity_reserved: 0,
            reorder_threshold: default_threshold,
            bin_location: None,
            average_cost: default_cost,
            updated_at: now,
        })
    }

    async fn update_stock_item_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        item_id: Uuid,
        quantity_on_hand: i64,
        average_cost: Rupiah,
    ) -> Result<(), DbError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE stock_items
            SET quantity_on_hand = ?1, average_cost = ?2, updated_at = ?3
            WHERE id = ?4 AND tenant_id = ?5
            "#,
        )
        .bind(quantity_on_hand)
        .bind(average_cost.as_i64())
        .bind(&now)
        .bind(item_id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn list_stock_items_with_details(
        &self,
        ctx: &TenantContext,
        warehouse_id: Option<Uuid>,
        product_id: Option<Uuid>,
        low_stock_only: bool,
    ) -> Result<Vec<StockItemWithDetails>, DbError> {
        let mut query = String::from(
            r#"
            SELECT si.id, si.tenant_id, si.warehouse_id, si.product_id,
                   si.quantity_on_hand, si.quantity_reserved, si.reorder_threshold,
                   si.bin_location, si.average_cost, si.updated_at,
                   p.name as product_name, p.sku as product_sku, p.unit as product_unit,
                   w.name as warehouse_name, w.code as warehouse_code
            FROM stock_items si
            JOIN products p ON p.id = si.product_id AND p.tenant_id = si.tenant_id
            JOIN warehouses w ON w.id = si.warehouse_id AND w.tenant_id = si.tenant_id
            WHERE si.tenant_id = ?1
            "#,
        );

        if warehouse_id.is_some() {
            query.push_str(" AND si.warehouse_id = ?2");
        }
        if product_id.is_some() {
            if warehouse_id.is_some() {
                query.push_str(" AND si.product_id = ?3");
            } else {
                query.push_str(" AND si.product_id = ?2");
            }
        }
        query.push_str(" ORDER BY si.updated_at DESC");

        let mut q = sqlx::query(&query).bind(ctx.tenant_id_str());
        if let Some(wh_id) = warehouse_id {
            q = q.bind(wh_id.to_string());
        }
        if let Some(prod_id) = product_id {
            q = q.bind(prod_id.to_string());
        }

        let rows = q.fetch_all(&self.pool).await.map_err(DbError::from_sqlx)?;
        let mut result = Vec::with_capacity(rows.len());

        for row in rows {
            let id_str: String = row.get("id");
            let tenant_id_str: String = row.get("tenant_id");
            let warehouse_id_str: String = row.get("warehouse_id");
            let product_id_str: String = row.get("product_id");
            let updated_at_str: String = row.get("updated_at");
            let avg_cost_raw: i64 = row.get("average_cost");
            let q_on_hand: i64 = row.get("quantity_on_hand");
            let threshold: i64 = row.get("reorder_threshold");
            let is_low_stock = q_on_hand <= threshold;

            if low_stock_only && !is_low_stock {
                continue;
            }

            let id = Uuid::parse_str(&id_str)
                .map_err(|e| DbError::Serialization(format!("Invalid stock item id: {}", e)))?;
            let tenant_id = Uuid::parse_str(&tenant_id_str)
                .map_err(|e| DbError::Serialization(format!("Invalid tenant id: {}", e)))?;
            let wh_id = Uuid::parse_str(&warehouse_id_str)
                .map_err(|e| DbError::Serialization(format!("Invalid warehouse id: {}", e)))?;
            let pr_id = Uuid::parse_str(&product_id_str)
                .map_err(|e| DbError::Serialization(format!("Invalid product id: {}", e)))?;
            let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| DbError::Serialization(format!("Invalid updated_at timestamp: {}", e)))?;

            result.push(StockItemWithDetails {
                id,
                tenant_id,
                warehouse_id: wh_id,
                product_id: pr_id,
                quantity_on_hand: q_on_hand,
                quantity_reserved: row.get("quantity_reserved"),
                reorder_threshold: threshold,
                bin_location: row.get("bin_location"),
                average_cost: Rupiah::new(avg_cost_raw),
                updated_at,
                product_name: row.get("product_name"),
                product_sku: row.get("product_sku"),
                product_unit: row.get("product_unit"),
                warehouse_name: row.get("warehouse_name"),
                warehouse_code: row.get("warehouse_code"),
                is_low_stock,
            });
        }

        Ok(result)
    }

    async fn list_stock_items_by_warehouse(
        &self,
        ctx: &TenantContext,
        warehouse_id: Uuid,
    ) -> Result<Vec<StockItem>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at
            FROM stock_items
            WHERE tenant_id = ?1 AND warehouse_id = ?2
            ORDER BY updated_at DESC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(warehouse_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        rows.iter().map(Self::row_to_stock_item).collect()
    }

    async fn list_low_stock_items(
        &self,
        ctx: &TenantContext,
        warehouse_id: Option<Uuid>,
    ) -> Result<Vec<StockItem>, DbError> {
        let rows = if let Some(wh_id) = warehouse_id {
            sqlx::query(
                r#"
                SELECT id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at
                FROM stock_items
                WHERE tenant_id = ?1 AND warehouse_id = ?2 AND quantity_on_hand <= reorder_threshold
                ORDER BY updated_at DESC
                "#,
            )
            .bind(ctx.tenant_id_str())
            .bind(wh_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::from_sqlx)?
        } else {
            sqlx::query(
                r#"
                SELECT id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at
                FROM stock_items
                WHERE tenant_id = ?1 AND quantity_on_hand <= reorder_threshold
                ORDER BY updated_at DESC
                "#,
            )
            .bind(ctx.tenant_id_str())
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::from_sqlx)?
        };

        rows.iter().map(Self::row_to_stock_item).collect()
    }

    // --- Stock Movement Operations ---

    async fn insert_stock_movement_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        movement: &StockMovement,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO stock_movements (
                id, tenant_id, movement_type, product_id, source_warehouse_id,
                destination_warehouse_id, quantity, unit_cost, reference_type,
                reference_id, batch_number, notes, created_at, actor_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
        )
        .bind(movement.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(movement.movement_type.as_str())
        .bind(movement.product_id.to_string())
        .bind(movement.source_warehouse_id.map(|id| id.to_string()))
        .bind(movement.destination_warehouse_id.map(|id| id.to_string()))
        .bind(movement.quantity)
        .bind(movement.unit_cost.map(|c| c.as_i64()))
        .bind(&movement.reference_type)
        .bind(movement.reference_id.map(|id| id.to_string()))
        .bind(&movement.batch_number)
        .bind(&movement.notes)
        .bind(movement.created_at.to_rfc3339())
        .bind(movement.actor_id.map(|id| id.to_string()))
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn list_stock_movements(
        &self,
        ctx: &TenantContext,
        product_id: Option<Uuid>,
        warehouse_id: Option<Uuid>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<StockMovement>, DbError> {
        let mut query = String::from(
            r#"
            SELECT id, tenant_id, movement_type, product_id, source_warehouse_id,
                   destination_warehouse_id, quantity, unit_cost, reference_type,
                   reference_id, batch_number, notes, created_at, actor_id
            FROM stock_movements
            WHERE tenant_id = ?1
            "#,
        );

        if product_id.is_some() {
            query.push_str(" AND product_id = ?2");
        }
        if warehouse_id.is_some() {
            if product_id.is_some() {
                query.push_str(" AND (source_warehouse_id = ?3 OR destination_warehouse_id = ?3)");
            } else {
                query.push_str(" AND (source_warehouse_id = ?2 OR destination_warehouse_id = ?2)");
            }
        }
        query.push_str(" ORDER BY created_at DESC");

        if let Some(l) = limit {
            query.push_str(&format!(" LIMIT {}", l));
            if let Some(o) = offset {
                query.push_str(&format!(" OFFSET {}", o));
            }
        }

        let mut q = sqlx::query(&query).bind(ctx.tenant_id_str());
        if let Some(p_id) = product_id {
            q = q.bind(p_id.to_string());
        }
        if let Some(w_id) = warehouse_id {
            q = q.bind(w_id.to_string());
        }

        let rows = q.fetch_all(&self.pool).await.map_err(DbError::from_sqlx)?;
        rows.iter().map(Self::row_to_stock_movement).collect()
    }

    // --- Stock Adjustment Operations ---

    async fn insert_stock_adjustment_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        adjustment: &StockAdjustment,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO stock_adjustments (
                id, tenant_id, adjustment_number, warehouse_id, product_id,
                variance_quantity, previous_quantity, new_quantity, reason,
                journal_entry_id, created_at, actor_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        )
        .bind(adjustment.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(&adjustment.adjustment_number)
        .bind(adjustment.warehouse_id.to_string())
        .bind(adjustment.product_id.to_string())
        .bind(adjustment.variance_quantity)
        .bind(adjustment.previous_quantity)
        .bind(adjustment.new_quantity)
        .bind(&adjustment.reason)
        .bind(adjustment.journal_entry_id.map(|id| id.to_string()))
        .bind(adjustment.created_at.to_rfc3339())
        .bind(adjustment.actor_id.map(|id| id.to_string()))
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn list_stock_adjustments(
        &self,
        ctx: &TenantContext,
        warehouse_id: Option<Uuid>,
        product_id: Option<Uuid>,
    ) -> Result<Vec<StockAdjustment>, DbError> {
        let mut query = String::from(
            r#"
            SELECT id, tenant_id, adjustment_number, warehouse_id, product_id,
                   variance_quantity, previous_quantity, new_quantity, reason,
                   journal_entry_id, created_at, actor_id
            FROM stock_adjustments
            WHERE tenant_id = ?1
            "#,
        );

        if warehouse_id.is_some() {
            query.push_str(" AND warehouse_id = ?2");
        }
        if product_id.is_some() {
            if warehouse_id.is_some() {
                query.push_str(" AND product_id = ?3");
            } else {
                query.push_str(" AND product_id = ?2");
            }
        }
        query.push_str(" ORDER BY created_at DESC");

        let mut q = sqlx::query(&query).bind(ctx.tenant_id_str());
        if let Some(w_id) = warehouse_id {
            q = q.bind(w_id.to_string());
        }
        if let Some(p_id) = product_id {
            q = q.bind(p_id.to_string());
        }

        let rows = q.fetch_all(&self.pool).await.map_err(DbError::from_sqlx)?;
        rows.iter().map(Self::row_to_stock_adjustment).collect()
    }

    async fn get_next_adjustment_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError> {
        let year = Utc::now().year();
        let prefix = format!("ADJ-{}-", year);
        let pattern = format!("{}%", prefix);

        let max_val: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT MAX(CAST(SUBSTR(adjustment_number, 10) AS INTEGER))
            FROM stock_adjustments
            WHERE tenant_id = ?1 AND adjustment_number LIKE ?2
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(&pattern)
        .fetch_one(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        let next_num = max_val.unwrap_or(0) + 1;
        Ok(format!("ADJ-{}-{:06}", year, next_num))
    }

    // --- Purchase Order Operations ---

    async fn create_purchase_order_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        po: &PurchaseOrder,
        items: &[PurchaseOrderItem],
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO purchase_orders (
                id, tenant_id, po_number, supplier_name, destination_warehouse_id,
                status, total_amount, notes, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(po.id.to_string())
        .bind(ctx.tenant_id_str())
        .bind(&po.po_number)
        .bind(&po.supplier_name)
        .bind(po.destination_warehouse_id.to_string())
        .bind(po.status.as_str())
        .bind(po.total_amount.as_i64())
        .bind(&po.notes)
        .bind(po.created_at.to_rfc3339())
        .bind(po.updated_at.to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        for item in items {
            sqlx::query(
                r#"
                INSERT INTO purchase_order_items (
                    id, tenant_id, purchase_order_id, product_id,
                    quantity_ordered, quantity_received, unit_cost, total_cost
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )
            .bind(item.id.to_string())
            .bind(ctx.tenant_id_str())
            .bind(po.id.to_string())
            .bind(item.product_id.to_string())
            .bind(item.quantity_ordered)
            .bind(item.quantity_received)
            .bind(item.unit_cost.as_i64())
            .bind(item.total_cost.as_i64())
            .execute(&mut **tx)
            .await
            .map_err(DbError::from_sqlx)?;
        }

        Ok(())
    }

    async fn get_purchase_order(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<PurchaseOrderWithItems>, DbError> {
        let po_row = sqlx::query(
            r#"
            SELECT * FROM purchase_orders
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let po_row = match po_row {
            Some(row) => row,
            None => return Ok(None),
        };

        let po = Self::row_to_purchase_order_header(&po_row)?;

        let item_rows = sqlx::query(
            r#"
            SELECT * FROM purchase_order_items
            WHERE purchase_order_id = ?1 AND tenant_id = ?2
            ORDER BY rowid ASC
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        let items = item_rows
            .iter()
            .map(Self::row_to_purchase_order_item)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(PurchaseOrderWithItems {
            id: po.id,
            tenant_id: po.tenant_id,
            po_number: po.po_number,
            supplier_name: po.supplier_name,
            destination_warehouse_id: po.destination_warehouse_id,
            status: po.status,
            total_amount: po.total_amount,
            notes: po.notes,
            items,
            created_at: po.created_at,
            updated_at: po.updated_at,
        }))
    }

    async fn get_purchase_order_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<Option<PurchaseOrderWithItems>, DbError> {
        let po_row = sqlx::query(
            r#"
            SELECT * FROM purchase_orders
            WHERE id = ?1 AND tenant_id = ?2
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        let po_row = match po_row {
            Some(row) => row,
            None => return Ok(None),
        };

        let po = Self::row_to_purchase_order_header(&po_row)?;

        let item_rows = sqlx::query(
            r#"
            SELECT * FROM purchase_order_items
            WHERE purchase_order_id = ?1 AND tenant_id = ?2
            ORDER BY rowid ASC
            "#,
        )
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        let items = item_rows
            .iter()
            .map(Self::row_to_purchase_order_item)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(PurchaseOrderWithItems {
            id: po.id,
            tenant_id: po.tenant_id,
            po_number: po.po_number,
            supplier_name: po.supplier_name,
            destination_warehouse_id: po.destination_warehouse_id,
            status: po.status,
            total_amount: po.total_amount,
            notes: po.notes,
            items,
            created_at: po.created_at,
            updated_at: po.updated_at,
        }))
    }

    async fn list_purchase_orders(
        &self,
        ctx: &TenantContext,
    ) -> Result<Vec<PurchaseOrderWithItems>, DbError> {
        let po_rows = sqlx::query(
            r#"
            SELECT * FROM purchase_orders
            WHERE tenant_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        if po_rows.is_empty() {
            return Ok(Vec::new());
        }

        // Batch query all items for the tenant's purchase orders (O(N+M) instead of N+1)
        let item_rows = sqlx::query(
            r#"
            SELECT * FROM purchase_order_items
            WHERE tenant_id = ?1
            ORDER BY rowid ASC
            "#,
        )
        .bind(ctx.tenant_id_str())
        .fetch_all(&self.pool)
        .await
        .map_err(DbError::from_sqlx)?;

        use std::collections::HashMap;
        let mut items_by_po: HashMap<Uuid, Vec<crate::domain::inventory::PurchaseOrderItem>> = HashMap::new();
        for ir in &item_rows {
            let item = Self::row_to_purchase_order_item(ir)?;
            items_by_po.entry(item.purchase_order_id).or_default().push(item);
        }

        let mut res = Vec::with_capacity(po_rows.len());
        for row in &po_rows {
            let po = Self::row_to_purchase_order_header(row)?;
            let items = items_by_po.remove(&po.id).unwrap_or_default();

            res.push(PurchaseOrderWithItems {
                id: po.id,
                tenant_id: po.tenant_id,
                po_number: po.po_number,
                supplier_name: po.supplier_name,
                destination_warehouse_id: po.destination_warehouse_id,
                status: po.status,
                total_amount: po.total_amount,
                notes: po.notes,
                items,
                created_at: po.created_at,
                updated_at: po.updated_at,
            });
        }

        Ok(res)
    }

    async fn update_purchase_order_status_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        id: Uuid,
        status: PurchaseOrderStatus,
        updated_at: DateTime<Utc>,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE purchase_orders
            SET status = ?1, updated_at = ?2
            WHERE id = ?3 AND tenant_id = ?4
            "#,
        )
        .bind(status.as_str())
        .bind(updated_at.to_rfc3339())
        .bind(id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn update_purchase_order_item_received_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
        item_id: Uuid,
        quantity_received: i64,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE purchase_order_items
            SET quantity_received = ?1
            WHERE id = ?2 AND tenant_id = ?3
            "#,
        )
        .bind(quantity_received)
        .bind(item_id.to_string())
        .bind(ctx.tenant_id_str())
        .execute(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(())
    }

    async fn get_next_po_number_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &TenantContext,
    ) -> Result<String, DbError> {
        let year = Utc::now().year();
        let prefix = format!("PO-{}-", year);
        let pattern = format!("{}%", prefix);

        let max_val: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT MAX(CAST(SUBSTR(po_number, 9) AS INTEGER))
            FROM purchase_orders
            WHERE tenant_id = ?1 AND po_number LIKE ?2
            "#,
        )
        .bind(ctx.tenant_id_str())
        .bind(&pattern)
        .fetch_one(&mut **tx)
        .await
        .map_err(DbError::from_sqlx)?;

        let next_num = max_val.unwrap_or(0) + 1;
        Ok(format!("PO-{}-{:06}", year, next_num))
    }
}

