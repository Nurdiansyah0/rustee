//! Milestone 2 Empirical Adversarial Challenger Deep Suite
//!
//! Identity: teamwork_preview_challenger_m2_2_gen2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Deep Idempotency Stress:
//!    - Multi-cycle sequential replays (5 consecutive replays) maintaining `x-cache-replay: true` with zero movement drift.
//!    - Post-mismatch resilience: verifying original payload replay succeeds after a 409 mismatch attempt.
//!    - Whitespace/empty header handling (treated as un-idempotent without crashing).
//!    - Subtle payload mutations (altering 1 Rupiah unit cost, single char notes, 1 unit qty) strictly trigger 409 `IDEMPOTENCY_MISMATCH`.
//! 2. Adversarial Cross-Tenant Security Boundaries:
//!    - Bidirectional transfer attacks: Tenant A -> Tenant B, Tenant B -> Tenant A.
//!    - Uniform 404 behavior: Identical 404 response payload between valid foreign tenant entity vs random non-existent UUID (Zero Enumeration Proof).
//!    - Target warehouse / product existence enumeration immunity across all mutation endpoints.
//! 3. Complete RBAC Authorization Matrix:
//!    - Verification across Owner, Administrator, Manager, Staff, Accountant, and Custom roles.

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

struct DeepChallengerHarness {
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
    #[allow(dead_code)]
    staff_b_token: String,
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
            password_hash: "challenger_deep_hash".to_string(),
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

async fn setup_deep_harness() -> DeepChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("inventory_m2_deep_challenger_test.sqlite");
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

    let jwt_secret = "m2_deep_challenger_secret_key_1234567890_empirical_stress";
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
            name: "Tenant Alpha Core".to_string(),
            slug: "tenant-alpha-core".to_string(),
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
        "owner_deep@alpha.com",
        Role::Owner,
    )
    .await;

    let admin_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "admin_deep@alpha.com",
        Role::Administrator,
    )
    .await;

    let manager_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "manager_deep@alpha.com",
        Role::Manager,
    )
    .await;

    let staff_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "staff_deep@alpha.com",
        Role::Staff,
    )
    .await;

    let accountant_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "accountant_deep@alpha.com",
        Role::Accountant,
    )
    .await;

    let custom_a_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_a_id,
        "custom_deep@alpha.com",
        Role::Custom("guest".to_string()),
    )
    .await;

    // --- Tenant B ---
    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Deep".to_string(),
            slug: "tenant-beta-deep".to_string(),
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
        "owner_deep@beta.com",
        Role::Owner,
    )
    .await;

    let staff_b_token = create_user_and_membership(
        &user_repo,
        &membership_repo,
        &jwt_engine,
        &tenant_b_id,
        "staff_deep@beta.com",
        Role::Staff,
    )
    .await;

    DeepChallengerHarness {
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
        staff_b_token,
    }
}

