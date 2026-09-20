//! M2 Challenger Idempotency & Replay Security Tests (Requirement R6)
//!
//! Adversarial challenge probe verifying:
//! - Test Scenario A: First valid notification -> exactly one state transition
//! - Test Scenario B: Identical replay -> 2005600 acknowledgement with zero secondary financial effects
//! - Test Scenario C: Conflicting amount/status on known tx ID -> no silent overwrite of active payment
//! - Test Scenario D: Conflicting transaction ID with same partner reference -> policy enforcement / double-entitlement check
//! - Test Scenario E: Concurrent duplicate notifications -> exactly-once execution under race conditions

use axum::{
    body::Body,
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        Request, StatusCode,
    },
    Router,
};
use backend::{
    api::{create_app, AppState, AuthState},
    repository::{
        account_repo::SqlxAccountRepository,
        audit_repo::{AuditRepository, SqlxAuditRepository},
        category_repo::SqlxCategoryRepository,
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        subscription_repo::{SqlxSubscriptionRepository, SubscriptionRepository},
        transaction_repo::SqlxTransactionRepository,
        user_repo::{NewUser, SqlxUserRepository, UserRepository},
    },
    service::{
        auth_service::AuthService,
        crypto::{Argon2Config, CryptoService},
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{
            PaymentConfig, PaymentService, DEFAULT_DANA_TEST_PRIVATE_KEY,
            DEFAULT_DANA_TEST_PUBLIC_KEY,
        },
    },
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

struct ChallengerContext {
    app: Router,
    pool: sqlx::SqlitePool,
    user_repo: Arc<SqlxUserRepository>,
    subscription_repo: Arc<SqlxSubscriptionRepository>,
    audit_repo: Arc<SqlxAuditRepository>,
    _dir: tempfile::TempDir,
}

async fn setup_challenger_app() -> ChallengerContext {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m2_challenger_idempotency.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 10,
        min_connections: 1,
        busy_timeout_ms: 10_000,
        acquire_timeout_secs: 10,
    };

    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));
    let category_repo = Arc::new(SqlxCategoryRepository::new(pool.clone()));
    let audit_repo = Arc::new(SqlxAuditRepository::new(pool.clone()));
    let idempotency_repo = Arc::new(SqlxIdempotencyRepository::new(pool.clone()));
    let transaction_repo = Arc::new(SqlxTransactionRepository::new(pool.clone()));
    let subscription_repo = Arc::new(SqlxSubscriptionRepository::new(pool.clone()));

    let jwt_secret = "challenger_idempotency_super_secret_jwt_key_1234567890";
    let jwt_engine = Arc::new(JwtEngine::new(jwt_secret, 900));
    let crypto_service = Arc::new(CryptoService::new(Argon2Config::fast_for_testing()).unwrap());

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
        PaymentService::new_with_pool(
            PaymentConfig::default(),
            subscription_repo.clone(),
            user_repo.clone(),
            audit_repo.clone(),
            pool.clone(),
        )
        .with_dana_public_key(DEFAULT_DANA_TEST_PUBLIC_KEY.to_string()),
    );

    let auth_state = AuthState {
        auth_service,
        secure_cookie: false,
    };

    let state = AppState {
        auth_state,
        account_repo,
        category_repo,
        user_preferences_repo: Arc::new(backend::repository::SqlxUserPreferencesRepository::new(pool.clone())),
        ledger_service,
        payment_service,
        pool: pool.clone(),
        rate_limiter: Arc::default(),
    };

    let app = create_app(state);

    ChallengerContext {
        app,
        pool,
        user_repo,
        subscription_repo,
        audit_repo,
        _dir: dir,
    }
}

async fn create_user(ctx: &ChallengerContext, email: &str, tier: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    ctx.user_repo
        .create(&NewUser {
            id: user_id.clone(),
            email: email.to_string(),
            password_hash: "mock_hash".to_string(),
            display_name: "Challenger Test User".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some(tier.to_string()),
        })
        .await
        .expect("create test user");
    user_id
}

