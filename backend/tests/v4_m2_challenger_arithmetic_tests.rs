//! Adversarial Empirical Verification Test Suite for Milestone 2:
//! Costing Integer Arithmetic, High-Magnitude Stress & Boundary Conditions.
//!
//! Subagent: challenger_m2_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//! References: PRD §10, §11, §31, §38, §60, §61, §62, PROJECT.md
//!
//! Test Suite Invariants:
//! 1. Pure integer Rupiah arithmetic & multi-billion/multi-trillion stress testing (no float loss or overflow).
//! 2. Profitability margin calculation across extreme scenarios (zero revenue, extreme profit, deep loss, breakeven).
//! 3. Boundary input fuzzing and strict rejection:
//!    - hours_worked <= 0 -> HTTP 400 INVALID_HOURS
//!    - hourly_rate / billing_rate < 0 -> HTTP 400 INVALID_RATE
//!    - amount <= 0 -> HTTP 400 INVALID_AMOUNT
//!    - quantity_planned <= 0 -> HTTP 400 INVALID_QUANTITY
//!    - floating-point payload injection -> Strict rejection by JSON deserializer / Axum.
//! 4. SQLite database check constraints defense-in-depth verification.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::{
    money::Rupiah,
    project_costing::{
        CreateProjectExpenseRequest, CreateProjectMaterialRequest,
        LogProjectLaborRequest, ProjectLabor, ProjectMaterial,
        ProjectProfitabilitySummary,
    },
    tenant::Role,
};
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

struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    #[allow(dead_code)]
    _dir: tempfile::TempDir,
    owner_token: String,
    manager_token: String,
    tenant_a_id: String,
    product_a_id: String,
    warehouse_a_id: String,
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m2_arithmetic.sqlite");
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

    let jwt_secret = "m2_challenger_secret_key_1234567890_empirical_adversarial";
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

    // 1. Owner User
    let owner_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_id.clone(),
            email: "owner@challenger.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner Challenger".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner@challenger.com", "user", "premium")
        .unwrap();

    // 2. Manager User
    let manager_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: manager_id.clone(),
            email: "manager@challenger.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Manager Challenger".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (manager_token, _) = jwt_engine
        .generate_token(&manager_id, "manager@challenger.com", "user", "premium")
        .unwrap();

    // 3. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Challenger Operations".to_string(),
            slug: "tenant-challenger-ops".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx = pool.begin().await.unwrap();
    backend::repository::accounting_repo::SqlxAccountingRepository::seed_default_accounts_tx(
        &mut tx,
        &tenant_a_id,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

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

    // 4. Warehouse & Product
    let warehouse_a_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
        VALUES (?1, ?2, 'WH-MAIN', 'Main Construction Yard', 1, ?3, ?3);
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
        VALUES (?1, ?2, 'BEAM-001', 'High-Grade Steel Beam', 'PCS', 5000000, 7500000, 10, 1, ?3, ?3);
        "#,
    )
    .bind(&product_a_id)
    .bind(&tenant_a_id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .unwrap();

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        owner_token,
        manager_token,
        tenant_a_id,
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

async fn send_raw_request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: &str,
    raw_body: &str,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(raw_body.to_string()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let resp_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json_val = if resp_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&resp_bytes).unwrap_or(Value::Null)
    };

    (status, json_val)
}

/// Helper: creates and activates a project
async fn create_active_project(
    h: &ChallengerHarness,
    name: &str,
    budget: i64,
    contract: i64,
) -> String {
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": name,
            "description": "Adversarial Stress Test Project",
            "customer_name": "PT Mega Infrastruktur",
            "billing_type": "MILESTONE",
            "budget_amount": budget,
            "contract_amount": contract
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Failed to create project: {:?}", resp);
    let project_id = resp["id"].as_str().unwrap().to_string();

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
    assert_eq!(status, StatusCode::OK, "Failed to activate project: {:?}", update_resp);
    assert_eq!(update_resp["status"], "ACTIVE");

    project_id
}

// ============================================================================
// 1. Pure Integer Rupiah High-Magnitude Arithmetic Stress Testing
// ============================================================================

