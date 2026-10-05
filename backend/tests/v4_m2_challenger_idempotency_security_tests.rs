//! Milestone 2 Empirical Adversarial Challenger Verification Suite
//!
//! Identity: teamwork_preview_challenger_m2_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Mission Objectives:
//! 1. Empirical Idempotency Caching:
//!    - INBOUND identical replay returns cached response with `x-cache-replay: true` without duplicating movements.
//!    - OUTBOUND identical replay returns cached response with `x-cache-replay: true` without duplicating stock deduction.
//!    - TRANSFER identical replay returns cached response with `x-cache-replay: true` without duplicating transfer.
//!    - ADJUSTMENT identical replay returns cached response with `x-cache-replay: true` without duplicating adjustment.
//!    - Payload mismatch on same Idempotency-Key returns HTTP 409 Conflict with code "IDEMPOTENCY_MISMATCH".
//!    - Cross-tenant identical idempotency key independence.
//! 2. Cross-Tenant Security Boundaries:
//!    - Attempting to query another tenant's warehouse or product returns HTTP 404 NOT_FOUND.
//!    - Attempting to list stock or movements by other tenant's warehouse/product returns zero items (zero existence enumeration).
//!    - Attempting OUTBOUND deduction against another tenant's warehouse/product strictly returns HTTP 404 NOT_FOUND.
//!    - Attempting INBOUND addition against another tenant's warehouse/product strictly returns HTTP 404 NOT_FOUND.
//!    - Attempting inter-warehouse TRANSFER using another tenant's warehouse/product strictly returns HTTP 404 NOT_FOUND.
//!    - Attempting stock ADJUSTMENT against another tenant's warehouse/product strictly returns HTTP 404 NOT_FOUND.
//!    - Victim tenant's stock and audit trails remain strictly uncorrupted.
//! 3. Complete RBAC Authorization Matrix:
//!    - POST /api/v1/warehouses:
//!      - Owner -> 201 Created
//!      - Administrator -> 201 Created
//!      - Manager -> 201 Created
//!      - Staff -> 403 Forbidden ("FORBIDDEN")
//!      - Accountant -> 403 Forbidden ("FORBIDDEN")
//!      - Custom("guest") -> 403 Forbidden ("FORBIDDEN")
//!    - POST /api/v1/inventory/adjust (and /adjustments):
//!      - Owner -> 200 OK
//!      - Administrator -> 200 OK
//!      - Manager -> 200 OK
//!      - Staff -> 403 Forbidden ("FORBIDDEN")
//!      - Accountant -> 403 Forbidden ("FORBIDDEN")
//!      - Custom("viewer") -> 403 Forbidden ("FORBIDDEN")
//!    - Staff operational warehouse movements & transfers succeed when authorized.

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

struct ChallengerHarness {
    app: axum::Router,
    #[allow(dead_code)]
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    tenant_a_id: String,
    owner_a_token: String,
    admin_a_token: String,
    manager_a_token: String,
    staff_a_token: String,
    accountant_a_token: String,
    custom_a_token: String,
    tenant_b_id: String,
    owner_b_token: String,
}

async fn create_user_and_membership(
    user_repo: &SqlxUserRepository,
    membership_repo: &SqlxMembershipRepository,
    jwt_engine: &JwtEngine,
    tenant_id: &str,
    email: &str,
    role: Role,
) -> String {
    let user_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_id.clone(),
            email: email.to_string(),
            password_hash: "challenger_test_hash".to_string(),
            display_name: email.to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (token, _) = jwt_engine
        .generate_token(&user_id, email, "user", "premium")
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_id.to_string(),
            user_id: user_id.clone(),
            role,
        })
        .await
        .unwrap();

    token
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("inventory_m2_challenger_test.sqlite");
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

    let jwt_secret = "m2_challenger_secret_key_1234567890_empirical_stress";
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

    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    // --- Tenant A ---
    let tenant_a_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Enterprise".to_string(),
            slug: "tenant-alpha-enterprise".to_string(),
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

    let owner_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "owner@alpha.com",
        Role::Owner,
    )
    .await;

    let admin_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "admin@alpha.com",
        Role::Administrator,
    )
    .await;

    let manager_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "manager@alpha.com",
        Role::Manager,
    )
    .await;

    let staff_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "staff@alpha.com",
        Role::Staff,
    )
    .await;

    let accountant_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "accountant@alpha.com",
        Role::Accountant,
    )
    .await;

    let custom_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "custom@alpha.com",
        Role::Custom("guest".to_string()),
    )
    .await;

    // --- Tenant B ---
    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Competitor".to_string(),
            slug: "tenant-beta-competitor".to_string(),
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

    let owner_b_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_b_id,
        "owner@beta.com",
        Role::Owner,
    )
    .await;

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        tenant_a_id,
        owner_a_token,
        admin_a_token,
        manager_a_token,
        staff_a_token,
        accountant_a_token,
        custom_a_token,
        tenant_b_id,
        owner_b_token,
    }
}

