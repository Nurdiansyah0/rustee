//! Milestone 2 Round 2 Empirical Adversarial Challenger Verification Suite
//!
//! Identity: teamwork_preview_challenger_m2_r2_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Empirical Verification Objectives:
//! 1. Reversal Concurrency Races:
//!    - High concurrency (10 concurrent racing tasks on the same journal entry)
//!    - Extreme concurrency (20 concurrent racing tasks on the same journal entry)
//!    - Assert: Exactly 1 winner (HTTP 201 Created)
//!    - Assert: All competing tasks return HTTP 409 Conflict with code "ALREADY_REVERSED"
//!    - Assert: ZERO HTTP 500 Internal Server Errors or lock deadlocks
//!    - Assert: Invariant database state: exactly 1 reversal entry created, original.reversal_entry_id linked, trial balance balanced to net zero.
//! 2. Complete RBAC Authorization Matrix:
//!    - POST /api/v1/accounting/journals:
//!      - Owner -> 201 Created
//!      - Administrator -> 201 Created
//!      - Accountant -> 201 Created
//!      - Manager -> 403 Forbidden ("FORBIDDEN")
//!      - Staff -> 403 Forbidden ("FORBIDDEN")
//!      - Custom("viewer") -> 403 Forbidden ("FORBIDDEN")
//!    - POST /api/v1/accounting/journals/{id}/reverse:
//!      - Owner -> 201 Created
//!      - Accountant -> 201 Created
//!      - Administrator -> 403 Forbidden ("FORBIDDEN")
//!      - Manager -> 403 Forbidden ("FORBIDDEN")
//!      - Staff -> 403 Forbidden ("FORBIDDEN")
//!      - Custom("viewer") -> 403 Forbidden ("FORBIDDEN")
//!    - Programmatic post_journal_command RBAC validation:
//!      - Owner, Admin, Accountant succeed; Manager, Staff, Custom fail with Forbidden.
//!    - Chart of Accounts RBAC validation:
//!      - create_account & delete_account: Owner, Admin, Accountant succeed; Manager, Staff, Custom fail.
//! 3. Adversarial Edge Cases:
//!    - Sequential double reversal strictly returns 409 ALREADY_REVERSED.
//!    - Cross-tenant reversal attempt strictly returns 404 NOT_FOUND.
//!    - Database trigger verification: direct SQL mutation on posted journals/lines blocked.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::accounting::{PostJournalEntryCommand, PostJournalLineCommand};
use backend::domain::money::Rupiah;
use backend::domain::tenant::{Role, TenantContext};
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
    accounting_service::AccountingService,
    auth_service::AuthService,
    crypto::{Argon2Config, CryptoService},
    jwt::JwtEngine,
    ledger_service::LedgerService,
    payment_service::{PaymentConfig, PaymentService},
    tenant_service::TenantService,
};
use chrono::Utc;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

async fn create_test_role_user(
    pool: &sqlx::SqlitePool,
    user_repo: &SqlxUserRepository,
    jwt_engine: &JwtEngine,
    tenant_id: &str,
    role: Role,
    email: &str,
    name: &str,
) -> String {
    let uid = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: uid.clone(),
            email: email.to_string(),
            password_hash: "hash".to_string(),
            display_name: name.to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let m_repo = SqlxMembershipRepository::new(pool.clone());
    m_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_id.to_string(),
            user_id: uid.clone(),
            role,
        })
        .await
        .unwrap();

    let (token, _) = jwt_engine.generate_token(&uid, email, "user", "premium").unwrap();
    token
}

