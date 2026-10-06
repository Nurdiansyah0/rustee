//! Milestone 3 Adversarial Concurrency & Stress Verification Suite:
//! Direct Job Costing Material Allocation & Inventory Integration
//! (PRD §10, §11, §27, §31, §60, §62, §72).
//!
//! Subagent: challenger_m3_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//! References: ORIGINAL_REQUEST.md, PROJECT.md
//!
//! Empirical Challenge Invariants:
//! 1. Concurrency & Race Condition Verification:
//!    - Parallel competing requests for finite stock under SQLite `BEGIN IMMEDIATE`.
//!    - Absolute guarantee that stock on hand NEVER drops below zero.
//!    - Excess competing requests receive HTTP 422 `INSUFFICIENT_STOCK`.
//!    - All successful issues have exact matching `OUTBOUND` movements and balanced GL journals (`Debit 5000 == Credit 1300`).
//! 2. Double-Submit Race Serialization on Identical Material:
//!    - Parallel concurrent issue calls on the exact same material ID: exactly one succeeds, others rejected with HTTP 422 `MATERIAL_NOT_PLANNED`.
//! 3. Extreme Arithmetic & Boundary Conditions:
//!    - High volume values, large quantities, integer Rupiah overflow resistance, exact basis point margins.
//! 4. Zero-Cost Material Issuance Edge Case:
//!    - Zero-dollar WAC items handled safely without corrupting double-entry GL journals.
//! 5. Multi-Project Stock Contention Under Multi-Tenant Isolation:
//!    - Cross-project contention in the same tenant serialized safely while foreign tenant attacks receive HTTP 404.
//! 6. Immutability & Audit Trail Integrity:
//!    - Issued materials cannot be deleted (HTTP 422 `CANNOT_DELETE_ISSUED_MATERIAL`).

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::project_costing::ProjectProfitabilitySummary;
use backend::domain::money::Rupiah;
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
}