async fn challenge_req(
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

// =========================================================================
// SECTION 1: EMPIRICAL IDEMPOTENCY CACHING & MISMATCH TESTS
// =========================================================================

#[tokio::test]
async fn test_empirical_idempotency_movement_inbound_and_outbound() {
    let h = setup_challenger_harness().await;

    // Create warehouse & product in Tenant A
    let (_, wh, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-IDEMP-TEST", "name": "Idemp WH" })),
        None,
    )
    .await;
    let wh_id = wh["id"].as_str().unwrap();

    let (_, prod, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Idemp Gadget", "cost_price": 5000, "sale_price": 10000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    // 1. INBOUND Idempotency Replay & Mismatch
    let key_inb = "IDEMP-KEY-INB-001";
    let mut inb_headers = HeaderMap::new();
    inb_headers.insert("Idempotency-Key", key_inb.parse().unwrap());

    let inb_payload = json!({
        "movement_type": "INBOUND",
        "product_id": prod_id,
        "destination_warehouse_id": wh_id,
        "quantity": 100,
        "unit_cost": 5000
    });

    // 1a. Initial INBOUND request -> 201 Created without x-cache-replay
    let (s1, b1, h1) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(inb_payload.clone()),
        Some(inb_headers.clone()),
    )
    .await;
    assert_eq!(s1, StatusCode::CREATED);
    assert!(!h1.contains_key("x-cache-replay"), "Initial response must not contain x-cache-replay");
    assert_eq!(b1["resulting_stock"], 100);
    let movement_1_id = b1["id"].as_str().unwrap().to_string();

    // 1b. Replay identical INBOUND request -> 201 Created WITH x-cache-replay: true
    let (s2, b2, h2) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(inb_payload.clone()),
        Some(inb_headers.clone()),
    )
    .await;
    assert_eq!(s2, StatusCode::CREATED);
    assert_eq!(
        h2.get("x-cache-replay").and_then(|v| v.to_str().ok()),
        Some("true"),
        "Replay must include x-cache-replay: true"
    );
    assert_eq!(b2["id"].as_str().unwrap(), movement_1_id);
    assert_eq!(b2["resulting_stock"], 100);

    // 1c. Verify Database: stock is strictly 100 (NOT 200!) and movement count is 1
    let (s_stock, b_stock, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_stock, StatusCode::OK);
    assert_eq!(b_stock["stock_items"][0]["quantity_on_hand"], 100);

    let (s_movs, b_movs, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory/movements?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_movs, StatusCode::OK);
    assert_eq!(b_movs["count"], 1, "There must be exactly 1 stock movement in DB");

    // 1d. Mismatched payload with same key -> 409 Conflict IDEMPOTENCY_MISMATCH
    let mismatched_inb_payload = json!({
        "movement_type": "INBOUND",
        "product_id": prod_id,
        "destination_warehouse_id": wh_id,
        "quantity": 250, // altered quantity
        "unit_cost": 5000
    });
    let (s3, b3, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(mismatched_inb_payload),
        Some(inb_headers.clone()),
    )
    .await;
    assert_eq!(s3, StatusCode::CONFLICT);
    assert_eq!(b3["code"], "IDEMPOTENCY_MISMATCH");

    // Stock must remain strictly 100
    let (_, b_stock_check, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_stock_check["stock_items"][0]["quantity_on_hand"], 100);

    // 2. OUTBOUND Idempotency Replay & Mismatch
    let key_out = "IDEMP-KEY-OUT-001";
    let mut out_headers = HeaderMap::new();
    out_headers.insert("Idempotency-Key", key_out.parse().unwrap());

    let out_payload = json!({
        "movement_type": "OUTBOUND",
        "product_id": prod_id,
        "source_warehouse_id": wh_id,
        "quantity": 30
    });

    // 2a. Initial OUTBOUND -> 201 Created
    let (s_out1, b_out1, h_out1) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(out_payload.clone()),
        Some(out_headers.clone()),
    )
    .await;
    assert_eq!(s_out1, StatusCode::CREATED);
    assert!(!h_out1.contains_key("x-cache-replay"));
    assert_eq!(b_out1["remaining_stock"], 70);
    let out_mov_id = b_out1["id"].as_str().unwrap().to_string();

    // 2b. Replay identical OUTBOUND -> 201 Created with x-cache-replay: true
    let (s_out2, b_out2, h_out2) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(out_payload.clone()),
        Some(out_headers.clone()),
    )
    .await;
    assert_eq!(s_out2, StatusCode::CREATED);
    assert_eq!(
        h_out2.get("x-cache-replay").and_then(|v| v.to_str().ok()),
        Some("true")
    );
    assert_eq!(b_out2["id"].as_str().unwrap(), out_mov_id);
    assert_eq!(b_out2["remaining_stock"], 70);

    // Stock must be 70 (NOT deducted twice to 40!)
    let (_, b_stock_after_out, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_stock_after_out["stock_items"][0]["quantity_on_hand"], 70);

    // 2c. Mismatched OUTBOUND -> 409 Conflict
    let mismatched_out = json!({
        "movement_type": "OUTBOUND",
        "product_id": prod_id,
        "source_warehouse_id": wh_id,
        "quantity": 10 // changed
    });
    let (s_out3, b_out3, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(mismatched_out),
        Some(out_headers),
    )
    .await;
    assert_eq!(s_out3, StatusCode::CONFLICT);
    assert_eq!(b_out3["code"], "IDEMPOTENCY_MISMATCH");

    // Stock remains 70
    let (_, b_stock_final, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_stock_final["stock_items"][0]["quantity_on_hand"], 70);
}