#[allow(dead_code)]
struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    jwt_engine: Arc<JwtEngine>,
    tenant_a_id: String,
    tenant_b_id: String,
    accounting_service: Arc<AccountingService>,
    // Tokens for all 6 roles
    owner_token: String,
    admin_token: String,
    accountant_token: String,
    manager_token: String,
    staff_token: String,
    viewer_token: String,
    // Cross tenant token
    tenant_b_accountant_token: String,
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m2_r2.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 20,
        min_connections: 2,
        busy_timeout_ms: 10_000,
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

    let jwt_secret = "m2_r2_challenger_secret_key_1234567890_ultra_secure";
    let jwt_engine = Arc::new(JwtEngine::new(jwt_secret, 3600));

    let crypto_service = Arc::new(
        CryptoService::new(Argon2Config::fast_for_testing()).unwrap(),
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

    let payment_service = Arc::new(PaymentService::new(
        PaymentConfig::default(),
        subscription_repo,
        user_repo.clone(),
        audit_repo,
    ));

    let tenant_repo = Arc::new(SqlxTenantRepository::new(pool.clone()));
    let tenant_service = Arc::new(TenantService::new_with_pool(pool.clone()));
    let accounting_repo = Arc::new(backend::repository::accounting_repo::SqlxAccountingRepository::new(pool.clone()));
    let accounting_service = Arc::new(AccountingService::new(pool.clone(), accounting_repo));

    let auth_state = AuthState {
        auth_service,
        secure_cookie: false,
    };

    let state = AppState {
        auth_state,
        account_repo,
        category_repo,
        user_preferences_repo: Arc::new(backend::repository::SqlxUserPreferencesRepository::new(pool.clone())),
        ledger_service,
        payment_service,
        tenant_service,
        tenant_repo,
        pool: pool.clone(),
        rate_limiter: Arc::default(),
    };

    let app = create_app(state);

    // 1. Setup Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo_inst = SqlxTenantRepository::new(pool.clone());

    tenant_repo_inst
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Corp".to_string(),
            slug: "tenant-alpha-corp".to_string(),
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

    let owner_token = create_test_role_user(&pool, &user_repo, &jwt_engine, &tenant_a_id, Role::Owner, "owner@alpha.com", "Alpha Owner").await;
    let admin_token = create_test_role_user(&pool, &user_repo, &jwt_engine, &tenant_a_id, Role::Administrator, "admin@alpha.com", "Alpha Admin").await;
    let accountant_token = create_test_role_user(&pool, &user_repo, &jwt_engine, &tenant_a_id, Role::Accountant, "accountant@alpha.com", "Alpha Accountant").await;
    let manager_token = create_test_role_user(&pool, &user_repo, &jwt_engine, &tenant_a_id, Role::Manager, "manager@alpha.com", "Alpha Manager").await;
    let staff_token = create_test_role_user(&pool, &user_repo, &jwt_engine, &tenant_a_id, Role::Staff, "staff@alpha.com", "Alpha Staff").await;
    let viewer_token = create_test_role_user(&pool, &user_repo, &jwt_engine, &tenant_a_id, Role::Custom("viewer".to_string()), "viewer@alpha.com", "Alpha Viewer").await;

    // 2. Setup Tenant B (for cross-tenant attack tests)
    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo_inst
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Ltd".to_string(),
            slug: "tenant-beta-ltd".to_string(),
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

    let tenant_b_accountant_token = create_test_role_user(
        &pool,
        &user_repo,
        &jwt_engine,
        &tenant_b_id,
        Role::Accountant,
        "acct@beta.com",
        "Beta Accountant",
    )
    .await;

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        jwt_engine,
        tenant_a_id,
        tenant_b_id,
        accounting_service,
        owner_token,
        admin_token,
        accountant_token,
        manager_token,
        staff_token,
        viewer_token,
        tenant_b_accountant_token,
    }
}

async fn send_request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token));

    if let Some(t_id) = tenant_id {
        req = req.header("X-Tenant-ID", t_id);
    }

    let req = if let Some(b) = body {
        req.header(CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&b).unwrap()))
            .unwrap()
    } else {
        req.body(Body::empty()).unwrap()
    };

    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

    (status, json)
}

// ============================================================================
// 1. REVERSAL CONCURRENCY RACES EMPIRICAL CHALLENGE
// ============================================================================

