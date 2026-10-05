//! Integration Test Suite for Milestone 2:
//! Stock Movements Engine, Multi-Warehouse Architecture, and Atomic Negative Balance Prevention (R1, R2).

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

struct TestHarness {
    app: axum::Router,
    #[allow(dead_code)]
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("inventory_m2_test.sqlite");
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

    let jwt_secret = "m2_inventory_secret_key_1234567890_super_secret_test";
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
            email: "owner_a@warehouse.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_a@warehouse.com", "user", "premium")
        .unwrap();

    // 2. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_a@warehouse.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_a@warehouse.com", "user", "free")
        .unwrap();

    // 3. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Logistics".to_string(),
            slug: "tenant-alpha-logistics".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    // Seed COA for Tenant A so GL accounts 1300, 2000, 5000 exist
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
            email: "user_b@beta-wh.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "User B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "user_b@beta-wh.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Storage".to_string(),
            slug: "tenant-beta-storage".to_string(),
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
        staff_token,
        tenant_a_id,
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

#[tokio::test]
async fn test_multi_warehouse_crud_and_default_handling() {
    let harness = setup_harness().await;

    // 1. Create Main Warehouse (is_default = true)
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-MAIN",
            "name": "Main Distribution Center",
            "address": "Jl. Industri No. 1, Jakarta",
            "is_default": true
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Create WH-MAIN body: {:?}", body);
    assert_eq!(body["code"], "WH-MAIN");
    assert_eq!(body["is_default"], true);
    let wh_main_id = body["id"].as_str().unwrap().to_string();

    // 2. Create Branch Warehouse (is_default = false)
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-BRANCH",
            "name": "Branch Depot Surabaya",
            "address": "Jl. Perak Timur No. 10, Surabaya",
            "is_default": false
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["code"], "WH-BRANCH");
    assert_eq!(body["is_default"], false);

    // 3. Duplicate code rejection
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-MAIN",
            "name": "Duplicate Main"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 4. List warehouses
    let (status, body, _) = send_req(
        &harness.app,
        Method::GET,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 2);

    // 5. Get warehouse by ID
    let (status, body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", wh_main_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], wh_main_id);
    assert_eq!(body["is_default"], true);

    // 6. Create third warehouse with is_default = true -> unseats WH-MAIN as default
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-CENTRAL",
            "name": "Central Hub",
            "is_default": true
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["is_default"], true);

    // Verify WH-MAIN is no longer default
    let (status, body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", wh_main_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_default"], false);
}

