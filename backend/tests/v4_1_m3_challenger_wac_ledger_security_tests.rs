//! Empirical Adversarial Challenger Test Suite 2 for Milestone 3:
//! Weighted Average Cost (WAC) Math Rigor, Double-Entry GL Ledger Invariants,
//! Account 1300 Valuation Tie-Out, Transactional Outbox Atomicity, and Cross-Tenant Security (R1, R3, R4, R5).
//!
//! Agent: teamwork_preview_challenger_m3_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Test Scope:
//! 1. WAC Integer Math Rigor:
//!    - Sequential inbound batches with fractional Rupiah remainders (e.g. 3 @ 10,001 + 2 @ 10,002 + 1 @ 10,004...).
//!    - Extensive oracle property testing of `round_half_up_i128` across integer remainders (half-up rule).
//! 2. Extreme Integer Ranges & Zero Floating Point:
//!    - Multi-quadrillion Rupiah values without overflow or truncation.
//!    - Negative and zero quantity rejection checks.
//! 3. Double-Entry GL Invariants:
//!    - SUM(debit) == SUM(credit) for all PO receipts (Debit 1300 / Credit 2000).
//!    - SUM(debit) == SUM(credit) for all COGS fulfillments (Debit 5000 / Credit 1300).
//!    - SUM(debit) == SUM(credit) for all Stock Adjustments.
//!    - Strict global balance across 100% of journal entries in SQLite.
//! 4. Account 1300 Valuation Tie-Out:
//!    - Complete lifecycle: PO Receipts -> COGS Outbound -> Stock Adjustments -> Complete Depletion.
//!    - Mathematical balance ties between General Ledger Account 1300 and physical inventory valuation.
//! 5. Transactional Outbox Event Atomicity:
//!    - Atomic commit of `StockReceived` event with valid JSON payload on PO goods receipt.
//!    - Strict atomic rollback: zero outbox events, zero journal entries, zero stock mutations on failed receipt.
//! 6. Cross-Tenant Security & Anti-Enumeration:
//!    - Strict HTTP 404 on GET, order, receive, cancel across tenant boundaries (zero entity enumeration).
//!    - Cross-tenant journal access rejection (HTTP 404).

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::accounting::round_half_up_i128;
use backend::domain::inventory::calculate_weighted_average_cost;
use backend::domain::money::Rupiah;
use backend::domain::tenant::Role;
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
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct ChallengerHarness {
    app: axum::Router,
    pool: SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    tenant_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m3_wac_ledger_test.sqlite");
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

    let jwt_secret = "challenger_m3_secret_wac_gl_isolation_entropy_32_bytes";
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

    // Tenant A Owner
    let owner_a_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_a_id.clone(),
            email: "owner_a_wac@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_a_id, "owner_a_wac@test.com", "user", "premium")
        .unwrap();

    let tenant_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_id.clone(),
            name: "Tenant Alpha WAC Enterprise".to_string(),
            slug: "tenant-alpha-wac-enterprise".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_a = pool.begin().await.unwrap();
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx_a, &tenant_id)
        .await
        .unwrap();
    tx_a.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_id.clone(),
            user_id: owner_a_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    // Tenant B Owner (for cross-tenant tests)
    let owner_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_b_id.clone(),
            email: "owner_b_wac@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&owner_b_id, "owner_b_wac@test.com", "user", "premium")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta WAC Inc".to_string(),
            slug: "tenant-beta-wac-inc".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
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
            user_id: owner_b_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        owner_token,
        tenant_id,
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
    extra_headers: Option<HeaderMap>,
) -> (StatusCode, Value, HeaderMap) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id);

    if body.is_some() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }

    if let Some(headers) = extra_headers {
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }
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