#[tokio::test]
async fn challenge_reversal_concurrency_race_10_simultaneous_tasks() {
    let harness = setup_challenger_harness().await;

    // 1. Post a legitimate balanced journal entry in Tenant A
    let (post_status, post_body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Journal for 10-Task Concurrency Race",
            "lines": [
                {"account_code": "1000", "debit": 5_000_000, "credit": 0, "memo": "Cash In"},
                {"account_code": "4000", "debit": 0, "credit": 5_000_000, "memo": "Revenue"}
            ]
        })),
    )
    .await;

    assert_eq!(post_status, StatusCode::CREATED);
    let orig_id = post_body["id"].as_str().unwrap().to_string();

    // 2. Concurrently spawn 10 simultaneous tasks competing to reverse the exact same journal
    let num_tasks = 10;
    let mut join_set = tokio::task::JoinSet::new();

    for i in 0..num_tasks {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_a_id.clone();
        let journal_id = orig_id.clone();

        join_set.spawn(async move {
            send_request(
                &app,
                Method::POST,
                &format!("/api/v1/accounting/journals/{}/reverse", journal_id),
                &token,
                Some(&tenant_id),
                Some(json!({
                    "reason": format!("Concurrent Race Attempt #{}", i)
                })),
            )
            .await
        });
    }

    let mut created_count = 0;
    let mut conflict_count = 0;
    let mut server_error_count = 0;
    let mut other_statuses = Vec::new();

    while let Some(res) = join_set.join_next().await {
        let (status, body) = res.expect("Task failed to join");
        match status {
            StatusCode::CREATED => {
                created_count += 1;
                // Verify created response format
                assert_eq!(body["status"], "POSTED");
                assert_eq!(body["source_type"], "REVERSAL");
                assert_eq!(body["source_id"], orig_id);
            }
            StatusCode::CONFLICT => {
                conflict_count += 1;
                // Verify RFC7807/AppError structure and specific ALREADY_REVERSED code
                assert_eq!(
                    body["code"], "ALREADY_REVERSED",
                    "Expected ALREADY_REVERSED code on 409 Conflict, got: {:?}",
                    body
                );
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                server_error_count += 1;
                eprintln!("UNEXPECTED HTTP 500: {:?}", body);
            }
            other => {
                other_statuses.push((other, body));
            }
        }
    }

    // EMPIRICAL INVARIANTS:
    // 1. Exactly one winner (201 Created)
    assert_eq!(
        created_count, 1,
        "Violation: Exactly 1 reversal must succeed! Observed {} winners.",
        created_count
    );

    // 2. All competing attempts must fail with 409 Conflict (ALREADY_REVERSED)
    assert_eq!(
        conflict_count,
        num_tasks - 1,
        "Violation: Expected {} conflicts, observed {}.",
        num_tasks - 1,
        conflict_count
    );

    // 3. ZERO 500 Internal Server Errors
    assert_eq!(
        server_error_count, 0,
        "Violation: Zero HTTP 500 errors allowed during concurrency race! Got {}.",
        server_error_count
    );

    // 4. Zero unexpected statuses
    assert!(
        other_statuses.is_empty(),
        "Unexpected HTTP statuses encountered: {:?}",
        other_statuses
    );

    // 5. Database state invariants verification
    let (get_s, get_b) = send_request(
        &harness.app,
        Method::GET,
        &format!("/api/v1/accounting/journals/{}", orig_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(get_s, StatusCode::OK);
    assert_eq!(get_b["is_reversed"], 1);
    assert!(get_b["reversal_entry_id"].is_string());

    // 6. Trial Balance net impact must be exactly 0
    let (tb_s, tb_b) = send_request(
        &harness.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb_s, StatusCode::OK);
    assert_eq!(tb_b["is_balanced"], true);
    assert_eq!(tb_b["net_balance"], 0);
}

#[tokio::test]
async fn challenge_reversal_concurrency_race_20_simultaneous_tasks() {
    let harness = setup_challenger_harness().await;

    // 1. Post a journal entry
    let (post_status, post_body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.accountant_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Journal for 20-Task Extreme Concurrency Race",
            "lines": [
                {"account_code": "1000", "debit": 12_345_678, "credit": 0, "memo": "Asset Debit"},
                {"account_code": "4000", "debit": 0, "credit": 12_345_678, "memo": "Revenue Credit"}
            ]
        })),
    )
    .await;

    assert_eq!(post_status, StatusCode::CREATED);
    let orig_id = post_body["id"].as_str().unwrap().to_string();

    // 2. Concurrently spawn 20 simultaneous tasks
    let num_tasks = 20;
    let mut join_set = tokio::task::JoinSet::new();

    for i in 0..num_tasks {
        let app = harness.app.clone();
        let token = harness.accountant_token.clone();
        let tenant_id = harness.tenant_a_id.clone();
        let journal_id = orig_id.clone();

        join_set.spawn(async move {
            send_request(
                &app,
                Method::POST,
                &format!("/api/v1/accounting/journals/{}/reverse", journal_id),
                &token,
                Some(&tenant_id),
                Some(json!({
                    "reason": format!("Extreme Race #{}", i)
                })),
            )
            .await
        });
    }

    let mut created_count = 0;
    let mut conflict_count = 0;
    let mut server_error_count = 0;

    while let Some(res) = join_set.join_next().await {
        let (status, body) = res.expect("Task failed to join");
        match status {
            StatusCode::CREATED => created_count += 1,
            StatusCode::CONFLICT => {
                assert_eq!(body["code"], "ALREADY_REVERSED");
                conflict_count += 1;
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                server_error_count += 1;
                eprintln!("UNEXPECTED HTTP 500: {:?}", body);
            }
            other => panic!("Unexpected status code in 20-task race: {}", other),
        }
    }

    assert_eq!(created_count, 1, "Exactly 1 winner expected in 20-task race");
    assert_eq!(conflict_count, 19, "Exactly 19 conflicts expected in 20-task race");
    assert_eq!(server_error_count, 0, "Zero 500 errors expected in 20-task race");
}