async fn setup_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m3_concurrency.sqlite");
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

    let jwt_secret = "m3_challenger_concurrency_secret_key_1234567890_test";
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
            email: "owner_conc_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_conc_a@contractor.com", "user", "premium")
        .unwrap();

    // 2. Manager User A
    let manager_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: manager_id.clone(),
            email: "manager_conc_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Manager A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (manager_token, _) = jwt_engine
        .generate_token(&manager_id, "manager_conc_a@contractor.com", "user", "premium")
        .unwrap();

    // 3. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_conc_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_conc_a@contractor.com", "user", "free")
        .unwrap();

    // 4. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Concurrency".to_string(),
            slug: "tenant-alpha-concurrency".to_string(),
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

    // Warehouse & Products for Tenant A
    let warehouse_a_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
        VALUES (?1, ?2, 'WH-CONC-A', 'Main Concurrency Warehouse', 1, ?3, ?3);
        "#,
    )
    .bind(&warehouse_a_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    let product_a_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-CMT-50KG', 'Semen Portland 50kg', 'SAK', 50000, 75000, 10, 1, ?3, ?3);
        "#,
    )
    .bind(&product_a_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    let product_steel_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-STL-12MM', 'Besi Beton 12mm', 'BTG', 120000, 150000, 5, 1, ?3, ?3);
        "#,
    )
    .bind(&product_steel_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    // 5. Tenant B & User B for isolation attacks
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "adversary_b@beta-contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Adversary B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "adversary_b@beta-contractor.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Adversary".to_string(),
            slug: "tenant-beta-adversary".to_string(),
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
            "description": "Adversarial Stress Project",
            "customer_name": "PT Mega Properti",
            "billing_type": "MILESTONE",
            "budget_amount": 500_000_000,
            "contract_amount": 750_000_000
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
// Test 1: Concurrency Race on Direct Material Issues Competing for Finite Stock
// ============================================================================

#[tokio::test]
async fn test_concurrent_direct_material_issue_stock_exhaustion_race() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness, "High Concurrency Direct Issue").await;

    // Seed stock: exactly 15 units of Cement Portland @ 50,000 IDR
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 15, 0, 5, 'BIN-C1', 50000, ?5);
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

    // Adversarial Setup:
    // 16 concurrent tokio tasks, each requesting 2 units.
    // Total requested = 32 units, exceeding available stock (15 units) by 17 units.
    // Expected under BEGIN IMMEDIATE serialization:
    // - Exactly 7 tasks succeed (7 * 2 = 14 units deducted, leaving 1 unit).
    // - 1 task requests 2 units when only 1 unit is left -> rejected with HTTP 422 INSUFFICIENT_STOCK.
    // - 8 tasks request 2 units when 1 unit is left -> rejected with HTTP 422 INSUFFICIENT_STOCK.
    // Total: Exactly 7 succeed (HTTP 201), exactly 9 fail (HTTP 422).
    // Remaining stock MUST BE exactly 1, and NEVER negative.
    let num_tasks = 16;
    let mut handles = Vec::new();

    for i in 0..num_tasks {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/materials/issue", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "product_id": h.product_a_id,
                    "warehouse_id": h.warehouse_a_id,
                    "quantity": 2,
                    "is_billable": true,
                    "notes": format!("Concurrent direct issue #{}", i)
                })),
            )
            .await;
            (status, resp)
        }));
    }

    let mut success_count = 0;
    let mut insufficient_stock_count = 0;

    for handle in handles {
        let (status, resp) = handle.await.unwrap();
        match status {
            StatusCode::CREATED => {
                success_count += 1;
                assert_eq!(resp["quantity_issued"], 2);
                assert_eq!(resp["unit_cost"], 50000);
                assert_eq!(resp["total_cost"], 100000);
                assert_eq!(resp["status"], "ISSUED");
            }
            StatusCode::UNPROCESSABLE_ENTITY => {
                insufficient_stock_count += 1;
                assert_eq!(
                    resp["code"], "INSUFFICIENT_STOCK",
                    "Expected INSUFFICIENT_STOCK error code, got: {:?}",
                    resp
                );
            }
            other => panic!("Unexpected HTTP status under concurrency: {}", other),
        }
    }

    assert_eq!(
        success_count, 7,
        "Exactly 7 requests must succeed (14 units deducted from 15)"
    );
    assert_eq!(
        insufficient_stock_count, 9,
        "Exactly 9 requests must receive HTTP 422 INSUFFICIENT_STOCK"
    );

    // Strict Database Verifications:
    // 1. Stock on hand is exactly 1 (15 - 14 = 1), NEVER negative
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let final_on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(final_on_hand, 1, "Stock on hand must be exactly 1");
    assert!(final_on_hand >= 0, "Stock on hand MUST NEVER be negative!");

    // 2. Exactly 7 OUTBOUND StockMovement records created
    let mov_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM stock_movements
        WHERE tenant_id = ?1 AND reference_type = 'PROJECT_MATERIAL' AND movement_type = 'OUTBOUND'
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mov_count, 7, "Exactly 7 stock movements must be recorded");

    // 3. Balanced double-entry GL journal entries: Debit 5000 == Credit 1300
    let journal_rows = sqlx::query(
        r#"
        SELECT jl.account_code, SUM(jl.debit) as total_debit, SUM(jl.credit) as total_credit
        FROM journal_lines jl
        JOIN journal_entries je ON je.id = jl.journal_id
        WHERE je.tenant_id = ?1 AND je.source_type = 'INVENTORY_OUTBOUND'
        GROUP BY jl.account_code
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    let mut sum_debit_5000: i64 = 0;
    let mut sum_credit_1300: i64 = 0;
    for row in journal_rows {
        let acct: String = row.get("account_code");
        let deb: i64 = row.get("total_debit");
        let cr: i64 = row.get("total_credit");
        if acct == "5000" {
            sum_debit_5000 = deb;
            assert_eq!(cr, 0, "Account 5000 must have 0 credit");
        } else if acct == "1300" {
            sum_credit_1300 = cr;
            assert_eq!(deb, 0, "Account 1300 must have 0 debit");
        }
    }

    assert_eq!(
        sum_debit_5000, 700_000,
        "Debit 5000 must equal exactly 7 * 100,000 = 700,000"
    );
    assert_eq!(
        sum_credit_1300, 700_000,
        "Credit 1300 must equal exactly 7 * 100,000 = 700,000"
    );
    assert_eq!(
        sum_debit_5000, sum_credit_1300,
        "FINANCIAL INVARIANT: SUM(debit) MUST EQUAL SUM(credit)"
    );

    // 4. Outbox events: exactly 7 ProjectMaterialIssued events
    let outbox_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM outbox_events
        WHERE tenant_id = ?1 AND event_type = 'ProjectMaterialIssued'
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 7, "Exactly 7 outbox events must be emitted");

    // 5. Profitability engine reflects exactly 700,000 IDR material cost
    let (status, prof_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_resp["total_material_cost"], 700_000);
    assert_eq!(prof_resp["total_actual_cost"], 700_000);
}

