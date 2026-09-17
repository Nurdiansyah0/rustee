//! M2 Integration Tests — Direct DANA Open API & Webhook Engine
//!
//! Validates:
//! - Cryptographic signature verification using PKCS#1 v1.5 RSA-SHA256 against Bank Indonesia SNAP String-to-Sign
//! - Missing or invalid signature rejection with HTTP 401 Unauthorized: {"responseCode": "4015600", "responseMessage": "Unauthorized: Invalid Signature"}
//! - Idempotent deduplication preventing replay attacks (HTTP 200 without duplicate days or audit logs)
//! - Atomic subscription settlement from Free tier to Active Premium (+30 days)
//! - Atomic subscription settlement from 7-day Trialing tier to Active Premium (+30 days)
//! - Cumulative subscription extension (+30 days added to existing active period end)
//! - DANA checkout session generation for Rp 5.000 / month with order ID, checkout URL, reference number
//! - Immutable audit log trail verification ("subscription_activated", provider "dana")

use axum::{
    body::Body,
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, COOKIE},
        Request, StatusCode,
    },
    Router,
};
use backend::{
    api::{create_app, AppState, AuthState},
    domain::money::Rupiah,
    repository::{
        account_repo::SqlxAccountRepository,
        audit_repo::{AuditRepository, SqlxAuditRepository},
        category_repo::SqlxCategoryRepository,
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        subscription_repo::{NewSubscription, SqlxSubscriptionRepository, SubscriptionRepository},
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
use base64::prelude::*;
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use rsa::{pkcs1v15::VerifyingKey, pkcs8::DecodePublicKey, signature::Verifier, RsaPublicKey};
use serde_json::Value;
use sha2::Sha256;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

struct TestContext {
    app: Router,
    pool: sqlx::SqlitePool,
    jwt_engine: Arc<JwtEngine>,
    user_repo: Arc<SqlxUserRepository>,
    subscription_repo: Arc<SqlxSubscriptionRepository>,
    audit_repo: Arc<SqlxAuditRepository>,
    _dir: tempfile::TempDir,
}

async fn setup_app() -> TestContext {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m2_dana_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 5,
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

    let jwt_secret = "m2_dana_super_secure_jwt_secret_testing_key_123456";
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

    TestContext {
        app,
        pool,
        jwt_engine,
        user_repo,
        subscription_repo,
        audit_repo,
        _dir: dir,
    }
}

async fn create_test_user(ctx: &TestContext, email: &str, tier: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    ctx.user_repo
        .create(&NewUser {
            id: user_id.clone(),
            email: email.to_string(),
            password_hash: "hash".to_string(),
            display_name: "DANA Test User".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some(tier.to_string()),
        })
        .await
        .expect("create test user");
    user_id
}

fn generate_auth_token(ctx: &TestContext, user_id: &str, email: &str, tier: &str) -> String {
    let (token, _) = ctx
        .jwt_engine
        .generate_token(user_id, email, "user", tier)
        .expect("token generation");
    token
}

// ---------------------------------------------------------------------------
// 1. Pure Crypto Tests
// ---------------------------------------------------------------------------

#[test]
fn test_rsa_crypto_signing_and_verification() {
    let string_to_sign = "POST:/api/v1/webhooks/dana:123456abcdef:2026-09-13T02:30:00+07:00";

    // Sign with test private key
    let sig_b64 =
        PaymentService::sign_dana_string_to_sign(string_to_sign, DEFAULT_DANA_TEST_PRIVATE_KEY)
            .expect("sign payload");

    // Verify signature with test public key
    let pub_key =
        RsaPublicKey::from_public_key_pem(DEFAULT_DANA_TEST_PUBLIC_KEY).expect("decode public key");
    let sig_bytes = BASE64_STANDARD.decode(&sig_b64).expect("decode base64");
    let parsed_sig = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice()).expect("parse sig");
    let verifying_key = VerifyingKey::<Sha256>::new(pub_key);

    assert!(verifying_key
        .verify(string_to_sign.as_bytes(), &parsed_sig)
        .is_ok());

    // Tampered String-to-Sign fails verification
    let tampered = "POST:/api/v1/webhooks/dana:999999tamper:2026-09-13T02:30:00+07:00";
    assert!(verifying_key
        .verify(tampered.as_bytes(), &parsed_sig)
        .is_err());
}

// ---------------------------------------------------------------------------
// 2. DANA Checkout Session Generation Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_checkout_session_generation() {
    let ctx = setup_app().await;
    let user_id = create_test_user(&ctx, "checkout@dana.com", "free").await;
    let token = generate_auth_token(&ctx, &user_id, "checkout@dana.com", "free");

    // 1. DANA checkout
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/subscriptions/checkout")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"provider": "dana"}"#))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(CACHE_CONTROL).unwrap().to_str().unwrap(),
        "private, no-store, must-revalidate"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    let order_id = json["order_id"].as_str().expect("order_id string");
    assert!(
        order_id.starts_with("ORDER-DANA-"),
        "Order ID should start with ORDER-DANA-, got {}",
        order_id
    );

    let checkout_url = json["checkout_url"].as_str().expect("checkout_url string");
    assert!(
        checkout_url.starts_with("https://m.dana.id/d/checkout?orderId="),
        "Checkout URL should point to DANA checkout, got {}",
        checkout_url
    );
    assert!(checkout_url.contains(order_id));

    let reference_no = json["reference_no"].as_str().expect("reference_no string");
    assert!(
        reference_no.starts_with("REF-DANA-"),
        "Reference no should start with REF-DANA-, got {}",
        reference_no
    );

    assert_eq!(json["amount"], 10000);
    assert_eq!(json["currency"], "IDR");
    assert_eq!(json["provider"], "dana");
    assert_eq!(json["plan_id"], "premium_monthly");

    // 1b. DANA annual checkout session generation
    let req_annual = Request::builder()
        .method("POST")
        .uri("/api/v1/subscriptions/checkout")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"provider": "dana", "plan_id": "premium_annual"}"#))
        .unwrap();

    let resp_annual = ctx.app.clone().oneshot(req_annual).await.unwrap();
    assert_eq!(resp_annual.status(), StatusCode::OK);
    let json_annual: Value =
        serde_json::from_slice(&resp_annual.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json_annual["amount"], 110000);
    assert_eq!(json_annual["currency"], "IDR");
    assert_eq!(json_annual["provider"], "dana");
    assert_eq!(json_annual["plan_id"], "premium_annual");

    // 2. Non-DANA checkout provider (e.g., midtrans) rejected with HTTP 400 (DANA exclusivity)
    let req_mid = Request::builder()
        .method("POST")
        .uri("/api/v1/subscriptions/checkout")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"provider": "midtrans"}"#))
        .unwrap();

    let resp_mid = ctx.app.clone().oneshot(req_mid).await.unwrap();
    assert_eq!(resp_mid.status(), StatusCode::BAD_REQUEST);

    // 3. Unsupported provider rejected
    let req_bad = Request::builder()
        .method("POST")
        .uri("/api/v1/subscriptions/checkout")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"provider": "paypal"}"#))
        .unwrap();

    let resp_bad = ctx.app.clone().oneshot(req_bad).await.unwrap();
    assert_eq!(resp_bad.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// 3. Valid RSA-SHA256 Webhook Settlement From Free Tier
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_valid_signature_settlement_from_free() {
    let ctx = setup_app().await;
    let user_id = create_test_user(&ctx, "free_to_premium@dana.com", "free").await;

    let timestamp = "2026-09-13T09:30:00+07:00";
    let order_id = format!("ORDER-DANA-{}-1726200000", &user_id[..8]);
    let ref_no = "202609131234567890";

    let payload = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": ref_no,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": {
            "value": "5000.00",
            "currency": "IDR"
        },
        "additionalInfo": {
            "userId": user_id
        }
    })
    .to_string();

    let signature = PaymentService::sign_dana_payload(
        "POST",
        "/api/v1/webhooks/dana",
        timestamp,
        payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .expect("sign payload");

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", signature)
        .header("X-TIMESTAMP", timestamp)
        .header("X-PARTNER-ID", "DANA_PARTNER_001")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

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

    // Verify user tier is now premium
    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier, "premium");

    // Verify subscription record
    let sub = ctx
        .subscription_repo
        .find_by_user_id(&user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.status, "active");
    assert_eq!(sub.provider, "dana");
    assert_eq!(sub.amount.0, 5000);

    let start_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_start).unwrap();
    let end_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_end).unwrap();
    let days = (end_dt - start_dt).num_days();
    assert_eq!(days, 30, "Subscription period should be 30 days");

    // Verify audit log entry
    let logs = ctx.audit_repo.list_by_user(&user_id, 10, 0).await.unwrap();
    assert!(!logs.is_empty(), "Audit log should be recorded");
    let log = logs
        .iter()
        .find(|l| l.action == "subscription_activated")
        .expect("subscription_activated audit log");
    assert!(log.details.as_ref().unwrap().contains("provider=dana"));
    assert!(log.details.as_ref().unwrap().contains(ref_no));
}