#[tokio::test]
async fn challenge_sequential_double_reversal_strictly_returns_409() {
    let harness = setup_challenger_harness().await;

    // 1. Post a journal
    let (post_status, post_body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Sequential Double Reversal Test",
            "lines": [
                {"account_code": "1000", "debit": 100_000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100_000}
            ]
        })),
    )
    .await;

    assert_eq!(post_status, StatusCode::CREATED);
    let orig_id = post_body["id"].as_str().unwrap().to_string();

    // 2. First reversal -> 201 Created
    let (rev1_status, rev1_body) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "First Reversal"})),
    )
    .await;

    assert_eq!(rev1_status, StatusCode::CREATED);
    assert_eq!(rev1_body["source_type"], "REVERSAL");

    // 3. Second reversal attempt -> MUST return 409 Conflict with ALREADY_REVERSED
    let (rev2_status, rev2_body) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Duplicate Reversal Attempt"})),
    )
    .await;

    assert_eq!(rev2_status, StatusCode::CONFLICT);
    assert_eq!(rev2_body["code"], "ALREADY_REVERSED");

    // 4. Third reversal attempt -> STILL returns 409 Conflict with ALREADY_REVERSED
    let (rev3_status, rev3_body) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &harness.accountant_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Third Attempt by Accountant"})),
    )
    .await;

    assert_eq!(rev3_status, StatusCode::CONFLICT);
    assert_eq!(rev3_body["code"], "ALREADY_REVERSED");
}

// ============================================================================
// 2. COMPLETE RBAC AUTHORIZATION MATRIX EMPIRICAL CHALLENGE
// ============================================================================

#[tokio::test]
async fn challenge_rbac_post_journal_all_roles_matrix() {
    let harness = setup_challenger_harness().await;

    let payload = json!({
        "description": "RBAC Posting Test",
        "lines": [
            {"account_code": "1000", "debit": 250_000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 250_000}
        ]
    });

    // Sub-test 1: Owner -> ALLOWED (201)
    let (s_owner, b_owner) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload.clone()),
    )
    .await;
    assert_eq!(s_owner, StatusCode::CREATED, "Owner must be allowed to post journal");
    assert_eq!(b_owner["status"], "POSTED");

    // Sub-test 2: Administrator -> ALLOWED (201)
    let (s_admin, b_admin) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.admin_token,
        Some(&harness.tenant_a_id),
        Some(payload.clone()),
    )
    .await;
    assert_eq!(s_admin, StatusCode::CREATED, "Administrator must be allowed to post journal");
    assert_eq!(b_admin["status"], "POSTED");

    // Sub-test 3: Accountant -> ALLOWED (201)
    let (s_acct, b_acct) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.accountant_token,
        Some(&harness.tenant_a_id),
        Some(payload.clone()),
    )
    .await;
    assert_eq!(s_acct, StatusCode::CREATED, "Accountant must be allowed to post journal");
    assert_eq!(b_acct["status"], "POSTED");

    // Sub-test 4: Manager -> FORBIDDEN (403)
    let (s_mgr, b_mgr) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.manager_token,
        Some(&harness.tenant_a_id),
        Some(payload.clone()),
    )
    .await;
    assert_eq!(s_mgr, StatusCode::FORBIDDEN, "Manager must be rejected with 403 Forbidden");
    assert_eq!(b_mgr["code"], "FORBIDDEN");

    // Sub-test 5: Staff -> FORBIDDEN (403)
    let (s_staff, b_staff) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.staff_token,
        Some(&harness.tenant_a_id),
        Some(payload.clone()),
    )
    .await;
    assert_eq!(s_staff, StatusCode::FORBIDDEN, "Staff must be rejected with 403 Forbidden");
    assert_eq!(b_staff["code"], "FORBIDDEN");

    // Sub-test 6: Custom("viewer") -> FORBIDDEN (403)
    let (s_viewer, b_viewer) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.viewer_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;
    assert_eq!(s_viewer, StatusCode::FORBIDDEN, "Viewer/Custom must be rejected with 403 Forbidden");
    assert_eq!(b_viewer["code"], "FORBIDDEN");
}

