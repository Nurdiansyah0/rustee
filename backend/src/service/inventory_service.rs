//! Inventory Service with Atomic Concurrency Serialization & Negative Balance Prevention (R1, R2)
//!
//! Enforces:
//! - Strict multi-tenant isolation via TenantContext
//! - BEGIN IMMEDIATE exclusive write locking to prevent race-condition overselling
//! - Hard negative balance rejection (HTTP 422 INSUFFICIENT_STOCK)
//! - Atomic inter-warehouse transfers with source != destination constraint
//! - Physical cycle count adjustments with sequential numbering (ADJ-YYYY-XXXXXX)
//! - Idempotency-Key caching and payload hash verification
//! - Transactional outbox event emission (StockReceived, StockDeducted, StockTransferred, StockAdjusted)

use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::accounting::{PostJournalEntryCommand, PostJournalLineCommand};
use crate::domain::inventory::{
    calculate_weighted_average_cost, AdjustStockRequest, CancelPurchaseOrderRequest,
    CancelPurchaseOrderResult, CreateMovementRequest, CreateProductRequest,
    CreatePurchaseOrderRequest, CreateWarehouseRequest, OrderPurchaseOrderResult, Product,
    PurchaseOrder, PurchaseOrderItem, PurchaseOrderStatus, PurchaseOrderWithItems,
    ReceivePurchaseOrderRequest, ReceivePurchaseOrderResult, StockAdjustment,
    StockAdjustmentResult, StockItemWithDetails, StockMovement, StockMovementResult,
    StockMovementType, StockTransferResult, TransferStockRequest, Warehouse,
};
use crate::domain::money::Rupiah;
use crate::domain::outbox::OutboxEventDraft;
use crate::domain::tenant::{Role, TenantContext};
use crate::error::AppError;
use crate::repository::idempotency_repo::{IdempotencyLockResult, SqlxIdempotencyRepository};
use crate::repository::inventory_repo::{InventoryRepository, SqlxInventoryRepository};
use crate::repository::outbox_repo::{OutboxRepository, SqlxOutboxRepository};
use crate::repository::IdempotencyRepository;
use crate::service::accounting_service::AccountingService;

pub struct InventoryService {
    pool: SqlitePool,
    repo: Arc<dyn InventoryRepository>,
    idempotency_repo: Arc<dyn IdempotencyRepository>,
    outbox_repo: Arc<dyn OutboxRepository>,
    accounting_service: Arc<AccountingService>,
}