#[tokio::test]
async fn test_pure_integer_rupiah_high_magnitude_stress() {
    let h = setup_challenger_harness().await;

    // --- Part A: Mathematical domain invariants with multi-trillion Rupiah ---
    // 50 Trillion IDR = Rp 50,000,000,000,000
    let fifty_trillion = Rupiah::new(50_000_000_000_000);
    // 75 Trillion IDR = Rp 75,000,000,000,000
    let seventy_five_trillion = Rupiah::new(75_000_000_000_000);

    // Multiplication: 20,000 hours @ Rp 250,000,000 / hour = Rp 5,000,000,000,000 (5 Trillion IDR)
    let labor_calc = ProjectLabor::calculate_total_cost(20_000, Rupiah::new(250_000_000));
    assert_eq!(labor_calc, Rupiah::new(5_000_000_000_000));

    // Material multiplication: 50,000 units @ Rp 100,000,000 / unit = Rp 5,000,000,000,000 (5 Trillion IDR)
    let mat_calc = ProjectMaterial::calculate_total_cost(50_000, Rupiah::new(100_000_000));
    assert_eq!(mat_calc, Rupiah::new(5_000_000_000_000));

    // Saturation addition & subtraction boundary checks
    assert_eq!(Rupiah::MAX.saturating_add(Rupiah::new(1_000)), Rupiah::MAX);
    assert_eq!(Rupiah::MIN.saturating_sub(Rupiah::new(1_000)), Rupiah::MIN);

    // Serde roundtrip integrity: JSON representation must be exact integer string, NOT float or exponent
    let json_serialized = serde_json::to_string(&fifty_trillion).unwrap();
    assert_eq!(json_serialized, "50000000000000");
    assert!(!json_serialized.contains('.'));
    assert!(!json_serialized.to_lowercase().contains('e'));

    let deserialized: Rupiah = serde_json::from_str(&json_serialized).unwrap();
    assert_eq!(deserialized, fifty_trillion);

    // --- Part B: End-to-end API & database stress test ---
    // Create multi-trillion mega-infrastructure project
    let project_id = create_active_project(
        &h,
        "Trans-Java High-Speed Transit Mega Project",
        fifty_trillion.as_i64(),
        seventy_five_trillion.as_i64(),
    )
    .await;

    // Log multi-billion labor cost: 100 hours @ Rp 50,000,000/hr = Rp 5,000,000,000 (5 Billion IDR)
    let (status, labor_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Chief Tunneling Engineer",
            "work_date": "2026-10-05",
            "hours_worked": 100,
            "hourly_rate": 50_000_000,
            "billing_rate": 75_000_000,
            "description": "Deep bedrock seismic boring verification"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Labor logging failed: {:?}", labor_resp);
    assert_eq!(labor_resp["total_cost"], 5_000_000_000_i64);

    // Log multi-billion expense 1: Rp 12,500,000,000 Subcontractor
    let (status, exp1_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "SUBCONTRACTOR",
            "description": "Heavy Tunnel Boring Machine Mobilization",
            "amount": 12_500_000_000_i64,
            "expense_date": "2026-10-05"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Expense 1 failed: {:?}", exp1_resp);
    assert_eq!(exp1_resp["amount"], 12_500_000_000_i64);

    // Log multi-billion expense 2: Rp 2,500,000,000 Permits
    let (status, exp2_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "PERMITS",
            "description": "Provincial Environmental Impact Permit",
            "amount": 2_500_000_000_i64,
            "expense_date": "2026-10-05"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Expense 2 failed: {:?}", exp2_resp);
    assert_eq!(exp2_resp["amount"], 2_500_000_000_i64);

    // Plan large material requisition: quantity 1,000,000
    let (status, mat_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 1_000_000,
            "notes": "Bulk steel allocation for viaduct spans"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Material planning failed: {:?}", mat_resp);
    assert_eq!(mat_resp["quantity_planned"], 1_000_000);
    assert_eq!(mat_resp["status"], "PLANNED");

    // Fetch Profitability Summary
    // Total labor cost: Rp 5,000,000,000
    // Total expense cost: Rp 15,000,000,000 (12.5B + 2.5B)
    // Total actual cost: Rp 20,000,000,000
    // Total billed revenue: Rp 0 (unbilled)
    // Net profit: -20,000,000,000
    // Margin bps: 0 (division by zero safely guarded)
    let (status, prof_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Profitability failed: {:?}", prof_resp);

    assert_eq!(prof_resp["budget_amount"], 50_000_000_000_000_i64);
    assert_eq!(prof_resp["contract_amount"], 75_000_000_000_000_i64);
    assert_eq!(prof_resp["total_labor_cost"], 5_000_000_000_i64);
    assert_eq!(prof_resp["total_expense_cost"], 15_000_000_000_i64);
    assert_eq!(prof_resp["total_material_cost"], 0); // planned materials are unissued
    assert_eq!(prof_resp["total_actual_cost"], 20_000_000_000_i64);
    assert_eq!(prof_resp["total_billed_revenue"], 0);
    assert_eq!(prof_resp["net_profit_amount"], -20_000_000_000_i64);
    assert_eq!(prof_resp["margin_percentage_basis_points"], 0);
}

// ============================================================================
// 2. Profitability Margin Calculation Across Extreme Scenarios
// ============================================================================

#[tokio::test]
async fn test_profitability_margin_extreme_scenarios() {
    let h = setup_challenger_harness().await;

    // --- Part A: Unit-Level Boundary Analysis of calculate_margin_bps ---
    // 1. Zero revenue: MUST safely return 0 bps without panic/division-by-zero
    assert_eq!(ProjectProfitabilitySummary::calculate_margin_bps(Rupiah::ZERO, Rupiah::ZERO), 0);
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(-50_000_000_000),
            Rupiah::ZERO
        ),
        0
    );
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(50_000_000_000),
            Rupiah::ZERO
        ),
        0
    );
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(Rupiah::MIN, Rupiah::ZERO),
        0
    );
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(Rupiah::MAX, Rupiah::ZERO),
        0
    );

    // 2. Exact Breakeven (revenue == cost -> net_profit == 0) MUST return 0 bps
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::ZERO,
            Rupiah::new(100_000_000)
        ),
        0
    );
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::ZERO,
            Rupiah::new(10_000_000_000_000) // 10 Trillion IDR breakeven
        ),
        0
    );
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(Rupiah::ZERO, Rupiah::new(1)),
        0
    );

    // 3. Extreme Profit (revenue >> cost)
    // 100% margin (Cost = 0, Profit == Revenue)
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(1_000_000_000),
            Rupiah::new(1_000_000_000)
        ),
        10_000 // 100.00% = 10,000 bps
    );
    // 99.99% margin (Revenue = 100 Billion, Cost = 1 Million -> Profit = 99,999 Million)
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(99_999_000_000),
            Rupiah::new(100_000_000_000)
        ),
        9_999 // 99.99% = 9,999 bps
    );
    // Multi-trillion profit: Revenue Rp 50 Trillion, Cost Rp 10 Trillion -> Profit Rp 40 Trillion
    // (40T * 10,000) / 50T = 8,000 bps (80.00%)
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(40_000_000_000_000),
            Rupiah::new(50_000_000_000_000)
        ),
        8_000
    );

    // 4. Extreme Loss / Deep Deficit (cost >> revenue, returning negative basis points)
    // Revenue Rp 10 Million, Cost Rp 50 Million -> Profit -Rp 40 Million
    // (-40M * 10,000) / 10M = -40,000 bps (-400.00%)
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(-40_000_000),
            Rupiah::new(10_000_000)
        ),
        -40_000
    );
    // Catastrophic deep deficit: Revenue Rp 1 Million, Cost Rp 100 Billion -> Profit -Rp 99,999 Million
    // (-99,999M * 10,000) / 1M = -999,990,000 bps (-9,999.9%)
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(-99_999_000_000),
            Rupiah::new(1_000_000)
        ),
        -999_990_000
    );

    // Sub-basis point rounding: integer division truncates toward zero
    // Revenue Rp 100M, Cost Rp 100,000,001 -> Profit -1 IDR -> (-10,000) / 100M = 0 bps
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(-1),
            Rupiah::new(100_000_000)
        ),
        0
    );
    // Profit -Rp 10,000 on Rp 100M revenue -> (-10,000 * 10,000) / 100,000,000 = -1 bps (-0.01%)
    assert_eq!(
        ProjectProfitabilitySummary::calculate_margin_bps(
            Rupiah::new(-10_000),
            Rupiah::new(100_000_000)
        ),
        -1
    );

    // --- Part B: End-to-end API & DB Verification ---
    let project_id = create_active_project(
        &h,
        "Commercial Margin Boundary Test Facility",
        500_000_000,
        750_000_000,
    )
    .await;

    // Stage 1: Add a billed milestone of Rp 100,000,000
    let milestone_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO milestones (
            id, tenant_id, project_id, sequence_order, title, target_date,
            status, billable_amount, is_billed, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, 1, 'Contractor Mobilization Milestone', '2026-11-01',
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

    // Stage 2: Create exact breakeven (actual cost == revenue == 100M)
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "SUBCONTRACTOR",
            "description": "Specialist contractor initial billing",
            "amount": 100_000_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, p1, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p1["total_billed_revenue"], 100_000_000);
    assert_eq!(p1["total_actual_cost"], 100_000_000);
    assert_eq!(p1["net_profit_amount"], 0);
    assert_eq!(p1["margin_percentage_basis_points"], 0); // Exact breakeven -> 0 bps

    // Stage 3: Incur moderate deficit (add Rp 25M expense -> Net profit -25M, margin -2500 bps)
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "EQUIPMENT_RENTAL",
            "description": "Hydraulic crane overtime",
            "amount": 25_000_000,
            "expense_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, p2, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p2["total_billed_revenue"], 100_000_000);
    assert_eq!(p2["total_actual_cost"], 125_000_000);
    assert_eq!(p2["net_profit_amount"], -25_000_000);
    assert_eq!(p2["margin_percentage_basis_points"], -2500); // -25.00% = -2500 bps

    // Stage 4: Incur deep deficit (add Rp 175M expense -> Total cost 300M, Net profit -200M, margin -20000 bps)
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "OTHER",
            "description": "Geotechnical unforeseen remediation cost",
            "amount": 175_000_000,
            "expense_date": "2026-10-07"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, p3, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(p3["total_billed_revenue"], 100_000_000);
    assert_eq!(p3["total_actual_cost"], 300_000_000);
    assert_eq!(p3["net_profit_amount"], -200_000_000);
    assert_eq!(p3["margin_percentage_basis_points"], -20000); // -200.00% = -20,000 bps
}

