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
    pool: sqlx::SqlitePool,
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
        pool: pool.clone(),
        rate_limiter: Arc::default(),
    };

    let app = create_app(state);

    TestContext {
        app,
        jwt_engine,
        user_repo,
        pool,
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

#[tokio::test]
async fn test_category_custom_vocabulary_and_metadata() {
    let ctx = setup_app().await;
    let (_user_id, token) = create_test_user(&ctx, "vocab_user@example.com").await;

    // 1. Create custom category with display_name, normalized_name, metadata
    let create_payload = serde_json::json!({
        "name": "Kopi Senja",
        "category_type": "expense",
        "display_name": "Ngopi Sore",
        "normalized_name": "ngopi sore",
        "icon": "coffee",
        "color": "#6F4E37",
        "metadata": "{\"icon_variant\":\"coffee\",\"target_budget\":75000}"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/categories")
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let resp = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let cat: Value = serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let cat_id = cat["id"].as_str().unwrap();
    assert_eq!(cat["name"], "Kopi Senja");
    assert_eq!(cat["display_name"], "Ngopi Sore");
    assert_eq!(cat["normalized_name"], "ngopi sore");
    assert_eq!(cat["metadata"], "{\"icon_variant\":\"coffee\",\"target_budget\":75000}");

    // 2. GET /api/v1/categories/{id}
    let req_get = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/categories/{}", cat_id))
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();
    let resp_get = ctx.app.clone().oneshot(req_get).await.unwrap();
    assert_eq!(resp_get.status(), StatusCode::OK);
    let cat_get: Value = serde_json::from_slice(&resp_get.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(cat_get["id"], cat_id);
    assert_eq!(cat_get["display_name"], "Ngopi Sore");
    assert_eq!(cat_get["metadata"], "{\"icon_variant\":\"coffee\",\"target_budget\":75000}");

    // 3. PUT /api/v1/categories/{id}
    let update_payload = serde_json::json!({
        "display_name": "Ngopi Malam",
        "metadata": "{\"icon_variant\":\"coffee_dark\"}",
        "color": "#3B1E08"
    });
    let req_put = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/categories/{}", cat_id))
        .header(COOKIE, format!("auth_token={}", token))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&update_payload).unwrap()))
        .unwrap();
    let resp_put = ctx.app.clone().oneshot(req_put).await.unwrap();
    assert_eq!(resp_put.status(), StatusCode::OK);
    let cat_updated: Value = serde_json::from_slice(&resp_put.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(cat_updated["display_name"], "Ngopi Malam");
    assert_eq!(cat_updated["metadata"], "{\"icon_variant\":\"coffee_dark\"}");
    assert_eq!(cat_updated["color"], "#3B1E08");

    // 4. DELETE /api/v1/categories/{id} (soft-delete)
    let req_del = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/categories/{}", cat_id))
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();
    let resp_del = ctx.app.clone().oneshot(req_del).await.unwrap();
    assert_eq!(resp_del.status(), StatusCode::NO_CONTENT);

    // 5. Subsequent GET returns 404
    let req_get2 = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/categories/{}", cat_id))
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();
    let resp_get2 = ctx.app.clone().oneshot(req_get2).await.unwrap();
    assert_eq!(resp_get2.status(), StatusCode::NOT_FOUND);

    // 6. Direct DB verification: row exists but deleted_at IS NOT NULL
    let deleted_at: Option<String> = sqlx::query_scalar("SELECT deleted_at FROM categories WHERE id = ?1")
        .bind(cat_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert!(deleted_at.is_some(), "Category must be soft-deleted with non-null timestamp");
}

#[tokio::test]
async fn test_starter_categories_display_name_and_normalized_name_populated() {
    let ctx = setup_app().await;

    // Register via auth API
    let reg_payload = serde_json::json!({
        "email": "starter_test@example.com",
        "password": "ValidPassword123!",
        "display_name": "Starter Tester"
    });

    let req_reg = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&reg_payload).unwrap()))
        .unwrap();
    let resp_reg = ctx.app.clone().oneshot(req_reg).await.unwrap();
    assert_eq!(resp_reg.status(), StatusCode::CREATED);
    let reg_body: Value = serde_json::from_slice(&resp_reg.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let token = reg_body["token"].as_str().unwrap();

    // Query categories via API
    let req_cats = Request::builder()
        .method("GET")
        .uri("/api/v1/categories")
        .header(COOKIE, format!("auth_token={}", token))
        .body(Body::empty())
        .unwrap();
    let resp_cats = ctx.app.clone().oneshot(req_cats).await.unwrap();
    assert_eq!(resp_cats.status(), StatusCode::OK);
    let cats: Value = serde_json::from_slice(&resp_cats.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let cat_arr = cats.as_array().unwrap();
    assert!(!cat_arr.is_empty());

    for cat in cat_arr {
        let name = cat["name"].as_str().unwrap();
        let disp = cat["display_name"].as_str();
        let norm = cat["normalized_name"].as_str();
        assert!(disp.is_some(), "Starter category '{}' missing display_name", name);
        assert!(norm.is_some(), "Starter category '{}' missing normalized_name", name);
        assert_eq!(norm.unwrap(), name.to_lowercase());
    }
}

#[tokio::test]
async fn test_multi_tenant_vocabulary_and_personalization_isolation() {
    let ctx = setup_app().await;
    let (user_a_id, token_a) = create_test_user(&ctx, "user_alpha@example.com").await;
    let (user_b_id, token_b) = create_test_user(&ctx, "user_beta@example.com").await;

    // User A Onboarding
    let payload_a = serde_json::json!({
        "display_name": "Alpha User",
        "income_title": "Gaji Utama PT Alpha",
        "expense_title": "Biaya Hidup Alpha",
        "financial_goals": ["emergency_fund"],
        "wallets": [{ "name": "BCA Payroll Alpha", "account_type": "checking", "initial_balance": 1000000 }],
        "categories": [{ "name": "Makan Resto Alpha", "category_type": "expense", "display_name": "Kuliner Alpha" }]
    });
    let req_a = Request::builder()
        .method("POST")
        .uri("/api/v1/users/onboarding")
        .header(COOKIE, format!("auth_token={}", token_a))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload_a).unwrap()))
        .unwrap();
    let resp_a = ctx.app.clone().oneshot(req_a).await.unwrap();
    assert_eq!(resp_a.status(), StatusCode::OK);

    // User B Onboarding
    let payload_b = serde_json::json!({
        "display_name": "Beta User",
        "income_title": "Freelance Side PT Beta",
        "expense_title": "Pengeluaran Santai Beta",
        "financial_goals": ["gadget_fund"],
        "wallets": [{ "name": "Jago Tabungan Beta", "account_type": "savings", "initial_balance": 2000000 }],
        "categories": [{ "name": "Game Beta", "category_type": "expense", "display_name": "Steam Game Beta" }]
    });
    let req_b = Request::builder()
        .method("POST")
        .uri("/api/v1/users/onboarding")
        .header(COOKIE, format!("auth_token={}", token_b))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload_b).unwrap()))
        .unwrap();
    let resp_b = ctx.app.clone().oneshot(req_b).await.unwrap();
    assert_eq!(resp_b.status(), StatusCode::OK);

    // Check User A Personalization
    let req_get_a = Request::builder()
        .method("GET")
        .uri("/api/v1/users/personalization")
        .header(COOKIE, format!("auth_token={}", token_a))
        .body(Body::empty())
        .unwrap();
    let resp_get_a = ctx.app.clone().oneshot(req_get_a).await.unwrap();
    assert_eq!(resp_get_a.status(), StatusCode::OK);
    let body_a: Value = serde_json::from_slice(&resp_get_a.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body_a["user_id"], user_a_id);
    assert_eq!(body_a["display_name"], "Alpha User");
    assert_eq!(body_a["income_title"], "Gaji Utama PT Alpha");
    assert_eq!(body_a["expense_title"], "Biaya Hidup Alpha");

    // Check User B Personalization
    let req_get_b = Request::builder()
        .method("GET")
        .uri("/api/v1/users/personalization")
        .header(COOKIE, format!("auth_token={}", token_b))
        .body(Body::empty())
        .unwrap();
    let resp_get_b = ctx.app.clone().oneshot(req_get_b).await.unwrap();
    assert_eq!(resp_get_b.status(), StatusCode::OK);
    let body_b: Value = serde_json::from_slice(&resp_get_b.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body_b["user_id"], user_b_id);
    assert_eq!(body_b["display_name"], "Beta User");
    assert_eq!(body_b["income_title"], "Freelance Side PT Beta");
    assert_eq!(body_b["expense_title"], "Pengeluaran Santai Beta");

    // Check Category isolation
    let req_cats_a = Request::builder()
        .method("GET")
        .uri("/api/v1/categories")
        .header(COOKIE, format!("auth_token={}", token_a))
        .body(Body::empty())
        .unwrap();
    let resp_cats_a = ctx.app.clone().oneshot(req_cats_a).await.unwrap();
    let cats_a: Value = serde_json::from_slice(&resp_cats_a.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let names_a: Vec<&str> = cats_a.as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names_a.contains(&"Makan Resto Alpha"));
    assert!(!names_a.contains(&"Game Beta"), "User A must not see User B categories");

    let req_cats_b = Request::builder()
        .method("GET")
        .uri("/api/v1/categories")
        .header(COOKIE, format!("auth_token={}", token_b))
        .body(Body::empty())
        .unwrap();
    let resp_cats_b = ctx.app.clone().oneshot(req_cats_b).await.unwrap();
    let cats_b: Value = serde_json::from_slice(&resp_cats_b.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let names_b: Vec<&str> = cats_b.as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names_b.contains(&"Game Beta"));
    assert!(!names_b.contains(&"Makan Resto Alpha"), "User B must not see User A categories");

    // Cross-tenant update rejection
    let user_a_cat = cats_a.as_array().unwrap().iter().find(|c| c["name"] == "Makan Resto Alpha").unwrap();
    let user_a_cat_id = user_a_cat["id"].as_str().unwrap();

    let req_cross_update = Request::builder()
        .method("PUT")
        .uri(format!("/api/v1/categories/{}", user_a_cat_id))
        .header(COOKIE, format!("auth_token={}", token_b))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"display_name":"Hacked Name"}"#))
        .unwrap();
    let resp_cross = ctx.app.clone().oneshot(req_cross_update).await.unwrap();
    assert_eq!(resp_cross.status(), StatusCode::NOT_FOUND, "Cross-user category update must be rejected");

    let req_cross_delete = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/categories/{}", user_a_cat_id))
        .header(COOKIE, format!("auth_token={}", token_b))
        .body(Body::empty())
        .unwrap();
    let resp_cross_del = ctx.app.clone().oneshot(req_cross_delete).await.unwrap();
    assert_eq!(resp_cross_del.status(), StatusCode::NOT_FOUND, "Cross-user category delete must be rejected");
}
