//! Integration Test Suite for Milestone 1:
//! Core Project Management & Structure (PRD §10, §31, §60, §61, §62).
//! Tests Features 1-10:
//! - Multi-tenancy isolation and anti-enumeration 404
//! - Gapless sequential project numbering (PRJ-YYYY-XXXXXX)
//! - Role-Based Access Control (Owner/Manager vs Staff)
//! - Project lifecycle state machine
//! - Project member assignment and rate validation
//! - Milestone state machine and sequential ordering
//! - Task management and staff execution

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
use chrono::Datelike;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: axum::Router,
    #[allow(dead_code)]
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    manager_token: String,
    staff_token: String,
    tenant_a_id: String,
    staff_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("projects_m1_test.sqlite");
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

    let jwt_secret = "m1_projects_test_secret_key_1234567890_super_secret";
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

    // 5. Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "owner_b@beta-arch.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "owner_b@beta-arch.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Architecture".to_string(),
            slug: "tenant-beta-architecture".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    // Seed COA for Tenant B
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
        tenant_a_id,
        staff_id,
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

#[tokio::test]
async fn test_project_crud_and_gapless_sequential_numbering() {
    let h = setup_harness().await;
    let current_year = chrono::Utc::now().year();

    // 1. Owner creates 1st project in Tenant A
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Wisma Atlet Tower A",
            "description": "High-rise residential project",
            "customer_name": "PT Jaya Konstruksi",
            "budget_amount": 5000000000i64,
            "contract_amount": 6500000000i64,
            "billing_type": "MILESTONE",
            "start_date": "2026-01-01",
            "end_date": "2026-12-31"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let expected_first_code = format!("PRJ-{}-000001", current_year);
    assert_eq!(body["project_number"], expected_first_code);
    assert_eq!(body["status"], "DRAFT");
    assert_eq!(body["name"], "Wisma Atlet Tower A");
    assert_eq!(body["budget_amount"], 5000000000i64);
    assert_eq!(body["contract_amount"], 6500000000i64);

    // 2. Manager creates 2nd project in Tenant A
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Gedung Perkantoran Sudirman",
            "customer_name": "PT Sinarmas Land",
            "budget_amount": 12000000000i64,
            "contract_amount": 15000000000i64,
            "billing_type": "PERCENTAGE_OF_COMPLETION"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let expected_second_code = format!("PRJ-{}-000002", current_year);
    assert_eq!(body["project_number"], expected_second_code);
    assert_eq!(body["status"], "DRAFT");

    // 3. Tenant B creates project (must have independent numbering starting at 000001)
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "name": "Beta Design Project",
            "customer_name": "PT Beta Client",
            "budget_amount": 2000000000i64,
            "contract_amount": 2500000000i64
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["project_number"], expected_first_code);

    // 4. Input validation: negative budget returns 400 Bad Request
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Invalid Budget Project",
            "customer_name": "PT Client",
            "budget_amount": -500000i64
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");

    // 5. Input validation: end date preceding start date returns 400 Bad Request
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Invalid Date Project",
            "customer_name": "PT Client",
            "start_date": "2026-12-31",
            "end_date": "2026-01-01"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_DATES");
}

#[tokio::test]
async fn test_project_retrieval_and_listing() {
    let h = setup_harness().await;

    // Create 2 projects in Tenant A
    let (_, p1, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Project Alpha 1",
            "customer_name": "Customer A1"
        })),
    )
    .await;
    let p1_id = p1["id"].as_str().unwrap();

    let (_, _p2, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Project Alpha 2",
            "customer_name": "Customer A2"
        })),
    )
    .await;

    // Create 1 project in Tenant B
    let (_, _pb, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "name": "Project Beta 1",
            "customer_name": "Customer B1"
        })),
    )
    .await;

    // 1. GET /api/v1/projects/{id}
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}", p1_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], p1_id);
    assert_eq!(body["name"], "Project Alpha 1");

    // 2. GET /api/v1/projects for Tenant A -> count == 2
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 2);
    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);

    // 3. GET /api/v1/projects for Tenant B -> count == 1
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        "/api/v1/projects",
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
    assert_eq!(body["projects"][0]["name"], "Project Beta 1");
}

