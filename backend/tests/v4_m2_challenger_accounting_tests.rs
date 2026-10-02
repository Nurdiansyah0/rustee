//! Empirical Adversarial Verification Test Suite for Milestone 2 (M2)
//!
//! Identity: teamwork_preview_challenger_m2_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Scope of Empirical Testing:
//! 1. Tax Golden Vectors & Boundary Rounding:
//!    - PPN 11% inclusive & exclusive across diverse amounts (Rp 1, Rp 5, Rp 105, Rp 1,000,000, etc.)
//!    - PPN 12% inclusive & exclusive
//!    - UMKM 0.5% (Rp 99 -> 0, Rp 100 -> 1, Rp 50,000,000 -> 250,000, Rp 4.8B -> 24,000,000)
//!    - Verify `net + tax == gross` invariant in all cases.
//! 2. Reversal Lifecycle & Concurrency:
//!    - Verify compensating journal swaps debits and credits exactly.
//!    - Verify trial balance after posting + reversal results in net 0 impact.
//!    - Concurrency: race two simultaneous reversal requests on the same journal -> exactly one must succeed with 201, the other must fail with 409 `ALREADY_REVERSED`.
//! 3. Cross-Tenant Attack Simulation:
//!    - Attempt to access or reverse Tenant A's journal from Tenant B's context -> must return 404 Not Found.
//!    - Attempt to post journal using Tenant B's account code in Tenant A's tenant -> must reject.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
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
struct TestHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    jwt_engine: Arc<JwtEngine>,
    owner_token: String,
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m2_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 10,
        min_connections: 1,
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

    let jwt_secret = "challenger_m2_jwt_secret_key_1234567890_super_secret";
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

    let payment_service = Arc::new(PaymentService::new(
        PaymentConfig::default(),
        subscription_repo,
        user_repo.clone(),
        audit_repo,
    ));

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

    // 1. Create Owner User A
    let owner_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_id.clone(),
            email: "owner_a@challenger.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_a@challenger.com", "user", "premium")
        .unwrap();

    // 2. Create Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_a@challenger.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_a@challenger.com", "user", "free")
        .unwrap();

    // 3. Create Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
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

    // 4. Create Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "user_b@challenger.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "User B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "user_b@challenger.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
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

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: user_b_id,
            role: Role::Owner,
        })
        .await
        .unwrap();

    TestHarness {
        app,
        pool,
        _dir: dir,
        jwt_engine,
        owner_token,
        staff_token,
        tenant_a_id,
        tenant_b_id,
        tenant_b_token,
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
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header(CONTENT_TYPE, "application/json");

    if let Some(t_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", t_id);
    }

    let req_body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };

    let req = builder.body(req_body).unwrap();
    let resp = app.clone().oneshot(req).await.expect("Request failed");
    let status = resp.status();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json_val = serde_json::from_slice::<Value>(&body_bytes).unwrap_or(Value::Null);

    (status, json_val)
}

// ============================================================================
// 1. Tax Golden Vectors & Boundary Rounding
// ============================================================================

