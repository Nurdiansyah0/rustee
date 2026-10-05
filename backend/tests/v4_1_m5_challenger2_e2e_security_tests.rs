//! Milestone 5 Challenger 2 Adversarial Test Suite
//! Tier 5 — End-to-End Workflow & Security Hardening (Features F33-F42 & Core Security).
//!
//! Agent: teamwork_preview_challenger_m5_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Complete end-to-end multi-module workflows:
//!    - Workflow 1: PO Procurement -> Inbound Goods Receipt -> WAC Update -> Stock Increase -> GL Inbound Journal (1300/2000) -> Outbox Event (`StockReceived`).
//!    - Workflow 2: Sales Fulfillment -> Stock Decrease -> COGS Journal (5000/1300) -> WAC Invariant Enforcement.
//!    - Workflow 3: Warehouse Transfer -> Atomic Decrement/Increment -> Outbox Event (`StockTransferred`).
//!    - Workflow 4: Physical Count Opname Adjustment -> Variance Reconciliation -> GL Journal -> Outbox Event (`StockAdjusted`).
//! 2. Adversarial security boundaries and tenant isolation:
//!    - Security 1: Cross-tenant lookups strictly return RFC 7807 HTTP 404 (preventing entity existence enumeration).
//!    - Security 2: Tenant-internal unauthorized staff role actions return HTTP 403 Forbidden.
//!    - Security 3: Personal workspace isolation strictly prevents capability surfacing or access.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::tenant::Role;
use backend::repository::{
    account_repo::SqlxAccountRepository,
    accounting_repo::SqlxAccountingRepository,
    audit_repo::SqlxAuditRepository,
    category_repo::SqlxCategoryRepository,
    db::{init_pool, run_migrations, DbConfig},
    idempotency_repo::SqlxIdempotencyRepository,
    subscription_repo::SqlxSubscriptionRepository,
    tenant_repo::{
        MembershipRepository, NewMembership, NewTenant, SqlxMembershipRepository,
        SqlxTenantRepository, TenantRepository,
    },
    transaction_repo::SqlxTransactionRepository,
    user_repo::{NewUser, SqlxUserRepository, UserRepository},
};
use backend::service::{
    auth_service::AuthService,
    jwt::JwtEngine,
    ledger_service::LedgerService,
    payment_service::{PaymentConfig, PaymentService},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct Challenger2Harness {
    app: axum::Router,
    pool: SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    staff_token: String,
    tenant_id: String,
    personal_tenant_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_m5_challenger2_harness() -> Challenger2Harness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger2_m5_adversarial_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 30,
        min_connections: 1,
        busy_timeout_ms: 10_000,
        acquire_timeout_secs: 15,
    };

    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool).await.expect("Failed to run migrations");

    let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));
    let category_repo = Arc::new(SqlxCategoryRepository::new(pool.clone()));
    let audit_repo = Arc::new(SqlxAuditRepository::new(pool.clone()));
    let idempotency_repo = Arc::new(SqlxIdempotencyRepository::new(pool.clone()));
    let transaction_repo = Arc::new(SqlxTransactionRepository::new(pool.clone()));
    let subscription_repo = Arc::new(SqlxSubscriptionRepository::new(pool.clone()));

    let jwt_secret = "m5_challenger2_secret_key_1234567890_super_secure_entropy_key";
    let jwt_engine = Arc::new(JwtEngine::new(jwt_secret, 3600));

    let crypto_service = Arc::new(
        backend::service::crypto::CryptoService::new(
            backend::service::crypto::Argon2Config::fast_for_testing(),
        )
        .unwrap(),
    );

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo.clone(),
        crypto_service,
        jwt_engine.clone(),
    ));

    let ledger_service = Arc::new(LedgerService::new(
        pool.clone(),
        transaction_repo,
        idempotency_repo,
    ));

    let payment_service = Arc::new(
        PaymentService::new(
            PaymentConfig::default(),
            subscription_repo,
            user_repo.clone(),
            audit_repo,
        )
        .with_pool(pool.clone()),
    );

    let auth_state = AuthState {
        auth_service,
        secure_cookie: false,
    };

    let state = AppState {
        auth_state,
        account_repo,
        category_repo,
        user_preferences_repo: Arc::new(backend::repository::SqlxUserPreferencesRepository::new(
            pool.clone(),
        )),
        ledger_service,
        payment_service,
        tenant_service: Arc::new(
            backend::service::tenant_service::TenantService::new_with_pool(pool.clone()),
        ),
        tenant_repo: Arc::new(backend::repository::tenant_repo::SqlxTenantRepository::new(
            pool.clone(),
        )),
        pool: pool.clone(),
        rate_limiter: Arc::default(),
    };

    let app = create_app(state);

    // Tenant A Owner
    let owner_a_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_a_id.clone(),
            email: "m5_c2_owner_a@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "M5 C2 Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_a_id, "m5_c2_owner_a@test.com", "user", "premium")
        .unwrap();

    let tenant_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_id.clone(),
            name: "M5 C2 Tenant Alpha".to_string(),
            slug: "m5-c2-tenant-alpha".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_a = pool.begin().await.unwrap();
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx_a, &tenant_id)
        .await
        .unwrap();
    tx_a.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_id.clone(),
            user_id: owner_a_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    // Tenant A Staff Member (unauthorized for admin/manager ops)
    let staff_a_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_a_id.clone(),
            email: "m5_c2_staff_a@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "M5 C2 Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_a_id, "m5_c2_staff_a@test.com", "user", "premium")
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_id.clone(),
            user_id: staff_a_id.clone(),
            role: Role::Staff,
        })
        .await
        .unwrap();

    // Personal Workspace for User A
    let personal_tenant_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: personal_tenant_id.clone(),
            name: "Personal Workspace".to_string(),
            slug: "personal-workspace-user-c2".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: true,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: personal_tenant_id.clone(),
            user_id: owner_a_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    // Tenant B Owner (for cross-tenant checks)
    let owner_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_b_id.clone(),
            email: "m5_c2_owner_b@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "M5 C2 Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&owner_b_id, "m5_c2_owner_b@test.com", "user", "premium")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "M5 C2 Tenant Beta".to_string(),
            slug: "m5-c2-tenant-beta".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_b = pool.begin().await.unwrap();
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx_b, &tenant_b_id)
        .await
        .unwrap();
    tx_b.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: owner_b_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    Challenger2Harness {
        app,
        pool,
        _dir: dir,
        owner_token,
        staff_token,
        tenant_id,
        personal_tenant_id,
        tenant_b_id,
        tenant_b_token,
    }
}