#[tokio::test]
async fn test_empirical_idempotency_transfer_replay_and_mismatch() {
    let h = setup_challenger_harness().await;

    // Create 2 warehouses and a product
    let (_, wh1, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-XFER-SRC", "name": "Transfer Source WH" })),
        None,
    )
    .await;
    let wh1_id = wh1["id"].as_str().unwrap();

    let (_, wh2, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-XFER-DST", "name": "Transfer Dest WH" })),
        None,
    )
    .await;
    let wh2_id = wh2["id"].as_str().unwrap();

    let (_, prod, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Transfer Widget", "cost_price": 4000, "sale_price": 8000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    // Inbound initial stock 50 into WH1
    challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh1_id,
            "quantity": 50,
            "unit_cost": 4000
        })),
        None,
    )
    .await;

    let key_xfer = "IDEMP-KEY-XFER-999";
    let mut xfer_headers = HeaderMap::new();
    xfer_headers.insert("Idempotency-Key", key_xfer.parse().unwrap());

    let xfer_payload = json!({
        "source_warehouse_id": wh1_id,
        "destination_warehouse_id": wh2_id,
        "product_id": prod_id,
        "quantity": 20
    });

    // 1. Initial Transfer -> 200 OK
    let (s1, b1, h1) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(xfer_payload.clone()),
        Some(xfer_headers.clone()),
    )
    .await;
    assert_eq!(s1, StatusCode::OK);
    assert!(!h1.contains_key("x-cache-replay"));
    assert_eq!(b1["source_remaining"], 30);
    assert_eq!(b1["destination_total"], 20);
    let movement_id = b1["movement_id"].as_str().unwrap().to_string();

    // 2. Replay identical Transfer -> 200 OK with x-cache-replay: true
    let (s2, b2, h2) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(xfer_payload.clone()),
        Some(xfer_headers.clone()),
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(
        h2.get("x-cache-replay").and_then(|v| v.to_str().ok()),
        Some("true")
    );
    assert_eq!(b2["movement_id"].as_str().unwrap(), movement_id);
    assert_eq!(b2["source_remaining"], 30);
    assert_eq!(b2["destination_total"], 20);

    // Verify stock at source and destination
    let (_, stock_src, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh1_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_src["stock_items"][0]["quantity_on_hand"], 30);

    let (_, stock_dst, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh2_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_dst["stock_items"][0]["quantity_on_hand"], 20);

    // 3. Mismatched payload with same key -> 409 Conflict
    let mismatched_xfer = json!({
        "source_warehouse_id": wh1_id,
        "destination_warehouse_id": wh2_id,
        "product_id": prod_id,
        "quantity": 15 // changed
    });
    let (s3, b3, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(mismatched_xfer),
        Some(xfer_headers),
    )
    .await;
    assert_eq!(s3, StatusCode::CONFLICT);
    assert_eq!(b3["code"], "IDEMPOTENCY_MISMATCH");

    // Stock in both warehouses must remain unchanged
    let (_, stock_src_check, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh1_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_src_check["stock_items"][0]["quantity_on_hand"], 30);
}