#[tokio::test]
async fn challenge_rbac_reverse_journal_all_roles_matrix() {
    let harness = setup_challenger_harness().await;

    // Helper to post a journal
    let post_entry = |desc: &str| {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_a_id.clone();
        let desc = desc.to_string();
        async move {
            let (status, body) = send_request(
                &app,
                Method::POST,
                "/api/v1/accounting/journals",
                &token,
                Some(&tenant_id),
                Some(json!({
                    "description": desc,
                    "lines": [
                        {"account_code": "1000", "debit": 150_000, "credit": 0},
                        {"account_code": "4000", "debit": 0, "credit": 150_000}
                    ]
                })),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED);
            body["id"].as_str().unwrap().to_string()
        }
    };

    // Sub-test 1: Owner -> ALLOWED (201)
    let j_owner = post_entry("Entry for Owner Reversal").await;
    let (s_owner, b_owner) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j_owner),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Owner Reversal"})),
    )
    .await;
    assert_eq!(s_owner, StatusCode::CREATED, "Owner must be allowed to reverse journal");
    assert_eq!(b_owner["status"], "POSTED");
    assert_eq!(b_owner["source_type"], "REVERSAL");

    // Sub-test 2: Accountant -> ALLOWED (201)
    let j_acct = post_entry("Entry for Accountant Reversal").await;
    let (s_acct, b_acct) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j_acct),
        &harness.accountant_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Accountant Reversal"})),
    )
    .await;
    assert_eq!(s_acct, StatusCode::CREATED, "Accountant must be allowed to reverse journal");
    assert_eq!(b_acct["status"], "POSTED");
    assert_eq!(b_acct["source_type"], "REVERSAL");

    // Sub-test 3: Administrator -> FORBIDDEN (403)
    // Per domain contract in Role::can_reverse_journal(), only Owner and Accountant can reverse
    let j_admin = post_entry("Entry for Admin Reversal").await;
    let (s_admin, b_admin) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j_admin),
        &harness.admin_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Admin Reversal Attempt"})),
    )
    .await;
    assert_eq!(s_admin, StatusCode::FORBIDDEN, "Administrator cannot reverse journals");
    assert_eq!(b_admin["code"], "FORBIDDEN");

    // Sub-test 4: Manager -> FORBIDDEN (403)
    let j_mgr = post_entry("Entry for Manager Reversal").await;
    let (s_mgr, b_mgr) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j_mgr),
        &harness.manager_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Manager Reversal Attempt"})),
    )
    .await;
    assert_eq!(s_mgr, StatusCode::FORBIDDEN, "Manager cannot reverse journals");
    assert_eq!(b_mgr["code"], "FORBIDDEN");

    // Sub-test 5: Staff -> FORBIDDEN (403)
    let j_staff = post_entry("Entry for Staff Reversal").await;
    let (s_staff, b_staff) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j_staff),
        &harness.staff_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Staff Reversal Attempt"})),
    )
    .await;
    assert_eq!(s_staff, StatusCode::FORBIDDEN, "Staff cannot reverse journals");
    assert_eq!(b_staff["code"], "FORBIDDEN");

    // Sub-test 6: Custom("viewer") -> FORBIDDEN (403)
    let j_viewer = post_entry("Entry for Viewer Reversal").await;
    let (s_viewer, b_viewer) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j_viewer),
        &harness.viewer_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Viewer Reversal Attempt"})),
    )
    .await;
    assert_eq!(s_viewer, StatusCode::FORBIDDEN, "Viewer cannot reverse journals");
    assert_eq!(b_viewer["code"], "FORBIDDEN");
}

