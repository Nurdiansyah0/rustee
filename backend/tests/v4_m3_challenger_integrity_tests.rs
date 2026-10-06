//! Milestone 3 Adversarial Integrity & Verification Test Harness:
//! Direct Job Costing Material Allocation & Inventory Integration
//! (PRD §10, §11, §27, §31, §60, §62, §72).
//!
//! Subagent: challenger_m3_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//! References: ORIGINAL_REQUEST.md, PROJECT.md
//!
//! Empirical Challenge Mandates:
//! 1. Stock Reservation & Available Stock Edge Cases:
//!    - Verify available stock formula: `available = on_hand.saturating_sub(reserved)`.
//!    - Explicit mandate scenario: `on_hand = 20`, `reserved = 15`.
//!      Attempting to issue 6 units MUST fail with HTTP 422 `INSUFFICIENT_STOCK`.
//!      Issuing 5 units (exact available) MUST succeed.
//!      Subsequently issuing 1 unit MUST fail with HTTP 422 `INSUFFICIENT_STOCK`.
//!    - Saturating subtraction guard: `reserved > on_hand` (e.g. `on_hand = 10`, `reserved = 15`)
//!      yields available 0; issuing 1 unit fails with HTTP 422 `INSUFFICIENT_STOCK`.
//! 2. Transactional Rollback Integrity:
//!    - Prove that on any error or rejection, 100% of mutations roll back atomically:
//!      * Zero stock decrement (`quantity_on_hand` and `quantity_reserved` unchanged)
//!      * Zero stock movements inserted
//!      * Zero GL journals created or posted
//!      * Zero outbox events inserted
//!      * Material status unchanged (`PLANNED` with `quantity_issued = 0`, null references)
//!    - Tested across insufficient stock, inactive project lifecycle states, and invalid quantities.
//! 3. Outbox Event Exact Field Verification:
//!    - Inspect `outbox_events` table payload for `ProjectMaterialIssued`.
//!    - Assert exact match with domain state, foreign keys, costs, and schema invariants.
//!    - Cross-verify `stock_movement_id` and `journal_entry_id` foreign references in DB.

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
use sqlx::Row;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    #[allow(dead_code)]
    _dir: tempfile::TempDir,
    owner_token: String,
    manager_token: String,
    #[allow(dead_code)]
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
    product_a_id: String,
    warehouse_a_id: String,
    product_steel_id: String,
    product_sample_id: String,
}

async fn setup_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m3_integrity.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 20,
        min_connections: 1,
        busy_timeout_ms: 15_000,
        acquire_timeout_secs: 20,
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

    let jwt_secret = "m3_challenger_integrity_secret_key_1234567890_super_secret";
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

    // 1. Owner User A
    let owner_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_id.clone(),
            email: "owner_integ_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_integ_a@contractor.com", "user", "premium")
        .unwrap();

    // 2. Manager User A
    let manager_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: manager_id.clone(),
            email: "manager_integ_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Manager A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (manager_token, _) = jwt_engine
        .generate_token(&manager_id, "manager_integ_a@contractor.com", "user", "premium")
        .unwrap();

    // 3. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_integ_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_integ_a@contractor.com", "user", "free")
        .unwrap();

    // 4. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Integrity".to_string(),
            slug: "tenant-alpha-integrity".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    // Seed default chart of accounts for Tenant A
    let mut tx_a = pool.begin().await.unwrap();
    backend::repository::accounting_repo::SqlxAccountingRepository::seed_default_accounts_tx(
        &mut tx_a,
        &tenant_a_id,
    )
    .await
    .unwrap();
    tx_a.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: owner_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: manager_id.clone(),
            role: Role::Manager,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: staff_id.clone(),
            role: Role::Staff,
        })
        .await
        .unwrap();

    // Setup Warehouses & Products for Tenant A
    let warehouse_a_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
        VALUES (?1, ?2, 'WH-MAIN-INTEG', 'Main Warehouse Alpha', 1, ?3, ?3);
        "#,
    )
    .bind(&warehouse_a_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    // Product A: Portland Cement (cost 50,000 IDR)
    let product_a_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-CMT-INTEG', 'Semen Portland 50kg', 'SAK', 50000, 75000, 10, 1, ?3, ?3);
        "#,
    )
    .bind(&product_a_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    // Product Steel: Reinforced Steel 12mm (cost 120,000 IDR)
    let product_steel_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-STL-INTEG', 'Besi Beton 12mm', 'BTG', 120000, 160000, 5, 1, ?3, ?3);
        "#,
    )
    .bind(&product_steel_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    // Product Sample: Promotional sample (cost 0 IDR)
    let product_sample_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-SMP-INTEG', 'Sample Keramik Granit', 'PCS', 0, 0, 0, 1, ?3, ?3);
        "#,
    )
    .bind(&product_sample_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    // 5. Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "owner_integ_b@adversary.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "owner_integ_b@adversary.com", "user", "premium")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Integrity".to_string(),
            slug: "tenant-beta-integrity".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_b = pool.begin().await.unwrap();
    backend::repository::accounting_repo::SqlxAccountingRepository::seed_default_accounts_tx(
        &mut tx_b,
        &tenant_b_id,
    )
    .await
    .unwrap();
    tx_b.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: user_b_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        owner_token,
        manager_token,
        staff_token,
        tenant_a_id,
        tenant_b_id,
        tenant_b_token,
        product_a_id,
        warehouse_a_id,
        product_steel_id,
        product_sample_id,
    }
}