#[tokio::test]
async fn test_empirical_idempotency_adjustment_replay_and_mismatch() {
    let h = setup_challenger_harness().await;

    let (_, wh, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-ADJ-IDEMP", "name": "Adj WH" })),
        None,
    )
    .await;
    let wh_id = wh["id"].as_str().unwrap();

    let (_, prod, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Adjustment Product", "cost_price": 1000, "sale_price": 2000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    // Initial stock 10
    challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10
        })),
        None,
    )
    .await;

    let key_adj = "IDEMP-KEY-ADJ-777";
    let mut adj_headers = HeaderMap::new();
    adj_headers.insert("Idempotency-Key", key_adj.parse().unwrap());

    let adj_payload = json!({
        "warehouse_id": wh_id,
        "product_id": prod_id,
        "actual_quantity": 25,
        "reason": "Stock count correction"
    });

    // 1. Initial Adjustment -> 200 OK
    let (s1, b1, h1) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(adj_payload.clone()),
        Some(adj_headers.clone()),
    )
    .await;
    assert_eq!(s1, StatusCode::OK);
    assert!(!h1.contains_key("x-cache-replay"));
    let adj_number = b1["adjustment_number"].as_str().unwrap().to_string();
    assert_eq!(b1["previous_quantity"], 10);
    assert_eq!(b1["actual_quantity"], 25);
    assert_eq!(b1["variance"], 15);

    // 2. Replay identical Adjustment -> 200 OK with x-cache-replay: true
    let (s2, b2, h2) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(adj_payload.clone()),
        Some(adj_headers.clone()),
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(
        h2.get("x-cache-replay").and_then(|v| v.to_str().ok()),
        Some("true")
    );
    assert_eq!(b2["adjustment_number"].as_str().unwrap(), adj_number);
    assert_eq!(b2["actual_quantity"], 25);

    // 3. Mismatched Adjustment with same key -> 409 Conflict
    let mismatched_adj = json!({
        "warehouse_id": wh_id,
        "product_id": prod_id,
        "actual_quantity": 40, // changed
        "reason": "Altered count"
    });
    let (s3, b3, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(mismatched_adj),
        Some(adj_headers),
    )
    .await;
    assert_eq!(s3, StatusCode::CONFLICT);
    assert_eq!(b3["code"], "IDEMPOTENCY_MISMATCH");

    // Stock remains 25
    let (_, b_stock, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_stock["stock_items"][0]["quantity_on_hand"], 25);
}