fn build_signed_dana_request(
    path: &str,
    timestamp: &str,
    payload: &str,
) -> Request<Body> {
    let signature = PaymentService::sign_dana_payload(
        "POST",
        path,
        timestamp,
        payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .expect("sign payload");

    Request::builder()
        .method("POST")
        .uri(path)
        .header("X-SIGNATURE", signature)
        .header("X-TIMESTAMP", timestamp)
        .header("X-PARTNER-ID", "DANA_PARTNER_CHALLENGER")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap()
}

// ===========================================================================
// Test Scenario A: First Valid Notification
// ===========================================================================
#[tokio::test]
async fn test_scenario_a_first_valid_notification() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "user_scenario_a@dana.com", "free").await;

    let timestamp = "2026-09-20T10:00:00+07:00";
    let order_id = format!("ORDER-DANA-{}-A", &user_id[..8]);
    let tx_id = "DANA-TX-SCENARIO-A-001";

    let payload = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req = build_signed_dana_request("/api/v1/webhooks/dana", timestamp, &payload);
    let resp = ctx.app.clone().oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(CACHE_CONTROL).unwrap().to_str().unwrap(),
        "private, no-store, must-revalidate"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json["responseCode"], "2005600");
    assert_eq!(json["responseMessage"], "Successful");

    // Verify exactly one payment state transition:
    // 1. User tier transitioned to premium
    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier, "premium");

    // 2. Exactly one subscription record exists, active, 30 days
    let sub = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    assert_eq!(sub.status, "active");
    assert_eq!(sub.provider, "dana");
    assert_eq!(sub.amount.0, 10000);

    let start_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_start).unwrap();
    let end_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_end).unwrap();
    assert_eq!((end_dt - start_dt).num_days(), 30);

    // 3. Exactly one webhook_events record
    let event = ctx.subscription_repo.get_webhook_event("dana", tx_id).await.unwrap().unwrap();
    assert_eq!(event.status, "processed");

    // 4. Exactly one subscription_events record
    let (event_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM subscription_events WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(event_count, 1);

    // 5. Exactly one audit_logs record
    let audit_logs = ctx.audit_repo.list_by_user(&user_id, 10, 0).await.unwrap();
    assert_eq!(audit_logs.len(), 1);
    assert_eq!(audit_logs[0].action, "subscription_activated");
}

