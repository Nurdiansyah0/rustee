//! Final Milestone Phase 2 Adversarial Coverage Hardening Verification Suite 2
//!
//! Identity: teamwork_preview_challenger_final_2
//! Role: Final Milestone Adversarial Challenger 2 (critic, specialist)
//! Milestone: Final Milestone Phase 2 Adversarial Coverage Hardening
//!
//! Empirical Verification Objectives:
//! 1. Idempotency & Replay Attack Resilience:
//!    - Identical payload replay returns cached response with `x-cache-replay: true` header.
//!    - Modified payload replay with identical Idempotency-Key strictly returns HTTP 409 Conflict
//!      with code `IDEMPOTENCY_KEY_MISMATCH`.
//!    - Concurrent replay race produces exactly one entity with zero duplicates.
//!    - Cross-tenant/user key isolation prevents collision.
//!    - Payment allocation idempotency prevents double deducting receivable balances.
//! 2. Indonesian Tax Engine & Rounding Invariants:
//!    - PPN 11% and 12% Exclusive: deterministic statutory half-up rounding (round_half_up_i128).
//!    - PPN 11% and 12% Inclusive: tax extraction preserves gross invariant (net + tax == gross).
//!    - UMKM 0.5% (50 bps) final tax: deterministic half-up rounding, turnover thresholds, and extreme amounts.
//!    - Complete Invoice-to-GL lifecycle: verified journal debit == credit balancing with Utang Pajak (2100).

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::accounting::calculate_tax_integer;
use backend::domain::tenant::{Role, TenantStatus};
use backend::repository::{
    account_repo::SqlxAccountRepository,
    accounting_repo::SqlxAccountingRepository,
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
use sqlx::SqlitePool;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::task::JoinSet;
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: axum::Router,
    pool: SqlitePool,
    _dir: tempfile::TempDir,
    owner_token_a: String,
    tenant_a_id: String,
    owner_token_b: String,
    tenant_b_id: String,
}

async fn setup_challenger_2_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("final_challenger_2.sqlite");
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

    let jwt_secret = "challenger_2_jwt_secret_key_1234567890_enterprise";
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

    // 1. Create Tenant A
    let owner_a_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_a_id.clone(),
            email: "owner_a_c2@final.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A C2".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token_a, _) = jwt_engine
        .generate_token(&owner_a_id, "owner_a_c2@final.com", "user", "premium")
        .unwrap();

    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Final Workspace A C2".to_string(),
            slug: "final-workspace-a-c2".to_string(),
            status: Some(TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_a = pool.begin().await.unwrap();
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx_a, &tenant_a_id)
        .await
        .unwrap();
    tx_a.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: owner_a_id,
            role: Role::Owner,
        })
        .await
        .unwrap();

    // 2. Create Tenant B
    let owner_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_b_id.clone(),
            email: "owner_b_c2@final.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B C2".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token_b, _) = jwt_engine
        .generate_token(&owner_b_id, "owner_b_c2@final.com", "user", "premium")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Final Workspace B C2".to_string(),
            slug: "final-workspace-b-c2".to_string(),
            status: Some(TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_b = pool.begin().await.unwrap();
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx_b, &tenant_b_id)
        .await
        .unwrap();
    tx_b.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: owner_b_id,
            role: Role::Owner,
        })
        .await
        .unwrap();

    TestHarness {
        app,
        pool,
        _dir: dir,
        owner_token_a,
        tenant_a_id,
        owner_token_b,
        tenant_b_id,
    }
}

async fn send_req_with_idem(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: Option<&str>,
    idem_key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header(CONTENT_TYPE, "application/json");

    if let Some(t_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", t_id);
    }

    if let Some(key) = idem_key {
        builder = builder.header("Idempotency-Key", key);
    }

    let req_body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };

    let req = builder.body(req_body).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body_json: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };
    (status, headers, body_json)
}