#[tokio::test]
async fn test_empirical_idempotency_cross_tenant_key_collision_independence() {
    let h = setup_challenger_harness().await;

    // Tenant A creates WH and Product
    let (_, wh_a, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-IDEMP-A", "name": "WH A" })),
        None,
    )
    .await;
    let wh_a_id = wh_a["id"].as_str().unwrap();

    let (_, prod_a, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Product A", "cost_price": 1000, "sale_price": 2000 })),
        None,
    )
    .await;
    let prod_a_id = prod_a["id"].as_str().unwrap();

    // Tenant B creates WH and Product
    let (_, wh_b, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "code": "WH-IDEMP-B", "name": "WH B" })),
        None,
    )
    .await;
    let wh_b_id = wh_b["id"].as_str().unwrap();

    let (_, prod_b, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "name": "Product B", "cost_price": 3000, "sale_price": 6000 })),
        None,
    )
    .await;
    let prod_b_id = prod_b["id"].as_str().unwrap();

    // Shared Key between two completely different tenants
    let shared_key = "UNIVERSAL-IDEMP-SHARED-KEY";
    let mut shared_headers = HeaderMap::new();
    shared_headers.insert("Idempotency-Key", shared_key.parse().unwrap());

    // Tenant A uses shared key for INBOUND
    let (s_a, b_a, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_a_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 100
        })),
        Some(shared_headers.clone()),
    )
    .await;
    assert_eq!(s_a, StatusCode::CREATED);
    assert_eq!(b_a["resulting_stock"], 100);

    // Tenant B uses THE SAME shared key for their own INBOUND
    let (s_b, b_b, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_b_id,
            "destination_warehouse_id": wh_b_id,
            "quantity": 50
        })),
        Some(shared_headers.clone()),
    )
    .await;
    // Must succeed independently because user/actor ID differs!
    assert_eq!(s_b, StatusCode::CREATED);
    assert_eq!(b_b["resulting_stock"], 50);

    // Both tenants replay and get their own cached responses
    let (s_a_rep, b_a_rep, h_a_rep) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_a_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 100
        })),
        Some(shared_headers.clone()),
    )
    .await;
    assert_eq!(s_a_rep, StatusCode::CREATED);
    assert_eq!(h_a_rep.get("x-cache-replay").unwrap(), "true");
    assert_eq!(b_a_rep["id"], b_a["id"]);

    let (s_b_rep, b_b_rep, h_b_rep) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_b_id,
            "destination_warehouse_id": wh_b_id,
            "quantity": 50
        })),
        Some(shared_headers),
    )
    .await;
    assert_eq!(s_b_rep, StatusCode::CREATED);
    assert_eq!(h_b_rep.get("x-cache-replay").unwrap(), "true");
    assert_eq!(b_b_rep["id"], b_b["id"]);
}

// =========================================================================
// SECTION 2: EMPIRICAL CROSS-TENANT SECURITY BOUNDARY TESTS (ZERO ENUMERATION)
// =========================================================================