// ---------------------------------------------------------------------------
// 4. Webhook Settlement From 7-Day Trialing State
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_settlement_from_trialing() {
    let ctx = setup_app().await;
    let user_id = create_test_user(&ctx, "trial_to_premium@dana.com", "premium").await;

    // Set user up in trialing state
    let now = Utc::now();
    let trial_end = now + Duration::days(7);
    let now_str = now.to_rfc3339();
    let trial_end_str = trial_end.to_rfc3339();

    sqlx::query(
        "UPDATE users SET has_used_trial = 1, trial_started_at = ?1, trial_ends_at = ?2 WHERE id = ?3",
    )
    .bind(&now_str)
    .bind(&trial_end_str)
    .bind(&user_id)
    .execute(&ctx.pool)
    .await
    .unwrap();

    ctx.subscription_repo
        .upsert_subscription(&NewSubscription {
            id: format!("sub_trial_{}", user_id),
            user_id: user_id.clone(),
            provider: "trial".to_string(),
            provider_subscription_id: Some("trial-sub".to_string()),
            plan_id: "premium_trial_7d".to_string(),
            status: "trialing".to_string(),
            amount: Rupiah(0),
            current_period_start: now_str.clone(),
            current_period_end: trial_end_str.clone(),
            cancel_at_period_end: true,
        })
        .await
        .unwrap();

    // Now send DANA payment webhook
    let timestamp = "2026-09-13T09:35:00+07:00";
    let payload = serde_json::json!({
        "originalPartnerReferenceNo": format!("ORDER-DANA-{}-1726200000", &user_id[..8]),
        "originalReferenceNo": "202609139988776655",
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": {
            "value": "5000.00",
            "currency": "IDR"
        },
        "additionalInfo": {
            "userId": user_id
        }
    })
    .to_string();

    let signature = PaymentService::sign_dana_payload(
        "POST",
        "/api/v1/webhooks/dana",
        timestamp,
        payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .expect("sign payload");

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", signature)
        .header("X-TIMESTAMP", timestamp)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify user tier is still premium and subscription is active DANA
    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier, "premium");

    let sub = ctx
        .subscription_repo
        .find_by_user_id(&user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.status, "active");
    assert_eq!(sub.provider, "dana");
    assert_eq!(sub.plan_id, "premium_monthly");
    assert_eq!(sub.amount.0, 5000);

    // Should have 30 days from settlement time
    let start_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_start).unwrap();
    let end_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_end).unwrap();
    assert_eq!((end_dt - start_dt).num_days(), 30);
}