impl InventoryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool: pool.clone(),
            repo: Arc::new(SqlxInventoryRepository::new(pool.clone())),
            idempotency_repo: Arc::new(SqlxIdempotencyRepository::new(pool.clone())),
            outbox_repo: Arc::new(SqlxOutboxRepository::new(pool.clone())),
            accounting_service: Arc::new(AccountingService::new_with_pool(pool)),
        }
    }

    pub fn new_with_repos(
        pool: SqlitePool,
        repo: Arc<dyn InventoryRepository>,
        idempotency_repo: Arc<dyn IdempotencyRepository>,
        outbox_repo: Arc<dyn OutboxRepository>,
        accounting_service: Arc<AccountingService>,
    ) -> Self {
        Self {
            pool,
            repo,
            idempotency_repo,
            outbox_repo,
            accounting_service,
        }
    }

    fn compute_payload_hash(payload_bytes: Option<&[u8]>, fallback_json: &serde_json::Value) -> String {
        let mut hasher = Sha256::new();
        if let Some(bytes) = payload_bytes {
            hasher.update(bytes);
        } else {
            hasher.update(fallback_json.to_string().as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    // =========================================================================
    // Warehouse Operations
    // =========================================================================

    pub async fn create_warehouse(
        &self,
        ctx: &TenantContext,
        req: CreateWarehouseRequest,
    ) -> Result<Warehouse, AppError> {
        // RBAC: Requires Owner, Administrator, or Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient role permissions to create warehouse".to_string(),
                    "FORBIDDEN",
                )
            })?;

        let code = req.code.trim();
        let name = req.name.trim();

        if code.is_empty() {
            return Err(AppError::BadRequest(
                "Warehouse code cannot be empty".to_string(),
                "INVALID_CODE",
            ));
        }
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "Warehouse name cannot be empty".to_string(),
                "INVALID_NAME",
            ));
        }

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // Check duplicate code
        if self.repo.find_warehouse_by_code(ctx, code).await?.is_some() {
            return Err(AppError::Conflict(
                format!("Warehouse with code '{}' already exists", code),
                "DUPLICATE_WAREHOUSE_CODE",
            ));
        }

        let warehouse = Warehouse {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            code: code.to_string(),
            name: name.to_string(),
            address: req.address.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            is_default: req.is_default,
            created_at: Utc::now(),
        };

        self.repo.create_warehouse_tx(&mut tx, ctx, &warehouse).await?;
        tx.commit().await.map_err(AppError::from)?;

        Ok(warehouse)
    }

    pub async fn get_warehouse(&self, ctx: &TenantContext, id: Uuid) -> Result<Warehouse, AppError> {
        self.repo
            .find_warehouse_by_id(ctx, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Warehouse '{}' not found", id), "NOT_FOUND"))
    }

    pub async fn list_warehouses(&self, ctx: &TenantContext) -> Result<Vec<Warehouse>, AppError> {
        let warehouses = self.repo.list_warehouses(ctx).await?;
        Ok(warehouses)
    }

    // =========================================================================
    // Product Operations
    // =========================================================================

    pub async fn create_product(
        &self,
        ctx: &TenantContext,
        req: CreateProductRequest,
    ) -> Result<Product, AppError> {
        let name = req.name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "Product name cannot be empty".to_string(),
                "INVALID_NAME",
            ));
        }
        if req.cost_price.as_i64() < 0 {
            return Err(AppError::BadRequest(
                "Cost price cannot be negative".to_string(),
                "INVALID_PRICE",
            ));
        }
        if req.sale_price.as_i64() < 0 {
            return Err(AppError::BadRequest(
                "Sale price cannot be negative".to_string(),
                "INVALID_PRICE",
            ));
        }
        if req.reorder_threshold < 0 {
            return Err(AppError::BadRequest(
                "Reorder threshold cannot be negative".to_string(),
                "INVALID_THRESHOLD",
            ));
        }

        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        let sku = match req.sku.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(custom_sku) => {
                if self.repo.find_product_by_sku(ctx, custom_sku).await?.is_some() {
                    return Err(AppError::Conflict(
                        format!("Product with SKU '{}' already exists", custom_sku),
                        "DUPLICATE_SKU",
                    ));
                }
                custom_sku.to_string()
            }
            None => self.repo.get_next_sku_number_tx(&mut tx, ctx).await?,
        };

        let product = Product {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            sku,
            name: name.to_string(),
            unit: if req.unit.trim().is_empty() { "pcs".to_string() } else { req.unit.trim().to_string() },
            cost_price: req.cost_price,
            sale_price: req.sale_price,
            reorder_threshold: req.reorder_threshold,
            is_active: true,
            created_at: Utc::now(),
        };

        self.repo.create_product_tx(&mut tx, ctx, &product).await?;
        tx.commit().await.map_err(AppError::from)?;

        Ok(product)
    }

    pub async fn get_product(&self, ctx: &TenantContext, id: Uuid) -> Result<Product, AppError> {
        self.repo
            .find_product_by_id(ctx, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Product '{}' not found", id), "NOT_FOUND"))
    }

    pub async fn list_products(&self, ctx: &TenantContext) -> Result<Vec<Product>, AppError> {
        let products = self.repo.list_products(ctx).await?;
        Ok(products)
    }

    // =========================================================================
    // Stock Level Queries
    // =========================================================================

    pub async fn list_stock(
        &self,
        ctx: &TenantContext,
        warehouse_id: Option<Uuid>,
        product_id: Option<Uuid>,
        low_stock_only: bool,
    ) -> Result<Vec<StockItemWithDetails>, AppError> {
        let items = self
            .repo
            .list_stock_items_with_details(ctx, warehouse_id, product_id, low_stock_only)
            .await?;
        Ok(items)
    }

    // =========================================================================
    // Stock Movement Mutations (INBOUND, OUTBOUND)
    // =========================================================================

    pub async fn create_movement(
        &self,
        ctx: &TenantContext,
        req: CreateMovementRequest,
        idempotency_key: Option<&str>,
        payload_bytes: Option<&[u8]>,
    ) -> Result<(StockMovementResult, bool), AppError> {
        if req.quantity <= 0 {
            return Err(AppError::BadRequest(
                "Quantity must be a positive integer".to_string(),
                "INVALID_QUANTITY",
            ));
        }

        // Validate product existence
        let product = self.get_product(ctx, req.product_id).await?;

        // Idempotency lock check
        let user_id = ctx.actor_id_str();
        if let Some(key) = idempotency_key {
            let req_val = json!(req);
            let req_hash = Self::compute_payload_hash(payload_bytes, &req_val);

            match self.idempotency_repo.acquire_lock(&user_id, key, &req_hash, 86_400).await? {
                IdempotencyLockResult::Cached { response_body, .. } => {
                    let cached_res: StockMovementResult = serde_json::from_str(&response_body)
                        .map_err(|e| AppError::Internal(format!("Failed to parse cached response: {}", e)))?;
                    return Ok((cached_res, true));
                }
                IdempotencyLockResult::MismatchedPayload => {
                    return Err(AppError::Conflict(
                        format!("Idempotency key '{}' was used with a different payload", key),
                        "IDEMPOTENCY_MISMATCH",
                    ));
                }
                IdempotencyLockResult::InProgress => {
                    return Err(AppError::Conflict(
                        format!("Request '{}' is already in progress", key),
                        "IDEMPOTENCY_IN_PROGRESS",
                    ));
                }
                IdempotencyLockResult::Acquired => {}
            }
        }

        // Concurrency write lock serialization
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        let now = Utc::now();
        let mov_id = Uuid::new_v4();

        let result = match req.movement_type {
            StockMovementType::Outbound => {
                let src_wh = req.source_warehouse_id.ok_or_else(|| {
                    AppError::BadRequest(
                        "Source warehouse required for OUTBOUND".to_string(),
                        "MISSING_SOURCE_WAREHOUSE",
                    )
                })?;

                // Validate warehouse exists
                if self.repo.find_warehouse_by_id(ctx, src_wh).await?.is_none() {
                    return Err(AppError::NotFound(
                        format!("Source warehouse '{}' not found", src_wh),
                        "NOT_FOUND",
                    ));
                }

                // Check stock item under transaction
                let stock_item = self.repo.find_stock_item_tx(&mut tx, ctx, src_wh, req.product_id).await?;
                let current_on_hand = stock_item.as_ref().map(|s| s.quantity_on_hand).unwrap_or(0);
                let current_reserved = stock_item.as_ref().map(|s| s.quantity_reserved).unwrap_or(0);
                let available_stock = current_on_hand.saturating_sub(current_reserved);

                // ATOMIC NEGATIVE BALANCE PREVENTION INVARIANT
                if req.quantity > available_stock {
                    return Err(AppError::UnprocessableEntity(
                        format!(
                            "Insufficient stock: requested {}, available {}",
                            req.quantity, available_stock
                        ),
                        "INSUFFICIENT_STOCK",
                    ));
                }

                let s_item = stock_item.expect("Stock item must exist if available_stock >= req.quantity");
                let new_on_hand = current_on_hand - req.quantity;

                self.repo
                    .update_stock_item_tx(&mut tx, ctx, s_item.id, new_on_hand, s_item.average_cost)
                    .await?;

                let unit_cost = req.unit_cost.unwrap_or(s_item.average_cost);

                let movement = StockMovement {
                    id: mov_id,
                    tenant_id: ctx.tenant_id,
                    movement_type: StockMovementType::Outbound,
                    product_id: req.product_id,
                    source_warehouse_id: Some(src_wh),
                    destination_warehouse_id: None,
                    quantity: req.quantity,
                    unit_cost: Some(unit_cost),
                    reference_type: Some("DIRECT_MUTATION".to_string()),
                    reference_id: None,
                    batch_number: req.batch_number.clone(),
                    notes: req.notes.clone(),
                    created_at: now,
                    actor_id: Some(ctx.actor_id),
                };

                self.repo.insert_stock_movement_tx(&mut tx, ctx, &movement).await?;

                // Auto-post balanced COGS GL journal: Debit 5000 (Beban Pokok) / Credit 1300 (Persediaan)
                let cogs_val = (req.quantity as i128 * unit_cost.as_i64() as i128) as i64;
                if cogs_val > 0 {
                    let post_ctx = if ctx.role.can_post_ledger() {
                        ctx.clone()
                    } else {
                        TenantContext {
                            tenant_id: ctx.tenant_id,
                            actor_id: ctx.actor_id,
                            role: Role::Owner,
                        }
                    };

                    let cmd = PostJournalEntryCommand {
                        tenant_id: ctx.tenant_id,
                        entry_date: now,
                        description: format!("Fulfillment Beban Pokok Penjualan ({})", product.name),
                        source_type: "INVENTORY_OUTBOUND".to_string(),
                        source_id: Some(mov_id),
                        lines: vec![
                            PostJournalLineCommand {
                                account_code: "5000".to_string(),
                                debit: Rupiah::new(cogs_val),
                                credit: Rupiah::ZERO,
                                memo: Some("Beban Pokok Penjualan".to_string()),
                            },
                            PostJournalLineCommand {
                                account_code: "1300".to_string(),
                                debit: Rupiah::ZERO,
                                credit: Rupiah::new(cogs_val),
                                memo: Some("Persediaan Barang Dagang".to_string()),
                            },
                        ],
                    };

                    self.accounting_service.post_journal_command_tx(&mut tx, &post_ctx, cmd).await?;
                }

                // Commit Transactional Outbox Event
                let draft = OutboxEventDraft::new(
                    ctx.tenant_id,
                    "StockDeducted",
                    "Inventory",
                    mov_id.to_string(),
                    json!({
                        "movement_id": mov_id,
                        "product_id": req.product_id,
                        "warehouse_id": src_wh,
                        "quantity": req.quantity
                    }),
                );
                self.outbox_repo.insert_tx(&mut tx, &draft).await.map_err(AppError::from)?;

                StockMovementResult {
                    id: mov_id,
                    movement_type: StockMovementType::Outbound,
                    product_id: req.product_id,
                    quantity: req.quantity,
                    remaining_stock: Some(new_on_hand),
                    resulting_stock: None,
                    average_cost: None,
                    created_at: now,
                }
            }

            StockMovementType::Inbound => {
                let dst_wh = req.destination_warehouse_id.ok_or_else(|| {
                    AppError::BadRequest(
                        "Destination warehouse required for INBOUND".to_string(),
                        "MISSING_DESTINATION_WAREHOUSE",
                    )
                })?;

                // Validate warehouse exists
                if self.repo.find_warehouse_by_id(ctx, dst_wh).await?.is_none() {
                    return Err(AppError::NotFound(
                        format!("Destination warehouse '{}' not found", dst_wh),
                        "NOT_FOUND",
                    ));
                }

                let effective_cost = req.unit_cost.unwrap_or(product.cost_price);

                let stock_item = self
                    .repo
                    .get_or_create_stock_item_tx(
                        &mut tx,
                        ctx,
                        dst_wh,
                        req.product_id,
                        product.reorder_threshold,
                        effective_cost,
                    )
                    .await?;

                let new_on_hand = stock_item.quantity_on_hand + req.quantity;
                let new_wac = calculate_weighted_average_cost(
                    stock_item.quantity_on_hand,
                    stock_item.average_cost,
                    req.quantity,
                    effective_cost,
                )?;

                self.repo
                    .update_stock_item_tx(&mut tx, ctx, stock_item.id, new_on_hand, new_wac)
                    .await?;

                let movement = StockMovement {
                    id: mov_id,
                    tenant_id: ctx.tenant_id,
                    movement_type: StockMovementType::Inbound,
                    product_id: req.product_id,
                    source_warehouse_id: None,
                    destination_warehouse_id: Some(dst_wh),
                    quantity: req.quantity,
                    unit_cost: Some(effective_cost),
                    reference_type: Some("DIRECT_MUTATION".to_string()),
                    reference_id: None,
                    batch_number: req.batch_number.clone(),
                    notes: req.notes.clone(),
                    created_at: now,
                    actor_id: Some(ctx.actor_id),
                };

                self.repo.insert_stock_movement_tx(&mut tx, ctx, &movement).await?;

                // Auto-post balanced GL journal: Debit 1300 (Persediaan) / Credit 2000 (Utang Usaha)
                let rcv_val = (req.quantity as i128 * effective_cost.as_i64() as i128) as i64;
                if rcv_val > 0 {
                    let post_ctx = if ctx.role.can_post_ledger() {
                        ctx.clone()
                    } else {
                        TenantContext {
                            tenant_id: ctx.tenant_id,
                            actor_id: ctx.actor_id,
                            role: Role::Owner,
                        }
                    };

                    let cmd = PostJournalEntryCommand {
                        tenant_id: ctx.tenant_id,
                        entry_date: now,
                        description: format!("Inbound Stock Receipt ({})", product.name),
                        source_type: "INVENTORY_INBOUND".to_string(),
                        source_id: Some(mov_id),
                        lines: vec![
                            PostJournalLineCommand {
                                account_code: "1300".to_string(),
                                debit: Rupiah::new(rcv_val),
                                credit: Rupiah::ZERO,
                                memo: Some("Persediaan Barang Dagang".to_string()),
                            },
                            PostJournalLineCommand {
                                account_code: "2000".to_string(),
                                debit: Rupiah::ZERO,
                                credit: Rupiah::new(rcv_val),
                                memo: Some("Utang Usaha".to_string()),
                            },
                        ],
                    };

                    self.accounting_service.post_journal_command_tx(&mut tx, &post_ctx, cmd).await?;
                }

                // Commit Transactional Outbox Event
                let draft = OutboxEventDraft::new(
                    ctx.tenant_id,
                    "StockReceived",
                    "Inventory",
                    mov_id.to_string(),
                    json!({
                        "movement_id": mov_id,
                        "product_id": req.product_id,
                        "warehouse_id": dst_wh,
                        "quantity": req.quantity
                    }),
                );
                self.outbox_repo.insert_tx(&mut tx, &draft).await.map_err(AppError::from)?;

                StockMovementResult {
                    id: mov_id,
                    movement_type: StockMovementType::Inbound,
                    product_id: req.product_id,
                    quantity: req.quantity,
                    remaining_stock: None,
                    resulting_stock: Some(new_on_hand),
                    average_cost: Some(new_wac),
                    created_at: now,
                }
            }

            _ => {
                return Err(AppError::BadRequest(
                    format!("Unsupported movement type: {:?}", req.movement_type),
                    "INVALID_MOVEMENT_TYPE",
                ));
            }
        };

        // Cache response atomically if Idempotency-Key provided
        if let Some(key) = idempotency_key {
            let res_body = serde_json::to_string(&result)
                .map_err(|e| AppError::Internal(format!("Failed to serialize result: {}", e)))?;
            self.idempotency_repo
                .save_response_tx(&mut tx, &user_id, key, 201, &res_body)
                .await?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok((result, false))
    }

    // =========================================================================
    // Inter-Warehouse Stock Transfer
    // =========================================================================

    pub async fn transfer_stock(
        &self,
        ctx: &TenantContext,
        req: TransferStockRequest,
        idempotency_key: Option<&str>,
        payload_bytes: Option<&[u8]>,
    ) -> Result<(StockTransferResult, bool), AppError> {
        // Source and destination warehouse uniqueness check
        if req.source_warehouse_id == req.destination_warehouse_id {
            return Err(AppError::BadRequest(
                "Source and destination warehouses cannot be the same".to_string(),
                "SAME_WAREHOUSE_TRANSFER",
            ));
        }

        if req.quantity <= 0 {
            return Err(AppError::BadRequest(
                "Transfer quantity must be a positive integer".to_string(),
                "INVALID_QUANTITY",
            ));
        }

        // Idempotency lock check
        let user_id = ctx.actor_id_str();
        if let Some(key) = idempotency_key {
            let req_val = json!(req);
            let req_hash = Self::compute_payload_hash(payload_bytes, &req_val);

            match self.idempotency_repo.acquire_lock(&user_id, key, &req_hash, 86_400).await? {
                IdempotencyLockResult::Cached { response_body, .. } => {
                    let cached_res: StockTransferResult = serde_json::from_str(&response_body)
                        .map_err(|e| AppError::Internal(format!("Failed to parse cached response: {}", e)))?;
                    return Ok((cached_res, true));
                }
                IdempotencyLockResult::MismatchedPayload => {
                    return Err(AppError::Conflict(
                        format!("Idempotency key '{}' was used with a different payload", key),
                        "IDEMPOTENCY_MISMATCH",
                    ));
                }
                IdempotencyLockResult::InProgress => {
                    return Err(AppError::Conflict(
                        format!("Request '{}' is already in progress", key),
                        "IDEMPOTENCY_IN_PROGRESS",
                    ));
                }
                IdempotencyLockResult::Acquired => {}
            }
        }

        // Concurrency write lock serialization
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // Validate warehouses and product
        if self.repo.find_warehouse_by_id(ctx, req.source_warehouse_id).await?.is_none() {
            return Err(AppError::NotFound(
                format!("Source warehouse '{}' not found", req.source_warehouse_id),
                "NOT_FOUND",
            ));
        }
        if self.repo.find_warehouse_by_id(ctx, req.destination_warehouse_id).await?.is_none() {
            return Err(AppError::NotFound(
                format!("Destination warehouse '{}' not found", req.destination_warehouse_id),
                "NOT_FOUND",
            ));
        }
        let product = self.get_product(ctx, req.product_id).await?;

        // Lock & check source warehouse stock
        let src_stock_item = self
            .repo
            .find_stock_item_tx(&mut tx, ctx, req.source_warehouse_id, req.product_id)
            .await?;
        let src_on_hand = src_stock_item.as_ref().map(|s| s.quantity_on_hand).unwrap_or(0);
        let src_reserved = src_stock_item.as_ref().map(|s| s.quantity_reserved).unwrap_or(0);
        let src_available = src_on_hand.saturating_sub(src_reserved);

        // ATOMIC NEGATIVE BALANCE PREVENTION INVARIANT
        if req.quantity > src_available {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Insufficient source stock: requested {}, available {}",
                    req.quantity, src_available
                ),
                "INSUFFICIENT_STOCK",
            ));
        }

        let src_item = src_stock_item.expect("Source stock item must exist");
        let new_src_q = src_on_hand - req.quantity;

        self.repo
            .update_stock_item_tx(&mut tx, ctx, src_item.id, new_src_q, src_item.average_cost)
            .await?;

        // Lock & update/create destination stock item
        let dst_stock_item = self
            .repo
            .get_or_create_stock_item_tx(
                &mut tx,
                ctx,
                req.destination_warehouse_id,
                req.product_id,
                product.reorder_threshold,
                src_item.average_cost,
            )
            .await?;

        let new_dst_q = dst_stock_item.quantity_on_hand + req.quantity;
        self.repo
            .update_stock_item_tx(&mut tx, ctx, dst_stock_item.id, new_dst_q, dst_stock_item.average_cost)
            .await?;

        let now = Utc::now();
        let mov_id = Uuid::new_v4();

        let movement = StockMovement {
            id: mov_id,
            tenant_id: ctx.tenant_id,
            movement_type: StockMovementType::Transfer,
            product_id: req.product_id,
            source_warehouse_id: Some(req.source_warehouse_id),
            destination_warehouse_id: Some(req.destination_warehouse_id),
            quantity: req.quantity,
            unit_cost: Some(src_item.average_cost),
            reference_type: Some("TRANSFER".to_string()),
            reference_id: None,
            batch_number: None,
            notes: req.notes.clone(),
            created_at: now,
            actor_id: Some(ctx.actor_id),
        };

        self.repo.insert_stock_movement_tx(&mut tx, ctx, &movement).await?;

        // Transactional Outbox Event
        let draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "StockTransferred",
            "Inventory",
            mov_id.to_string(),
            json!({
                "movement_id": mov_id,
                "source_warehouse_id": req.source_warehouse_id,
                "destination_warehouse_id": req.destination_warehouse_id,
                "product_id": req.product_id,
                "quantity": req.quantity
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &draft).await.map_err(AppError::from)?;

        let result = StockTransferResult {
            movement_id: mov_id,
            source_warehouse_id: req.source_warehouse_id,
            destination_warehouse_id: req.destination_warehouse_id,
            product_id: req.product_id,
            quantity: req.quantity,
            status: "COMPLETED".to_string(),
            source_remaining: new_src_q,
            destination_total: new_dst_q,
            created_at: now,
        };

        // Cache response atomically if Idempotency-Key provided
        if let Some(key) = idempotency_key {
            let res_body = serde_json::to_string(&result)
                .map_err(|e| AppError::Internal(format!("Failed to serialize result: {}", e)))?;
            self.idempotency_repo
                .save_response_tx(&mut tx, &user_id, key, 200, &res_body)
                .await?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok((result, false))
    }

    // =========================================================================
    // Physical Stock Count Adjustment
    // =========================================================================

    pub async fn adjust_stock(
        &self,
        ctx: &TenantContext,
        req: AdjustStockRequest,
        idempotency_key: Option<&str>,
        payload_bytes: Option<&[u8]>,
    ) -> Result<(StockAdjustmentResult, bool), AppError> {
        // RBAC: Requires Owner, Administrator, or Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient role permissions to post stock adjustment".to_string(),
                    "FORBIDDEN",
                )
            })?;

        if req.actual_quantity < 0 {
            return Err(AppError::UnprocessableEntity(
                "Actual stock quantity cannot be negative".to_string(),
                "NEGATIVE_STOCK_PROHIBITED",
            ));
        }

        // Idempotency lock check
        let user_id = ctx.actor_id_str();
        if let Some(key) = idempotency_key {
            let req_val = json!(req);
            let req_hash = Self::compute_payload_hash(payload_bytes, &req_val);

            match self.idempotency_repo.acquire_lock(&user_id, key, &req_hash, 86_400).await? {
                IdempotencyLockResult::Cached { response_body, .. } => {
                    let cached_res: StockAdjustmentResult = serde_json::from_str(&response_body)
                        .map_err(|e| AppError::Internal(format!("Failed to parse cached response: {}", e)))?;
                    return Ok((cached_res, true));
                }
                IdempotencyLockResult::MismatchedPayload => {
                    return Err(AppError::Conflict(
                        format!("Idempotency key '{}' was used with a different payload", key),
                        "IDEMPOTENCY_MISMATCH",
                    ));
                }
                IdempotencyLockResult::InProgress => {
                    return Err(AppError::Conflict(
                        format!("Request '{}' is already in progress", key),
                        "IDEMPOTENCY_IN_PROGRESS",
                    ));
                }
                IdempotencyLockResult::Acquired => {}
            }
        }

        // Concurrency write lock serialization
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(AppError::from)?;

        // Validate warehouse & product
        if self.repo.find_warehouse_by_id(ctx, req.warehouse_id).await?.is_none() {
            return Err(AppError::NotFound(
                format!("Warehouse '{}' not found", req.warehouse_id),
                "NOT_FOUND",
            ));
        }
        let product = self.get_product(ctx, req.product_id).await?;

        let stock_item = self
            .repo
            .get_or_create_stock_item_tx(
                &mut tx,
                ctx,
                req.warehouse_id,
                req.product_id,
                product.reorder_threshold,
                product.cost_price,
            )
            .await?;

        let prev_q = stock_item.quantity_on_hand;
        let variance = req.actual_quantity - prev_q;

        self.repo
            .update_stock_item_tx(
                &mut tx,
                ctx,
                stock_item.id,
                req.actual_quantity,
                stock_item.average_cost,
            )
            .await?;

        let adj_num = self.repo.get_next_adjustment_number_tx(&mut tx, ctx).await?;
        let now = Utc::now();
        let adj_id = Uuid::new_v4();
        let mov_id = Uuid::new_v4();
        let reason = req
            .reason
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Physical inventory count adjustment".to_string());

        // Post balanced GL journal for physical stock count variance
        let mut journal_entry_id = None;
        let diff_val = variance.abs().saturating_mul(stock_item.average_cost.as_i64());
        if diff_val > 0 {
            let post_ctx = if ctx.role.can_post_ledger() {
                ctx.clone()
            } else {
                TenantContext {
                    tenant_id: ctx.tenant_id,
                    actor_id: ctx.actor_id,
                    role: Role::Owner,
                }
            };

            let journal_lines = if variance > 0 {
                vec![
                    PostJournalLineCommand {
                        account_code: "1300".to_string(),
                        debit: Rupiah::new(diff_val),
                        credit: Rupiah::ZERO,
                        memo: Some(format!("Penyesuaian Fisik Lebih {}", adj_num)),
                    },
                    PostJournalLineCommand {
                        account_code: "5000".to_string(),
                        debit: Rupiah::ZERO,
                        credit: Rupiah::new(diff_val),
                        memo: Some("Penyesuaian Selisih Persediaan Lebih".to_string()),
                    },
                ]
            } else {
                vec![
                    PostJournalLineCommand {
                        account_code: "5000".to_string(),
                        debit: Rupiah::new(diff_val),
                        credit: Rupiah::ZERO,
                        memo: Some("Penyesuaian Selisih Persediaan Kurang".to_string()),
                    },
                    PostJournalLineCommand {
                        account_code: "1300".to_string(),
                        debit: Rupiah::ZERO,
                        credit: Rupiah::new(diff_val),
                        memo: Some(format!("Penyesuaian Fisik Kurang {}", adj_num)),
                    },
                ]
            };

            let cmd = PostJournalEntryCommand {
                tenant_id: ctx.tenant_id,
                entry_date: now,
                description: format!("Penyesuaian Stok Fisik {} ({})", adj_num, product.name),
                source_type: "STOCK_ADJUSTMENT".to_string(),
                source_id: Some(adj_id),
                lines: journal_lines,
            };

            let j_entry = self.accounting_service.post_journal_command_tx(&mut tx, &post_ctx, cmd).await?;
            journal_entry_id = Uuid::parse_str(&j_entry.id).ok();
        }

        let adjustment = StockAdjustment {
            id: adj_id,
            tenant_id: ctx.tenant_id,
            adjustment_number: adj_num.clone(),
            warehouse_id: req.warehouse_id,
            product_id: req.product_id,
            variance_quantity: variance,
            previous_quantity: prev_q,
            new_quantity: req.actual_quantity,
            reason: reason.clone(),
            journal_entry_id,
            created_at: now,
            actor_id: Some(ctx.actor_id),
        };

        self.repo.insert_stock_adjustment_tx(&mut tx, ctx, &adjustment).await?;

        let movement = StockMovement {
            id: mov_id,
            tenant_id: ctx.tenant_id,
            movement_type: StockMovementType::Adjustment,
            product_id: req.product_id,
            source_warehouse_id: if variance < 0 { Some(req.warehouse_id) } else { None },
            destination_warehouse_id: if variance >= 0 { Some(req.warehouse_id) } else { None },
            quantity: if variance != 0 { variance.abs() } else { 1 },
            unit_cost: Some(stock_item.average_cost),
            reference_type: Some("ADJUSTMENT".to_string()),
            reference_id: Some(adj_id),
            batch_number: None,
            notes: Some(reason.clone()),
            created_at: now,
            actor_id: Some(ctx.actor_id),
        };

        self.repo.insert_stock_movement_tx(&mut tx, ctx, &movement).await?;

        // Transactional Outbox Event
        let draft = OutboxEventDraft::new(
            ctx.tenant_id,
            "StockAdjusted",
            "Inventory",
            adj_id.to_string(),
            json!({
                "adjustment_id": adj_id,
                "adjustment_number": adj_num,
                "warehouse_id": req.warehouse_id,
                "product_id": req.product_id,
                "previous_quantity": prev_q,
                "actual_quantity": req.actual_quantity,
                "variance": variance
            }),
        );
        self.outbox_repo.insert_tx(&mut tx, &draft).await.map_err(AppError::from)?;

        let result = StockAdjustmentResult {
            id: adj_id,
            adjustment_number: adj_num,
            warehouse_id: req.warehouse_id,
            product_id: req.product_id,
            previous_quantity: prev_q,
            actual_quantity: req.actual_quantity,
            new_quantity: req.actual_quantity,
            variance,
            variance_quantity: variance,
            reason,
            journal_entry_id,
            created_at: now,
        };

        // Cache response atomically if Idempotency-Key provided
        if let Some(key) = idempotency_key {
            let res_body = serde_json::to_string(&result)
                .map_err(|e| AppError::Internal(format!("Failed to serialize result: {}", e)))?;
            self.idempotency_repo
                .save_response_tx(&mut tx, &user_id, key, 200, &res_body)
                .await?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok((result, false))
    }

    // =========================================================================
    // Stock Movements History
    // =========================================================================

    pub async fn list_movements(
        &self,
        ctx: &TenantContext,
        product_id: Option<Uuid>,
        warehouse_id: Option<Uuid>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<StockMovement>, AppError> {
        let movements = self
            .repo
            .list_stock_movements(ctx, product_id, warehouse_id, limit, offset)
            .await?;
        Ok(movements)
    }

    // =========================================================================
    // Purchase Order Operations (R3, R4)
    // =========================================================================

    pub async fn create_purchase_order(
        &self,
        ctx: &TenantContext,
        req: CreatePurchaseOrderRequest,
    ) -> Result<PurchaseOrderWithItems, AppError> {
        // RBAC: Requires Owner, Administrator, or Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient role permissions to create purchase order".to_string(),
                    "FORBIDDEN",
                )
            })?;

        let supplier_name = req.supplier_name.trim();
        if supplier_name.is_empty() {
            return Err(AppError::BadRequest(
                "supplier_name, destination_warehouse_id, and items are required".to_string(),
                "MISSING_REQUIRED_FIELDS",
            ));
        }

        if req.items.is_empty() {
            return Err(AppError::BadRequest(
                "Purchase order requires at least one line item".to_string(),
                "EMPTY_LINE_ITEMS",
            ));
        }

        for item in &req.items {
            if item.quantity_ordered <= 0 {
                return Err(AppError::BadRequest(
                    "quantity_ordered must be a positive integer".to_string(),
                    "INVALID_QUANTITY",
                ));
            }
            if item.unit_cost.as_i64() < 0 {
                return Err(AppError::BadRequest(
                    "unit_cost must be non-negative integer".to_string(),
                    "INVALID_UNIT_COST",
                ));
            }
        }

        // Validate destination warehouse exists
        if self
            .repo
            .find_warehouse_by_id(ctx, req.destination_warehouse_id)
            .await?
            .is_none()
        {
            return Err(AppError::NotFound(
                format!(
                    "Destination warehouse '{}' not found",
                    req.destination_warehouse_id
                ),
                "NOT_FOUND",
            ));
        }

        // Validate products exist
        for item in &req.items {
            if self.repo.find_product_by_id(ctx, item.product_id).await?.is_none() {
                return Err(AppError::NotFound(
                    format!("Product '{}' not found", item.product_id),
                    "NOT_FOUND",
                ));
            }
        }

        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(AppError::from)?;

        let po_number = self.repo.get_next_po_number_tx(&mut tx, ctx).await?;
        let now = Utc::now();
        let po_id = Uuid::new_v4();

        let mut total_amount_i64: i64 = 0;
        for it in &req.items {
            let subtot = (it.quantity_ordered as i128) * (it.unit_cost.as_i64() as i128);
            total_amount_i64 = total_amount_i64
                .checked_add(subtot as i64)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "Total amount calculation overflowed".to_string(),
                        "OVERFLOW",
                    )
                })?;
        }
        let total_amount = Rupiah::new(total_amount_i64);

        let po = PurchaseOrder {
            id: po_id,
            tenant_id: ctx.tenant_id,
            po_number: po_number.clone(),
            supplier_name: supplier_name.to_string(),
            destination_warehouse_id: req.destination_warehouse_id,
            status: PurchaseOrderStatus::Draft,
            total_amount,
            notes: req.notes.clone(),
            created_at: now,
            updated_at: now,
        };

        let po_items: Vec<PurchaseOrderItem> = req
            .items
            .iter()
            .map(|it| {
                let subtot = (it.quantity_ordered as i128) * (it.unit_cost.as_i64() as i128);
                PurchaseOrderItem {
                    id: Uuid::new_v4(),
                    tenant_id: ctx.tenant_id,
                    purchase_order_id: po_id,
                    product_id: it.product_id,
                    quantity_ordered: it.quantity_ordered,
                    quantity_received: 0,
                    unit_cost: it.unit_cost,
                    total_cost: Rupiah::new(subtot as i64),
                }
            })
            .collect();

        self.repo
            .create_purchase_order_tx(&mut tx, ctx, &po, &po_items)
            .await?;
        tx.commit().await.map_err(AppError::from)?;

        Ok(PurchaseOrderWithItems {
            id: po.id,
            tenant_id: po.tenant_id,
            po_number: po.po_number,
            supplier_name: po.supplier_name,
            destination_warehouse_id: po.destination_warehouse_id,
            status: po.status,
            total_amount: po.total_amount,
            notes: po.notes,
            items: po_items,
            created_at: po.created_at,
            updated_at: po.updated_at,
        })
    }

    pub async fn get_purchase_order(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<PurchaseOrderWithItems, AppError> {
        let po = self.repo.get_purchase_order(ctx, id).await?;
        po.ok_or_else(|| {
            AppError::NotFound(
                format!("Purchase order '{}' not found", id),
                "NOT_FOUND",
            )
        })
    }

    pub async fn list_purchase_orders(
        &self,
        ctx: &TenantContext,
    ) -> Result<Vec<PurchaseOrderWithItems>, AppError> {
        self.repo
            .list_purchase_orders(ctx)
            .await
            .map_err(AppError::from)
    }

    pub async fn order_purchase_order(
        &self,
        ctx: &TenantContext,
        id: Uuid,
    ) -> Result<OrderPurchaseOrderResult, AppError> {
        // RBAC: Requires Owner, Administrator, or Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient role permissions to order purchase order".to_string(),
                    "FORBIDDEN",
                )
            })?;

        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(AppError::from)?;

        let po = self.repo.get_purchase_order_tx(&mut tx, ctx, id).await?;
        let po = match po {
            Some(p) => p,
            None => {
                return Err(AppError::NotFound(
                    format!("Purchase order '{}' not found", id),
                    "NOT_FOUND",
                ));
            }
        };

        if po.status != PurchaseOrderStatus::Draft {
            return Err(AppError::UnprocessableEntity(
                format!("Cannot order PO in status '{}'", po.status.as_str()),
                "PO_NOT_DRAFT",
            ));
        }

        let now = Utc::now();
        self.repo
            .update_purchase_order_status_tx(&mut tx, ctx, id, PurchaseOrderStatus::Ordered, now)
            .await?;

        tx.commit().await.map_err(AppError::from)?;

        Ok(OrderPurchaseOrderResult {
            id,
            status: PurchaseOrderStatus::Ordered,
            updated_at: now,
        })
    }

    pub async fn receive_purchase_order(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        req: ReceivePurchaseOrderRequest,
        idempotency_key: Option<&str>,
        payload_bytes: Option<&[u8]>,
    ) -> Result<(ReceivePurchaseOrderResult, bool), AppError> {
        // RBAC: Requires Owner, Administrator, or Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient role permissions to receive purchase order".to_string(),
                    "FORBIDDEN",
                )
            })?;

        // Idempotency check
        let user_id = ctx.actor_id_str();
        if let Some(key) = idempotency_key {
            let req_val = json!(req);
            let req_hash = Self::compute_payload_hash(payload_bytes, &req_val);

            match self
                .idempotency_repo
                .acquire_lock(&user_id, key, &req_hash, 86_400)
                .await?
            {
                IdempotencyLockResult::Cached { response_body, .. } => {
                    let cached_res: ReceivePurchaseOrderResult = serde_json::from_str(&response_body)
                        .map_err(|e| {
                            AppError::Internal(format!("Failed to parse cached response: {}", e))
                        })?;
                    return Ok((cached_res, true));
                }
                IdempotencyLockResult::MismatchedPayload => {
                    return Err(AppError::Conflict(
                        format!(
                            "Idempotency key '{}' was used with a different payload",
                            key
                        ),
                        "IDEMPOTENCY_MISMATCH",
                    ));
                }
                IdempotencyLockResult::InProgress => {
                    return Err(AppError::Conflict(
                        format!("Request '{}' is already in progress", key),
                        "IDEMPOTENCY_IN_PROGRESS",
                    ));
                }
                IdempotencyLockResult::Acquired => {}
            }
        }

        if req.items.is_empty() {
            return Err(AppError::BadRequest(
                "Goods receipt requires at least one received item".to_string(),
                "EMPTY_ITEMS",
            ));
        }

        for item in &req.items {
            if item.quantity_received <= 0 {
                return Err(AppError::BadRequest(
                    "quantity_received must be a positive integer".to_string(),
                    "INVALID_QUANTITY",
                ));
            }
        }

        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(AppError::from)?;

        let po = self.repo.get_purchase_order_tx(&mut tx, ctx, id).await?;
        let po = match po {
            Some(p) => p,
            None => {
                return Err(AppError::NotFound(
                    format!("Purchase order '{}' not found", id),
                    "NOT_FOUND",
                ));
            }
        };

        if po.status == PurchaseOrderStatus::Draft {
            return Err(AppError::UnprocessableEntity(
                "Cannot receive items on DRAFT purchase order; must be ORDERED".to_string(),
                "PO_NOT_ORDERED",
            ));
        }
        if po.status == PurchaseOrderStatus::Cancelled {
            return Err(AppError::UnprocessableEntity(
                "Cannot receive items on CANCELLED purchase order".to_string(),
                "PO_CANCELLED",
            ));
        }
        if po.status == PurchaseOrderStatus::Received {
            return Err(AppError::UnprocessableEntity(
                "Purchase order has already been fully received".to_string(),
                "PO_ALREADY_RECEIVED",
            ));
        }

        let mut po_items = po.items;

        // 1. Validate that all requested products belong to this purchase order
        for r_it in &req.items {
            if !po_items.iter().any(|l| l.product_id == r_it.product_id) {
                return Err(AppError::UnprocessableEntity(
                    format!(
                        "Product '{}' is not part of this purchase order",
                        r_it.product_id
                    ),
                    "INVALID_PRODUCT_LINE",
                ));
            }
        }

        // 2. Accumulate requested quantities per product_id to prevent intra-request over-receipt
        let mut req_qty_by_prod: HashMap<Uuid, i64> = HashMap::new();
        for r_it in &req.items {
            let entry = req_qty_by_prod.entry(r_it.product_id).or_insert(0);
            *entry = entry.checked_add(r_it.quantity_received).ok_or_else(|| {
                AppError::BadRequest(
                    "Received quantity calculation overflowed".to_string(),
                    "OVERFLOW",
                )
            })?;
        }

        // 3. Calculate total remaining quantity per product_id across all PO line items
        let mut po_remain_by_prod: HashMap<Uuid, i64> = HashMap::new();
        for line in &po_items {
            let entry = po_remain_by_prod.entry(line.product_id).or_insert(0);
            *entry = entry.checked_add(line.remaining_quantity()).ok_or_else(|| {
                AppError::BadRequest(
                    "Remaining quantity calculation overflowed".to_string(),
                    "OVERFLOW",
                )
            })?;
        }

        // 4. Validate that cumulative requested quantity does not exceed cumulative remaining quantity
        for (prod_id, total_req) in &req_qty_by_prod {
            let remain = po_remain_by_prod.get(prod_id).copied().unwrap_or(0);
            if *total_req > remain {
                return Err(AppError::UnprocessableEntity(
                    format!(
                        "Received quantity {} exceeds remaining ordered quantity {}",
                        total_req, remain
                    ),
                    "QUANTITY_EXCEEDS_ORDERED",
                ));
            }
        }

        let now = Utc::now();
        let mut total_receipt_value: i64 = 0;

        // 5. Apply receipt quantities across PO lines (FIFO per product) and update inventory
        for r_it in &req.items {
            let mut remaining_to_fill = r_it.quantity_received;
            let mut fallback_unit_cost = None;

            for line in po_items.iter_mut() {
                if line.product_id == r_it.product_id {
                    if fallback_unit_cost.is_none() {
                        fallback_unit_cost = Some(line.unit_cost);
                    }
                    let line_remain = line.remaining_quantity();
                    if line_remain > 0 && remaining_to_fill > 0 {
                        let alloc = std::cmp::min(remaining_to_fill, line_remain);
                        line.quantity_received += alloc;
                        remaining_to_fill -= alloc;

                        self.repo
                            .update_purchase_order_item_received_tx(
                                &mut tx,
                                ctx,
                                line.id,
                                line.quantity_received,
                            )
                            .await?;
                    }
                }
            }

            let unit_cost = r_it
                .unit_cost
                .or(fallback_unit_cost)
                .unwrap_or(Rupiah::ZERO);

            let batch_number = r_it
                .batch_number
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "BATCH-DEFAULT".to_string());

            let line_val = (r_it.quantity_received as i128) * (unit_cost.as_i64() as i128);
            total_receipt_value = total_receipt_value
                .checked_add(line_val as i64)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "Receipt value calculation overflowed".to_string(),
                        "OVERFLOW",
                    )
                })?;

            let product = self.get_product(ctx, r_it.product_id).await?;
            let stock_item = self
                .repo
                .get_or_create_stock_item_tx(
                    &mut tx,
                    ctx,
                    po.destination_warehouse_id,
                    r_it.product_id,
                    product.reorder_threshold,
                    unit_cost,
                )
                .await?;

            let prev_q = stock_item.quantity_on_hand;
            let prev_wac = stock_item.average_cost;
            let new_q = prev_q + r_it.quantity_received;
            let new_wac =
                calculate_weighted_average_cost(prev_q, prev_wac, r_it.quantity_received, unit_cost)?;

            self.repo
                .update_stock_item_tx(&mut tx, ctx, stock_item.id, new_q, new_wac)
                .await?;

            let mov_id = Uuid::new_v4();
            let movement = StockMovement {
                id: mov_id,
                tenant_id: ctx.tenant_id,
                movement_type: StockMovementType::Inbound,
                product_id: r_it.product_id,
                source_warehouse_id: None,
                destination_warehouse_id: Some(po.destination_warehouse_id),
                quantity: r_it.quantity_received,
                unit_cost: Some(unit_cost),
                reference_type: Some("PURCHASE_ORDER".to_string()),
                reference_id: Some(po.id),
                batch_number: Some(batch_number),
                notes: None,
                created_at: now,
                actor_id: Some(ctx.actor_id),
            };

            self.repo
                .insert_stock_movement_tx(&mut tx, ctx, &movement)
                .await?;
        }

        // Post balanced GL journal entry: Debit 1300 (Persediaan) / Credit 2000 (Utang Usaha)
        let mut journal_entry_id = None;
        if total_receipt_value > 0 {
            let post_ctx = if ctx.role.can_post_ledger() {
                ctx.clone()
            } else {
                TenantContext {
                    tenant_id: ctx.tenant_id,
                    actor_id: ctx.actor_id,
                    role: Role::Owner,
                }
            };

            let cmd = PostJournalEntryCommand {
                tenant_id: ctx.tenant_id,
                entry_date: now,
                description: format!("Penerimaan Barang Pesanan {}", po.po_number),
                source_type: "PURCHASE_ORDER_RECEIPT".to_string(),
                source_id: Some(po.id),
                lines: vec![
                    PostJournalLineCommand {
                        account_code: "1300".to_string(),
                        debit: Rupiah::new(total_receipt_value),
                        credit: Rupiah::ZERO,
                        memo: Some(format!("Persediaan Masuk PO {}", po.po_number)),
                    },
                    PostJournalLineCommand {
                        account_code: "2000".to_string(),
                        debit: Rupiah::ZERO,
                        credit: Rupiah::new(total_receipt_value),
                        memo: Some(format!("Utang Usaha PO {}", po.po_number)),
                    },
                ],
            };

            let j_entry = self
                .accounting_service
                .post_journal_command_tx(&mut tx, &post_ctx, cmd)
                .await?;
            journal_entry_id = Uuid::parse_str(&j_entry.id).ok();
        }

        // Transactional Outbox Event: StockReceived
        let draft = OutboxEventDraft::stock_received(
            ctx.tenant_id,
            "PurchaseOrder",
            po.id.to_string(),
            json!({
                "po_id": po.id,
                "po_number": po.po_number,
                "total_receipt_value": total_receipt_value,
                "journal_id": journal_entry_id,
            }),
        );
        self.outbox_repo
            .insert_tx(&mut tx, &draft)
            .await
            .map_err(AppError::from)?;

        let all_received = po_items
            .iter()
            .all(|l| l.quantity_received >= l.quantity_ordered);
        let new_status = if all_received {
            PurchaseOrderStatus::Received
        } else {
            PurchaseOrderStatus::PartiallyReceived
        };

        self.repo
            .update_purchase_order_status_tx(&mut tx, ctx, po.id, new_status, now)
            .await?;

        let result = ReceivePurchaseOrderResult {
            id: po.id,
            po_number: po.po_number,
            status: new_status,
            total_receipt_value: Rupiah::new(total_receipt_value),
            journal_entry_id,
            updated_at: now,
        };

        // Cache response atomically if Idempotency-Key provided
        if let Some(key) = idempotency_key {
            let res_body = serde_json::to_string(&result)
                .map_err(|e| AppError::Internal(format!("Failed to serialize result: {}", e)))?;
            self.idempotency_repo
                .save_response_tx(&mut tx, &user_id, key, 200, &res_body)
                .await?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok((result, false))
    }

    pub async fn cancel_purchase_order(
        &self,
        ctx: &TenantContext,
        id: Uuid,
        _req: CancelPurchaseOrderRequest,
    ) -> Result<CancelPurchaseOrderResult, AppError> {
        // RBAC: Requires Owner, Administrator, or Manager
        ctx.require_role(&[Role::Owner, Role::Administrator, Role::Manager])
            .map_err(|_| {
                AppError::Forbidden(
                    "Insufficient role permissions to cancel purchase order".to_string(),
                    "FORBIDDEN",
                )
            })?;

        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(AppError::from)?;

        let po = self.repo.get_purchase_order_tx(&mut tx, ctx, id).await?;
        let po = match po {
            Some(p) => p,
            None => {
                return Err(AppError::NotFound(
                    format!("Purchase order '{}' not found", id),
                    "NOT_FOUND",
                ));
            }
        };

        if po.status == PurchaseOrderStatus::PartiallyReceived
            || po.status == PurchaseOrderStatus::Received
        {
            return Err(AppError::UnprocessableEntity(
                format!(
                    "Cannot cancel purchase order in status '{}'",
                    po.status.as_str()
                ),
                "CANNOT_CANCEL_RECEIVED_PO",
            ));
        }

        if po.items.iter().any(|it| it.quantity_received > 0) {
            return Err(AppError::UnprocessableEntity(
                "Cannot cancel purchase order with received items".to_string(),
                "CANNOT_CANCEL_RECEIVED_PO",
            ));
        }

        let now = Utc::now();
        self.repo
            .update_purchase_order_status_tx(&mut tx, ctx, id, PurchaseOrderStatus::Cancelled, now)
            .await?;

        tx.commit().await.map_err(AppError::from)?;

        Ok(CancelPurchaseOrderResult {
            id,
            status: PurchaseOrderStatus::Cancelled,
            updated_at: now,
        })
    }
}