#[tokio::test]
async fn test_empirical_cross_tenant_zero_enumeration_queries() {
    let h = setup_challenger_harness().await;

    // Setup Tenant A: Warehouse, Product, and Stock Movements
    let (_, wh_a, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-ISOLATION-A", "name": "Warehouse Alpha Secret" })),
        None,
    )
    .await;
    let wh_a_id = wh_a["id"].as_str().unwrap();

    let (_, prod_a, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Classified Product A", "cost_price": 50000, "sale_price": 99000 })),
        None,
    )
    .await;
    let prod_a_id = prod_a["id"].as_str().unwrap();

    challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_a_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 100
        })),
        None,
    )
    .await;

    // Tenant B queries Tenant A entities:
    // 1. Direct Warehouse lookup -> strictly 404 NOT_FOUND
    let (s1, b1, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", wh_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s1, StatusCode::NOT_FOUND, "Direct warehouse lookup must return 404");
    assert_eq!(b1["code"], "NOT_FOUND");

    // 2. Direct Product lookup -> strictly 404 NOT_FOUND
    let (s2, b2, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/products/{}", prod_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s2, StatusCode::NOT_FOUND, "Direct product lookup must return 404");
    assert_eq!(b2["code"], "NOT_FOUND");

    // 3. Stock list filtered by Tenant A's warehouse_id -> count 0, empty list
    let (s3, b3, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(b3["count"], 0);
    assert_eq!(b3["stock_items"].as_array().unwrap().len(), 0);

    // 4. Stock list filtered by Tenant A's product_id -> count 0, empty list
    let (s4, b4, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?product_id={}", prod_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s4, StatusCode::OK);
    assert_eq!(b4["count"], 0);
    assert_eq!(b4["stock_items"].as_array().unwrap().len(), 0);

    // 5. Movements filtered by Tenant A's warehouse_id -> count 0
    let (s5, b5, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory/movements?warehouse_id={}", wh_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s5, StatusCode::OK);
    assert_eq!(b5["count"], 0);

    // 6. Movements filtered by Tenant A's product_id -> count 0
    let (s6, b6, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory/movements?product_id={}", prod_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s6, StatusCode::OK);
    assert_eq!(b6["count"], 0);
}

#[tokio::test]
async fn test_empirical_cross_tenant_unauthorized_mutations() {
    let h = setup_challenger_harness().await;

    // Tenant A: WH_A, Prod_A with 100 units
    let (_, wh_a, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-MUT-A", "name": "Warehouse Alpha" })),
        None,
    )
    .await;
    let wh_a_id = wh_a["id"].as_str().unwrap();

    let (_, prod_a, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Product Alpha", "cost_price": 1000, "sale_price": 2000 })),
        None,
    )
    .await;
    let prod_a_id = prod_a["id"].as_str().unwrap();

    challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_a_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 100
        })),
        None,
    )
    .await;

    // Tenant B: WH_B, Prod_B with 50 units
    let (_, wh_b, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "code": "WH-MUT-B", "name": "Warehouse Beta" })),
        None,
    )
    .await;
    let wh_b_id = wh_b["id"].as_str().unwrap();

    let (_, prod_b, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "name": "Product Beta", "cost_price": 3000, "sale_price": 5000 })),
        None,
    )
    .await;
    let prod_b_id = prod_b["id"].as_str().unwrap();

    // --- ATTACK 1: Cross-tenant OUTBOUND deduction attempts ---
    // 1a. Tenant B attempts to deduct from Tenant A's warehouse and product
    let (s_out1, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_a_id,
            "source_warehouse_id": wh_a_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_out1, StatusCode::NOT_FOUND);

    // 1b. Tenant B attempts OUTBOUND with own product but Tenant A's warehouse
    let (s_out2, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_b_id,
            "source_warehouse_id": wh_a_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    assert_eq!(s_out2, StatusCode::NOT_FOUND);

    // 1c. Tenant B attempts OUTBOUND with Tenant A's product from Tenant B's warehouse
    let (s_out3, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_a_id,
            "source_warehouse_id": wh_b_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    assert_eq!(s_out3, StatusCode::NOT_FOUND);

    // --- ATTACK 2: Cross-tenant INBOUND injection attempts ---
    // 2a. Inbound into Tenant A's warehouse
    let (s_inb1, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_b_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 20
        })),
        None,
    )
    .await;
    assert_eq!(s_inb1, StatusCode::NOT_FOUND);

    // 2b. Inbound referencing Tenant A's product
    let (s_inb2, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_a_id,
            "destination_warehouse_id": wh_b_id,
            "quantity": 20
        })),
        None,
    )
    .await;
    assert_eq!(s_inb2, StatusCode::NOT_FOUND);

    // --- ATTACK 3: Cross-tenant TRANSFER attempts ---
    // 3a. Source in Tenant A, Destination in Tenant B
    let (s_xfer1, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": wh_b_id,
            "product_id": prod_b_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_xfer1, StatusCode::NOT_FOUND);

    // 3b. Source in Tenant B, Destination in Tenant A
    let (s_xfer2, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "source_warehouse_id": wh_b_id,
            "destination_warehouse_id": wh_a_id,
            "product_id": prod_b_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_xfer2, StatusCode::NOT_FOUND);

    // 3c. Both warehouses in Tenant B, but product belongs to Tenant A
    let (_, wh_b2, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "code": "WH-MUT-B2", "name": "Warehouse Beta 2" })),
        None,
    )
    .await;
    let wh_b2_id = wh_b2["id"].as_str().unwrap();

    let (s_xfer3, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "source_warehouse_id": wh_b_id,
            "destination_warehouse_id": wh_b2_id,
            "product_id": prod_a_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_xfer3, StatusCode::NOT_FOUND);

    // --- ATTACK 4: Cross-tenant ADJUSTMENT attempts ---
    // 4a. Tenant B attempts to adjust Tenant A's warehouse
    let (s_adj1, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "warehouse_id": wh_a_id,
            "product_id": prod_b_id,
            "actual_quantity": 0
        })),
        None,
    )
    .await;
    assert_eq!(s_adj1, StatusCode::NOT_FOUND);

    // 4b. Tenant B attempts to adjust Tenant A's product
    let (s_adj2, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({
            "warehouse_id": wh_b_id,
            "product_id": prod_a_id,
            "actual_quantity": 0
        })),
        None,
    )
    .await;
    assert_eq!(s_adj2, StatusCode::NOT_FOUND);

    // --- INVARIANT VERIFICATION: Tenant A's stock is 100% UNTOUCHED ---
    let (s_verify, b_verify, _) = challenge_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_a_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_verify, StatusCode::OK);
    assert_eq!(b_verify["stock_items"][0]["quantity_on_hand"], 100);
}

