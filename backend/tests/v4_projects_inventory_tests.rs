//! Integration Test Suite for Milestone 3:
//! Direct Job Costing Material Allocation & Inventory Integration
//! (PRD §10, §11, §27, §31, §60, §62, §72).
//!
//! Tests Features 12, 13, 14, 27:
//! 1. `test_material_issuance_happy_path`:
//!    Planned material requisition issuance transitions PLANNED -> ISSUED,
//!    atomically deducts warehouse stock, captures WAC unit cost, inserts OUTBOUND
//!    stock movement with reference_type "PROJECT_MATERIAL", posts balanced GL journal
//!    (Debit 5000 = Credit 1300), and emits transactional outbox event "ProjectMaterialIssued".
//! 2. `test_negative_stock_prevention_insufficient_stock_rejection`:
//!    Atomic negative balance prevention under BEGIN IMMEDIATE: requesting > available
//!    stock strictly aborts with HTTP 422 INSUFFICIENT_STOCK with zero side effects.
//! 3. `test_inactive_project_rejection`:
//!    Material issuance on non-active projects (DRAFT, ON_HOLD, COMPLETED, CANCELLED)
//!    strictly returns HTTP 422 PROJECT_NOT_ACTIVE.
//! 4. `test_material_state_machine_duplicate_issue_rejection`:
//!    Re-issuing an already ISSUED material strictly returns HTTP 422 MATERIAL_NOT_PLANNED.
//! 5. `test_direct_material_issue`:
//!    POST /api/v1/projects/:id/materials/issue creates material directly in ISSUED status,
//!    deducting physical stock, posting GL journal, and emitting outbox event.
//! 6. `test_multi_tenant_anti_enumeration_isolation`:
//!    Cross-tenant operations (cross-tenant material issue, cross-tenant warehouse, cross-tenant
//!    product) strictly return HTTP 404 NOT_FOUND.
//! 7. `test_rbac_staff_forbidden_from_issuing`:
//!    Staff role is forbidden (HTTP 403 FORBIDDEN), while Manager role can issue materials
//!    with automated system GL context elevation.
//! 8. `test_real_time_profitability_reflects_actual_material_cost`:
//!    Real-time profitability engine reflects material actual cost only once ISSUED,
//!    updating margins with pure integer Rupiah math.

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
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    #[allow(dead_code)]
    _dir: tempfile::TempDir,
    owner_token: String,
    manager_token: String,
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
    product_a_id: String,
    warehouse_a_id: String,
    product_b_id: String,
    warehouse_b_id: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("projects_m3_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 20,
        min_connections: 1,
        busy_timeout_ms: 5_000,
        acquire_timeout_secs: 10,
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

    let jwt_secret = "m3_projects_inventory_secret_key_1234567890_super_secret";
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
            email: "owner_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_a@contractor.com", "user", "premium")
        .unwrap();

    // 2. Manager User A
    let manager_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: manager_id.clone(),
            email: "manager_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Manager A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (manager_token, _) = jwt_engine
        .generate_token(&manager_id, "manager_a@contractor.com", "user", "premium")
        .unwrap();

    // 3. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_a@contractor.com", "user", "free")
        .unwrap();

    // 4. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Construction".to_string(),
            slug: "tenant-alpha-construction".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    // Seed Chart of Accounts for Tenant A
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

    // Warehouse & Product & Stock for Tenant A
    let warehouse_a_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
        VALUES (?1, ?2, 'WH-MAIN-A', 'Main Warehouse Alpha', 1, ?3, ?3);
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

    let stock_item_a_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 100, 0, 10, 'BIN-A1', 50000, ?5);
        "#,
    )
    .bind(&stock_item_a_id)
    .bind(&tenant_a_id)
    .bind(&warehouse_a_id)
    .bind(&product_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    // 5. Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "owner_b@beta-engineering.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "owner_b@beta-engineering.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Engineering".to_string(),
            slug: "tenant-beta-engineering".to_string(),
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

    let warehouse_b_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
        VALUES (?1, ?2, 'WH-MAIN-B', 'Main Warehouse Beta', 1, ?3, ?3);
        "#,
    )
    .bind(&warehouse_b_id)
    .bind(&tenant_b_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    let product_b_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO products (id, tenant_id, sku, name, unit, cost_price, sale_price, reorder_threshold, is_active, created_at, updated_at)
        VALUES (?1, ?2, 'PRD-STL-12MM', 'Besi Beton 12mm', 'BTG', 120000, 150000, 5, 1, ?3, ?3);
        "#,
    )
    .bind(&product_b_id)
    .bind(&tenant_b_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    let stock_item_b_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO stock_items (id, tenant_id, warehouse_id, product_id, quantity_on_hand, quantity_reserved, reorder_threshold, bin_location, average_cost, updated_at)
        VALUES (?1, ?2, ?3, ?4, 50, 0, 5, 'BIN-B1', 120000, ?5);
        "#,
    )
    .bind(&stock_item_b_id)
    .bind(&tenant_b_id)
    .bind(&warehouse_b_id)
    .bind(&product_b_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    TestHarness {
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
        product_b_id,
        warehouse_b_id,
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

/// Helper: creates and activates a project for testing
async fn create_active_project(h: &TestHarness) -> String {
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Tower Alpha Construction",
            "description": "Multi-story office tower",
            "customer_name": "PT Mega Properti",
            "billing_type": "MILESTONE",
            "budget_amount": 100_000_000,
            "contract_amount": 150_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = resp["id"].as_str().unwrap().to_string();

    // Transition from DRAFT -> ACTIVE
    let (status, update_resp, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "status": "ACTIVE"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_resp["status"], "ACTIVE");

    project_id
}

async fn get_stock_on_hand(pool: &sqlx::SqlitePool, warehouse_id: &str, product_id: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE warehouse_id = ?1 AND product_id = ?2;",
    )
    .bind(warehouse_id)
    .bind(product_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

async fn set_stock_on_hand(
    pool: &sqlx::SqlitePool,
    warehouse_id: &str,
    product_id: &str,
    qty: i64,
    avg_cost: i64,
) {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE stock_items SET quantity_on_hand = ?1, average_cost = ?2, updated_at = ?3 WHERE warehouse_id = ?4 AND product_id = ?5;",
    )
    .bind(qty)
    .bind(avg_cost)
    .bind(&now)
    .bind(warehouse_id)
    .bind(product_id)
    .execute(pool)
    .await
    .unwrap();
}

// ============================================================================
// Test 1: Material Issuance Happy Path (Atomic Deduct, WAC, GL & Outbox)
// ============================================================================

#[tokio::test]
async fn test_material_issuance_happy_path() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // 1. Initial stock on hand is 100 units at average_cost Rp 50,000
    let initial_stock = get_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id).await;
    assert_eq!(initial_stock, 100);

    // 2. Plan a project material requisition for 25 units
    let (status, plan_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 25,
            "is_billable": true,
            "notes": "Foundation cement batch 1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(plan_resp["status"], "PLANNED");
    assert_eq!(plan_resp["quantity_planned"], 25);
    assert_eq!(plan_resp["quantity_issued"], 0);
    assert_eq!(plan_resp["unit_cost"], 0);
    assert_eq!(plan_resp["total_cost"], 0);

    let material_id = plan_resp["id"].as_str().unwrap().to_string();

    // 3. Issue the planned material (defaulting to planned quantity = 25)
    let (status, issue_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "notes": "Issued by warehouse manager"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(issue_resp["id"], material_id);
    assert_eq!(issue_resp["status"], "ISSUED");
    assert_eq!(issue_resp["quantity_issued"], 25);
    assert_eq!(issue_resp["unit_cost"], 50000);
    assert_eq!(issue_resp["total_cost"], 1_250_000); // 25 * 50,000 = 1,250,000
    assert!(issue_resp["issued_at"].is_string());
    assert!(issue_resp["stock_movement_id"].is_string());
    assert!(issue_resp["journal_entry_id"].is_string());

    let stock_mov_id = issue_resp["stock_movement_id"].as_str().unwrap().to_string();
    let journal_id = issue_resp["journal_entry_id"].as_str().unwrap().to_string();

    // 4. Assert stock on hand is decremented by exactly 25: 100 - 25 = 75
    let remaining_stock = get_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id).await;
    assert_eq!(remaining_stock, 75);

    // 5. Assert stock movement audit trail
    let mov_row: (String, String, String, i64, i64) = sqlx::query_as(
        r#"
        SELECT movement_type, reference_type, reference_id, quantity, unit_cost
        FROM stock_movements
        WHERE id = ?1 AND tenant_id = ?2;
        "#,
    )
    .bind(&stock_mov_id)
    .bind(&h.tenant_a_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(mov_row.0, "OUTBOUND");
    assert_eq!(mov_row.1, "PROJECT_MATERIAL");
    assert_eq!(mov_row.2, material_id);
    assert_eq!(mov_row.3, 25);
    assert_eq!(mov_row.4, 50000);

    // 6. Assert General Ledger journal entry (Debit 5000 Beban Pokok = Credit 1300 Persediaan = 1,250,000)
    let journal_row: (String, String) = sqlx::query_as(
        r#"
        SELECT source_type, source_id
        FROM journal_entries
        WHERE id = ?1 AND tenant_id = ?2;
        "#,
    )
    .bind(&journal_id)
    .bind(&h.tenant_a_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(journal_row.0, "INVENTORY_OUTBOUND");
    assert_eq!(journal_row.1, material_id);

    let lines: Vec<(String, i64, i64)> = sqlx::query_as(
        r#"
        SELECT account_code, debit, credit
        FROM journal_lines
        WHERE journal_id = ?1
        ORDER BY debit DESC;
        "#,
    )
    .bind(&journal_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();

    assert_eq!(lines.len(), 2);
    // Line 1: Debit 5000 = 1,250,000
    assert_eq!(lines[0].0, "5000");
    assert_eq!(lines[0].1, 1_250_000);
    assert_eq!(lines[0].2, 0);
    // Line 2: Credit 1300 = 1,250,000
    assert_eq!(lines[1].0, "1300");
    assert_eq!(lines[1].1, 0);
    assert_eq!(lines[1].2, 1_250_000);

    // 7. Assert Transactional Outbox Event: ProjectMaterialIssued
    let outbox_row: (String, String, String, String) = sqlx::query_as(
        r#"
        SELECT event_type, aggregate_type, aggregate_id, payload_json
        FROM outbox_events
        WHERE aggregate_id = ?1 AND tenant_id = ?2;
        "#,
    )
    .bind(&material_id)
    .bind(&h.tenant_a_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(outbox_row.0, "ProjectMaterialIssued");
    assert_eq!(outbox_row.1, "ProjectMaterial");
    assert_eq!(outbox_row.2, material_id);

    let payload: Value = serde_json::from_str(&outbox_row.3).unwrap();
    assert_eq!(payload["material_id"], material_id);
    assert_eq!(payload["project_id"], project_id);
    assert_eq!(payload["product_id"], h.product_a_id);
    assert_eq!(payload["warehouse_id"], h.warehouse_a_id);
    assert_eq!(payload["quantity_issued"], 25);
    assert_eq!(payload["unit_cost"], 50000);
    assert_eq!(payload["total_cost"], 1_250_000);
    assert_eq!(payload["stock_movement_id"], stock_mov_id);
    assert_eq!(payload["journal_entry_id"], journal_id);
}

// ============================================================================
// Test 2: Negative Stock Prevention (Insufficient Stock Rejection HTTP 422)
// ============================================================================

#[tokio::test]
async fn test_negative_stock_prevention_insufficient_stock_rejection() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // Set stock on hand to exactly 20 units
    set_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id, 20, 50000).await;

    // Plan material for 50 units (50 > 20 available)
    let (status, plan_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 50
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let material_id = plan_resp["id"].as_str().unwrap().to_string();

    // Attempt to issue 50 units -> Must reject with HTTP 422 INSUFFICIENT_STOCK
    let (status, error_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error_resp["code"], "INSUFFICIENT_STOCK");

    // Invariant: Stock remains at 20, never drops below zero or decreases
    let current_stock = get_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id).await;
    assert_eq!(current_stock, 20);

    // Invariant: Zero side effects persisted in database
    let mov_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM stock_movements WHERE reference_id = ?1;",
    )
    .bind(&material_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(mov_count.0, 0);

    let journal_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM journal_entries WHERE source_id = ?1;",
    )
    .bind(&material_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(journal_count.0, 0);

    let outbox_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1;",
    )
    .bind(&material_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count.0, 0);

    // Invariant: Material status is still PLANNED
    let mat_status: (String,) = sqlx::query_as(
        "SELECT status FROM project_materials WHERE id = ?1;",
    )
    .bind(&material_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(mat_status.0, "PLANNED");
}

// ============================================================================
// Test 3: Inactive Project Rejection (HTTP 422 PROJECT_NOT_ACTIVE)
// ============================================================================

#[tokio::test]
async fn test_inactive_project_rejection() {
    let h = setup_harness().await;

    // 1. Project in DRAFT status
    let (status, draft_resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Draft Commercial Complex",
            "customer_name": "PT Properti Maju",
            "billing_type": "MILESTONE",
            "budget_amount": 50_000_000,
            "contract_amount": 75_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let draft_project_id = draft_resp["id"].as_str().unwrap().to_string();

    // Direct issue on DRAFT project -> 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", draft_project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // 2. Active project transitioned to ON_HOLD
    let project_id = create_active_project(&h).await;

    let (status, plan_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let material_id = plan_resp["id"].as_str().unwrap().to_string();

    // Put project ON_HOLD
    let (status, _, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ON_HOLD" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Attempt planned issue on ON_HOLD project -> 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // Transition ON_HOLD -> ACTIVE -> COMPLETED
    let (status, _, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "COMPLETED" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Attempt planned issue on COMPLETED project -> 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // Stock was never touched
    let stock = get_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id).await;
    assert_eq!(stock, 100);
}

// ============================================================================
// Test 4: Material State Machine (Duplicate Issue Rejection HTTP 422)
// ============================================================================

#[tokio::test]
async fn test_material_state_machine_duplicate_issue_rejection() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // 1. Plan material for 10 units
    let (status, plan_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let material_id = plan_resp["id"].as_str().unwrap().to_string();

    // 2. Issue material first time -> 200 OK
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(get_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id).await, 90);

    // 3. Re-issue the already ISSUED material -> Must fail with HTTP 422 MATERIAL_NOT_PLANNED
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "MATERIAL_NOT_PLANNED");

    // Invariant: Stock is NOT decremented twice (still 90)
    assert_eq!(get_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id).await, 90);

    // Invariant: Only 1 movement and 1 journal entry exist for this material
    let mov_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM stock_movements WHERE reference_id = ?1;",
    )
    .bind(&material_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(mov_count.0, 1);
}

// ============================================================================
// Test 5: Direct Material Issue Endpoint (POST /projects/:id/materials/issue)
// ============================================================================

#[tokio::test]
async fn test_direct_material_issue() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // Call direct issue endpoint
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity": 15,
            "is_billable": true,
            "notes": "Direct site emergency allocation"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp["status"], "ISSUED");
    assert_eq!(resp["quantity_planned"], 15);
    assert_eq!(resp["quantity_issued"], 15);
    assert_eq!(resp["unit_cost"], 50000);
    assert_eq!(resp["total_cost"], 750000); // 15 * 50,000 = 750,000
    assert!(resp["stock_movement_id"].is_string());
    assert!(resp["journal_entry_id"].is_string());
    assert!(resp["issued_at"].is_string());

    let material_id = resp["id"].as_str().unwrap().to_string();

    // Verify stock decremented: 100 - 15 = 85
    assert_eq!(get_stock_on_hand(&h.pool, &h.warehouse_a_id, &h.product_a_id).await, 85);

    // Verify GL journal entry posted with Debit 5000 / Credit 1300
    let journal_id = resp["journal_entry_id"].as_str().unwrap().to_string();
    let lines: Vec<(String, i64, i64)> = sqlx::query_as(
        r#"
        SELECT account_code, debit, credit
        FROM journal_lines
        WHERE journal_id = ?1
        ORDER BY debit DESC;
        "#,
    )
    .bind(&journal_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].0, "5000");
    assert_eq!(lines[0].1, 750_000);
    assert_eq!(lines[1].0, "1300");
    assert_eq!(lines[1].2, 750_000);

    // Verify Outbox Event emitted
    let outbox_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1 AND event_type = 'ProjectMaterialIssued';",
    )
    .bind(&material_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count.0, 1);
}

