//! M5 Integration Tests — Full Axum REST API Layer, Feature Gating, & Health Probes
//!
//! Validates:
//! - Public health & readiness probes (/health, /ready)
//! - RFC 9111 Cache-Control headers on sensitive financial endpoints
//! - Server-side feature gating: Free users receive HTTP 403 on Premium endpoints ("analytics.advanced")
//! - Premium users successfully access gated endpoints
//! - Multi-wallet account REST endpoints
//! - Custom categories REST endpoints & soft deletion
//! - Transaction REST endpoints with Idempotency-Key header deduplication
//! - Unified dashboard endpoint returning authoritative net cash flow and balance aggregations

#[cfg(test)]
mod m5_api_tests {
    use axum::{
        body::Body,
        http::{
            header::{AUTHORIZATION, CACHE_CONTROL},
            Method, Request, StatusCode,
        },
        Router,
    };
    use backend::{
        api::{create_app, AppState, AuthState},
        repository::{
            account_repo::SqlxAccountRepository,
            audit_repo::SqlxAuditRepository,
            category_repo::SqlxCategoryRepository,
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
    use http_body_util::BodyExt;
    use serde_json::Value;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tower::ServiceExt;

    async fn setup_app() -> (
        Router,
        String,
        String,
        String,
        String,
        sqlx::SqlitePool,
        tempfile::TempDir,
    ) {
        let dir = tempdir().expect("Failed to create tempdir");
        let db_path = dir.path().join("m5_test.sqlite");
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

        let jwt_secret = "m5_super_secure_jwt_secret_testing_key_123456";
        let jwt_engine = Arc::new(JwtEngine::new(jwt_secret, 900));

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
            account_repo,
            category_repo,
            user_preferences_repo: Arc::new(backend::repository::SqlxUserPreferencesRepository::new(pool.clone())),
            ledger_service,
            payment_service,
            pool: pool.clone(),
            rate_limiter: Arc::default(),
        };

        let app = create_app(state);

        // 1. Create Free User
        let free_user_id = uuid::Uuid::new_v4().to_string();
        user_repo
            .create(&NewUser {
                id: free_user_id.clone(),
                email: "free_tier@test.com".to_string(),
                password_hash: "hash".to_string(),
                display_name: "Free User".to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some("free".to_string()),
            })
            .await
            .unwrap();
        let (free_token, _) = jwt_engine
            .generate_token(&free_user_id, "free_tier@test.com", "user", "free")
            .unwrap();

        // 2. Create Premium User
        let premium_user_id = uuid::Uuid::new_v4().to_string();
        user_repo
            .create(&NewUser {
                id: premium_user_id.clone(),
                email: "premium_tier@test.com".to_string(),
                password_hash: "hash".to_string(),
                display_name: "Premium User".to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some("premium".to_string()),
            })
            .await
            .unwrap();
        let (premium_token, _) = jwt_engine
            .generate_token(&premium_user_id, "premium_tier@test.com", "user", "premium")
            .unwrap();

        (
            app,
            free_user_id,
            free_token,
            premium_user_id,
            premium_token,
            pool,
            dir,
        )
    }

    // -----------------------------------------------------------------------
    // T1: Liveness and Readiness Probes (/health, /ready)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_health_and_readiness_probes() {
        let (app, _, _, _, _, _, _dir) = setup_app().await;

        // /health
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(CACHE_CONTROL).unwrap(),
            "private, no-store, must-revalidate"
        );

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "pass");
        assert_eq!(json["version"], "1.0.0");
        assert_eq!(json["service"], "personal_finance_pwa");

        // /ready
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "pass");
        assert_eq!(json["database"], "connected");
    }

    // -----------------------------------------------------------------------
    // T2: Feature Gating — Free user blocked from Premium endpoint (HTTP 403)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_feature_gating_free_user_forbidden() {
        let (app, _, free_token, _, _, _, _dir) = setup_app().await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/analytics/advanced")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "FEATURE_LOCKED");
        assert_eq!(json["type"], "https://api.nurdiansyahlabs.com/errors/feature-locked");
        assert_eq!(json["detail"], "Subscription feature 'analytics.advanced' required.");
    }

    // -----------------------------------------------------------------------
    // T2b: Feature Gating — Trialing user allowed to access Premium endpoint (HTTP 200)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_feature_gating_trialing_user_allowed() {
        let (app, _, _, _, _, pool, _dir) = setup_app().await;

        let trial_user_id = uuid::Uuid::new_v4().to_string();
        let user_repo = SqlxUserRepository::new(pool.clone());
        user_repo
            .create(&NewUser {
                id: trial_user_id.clone(),
                email: "trial_tier@test.com".to_string(),
                password_hash: "hash".to_string(),
                display_name: "Trial User".to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some("premium".to_string()),
            })
            .await
            .unwrap();

        let jwt_engine = JwtEngine::new("m5_super_secure_jwt_secret_testing_key_123456", 900);
        let (trial_token, _) = jwt_engine
            .generate_token(&trial_user_id, "trial_tier@test.com", "user", "trialing")
            .unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/analytics/advanced")
                    .header(AUTHORIZATION, format!("Bearer {}", trial_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["tier"], "trialing");
        assert_eq!(json["message"], "Premium analytics unlocked");
    }

    // -----------------------------------------------------------------------
    // T3: Feature Gating — Premium user allowed to access Premium endpoint (HTTP 200)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_feature_gating_premium_user_allowed() {
        let (app, _, _, _, premium_token, _, _dir) = setup_app().await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/analytics/advanced")
                    .header(AUTHORIZATION, format!("Bearer {}", premium_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["tier"], "premium");
        assert_eq!(json["message"], "Premium analytics unlocked");
    }

    // -----------------------------------------------------------------------
    // T4: Basic analytics available to Free users
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_basic_analytics_available_to_free_tier() {
        let (app, _, free_token, _, _, _, _dir) = setup_app().await;

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/analytics/basic")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["tier"], "free");
    }

    // -----------------------------------------------------------------------
    // T5: Account REST lifecycle (create, list, get, archive)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_account_rest_endpoints() {
        let (app, _, free_token, _, _, _, _dir) = setup_app().await;

        // 1. Create account
        let create_payload = serde_json::json!({
            "name": "BCA Savings",
            "account_type": "savings",
            "initial_balance": 500000
        });

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/accounts")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::CREATED);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let created_acc: Value = serde_json::from_slice(&body).unwrap();
        let acc_id = created_acc["id"].as_str().unwrap();

        // 2. List accounts
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/accounts")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let list: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);

        // 3. Get single account
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/accounts/{}", acc_id))
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 4. Archive account
        let res = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/accounts/{}/archive", acc_id))
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    // -----------------------------------------------------------------------
    // T6: Category REST endpoints and soft-delete
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_category_rest_endpoints() {
        let (app, _, free_token, _, _, _, _dir) = setup_app().await;

        // 1. Create custom category
        let create_payload = serde_json::json!({
            "name": "Coffee & Snacks",
            "category_type": "expense",
            "icon": "coffee",
            "color": "#8B4513"
        });

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/categories")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::CREATED);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let cat: Value = serde_json::from_slice(&body).unwrap();
        let cat_id = cat["id"].as_str().unwrap();

        // 2. Soft-delete category
        let res = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/api/v1/categories/{}", cat_id))
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    // -----------------------------------------------------------------------
    // T7: Transactions REST with Idempotency-Key header deduplication
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_transactions_rest_with_idempotency_key() {
        let (app, _, free_token, _, _, _, _dir) = setup_app().await;

        // First create an account
        let acc_payload = serde_json::json!({
            "name": "Wallet",
            "account_type": "cash",
            "initial_balance": 1000000
        });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/accounts")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&acc_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let acc_body = res.into_body().collect().await.unwrap().to_bytes();
        let acc: Value = serde_json::from_slice(&acc_body).unwrap();
        let account_id = acc["id"].as_str().unwrap();

        // Post transaction with Idempotency-Key
        let idem_key = "tx_idempotent_test_key_999";
        let tx_payload = serde_json::json!({
            "account_id": account_id,
            "transaction_type": "income",
            "amount": 250000,
            "date": "2026-09-12T10:00:00Z",
            "description": "Freelance payment"
        });

        // First submission -> 201 Created
        let res1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/transactions")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .header("idempotency-key", idem_key)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&tx_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res1.status(), StatusCode::CREATED);
        let body1 = res1.into_body().collect().await.unwrap().to_bytes();
        let tx1: Value = serde_json::from_slice(&body1).unwrap();
        assert_eq!(tx1["account_balance"], 1250000);

        // Replayed submission with SAME Idempotency-Key -> Returns cached 201
        let res2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/transactions")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .header("idempotency-key", idem_key)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&tx_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::CREATED);
        let body2 = res2.into_body().collect().await.unwrap().to_bytes();
        let tx2: Value = serde_json::from_slice(&body2).unwrap();

        // Exact same transaction ID and balance: no duplicate charge!
        assert_eq!(tx1["transaction"]["id"], tx2["transaction"]["id"]);
        assert_eq!(tx2["account_balance"], 1250000);

        // 3. Verify Dashboard endpoint includes this balance and cash flow
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/dashboard")
                    .header(AUTHORIZATION, format!("Bearer {}", free_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let dash_body = res.into_body().collect().await.unwrap().to_bytes();
        let dash: Value = serde_json::from_slice(&dash_body).unwrap();

        assert_eq!(dash["total_balance"], 1250000);
        assert_eq!(dash["cash_flow"]["total_income"], 250000);
        assert_eq!(dash["cash_flow"]["net_cash_flow"], 250000);
    }
}
