//! M6 Integration Tests — Automated Transaction Ingestion & Deduplication Pipeline
//!
//! Validates strictly against Master Specification v3.1.0 and Milestone 3 (R3):
//! 1. 9-Stage Ingestion Pipeline:
//!    Source Adapter -> Parser -> Normalizer -> Validator -> Confidence Engine ->
//!    Deduplication -> Transaction Candidate -> Domain Validation -> Ledger Ingestion.
//! 2. Parsers for 8 Indonesian providers (BCA, Mandiri, BRI, BNI, DANA, GoPay, OVO, ShopeePay),
//!    SMS capability adapter, and Gmail OAuth payload adapter.
//! 3. Confidence Engine & Actions:
//!    - HIGH: Recognized provider pattern & resolved account -> auto-creates in ledger.
//!    - MEDIUM: Ambiguity in account/category -> candidate requiring user confirmation.
//!    - LOW: Unresolved or non-financial -> rejected.
//! 4. Cross-Source Signal Deduplication:
//!    Matching signals (provider, ref, amount, direction, window ±300s, merchant) prevents
//!    duplicate transaction creation across notification, SMS, and email.
//! 5. Candidate Review API:
//!    - POST /api/v1/ingestion/notification with Idempotency-Key
//!    - GET /api/v1/ingestion/candidates
//!    - POST /api/v1/ingestion/candidates/:id/confirm (atomic ledger commit)
//!    - POST /api/v1/ingestion/candidates/:id/reject
//! 6. Privacy & Payload Minimization (REQ-INGEST-08):
//!    Raw payload is discarded; only SHA-256 hash stored in ingestion_events.raw_payload_hash.
//! 7. Multi-tenant isolation & Server-side feature gating.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
    },
    Router,
};
use backend::{
    api::{create_app, AppState, AuthState},
    domain::ingestion::*,
    domain::money::Rupiah,
    repository::{
        account_repo::{AccountRepository, NewAccount, SqlxAccountRepository},
        audit_repo::SqlxAuditRepository,
        category_repo::{CategoryRepository, NewCategory, SqlxCategoryRepository},
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        subscription_repo::SqlxSubscriptionRepository,
        transaction_repo::SqlxTransactionRepository,
        user_repo::{NewUser, SqlxUserRepository, UserRepository},
    },
    service::{
        auth_service::AuthService,
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{PaymentConfig, PaymentService},
    },
};
use chrono::Utc;
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

#[allow(dead_code)]
struct TestHarness {
    app: Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    jwt_engine: Arc<JwtEngine>,
    premium_user_id: String,
    premium_token: String,
    free_user_id: String,
    free_token: String,
    user_b_id: String,
    user_b_token: String,
    bca_account_id: String,
    mandiri_account_id: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m6_ingestion_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 5,
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

