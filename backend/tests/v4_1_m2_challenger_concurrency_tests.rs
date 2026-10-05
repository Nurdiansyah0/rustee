//! Empirical Adversarial Challenger Test Suite for Milestone 2:
//! Concurrency Serialization, Write Locking, and Zero Overselling Guarantees (R1, R2).
//!
//! Identity: teamwork_preview_challenger_m2_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Challenge racing concurrent stock allocations/deductions on the exact same SKU across multiple tokio tasks.
//! 2. Verify that overselling is strictly impossible: total successful deductions must NEVER exceed initial stock on hand,
//!    and final balance must never drop below zero.
//! 3. Test edge case of simultaneous racing deductions attempting to deduct the entire remaining stock.
//! 4. Test racing inter-warehouse transfers vs outbound deductions on the exact same source stock item.
//! 5. Test racing concurrent inbound replenishment vs outbound deductions under zero-start condition.
//! 6. Verify adversarial boundary rejections (<= 0 quantities, self-transfers, negative cycle count adjustments).
//! 7. Verify concurrent idempotency key replaying under heavy racing contention.
//! 8. Verify SQLite-level invariants (CHECK constraints and triggers) prevent direct out-of-band manipulation.

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
use sqlx::SqlitePool;
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
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_m2_concurrency_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 25,
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

    let jwt_secret = "challenger_m2_concurrency_secret_key_1234567890_super_secret";
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

    let owner_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_id.clone(),
            email: "challenger_owner@race-test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Challenger Owner".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "challenger_owner@race-test.com", "user", "premium")
        .unwrap();

    let tenant_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_id.clone(),
            name: "Concurrency Racing Tenant".to_string(),
            slug: "concurrency-racing-tenant".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_id.clone(),
            user_id: owner_id.clone(),
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

// =========================================================================
// CHALLENGE 1: Massive Concurrent Stock Deduction Race (Over-Subscription)
// 50 tasks racing to deduct 5 units each from 100 available units (250 requested).
// =========================================================================
#[tokio::test]
async fn test_challenge_1_massive_50_tasks_concurrency_zero_overselling() {
    let harness = setup_challenger_harness().await;

    // 1. Create Warehouse & Product
    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-RACE-50", "name": "Warehouse Race 50" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap().to_string();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "name": "Massive Race SKU", "sale_price": 75000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    // 2. Inbound exactly 100 units
    let (in_status, in_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 100
        })),
        None,
    )
    .await;
    assert_eq!(in_status, StatusCode::CREATED);
    assert_eq!(in_body["resulting_stock"], 100);

    // 3. Spawn 50 concurrent tokio tasks, each trying to deduct 5 units (total requested = 250 units)
    let app = harness.app.clone();
    let token = harness.owner_token.clone();
    let tenant_id = harness.tenant_id.clone();

    let mut handles = Vec::new();
    for task_idx in 0..50 {
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
                        "quantity": 5
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY {
                    return (status, body, attempts);
                }

                if status == StatusCode::CONFLICT && body["code"] == "LOCK_CONTENTION" && attempts < 40 {
                    // Jittered backoff to simulate production retry loop
                    let jitter_ms = (task_idx * 7 + attempts * 13) % 25 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return (status, body, attempts);
            }
        }));
    }

    let mut success_count = 0;
    let mut rejected_insufficient_count = 0;
    let mut total_deducted_reported = 0i64;

    for handle in handles {
        let (status, body, _) = handle.await.unwrap();
        if status == StatusCode::CREATED {
            success_count += 1;
            total_deducted_reported += body["quantity"].as_i64().unwrap();
        } else if status == StatusCode::UNPROCESSABLE_ENTITY {
            assert_eq!(body["code"], "INSUFFICIENT_STOCK");
            rejected_insufficient_count += 1;
        } else {
            panic!("Unexpected response during concurrency test: status={}, body={:?}", status, body);
        }
    }

    // Invariants:
    // Exactly 20 deductions of 5 units must succeed (20 * 5 = 100 units)
    assert_eq!(success_count, 20, "Empirical failure: exactly 20 tasks should have succeeded");
    assert_eq!(rejected_insufficient_count, 30, "Empirical failure: exactly 30 tasks should have failed with INSUFFICIENT_STOCK");
    assert_eq!(total_deducted_reported, 100, "Empirical failure: total deducted units reported must be exactly 100");

    // Check balance in DB via API
    let (get_status, stock_list, _) = send_req(
        &harness.app,
        Method::GET,
        &format!("/api/v1/inventory?warehouse_id={}", wh_id),
        &harness.owner_token,
        &harness.tenant_id,
        None,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(
        stock_list["stock_items"][0]["quantity_on_hand"], 0,
        "Stock on hand must be exactly 0 (no overselling, no negative balance)"
    );

    // Direct SQLite verification of persistence invariants
    let db_stock: (i64, i64) = sqlx::query_as(
        "SELECT quantity_on_hand, quantity_reserved FROM stock_items WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(db_stock.0, 0, "DB quantity_on_hand must be exactly 0");
    assert_eq!(db_stock.1, 0, "DB quantity_reserved must be exactly 0");

    // Check stock movements count
    let mov_count: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), SUM(quantity) FROM stock_movements WHERE tenant_id = ?1 AND product_id = ?2 AND movement_type = 'OUTBOUND'"
    )
    .bind(&harness.tenant_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mov_count.0, 20, "Exactly 20 OUTBOUND stock movement rows must exist");
    assert_eq!(mov_count.1, 100, "Sum of outbound stock movements must be exactly 100");

    // Check outbox events count
    let outbox_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?1 AND event_type = 'StockDeducted'"
    )
    .bind(&harness.tenant_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count.0, 20, "Exactly 20 StockDeducted outbox events must be inserted");
}