#[tokio::test]
async fn test_strict_tenant_context_isolation_cross_tenant_404() {
    let harness = setup_harness().await;

    // Tenant A creates warehouse and product
    let (status, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-A1",
            "name": "Warehouse Alpha"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let wh_id = wh_body["id"].as_str().unwrap();

    let (status, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Alpha Widget",
            "cost_price": 5000,
            "sale_price": 10000
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let prod_id = prod_body["id"].as_str().unwrap();

    // Tenant B attempts to access Tenant A's warehouse -> strictly 404
    let (status, _, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", wh_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Tenant B attempts to access Tenant A's product -> strictly 404
    let (status, _, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/products/{}", prod_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Tenant B lists products -> does NOT see Tenant A's product
    let (status, body, _) = send_req(
        &harness.app,
        Method::GET,
        "/api/v1/products",
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 0);
}

#[tokio::test]
async fn test_rbac_staff_role_permissions() {
    let harness = setup_harness().await;

    // Staff attempts to create warehouse -> 403 Forbidden
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.staff_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-STAFF",
            "name": "Staff Warehouse"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Owner creates warehouse & product
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-BASE",
            "name": "Base Warehouse"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Base Product",
            "cost_price": 1000,
            "sale_price": 2000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // Staff attempts to post stock adjustment -> 403 Forbidden
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &harness.staff_token,
        &harness.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 50
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_sequential_sku_generation() {
    let harness = setup_harness().await;

    // Product 1: auto SKU
    let (status, body1, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Kopi Arabika 250g",
            "sale_price": 55000,
            "cost_price": 30000
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body1["sku"], "SKU-000001");

    // Product 2: auto SKU
    let (status, body2, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Kopi Robusta 250g",
            "sale_price": 40000,
            "cost_price": 20000
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body2["sku"], "SKU-000002");

    // Product 3: custom SKU
    let (status, body3, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "sku": "CUSTOM-SPECIAL-01",
            "name": "Special Blend",
            "sale_price": 75000,
            "cost_price": 45000
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body3["sku"], "CUSTOM-SPECIAL-01");

    // Duplicate custom SKU rejection
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "sku": "CUSTOM-SPECIAL-01",
            "name": "Duplicate Special"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_atomic_movements_wac_and_audit_trail() {
    let harness = setup_harness().await;

    // Create warehouse & product
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-JKT",
            "name": "Jakarta Central"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Beras Premium 5kg",
            "sale_price": 75000,
            "cost_price": 50000,
            "reorder_threshold": 10
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // 1. Inbound 20 units @ Rp 10.000
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 20,
            "unit_cost": 10000,
            "notes": "First batch delivery"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["resulting_stock"], 20);
    assert_eq!(body["average_cost"], 10000);

    // 2. Outbound 5 units
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["remaining_stock"], 15);

    // 3. Inbound 10 units @ Rp 25.000 (WAC recomputation)
    // (15 * 10.000 + 10 * 25.000) / 25 = 400.000 / 25 = 16.000
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10,
            "unit_cost": 25000
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["resulting_stock"], 25);
    assert_eq!(body["average_cost"], 16000);

    // 4. Query Stock Level
    let (status, stock_body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stock_body["count"], 1);
    assert_eq!(stock_body["stock_items"][0]["quantity_on_hand"], 25);
    assert_eq!(stock_body["stock_items"][0]["average_cost"], 16000);
    assert_eq!(stock_body["stock_items"][0]["is_low_stock"], false);

    // 5. Query Audit Trail
    let (status, mov_body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory/movements?product_id={}", prod_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mov_body["count"], 3);
}

#[tokio::test]
async fn test_negative_stock_prevention_rejection() {
    let harness = setup_harness().await;

    // Create warehouse & product with 5 units stock
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-TEST",
            "name": "Negative Test WH"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Limited Stock Item",
            "sale_price": 20000,
            "cost_price": 10000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // Inbound 5 units
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Attempt to deduct 6 units (exceeds balance 5) -> strictly 422
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 6
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INSUFFICIENT_STOCK");

    // Stock level must still be exactly 5
    let (status, stock_body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stock_body["stock_items"][0]["quantity_on_hand"], 5);
}

#[tokio::test]
async fn test_inter_warehouse_transfer_invariants() {
    let harness = setup_harness().await;

    // Create Warehouse A and Warehouse B
    let (_, wh_a_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "code": "WH-SRC", "name": "Source WH" })),
        None,
    )
    .await;
    let wh_a_id = wh_a_body["id"].as_str().unwrap();

    let (_, wh_b_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "code": "WH-DST", "name": "Destination WH" })),
        None,
    )
    .await;
    let wh_b_id = wh_b_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "name": "Transfer Widget", "sale_price": 50000, "cost_price": 30000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // Inbound 10 units to Warehouse A
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 10,
            "unit_cost": 30000
        })),
        None,
    )
    .await;

    // 1. Same-warehouse transfer rejection
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": wh_a_id,
            "product_id": prod_id,
            "quantity": 2
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "SAME_WAREHOUSE_TRANSFER");

    // 2. Successful atomic transfer of 4 units
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/transfer",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": wh_b_id,
            "product_id": prod_id,
            "quantity": 4
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "COMPLETED");
    assert_eq!(body["source_remaining"], 6);
    assert_eq!(body["destination_total"], 4);

    // Verify stock at both warehouses
    let (_, stock_a, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_a_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_a["stock_items"][0]["quantity_on_hand"], 6);

    let (_, stock_b, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_b_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_b["stock_items"][0]["quantity_on_hand"], 4);

    // 3. Attempt transfer exceeding source stock (attempt 10 when only 6 available) -> 422
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/transfers",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "source_warehouse_id": wh_a_id,
            "destination_warehouse_id": wh_b_id,
            "product_id": prod_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "INSUFFICIENT_STOCK");
}

