//! Milestone 3 Empirical Adversarial Challenger Verification Suite
//!
//! Identity: teamwork_preview_challenger_m3_2
//! Roles: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Payment allocation boundary attacks:
//!    - Overpayment rejection (400 OVERPAYMENT_NOT_ALLOWED on single, incremental, and settled invoices)
//!    - Zero and negative payment rejection (400 INVALID_AMOUNT)
//!    - Payment on voided invoice (409 INVOICE_VOIDED), draft invoice (404 NOT_FOUND), and cross-tenant (404 NOT_FOUND)
//! 2. Concurrent payment races against same receivable:
//!    - Simultaneous competing payments whose sum exceeds outstanding balance: at most one succeeds, no underflow
//!    - Simultaneous partial payments: integrity preserved, zero negative balance, GL entries balanced
//! 3. Idempotency Engine:
//!    - Cache replay returns 201 Created with X-Cache-Replay: true header
//!    - Mismatched payload returns 409 IDEMPOTENCY_KEY_MISMATCH
//!    - User scoping isolation
//! 4. Aging Engine buckets across boundary timestamps:
//!    - Exact boundary checks: Day 0, Day 30, Day 31, Day 60, Day 61, Day 90, Day 91, Day 180
//!    - Exclusions: Voided, Paid (balance 0), and cross-tenant receivables
//! 5. SQLite immutability triggers under direct SQL attacks
//! 6. General Ledger double-entry financial invariants (Debit == Credit)

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
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    #[allow(dead_code)]
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m3_challenger_adversarial.sqlite");
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

    let jwt_secret = "m3_challenger_jwt_secret_key_1234567890_adversarial";
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
            email: "owner_a@alpha.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Alpha Owner".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_a@alpha.com", "user", "premium")
        .unwrap();

    // 2. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_a@alpha.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Alpha Staff".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_a@alpha.com", "user", "free")
        .unwrap();

    // 3. Tenant A
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

    // 4. Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "user_b@beta.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Beta User".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "user_b@beta.com", "user", "free")
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
        tenant_b_id,
        tenant_b_token,
    }
}

async fn parse_response(res: axum::response::Response) -> (StatusCode, Value, HeaderMap) {
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json, headers)
}

async fn create_issued_invoice(harness: &ChallengerHarness, token: &str, tenant_id: &str, amount: i64) -> String {
    let payload = json!({
        "customer_name": "Adversarial Client Corp",
        "tax_type": "NONE",
        "items": [{ "description": "Consulting Services", "quantity": 1, "unit_price": amount }]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED);
    let invoice_id = body["id"].as_str().unwrap().to_string();

    let issue_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id)
        .body(Body::empty())
        .unwrap();

    let (issue_status, _, _) = parse_response(harness.app.clone().oneshot(issue_req).await.unwrap()).await;
    assert_eq!(issue_status, StatusCode::OK);
    invoice_id
}

// ============================================================================
// 1. PAYMENT ALLOCATION BOUNDARY ATTACKS
// ============================================================================

#[tokio::test]
async fn test_adversarial_overpayment_boundary_attacks() {
    let harness = setup_harness().await;

    // Issue invoice for 1,000,000 Rupiah exactly
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;

    // Attack 1.1: 1 Rupiah overpayment (amount = 1_000_001)
    let req_over_1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 1_000_001 }).to_string()))
        .unwrap();
    let (s1, b1, _) = parse_response(harness.app.clone().oneshot(req_over_1).await.unwrap()).await;
    assert_eq!(s1, StatusCode::BAD_REQUEST);
    assert_eq!(b1["code"], "OVERPAYMENT_NOT_ALLOWED");

    // Attack 1.2: Massive overpayment (amount = 1,000,000,000)
    let req_over_huge = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 1_000_000_000 }).to_string()))
        .unwrap();
    let (sh, bh, _) = parse_response(harness.app.clone().oneshot(req_over_huge).await.unwrap()).await;
    assert_eq!(sh, StatusCode::BAD_REQUEST);
    assert_eq!(bh["code"], "OVERPAYMENT_NOT_ALLOWED");

    // Step: Legitimate partial payment of 600,000 -> remaining balance is 400,000
    let req_partial = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 600_000 }).to_string()))
        .unwrap();
    let (sp, bp, _) = parse_response(harness.app.clone().oneshot(req_partial).await.unwrap()).await;
    assert_eq!(sp, StatusCode::CREATED);
    assert_eq!(bp["outstanding_balance"], 400_000);

    // Attack 1.3: Overpayment on remaining partial balance (pay 400,001 on 400,000 balance)
    let req_over_part = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 400_001 }).to_string()))
        .unwrap();
    let (sop, bop, _) = parse_response(harness.app.clone().oneshot(req_over_part).await.unwrap()).await;
    assert_eq!(sop, StatusCode::BAD_REQUEST);
    assert_eq!(bop["code"], "OVERPAYMENT_NOT_ALLOWED");

    // Step: Exact final settlement (pay 400,000 -> balance becomes 0, status PAID)
    let req_settle = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 400_000 }).to_string()))
        .unwrap();
    let (ss, bs, _) = parse_response(harness.app.clone().oneshot(req_settle).await.unwrap()).await;
    assert_eq!(ss, StatusCode::CREATED);
    assert_eq!(bs["outstanding_balance"], 0);

    // Attack 1.4: Payment against already fully paid invoice (pay 1 Rupiah on balance 0)
    let req_over_settled = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 1 }).to_string()))
        .unwrap();
    let (sos, bos, _) = parse_response(harness.app.clone().oneshot(req_over_settled).await.unwrap()).await;
    assert_eq!(sos, StatusCode::BAD_REQUEST);
    assert_eq!(bos["code"], "OVERPAYMENT_NOT_ALLOWED");
}