// =========================================================================
// CHALLENGE 2: Simultaneous Racing Deductions of ENTIRE Remaining Stock
// 15 tasks each attempting to deduct 100 units from 100 available units.
// Exactly ONE must succeed; 14 must fail with INSUFFICIENT_STOCK.
// =========================================================================
#[tokio::test]
async fn test_challenge_2_simultaneous_full_stock_deduction_race() {
    let harness = setup_challenger_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-FULL-RACE", "name": "Warehouse Full Race" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap().to_string();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "name": "Full Stock Drain SKU", "sale_price": 100000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    // Inbound 100 units
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 100
        })),
        None,
    )
    .await;

    // 15 concurrent tasks, each demanding all 100 units
    let app = harness.app.clone();
    let token = harness.owner_token.clone();
    let tenant_id = harness.tenant_id.clone();

    let mut handles = Vec::new();
    for task_idx in 0..15 {
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
                        "quantity": 100
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY {
                    return (status, body);
                }

                if status == StatusCode::CONFLICT && body["code"] == "LOCK_CONTENTION" && attempts < 40 {
                    let jitter_ms = (task_idx * 11 + attempts * 17) % 30 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return (status, body);
            }
        }));
    }

    let mut success_count = 0;
    let mut failure_count = 0;

    for handle in handles {
        let (status, body) = handle.await.unwrap();
        if status == StatusCode::CREATED {
            success_count += 1;
            assert_eq!(body["quantity"], 100);
            assert_eq!(body["remaining_stock"], 0);
        } else if status == StatusCode::UNPROCESSABLE_ENTITY {
            assert_eq!(body["code"], "INSUFFICIENT_STOCK");
            failure_count += 1;
        } else {
            panic!("Unexpected response in full stock race: status={}, body={:?}", status, body);
        }
    }

    // Invariants:
    assert_eq!(success_count, 1, "CRITICAL OVERSOLD BUG: Exactly 1 task must succeed when racing for entire stock!");
    assert_eq!(failure_count, 14, "Remaining 14 tasks must fail with INSUFFICIENT_STOCK");

    // Final balance in DB must be exactly 0
    let db_qty: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(db_qty.0, 0, "Final balance in DB must be exactly 0");
}