#[tokio::test]
async fn test_cross_tenant_isolation_404_anti_enumeration() {
    let h = setup_harness().await;

    // Create project in Tenant A
    let (_, p1, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Alpha Confidential Project",
            "customer_name": "Secret Client"
        })),
    )
    .await;
    let p1_id = p1["id"].as_str().unwrap();

    // Tenant B queries Tenant A's project -> strictly 404 Not Found (NEVER 403 or 200)
    let endpoints = vec![
        (Method::GET, format!("/api/v1/projects/{}", p1_id), None),
        (
            Method::PATCH,
            format!("/api/v1/projects/{}/status", p1_id),
            Some(json!({ "status": "ACTIVE" })),
        ),
        (
            Method::POST,
            format!("/api/v1/projects/{}/members", p1_id),
            Some(json!({ "user_id": Uuid::new_v4().to_string(), "role": "WORKER" })),
        ),
        (Method::GET, format!("/api/v1/projects/{}/members", p1_id), None),
        (
            Method::POST,
            format!("/api/v1/projects/{}/milestones", p1_id),
            Some(json!({ "title": "M1", "target_date": "2026-06-01" })),
        ),
        (Method::GET, format!("/api/v1/projects/{}/milestones", p1_id), None),
        (
            Method::POST,
            format!("/api/v1/projects/{}/tasks", p1_id),
            Some(json!({ "title": "T1" })),
        ),
        (Method::GET, format!("/api/v1/projects/{}/tasks", p1_id), None),
    ];

    for (method, uri, body) in endpoints {
        let (status, res, _) = send_req(
            &h.app,
            method.clone(),
            &uri,
            &h.tenant_b_token,
            &h.tenant_b_id,
            body,
        )
        .await;

        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "Endpoint {} {} leaked cross-tenant existence with status {}",
            method,
            uri,
            status
        );
        assert_eq!(res["code"], "NOT_FOUND");
    }
}

#[tokio::test]
async fn test_rbac_permissions_owner_manager_vs_staff() {
    let h = setup_harness().await;

    // Create project as Owner
    let (_, p1, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Infrastructure Project",
            "customer_name": "Public Works"
        })),
    )
    .await;
    let p1_id = p1["id"].as_str().unwrap();

    // 1. Staff attempting project creation -> 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Unauthorized Project",
            "customer_name": "Client"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");

    // 2. Staff attempting project status update -> 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", p1_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");

    // 3. Staff attempting member addition -> 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", p1_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "WORKER"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");

    // 4. Staff attempting milestone creation -> 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", p1_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Unauthorized Milestone",
            "target_date": "2026-06-30"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");

    // 5. Staff attempting task creation -> 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", p1_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Unauthorized Task"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");

    // 6. Staff reading project details -> 200 OK
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}", p1_id),
        &h.staff_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Infrastructure Project");

    // 7. Staff listing projects -> 200 OK
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        "/api/v1/projects",
        &h.staff_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
}

#[tokio::test]
async fn test_project_state_machine_lifecycle() {
    let h = setup_harness().await;

    // Create project in DRAFT
    let (_, p, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Lifecycle Project",
            "customer_name": "Client X"
        })),
    )
    .await;
    let p_id = p["id"].as_str().unwrap();
    assert_eq!(p["status"], "DRAFT");

    // 1. Illegal transition: DRAFT -> COMPLETED directly returns 422
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "COMPLETED" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");

    // 2. Legal transition: DRAFT -> ACTIVE
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ACTIVE");

    // 3. Legal transition: ACTIVE -> ON_HOLD
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ON_HOLD" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ON_HOLD");

    // 4. Legal transition: ON_HOLD -> ACTIVE
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ACTIVE");

    // 5. Legal transition: ACTIVE -> COMPLETED (sets actual_completion_date)
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "COMPLETED" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "COMPLETED");
    assert!(body["actual_completion_date"].is_string());

    // 6. Illegal transition: transitioning out of terminal COMPLETED -> 422
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");
}