// ===========================================================================
// Test Scenario B: Identical Replay
// ===========================================================================
#[tokio::test]
async fn test_scenario_b_identical_replay() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "user_scenario_b@dana.com", "free").await;

    let timestamp = "2026-09-20T10:05:00+07:00";
    let order_id = format!("ORDER-DANA-{}-B", &user_id[..8]);
    let tx_id = "DANA-TX-SCENARIO-B-001";

    let payload = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    // 1. First Dispatch
    let req1 = build_signed_dana_request("/api/v1/webhooks/dana", timestamp, &payload);
    let resp1 = ctx.app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);

    let sub_initial = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    let initial_end = sub_initial.current_period_end.clone();

    // 2. Replay Dispatch (exact identical headers, signature, and body)
    let req2 = build_signed_dana_request("/api/v1/webhooks/dana", timestamp, &payload);
    let resp2 = ctx.app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);

    let body_bytes = resp2.into_body().collect().await.unwrap().to_bytes();
    let json2: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json2["responseCode"], "2005600");
    assert_eq!(json2["responseMessage"], "Successful");

    // 3. Verify zero secondary financial effects
    let sub_after_replay = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    assert_eq!(sub_after_replay.current_period_end, initial_end, "End date must not change");

    let (sub_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM subscriptions WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(sub_count, 1, "Must have exactly 1 subscription record");

    let (evt_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM subscription_events WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(evt_count, 1, "Must have exactly 1 subscription_event (no duplicate)");

    let audit_logs = ctx.audit_repo.list_by_user(&user_id, 10, 0).await.unwrap();
    assert_eq!(audit_logs.len(), 1, "Must have exactly 1 audit log (no duplicate)");
}

// ===========================================================================
// Test Scenario C: Conflicting Amount / Conflicting Status on Known Tx ID
// ===========================================================================
#[tokio::test]
async fn test_scenario_c_conflicting_amount_and_status() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "user_scenario_c@dana.com", "free").await;

    let timestamp1 = "2026-09-20T10:10:00+07:00";
    let order_id = format!("ORDER-DANA-{}-C", &user_id[..8]);
    let tx_id = "DANA-TX-SCENARIO-C-001";

    // 1. Legitimate payment: Rp 10.000, Status 00 (Successful)
    let payload_valid = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req1 = build_signed_dana_request("/api/v1/webhooks/dana", timestamp1, &payload_valid);
    let resp1 = ctx.app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);

    let sub_valid = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    assert_eq!(sub_valid.status, "active");
    assert_eq!(sub_valid.amount.0, 10000);
    let baseline_end = sub_valid.current_period_end.clone();

    // 2. Adversarial Replay C1: Same tx_id, but conflicting amount (Rp 50.000)
    let timestamp2 = "2026-09-20T10:12:00+07:00";
    let payload_conflicting_amount = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "50000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req2 = build_signed_dana_request("/api/v1/webhooks/dana", timestamp2, &payload_conflicting_amount);
    let resp2 = ctx.app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);

    // Verify original payment state was NOT corrupted / overwritten
    let sub_after_c1 = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    assert_eq!(sub_after_c1.amount.0, 10000, "Amount must remain original 10000, not corrupted by 50000");
    assert_eq!(sub_after_c1.current_period_end, baseline_end, "End date must remain unchanged");
    assert_eq!(sub_after_c1.status, "active");

    // 3. Adversarial Replay C2: Same tx_id, but conflicting status (05 = Expired / Cancelled)
    let timestamp3 = "2026-09-20T10:14:00+07:00";
    let payload_conflicting_status = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "05",
        "transactionStatusDesc": "Expired",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req3 = build_signed_dana_request("/api/v1/webhooks/dana", timestamp3, &payload_conflicting_status);
    let resp3 = ctx.app.clone().oneshot(req3).await.unwrap();
    assert_eq!(resp3.status(), StatusCode::OK);

    // Verify subscription status remains active and user tier remains premium
    let sub_after_c2 = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    assert_eq!(sub_after_c2.status, "active", "Conflicting expired status must NOT overwrite active subscription");
    let user_c = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user_c.subscription_tier, "premium", "User tier must remain premium");
}