#[tokio::test]
async fn test_empirical_ppn_11_golden_vectors_and_boundary_rounding() {
    let harness = setup_harness().await;

    // Table of (amount, is_inclusive, expected_tax, expected_net, expected_gross)
    let cases = vec![
        // PPN 11% Exclusive:
        (0i64, false, 0i64, 0i64, 0i64),
        (1, false, 0, 1, 1),           // 1 * 11 / 100 = 0.11 -> 0
        (4, false, 0, 4, 4),           // 4 * 11 / 100 = 0.44 -> 0
        (5, false, 1, 5, 6),           // 5 * 11 / 100 = 0.55 -> 1
        (9, false, 1, 9, 10),          // 9 * 11 / 100 = 0.99 -> 1
        (10, false, 1, 10, 11),        // 10 * 11 / 100 = 1.10 -> 1
        (50, false, 6, 50, 56),        // 50 * 11 / 100 = 5.50 -> 6 (exact half rounds up)
        (105, false, 12, 105, 117),    // 105 * 11 / 100 = 11.55 -> 12
        (100_000, false, 11_000, 100_000, 111_000),
        (1_000_000, false, 110_000, 1_000_000, 1_110_000),

        // PPN 11% Inclusive:
        (0, true, 0, 0, 0),
        (1, true, 0, 1, 1),            // 1 * 11 / 111 = 0.099 -> 0
        (5, true, 0, 5, 5),            // 5 * 11 / 111 = 55 / 111 = 0.495... -> 0
        (6, true, 1, 5, 6),            // 6 * 11 / 111 = 66 / 111 = 0.594... -> 1
        (105, true, 10, 95, 105),      // 105 * 11 / 111 = 1155 / 111 = 10.405... -> 10
        (111, true, 11, 100, 111),     // 111 * 11 / 111 = 11
        (111_000, true, 11_000, 100_000, 111_000),
        (1_110_000, true, 110_000, 1_000_000, 1_110_000),
    ];

    for (amt, is_incl, exp_tax, exp_net, exp_gross) in cases {
        let (status, body) = send_request(
            &harness.app,
            Method::POST,
            "/api/v1/accounting/tax/calculate",
            &harness.owner_token,
            Some(&harness.tenant_a_id),
            Some(json!({
                "amount": amt,
                "tax_type": if is_incl { "PPN_11_INCL" } else { "PPN_11_EXCL" },
                "is_inclusive": is_incl
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "Failed for amount {}", amt);
        let tax = body["tax_amount"].as_i64().unwrap();
        let net = body["net_amount"].as_i64().unwrap();
        let gross = body["gross_amount"].as_i64().unwrap();

        assert_eq!(tax, exp_tax, "Tax mismatch for amt={} is_incl={}", amt, is_incl);
        assert_eq!(net, exp_net, "Net mismatch for amt={} is_incl={}", amt, is_incl);
        assert_eq!(gross, exp_gross, "Gross mismatch for amt={} is_incl={}", amt, is_incl);
        assert_eq!(net + tax, gross, "Invariant net + tax == gross violated for amt={}", amt);
    }
}

#[tokio::test]
async fn test_empirical_ppn_12_golden_vectors_and_boundary_rounding() {
    let harness = setup_harness().await;

    let cases = vec![
        // PPN 12% Exclusive:
        (0i64, false, 0i64, 0i64, 0i64),
        (1, false, 0, 1, 1),           // 1 * 12 / 100 = 0.12 -> 0
        (4, false, 0, 4, 4),           // 4 * 12 / 100 = 0.48 -> 0
        (5, false, 1, 5, 6),           // 5 * 12 / 100 = 0.60 -> 1
        (50, false, 6, 50, 56),        // 50 * 12 / 100 = 6.00 -> 6
        (100_000, false, 12_000, 100_000, 112_000),
        (1_000_000, false, 120_000, 1_000_000, 1_120_000),

        // PPN 12% Inclusive:
        (0, true, 0, 0, 0),
        (1, true, 0, 1, 1),            // 1 * 12 / 112 = 12 / 112 = 0.107 -> 0
        (4, true, 0, 4, 4),            // 4 * 12 / 112 = 48 / 112 = 0.428 -> 0
        (5, true, 1, 4, 5),            // 5 * 12 / 112 = 60 / 112 = 0.5357 -> 1 (5 - 1 = 4)
        (112, true, 12, 100, 112),     // 112 * 12 / 112 = 12
        (112_000, true, 12_000, 100_000, 112_000),
        (1_120_000, true, 120_000, 1_000_000, 1_120_000),
    ];

    for (amt, is_incl, exp_tax, exp_net, exp_gross) in cases {
        let (status, body) = send_request(
            &harness.app,
            Method::POST,
            "/api/v1/accounting/tax/calculate",
            &harness.owner_token,
            Some(&harness.tenant_a_id),
            Some(json!({
                "amount": amt,
                "tax_type": if is_incl { "PPN_12_INCL" } else { "PPN_12_EXCL" },
                "is_inclusive": is_incl
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "Failed for amount {}", amt);
        let tax = body["tax_amount"].as_i64().unwrap();
        let net = body["net_amount"].as_i64().unwrap();
        let gross = body["gross_amount"].as_i64().unwrap();

        assert_eq!(tax, exp_tax, "Tax mismatch for amt={} is_incl={}", amt, is_incl);
        assert_eq!(net, exp_net, "Net mismatch for amt={} is_incl={}", amt, is_incl);
        assert_eq!(gross, exp_gross, "Gross mismatch for amt={} is_incl={}", amt, is_incl);
        assert_eq!(net + tax, gross, "Invariant net + tax == gross violated for amt={}", amt);
    }
}

#[tokio::test]
async fn test_empirical_umkm_05_golden_vectors_and_boundary_rounding() {
    let harness = setup_harness().await;

    let cases = vec![
        (0i64, 0i64),
        (99, 0),                       // 99 * 50 / 10000 = 4950 / 10000 = 0.495 -> 0
        (100, 1),                      // 100 * 50 / 10000 = 5000 / 10000 = 0.50 -> 1 (half-up)
        (101, 1),                      // 101 * 50 / 10000 = 5050 / 10000 = 0.505 -> 1
        (299, 1),                      // 299 * 50 / 10000 = 14950 / 10000 = 1.495 -> 1
        (300, 2),                      // 300 * 50 / 10000 = 15000 / 10000 = 1.50 -> 2 (half-up)
        (50_000_000, 250_000),         // 50M * 0.5% = 250k
        (4_800_000_000, 24_000_000),   // Statutory threshold Rp 4.8B -> Rp 24M
        (9_000_000_000_000_000, 45_000_000_000_000), // 9 Quadrillion IDR
    ];

    for (amt, exp_tax) in cases {
        let (status, body) = send_request(
            &harness.app,
            Method::POST,
            "/api/v1/accounting/tax/calculate",
            &harness.owner_token,
            Some(&harness.tenant_a_id),
            Some(json!({
                "amount": amt,
                "tax_type": "UMKM_05"
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "Failed for amount {}", amt);
        let tax = body["tax_amount"].as_i64().unwrap();
        assert_eq!(tax, exp_tax, "Tax mismatch for amt={}", amt);
    }
}

#[test]
fn test_tax_invariant_exhaustive_sweep() {
    use backend::domain::accounting::calculate_tax_integer;

    // Exhaustive property test over range 0..10_000 for all inclusive & exclusive types
    for amt in 0..=10_000 {
        // PPN 11 Excl
        let r1 = calculate_tax_integer(amt, "PPN_11_EXCL", false).unwrap();
        assert_eq!(r1.net_amount + r1.tax_amount, r1.gross_amount);

        // PPN 11 Incl
        let r2 = calculate_tax_integer(amt, "PPN_11_INCL", true).unwrap();
        assert_eq!(r2.net_amount + r2.tax_amount, r2.gross_amount);

        // PPN 12 Excl
        let r3 = calculate_tax_integer(amt, "PPN_12_EXCL", false).unwrap();
        assert_eq!(r3.net_amount + r3.tax_amount, r3.gross_amount);

        // PPN 12 Incl
        let r4 = calculate_tax_integer(amt, "PPN_12_INCL", true).unwrap();
        assert_eq!(r4.net_amount + r4.tax_amount, r4.gross_amount);

        // Exempt
        let r5 = calculate_tax_integer(amt, "EXEMPT", false).unwrap();
        assert_eq!(r5.net_amount + r5.tax_amount, r5.gross_amount);
    }
}

// ============================================================================
// 2. Reversal Lifecycle & Concurrency Race Condition
// ============================================================================

#[tokio::test]
async fn test_reversal_lifecycle_and_trial_balance_net_zero() {
    let harness = setup_harness().await;

    // 1. Post a 4-line journal
    let (post_s, post_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Multi-Line Commercial Sale",
            "lines": [
                {"account_code": "1000", "debit": 500000, "credit": 0, "memo": "Cash received"},
                {"account_code": "1100", "debit": 610000, "credit": 0, "memo": "Bank transfer"},
                {"account_code": "4000", "debit": 0, "credit": 1000000, "memo": "Revenue"},
                {"account_code": "2100", "debit": 0, "credit": 110000, "memo": "PPN 11% payable"}
            ]
        })),
    )
    .await;

    assert_eq!(post_s, StatusCode::CREATED);
    let orig_id = post_b["id"].as_str().unwrap().to_string();

    // Check trial balance reflects the entry
    let (tb_s1, tb_b1) = send_request(
        &harness.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb_s1, StatusCode::OK);
    assert_eq!(tb_b1["is_balanced"], true);
    assert_eq!(tb_b1["total_debit"], 1110000);
    assert_eq!(tb_b1["total_credit"], 1110000);

    // 2. Reverse the journal
    let (rev_s, rev_b) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Customer cancellation"})),
    )
    .await;

    assert_eq!(rev_s, StatusCode::CREATED);
    assert_eq!(rev_b["source_type"], "REVERSAL");
    assert_eq!(rev_b["source_id"], orig_id);

    // Verify lines are inverted
    let rev_lines = rev_b["lines"].as_array().unwrap();
    assert_eq!(rev_lines.len(), 4);

    let l1000 = rev_lines.iter().find(|l| l["account_code"] == "1000").unwrap();
    assert_eq!(l1000["debit"], 0);
    assert_eq!(l1000["credit"], 500000);

    let l1100 = rev_lines.iter().find(|l| l["account_code"] == "1100").unwrap();
    assert_eq!(l1100["debit"], 0);
    assert_eq!(l1100["credit"], 610000);

    let l4000 = rev_lines.iter().find(|l| l["account_code"] == "4000").unwrap();
    assert_eq!(l4000["debit"], 1000000);
    assert_eq!(l4000["credit"], 0);

    let l2100 = rev_lines.iter().find(|l| l["account_code"] == "2100").unwrap();
    assert_eq!(l2100["debit"], 110000);
    assert_eq!(l2100["credit"], 0);

    // 3. Verify trial balance net impact is ZERO for all accounts
    let (tb_s2, tb_b2) = send_request(
        &harness.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb_s2, StatusCode::OK);
    assert_eq!(tb_b2["is_balanced"], true);
    assert_eq!(tb_b2["net_balance"], 0);

    let accounts = tb_b2["accounts"].as_array().unwrap();
    for acc in accounts {
        assert_eq!(acc["balance"], 0, "Account {} has non-zero net balance after reversal", acc["code"]);
    }
}

#[tokio::test]
async fn test_concurrent_reversal_race_condition() {
    let harness = setup_harness().await;

    // 1. Post a journal in Tenant A
    let (post_s, post_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Journal to Race Reversal",
            "lines": [
                {"account_code": "1000", "debit": 200000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 200000}
            ]
        })),
    )
    .await;

    assert_eq!(post_s, StatusCode::CREATED);
    let orig_id = post_b["id"].as_str().unwrap().to_string();

    // 2. Concurrently execute two simultaneous reversal requests
    let app1 = harness.app.clone();
    let app2 = harness.app.clone();
    let token1 = harness.owner_token.clone();
    let token2 = harness.owner_token.clone();
    let tenant1 = harness.tenant_a_id.clone();
    let tenant2 = harness.tenant_a_id.clone();
    let orig_id1 = orig_id.clone();
    let orig_id2 = orig_id.clone();

    let task1 = tokio::spawn(async move {
        send_request(
            &app1,
            Method::POST,
            &format!("/api/v1/accounting/journals/{}/reverse", orig_id1),
            &token1,
            Some(&tenant1),
            Some(json!({"reason": "Concurrent race request 1"})),
        )
        .await
    });

    let task2 = tokio::spawn(async move {
        send_request(
            &app2,
            Method::POST,
            &format!("/api/v1/accounting/journals/{}/reverse", orig_id2),
            &token2,
            Some(&tenant2),
            Some(json!({"reason": "Concurrent race request 2"})),
        )
        .await
    });

    let (res1, res2) = tokio::join!(task1, task2);
    let (status1, body1) = res1.unwrap();
    let (status2, body2) = res2.unwrap();

    let mut statuses = vec![status1, status2];
    statuses.sort_by_key(|s| s.as_u16());

    // Invariant: Exactly one must succeed with 201 Created.
    // The other MUST fail with 409 Conflict (ALREADY_REVERSED).
    // It must NEVER result in two 201s (duplicate reversal) or an unhandled 500 error!
    assert_eq!(
        statuses,
        vec![StatusCode::CREATED, StatusCode::CONFLICT],
        "Race condition failure! Responses were: (status={}, body={:?}) and (status={}, body={:?})",
        status1, body1, status2, body2
    );

    let conflict_body = if status1 == StatusCode::CONFLICT { body1 } else { body2 };
    assert_eq!(conflict_body["code"], "ALREADY_REVERSED");
}

// ============================================================================
// 3. Cross-Tenant Attack Simulation
// ============================================================================

#[tokio::test]
async fn test_cross_tenant_access_and_reversal_strictly_404() {
    let harness = setup_harness().await;

    // 1. Post a journal in Tenant A
    let (post_s, post_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Tenant A Confidential Transaction",
            "lines": [
                {"account_code": "1000", "debit": 5000000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 5000000}
            ]
        })),
    )
    .await;
    assert_eq!(post_s, StatusCode::CREATED);
    let j_id = post_b["id"].as_str().unwrap();

    // 2. Tenant B attempts to read Tenant A's journal
    let (read_s, read_b) = send_request(
        &harness.app,
        Method::GET,
        &format!("/api/v1/accounting/journals/{}", j_id),
        &harness.tenant_b_token,
        Some(&harness.tenant_b_id),
        None,
    )
    .await;
    assert_eq!(read_s, StatusCode::NOT_FOUND, "Cross-tenant read must return 404");
    assert_eq!(read_b["code"], "NOT_FOUND");

    // 3. Tenant B attempts to reverse Tenant A's journal
    let (rev_s, rev_b) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j_id),
        &harness.tenant_b_token,
        Some(&harness.tenant_b_id),
        Some(json!({"reason": "Cross-tenant malicious attack"})),
    )
    .await;
    assert_eq!(rev_s, StatusCode::NOT_FOUND, "Cross-tenant reversal must return 404");
    assert_eq!(rev_b["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_cross_tenant_foreign_or_nonexistent_account_code_rejected() {
    let harness = setup_harness().await;

    // 1. Tenant B creates a custom account "7777" (e.g. "Tenant B Special Account")
    let (ca_s, ca_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.tenant_b_token,
        Some(&harness.tenant_b_id),
        Some(json!({
            "code": "7777",
            "name": "Tenant B Special Account",
            "account_type": "asset"
        })),
    )
    .await;
    assert_eq!(ca_s, StatusCode::CREATED);
    assert_eq!(ca_b["code"], "7777");

    // 2. Attack Scenario A: Tenant A attempts to post a journal using Tenant B's custom account "7777"
    let (post_a_s, post_a_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Tenant A using Tenant B Account",
            "lines": [
                {"account_code": "7777", "debit": 100000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100000}
            ]
        })),
    )
    .await;

    // Requirement: Attempt to post journal using Tenant B's account code in Tenant A's tenant -> must reject.
    // Worker claimed: "Referenced accounts must exist within the active TenantContext; non-existent or foreign accounts return HTTP 422 Unprocessable Entity (ACCOUNT_NOT_FOUND)."
    println!("Posting with Tenant B's account code: status={}, body={:?}", post_a_s, post_a_b);

    // 3. Attack Scenario B: Tenant A attempts to post a journal using completely non-existent account "9999"
    let (post_b_s, post_b_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Tenant A using Non-existent Account",
            "lines": [
                {"account_code": "9999", "debit": 100000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100000}
            ]
        })),
    )
    .await;

    println!("Posting with non-existent account code: status={}, body={:?}", post_b_s, post_b_b);

    // The system MUST reject both attacks with 422 Unprocessable Entity and ACCOUNT_NOT_FOUND.
    assert_eq!(
        post_a_s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Posting with Tenant B's account code was NOT rejected with 422! Got HTTP {}",
        post_a_s
    );
    assert_eq!(post_a_b["code"], "ACCOUNT_NOT_FOUND");

    assert_eq!(
        post_b_s,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Posting with non-existent account code was NOT rejected with 422! Got HTTP {}",
        post_b_s
    );
    assert_eq!(post_b_b["code"], "ACCOUNT_NOT_FOUND");
}

