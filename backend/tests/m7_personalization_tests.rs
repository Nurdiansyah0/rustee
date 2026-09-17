use axum::{
    body::Body,
    http::{header::{CONTENT_TYPE, COOKIE}, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

use backend::{
    api::{create_app, AppState, AuthState},
    repository::{
        db::{init_pool, run_migrations, DbConfig},
        user_repo::{NewUser, SqlxUserRepository, UserRepository},
        account_repo::SqlxAccountRepository,
        category_repo::SqlxCategoryRepository,
        audit_repo::SqlxAuditRepository,
        idempotency_repo::SqlxIdempotencyRepository,
        transaction_repo::SqlxTransactionRepository,
        subscription_repo::SqlxSubscriptionRepository,
        user_preferences_repo::SqlxUserPreferencesRepository,
    },
    service::{
        auth_service::AuthService,
        crypto::{Argon2Config, CryptoService},
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{PaymentConfig, PaymentService},
    },
};

struct TestContext {
    app: axum::Router,
    jwt_engine: Arc<JwtEngine>,
    user_repo: Arc<SqlxUserRepository>,
    _dir: tempfile::TempDir,
}

async fn setup_app() -> TestContext {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_personalization.db");
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
    let user_preferences_repo = Arc::new(SqlxUserPreferencesRepository::new(pool.clone()));
    let audit_repo = Arc::new(SqlxAuditRepository::new(pool.clone()));
    let idempotency_repo = Arc::new(SqlxIdempotencyRepository::new(pool.clone()));
    let transaction_repo = Arc::new(SqlxTransactionRepository::new(pool.clone()));
    let subscription_repo = Arc::new(SqlxSubscriptionRepository::new(pool.clone()));

    let jwt_secret = "m7_personalization_secure_test_jwt_secret_123456";
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

    let payment_service = Arc::new(PaymentService::new_with_pool(
        PaymentConfig::default(),
        subscription_repo,
        user_repo.clone(),
        audit_repo,
        pool.clone(),
    ));

    let auth_state = AuthState {
        auth_service,
        secure_cookie: false,
    };

    let state = AppState {
        auth_state,
        account_repo,
        category_repo,
        user_preferences_repo,
        ledger_service,
        payment_service,
        pool,
        rate_limiter: Arc::default(),
    };

    let app = create_app(state);

    TestContext {
        app,
        jwt_engine,
        user_repo,
        _dir: dir,
    }
}

async fn create_test_user(ctx: &TestContext, email: &str) -> (String, String) {
    let user_id = uuid::Uuid::new_v4().to_string();
    ctx.user_repo
        .create(&NewUser {
            id: user_id.clone(),
            email: email.to_string(),
            password_hash: "dummy_hash".to_string(),
            display_name: "Test User".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (token, _) = ctx
        .jwt_engine
        .generate_token(&user_id, email, "user", "free")
        .unwrap();

    (user_id, token)
}

#[tokio::test]
async fn test_personalization_get_and_put() {
    let ctx = setup_app().await;
    let (_user_id, token) = create_test_user(&ctx, "perso_test@example.com").await;

    // 1. Initial GET /api/v1/users/personalization
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/users/personalization")
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["onboarding_completed"], false);

    // 2. PUT /api/v1/users/personalization
    let update_body = serde_json::json!({
        "display_name": "Sobat Hebat",
        "income_title": "Gaji Pokok",
        "expense_title": "Pengeluaran Rumah",
        "financial_goals": ["emergency", "debt_free"],
        "onboarding_completed": true
    });

    let req_put = Request::builder()
        .method("PUT")
        .uri("/api/v1/users/personalization")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&update_body).unwrap()))
        .unwrap();

    let resp_put = ctx.app.clone().oneshot(req_put).await.unwrap();
    assert_eq!(resp_put.status(), StatusCode::OK);
    let body_put: Value = serde_json::from_slice(&resp_put.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body_put["display_name"], "Sobat Hebat");
    assert_eq!(body_put["income_title"], "Gaji Pokok");
    assert_eq!(body_put["expense_title"], "Pengeluaran Rumah");
    assert_eq!(body_put["financial_goals"][0], "emergency");
    assert_eq!(body_put["onboarding_completed"], true);

    // 3. Subsequent GET /api/v1/users/personalization persists
    let req_get2 = Request::builder()
        .method("GET")
        .uri("/api/v1/users/personalization")
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();

    let resp_get2 = ctx.app.clone().oneshot(req_get2).await.unwrap();
    assert_eq!(resp_get2.status(), StatusCode::OK);
    let body_get2: Value = serde_json::from_slice(&resp_get2.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body_get2["display_name"], "Sobat Hebat");
    assert_eq!(body_get2["income_title"], "Gaji Pokok");
    assert_eq!(body_get2["onboarding_completed"], true);
}