#[tokio::test]
async fn test_project_member_assignment_and_rate_validation() {
    let h = setup_harness().await;

    let (_, p, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Team Project",
            "customer_name": "Client Y"
        })),
    )
    .await;
    let p_id = p["id"].as_str().unwrap();

    // 1. Assign member with hourly rates -> 201 Created
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "SITE_ENGINEER",
            "cost_rate": 150000i64,
            "billing_rate": 250000i64
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["user_id"], h.staff_id);
    assert_eq!(body["role"], "SITE_ENGINEER");
    assert_eq!(body["cost_rate"], 150000i64);
    assert_eq!(body["billing_rate"], 250000i64);

    // 2. Listing members returns assigned user
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/members", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
    assert_eq!(body["members"][0]["user_id"], h.staff_id);

    // 3. Assigning duplicate user to same project returns 409 Conflict
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "FOREMAN"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "MEMBER_ALREADY_ASSIGNED");

    // 4. Negative rate returns 400 Bad Request
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": Uuid::new_v4().to_string(),
            "role": "WORKER",
            "cost_rate": -50000i64
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_RATE");

    // 5. User not belonging to tenant returns 404 Not Found
    let random_user = Uuid::new_v4().to_string();
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": random_user,
            "role": "WORKER"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "USER_NOT_FOUND");
}

#[tokio::test]
async fn test_milestone_management_and_state_machine() {
    let h = setup_harness().await;

    let (_, p, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Milestone Project",
            "customer_name": "Client Z"
        })),
    )
    .await;
    let p_id = p["id"].as_str().unwrap();

    // 1. Create Milestone 1 without sequence -> auto-assigned sequence_order 1
    let (status, m1, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Pondasi & Struktur Bawah",
            "description": "Pekerjaan pondasi tiang pancang",
            "target_date": "2026-03-31",
            "billable_amount": 1500000000i64
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let m1_id = m1["id"].as_str().unwrap();
    assert_eq!(m1["sequence_order"], 1);
    assert_eq!(m1["status"], "PENDING");
    assert_eq!(m1["billable_amount"], 1500000000i64);

    // 2. Create Milestone 2 -> auto-assigned sequence_order 2
    let (status, m2, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Struktur Atas & Finishing",
            "target_date": "2026-09-30",
            "billable_amount": 3500000000i64
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(m2["sequence_order"], 2);

    // 3. List milestones returns ordered by sequence_order ASC
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/milestones", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 2);
    assert_eq!(body["milestones"][0]["title"], "Pondasi & Struktur Bawah");
    assert_eq!(body["milestones"][1]["title"], "Struktur Atas & Finishing");

    // 4. Milestone State Transitions: PENDING -> IN_PROGRESS -> COMPLETED
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/milestones/{}/status", p_id, m1_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "IN_PROGRESS");

    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/milestones/{}/status", p_id, m1_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "COMPLETED" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "COMPLETED");
    assert!(body["completed_at"].is_string());

    // 5. Illegal transition out of COMPLETED -> 422
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/milestones/{}/status", p_id, m1_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");
}

#[tokio::test]
async fn test_task_management_and_staff_progress_updates() {
    let h = setup_harness().await;

    // Create project
    let (_, p, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Task Driven Project",
            "customer_name": "Client W"
        })),
    )
    .await;
    let p_id = p["id"].as_str().unwrap();

    // Create milestone
    let (_, m, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Phase 1 Foundation",
            "target_date": "2026-04-30"
        })),
    )
    .await;
    let m_id = m["id"].as_str().unwrap();

    // Assign staff member to project
    send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "WORKER"
        })),
    )
    .await;

    // 1. Create task linked to Milestone and assigned to Staff
    let (status, t, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "milestone_id": m_id,
            "assignee_id": h.staff_id,
            "title": "Pengecoran Tiang Pancang Titik 1-20",
            "priority": "HIGH",
            "estimated_hours": 40i64,
            "due_date": "2026-03-15"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let t_id = t["id"].as_str().unwrap();
    assert_eq!(t["status"], "TODO");
    assert_eq!(t["priority"], "HIGH");
    assert_eq!(t["estimated_hours"], 40i64);

    // 2. Task with non-existent milestone ID returns 404
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", p_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "milestone_id": Uuid::new_v4().to_string(),
            "title": "Invalid Milestone Task"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");

    // 3. Staff user updates task: TODO -> IN_PROGRESS (Staff IS permitted!)
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/tasks/{}/status", p_id, t_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "IN_PROGRESS");

    // 4. Staff user updates task: IN_PROGRESS -> DONE
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/tasks/{}/status", p_id, t_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({ "status": "DONE" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "DONE");
    assert!(body["completed_at"].is_string());

    // 5. Reopen task: DONE -> IN_PROGRESS
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/tasks/{}/status", p_id, t_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "IN_PROGRESS");
}
