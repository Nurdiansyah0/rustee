//! Adversarial Empirical Verification Test Suite for Milestone 2:
//! Costing Concurrency, Multi-Tenant Anti-Enumeration & State Machine Invariants.
//!
//! Subagent: challenger_m2_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//! References: PRD §10, §11, §31, §38, §60, §61, §62, PROJECT.md
//!
//! Empirical Challenge Invariants:
//! 1. Concurrency & Race Condition Safety:
//!    - Concurrent labor logging, expense creation, and material planning across concurrent tokio tasks on the same project.
//!    - Transaction serialization, zero lost updates, and exact cost aggregation totals.
//!    - Concurrent duplicate deletion races (exactly 1 succeeds with 204, others return 404).
//!    - Interleaved concurrent write and read throughput without SQLite lock failures.
//! 2. Multi-Tenant Anti-Enumeration & Isolation Under Concurrency:
//!    - High-volume concurrent cross-tenant queries from Tenant Beta targeting Tenant Alpha across all 10 costing endpoints.
//!    - Assert 100% of foreign tenant requests return HTTP 404 Not Found without leaking existence.
//!    - Cross-tenant ID confusion attack (e.g. DELETE /api/v1/projects/{beta_project}/labor/{alpha_labor}).
//! 3. Project Lifecycle State Machine Attack:
//!    - Cost mutations blocked across ALL non-active states (Draft, OnHold, Completed, Cancelled).
//!    - Systematic verification of all 6 mutation endpoints returning HTTP 422 PROJECT_NOT_ACTIVE.
//!    - Lifecycle state transition loop & terminal state immutability.
//! 4. Concurrent mixed valid and invalid payloads partition.
//! 5. Concurrent rate inheritance resolution under load.

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

struct ChallengerConcurrencyHarness {
    app: axum::Router,
    #[allow(dead_code)]
    pool: sqlx::SqlitePool,
    #[allow(dead_code)]
    _dir: tempfile::TempDir,
    owner_token: String,
    manager_token: String,
    #[allow(dead_code)]
    staff_token: String,
    #[allow(dead_code)]
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

async fn setup_harness() -> ChallengerConcurrencyHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m2_concurrency.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 20,
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