// =========================================================================
// CHALLENGE 3: Asymmetric Non-Uniform Quantities with Prime Initial Stock
// Initial stock = 73 units. 35 tasks racing with varied quantities (3..9 units).
// Guarantees no fractional oversell, conservation law holds strictly.
// =========================================================================
#[tokio::test]
async fn test_challenge_3_asymmetric_quantities_prime_stock_conservation() {
    let harness = setup_challenger_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-PRIME", "name": "Warehouse Prime" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap().to_string();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "name": "Prime Stock SKU", "sale_price": 25000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    // Inbound exactly 73 units
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 73
        })),
        None,
    )
    .await;

    let app = harness.app.clone();
    let token = harness.owner_token.clone();
    let tenant_id = harness.tenant_id.clone();

    let mut handles = Vec::new();
    // 35 tasks with non-uniform requested quantities: [3, 4, 5, 6, 7, 8, 9, 3, 4...]
    for task_idx in 0..35 {
        let app_clone = app.clone();
        let token_clone = token.clone();
        let tenant_id_clone = tenant_id.clone();
        let prod_id_clone = prod_id.clone();
        let wh_id_clone = wh_id.clone();
        let req_qty = (task_idx % 7) as i64 + 3; // 3 to 9

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
                        "quantity": req_qty
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY {
                    return (status, body, req_qty);
                }

                if status == StatusCode::CONFLICT && body["code"] == "LOCK_CONTENTION" && attempts < 40 {
                    let jitter_ms = (task_idx * 13 + attempts * 19) % 30 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return (status, body, req_qty);
            }
        }));
    }

    let mut total_deducted = 0i64;
    let mut success_count = 0;
    let mut failure_count = 0;

    for handle in handles {
        let (status, body, req_qty) = handle.await.unwrap();
        if status == StatusCode::CREATED {
            success_count += 1;
            total_deducted += req_qty;
        } else if status == StatusCode::UNPROCESSABLE_ENTITY {
            assert_eq!(body["code"], "INSUFFICIENT_STOCK");
            failure_count += 1;
        } else {
            panic!("Unexpected response: status={}, body={:?}", status, body);
        }
    }

    assert!(total_deducted <= 73, "OVERSOLD INVARIANT BROKEN: total deducted {} exceeds initial 73!", total_deducted);
    assert!(success_count > 0);
    assert!(failure_count > 0);

    // Verify remaining stock in DB satisfies exact conservation: remaining = 73 - total_deducted
    let db_qty: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(
        db_qty.0,
        73 - total_deducted,
        "Conservation law broken: DB remaining ({}) != 73 - total_deducted ({})",
        db_qty.0,
        total_deducted
    );
    assert!(db_qty.0 >= 0, "Stock became negative!");
}