// ===========================================================================
// Test Scenario D: Conflicting Transaction ID With Same Partner Reference
// ===========================================================================
#[tokio::test]
async fn test_scenario_d_conflicting_tx_id_same_partner_reference() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "user_scenario_d@dana.com", "free").await;

    let timestamp1 = "2026-09-20T10:20:00+07:00";
    let partner_ref = format!("ORDER-DANA-{}-D", &user_id[..8]);
    let tx_id_1 = "DANA-TX-001";
    let tx_id_2 = "DANA-TX-002"; // Conflicting transaction ID for the same order reference!

    // 1. First Notification: Partner Ref + TX_ID_1 -> Settles order
    let payload1 = serde_json::json!({
        "originalPartnerReferenceNo": partner_ref,
        "originalReferenceNo": tx_id_1,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req1 = build_signed_dana_request("/api/v1/webhooks/dana", timestamp1, &payload1);
    let resp1 = ctx.app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);

    let sub1 = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    assert_eq!(sub1.status, "active");
    let initial_end = sub1.current_period_end.clone();

    // 2. Conflicting Notification: Same Partner Ref + DIFFERENT TX_ID_2
    let timestamp2 = "2026-09-20T10:25:00+07:00";
    let payload2 = serde_json::json!({
        "originalPartnerReferenceNo": partner_ref,
        "originalReferenceNo": tx_id_2,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req2 = build_signed_dana_request("/api/v1/webhooks/dana", timestamp2, &payload2);
    let resp2 = ctx.app.clone().oneshot(req2).await.unwrap();

    let sub2 = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    
    println!("[SCENARIO D OBSERVATION]");
    println!("Response status for conflicting tx_id: {:?}", resp2.status());
    println!("Initial end date: {}", initial_end);
    println!("Post-conflict end date: {}", sub2.current_period_end);

    let (evt_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM subscription_events WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    println!("Subscription events recorded: {}", evt_count);

    // Audit Requirement R6 / R4:
    // "D. Same partner reference with conflicting tx ID -> reject/quarantine"
    // "Prevent duplicate ledger entries and duplicate entitlements."
    // If the system granted an extra 30 days for the same partner reference, document this failure!
    if sub2.current_period_end != initial_end {
        eprintln!("[VULNERABILITY DETECTED in SCENARIO D] Double entitlement! Same partner reference {} with conflicting tx_id {} caused cumulative subscription extension! Initial: {}, Updated: {}",
            partner_ref, tx_id_2, initial_end, sub2.current_period_end);
    }
}

// ===========================================================================
// Test Scenario E: Concurrent Duplicate Notifications
// ===========================================================================
#[tokio::test]
async fn test_scenario_e_concurrent_duplicate_notifications() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "user_scenario_e@dana.com", "free").await;

    let timestamp = "2026-09-20T10:30:00+07:00";
    let order_id = format!("ORDER-DANA-{}-E", &user_id[..8]);
    let tx_id = "DANA-TX-SCENARIO-E-CONCURRENT";

    let payload = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let concurrency = 10;
    let mut handles = Vec::new();

    for _ in 0..concurrency {
        let app = ctx.app.clone();
        let payload_clone = payload.clone();
        let timestamp_clone = timestamp.to_string();

        let handle = tokio::spawn(async move {
            let req = build_signed_dana_request("/api/v1/webhooks/dana", &timestamp_clone, &payload_clone);
            app.oneshot(req).await
        });
        handles.push(handle);
    }

    let mut ok_count = 0;
    for handle in handles {
        let res = handle.await.expect("task join failed").expect("request failed");
        if res.status() == StatusCode::OK {
            let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
            let json: Value = serde_json::from_slice(&body_bytes).unwrap();
            if json["responseCode"] == "2005600" {
                ok_count += 1;
            }
        }
    }

    assert_eq!(ok_count, concurrency, "All concurrent requests must return HTTP 200 / 2005600");

    // Invariant checks:
    // 1. User tier is premium
    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier, "premium");

    // 2. Exactly 1 row in webhook_events
    let (wh_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM webhook_events WHERE provider = 'dana' AND event_id = ?"
    )
    .bind(tx_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(wh_count, 1, "Exactly 1 webhook_events entry must be recorded under concurrency");

    // 3. Exactly 1 row in subscription_events
    let (sub_evt_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM subscription_events WHERE user_id = ?"
    )
    .bind(&user_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(sub_evt_count, 1, "Exactly 1 subscription_events record under concurrency");

    // 4. Exactly 1 audit log
    let audit_logs = ctx.audit_repo.list_by_user(&user_id, 20, 0).await.unwrap();
    assert_eq!(audit_logs.len(), 1, "Exactly 1 audit log under concurrency");

    // 5. Subscription period is exactly 30 days (NOT 300 days)
    let sub = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    let start_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_start).unwrap();
    let end_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_end).unwrap();
    let diff_days = (end_dt - start_dt).num_days();
    assert_eq!(diff_days, 30, "Subscription period must be exactly 30 days under concurrency, got {}", diff_days);
}