// ============================================================================
// Test 2: Concurrency Race on Planned Material Requisitions Competing for Stock
// ============================================================================

#[tokio::test]
async fn test_concurrent_planned_requisition_issues_competing_for_finite_stock() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness, "Planned Requisition Race").await;

    // Seed stock: exactly 10 units of Besi Beton 12mm @ 120,000 IDR
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 10, 0, 5, 'BIN-S1', 120000, ?5);
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

    // Create 10 distinct planned material items, each planned for 2 units (Total = 20 units)
    let num_materials = 10;
    let mut material_ids = Vec::new();
    for i in 0..num_materials {
        let (status, resp, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials", project_id),
            &harness.owner_token,
            &harness.tenant_a_id,
            Some(json!({
                "product_id": harness.product_steel_id,
                "warehouse_id": harness.warehouse_a_id,
                "quantity_planned": 2,
                "is_billable": true,
                "notes": format!("Planned batch #{}", i)
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(resp["status"], "PLANNED");
        material_ids.push(resp["id"].as_str().unwrap().to_string());
    }

    // Concurrently issue all 10 planned materials simultaneously
    // Total stock is 10 units. Each needs 2 units.
    // Exactly 5 must succeed (5 * 2 = 10 units deducted).
    // Exactly 5 must fail with HTTP 422 INSUFFICIENT_STOCK.
    let mut handles = Vec::new();
    for mat_id in material_ids.clone() {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/materials/{}/issue", pid, mat_id),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({ "quantity": 2 })),
            )
            .await;
            (mat_id, status, resp)
        }));
    }

    let mut success_count = 0;
    let mut failure_count = 0;
    let mut issued_mat_ids = Vec::new();
    let mut planned_mat_ids = Vec::new();

    for handle in handles {
        let (mat_id, status, resp) = handle.await.unwrap();
        match status {
            StatusCode::OK => {
                success_count += 1;
                assert_eq!(resp["status"], "ISSUED");
                assert_eq!(resp["quantity_issued"], 2);
                assert_eq!(resp["total_cost"], 240000);
                issued_mat_ids.push(mat_id);
            }
            StatusCode::UNPROCESSABLE_ENTITY => {
                failure_count += 1;
                assert_eq!(resp["code"], "INSUFFICIENT_STOCK");
                planned_mat_ids.push(mat_id);
            }
            other => panic!("Unexpected status: {}", other),
        }
    }

    assert_eq!(success_count, 5, "Exactly 5 planned issues must succeed");
    assert_eq!(failure_count, 5, "Exactly 5 planned issues must fail");

    // Stock on hand must be exactly 0, NEVER negative
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let final_on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(final_on_hand, 0, "Stock on hand must be exactly 0");

    // Verify materials in DB: exactly 5 ISSUED, exactly 5 PLANNED
    let issued_db_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_materials WHERE project_id = ?1 AND status = 'ISSUED'",
    )
    .bind(&project_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(issued_db_count, 5);

    let planned_db_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_materials WHERE project_id = ?1 AND status = 'PLANNED'",
    )
    .bind(&project_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(planned_db_count, 5);

    // Failed materials must have quantity_issued == 0 and no stock movement
    for p_id in planned_mat_ids {
        let row = sqlx::query(
            "SELECT quantity_issued, stock_movement_id, journal_entry_id FROM project_materials WHERE id = ?1",
        )
        .bind(&p_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
        let q_issued: i64 = row.get("quantity_issued");
        let sm_id: Option<String> = row.get("stock_movement_id");
        let je_id: Option<String> = row.get("journal_entry_id");
        assert_eq!(q_issued, 0, "Failed material must have 0 issued quantity");
        assert!(sm_id.is_none(), "Failed material must have no stock movement");
        assert!(je_id.is_none(), "Failed material must have no journal entry");
    }
}

// ============================================================================
// Test 3: Concurrent Double-Issue Race on the Same Planned Material Record
// ============================================================================

#[tokio::test]
async fn test_concurrent_double_issue_race_on_single_planned_material() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness, "Double-Issue Race Project").await;

    // Seed ample stock: 100 units
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 100, 0, 5, 'BIN-C2', 50000, ?5);
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

    // Create a single planned material for 10 units
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 10,
            "is_billable": true,
            "notes": "Single planned material under double-issue attack"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let target_material_id = resp["id"].as_str().unwrap().to_string();

    // Adversarial Setup:
    // Spawn 10 concurrent tokio tasks, ALL calling issue on this exact same material ID!
    let num_tasks = 10;
    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        let mid = target_material_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/materials/{}/issue", pid, mid),
                &h.manager_token,
                &h.tenant_a_id,
                None, // default planned quantity
            )
            .await;
            (status, resp)
        }));
    }

    let mut success_count = 0;
    let mut not_planned_count = 0;

    for handle in handles {
        let (status, resp) = handle.await.unwrap();
        match status {
            StatusCode::OK => {
                success_count += 1;
                assert_eq!(resp["status"], "ISSUED");
                assert_eq!(resp["quantity_issued"], 10);
            }
            StatusCode::UNPROCESSABLE_ENTITY => {
                not_planned_count += 1;
                assert_eq!(
                    resp["code"], "MATERIAL_NOT_PLANNED",
                    "Expected MATERIAL_NOT_PLANNED, got: {:?}",
                    resp
                );
            }
            other => panic!("Unexpected status: {}", other),
        }
    }

    assert_eq!(
        success_count, 1,
        "CRITICAL INVARIANT: Exactly ONE issue request must succeed!"
    );
    assert_eq!(
        not_planned_count, 9,
        "CRITICAL INVARIANT: All 9 competing requests must be rejected!"
    );

    // Stock on hand must be exactly 90 (100 - 10 = 90), NEVER double-deducted
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let final_on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(
        final_on_hand, 90,
        "Stock on hand must be exactly 90 (deducted once)"
    );

    // Stock movements for this material ID: exactly 1
    let mov_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_movements WHERE reference_id = ?1",
    )
    .bind(&target_material_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mov_count, 1, "Exactly 1 stock movement must exist");
}