// ============================================================================
// Test 6: Multi-Tenant Anti-Enumeration Isolation (HTTP 404 Not Found)
// ============================================================================

#[tokio::test]
async fn test_multi_tenant_anti_enumeration_isolation() {
    let h = setup_harness().await;

    // Tenant Alpha creates active project and plans material
    let project_a_id = create_active_project(&h).await;
    let (status, plan_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_a_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let material_a_id = plan_resp["id"].as_str().unwrap().to_string();

    // 1. Tenant Beta attempts to issue Tenant Alpha's material -> 404 NOT_FOUND
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_a_id, material_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 2. Tenant Alpha attempts direct issue with Tenant Beta's foreign warehouse -> 404 NOT_FOUND
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_a_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_b_id, // Foreign warehouse
            "quantity": 5
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 3. Tenant Alpha attempts direct issue with Tenant Beta's foreign product -> 404 NOT_FOUND
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_a_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_b_id, // Foreign product
            "warehouse_id": h.warehouse_a_id,
            "quantity": 5
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ============================================================================
// Test 7: RBAC Matrix (Staff Forbidden HTTP 403 vs Manager Elevation)
// ============================================================================

#[tokio::test]
async fn test_rbac_staff_forbidden_from_issuing() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // Plan material using Owner
    let (status, plan_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let material_id = plan_resp["id"].as_str().unwrap().to_string();

    // 1. Staff role attempts to issue planned material -> HTTP 403 FORBIDDEN
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.staff_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_resp["code"], "FORBIDDEN");

    // 2. Staff role attempts direct material issue -> HTTP 403 FORBIDDEN
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/issue", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity": 5
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_resp["code"], "FORBIDDEN");

    // 3. Manager role CAN issue material (with automated system context elevation for GL)
    let (status, issue_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(issue_resp["status"], "ISSUED");
    assert!(issue_resp["journal_entry_id"].is_string());
}

// ============================================================================
// Test 8: Real-Time Profitability Synchronization with Actual Material Cost
// ============================================================================

#[tokio::test]
async fn test_real_time_profitability_reflects_actual_material_cost() {
    let h = setup_harness().await;

    // Create project: Budget = Rp 100M, Contract = Rp 150M
    let (status, p_resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Highway Interchange Project",
            "customer_name": "Dinas Bina Marga",
            "billing_type": "MILESTONE",
            "budget_amount": 100_000_000,
            "contract_amount": 150_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = p_resp["id"].as_str().unwrap().to_string();

    // Activate project
    let (status, _, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Seed a billed milestone: Rp 50,000,000 revenue
    let milestone_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO milestones (
            id, tenant_id, project_id, sequence_order, title, target_date,
            status, billable_amount, is_billed, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, 1, 'Site Clearance Complete', '2026-11-01',
            'COMPLETED', 50000000, 1, ?4, ?4
        );
        "#,
    )
    .bind(&milestone_id)
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(&now_str)
    .execute(&h.pool)
    .await
    .unwrap();

    // 1. Plan material for 100 units @ Rp 50,000 = Rp 5,000,000 planned cost
    let (status, plan_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 100
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let material_id = plan_resp["id"].as_str().unwrap().to_string();

    // Verify BEFORE issue: Planned material has 0 actual cost in profitability
    let (status, summary_before, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary_before["total_material_cost"], 0);
    assert_eq!(summary_before["total_actual_cost"], 0);
    assert_eq!(summary_before["total_billed_revenue"], 50_000_000);
    assert_eq!(summary_before["net_profit_amount"], 50_000_000);
    assert_eq!(summary_before["margin_percentage_basis_points"], 10000); // 100.00%

    // 2. Issue the material (100 units @ Rp 50,000 = Rp 5,000,000)
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials/{}/issue", project_id, material_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 3. Verify AFTER issue: Actual material cost immediately reflects Rp 5,000,000
    // Net profit = 50,000,000 - 5,000,000 = 45,000,000
    // Margin bps = (45,000,000 * 10,000) / 50,000,000 = 9000 bps (90.00%)
    let (status, summary_after, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary_after["total_material_cost"], 5_000_000);
    assert_eq!(summary_after["total_actual_cost"], 5_000_000);
    assert_eq!(summary_after["total_billed_revenue"], 50_000_000);
    assert_eq!(summary_after["net_profit_amount"], 45_000_000);
    assert_eq!(summary_after["margin_percentage_basis_points"], 9000);
}