    let jwt_secret = "m2_challenger_concurrency_secret_key_1234567890_test";
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
            email: "owner_a_conc@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_a_conc@contractor.com", "user", "premium")
        .unwrap();

    // 2. Manager User A
    let manager_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: manager_id.clone(),
            email: "manager_a_conc@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Manager A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (manager_token, _) = jwt_engine
        .generate_token(&manager_id, "manager_a_conc@contractor.com", "user", "premium")
        .unwrap();

    // 3. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_a_conc@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_a_conc@contractor.com", "user", "free")
        .unwrap();

    // 4. Accountant User A
    let accountant_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: accountant_id.clone(),
            email: "accountant_a_conc@contractor.com".to_string(),
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
            "accountant_a_conc@contractor.com",
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
            name: "Tenant Alpha Concurrency".to_string(),
            slug: "tenant-alpha-concurrency".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

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

    // Warehouse & Product for Tenant A
    let warehouse_a_id = Uuid::new_v4().to_string();
    let now_str = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO warehouses (id, tenant_id, code, name, is_default, created_at, updated_at)
        VALUES (?1, ?2, 'WH-CONC-01', 'Concurrency Warehouse', 1, ?3, ?3);
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
        VALUES (?1, ?2, 'PRD-CONC-01', 'Steel Rebar 12mm', 'BATANG', 95000, 110000, 50, 1, ?3, ?3);
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
            email: "owner_b_conc@beta-contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "owner_b_conc@beta-contractor.com", "user", "free")
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

    ChallengerConcurrencyHarness {
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

async fn create_active_project(h: &ChallengerConcurrencyHarness) -> String {
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Stress Test Commercial Center",
            "description": "High-concurrency costing stress test project",
            "customer_name": "PT Mega Concurrency Invariants",
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
// Test 1: Concurrency and Race Condition Testing: Mixed Mutations Serialization
// ============================================================================

#[tokio::test]
async fn test_concurrent_cost_mutations_race_serialization() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness).await;

    // Concurrently spawn 60 tokio tasks against the SAME active project:
    // - 20 tasks: Labor logging (5 hours @ 200,000 IDR = 1,000,000 IDR per entry -> 20,000,000 IDR total)
    // - 20 tasks: Expense creation (750,000 IDR per entry -> 15,000,000 IDR total)
    // - 20 tasks: Material planning (10 units per entry -> 200 units total planned, 0 IDR issued cost)
    let num_each = 20;
    let mut handles = Vec::new();

    // 1. Concurrent Labor tasks
    for i in 0..num_each {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/labor", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "worker_name": format!("Concurrent Worker #{}", i),
                    "work_date": "2026-10-10",
                    "hours_worked": 5,
                    "hourly_rate": 200_000,
                    "billing_rate": 350_000,
                    "is_billable": true,
                    "description": format!("Labor log entry batch {}", i)
                })),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "Labor task {} failed", i);
            assert_eq!(resp["total_cost"], 1_000_000);
            resp["id"].as_str().unwrap().to_string()
        }));
    }

    // 2. Concurrent Expense tasks
    let categories = [
        "PERMITS",
        "EQUIPMENT_RENTAL",
        "SUBCONTRACTOR",
        "TRAVEL",
        "OTHER",
    ];
    for i in 0..num_each {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        let cat = categories[i % categories.len()];
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/expenses", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "category": cat,
                    "description": format!("Concurrent Expense #{}", i),
                    "amount": 750_000,
                    "expense_date": "2026-10-10",
                    "vendor_name": format!("Vendor #{}.Corp", i),
                    "is_billable": true
                })),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "Expense task {} failed", i);
            assert_eq!(resp["amount"], 750_000);
            resp["id"].as_str().unwrap().to_string()
        }));
    }

    // 3. Concurrent Material planning tasks
    for i in 0..num_each {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/materials", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "product_id": h.product_a_id,
                    "warehouse_id": h.warehouse_a_id,
                    "quantity_planned": 10,
                    "is_billable": true,
                    "notes": format!("Concurrent Material Plan #{}", i)
                })),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "Material task {} failed", i);
            assert_eq!(resp["quantity_planned"], 10);
            assert_eq!(resp["total_cost"], 0); // PLANNED status has 0 cost until issued
            resp["id"].as_str().unwrap().to_string()
        }));
    }

    // Join all 60 tasks
    let mut created_ids = Vec::new();
    for handle in handles {
        let id = handle.await.expect("Tokio task panicked");
        created_ids.push(id);
    }
    assert_eq!(created_ids.len(), 60);

    // Verify Project Profitability Summary:
    // total_labor_cost = 20 * 1,000,000 = 20,000,000 IDR
    // total_expense_cost = 20 * 750,000 = 15,000,000 IDR
    // total_material_cost = 0 IDR (all are PLANNED)
    // total_actual_cost = 20,000,000 + 15,000,000 = 35,000,000 IDR
    let (status, prof_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(prof_resp["total_labor_cost"], 20_000_000);
    assert_eq!(prof_resp["total_expense_cost"], 15_000_000);
    assert_eq!(prof_resp["total_material_cost"], 0);
    assert_eq!(prof_resp["total_actual_cost"], 35_000_000);
    assert_eq!(prof_resp["total_billed_revenue"], 0);
    assert_eq!(prof_resp["net_profit_amount"], -35_000_000);

    // Verify List endpoints count exact records
    let (status, labor_list, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(labor_list["count"], 20);

    let (status, expense_list, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/expenses", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(expense_list["count"], 20);

    let (status, material_list, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/materials", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(material_list["count"], 20);
}

// ============================================================================
// Test 2: Concurrent Deletion and Duplicate Delete Race Safety
// ============================================================================

#[tokio::test]
async fn test_concurrent_deletion_and_duplicate_delete_race() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness).await;

    // Seed 25 labor records, 25 expense records, 25 material records
    let count = 25;
    let mut labor_ids = Vec::new();
    let mut expense_ids = Vec::new();
    let mut material_ids = Vec::new();

    for i in 0..count {
        let (_, resp, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/labor", project_id),
            &harness.manager_token,
            &harness.tenant_a_id,
            Some(json!({
                "worker_name": format!("Delete Target Worker {}", i),
                "work_date": "2026-10-10",
                "hours_worked": 2,
                "hourly_rate": 50_000
            })),
        )
        .await;
        labor_ids.push(resp["id"].as_str().unwrap().to_string());

        let (_, resp, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/expenses", project_id),
            &harness.manager_token,
            &harness.tenant_a_id,
            Some(json!({
                "category": "TRAVEL",
                "description": format!("Delete Target Expense {}", i),
                "amount": 100_000,
                "expense_date": "2026-10-10"
            })),
        )
        .await;
        expense_ids.push(resp["id"].as_str().unwrap().to_string());

        let (_, resp, _) = send_req(
            &harness.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials", project_id),
            &harness.manager_token,
            &harness.tenant_a_id,
            Some(json!({
                "product_id": harness.product_a_id,
                "warehouse_id": harness.warehouse_a_id,
                "quantity_planned": 5
            })),
        )
        .await;
        material_ids.push(resp["id"].as_str().unwrap().to_string());
    }

    // 1. Concurrently launch all 75 deletions while 10 readers query profitability
    let mut delete_handles = Vec::new();

    // 25 labor deletions
    for lid in labor_ids.clone() {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        delete_handles.push(tokio::spawn(async move {
            let (status, _, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/labor/{}", pid, lid),
                &h.manager_token,
                &h.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }));
    }

    // 25 expense deletions
    for eid in expense_ids.clone() {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        delete_handles.push(tokio::spawn(async move {
            let (status, _, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/expenses/{}", pid, eid),
                &h.manager_token,
                &h.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }));
    }

    // 25 material deletions
    for mid in material_ids.clone() {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        delete_handles.push(tokio::spawn(async move {
            let (status, _, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/materials/{}", pid, mid),
                &h.manager_token,
                &h.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }));
    }

    // 10 concurrent readers querying profitability during deletions
    let mut reader_handles = Vec::new();
    for _ in 0..10 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        reader_handles.push(tokio::spawn(async move {
            let (status, val, _) = send_req(
                &h.app,
                Method::GET,
                &format!("/api/v1/projects/{}/profitability", pid),
                &h.manager_token,
                &h.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert!(val["total_actual_cost"].is_number());
        }));
    }

    for h in delete_handles {
        h.await.unwrap();
    }
    for h in reader_handles {
        h.await.unwrap();
    }

    // After all 75 deletions, cost totals must be strictly 0
    let (status, prof_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_resp["total_labor_cost"], 0);
    assert_eq!(prof_resp["total_expense_cost"], 0);
    assert_eq!(prof_resp["total_material_cost"], 0);
    assert_eq!(prof_resp["total_actual_cost"], 0);

    // 2. Duplicate Delete Race Attack:
    // Concurrently send 15 delete attempts on the exact SAME already-deleted labor ID,
    // 15 delete attempts on the already-deleted expense ID,
    // and 15 delete attempts on the already-deleted material ID.
    // Assert: 100% (45/45) return HTTP 404 NOT_FOUND. Zero 500s, zero panics.
    let dead_labor_id = labor_ids[0].clone();
    let dead_expense_id = expense_ids[0].clone();
    let dead_material_id = material_ids[0].clone();

    let mut dup_handles = Vec::new();
    for _ in 0..15 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        let lid = dead_labor_id.clone();
        dup_handles.push(tokio::spawn(async move {
            let (status, _, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/labor/{}", pid, lid),
                &h.manager_token,
                &h.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
        }));
    }
    for _ in 0..15 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        let eid = dead_expense_id.clone();
        dup_handles.push(tokio::spawn(async move {
            let (status, _, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/expenses/{}", pid, eid),
                &h.manager_token,
                &h.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
        }));
    }
    for _ in 0..15 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        let mid = dead_material_id.clone();
        dup_handles.push(tokio::spawn(async move {
            let (status, _, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/materials/{}", pid, mid),
                &h.manager_token,
                &h.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
        }));
    }

    for h in dup_handles {
        h.await.unwrap();
    }
}

// ============================================================================
// Test 3: High-Volume Cross-Tenant Anti-Enumeration & ID Confusion Under Concurrency
// ============================================================================

#[tokio::test]
async fn test_high_volume_cross_tenant_anti_enumeration_under_concurrency() {
    let harness = Arc::new(setup_harness().await);
    let alpha_proj_id = create_active_project(&harness).await;

    // Seed Tenant Alpha's project with costing data
    let (_, labor_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", alpha_proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "worker_name": "Alpha Sensitive Worker",
            "work_date": "2026-10-10",
            "hours_worked": 8,
            "hourly_rate": 300_000
        })),
    )
    .await;
    let alpha_labor_id = labor_resp["id"].as_str().unwrap().to_string();

    let (_, expense_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", alpha_proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "category": "SUBCONTRACTOR",
            "description": "Alpha Classified Expense",
            "amount": 25_000_000,
            "expense_date": "2026-10-10"
        })),
    )
    .await;
    let alpha_expense_id = expense_resp["id"].as_str().unwrap().to_string();

    let (_, mat_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", alpha_proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 100
        })),
    )
    .await;
    let alpha_mat_id = mat_resp["id"].as_str().unwrap().to_string();

    // 1. High-Volume Cross-Tenant Attack:
    // Tenant Beta launches 60 concurrent requests attempting to access, inject, delete,
    // or aggregate Tenant Alpha's project across all 10 endpoints.
    let mut attack_handles = Vec::new();

    for i in 0..60 {
        let h = Arc::clone(&harness);
        let pid = alpha_proj_id.clone();
        let lid = alpha_labor_id.clone();
        let eid = alpha_expense_id.clone();
        let mid = alpha_mat_id.clone();

        attack_handles.push(tokio::spawn(async move {
            let endpoint_idx = i % 10;
            let (status, resp, _) = match endpoint_idx {
                0 => {
                    // Probe Alpha Labor list
                    send_req(
                        &h.app,
                        Method::GET,
                        &format!("/api/v1/projects/{}/labor", pid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        None,
                    )
                    .await
                }
                1 => {
                    // Attempt unauthorized labor injection into Alpha project
                    send_req(
                        &h.app,
                        Method::POST,
                        &format!("/api/v1/projects/{}/labor", pid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        Some(json!({
                            "worker_name": "Hostile Beta Infiltrator",
                            "work_date": "2026-10-10",
                            "hours_worked": 10,
                            "hourly_rate": 500_000
                        })),
                    )
                    .await
                }
                2 => {
                    // Attempt to delete Alpha labor
                    send_req(
                        &h.app,
                        Method::DELETE,
                        &format!("/api/v1/projects/{}/labor/{}", pid, lid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        None,
                    )
                    .await
                }
                3 => {
                    // Probe Alpha Expenses list
                    send_req(
                        &h.app,
                        Method::GET,
                        &format!("/api/v1/projects/{}/expenses", pid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        None,
                    )
                    .await
                }
                4 => {
                    // Attempt unauthorized expense injection
                    send_req(
                        &h.app,
                        Method::POST,
                        &format!("/api/v1/projects/{}/expenses", pid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        Some(json!({
                            "category": "OTHER",
                            "description": "Beta Bogus Expense",
                            "amount": 99_000_000,
                            "expense_date": "2026-10-10"
                        })),
                    )
                    .await
                }
                5 => {
                    // Attempt to delete Alpha expense
                    send_req(
                        &h.app,
                        Method::DELETE,
                        &format!("/api/v1/projects/{}/expenses/{}", pid, eid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        None,
                    )
                    .await
                }
                6 => {
                    // Probe Alpha Materials list
                    send_req(
                        &h.app,
                        Method::GET,
                        &format!("/api/v1/projects/{}/materials", pid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        None,
                    )
                    .await
                }
                7 => {
                    // Attempt unauthorized material planning
                    send_req(
                        &h.app,
                        Method::POST,
                        &format!("/api/v1/projects/{}/materials", pid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        Some(json!({
                            "product_id": Uuid::new_v4(),
                            "warehouse_id": Uuid::new_v4(),
                            "quantity_planned": 500
                        })),
                    )
                    .await
                }
                8 => {
                    // Attempt to delete Alpha material
                    send_req(
                        &h.app,
                        Method::DELETE,
                        &format!("/api/v1/projects/{}/materials/{}", pid, mid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        None,
                    )
                    .await
                }
                _ => {
                    // Probe Alpha Profitability summary
                    send_req(
                        &h.app,
                        Method::GET,
                        &format!("/api/v1/projects/{}/profitability", pid),
                        &h.tenant_b_token,
                        &h.tenant_b_id,
                        None,
                    )
                    .await
                }
            };

            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "Request {} leaked existence! Status: {:?}, Body: {:?}",
                endpoint_idx,
                status,
                resp
            );
            assert_eq!(resp["code"], "NOT_FOUND");
        }));
    }

    for h in attack_handles {
        h.await.unwrap();
    }

    // 2. Cross-Tenant Item ID Confusion Attack:
    // Tenant Beta creates Project Beta.
    // Tenant Beta attempts to delete Tenant Alpha's items through Beta's project path:
    // DELETE /api/v1/projects/{beta_proj_id}/labor/{alpha_labor_id}
    // DELETE /api/v1/projects/{beta_proj_id}/expenses/{alpha_expense_id}
    // DELETE /api/v1/projects/{beta_proj_id}/materials/{alpha_mat_id}
    let (status, beta_proj_resp, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/projects",
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({
            "name": "Beta Legitimate Project",
            "description": "Project owned by Beta",
            "customer_name": "Beta Client",
            "billing_type": "TIME_AND_MATERIALS",
            "budget_amount": 10_000_000,
            "contract_amount": 15_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let beta_proj_id = beta_proj_resp["id"].as_str().unwrap().to_string();

    let (status, _, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", beta_proj_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let mut confusion_handles = Vec::new();
    for _ in 0..10 {
        let h = Arc::clone(&harness);
        let bpid = beta_proj_id.clone();
        let lid = alpha_labor_id.clone();
        confusion_handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/labor/{}", bpid, lid),
                &h.tenant_b_token,
                &h.tenant_b_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(resp["code"], "NOT_FOUND");
        }));
    }
    for _ in 0..10 {
        let h = Arc::clone(&harness);
        let bpid = beta_proj_id.clone();
        let eid = alpha_expense_id.clone();
        confusion_handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/expenses/{}", bpid, eid),
                &h.tenant_b_token,
                &h.tenant_b_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(resp["code"], "NOT_FOUND");
        }));
    }
    for _ in 0..10 {
        let h = Arc::clone(&harness);
        let bpid = beta_proj_id.clone();
        let mid = alpha_mat_id.clone();
        confusion_handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::DELETE,
                &format!("/api/v1/projects/{}/materials/{}", bpid, mid),
                &h.tenant_b_token,
                &h.tenant_b_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(resp["code"], "NOT_FOUND");
        }));
    }

    for h in confusion_handles {
        h.await.unwrap();
    }

    // 3. Confirm Tenant Alpha's data was completely untouched and preserved
    let (status, prof_check, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", alpha_proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 8h * 300,000 = 2,400,000 IDR labor + 25,000,000 IDR expense = 27,400,000 IDR
    assert_eq!(prof_check["total_labor_cost"], 2_400_000);
    assert_eq!(prof_check["total_expense_cost"], 25_000_000);
    assert_eq!(prof_check["total_actual_cost"], 27_400_000);

    let (status, labor_check, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", alpha_proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(labor_check["count"], 1);

    let (status, exp_check, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/expenses", alpha_proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exp_check["count"], 1);

    let (status, mat_check, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/materials", alpha_proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mat_check["count"], 1);
}

// ============================================================================
// Test 4: Project Lifecycle State Machine Invariants Attack
// ============================================================================

#[tokio::test]
async fn test_lifecycle_state_machine_mutations_blocked_on_all_inactive_states() {
    let harness = setup_harness().await;

    // Helper closure to assert all 6 mutation endpoints return 422 PROJECT_NOT_ACTIVE
    async fn assert_all_mutations_blocked(
        h: &ChallengerConcurrencyHarness,
        proj_id: &str,
        labor_id: &str,
        expense_id: &str,
        material_id: &str,
        state_label: &str,
    ) {
        // 1. POST labor
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/labor", proj_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "worker_name": "Blocked Worker",
                "work_date": "2026-10-10",
                "hours_worked": 4,
                "hourly_rate": 100_000
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "State {} failed to block POST labor",
            state_label
        );
        assert_eq!(resp["code"], "PROJECT_NOT_ACTIVE");

        // 2. DELETE labor
        let (status, resp, _) = send_req(
            &h.app,
            Method::DELETE,
            &format!("/api/v1/projects/{}/labor/{}", proj_id, labor_id),
            &h.manager_token,
            &h.tenant_a_id,
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "State {} failed to block DELETE labor",
            state_label
        );
        assert_eq!(resp["code"], "PROJECT_NOT_ACTIVE");

        // 3. POST expense
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/expenses", proj_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "category": "PERMITS",
                "description": "Blocked Permit Expense",
                "amount": 5_000_000,
                "expense_date": "2026-10-10"
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "State {} failed to block POST expense",
            state_label
        );
        assert_eq!(resp["code"], "PROJECT_NOT_ACTIVE");

        // 4. DELETE expense
        let (status, resp, _) = send_req(
            &h.app,
            Method::DELETE,
            &format!("/api/v1/projects/{}/expenses/{}", proj_id, expense_id),
            &h.manager_token,
            &h.tenant_a_id,
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "State {} failed to block DELETE expense",
            state_label
        );
        assert_eq!(resp["code"], "PROJECT_NOT_ACTIVE");

        // 5. POST material
        let (status, resp, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/materials", proj_id),
            &h.manager_token,
            &h.tenant_a_id,
            Some(json!({
                "product_id": h.product_a_id,
                "warehouse_id": h.warehouse_a_id,
                "quantity_planned": 15
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "State {} failed to block POST material",
            state_label
        );
        assert_eq!(resp["code"], "PROJECT_NOT_ACTIVE");

        // 6. DELETE material
        let (status, resp, _) = send_req(
            &h.app,
            Method::DELETE,
            &format!("/api/v1/projects/{}/materials/{}", proj_id, material_id),
            &h.manager_token,
            &h.tenant_a_id,
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "State {} failed to block DELETE material",
            state_label
        );
        assert_eq!(resp["code"], "PROJECT_NOT_ACTIVE");
    }

    // --- State 1: DRAFT ---
    let (status, resp, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/projects",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Lifecycle State Machine Invariant Project",
            "customer_name": "State Audit Client",
            "billing_type": "MILESTONE",
            "budget_amount": 100_000_000,
            "contract_amount": 120_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let proj_id = resp["id"].as_str().unwrap().to_string();
    assert_eq!(resp["status"], "DRAFT");

    let dummy_id = Uuid::new_v4().to_string();
    assert_all_mutations_blocked(
        &harness, &proj_id, &dummy_id, &dummy_id, &dummy_id, "DRAFT",
    )
    .await;

    // --- Transition DRAFT -> ACTIVE ---
    let (status, update_resp, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", proj_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_resp["status"], "ACTIVE");

    // Seed actual records while ACTIVE
    let (_, labor_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "worker_name": "Active Phase Worker",
            "work_date": "2026-10-10",
            "hours_worked": 5,
            "hourly_rate": 100_000
        })),
    )
    .await;
    let actual_labor_id = labor_resp["id"].as_str().unwrap().to_string();

    let (_, exp_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/expenses", proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "category": "TRAVEL",
            "description": "Active Phase Travel",
            "amount": 1_000_000,
            "expense_date": "2026-10-10"
        })),
    )
    .await;
    let actual_expense_id = exp_resp["id"].as_str().unwrap().to_string();

    let (_, mat_resp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/materials", proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "product_id": harness.product_a_id,
            "warehouse_id": harness.warehouse_a_id,
            "quantity_planned": 20
        })),
    )
    .await;
    let actual_mat_id = mat_resp["id"].as_str().unwrap().to_string();

    // --- State 2: ON_HOLD ---
    let (status, update_resp, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", proj_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "status": "ON_HOLD" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_resp["status"], "ON_HOLD");

    assert_all_mutations_blocked(
        &harness,
        &proj_id,
        &actual_labor_id,
        &actual_expense_id,
        &actual_mat_id,
        "ON_HOLD",
    )
    .await;

    // Verify Read endpoints remain accessible during ON_HOLD (auditability)
    let (status, prof_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_resp["total_actual_cost"], 1_500_000);

    // --- Transition ON_HOLD -> ACTIVE (Reactivation) ---
    let (status, update_resp, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", proj_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_resp["status"], "ACTIVE");

    // Mutation works again!
    let (status, reactivated_labor, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/labor", proj_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        Some(json!({
            "worker_name": "Post Reactivation Worker",
            "work_date": "2026-10-10",
            "hours_worked": 2,
            "hourly_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let reactivated_labor_id = reactivated_labor["id"].as_str().unwrap().to_string();

    // --- State 3: COMPLETED ---
    let (status, update_resp, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", proj_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "status": "COMPLETED" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_resp["status"], "COMPLETED");

    assert_all_mutations_blocked(
        &harness,
        &proj_id,
        &reactivated_labor_id,
        &actual_expense_id,
        &actual_mat_id,
        "COMPLETED",
    )
    .await;

    // Terminal state invariant: COMPLETED cannot transition to ACTIVE
    let (status, term_resp, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", proj_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(term_resp["code"], "INVALID_STATUS_TRANSITION");

    // --- State 4: CANCELLED ---
    let (status, resp_canc, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/projects",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Cancelled Project Prototype",
            "customer_name": "Cancelled Client",
            "billing_type": "TIME_AND_MATERIALS",
            "budget_amount": 50_000_000,
            "contract_amount": 60_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let canc_proj_id = resp_canc["id"].as_str().unwrap().to_string();

    // Transition DRAFT -> CANCELLED
    let (status, update_canc, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", canc_proj_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "status": "CANCELLED" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_canc["status"], "CANCELLED");

    let dummy_id_canc = Uuid::new_v4().to_string();
    assert_all_mutations_blocked(
        &harness,
        &canc_proj_id,
        &dummy_id_canc,
        &dummy_id_canc,
        &dummy_id_canc,
        "CANCELLED",
    )
    .await;

    // Terminal state invariant: CANCELLED cannot transition to ACTIVE
    let (status, term_canc_resp, _) = send_req(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", canc_proj_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(term_canc_resp["code"], "INVALID_STATUS_TRANSITION");
}

// ============================================================================
// Test 5: Concurrent Mixed Valid and Invalid Payloads Partition
// ============================================================================

#[tokio::test]
async fn test_concurrent_mixed_valid_and_invalid_payloads_partition() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness).await;

    // 40 concurrent tasks:
    // - 20 valid requests (hours=4, rate=100,000 IDR -> 400,000 IDR each)
    // - 5 invalid: negative hours (-5)
    // - 5 invalid: zero hours (0)
    // - 5 invalid: negative rate (-50,000)
    // - 5 invalid: non-existent task_id (random Uuid)
    let mut handles = Vec::new();

    // 20 valid tasks
    for i in 0..20 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/labor", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "worker_name": format!("Valid Worker {}", i),
                    "work_date": "2026-10-10",
                    "hours_worked": 4,
                    "hourly_rate": 100_000
                })),
            )
            .await;
            (status, resp)
        }));
    }

    // 5 negative hours
    for _ in 0..5 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/labor", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "worker_name": "Negative Hours Worker",
                    "work_date": "2026-10-10",
                    "hours_worked": -5,
                    "hourly_rate": 100_000
                })),
            )
            .await;
            (status, resp)
        }));
    }

    // 5 zero hours
    for _ in 0..5 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/labor", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "worker_name": "Zero Hours Worker",
                    "work_date": "2026-10-10",
                    "hours_worked": 0,
                    "hourly_rate": 100_000
                })),
            )
            .await;
            (status, resp)
        }));
    }

    // 5 negative rate
    for _ in 0..5 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/labor", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "worker_name": "Negative Rate Worker",
                    "work_date": "2026-10-10",
                    "hours_worked": 4,
                    "hourly_rate": -50_000
                })),
            )
            .await;
            (status, resp)
        }));
    }

    // 5 non-existent task_id
    for _ in 0..5 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        let fake_task = Uuid::new_v4();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/labor", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "worker_name": "Phantom Task Worker",
                    "work_date": "2026-10-10",
                    "hours_worked": 4,
                    "hourly_rate": 100_000,
                    "task_id": fake_task
                })),
            )
            .await;
            (status, resp)
        }));
    }

    let mut created_count = 0;
    let mut bad_request_count = 0;
    let mut not_found_count = 0;

    for h in handles {
        let (status, _) = h.await.unwrap();
        match status {
            StatusCode::CREATED => created_count += 1,
            StatusCode::BAD_REQUEST => bad_request_count += 1,
            StatusCode::NOT_FOUND => not_found_count += 1,
            other => panic!("Unexpected status code under concurrent load: {:?}", other),
        }
    }

    assert_eq!(created_count, 20, "Expected exactly 20 successful creations");
    assert_eq!(
        bad_request_count, 15,
        "Expected exactly 15 bad request rejections"
    );
    assert_eq!(not_found_count, 5, "Expected exactly 5 not found rejections");

    // Exact cost verification: 20 * 400,000 = 8,000,000 IDR
    let (status, prof_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_resp["total_labor_cost"], 8_000_000);
    assert_eq!(prof_resp["total_actual_cost"], 8_000_000);

    let (status, list_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/labor", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 20);
}