#[tokio::test]
async fn test_adversarial_zero_and_negative_payment_rejection() {
    let harness = setup_harness().await;
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 500_000).await;

    // Boundary 2.1: Exactly zero amount
    let req_zero = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 0 }).to_string()))
        .unwrap();
    let (sz, bz, _) = parse_response(harness.app.clone().oneshot(req_zero).await.unwrap()).await;
    assert_eq!(sz, StatusCode::BAD_REQUEST);
    assert_eq!(bz["code"], "INVALID_AMOUNT");

    // Boundary 2.2: Negative 1 Rupiah
    let req_neg1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": -1 }).to_string()))
        .unwrap();
    let (sn1, bn1, _) = parse_response(harness.app.clone().oneshot(req_neg1).await.unwrap()).await;
    assert_eq!(sn1, StatusCode::BAD_REQUEST);
    assert_eq!(bn1["code"], "INVALID_AMOUNT");

    // Boundary 2.3: Large negative amount (-500,000)
    let req_neg_large = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": -500_000 }).to_string()))
        .unwrap();
    let (snl, bnl, _) = parse_response(harness.app.clone().oneshot(req_neg_large).await.unwrap()).await;
    assert_eq!(snl, StatusCode::BAD_REQUEST);
    assert_eq!(bnl["code"], "INVALID_AMOUNT");

    // Boundary 2.4: Extreme negative (i64::MIN + 1)
    let req_min = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": -9_223_372_036_854_775_807i64 }).to_string()))
        .unwrap();
    let (smin, bmin, _) = parse_response(harness.app.clone().oneshot(req_min).await.unwrap()).await;
    assert_eq!(smin, StatusCode::BAD_REQUEST);
    assert_eq!(bmin["code"], "INVALID_AMOUNT");
}

#[tokio::test]
async fn test_adversarial_payment_against_voided_and_draft_invoices() {
    let harness = setup_harness().await;

    // 1. DRAFT Invoice (not yet issued -> no receivable created yet)
    let draft_payload = json!({
        "customer_name": "Draft Only Corp",
        "items": [{ "description": "Consulting", "quantity": 1, "unit_price": 400_000 }]
    });
    let req_draft = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(draft_payload.to_string()))
        .unwrap();
    let (sd, bd, _) = parse_response(harness.app.clone().oneshot(req_draft).await.unwrap()).await;
    assert_eq!(sd, StatusCode::CREATED);
    let draft_id = bd["id"].as_str().unwrap();

    // Attempt payment on DRAFT -> HTTP 404 NOT_FOUND
    let req_pay_draft = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": draft_id, "amount": 100_000 }).to_string()))
        .unwrap();
    let (spd, bpd, _) = parse_response(harness.app.clone().oneshot(req_pay_draft).await.unwrap()).await;
    assert_eq!(spd, StatusCode::NOT_FOUND);
    assert_eq!(bpd["code"], "NOT_FOUND");

    // 2. VOIDED Invoice (issued and then voided)
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 350_000).await;
    let void_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Adversarial void testing" }).to_string()))
        .unwrap();
    let (sv, _, _) = parse_response(harness.app.clone().oneshot(void_req).await.unwrap()).await;
    assert_eq!(sv, StatusCode::OK);

    // Attempt payment on VOIDED invoice -> HTTP 409 Conflict with code INVOICE_VOIDED
    let pay_void_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 100_000 }).to_string()))
        .unwrap();
    let (spv, bpv, _) = parse_response(harness.app.clone().oneshot(pay_void_req).await.unwrap()).await;
    assert_eq!(spv, StatusCode::CONFLICT);
    assert_eq!(bpv["code"], "INVOICE_VOIDED");
}