#[tokio::test]
async fn challenge_rbac_programmatic_post_journal_command() {
    let harness = setup_challenger_harness().await;
    let tenant_id = Uuid::parse_str(&harness.tenant_a_id).unwrap();

    let cmd = PostJournalEntryCommand {
        tenant_id,
        entry_date: Utc::now(),
        description: "Programmatic Contract Test".to_string(),
        source_type: "MANUAL".to_string(),
        source_id: None,
        lines: vec![
            PostJournalLineCommand {
                account_code: "1000".to_string(),
                debit: Rupiah::new(75_000),
                credit: Rupiah::new(0),
                memo: Some("Debit Cash".to_string()),
            },
            PostJournalLineCommand {
                account_code: "4000".to_string(),
                debit: Rupiah::new(0),
                credit: Rupiah::new(75_000),
                memo: Some("Credit Rev".to_string()),
            },
        ],
    };

    // Owner succeeds
    let ctx_owner = TenantContext {
        tenant_id,
        actor_id: Uuid::new_v4(),
        role: Role::Owner,
    };
    let res_owner = harness.accounting_service.post_journal_command(&ctx_owner, cmd.clone()).await;
    assert!(res_owner.is_ok(), "Owner must succeed programmatic post");

    // Administrator succeeds
    let ctx_admin = TenantContext {
        tenant_id,
        actor_id: Uuid::new_v4(),
        role: Role::Administrator,
    };
    let res_admin = harness.accounting_service.post_journal_command(&ctx_admin, cmd.clone()).await;
    assert!(res_admin.is_ok(), "Administrator must succeed programmatic post");

    // Accountant succeeds
    let ctx_acct = TenantContext {
        tenant_id,
        actor_id: Uuid::new_v4(),
        role: Role::Accountant,
    };
    let res_acct = harness.accounting_service.post_journal_command(&ctx_acct, cmd.clone()).await;
    assert!(res_acct.is_ok(), "Accountant must succeed programmatic post");

    // Manager fails with Forbidden
    let ctx_mgr = TenantContext {
        tenant_id,
        actor_id: Uuid::new_v4(),
        role: Role::Manager,
    };
    let res_mgr = harness.accounting_service.post_journal_command(&ctx_mgr, cmd.clone()).await;
    assert!(res_mgr.is_err(), "Manager must fail programmatic post");
    assert_eq!(res_mgr.unwrap_err().status_code(), StatusCode::FORBIDDEN);

    // Staff fails with Forbidden
    let ctx_staff = TenantContext {
        tenant_id,
        actor_id: Uuid::new_v4(),
        role: Role::Staff,
    };
    let res_staff = harness.accounting_service.post_journal_command(&ctx_staff, cmd.clone()).await;
    assert!(res_staff.is_err(), "Staff must fail programmatic post");
    assert_eq!(res_staff.unwrap_err().status_code(), StatusCode::FORBIDDEN);

    // Custom/Viewer fails with Forbidden
    let ctx_viewer = TenantContext {
        tenant_id,
        actor_id: Uuid::new_v4(),
        role: Role::Custom("viewer".to_string()),
    };
    let res_viewer = harness.accounting_service.post_journal_command(&ctx_viewer, cmd).await;
    assert!(res_viewer.is_err(), "Viewer must fail programmatic post");
    assert_eq!(res_viewer.unwrap_err().status_code(), StatusCode::FORBIDDEN);
}

// ============================================================================
// 3. ADVERSARIAL EDGE CASES & CROSS-TENANT ISOLATION
// ============================================================================

