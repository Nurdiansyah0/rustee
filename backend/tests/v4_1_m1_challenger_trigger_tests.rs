//! Adversarial Empirical Challenger Suite for Milestone 1
//!
//! Identity: teamwork_preview_challenger_m1_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Verify that `stock_items` strictly prohibits negative balances:
//!    CHECK (quantity_on_hand >= 0) and CHECK (quantity_reserved >= 0)
//!    abort inserts and updates with negative values.
//! 2. Verify that `stock_movements` and `stock_adjustments` immutability
//!    triggers abort any direct UPDATE or DELETE statements.
//! 3. Verify that Account 1300 cannot be deleted directly due to `trg_protect_system_accounts`.
//! 4. Stress test edge cases: boundary values, batch operations, upsert conflicts,
//!    custom non-system accounts deletion comparison, and invalid movement types.

use backend::domain::accounting::Account;
use backend::repository::accounting_repo::SqlxAccountingRepository;
use backend::repository::db::{init_pool, run_migrations, DbConfig};
use sqlx::SqlitePool;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_challenger_db() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m1_test.db");
    let config = DbConfig {
        database_url: format!("sqlite://{}?mode=rwc", db_path.to_str().unwrap()),
        max_connections: 5,
        min_connections: 1,
        busy_timeout_ms: 5000,
        acquire_timeout_secs: 5,
    };
    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");
    (pool, dir)
}

async fn seed_test_tenant(pool: &SqlitePool, tenant_id: &str) {
    sqlx::query(
        "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?1, 'Challenger Tenant', ?2, 'ACTIVE', 0, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(tenant_id)
    .bind(format!("slug-{}", tenant_id))
    .execute(pool)
    .await
    .expect("Failed to seed tenant");
}

async fn seed_warehouse_and_product(pool: &SqlitePool, tenant_id: &str, wh_id: &str, prod_id: &str) {
    sqlx::query(
        "INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at) VALUES (?1, ?2, 'WH-01', 'Main Warehouse', 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(wh_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("Failed to insert warehouse");

    sqlx::query(
        "INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at) VALUES (?1, ?2, 'SKU-000001', 'Widget A', 'PCS', 1000, 2000, 10, 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(prod_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("Failed to insert product");
}

#[tokio::test]
async fn challenge_stock_items_negative_on_hand_prevented_on_insert() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    let wh_id = &Uuid::new_v4().to_string();
    let prod_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;
    seed_warehouse_and_product(&pool, tenant_id, wh_id, prod_id).await;

    // Adversarial Case 1: Insert with quantity_on_hand = -1
    let stock_id_1 = &Uuid::new_v4().to_string();
    let err_neg_1 = sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at) VALUES (?1, ?2, ?3, ?4, -1, 0, 5, 'A-01', 1000, '2026-10-01T00:00:00Z')"
    )
    .bind(stock_id_1)
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .unwrap_err();

    let err_msg = err_neg_1.to_string().to_lowercase();
    assert!(
        err_msg.contains("check constraint failed"),
        "Expected CHECK constraint error for quantity_on_hand = -1, got: {err_msg}"
    );

    // Adversarial Case 2: Insert with extreme negative i64::MIN
    let stock_id_2 = &Uuid::new_v4().to_string();
    let err_min = sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 0, 5, 'A-01', 1000, '2026-10-01T00:00:00Z')"
    )
    .bind(stock_id_2)
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .bind(i64::MIN)
    .execute(&pool)
    .await
    .unwrap_err();

    let err_msg_min = err_min.to_string().to_lowercase();
    assert!(
        err_msg_min.contains("check constraint failed"),
        "Expected CHECK constraint error for quantity_on_hand = i64::MIN, got: {err_msg_min}"
    );
}