#[tokio::test]
async fn test_adversarial_cross_tenant_payment_strictly_404() {
    let harness = setup_harness().await;

    // Tenant A creates and issues invoice
    let inv_a = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 800_000).await;

    // Tenant B attempts to allocate payment to Tenant A's invoice
    let req_cross = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.tenant_b_token))
        .header("X-Tenant-ID", &harness.tenant_b_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_a, "amount": 200_000 }).to_string()))
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(req_cross).await.unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

// ============================================================================
// 2. CONCURRENT PAYMENT RACES EMPIRICAL STRESS TESTS
// ============================================================================

#[tokio::test]
async fn test_adversarial_concurrent_payment_races_exceeding_balance() {
    let harness = setup_harness().await;

    // Create an invoice with exactly 1,000,000 Rupiah balance
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;

    // Concurrently fire 5 simultaneous payment allocations of 600,000 each
    // Total attempted = 3,000,000 > 1,000,000.
    // Invariant: At most ONE may succeed (because 1,000,000 - 600,000 = 400,000; all remaining attempts demand 600k which exceeds 400k)
    let num_tasks = 5;
    let mut join_set = tokio::task::JoinSet::new();

    for i in 0..num_tasks {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_a_id.clone();
        let invoice_id = inv_id.clone();

        join_set.spawn(async move {
            let payload = json!({
                "invoice_id": invoice_id,
                "amount": 600_000,
                "payment_method": "BANK_TRANSFER",
                "reference": format!("RACE-OVERPAY-{}", i)
            });

            let req = Request::builder()
                .method(Method::POST)
                .uri("/api/v1/payments")
                .header(AUTHORIZATION, format!("Bearer {}", token))
                .header("X-Tenant-ID", &tenant_id)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();

            let res = app.oneshot(req).await.unwrap();
            parse_response(res).await
        });
    }

    let mut created_count = 0;
    let mut rejected_count = 0;
    let mut unexpected_errors = Vec::new();

    while let Some(res) = join_set.join_next().await {
        let (status, body, _) = res.expect("Task failed to join");
        match status {
            StatusCode::CREATED => {
                created_count += 1;
                assert_eq!(body["amount"], 600_000);
            }
            StatusCode::BAD_REQUEST => {
                rejected_count += 1;
                assert_eq!(body["code"], "OVERPAYMENT_NOT_ALLOWED");
            }
            StatusCode::CONFLICT => {
                // If SQLite concurrency/idempotency returns conflict, it is also a safe rejection
                rejected_count += 1;
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                // If SQLite busy snapshot aborts concurrent transaction, it prevented overpayment
                // but let's record it
                unexpected_errors.push((status, body));
            }
            other => {
                unexpected_errors.push((other, body));
            }
        }
    }

    // CRITICAL EMPIRICAL INVARIANT:
    // Exactly 1 winner can succeed! (Or 0 if all clashed on lock)
    assert!(
        created_count <= 1,
        "CRITICAL BUG: Multiple competing payments succeeded exceeding balance! created_count={}",
        created_count
    );
    assert_eq!(created_count, 1, "Exactly 1 concurrent payment should have succeeded");
    assert_eq!(created_count + rejected_count + unexpected_errors.len(), 5, "All 5 concurrent payment tasks must be accounted for");

    // Invariant: Database state MUST reflect exactly the 1 successful payment
    let rec: (i64, i64, i64) = sqlx::query_as(
        "SELECT total_amount, allocated_amount, outstanding_amount FROM receivables WHERE invoice_id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let total_amount = rec.0;
    let allocated_amount = rec.1;
    let outstanding_amount = rec.2;

    // Verify non-underflow
    assert_eq!(total_amount, 1_000_000);
    assert_eq!(allocated_amount, 600_000);
    assert_eq!(outstanding_amount, 400_000);
    assert!(outstanding_amount >= 0, "Receivable outstanding balance underflowed!");

    // Verify payment count in database
    let pay_count: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(pay_count, 1);

    // Verify payment allocation count in database
    let alloc_count: i64 = sqlx::query_scalar("SELECT count(*) FROM payment_allocations WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(alloc_count, 1);

    // Verify total allocated amount in payment_allocations table matches allocated_amount on receivable
    let sum_allocations: i64 = sqlx::query_scalar("SELECT SUM(amount) FROM payment_allocations WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(sum_allocations, 600_000);
}

#[tokio::test]
async fn test_adversarial_concurrent_partial_payments_no_underflow() {
    let harness = setup_harness().await;

    // Create an invoice with 500,000 Rupiah balance
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 500_000).await;

    // Fire 5 concurrent requests of 100,000 each (Sum = 500,000, exactly matching balance)
    let num_tasks = 5;
    let mut join_set = tokio::task::JoinSet::new();

    for i in 0..num_tasks {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_a_id.clone();
        let invoice_id = inv_id.clone();

        join_set.spawn(async move {
            let payload = json!({
                "invoice_id": invoice_id,
                "amount": 100_000,
                "payment_method": "BANK_TRANSFER",
                "reference": format!("RACE-EXACT-{}", i)
            });

            let req = Request::builder()
                .method(Method::POST)
                .uri("/api/v1/payments")
                .header(AUTHORIZATION, format!("Bearer {}", token))
                .header("X-Tenant-ID", &tenant_id)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();

            let res = app.oneshot(req).await.unwrap();
            let (status, body, _) = parse_response(res).await;
            (i, status, body)
        });
    }

    let mut successful_payments = 0;
    let mut task_results = Vec::new();
    while let Some(res) = join_set.join_next().await {
        let (task_idx, status, body) = res.expect("Task failed to join");
        if status == StatusCode::CREATED {
            successful_payments += 1;
        }
        task_results.push((task_idx, status, body));
    }

    println!("Task results: {:?}", task_results);

    // Invariant: successful_payments <= 5
    assert!(successful_payments <= 5, "More than 5 payments succeeded for a 500k invoice!");

    // Check database state
    let (rec_alloc, rec_out): (i64, i64) = sqlx::query_as(
        "SELECT allocated_amount, outstanding_amount FROM receivables WHERE invoice_id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let db_payments_count: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    let db_alloc_sum: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM payment_allocations WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    println!("DB state: rec_alloc={}, rec_out={}, db_payments_count={}, db_alloc_sum={}, successful_payments={}",
        rec_alloc, rec_out, db_payments_count, db_alloc_sum, successful_payments);

    // Check whether all recorded payments have corresponding GL journal entries
    let payments_without_journals: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM payments p
        WHERE p.tenant_id = ?1
        AND NOT EXISTS (
            SELECT 1 FROM journal_entries j
            WHERE j.source_type = 'PAYMENT' AND j.source_id = p.id
        )
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    println!("AUDIT: payments_without_journals = {}", payments_without_journals);

    // Invariant 1: No balance underflow (outstanding amount cannot be negative)
    assert!(rec_out >= 0, "Outstanding balance must not underflow!");

    // Invariant 2: Total allocated cannot exceed total invoice amount
    assert!(rec_alloc <= 500_000, "Allocated amount cannot exceed invoice total!");

    // Invariant 3: Balance conservation holds
    assert_eq!(rec_alloc + rec_out, 500_000, "Allocated + Outstanding must equal total invoice amount!");

    // Invariant 4: Payment allocations table matches receivable allocated amount
    assert_eq!(db_alloc_sum, rec_alloc, "Sum of payment allocations must match receivable allocated_amount!");

    // Invariant 5: Unique payment numbers
    let total_payments: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let distinct_numbers: i64 = sqlx::query_scalar("SELECT count(DISTINCT payment_number) FROM payments WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(total_payments, distinct_numbers, "Payment numbers must be strictly unique!");

    // Invariant 6: Strict Double-Entry Atomicity - Zero orphaned payments
    assert_eq!(
        payments_without_journals, 0,
        "CRITICAL BUG: Non-atomic split transaction resulted in {} payments committed without a General Ledger journal entry!",
        payments_without_journals
    );

    // Invariant 7: Response Consistency - Database payment count matches HTTP 201 count
    assert_eq!(
        db_payments_count, successful_payments,
        "CRITICAL BUG: Database committed payments ({}) does not match HTTP 201 Created count ({})!",
        db_payments_count, successful_payments
    );
}

#[tokio::test]
async fn test_adversarial_sequential_full_settlement_reaches_exact_zero_and_paid_status() {
    let harness = setup_harness().await;

    // Create and issue an invoice for exactly 500,000 Rupiah
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 500_000).await;

    // Fire 5 sequential payments of 100,000 each
    for i in 1..=5 {
        let payload = json!({
            "invoice_id": inv_id,
            "amount": 100_000,
            "payment_method": "BANK_TRANSFER",
            "reference": format!("SEQ-PAY-{}", i)
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/payments")
            .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
            .header("X-Tenant-ID", &harness.tenant_a_id)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();

        let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["amount"], 100_000);
        assert_eq!(body["outstanding_balance"], 500_000 - (i * 100_000));
    }

    // Verify final state in database:
    // 1. Receivable status is PAID, outstanding is 0, allocated is 500,000
    let (rec_status, rec_alloc, rec_out): (String, i64, i64) = sqlx::query_as(
        "SELECT status, allocated_amount, outstanding_amount FROM receivables WHERE invoice_id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(rec_status, "PAID");
    assert_eq!(rec_alloc, 500_000);
    assert_eq!(rec_out, 0);

    // 2. Invoice status is PAID, balance_due is 0
    let (inv_status, inv_balance): (String, i64) = sqlx::query_as(
        "SELECT status, balance_due FROM invoices WHERE id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(inv_status, "PAID");
    assert_eq!(inv_balance, 0);

    // 3. Exactly 5 payments exist and each has a GL journal entry
    let pay_count: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(pay_count, 5);

    let payments_without_journals: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM payments p
        WHERE p.tenant_id = ?1 AND p.invoice_id = ?2
        AND NOT EXISTS (
            SELECT 1 FROM journal_entries j
            WHERE j.source_type = 'PAYMENT' AND j.source_id = p.id
        )
        "#,
    )
    .bind(&harness.tenant_a_id)
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(payments_without_journals, 0, "Every sequential payment must have a GL journal entry");

    // 4. Overpayment on settled invoice must be rejected
    let overpay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 10_000 }).to_string()))
        .unwrap();
    let (o_status, o_body, _) = parse_response(harness.app.clone().oneshot(overpay_req).await.unwrap()).await;
    assert_eq!(o_status, StatusCode::BAD_REQUEST);
    assert_eq!(o_body["code"], "OVERPAYMENT_NOT_ALLOWED");
}