// ============================================================================
// 3. Boundary Input Fuzzing: Labor Hours (hours_worked <= 0)
// ============================================================================

#[tokio::test]
async fn test_boundary_labor_hours_fuzzing_strict_rejection() {
    let h = setup_challenger_harness().await;
    let project_id = create_active_project(&h, "Labor Hours Boundary Test", 100_000_000, 150_000_000).await;

    // Fuzz boundary non-positive values: 0, -1, -8, -100, i64::MIN
    let invalid_hours = vec![0_i64, -1, -8, -100, i64::MIN];

    for bad_hour in invalid_hours {
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/labor", project_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "worker_name": "Test Technician",
                "work_date": "2026-10-05",
                "hours_worked": bad_hour,
                "hourly_rate": 150_000,
                "billing_rate": 200_000
            })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected HTTP 400 for hours_worked = {}, got {}",
            bad_hour,
            status
        );
        assert_eq!(
            resp["code"], "INVALID_HOURS",
            "Expected code INVALID_HOURS for hours_worked = {}, got {:?}",
            bad_hour,
            resp
        );
    }

    // Positive values must be accepted
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Test Technician",
            "work_date": "2026-10-05",
            "hours_worked": 1,
            "hourly_rate": 150_000,
            "billing_rate": 200_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Valid 1 hour rejected: {:?}", resp);
    assert_eq!(resp["hours_worked"], 1);
}