// ---------------------------------------------------------------------------
// 5. Cumulative Extension for Already Active Subscriber
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_extension_for_already_active_subscriber() {
    let ctx = setup_app().await;
    let user_id = create_test_user(&ctx, "active_extend@dana.com", "premium").await;

    // Existing active subscription with 15 days remaining
    let now = Utc::now();
    let initial_end = now + Duration::days(15);
    ctx.subscription_repo
        .upsert_subscription(&NewSubscription {
            id: format!("sub_dana_{}", user_id),
            user_id: user_id.clone(),
            provider: "dana".to_string(),
            provider_subscription_id: Some("prev_order".to_string()),
            plan_id: "premium_monthly".to_string(),
            status: "active".to_string(),
            amount: Rupiah(5000),
            current_period_start: now.to_rfc3339(),
            current_period_end: initial_end.to_rfc3339(),
            cancel_at_period_end: false,
        })
        .await
        .unwrap();

    // Deliver new payment notification
    let timestamp = "2026-09-13T09:40:00+07:00";
    let payload = serde_json::json!({
        "originalPartnerReferenceNo": format!("ORDER-DANA-{}-renewal", &user_id[..8]),
        "originalReferenceNo": "20260913_renewal_12345",
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": {
            "value": "5000.00",
            "currency": "IDR"
        },
        "additionalInfo": {
            "userId": user_id
        }
    })
    .to_string();

    let signature = PaymentService::sign_dana_payload(
        "POST",
        "/api/v1/webhooks/dana",
        timestamp,
        payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .expect("sign payload");

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", signature)
        .header("X-TIMESTAMP", timestamp)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let sub = ctx
        .subscription_repo
        .find_by_user_id(&user_id)
        .await
        .unwrap()
        .unwrap();
    let updated_end = chrono::DateTime::parse_from_rfc3339(&sub.current_period_end)
        .unwrap()
        .with_timezone(&Utc);

    // 15 days + 30 days = 45 days total from now
    let diff_days = (updated_end - now).num_days();
    assert!(
        (44..=46).contains(&diff_days),
        "Expected ~45 days remaining after 30-day renewal extension, got {}",
        diff_days
    );
}