// =========================================================================
// CHALLENGE 4: Racing Inter-Warehouse Transfers vs Outbound Deductions
// Warehouse A: 50 units.
// 15 tasks attempt OUTBOUND (4 units each = 60 requested)
// 15 tasks attempt TRANSFER A -> B (4 units each = 60 requested)
// Global conservation invariant: (A remaining) + (B final) + (Outbound deducted) == 50.
// =========================================================================
#[tokio::test]
async fn test_challenge_4_racing_transfers_vs_outbound_global_conservation() {
    let harness = setup_challenger_harness().await;

    // Create Warehouse A
    let (_, wh_a_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-SRC", "name": "Warehouse Source" })),
        None,
    )
    .await;
    let wh_a_id = wh_a_body["id"].as_str().unwrap().to_string();

    // Create Warehouse B
    let (_, wh_b_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-DST", "name": "Warehouse Dest" })),
        None,
    )
    .await;
    let wh_b_id = wh_b_body["id"].as_str().unwrap().to_string();

    // Create Product
    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "name": "Dual Drain SKU", "sale_price": 40000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    // Inbound 50 units into Warehouse A
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_a_id,
            "quantity": 50
        })),
        None,
    )
    .await;

    let app = harness.app.clone();
    let token = harness.owner_token.clone();
    let tenant_id = harness.tenant_id.clone();

    let mut handles = Vec::new();

    // 15 OUTBOUND tasks
    for task_idx in 0..15 {
        let app_c = app.clone();
        let tok_c = token.clone();
        let ten_c = tenant_id.clone();
        let pr_c = prod_id.clone();
        let wh_a_c = wh_a_id.clone();

        handles.push(tokio::spawn(async move {
            let mut attempts = 0;
            loop {
                attempts += 1;
                let (status, _body, _) = send_req(
                    &app_c,
                    Method::POST,
                    "/api/v1/inventory/movements",
                    &tok_c,
                    &ten_c,
                    Some(json!({
                        "movement_type": "OUTBOUND",
                        "product_id": pr_c,
                        "source_warehouse_id": wh_a_c,
                        "quantity": 4
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY {
                    return ("OUTBOUND", status, 4i64);
                }

                if status == StatusCode::CONFLICT && attempts < 40 {
                    let jitter_ms = (task_idx * 17 + attempts * 13) % 25 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return ("OUTBOUND", status, 4i64);
            }
        }));
    }

    // 15 TRANSFER tasks
    for task_idx in 0..15 {
        let app_c = app.clone();
        let tok_c = token.clone();
        let ten_c = tenant_id.clone();
        let pr_c = prod_id.clone();
        let wh_a_c = wh_a_id.clone();
        let wh_b_c = wh_b_id.clone();

        handles.push(tokio::spawn(async move {
            let mut attempts = 0;
            loop {
                attempts += 1;
                let (status, _body, _) = send_req(
                    &app_c,
                    Method::POST,
                    "/api/v1/inventory/transfers",
                    &tok_c,
                    &ten_c,
                    Some(json!({
                        "product_id": pr_c,
                        "source_warehouse_id": wh_a_c,
                        "destination_warehouse_id": wh_b_c,
                        "quantity": 4
                    })),
                    None,
                )
                .await;

                if status == StatusCode::OK || status == StatusCode::UNPROCESSABLE_ENTITY {
                    return ("TRANSFER", status, 4i64);
                }

                if status == StatusCode::CONFLICT && attempts < 40 {
                    let jitter_ms = (task_idx * 23 + attempts * 11) % 25 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return ("TRANSFER", status, 4i64);
            }
        }));
    }

    let mut successful_outbound_qty = 0i64;
    let mut successful_transfer_qty = 0i64;

    for handle in handles {
        let (op_type, status, qty) = handle.await.unwrap();
        if op_type == "OUTBOUND" && status == StatusCode::CREATED {
            successful_outbound_qty += qty;
        } else if op_type == "TRANSFER" && status == StatusCode::OK {
            successful_transfer_qty += qty;
        }
    }

    let total_taken_from_a = successful_outbound_qty + successful_transfer_qty;
    assert!(
        total_taken_from_a <= 50,
        "OVERSOLD INVARIANT VIOLATED: total taken from A ({}) > initial 50!",
        total_taken_from_a
    );

    // Read DB stock for Warehouse A and Warehouse B
    let stock_a: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_a_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let stock_b: Option<(i64,)> = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_b_id)
    .bind(&prod_id)
    .fetch_optional(&harness.pool)
    .await
    .unwrap();

    let actual_stock_b = stock_b.map(|r| r.0).unwrap_or(0);

    assert!(stock_a.0 >= 0, "Warehouse A has negative stock!");
    assert!(actual_stock_b >= 0, "Warehouse B has negative stock!");

    assert_eq!(
        stock_a.0,
        50 - total_taken_from_a,
        "Warehouse A stock must equal 50 - total_taken_from_a"
    );
    assert_eq!(
        actual_stock_b,
        successful_transfer_qty,
        "Warehouse B stock must exactly match successful transfers"
    );

    // Global conservation law: (A remaining) + (B stock) + (Outbound deducted) == 50
    assert_eq!(
        stock_a.0 + actual_stock_b + successful_outbound_qty,
        50,
        "GLOBAL MASS CONSERVATION VIOLATED!"
    );
}

// =========================================================================
// CHALLENGE 5: Concurrent Inbound Replenishment vs Outbound Drain (Zero Start)
// 10 tasks INBOUND 10 units each (+100 total)
// 20 tasks OUTBOUND 10 units each (-200 requested total)
// Starts at 0 units. Invariant: balance NEVER drops below 0 at any instant.
// =========================================================================
#[tokio::test]
async fn test_challenge_5_concurrent_inbound_vs_outbound_zero_start() {
    let harness = setup_challenger_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-ZERO-START", "name": "Warehouse Zero Start" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap().to_string();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "name": "Zero Start SKU", "sale_price": 50000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    let app = harness.app.clone();
    let token = harness.owner_token.clone();
    let tenant_id = harness.tenant_id.clone();

    let mut handles = Vec::new();

    // 10 INBOUND tasks (+10 each)
    for task_idx in 0..10 {
        let app_c = app.clone();
        let tok_c = token.clone();
        let ten_c = tenant_id.clone();
        let pr_c = prod_id.clone();
        let wh_c = wh_id.clone();

        handles.push(tokio::spawn(async move {
            let mut attempts = 0;
            loop {
                attempts += 1;
                let (status, _body, _) = send_req(
                    &app_c,
                    Method::POST,
                    "/api/v1/inventory/movements",
                    &tok_c,
                    &ten_c,
                    Some(json!({
                        "movement_type": "INBOUND",
                        "product_id": pr_c,
                        "destination_warehouse_id": wh_c,
                        "quantity": 10
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CREATED {
                    return ("INBOUND", status, 10i64);
                }

                if status == StatusCode::CONFLICT && attempts < 40 {
                    let jitter_ms = (task_idx * 13 + attempts * 7) % 25 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return ("INBOUND", status, 10i64);
            }
        }));
    }

    // 20 OUTBOUND tasks (-10 each requested)
    for task_idx in 0..20 {
        let app_c = app.clone();
        let tok_c = token.clone();
        let ten_c = tenant_id.clone();
        let pr_c = prod_id.clone();
        let wh_c = wh_id.clone();

        handles.push(tokio::spawn(async move {
            let mut attempts = 0;
            loop {
                attempts += 1;
                let (status, _body, _) = send_req(
                    &app_c,
                    Method::POST,
                    "/api/v1/inventory/movements",
                    &tok_c,
                    &ten_c,
                    Some(json!({
                        "movement_type": "OUTBOUND",
                        "product_id": pr_c,
                        "source_warehouse_id": wh_c,
                        "quantity": 10
                    })),
                    None,
                )
                .await;

                if status == StatusCode::CREATED || status == StatusCode::UNPROCESSABLE_ENTITY {
                    return ("OUTBOUND", status, 10i64);
                }

                if status == StatusCode::CONFLICT && attempts < 40 {
                    let jitter_ms = (task_idx * 19 + attempts * 11) % 25 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return ("OUTBOUND", status, 10i64);
            }
        }));
    }

    let mut successful_inbound_qty = 0i64;
    let mut successful_outbound_qty = 0i64;

    for handle in handles {
        let (op, status, qty) = handle.await.unwrap();
        if op == "INBOUND" && status == StatusCode::CREATED {
            successful_inbound_qty += qty;
        } else if op == "OUTBOUND" && status == StatusCode::CREATED {
            successful_outbound_qty += qty;
        }
    }

    assert_eq!(successful_inbound_qty, 100, "All 10 inbound tasks must eventually succeed");
    assert!(
        successful_outbound_qty <= successful_inbound_qty,
        "OVERSOLD VIOLATION: outbound ({}) > inbound ({})",
        successful_outbound_qty,
        successful_inbound_qty
    );

    // Final balance check
    let db_qty: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(
        db_qty.0,
        successful_inbound_qty - successful_outbound_qty,
        "DB balance must equal inbound minus outbound"
    );
    assert!(db_qty.0 >= 0, "DB balance cannot be negative");
}

// =========================================================================
// CHALLENGE 6: Adversarial Boundary Attacks & DB Invariant Verification
// - Outbound quantity <= 0
// - Same warehouse transfer
// - Negative stock adjustment
// - Direct SQL violation against CHECK constraints and triggers
// =========================================================================
#[tokio::test]
async fn test_challenge_6_adversarial_boundary_and_db_immutability() {
    let harness = setup_challenger_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-ADV", "name": "Warehouse Adversarial" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap().to_string();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "name": "Adversarial SKU", "sale_price": 10000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    // Inbound 20 units
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 20
        })),
        None,
    )
    .await;

    // 1. Adversarial: Zero quantity outbound
    let (s_zero, b_zero, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": 0
        })),
        None,
    )
    .await;
    assert_eq!(s_zero, StatusCode::BAD_REQUEST);
    assert_eq!(b_zero["code"], "INVALID_QUANTITY");

    // 2. Adversarial: Negative quantity outbound
    let (s_neg, b_neg, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "OUTBOUND",
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "quantity": -10
        })),
        None,
    )
    .await;
    assert_eq!(s_neg, StatusCode::BAD_REQUEST);
    assert_eq!(b_neg["code"], "INVALID_QUANTITY");

    // 3. Adversarial: Transfer to same warehouse
    let (s_same, b_same, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/transfers",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "product_id": prod_id,
            "source_warehouse_id": wh_id,
            "destination_warehouse_id": wh_id,
            "quantity": 5
        })),
        None,
    )
    .await;
    assert_eq!(s_same, StatusCode::BAD_REQUEST);
    assert_eq!(b_same["code"], "SAME_WAREHOUSE_TRANSFER");

    // 4. Adversarial: Stock adjustment with negative actual_quantity
    let (s_adj_neg, b_adj_neg, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/adjustments",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "warehouse_id": wh_id,
            "product_id": prod_id,
            "actual_quantity": -5
        })),
        None,
    )
    .await;
    assert_eq!(s_adj_neg, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b_adj_neg["code"], "NEGATIVE_STOCK_PROHIBITED");

    // 5. Direct SQLite CHECK constraint enforcement
    let update_res = sqlx::query(
        "UPDATE stock_items SET quantity_on_hand = -10 WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .execute(&harness.pool)
    .await;
    assert!(update_res.is_err(), "SQLite CHECK constraint chk_stock_items_on_hand MUST fail on negative value");

    // 6. Direct SQLite Trigger: UPDATE stock_movements must abort
    let trigger_update_res = sqlx::query(
        "UPDATE stock_movements SET quantity = 999 WHERE tenant_id = ?1 AND product_id = ?2"
    )
    .bind(&harness.tenant_id)
    .bind(&prod_id)
    .execute(&harness.pool)
    .await;
    assert!(trigger_update_res.is_err(), "SQLite trigger trg_stock_movements_prevent_update MUST abort");

    // 7. Direct SQLite Trigger: DELETE stock_movements must abort
    let trigger_delete_res = sqlx::query(
        "DELETE FROM stock_movements WHERE tenant_id = ?1 AND product_id = ?2"
    )
    .bind(&harness.tenant_id)
    .bind(&prod_id)
    .execute(&harness.pool)
    .await;
    assert!(trigger_delete_res.is_err(), "SQLite trigger trg_stock_movements_prevent_delete MUST abort");
}