#[tokio::test]
async fn test_stock_adjustments_and_sequential_numbering() {
    let harness = setup_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "code": "WH-ADJ", "name": "Adjustment WH" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "name": "Counted Item", "sale_price": 10000, "cost_price": 5000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // Initial stock: 10 units via inbound
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10
        })),
        None,
    )
    .await;

    // 1. Adjustment 1: physical count shows 14 (+4 variance)
    let (status, body1, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 14,
            "reason": "Quarterly physical count surplus"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body1["adjustment_number"].as_str().unwrap().starts_with("ADJ-"));
    assert!(body1["adjustment_number"].as_str().unwrap().ends_with("-000001"));
    assert_eq!(body1["previous_quantity"], 10);
    assert_eq!(body1["actual_quantity"], 14);
    assert_eq!(body1["variance"], 4);

    // 2. Adjustment 2: physical count shows 11 (-3 variance)
    let (status, body2, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 11,
            "reason": "Damaged goods write-off"
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body2["adjustment_number"].as_str().unwrap().ends_with("-000002"));
    assert_eq!(body2["previous_quantity"], 14);
    assert_eq!(body2["actual_quantity"], 11);
    assert_eq!(body2["variance"], -3);

    // 3. Negative actual quantity rejection
    let (status, body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjust",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": -5
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "NEGATIVE_STOCK_PROHIBITED");
}

#[tokio::test]
async fn test_idempotency_key_replay_and_mismatch() {
    let harness = setup_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "code": "WH-IDEMP", "name": "Idempotency WH" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "name": "Idempotent Product", "sale_price": 10000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    let key = "idemp_key_inbound_9999";
    let mut headers = HeaderMap::new();
    headers.insert("Idempotency-Key", key.parse().unwrap());

    let payload = json!({
        "movement_type": "INBOUND",
        "product_id": prod_id,
        "destination_warehouse_id": wh_id,
        "quantity": 10,
        "unit_cost": 5000
    });

    // 1. Initial request -> 201 Created
    let (status, body1, resp_headers1) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(payload.clone()),
        Some(headers.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(!resp_headers1.contains_key("x-cache-replay"));

    // 2. Replay with identical key & payload -> cached response
    let (status, body2, resp_headers2) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(payload),
        Some(headers.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(resp_headers2.get("x-cache-replay").unwrap(), "true");
    assert_eq!(body1["id"], body2["id"]);

    // Verify stock was incremented ONCE (10 units, not 20)
    let (_, stock, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock["stock_items"][0]["quantity_on_hand"], 10);

    // 3. Different payload with same key -> 409 Conflict
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 50
        })),
        Some(headers),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_concurrent_racing_stock_movements_zero_overselling() {
    let harness = setup_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "code": "WH-RACE", "name": "Racing WH" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap().to_string();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "name": "Concurrently Sold Item", "sale_price": 50000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    // Inbound exactly 10 units
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10
        })),
        None,
    )
    .await;

    // Spawn 10 concurrent requests, each attempting to deduct 2 units (20 total requested)
    let app = harness.app.clone();
    let token = harness.owner_token.clone();
    let tenant_id = harness.tenant_a_id.clone();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let app_clone = app.clone();
        let token_clone = token.clone();
        let tenant_id_clone = tenant_id.clone();
        let prod_id_clone = prod_id.clone();
        let wh_id_clone = wh_id.clone();

        handles.push(tokio::spawn(async move {
            let mut attempts = 0;
            loop {
                attempts += 1;
                let (status, body, _) = send_req(
                    &app_clone,
                    Method::POST,
                    "/api/v1/inventory/movements",
                    &token_clone,
                    &tenant_id_clone,
                    Some(json!({
                        "movement_type": "OUTBOUND",
                        "product_id": prod_id_clone,
                        "source_warehouse_id": wh_id_clone,
                        "quantity": 2
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY {
                    return (status, body);
                }

                if status == StatusCode::CONFLICT && body["code"] == "LOCK_CONTENTION" && attempts < 20 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(15 * attempts)).await;
                    continue;
                }

                return (status, body);
            }
        }));
    }

    let mut success_count = 0;
    let mut insufficient_count = 0;

    for handle in handles {
        let (status, body) = handle.await.unwrap();
        if status == StatusCode::CREATED {
            success_count += 1;
        } else if status == StatusCode::UNPROCESSABLE_ENTITY {
            assert_eq!(body["code"], "INSUFFICIENT_STOCK");
            insufficient_count += 1;
        }
    }

    // Exactly 5 requests must succeed (5 * 2 = 10 units deducted), exactly 5 must be rejected
    assert_eq!(success_count, 5, "Exactly 5 deductions of 2 units should succeed");
    assert_eq!(insufficient_count, 5, "Remaining 5 deductions must fail with INSUFFICIENT_STOCK");

    // Final stock on hand MUST be exactly 0
    let (_, stock, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(
        stock["stock_items"][0]["quantity_on_hand"], 0,
        "Zero overselling invariant: final stock on hand must be exactly 0"
    );
}

#[tokio::test]
async fn test_reorder_threshold_and_low_stock_filters() {
    let harness = setup_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({ "code": "WH-LOW", "name": "Low Stock WH" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    // Product with threshold = 5
    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Threshold Item",
            "sale_price": 10000,
            "cost_price": 5000,
            "reorder_threshold": 5
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // Inbound 10 units (10 > 5 -> not low stock)
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10
        })),
        None,
    )
    .await;

    // Filter low stock -> returns 0 items
    let (status, body, _) = send_req(
        &harness.app,
        Method::GET,
        "/api/v1/inventory?low_stock=true",
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 0);

    // Outbound 6 units -> remaining 4 (4 <= 5 -> IS low stock)
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 6
        })),
        None,
    )
    .await;

    // Filter low stock -> returns 1 item
    let (status, body, _) = send_req(
        &harness.app,
        Method::GET,
        "/api/v1/inventory/stock?low_stock=true",
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 1);
    assert_eq!(body["stock_items"][0]["quantity_on_hand"], 4);
    assert_eq!(body["stock_items"][0]["is_low_stock"], true);
}

