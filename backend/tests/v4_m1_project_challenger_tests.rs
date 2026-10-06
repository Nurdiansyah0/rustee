//! Adversarial Empirical Challenger Integration Test Suite for Milestone 1:
//! Project & Milestone Domain Architecture & Migrations (Features 1-10, 33).
//!
//! Subagent: challenger_m1_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Mandatory empirical challenge criteria:
//! 1. Gapless sequential project numbering (PRJ-YYYY-XXXXXX): Concurrency stress testing
//!    verifying consecutive numbering with zero collisions and zero gaps.
//! 2. Multi-tenant independence: Independent tenants starting at 000001 with zero cross-tenant bleeding.
//! 3. Transaction rollback resilience: Aborted / failed transactions do not leak or corrupt sequence numbers.
//! 4. Cross-tenant anti-enumeration: Strict HTTP 404 across all endpoints and anti-enumeration equivalence.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::tenant::{Role, TenantContext};
use backend::repository::{
    account_repo::SqlxAccountRepository,
    audit_repo::SqlxAuditRepository,
    category_repo::SqlxCategoryRepository,
    db::{init_pool, run_migrations, DbConfig},
    idempotency_repo::SqlxIdempotencyRepository,
    project_repo::{ProjectRepository, SqlxProjectRepository},
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
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    #[allow(dead_code)]
    jwt_engine: Arc<JwtEngine>,
    owner_a_token: String,
    #[allow(dead_code)]
    manager_a_token: String,
    #[allow(dead_code)]
    staff_a_token: String,
    tenant_a_id: String,
    owner_a_id: String,
    staff_a_id: String,
    owner_b_token: String,
    #[allow(dead_code)]
    manager_b_token: String,
    staff_b_token: String,
    tenant_b_id: String,
    owner_b_id: String,
    owner_c_token: String,
    tenant_c_id: String,
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m1_test.sqlite");
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
    let tenant_repo = Arc::new(SqlxTenantRepository::new(pool.clone()));
    let membership_repo = Arc::new(SqlxMembershipRepository::new(pool.clone()));

    let jwt_secret = "challenger_m1_test_secret_key_1234567890_super_secret";
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
        tenant_repo: tenant_repo.clone(),
        pool: pool.clone(),
        rate_limiter: Arc::default(),
    };

    let app = create_app(state);

    // --- Helper for creating user and token ---
    async fn create_test_user(
        user_repo: &SqlxUserRepository,
        jwt_engine: &JwtEngine,
        email: &str,
        name: &str,
    ) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        user_repo
            .create(&NewUser {
                id: user_id.clone(),
                email: email.to_string(),
                password_hash: "hash".to_string(),
                display_name: name.to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some("premium".to_string()),
            })
            .await
            .unwrap();

        let (token, _) = jwt_engine
            .generate_token(&user_id, email, "user", "premium")
            .unwrap();
        (user_id, token)
    }

    // Tenant Alpha Users
    let (owner_a_id, owner_a_token) =
        create_test_user(&user_repo, &jwt_engine, "owner_alpha@inv.com", "Owner Alpha").await;
    let (manager_a_id, manager_a_token) =
        create_test_user(&user_repo, &jwt_engine, "manager_alpha@inv.com", "Manager Alpha").await;
    let (staff_a_id, staff_a_token) =
        create_test_user(&user_repo, &jwt_engine, "staff_alpha@inv.com", "Staff Alpha").await;

    // Tenant Beta Users
    let (owner_b_id, owner_b_token) =
        create_test_user(&user_repo, &jwt_engine, "owner_beta@inv.com", "Owner Beta").await;
    let (manager_b_id, manager_b_token) =
        create_test_user(&user_repo, &jwt_engine, "manager_beta@inv.com", "Manager Beta").await;
    let (_staff_b_id, staff_b_token) =
        create_test_user(&user_repo, &jwt_engine, "staff_beta@inv.com", "Staff Beta").await;

    // Tenant Gamma Users
    let (owner_c_id, owner_c_token) =
        create_test_user(&user_repo, &jwt_engine, "owner_gamma@inv.com", "Owner Gamma").await;

    // Seed Tenant Alpha
    let tenant_a_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Builders".to_string(),
            slug: "tenant-alpha-builders".to_string(),
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
            user_id: owner_a_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: manager_a_id.clone(),
            role: Role::Manager,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: staff_a_id.clone(),
            role: Role::Staff,
        })
        .await
        .unwrap();

    // Seed Tenant Beta
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
            user_id: owner_b_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: manager_b_id.clone(),
            role: Role::Manager,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: _staff_b_id.clone(),
            role: Role::Staff,
        })
        .await
        .unwrap();

    // Seed Tenant Gamma
    let tenant_c_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_c_id.clone(),
            name: "Tenant Gamma Infrastructure".to_string(),
            slug: "tenant-gamma-infrastructure".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_c = pool.begin().await.unwrap();
    backend::repository::accounting_repo::SqlxAccountingRepository::seed_default_accounts_tx(
        &mut tx_c,
        &tenant_c_id,
    )
    .await
    .unwrap();
    tx_c.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_c_id.clone(),
            user_id: owner_c_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        jwt_engine,
        owner_a_token,
        manager_a_token,
        staff_a_token,
        tenant_a_id,
        owner_a_id,
        staff_a_id,
        owner_b_token,
        manager_b_token,
        staff_b_token,
        tenant_b_id,
        owner_b_id,
        owner_c_token,
        tenant_c_id,
    }
}