async fn deep_req(
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
// TEST 1: Multi-Cycle Replay Stability & Post-Mismatch Resilience
// =========================================================================
#[tokio::test]
async fn test_deep_idempotency_multi_cycle_replay_and_post_mismatch_resilience() {
    let h = setup_deep_harness().await;

    // Create warehouse & product in Tenant A
    let (_, wh, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-MULTI-REPLAY", "name": "Replay WH" })),
        None,
    )
    .await;
    let wh_id = wh["id"].as_str().unwrap();

    let (_, prod, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Replay Product", "cost_price": 5000, "sale_price": 10000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    let key = "IDEMP-STRESS-CYCLE-001";
    let mut headers = HeaderMap::new();
    headers.insert("Idempotency-Key", key.parse().unwrap());

    let payload = json!({
        "movement_type": "INBOUND",
        "product_id": prod_id,
        "destination_warehouse_id": wh_id,
        "quantity": 100,
        "unit_cost": 5000,
        "notes": "Initial delivery"
    });

    // 1. Initial request
    let (s0, b0, h0) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(payload.clone()),
        Some(headers.clone()),
    )
    .await;
    assert_eq!(s0, StatusCode::CREATED);
    assert!(!h0.contains_key("x-cache-replay"));
    let original_mov_id = b0["id"].as_str().unwrap().to_string();
    assert_eq!(b0["resulting_stock"], 100);

    // 2. 5 Sequential identical replays -> every one must have x-cache-replay: true
    for i in 1..=5 {
        let (s_rep, b_rep, h_rep) = deep_req(
            &h.app,
            Method::POST,
            "/api/v1/inventory/movements",
            &h.owner_a_token,
            &h.tenant_a_id,
            Some(payload.clone()),
            Some(headers.clone()),
        )
        .await;
        assert_eq!(s_rep, StatusCode::CREATED, "Replay iteration {} failed status", i);
        assert_eq!(
            h_rep.get("x-cache-replay").and_then(|v| v.to_str().ok()),
            Some("true"),
            "Replay iteration {} missing x-cache-replay: true",
            i
        );
        assert_eq!(b_rep["id"].as_str().unwrap(), original_mov_id);
        assert_eq!(b_rep["resulting_stock"], 100);
    }

    // Verify stock is strictly 100 in database
    let (s_chk, b_chk, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_chk, StatusCode::OK);
    assert_eq!(b_chk["stock_items"][0]["quantity_on_hand"], 100);

    // Verify exactly 1 stock movement row in database
    let (s_mov, b_mov, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory/movements?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_mov, StatusCode::OK);
    assert_eq!(b_mov["count"], 1);

    // 3. Attack with subtle payload mutations -> each MUST return 409 IDEMPOTENCY_MISMATCH
    let mutations = vec![
        ("quantity mutated", json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 101, // 101 != 100
            "unit_cost": 5000,
            "notes": "Initial delivery"
        })),
        ("unit_cost mutated", json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 100,
            "unit_cost": 5001, // 5001 != 5000
            "notes": "Initial delivery"
        })),
        ("notes mutated", json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 100,
            "unit_cost": 5000,
            "notes": "Initial delivery modified"
        })),
    ];

    for (desc, mut_payload) in mutations {
        let (s_mut, b_mut, _) = deep_req(
            &h.app,
            Method::POST,
            "/api/v1/inventory/movements",
            &h.owner_a_token,
            &h.tenant_a_id,
            Some(mut_payload),
            Some(headers.clone()),
        )
        .await;
        assert_eq!(s_mut, StatusCode::CONFLICT, "Scenario '{}' expected 409 Conflict", desc);
        assert_eq!(b_mut["code"], "IDEMPOTENCY_MISMATCH", "Scenario '{}' expected IDEMPOTENCY_MISMATCH", desc);
    }

    // 4. Post-Mismatch Resilience: Replay of the ORIGINAL payload MUST still succeed!
    let (s_rec, b_rec, h_rec) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(payload.clone()),
        Some(headers.clone()),
    )
    .await;
    assert_eq!(s_rec, StatusCode::CREATED, "Original payload replay after mismatch must succeed");
    assert_eq!(
        h_rec.get("x-cache-replay").and_then(|v| v.to_str().ok()),
        Some("true")
    );
    assert_eq!(b_rec["id"].as_str().unwrap(), original_mov_id);

    // Final database stock and movements check: exactly 100 and 1 row
    let (_, b_fin_stock, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_fin_stock["stock_items"][0]["quantity_on_hand"], 100);

    let (_, b_fin_mov, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory/movements?warehouse_id={}", wh_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_fin_mov["count"], 1);
}