// ============================================================================
// Test 4: Extreme Arithmetic & Boundary Conditions: Overflow Resistance & BPS
// ============================================================================

#[tokio::test]
async fn test_extreme_volume_arithmetic_rupiah_overflow_resistance() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness, "Megaproject Extreme Arithmetic").await;

    // High unit cost product: Rp 50,000,000 per unit (50 Juta IDR)
    let mega_prod_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-TURBINE-XL', 'Turbin Pembangkit Listrik XL', 'UNIT', 50000000, 75000000, 1, 1, ?3, ?3);
        "#,
    )
    .bind(&mega_prod_id)
    .bind(&harness.tenant_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // High quantity stock: 1,000,000 units on hand @ Rp 50,000,000 average cost
    // Total stock valuation: 50,000,000,000,000 IDR (50 Trillion IDR)
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 1000000, 0, 10, 'YARD-TURBINE', 50000000, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&mega_prod_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Issue 500,000 units:
    // Total cost = 500,000 * 50,000,000 = 25,000,000,000,000 IDR (25 Trillion IDR)
    // Fits securely within signed 64-bit integer i64 (max ~ 9.22 * 10^18)
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": mega_prod_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 500_000,
            "is_billable": true,
            "notes": "Issue 500k turbines"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp["quantity_issued"], 500_000);
    assert_eq!(resp["unit_cost"], 50_000_000);
    assert_eq!(resp["total_cost"], 25_000_000_000_000i64);

    let mat_id = resp["id"].as_str().unwrap();

    // Verify stock on hand decremented: 1,000,000 - 500,000 = 500,000
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let remaining_on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(remaining_on_hand, 500_000);

    // Verify GL journal entry: Debit 5000 = 25 Trillion, Credit 1300 = 25 Trillion
    let journal_id_str = resp["journal_entry_id"].as_str().expect("Must have journal entry");
    let lines = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ?1",
    )
    .bind(journal_id_str)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    assert_eq!(lines.len(), 2);
    for line in lines {
        let code: String = line.get("account_code");
        let deb: i64 = line.get("debit");
        let cr: i64 = line.get("credit");
        if code == "5000" {
            assert_eq!(deb, 25_000_000_000_000i64);
            assert_eq!(cr, 0);
        } else if code == "1300" {
            assert_eq!(deb, 0);
            assert_eq!(cr, 25_000_000_000_000i64);
        }
    }

    // Verify Profitability API aggregation under extreme i64 values
    let (status, prof_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_resp["total_material_cost"], 25_000_000_000_000i64);
    assert_eq!(prof_resp["total_actual_cost"], 25_000_000_000_000i64);

    // Deep Boundary Test on pure integer margin calculation:
    // Profitability margin basis points: (profit * 10,000) / revenue
    // 1. High Trillion IDR margin:
    // Revenue: 40,000,000,000,000 IDR (40 Trillion)
    // Actual Cost: 25,000,000,000,000 IDR (25 Trillion)
    // Profit: 15,000,000,000,000 IDR (15 Trillion)
    // Expected BPS: (15T * 10,000) / 40T = 3,750 bps (37.50%)
    let bps_40t = ProjectProfitabilitySummary::calculate_margin_bps(
        Rupiah::new(15_000_000_000_000),
        Rupiah::new(40_000_000_000_000),
    );
    assert_eq!(bps_40t, 3750, "Margin bps must be exactly 3750 bps (37.50%)");

    // 2. Exact 1 basis point (0.01%):
    // Profit = 1 IDR, Revenue = 10,000 IDR -> (1 * 10,000) / 10,000 = 1 bps
    let bps_1 = ProjectProfitabilitySummary::calculate_margin_bps(
        Rupiah::new(1),
        Rupiah::new(10_000),
    );
    assert_eq!(bps_1, 1, "Exact 1 bps resolution");

    // 3. Zero revenue: must return 0 bps without division by zero panic
    let bps_zero_rev = ProjectProfitabilitySummary::calculate_margin_bps(
        Rupiah::new(-500_000),
        Rupiah::ZERO,
    );
    assert_eq!(bps_zero_rev, 0, "Zero revenue must safely yield 0 bps");

    // 4. Negative profit (net loss):
    // Net profit = -5,000,000,000,000 IDR (-5 Trillion), Revenue = 20,000,000,000,000 IDR (20 Trillion)
    // Expected BPS: (-5T * 10,000) / 20T = -2,500 bps (-25.00%)
    let bps_neg = ProjectProfitabilitySummary::calculate_margin_bps(
        Rupiah::new(-5_000_000_000_000),
        Rupiah::new(20_000_000_000_000),
    );
    assert_eq!(bps_neg, -2500, "Negative profit must yield exact negative bps");

    // 5. Extreme Quadrillion IDR boundary (testing i128 intermediate safety):
    // Profit = 9_000_000_000_000_000 (9 Quadrillion IDR)
    // Revenue = 10_000_000_000_000_000 (10 Quadrillion IDR)
    // Numerator in i128 = 9 * 10^15 * 10,000 = 9 * 10^19 (well under i128::MAX ~ 1.7 * 10^38)
    // Expected BPS: 9,000 bps (90.00%)
    let bps_quad = ProjectProfitabilitySummary::calculate_margin_bps(
        Rupiah::new(9_000_000_000_000_000),
        Rupiah::new(10_000_000_000_000_000),
    );
    assert_eq!(bps_quad, 9000, "Quadrillion IDR margin must not overflow i128");

    // Cannot delete issued megaproject material
    let (status, del_resp, _) = send_req(
        &harness.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/materials/{}", project_id, mat_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(del_resp["code"], "CANNOT_DELETE_ISSUED_MATERIAL");
}