// =========================================================================
// SECTION 3: EMPIRICAL RBAC AUTHORIZATION MATRIX TESTS
// =========================================================================

#[tokio::test]
async fn test_empirical_rbac_warehouse_creation_matrix() {
    let h = setup_challenger_harness().await;

    // 1. Owner -> 201 Created
    let (s_owner, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-RBAC-OWNER", "name": "Owner Warehouse" })),
        None,
    )
    .await;
    assert_eq!(s_owner, StatusCode::CREATED, "Owner must be allowed to create warehouse");

    // 2. Administrator -> 201 Created
    let (s_admin, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.admin_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-RBAC-ADMIN", "name": "Admin Warehouse" })),
        None,
    )
    .await;
    assert_eq!(s_admin, StatusCode::CREATED, "Admin must be allowed to create warehouse");

    // 3. Manager -> 201 Created
    let (s_mgr, _, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.manager_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-RBAC-MGR", "name": "Manager Warehouse" })),
        None,
    )
    .await;
    assert_eq!(s_mgr, StatusCode::CREATED, "Manager must be allowed to create warehouse");

    // 4. Staff -> 403 Forbidden
    let (s_staff, b_staff, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.staff_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-RBAC-STAFF", "name": "Staff Warehouse" })),
        None,
    )
    .await;
    assert_eq!(s_staff, StatusCode::FORBIDDEN, "Staff must be rejected with 403 Forbidden");
    assert_eq!(b_staff["code"], "FORBIDDEN");

    // 5. Accountant -> 403 Forbidden
    let (s_acct, b_acct, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.accountant_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-RBAC-ACCT", "name": "Accountant Warehouse" })),
        None,
    )
    .await;
    assert_eq!(s_acct, StatusCode::FORBIDDEN, "Accountant must be rejected with 403 Forbidden");
    assert_eq!(b_acct["code"], "FORBIDDEN");

    // 6. Custom("guest") -> 403 Forbidden
    let (s_cust, b_cust, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.custom_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-RBAC-CUST", "name": "Custom Guest Warehouse" })),
        None,
    )
    .await;
    assert_eq!(s_cust, StatusCode::FORBIDDEN, "Custom guest must be rejected with 403 Forbidden");
    assert_eq!(b_cust["code"], "FORBIDDEN");
}

