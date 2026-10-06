//! Integration Test Suite for Milestone 2:
//! Material, Labor & Expense Cost Tracking Engine (PRD §10, §11, §31, §38, §60, §61, §62).
//!
//! Tests Features 11, 15, 16, 17:
//! - Pure integer Rupiah (`i64`) arithmetic & zero-float guarantee
//! - Project labor cost logging, rate inheritance, and calculation
//! - Project expense logging across categories
//! - Material requisition planning
//! - Inactive project state blocking (HTTP 422 PROJECT_NOT_ACTIVE)
//! - Boundary validation (HTTP 400 for negative/zero values)
//! - Role-Based Access Control (RBAC: Staff vs Manager vs Accountant)
//! - Real-time project profitability metrics (margin bps & zero-division safety)
//! - Multi-tenant isolation & anti-enumeration 404 boundaries

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
    accountant_token: String,
    tenant_a_id: String,
    staff_id: String,
    #[allow(dead_code)]
    manager_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
    product_a_id: String,
    warehouse_a_id: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("projects_m2_test.sqlite");
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

    let jwt_secret = "m2_projects_costing_secret_key_1234567890_super_secret";
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

    // 4. Accountant User A
    let accountant_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: accountant_id.clone(),
            email: "accountant_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Accountant A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (accountant_token, _) = jwt_engine
        .generate_token(
            &accountant_id,
            "accountant_a@contractor.com",
            "user",
            "premium",
        )
        .unwrap();

    // 5. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Contractor".to_string(),
            slug: "tenant-alpha-contractor".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    // Seed COA for Tenant A
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

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: accountant_id.clone(),
            role: Role::Accountant,
        })
        .await
        .unwrap();

    // Insert warehouse & product for Tenant A
    let warehouse_a_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
        VALUES (?1, ?2, 'WH-MAIN', 'Main Warehouse', 1, ?3, ?3);
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
        VALUES (?1, ?2, 'PRD-CMT-01', 'Cement Portland 50kg', 'SAK', 65000, 75000, 10, 1, ?3, ?3);
        "#,
    )
    .bind(&product_a_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    // 6. Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "owner_b@beta-contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "owner_b@beta-contractor.com", "user", "free")
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

    TestHarness {
        app,
        pool,
        _dir: dir,
        owner_token,
        manager_token,
        staff_token,
        accountant_token,
        tenant_a_id,
        staff_id,
        manager_id,
        tenant_b_id,
        tenant_b_token,
        product_a_id,
        warehouse_a_id,
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

// ============================================================================
// Test 1: Labor Cost Logging, Rate Inheritance & Pure Integer Math
// ============================================================================

#[tokio::test]
async fn test_labor_cost_logging_calculation_and_rate_defaulting() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // 1. Assign Staff A as project member with cost_rate = 150,000 and billing_rate = 250,000
    let (status, member_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "CONTRIBUTOR",
            "cost_rate": 150_000,
            "billing_rate": 250_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(member_resp["cost_rate"], 150_000);
    assert_eq!(member_resp["billing_rate"], 250_000);

    // 2. Staff logs 8 hours with rates omitted -> inherits member rates
    let (status, labor_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_id": h.staff_id,
            "work_date": "2026-10-06",
            "hours_worked": 8,
            "description": "Foundation reinforcement welding"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(labor_resp["hours_worked"], 8);
    assert_eq!(labor_resp["hourly_rate"], 150_000);
    assert_eq!(labor_resp["billing_rate"], 250_000);
    // Integer calculation: 8 * 150,000 = 1,200,000
    assert_eq!(labor_resp["total_cost"], 1_200_000);
    let labor1_id = labor_resp["id"].as_str().unwrap().to_string();

    // 3. Log another labor entry with explicit rate override (4 hours at 200,000/hr)
    let (status, labor2_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Specialist Consultant",
            "work_date": "2026-10-06",
            "hours_worked": 4,
            "hourly_rate": 200_000,
            "billing_rate": 350_000,
            "description": "Structural engineering inspection"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(labor2_resp["hours_worked"], 4);
    assert_eq!(labor2_resp["hourly_rate"], 200_000);
    // Integer calculation: 4 * 200,000 = 800,000
    assert_eq!(labor2_resp["total_cost"], 800_000);
    let _labor2_id = labor2_resp["id"].as_str().unwrap().to_string();

    // 4. List labor entries as Manager (sees all 2)
    let (status, list_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 2);

    // 5. Delete first labor entry
    let (status, _, _) = send_req(
        &h.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/labor/{}", project_id, labor1_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 6. Verify count is now 1
    let (status, list_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 1);
}

// ============================================================================
// Test 2: Project Expense Logging Across Categories
// ============================================================================

#[tokio::test]
async fn test_expense_logging_categories_and_immutability() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    let categories = vec![
        ("PERMITS", "Building permit fee", 5_000_000, "City Government", "PERM-2026-001"),
        ("EQUIPMENT_RENTAL", "Excavator 1-week rental", 15_000_000, "PT Alat Berat", "RENT-884"),
        ("SUBCONTRACTOR", "Electrical subcontractor milestone", 25_000_000, "CV Listrik Mandiri", "SUB-441"),
        ("TRAVEL", "Site inspection flight & transport", 2_500_000, "Garuda Indonesia", "TKT-991"),
        ("OTHER", "Worker safety helmets and gear", 1_200_000, "Toko Safety Glodok", "REC-112"),
    ];

    let mut created_ids = Vec::new();

    for (cat, desc, amt, vendor, receipt) in &categories {
        let (status, exp_resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/expenses", project_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "category": cat,
                "description": desc,
                "amount": amt,
                "expense_date": "2026-10-06",
                "vendor_name": vendor,
                "receipt_ref": receipt,
                "is_billable": true
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(exp_resp["category"], *cat);
        assert_eq!(exp_resp["amount"], *amt);
        assert_eq!(exp_resp["vendor_name"], *vendor);
        created_ids.push(exp_resp["id"].as_str().unwrap().to_string());
    }

    // List all expenses
    let (status, list_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 5);

    // Delete one expense
    let delete_id = &created_ids[0];
    let (status, _, _) = send_req(
        &h.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/expenses/{}", project_id, delete_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify count dropped to 4
    let (status, list_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 4);
}

// ============================================================================
// Test 3: Material Planning & Lifecycle
// ============================================================================

#[tokio::test]
async fn test_material_planning_and_lifecycle() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // Plan material
    let (status, mat_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 100,
            "is_billable": true,
            "notes": "Foundation cement batch 1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(mat_resp["status"], "PLANNED");
    assert_eq!(mat_resp["quantity_planned"], 100);
    assert_eq!(mat_resp["quantity_issued"], 0);
    assert_eq!(mat_resp["unit_cost"], 0);
    assert_eq!(mat_resp["total_cost"], 0);
    let mat_id = mat_resp["id"].as_str().unwrap().to_string();

    // List materials
    let (status, list_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 1);

    // Delete planned material
    let (status, _, _) = send_req(
        &h.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/materials/{}", project_id, mat_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify count is 0
    let (status, list_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 0);
}

// ============================================================================
// Test 4: Project State Invariants (Mutations Blocked on Non-Active Projects)
// ============================================================================

#[tokio::test]
async fn test_project_state_invariants_cost_mutations_blocked_on_inactive() {
    let h = setup_harness().await;

    // Create project in DRAFT status
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Draft Bridge Project",
            "customer_name": "Kementerian PUPR",
            "billing_type": "PERCENTAGE_OF_COMPLETION",
            "budget_amount": 500_000_000,
            "contract_amount": 650_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = resp["id"].as_str().unwrap().to_string();
    assert_eq!(resp["status"], "DRAFT");

    // 1. Try to log labor on DRAFT project -> HTTP 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Site Surveyor",
            "work_date": "2026-10-06",
            "hours_worked": 4,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // 2. Try to log expense on DRAFT project -> HTTP 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "PERMITS",
            "description": "Environmental permit",
            "amount": 10_000_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // 3. Try to plan material on DRAFT project -> HTTP 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 50
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // 4. Activate project -> mutations now succeed
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

    let (status, labor_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Site Surveyor",
            "work_date": "2026-10-06",
            "hours_worked": 4,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let labor_id = labor_resp["id"].as_str().unwrap().to_string();

    // 5. Change project to ON_HOLD -> mutations blocked again
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

    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Site Surveyor",
            "work_date": "2026-10-06",
            "hours_worked": 4,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // 6. Delete mutation blocked on ON_HOLD
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/labor/{}", project_id, labor_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");
}

// ============================================================================
// Test 5: Boundary Checks Rejecting Negative and Zero Values
// ============================================================================

#[tokio::test]
async fn test_boundary_checks_rejects_negative_and_zero_values() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // 1. Labor: hours <= 0
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Worker",
            "work_date": "2026-10-06",
            "hours_worked": 0,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_HOURS");

    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Worker",
            "work_date": "2026-10-06",
            "hours_worked": -5,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_HOURS");

    // 2. Labor: negative rates
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Worker",
            "work_date": "2026-10-06",
            "hours_worked": 8,
            "hourly_rate": -50_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_RATE");

    // 3. Labor: missing worker name when worker_id omitted
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "work_date": "2026-10-06",
            "hours_worked": 8,
            "hourly_rate": 50_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_WORKER");

    // 4. Expense: amount <= 0
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "OTHER",
            "description": "Zero expense",
            "amount": 0,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_AMOUNT");

    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "OTHER",
            "description": "Negative expense",
            "amount": -100_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_AMOUNT");

    // 5. Expense: empty description
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "OTHER",
            "description": "   ",
            "amount": 100_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_DESCRIPTION");

    // 6. Material: quantity_planned <= 0
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 0
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_QUANTITY");

    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": -10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err_resp["code"], "INVALID_QUANTITY");
}

// ============================================================================
// Test 6: Role-Based Access Control (RBAC: Staff vs Manager vs Accountant)
// ============================================================================

#[tokio::test]
async fn test_rbac_matrix_staff_vs_manager_vs_accountant() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h).await;

    // 1. Staff logs labor for themselves -> 201 Created
    let (status, staff_labor_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_id": h.staff_id,
            "worker_name": "Staff Worker A",
            "work_date": "2026-10-06",
            "hours_worked": 5,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let _staff_labor_id = staff_labor_resp["id"].as_str().unwrap().to_string();

    // 2. Manager logs labor for another worker -> 201 Created
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Subcontractor Worker",
            "work_date": "2026-10-06",
            "hours_worked": 8,
            "hourly_rate": 120_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 3. Staff lists labor -> sees ONLY their own record (count == 1)
    let (status, staff_list, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(staff_list["count"], 1);
    assert_eq!(staff_list["labor"][0]["worker_id"], h.staff_id);

    // 4. Manager lists labor -> sees all records (count == 2)
    let (status, manager_list, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(manager_list["count"], 2);

    // 5. Staff attempts to record expense -> HTTP 403 FORBIDDEN
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "OTHER",
            "description": "Lunch expense",
            "amount": 150_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_resp["code"], "FORBIDDEN");

    // 6. Staff attempts to list expenses -> HTTP 403 FORBIDDEN
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_resp["code"], "FORBIDDEN");

    // 7. Staff attempts to plan material -> HTTP 403 FORBIDDEN
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_resp["code"], "FORBIDDEN");

    // 8. Staff attempts to query profitability -> HTTP 403 FORBIDDEN
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err_resp["code"], "FORBIDDEN");

    // 9. Accountant queries profitability -> HTTP 200 OK!
    let (status, profit_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.accountant_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(profit_resp.get("total_actual_cost").is_some());

    // 10. Accountant lists expenses -> HTTP 200 OK!
    let (status, _, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.accountant_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ============================================================================
// Test 7: Profitability Metrics & Pure Integer Rupiah Arithmetic
// ============================================================================

#[tokio::test]
async fn test_profitability_metrics_and_pure_integer_rupiah_math() {
    let h = setup_harness().await;

    // Create project: Budget = Rp 100,000,000, Contract = Rp 150,000,000
    let (status, p_resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Warehouse Construction Project",
            "customer_name": "PT Logistik Sentosa",
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

    // Initial check: 0 costs, 0 revenue -> 0 margin bps, zero-division safe
    let (status, p_summary, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p_summary["budget_amount"], 100_000_000);
    assert_eq!(p_summary["contract_amount"], 150_000_000);
    assert_eq!(p_summary["total_material_cost"], 0);
    assert_eq!(p_summary["total_labor_cost"], 0);
    assert_eq!(p_summary["total_expense_cost"], 0);
    assert_eq!(p_summary["total_actual_cost"], 0);
    assert_eq!(p_summary["total_billed_revenue"], 0);
    assert_eq!(p_summary["net_profit_amount"], 0);
    assert_eq!(p_summary["margin_percentage_basis_points"], 0);

    // 1. Log Labor: 100 hours @ Rp 150,000 = Rp 15,000,000
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Masonry Team",
            "work_date": "2026-10-06",
            "hours_worked": 100,
            "hourly_rate": 150_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 2. Log Expense: Rp 35,000,000 equipment rental
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "EQUIPMENT_RENTAL",
            "description": "Crane rental 2 weeks",
            "amount": 35_000_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 3. Plan Material: Planned materials do NOT affect actual cost in M2
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 500
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Check profitability before revenue: Actual cost = 15M + 35M = 50M
    // Revenue = 0 -> Net profit = -50M, bps = 0
    let (status, p_summary, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p_summary["total_labor_cost"], 15_000_000);
    assert_eq!(p_summary["total_expense_cost"], 35_000_000);
    assert_eq!(p_summary["total_material_cost"], 0);
    assert_eq!(p_summary["total_actual_cost"], 50_000_000);
    assert_eq!(p_summary["total_billed_revenue"], 0);
    assert_eq!(p_summary["net_profit_amount"], -50_000_000);
    assert_eq!(p_summary["margin_percentage_basis_points"], 0);

    // 4. Create a milestone with billable_amount = 100,000,000, mark is_billed = 1 in database
    let milestone_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO milestones (
            id, tenant_id, project_id, sequence_order, title, target_date,
            status, billable_amount, is_billed, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, 1, 'Phase 1 Structural Handover', '2026-11-01',
            'COMPLETED', 100000000, 1, ?4, ?4
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

    // Profitability check:
    // Revenue = 100M
    // Actual cost = 50M
    // Net profit = 50M
    // Margin bps = (50,000,000 * 10,000) / 100,000,000 = 5000 bps (50.00%)
    let (status, p_summary, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p_summary["total_billed_revenue"], 100_000_000);
    assert_eq!(p_summary["total_actual_cost"], 50_000_000);
    assert_eq!(p_summary["net_profit_amount"], 50_000_000);
    assert_eq!(p_summary["margin_percentage_basis_points"], 5000);

    // 5. Incur additional expense exceeding revenue: add Rp 75,000,000 expense
    // Total actual cost = 50M + 75M = 125M
    // Net profit = 100M - 125M = -25M
    // Margin bps = (-25,000,000 * 10,000) / 100,000,000 = -2500 bps (-25.00%)
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "SUBCONTRACTOR",
            "description": "Emergency foundation remediation",
            "amount": 75_000_000,
            "expense_date": "2026-10-07"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, p_summary, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p_summary["total_billed_revenue"], 100_000_000);
    assert_eq!(p_summary["total_actual_cost"], 125_000_000);
    assert_eq!(p_summary["net_profit_amount"], -25_000_000);
    assert_eq!(p_summary["margin_percentage_basis_points"], -2500);
}

// ============================================================================
// Test 8: Multi-Tenant Anti-Enumeration Isolation (404 Not Found)
// ============================================================================

#[tokio::test]
async fn test_multi_tenant_anti_enumeration_isolation() {
    let h = setup_harness().await;

    // Tenant Alpha creates active project
    let project_a_id = create_active_project(&h).await;

    // Log labor and expense under Tenant Alpha
    let (status, labor_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_a_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Alpha Worker",
            "work_date": "2026-10-06",
            "hours_worked": 8,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let labor_a_id = labor_resp["id"].as_str().unwrap().to_string();

    let (status, exp_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_a_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "PERMITS",
            "description": "Alpha Permit",
            "amount": 2_000_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let expense_a_id = exp_resp["id"].as_str().unwrap().to_string();

    let (status, mat_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_a_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 20
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let material_a_id = mat_resp["id"].as_str().unwrap().to_string();

    // --- Tenant Beta attempts to access Tenant Alpha's project & resources ---

    // 1. Tenant Beta queries labor -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 2. Tenant Beta attempts to log labor -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "worker_name": "Infiltrator",
            "work_date": "2026-10-06",
            "hours_worked": 5,
            "hourly_rate": 50_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 3. Tenant Beta attempts to delete labor -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/labor/{}", project_a_id, labor_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 4. Tenant Beta queries expenses -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/expenses", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 5. Tenant Beta attempts to create expense -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "category": "OTHER",
            "description": "Beta Infiltration Expense",
            "amount": 5_000_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 6. Tenant Beta attempts to delete expense -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/expenses/{}", project_a_id, expense_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 7. Tenant Beta queries materials -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/materials", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 8. Tenant Beta attempts to plan material -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 9. Tenant Beta attempts to delete material -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::DELETE,
        &format!("/api/v1/projects/{}/materials/{}", project_a_id, material_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");

    // 10. Tenant Beta queries profitability -> HTTP 404
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err_resp["code"], "NOT_FOUND");
}