    let jwt_secret = "m6_ingestion_jwt_secret_testing_key_123456";
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
        transaction_repo.clone(),
        idempotency_repo.clone(),
    ));

    let payment_service = Arc::new(PaymentService::new(
        PaymentConfig::default(),
        subscription_repo,
        user_repo.clone(),
        audit_repo,
    ));

    let auth_state = AuthState {
        auth_service,
        secure_cookie: false,
    };

    let state = AppState {
        auth_state,
        account_repo: account_repo.clone(),
        category_repo: category_repo.clone(),
        user_preferences_repo: Arc::new(backend::repository::SqlxUserPreferencesRepository::new(pool.clone())),
        ledger_service,
        payment_service,
        pool: pool.clone(),
        rate_limiter: Arc::default(),
    };

    let app = create_app(state);

    // 1. Create Premium User A
    let premium_user_id = uuid::Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: premium_user_id.clone(),
            email: "premium_ingest@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Premium Tester".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .expect("create premium user");

    let (premium_token, _) = jwt_engine
        .generate_token(&premium_user_id, "premium_ingest@test.com", "user", "premium")
        .expect("jwt token");

    // 2. Create Free User
    let free_user_id = uuid::Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: free_user_id.clone(),
            email: "free_user@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Free Tester".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .expect("create free user");

    let (free_token, _) = jwt_engine
        .generate_token(&free_user_id, "free_user@test.com", "user", "free")
        .expect("jwt token");

    // 3. Create User B (for tenant isolation)
    let user_b_id = uuid::Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "user_b@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "User B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .expect("create user b");

    let (user_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "user_b@test.com", "user", "premium")
        .expect("jwt token");

    // Seed User A Accounts
    let bca_account_id = uuid::Uuid::new_v4().to_string();
    account_repo
        .create(&NewAccount {
            id: bca_account_id.clone(),
            user_id: premium_user_id.clone(),
            name: "BCA Tabungan".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(1_000_000),
            color: None,
            icon: None,
        })
        .await
        .expect("create bca account");

    let mandiri_account_id = uuid::Uuid::new_v4().to_string();
    account_repo
        .create(&NewAccount {
            id: mandiri_account_id.clone(),
            user_id: premium_user_id.clone(),
            name: "Mandiri Utama".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(500_000),
            color: None,
            icon: None,
        })
        .await
        .expect("create mandiri account");

    // Seed User A Category
    let cat_id = uuid::Uuid::new_v4().to_string();
    category_repo
        .create(&NewCategory {
            id: cat_id,
            user_id: Some(premium_user_id.clone()),
            name: "Makanan & Minuman".to_string(),
            category_type: "expense".to_string(),
            icon: None,
            color: None,
            is_system: false,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .expect("create category");

    TestHarness {
        app,
        pool,
        _dir: dir,
        jwt_engine,
        premium_user_id,
        premium_token,
        free_user_id,
        free_token,
        user_b_id,
        user_b_token,
        bca_account_id,
        mandiri_account_id,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_9_stage_pipeline_android_notification_bca_auto_created() {
    let harness = setup_harness().await;

    // 9-Stage Ingestion Pipeline: BCA Notification with resolved account -> HIGH confidence -> auto-create
    let payload = serde_json::json!({
        "package_name": "com.bca",
        "title": "m-Transfer Berhasil",
        "text": "Transfer Rp 150.000 ke KOPI KENANGAN BERHASIL. Ref: BCA-998811",
        "sub_text": null,
        "posted_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", uuid::Uuid::new_v4().to_string())
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Verify 9 stages:
    // Confidence = HIGH, status = auto_created, transaction_id present
    assert_eq!(json["status"], "auto_created");
    assert_eq!(json["confidence"], "HIGH");
    let tx_id = json["transaction_id"].as_str().expect("transaction_id present");

    // Stage 9: Verify atomic ledger insert & account balance mutation
    let tx_row: (i64, String, String, String) = sqlx::query_as(
        "SELECT amount, transaction_type, source, confidence FROM transactions WHERE id = ?1 AND user_id = ?2",
    )
    .bind(tx_id)
    .bind(&harness.premium_user_id)
    .fetch_one(&harness.pool)
    .await
    .expect("fetch transaction");

    assert_eq!(tx_row.0, 150_000); // 150k integer Rupiah
    assert_eq!(tx_row.1, "expense");
    assert_eq!(tx_row.2, "notification");
    assert_eq!(tx_row.3, "HIGH");

    // Balance mutation check: Initial 1,000,000 - 150,000 = 850,000
    let balance_row: (i64,) = sqlx::query_as(
        "SELECT current_balance FROM accounts WHERE id = ?1",
    )
    .bind(&harness.bca_account_id)
    .fetch_one(&harness.pool)
    .await
    .expect("fetch account balance");

    assert_eq!(balance_row.0, 850_000);

    // Verify ingestion_events record
    let event_id = json["event_id"].as_str().unwrap();
    let event_row: (String, String) = sqlx::query_as(
        "SELECT status, confidence FROM ingestion_events WHERE id = ?1",
    )
    .bind(event_id)
    .fetch_one(&harness.pool)
    .await
    .expect("fetch event");

    assert_eq!(event_row.0, "auto_created");
    assert_eq!(event_row.1, "HIGH");
}

#[tokio::test]
async fn test_all_8_indonesian_providers_parsers() {
    // 1. BCA
    let bca = parse_bca_notification(
        "m-Transfer Berhasil",
        "Pembayaran QRIS Rp 25.000 ke KOPI KENANGAN BERHASIL. Ref: BCA-12345",
    );
    assert_eq!(bca.provider, "bca");
    assert_eq!(bca.amount, Some(Rupiah::new(25000)));
    assert_eq!(bca.direction, Some(TransactionDirection::Expense));
    assert_eq!(bca.external_reference, Some("BCA-12345".to_string()));

    // 2. Mandiri (Livin' by Mandiri)
    let mandiri = parse_mandiri_notification(
        "Livin' by Mandiri",
        "Dana Masuk: Rekening Anda menerima Rp 500.000 dari PT SOLUSI. No. Ref: MNDR-7788",
    );
    assert_eq!(mandiri.provider, "mandiri");
    assert_eq!(mandiri.amount, Some(Rupiah::new(500000)));
    assert_eq!(mandiri.direction, Some(TransactionDirection::Income));
    assert_eq!(mandiri.external_reference, Some("MNDR-7788".to_string()));

    // 3. BRI (BRImo)
    let bri = parse_bri_notification(
        "BRImo",
        "Transaksi Berhasil: Pembayaran QRIS Rp 35.000 di Indomaret Ref: BRI-9900",
    );
    assert_eq!(bri.provider, "bri");
    assert_eq!(bri.amount, Some(Rupiah::new(35000)));
    assert_eq!(bri.direction, Some(TransactionDirection::Expense));

    // 4. BNI (wondr by BNI)
    let bni = parse_bni_notification(
        "wondr by BNI",
        "Transaksi Sukses: Transfer Rp 120.000 ke BCA 12345678 Ref: BNI-4455",
    );
    assert_eq!(bni.provider, "bni");
    assert_eq!(bni.amount, Some(Rupiah::new(120000)));
    assert_eq!(bni.direction, Some(TransactionDirection::Expense));

    // 5. DANA
    let dana = parse_dana_notification(
        "DANA",
        "Pembayaran Berhasil! Kamu telah membayar Rp 15.000 di Alfamart Ref: DANA-001",
    );
    assert_eq!(dana.provider, "dana");
    assert_eq!(dana.amount, Some(Rupiah::new(15000)));
    assert_eq!(dana.direction, Some(TransactionDirection::Expense));

    // 6. GoPay
    let gopay = parse_gopay_notification(
        "GoPay",
        "Pembayaran berhasil! Kamu telah bayar Rp 45.000 ke GoFood Ref: GP-552",
    );
    assert_eq!(gopay.provider, "gopay");
    assert_eq!(gopay.amount, Some(Rupiah::new(45000)));
    assert_eq!(gopay.direction, Some(TransactionDirection::Expense));

    // 7. OVO
    let ovo = parse_ovo_notification(
        "OVO",
        "Pembayaran Berhasil Rp 30.000 di GrabFood Ref: OVO-919",
    );
    assert_eq!(ovo.provider, "ovo");
    assert_eq!(ovo.amount, Some(Rupiah::new(30000)));
    assert_eq!(ovo.direction, Some(TransactionDirection::Expense));

    // 8. ShopeePay
    let shopee = parse_shopeepay_notification(
        "ShopeePay",
        "Pembayaran Berhasil! Kamu telah membayar Rp 65.000 di Shopee Ref: SP-332",
    );
    assert_eq!(shopee.provider, "shopeepay");
    assert_eq!(shopee.amount, Some(Rupiah::new(65000)));
    assert_eq!(shopee.direction, Some(TransactionDirection::Expense));
}

#[tokio::test]
async fn test_sms_capability_ingestion() {
    let harness = setup_harness().await;

    let payload = serde_json::json!({
        "sender": "MANDIRI",
        "body": "TRSF E-BANKING DB 250.000,00 17/09 REK 1234567890 REF: MNDR-9921",
        "received_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/sms")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "auto_created");
    assert_eq!(json["confidence"], "HIGH");
    let _tx_id = json["transaction_id"].as_str().expect("tx_id present");

    // Check balance of Mandiri account (500_000 - 250_000 = 250_000)
    let balance_row: (i64,) = sqlx::query_as(
        "SELECT current_balance FROM accounts WHERE id = ?1",
    )
    .bind(&harness.mandiri_account_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(balance_row.0, 250_000);
}

#[tokio::test]
async fn test_gmail_oauth_ingestion() {
    let harness = setup_harness().await;

    let payload = serde_json::json!({
        "message_id": "msg_gmail_99182",
        "snippet": "Bukti Pembayaran Google Play Rp 49.000",
        "body": "Pembayaran Anda sebesar Rp 49.000 telah berhasil. Ref: GPA-1234-5678",
        "internal_date": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/gmail")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Gmail with multiple accounts -> MEDIUM confidence requiring confirmation or auto created if default
    assert!(json["status"] == "requires_confirmation" || json["status"] == "auto_created");
}

#[tokio::test]
async fn test_confidence_engine_medium_requires_confirmation() {
    let harness = setup_harness().await;

    // DANA notification, but user A has only BCA and Mandiri accounts (no DANA wallet)
    let payload = serde_json::json!({
        "package_name": "id.dana",
        "title": "DANA",
        "text": "Pembayaran Berhasil! Kamu telah membayar Rp 85.000 di Alfamart Ref: DANA-7722",
        "sub_text": null,
        "posted_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "requires_confirmation");
    assert_eq!(json["confidence"], "MEDIUM");
    assert!(json["transaction_id"].is_null());

    // Zero balance changes
    let balance_bca: (i64,) = sqlx::query_as("SELECT current_balance FROM accounts WHERE id = ?1")
        .bind(&harness.bca_account_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(balance_bca.0, 1_000_000);
}

#[tokio::test]
async fn test_confidence_engine_low_rejected() {
    let harness = setup_harness().await;

    // Non-financial promo spam notification
    let payload = serde_json::json!({
        "package_name": "id.dana",
        "title": "DANA Promo",
        "text": "Dapatkan diskon heboh hingga 70% di Alfamart hari ini saja! Buka aplikasi sekarang!",
        "sub_text": null,
        "posted_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "rejected");
    assert_eq!(json["confidence"], "LOW");
    assert!(json["transaction_id"].is_null());
}

#[tokio::test]
async fn test_cross_source_signal_deduplication() {
    let harness = setup_harness().await;

    let now_millis = Utc::now().timestamp_millis();

    // Signal 1: Android Notification arrives at T+0s
    let notif_payload = serde_json::json!({
        "package_name": "com.bca",
        "title": "m-Transfer Berhasil",
        "text": "Pembayaran QRIS Rp 50.000 ke KOPI KENANGAN BERHASIL. Ref: QRIS-9988",
        "sub_text": null,
        "posted_at": now_millis
    });

    let req1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&notif_payload).unwrap()))
        .unwrap();

    let resp1 = harness.app.clone().oneshot(req1).await.unwrap();
    let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
    let json1: Value = serde_json::from_slice(&body1).unwrap();
    assert_eq!(json1["status"], "auto_created");
    let _tx1_id = json1["transaction_id"].as_str().unwrap();

    // Signal 2: Bank SMS arrives 15s later (T+15s) with identical amount, provider, direction, within ±300s
    let sms_payload = serde_json::json!({
        "sender": "BCA",
        "body": "TRSF E-BANKING DB 50.000,00 KOPI KENANGAN REF: QRIS-9988",
        "received_at": now_millis + 15_000
    });

    let req2 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/sms")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&sms_payload).unwrap()))
        .unwrap();

    let resp2 = harness.app.clone().oneshot(req2).await.unwrap();
    let body2 = resp2.into_body().collect().await.unwrap().to_bytes();
    let json2: Value = serde_json::from_slice(&body2).unwrap();

    // MUST be detected as duplicate!
    assert_eq!(json2["status"], "duplicate");

    // Signal 3: Gmail receipt arrives 45s later (T+45s)
    let gmail_payload = serde_json::json!({
        "message_id": "receipt_kopi_99",
        "snippet": "Bukti Pembayaran BCA Rp 50.000",
        "body": "Pembayaran Rp 50.000 ke KOPI KENANGAN berhasil. Ref: QRIS-9988",
        "internal_date": now_millis + 45_000
    });

    let req3 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/gmail")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&gmail_payload).unwrap()))
        .unwrap();

    let resp3 = harness.app.clone().oneshot(req3).await.unwrap();
    let body3 = resp3.into_body().collect().await.unwrap().to_bytes();
    let json3: Value = serde_json::from_slice(&body3).unwrap();

    // MUST be detected as duplicate!
    assert_eq!(json3["status"], "duplicate");

    // CRITICAL LEDGER VERIFICATION:
    // Exactly ONE transaction exists for Rp 50.000!
    let count_row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM transactions WHERE user_id = ?1 AND amount = 50000",
    )
    .bind(&harness.premium_user_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(count_row.0, 1, "Exactly one ledger transaction must be created across multiple sources");

    // Balance deducted exactly once: 1,000,000 - 50,000 = 950,000
    let balance_row: (i64,) = sqlx::query_as(
        "SELECT current_balance FROM accounts WHERE id = ?1",
    )
    .bind(&harness.bca_account_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(balance_row.0, 950_000);
}

#[tokio::test]
async fn test_candidate_review_api_confirm_atomically_commits_to_ledger() {
    let harness = setup_harness().await;

    // 1. Ingest notification for DANA (ambiguous account -> requires_confirmation)
    let payload = serde_json::json!({
        "package_name": "id.dana",
        "title": "DANA",
        "text": "Pembayaran Berhasil! Kamu telah membayar Rp 60.000 di Hokben Ref: DANA-4411",
        "posted_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "requires_confirmation");
    let candidate_id = json["event_id"].as_str().unwrap();

    // 2. GET /api/v1/ingestion/candidates -> 1 candidate present
    let list_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/ingestion/candidates")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .body(Body::empty())
        .unwrap();

    let list_resp = harness.app.clone().oneshot(list_req).await.unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body = list_resp.into_body().collect().await.unwrap().to_bytes();
    let candidates: Value = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(candidates.as_array().unwrap().len(), 1);

    // 3. User confirms candidate, selecting BCA account
    let confirm_payload = serde_json::json!({
        "account_id": harness.bca_account_id,
        "category_id": null
    });

    let confirm_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/ingestion/candidates/{}/confirm", candidate_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&confirm_payload).unwrap()))
        .unwrap();

    let confirm_resp = harness.app.clone().oneshot(confirm_req).await.unwrap();
    assert_eq!(confirm_resp.status(), StatusCode::OK);
    let confirm_body = confirm_resp.into_body().collect().await.unwrap().to_bytes();
    let confirm_json: Value = serde_json::from_slice(&confirm_body).unwrap();
    assert_eq!(confirm_json["status"], "confirmed");

    // 4. Ledger verification:
    // Initial 1,000,000 - 60,000 = 940,000
    let balance_row: (i64,) = sqlx::query_as("SELECT current_balance FROM accounts WHERE id = ?1")
        .bind(&harness.bca_account_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(balance_row.0, 940_000);

    // 5. GET /api/v1/ingestion/candidates -> now 0 pending
    let list_req2 = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/ingestion/candidates")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .body(Body::empty())
        .unwrap();

    let list_resp2 = harness.app.clone().oneshot(list_req2).await.unwrap();
    let list_body2 = list_resp2.into_body().collect().await.unwrap().to_bytes();
    let candidates2: Value = serde_json::from_slice(&list_body2).unwrap();
    assert_eq!(candidates2.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_candidate_review_api_reject_flow() {
    let harness = setup_harness().await;

    // 1. Ingest notification resulting in requires_confirmation
    let payload = serde_json::json!({
        "package_name": "id.dana",
        "title": "DANA",
        "text": "Pembayaran Berhasil! Kamu telah membayar Rp 30.000 di Warung Kopi Ref: DANA-9900",
        "posted_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let candidate_id = json["event_id"].as_str().unwrap();

    // 2. Reject candidate
    let reject_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/ingestion/candidates/{}/reject", candidate_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .body(Body::empty())
        .unwrap();

    let reject_resp = harness.app.clone().oneshot(reject_req).await.unwrap();
    assert_eq!(reject_resp.status(), StatusCode::OK);
    let reject_body = reject_resp.into_body().collect().await.unwrap().to_bytes();
    let reject_json: Value = serde_json::from_slice(&reject_body).unwrap();
    assert_eq!(reject_json["status"], "rejected");

    // Verify DB status
    let event: (String,) = sqlx::query_as("SELECT status FROM ingestion_events WHERE id = ?1")
        .bind(candidate_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(event.0, "rejected");

    // Zero balance changes
    let balance_bca: (i64,) = sqlx::query_as("SELECT current_balance FROM accounts WHERE id = ?1")
        .bind(&harness.bca_account_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(balance_bca.0, 1_000_000);
}

#[tokio::test]
async fn test_privacy_and_payload_minimization_req_ingest_08() {
    let harness = setup_harness().await;

    // Sensitive payload containing PII / private text
    let secret_pin = "PIN_RAHASIA_99887766";
    let payload = serde_json::json!({
        "package_name": "com.bca",
        "title": "m-Transfer Berhasil",
        "text": format!("Transfer Rp 100.000 ke 1234567890 BERHASIL. Catatan: {}", secret_pin),
        "posted_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let event_id = json["event_id"].as_str().unwrap();

    // Query raw database row in SQLite ingestion_events
    let event_row: (String, String, Option<String>) = sqlx::query_as(
        "SELECT source, raw_payload_hash, parsed_candidate FROM ingestion_events WHERE id = ?1",
    )
    .bind(event_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    let raw_hash = event_row.1;
    let parsed_candidate_json = event_row.2.unwrap_or_default();

    // 1. Assert raw_payload_hash is exactly 64 hex characters (SHA-256)
    assert_eq!(raw_hash.len(), 64);
    assert!(raw_hash.chars().all(|c| c.is_ascii_hexdigit()));

    // 2. REQ-INGEST-08: Assert secret PIN is NOT stored anywhere in the DB record!
    assert!(!raw_hash.contains(secret_pin));
    assert!(!parsed_candidate_json.contains(secret_pin));
}

#[tokio::test]
async fn test_idempotency_key_deduplication() {
    let harness = setup_harness().await;

    let idempotency_key = uuid::Uuid::new_v4().to_string();
    let payload = serde_json::json!({
        "package_name": "com.bca",
        "title": "m-Transfer Berhasil",
        "text": "Transfer Rp 70.000 ke TOKO ABC BERHASIL. Ref: BCA-IDEM-01",
        "posted_at": 1773715200000_i64
    });

    // First request
    let req1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", &idempotency_key)
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp1 = harness.app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
    let json1: Value = serde_json::from_slice(&body1).unwrap();
    let _tx_id = json1["transaction_id"].as_str().unwrap();

    // Replay identical request with identical Idempotency-Key
    let req2 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", &idempotency_key)
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp2 = harness.app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body2 = resp2.into_body().collect().await.unwrap().to_bytes();
    let json2: Value = serde_json::from_slice(&body2).unwrap();

    // Must return identical cached response
    assert_eq!(json1, json2);

    // Exactly one transaction in database
    let tx_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM transactions WHERE user_id = ?1 AND amount = 70000",
    )
    .bind(&harness.premium_user_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(tx_count.0, 1);
}

#[tokio::test]
async fn test_multi_tenant_isolation_candidate_review() {
    let harness = setup_harness().await;

    // User A has a candidate
    let payload = serde_json::json!({
        "package_name": "id.dana",
        "title": "DANA",
        "text": "Pembayaran Berhasil! Rp 20.000 di Alfamart Ref: DANA-TENANT-1",
        "posted_at": 1773715200000_i64
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = harness.app.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let user_a_candidate_id = json["event_id"].as_str().unwrap();

    // User B attempts to confirm User A's candidate
    let confirm_payload = serde_json::json!({
        "account_id": harness.bca_account_id,
        "category_id": null
    });

    let bad_confirm_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/ingestion/candidates/{}/confirm", user_a_candidate_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.user_b_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&confirm_payload).unwrap()))
        .unwrap();

    let bad_resp = harness.app.clone().oneshot(bad_confirm_req).await.unwrap();
    assert_eq!(bad_resp.status(), StatusCode::NOT_FOUND);

    // User B lists candidates -> User A's candidate is NOT visible
    let list_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/ingestion/candidates")
        .header(AUTHORIZATION, format!("Bearer {}", harness.user_b_token))
        .body(Body::empty())
        .unwrap();

    let list_resp = harness.app.clone().oneshot(list_req).await.unwrap();
    let list_body = list_resp.into_body().collect().await.unwrap().to_bytes();
    let candidates: Value = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(candidates.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_feature_gating_free_vs_premium() {
    let harness = setup_harness().await;

    let payload = serde_json::json!({
        "package_name": "com.bca",
        "title": "m-Transfer Berhasil",
        "text": "Transfer Rp 50.000 ke KOPI KENANGAN BERHASIL",
        "posted_at": 1773715200000_i64
    });

    // 1. Free user request -> 403 Forbidden with FEATURE_LOCKED
    let free_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.free_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let free_resp = harness.app.clone().oneshot(free_req).await.unwrap();
    assert_eq!(free_resp.status(), StatusCode::FORBIDDEN);
    let free_body = free_resp.into_body().collect().await.unwrap().to_bytes();
    let free_json: Value = serde_json::from_slice(&free_body).unwrap();
    assert_eq!(free_json["code"], "FEATURE_LOCKED");

    // 2. Premium user request -> 200 OK
    let prem_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/ingestion/notification")
        .header(AUTHORIZATION, format!("Bearer {}", harness.premium_token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let prem_resp = harness.app.clone().oneshot(prem_req).await.unwrap();
    assert_eq!(prem_resp.status(), StatusCode::OK);
}