// ---------------------------------------------------------------------------
// 6. Invalid Signature Rejection (HTTP 401)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_invalid_signature_rejected_401() {
    let ctx = setup_app().await;
    let user_id = create_test_user(&ctx, "tamper@dana.com", "free").await;

    let timestamp = "2026-09-13T09:45:00+07:00";
    let payload = serde_json::json!({
        "originalPartnerReferenceNo": format!("ORDER-DANA-{}", &user_id[..8]),
        "originalReferenceNo": "20260913tamper01",
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "5000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    // 1. Invalid signature bytes
    let req_bad_sig = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", "dGhpc19pc19hbl9pbnZhbGlkX3NpZ25hdHVyZQ==")
        .header("X-TIMESTAMP", timestamp)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.clone()))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req_bad_sig).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let json: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["responseCode"], "4015600");
    assert_eq!(json["responseMessage"], "Unauthorized: Invalid Signature");

    // 2. Tampered body with signature calculated on different content
    let valid_sig = PaymentService::sign_dana_payload(
        "POST",
        "/api/v1/webhooks/dana",
        timestamp,
        payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .unwrap();

    let tampered_payload = payload.replace("5000.00", "1000.00");
    let req_tampered_body = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", valid_sig.clone())
        .header("X-TIMESTAMP", timestamp)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(tampered_payload))
        .unwrap();

    let resp2 = ctx.app.clone().oneshot(req_tampered_body).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::UNAUTHORIZED);
    let json2: Value =
        serde_json::from_slice(&resp2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json2["responseCode"], "4015600");

    // 3. Tampered timestamp
    let req_tampered_ts = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", valid_sig)
        .header("X-TIMESTAMP", "2026-09-13T09:45:01+07:00") // 1 sec different
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp3 = ctx.app.clone().oneshot(req_tampered_ts).await.unwrap();
    assert_eq!(resp3.status(), StatusCode::UNAUTHORIZED);

    // Verify user remained free
    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier, "free");
}

// ---------------------------------------------------------------------------
// 7. Missing Signature Headers Rejection (HTTP 401)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_missing_signature_headers_rejected() {
    let ctx = setup_app().await;

    let payload = r#"{"originalReferenceNo": "12345"}"#;

    // Missing X-SIGNATURE
    let req_no_sig = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-TIMESTAMP", "2026-09-13T09:00:00+07:00")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp1 = ctx.app.clone().oneshot(req_no_sig).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::UNAUTHORIZED);
    let json1: Value =
        serde_json::from_slice(&resp1.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json1["responseCode"], "4015600");

    // Missing X-TIMESTAMP
    let req_no_ts = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", "some_signature")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp2 = ctx.app.clone().oneshot(req_no_ts).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::UNAUTHORIZED);
    let json2: Value =
        serde_json::from_slice(&resp2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json2["responseCode"], "4015600");

    // Completely empty headers
    let req_empty = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp3 = ctx.app.clone().oneshot(req_empty).await.unwrap();
    assert_eq!(resp3.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// 8. Replay Attack & Duplicate Webhook Idempotency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_idempotency_replay_protection() {
    let ctx = setup_app().await;
    let user_id = create_test_user(&ctx, "replay@dana.com", "free").await;

    let timestamp = "2026-09-13T09:50:00+07:00";
    let event_ref = "DANA-UNIQUE-TX-998877";
    let payload = serde_json::json!({
        "originalPartnerReferenceNo": format!("ORDER-DANA-{}-9988", &user_id[..8]),
        "originalReferenceNo": event_ref,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": { "value": "5000.00", "currency": "IDR" },
        "additionalInfo": { "userId": user_id }
    })
    .to_string();

    let signature = PaymentService::sign_dana_payload(
        "POST",
        "/api/v1/webhooks/dana",
        timestamp,
        payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .expect("sign payload");

    // 1. First Delivery: Processes successfully
    let req1 = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", signature.clone())
        .header("X-TIMESTAMP", timestamp)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.clone()))
        .unwrap();

    let resp1 = ctx.app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let json1: Value =
        serde_json::from_slice(&resp1.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json1["responseCode"], "2005600");

    let sub_after_first = ctx
        .subscription_repo
        .find_by_user_id(&user_id)
        .await
        .unwrap()
        .unwrap();
    let initial_end = sub_after_first.current_period_end.clone();

    let logs_after_first = ctx.audit_repo.list_by_user(&user_id, 10, 0).await.unwrap();
    let initial_log_count = logs_after_first.len();

    // 2. Replay Delivery: Exact same notification delivered again
    let req2 = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", signature)
        .header("X-TIMESTAMP", timestamp)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp2 = ctx.app.clone().oneshot(req2).await.unwrap();
    assert_eq!(
        resp2.status(),
        StatusCode::OK,
        "Replay should return HTTP 200 OK"
    );
    let json2: Value =
        serde_json::from_slice(&resp2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json2["responseCode"], "2005600");
    assert_eq!(json2["responseMessage"], "Successful");

    // 3. Invariant check: zero extra days added
    let sub_after_replay = ctx
        .subscription_repo
        .find_by_user_id(&user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        sub_after_replay.current_period_end, initial_end,
        "Replayed notification must NOT extend subscription end date"
    );

    // 4. Invariant check: zero extra audit logs
    let logs_after_replay = ctx.audit_repo.list_by_user(&user_id, 10, 0).await.unwrap();
    assert_eq!(
        logs_after_replay.len(),
        initial_log_count,
        "Replayed notification must NOT create duplicate audit logs"
    );
}