// ============================================================================
// 4. Boundary Input Fuzzing: Labor Rates (rate < 0)
// ============================================================================

#[tokio::test]
async fn test_boundary_labor_rates_fuzzing_strict_rejection() {
    let h = setup_challenger_harness().await;
    let project_id = create_active_project(&h, "Labor Rates Boundary Test", 100_000_000, 150_000_000).await;

    // Fuzz negative hourly rates: -1, -100_000, i64::MIN
    let negative_rates = vec![-1_i64, -100_000, i64::MIN];

    for bad_rate in &negative_rates {
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/labor", project_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "worker_name": "Test Electrician",
                "work_date": "2026-10-05",
                "hours_worked": 8,
                "hourly_rate": *bad_rate,
                "billing_rate": 200_000
            })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected HTTP 400 for hourly_rate = {}, got {}",
            bad_rate,
            status
        );
        assert_eq!(
            resp["code"], "INVALID_RATE",
            "Expected code INVALID_RATE for hourly_rate = {}, got {:?}",
            bad_rate,
            resp
        );
    }

    // Fuzz negative billing rates: -1, -250_000, i64::MIN
    for bad_rate in &negative_rates {
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/labor", project_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "worker_name": "Test Electrician",
                "work_date": "2026-10-05",
                "hours_worked": 8,
                "hourly_rate": 150_000,
                "billing_rate": *bad_rate
            })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected HTTP 400 for billing_rate = {}, got {}",
            bad_rate,
            status
        );
        assert_eq!(
            resp["code"], "INVALID_RATE",
            "Expected code INVALID_RATE for billing_rate = {}, got {:?}",
            bad_rate,
            resp
        );
    }

    // Zero rates are legitimate (e.g., pro-bono, intern, salaried non-billable)
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "worker_name": "Volunteer Intern",
            "work_date": "2026-10-05",
            "hours_worked": 8,
            "hourly_rate": 0,
            "billing_rate": 0
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Zero rates must be accepted: {:?}", resp);
    assert_eq!(resp["hourly_rate"], 0);
    assert_eq!(resp["billing_rate"], 0);
    assert_eq!(resp["total_cost"], 0);
}