// =============================================================================
// Milestone 3 Tests: Purchase Orders, Receipts & WAC GL Accounting (R3, R4)
// =============================================================================

#[tokio::test]
async fn test_purchase_order_lifecycle_and_sequential_numbering() {
    let harness = setup_harness().await;

    // 1. Create Warehouse and Product
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-PO-1",
            "name": "PO Warehouse 1",
            "is_default": true
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Beras Premium",
            "unit": "kg",
            "cost_price": 50000,
            "sale_price": 60000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // 2. Create First PO (PO-YYYY-000001)
    let (status, po1_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "PT Pangan Mandiri",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 50,
                    "unit_cost": 48000
                }
            ],
            "notes": "Pesanan pertama"
        })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(po1_body["status"], "DRAFT");
    assert_eq!(po1_body["total_amount"], 2400000); // 50 * 48000
    let po1_num = po1_body["po_number"].as_str().unwrap();
    assert!(po1_num.starts_with("PO-"));
    assert!(po1_num.ends_with("000001"));
    let po1_id = po1_body["id"].as_str().unwrap();

    // 3. Create Second PO (PO-YYYY-000002)
    let (status, po2_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "CV Beras Jaya",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 20,
                    "unit_cost": 49000
                }
            ]
        })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let po2_num = po2_body["po_number"].as_str().unwrap();
    assert!(po2_num.ends_with("000002"));

    // 4. Order First PO: DRAFT -> ORDERED
    let (status, ord_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po1_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ord_body["status"], "ORDERED");

    // 5. Ordering again is rejected with 422 PO_NOT_DRAFT
    let (status, err_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po1_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_body["code"], "PO_NOT_DRAFT");
}