// ============================================================================
// Test 5: Zero-Cost Sample Inventory Material Issuance Edge Case
// ============================================================================

#[tokio::test]
async fn test_zero_cost_sample_inventory_material_issuance() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness, "Zero Cost Material Project").await;

    // Create free sample promotional product: cost_price = 0, average_cost = 0
    let free_prod_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-SAMPLE-FREE', 'Sample Uji Laboratorium Semen', 'PAK', 0, 0, 5, 1, ?3, ?3);
        "#,
    )
    .bind(&free_prod_id)
    .bind(&harness.tenant_a_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 20, 0, 5, 'BIN-SMPL', 0, ?5);
        "#,
    )
    .bind(&stock_item_id)
    .bind(&harness.tenant_a_id)
    .bind(&harness.warehouse_a_id)
    .bind(&free_prod_id)
    .bind(&now_str)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Direct issue 5 units of free sample product
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": free_prod_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 5,
            "is_billable": false,
            "notes": "Testing zero-cost sample material"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp["quantity_issued"], 5);
    assert_eq!(resp["unit_cost"], 0);
    assert_eq!(resp["total_cost"], 0);
    assert_eq!(resp["status"], "ISSUED");

    // Stock on hand decremented properly: 20 -> 15
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(on_hand, 15);

    // StockMovement created
    let mov_id = resp["stock_movement_id"].as_str().expect("Movement must exist");
    let mov_row = sqlx::query("SELECT quantity, unit_cost FROM stock_movements WHERE id = ?1")
        .bind(mov_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let mov_qty: i64 = mov_row.get("quantity");
    let mov_cost: Option<i64> = mov_row.get("unit_cost");
    assert_eq!(mov_qty, 5);
    assert_eq!(mov_cost, Some(0));

    // Zero-cost material safely skips GL journal posting (preventing empty/zero journal error)
    assert!(
        resp["journal_entry_id"].is_null(),
        "Zero-cost material must safely have null journal_entry_id"
    );

    // Outbox event was emitted with total_cost = 0
    let outbox_row = sqlx::query(
        "SELECT payload_json FROM outbox_events WHERE aggregate_id = ?1",
    )
    .bind(resp["id"].as_str().unwrap())
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    let payload_str: String = outbox_row.get("payload_json");
    let payload: Value = serde_json::from_str(&payload_str).unwrap();
    assert_eq!(payload["total_cost"], 0);
    assert_eq!(payload["unit_cost"], 0);
}