// =========================================================================
// CHALLENGE 7: Concurrent Idempotency Replay under Racing Load
// 10 concurrent tasks firing identical mutation with identical Idempotency-Key.
// Exactly ONE movement created; stock deducted exactly once (not 10 times).
// =========================================================================
#[tokio::test]
async fn test_challenge_7_concurrent_idempotency_key_replay_racing() {
    let harness = setup_challenger_harness().await;

    let (_, wh_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/warehouses",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "code": "WH-IDEMP", "name": "Warehouse Idempotency" })),
        None,
    )
    .await;
    let wh_id = wh_body["id"].as_str().unwrap().to_string();

    let (_, prod_body, _) = send_req(
        &harness.app,
        Method::POST,
        "/api/v1/products",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({ "name": "Idempotency SKU", "sale_price": 30000 })),
        None,
    )
    .await;
    let prod_id = prod_body["id"].as_str().unwrap().to_string();

    // Inbound 10 units
    send_req(
        &harness.app,
        Method::POST,
        "/api/v1/inventory/movements",
        &harness.owner_token,
        &harness.tenant_id,
        Some(json!({
            "movement_type": "INBOUND",
            "product_id": prod_id,
            "destination_warehouse_id": wh_id,
            "quantity": 10
        })),
        None,
    )
    .await;

    let idempotency_key = format!("idem-race-{}", Uuid::new_v4());
    let app = harness.app.clone();
    let token = harness.owner_token.clone();
    let tenant_id = harness.tenant_id.clone();

    let mut handles = Vec::new();
    for task_idx in 0..10 {
        let app_c = app.clone();
        let tok_c = token.clone();
        let ten_c = tenant_id.clone();
        let pr_c = prod_id.clone();
        let wh_c = wh_id.clone();
        let idemp_k = idempotency_key.clone();

        handles.push(tokio::spawn(async move {
            let mut headers = HeaderMap::new();
            headers.insert("Idempotency-Key", idemp_k.parse().unwrap());

            let mut attempts = 0;
            loop {
                attempts += 1;
                let (status, body, resp_headers) = send_req(
                    &app_c,
                    Method::POST,
                    "/api/v1/inventory/movements",
                    &tok_c,
                    &ten_c,
                    Some(json!({
                        "movement_type": "OUTBOUND",
                        "product_id": pr_c,
                        "source_warehouse_id": wh_c,
                        "quantity": 10
                    })),
                    Some(headers.clone()),
                )
                .await;

                // If in progress or lock contention, back off and retry
                if (status == StatusCode::CONFLICT && (body["code"] == "LOCK_CONTENTION" || body["code"] == "IDEMPOTENCY_IN_PROGRESS"))
                    && attempts < 40
                {
                    let jitter_ms = (task_idx * 17 + attempts * 13) % 25 + 10;
                    tokio::time::sleep(tokio::time::Duration::from_millis(jitter_ms as u64)).await;
                    continue;
                }

                return (status, body, resp_headers);
            }
        }));
    }

    let mut success_count = 0;
    for handle in handles {
        let (status, body, _headers) = handle.await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "All concurrent idempotent requests must succeed; got status={}, body={:?}", status, body);
        assert_eq!(body["quantity"], 10);
        success_count += 1;
    }

    assert_eq!(success_count, 10);

    // Exactly 1 movement must be created in DB!
    let mov_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM stock_movements WHERE tenant_id = ?1 AND product_id = ?2 AND movement_type = 'OUTBOUND'"
    )
    .bind(&harness.tenant_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(mov_count.0, 1, "Only 1 stock movement should be created despite 10 racing requests");

    // Stock on hand must be exactly 0, NOT -90!
    let db_qty: (i64,) = sqlx::query_as(
        "SELECT quantity_on_hand FROM stock_items WHERE tenant_id = ?1 AND warehouse_id = ?2 AND product_id = ?3"
    )
    .bind(&harness.tenant_id)
    .bind(&wh_id)
    .bind(&prod_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(db_qty.0, 0, "Stock must be exactly 0 (no duplicate deduction under racing idempotency)");
}