async fn send_req(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: &str,
    body: Option<Value>,
    extra_headers: Option<HeaderMap>,
) -> (StatusCode, Value, HeaderMap) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id);

    if body.is_some() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }

    if let Some(headers) = extra_headers {
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }
    }

    let body_bytes = match body {
        Some(val) => Body::from(serde_json::to_vec(&val).unwrap()),
        None => Body::empty(),
    };

    let req = builder.body(body_bytes).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();

    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let resp_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json_val = if resp_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&resp_bytes).unwrap_or(Value::Null)
    };

    (status, json_val, resp_headers)
}

// =========================================================================
// TEST 1: Workflow 1 — PO Procurement -> Inbound Goods Receipt -> WAC Update
//         -> Stock Increase -> GL Inbound Journal (1300/2000) -> Outbox Event (StockReceived)
// =========================================================================
#[tokio::test]
async fn test_challenge_e2e_workflow_1_po_procurement_to_stock_receipt_wac_gl_outbox() {
    let h = setup_m5_challenger2_harness().await;

    // 1. Create Warehouse WH-CENTRAL
    let (st_wh, wh_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "code": "WH-CENTRAL",
            "name": "Central Warehouse",
            "is_default": true
        })),
        None,
    )
    .await;
    assert_eq!(st_wh, StatusCode::CREATED);
    let wh_id = wh_res["id"].as_str().unwrap().to_string();

    // 2. Create Product P-WIDGET
    let (st_prod, prod_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "name": "Widget Premium",
            "unit": "PCS",
            "cost_price": 50_000,
            "sale_price": 100_000,
            "reorder_threshold": 5
        })),
        None,
    )
    .await;
    assert_eq!(st_prod, StatusCode::CREATED);
    let prod_id = prod_res["id"].as_str().unwrap().to_string();

    // 3. Create PO with 10 units @ Rp 60,000 = Rp 600,000 total
    let (st_po, po_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "supplier_name": "Supplier Utama Perkasa",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 10,
                    "unit_cost": 60_000
                }
            ],
            "notes": "PO Workflow Empirical Verification"
        })),
        None,
    )
    .await;
    assert_eq!(st_po, StatusCode::CREATED);
    let po_id = po_res["id"].as_str().unwrap().to_string();
    assert_eq!(po_res["status"], "DRAFT");
    assert_eq!(po_res["total_amount"], 600_000);

    // 4. Transition PO from DRAFT to ORDERED
    let (st_order, order_res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &h.owner_token,
        &h.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(st_order, StatusCode::OK);
    assert_eq!(order_res["status"], "ORDERED");

    // 5. Staged Receipt 1: Receive 4 units @ Rp 60,000 = Rp 240,000
    let (st_rec1, rec1_res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 4,
                    "unit_cost": 60_000,
                    "batch_number": "BATCH-PO-01"
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(st_rec1, StatusCode::OK);
    assert_eq!(rec1_res["status"], "PARTIALLY_RECEIVED");

    // Check stock item after Receipt 1: quantity = 4, average_cost = 60,000
    let stock_item_1: (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, average_cost FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_item_1.0, 4, "Stock quantity on hand must be exactly 4");
    assert_eq!(stock_item_1.1, 60_000, "Initial WAC must be exactly Rp 60,000");

    // Check GL Inbound Journal 1: Debit 1300 / Credit 2000 for Rp 240,000
    let j_entry_1 = sqlx::query(
        "SELECT id FROM journal_entries WHERE tenant_id = ? AND source_type = 'PURCHASE_ORDER_RECEIPT' ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let j_id_1: String = j_entry_1.get("id");

    let sums_1: (i64, i64) = sqlx::query_as(
        "SELECT SUM(debit), SUM(credit) FROM journal_lines WHERE journal_id = ?",
    )
    .bind(&j_id_1)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(sums_1.0, 240_000, "Debit total must be 240,000");
    assert_eq!(sums_1.1, 240_000, "Credit total must be 240,000");

    let lines_1 = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ? ORDER BY debit DESC",
    )
    .bind(&j_id_1)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(lines_1.len(), 2);
    let debit_line = &lines_1[0];
    let credit_line = &lines_1[1];
    assert_eq!(debit_line.get::<String, _>("account_code"), "1300");
    assert_eq!(debit_line.get::<i64, _>("debit"), 240_000);
    assert_eq!(debit_line.get::<i64, _>("credit"), 0);
    assert_eq!(credit_line.get::<String, _>("account_code"), "2000");
    assert_eq!(credit_line.get::<i64, _>("debit"), 0);
    assert_eq!(credit_line.get::<i64, _>("credit"), 240_000);

    // Check Outbox Event 1: StockReceived
    let outbox_1 = sqlx::query(
        "SELECT event_type, aggregate_type, payload_json FROM outbox_events WHERE tenant_id = ? AND event_type = 'StockReceived' ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(outbox_1.get::<String, _>("event_type"), "StockReceived");
    assert_eq!(outbox_1.get::<String, _>("aggregate_type"), "PurchaseOrder");
    let payload_1: Value = serde_json::from_str(&outbox_1.get::<String, _>("payload_json")).unwrap();
    assert_eq!(payload_1["total_receipt_value"], 240_000);
    assert_eq!(payload_1["po_id"], po_id);

    // 6. Staged Receipt 2: Receive remaining 6 units @ Rp 70,000 = Rp 420,000
    // Expected new WAC: (4 * 60,000 + 6 * 70,000) / 10 = (240,000 + 420,000) / 10 = 66,000
    let (st_rec2, rec2_res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 6,
                    "unit_cost": 70_000,
                    "batch_number": "BATCH-PO-02"
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(st_rec2, StatusCode::OK);
    assert_eq!(rec2_res["status"], "RECEIVED", "PO must transition to RECEIVED on completion");

    // Check stock item after Receipt 2: quantity = 10, average_cost = 66,000
    let stock_item_2: (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, average_cost FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_item_2.0, 10, "Stock quantity on hand must be exactly 10");
    assert_eq!(stock_item_2.1, 66_000, "WAC must recalculate to exactly Rp 66,000 via integer math");

    // Check GL Inbound Journal 2: Debit 1300 / Credit 2000 for Rp 420,000
    let j_entries = sqlx::query(
        "SELECT id FROM journal_entries WHERE tenant_id = ? AND source_type = 'PURCHASE_ORDER_RECEIPT' ORDER BY created_at ASC",
    )
    .bind(&h.tenant_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(j_entries.len(), 2, "Must be exactly 2 GL journal entries from 2 goods receipts");
    let j_id_2: String = j_entries[1].get("id");
    let sums_2: (i64, i64) = sqlx::query_as(
        "SELECT SUM(debit), SUM(credit) FROM journal_lines WHERE journal_id = ?",
    )
    .bind(&j_id_2)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(sums_2.0, 420_000);
    assert_eq!(sums_2.1, 420_000);

    // 7. Negative Boundary: Attempt further receipt on already RECEIVED PO -> HTTP 422
    let (st_rec_extra, rec_extra_res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 1,
                    "unit_cost": 70_000
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(st_rec_extra, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(rec_extra_res["code"], "PO_ALREADY_RECEIVED");
}

// =========================================================================
// TEST 2: Workflow 2 — Sales Fulfillment -> Stock Decrease -> COGS Journal
//         (Debit 5000 / Credit 1300) -> WAC Invariant Enforcement
// =========================================================================
#[tokio::test]
async fn test_challenge_e2e_workflow_2_sales_fulfillment_stock_decrease_cogs_gl_wac_invariant() {
    let h = setup_m5_challenger2_harness().await;

    // 1. Setup warehouse and product with 10 units @ WAC Rp 66,000
    let (_, wh_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({ "code": "WH-SALES-1", "name": "Sales Warehouse", "is_default": true })),
        None,
    )
    .await;
    let wh_id = wh_res["id"].as_str().unwrap().to_string();

    let (_, prod_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "name": "Barang Dagangan",
            "unit": "PCS",
            "cost_price": 66_000,
            "sale_price": 120_000,
            "reorder_threshold": 5
        })),
        None,
    )
    .await;
    let prod_id = prod_res["id"].as_str().unwrap().to_string();

    // Inbound 10 units @ Rp 66,000
    let (st_in, _, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10,
            "unit_cost": 66_000,
            "notes": "Initial Stock for Sales"
        })),
        None,
    )
    .await;
    assert_eq!(st_in, StatusCode::CREATED);

    // Verify stock before fulfillment: 10 units @ Rp 66,000
    let stock_before: (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, average_cost FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_before.0, 10);
    assert_eq!(stock_before.1, 66_000);

    // 2. Perform Sales Fulfillment / Outbound Movement: deduct 3 units
    // Expected COGS: 3 units * Rp 66,000 = Rp 198,000
    let (st_out, out_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 3,
            "notes": "Sales fulfillment order INV-2026-001"
        })),
        None,
    )
    .await;
    assert_eq!(st_out, StatusCode::CREATED);
    assert_eq!(out_res["movement_type"], "OUTBOUND");
    assert_eq!(out_res["quantity"], 3);

    // 3. WAC Invariant Enforcement: Outbound sales movement must NOT alter unit WAC!
    let stock_after: (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, average_cost FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_after.0, 7, "Stock must decrease from 10 to 7");
    assert_eq!(stock_after.1, 66_000, "WAC invariant: unit cost must remain strictly Rp 66,000 on outbound");

    // 4. Verify COGS GL Journal: Debit 5000 (Beban Pokok) / Credit 1300 (Persediaan) for Rp 198,000
    let cogs_journal = sqlx::query(
        "SELECT id FROM journal_entries WHERE tenant_id = ? AND source_type = 'INVENTORY_OUTBOUND' LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let cogs_j_id: String = cogs_journal.get("id");

    let cogs_sums: (i64, i64) = sqlx::query_as(
        "SELECT SUM(debit), SUM(credit) FROM journal_lines WHERE journal_id = ?",
    )
    .bind(&cogs_j_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(cogs_sums.0, 198_000);
    assert_eq!(cogs_sums.1, 198_000);

    let cogs_lines = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ? ORDER BY debit DESC",
    )
    .bind(&cogs_j_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(cogs_lines.len(), 2);
    assert_eq!(cogs_lines[0].get::<String, _>("account_code"), "5000", "Debit line must be Account 5000");
    assert_eq!(cogs_lines[0].get::<i64, _>("debit"), 198_000);
    assert_eq!(cogs_lines[0].get::<i64, _>("credit"), 0);
    assert_eq!(cogs_lines[1].get::<String, _>("account_code"), "1300", "Credit line must be Account 1300");
    assert_eq!(cogs_lines[1].get::<i64, _>("debit"), 0);
    assert_eq!(cogs_lines[1].get::<i64, _>("credit"), 198_000);

    // 5. Verify Transactional Outbox Event: StockDeducted
    let outbox_out = sqlx::query(
        "SELECT event_type, aggregate_type, payload_json FROM outbox_events WHERE tenant_id = ? AND event_type = 'StockDeducted' LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(outbox_out.get::<String, _>("event_type"), "StockDeducted");
    assert_eq!(outbox_out.get::<String, _>("aggregate_type"), "Inventory");

    // 6. Adversarial overselling test: Attempt to deduct 8 units (only 7 on hand)
    let (st_oversell, err_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 8
        })),
        None,
    )
    .await;
    assert_eq!(st_oversell, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_res["code"], "INSUFFICIENT_STOCK");

    // Stock must remain strictly 7
    let stock_unchanged: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_unchanged.0, 7);

    // 7. Full exact balance exhaustion: Deduct exact remaining 7 units
    let (st_exact, _, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 7
        })),
        None,
    )
    .await;
    assert_eq!(st_exact, StatusCode::CREATED);

    let stock_zero: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_zero.0, 0, "Stock must reach exactly 0 without negative underflow");
}