// ---------------------------------------------------------------------------
// 9. Invalid Payload Format Rejection (HTTP 400)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_invalid_payload_rejected_400() {
    let ctx = setup_app().await;

    let timestamp = "2026-09-13T09:55:00+07:00";
    let bad_payload = "{ this is not valid json }";

    let signature = PaymentService::sign_dana_payload(
        "POST",
        "/api/v1/webhooks/dana",
        timestamp,
        bad_payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .expect("sign payload");

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", signature)
        .header("X-TIMESTAMP", timestamp)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(bad_payload))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["responseCode"], "4005600");
}

// ---------------------------------------------------------------------------
// 10. DANA Annual Settlement (Rp 110.000, +365 days) and subscription_events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dana_webhook_annual_settlement_and_subscription_events() {
    let ctx = setup_app().await;
    let user_id = create_test_user(&ctx, "annual_user@dana.com", "free").await;

    let timestamp = "2026-09-13T10:00:00+07:00";
    let order_id = format!("ORDER-DANA-ANNUAL-{}", &user_id[..8]);
    let ref_no = "202609139988776655";

    let payload = serde_json::json!({
        "originalPartnerReferenceNo": order_id,
        "originalReferenceNo": ref_no,
        "latestTransactionStatus": "00",
        "transactionStatusDesc": "Successful",
        "amount": {
            "value": "110000.00",
            "currency": "IDR"
        },
        "additionalInfo": {
            "userId": user_id,
            "planId": "premium_annual"
        }
    })
    .to_string();

    let signature = PaymentService::sign_dana_payload(
        "POST",
        "/api/v1/webhooks/dana",
        timestamp,
        payload.as_bytes(),
        DEFAULT_DANA_TEST_PRIVATE_KEY,
    )
    .expect("sign payload");

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/webhooks/dana")
        .header("X-SIGNATURE", signature)
        .header("X-TIMESTAMP", timestamp)
        .header("X-PARTNER-ID", "DANA_PARTNER_001")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(json["responseCode"], "2005600");
    assert_eq!(json["responseMessage"], "Successful");

    // 1. Verify user tier upgraded to premium
    let user = ctx.user_repo.find_by_id(&user_id).await.unwrap().unwrap();
    assert_eq!(user.subscription_tier, "premium");

    // 2. Verify subscription amount is 110,000 and duration is ~365 days
    let sub = ctx
        .subscription_repo
        .find_by_user_id(&user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sub.amount.0, 110000);
    let start_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_start).unwrap();
    let end_dt = chrono::DateTime::parse_from_rfc3339(&sub.current_period_end).unwrap();
    let days_extended = (end_dt - start_dt).num_days();
    assert!(
        days_extended >= 364 && days_extended <= 366,
        "Annual subscription should be extended ~365 days, got {}",
        days_extended
    );

    // 3. Verify subscription_events record was persisted
    let row: (i64, String, String) = sqlx::query_as(
        "SELECT COUNT(*), event_type, provider FROM subscription_events WHERE user_id = ?"
    )
    .bind(&user_id)
    .fetch_one(&ctx.pool)
    .await
    .expect("fetch subscription event");
    assert!(row.0 >= 1, "At least 1 subscription_event should be recorded");
    assert_eq!(row.1, "subscription_activated");
    assert_eq!(row.2, "dana");

    // 4. Verify audit_logs record exists
    let audit_logs = ctx.audit_repo.list_by_user(&user_id, 10, 0).await.unwrap();
    assert!(!audit_logs.is_empty(), "Audit logs must contain settlement record");
    assert!(audit_logs.iter().any(|l| l.action == "subscription_activated" && l.details.as_ref().map(|d| d.contains("premium_annual")).unwrap_or(false)));
}


