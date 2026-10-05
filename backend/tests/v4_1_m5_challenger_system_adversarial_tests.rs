//! Milestone 5 Adversarial Challenger Test Suite
//! Tier 5 Full System Hardening across Backend Invariants (Features 1-26).
//!
//! Agent: teamwork_preview_challenger_m5_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Multi-location stock concurrency (`BEGIN IMMEDIATE` write lock serialization, zero overselling).
//! 2. Stock balance floor `CHECK (quantity_on_hand >= 0)` and negative balance rejection.
//! 3. Idempotency key replay returning exact cached response.
//! 4. PO state machine lifecycle validation (`DRAFT` -> `ORDERED` -> `PARTIALLY_RECEIVED` -> `RECEIVED` / `CANCELLED`).
//! 5. WAC integer Rupiah math via `round_half_up_i128` (zero floating point arithmetic).
//! 6. Double-entry GL balanced journals (`Debit 1300 / Credit 2000`, `Debit 5000 / Credit 1300`, `SUM(debit) == SUM(credit)`).
//! 7. Transactional outbox events atomicity and deduplication.

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

async fn setup_m5_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m5_adversarial_test.sqlite");
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

    let jwt_secret = "m5_challenger_secret_key_1234567890_super_secure_entropy_key";
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
            email: "m5_challenger_owner_a@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "M5 Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_a_id, "m5_challenger_owner_a@test.com", "user", "premium")
        .unwrap();

    let tenant_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_id.clone(),
            name: "M5 Challenger Tenant Alpha".to_string(),
            slug: "m5-challenger-tenant-alpha".to_string(),
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

    // Tenant B Owner (for cross-tenant checks)
    let owner_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_b_id.clone(),
            email: "m5_challenger_owner_b@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "M5 Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&owner_b_id, "m5_challenger_owner_b@test.com", "user", "premium")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "M5 Challenger Tenant Beta".to_string(),
            slug: "m5-challenger-tenant-beta".to_string(),
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
            "unit": "PCS",
            "cost_price": cost,
            "sale_price": cost * 2,
            "reorder_threshold": 10
        })),
        None,
    )
    .await;
    let prod_id = prod_res["id"].as_str().unwrap().to_string();

    (wh_id, prod_id)
}