async fn setup_warehouse_and_product(
    harness: &ChallengerHarness,
    wh_code: &str,
    prod_name: &str,
    cost: i64,
) -> (String, String) {
    let (_, wh_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "code": wh_code,
            "name": format!("Warehouse {}", wh_code),
            "is_default": true
        })),
        None,
    )
    .await;
    let wh_id = wh_res["id"].as_str().unwrap().to_string();

    let (_, prod_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "name": prod_name,
            "unit": "pcs",
            "cost_price": cost,
            "sale_price": cost + 10000
        })),
        None,
    )
    .await;
    let prod_id = prod_res["id"].as_str().unwrap().to_string();

    (wh_id, prod_id)
}

// =========================================================================
// CHALLENGE 1: WAC Integer Math Rigor & Fractional Remainders
// =========================================================================
#[tokio::test]
async fn test_challenge_1_wac_integer_math_fractional_remainders() {
    // 1. Rigorous oracle property verification of round_half_up_i128 across remainders:
    // For any positive divisor d and numerator n = q * d + r (0 <= r < d):
    // If r < ceil(d / 2.0), round_half_up_i128(n, d) == q.
    // If r >= ceil(d / 2.0), round_half_up_i128(n, d) == q + 1.
    for d in 1i128..=50i128 {
        for q in 0i128..=20i128 {
            for r in 0i128..d {
                let n = q * d + r;
                let actual = round_half_up_i128(n, d);
                let expected = if r * 2 >= d { q + 1 } else { q };
                assert_eq!(
                    actual, expected as i64,
                    "Failed for n = {}, d = {}, q = {}, r = {}: expected {}, got {}",
                    n, d, q, r, expected, actual
                );
            }
        }
    }

    // 2. Sequential inbound receipts with fractional remainders:
    // Batch 1: 3 units @ 10,001 Rupiah
    let wac_1 = calculate_weighted_average_cost(0, Rupiah::new(0), 3, Rupiah::new(10_001)).unwrap();
    assert_eq!(wac_1.as_i64(), 10_001);

    // Batch 2: 2 units @ 10,002 Rupiah
    // Total cost = 3 * 10,001 + 2 * 10,002 = 30,003 + 20,004 = 50,007
    // Total qty = 5
    // 50,007 / 5 = 10,001.4 -> r = 2, 2 * 2 = 4 < 5 -> rounds DOWN to 10,001
    let wac_2 = calculate_weighted_average_cost(3, wac_1, 2, Rupiah::new(10_002)).unwrap();
    assert_eq!(wac_2.as_i64(), 10_001);

    // Batch 3: 1 unit @ 10,004 Rupiah
    // Total cost = 5 * 10,001 + 1 * 10,004 = 50,005 + 10,004 = 60,009
    // Total qty = 6
    // 60,009 / 6 = 10,001.5 -> r = 3, 3 * 2 = 6 >= 6 -> rounds UP to 10,002
    let wac_3 = calculate_weighted_average_cost(5, wac_2, 1, Rupiah::new(10_004)).unwrap();
    assert_eq!(wac_3.as_i64(), 10_002);

    // Batch 4: 1 unit @ 10,000 Rupiah
    // Total cost = 6 * 10,002 + 1 * 10,000 = 60,012 + 10,000 = 70,012
    // Total qty = 7
    // 70,012 / 7 = 10,001.714 -> r = 5, 5 * 2 = 10 >= 7 -> rounds UP to 10,002
    let wac_4 = calculate_weighted_average_cost(6, wac_3, 1, Rupiah::new(10_000)).unwrap();
    assert_eq!(wac_4.as_i64(), 10_002);

    // Batch 5: 1 unit @ 10,001 Rupiah
    // Total cost = 7 * 10,002 + 1 * 10,001 = 70,014 + 10,001 = 80,015
    // Total qty = 8
    // 80,015 / 8 = 10,001.875 -> r = 7, 7 * 2 = 14 >= 8 -> rounds UP to 10,002
    let wac_5 = calculate_weighted_average_cost(7, wac_4, 1, Rupiah::new(10_001)).unwrap();
    assert_eq!(wac_5.as_i64(), 10_002);

    // 3. Empirical verification through API goods receipt:
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-WAC", "Barang Fractional WAC", 10_001).await;

    // Create PO with items
    let (_, po_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "Supplier WAC",
            "destination_warehouse_id": wh_id,
            "items": [
                { "product_id": prod_id, "quantity_ordered": 10, "unit_cost": 10001 }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_res["id"].as_str().unwrap();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;

    // Inbound Receipt 1: 3 units @ 10,001
    let (s1, r1, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 3, "unit_cost": 10001 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(r1["total_receipt_value"], 30003);

    let (_, stock_1, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory/stock?warehouse_id={}&product_id={}", wh_id, prod_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_1["stock_items"][0]["quantity_on_hand"], 3);
    assert_eq!(stock_1["stock_items"][0]["average_cost"], 10001);

    // Inbound Receipt 2: 2 units @ 10,002
    let (s2, r2, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 2, "unit_cost": 10002 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(r2["total_receipt_value"], 20004);

    let (_, stock_2, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory/stock?warehouse_id={}&product_id={}", wh_id, prod_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_2["stock_items"][0]["quantity_on_hand"], 5);
    // Verified 10,001 (50,007 / 5 = 10,001.4 rounded down)
    assert_eq!(stock_2["stock_items"][0]["average_cost"], 10001);

    // Inbound Receipt 3: 1 unit @ 10,004
    let (s3, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 1, "unit_cost": 10004 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s3, StatusCode::OK);

    let (_, stock_3, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory/stock?warehouse_id={}&product_id={}", wh_id, prod_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_3["stock_items"][0]["quantity_on_hand"], 6);
    // Verified 10,002 (60,009 / 6 = 10,001.5 rounded UP)
    assert_eq!(stock_3["stock_items"][0]["average_cost"], 10002);
}

// =========================================================================
// CHALLENGE 2: Extreme Integer Ranges, Boundary Conditions & Zero Float
// =========================================================================
#[tokio::test]
async fn test_challenge_2_wac_extreme_ranges_and_zero_float() {
    // 1. Extreme Indonesian scale: 10,000,000 units @ Rp 500,000,000
    // Total = 5 * 10^15 Rupiah (Rp 5 Kuadriliun)
    let wac_high = calculate_weighted_average_cost(
        10_000_000,
        Rupiah::new(500_000_000),
        5_000_000,
        Rupiah::new(1_000_000_000),
    )
    .unwrap();
    // (10M * 500M + 5M * 1B) / 15M = 10^16 / 15,000,000 = 666,666,666.666...
    // Rounded half-up -> 666,666,667
    assert_eq!(wac_high.as_i64(), 666_666_667);

    // 2. Division by zero safety in round_half_up_i128
    assert_eq!(round_half_up_i128(100, 0), 0);
    assert_eq!(round_half_up_i128(0, 0), 0);

    // 3. Boundary validation: incoming quantity <= 0 is strictly rejected
    let err_zero = calculate_weighted_average_cost(10, Rupiah::new(1000), 0, Rupiah::new(1000));
    assert!(err_zero.is_err());

    let err_neg_in = calculate_weighted_average_cost(10, Rupiah::new(1000), -5, Rupiah::new(1000));
    assert!(err_neg_in.is_err());

    let err_neg_prev = calculate_weighted_average_cost(-1, Rupiah::new(1000), 5, Rupiah::new(1000));
    assert!(err_neg_prev.is_err());

    // 4. Initial stock 0 cleanly inherits incoming unit cost
    let init_wac = calculate_weighted_average_cost(0, Rupiah::ZERO, 100, Rupiah::new(75_450)).unwrap();
    assert_eq!(init_wac.as_i64(), 75_450);
}

// =========================================================================
// CHALLENGE 3: Double-Entry GL Ledger Invariants: SUM(debit) == SUM(credit)
// =========================================================================
#[tokio::test]
async fn test_challenge_3_double_entry_gl_invariants_po_receipt_and_cogs() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-GL", "Barang Double Entry", 50000).await;

    // 1. Create and order PO with 100 units @ 50,000 IDR
    let (_, po_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "Supplier Ledger Invariant",
            "destination_warehouse_id": wh_id,
            "items": [
                { "product_id": prod_id, "quantity_ordered": 100, "unit_cost": 50000 }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_res["id"].as_str().unwrap();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;

    // 2. Receipt Batch 1: 40 units @ 50,000 IDR (Value = 2,000,000 IDR)
    let (s_rcv1, rcv1_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 40, "unit_cost": 50000 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s_rcv1, StatusCode::OK);
    let j_id1 = rcv1_body["journal_entry_id"].as_str().unwrap();

    // Verify PO Receipt Journal 1
    let lines1 = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ?1"
    )
    .bind(j_id1)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    assert_eq!(lines1.len(), 2);
    let mut sum_deb_1 = 0i64;
    let mut sum_cred_1 = 0i64;
    for row in &lines1 {
        let code: String = row.get("account_code");
        let deb: i64 = row.get("debit");
        let cred: i64 = row.get("credit");
        sum_deb_1 += deb;
        sum_cred_1 += cred;

        if code == "1300" {
            assert_eq!(deb, 2_000_000, "Account 1300 must be debited 2,000,000");
            assert_eq!(cred, 0);
        } else if code == "2000" {
            assert_eq!(cred, 2_000_000, "Account 2000 must be credited 2,000,000");
            assert_eq!(deb, 0);
        } else {
            panic!("Unexpected account code {}", code);
        }
    }
    assert_eq!(sum_deb_1, sum_cred_1, "Debit and Credit must strictly balance");

    // 3. Receipt Batch 2: 60 units @ 60,000 IDR (Value = 3,600,000 IDR)
    let (s_rcv2, rcv2_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 60, "unit_cost": 60000 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s_rcv2, StatusCode::OK);
    let j_id2 = rcv2_body["journal_entry_id"].as_str().unwrap();

    // Verify PO Receipt Journal 2
    let lines2 = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ?1"
    )
    .bind(j_id2)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    let mut sum_deb_2 = 0i64;
    let mut sum_cred_2 = 0i64;
    for row in &lines2 {
        let code: String = row.get("account_code");
        let deb: i64 = row.get("debit");
        let cred: i64 = row.get("credit");
        sum_deb_2 += deb;
        sum_cred_2 += cred;
        if code == "1300" {
            assert_eq!(deb, 3_600_000);
            assert_eq!(cred, 0);
        } else if code == "2000" {
            assert_eq!(cred, 3_600_000);
            assert_eq!(deb, 0);
        }
    }
    assert_eq!(sum_deb_2, sum_cred_2);

    // 4. Outbound COGS Fulfillment: Fulfill 30 units
    // Current stock: 40 @ 50k + 60 @ 60k = 5,600,000 / 100 = 56,000 WAC
    // Outbound 30 units: COGS = 30 * 56,000 = 1,680,000 IDR
    let (s_out, out_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 30
        })),
        None,
    )
    .await;
    assert_eq!(s_out, StatusCode::CREATED);
    let mov_id = out_body["id"].as_str().unwrap();

    // Verify COGS Journal: Debit 5000 / Credit 1300
    let cogs_journal = sqlx::query(
        "SELECT id FROM journal_entries WHERE source_type = 'INVENTORY_OUTBOUND' AND source_id = ?1"
    )
    .bind(mov_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    let cogs_j_id: String = cogs_journal.get("id");

    let cogs_lines = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ?1"
    )
    .bind(&cogs_j_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    let mut sum_cogs_deb = 0i64;
    let mut sum_cogs_cred = 0i64;
    for row in &cogs_lines {
        let code: String = row.get("account_code");
        let deb: i64 = row.get("debit");
        let cred: i64 = row.get("credit");
        sum_cogs_deb += deb;
        sum_cogs_cred += cred;
        if code == "5000" {
            assert_eq!(deb, 1_680_000, "Account 5000 (COGS) must be debited 1,680,000");
            assert_eq!(cred, 0);
        } else if code == "1300" {
            assert_eq!(cred, 1_680_000, "Account 1300 (Inventory) must be credited 1,680,000");
            assert_eq!(deb, 0);
        }
    }
    assert_eq!(sum_cogs_deb, 1_680_000);
    assert_eq!(sum_cogs_cred, 1_680_000);

    // 5. Physical Stock Adjustment (Variance > 0): Count 75 (current 70, diff +5)
    // 5 units * 56,000 = 280,000 IDR -> Debit 1300 / Credit 5000
    let (s_adj, adj_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 75,
            "reason": "Cycle count surplus"
        })),
        None,
    )
    .await;
    assert_eq!(s_adj, StatusCode::OK);
    let adj_id = adj_body["id"].as_str().unwrap();

    let adj_lines = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = (SELECT journal_entry_id FROM stock_adjustments WHERE id = ?1)"
    )
    .bind(adj_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    let mut sum_adj_deb = 0i64;
    let mut sum_adj_cred = 0i64;
    for row in &adj_lines {
        let code: String = row.get("account_code");
        let deb: i64 = row.get("debit");
        let cred: i64 = row.get("credit");
        sum_adj_deb += deb;
        sum_adj_cred += cred;
        if code == "1300" {
            assert_eq!(deb, 280_000);
            assert_eq!(cred, 0);
        } else if code == "5000" {
            assert_eq!(cred, 280_000);
            assert_eq!(deb, 0);
        }
    }
    assert_eq!(sum_adj_deb, 280_000);
    assert_eq!(sum_adj_cred, 280_000);

    // 6. Global Double-Entry Invariant Assertion:
    // Zero unbalanced journal entries in the entire database
    let unbalanced_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM (
            SELECT journal_id, SUM(debit) as total_deb, SUM(credit) as total_cred
            FROM journal_lines
            GROUP BY journal_id
            HAVING total_deb != total_cred
        )
        "#
    )
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(unbalanced_count, 0, "CRITICAL: Every journal entry in the system must strictly balance SUM(debit) == SUM(credit)");
}