#[tokio::test]
async fn challenge_stock_items_negative_on_hand_prevented_on_update() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    let wh_id = &Uuid::new_v4().to_string();
    let prod_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;
    seed_warehouse_and_product(&pool, tenant_id, wh_id, prod_id).await;

    // Seed a valid stock item with 10 on hand
    let stock_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at) VALUES (?1, ?2, ?3, ?4, 10, 0, 5, 'A-01', 1000, '2026-10-01T00:00:00Z')"
    )
    .bind(stock_id)
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .expect("Failed to insert valid stock item");

    // Adversarial Case 1: Direct update setting negative quantity
    let err_direct = sqlx::query("UPDATE stock_items SET quantity_on_hand = -1 WHERE id = ?1")
        .bind(stock_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_direct.to_string().to_lowercase().contains("check constraint failed"),
        "Expected CHECK constraint on negative update"
    );

    // Adversarial Case 2: Arithmetic deduction driving balance negative (10 - 25 = -15)
    let err_deduct = sqlx::query("UPDATE stock_items SET quantity_on_hand = quantity_on_hand - 25 WHERE id = ?1")
        .bind(stock_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_deduct.to_string().to_lowercase().contains("check constraint failed"),
        "Expected CHECK constraint on negative deduction underflow"
    );

    // Verify stock_items was untouched
    let current_qty: i64 = sqlx::query_scalar("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(stock_id)
        .fetch_one(&pool)
        .await
        .expect("Query failed");
    assert_eq!(current_qty, 10, "Stock balance must remain unchanged after failed updates");
}

#[tokio::test]
async fn challenge_stock_items_negative_reserved_and_boundary_checks() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    let wh_id = &Uuid::new_v4().to_string();
    let prod_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;
    seed_warehouse_and_product(&pool, tenant_id, wh_id, prod_id).await;

    // Adversarial Case 1: Insert negative quantity_reserved
    let stock_id = &Uuid::new_v4().to_string();
    let err_res = sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at) VALUES (?1, ?2, ?3, ?4, 10, -5, 5, 'A-01', 1000, '2026-10-01T00:00:00Z')"
    )
    .bind(stock_id)
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_res.to_string().to_lowercase().contains("check constraint failed"));

    // Adversarial Case 2: Insert negative average_cost
    let err_cost = sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at) VALUES (?1, ?2, ?3, ?4, 10, 0, 5, 'A-01', -50, '2026-10-01T00:00:00Z')"
    )
    .bind(stock_id)
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_cost.to_string().to_lowercase().contains("check constraint failed"));

    // Boundary Case: Zero values are valid
    sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at) VALUES (?1, ?2, ?3, ?4, 0, 0, 0, 'ZERO-BIN', 0, '2026-10-01T00:00:00Z')"
    )
    .bind(stock_id)
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .expect("Zero boundary values must be allowed");

    // Adversarial Case 3: Update quantity_reserved to negative
    let err_res_upd = sqlx::query("UPDATE stock_items SET quantity_reserved = -1 WHERE id = ?1")
        .bind(stock_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(err_res_upd.to_string().to_lowercase().contains("check constraint failed"));

    // Adversarial Case 4: Upsert attempting to force negative quantity_on_hand on conflict
    let err_upsert = sqlx::query(
        "INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at) VALUES (?1, ?2, ?3, ?4, 5, 0, 0, 'BIN', 0, '2026-10-01T00:00:00Z') ON CONFLICT(tenant_id, warehouse_id, product_id) DO UPDATE SET quantity_on_hand = -99"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_upsert.to_string().to_lowercase().contains("check constraint failed"));
}

#[tokio::test]
async fn challenge_stock_movements_immutability_triggers() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    let wh_id = &Uuid::new_v4().to_string();
    let prod_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;
    seed_warehouse_and_product(&pool, tenant_id, wh_id, prod_id).await;

    let mov_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, source_warehouse_id, destination_warehouse_id, quantity, unit_cost, reference_type, reference_id, batch_number, notes, created_at, actor_id) VALUES (?1, ?2, 'INBOUND', ?3, NULL, ?4, 100, 5000, 'PO', 'po_1', 'BATCH-123', 'Audit check', '2026-10-01T00:00:00Z', 'user_1')"
    )
    .bind(mov_id)
    .bind(tenant_id)
    .bind(prod_id)
    .bind(wh_id)
    .execute(&pool)
    .await
    .expect("Failed to insert stock movement");

    // Attack 1: Update quantity
    let err_upd_qty = sqlx::query("UPDATE stock_movements SET quantity = 200 WHERE id = ?1")
        .bind(mov_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_upd_qty.to_string().contains("Stock movements are immutable and cannot be updated"),
        "Unexpected error: {}", err_upd_qty
    );

    // Attack 2: Update notes/metadata
    let err_upd_notes = sqlx::query("UPDATE stock_movements SET notes = 'tampered' WHERE id = ?1")
        .bind(mov_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_upd_notes.to_string().contains("Stock movements are immutable and cannot be updated"),
        "Unexpected error: {}", err_upd_notes
    );

    // Attack 3: No-op update (SET quantity = quantity)
    let err_noop = sqlx::query("UPDATE stock_movements SET quantity = quantity WHERE id = ?1")
        .bind(mov_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_noop.to_string().contains("Stock movements are immutable and cannot be updated"),
        "Unexpected error: {}", err_noop
    );

    // Attack 4: Single row DELETE
    let err_del_single = sqlx::query("DELETE FROM stock_movements WHERE id = ?1")
        .bind(mov_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_del_single.to_string().contains("Stock movements are immutable and cannot be deleted"),
        "Unexpected error: {}", err_del_single
    );

    // Attack 5: Mass batch DELETE
    let err_del_batch = sqlx::query("DELETE FROM stock_movements WHERE tenant_id = ?1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_del_batch.to_string().contains("Stock movements are immutable and cannot be deleted"),
        "Unexpected error: {}", err_del_batch
    );

    // Verify row still exists and is completely intact
    let qty: i64 = sqlx::query_scalar("SELECT quantity FROM stock_movements WHERE id = ?1")
        .bind(mov_id)
        .fetch_one(&pool)
        .await
        .expect("Query failed");
    assert_eq!(qty, 100, "Movement record must remain completely intact and unaltered");
}