#[tokio::test]
async fn test_empirical_rbac_stock_adjustment_matrix() {
    let h = setup_challenger_harness().await;

    // Create warehouse & product by Owner
    let (_, wh, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-ADJ-RBAC", "name": "Adjustment RBAC WH" })),
        None,
    )
    .await;
    let wh_id = wh["id"].as_str().unwrap();

    let (_, prod, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Adjustment Item", "cost_price": 5000, "sale_price": 10000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    // 1. Staff attempts stock adjustment -> strictly 403 Forbidden
    let (s_staff, b_staff, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.staff_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 50
        })),
        None,
    )
    .await;
    assert_eq!(s_staff, StatusCode::FORBIDDEN, "Staff must be forbidden from adjusting stock");
    assert_eq!(b_staff["code"], "FORBIDDEN");

    // 2. Accountant attempts stock adjustment -> strictly 403 Forbidden
    let (s_acct, b_acct, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.accountant_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 50
        })),
        None,
    )
    .await;
    assert_eq!(s_acct, StatusCode::FORBIDDEN, "Accountant must be forbidden from adjusting stock");
    assert_eq!(b_acct["code"], "FORBIDDEN");

    // 3. Custom role attempts stock adjustment -> strictly 403 Forbidden
    let (s_cust, b_cust, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.custom_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 50
        })),
        None,
    )
    .await;
    assert_eq!(s_cust, StatusCode::FORBIDDEN, "Custom role must be forbidden from adjusting stock");
    assert_eq!(b_cust["code"], "FORBIDDEN");

    // 4. Manager -> 200 OK
    let (s_mgr, b_mgr, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.manager_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 30,
            "reason": "Manager count"
        })),
        None,
    )
    .await;
    assert_eq!(s_mgr, StatusCode::OK, "Manager must be permitted to adjust stock");
    assert_eq!(b_mgr["actual_quantity"], 30);

    // 5. Administrator -> 200 OK
    let (s_admin, b_admin, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &h.admin_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 45,
            "reason": "Admin recount"
        })),
        None,
    )
    .await;
    assert_eq!(s_admin, StatusCode::OK, "Administrator must be permitted to adjust stock");
    assert_eq!(b_admin["actual_quantity"], 45);

    // 6. Owner -> 200 OK
    let (s_owner, b_owner, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 60,
            "reason": "Owner override"
        })),
        None,
    )
    .await;
    assert_eq!(s_owner, StatusCode::OK, "Owner must be permitted to adjust stock");
    assert_eq!(b_owner["actual_quantity"], 60);
}

#[tokio::test]
async fn test_empirical_rbac_staff_operational_movements_permitted() {
    let h = setup_challenger_harness().await;

    // Owner creates 2 warehouses and a product
    let (_, wh1, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-STAFF-1", "name": "Warehouse 1" })),
        None,
    )
    .await;
    let wh1_id = wh1["id"].as_str().unwrap();

    let (_, wh2, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-STAFF-2", "name": "Warehouse 2" })),
        None,
    )
    .await;
    let wh2_id = wh2["id"].as_str().unwrap();

    let (_, prod, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Operational Goods", "cost_price": 2000, "sale_price": 5000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    // Staff performs operational INBOUND
    let (s_inb, b_inb, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.staff_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh1_id,
            "quantity": 50
        })),
        None,
    )
    .await;
    assert_eq!(s_inb, StatusCode::CREATED, "Staff should be permitted to receive inbound goods");
    assert_eq!(b_inb["resulting_stock"], 50);

    // Staff performs operational TRANSFER
    let (s_xfer, b_xfer, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.staff_a_token,
        &h.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh1_id,
            "destination_warehouse_id": wh2_id,
            "product_id": prod_id,
            "quantity": 20
        })),
        None,
    )
    .await;
    assert_eq!(s_xfer, StatusCode::OK, "Staff should be permitted to transfer goods");
    assert_eq!(b_xfer["source_remaining"], 30);
    assert_eq!(b_xfer["destination_total"], 20);

    // Staff performs operational OUTBOUND
    let (s_out, b_out, _) = challenge_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.staff_a_token,
        &h.tenant_a_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh1_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_out, StatusCode::CREATED, "Staff should be permitted to dispatch outbound goods");
    assert_eq!(b_out["remaining_stock"], 20);
}