// =============================================================================
// TEST 1: Identical Payload Idempotency Replay (Invoices & Payments)
// =============================================================================
#[tokio::test]
async fn test_final_idempotency_replay_identical_returns_cached_response() {
    let h = setup_challenger_2_harness().await;

    let invoice_payload = json!({
        "customer_name": "PT Sinar Idemp",
        "customer_email": "sinar@idemp.com",
        "due_date": "2026-11-01T00:00:00Z",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Enterprise Cloud License",
                "quantity": 1,
                "unit_price": 500_000,
                "discount": 0
            }
        ]
    });

    let idem_key = "IDEM-INV-IDENTICAL-001";

    // Request 1: Initial Creation
    let (status1, headers1, body1) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(idem_key),
        Some(invoice_payload.clone()),
    )
    .await;

    assert_eq!(status1, StatusCode::CREATED, "First invoice creation must return 201 Created");
    assert!(!headers1.contains_key("x-cache-replay"), "First response must not have x-cache-replay header");
    let invoice_id = body1["id"].as_str().expect("Must have invoice ID").to_string();
    assert_eq!(body1["subtotal"], 500_000);
    assert_eq!(body1["tax_amount"], 55_000);
    assert_eq!(body1["total_amount"], 555_000);

    // Request 2: Identical Replay Attack
    let (status2, headers2, body2) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(idem_key),
        Some(invoice_payload),
    )
    .await;

    assert_eq!(status2, StatusCode::CREATED, "Replayed invoice creation must return 201 Created");
    assert_eq!(
        headers2.get("x-cache-replay").map(|v| v.to_str().unwrap()),
        Some("true"),
        "Replayed response must include x-cache-replay: true header"
    );
    assert_eq!(body2["id"], invoice_id, "Replayed response must return exact same invoice ID");
    assert_eq!(body2["total_amount"], 555_000);

    // Database Invariant Check: Exactly 1 invoice in DB, zero duplicate records
    let inv_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices WHERE tenant_id = ?1")
        .bind(&h.tenant_a_id)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(inv_count, 1, "Exactly 1 invoice record must exist in DB; replay must not duplicate");

    // Issue invoice to open receivable for payment idempotency test
    let (issue_status, _, _) = send_req_with_idem(
        &h.app,
        Method::POST,
        &format!("/api/v1/invoices/{}/issue", invoice_id),
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        None,
    )
    .await;
    assert_eq!(issue_status, StatusCode::OK, "Invoice issue must succeed");

    // Payment Idempotency Replay Test
    let payment_payload = json!({
        "invoice_id": invoice_id,
        "amount": 200_000,
        "payment_method": "BANK_TRANSFER",
        "payment_date": "2026-10-02T00:00:00Z",
        "reference": "TRX-IDEM-01"
    });

    let pay_key = "IDEM-PAY-IDENTICAL-002";

    // Payment 1: Allocate partial payment
    let (pay_status1, pay_hdr1, pay_body1) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/payments",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(pay_key),
        Some(payment_payload.clone()),
    )
    .await;
    assert_eq!(pay_status1, StatusCode::CREATED, "First payment allocation must return 201 Created");
    assert!(!pay_hdr1.contains_key("x-cache-replay"));
    let payment_id = pay_body1["id"].as_str().unwrap().to_string();

    // Payment 2: Replay exact payment with same Idempotency-Key
    let (pay_status2, pay_hdr2, pay_body2) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/payments",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(pay_key),
        Some(payment_payload),
    )
    .await;
    assert_eq!(pay_status2, StatusCode::CREATED, "Replayed payment must return 201 Created");
    assert_eq!(
        pay_hdr2.get("x-cache-replay").map(|v| v.to_str().unwrap()),
        Some("true"),
        "Replayed payment must have x-cache-replay: true"
    );
    assert_eq!(pay_body2["id"], payment_id);

    // Verify Receivable Balance Invariant: exactly 200,000 allocated, NOT 400,000!
    let (allocated, outstanding): (i64, i64) = sqlx::query_as(
        "SELECT allocated_amount, outstanding_amount FROM receivables WHERE invoice_id = ?1",
    )
    .bind(&invoice_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(allocated, 200_000, "Receivable allocated amount must be exactly 200,000 (not double counted)");
    assert_eq!(outstanding, 355_000, "Receivable outstanding must be exactly 555,000 - 200,000 = 355,000");

    let pay_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE tenant_id = ?1")
        .bind(&h.tenant_a_id)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(pay_count, 1, "Exactly 1 payment record must exist in DB");
}