#[tokio::test]
async fn test_goods_receipt_wac_recalculation_and_journal_posting() {
    let harness = setup_harness().await;

    // 1. Warehouse & Product
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-RCV",
            "name": "Receiving Warehouse"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Minyak Goreng 2L",
            "unit": "pouch",
            "cost_price": 25000,
            "sale_price": 32000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // Seed initial stock: 10 units @ 20,000
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10,
            "unit_cost": 20000
        })),
        None,
    )
    .await;

    // 2. Create and Order PO: 100 units @ 30,000
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "Pabrik Minyak Sawit",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 100,
                    "unit_cost": 30000
                }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({})),
        None,
    )
    .await;

    // 3. Partial Goods Receipt: 40 units @ 30,000 = 1,200,000
    let (status, rcv1_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 40,
                    "batch_number": "BATCH-MY-001"
                }
            ]
        })),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(rcv1_body["status"], "PARTIALLY_RECEIVED");
    assert_eq!(rcv1_body["total_receipt_value"], 1200000);
    let j_id1 = rcv1_body["journal_entry_id"].as_str().unwrap();

    // Verify stock & WAC recalculation:
    // prev: 10 @ 20,000 = 200,000
    // in:   40 @ 30,000 = 1,200,000
    // total: 50 units, total value 1,400,000 -> WAC = 1,400,000 / 50 = 28,000
    let (_, stock_body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}&product_id={}", wh_id, prod_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_body["stock_items"][0]["quantity_on_hand"], 50);
    assert_eq!(stock_body["stock_items"][0]["average_cost"], 28000);

    // Verify Balanced GL Journal: Debit 1300 = 1,200,000 / Credit 2000 = 1,200,000
    let (status, jrn_body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/accounting/journals/{}", j_id1),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(jrn_body["source_type"], "PURCHASE_ORDER_RECEIPT");
    assert_eq!(jrn_body["total_debit"], 1200000);
    assert_eq!(jrn_body["total_credit"], 1200000);

    // Verify Transactional Outbox Event: StockReceived
    let outbox_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_type, aggregate_id FROM outbox_events WHERE tenant_id = ?1 AND aggregate_type = 'PurchaseOrder'"
    )
    .bind(&harness.tenant_a_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();
    assert!(outbox_rows.iter().any(|(e, agg)| e == "StockReceived" && agg == po_id));

    // 4. Over-receipt rejection: remaining is 60, trying to receive 61
    let (status, err_over, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 61
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_over["code"], "QUANTITY_EXCEEDS_ORDERED");

    // 5. Complete Final Receipt: remaining 60 units @ 30,000 = 1,800,000
    let (status, rcv2_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 60
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rcv2_body["status"], "RECEIVED");

    // 6. Further receipt on RECEIVED PO rejected with 422 PO_ALREADY_RECEIVED
    let (status, err_recvd, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 1
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_recvd["code"], "PO_ALREADY_RECEIVED");
}

#[tokio::test]
async fn test_outbound_movement_cogs_journal_posting() {
    let harness = setup_harness().await;

    // 1. Warehouse & Product
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-COGS",
            "name": "COGS Warehouse"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Kopi Arabika 1kg",
            "unit": "pack",
            "cost_price": 100000,
            "sale_price": 150000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // 2. Inbound stock: 20 units @ 100,000
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 20,
            "unit_cost": 100000
        })),
        None,
    )
    .await;

    // 3. Outbound fulfillment: 5 units (COGS = 5 * 100,000 = 500,000)
    let (status, out_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(out_body["remaining_stock"], 15);

    // 4. Verify COGS Journal posted in Accounting Subledger
    let (_, jrns_body, _) = send_req(
        &harness.app,
        Method::GET,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    let cogs_jrn = jrns_body["journals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["source_type"] == "INVENTORY_OUTBOUND")
        .expect("Must find INVENTORY_OUTBOUND COGS journal");

    assert_eq!(cogs_jrn["total_debit"], 500000);
    assert_eq!(cogs_jrn["total_credit"], 500000);

    let lines = cogs_jrn["lines"].as_array().unwrap();
    let d_5000 = lines
        .iter()
        .find(|l| l["account_code"] == "5000")
        .unwrap()["debit"]
        .as_i64()
        .unwrap();
    let c_1300 = lines
        .iter()
        .find(|l| l["account_code"] == "1300")
        .unwrap()["credit"]
        .as_i64()
        .unwrap();
    assert_eq!(d_5000, 500000);
    assert_eq!(c_1300, 500000);
}

#[tokio::test]
async fn test_purchase_order_cancellation_rules() {
    let harness = setup_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-CAN",
            "name": "Cancel Test Warehouse"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Teh Botol Kotak",
            "unit": "dus",
            "cost_price": 40000,
            "sale_price": 50000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // 1. Cancel on fresh DRAFT -> allowed
    let (_, po_draft, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "Supplier Batal 1",
            "destination_warehouse_id": wh_id,
            "items": [{"product_id": prod_id, "quantity_ordered": 10, "unit_cost": 40000}]
        })),
        None,
    )
    .await;
    let po_d_id = po_draft["id"].as_str().unwrap();

    let (status, can_res, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_d_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({"reason": "Salah input"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(can_res["status"], "CANCELLED");

    // 2. Cancel on ORDERED (0 received) -> allowed
    let (_, po_ord, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "Supplier Batal 2",
            "destination_warehouse_id": wh_id,
            "items": [{"product_id": prod_id, "quantity_ordered": 10, "unit_cost": 40000}]
        })),
        None,
    )
    .await;
    let po_o_id = po_ord["id"].as_str().unwrap();
    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_o_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({})),
        None,
    )
    .await;

    let (status, can_ord_res, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_o_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(can_ord_res["status"], "CANCELLED");

    // Attempt receipt on CANCELLED -> 422 PO_CANCELLED
    let (status, err_can, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_o_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [{"product_id": prod_id, "quantity_received": 5}]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_can["code"], "PO_CANCELLED");

    // 3. Cancel on PARTIALLY_RECEIVED -> rejected with 422 CANNOT_CANCEL_RECEIVED_PO
    let (_, po_pr, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "Supplier Parsial",
            "destination_warehouse_id": wh_id,
            "items": [{"product_id": prod_id, "quantity_ordered": 20, "unit_cost": 40000}]
        })),
        None,
    )
    .await;
    let po_pr_id = po_pr["id"].as_str().unwrap();
    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_pr_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({})),
        None,
    )
    .await;
    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_pr_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [{"product_id": prod_id, "quantity_received": 5}]
        })),
        None,
    )
    .await;

    let (status, err_pr_can, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_pr_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_pr_can["code"], "CANNOT_CANCEL_RECEIVED_PO");
}