// =========================================================================
// TEST 3: Workflow 3 — Warehouse Transfer -> Atomic Decrement/Increment
//         -> Outbox Event (StockTransferred)
// =========================================================================
#[tokio::test]
async fn test_challenge_e2e_workflow_3_warehouse_transfer_atomic_decrement_increment_outbox() {
    let h = setup_m5_challenger2_harness().await;

    // 1. Create two warehouses WH-ALPHA and WH-BETA
    let (_, wh1_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({ "code": "WH-ALPHA", "name": "Source Hub Alpha", "is_default": true })),
        None,
    )
    .await;
    let wh1_id = wh1_res["id"].as_str().unwrap().to_string();

    let (_, wh2_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({ "code": "WH-BETA", "name": "Dest Hub Beta", "is_default": false })),
        None,
    )
    .await;
    let wh2_id = wh2_res["id"].as_str().unwrap().to_string();

    let (_, prod_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "name": "Barang Transfer",
            "unit": "BOX",
            "cost_price": 45_000,
            "sale_price": 90_000,
            "reorder_threshold": 5
        })),
        None,
    )
    .await;
    let prod_id = prod_res["id"].as_str().unwrap().to_string();

    // 2. Inbound 20 units @ Rp 45,000 into WH-ALPHA
    let (st_in, _, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh1_id,
            "quantity": 20,
            "unit_cost": 45_000
        })),
        None,
    )
    .await;
    assert_eq!(st_in, StatusCode::CREATED);

    // 3. Perform inter-warehouse transfer of 8 units from WH-ALPHA to WH-BETA
    let (st_tr, tr_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfers",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "source_warehouse_id": wh1_id,
            "destination_warehouse_id": wh2_id,
            "product_id": prod_id,
            "quantity": 8,
            "notes": "Rebalancing stock between hubs"
        })),
        None,
    )
    .await;
    assert_eq!(st_tr, StatusCode::OK);
    assert_eq!(tr_res["quantity"], 8);
    assert_eq!(tr_res["source_remaining"], 12);
    assert_eq!(tr_res["destination_total"], 8);

    // 4. Verify Atomic Balances & Net Stock Conservation: 12 + 8 == 20
    let q1: (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, average_cost FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh1_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let q2: (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, average_cost FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh2_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(q1.0, 12, "Source stock must decrease atomically to 12");
    assert_eq!(q2.0, 8, "Destination stock must increase atomically to 8");
    assert_eq!(q1.0 + q2.0, 20, "Total net stock across warehouses must be conserved strictly");
    assert_eq!(q2.1, 45_000, "Destination WAC must carry forward source average cost");

    // 5. Verify Transactional Outbox Event: StockTransferred
    let outbox_tr = sqlx::query(
        "SELECT event_type, aggregate_type, payload_json FROM outbox_events WHERE tenant_id = ? AND event_type = 'StockTransferred' LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(outbox_tr.get::<String, _>("event_type"), "StockTransferred");
    assert_eq!(outbox_tr.get::<String, _>("aggregate_type"), "Inventory");
    let tr_payload: Value = serde_json::from_str(&outbox_tr.get::<String, _>("payload_json")).unwrap();
    assert_eq!(tr_payload["quantity"], 8);
    assert_eq!(tr_payload["source_warehouse_id"], wh1_id);
    assert_eq!(tr_payload["destination_warehouse_id"], wh2_id);

    // 6. Negative Boundary Tests:
    // a. Transfer to same warehouse -> HTTP 400
    let (st_same, _, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfers",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "source_warehouse_id": wh1_id,
            "destination_warehouse_id": wh1_id,
            "product_id": prod_id,
            "quantity": 1
        })),
        None,
    )
    .await;
    assert_eq!(st_same, StatusCode::BAD_REQUEST);

    // b. Transfer exceeding available source stock (15 requested, 12 available) -> HTTP 422
    let (st_excess, excess_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfers",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "source_warehouse_id": wh1_id,
            "destination_warehouse_id": wh2_id,
            "product_id": prod_id,
            "quantity": 15
        })),
        None,
    )
    .await;
    assert_eq!(st_excess, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(excess_res["code"], "INSUFFICIENT_STOCK");

    // Verify balances completely unchanged after aborted transfer
    let q1_after: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh1_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(q1_after.0, 12);
}