// ============================================================================
// Test 6: Concurrent Multi-Project Contention Under Multi-Tenant Isolation
// ============================================================================

#[tokio::test]
async fn test_concurrent_multi_project_stock_contention_under_cross_tenant_isolation() {
    let harness = Arc::new(setup_harness().await);
    let project_alpha_1 = create_active_project(&harness, "Alpha Tower 1").await;
    let project_alpha_2 = create_active_project(&harness, "Alpha Tower 2").await;

    // Seed shared stock in Tenant A: exactly 6 units of Cement
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 6, 0, 2, 'BIN-SHARED', 50000, ?5);
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

    // Adversarial Multi-Vector Load:
    // - 5 concurrent tasks from Project Alpha 1 (1 unit each)
    // - 5 concurrent tasks from Project Alpha 2 (1 unit each)
    // - 5 concurrent tasks from Adversary Tenant Beta attempting to issue from Tenant A's warehouse!
    // Total Tenant A requests = 10 (competing for 6 units).
    // Total Tenant B requests = 5.
    let mut handles = Vec::new();

    // Vector 1: Project Alpha 1
    for i in 0..5 {
        let h = Arc::clone(&harness);
        let p1 = project_alpha_1.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/materials/issue", p1),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "product_id": h.product_a_id,
                    "warehouse_id": h.warehouse_a_id,
                    "quantity": 1,
                    "is_billable": true,
                    "notes": format!("Project 1 issue #{}", i)
                })),
            )
            .await;
            ("alpha_1", status, resp)
        }));
    }

    // Vector 2: Project Alpha 2
    for i in 0..5 {
        let h = Arc::clone(&harness);
        let p2 = project_alpha_2.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/materials/issue", p2),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "product_id": h.product_a_id,
                    "warehouse_id": h.warehouse_a_id,
                    "quantity": 1,
                    "is_billable": true,
                    "notes": format!("Project 2 issue #{}", i)
                })),
            )
            .await;
            ("alpha_2", status, resp)
        }));
    }

    // Vector 3: Cross-Tenant Attack from Tenant Beta
    for i in 0..5 {
        let h = Arc::clone(&harness);
        let p1 = project_alpha_1.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/materials/issue", p1),
                &h.tenant_b_token,
                &h.tenant_b_id, // Foreign tenant!
                Some(json!({
                    "product_id": h.product_a_id,
                    "warehouse_id": h.warehouse_a_id,
                    "quantity": 1,
                    "is_billable": true,
                    "notes": format!("Tenant Beta malicious issue #{}", i)
                })),
            )
            .await;
            ("beta_attack", status, resp)
        }));
    }

    let mut tenant_a_successes = 0;
    let mut tenant_a_insufficient = 0;
    let mut tenant_a_lock_contention = 0;
    let mut tenant_b_not_founds = 0;

    for handle in handles {
        let (origin, status, resp) = handle.await.unwrap();
        match origin {
            "alpha_1" | "alpha_2" => match status {
                StatusCode::CREATED => tenant_a_successes += 1,
                StatusCode::UNPROCESSABLE_ENTITY => {
                    assert_eq!(resp["code"], "INSUFFICIENT_STOCK");
                    tenant_a_insufficient += 1;
                }
                StatusCode::CONFLICT => {
                    assert_eq!(resp["code"], "LOCK_CONTENTION");
                    tenant_a_lock_contention += 1;
                }
                other => panic!("Unexpected status from Tenant A: {:?} with body: {:?}", other, resp),
            },
            "beta_attack" => {
                assert_eq!(
                    status,
                    StatusCode::NOT_FOUND,
                    "Foreign tenant attack MUST receive HTTP 404 NOT_FOUND"
                );
                assert_eq!(resp["code"], "NOT_FOUND");
                tenant_b_not_founds += 1;
            }
            _ => unreachable!(),
        }
    }

    assert_eq!(
        tenant_a_successes + tenant_a_insufficient + tenant_a_lock_contention,
        10,
        "All 10 requests from Tenant A must resolve cleanly"
    );
    assert!(
        tenant_a_successes <= 6,
        "Successful issues cannot exceed initial available stock of 6"
    );
    assert_eq!(
        tenant_b_not_founds, 5,
        "100% of foreign tenant attacks must receive HTTP 404 NOT_FOUND"
    );

    // Stock on hand in Tenant A must be exactly (6 - tenant_a_successes), NEVER negative!
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(on_hand, 6 - tenant_a_successes);
    assert!(on_hand >= 0, "Stock on hand MUST NEVER drop below zero!");

    // Stock movements in Tenant A must match exactly the number of successes
    let mov_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_movements WHERE tenant_id = ?1 AND reference_type = 'PROJECT_MATERIAL'",
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mov_count, tenant_a_successes);

    // Zero stock movements in Tenant B
    let b_mov_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_movements WHERE tenant_id = ?1",
    )
    .bind(&harness.tenant_b_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(b_mov_count, 0, "Tenant B must have zero stock movements");
}