// ============================================================================
// 5. Boundary Input Fuzzing: Expense Amounts & Descriptions (amount <= 0)
// ============================================================================

#[tokio::test]
async fn test_boundary_expense_amounts_and_descriptions_rejection() {
    let h = setup_challenger_harness().await;
    let project_id = create_active_project(&h, "Expense Boundary Test", 100_000_000, 150_000_000).await;

    // Fuzz boundary non-positive amounts: 0, -1, -500_000, i64::MIN
    let invalid_amounts = vec![0_i64, -1, -500_000, i64::MIN];

    for bad_amt in invalid_amounts {
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/expenses", project_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "category": "PERMITS",
                "description": "Zoning Permit Fee",
                "amount": bad_amt,
                "expense_date": "2026-10-05"
            })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected HTTP 400 for amount = {}, got {}",
            bad_amt,
            status
        );
        assert_eq!(
            resp["code"], "INVALID_AMOUNT",
            "Expected code INVALID_AMOUNT for amount = {}, got {:?}",
            bad_amt,
            resp
        );
    }

    // Fuzz empty and whitespace-only descriptions
    let bad_descriptions = vec!["", "   ", "\t\t", "\n\r "];
    for bad_desc in bad_descriptions {
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/expenses", project_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "category": "PERMITS",
                "description": bad_desc,
                "amount": 500_000,
                "expense_date": "2026-10-05"
            })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected HTTP 400 for description = '{:?}', got {}",
            bad_desc,
            status
        );
        assert_eq!(
            resp["code"], "INVALID_DESCRIPTION",
            "Expected code INVALID_DESCRIPTION for description = '{:?}', got {:?}",
            bad_desc,
            resp
        );
    }

    // Valid positive amount must succeed
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "category": "PERMITS",
            "description": "Valid Municipal Building Permit",
            "amount": 1,
            "expense_date": "2026-10-05"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Valid 1 Rupiah expense rejected: {:?}", resp);
    assert_eq!(resp["amount"], 1);
}

// ============================================================================
// 6. Boundary Input Fuzzing: Material Quantities (quantity_planned <= 0)
// ============================================================================

#[tokio::test]
async fn test_boundary_material_quantities_fuzzing_strict_rejection() {
    let h = setup_challenger_harness().await;
    let project_id = create_active_project(&h, "Material Quantities Boundary Test", 100_000_000, 150_000_000).await;

    // Fuzz non-positive planned quantities: 0, -1, -500, i64::MIN
    let invalid_quantities = vec![0_i64, -1, -500, i64::MIN];

    for bad_qty in invalid_quantities {
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials", project_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "product_id": h.product_a_id,
                "warehouse_id": h.warehouse_a_id,
                "quantity_planned": bad_qty
            })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Expected HTTP 400 for quantity_planned = {}, got {}",
            bad_qty,
            status
        );
        assert_eq!(
            resp["code"], "INVALID_QUANTITY",
            "Expected code INVALID_QUANTITY for quantity_planned = {}, got {:?}",
            bad_qty,
            resp
        );
    }

    // Valid planned quantity must succeed
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "product_id": h.product_a_id,
            "warehouse_id": h.warehouse_a_id,
            "quantity_planned": 1
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Valid 1 quantity planned rejected: {:?}", resp);
    assert_eq!(resp["quantity_planned"], 1);
}

// ============================================================================
// 7. Floating-Point Payload Injection Strict Rejection
// ============================================================================