#[tokio::test]
async fn test_anti_enumeration_and_rbac_purchase_orders() {
    let harness = setup_harness().await;

    // Create WH and Product in Tenant A
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-SEC",
            "name": "Security WH"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Barang Rahasia",
            "unit": "unit",
            "cost_price": 50000,
            "sale_price": 75000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    // Create PO in Tenant A
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "Supplier A",
            "destination_warehouse_id": wh_id,
            "items": [{"product_id": prod_id, "quantity_ordered": 10, "unit_cost": 50000}]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap();

    // --- Anti-Enumeration: Cross-Tenant access from Tenant B strictly returns 404 NOT_FOUND ---
    let (status, _, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/purchase-orders/{}", po_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({"items": [{"product_id": prod_id, "quantity_received": 5}]})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // --- RBAC: Staff role in Tenant A denied for mutations (403 Forbidden) ---
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.staff_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "Supplier Staff",
            "destination_warehouse_id": wh_id,
            "items": [{"product_id": prod_id, "quantity_ordered": 10, "unit_cost": 50000}]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.staff_token,
        &harness.tenant_a_id,
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.staff_token,
        &harness.tenant_a_id,
        Some(json!({"items": [{"product_id": prod_id, "quantity_received": 5}]})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.staff_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_idempotency_key_replay_goods_receipt() {
    let harness = setup_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "code": "WH-IDEMP",
            "name": "Idempotency Warehouse"
        })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "name": "Produk Idemp",
            "unit": "pcs",
            "cost_price": 10000,
            "sale_price": 15000
        })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap();

    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "supplier_name": "Supplier Idemp",
            "destination_warehouse_id": wh_id,
            "items": [{"product_id": prod_id, "quantity_ordered": 50, "unit_cost": 10000}]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({})),
        None,
    )
    .await;

    let mut idemp_headers = HeaderMap::new();
    idemp_headers.insert("Idempotency-Key", "idemp-rcv-key-001".parse().unwrap());

    // First request
    let (status1, body1, headers1) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [{"product_id": prod_id, "quantity_received": 15}]
        })),
        Some(idemp_headers.clone()),
    )
    .await;
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(body1["status"], "PARTIALLY_RECEIVED");
    assert!(!headers1.contains_key("x-cache-replay"));

    // Second (replayed) request with identical Idempotency-Key
    let (status2, body2, headers2) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        Some(json!({
            "items": [{"product_id": prod_id, "quantity_received": 15}]
        })),
        Some(idemp_headers),
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["id"], body1["id"]);
    assert_eq!(headers2.get("x-cache-replay").unwrap(), "true");

    // Stock must only have increased once (+15), not twice (+30)
    let (_, stock_body, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}&product_id={}", wh_id, prod_id),
        &harness.owner_token,
        &harness.tenant_a_id,
        None,
        None,
    )
    .await;
    assert_eq!(stock_body["stock_items"][0]["quantity_on_hand"], 15);
}