// ============================================================================
// Test 7: Boundary and Adversarial Quantity Rejections
// ============================================================================

#[tokio::test]
async fn test_boundary_and_adversarial_quantity_rejections() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness, "Boundary Quantities Project").await;

    // Seed stock: exactly 5 units
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 5, 0, 1, 'BIN-BND', 50000, ?5);
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

    // Case 1: quantity = 0 -> HTTP 400 INVALID_QUANTITY
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 0
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], "INVALID_QUANTITY");

    // Case 2: quantity = -1 -> HTTP 400 INVALID_QUANTITY
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": -1
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], "INVALID_QUANTITY");

    // Case 3: quantity = i64::MIN -> HTTP 400 INVALID_QUANTITY
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": i64::MIN
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], "INVALID_QUANTITY");

    // Case 4: quantity = i64::MAX -> HTTP 422 INSUFFICIENT_STOCK (no arithmetic overflow panic!)
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": i64::MAX
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(resp["code"], "INSUFFICIENT_STOCK");

    // Case 5: quantity = 6 (available = 5, off-by-one boundary) -> HTTP 422 INSUFFICIENT_STOCK
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 6
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(resp["code"], "INSUFFICIENT_STOCK");

    // Case 6: quantity = 5 (exact available amount) -> HTTP 201 CREATED
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 5
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp["quantity_issued"], 5);

    // Stock on hand is now 0
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE id = ?1")
        .bind(&stock_item_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(on_hand, 0);

    // Case 7: Subsequent request for 1 unit when stock is 0 -> HTTP 422 INSUFFICIENT_STOCK
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity": 1
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(resp["code"], "INSUFFICIENT_STOCK");
}