// ============================================================================
// 3. IDEMPOTENCY ENGINE VERIFICATION
// ============================================================================

#[tokio::test]
async fn test_adversarial_idempotency_cache_replay_and_mismatch_rejection() {
    let harness = setup_harness().await;
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;

    let idempotency_key = format!("test_idemp_{}", Uuid::new_v4());
    let valid_payload = json!({
        "invoice_id": inv_id,
        "amount": 350_000,
        "payment_method": "BANK_TRANSFER",
        "reference": "INITIAL-REF"
    });

    // 1. Initial request with Idempotency-Key
    let req1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", &idempotency_key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(valid_payload.to_string()))
        .unwrap();

    let (s1, b1, h1) = parse_response(harness.app.clone().oneshot(req1).await.unwrap()).await;
    assert_eq!(s1, StatusCode::CREATED);
    assert!(h1.get("x-cache-replay").is_none());
    let orig_payment_id = b1["id"].as_str().unwrap().to_string();
    let orig_outstanding = b1["outstanding_balance"].as_i64().unwrap();
    assert_eq!(orig_outstanding, 650_000);

    // 2. Replay EXACT same request payload with same Idempotency-Key
    // Must return cached response with X-Cache-Replay: true header
    let req_replay = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", &idempotency_key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(valid_payload.to_string()))
        .unwrap();

    let (sr, br, hr) = parse_response(harness.app.clone().oneshot(req_replay).await.unwrap()).await;
    assert_eq!(sr, StatusCode::CREATED);
    assert_eq!(
        hr.get("x-cache-replay").and_then(|v| v.to_str().ok()),
        Some("true"),
        "Cached response must contain X-Cache-Replay: true"
    );
    assert_eq!(br["id"].as_str().unwrap(), orig_payment_id);
    assert_eq!(br["outstanding_balance"].as_i64().unwrap(), orig_outstanding);

    // Verify balance in DB was NOT decremented again!
    let db_outstanding: i64 = sqlx::query_scalar("SELECT outstanding_amount FROM receivables WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(db_outstanding, 650_000);

    let payment_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(payment_rows, 1, "Only 1 payment record must exist after idempotency replay");

    // 3. Attack: Replay SAME key with altered amount (350,000 -> 450,000)
    let altered_payload_amount = json!({
        "invoice_id": inv_id,
        "amount": 450_000,
        "payment_method": "BANK_TRANSFER",
        "reference": "INITIAL-REF"
    });
    let req_mismatch_amount = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", &idempotency_key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(altered_payload_amount.to_string()))
        .unwrap();

    let (sma, bma, _) = parse_response(harness.app.clone().oneshot(req_mismatch_amount).await.unwrap()).await;
    assert_eq!(sma, StatusCode::CONFLICT);
    assert_eq!(bma["code"], "IDEMPOTENCY_KEY_MISMATCH");

    // 4. Attack: Replay SAME key with altered payment method ("BANK_TRANSFER" -> "CASH")
    let altered_payload_method = json!({
        "invoice_id": inv_id,
        "amount": 350_000,
        "payment_method": "CASH",
        "reference": "INITIAL-REF"
    });
    let req_mismatch_method = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", &idempotency_key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(altered_payload_method.to_string()))
        .unwrap();

    let (smm, bmm, _) = parse_response(harness.app.clone().oneshot(req_mismatch_method).await.unwrap()).await;
    assert_eq!(smm, StatusCode::CONFLICT);
    assert_eq!(bmm["code"], "IDEMPOTENCY_KEY_MISMATCH");
}

// ============================================================================
// 4. AGING ENGINE BUCKETS ACROSS BOUNDARY TIMESTAMPS
// ============================================================================

#[tokio::test]
async fn test_adversarial_aging_buckets_exact_boundary_timestamps() {
    let harness = setup_harness().await;

    let today = Utc::now().date_naive();

    // Clean any seeded invoices for Tenant A
    sqlx::query("DELETE FROM receivables WHERE tenant_id = ?1")
        .bind(&harness.tenant_a_id)
        .execute(&harness.pool)
        .await
        .unwrap();

    // Define 9 test invoices with exact boundary dates:
    // 1. Day -15 (Future, due in 15 days): 100,000 -> current_0_30
    // 2. Day 0 (Due today): 200,000 -> current_0_30
    // 3. Day 30 (Exactly 30 days overdue): 300,000 -> current_0_30
    // 4. Day 31 (Exactly 31 days overdue): 400,000 -> overdue_31_60
    // 5. Day 60 (Exactly 60 days overdue): 500,000 -> overdue_31_60
    // 6. Day 61 (Exactly 61 days overdue): 600,000 -> overdue_61_90
    // 7. Day 90 (Exactly 90 days overdue): 700,000 -> overdue_61_90
    // 8. Day 91 (Exactly 91 days overdue): 800,000 -> overdue_90_plus
    // 9. Day 180 (180 days overdue): 900,000 -> overdue_90_plus

    let test_cases = vec![
        ("future_15", (today + Duration::days(15)).format("%Y-%m-%d").to_string(), 100_000),
        ("day_0", today.format("%Y-%m-%d").to_string(), 200_000),
        ("day_30", (today - Duration::days(30)).format("%Y-%m-%d").to_string(), 300_000),
        ("day_31", (today - Duration::days(31)).format("%Y-%m-%d").to_string(), 400_000),
        ("day_60", (today - Duration::days(60)).format("%Y-%m-%d").to_string(), 500_000),
        ("day_61", (today - Duration::days(61)).format("%Y-%m-%d").to_string(), 600_000),
        ("day_90", (today - Duration::days(90)).format("%Y-%m-%d").to_string(), 700_000),
        ("day_91", (today - Duration::days(91)).format("%Y-%m-%d").to_string(), 800_000),
        ("day_180", (today - Duration::days(180)).format("%Y-%m-%d").to_string(), 900_000),
    ];

    for (name, due_date, amount) in test_cases {
        let inv_id = format!("inv_{}", name);
        let rec_id = format!("rec_{}", name);

        sqlx::query(
            r#"
            INSERT INTO invoices (id, tenant_id, customer_name, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, balance_due, status, created_at, updated_at)
            VALUES (?1, ?2, 'Aged Client', ?3, 'IDR', 'NONE', ?4, 0, 0, ?4, ?4, 'ISSUED', '2026-01-01', '2026-01-01')
            "#,
        )
        .bind(&inv_id)
        .bind(&harness.tenant_a_id)
        .bind(&due_date)
        .bind(amount)
        .execute(&harness.pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 0, ?4, ?5, 'OPEN', '2026-01-01', '2026-01-01')
            "#,
        )
        .bind(&rec_id)
        .bind(&harness.tenant_a_id)
        .bind(&inv_id)
        .bind(amount)
        .bind(&due_date)
        .execute(&harness.pool)
        .await
        .unwrap();
    }

    // Also add:
    // 10. VOIDED invoice with overdue date (day -100): 5,000,000 -> MUST BE EXCLUDED
    let void_inv_id = "inv_voided_aged";
    let void_rec_id = "rec_voided_aged";
    let void_due = (today - Duration::days(100)).format("%Y-%m-%d").to_string();
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, customer_name, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, balance_due, status, created_at, updated_at) VALUES (?1, ?2, 'Void Client', ?3, 'IDR', 'NONE', 5000000, 0, 0, 5000000, 0, 'VOIDED', '2026-01-01', '2026-01-01')"
    )
    .bind(void_inv_id)
    .bind(&harness.tenant_a_id)
    .bind(&void_due)
    .execute(&harness.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at) VALUES (?1, ?2, ?3, 5000000, 0, 0, ?4, 'VOIDED', '2026-01-01', '2026-01-01')"
    )
    .bind(void_rec_id)
    .bind(&harness.tenant_a_id)
    .bind(void_inv_id)
    .bind(&void_due)
    .execute(&harness.pool)
    .await
    .unwrap();

    // 11. PAID invoice with overdue date (day -100): 3,000,000 -> MUST BE EXCLUDED (outstanding = 0)
    let paid_inv_id = "inv_paid_aged";
    let paid_rec_id = "rec_paid_aged";
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, customer_name, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, balance_due, status, created_at, updated_at) VALUES (?1, ?2, 'Paid Client', ?3, 'IDR', 'NONE', 3000000, 0, 0, 3000000, 0, 'PAID', '2026-01-01', '2026-01-01')"
    )
    .bind(paid_inv_id)
    .bind(&harness.tenant_a_id)
    .bind(&void_due)
    .execute(&harness.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at) VALUES (?1, ?2, ?3, 3000000, 3000000, 0, ?4, 'PAID', '2026-01-01', '2026-01-01')"
    )
    .bind(paid_rec_id)
    .bind(&harness.tenant_a_id)
    .bind(paid_inv_id)
    .bind(&void_due)
    .execute(&harness.pool)
    .await
    .unwrap();

    // 12. Tenant B overdue invoice: 10,000,000 -> MUST BE EXCLUDED (cross-tenant)
    let b_inv_id = "inv_tenant_b_aged";
    let b_rec_id = "rec_tenant_b_aged";
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, customer_name, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, balance_due, status, created_at, updated_at) VALUES (?1, ?2, 'Tenant B Client', ?3, 'IDR', 'NONE', 10000000, 0, 0, 10000000, 10000000, 'ISSUED', '2026-01-01', '2026-01-01')"
    )
    .bind(b_inv_id)
    .bind(&harness.tenant_b_id)
    .bind(&void_due)
    .execute(&harness.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at) VALUES (?1, ?2, ?3, 10000000, 0, 10000000, ?4, 'OPEN', '2026-01-01', '2026-01-01')"
    )
    .bind(b_rec_id)
    .bind(&harness.tenant_b_id)
    .bind(b_inv_id)
    .bind(&void_due)
    .execute(&harness.pool)
    .await
    .unwrap();

    // Request Aging Report for Tenant A
    let aging_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/receivables/aging")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(aging_req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);

    // Exact Bucket Assertions:
    // current_0_30 = 100k + 200k + 300k = 600,000
    assert_eq!(body["current_0_30"], 600_000, "current_0_30 bucket mismatch");

    // overdue_31_60 = 400k + 500k = 900,000
    assert_eq!(body["overdue_31_60"], 900_000, "overdue_31_60 bucket mismatch");

    // overdue_61_90 = 600k + 700k = 1,300,000
    assert_eq!(body["overdue_61_90"], 1_300_000, "overdue_61_90 bucket mismatch");

    // overdue_90_plus = 800k + 900k = 1,700,000 (excluding 5M voided, 3M paid, 10M Tenant B)
    assert_eq!(body["overdue_90_plus"], 1_700_000, "overdue_90_plus bucket mismatch");

    // total_outstanding = 600k + 900k + 1.3M + 1.7M = 4,500,000
    assert_eq!(body["total_outstanding"], 4_500_000, "total_outstanding mismatch");
}