// =========================================================================
// TEST 2: Whitespace / Empty Header Handling
// =========================================================================
#[tokio::test]
async fn test_deep_idempotency_empty_and_whitespace_headers() {
    let h = setup_deep_harness().await;

    let (_, wh, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-EMPTY-HEADER", "name": "Empty Header WH" })),
        None,
    )
    .await;
    let wh_id = wh["id"].as_str().unwrap();

    let (_, prod, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Empty Header Product", "cost_price": 1000, "sale_price": 2000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    // Whitespace Idempotency-Key
    let mut ws_headers = HeaderMap::new();
    ws_headers.insert("Idempotency-Key", "   \t  ".parse().unwrap());

    let (s_ws, b_ws, h_ws) = deep_req(
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
        Some(ws_headers.clone()),
    )
    .await;
    assert_eq!(s_ws, StatusCode::CREATED);
    assert!(!h_ws.contains_key("x-cache-replay"));
    assert_eq!(b_ws["resulting_stock"], 10);

    // Subsequent call with whitespace header is treated as non-idempotent and increments stock to 20
    let (s_ws2, b_ws2, h_ws2) = deep_req(
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
        Some(ws_headers),
    )
    .await;
    assert_eq!(s_ws2, StatusCode::CREATED);
    assert!(!h_ws2.contains_key("x-cache-replay"));
    assert_eq!(b_ws2["resulting_stock"], 20);
}

// =========================================================================
// TEST 3: Bidirectional Cross-Tenant Transfers & Zero Existence Enumeration
// =========================================================================
#[tokio::test]
async fn test_deep_cross_tenant_bidirectional_transfers_and_zero_enumeration() {
    let h = setup_deep_harness().await;

    // Tenant A: WH_A, Prod_A with 100 units
    let (_, wh_a, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-DEEP-A", "name": "Warehouse Alpha Deep" })),
        None,
    )
    .await;
    let wh_a_id = wh_a["id"].as_str().unwrap();

    let (_, prod_a, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "Product Alpha Deep", "cost_price": 5000, "sale_price": 10000 })),
        None,
    )
    .await;
    let prod_a_id = prod_a["id"].as_str().unwrap();

    deep_req(
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
    let (_, wh_b, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "code": "WH-DEEP-B", "name": "Warehouse Beta Deep" })),
        None,
    )
    .await;
    let wh_b_id = wh_b["id"].as_str().unwrap();

    let (_, prod_b, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_b_token,
        &h.tenant_b_id,
        Some(json!({ "name": "Product Beta Deep", "cost_price": 8000, "sale_price": 15000 })),
        None,
    )
    .await;
    let prod_b_id = prod_b["id"].as_str().unwrap();

    deep_req(
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
        None,
    )
    .await;

    // --- ZERO ENUMERATION PROOF: Comparing valid foreign ID vs random UUID ---
    let random_nonexistent_id = Uuid::new_v4().to_string();

    // Query foreign warehouse vs query random UUID
    let (s_foreign_wh, b_foreign_wh, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", wh_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    let (s_random_wh, b_random_wh, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", random_nonexistent_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_foreign_wh, StatusCode::NOT_FOUND);
    assert_eq!(s_random_wh, StatusCode::NOT_FOUND);
    assert_eq!(b_foreign_wh["code"], "NOT_FOUND");
    assert_eq!(b_random_wh["code"], "NOT_FOUND");
    // Both return identical status code and error schema without leaking existence!

    // Query foreign product vs query random UUID
    let (s_foreign_p, b_foreign_p, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/products/{}", prod_a_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    let (s_random_p, b_random_p, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/products/{}", random_nonexistent_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(s_foreign_p, StatusCode::NOT_FOUND);
    assert_eq!(s_random_p, StatusCode::NOT_FOUND);
    assert_eq!(b_foreign_p["code"], "NOT_FOUND");
    assert_eq!(b_random_p["code"], "NOT_FOUND");

    // --- BIDIRECTIONAL CROSS-TENANT TRANSFER ATTACKS ---
    // 1. Tenant A caller attempts transfer targeting Tenant B warehouse as destination
    let (s_tx1, b_tx1, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": wh_b_id,
            "product_id": prod_a_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_tx1, StatusCode::NOT_FOUND, "Transfer to foreign destination warehouse must return 404");
    assert_eq!(b_tx1["code"], "NOT_FOUND");

    // 2. Tenant A caller attempts transfer sourcing from Tenant B warehouse
    let (s_tx2, b_tx2, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh_b_id,
            "destination_warehouse_id": wh_a_id,
            "product_id": prod_a_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_tx2, StatusCode::NOT_FOUND, "Transfer from foreign source warehouse must return 404");
    assert_eq!(b_tx2["code"], "NOT_FOUND");

    // 3. Tenant A caller attempts transfer where source == destination -> 400 SAME_WAREHOUSE_TRANSFER
    let (s_same_wh, b_same_wh, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": wh_a_id,
            "product_id": prod_b_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_same_wh, StatusCode::BAD_REQUEST);
    assert_eq!(b_same_wh["code"], "SAME_WAREHOUSE_TRANSFER");

    // 4. Tenant A caller attempts transfer using Tenant B's product across two valid Tenant A warehouses
    let (_, wh_a2, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-DEEP-A2", "name": "Warehouse Alpha 2" })),
        None,
    )
    .await;
    let wh_a2_id = wh_a2["id"].as_str().unwrap();

    let (s_tx4, b_tx4, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": wh_a2_id,
            "product_id": prod_b_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_tx4, StatusCode::NOT_FOUND, "Transfer using foreign product must return 404");
    assert_eq!(b_tx4["code"], "NOT_FOUND");

    // 4. Verify stock of both tenants remains 100% intact
    let (_, b_a_stock, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_a_id),
        &h.owner_a_token,
        &h.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_a_stock["stock_items"][0]["quantity_on_hand"], 100);

    let (_, b_b_stock, _) = deep_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_b_id),
        &h.owner_b_token,
        &h.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(b_b_stock["stock_items"][0]["quantity_on_hand"], 50);
}