#[tokio::test]
async fn challenge_cross_tenant_reversal_strictly_returns_404() {
    let harness = setup_challenger_harness().await;

    // 1. Post entry in Tenant A by Owner A
    let (s_post, b_post) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Tenant A Secret Journal",
            "lines": [
                {"account_code": "1000", "debit": 999_000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 999_000}
            ]
        })),
    )
    .await;
    assert_eq!(s_post, StatusCode::CREATED);
    let tenant_a_journal_id = b_post["id"].as_str().unwrap().to_string();

    // 2. Tenant B's Accountant attempts to reverse Tenant A's journal
    // Must return 404 NOT_FOUND (anti-enumeration: Tenant B does not even know it exists)
    let (s_rev, b_rev) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", tenant_a_journal_id),
        &harness.tenant_b_accountant_token,
        Some(&harness.tenant_b_id),
        Some(json!({"reason": "Cross-tenant malicious reversal"})),
    )
    .await;

    assert_eq!(
        s_rev,
        StatusCode::NOT_FOUND,
        "Cross-tenant reversal must return 404 Not Found to prevent ID enumeration"
    );
    assert_eq!(b_rev["code"], "NOT_FOUND");
}

#[tokio::test]
async fn challenge_database_triggers_prevent_direct_line_mutation_and_deletion() {
    let harness = setup_challenger_harness().await;
    let pool = &harness.pool;

    let (s_post, b_post) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Trigger Verification Journal",
            "lines": [
                {"account_code": "1000", "debit": 500_000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 500_000}
            ]
        })),
    )
    .await;
    assert_eq!(s_post, StatusCode::CREATED);
    let j_id = b_post["id"].as_str().unwrap().to_string();
    let l_id = b_post["lines"][0]["id"].as_str().unwrap().to_string();

    // 1. Direct SQL UPDATE on posted line must be aborted by trigger
    let upd_res = sqlx::query("UPDATE journal_lines SET debit = 999999 WHERE id = ?1")
        .bind(&l_id)
        .execute(pool)
        .await;
    assert!(upd_res.is_err(), "Trigger failed! Direct update on posted line succeeded");

    // 2. Direct SQL DELETE on posted line must be aborted by trigger
    let del_res = sqlx::query("DELETE FROM journal_lines WHERE id = ?1")
        .bind(&l_id)
        .execute(pool)
        .await;
    assert!(del_res.is_err(), "Trigger failed! Direct delete on posted line succeeded");

    // 3. Direct SQL DELETE on posted journal must be aborted by trigger
    let j_del_res = sqlx::query("DELETE FROM journal_entries WHERE id = ?1")
        .bind(&j_id)
        .execute(pool)
        .await;
    assert!(j_del_res.is_err(), "Trigger failed! Direct delete on posted journal succeeded");

    // 4. Direct SQL status downgrade to DRAFT must be aborted by trigger
    let downgrade_res = sqlx::query("UPDATE journal_entries SET status = 'DRAFT' WHERE id = ?1")
        .bind(&j_id)
        .execute(pool)
        .await;
    assert!(downgrade_res.is_err(), "Trigger failed! Status downgrade to DRAFT succeeded");
}


// ============================================================================
// 4. CHART OF ACCOUNTS RBAC & MULTI-JOURNAL CONCURRENCY STRESS
// ============================================================================

