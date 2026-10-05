//! Empirical Adversarial Challenger Test Suite for Milestone 3:
//! Purchase Order Lifecycle State Machine, Boundary Validation, and Concurrent Receipt Racing (R3, R4).
//!
//! Agent: teamwork_preview_challenger_m3_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Rejection of receipt on DRAFT or CANCELLED PO (HTTP 422).
//! 2. Strict cancellation blocking once items are partially or fully received (HTTP 422 CANNOT_CANCEL_RECEIVED_PO).
//! 3. Over-receipt prevention: attempting to receive quantity exceeding `quantity_ordered - quantity_received`.
//! 4. Boundary tests: negative, zero, and out-of-order quantity receipt requests.
//! 5. Concurrent racing receipts against the same PO item across multiple tokio tasks:
//!    - Total received quantity strictly matches the valid sum of accepted receipts and never exceeds ordered quantity.
//!    - Terminal state transition RECEIVED occurs cleanly without duplicate completion journals.
//! 6. Concurrent receipt vs cancellation race condition hardening.
//! 7. Cross-tenant anti-enumeration isolation on PO receipt and cancellation.

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
use tokio::sync::Barrier;
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
    let db_path = dir.path().join("challenger_m3_po_receipt_test.sqlite");
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

    let jwt_secret = "challenger_m3_secret_key_1234567890_super_secure_entropy";
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
            email: "owner_a_challenger@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_a_id, "owner_a_challenger@test.com", "user", "premium")
        .unwrap();

    let tenant_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_id.clone(),
            name: "Tenant Alpha Challenger Corp".to_string(),
            slug: "tenant-alpha-challenger-corp".to_string(),
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

    // Tenant B Owner (for anti-enumeration tests)
    let owner_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_b_id.clone(),
            email: "owner_b_challenger@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&owner_b_id, "owner_b_challenger@test.com", "user", "premium")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Challenger Corp".to_string(),
            slug: "tenant-beta-challenger-corp".to_string(),
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

// Helper to seed a standard warehouse and product
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
// CHALLENGE 1: Rejection of receipt on DRAFT or CANCELLED PO (HTTP 422)
// =========================================================================
#[tokio::test]
async fn test_challenge_1_receipt_rejected_on_draft_and_cancelled_po() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C1", "Barang Lifecycle", 15000).await;

    // 1. Create a PO in DRAFT status
    let (status, po_draft, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Supplier Draft",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 50,
                    "unit_cost": 15000
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(po_draft["status"], "DRAFT");
    let po_id = po_draft["id"].as_str().unwrap();

    // 2. Attempt receipt on DRAFT PO -> Must fail with HTTP 422 PO_NOT_ORDERED
    let (status, err_draft, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 10
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "Receipt on DRAFT PO must return HTTP 422");
    assert_eq!(err_draft["code"], "PO_NOT_ORDERED");

    // Verify zero stock movements and zero journals were posted
    let mov_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movements WHERE reference_id = ?1")
        .bind(po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(mov_count, 0, "No stock movements should exist for failed receipt on DRAFT");

    let journal_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries WHERE source_id = ?1")
        .bind(po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(journal_count, 0, "No journal entries should exist for failed receipt on DRAFT");

    // 3. Cancel the DRAFT PO directly
    let (status, can_res, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({"reason": "Batalkan draft"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(can_res["status"], "CANCELLED");

    // 4. Attempt receipt on CANCELLED PO -> Must fail with HTTP 422 PO_CANCELLED
    let (status, err_can, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 10
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "Receipt on CANCELLED PO must return HTTP 422");
    assert_eq!(err_can["code"], "PO_CANCELLED");

    // 5. Test another PO that was ORDERED then CANCELLED (0 received items)
    let (_, po_ord_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Supplier Ordered Cancel",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 20,
                    "unit_cost": 15000
                }
            ]
        })),
        None,
    )
    .await;
    let po_ord_id = po_ord_body["id"].as_str().unwrap();

    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_ord_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, can_ord_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_ord_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(can_ord_body["status"], "CANCELLED");

    // Attempt receipt on this cancelled PO
    let (status, err_can2, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_ord_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 5
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_can2["code"], "PO_CANCELLED");
}