// =========================================================================
// TEST 1: Multi-Location Stock Concurrency & Zero Overselling Invariant
// =========================================================================
#[tokio::test]
async fn test_challenge_1_concurrency_zero_overselling_exact_conservation() {
    let harness = setup_m5_challenger_harness().await;
    let (wh_id, prod_id) = setup_warehouse_and_product(&harness, "WH-CONC-1", "Adversarial Item", 50_000).await;

    // Seed initial stock of exactly 100 units
    let (st, res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 100,
            "unit_cost": 50_000,
            "notes": "Initial batch for concurrency challenge"
        })),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "Inbound movement failed: {:?}", res);

    // Concurrency attack: 50 tasks each attempting to deduct 3 units (150 units attempted vs 100 available)
    let barrier = Arc::new(Barrier::new(50));
    let mut tasks = Vec::new();

    for i in 0..50 {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_id.clone();
        let prod_id_c = prod_id.clone();
        let wh_id_c = wh_id.clone();
        let b = barrier.clone();

        tasks.push(tokio::spawn(async move {
            b.wait().await;
            let mut attempts = 0;
            loop {
                attempts += 1;
                let (status, res, headers) = send_req(
                    &app,
                    Method::POST,
                    "/api/v1/inventory/movements",
                    &token,
                    &tenant_id,
                    Some(json!({
                        "movement_type": "OUTBOUND",
                        "product_id": prod_id_c,
                        "source_warehouse_id": wh_id_c,
                        "quantity": 3,
                        "notes": format!("Concurrent deduction #{}", i)
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CONFLICT && res["code"] == "LOCK_CONTENTION" && attempts < 15 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(25 * attempts)).await;
                    continue;
                }
                return (status, res, headers);
            }
        }));
    }

    let mut successes = 0;
    let mut insufficient_failures = 0;

    for t in tasks {
        let (status, res, _) = t.await.unwrap();
        if status == StatusCode::CREATED {
            successes += 1;
        } else if status == StatusCode::UNPROCESSABLE_ENTITY {
            assert_eq!(
                res["code"], "INSUFFICIENT_STOCK",
                "Expected INSUFFICIENT_STOCK code, got {:?}", res
            );
            insufficient_failures += 1;
        } else {
            panic!("Unexpected response during concurrency test: status {} {:?}", status, res);
        }
    }

    // Mathematical conservation verification:
    // With 100 units initially, each task takes 3 units:
    // floor(100 / 3) = 33 successful deductions = 99 units deducted.
    // 17 tasks must be rejected with INSUFFICIENT_STOCK.
    assert_eq!(successes, 33, "Exactly 33 tasks must succeed (33 * 3 = 99 units)");
    assert_eq!(insufficient_failures, 17, "Exactly 17 tasks must fail with INSUFFICIENT_STOCK");

    // Verify remaining stock in DB is EXACTLY 1 unit (100 - 99 = 1)
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE warehouse_id = ? AND product_id = ?")
        .bind(&wh_id)
        .bind(&prod_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let remaining_on_hand: i64 = row.get("quantity_on_hand");
    assert_eq!(remaining_on_hand, 1, "Remaining stock on hand must strictly equal 1 unit (zero overselling)");
}

// =========================================================================
// TEST 2: Hard Database & Domain Balance Floor (CHECK quantity_on_hand >= 0)
// =========================================================================
#[tokio::test]
async fn test_challenge_2_stock_balance_floor_and_db_immutability() {
    let harness = setup_m5_challenger_harness().await;
    let (wh_id, prod_id) = setup_warehouse_and_product(&harness, "WH-FLOOR-1", "Floor Item", 10_000).await;

    // 1. Attempt outbound movement on uninitialized / zero stock -> HTTP 422
    let (st, res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 1
        })),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["code"], "INSUFFICIENT_STOCK");

    // 2. Attempt physical adjustment with negative actual quantity -> HTTP 422
    let (st, res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": -5,
            "reason": "Adversarial negative count"
        })),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res["code"], "NEGATIVE_STOCK_PROHIBITED");

    // 3. Inbound 10 units to create stock item and immutable movement record
    let (_, mov_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10,
            "unit_cost": 10_000
        })),
        None,
    )
    .await;
    let mov_id = mov_res["id"].as_str().unwrap();

    // 4. Attempt direct database UPDATE forcing quantity_on_hand < 0 -> must violate SQLite CHECK constraint
    let direct_db_update = sqlx::query("UPDATE stock_items SET quantity_on_hand = -10 WHERE warehouse_id = ? AND product_id = ?")
        .bind(&wh_id)
        .bind(&prod_id)
        .execute(&harness.pool)
        .await;
    assert!(
        direct_db_update.is_err(),
        "Direct SQL update setting quantity_on_hand < 0 must be blocked by SQLite CHECK constraint"
    );

    // 5. Attempt direct database UPDATE forcing quantity_reserved < 0 -> must violate SQLite CHECK constraint
    let direct_reserved_update = sqlx::query("UPDATE stock_items SET quantity_reserved = -1 WHERE warehouse_id = ? AND product_id = ?")
        .bind(&wh_id)
        .bind(&prod_id)
        .execute(&harness.pool)
        .await;
    assert!(
        direct_reserved_update.is_err(),
        "Direct SQL update setting quantity_reserved < 0 must be blocked by SQLite CHECK constraint"
    );

    // 6. Direct UPDATE on stock_movements must be aborted by trigger trg_stock_movements_prevent_update
    let trigger_update = sqlx::query("UPDATE stock_movements SET quantity = 999 WHERE id = ?")
        .bind(mov_id)
        .execute(&harness.pool)
        .await;
    assert!(
        trigger_update.is_err(),
        "UPDATE on stock_movements must be aborted by SQL trigger"
    );

    // 7. Direct DELETE on stock_movements must be aborted by trigger trg_stock_movements_prevent_delete
    let trigger_delete = sqlx::query("DELETE FROM stock_movements WHERE id = ?")
        .bind(mov_id)
        .execute(&harness.pool)
        .await;
    assert!(
        trigger_delete.is_err(),
        "DELETE on stock_movements must be aborted by SQL trigger"
    );
}

