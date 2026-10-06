//! Adversarial Stress & Chaos Test Suite for Milestone 1 (Project Management Foundation)
//!
//! Author: challenger_m1_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Mission:
//! 1. State machine invalid transition permutations:
//!    - ProjectStatus illegal transitions (Draft -> Completed, etc.) must return HTTP 422.
//!    - MilestoneStatus illegal transitions (Pending -> Completed, InProgress -> Pending, etc.) must return HTTP 422.
//!    - TaskStatus illegal transitions (Todo -> Done, Blocked -> Done, etc.) must return HTTP 422.
//! 2. RBAC permission boundaries:
//!    - Staff attempting management mutations must return HTTP 403 Forbidden.
//!    - Staff reading project data and updating task status must succeed with HTTP 200 OK.
//! 3. Boundary value validation:
//!    - Negative budget, negative hourly rates, negative sequence orders rejected by service validation (HTTP 400).
//!    - Direct SQLite check constraints reject negative or invalid domain values.
//! 4. Extreme inputs:
//!    - Empty names, inverted dates, malformed date strings, oversized strings, and SQL injection strings.

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

#[allow(dead_code)]
struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    staff_token: String,
    tenant_a_id: String,
    staff_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
    user_b_id: String,
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m1_stress.sqlite");
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

    let jwt_secret = "m1_challenger_stress_secret_key_9876543210_adversarial";
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
            email: "adversarial_owner@tenant-a.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "adversarial_owner@tenant-a.com", "user", "premium")
        .unwrap();

    // 2. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "adversarial_staff@tenant-a.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "adversarial_staff@tenant-a.com", "user", "free")
        .unwrap();

    // 3. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Adversarial Tenant Alpha".to_string(),
            slug: "adversarial-tenant-alpha".to_string(),
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
            user_id: staff_id.clone(),
            role: Role::Staff,
        })
        .await
        .unwrap();

    // 4. Tenant B & User B for isolation checks
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "adversarial_b@tenant-b.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "User B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "adversarial_b@tenant-b.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Adversarial Tenant Beta".to_string(),
            slug: "adversarial-tenant-beta".to_string(),
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
        staff_token,
        tenant_a_id,
        staff_id,
        tenant_b_id,
        tenant_b_token,
        user_b_id,
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

// ============================================================================
// 1. Project State Machine Permutation Stress Tests
// ============================================================================