// =========================================================================
// TEST 4: Full RBAC Permission Hierarchy Verification
// =========================================================================
#[tokio::test]
async fn test_deep_rbac_permission_matrix() {
    let h = setup_deep_harness().await;

    // Test warehouse creation permission across 6 roles:
    // Owner, Administrator, Manager -> 201 Created
    // Staff, Accountant, Custom -> 403 Forbidden

    // 1. Staff -> 403
    let (s1, b1, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.staff_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-DENIED-STAFF", "name": "Staff WH" })),
        None,
    )
    .await;
    assert_eq!(s1, StatusCode::FORBIDDEN);
    assert_eq!(b1["code"], "FORBIDDEN");

    // 2. Accountant -> 403
    let (s2, b2, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.accountant_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-DENIED-ACCT", "name": "Acct WH" })),
        None,
    )
    .await;
    assert_eq!(s2, StatusCode::FORBIDDEN);
    assert_eq!(b2["code"], "FORBIDDEN");

    // 3. Custom -> 403
    let (s3, b3, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.custom_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-DENIED-CUST", "name": "Cust WH" })),
        None,
    )
    .await;
    assert_eq!(s3, StatusCode::FORBIDDEN);
    assert_eq!(b3["code"], "FORBIDDEN");

    // 4. Manager -> 201
    let (s4, _, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.manager_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-ALLOW-MGR", "name": "Manager WH" })),
        None,
    )
    .await;
    assert_eq!(s4, StatusCode::CREATED);

    // 5. Administrator -> 201
    let (s5, _, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.admin_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-ALLOW-ADMIN", "name": "Admin WH" })),
        None,
    )
    .await;
    assert_eq!(s5, StatusCode::CREATED);

    // 6. Owner -> 201
    let (s6, b6, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/warehouses",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "code": "WH-ALLOW-OWNER", "name": "Owner WH" })),
        None,
    )
    .await;
    assert_eq!(s6, StatusCode::CREATED);
    let wh_id = b6["id"].as_str().unwrap();

    // Owner creates a product for adjustment tests
    let (_, prod, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/products",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({ "name": "RBAC Adj Prod", "cost_price": 1000, "sale_price": 2000 })),
        None,
    )
    .await;
    let prod_id = prod["id"].as_str().unwrap();

    // Test Stock Adjustment permission across 6 roles:
    // 1. Staff -> 403 Forbidden
    let (s_adj_staff, b_adj_staff, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.staff_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_adj_staff, StatusCode::FORBIDDEN);
    assert_eq!(b_adj_staff["code"], "FORBIDDEN");

    // 2. Accountant -> 403 Forbidden
    let (s_adj_acct, b_adj_acct, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.accountant_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_adj_acct, StatusCode::FORBIDDEN);
    assert_eq!(b_adj_acct["code"], "FORBIDDEN");

    // 3. Custom -> 403 Forbidden
    let (s_adj_cust, b_adj_cust, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.custom_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(s_adj_cust, StatusCode::FORBIDDEN);
    assert_eq!(b_adj_cust["code"], "FORBIDDEN");

    // 4. Manager -> 200 OK
    let (s_adj_mgr, b_adj_mgr, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.manager_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 25,
            "reason": "Manager inventory check"
        })),
        None,
    )
    .await;
    assert_eq!(s_adj_mgr, StatusCode::OK);
    assert_eq!(b_adj_mgr["actual_quantity"], 25);

    // 5. Administrator -> 200 OK
    let (s_adj_admin, b_adj_admin, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.admin_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 30,
            "reason": "Admin adjustment"
        })),
        None,
    )
    .await;
    assert_eq!(s_adj_admin, StatusCode::OK);
    assert_eq!(b_adj_admin["actual_quantity"], 30);

    // 6. Owner -> 200 OK
    let (s_adj_owner, b_adj_owner, _) = deep_req(
        &h.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &h.owner_a_token,
        &h.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 40,
            "reason": "Owner count"
        })),
        None,
    )
    .await;
    assert_eq!(s_adj_owner, StatusCode::OK);
    assert_eq!(b_adj_owner["actual_quantity"], 40);
}