async fn send_req(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: &str,
    body: Option<Value>,
) -> (StatusCode, Value, HeaderMap) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id);

    if body.is_some() {
        builder = builder.header(CONTENT_TYPE, "application/json");
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

async fn create_active_project(h: &ChallengerHarness, name: &str) -> String {
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": name,
            "description": "Adversarial Integrity Project",
            "customer_name": "PT Jaya Mandiri",
            "billing_type": "MILESTONE",
            "budget_amount": 100_000_000,
            "contract_amount": 150_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = resp["id"].as_str().unwrap().to_string();

    let (status, update_resp, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_resp["status"], "ACTIVE");

    project_id
}

// ============================================================================
// Test 1: Mandatory Stock Reservation Scenario (on_hand=20, reserved=15)
// Planned Material Requisition Issuance
// ============================================================================

#[tokio::test]
async fn test_mandate_stock_reservation_20_onhand_15_reserved_issue_6_fails_5_succeeds() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Stock Reservation Requisition Test").await;

    // Seed stock: exactly on_hand = 20, reserved = 15 => available = 5
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 20, 15, 5, 'BIN-RES-1', 50000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Create Planned Material #1: planned quantity = 6
    let (status, resp_mat1, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 6,
            "is_billable": true,
            "notes": "Attempting to issue 6 units when available is 5"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mat1_id = resp_mat1["id"].as_str().unwrap().to_string();

    // Adversarial Challenge Step 1:
    // Attempting to issue 6 units must strictly FAIL with HTTP 422 INSUFFICIENT_STOCK
    let (status, err_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat1_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({ "quantity": 6 })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Issue 6 units must be rejected with HTTP 422"
    );
    assert_eq!(
        err_resp["code"], "INSUFFICIENT_STOCK",
        "Error code must be INSUFFICIENT_STOCK"
    );
    let detail = err_resp["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("requested 6") && detail.contains("available 5"),
        "Detail must state requested 6, available 5, got: {}",
        detail
    );

    // Verify 100% Transactional Rollback on failure:
    // Stock item must remain exactly on_hand = 20, reserved = 15
    let (on_hand, reserved): (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, quantity_reserved FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(on_hand, 20, "Stock on hand must remain 20 after rollback");
    assert_eq!(reserved, 15, "Reserved stock must remain 15 after rollback");

    // Zero stock movements, zero journals, zero outbox events
    let mov_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_movements WHERE reference_id = ?1",
    )
    .bind(&mat1_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mov_count, 0, "Zero stock movements on rollback");

    let jnl_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_entries WHERE source_id = ?1",
    )
    .bind(&mat1_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(jnl_count, 0, "Zero journal entries on rollback");

    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1",
    )
    .bind(&mat1_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 0, "Zero outbox events on rollback");

    // Material record must remain PLANNED
    let mat_status: String = sqlx::query_scalar(
        "SELECT status FROM project_materials WHERE id = ?1",
    )
    .bind(&mat1_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mat_status, "PLANNED", "Material must remain in PLANNED status");

    // Adversarial Challenge Step 2:
    // Create Planned Material #2 with planned quantity = 5 (exact available stock)
    let (status, resp_mat2, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 5,
            "is_billable": true,
            "notes": "Issuing exactly 5 units (exact available)"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mat2_id = resp_mat2["id"].as_str().unwrap().to_string();

    let (status, resp_issue, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat2_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({ "quantity": 5 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Issue of exactly available stock (5) must succeed");
    assert_eq!(resp_issue["status"], "ISSUED");
    assert_eq!(resp_issue["quantity_issued"], 5);

    // Verify stock state: on_hand must drop from 20 to 15; reserved remains 15
    let (on_hand_after, reserved_after): (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, quantity_reserved FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(on_hand_after, 15, "Stock on hand must now be 15 (20 - 5)");
    assert_eq!(reserved_after, 15, "Reserved stock must remain 15");

    // Adversarial Challenge Step 3:
    // Available stock is now 15 - 15 = 0. Attempting to issue even 1 unit must FAIL with 422
    let (status, resp_mat3, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 1,
            "is_billable": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mat3_id = resp_mat3["id"].as_str().unwrap().to_string();

    let (status, err_resp3, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat3_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({ "quantity": 1 })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Issue 1 unit when available is 0 must fail with 422"
    );
    assert_eq!(err_resp3["code"], "INSUFFICIENT_STOCK");
    let detail3 = err_resp3["detail"].as_str().unwrap_or("");
    assert!(
        detail3.contains("requested 1") && detail3.contains("available 0"),
        "Detail must indicate available 0, got: {}",
        detail3
    );

    // Confirm stock remained 15
    let on_hand_final: i64 = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(on_hand_final, 15);
}

// ============================================================================
// Test 2: Stock Reservation on Direct Material Issue (POST /materials/issue)
// ============================================================================

#[tokio::test]
async fn test_mandate_direct_issue_with_stock_reservation_constraint() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Stock Reservation Direct Issue Test").await;

    // Seed stock: on_hand = 20, reserved = 15 => available = 5
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 20, 15, 5, 'BIN-RES-DIR', 50000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Direct issue requesting 6 units must fail with HTTP 422 INSUFFICIENT_STOCK
    let (status, err_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 6,
            "is_billable": true,
            "notes": "Direct issue 6 units against available 5"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "INSUFFICIENT_STOCK");

    // 100% Rollback check: no project_materials inserted, stock unchanged
    let mat_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_materials WHERE project_id = ?1",
    )
    .bind(&project_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mat_count, 0, "Zero materials created on rollback");

    let (on_hand, reserved): (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, quantity_reserved FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(on_hand, 20);
    assert_eq!(reserved, 15);

    // Direct issue requesting exactly 5 units must succeed with HTTP 201 CREATED
    let (status, resp_ok, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 5,
            "is_billable": true,
            "notes": "Direct issue exact available 5 units"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp_ok["status"], "ISSUED");
    assert_eq!(resp_ok["quantity_issued"], 5);

    // Stock on hand drops to 15 (20 - 5), reserved is still 15
    let (on_hand2, reserved2): (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, quantity_reserved FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(on_hand2, 15);
    assert_eq!(reserved2, 15);

    // Direct issue requesting 1 unit when available = 0 must fail
    let (status, err_resp2, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp2["code"], "INSUFFICIENT_STOCK");
}

// ============================================================================
// Test 3: Saturating Subtraction Guard When Reserved Exceeds On Hand
// ============================================================================

#[tokio::test]
async fn test_stock_reservation_saturating_sub_when_reserved_exceeds_on_hand() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Saturating Sub Guard Test").await;

    // Abnormal condition: on_hand = 10, reserved = 15 (e.g. after write-down)
    // Formula: on_hand.saturating_sub(reserved) => 10.saturating_sub(15) = 0
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 10, 15, 5, 'BIN-SAT-SUB', 50000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // 1. Direct issue of 1 unit fails with 422 INSUFFICIENT_STOCK
    let (status, resp_dir, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(resp_dir["code"], "INSUFFICIENT_STOCK");
    let detail_dir = resp_dir["detail"].as_str().unwrap_or("");
    let expected_available = 10i64.saturating_sub(15i64);
    assert!(
        detail_dir.contains(&format!("available {}", expected_available)),
        "Detail must show available {} per i64 saturating_sub, got: {}",
        expected_available,
        detail_dir
    );

    // 2. Planned issue of 1 unit also fails with 422 INSUFFICIENT_STOCK
    let (status, resp_mat, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mat_id = resp_mat["id"].as_str().unwrap().to_string();

    let (status, resp_plan, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({ "quantity": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(resp_plan["code"], "INSUFFICIENT_STOCK");

    // Stock levels remain intact
    let (on_hand, reserved): (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, quantity_reserved FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(on_hand, 10);
    assert_eq!(reserved, 15);
}

// ============================================================================
// Test 4: Transactional Rollback Integrity on Insufficient Stock
// Exhaustive Check across all 5 mutated tables
// ============================================================================

#[tokio::test]
async fn test_transactional_rollback_integrity_on_insufficient_stock_planned_issue() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Rollback Insufficient Stock Test").await;

    // Seed stock: exactly 10 units of Steel @ 120,000 IDR
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 10, 0, 5, 'BIN-ROLLBACK-1', 120000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_steel_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Baseline counts across all potential mutation tables
    let baseline_movements: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movements")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let baseline_journals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let baseline_lines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let baseline_outbox: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events")
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    // Create planned material for 50 units (exceeding stock of 10)
    let (status, resp_mat, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_steel_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 50,
            "is_billable": true,
            "notes": "Large planned material requisition"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mat_id = resp_mat["id"].as_str().unwrap().to_string();

    // Attempt issue of 50 units -> Rejected with HTTP 422
    let (status, err_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({ "quantity": 50 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "INSUFFICIENT_STOCK");

    // INVARIANT 1: Zero stock decrement
    let current_stock: i64 = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(current_stock, 10, "Stock must remain 10");

    // INVARIANT 2: Zero stock movements
    let current_movements: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movements")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(
        current_movements, baseline_movements,
        "No stock movement may be created on rollback"
    );

    // INVARIANT 3: Zero GL journals and lines
    let current_journals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(
        current_journals, baseline_journals,
        "No journal entry may be created on rollback"
    );

    let current_lines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_lines")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(
        current_lines, baseline_lines,
        "No journal line may be created on rollback"
    );

    // INVARIANT 4: Zero outbox events
    let current_outbox: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(
        current_outbox, baseline_outbox,
        "No outbox event may be created on rollback"
    );

    // INVARIANT 5: Material record state completely unchanged
    let row = sqlx::query(
        "SELECT status, quantity_issued, unit_cost, total_cost, stock_movement_id, journal_entry_id, issued_at FROM project_materials WHERE id = ?1",
    )
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let status_val: String = row.get("status");
    let qty_issued: i64 = row.get("quantity_issued");
    let unit_cost: i64 = row.get("unit_cost");
    let total_cost: i64 = row.get("total_cost");
    let mov_id: Option<String> = row.get("stock_movement_id");
    let jnl_id: Option<String> = row.get("journal_entry_id");
    let issued_at: Option<String> = row.get("issued_at");

    assert_eq!(status_val, "PLANNED");
    assert_eq!(qty_issued, 0);
    assert_eq!(unit_cost, 0);
    assert_eq!(total_cost, 0);
    assert!(mov_id.is_none());
    assert!(jnl_id.is_none());
    assert!(issued_at.is_none());
}

// ============================================================================
// Test 5: Transactional Rollback on Inactive Project States (DRAFT, ON_HOLD, etc.)
// ============================================================================

#[tokio::test]
async fn test_transactional_rollback_on_inactive_project_lifecycle() {
    let harness = setup_harness().await;

    // Seed stock: 100 units
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 100, 0, 5, 'BIN-INACTIVE', 50000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    let inactive_statuses = ["DRAFT", "ON_HOLD", "COMPLETED", "CANCELLED"];

    for &proj_status in &inactive_statuses {
        // Create project in DRAFT
        let (status, resp, _) = send_req(
            &harness.app,
            Method::POST,
            "/api/v1/projects",
            &harness.owner_token,
            &harness.tenant_a_id,
            Some(json!({
                "name": format!("Project Status {}", proj_status),
                "customer_name": "PT Customer",
                "billing_type": "MILESTONE"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let pid = resp["id"].as_str().unwrap().to_string();

        if proj_status != "DRAFT" {
            // First transition to ACTIVE, then to target status
            let _ = send_req(
                &harness.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/status", pid),
                &harness.owner_token,
                &harness.tenant_a_id,
                Some(json!({ "status": "ACTIVE" })),
            )
            .await;

            let (status, update_resp, _) = send_req(
                &harness.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/status", pid),
                &harness.owner_token,
                &harness.tenant_a_id,
                Some(json!({ "status": proj_status })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(update_resp["status"], proj_status);
        }

        // 1. Direct issue attempt on inactive project must fail with 422 PROJECT_NOT_ACTIVE
        let (status, err_resp, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials/issue", pid),
            &harness.manager_token,
            &harness.tenant_a_id,
            Some(json!({
                "product_id": harness.product_a_id,
                "warehouse_id": harness.warehouse_a_id,
                "quantity": 5,
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Direct issue on project status {} must fail with 422",
            proj_status
        );
        assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

        // 2. Planned issue attempt: create planned material directly in DB or via API if allowed in DRAFT
        let mat_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO project_materials (id, tenant_id, project_id, product_id, warehouse_id, quantity_planned, quantity_issued, unit_cost, total_cost, status, is_billable, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, 10, 0, 0, 0, 'PLANNED', 1, ?6, ?6);
            "#,
        )
        .bind(&mat_id)
        .bind(&harness.tenant_a_id)
        .bind(&pid)
        .bind(&harness.product_a_id)
        .bind(&harness.warehouse_a_id)
        .bind(&now_str)
        .execute(&harness.pool)
        .await
        .unwrap();

        let (status, err_plan, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials/{}/issue", pid, mat_id),
            &harness.manager_token,
            &harness.tenant_a_id,
            Some(json!({ "quantity": 10 })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Planned issue on project status {} must fail with 422",
            proj_status
        );
        assert_eq!(err_plan["code"], "PROJECT_NOT_ACTIVE");

        // Assert stock remains 100
        let current_stock: i64 = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_items WHERE id = ?1",
        )
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
        assert_eq!(current_stock, 100);
    }
}

// ============================================================================
// Test 6: Transactional Rollback on Invalid Quantity Boundary Rejections
// ============================================================================

#[tokio::test]
async fn test_transactional_rollback_on_invalid_quantities_boundary() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Invalid Quantity Boundary Test").await;

    // Seed stock: 50 units
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 50, 0, 5, 'BIN-BOUNDARY', 50000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    let invalid_quantities = [0, -1, -999];

    for &q in &invalid_quantities {
        // Direct issue with invalid quantity
        let (status, resp_dir, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials/issue", project_id),
            &harness.manager_token,
            &harness.tenant_a_id,
            Some(json!({
                "product_id": harness.product_a_id,
                "warehouse_id": harness.warehouse_a_id,
                "quantity": q,
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Direct issue with quantity {} must return HTTP 400",
            q
        );
        assert_eq!(resp_dir["code"], "INVALID_QUANTITY");

        // Planned issue with invalid requested quantity
        let (status, resp_mat, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials", project_id),
            &harness.owner_token,
            &harness.tenant_a_id,
            Some(json!({
                "product_id": harness.product_a_id,
                "warehouse_id": harness.warehouse_a_id,
                "quantity_planned": 10,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let mat_id = resp_mat["id"].as_str().unwrap().to_string();

        let (status, resp_plan, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat_id),
            &harness.manager_token,
            &harness.tenant_a_id,
            Some(json!({ "quantity": q })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Planned issue with quantity {} must return HTTP 400",
            q
        );
        assert_eq!(resp_plan["code"], "INVALID_QUANTITY");
    }

    // Stock on hand remains 50
    let current_stock: i64 = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(current_stock, 50);
}

// ============================================================================
// Test 7: Outbox Event Exact Field Verification (Planned Requisition Issue)
// ============================================================================

#[tokio::test]
async fn test_outbox_event_exact_payload_and_foreign_keys_planned_issue() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Outbox Exact Verification Planned").await;

    // Seed stock: 30 units of Cement @ 50,000 IDR
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 30, 0, 5, 'BIN-OUTBOX-1', 50000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Create Planned Material: 7 units @ 50,000 IDR
    let (status, resp_mat, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 7,
            "is_billable": true,
            "notes": "Planned for Foundation Block A"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mat_id = resp_mat["id"].as_str().unwrap().to_string();

    // Issue planned material
    let (status, resp_issue, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({ "quantity": 7 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp_issue["status"], "ISSUED");

    let expected_qty = 7i64;
    let expected_unit_cost = 50_000i64;
    let expected_total_cost = expected_qty * expected_unit_cost; // 350,000 IDR

    let stock_movement_id = resp_issue["stock_movement_id"].as_str().unwrap().to_string();
    let journal_entry_id = resp_issue["journal_entry_id"].as_str().unwrap().to_string();
    let issued_at_str = resp_issue["issued_at"].as_str().unwrap().to_string();

    // 1. Query outbox_events row directly
    let outbox_row = sqlx::query(
        r#"
        SELECT id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json,
               status, retry_count, attempt_count, max_retries, created_at, published_at
        FROM outbox_events
        WHERE aggregate_id = ?1 AND event_type = 'ProjectMaterialIssued'
        "#,
    )
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let outbox_id: String = outbox_row.get("id");
    let outbox_tenant: String = outbox_row.get("tenant_id");
    let outbox_type: String = outbox_row.get("event_type");
    let outbox_agg_type: String = outbox_row.get("aggregate_type");
    let outbox_agg_id: String = outbox_row.get("aggregate_id");
    let payload_str: String = outbox_row.get("payload_json");
    let outbox_status: String = outbox_row.get("status");
    let retry_count: i64 = outbox_row.get("retry_count");
    let attempt_count: i64 = outbox_row.get("attempt_count");
    let max_retries: i64 = outbox_row.get("max_retries");
    let published_at: Option<String> = outbox_row.get("published_at");

    assert!(Uuid::parse_str(&outbox_id).is_ok(), "Outbox event ID must be valid UUID");
    assert_eq!(outbox_tenant, harness.tenant_a_id);
    assert_eq!(outbox_type, "ProjectMaterialIssued");
    assert_eq!(outbox_agg_type, "ProjectMaterial");
    assert_eq!(outbox_agg_id, mat_id);
    assert_eq!(outbox_status, "PENDING");
    assert_eq!(retry_count, 0);
    assert_eq!(attempt_count, 0);
    assert_eq!(max_retries, 5);
    assert!(published_at.is_none(), "Published at must be NULL initially");

    // 2. Parse and verify payload_json exact fields
    let payload: Value = serde_json::from_str(&payload_str).expect("Payload must be valid JSON");
    assert_eq!(payload["material_id"], mat_id);
    assert_eq!(payload["project_id"], project_id);
    assert_eq!(payload["product_id"], harness.product_a_id);
    assert_eq!(payload["warehouse_id"], harness.warehouse_a_id);
    assert_eq!(payload["quantity_issued"], expected_qty);
    assert_eq!(payload["unit_cost"], expected_unit_cost);
    assert_eq!(payload["total_cost"], expected_total_cost);
    assert_eq!(payload["stock_movement_id"], stock_movement_id);
    assert_eq!(payload["journal_entry_id"], journal_entry_id);
    let payload_issued_at = chrono::DateTime::parse_from_rfc3339(payload["issued_at"].as_str().unwrap())
        .expect("payload issued_at must be valid RFC3339");
    let resp_issued_at = chrono::DateTime::parse_from_rfc3339(&issued_at_str)
        .expect("resp issued_at must be valid RFC3339");
    assert_eq!(payload_issued_at, resp_issued_at);

    // 3. Cross-verify referenced stock_movements record in DB
    let mov_row = sqlx::query(
        r#"
        SELECT id, tenant_id, movement_type, product_id, source_warehouse_id,
               destination_warehouse_id, quantity, unit_cost, reference_type, reference_id
        FROM stock_movements
        WHERE id = ?1
        "#,
    )
    .bind(&stock_movement_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let m_tenant: String = mov_row.get("tenant_id");
    let m_type: String = mov_row.get("movement_type");
    let m_prod: String = mov_row.get("product_id");
    let m_src_wh: Option<String> = mov_row.get("source_warehouse_id");
    let m_dst_wh: Option<String> = mov_row.get("destination_warehouse_id");
    let m_qty: i64 = mov_row.get("quantity");
    let m_unit_cost: Option<i64> = mov_row.get("unit_cost");
    let m_ref_type: Option<String> = mov_row.get("reference_type");
    let m_ref_id: Option<String> = mov_row.get("reference_id");

    assert_eq!(m_tenant, harness.tenant_a_id);
    assert_eq!(m_type, "OUTBOUND");
    assert_eq!(m_prod, harness.product_a_id);
    assert_eq!(m_src_wh, Some(harness.warehouse_a_id.clone()));
    assert_eq!(m_dst_wh, None);
    assert_eq!(m_qty, expected_qty);
    assert_eq!(m_unit_cost, Some(expected_unit_cost));
    assert_eq!(m_ref_type, Some("PROJECT_MATERIAL".to_string()));
    assert_eq!(m_ref_id, Some(mat_id.clone()));

    // 4. Cross-verify referenced journal_entries and journal_lines in DB
    let jnl_row = sqlx::query(
        r#"
        SELECT id, tenant_id, source_type, source_id, status
        FROM journal_entries
        WHERE id = ?1
        "#,
    )
    .bind(&journal_entry_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let j_tenant: String = jnl_row.get("tenant_id");
    let j_src_type: String = jnl_row.get("source_type");
    let j_src_id: Option<String> = jnl_row.get("source_id");
    let j_status: String = jnl_row.get("status");

    assert_eq!(j_tenant, harness.tenant_a_id);
    assert_eq!(j_src_type, "INVENTORY_OUTBOUND");
    assert_eq!(j_src_id, Some(mat_id.clone()));
    assert_eq!(j_status, "POSTED");

    // Lines: Debit 5000 = 350,000, Credit 1300 = 350,000
    let lines = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ?1 ORDER BY account_code DESC",
    )
    .bind(&journal_entry_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    assert_eq!(lines.len(), 2);
    let mut debit_5000 = 0i64;
    let mut credit_1300 = 0i64;
    for l in lines {
        let code: String = l.get("account_code");
        let d: i64 = l.get("debit");
        let c: i64 = l.get("credit");
        if code == "5000" {
            debit_5000 = d;
            assert_eq!(c, 0);
        } else if code == "1300" {
            credit_1300 = c;
            assert_eq!(d, 0);
        }
    }
    assert_eq!(debit_5000, expected_total_cost);
    assert_eq!(credit_1300, expected_total_cost);
}

// ============================================================================
// Test 8: Outbox Event Exact Field Verification (Direct Material Issue)
// ============================================================================

#[tokio::test]
async fn test_outbox_event_exact_payload_and_foreign_keys_direct_issue() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Outbox Exact Verification Direct").await;

    // Seed stock: 20 units of Steel @ 120,000 IDR
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 20, 0, 5, 'BIN-OUTBOX-DIR', 120000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_steel_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Direct issue of 3 units @ 120,000 IDR = 360,000 IDR
    let (status, resp_direct, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_steel_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 3,
            "is_billable": true,
            "notes": "Direct issue for roof reinforcement"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp_direct["status"], "ISSUED");

    let mat_id = resp_direct["id"].as_str().unwrap().to_string();
    let mov_id = resp_direct["stock_movement_id"].as_str().unwrap().to_string();
    let jnl_id = resp_direct["journal_entry_id"].as_str().unwrap().to_string();

    let expected_qty = 3i64;
    let expected_unit_cost = 120_000i64;
    let expected_total_cost = 360_000i64;

    // Query outbox_events table
    let outbox_row = sqlx::query(
        r#"
        SELECT id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status
        FROM outbox_events
        WHERE aggregate_id = ?1 AND event_type = 'ProjectMaterialIssued'
        "#,
    )
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let outbox_status: String = outbox_row.get("status");
    let payload_str: String = outbox_row.get("payload_json");
    assert_eq!(outbox_status, "PENDING");

    let payload: Value = serde_json::from_str(&payload_str).unwrap();
    assert_eq!(payload["material_id"], mat_id);
    assert_eq!(payload["project_id"], project_id);
    assert_eq!(payload["product_id"], harness.product_steel_id);
    assert_eq!(payload["warehouse_id"], harness.warehouse_a_id);
    assert_eq!(payload["quantity_issued"], expected_qty);
    assert_eq!(payload["unit_cost"], expected_unit_cost);
    assert_eq!(payload["total_cost"], expected_total_cost);
    assert_eq!(payload["stock_movement_id"], mov_id);
    assert_eq!(payload["journal_entry_id"], jnl_id);

    // Cross-verify foreign records exist
    let mov_exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM stock_movements WHERE id = ?1 AND reference_id = ?2",
    )
    .bind(&mov_id)
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert!(mov_exists, "Stock movement must exist and reference material");

    let jnl_exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM journal_entries WHERE id = ?1 AND source_id = ?2",
    )
    .bind(&jnl_id)
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert!(jnl_exists, "Journal entry must exist and reference material");
}

// ============================================================================
// Test 9: Outbox Event Payload for Zero-Cost Sample Inventory
// (Ensures null journal_entry_id is properly serialized)
// ============================================================================

#[tokio::test]
async fn test_outbox_event_zero_cost_sample_material_payload() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Outbox Zero Cost Sample Test").await;

    // Seed stock: 10 units of Sample Keramik @ 0 IDR
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 10, 0, 0, 'BIN-SAMPLE', 0, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_sample_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Issue 4 units of zero-cost sample material
    let (status, resp_sample, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_sample_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 4,
            "is_billable": false,
            "notes": "Free sample tiles for client selection"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp_sample["status"], "ISSUED");
    assert_eq!(resp_sample["unit_cost"], 0);
    assert_eq!(resp_sample["total_cost"], 0);
    assert!(resp_sample["journal_entry_id"].is_null());

    let mat_id = resp_sample["id"].as_str().unwrap().to_string();

    // Inspect outbox_events payload
    let payload_str: String = sqlx::query_scalar(
        "SELECT payload_json FROM outbox_events WHERE aggregate_id = ?1",
    )
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let payload: Value = serde_json::from_str(&payload_str).unwrap();
    assert_eq!(payload["unit_cost"], 0);
    assert_eq!(payload["total_cost"], 0);
    assert!(
        payload["journal_entry_id"].is_null(),
        "journal_entry_id in payload must be JSON null for zero-cost item"
    );
    assert!(
        payload["stock_movement_id"].is_string(),
        "stock_movement_id must still be present and valid"
    );

    // Verify ZERO journal entries were created in the database
    let jnl_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_entries WHERE source_id = ?1",
    )
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(jnl_count, 0, "No journal entry should be created for 0-cost material");
}

// ============================================================================
// Test 10: Multi-Tenant Anti-Enumeration & Zero Cross-Tenant Leakage
// ============================================================================

#[tokio::test]
async fn test_cross_tenant_rollback_and_zero_leakage() {
    let harness = setup_harness().await;
    let project_id = create_active_project(&harness, "Tenant Alpha Target Project").await;

    // Seed stock for Tenant A: 20 units
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 20, 0, 5, 'BIN-TENANT-A', 50000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&harness.product_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Create planned material in Tenant A
    let (status, resp_mat, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 5,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let mat_id = resp_mat["id"].as_str().unwrap().to_string();

    // Adversarial attack: Tenant B attempts to issue Tenant A's material
    let (status, resp_err, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, mat_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({ "quantity": 5 })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Cross-tenant material issue must return 404 NOT_FOUND"
    );
    assert_eq!(resp_err["code"], "NOT_FOUND");

    // Adversarial attack: Tenant B attempts direct issue pointing to Tenant A's project
    let (status, resp_dir_err, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 5,
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Cross-tenant direct issue must return 404 NOT_FOUND"
    );
    assert_eq!(resp_dir_err["code"], "NOT_FOUND");

    // Zero side-effects verification:
    // Tenant A stock remains 20
    let stock_a: i64 = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_items WHERE id = ?1",
    )
    .bind(&stock_item_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(stock_a, 20);

    // Tenant A material remains PLANNED
    let mat_status: String = sqlx::query_scalar(
        "SELECT status FROM project_materials WHERE id = ?1",
    )
    .bind(&mat_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mat_status, "PLANNED");

    // Tenant B has 0 movements, 0 journals, 0 outbox events
    let b_movs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_movements WHERE tenant_id = ?1",
    )
    .bind(&harness.tenant_b_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(b_movs, 0);

    let b_jnls: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_entries WHERE tenant_id = ?1",
    )
    .bind(&harness.tenant_b_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(b_jnls, 0);

    let b_outbox: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?1",
    )
    .bind(&harness.tenant_b_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(b_outbox, 0);
}