#[tokio::test]
async fn test_project_state_machine_illegal_transition_permutations() {
    let h = setup_challenger_harness().await;

    // Helper to create a fresh project in DRAFT status
    async fn create_project_in_status(
        h: &ChallengerHarness,
        target_status: &str,
    ) -> String {
        let (status, body, _) = send_req(
            &h.app,
            Method::POST,
            "/api/v1/projects",
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({
                "name": format!("Permutation Test Project {}", Uuid::new_v4()),
                "customer_name": "Test Client"
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let project_id = body["id"].as_str().unwrap().to_string();

        if target_status == "DRAFT" {
            return project_id;
        }

        if target_status == "ACTIVE" {
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
            return project_id;
        }

        if target_status == "ON_HOLD" {
            // DRAFT -> ACTIVE -> ON_HOLD
            send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/status", project_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "ACTIVE" })),
            )
            .await;
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
            return project_id;
        }

        if target_status == "COMPLETED" {
            // DRAFT -> ACTIVE -> COMPLETED
            send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/status", project_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "ACTIVE" })),
            )
            .await;
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
            return project_id;
        }

        if target_status == "CANCELLED" {
            // DRAFT -> CANCELLED
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/status", project_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "CANCELLED" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return project_id;
        }

        panic!("Unknown status {}", target_status);
    }

    // List of all illegal transitions: (start_status, target_illegal_status)
    let illegal_transitions = vec![
        // From DRAFT:
        ("DRAFT", "ON_HOLD"),
        ("DRAFT", "COMPLETED"), // Explicitly stated in requirements
        // From ACTIVE:
        ("ACTIVE", "DRAFT"),
        // From ON_HOLD:
        ("ON_HOLD", "DRAFT"),
        ("ON_HOLD", "COMPLETED"),
        // From COMPLETED (terminal):
        ("COMPLETED", "DRAFT"),
        ("COMPLETED", "ACTIVE"),
        ("COMPLETED", "ON_HOLD"),
        ("COMPLETED", "CANCELLED"),
        // From CANCELLED (terminal):
        ("CANCELLED", "DRAFT"),
        ("CANCELLED", "ACTIVE"),
        ("CANCELLED", "ON_HOLD"),
        ("CANCELLED", "COMPLETED"),
    ];

    for (start_status, illegal_target) in illegal_transitions {
        let p_id = create_project_in_status(&h, start_status).await;

        let (status, body, _) = send_req(
            &h.app,
            Method::PATCH,
            &format!("/api/v1/projects/{}/status", p_id),
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({ "status": illegal_target })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Illegal transition from {} to {} must return HTTP 422 Unprocessable Entity, got {} (body: {:?})",
            start_status,
            illegal_target,
            status,
            body
        );
        assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");
    }
}

// ============================================================================
// 2. Milestone State Machine Permutation Stress Tests
// ============================================================================

#[tokio::test]
async fn test_milestone_state_machine_illegal_transition_permutations() {
    let h = setup_challenger_harness().await;

    // Create a base project
    let (status, p_body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Milestone State Permutation Project",
            "customer_name": "Test Customer"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = p_body["id"].as_str().unwrap();

    // Helper to create milestone in given status
    async fn create_milestone_in_status(
        h: &ChallengerHarness,
        project_id: &str,
        target_status: &str,
    ) -> String {
        let (status, body, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/milestones", project_id),
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({
                "title": format!("Milestone {}", Uuid::new_v4()),
                "target_date": "2026-12-01",
                "billable_amount": 50000000
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let m_id = body["id"].as_str().unwrap().to_string();

        if target_status == "PENDING" {
            return m_id;
        }

        if target_status == "IN_PROGRESS" {
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/milestones/{}/status", project_id, m_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "IN_PROGRESS" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return m_id;
        }

        if target_status == "COMPLETED" {
            send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/milestones/{}/status", project_id, m_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "IN_PROGRESS" })),
            )
            .await;
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/milestones/{}/status", project_id, m_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "COMPLETED" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return m_id;
        }

        if target_status == "CANCELLED" {
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/milestones/{}/status", project_id, m_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "CANCELLED" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return m_id;
        }

        panic!("Unknown status {}", target_status);
    }

    let illegal_milestone_transitions = vec![
        // From PENDING:
        ("PENDING", "COMPLETED"), // illegal jump without IN_PROGRESS
        // From IN_PROGRESS:
        ("IN_PROGRESS", "PENDING"), // illegal backward transition
        // From COMPLETED (terminal):
        ("COMPLETED", "PENDING"),
        ("COMPLETED", "IN_PROGRESS"),
        ("COMPLETED", "CANCELLED"),
        // From CANCELLED (terminal):
        ("CANCELLED", "PENDING"),
        ("CANCELLED", "IN_PROGRESS"),
        ("CANCELLED", "COMPLETED"),
    ];

    for (start_status, illegal_target) in illegal_milestone_transitions {
        let m_id = create_milestone_in_status(&h, project_id, start_status).await;

        let (status, body, _) = send_req(
            &h.app,
            Method::PATCH,
            &format!("/api/v1/projects/{}/milestones/{}/status", project_id, m_id),
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({ "status": illegal_target })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Illegal milestone transition from {} to {} must return HTTP 422, got {}",
            start_status,
            illegal_target,
            status
        );
        assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");
    }

    // Additional test: Billed milestone status update guard
    let completed_m_id = create_milestone_in_status(&h, project_id, "COMPLETED").await;
    // Simulate billing by setting is_billed = 1 directly in DB
    sqlx::query("UPDATE milestones SET is_billed = 1 WHERE id = ?")
        .bind(&completed_m_id)
        .execute(&h.pool)
        .await
        .unwrap();

    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/milestones/{}/status", project_id, completed_m_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "MILESTONE_ALREADY_BILLED");
}

// ============================================================================
// 3. Task State Machine Permutation Stress Tests
// ============================================================================

#[tokio::test]
async fn test_task_state_machine_illegal_transition_permutations() {
    let h = setup_challenger_harness().await;

    // Create a base project
    let (status, p_body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Task State Permutation Project",
            "customer_name": "Test Customer"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = p_body["id"].as_str().unwrap();

    // Helper to create task in given status
    async fn create_task_in_status(
        h: &ChallengerHarness,
        project_id: &str,
        target_status: &str,
    ) -> String {
        let (status, body, _) = send_req(
            &h.app,
            Method::POST,
            &format!("/api/v1/projects/{}/tasks", project_id),
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({
                "title": format!("Task {}", Uuid::new_v4()),
                "estimated_hours": 10
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let t_id = body["id"].as_str().unwrap().to_string();

        if target_status == "TODO" {
            return t_id;
        }

        if target_status == "IN_PROGRESS" {
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/tasks/{}/status", project_id, t_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "IN_PROGRESS" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return t_id;
        }

        if target_status == "BLOCKED" {
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/tasks/{}/status", project_id, t_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "BLOCKED" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return t_id;
        }

        if target_status == "DONE" {
            send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/tasks/{}/status", project_id, t_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "IN_PROGRESS" })),
            )
            .await;
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/tasks/{}/status", project_id, t_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "DONE" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return t_id;
        }

        if target_status == "CANCELLED" {
            let (status, _, _) = send_req(
                &h.app,
                Method::PATCH,
                &format!("/api/v1/projects/{}/tasks/{}/status", project_id, t_id),
                &h.owner_token,
                &h.tenant_a_id,
                Some(json!({ "status": "CANCELLED" })),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            return t_id;
        }

        panic!("Unknown status {}", target_status);
    }

    let illegal_task_transitions = vec![
        // From TODO:
        ("TODO", "DONE"), // illegal direct jump to DONE
        // From BLOCKED:
        ("BLOCKED", "TODO"),
        ("BLOCKED", "DONE"), // cannot jump straight to DONE from BLOCKED
        // From DONE:
        ("DONE", "TODO"),
        ("DONE", "BLOCKED"),
        ("DONE", "CANCELLED"),
        // From CANCELLED (terminal):
        ("CANCELLED", "TODO"),
        ("CANCELLED", "IN_PROGRESS"),
        ("CANCELLED", "BLOCKED"),
        ("CANCELLED", "DONE"),
    ];

    for (start_status, illegal_target) in illegal_task_transitions {
        let t_id = create_task_in_status(&h, project_id, start_status).await;

        let (status, body, _) = send_req(
            &h.app,
            Method::PATCH,
            &format!("/api/v1/projects/{}/tasks/{}/status", project_id, t_id),
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({ "status": illegal_target })),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Illegal task transition from {} to {} must return HTTP 422, got {}",
            start_status,
            illegal_target,
            status
        );
        assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");
    }
}

// ============================================================================
// 4. RBAC Permission Boundary Stress Tests
// ============================================================================

#[tokio::test]
async fn test_rbac_permission_boundaries_staff_vs_management() {
    let h = setup_challenger_harness().await;

    // 1. Owner sets up a valid project, milestone, and task
    let (status, p_body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "RBAC Verification Project",
            "customer_name": "Test Customer"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = p_body["id"].as_str().unwrap();

    let (status, m_body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Foundation Milestone",
            "target_date": "2026-11-01",
            "billable_amount": 20000000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let milestone_id = m_body["id"].as_str().unwrap();

    let (status, t_body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Excavation Task",
            "milestone_id": milestone_id,
            "estimated_hours": 40
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = t_body["id"].as_str().unwrap();

    // 2. Staff attempts Management Mutation 1: Create Project -> MUST return 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Staff Illegal Project",
            "customer_name": "Client"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff create project must return 403 Forbidden");
    assert_eq!(body["code"], "FORBIDDEN");

    // 3. Staff attempts Management Mutation 2: Update Project Status -> MUST return 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff update project status must return 403 Forbidden");
    assert_eq!(body["code"], "FORBIDDEN");

    // 4. Staff attempts Management Mutation 3: Add Project Member -> MUST return 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "WORKER",
            "cost_rate": 50000,
            "billing_rate": 100000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff add project member must return 403 Forbidden");
    assert_eq!(body["code"], "FORBIDDEN");

    // 5. Staff attempts Management Mutation 4: Create Milestone -> MUST return 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Staff Illegal Milestone",
            "target_date": "2026-12-01"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff create milestone must return 403 Forbidden");
    assert_eq!(body["code"], "FORBIDDEN");

    // 6. Staff attempts Management Mutation 5: Update Milestone Status -> MUST return 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/milestones/{}/status", project_id, milestone_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff update milestone status must return 403 Forbidden");
    assert_eq!(body["code"], "FORBIDDEN");

    // 7. Staff attempts Management Mutation 6: Create Task -> MUST return 403 Forbidden
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Staff Illegal Task"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff create task must return 403 Forbidden");
    assert_eq!(body["code"], "FORBIDDEN");

    // 8. Staff performs Allowed Reads -> MUST return 200 OK
    let (status, _, _) = send_req(&h.app, Method::GET, "/api/v1/projects", &h.staff_token, &h.tenant_a_id, None).await;
    assert_eq!(status, StatusCode::OK, "Staff must be allowed to list projects");

    let (status, _, _) = send_req(&h.app, Method::GET, &format!("/api/v1/projects/{}", project_id), &h.staff_token, &h.tenant_a_id, None).await;
    assert_eq!(status, StatusCode::OK, "Staff must be allowed to get project details");

    let (status, _, _) = send_req(&h.app, Method::GET, &format!("/api/v1/projects/{}/members", project_id), &h.staff_token, &h.tenant_a_id, None).await;
    assert_eq!(status, StatusCode::OK, "Staff must be allowed to list project members");

    let (status, _, _) = send_req(&h.app, Method::GET, &format!("/api/v1/projects/{}/milestones", project_id), &h.staff_token, &h.tenant_a_id, None).await;
    assert_eq!(status, StatusCode::OK, "Staff must be allowed to list milestones");

    let (status, _, _) = send_req(&h.app, Method::GET, &format!("/api/v1/projects/{}/tasks", project_id), &h.staff_token, &h.tenant_a_id, None).await;
    assert_eq!(status, StatusCode::OK, "Staff must be allowed to list tasks");

    // 9. Staff updates Task Status -> MUST succeed with 200 OK
    let (status, body, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/tasks/{}/status", project_id, task_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Staff must be allowed to transition task status to IN_PROGRESS");
    assert_eq!(body["status"], "IN_PROGRESS");
}

// ============================================================================
// 5. Boundary Value Validation (Service & HTTP Layer)
// ============================================================================

#[tokio::test]
async fn test_boundary_values_service_validation() {
    let h = setup_challenger_harness().await;

    // 1. Negative budget amount
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Negative Budget Project",
            "budget_amount": -1000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");

    // 2. Extreme negative budget amount (i64 underflow boundary)
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Underflow Budget Project",
            "budget_amount": -9223372036854775808i64
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");

    // 3. Negative contract amount
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Negative Contract Project",
            "contract_amount": -500000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");

    // Create a valid project for subsequent member, milestone, and task checks
    let (status, p_body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Boundary Testing Project",
            "budget_amount": 100000000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = p_body["id"].as_str().unwrap();

    // 4. Negative cost rate for member
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "WORKER",
            "cost_rate": -1
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_RATE");

    // 5. Negative billing rate for member
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_id,
            "role": "WORKER",
            "billing_rate": -50000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_RATE");

    // 6. Negative sequence order for milestone
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Invalid Sequence Milestone",
            "target_date": "2026-12-01",
            "sequence_order": -1
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_SEQUENCE");

    // 7. Zero sequence order for milestone (must be strictly > 0 per schema CHECK)
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Zero Sequence Milestone",
            "target_date": "2026-12-01",
            "sequence_order": 0
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_SEQUENCE");

    // 8. Negative billable amount for milestone
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Negative Billable Milestone",
            "target_date": "2026-12-01",
            "billable_amount": -100
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");

    // 9. Negative estimated hours for task
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Negative Hours Task",
            "estimated_hours": -5
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_HOURS");
}

// ============================================================================
// 6. Direct SQLite Database Check Constraint Invariant Enforcement
// ============================================================================

#[tokio::test]
async fn test_boundary_values_sqlite_check_constraints() {
    let h = setup_challenger_harness().await;

    let now_str = chrono::Utc::now().to_rfc3339();
    let project_id = Uuid::new_v4().to_string();

    // 1. Projects: budget_amount >= 0 constraint
    let res = sqlx::query(
        "INSERT INTO projects (id, tenant_id, project_number, name, customer_name, budget_amount, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind("PRJ-2026-TEST01")
    .bind("Raw DB Project")
    .bind("Client")
    .bind(-50)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject budget_amount < 0");

    // 2. Projects: contract_amount >= 0 constraint
    let res = sqlx::query(
        "INSERT INTO projects (id, tenant_id, project_number, name, customer_name, contract_amount, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind("PRJ-2026-TEST02")
    .bind("Raw DB Project")
    .bind("Client")
    .bind(-1)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject contract_amount < 0");

    // 3. Projects: status check constraint
    let res = sqlx::query(
        "INSERT INTO projects (id, tenant_id, project_number, name, customer_name, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind("PRJ-2026-TEST03")
    .bind("Raw DB Project")
    .bind("Client")
    .bind("ILLEGAL_STATUS")
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject invalid status");

    // Insert valid parent project for subsequent table checks
    sqlx::query(
        "INSERT INTO projects (id, tenant_id, project_number, name, customer_name, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&project_id)
    .bind(&h.tenant_a_id)
    .bind("PRJ-2026-VALID01")
    .bind("Valid Parent Project")
    .bind("Client")
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await
    .unwrap();

    // 4. Project Members: cost_rate >= 0
    let res = sqlx::query(
        "INSERT INTO project_members (id, tenant_id, project_id, user_id, cost_rate, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(&h.staff_id)
    .bind(-10)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject cost_rate < 0");

    // 5. Milestones: sequence_order > 0 constraint (test sequence_order = 0)
    let res = sqlx::query(
        "INSERT INTO milestones (id, tenant_id, project_id, sequence_order, title, target_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(0)
    .bind("Zero Seq")
    .bind("2026-12-01")
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject sequence_order <= 0");

    // 6. Milestones: billable_amount >= 0 constraint
    let res = sqlx::query(
        "INSERT INTO milestones (id, tenant_id, project_id, sequence_order, title, target_date, billable_amount, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(1)
    .bind("Negative Billable")
    .bind("2026-12-01")
    .bind(-500)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject billable_amount < 0");

    // 7. Milestones: is_billed IN (0, 1) constraint
    let res = sqlx::query(
        "INSERT INTO milestones (id, tenant_id, project_id, sequence_order, title, target_date, is_billed, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(1)
    .bind("Invalid is_billed")
    .bind("2026-12-01")
    .bind(2) // Not 0 or 1
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject is_billed not in (0, 1)");

    // 8. Tasks: estimated_hours >= 0 constraint
    let res = sqlx::query(
        "INSERT INTO tasks (id, tenant_id, project_id, title, estimated_hours, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind("Negative Hours")
    .bind(-1)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject estimated_hours < 0");

    // 9. Progress Records: percentage >= 0 AND percentage <= 100
    let res = sqlx::query(
        "INSERT INTO progress_records (id, tenant_id, project_id, percentage, record_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(101) // Greater than 100
    .bind("2026-10-05")
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject percentage > 100");

    let res = sqlx::query(
        "INSERT INTO progress_records (id, tenant_id, project_id, percentage, record_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind(-5) // Less than 0
    .bind("2026-10-05")
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject percentage < 0");

    // 10. Project Labor: hours_worked > 0 constraint
    let res = sqlx::query(
        "INSERT INTO project_labor (id, tenant_id, project_id, worker_name, work_date, hours_worked, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind("Worker")
    .bind("2026-10-05")
    .bind(0) // hours_worked > 0
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject hours_worked <= 0");

    // 11. Project Expenses: amount > 0 constraint
    let res = sqlx::query(
        "INSERT INTO project_expenses (id, tenant_id, project_id, category, description, amount, expense_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .bind("Permits")
    .bind("Building Permit")
    .bind(0) // amount > 0
    .bind("2026-10-05")
    .bind(&now_str)
    .bind(&now_str)
    .execute(&h.pool)
    .await;
    assert!(res.is_err(), "SQLite CHECK constraint must reject expense amount <= 0");
}

// ============================================================================
// 7. Extreme Inputs & Adversarial Malformed Payloads
// ============================================================================

#[tokio::test]
async fn test_extreme_inputs_and_edge_cases() {
    let h = setup_challenger_harness().await;

    // 1. Empty project names (empty string and whitespace variants)
    for empty_name in &["", "   ", "\t\r\n   "] {
        let (status, body, _) = send_req(
            &h.app,
            Method::POST,
            "/api/v1/projects",
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({ "name": empty_name })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "Empty name '{:?}' must be rejected with HTTP 400 Bad Request",
            empty_name
        );
        assert_eq!(body["code"], "INVALID_NAME");
    }

    // 2. Inverted project date ranges (end_date < start_date)
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Inverted Dates Project",
            "start_date": "2026-12-31",
            "end_date": "2026-01-01"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_DATES");

    // 3. Malformed date strings in JSON (e.g. non-ISO date, impossible dates)
    for malformed_date in &["not-a-date", "2026-02-31", "31/12/2026", "2026-13-45"] {
        let (status, _, _) = send_req(
            &h.app,
            Method::POST,
            "/api/v1/projects",
            &h.owner_token,
            &h.tenant_a_id,
            Some(json!({
                "name": "Malformed Date Project",
                "start_date": malformed_date
            })),
        )
        .await;
        // In Axum, JSON deserialization error for NaiveDate returns 400 or 422
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
            "Malformed date '{}' must fail deserialization with 400 or 422, got {}",
            malformed_date,
            status
        );
    }

    // 4. Oversized inputs: 10KB name, 64KB description, 64KB notes
    let huge_name = "P".repeat(10_000);
    let huge_desc = "D".repeat(65_536);
    let huge_notes = "N".repeat(65_536);

    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": huge_name,
            "description": huge_desc,
            "notes": huge_notes,
            "budget_amount": 500000000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Server must cleanly handle large string payloads without panic");
    let oversized_project_id = body["id"].as_str().unwrap().to_string();

    // 5. Empty titles for sub-entities
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", oversized_project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "   ",
            "target_date": "2026-12-01"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_TITLE");

    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", oversized_project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": ""
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_TITLE");

    // 6. SQL Injection attempts in string fields
    let sqli_payload = "'; DROP TABLE projects; -- ' OR '1'='1";
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": sqli_payload,
            "description": sqli_payload,
            "customer_name": sqli_payload
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "SQL injection string must be safely bound as text");
    assert_eq!(body["name"], sqli_payload);

    // Verify projects table is still intact and queryable
    let (status, _, _) = send_req(&h.app, Method::GET, "/api/v1/projects", &h.owner_token, &h.tenant_a_id, None).await;
    assert_eq!(status, StatusCode::OK);

    // 7. Cross-tenant user membership isolation:
    // Attempt to assign Tenant B's user as member in Tenant A's project
    let (status, body, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", oversized_project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.user_b_id,
            "role": "PROJECT_MANAGER"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Assigning foreign tenant user must return 404 User Not Found");
    assert_eq!(body["code"], "USER_NOT_FOUND");

    // 8. Cross-tenant probing by Tenant B caller on Tenant A's project:
    let (status, body, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}", oversized_project_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant probe must strictly return HTTP 404 Not Found");
    assert_eq!(body["code"], "NOT_FOUND");
}