// =========================================================================
// CHALLENGE 2: Strict cancellation blocking once items are partially or fully received
// =========================================================================
#[tokio::test]
async fn test_challenge_2_strict_cancellation_blocking_on_received_po() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C2", "Barang Cancel Block", 20000).await;

    // Create & Order PO with 25 units
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Anti Cancel",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 25,
                    "unit_cost": 20000
                }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap();

    let (status, ord_res, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ord_res["status"], "ORDERED");

    // 1. Partial Receipt: 10 units received
    let (status, rcv_res, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 10
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rcv_res["status"], "PARTIALLY_RECEIVED");

    // 2. Attempt to cancel PARTIALLY_RECEIVED PO -> Strictly BLOCKED with HTTP 422 CANNOT_CANCEL_RECEIVED_PO
    let (status, err_cancel_part, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({"reason": "Cancel partially received"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "Cancelling PARTIALLY_RECEIVED PO must return HTTP 422");
    assert_eq!(err_cancel_part["code"], "CANNOT_CANCEL_RECEIVED_PO");

    // 3. Receive remaining 15 units -> status transitions to RECEIVED
    let (status, rcv_full_res, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 15
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rcv_full_res["status"], "RECEIVED");

    // 4. Attempt to cancel fully RECEIVED PO -> Strictly BLOCKED with HTTP 422 CANNOT_CANCEL_RECEIVED_PO
    let (status, err_cancel_full, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({"reason": "Cancel fully received"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "Cancelling RECEIVED PO must return HTTP 422");
    assert_eq!(err_cancel_full["code"], "CANNOT_CANCEL_RECEIVED_PO");

    // 5. Inspect database to verify status remained strictly 'RECEIVED'
    let db_status: String = sqlx::query_scalar("SELECT status FROM purchase_orders WHERE id = ?1")
        .bind(po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(db_status, "RECEIVED", "PO status in database must remain RECEIVED");
}

// =========================================================================
// CHALLENGE 3: Over-receipt prevention: attempting to receive > remaining ordered
// =========================================================================
#[tokio::test]
async fn test_challenge_3_over_receipt_prevention() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C3", "Barang Over-receipt", 30000).await;

    // Create & Order PO with 40 units
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Supply Tepat",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 40,
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
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;

    // Scenario A: Attempt to receive 41 units (exceeding initial 40)
    let (status, err_over1, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 41
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_over1["code"], "QUANTITY_EXCEEDS_ORDERED");

    // Scenario B: Valid partial receipt of 25 units (remaining = 15)
    let (status, rcv_part, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 25
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rcv_part["status"], "PARTIALLY_RECEIVED");

    // Scenario C: Attempt to receive 16 units when only 15 remain
    let (status, err_over2, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 16
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_over2["code"], "QUANTITY_EXCEEDS_ORDERED");

    // Scenario D: Valid receipt of exact remaining 15 units
    let (status, rcv_comp, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 15
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rcv_comp["status"], "RECEIVED");

    // Scenario E: Attempt any receipt after PO is fully RECEIVED -> HTTP 422 PO_ALREADY_RECEIVED
    let (status, err_already, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
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
    assert_eq!(err_already["code"], "PO_ALREADY_RECEIVED");

    // Scenario F: Verify that database quantity_received is strictly 40 and not 41+
    let db_received: i64 = sqlx::query_scalar("SELECT quantity_received FROM purchase_order_items WHERE purchase_order_id = ?1")
        .bind(po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(db_received, 40);
}

// =========================================================================
// CHALLENGE 4: Boundary tests: negative, zero, and malformed quantity receipts
// =========================================================================
#[tokio::test]
async fn test_challenge_4_boundary_quantity_and_malformed_receipt_attempts() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C4", "Barang Boundary", 10000).await;

    // Create & Order PO with 20 units
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Boundary Supply",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 20,
                    "unit_cost": 10000
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
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;

    // 1. Zero quantity receipt: quantity_received = 0 -> HTTP 400 INVALID_QUANTITY
    let (status, err_zero, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 0
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Zero quantity receipt must return HTTP 400");
    assert_eq!(err_zero["code"], "INVALID_QUANTITY");

    // 2. Negative quantity receipt: quantity_received = -5 -> HTTP 400 INVALID_QUANTITY
    let (status, err_neg, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": -5
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Negative quantity receipt must return HTTP 400");
    assert_eq!(err_neg["code"], "INVALID_QUANTITY");

    // 3. Empty items array: items = [] -> HTTP 400 EMPTY_ITEMS
    let (status, err_empty, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": []
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Empty items array must return HTTP 400");
    assert_eq!(err_empty["code"], "EMPTY_ITEMS");

    // 4. Product not in PO lines -> HTTP 422 INVALID_PRODUCT_LINE
    let bogus_prod_id = Uuid::new_v4().to_string();
    let (status, err_bogus, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": bogus_prod_id,
                    "quantity_received": 5
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "Unrecognized product line must return HTTP 422");
    assert_eq!(err_bogus["code"], "INVALID_PRODUCT_LINE");
}

// =========================================================================
// CHALLENGE 5: Massive Concurrent Racing Receipts Across Multiple Tokio Tasks
// 20 concurrent tasks racing to receive 10 units each on ordered 100 (200 attempted).
// Invariant: Exactly 10 succeed, total received strictly 100, no oversell,
// clean terminal RECEIVED transition, exactly 10 balanced journals, 10 outbox events.
// =========================================================================
#[tokio::test]
async fn test_challenge_5_concurrent_racing_receipts_oversubscription() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C5", "Barang Race 100", 25000).await;

    // Create PO with 100 units
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Concurrency Race Master",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 100,
                    "unit_cost": 25000
                }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap().to_string();

    // Transition to ORDERED
    let (status, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Prepare 20 concurrent tasks racing with 10 units each
    let total_tasks = 20;
    let barrier = Arc::new(Barrier::new(total_tasks));
    let mut handles = Vec::new();

    for i in 0..total_tasks {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_id.clone();
        let po_id = po_id.clone();
        let prod_id = prod_id.clone();
        let barrier = barrier.clone();

        let handle = tokio::spawn(async move {
            barrier.wait().await;
            send_req(
                &app,
                Method::POST,
                &format!("/api/v1/purchase-orders/{}/receive", po_id),
                &token,
                &tenant_id,
                Some(json!({
                    "items": [
                        {
                            "product_id": prod_id,
                            "quantity_received": 10,
                            "batch_number": format!("BATCH-RACE-{}", i)
                        }
                    ]
                })),
                None,
            )
            .await
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    let mut rejected_count = 0;
    let mut other_errors = 0;

    for handle in handles {
        let (status, body, _) = handle.await.unwrap();
        if status == StatusCode::OK {
            success_count += 1;
        } else if status == StatusCode::UNPROCESSABLE_ENTITY {
            let code = body["code"].as_str().unwrap_or("");
            if code == "QUANTITY_EXCEEDS_ORDERED" || code == "PO_ALREADY_RECEIVED" {
                rejected_count += 1;
            } else {
                eprintln!("Unexpected 422 error code: {}", body);
                other_errors += 1;
            }
        } else {
            eprintln!("Unexpected status: {} body: {}", status, body);
            other_errors += 1;
        }
    }

    assert_eq!(other_errors, 0, "No unexpected errors or SQLite lock failures allowed");
    assert_eq!(success_count, 10, "Exactly 10 receipts of 10 units each must succeed");
    assert_eq!(rejected_count, 10, "Exactly 10 over-subscription receipts must be rejected with 422");

    // Invariant Verification in Database
    // 1. PO Status and Item Received
    let po_row = sqlx::query("SELECT status FROM purchase_orders WHERE id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let final_status: String = po_row.get("status");
    assert_eq!(final_status, "RECEIVED", "PO must reach terminal RECEIVED status");

    let item_received: i64 = sqlx::query_scalar("SELECT quantity_received FROM purchase_order_items WHERE purchase_order_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(item_received, 100, "purchase_order_items.quantity_received must be strictly 100");

    // 2. Stock Items on Hand
    let stock_qoh: i64 = sqlx::query_scalar("SELECT quantity_on_hand FROM stock_items WHERE product_id = ?1")
        .bind(&prod_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(stock_qoh, 100, "stock_items.quantity_on_hand must strictly equal 100");

    // 3. Stock movements count and sum
    let movement_sum: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(quantity), 0) FROM stock_movements WHERE reference_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(movement_sum, 100, "Sum of stock movements must strictly equal 100");

    let movement_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movements WHERE reference_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(movement_count, 10, "Exactly 10 stock movement records must be recorded");

    // 4. Balanced double-entry GL journals
    let journal_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries WHERE source_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(journal_count, 10, "Exactly 10 journal entries must be posted");

    // Verify each journal is perfectly balanced: Debit 1300 = Credit 2000 = 250,000 IDR (10 * 25,000)
    let journal_lines = sqlx::query(
        "SELECT jl.account_code, SUM(jl.debit) as total_debit, SUM(jl.credit) as total_credit
         FROM journal_lines jl
         JOIN journal_entries je ON jl.journal_id = je.id
         WHERE je.source_id = ?1
         GROUP BY jl.account_code"
    )
    .bind(&po_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    let mut total_1300_debit: i64 = 0;
    let mut total_2000_credit: i64 = 0;
    for row in journal_lines {
        let code: String = row.get("account_code");
        let debit: i64 = row.get("total_debit");
        let credit: i64 = row.get("total_credit");
        if code == "1300" {
            total_1300_debit += debit;
        } else if code == "2000" {
            total_2000_credit += credit;
        }
    }
    assert_eq!(total_1300_debit, 2_500_000, "Total debit to 1300 must be 100 * 25,000 = 2,500,000 IDR");
    assert_eq!(total_2000_credit, 2_500_000, "Total credit to 2000 must be 100 * 25,000 = 2,500,000 IDR");

    // 5. Outbox events count
    let outbox_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1 AND event_type = 'StockReceived'")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(outbox_count, 10, "Exactly 10 StockReceived outbox events must be emitted");
}

// =========================================================================
// CHALLENGE 6: Concurrent Full-Batch Race (Simultaneous 100% Completion)
// 10 concurrent tasks, each attempting to receive 100% of the PO (50 units).
// Invariant: Exactly ONE succeeds, 9 fail with PO_ALREADY_RECEIVED.
// Terminal state RECEIVED occurs cleanly without duplicate completion journals.
// =========================================================================
#[tokio::test]
async fn test_challenge_6_concurrent_full_batch_racing_single_winner() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C6", "Barang Full Race", 50000).await;

    // Create & Order PO with 50 units
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Full Batch Race",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 50,
                    "unit_cost": 50000
                }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap().to_string();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;

    // 10 concurrent tasks racing to receive all 50 units
    let total_tasks = 10;
    let barrier = Arc::new(Barrier::new(total_tasks));
    let mut handles = Vec::new();

    for _ in 0..total_tasks {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_id.clone();
        let po_id = po_id.clone();
        let prod_id = prod_id.clone();
        let barrier = barrier.clone();

        let handle = tokio::spawn(async move {
            barrier.wait().await;
            send_req(
                &app,
                Method::POST,
                &format!("/api/v1/purchase-orders/{}/receive", po_id),
                &token,
                &tenant_id,
                Some(json!({
                    "items": [
                        {
                            "product_id": prod_id,
                            "quantity_received": 50
                        }
                    ]
                })),
                None,
            )
            .await
        });
        handles.push(handle);
    }

    let mut winners = 0;
    let mut losers = 0;

    for handle in handles {
        let (status, body, _) = handle.await.unwrap();
        if status == StatusCode::OK {
            winners += 1;
            assert_eq!(body["status"], "RECEIVED");
        } else if status == StatusCode::UNPROCESSABLE_ENTITY {
            losers += 1;
            assert_eq!(body["code"], "PO_ALREADY_RECEIVED");
        } else {
            panic!("Unexpected status {} body {}", status, body);
        }
    }

    assert_eq!(winners, 1, "Exactly ONE task must succeed in receiving full batch");
    assert_eq!(losers, 9, "All 9 losing tasks must be rejected with PO_ALREADY_RECEIVED");

    // Exactly 1 journal entry
    let journal_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entries WHERE source_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(journal_count, 1, "Exactly ONE completion journal entry must exist, zero duplicates");

    // Quantity on hand is strictly 50
    let stock_qoh: i64 = sqlx::query_scalar("SELECT quantity_on_hand FROM stock_items WHERE product_id = ?1")
        .bind(&prod_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(stock_qoh, 50);
}

// =========================================================================
// CHALLENGE 7: Concurrent Receipt vs Cancellation Race
// One task attempts receipt while another attempts cancellation on an ORDERED PO.
// Exactly one outcome: either Received and cancellation rejected, OR Cancelled and receipt rejected.
// Never both!
// =========================================================================
#[tokio::test]
async fn test_challenge_7_concurrent_receipt_vs_cancellation_race() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C7", "Barang Race Cancel", 12000).await;

    // Create & Order PO with 30 units
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Cancel Race",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 30,
                    "unit_cost": 12000
                }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap().to_string();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;

    let barrier = Arc::new(Barrier::new(2));

    // Task 1: Receive 15 units
    let app1 = harness.app.clone();
    let token1 = harness.owner_token.clone();
    let tenant1 = harness.tenant_id.clone();
    let po1 = po_id.clone();
    let prod1 = prod_id.clone();
    let barrier1 = barrier.clone();
    let t1 = tokio::spawn(async move {
        barrier1.wait().await;
        send_req(
            &app1,
            Method::POST,
            &format!("/api/v1/purchase-orders/{}/receive", po1),
            &token1,
            &tenant1,
            Some(json!({
                "items": [
                    {
                        "product_id": prod1,
                        "quantity_received": 15
                    }
                ]
            })),
            None,
        )
        .await
    });

    // Task 2: Cancel PO
    let app2 = harness.app.clone();
    let token2 = harness.owner_token.clone();
    let tenant2 = harness.tenant_id.clone();
    let po2 = po_id.clone();
    let barrier2 = barrier.clone();
    let t2 = tokio::spawn(async move {
        barrier2.wait().await;
        send_req(
            &app2,
            Method::POST,
            &format!("/api/v1/purchase-orders/{}/cancel", po2),
            &token2,
            &tenant2,
            Some(json!({"reason": "Racing cancellation"})),
            None,
        )
        .await
    });

    let (rcv_status, rcv_body, _) = t1.await.unwrap();
    let (can_status, can_body, _) = t2.await.unwrap();

    let po_final: String = sqlx::query_scalar("SELECT status FROM purchase_orders WHERE id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    if rcv_status == StatusCode::OK {
        // Receipt won: cancellation MUST fail with 422 CANNOT_CANCEL_RECEIVED_PO
        assert_eq!(can_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(can_body["code"], "CANNOT_CANCEL_RECEIVED_PO");
        assert_eq!(po_final, "PARTIALLY_RECEIVED");
    } else {
        // Cancellation won: receipt MUST fail with 422 PO_CANCELLED
        assert_eq!(rcv_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(rcv_body["code"], "PO_CANCELLED");
        assert_eq!(can_status, StatusCode::OK);
        assert_eq!(po_final, "CANCELLED");
    }
}

// =========================================================================
// CHALLENGE 8: Cross-Tenant Isolation and Anti-Enumeration (404 Not Found)
// Tenant B must receive 404 when attempting to receive or cancel Tenant A's PO.
// =========================================================================
#[tokio::test]
async fn test_challenge_8_cross_tenant_isolation_anti_enumeration() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C8", "Barang Tenant A", 18000).await;

    // Create & Order PO in Tenant A
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Rahasia Tenant A",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 20,
                    "unit_cost": 18000
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
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;

    // Tenant B attempts to receive Tenant A's PO -> Must return HTTP 404 NOT_FOUND
    let (status, err_b_rcv, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 10
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant receive must return HTTP 404");
    assert_eq!(err_b_rcv["code"], "NOT_FOUND");

    // Tenant B attempts to cancel Tenant A's PO -> Must return HTTP 404 NOT_FOUND
    let (status, err_b_can, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant cancel must return HTTP 404");
    assert_eq!(err_b_can["code"], "NOT_FOUND");
}

// =========================================================================
// CHALLENGE 9: Adversarial Intra-Request Duplicate Product Over-Receipt
// An adversarial request contains multiple items for the SAME product_id:
// e.g. [ { product_id: P, quantity_received: 30 }, { product_id: P, quantity_received: 30 } ]
// where quantity_ordered is 50. Total attempted: 60 > 50.
// Verify whether intra-request validation accumulates received quantities.
// =========================================================================
#[tokio::test]
async fn test_challenge_9_intra_request_duplicate_product_over_receipt() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C9", "Barang Intra Race", 20000).await;

    // Create & Order PO with 50 units
    let (_, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Intra Duplicate",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 50,
                    "unit_cost": 20000
                }
            ]
        })),
        None,
    )
    .await;
    let po_id = po_body["id"].as_str().unwrap().to_string();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;

    // Adversarial receipt with two lines of 30 for the same product
    let (status, resp_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 30
                },
                {
                    "product_id": prod_id,
                    "quantity_received": 30
                }
            ]
        })),
        None,
    )
    .await;

    // Total received in DB
    let item_received: i64 = sqlx::query_scalar("SELECT quantity_received FROM purchase_order_items WHERE purchase_order_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    eprintln!("CHALLENGE 9 RESULT: status = {}, body = {}, item_received in DB = {}", status, resp_body, item_received);

    // If it succeeded, then item_received is 60 which violates quantity_ordered = 50!
    // Or if it failed, it must be 422 QUANTITY_EXCEEDS_ORDERED or 400.
    assert!(
        item_received <= 50,
        "CRITICAL BUG: item_received ({}) exceeded quantity_ordered (50) due to duplicate product lines in single request!",
        item_received
    );
}

// =========================================================================
// CHALLENGE 10: PO Creation with Duplicate Product Lines Anomaly
// When a PO is created with duplicate line items for the same product,
// receive_purchase_order collapses them into a single HashMap entry by product_id,
// causing line item orphan/desynchronization.
// =========================================================================
#[tokio::test]
async fn test_challenge_10_po_creation_duplicate_product_anomaly() {
    let harness = setup_challenger_harness().await;
    let (wh_id, prod_id) =
        setup_warehouse_and_product(&harness, "WH-C10", "Barang PO Dup", 15000).await;

    // Create PO with duplicate items for the SAME product_id: 10 pcs and 20 pcs
    let (status, po_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Duplicate Items",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 10,
                    "unit_cost": 15000
                },
                {
                    "product_id": prod_id,
                    "quantity_ordered": 20,
                    "unit_cost": 15000
                }
            ]
        })),
        None,
    )
    .await;
    eprintln!("CHALLENGE 10 PO CREATE: status = {}, body = {}", status, po_body);
    let po_id = po_body["id"].as_str().unwrap().to_string();

    send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({})),
        None,
    )
    .await;

    // Try to receive 20 units
    let (status, rcv_body, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 20
                }
            ]
        })),
        None,
    )
    .await;
    eprintln!("CHALLENGE 10 RECEIVE: status = {}, body = {}", status, rcv_body);

    let items_rows = sqlx::query("SELECT id, quantity_ordered, quantity_received FROM purchase_order_items WHERE purchase_order_id = ?1")
        .bind(&po_id)
        .fetch_all(&harness.pool)
        .await
        .unwrap();

    for row in items_rows {
        let q_ord: i64 = row.get("quantity_ordered");
        let q_rcv: i64 = row.get("quantity_received");
        eprintln!("CHALLENGE 10 ITEM ROW: ordered = {}, received = {}", q_ord, q_rcv);
    }

    let po_final_status: String = sqlx::query_scalar("SELECT status FROM purchase_orders WHERE id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    let total_ordered: i64 = sqlx::query_scalar("SELECT SUM(quantity_ordered) FROM purchase_order_items WHERE purchase_order_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    let total_received: i64 = sqlx::query_scalar("SELECT SUM(quantity_received) FROM purchase_order_items WHERE purchase_order_id = ?1")
        .bind(&po_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    eprintln!("CHALLENGE 10 INVARIANT CHECK: status = {}, total_ordered = {}, total_received = {}", po_final_status, total_ordered, total_received);

    assert_ne!(
        po_final_status, "RECEIVED",
        "CRITICAL BUG: PO prematurely marked RECEIVED when only {} of {} units were received!",
        total_received, total_ordered
    );
}