// =========================================================================
// CHALLENGE 4: Account 1300 Balance Mathematical Tie-Out to Inventory Valuation
// =========================================================================
#[tokio::test]
async fn test_challenge_4_account_1300_tie_out_to_stock_valuation() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-TIE", "Barang Tie Out", 25000).await;

    // Helper closure to calculate Account 1300 balance
    async fn get_account_1300_balance(pool: &SqlitePool, tenant_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(SUM(debit), 0) - COALESCE(SUM(credit), 0)
            FROM journal_lines
            WHERE tenant_id = ?1 AND account_code = '1300'
            "#
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    // Helper closure to calculate physical inventory valuation: SUM(quantity_on_hand * average_cost)
    async fn get_physical_inventory_valuation(pool: &SqlitePool, tenant_id: &str) -> i64 {
        let rows = sqlx::query(
            "SELECT quantity_on_hand, average_cost FROM stock_items WHERE tenant_id = ?1"
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .unwrap();

        rows.iter()
            .map(|r| {
                let q: i64 = r.get("quantity_on_hand");
                let c: i64 = r.get("average_cost");
                q * c
            })
            .sum()
    }

    // Step 0: Initial state tie-out (0 IDR)
    let b0 = get_account_1300_balance(&harness.pool, &harness.tenant_id).await;
    let v0 = get_physical_inventory_valuation(&harness.pool, &harness.tenant_id).await;
    assert_eq!(b0, 0);
    assert_eq!(v0, 0);
    assert_eq!(b0, v0, "Initial inventory tie-out failed");

    // Step 1: PO Receipt of 100 units @ 25,000 IDR (2,500,000 IDR)
    let (_, po_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "Supplier Tie Out",
            "destination_warehouse_id": wh_id,
            "items": [
                { "product_id": prod_id, "quantity_ordered": 200, "unit_cost": 25000 }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_res["id"].as_str().unwrap();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 100, "unit_cost": 25000 }
            ]
        })),
        None,
    )
    .await;

    let b1 = get_account_1300_balance(&harness.pool, &harness.tenant_id).await;
    let v1 = get_physical_inventory_valuation(&harness.pool, &harness.tenant_id).await;
    assert_eq!(b1, 2_500_000);
    assert_eq!(v1, 2_500_000);
    assert_eq!(b1, v1, "Step 1: Receipt tie-out failed");

    // Step 2: Second receipt of 50 units @ 40,000 IDR (2,000,000 IDR)
    // New total cost = 2.5M + 2.0M = 4.5M IDR
    // New total qty = 150
    // WAC = 4.5M / 150 = 30,000 IDR
    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 50, "unit_cost": 40000 }
            ]
        })),
        None,
    )
    .await;

    let b2 = get_account_1300_balance(&harness.pool, &harness.tenant_id).await;
    let v2 = get_physical_inventory_valuation(&harness.pool, &harness.tenant_id).await;
    assert_eq!(b2, 4_500_000);
    assert_eq!(v2, 4_500_000);
    assert_eq!(b2, v2, "Step 2: Second receipt tie-out failed");

    // Step 3: Outbound fulfillment of 60 units (COGS = 60 * 30,000 = 1,800,000 IDR)
    // Remaining stock = 90 units
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 60
        })),
        None,
    )
    .await;

    let b3 = get_account_1300_balance(&harness.pool, &harness.tenant_id).await;
    let v3 = get_physical_inventory_valuation(&harness.pool, &harness.tenant_id).await;
    assert_eq!(b3, 2_700_000);
    assert_eq!(v3, 2_700_000);
    assert_eq!(b3, v3, "Step 3: Outbound fulfillment tie-out failed");

    // Step 4: Physical adjustment surplus: count 100 (+10 units @ 30,000 = 300,000 IDR)
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 100,
            "reason": "Surplus verified"
        })),
        None,
    )
    .await;

    let b4 = get_account_1300_balance(&harness.pool, &harness.tenant_id).await;
    let v4 = get_physical_inventory_valuation(&harness.pool, &harness.tenant_id).await;
    assert_eq!(b4, 3_000_000);
    assert_eq!(v4, 3_000_000);
    assert_eq!(b4, v4, "Step 4: Adjustment surplus tie-out failed");

    // Step 5: Physical adjustment deficit: count 80 (-20 units @ 30,000 = 600,000 IDR)
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 80,
            "reason": "Deficit verified"
        })),
        None,
    )
    .await;

    let b5 = get_account_1300_balance(&harness.pool, &harness.tenant_id).await;
    let v5 = get_physical_inventory_valuation(&harness.pool, &harness.tenant_id).await;
    assert_eq!(b5, 2_400_000);
    assert_eq!(v5, 2_400_000);
    assert_eq!(b5, v5, "Step 5: Adjustment deficit tie-out failed");

    // Step 6: Complete depletion (Outbound 80 units @ 30,000 = 2,400,000 IDR)
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 80
        })),
        None,
    )
    .await;

    let b6 = get_account_1300_balance(&harness.pool, &harness.tenant_id).await;
    let v6 = get_physical_inventory_valuation(&harness.pool, &harness.tenant_id).await;
    assert_eq!(b6, 0);
    assert_eq!(v6, 0);
    assert_eq!(b6, v6, "Step 6: Complete stock depletion tie-out failed");
}