// =========================================================================
// TEST 4: Workflow 4 — Physical Count Opname Adjustment -> Variance Reconciliation
//         -> GL Journal -> Outbox Event (StockAdjusted)
// =========================================================================
#[tokio::test]
async fn test_challenge_e2e_workflow_4_physical_count_opname_variance_gl_outbox() {
    let h = setup_m5_challenger2_harness().await;

    // 1. Setup warehouse and product with 10 units @ Rp 30,000
    let (_, wh_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({ "code": "WH-OPNAME", "name": "Opname Warehouse", "is_default": true })),
        None,
    )
    .await;
    let wh_id = wh_res["id"].as_str().unwrap().to_string();

    let (_, prod_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "name": "Barang Audit Opname",
            "unit": "KG",
            "cost_price": 30_000,
            "sale_price": 50_000,
            "reorder_threshold": 5
        })),
        None,
    )
    .await;
    let prod_id = prod_res["id"].as_str().unwrap().to_string();

    // Initial stock: 10 units @ Rp 30,000
    let (st_in, _, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10,
            "unit_cost": 30_000
        })),
        None,
    )
    .await;
    assert_eq!(st_in, StatusCode::CREATED);

    // 2. Opname Case A: Surplus Adjustment (+4 units, actual = 14)
    // Variance = +4 * Rp 30,000 = Rp 120,000
    // Journal: Debit 1300 (Persediaan) Rp 120,000 / Credit 5000 (Selisih Lebih) Rp 120,000
    let (st_adj1, adj1_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 14,
            "reason": "Surplus cycle count audit Q1"
        })),
        None,
    )
    .await;
    assert_eq!(st_adj1, StatusCode::OK);
    assert_eq!(adj1_res["variance_quantity"], 4);
    assert_eq!(adj1_res["previous_quantity"], 10);
    assert_eq!(adj1_res["new_quantity"], 14);

    // Verify stock balance updated to 14
    let stock_adj1: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_adj1.0, 14);

    // Verify GL adjusting journal: Debit 1300 / Credit 5000 for Rp 120,000
    let j_adj1 = sqlx::query(
        "SELECT id FROM journal_entries WHERE tenant_id = ? AND source_type = 'STOCK_ADJUSTMENT' ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let j_adj1_id: String = j_adj1.get("id");

    let sums_adj1: (i64, i64) = sqlx::query_as(
        "SELECT SUM(debit), SUM(credit) FROM journal_lines WHERE journal_id = ?",
    )
    .bind(&j_adj1_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(sums_adj1.0, 120_000);
    assert_eq!(sums_adj1.1, 120_000);

    let lines_adj1 = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ? ORDER BY debit DESC",
    )
    .bind(&j_adj1_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(lines_adj1[0].get::<String, _>("account_code"), "1300");
    assert_eq!(lines_adj1[0].get::<i64, _>("debit"), 120_000);
    assert_eq!(lines_adj1[1].get::<String, _>("account_code"), "5000");
    assert_eq!(lines_adj1[1].get::<i64, _>("credit"), 120_000);

    // Verify Outbox Event: StockAdjusted
    let outbox_adj1 = sqlx::query(
        "SELECT event_type, aggregate_type, payload_json FROM outbox_events WHERE tenant_id = ? AND event_type = 'StockAdjusted' ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(outbox_adj1.get::<String, _>("event_type"), "StockAdjusted");
    let p_adj1: Value = serde_json::from_str(&outbox_adj1.get::<String, _>("payload_json")).unwrap();
    assert_eq!(p_adj1["variance"], 4);
    assert_eq!(p_adj1["previous_quantity"], 10);
    assert_eq!(p_adj1["actual_quantity"], 14);

    // 3. Opname Case B: Shrinkage Adjustment (-3 units, actual = 11)
    // Variance = -3 * Rp 30,000 = Rp 90,000
    // Journal: Debit 5000 (Beban Pokok) Rp 90,000 / Credit 1300 (Persediaan) Rp 90,000
    let (st_adj2, adj2_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 11,
            "reason": "Damaged items shrinkage during count"
        })),
        None,
    )
    .await;
    assert_eq!(st_adj2, StatusCode::OK);
    assert_eq!(adj2_res["variance_quantity"], -3);
    assert_eq!(adj2_res["previous_quantity"], 14);
    assert_eq!(adj2_res["new_quantity"], 11);

    // Verify stock balance updated to 11
    let stock_adj2: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ? AND warehouse_id = ? AND product_id = ?",
    )
    .bind(&h.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(stock_adj2.0, 11);

    // Verify GL adjusting journal: Debit 5000 / Credit 1300 for Rp 90,000
    let j_adj2 = sqlx::query(
        "SELECT id FROM journal_entries WHERE tenant_id = ? AND source_type = 'STOCK_ADJUSTMENT' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&h.tenant_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    let j_adj2_id: String = j_adj2.get("id");

    let sums_adj2: (i64, i64) = sqlx::query_as(
        "SELECT SUM(debit), SUM(credit) FROM journal_lines WHERE journal_id = ?",
    )
    .bind(&j_adj2_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(sums_adj2.0, 90_000);
    assert_eq!(sums_adj2.1, 90_000);

    let lines_adj2 = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ? ORDER BY debit DESC",
    )
    .bind(&j_adj2_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(lines_adj2[0].get::<String, _>("account_code"), "5000", "Shrinkage debit must be 5000");
    assert_eq!(lines_adj2[0].get::<i64, _>("debit"), 90_000);
    assert_eq!(lines_adj2[1].get::<String, _>("account_code"), "1300", "Shrinkage credit must be 1300");
    assert_eq!(lines_adj2[1].get::<i64, _>("credit"), 90_000);

    // 4. Opname Case C: Zero variance (actual_quantity == previous_quantity)
    let (st_adj0, adj0_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 11,
            "reason": "Exact match cycle audit"
        })),
        None,
    )
    .await;
    assert_eq!(st_adj0, StatusCode::OK);
    assert_eq!(adj0_res["variance_quantity"], 0);
    assert!(adj0_res["journal_entry_id"].is_null());

    // 5. Negative boundary: Negative actual quantity -> HTTP 422
    let (st_neg, _, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": -5
        })),
        None,
    )
    .await;
    assert_eq!(st_neg, StatusCode::UNPROCESSABLE_ENTITY);
}