async fn req(
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
// 1. GAPLESS SEQUENTIAL NUMBERING CONCURRENCY STRESS HARNESS
// ============================================================================

#[tokio::test]
async fn challenge_concurrent_gapless_sequential_project_numbering() {
    let h = setup_challenger_harness().await;
    let current_year = chrono::Utc::now().year();
    let concurrency = 25;

    // Spawn 25 concurrent requests firing simultaneously to POST /api/v1/projects
    let mut tasks = Vec::with_capacity(concurrency);
    for i in 0..concurrency {
        let app = h.app.clone();
        let token = h.owner_a_token.clone();
        let tenant_id = h.tenant_a_id.clone();
        tasks.push(tokio::spawn(async move {
            req(
                &app,
                Method::POST,
                "/api/v1/projects",
                &token,
                &tenant_id,
                Some(json!({
                    "name": format!("Concurrent High-Load Tower #{}", i + 1),
                    "budget_amount": 100_000_000,
                    "contract_amount": 125_000_000
                })),
            )
            .await
        }));
    }

    let mut results = Vec::with_capacity(concurrency);
    for task in tasks {
        results.push(task.await.expect("Task panicked"));
    }

    let mut project_numbers = Vec::new();

    for (idx, (status, body, _)) in results.into_iter().enumerate() {
        assert_eq!(
            status,
            StatusCode::CREATED,
            "Concurrent request #{} failed with body: {:?}",
            idx + 1,
            body
        );
        let project_number = body["project_number"]
            .as_str()
            .expect("Expected project_number in response")
            .to_string();
        project_numbers.push(project_number);
    }

    // Invariant 1: Total successful projects == concurrency
    assert_eq!(
        project_numbers.len(),
        concurrency,
        "All concurrent project creations must succeed"
    );

    // Invariant 2: Zero collisions (all project numbers must be strictly unique)
    let unique_set: HashSet<String> = project_numbers.iter().cloned().collect();
    assert_eq!(
        unique_set.len(),
        concurrency,
        "COLLISION DETECTED! Unique project numbers ({}) < total created ({})",
        unique_set.len(),
        concurrency
    );

    // Invariant 3: Gapless consecutive sequence ordering (1..=25)
    let prefix = format!("PRJ-{}-", current_year);
    let mut sequences: Vec<i64> = project_numbers
        .iter()
        .map(|num| {
            assert!(
                num.starts_with(&prefix),
                "Project number '{}' does not start with expected prefix '{}'",
                num,
                prefix
            );
            let seq_str = &num[prefix.len()..];
            assert_eq!(
                seq_str.len(),
                6,
                "Sequence number '{}' must be exactly 6 digits",
                seq_str
            );
            seq_str
                .parse::<i64>()
                .expect("Sequence part must be valid integer")
        })
        .collect();

    sequences.sort();

    for (expected_idx, actual_seq) in (1..=(concurrency as i64)).zip(sequences.iter()) {
        assert_eq!(
            expected_idx, *actual_seq,
            "GAP DETECTED! Expected sequence #{}, but found sequence #{}",
            expected_idx, actual_seq
        );
    }

    // Invariant 4: Continued sequential gapless creation
    // Creating 5 more projects sequentially must continue from 26 to 30
    for expected_seq in (concurrency as i64 + 1)..=(concurrency as i64 + 5) {
        let (status, body, _) = req(
            &h.app,
            Method::POST,
            "/api/v1/projects",
            &h.owner_a_token,
            &h.tenant_a_id,
            Some(json!({
                "name": format!("Post-Concurrency Sequential Project #{}", expected_seq)
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let expected_number = format!("{}{:06}", prefix, expected_seq);
        assert_eq!(
            body["project_number"].as_str().unwrap(),
            expected_number,
            "Post-concurrency project numbering did not follow gaplessly"
        );
    }
}

// ============================================================================
// 2. MULTI-TENANT INDEPENDENCE UNDER MASSIVE INTERLEAVED CONCURRENCY
// ============================================================================

#[tokio::test]
async fn challenge_multi_tenant_gapless_independence() {
    let h = setup_challenger_harness().await;
    let current_year = chrono::Utc::now().year();
    let per_tenant_count = 10;

    // Launch 30 requests concurrently: 10 for Alpha, 10 for Beta, 10 for Gamma
    let mut tasks = Vec::new();

    // Interleave tenant requests in a round-robin schedule
    for i in 0..per_tenant_count {
        // Tenant Alpha
        {
            let app = h.app.clone();
            let token = h.owner_a_token.clone();
            let tenant_id = h.tenant_a_id.clone();
            tasks.push(tokio::spawn(async move {
                let res = req(
                    &app,
                    Method::POST,
                    "/api/v1/projects",
                    &token,
                    &tenant_id,
                    Some(json!({ "name": format!("Alpha Project #{}", i + 1) })),
                )
                .await;
                ("ALPHA", res)
            }));
        }

        // Tenant Beta
        {
            let app = h.app.clone();
            let token = h.owner_b_token.clone();
            let tenant_id = h.tenant_b_id.clone();
            tasks.push(tokio::spawn(async move {
                let res = req(
                    &app,
                    Method::POST,
                    "/api/v1/projects",
                    &token,
                    &tenant_id,
                    Some(json!({ "name": format!("Beta Project #{}", i + 1) })),
                )
                .await;
                ("BETA", res)
            }));
        }

        // Tenant Gamma
        {
            let app = h.app.clone();
            let token = h.owner_c_token.clone();
            let tenant_id = h.tenant_c_id.clone();
            tasks.push(tokio::spawn(async move {
                let res = req(
                    &app,
                    Method::POST,
                    "/api/v1/projects",
                    &token,
                    &tenant_id,
                    Some(json!({ "name": format!("Gamma Project #{}", i + 1) })),
                )
                .await;
                ("GAMMA", res)
            }));
        }
    }

    let mut alpha_numbers = Vec::new();
    let mut beta_numbers = Vec::new();
    let mut gamma_numbers = Vec::new();

    for task in tasks {
        let (tenant_tag, (status, body, _)) = task.await.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        let num = body["project_number"].as_str().unwrap().to_string();
        match tenant_tag {
            "ALPHA" => alpha_numbers.push(num),
            "BETA" => beta_numbers.push(num),
            "GAMMA" => gamma_numbers.push(num),
            _ => unreachable!(),
        }
    }

    let prefix = format!("PRJ-{}-", current_year);

    fn verify_tenant_sequences(numbers: &[String], prefix: &str, expected_count: usize, label: &str) {
        assert_eq!(numbers.len(), expected_count);
        let mut seqs: Vec<i64> = numbers
            .iter()
            .map(|n| {
                assert!(n.starts_with(prefix));
                n[prefix.len()..].parse::<i64>().unwrap()
            })
            .collect();
        seqs.sort();
        for (idx, val) in (1..=(expected_count as i64)).zip(seqs.iter()) {
            assert_eq!(
                idx, *val,
                "{}: Sequence mismatch! Expected {}, got {}",
                label, idx, val
            );
        }
    }

    // Invariant: Each tenant starts at sequence 000001 and ends at 000010 without interference
    verify_tenant_sequences(&alpha_numbers, &prefix, per_tenant_count, "Tenant Alpha");
    verify_tenant_sequences(&beta_numbers, &prefix, per_tenant_count, "Tenant Beta");
    verify_tenant_sequences(&gamma_numbers, &prefix, per_tenant_count, "Tenant Gamma");
}

// ============================================================================
// 3. TRANSACTION ROLLBACK RESILIENCE & GAPLESS SEQUENCE PRESERVATION
// ============================================================================

#[tokio::test]
async fn challenge_transaction_rollback_resilience_and_gapless_preservation() {
    let h = setup_challenger_harness().await;
    let repo = SqlxProjectRepository::new(h.pool.clone());
    let ctx = TenantContext::new(
        Uuid::parse_str(&h.tenant_a_id).unwrap(),
        Uuid::parse_str(&h.owner_a_id).unwrap(),
        Role::Owner,
    );
    let current_year = chrono::Utc::now().year();
    let prefix = format!("PRJ-{}-", current_year);

    // 1. Create first project successfully via API
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Initial Baseline Project" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["project_number"].as_str().unwrap(), format!("{}000001", prefix));

    // 2. Abort a transaction explicitly after number generation
    {
        let mut tx = h.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let generated_num = repo.generate_project_number_tx(&mut tx, &ctx).await.unwrap();
        assert_eq!(
            generated_num,
            format!("{}000002", prefix),
            "Sequence generator within transaction should suggest 000002"
        );
        // Explicitly rollback without committing
        tx.rollback().await.unwrap();
    }

    // 3. Verify subsequent project creation receives sequence 000002 (no gap/leak!)
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Project After Explicit Rollback" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        body["project_number"].as_str().unwrap(),
        format!("{}000002", prefix),
        "Sequence must NOT leak after explicit transaction rollback!"
    );

    // 4. Abort a transaction due to database constraint violation
    {
        let mut tx = h.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let _generated = repo.generate_project_number_tx(&mut tx, &ctx).await.unwrap();

        // Intentionally execute invalid SQL violating check constraint
        let bad_insert = sqlx::query(
            "INSERT INTO projects (id, tenant_id, project_number, name, customer_name, budget_amount, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&h.tenant_a_id)
        .bind(format!("{}000003", prefix))
        .bind("Illegal Negative Budget Project")
        .bind("Customer")
        .bind(-99999_i64) // Violates CHECK (budget_amount >= 0)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await;

        assert!(bad_insert.is_err(), "Constraint violation must fail insert");
        // Rollback the failed transaction
        tx.rollback().await.unwrap();
    }

    // 5. Verify subsequent project creation receives sequence 000003 without gaps
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Project After Constraint Failure Rollback" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        body["project_number"].as_str().unwrap(),
        format!("{}000003", prefix),
        "Sequence must remain gapless after database constraint abort"
    );

    // 6. Stress test: Intermixed aborted transactions and committed transactions
    for expected_seq in 4..=10 {
        // Rollback attempt
        {
            let mut tx = h.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
            let _ = repo.generate_project_number_tx(&mut tx, &ctx).await.unwrap();
            tx.rollback().await.unwrap();
        }

        // Real creation
        let (status, body, _) = req(
            &h.app,
            Method::POST,
            "/api/v1/projects",
            &h.owner_a_token,
            &h.tenant_a_id,
            Some(json!({ "name": format!("Intermixed Project #{}", expected_seq) })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(
            body["project_number"].as_str().unwrap(),
            format!("{}{:06}", prefix, expected_seq),
            "Sequence drift detected during intermixed rollbacks at sequence {}",
            expected_seq
        );
    }
}

// ============================================================================
// 4. CROSS-TENANT ANTI-ENUMERATION: STRICT HTTP 404 ACROSS ALL ENDPOINTS
// ============================================================================

#[tokio::test]
async fn challenge_cross_tenant_anti_enumeration_strict_404_all_endpoints() {
    let h = setup_challenger_harness().await;

    // --- Seed Alpha Entities ---
    // 1. Alpha Project 1
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Alpha Tower Secret Facility",
            "budget_amount": 500_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let alpha_project_id = body["id"].as_str().unwrap().to_string();

    // 1b. Alpha Project 2 (Sequence PRJ-YYYY-000002, which Tenant Beta does NOT have)
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Alpha Second Secret Underground Lab",
            "budget_amount": 750_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let alpha_project_number_2 = body["project_number"].as_str().unwrap().to_string();

    // 2. Alpha Member (Staff A)
    let (status, _body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", alpha_project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_a_id,
            "role": "SITE_ENGINEER",
            "cost_rate": 50_000,
            "billing_rate": 100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 3. Alpha Milestone
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", alpha_project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Alpha Phase 1 Foundation",
            "target_date": "2026-11-30",
            "billable_amount": 150_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let alpha_milestone_id = body["id"].as_str().unwrap().to_string();

    // 4. Alpha Task
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", alpha_project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "title": "Alpha Excavation Work",
            "milestone_id": alpha_milestone_id,
            "estimated_hours": 80
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let alpha_task_id = body["id"].as_str().unwrap().to_string();

    // --- Seed Beta Entities (Legitimate) ---
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "name": "Beta Bridge Construction"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let beta_project_id = body["id"].as_str().unwrap().to_string();

    // --- Adversarial Challenge: Tenant Beta probing Tenant Alpha entities ---

    // Attack 1: GET /projects/{alpha_id} from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Direct cross-tenant project GET must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 2: PATCH /projects/{alpha_id}/status from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant project status update must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 3: GET /projects/{alpha_id}/members from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/members", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant members GET must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 4: POST /projects/{alpha_id}/members from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "user_id": h.owner_b_id,
            "role": "PROJECT_MANAGER"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant member POST must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 5: GET /projects/{alpha_id}/milestones from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/milestones", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant milestones GET must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 6: POST /projects/{alpha_id}/milestones from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "title": "Injected Foreign Milestone",
            "target_date": "2026-12-31"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant milestone POST must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 7: PATCH /projects/{alpha_id}/milestones/{alpha_m_id}/status from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/milestones/{}/status", alpha_project_id, alpha_milestone_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant milestone PATCH must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 8: GET /projects/{alpha_id}/tasks from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/tasks", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant tasks GET must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 9: POST /projects/{alpha_id}/tasks from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", alpha_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "title": "Injected Foreign Task" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant task POST must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 10: PATCH /projects/{alpha_id}/tasks/{alpha_t_id}/status from Tenant Beta
    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/tasks/{}/status", alpha_project_id, alpha_task_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant task PATCH must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 11: Cross-entity mixing: Valid Beta project, but foreign Alpha milestone
    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/milestones/{}/status", beta_project_id, alpha_milestone_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-project foreign milestone PATCH must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 12: Cross-entity mixing: Valid Beta project, referencing foreign Alpha milestone in task creation
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", beta_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "title": "Task Referencing Foreign Milestone",
            "milestone_id": alpha_milestone_id
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Referencing foreign milestone in task must return 404");
    assert_eq!(body["code"], "NOT_FOUND");

    // Attack 13: Cross-entity mixing: Valid Beta project, referencing foreign Alpha user in member assignment
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", beta_project_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "user_id": h.staff_a_id, // Belongs strictly to Tenant Alpha
            "role": "WORKER"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Assigning foreign tenant user must return 404");
    assert_eq!(body["code"], "USER_NOT_FOUND");

    // Attack 14: Search enumeration probing: Probing foreign project_number via query parameter
    // Probing alpha_project_number_2 (which exists only in Tenant Alpha)
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects?search={}", alpha_project_number_2),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["count"].as_u64().unwrap(),
        0,
        "Search filter must NEVER reveal projects from foreign tenants"
    );
    assert_eq!(body["projects"].as_array().unwrap().len(), 0);

    // Attack 15: Search enumeration probing: Probing foreign project name
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        "/api/v1/projects?search=Alpha+Tower",
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["count"].as_u64().unwrap(),
        0,
        "Name search must NEVER leak foreign projects"
    );

    // Attack 16: Ensure any returned project strictly matches caller's tenant
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        "/api/v1/projects",
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["id"].as_str().unwrap(), beta_project_id);
    assert_eq!(projects[0]["tenant_id"].as_str().unwrap(), h.tenant_b_id);
}

// ============================================================================
// 5. ANTI-ENUMERATION ORACLE EQUIVALENCE
// ============================================================================

#[tokio::test]
async fn challenge_anti_enumeration_oracle_equivalence() {
    let h = setup_challenger_harness().await;

    // Create a real project in Tenant Alpha
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Existing Target Project Alpha" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let real_foreign_id = body["id"].as_str().unwrap().to_string();

    // Generate a random, completely non-existent UUID
    let non_existent_id = Uuid::new_v4().to_string();

    // Query 1: Probe existing foreign entity from Tenant Beta
    let (status_real, body_real, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}", real_foreign_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;

    // Query 2: Probe non-existent UUID from Tenant Beta
    let (status_fake, body_fake, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}", non_existent_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;

    // Invariant: Both responses must be empirically equivalent
    assert_eq!(status_real, StatusCode::NOT_FOUND);
    assert_eq!(status_fake, StatusCode::NOT_FOUND);

    assert_eq!(body_real["code"], "NOT_FOUND");
    assert_eq!(body_fake["code"], "NOT_FOUND");

    assert_eq!(body_real["status"], 404);
    assert_eq!(body_fake["status"], 404);

    assert_eq!(body_real["type"], body_fake["type"]);
    assert_eq!(body_real["title"], body_fake["title"]);
    assert_eq!(body_real["title"], "Not Found");
}

// ============================================================================
// 6. ANTI-ENUMERATION PRECEDENCE OVER RBAC ACROSS TENANT BOUNDARIES
// ============================================================================

#[tokio::test]
async fn challenge_anti_enumeration_precedence_over_rbac() {
    let h = setup_challenger_harness().await;

    // 1. Seed Alpha project and task
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Alpha Sensitive Installation" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let alpha_proj_id = body["id"].as_str().unwrap().to_string();

    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/tasks", alpha_proj_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "title": "Alpha Sensitive Task" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let alpha_task_id = body["id"].as_str().unwrap().to_string();

    // 2. Staff in Tenant Beta probes Alpha Project (Staff role has read rights inside own tenant)
    // When probing foreign tenant, must strictly return 404 (NOT 200 or 403)
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}", alpha_proj_id),
        &h.staff_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Foreign GET by Staff must return 404 Not Found"
    );
    assert_eq!(body["code"], "NOT_FOUND");

    // 3. Staff in Tenant Beta updates Alpha Task (Staff is authorized to update task status in own tenant)
    // When updating foreign task, must strictly return 404 (NOT 200 or 403)
    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/tasks/{}/status", alpha_proj_id, alpha_task_id),
        &h.staff_b_token,
        &h.tenant_b_id,
        Some(json!({ "status": "IN_PROGRESS" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Foreign task PATCH by Staff must return 404 Not Found"
    );
    assert_eq!(body["code"], "NOT_FOUND");

    // 4. Contrast: Staff in Tenant Beta creates project in Tenant Beta (tenant-internal unauthorized action)
    // Must return 403 Forbidden!
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.staff_b_token,
        &h.tenant_b_id,
        Some(json!({ "name": "Illegal Staff Project" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "Tenant-internal unauthorized creation must return 403 Forbidden"
    );
    assert_eq!(body["code"], "FORBIDDEN");
}

// ============================================================================
// 7. ADVERSARIAL VALIDATION, FUZZING & ILLEGAL STATE TRANSITIONS
// ============================================================================

#[tokio::test]
async fn challenge_adversarial_validation_and_fuzzing() {
    let h = setup_challenger_harness().await;

    // 1. Negative Budget Amount
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Negative Budget Construction",
            "budget_amount": -100_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");

    // 2. Negative Contract Amount
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Negative Contract Construction",
            "contract_amount": -50_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");

    // 3. Whitespace / Empty Project Name
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "    \t \n   "
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_NAME");

    // 4. Inverted Dates (end_date < start_date)
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Time-Travel Project",
            "start_date": "2026-12-31",
            "end_date": "2026-01-01"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_DATES");

    // 5. Valid Project for Lifecycle and Member Fuzzing
    let (status, body, _) = req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "State Machine Robustness Project"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = body["id"].as_str().unwrap().to_string();

    // 6. Illegal State Jump: DRAFT -> COMPLETED directly (must fail)
    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "status": "COMPLETED" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");

    // 7. Transition to ACTIVE, then COMPLETED
    let (status, _, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "status": "COMPLETED" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["actual_completion_date"].is_string());

    // 8. Illegal State Jump: COMPLETED -> ACTIVE (terminal state violation)
    let (status, body, _) = req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "status": "ACTIVE" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INVALID_STATUS_TRANSITION");

    // 9. Assign Member twice (Conflict 409 guard)
    let (status, _, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_a_id,
            "role": "WORKER"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body, _) = req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/members", project_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "user_id": h.staff_a_id,
            "role": "WORKER"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "MEMBER_ALREADY_ASSIGNED");

    // 10. Malformed ID in URL path (SQL injection & path fuzzing)
    let (status, body, _) = req(
        &h.app,
        Method::GET,
        "/api/v1/projects/'%20OR%20'1'='1",
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}