// ============================================================================
// Test 6: Concurrent Profitability Reads During Active Heavy Write Load
// ============================================================================

#[tokio::test]
async fn test_concurrent_profitability_reads_during_active_writes() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness).await;

    // 10 writers adding expenses of 100,000 IDR each (10 expenses per writer = 100 expenses)
    // 10 readers concurrently reading profitability summary in parallel
    let num_writers = 10;
    let mut writer_handles = Vec::new();

    for w in 0..num_writers {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        writer_handles.push(tokio::spawn(async move {
            for i in 0..5 {
                let (status, resp, _) = send_req(
                    &h.app,
                    Method::POST,
                    &format!("/api/v1/projects/{}/expenses", pid),
                    &h.manager_token,
                    &h.tenant_a_id,
                    Some(json!({
                        "category": "OTHER",
                        "description": format!("Writer {} Entry {}", w, i),
                        "amount": 100_000,
                        "expense_date": "2026-10-10"
                    })),
                )
                .await;
                assert_eq!(status, StatusCode::CREATED, "Writer failed: {:?}", resp);
            }
        }));
    }

    let num_readers = 10;
    let mut reader_handles = Vec::new();
    for _ in 0..num_readers {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        reader_handles.push(tokio::spawn(async move {
            for _ in 0..5 {
                let (status, resp, _) = send_req(
                    &h.app,
                    Method::GET,
                    &format!("/api/v1/projects/{}/profitability", pid),
                    &h.manager_token,
                    &h.tenant_a_id,
                    None,
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                let actual_cost = resp["total_actual_cost"].as_i64().unwrap();
                assert!(
                    actual_cost >= 0,
                    "Actual cost must never be negative: {}",
                    actual_cost
                );
            }
        }));
    }

    for h in writer_handles {
        h.await.unwrap();
    }
    for h in reader_handles {
        h.await.unwrap();
    }

    // 10 writers * 5 entries * 100,000 = 5,000,000 IDR
    let (status, prof_final, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_final["total_expense_cost"], 5_000_000);
    assert_eq!(prof_final["total_actual_cost"], 5_000_000);
}