// =============================================================================
// TEST 2: Modified Payload Idempotency Replay -> HTTP 409 Conflict
// =============================================================================
#[tokio::test]
async fn test_final_idempotency_mismatched_payload_returns_409_conflict() {
    let h = setup_challenger_2_harness().await;

    let payload_original = json!({
        "customer_name": "Original Customer",
        "customer_email": "orig@test.com",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Original Item",
                "quantity": 1,
                "unit_price": 100_000,
                "discount": 0
            }
        ]
    });

    let idem_key = "IDEM-MISMATCH-KEY-888";

    // 1. Initial Creation
    let (status1, _, body1) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(idem_key),
        Some(payload_original),
    )
    .await;
    assert_eq!(status1, StatusCode::CREATED);
    let original_id = body1["id"].as_str().unwrap().to_string();

    // 2. Adversarial Replay with MODIFIED unit_price (tampered payload)
    let payload_tampered_price = json!({
        "customer_name": "Original Customer",
        "customer_email": "orig@test.com",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Original Item",
                "quantity": 1,
                "unit_price": 200_000, // Modified!
                "discount": 0
            }
        ]
    });

    let (status2, _, body2) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(idem_key),
        Some(payload_tampered_price),
    )
    .await;

    assert_eq!(
        status2,
        StatusCode::CONFLICT,
        "Tampered payload with existing idempotency key must return HTTP 409 Conflict"
    );
    assert_eq!(
        body2["code"], "IDEMPOTENCY_KEY_MISMATCH",
        "RFC 7807 code must be IDEMPOTENCY_KEY_MISMATCH"
    );
    assert!(
        body2["detail"]
            .as_str()
            .unwrap()
            .contains("Idempotency key reused with different request payload"),
        "Detail must describe payload mismatch verbatim"
    );

    // 3. Adversarial Replay with MODIFIED customer_name
    let payload_tampered_name = json!({
        "customer_name": "Different Customer Infiltrator",
        "customer_email": "orig@test.com",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Original Item",
                "quantity": 1,
                "unit_price": 100_000,
                "discount": 0
            }
        ]
    });

    let (status3, _, body3) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(idem_key),
        Some(payload_tampered_name),
    )
    .await;

    assert_eq!(status3, StatusCode::CONFLICT);
    assert_eq!(body3["code"], "IDEMPOTENCY_KEY_MISMATCH");

    // Verify DB integrity: Only the original invoice exists
    let inv_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT id, subtotal FROM invoices WHERE tenant_id = ?1",
    )
    .bind(&h.tenant_a_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();

    assert_eq!(inv_rows.len(), 1, "Only 1 invoice must exist in database");
    assert_eq!(inv_rows[0].0, original_id);
    assert_eq!(inv_rows[0].1, 100_000, "Subtotal must remain original 100,000");
}