// =========================================================================
// TEST 5: Security Hardening 1 — Cross-Tenant Anti-Enumeration (RFC 7807 HTTP 404)
// =========================================================================
#[tokio::test]
async fn test_challenge_security_1_cross_tenant_anti_enumeration_rfc7807_404() {
    let h = setup_m5_challenger2_harness().await;

    // In Tenant A, create Warehouse A, Product A, and PO A
    let (_, wh_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({ "code": "WH-SEC-A", "name": "Tenant A Warehouse", "is_default": true })),
        None,
    )
    .await;
    let wh_a_id = wh_res["id"].as_str().unwrap().to_string();

    let (_, prod_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "name": "Secret Product A",
            "unit": "PCS",
            "cost_price": 50_000,
            "sale_price": 100_000,
            "reorder_threshold": 5
        })),
        None,
    )
    .await;
    let prod_a_id = prod_res["id"].as_str().unwrap().to_string();

    let (_, po_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "supplier_name": "Supplier A",
            "destination_warehouse_id": wh_a_id,
            "items": [
                {
                    "product_id": prod_a_id,
                    "quantity_ordered": 10,
                    "unit_cost": 50_000
                }
            ]
        })),
        None,
    )
    .await;
    let po_a_id = po_res["id"].as_str().unwrap().to_string();

    // Now Tenant B attempts to access Tenant A's resources
    // MUST strictly return RFC 7807 HTTP 404 Not Found (Zero 403s or 200s)
    let check_rfc7807_404 = |status: StatusCode, body: Value, endpoint_desc: &str| {
        assert_eq!(status, StatusCode::NOT_FOUND, "Endpoint '{}' must return 404", endpoint_desc);
        assert_eq!(body["status"], 404, "RFC 7807 status field must be 404 in '{}'", endpoint_desc);
        assert_eq!(body["code"], "NOT_FOUND", "RFC 7807 code must be NOT_FOUND in '{}'", endpoint_desc);
        assert_eq!(body["title"], "Not Found", "RFC 7807 title must be Not Found in '{}'", endpoint_desc);
        assert_eq!(
            body["type"],
            "https://api.nurdiansyahlabs.com/errors/not-found",
            "RFC 7807 type must point to not-found in '{}'",
            endpoint_desc
        );
        assert!(!body["detail"].as_str().unwrap_or("").is_empty(), "Detail must not be empty in '{}'", endpoint_desc);
    };

    // 1. Cross-tenant Warehouse lookup
    let (st, res, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", wh_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    check_rfc7807_404(st, res, "GET /api/v1/warehouses/{id}");

    // 2. Cross-tenant Product lookup
    let (st, res, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/products/{}", prod_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    check_rfc7807_404(st, res, "GET /api/v1/products/{id}");

    // 3. Cross-tenant Purchase Order lookup
    let (st, res, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/purchase-orders/{}", po_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    check_rfc7807_404(st, res, "GET /api/v1/purchase-orders/{id}");

    // 4. Cross-tenant PO order action
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    check_rfc7807_404(st, res, "POST /api/v1/purchase-orders/{id}/order");

    // 5. Cross-tenant PO receive action
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "items": [{ "product_id": prod_a_id, "quantity_received": 5 }]
        })),
        None,
    )
    .await;
    check_rfc7807_404(st, res, "POST /api/v1/purchase-orders/{id}/receive");

    // 6. Cross-tenant PO cancel action
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    check_rfc7807_404(st, res, "POST /api/v1/purchase-orders/{id}/cancel");

    // 7. Cross-tenant Stock Movement creation (referencing Tenant A's product)
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_a_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    check_rfc7807_404(st, res, "POST /api/v1/inventory/movements");

    // 8. Cross-tenant Stock Transfer (referencing Tenant A's warehouse)
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfers",
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": Uuid::new_v4().to_string(),
            "product_id": prod_a_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    check_rfc7807_404(st, res, "POST /api/v1/inventory/transfers");

    // 9. Cross-tenant Stock Adjustment
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "warehouse_id": wh_a_id,
            "product_id": prod_a_id,
            "actual_quantity": 20
        })),
        None,
    )
    .await;
    check_rfc7807_404(st, res, "POST /api/v1/inventory/adjustments");
}