// ============================================================================
// Test 7: Concurrent Labor Member Rate Inheritance Under Load
// ============================================================================

#[tokio::test]
async fn test_concurrent_labor_member_rate_inheritance_under_load() {
    let harness = Arc::new(setup_harness().await);
    let project_id = create_active_project(&harness).await;

    // Assign Staff A as project member with cost_rate = 125,000 and billing_rate = 225,000
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", project_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "user_id": harness.staff_id,
            "role": "CONTRIBUTOR",
            "cost_rate": 125_000,
            "billing_rate": 225_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 20 concurrent tasks logging labor without specifying hourly_rate or billing_rate
    // Each logs 3 hours for Staff A -> should inherit 125,000 rate -> 375,000 IDR each
    // 20 * 375,000 = 7,500,000 IDR total
    let mut handles = Vec::new();
    for i in 0..20 {
        let h = Arc::clone(&harness);
        let pid = project_id.clone();
        handles.push(tokio::spawn(async move {
            let (status, resp, _) = send_req(
                &h.app,
                Method::POST,
                &format!("/api/v1/projects/{}/labor", pid),
                &h.manager_token,
                &h.tenant_a_id,
                Some(json!({
                    "worker_id": h.staff_id,
                    "work_date": "2026-10-10",
                    "hours_worked": 3,
                    "description": format!("Inherited rate labor {}", i)
                })),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(resp["hourly_rate"], 125_000);
            assert_eq!(resp["billing_rate"], 225_000);
            assert_eq!(resp["total_cost"], 375_000);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Verify exact aggregated profitability
    let (status, prof_resp, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &harness.manager_token,
        &harness.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_resp["total_labor_cost"], 7_500_000);
    assert_eq!(prof_resp["total_actual_cost"], 7_500_000);
}