// =============================================================================
// TEST 3: Concurrent Replay Race Condition
// =============================================================================
#[tokio::test]
async fn test_final_idempotency_concurrent_replay_race() {
    let h = setup_challenger_2_harness().await;

    let invoice_payload = json!({
        "customer_name": "PT Concurrency Race",
        "due_date": "2026-11-01T00:00:00Z",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Concurrent Service Item",
                "quantity": 2,
                "unit_price": 75_000,
                "discount": 0
            }
        ]
    });

    let shared_idem_key = "IDEM-CONCURRENCY-RACE-999";
    let concurrency_count = 8;
    let mut tasks = JoinSet::new();

    for _ in 0..concurrency_count {
        let app = h.app.clone();
        let token = h.owner_token_a.clone();
        let tenant = h.tenant_a_id.clone();
        let payload = invoice_payload.clone();
        let key = shared_idem_key.to_string();

        tasks.spawn(async move {
            send_req_with_idem(
                &app,
                Method::POST,
                "/api/v1/invoices",
                &token,
                Some(&tenant),
                Some(&key),
                Some(payload),
            )
            .await
        });
    }

    let mut success_201_count = 0;
    let mut conflict_409_in_progress_count = 0;
    let mut created_invoice_ids = Vec::new();

    while let Some(res) = tasks.join_next().await {
        let (status, headers, body) = res.unwrap();
        if status == StatusCode::CREATED {
            success_201_count += 1;
            if let Some(id) = body["id"].as_str() {
                created_invoice_ids.push(id.to_string());
            }
            if headers.contains_key("x-cache-replay") {
                assert_eq!(headers.get("x-cache-replay").unwrap(), "true");
            }
        } else if status == StatusCode::CONFLICT {
            conflict_409_in_progress_count += 1;
            assert_eq!(body["code"], "IDEMPOTENCY_IN_PROGRESS");
        } else {
            panic!("Unexpected response status in concurrent race: {}", status);
        }
    }

    assert_eq!(
        success_201_count + conflict_409_in_progress_count,
        concurrency_count,
        "All requests must either succeed with 201 or return 409 conflict"
    );

    // Crucial Invariant: All successful 201 responses must point to the EXACT SAME invoice ID
    assert!(success_201_count >= 1, "At least one request must succeed with 201 Created");
    let first_id = &created_invoice_ids[0];
    for id in &created_invoice_ids {
        assert_eq!(id, first_id, "All successful 201 responses must return the identical entity ID");
    }

    // Database Invariant: Exactly 1 invoice stored in DB
    let db_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices WHERE tenant_id = ?1")
        .bind(&h.tenant_a_id)
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(db_count, 1, "Exactly 1 invoice must exist in DB under race condition");
}

// =============================================================================
// TEST 4: Multi-Tenant Idempotency Key Isolation
// =============================================================================
#[tokio::test]
async fn test_final_idempotency_tenant_isolation() {
    let h = setup_challenger_2_harness().await;

    let shared_key = "IDEM-COMMON-ACROSS-TENANTS-007";

    // Tenant A creates invoice with key
    let payload_a = json!({
        "customer_name": "Tenant A Customer",
        "tax_type": "PPN_11_EXCL",
        "items": [{ "description": "Item A", "quantity": 1, "unit_price": 100_000, "discount": 0 }]
    });

    let (status_a, _, body_a) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(shared_key),
        Some(payload_a),
    )
    .await;
    assert_eq!(status_a, StatusCode::CREATED);
    let id_a = body_a["id"].as_str().unwrap().to_string();

    // Tenant B uses the EXACT SAME key with their OWN payload
    let payload_b = json!({
        "customer_name": "Tenant B Customer",
        "tax_type": "PPN_12_EXCL",
        "items": [{ "description": "Item B", "quantity": 1, "unit_price": 250_000, "discount": 0 }]
    });

    let (status_b, _, body_b) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_b,
        Some(&h.tenant_b_id),
        Some(shared_key),
        Some(payload_b),
    )
    .await;

    // Tenant B must NOT be blocked or receive 409 conflict from Tenant A's key
    assert_eq!(
        status_b,
        StatusCode::CREATED,
        "Tenant B must successfully create invoice even if using same key as Tenant A"
    );
    let id_b = body_b["id"].as_str().unwrap().to_string();
    assert_ne!(id_a, id_b, "Tenant A and Tenant B must have distinct invoice records");
    assert_eq!(body_b["customer_name"], "Tenant B Customer");
}