// ============================================================================
// 5. DATABASE IMMUTABILITY TRIGGERS UNDER DIRECT SQL ATTACK
// ============================================================================

#[tokio::test]
async fn test_adversarial_database_triggers_prevent_all_unauthorized_tampering() {
    let harness = setup_harness().await;
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;

    // Trigger 8.1: Attempt to alter customer name on issued invoice via direct SQL
    let update_res = sqlx::query("UPDATE invoices SET customer_name = 'Fraudulent Name' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(update_res.is_err(), "Trigger must block altering customer_name on issued invoice");

    // Trigger 8.1: Attempt to alter subtotal on issued invoice
    let update_sub = sqlx::query("UPDATE invoices SET subtotal = 1 WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(update_sub.is_err(), "Trigger must block altering subtotal on issued invoice");

    // Trigger 8.2: Attempt to DELETE issued invoice directly
    let del_inv = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(del_inv.is_err(), "Trigger must block direct deletion of issued invoice");

    // Trigger 8.3 & 8.4: Attempt to UPDATE or DELETE invoice line items of issued invoice
    let update_item = sqlx::query("UPDATE invoice_items SET unit_price = 1 WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(update_item.is_err(), "Trigger must block updating items of issued invoice");

    let del_item = sqlx::query("DELETE FROM invoice_items WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(del_item.is_err(), "Trigger must block deleting items of issued invoice");

    // Trigger 8.5 & 8.6: Attempt to UPDATE or DELETE invoice snapshot
    let update_snap = sqlx::query("UPDATE invoice_snapshots SET snapshot_json = '{}' WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(update_snap.is_err(), "Trigger must block updating invoice snapshot");

    let del_snap = sqlx::query("DELETE FROM invoice_snapshots WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(del_snap.is_err(), "Trigger must block deleting invoice snapshot");

    // Now record payment
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 400_000 }).to_string()))
        .unwrap();
    let (sp, bp, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(sp, StatusCode::CREATED);
    let pay_id = bp["id"].as_str().unwrap();

    // Trigger 8.7: Attempt to DELETE confirmed payment
    let del_pay = sqlx::query("DELETE FROM payments WHERE id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(del_pay.is_err(), "Trigger must block deleting confirmed payment");

    // Trigger 8.8: Attempt to UPDATE amount on confirmed payment
    let mod_pay = sqlx::query("UPDATE payments SET amount = 999999 WHERE id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(mod_pay.is_err(), "Trigger must block altering amount on confirmed payment");

    // Trigger 8.9 & 8.10: Attempt to UPDATE or DELETE payment allocation
    let mod_alloc = sqlx::query("UPDATE payment_allocations SET amount = 1 WHERE payment_id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(mod_alloc.is_err(), "Trigger must block updating payment allocation");

    let del_alloc = sqlx::query("DELETE FROM payment_allocations WHERE payment_id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(del_alloc.is_err(), "Trigger must block deleting payment allocation");
}

// ============================================================================
// 6. DOUBLE-ENTRY FINANCIAL INVARIANTS ON PAYMENTS
// ============================================================================

#[tokio::test]
async fn test_adversarial_double_entry_payment_ledger_invariants() {
    let harness = setup_harness().await;

    // Issue invoice: 500,000 total
    let inv_id = create_issued_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, 500_000).await;

    // Allocate payment: 200,000 via BANK_TRANSFER -> Debit 1100 (Bank), Credit 1200 (AR)
    let pay1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 200_000, "payment_method": "BANK_TRANSFER" }).to_string()))
        .unwrap();
    let (s1, _, _) = parse_response(harness.app.clone().oneshot(pay1).await.unwrap()).await;
    assert_eq!(s1, StatusCode::CREATED);

    // Allocate payment: 300,000 via CASH -> Debit 1000 (Kas), Credit 1200 (AR)
    let pay2 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 300_000, "payment_method": "CASH" }).to_string()))
        .unwrap();
    let (s2, _, _) = parse_response(harness.app.clone().oneshot(pay2).await.unwrap()).await;
    assert_eq!(s2, StatusCode::CREATED);

    // Verify all journal entries in the database satisfy SUM(debit) == SUM(credit)
    let unbalanced_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM (
            SELECT journal_id, SUM(debit) as total_debit, SUM(credit) as total_credit
            FROM journal_lines
            GROUP BY journal_id
            HAVING total_debit != total_credit
        )
        "#,
    )
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(unbalanced_count, 0, "All journal entries must strictly balance (SUM debit == SUM credit)");

    // Verify specific account balances for Tenant A:
    // Debit 1200 (Invoice): +500,000
    // Credit 4000 (Revenue): +500,000
    // Debit 1100 (Bank): +200,000
    // Credit 1200 (AR settlement 1): -200,000
    // Debit 1000 (Kas): +300,000
    // Credit 1200 (AR settlement 2): -300,000
    // Net AR (1200) = 500,000 - 500,000 = 0!
    let ar_balance: (i64, i64) = sqlx::query_as(
        r#"
        SELECT COALESCE(SUM(jl.debit), 0), COALESCE(SUM(jl.credit), 0)
        FROM journal_lines jl
        JOIN journal_entries je ON je.id = jl.journal_id
        WHERE je.tenant_id = ?1 AND jl.account_code = '1200'
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(ar_balance.0, 500_000, "Total debit to 1200 must be 500,000");
    assert_eq!(ar_balance.1, 500_000, "Total credit to 1200 must be 500,000");
    assert_eq!(ar_balance.0 - ar_balance.1, 0, "Net AR balance must be 0 after full settlement");

    // Bank (1100) debit = 200,000
    let bank_debit: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.debit), 0)
        FROM journal_lines jl
        JOIN journal_entries je ON je.id = jl.journal_id
        WHERE je.tenant_id = ?1 AND jl.account_code = '1100'
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(bank_debit, 200_000);

    // Kas (1000) debit = 300,000
    let kas_debit: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(jl.debit), 0)
        FROM journal_lines jl
        JOIN journal_entries je ON je.id = jl.journal_id
        WHERE je.tenant_id = ?1 AND jl.account_code = '1000'
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(kas_debit, 300_000);
}