// =========================================================================
// TEST 6: Security Hardening 2 — Tenant-Internal Unauthorized Staff RBAC (HTTP 403 Forbidden)
// =========================================================================
#[tokio::test]
async fn test_challenge_security_2_tenant_internal_unauthorized_staff_rbac_403() {
    let h = setup_m5_challenger2_harness().await;

    // Setup an existing warehouse, product, and PO by Owner in Tenant A
    let (_, wh_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({ "code": "WH-RBAC", "name": "RBAC Warehouse", "is_default": true })),
        None,
    )
    .await;
    let wh_id = wh_res["id"].as_str().unwrap().to_string();

    let (_, prod_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "name": "RBAC Product",
            "unit": "PCS",
            "cost_price": 50_000,
            "sale_price": 100_000,
            "reorder_threshold": 5
        })),
        None,
    )
    .await;
    let prod_id = prod_res["id"].as_str().unwrap().to_string();

    let (_, po_res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &h.owner_token,
        &h.tenant_id,
        Some(json!({
            "supplier_name": "Supplier RBAC",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 10,
                    "unit_cost": 50_000
                }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_res["id"].as_str().unwrap().to_string();

    let check_rfc7807_403 = |status: StatusCode, body: Value, action_desc: &str| {
        assert_eq!(status, StatusCode::FORBIDDEN, "Staff action '{}' must return 403 Forbidden", action_desc);
        assert_eq!(body["status"], 403, "RFC 7807 status must be 403 for '{}'", action_desc);
        assert_eq!(body["code"], "FORBIDDEN", "RFC 7807 code must be FORBIDDEN for '{}'", action_desc);
        assert_eq!(body["title"], "Forbidden", "RFC 7807 title must be Forbidden for '{}'", action_desc);
        assert_eq!(
            body["type"],
            "https://api.nurdiansyahlabs.com/errors/forbidden",
            "RFC 7807 type must point to forbidden for '{}'",
            action_desc
        );
    };

    // 1. Staff attempts to create warehouse -> HTTP 403
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.staff_token,
        &h.tenant_id,
        Some(json!({ "code": "WH-ILLEGAL", "name": "Illegal Warehouse" })),
        None,
    )
    .await;
    check_rfc7807_403(st, res, "Staff Create Warehouse");

    // 2. Staff attempts to create stock adjustment -> HTTP 403
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.staff_token,
        &h.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 25
        })),
        None,
    )
    .await;
    check_rfc7807_403(st, res, "Staff Stock Adjustment");

    // 3. Staff attempts to create purchase order -> HTTP 403
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &h.staff_token,
        &h.tenant_id,
        Some(json!({
            "supplier_name": "Supplier Staff",
            "destination_warehouse_id": wh_id,
            "items": [{ "product_id": prod_id, "quantity_ordered": 5, "unit_cost": 50_000 }]
        })),
        None,
    )
    .await;
    check_rfc7807_403(st, res, "Staff Create Purchase Order");

    // 4. Staff attempts to order/approve purchase order -> HTTP 403
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &h.staff_token,
        &h.tenant_id,
        None,
        None,
    )
    .await;
    check_rfc7807_403(st, res, "Staff Approve/Order PO");

    // 5. Staff attempts to receive purchase order -> HTTP 403
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &h.staff_token,
        &h.tenant_id,
        Some(json!({
            "items": [{ "product_id": prod_id, "quantity_received": 5 }]
        })),
        None,
    )
    .await;
    check_rfc7807_403(st, res, "Staff Receive PO Goods");

    // 6. Staff attempts to cancel purchase order -> HTTP 403
    let (st, res, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &h.staff_token,
        &h.tenant_id,
        None,
        None,
    )
    .await;
    check_rfc7807_403(st, res, "Staff Cancel PO");
}

// =========================================================================
// TEST 7: Security Hardening 3 — Personal Workspace Capability Isolation
// =========================================================================
#[tokio::test]
async fn test_challenge_security_3_personal_workspace_isolation_and_capabilities() {
    let h = setup_m5_challenger2_harness().await;

    // Query capabilities for personal workspace
    let (st, res, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/tenants/{}/capabilities", h.personal_tenant_id),
        &h.owner_token,
        &h.personal_tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["business_type"], "personal");

    let capabilities: Vec<String> = res["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    // Must strictly contain only personal capabilities
    assert_eq!(
        capabilities,
        vec!["accounts", "transactions", "budgets", "analytics"],
        "Personal workspace capabilities must be strictly limited"
    );

    // Business modules must NOT be surfaced
    assert!(!capabilities.contains(&"inventory".to_string()));
    assert!(!capabilities.contains(&"purchasing".to_string()));
    assert!(!capabilities.contains(&"invoicing".to_string()));
    assert!(!capabilities.contains(&"pos".to_string()));
}