// =============================================================================
// TEST 5: Indonesian Tax Engine - Unit Vectors & Statutory Rounding Invariants
// =============================================================================
#[tokio::test]
async fn test_final_indonesian_tax_engine_rounding_and_statutory_invariants() {
    // 1. PPN 11% Exclusive: Statutory Half-Up Rounding
    // Standard IDR 100,000 -> Tax 11,000, Gross 111,000
    let t1 = calculate_tax_integer(100_000, "PPN_11_EXCL", false).unwrap();
    assert_eq!(t1.tax_amount, 11_000);
    assert_eq!(t1.net_amount, 100_000);
    assert_eq!(t1.gross_amount, 111_000);

    // Half-up boundary test: IDR 105 * 11 / 100 = 11.55 -> rounds half up to 12!
    let t2 = calculate_tax_integer(105, "PPN_11_EXCL", false).unwrap();
    assert_eq!(t2.tax_amount, 12);
    assert_eq!(t2.gross_amount, 117);

    // Rounding down boundary test: IDR 4 * 11 / 100 = 0.44 -> rounds to 0!
    let t3 = calculate_tax_integer(4, "PPN_11_EXCL", false).unwrap();
    assert_eq!(t3.tax_amount, 0);
    assert_eq!(t3.gross_amount, 4);

    // Exact 0.50 boundary test: IDR 5 * 11 / 100 = 0.55 -> rounds to 1!
    let t4 = calculate_tax_integer(5, "PPN_11_EXCL", false).unwrap();
    assert_eq!(t4.tax_amount, 1);
    assert_eq!(t4.gross_amount, 6);

    // IDR 999,999 * 11 / 100 = 109,999.89 -> rounds to 110,000!
    let t5 = calculate_tax_integer(999_999, "PPN_11_EXCL", false).unwrap();
    assert_eq!(t5.tax_amount, 110_000);
    assert_eq!(t5.gross_amount, 1_109_999);

    // 2. PPN 11% Inclusive: Gross Invariant (net + tax == gross strictly preserved)
    let gross_cases_11 = vec![111_000, 100_000, 50_000, 1, 999_999, 10_000_000];
    for gross in gross_cases_11 {
        let res = calculate_tax_integer(gross, "PPN_11_INCL", true).unwrap();
        assert_eq!(
            res.gross_amount, gross,
            "Inclusive gross_amount must strictly equal input amount"
        );
        assert_eq!(
            res.net_amount + res.tax_amount,
            gross,
            "Zero floating drift invariant: net_amount + tax_amount == gross_amount must strictly hold for gross = {}",
            gross
        );
    }
    // Specific inclusive vector: Gross 111,000 -> Tax 11,000, Net 100,000
    let t_incl = calculate_tax_integer(111_000, "PPN_11_INCL", true).unwrap();
    assert_eq!(t_incl.tax_amount, 11_000);
    assert_eq!(t_incl.net_amount, 100_000);

    // 3. PPN 12% Exclusive & Inclusive (Harmonisasi Peraturan Perpajakan / UU HPP)
    let t_12_excl = calculate_tax_integer(100_000, "PPN_12_EXCL", false).unwrap();
    assert_eq!(t_12_excl.tax_amount, 12_000);
    assert_eq!(t_12_excl.gross_amount, 112_000);

    let t_12_incl = calculate_tax_integer(112_000, "PPN_12_INCL", true).unwrap();
    assert_eq!(t_12_incl.tax_amount, 12_000);
    assert_eq!(t_12_incl.net_amount, 100_000);
    assert_eq!(t_12_incl.gross_amount, 112_000);

    // PPN 12% inclusive rounding invariant across multiple points
    let gross_cases_12 = vec![112_000, 100_000, 45_000, 1, 5_555_555];
    for gross in gross_cases_12 {
        let res = calculate_tax_integer(gross, "PPN_12_INCL", true).unwrap();
        assert_eq!(
            res.net_amount + res.tax_amount,
            gross,
            "PPN 12% Inclusive invariant violated for gross = {}",
            gross
        );
    }

    // 4. UMKM 0.5% (50 basis points) Final Tax (PP 23/2018 & PP 55/2022)
    // IDR 1,000,000 turnover -> tax: 5,000
    let u1 = calculate_tax_integer(1_000_000, "UMKM_05", false).unwrap();
    assert_eq!(u1.tax_amount, 5_000);
    assert_eq!(u1.base_amount, 1_000_000);

    // Boundary at IDR 100: 100 * 50 / 10000 = 0.50 -> rounds half up to 1!
    let u2 = calculate_tax_integer(100, "UMKM_05", false).unwrap();
    assert_eq!(u2.tax_amount, 1);

    // Boundary at IDR 99: 99 * 50 / 10000 = 0.495 -> rounds to 0!
    let u3 = calculate_tax_integer(99, "UMKM_05", false).unwrap();
    assert_eq!(u3.tax_amount, 0);

    // Statutory UMKM revenue threshold (Rp 4.8 Miliar): tax = 24,000,000
    let u4 = calculate_tax_integer(4_800_000_000, "UMKM_05", false).unwrap();
    assert_eq!(u4.tax_amount, 24_000_000);

    // Extreme quadrillion arithmetic: 9 quadrillion IDR turnover
    let u5 = calculate_tax_integer(9_000_000_000_000_000, "UMKM_05", false).unwrap();
    assert_eq!(u5.tax_amount, 45_000_000_000_000);

    // Negative turnover rejection
    assert!(calculate_tax_integer(-100, "UMKM_05", false).is_err());
}