// =========================================================================
// TEST 3: Idempotency Key Replay Exactness & Payload Mismatch Conflict
// =========================================================================
#[tokio::test]
async fn test_challenge_3_idempotency_replay_and_mismatch_verification() {
    let harness = setup_m5_challenger_harness().await;
    let (wh_id, prod_id) = setup_warehouse_and_product(&harness, "WH-IDEMP-1", "Idempotent Item", 25_000).await;

    let key = format!("idemp-key-{}", Uuid::new_v4());
    let mut headers = HeaderMap::new();
    headers.insert("Idempotency-Key", key.parse().unwrap());

    let payload = json!({
        "movement_type": "INBOUND",
        "product_id": prod_id,
        "destination_warehouse_id": wh_id,
        "quantity": 50,
        "unit_cost": 25_000,
        "notes": "First idempotent mutation"
    });

    // 1. Initial request -> HTTP 201 CREATED
    let (st1, res1, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(payload.clone()),
        Some(headers.clone()),
    )
    .await;
    assert_eq!(st1, StatusCode::CREATED);
    let movement_id = res1["id"].as_str().unwrap().to_string();

    // 2. Exact Replay with same key & payload -> HTTP 200 OK with cached response
    let (st2, res2, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(payload.clone()),
        Some(headers.clone()),
    )
    .await;
    assert!(st2 == StatusCode::OK || st2 == StatusCode::CREATED);
    assert_eq!(
        res2["id"].as_str().unwrap(),
        movement_id,
        "Replayed response must return the exact cached movement ID"
    );

    // Stock must have increased by 50 ONLY ONCE, not twice!
    let row = sqlx::query("SELECT quantity_on_hand FROM stock_items WHERE warehouse_id = ? AND product_id = ?")
        .bind(&wh_id)
        .bind(&prod_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let current_stock: i64 = row.get("quantity_on_hand");
    assert_eq!(current_stock, 50, "Stock on hand must strictly be 50, not 100");

    // 3. Different payload with SAME idempotency key -> HTTP 409 CONFLICT (IDEMPOTENCY_MISMATCH)
    let altered_payload = json!({
        "movement_type": "INBOUND",
        "product_id": prod_id,
        "destination_warehouse_id": wh_id,
        "quantity": 999, // altered quantity
        "unit_cost": 25_000,
        "notes": "Altered payload with duplicate key"
    });

    let (st3, res3, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(altered_payload),
        Some(headers.clone()),
    )
    .await;
    assert_eq!(
        st3,
        StatusCode::CONFLICT,
        "Mismatched payload with active idempotency key must return HTTP 409 CONFLICT"
    );
    assert_eq!(res3["code"], "IDEMPOTENCY_MISMATCH");
}

// =========================================================================
// TEST 4: Purchase Order Lifecycle State Machine Validation
// =========================================================================
#[tokio::test]
async fn test_challenge_4_po_state_machine_lifecycle_and_illegal_transitions() {
    let harness = setup_m5_challenger_harness().await;
    let (wh_id, prod_id) = setup_warehouse_and_product(&harness, "WH-PO-1", "PO Line Item", 15_000).await;

    // 1. Create Purchase Order -> DRAFT
    let (st, po_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "PT Sumber Rejeki",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 20,
                    "unit_cost": 15_000
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let po_id = po_res["id"].as_str().unwrap().to_string();
    assert_eq!(po_res["status"], "DRAFT");

    // Illegal: Attempt to receive items while in DRAFT -> HTTP 422
    let (st_err, res_err, _) = send_req(
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
    assert_eq!(st_err, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res_err["code"], "PO_NOT_ORDERED");

    // 2. Order Purchase Order -> ORDERED
    let (st_ord, res_ord, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(st_ord, StatusCode::OK);
    assert_eq!(res_ord["status"], "ORDERED");

    // Illegal: Attempt to order again -> HTTP 422
    let (st_double_ord, _, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/order", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(st_double_ord, StatusCode::UNPROCESSABLE_ENTITY);

    // Illegal: Over-receipt (> 20 ordered) -> HTTP 422
    let (st_over, res_over, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 25 // 25 > 20
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(st_over, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res_over["code"], "QUANTITY_EXCEEDS_ORDERED");

    // 3. Staged Receipt 1: Partial receipt of 8 units -> PARTIALLY_RECEIVED
    let (st_rcpt1, res_rcpt1, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 8,
                    "unit_cost": 15_000
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(st_rcpt1, StatusCode::OK);
    assert_eq!(res_rcpt1["status"], "PARTIALLY_RECEIVED");

    // Illegal: Attempt to cancel a PARTIALLY_RECEIVED PO -> HTTP 422
    let (st_cancel_err, res_cancel_err, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({"reason": "Cannot cancel partially received order"})),
        None,
    )
    .await;
    assert_eq!(st_cancel_err, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res_cancel_err["code"], "CANNOT_CANCEL_RECEIVED_PO");

    // 4. Staged Receipt 2: Fulfill remaining 12 units -> RECEIVED
    let (st_rcpt2, res_rcpt2, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 12,
                    "unit_cost": 15_000
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(st_rcpt2, StatusCode::OK);
    assert_eq!(res_rcpt2["status"], "RECEIVED");

    // Illegal: Attempt further receipts on fully RECEIVED PO -> HTTP 422
    let (st_post_rcpt, res_post_rcpt, _) = send_req(
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
    assert_eq!(st_post_rcpt, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res_post_rcpt["code"], "PO_ALREADY_RECEIVED");

    // Illegal: Attempt to cancel a fully RECEIVED PO -> HTTP 422
    let (st_cancel_post, res_cancel_post, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/cancel", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({"reason": "Adversarial cancel after full receipt"})),
        None,
    )
    .await;
    assert_eq!(st_cancel_post, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res_cancel_post["code"], "CANNOT_CANCEL_RECEIVED_PO");
}

// =========================================================================
// TEST 5: WAC Integer Rupiah Math via round_half_up_i128 (Zero Floating Point)
// =========================================================================
#[test]
fn test_challenge_5_wac_integer_rupiah_rounding_and_edge_values() {
    // 1. Rigorous round_half_up_i128 boundary sweep:
    // If remainder < d / 2 => round down
    // If remainder >= ceil(d / 2.0) => round up
    for d in 1..=200 {
        for q in 0..=20 {
            for r in 0..d {
                let n = (q as i128) * (d as i128) + (r as i128);
                let actual = round_half_up_i128(n, d as i128);
                let _half_d = (d as i128) / 2;
                let expected = if r as i128 >= ((d as i128 + 1) / 2) {
                    q + 1
                } else {
                    q
                };
                assert_eq!(
                    actual, expected,
                    "round_half_up_i128 failed for n={}, d={}, r={}: expected {}, got {}",
                    n, d, r, expected, actual
                );
            }
        }
    }

    // 2. Exact commercial Moving WAC calculation:
    // Batch 1: 100 units @ Rp 100.000 (total = 10.000.000)
    let wac1 = calculate_weighted_average_cost(0, Rupiah::ZERO, 100, Rupiah::new(100_000)).unwrap();
    assert_eq!(wac1, Rupiah::new(100_000));

    // Batch 2: 50 units @ Rp 130.000
    // Total cost = 10.000.000 + 6.500.000 = 16.500.000, Total qty = 150
    // 16.500.000 / 150 = 110.000 exactly
    let wac2 = calculate_weighted_average_cost(100, wac1, 50, Rupiah::new(130_000)).unwrap();
    assert_eq!(wac2, Rupiah::new(110_000));

    // Batch 3: 60 units @ Rp 120.000
    // Total cost = 150 * 110.000 + 60 * 120.000 = 16.500.000 + 7.200.000 = 23.700.000
    // Total qty = 210
    // 23.700.000 / 210 = 112.857,142857...
    // Remainder: 23.700.000 % 210 = 30. 30 < 105 (210 / 2) => rounds down to 112.857
    let wac3 = calculate_weighted_average_cost(150, wac2, 60, Rupiah::new(120_000)).unwrap();
    assert_eq!(wac3, Rupiah::new(112_857));

    // 3. Huge number stress test: 20.000 units @ Rp 1.500.000.000 = Rp 30.000.000.000.000 (30 Trillion IDR)
    let huge_wac = calculate_weighted_average_cost(
        10_000,
        Rupiah::new(1_500_000_000),
        10_000,
        Rupiah::new(1_800_000_000),
    )
    .unwrap();
    assert_eq!(huge_wac, Rupiah::new(1_650_000_000));
}

// =========================================================================
// TEST 6: Double-Entry GL Journal Balancing & System Account 1300 Protection
// =========================================================================
#[tokio::test]
async fn test_challenge_6_double_entry_gl_journal_balancing_invariants() {
    let harness = setup_m5_challenger_harness().await;
    let (wh_id, prod_id) = setup_warehouse_and_product(&harness, "WH-GL-1", "GL Goods", 80_000).await;

    // 1. PO Inbound Receipt -> Auto-post Debit 1300 (Persediaan) / Credit 2000 (Utang Usaha)
    let (_, po_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/purchase-orders",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "supplier_name": "CV Maju Jaya",
            "destination_warehouse_id": wh_id,
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_ordered": 10,
                    "unit_cost": 80_000
                }
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

    let (_, rcpt_res, _) = send_req(
        &harness.app,
        Method::POST,
        &format!("/api/v1/purchase-orders/{}/receive", po_id),
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "items": [
                {
                    "product_id": prod_id,
                    "quantity_received": 10,
                    "unit_cost": 80_000
                }
            ]
        })),
        None,
    )
    .await;
    assert_eq!(rcpt_res["status"], "RECEIVED");

    // Verify Inbound Receipt Journal in database
    let j_entry = sqlx::query(
        "SELECT id FROM journal_entries WHERE tenant_id = ? AND source_type = 'PURCHASE_ORDER_RECEIPT'"
    )
    .bind(&harness.tenant_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let journal_id: String = j_entry.get("id");
    let sums = sqlx::query(
        "SELECT SUM(debit) as total_debit, SUM(credit) as total_credit FROM journal_lines WHERE journal_id = ?"
    )
    .bind(&journal_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let total_debit: i64 = sums.get("total_debit");
    let total_credit: i64 = sums.get("total_credit");

    assert_eq!(total_debit, 800_000, "Debit must equal 10 * 80.000 = Rp 800.000");
    assert_eq!(total_credit, 800_000, "Credit must equal Rp 800.000");
    assert_eq!(total_debit, total_credit, "Journal entry must be balanced (debit == credit)");

    // Verify lines: Debit 1300 / Credit 2000
    let lines = sqlx::query(
        "SELECT account_code, debit, credit FROM journal_lines WHERE journal_id = ? ORDER BY account_code"
    )
    .bind(&journal_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    assert_eq!(lines.len(), 2);
    let line_1300 = &lines[0];
    let line_2000 = &lines[1];

    let code_0: String = line_1300.get("account_code");
    let debit_0: i64 = line_1300.get("debit");
    let credit_0: i64 = line_1300.get("credit");
    assert_eq!(code_0, "1300");
    assert_eq!(debit_0, 800_000);
    assert_eq!(credit_0, 0);

    let code_1: String = line_2000.get("account_code");
    let debit_1: i64 = line_2000.get("debit");
    let credit_1: i64 = line_2000.get("credit");
    assert_eq!(code_1, "2000");
    assert_eq!(debit_1, 0);
    assert_eq!(credit_1, 800_000);

    // 2. Outbound Movement -> Auto-post Debit 5000 (COGS) / Credit 1300 (Persediaan)
    let (_, mov_res, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 3,
            "notes": "Sales fulfillment COGS test"
        })),
        None,
    )
    .await;
    assert_eq!(mov_res["quantity"], 3);

    let cogs_entry = sqlx::query(
        "SELECT id FROM journal_entries WHERE tenant_id = ? AND source_type = 'INVENTORY_OUTBOUND'"
    )
    .bind(&harness.tenant_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    let cogs_id: String = cogs_entry.get("id");

    let cogs_sums = sqlx::query(
        "SELECT SUM(debit) as total_debit, SUM(credit) as total_credit FROM journal_lines WHERE journal_id = ?"
    )
    .bind(&cogs_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let cogs_debit: i64 = cogs_sums.get("total_debit");
    let cogs_credit: i64 = cogs_sums.get("total_credit");
    assert_eq!(cogs_debit, 240_000, "COGS Debit must equal 3 * 80.000 = Rp 240.000");
    assert_eq!(cogs_credit, 240_000, "COGS Credit must equal Rp 240.000");
    assert_eq!(cogs_debit, cogs_credit);

    // 3. System Account 1300 Protection: Attempting to delete Account 1300 must return HTTP 403
    let (st_del, res_del, _) = send_req(
        &harness.app,
        Method::DELETE,
        "/api/v1/accounting/accounts/1300",
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(st_del, StatusCode::FORBIDDEN, "Deleting system account 1300 must be forbidden");
    assert_eq!(res_del["code"], "SYSTEM_ACCOUNT_PROTECTED");
}

// =========================================================================
// TEST 7: Transactional Outbox Events Atomicity and Deduplication
// =========================================================================
#[tokio::test]
async fn test_challenge_7_transactional_outbox_atomicity_and_deduplication() {
    let harness = setup_m5_challenger_harness().await;
    let (wh_id, prod_id) = setup_warehouse_and_product(&harness, "WH-OUTBOX-1", "Outbox Item", 30_000).await;

    // 1. Initial count of outbox events
    let _initial_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?")
        .bind(&harness.tenant_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    // 2. Perform a successful Stock Adjustment
    let (st_adj, res_adj, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": 40,
            "reason": "Stock count cycle audit"
        })),
        None,
    )
    .await;
    assert_eq!(st_adj, StatusCode::OK, "Stock adjustment failed: {:?}", res_adj);

    // Verify StockAdjusted event was atomically inserted
    let adj_event = sqlx::query(
        "SELECT id, event_type, aggregate_type, payload_json FROM outbox_events WHERE tenant_id = ? AND event_type = 'StockAdjusted'"
    )
    .bind(&harness.tenant_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let adj_event_id: String = adj_event.get("id");
    let adj_event_type: String = adj_event.get("event_type");
    let adj_agg_type: String = adj_event.get("aggregate_type");
    let adj_payload: String = adj_event.get("payload_json");

    assert!(Uuid::parse_str(&adj_event_id).is_ok(), "Outbox event ID must be a valid UUIDv4");
    assert_eq!(adj_event_type, "StockAdjusted");
    assert_eq!(adj_agg_type, "Inventory");
    let parsed_payload: Value = serde_json::from_str(&adj_payload).unwrap();
    assert_eq!(parsed_payload["variance"], 40);

    // 3. Atomicity Rollback verification: Trigger an unprocessable mutation (e.g. negative stock)
    let pre_rollback_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?")
        .bind(&harness.tenant_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    let (st_fail, _, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 9999 // Exceeds available stock 40 -> aborts
        })),
        None,
    )
    .await;
    assert_eq!(st_fail, StatusCode::UNPROCESSABLE_ENTITY);

    let post_rollback_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?")
        .bind(&harness.tenant_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    assert_eq!(
        pre_rollback_count, post_rollback_count,
        "Failed mutation must NOT insert any outbox events (atomic rollback preserved)"
    );

    // 4. Consumer Deduplication: Ensure all outbox events have distinct UUIDs
    let all_uuids: Vec<String> = sqlx::query_scalar("SELECT id FROM outbox_events WHERE tenant_id = ?")
        .bind(&harness.tenant_id)
        .fetch_all(&harness.pool)
        .await
        .unwrap();

    let mut deduped = all_uuids.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(
        all_uuids.len(),
        deduped.len(),
        "All outbox event IDs must be strictly unique for deduplication"
    );
}

// =========================================================================
// TEST 8: Cross-Tenant Anti-Enumeration Isolation
// =========================================================================
#[tokio::test]
async fn test_challenge_8_cross_tenant_anti_enumeration_isolation() {
    let harness = setup_m5_challenger_harness().await;
    let (wh_a_id, prod_a_id) = setup_warehouse_and_product(&harness, "WH-ISO-A", "Alpha Secret Item", 45_000).await;

    // Tenant B attempts to access Tenant A's warehouse by ID -> HTTP 404
    let (st_wh, res_wh, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/warehouses/{}", wh_a_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(st_wh, StatusCode::NOT_FOUND, "Cross-tenant warehouse access must return 404");
    assert_eq!(res_wh["code"], "NOT_FOUND");

    // Tenant B attempts to access Tenant A's product by ID -> HTTP 404
    let (st_prod, res_prod, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/products/{}", prod_a_id),
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        None,
        None,
    )
    .await;
    assert_eq!(st_prod, StatusCode::NOT_FOUND, "Cross-tenant product access must return 404");
    assert_eq!(res_prod["code"], "NOT_FOUND");

    // Tenant B attempts to mutate Tenant A's stock -> HTTP 404
    let (st_mov, res_mov, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.tenant_b_token,
        &harness.tenant_b_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_a_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 10
        })),
        None,
    )
    .await;
    assert_eq!(st_mov, StatusCode::NOT_FOUND, "Cross-tenant stock mutation must return 404");
    assert_eq!(res_mov["code"], "NOT_FOUND");
}