#[tokio::test]
async fn challenge_stock_adjustments_immutability_triggers() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    let wh_id = &Uuid::new_v4().to_string();
    let prod_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;
    seed_warehouse_and_product(&pool, tenant_id, wh_id, prod_id).await;

    let adj_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO stock_adjustments (id, tenant_id, adjustment_number, warehouse_id, product_id, variance_quantity, previous_quantity, new_quantity, reason, journal_entry_id, created_at, actor_id) VALUES (?1, ?2, 'ADJ-2026-000001', ?3, ?4, -5, 50, 45, 'Stock damage', NULL, '2026-10-01T00:00:00Z', 'user_1')"
    )
    .bind(adj_id)
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .expect("Failed to insert stock adjustment");

    // Attack 1: Update variance_quantity
    let err_upd_var = sqlx::query("UPDATE stock_adjustments SET variance_quantity = 0 WHERE id = ?1")
        .bind(adj_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_upd_var.to_string().contains("Stock adjustments are immutable and cannot be updated"),
        "Unexpected error: {}", err_upd_var
    );

    // Attack 2: Update reason
    let err_upd_reason = sqlx::query("UPDATE stock_adjustments SET reason = 'Tampered reason' WHERE id = ?1")
        .bind(adj_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_upd_reason.to_string().contains("Stock adjustments are immutable and cannot be updated"),
        "Unexpected error: {}", err_upd_reason
    );

    // Attack 3: Single row DELETE
    let err_del_single = sqlx::query("DELETE FROM stock_adjustments WHERE id = ?1")
        .bind(adj_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_del_single.to_string().contains("Stock adjustments are immutable and cannot be deleted"),
        "Unexpected error: {}", err_del_single
    );

    // Attack 4: Batch DELETE
    let err_del_batch = sqlx::query("DELETE FROM stock_adjustments WHERE tenant_id = ?1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        err_del_batch.to_string().contains("Stock adjustments are immutable and cannot be deleted"),
        "Unexpected error: {}", err_del_batch
    );

    // Verify row still exists
    let variance: i64 = sqlx::query_scalar("SELECT variance_quantity FROM stock_adjustments WHERE id = ?1")
        .bind(adj_id)
        .fetch_one(&pool)
        .await
        .expect("Query failed");
    assert_eq!(variance, -5);
}