#[tokio::test]
async fn challenge_chart_of_accounts_rbac_matrix() {
    let harness = setup_challenger_harness().await;

    let test_account = json!({
        "code": "1099",
        "name": "Petty Cash Test",
        "account_type": "ASSET",
        "normal_balance": "DEBIT",
        "description": "Test Petty Cash"
    });

    // 1. Viewer cannot create account (403)
    let (s_v, b_v) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.viewer_token,
        Some(&harness.tenant_a_id),
        Some(test_account.clone()),
    )
    .await;
    assert_eq!(s_v, StatusCode::FORBIDDEN);
    assert_eq!(b_v["code"], "FORBIDDEN");

    // 2. Staff cannot create account (403)
    let (s_s, b_s) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.staff_token,
        Some(&harness.tenant_a_id),
        Some(test_account.clone()),
    )
    .await;
    assert_eq!(s_s, StatusCode::FORBIDDEN);
    assert_eq!(b_s["code"], "FORBIDDEN");

    // 3. Manager cannot create account (403)
    let (s_m, b_m) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.manager_token,
        Some(&harness.tenant_a_id),
        Some(test_account.clone()),
    )
    .await;
    assert_eq!(s_m, StatusCode::FORBIDDEN);
    assert_eq!(b_m["code"], "FORBIDDEN");

    // 4. Accountant CAN create account (201)
    let (s_a, b_a) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.accountant_token,
        Some(&harness.tenant_a_id),
        Some(test_account.clone()),
    )
    .await;
    assert_eq!(s_a, StatusCode::CREATED);
    assert_eq!(b_a["code"], "1099");

    // 5. Staff cannot delete account (403)
    let (s_sd, b_sd) = send_request(
        &harness.app,
        Method::DELETE,
        "/api/v1/accounting/accounts/1099",
        &harness.staff_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(s_sd, StatusCode::FORBIDDEN);
    assert_eq!(b_sd["code"], "FORBIDDEN");

    // 6. Administrator CAN delete custom account (200)
    let (s_ad, b_ad) = send_request(
        &harness.app,
        Method::DELETE,
        "/api/v1/accounting/accounts/1099",
        &harness.admin_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(s_ad, StatusCode::OK);
    assert_eq!(b_ad["status"], "deleted");
}

#[tokio::test]
async fn challenge_concurrent_races_across_multiple_journals_simultaneously() {
    let harness = setup_challenger_harness().await;

    // 1. Post 5 independent journals in Tenant A
    let mut journal_ids = Vec::new();
    for i in 0..5 {
        let (post_s, post_b) = send_request(
            &harness.app,
            Method::POST,
            "/api/v1/accounting/journals",
            &harness.owner_token,
            Some(&harness.tenant_a_id),
            Some(json!({
                "description": format!("Multi-Journal Race Target #{}", i),
                "lines": [
                    {"account_code": "1000", "debit": 1_000_000 + i * 100, "credit": 0},
                    {"account_code": "4000", "debit": 0, "credit": 1_000_000 + i * 100}
                ]
            })),
        )
        .await;

        assert_eq!(post_s, StatusCode::CREATED);
        journal_ids.push(post_b["id"].as_str().unwrap().to_string());
    }

    // 2. For each of the 5 journals, spawn 10 concurrent tasks simultaneously
    // Total = 50 concurrent requests competing across 5 journal targets
    let mut join_set = tokio::task::JoinSet::new();

    for (target_idx, j_id) in journal_ids.iter().enumerate() {
        for attempt_idx in 0..10 {
            let app = harness.app.clone();
            let token = harness.owner_token.clone();
            let tenant_id = harness.tenant_a_id.clone();
            let journal_id = j_id.clone();

            join_set.spawn(async move {
                let (status, body) = send_request(
                    &app,
                    Method::POST,
                    &format!("/api/v1/accounting/journals/{}/reverse", journal_id),
                    &token,
                    Some(&tenant_id),
                    Some(json!({
                        "reason": format!("Multi-target race Target {} Attempt {}", target_idx, attempt_idx)
                    })),
                )
                .await;

                (target_idx, status, body)
            });
        }
    }

    let mut results_by_target = [(0, 0, 0); 5]; // (201s, 409s, 500s)

    while let Some(res) = join_set.join_next().await {
        let (target_idx, status, body) = res.expect("Task failed");
        match status {
            StatusCode::CREATED => {
                results_by_target[target_idx].0 += 1;
            }
            StatusCode::CONFLICT => {
                assert_eq!(body["code"], "ALREADY_REVERSED");
                results_by_target[target_idx].1 += 1;
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                results_by_target[target_idx].2 += 1;
                eprintln!("Unexpected 500 on target {}: {:?}", target_idx, body);
            }
            other => panic!("Unexpected HTTP status {} on target {}", other, target_idx),
        }
    }

    // Verify invariants for every target journal
    for (i, (created, conflicts, errors)) in results_by_target.iter().enumerate() {
        assert_eq!(*created, 1, "Target {} must have exactly 1 winner", i);
        assert_eq!(*conflicts, 9, "Target {} must have exactly 9 conflicts", i);
        assert_eq!(*errors, 0, "Target {} must have ZERO 500 errors", i);
    }

    // Verify final trial balance remains perfectly balanced to net 0
    let (tb_s, tb_b) = send_request(
        &harness.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb_s, StatusCode::OK);
    assert_eq!(tb_b["is_balanced"], true);
    assert_eq!(tb_b["net_balance"], 0);
}