#[tokio::test]
async fn test_floating_point_payload_injection_strict_rejection() {
    let h = setup_challenger_harness().await;

    // --- Part A: Direct Serde JSON Deserializer Rejection ---
    // Floating-point numeric strings and floats MUST fail to deserialize into Rupiah
    let invalid_float_jsons = vec![
        "123.45",
        "0.0",
        "1.0",
        "-15.75",
        "1e5",
        "1.2e3",
        "NaN",
        "Infinity",
    ];

    for float_str in invalid_float_jsons {
        let res: Result<Rupiah, _> = serde_json::from_str(float_str);
        assert!(
            res.is_err(),
            "Rupiah deserializer must strictly reject float '{}', but succeeded: {:?}",
            float_str,
            res
        );
    }

    // DTO float rejection checks
    let float_labor_req = r#"{
        "worker_name": "Float Worker",
        "work_date": "2026-10-05",
        "hours_worked": 8.5,
        "hourly_rate": 150000
    }"#;
    let res: Result<LogProjectLaborRequest, _> = serde_json::from_str(float_labor_req);
    assert!(res.is_err(), "Must reject float hours_worked in LogProjectLaborRequest");

    let float_labor_rate_req = r#"{
        "worker_name": "Float Worker",
        "work_date": "2026-10-05",
        "hours_worked": 8,
        "hourly_rate": 150000.75
    }"#;
    let res: Result<LogProjectLaborRequest, _> = serde_json::from_str(float_labor_rate_req);
    assert!(res.is_err(), "Must reject float hourly_rate in LogProjectLaborRequest");

    let float_expense_req = r#"{
        "category": "PERMITS",
        "description": "Float expense",
        "amount": 500000.25,
        "expense_date": "2026-10-05"
    }"#;
    let res: Result<CreateProjectExpenseRequest, _> = serde_json::from_str(float_expense_req);
    assert!(res.is_err(), "Must reject float amount in CreateProjectExpenseRequest");

    let float_mat_req = format!(
        r#"{{
            "product_id": "{}",
            "warehouse_id": "{}",
            "quantity_planned": 10.5
        }}"#,
        h.product_a_id, h.warehouse_a_id
    );
    let res: Result<CreateProjectMaterialRequest, _> = serde_json::from_str(&float_mat_req);
    assert!(res.is_err(), "Must reject float quantity_planned in CreateProjectMaterialRequest");

    // --- Part B: HTTP Layer Raw Payload Injection Rejection ---
    let project_id = create_active_project(&h, "Float Injection Target", 100_000_000, 150_000_000).await;

    // 1. Injected float in labor hours_worked (8.5)
    let raw_payload_labor_hours = r#"{
        "worker_name": "Float Tester",
        "work_date": "2026-10-05",
        "hours_worked": 8.5,
        "hourly_rate": 100000
    }"#;
    let (s, b) = send_raw_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        raw_payload_labor_hours,
    )
    .await;
    assert!(
        s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
        "Float hours_worked must be rejected with 422 or 400, got {}: {:?}",
        s,
        b
    );

    // 2. Injected float in labor hourly_rate (100000.50)
    let raw_payload_labor_rate = r#"{
        "worker_name": "Float Tester",
        "work_date": "2026-10-05",
        "hours_worked": 8,
        "hourly_rate": 100000.50
    }"#;
    let (s, b) = send_raw_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        raw_payload_labor_rate,
    )
    .await;
    assert!(
        s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
        "Float hourly_rate must be rejected with 422 or 400, got {}: {:?}",
        s,
        b
    );

    // 3. Injected float in labor billing_rate (150000.75)
    let raw_payload_labor_bill_rate = r#"{
        "worker_name": "Float Tester",
        "work_date": "2026-10-05",
        "hours_worked": 8,
        "hourly_rate": 100000,
        "billing_rate": 150000.75
    }"#;
    let (s, b) = send_raw_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        raw_payload_labor_bill_rate,
    )
    .await;
    assert!(
        s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
        "Float billing_rate must be rejected with 422 or 400, got {}: {:?}",
        s,
        b
    );

    // 4. Injected float in expense amount (250000.99)
    let raw_payload_expense_amt = r#"{
        "category": "PERMITS",
        "description": "Float permit fee",
        "amount": 250000.99,
        "expense_date": "2026-10-05"
    }"#;
    let (s, b) = send_raw_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        raw_payload_expense_amt,
    )
    .await;
    assert!(
        s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
        "Float expense amount must be rejected with 422 or 400, got {}: {:?}",
        s,
        b
    );

    // 5. Injected float in material quantity_planned (12.5)
    let raw_payload_material_qty = format!(
        r#"{{
            "product_id": "{}",
            "warehouse_id": "{}",
            "quantity_planned": 12.5
        }}"#,
        h.product_a_id, h.warehouse_a_id
    );
    let (s, b) = send_raw_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        &raw_payload_material_qty,
    )
    .await;
    assert!(
        s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
        "Float material quantity must be rejected with 422 or 400, got {}: {:?}",
        s,
        b
    );

    // 6. Injected float in project creation budget_amount (10000000.50)
    let raw_payload_project_budget = r#"{
        "name": "Float Project Invariant",
        "customer_name": "Client",
        "billing_type": "MILESTONE",
        "budget_amount": 10000000.50,
        "contract_amount": 20000000
    }"#;
    let (s, b) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        raw_payload_project_budget,
    )
    .await;
    assert!(
        s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
        "Float budget_amount must be rejected with 422 or 400, got {}: {:?}",
        s,
        b
    );
}