#[tokio::test]
async fn challenge_account_1300_system_protection_trigger() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;

    // Seed default accounts including 1300
    let mut tx = pool.begin().await.expect("Failed to begin transaction");
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx, tenant_id)
        .await
        .expect("Failed to seed default accounts");
    tx.commit().await.expect("Failed to commit");

    // 1. Verify Account 1300 exists with is_system = 1
    let (name, is_system): (String, i64) = sqlx::query_as(
        "SELECT name, is_system FROM chart_of_accounts WHERE tenant_id = ?1 AND code = '1300'"
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Account 1300 must exist");

    assert_eq!(name, "Persediaan Barang Dagang");
    assert_eq!(is_system, 1, "Account 1300 must be marked as system account");
    assert!(Account::is_system_code("1300"), "Domain must recognize 1300 as system code");

    // 2. Adversarial Attack: Attempt direct DELETE on Account 1300
    let err_del_1300 = sqlx::query(
        "DELETE FROM chart_of_accounts WHERE tenant_id = ?1 AND code = '1300'"
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap_err();

    assert!(
        err_del_1300.to_string().contains("System accounts are protected and cannot be deleted"),
        "Unexpected error: {}", err_del_1300
    );

    // 3. Adversarial Attack: Attempt batch DELETE across all accounts
    let err_del_batch = sqlx::query(
        "DELETE FROM chart_of_accounts WHERE tenant_id = ?1"
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap_err();

    assert!(
        err_del_batch.to_string().contains("System accounts are protected and cannot be deleted"),
        "Unexpected error: {}", err_del_batch
    );

    // 4. Differential Comparison: Insert a custom non-system account and verify deletion succeeds
    let custom_acc_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO chart_of_accounts (id, tenant_id, code, name, account_type, is_system, created_at, updated_at) VALUES (?1, ?2, '1399', 'Custom Temp Stock', 'asset', 0, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(custom_acc_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to insert custom account");

    let del_custom = sqlx::query(
        "DELETE FROM chart_of_accounts WHERE tenant_id = ?1 AND code = '1399'"
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("Custom non-system account deletion must succeed");
    assert_eq!(del_custom.rows_affected(), 1);

    // 5. Verify Account 1300 is still safe and intact
    let count_1300: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM chart_of_accounts WHERE tenant_id = ?1 AND code = '1300'"
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Query failed");
    assert_eq!(count_1300, 1, "Account 1300 must still exist intact");
}

#[tokio::test]
async fn challenge_additional_schema_invariants() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    let wh_id = &Uuid::new_v4().to_string();
    let prod_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;
    seed_warehouse_and_product(&pool, tenant_id, wh_id, prod_id).await;

    // 1. Stock movements: quantity must be strictly > 0
    let err_zero_qty = sqlx::query(
        "INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, source_warehouse_id, destination_warehouse_id, quantity, unit_cost, reference_type, reference_id, batch_number, notes, created_at, actor_id) VALUES (?1, ?2, 'INBOUND', ?3, NULL, ?4, 0, 5000, 'PO', 'po_1', 'BATCH-123', 'Audit check', '2026-10-01T00:00:00Z', 'user_1')"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(prod_id)
    .bind(wh_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_zero_qty.to_string().to_lowercase().contains("check constraint failed"));

    // 2. Stock movements: movement_type must be in ('INBOUND', 'OUTBOUND', 'TRANSFER', 'ADJUSTMENT')
    let err_invalid_type = sqlx::query(
        "INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, source_warehouse_id, destination_warehouse_id, quantity, unit_cost, reference_type, reference_id, batch_number, notes, created_at, actor_id) VALUES (?1, ?2, 'HACK', ?3, NULL, ?4, 10, 5000, 'PO', 'po_1', 'BATCH-123', 'Audit check', '2026-10-01T00:00:00Z', 'user_1')"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(prod_id)
    .bind(wh_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_invalid_type.to_string().to_lowercase().contains("check constraint failed"));

    // 3. Purchase order items: quantity_ordered must be > 0
    let po_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO purchase_orders (id, tenant_id, po_number, supplier_name, destination_warehouse_id, status, total_amount, notes, created_at, updated_at) VALUES (?1, ?2, 'PO-2026-000001', 'Supplier 1', ?3, 'DRAFT', 0, NULL, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(po_id)
    .bind(tenant_id)
    .bind(wh_id)
    .execute(&pool)
    .await
    .expect("Failed to insert purchase order");

    let err_poi_zero = sqlx::query(
        "INSERT INTO purchase_order_items (id, tenant_id, purchase_order_id, product_id, quantity_ordered, quantity_received, unit_cost, total_cost) VALUES (?1, ?2, ?3, ?4, 0, 0, 1000, 0)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(po_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_poi_zero.to_string().to_lowercase().contains("check constraint failed"));
}

#[tokio::test]
async fn challenge_foreign_key_and_trigger_interactions() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    let wh_id = &Uuid::new_v4().to_string();
    let prod_id = &Uuid::new_v4().to_string();

    seed_test_tenant(&pool, tenant_id).await;
    seed_warehouse_and_product(&pool, tenant_id, wh_id, prod_id).await;

    let mov_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO stock_movements (id, tenant_id, movement_type, product_id, source_warehouse_id, destination_warehouse_id, quantity, unit_cost, reference_type, reference_id, batch_number, notes, created_at, actor_id) VALUES (?1, ?2, 'INBOUND', ?3, NULL, ?4, 50, 1000, 'PO', 'po_1', 'B-1', 'Notes', '2026-10-01T00:00:00Z', 'user_1')"
    )
    .bind(mov_id)
    .bind(tenant_id)
    .bind(prod_id)
    .bind(wh_id)
    .execute(&pool)
    .await
    .expect("Failed to insert stock movement");

    // In SQLite, deleting a warehouse triggers ON DELETE SET NULL on stock_movements,
    // which SQLite executes as an UPDATE on stock_movements.
    // Because trg_stock_movements_prevent_update forbids ANY UPDATE on stock_movements,
    // the warehouse deletion is strictly blocked by the immutability trigger!
    let del_wh_err = sqlx::query("DELETE FROM warehouses WHERE id = ?1")
        .bind(wh_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        del_wh_err.to_string().contains("Stock movements are immutable and cannot be updated"),
        "Deleting warehouse must be blocked by immutability trigger because of SET NULL update action: {}", del_wh_err
    );

    // Similarly, deleting a product triggers ON DELETE CASCADE on stock_movements.
    // SQLite's CASCADE executes a DELETE on stock_movements, which is caught by trg_stock_movements_prevent_delete!
    let del_prod_err = sqlx::query("DELETE FROM products WHERE id = ?1")
        .bind(prod_id)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        del_prod_err.to_string().contains("Stock movements are immutable and cannot be deleted"),
        "Deleting product must be blocked by immutability trigger because of CASCADE delete action: {}", del_prod_err
    );
}

#[tokio::test]
async fn challenge_products_and_po_constraints() {
    let (pool, _dir) = setup_challenger_db().await;
    let tenant_id = &Uuid::new_v4().to_string();
    seed_test_tenant(&pool, tenant_id).await;

    // 1. Negative cost_price on product rejected
    let err_cost = sqlx::query(
        "INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at) VALUES (?1, ?2, 'SKU-000002', 'Widget B', 'PCS', -500, 2000, 10, 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_cost.to_string().to_lowercase().contains("check constraint failed"));

    // 2. Negative sale_price on product rejected
    let err_sale = sqlx::query(
        "INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at) VALUES (?1, ?2, 'SKU-000003', 'Widget C', 'PCS', 500, -2000, 10, 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_sale.to_string().to_lowercase().contains("check constraint failed"));

    // 3. Invalid PO status rejected
    let wh_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at) VALUES (?1, ?2, 'WH-02', 'Secondary Warehouse', 0, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(wh_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to insert warehouse");

    let err_po_status = sqlx::query(
        "INSERT INTO purchase_orders (id, tenant_id, po_number, supplier_name, destination_warehouse_id, status, total_amount, notes, created_at, updated_at) VALUES (?1, ?2, 'PO-2026-000002', 'Supplier 2', ?3, 'INVALID_STATUS', 0, NULL, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(wh_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_po_status.to_string().to_lowercase().contains("check constraint failed"));

    // 4. Stock adjustments negative previous_quantity rejected
    let prod_id = &Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at) VALUES (?1, ?2, 'SKU-000004', 'Widget D', 'PCS', 1000, 2000, 10, 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')"
    )
    .bind(prod_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("Failed to insert product");

    let err_adj_prev = sqlx::query(
        "INSERT INTO stock_adjustments (id, tenant_id, adjustment_number, warehouse_id, product_id, variance_quantity, previous_quantity, new_quantity, reason, journal_entry_id, created_at, actor_id) VALUES (?1, ?2, 'ADJ-2026-000002', ?3, ?4, -5, -1, 45, 'Reason', NULL, '2026-10-01T00:00:00Z', 'user_1')"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_adj_prev.to_string().to_lowercase().contains("check constraint failed"));

    // 5. Stock adjustments negative new_quantity rejected
    let err_adj_new = sqlx::query(
        "INSERT INTO stock_adjustments (id, tenant_id, adjustment_number, warehouse_id, product_id, variance_quantity, previous_quantity, new_quantity, reason, journal_entry_id, created_at, actor_id) VALUES (?1, ?2, 'ADJ-2026-000003', ?3, ?4, -50, 10, -40, 'Reason', NULL, '2026-10-01T00:00:00Z', 'user_1')"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(tenant_id)
    .bind(wh_id)
    .bind(prod_id)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err_adj_new.to_string().to_lowercase().contains("check constraint failed"));
}