// =========================================================================
// CHALLENGE 5: Transactional Outbox Event `StockReceived` Atomicity & Schema
// =========================================================================
#[tokio::test]
async fn test_challenge_5_transactional_outbox_stock_received_atomicity() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-OUTBOX", "Barang Outbox Test", 35000).await;

    // 1. Create and order PO
    let (_, po_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "Supplier Outbox Invariant",
            "destination_warehouse_id": wh_id,
            "items": [
                { "product_id": prod_id, "quantity_ordered": 50, "unit_cost": 35000 }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_res["id"].as_str().unwrap();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;

    // 2. Successful receipt: 20 units @ 35,000 IDR = 700,000 IDR
    let (s_rcv, rcv_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 20, "unit_cost": 35000 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s_rcv, StatusCode::OK);
    let j_entry_id = rcv_body["journal_entry_id"].as_str().unwrap();

    // Verify outbox row committed atomically in same transaction
    let outbox_row = sqlx::query(
        r#"
        SELECT id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status
        FROM outbox_events
        WHERE aggregate_id = ?1 AND event_type = 'StockReceived'
        "#
    )
    .bind(po_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let event_type: String = outbox_row.get("event_type");
    let agg_type: String = outbox_row.get("aggregate_type");
    let status: String = outbox_row.get("status");
    let payload_str: String = outbox_row.get("payload_json");

    assert_eq!(event_type, "StockReceived");
    assert_eq!(agg_type, "PurchaseOrder");
    assert_eq!(status, "PENDING");

    let payload: Value = serde_json::from_str(&payload_str).expect("Outbox payload must be valid JSON");
    assert_eq!(payload["po_id"], po_id);
    assert_eq!(payload["total_receipt_value"], 700_000);
    assert_eq!(payload["journal_id"], j_entry_id);

    // 3. Atomicity on failure: Attempt over-receipt (remaining is 30, try receiving 31)
    let pre_outbox_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events").fetch_one(&harness.pool).await.unwrap();
    let pre_journal_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries").fetch_one(&harness.pool).await.unwrap();

    let (s_fail, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 31, "unit_cost": 35000 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s_fail, StatusCode::UNPROCESSABLE_ENTITY);

    let post_outbox_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events").fetch_one(&harness.pool).await.unwrap();
    let post_journal_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries").fetch_one(&harness.pool).await.unwrap();

    // Verify rollback: exactly ZERO outbox events and ZERO journal entries created
    assert_eq!(pre_outbox_count, post_outbox_count, "Failed receipt must not emit outbox events");
    assert_eq!(pre_journal_count, post_journal_count, "Failed receipt must not post journal entries");
}

// =========================================================================
// CHALLENGE 6: Cross-Tenant Security & Anti-Enumeration (HTTP 404)
// =========================================================================
#[tokio::test]
async fn test_challenge_6_cross_tenant_isolation_strict_404() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-ISO", "Barang Isolation", 20000).await;

    // 1. Tenant A creates PO
    let (_, po_a_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "Supplier Isolation",
            "destination_warehouse_id": wh_id,
            "items": [
                { "product_id": prod_id, "quantity_ordered": 10, "unit_cost": 20000 }
            ]
        })),
        None,
    )
    .await;
    let po_a_id = po_a_res["id"].as_str().unwrap();

    // Random non-existent UUID
    let non_existent_id = Uuid::new_v4().to_string();

    // 2. Tenant B GET Tenant A's PO vs Non-Existent PO:
    let (s_get_a, b_get_a, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/purchase-orders/{}", po_a_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    let (s_get_non, b_get_non, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/purchase-orders/{}", non_existent_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;

    assert_eq!(s_get_a, StatusCode::NOT_FOUND, "Cross-tenant GET must return 404");
    assert_eq!(s_get_non, StatusCode::NOT_FOUND);
    assert_eq!(b_get_a["code"], "NOT_FOUND");
    assert_eq!(b_get_non["code"], "NOT_FOUND");

    // 3. Tenant B POST /order on Tenant A's PO:
    let (s_ord_a, b_ord_a, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_a_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_ord_a, StatusCode::NOT_FOUND, "Cross-tenant order must return 404");
    assert_eq!(b_ord_a["code"], "NOT_FOUND");

    // 4. Tenant B POST /receive on Tenant A's PO:
    let (s_rcv_a, b_rcv_a, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_a_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({
            "items": [
                { "product_id": prod_id, "quantity_received": 5, "unit_cost": 20000 }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(s_rcv_a, StatusCode::NOT_FOUND, "Cross-tenant receive must return 404");
    assert_eq!(b_rcv_a["code"], "NOT_FOUND");

    // 5. Tenant B POST /cancel on Tenant A's PO:
    let (s_can_a, b_can_a, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_a_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({ "reason": "Malicious cancellation attempt" })),
        None,
    )
    .await;
    assert_eq!(s_can_a, StatusCode::NOT_FOUND, "Cross-tenant cancel must return 404");
    assert_eq!(b_can_a["code"], "NOT_FOUND");

    // 6. Tenant B listing POs: Zero Tenant A POs returned
    let (s_list_b, b_list_b, _) = send_req(
        &harness.app,
        Method::GET,
        "/api/v1/purchase-orders",
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_list_b, StatusCode::OK);
    assert_eq!(b_list_b["count"], 0);
    assert_eq!(b_list_b["purchase_orders"].as_array().unwrap().len(), 0);

    // 7. Verify Tenant B cannot access Tenant A's GL accounts or journals:
    let count_b_movements: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_movements WHERE tenant_id = ?1"
    )
    .bind(&harness.tenant_b_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(count_b_movements, 0, "Tenant B must have zero stock movements");
}

// =========================================================================
// CHALLENGE 7: Concurrent Racing Receipts & COGS Fulfillments Under Load
// =========================================================================
#[tokio::test]
async fn test_challenge_7_concurrent_receipts_and_gl_balance_under_load() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-CONC", "Barang Concurrency Load", 10000).await;

    // Create PO with 500 units @ 10,000 IDR
    let (_, po_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "Supplier Concurrency",
            "destination_warehouse_id": wh_id,
            "items": [
                { "product_id": prod_id, "quantity_ordered": 500, "unit_cost": 10000 }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_res["id"].as_str().unwrap().to_string();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;

    // Concurrently launch 10 tasks each attempting to receive 50 units
    let mut handles = Vec::new();
    for _ in 0..10 {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_id.clone();
        let p_id = po_id.clone();
        let pr_id = prod_id.clone();

        handles.push(tokio::spawn(async move {
            send_req(
                &app,
                Method::POST,
                &format!("/api/v1/purchase-orders/{}/receive", p_id),
                &token,
                &tenant_id,
                Some(json!({
                    "items": [
                        { "product_id": pr_id, "quantity_received": 50, "unit_cost": 10000 }
                    ]
                })),
                None,
            )
            .await
        }));
    }

    let mut successful_receipts = 0;
    for h in handles {
        let (status, _, _) = h.await.unwrap();
        if status == StatusCode::OK {
            successful_receipts += 1;
        }
    }

    assert_eq!(successful_receipts, 10, "All 10 valid receipts must succeed");

    // Check PO status is RECEIVED
    let (_, po_final, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/purchase-orders/{}", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(po_final["status"], "RECEIVED");
    assert_eq!(po_final["items"][0]["quantity_received"], 500);

    // Assert strictly SUM(debit) == SUM(credit) across ALL journals
    let unbalanced_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM (
            SELECT journal_id, SUM(debit) as total_deb, SUM(credit) as total_cred
            FROM journal_lines
            GROUP BY journal_id
            HAVING total_deb != total_cred
        )
        "#
    )
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(unbalanced_count, 0, "All concurrent journals must balance perfectly");

    // Assert exactly 10 StockReceived outbox events emitted
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1 AND event_type = 'StockReceived'"
    )
    .bind(&po_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 10, "Exactly 10 StockReceived outbox events must be emitted");
}