// ============================================================================
// 8. SQLite Database Check Constraints Defense-in-Depth Verification
// ============================================================================

#[tokio::test]
async fn test_sqlite_check_constraints_defense_in_depth() {
    let h = setup_challenger_harness().await;
    let project_id = create_active_project(&h, "DB Constraints Stress Project", 100_000_000, 150_000_000).await;
    let now_str = chrono::Utc::now().to_rfc3339();

    // 1. project_labor: hours_worked > 0 check constraint
    let labor_zero_res = sqlx::query(
        r#"
        INSERT INTO project_labor (
            id, tenant_id, project_id, worker_name, work_date, hours_worked,
            hourly_rate, total_cost, billing_rate, is_billable, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, 'DB Check Worker', '2026-10-05', 0,
            100000, 0, 150000, 1, ?4, ?4
        );
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(
        labor_zero_res.is_err(),
        "SQLite must reject hours_worked = 0 via CHECK (hours_worked > 0)"
    );

    // 2. project_labor: hourly_rate >= 0 check constraint
    let labor_neg_rate_res = sqlx::query(
        r#"
        INSERT INTO project_labor (
            id, tenant_id, project_id, worker_name, work_date, hours_worked,
            hourly_rate, total_cost, billing_rate, is_billable, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, 'DB Check Worker', '2026-10-05', 8,
            -100000, 0, 150000, 1, ?4, ?4
        );
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(
        labor_neg_rate_res.is_err(),
        "SQLite must reject hourly_rate < 0 via CHECK (hourly_rate >= 0)"
    );

    // 3. project_labor: total_cost >= 0 check constraint
    let labor_neg_cost_res = sqlx::query(
        r#"
        INSERT INTO project_labor (
            id, tenant_id, project_id, worker_name, work_date, hours_worked,
            hourly_rate, total_cost, billing_rate, is_billable, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, 'DB Check Worker', '2026-10-05', 8,
            100000, -800000, 150000, 1, ?4, ?4
        );
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(
        labor_neg_cost_res.is_err(),
        "SQLite must reject total_cost < 0 via CHECK (total_cost >= 0)"
    );

    // 4. project_expenses: amount > 0 check constraint
    let expense_zero_res = sqlx::query(
        r#"
        INSERT INTO project_expenses (
            id, tenant_id, project_id, category, description, amount,
            expense_date, is_billable, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, 'PERMITS', 'Bypassed Fee', 0,
            '2026-10-05', 1, ?4, ?4
        );
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(
        expense_zero_res.is_err(),
        "SQLite must reject amount = 0 via CHECK (amount > 0)"
    );

    // 5. project_materials: quantity_planned >= 0 check constraint
    let mat_neg_qty_res = sqlx::query(
        r#"
        INSERT INTO project_materials (
            id, tenant_id, project_id, product_id, warehouse_id, quantity_planned,
            quantity_issued, unit_cost, total_cost, status, is_billable, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, -10,
            0, 0, 0, 'PLANNED', 1, ?6, ?6
        );
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(&h.product_a_id)
    .bind(&h.warehouse_a_id)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(
        mat_neg_qty_res.is_err(),
        "SQLite must reject quantity_planned < 0 via CHECK (quantity_planned >= 0)"
    );
}