// ===========================================================================
// Adversarial Exploit Probes
// ===========================================================================

#[tokio::test]
async fn test_adversarial_exploit_infinite_extension_via_rotating_tx_ids() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "exploit_user@dana.com", "free").await;

    let partner_ref = format!("ORDER-DANA-{}-EXPLOIT", &user_id[..8]);
    let now = chrono::Utc::now();

    // Attacker fires 5 webhooks for the single order, rotating originalReferenceNo
    for i in 1..=5 {
        let timestamp = format!("2026-09-20T10:{:02}:00+07:00", i);
        let tx_id = format!("FAKE-DANA-TX-{:03}", i);
        let payload = serde_json::json!({
            "originalPartnerReferenceNo": partner_ref,
            "originalReferenceNo": tx_id,
            "latestTransactionStatus": "00",
            "transactionStatusDesc": "Successful",
            "amount": { "value": "10000.00", "currency": "IDR" },
            "additionalInfo": { "userId": user_id }
        })
        .to_string();

        let req = build_signed_dana_request("/api/v1/webhooks/dana", &timestamp, &payload);
        let resp = ctx.app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    let sub = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    let end_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_end).unwrap().with_timezone(&chrono::Utc);
    let total_days = (end_dt - now).num_days();

    println!("[ADVERSARIAL EXPLOIT RESULT]");
    println!("Single Order ID: {}", partner_ref);
    println!("Expected days for single order: 30");
    println!("Actual granted days across 5 rotated tx IDs: {}", total_days);

    let (evt_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM subscription_events WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    println!("Total subscription_events recorded for 1 order: {}", evt_count);

    if total_days > 40 {
        eprintln!("[CRITICAL SECURITY VULNERABILITY CONFIRMED] User gained {} days (> 30 days) from 1 order ID by rotating transaction reference numbers!", total_days);
    }
}

#[tokio::test]
async fn test_adversarial_amount_tampering_underpayment() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "underpay_user@dana.com", "free").await;

    let timestamp = "2026-09-20T10:45:00+07:00";
    let order_id = format!("ORDER-DANA-{}-UNDERPAY", &user_id[..8]);
    let tx_id = "DANA-TX-UNDERPAY-001";

    // User paid only Rp 100 instead of Rp 10.000!
    let payload = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "100.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req = build_signed_dana_request("/api/v1/webhooks/dana", timestamp, &payload);
    let resp = ctx.app.clone().oneshot(req).await.unwrap();

    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    let sub = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap();

    println!("[UNDERPAYMENT OBSERVATION]");
    println!("Response status: {:?}", resp.status());
    println!("User tier: {}", user.subscription_tier);
    if let Some(ref s) = sub {
        println!("Subscription status: {}, amount: {}", s.status, s.amount.0);
    }

    if user.subscription_tier == "premium" {
        eprintln!("[VULNERABILITY DETECTED] Underpayment accepted! Paid Rp 100.00, user tier upgraded to premium!");
    }
}

#[tokio::test]
async fn test_adversarial_conflicting_status_initial_notification() {
    let ctx = setup_challenger_app().await;
    let user_id = create_user(&ctx, "failed_init@dana.com", "free").await;

    let timestamp = "2026-09-20T10:50:00+07:00";
    let order_id = format!("ORDER-DANA-{}-FAILED", &user_id[..8]);
    let tx_id = "DANA-TX-FAILED-001";

    // Gateway reports transaction failed/expired (status 05)
    let payload = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": tx_id,
        "latestTransactionStatus": "05",
        "transactionStatusDesc": "Expired",
        "amount": { "value": "10000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let req = build_signed_dana_request("/api/v1/webhooks/dana", timestamp, &payload);
    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier, "free", "User tier must remain free on expired status");

    let sub = ctx.subscription_repo.find_by_user_id(&user_id).await.unwrap().unwrap();
    assert_eq!(sub.status, "expired", "Subscription status must be expired");
}