#[tokio::test]
async fn test_onboarding_atomic_flow() {
    let ctx = setup_app().await;
    let (_user_id, token) = create_test_user(&ctx, "onboard_user@example.com").await;

    let payload = serde_json::json!({
        "display_name": "Rian Nurdiansyah",
        "income_title": "Honor Konsultasi",
        "expense_title": "Operasional Harian",
        "financial_goals": ["invest", "emergency"],
        "wallets": [
            { "name": "BCA Payroll", "account_type": "checking", "initial_balance": 2500000 },
            { "name": "DANA Dompet", "account_type": "e_wallet", "initial_balance": 350000 }
        ],
        "categories": [
            { "name": "Makan Restoran", "category_type": "expense" },
            { "name": "Bonus Klien", "category_type": "income" }
        ],
        "activate_trial": true
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/users/onboarding")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "success");
    assert_eq!(body["personalization"]["display_name"], "Rian Nurdiansyah");
    assert_eq!(body["personalization"]["onboarding_completed"], true);

    // Verify wallets created
    let req_acc = Request::builder()
        .method("GET")
        .uri("/api/v1/accounts")
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();
    let resp_acc = ctx.app.clone().oneshot(req_acc).await.unwrap();
    assert_eq!(resp_acc.status(), StatusCode::OK);
    let accounts: Value = serde_json::from_slice(&resp_acc.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(accounts.as_array().unwrap().len() >= 2);

    // Verify categories created
    let req_cat = Request::builder()
        .method("GET")
        .uri("/api/v1/categories")
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();
    let resp_cat = ctx.app.clone().oneshot(req_cat).await.unwrap();
    assert_eq!(resp_cat.status(), StatusCode::OK);
    let cats: Value = serde_json::from_slice(&resp_cat.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let cat_names: Vec<&str> = cats.as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(cat_names.contains(&"Makan Restoran"));
    assert!(cat_names.contains(&"Bonus Klien"));

    // Verify trial activated
    let req_sub = Request::builder()
        .method("GET")
        .uri("/api/v1/subscription")
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();
    let resp_sub = ctx.app.clone().oneshot(req_sub).await.unwrap();
    assert_eq!(resp_sub.status(), StatusCode::OK);
    let sub: Value = serde_json::from_slice(&resp_sub.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(sub["tier"], "premium");
    assert_eq!(sub["status"], "trialing");
}

#[tokio::test]
async fn test_checkout_dana_exclusive_enforcement() {
    let ctx = setup_app().await;
    let (_user_id, token) = create_test_user(&ctx, "checkout_test@example.com").await;

    // Midtrans rejected with 400
    let req_mid = Request::builder()
        .method("POST")
        .uri("/api/v1/subscriptions/checkout")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"provider": "midtrans"}"#))
        .unwrap();
    let resp_mid = ctx.app.clone().oneshot(req_mid).await.unwrap();
    assert_eq!(resp_mid.status(), StatusCode::BAD_REQUEST);

    // Xendit rejected with 400
    let req_xen = Request::builder()
        .method("POST")
        .uri("/api/v1/subscriptions/checkout")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"provider": "xendit"}"#))
        .unwrap();
    let resp_xen = ctx.app.clone().oneshot(req_xen).await.unwrap();
    assert_eq!(resp_xen.status(), StatusCode::BAD_REQUEST);

    // DANA accepted with 200
    let req_dana = Request::builder()
        .method("POST")
        .uri("/api/v1/subscriptions/checkout")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"provider": "dana", "plan_id": "premium_monthly"}"#))
        .unwrap();
    let resp_dana = ctx.app.clone().oneshot(req_dana).await.unwrap();
    assert_eq!(resp_dana.status(), StatusCode::OK);
    let body_dana: Value = serde_json::from_slice(&resp_dana.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body_dana["provider"], "dana");
    assert!(body_dana["checkout_url"].as_str().unwrap().contains("dana"));
}