// ============================================================================
// Test 8: Immutability and Audit Trail Protection of Issued Materials
// ============================================================================

#[tokio::test]
async fn test_issued_material_immutability_and_audit_trail_integrity() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness, "Immutability Audit Project").await;

    // Seed stock: 20 units
    let now_str = chrono::Utc::now().to_rfc3339();
    let stock_item_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 20, 0, 5, 'BIN-IMMUT', 50000, ?5);
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

    // 1. Create a planned material
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 5,
            "is_billable": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let planned_mat_id = resp["id"].as_str().unwrap().to_string();

    // 2. Planned material CAN be deleted (HTTP 204 No Content)
    let (status, _, _) = send_req(
        &harness.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/materials/{}", project_id, planned_mat_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify it is gone
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_materials WHERE id = ?1")
        .bind(&planned_mat_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // 3. Issue a direct material
    let (status, resp, _) = send_req(
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
            "notes": "Direct issue for immutability test"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let issued_mat_id = resp["id"].as_str().unwrap().to_string();
    let journal_id = resp["journal_entry_id"].as_str().unwrap().to_string();
    let mov_id = resp["stock_movement_id"].as_str().unwrap().to_string();

    // 4. Attempt to DELETE the issued material -> strictly rejected with HTTP 422
    let (status, resp, _) = send_req(
        &harness.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/materials/{}", project_id, issued_mat_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        resp["code"], "CANNOT_DELETE_ISSUED_MATERIAL",
        "Must strictly protect issued material records from deletion"
    );

    // 5. Verify database integrity: material, stock movement, and journal entries are intact
    let mat_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_materials WHERE id = ?1")
        .bind(&issued_mat_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(mat_exists, 1);

    let mov_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movements WHERE id = ?1")
        .bind(&mov_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(mov_exists, 1);

    let je_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries WHERE id = ?1")
        .bind(&journal_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(je_exists, 1);

    // 6. Direct SQL immutability test: attempt to DELETE posted journal entry directly via SQL
    // Database trigger trg_prevent_posted_journal_delete MUST abort this mutation!
    let trigger_res = sqlx::query("DELETE FROM journal_entries WHERE id = ?1")
        .bind(&journal_id)
        .execute(&harness.pool)
        .await;
    assert!(
        trigger_res.is_err(),
        "SQLite trigger trg_prevent_posted_journal_delete must reject direct SQL delete on posted journal!"
    );
}