// =============================================================================
// TEST 6: Complete Invoice-to-GL General Ledger Tax Lifecycle Invariants
// =============================================================================
#[tokio::test]
async fn test_final_invoice_to_gl_tax_lifecycle_and_exact_balance() {
    let h = setup_challenger_2_harness().await;

    // SCENARIO A: Issue Invoice with PPN 11% Exclusive
    let inv_excl_payload = json!({
        "customer_name": "PT Maju Logistik",
        "due_date": "2026-11-15T00:00:00Z",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Logistics Dispatch Core",
                "quantity": 1,
                "unit_price": 100_000,
                "discount": 0
            }
        ]
    });

    let (status_a, _, body_a) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        Some(inv_excl_payload),
    )
    .await;
    assert_eq!(status_a, StatusCode::CREATED);
    let inv_a_id = body_a["id"].as_str().unwrap().to_string();
    assert_eq!(body_a["subtotal"], 100_000);
    assert_eq!(body_a["tax_amount"], 11_000);
    assert_eq!(body_a["total_amount"], 111_000);

    // Issue Invoice A
    let (issue_a_status, _, issue_a_body) = send_req_with_idem(
        &h.app,
        Method::POST,
        &format!("/api/v1/invoices/{}/issue", inv_a_id),
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        None,
    )
    .await;
    assert_eq!(issue_a_status, StatusCode::OK);
    assert_eq!(issue_a_body["status"], "ISSUED");

    // Verify General Ledger Journal Entry for Invoice A
    // Find journal entry created for Invoice A
    let journals: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, description, entry_number FROM journal_entries WHERE tenant_id = ?1 AND description LIKE '%Invoice%'",
    )
    .bind(&h.tenant_a_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();

    assert_eq!(journals.len(), 1, "Exactly 1 general ledger journal must be created upon invoice issue");
    let (journal_id, _, entry_number) = &journals[0];
    assert!(!entry_number.is_empty(), "Journal must have a sequential entry_number");

    // Check sum of debits and sum of credits for this journal
    let (total_debit, total_credit): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(debit), 0), COALESCE(SUM(credit), 0) FROM journal_lines WHERE journal_id = ?1",
    )
    .bind(journal_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(total_debit, 111_000, "Journal total debit must equal invoice total amount (111,000)");
    assert_eq!(total_credit, 111_000, "Journal total credit must equal invoice total amount (111,000)");

    // Inspect individual journal lines
    let lines: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ?1 ORDER BY account_code ASC",
    )
    .bind(journal_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();

    // Accounts:
    // 1200: Piutang Usaha -> Debit 111,000, Credit 0
    // 2100: Utang Pajak -> Debit 0, Credit 11,000
    // 4000: Pendapatan Usaha -> Debit 0, Credit 100,000
    assert_eq!(lines.len(), 3, "Journal must contain exactly 3 lines: Piutang, Utang Pajak, Pendapatan");
    assert_eq!(lines[0], ("1200".to_string(), 111_000, 0), "Piutang Usaha (1200) line matches");
    assert_eq!(lines[1], ("2100".to_string(), 0, 11_000), "Utang Pajak (2100) line matches");
    assert_eq!(lines[2], ("4000".to_string(), 0, 100_000), "Pendapatan Usaha (4000) line matches");

    // SCENARIO B: Issue Invoice with PPN 11% Inclusive
    let inv_incl_payload = json!({
        "customer_name": "PT Ritel Prima",
        "due_date": "2026-11-20T00:00:00Z",
        "tax_type": "PPN_11_INCL",
        "items": [
            {
                "description": "Retail Inclusive Package",
                "quantity": 1,
                "unit_price": 111_000,
                "discount": 0
            }
        ]
    });

    let (status_b, _, body_b) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        Some(inv_incl_payload),
    )
    .await;
    assert_eq!(status_b, StatusCode::CREATED);
    let inv_b_id = body_b["id"].as_str().unwrap().to_string();
    assert_eq!(body_b["subtotal"], 111_000);
    assert_eq!(body_b["tax_amount"], 11_000);
    assert_eq!(body_b["total_amount"], 111_000);

    // Issue Invoice B
    let (issue_b_status, _, _) = send_req_with_idem(
        &h.app,
        Method::POST,
        &format!("/api/v1/invoices/{}/issue", inv_b_id),
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        None,
    )
    .await;
    assert_eq!(issue_b_status, StatusCode::OK);

    // SCENARIO C: Issue Invoice with PPN 12% Exclusive
    let inv_12_payload = json!({
        "customer_name": "PT Teknologi Maju 2025",
        "due_date": "2026-11-25T00:00:00Z",
        "tax_type": "PPN_12_EXCL",
        "items": [
            {
                "description": "Software Subscription 2025",
                "quantity": 1,
                "unit_price": 50_000,
                "discount": 0
            }
        ]
    });

    let (status_c, _, body_c) = send_req_with_idem(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        Some(inv_12_payload),
    )
    .await;
    assert_eq!(status_c, StatusCode::CREATED);
    let inv_c_id = body_c["id"].as_str().unwrap().to_string();
    assert_eq!(body_c["subtotal"], 50_000);
    assert_eq!(body_c["tax_amount"], 6_000); // 50,000 * 12% = 6,000
    assert_eq!(body_c["total_amount"], 56_000);

    // Issue Invoice C
    let (issue_c_status, _, _) = send_req_with_idem(
        &h.app,
        Method::POST,
        &format!("/api/v1/invoices/{}/issue", inv_c_id),
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        None,
    )
    .await;
    assert_eq!(issue_c_status, StatusCode::OK);

    // Query Trial Balance across all accounts in Tenant A
    let (tb_status, _, tb_body) = send_req_with_idem(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
        None,
    )
    .await;

    assert_eq!(tb_status, StatusCode::OK);
    assert_eq!(
        tb_body["is_balanced"], true,
        "Trial balance across all invoice tax journals must be strictly balanced"
    );
    assert_eq!(
        tb_body["net_balance"], 0,
        "Net trial balance variance must be exactly 0 Rupiah"
    );
    assert_eq!(
        tb_body["total_debit"], tb_body["total_credit"],
        "Total debits must strictly equal total credits across all invoice tax GL postings"
    );
}