#[tokio::test]
async fn test_unregistered_account_rejection_protects_trial_balance() {
    let harness = setup_harness().await;

    // Tenant A posts a journal referencing account "9999" (not in Tenant A's COA) and "4000" (in COA)
    let (post_s, post_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Bogus Account Entry",
            "lines": [
                {"account_code": "9999", "debit": 100000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100000}
            ]
        })),
    )
    .await;

    assert_eq!(post_s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(post_b["code"], "ACCOUNT_NOT_FOUND");

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
async fn test_stress_concurrent_reversals_20_iterations() {
    let harness = setup_harness().await;

    for i in 0..20 {
        // 1. Post a journal
        let (post_s, post_b) = send_request(
            &harness.app,
            Method::POST,
            "/api/v1/accounting/journals",
            &harness.owner_token,
            Some(&harness.tenant_a_id),
            Some(json!({
                "description": format!("Concurrent Test Journal {}", i),
                "lines": [
                    {"account_code": "1000", "debit": 1000 + i, "credit": 0},
                    {"account_code": "4000", "debit": 0, "credit": 1000 + i}
                ]
            })),
        )
        .await;

        assert_eq!(post_s, StatusCode::CREATED);
        let orig_id = post_b["id"].as_str().unwrap().to_string();

        let app1 = harness.app.clone();
        let app2 = harness.app.clone();
        let token1 = harness.owner_token.clone();
        let token2 = harness.owner_token.clone();
        let tenant1 = harness.tenant_a_id.clone();
        let tenant2 = harness.tenant_a_id.clone();
        let id1 = orig_id.clone();
        let id2 = orig_id.clone();

        let task1 = tokio::spawn(async move {
            send_request(
                &app1,
                Method::POST,
                &format!("/api/v1/accounting/journals/{}/reverse", id1),
                &token1,
                Some(&tenant1),
                Some(json!({"reason": "Race request 1"})),
            )
            .await
        });

        let task2 = tokio::spawn(async move {
            send_request(
                &app2,
                Method::POST,
                &format!("/api/v1/accounting/journals/{}/reverse", id2),
                &token2,
                Some(&tenant2),
                Some(json!({"reason": "Race request 2"})),
            )
            .await
        });

        let (res1, res2) = tokio::join!(task1, task2);
        let (status1, body1) = res1.unwrap();
        let (status2, body2) = res2.unwrap();

        let mut statuses = vec![status1, status2];
        statuses.sort_by_key(|s| s.as_u16());

        assert_eq!(
            statuses,
            vec![StatusCode::CREATED, StatusCode::CONFLICT],
            "Iteration {}: Race condition failure! Got (status={}, body={:?}) and (status={}, body={:?})",
            i, status1, body1, status2, body2
        );
    }
}
