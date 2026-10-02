//! M1 Integration Tests — 7-Day Premium Free Trial Lifecycle & Anti-Abuse Protection
//!
//! Validates:
//! - Trial activation via POST /api/v1/subscription/trial and POST /api/v1/subscriptions/trial
//! - Anti-abuse validation: duplicate activation rejected with HTTP 400 TRIAL_ALREADY_USED
//! - Anti-abuse validation: existing paid active subscriber rejected with HTTP 400 ALREADY_PREMIUM
//! - Token synchronization: Set-Cookie auth_token refreshed with tier: "premium" granting immediate Pro access
//! - Subscription status: accurate days_remaining integer calculation and feature list
//! - Lazy expiration: past-due trial transitions to 'free' and status 'expired' with zero data loss
//! - Concurrent race condition protection: simultaneous activation attempts guarantee exactly 1 winner

#[cfg(test)]
mod m1_trial_lifecycle_tests {
    use axum::{
        body::Body,
        http::{
            header::{CACHE_CONTROL, COOKIE, SET_COOKIE},
            Request, StatusCode,
        },
        Router,
    };
    use backend::{
        api::{create_app, AppState, AuthState},
        domain::money::Rupiah,
        repository::{
            account_repo::{NewAccount, SqlxAccountRepository},
            audit_repo::SqlxAuditRepository,
            category_repo::{NewCategory, SqlxCategoryRepository},
            db::{init_pool, run_migrations, DbConfig},
            idempotency_repo::SqlxIdempotencyRepository,
            subscription_repo::{NewSubscription, SqlxSubscriptionRepository},
            transaction_repo::{NewTransaction, SqlxTransactionRepository, TransactionFilter},
            user_repo::{NewUser, SqlxUserRepository, UserRepository},
            AccountRepository, CategoryRepository, SubscriptionRepository, TransactionRepository,
        },
        service::{
            auth_service::AuthService,
            crypto::{Argon2Config, CryptoService},
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

    struct TestContext {
        app: Router,
        pool: sqlx::SqlitePool,
        jwt_engine: Arc<JwtEngine>,
        _dir: tempfile::TempDir,
    }

    async fn setup_app() -> TestContext {
        let dir = tempdir().expect("Failed to create tempdir");
        let db_path = dir.path().join("m1_trial_test.sqlite");
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

        let jwt_secret = "m1_trial_super_secure_jwt_secret_testing_key_123456";
        let jwt_engine = Arc::new(JwtEngine::new(jwt_secret, 900));

        let crypto_service =
            Arc::new(CryptoService::new(Argon2Config::fast_for_testing()).unwrap());

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
            user_preferences_repo: Arc::new(
                backend::repository::SqlxUserPreferencesRepository::new(pool.clone()),
            ),
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

        TestContext {
            app,
            pool,
            jwt_engine,
            _dir: dir,
        }
    }

    async fn create_test_user(ctx: &TestContext, email: &str, tier: &str) -> (String, String) {
        let user_repo = SqlxUserRepository::new(ctx.pool.clone());
        let user_id = uuid::Uuid::new_v4().to_string();
        user_repo
            .create(&NewUser {
                id: user_id.clone(),
                email: email.to_string(),
                password_hash: "hash_placeholder".to_string(),
                display_name: "Test User".to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some(tier.to_string()),
            })
            .await
            .unwrap();

        let (token, _) = ctx
            .jwt_engine
            .generate_token(&user_id, email, "user", tier)
            .unwrap();

        (user_id, token)
    }

    // -----------------------------------------------------------------------
    // T1: Trial Activation Sets Status 'trialing', Tier 'premium', Days 7
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_trial_activation_sets_trialing_status_and_premium_tier() {
        let ctx = setup_app().await;
        let (user_id, token) = create_test_user(&ctx, "free_activator@test.com", "free").await;

        let req = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let res = ctx.app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Verify Set-Cookie header contains refreshed token and Max-Age=86400
        let set_cookie_header = res
            .headers()
            .get(SET_COOKIE)
            .expect("Expected Set-Cookie header in response")
            .to_str()
            .unwrap();
        assert!(set_cookie_header.contains("auth_token="));
        assert!(set_cookie_header.contains("Max-Age=86400"));
        assert!(set_cookie_header.contains("HttpOnly"));
        assert!(set_cookie_header.contains("SameSite=Lax"));

        // Verify Cache-Control
        assert_eq!(
            res.headers().get(CACHE_CONTROL).unwrap(),
            "private, no-store, must-revalidate"
        );

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "trialing");
        assert_eq!(json["tier"], "premium");
        assert_eq!(json["is_premium"], true);
        assert_eq!(json["days_remaining"], 90);
        assert_eq!(json["remaining_days"], 90);
        assert!(
            json["message"].as_str().unwrap().contains("3-month")
                || json["message"].as_str().unwrap().contains("90-day")
        );
        assert!(json["trial_started_at"].is_string());
        assert!(json["trial_ends_at"].is_string());

        // Verify SQLite database state directly
        let user_row: (i64, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT has_used_trial, subscription_tier, trial_started_at, trial_ends_at FROM users WHERE id = ?1"
        )
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();

        assert_eq!(user_row.0, 1, "has_used_trial latch must be 1");
        assert_eq!(user_row.1, "premium", "user tier must be premium");
        assert!(user_row.2.is_some(), "trial_started_at must be populated");
        assert!(user_row.3.is_some(), "trial_ends_at must be populated");

        // Verify subscriptions table state
        let sub_row: (String, String, String, i64, i64) = sqlx::query_as(
            "SELECT status, provider, plan_id, amount, cancel_at_period_end FROM subscriptions WHERE user_id = ?1"
        )
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();

        assert_eq!(sub_row.0, "trialing");
        assert_eq!(sub_row.1, "trial");
        assert_eq!(sub_row.2, "premium_trial_90d");
        assert_eq!(sub_row.3, 0);
        assert_eq!(sub_row.4, 1);

        // Verify audit log entry
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_logs WHERE user_id = ?1 AND action = 'subscription_trial_started'"
        )
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();

        assert_eq!(
            audit_count, 1,
            "Expected exactly 1 audit log entry for trial start"
        );
    }

    // -----------------------------------------------------------------------
    // T2: Anti-Abuse — Duplicate Activation Rejected with TRIAL_ALREADY_USED
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_trial_anti_abuse_duplicate_activation_rejected() {
        let ctx = setup_app().await;
        let (user_id, token) = create_test_user(&ctx, "abuser@test.com", "free").await;

        // 1. First activation succeeds
        let req1 = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let res1 = ctx.app.clone().oneshot(req1).await.unwrap();
        assert_eq!(res1.status(), StatusCode::OK);

        // 2. Second activation must be rejected with HTTP 400
        let req2 = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let res2 = ctx.app.clone().oneshot(req2).await.unwrap();
        assert_eq!(res2.status(), StatusCode::BAD_REQUEST);

        let body2 = res2.into_body().collect().await.unwrap().to_bytes();
        let json2: Value = serde_json::from_slice(&body2).unwrap();

        assert_eq!(json2["code"], "TRIAL_ALREADY_USED");
        assert_eq!(json2["status"], 400);

        // 3. Third attempt via plural alias /api/v1/subscriptions/trial is also rejected
        let req3 = Request::builder()
            .uri("/api/v1/subscriptions/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let res3 = ctx.app.clone().oneshot(req3).await.unwrap();
        assert_eq!(res3.status(), StatusCode::BAD_REQUEST);

        let body3 = res3.into_body().collect().await.unwrap().to_bytes();
        let json3: Value = serde_json::from_slice(&body3).unwrap();
        assert_eq!(json3["code"], "TRIAL_ALREADY_USED");

        // Audit log still has only 1 start entry
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_logs WHERE user_id = ?1 AND action = 'subscription_trial_started'"
        )
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
        assert_eq!(audit_count, 1);
    }

    // -----------------------------------------------------------------------
    // T3: Anti-Abuse — Existing Paid Active Subscriber Rejected
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_trial_anti_abuse_existing_paid_subscriber_rejected() {
        let ctx = setup_app().await;
        let (user_id, token) = create_test_user(&ctx, "paid_sub@test.com", "premium").await;

        // Seed an active paid subscription in database
        let now = Utc::now();
        let end_period = now + chrono::Duration::days(30);
        let sub_repo = SqlxSubscriptionRepository::new(ctx.pool.clone());
        sub_repo
            .upsert_subscription(&NewSubscription {
                id: format!("sub_midtrans_{}", user_id),
                user_id: user_id.clone(),
                provider: "midtrans".to_string(),
                provider_subscription_id: Some("ORDER-123".to_string()),
                plan_id: "premium_monthly".to_string(),
                status: "active".to_string(),
                amount: Rupiah(5000),
                current_period_start: now.to_rfc3339(),
                current_period_end: end_period.to_rfc3339(),
                cancel_at_period_end: false,
            })
            .await
            .unwrap();

        // Attempt trial activation
        let req = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let res = ctx.app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "ALREADY_PREMIUM");
    }

    // -----------------------------------------------------------------------
    // T4: Token Synchronization — Set-Cookie Grants Immediate Pro Permissions
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_trial_token_synchronization_immediate_pro_access() {
        let ctx = setup_app().await;
        let (_, free_token) = create_test_user(&ctx, "sync_test@test.com", "free").await;

        // 1. Free token cannot access /api/v1/analytics/advanced (HTTP 403)
        let blocked_req = Request::builder()
            .uri("/api/v1/analytics/advanced")
            .header(COOKIE, format!("auth_token={}", free_token))
            .body(Body::empty())
            .unwrap();

        let blocked_res = ctx.app.clone().oneshot(blocked_req).await.unwrap();
        assert_eq!(blocked_res.status(), StatusCode::FORBIDDEN);

        // 2. Activate trial and extract refreshed cookie
        let trial_req = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", free_token))
            .body(Body::empty())
            .unwrap();

        let trial_res = ctx.app.clone().oneshot(trial_req).await.unwrap();
        assert_eq!(trial_res.status(), StatusCode::OK);

        let cookie_raw = trial_res
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();

        // Extract value of auth_token cookie
        let refreshed_token = cookie_raw
            .split(';')
            .next()
            .unwrap()
            .trim()
            .strip_prefix("auth_token=")
            .expect("Must have auth_token prefix");

        // 3. Immediately request /api/v1/analytics/advanced with refreshed cookie -> HTTP 200 OK!
        let allowed_req = Request::builder()
            .uri("/api/v1/analytics/advanced")
            .header(COOKIE, format!("auth_token={}", refreshed_token))
            .body(Body::empty())
            .unwrap();

        let allowed_res = ctx.app.clone().oneshot(allowed_req).await.unwrap();
        assert_eq!(allowed_res.status(), StatusCode::OK);

        let body = allowed_res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["tier"], "premium");
        assert!(json["financial_health_score"].is_number());
    }

    // -----------------------------------------------------------------------
    // T5: Subscription Status Reports 'trialing', is_premium true, days_remaining 7
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_subscription_status_days_remaining_calculation() {
        let ctx = setup_app().await;
        let (_, token) = create_test_user(&ctx, "status_user@test.com", "free").await;

        // Activate trial
        let trial_req = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();
        let trial_res = ctx.app.clone().oneshot(trial_req).await.unwrap();
        assert_eq!(trial_res.status(), StatusCode::OK);

        // Query GET /api/v1/subscription
        let status_req = Request::builder()
            .uri("/api/v1/subscription")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let status_res = ctx.app.clone().oneshot(status_req).await.unwrap();
        assert_eq!(status_res.status(), StatusCode::OK);

        let body = status_res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "trialing");
        assert_eq!(json["tier"], "premium");
        assert_eq!(json["is_premium"], true);
        assert_eq!(json["days_remaining"], 90);
        assert_eq!(json["remaining_days"], 90);
        assert_eq!(json["price_monthly"], 0);

        let features = json["features"].as_array().unwrap();
        assert!(features.iter().any(|f| f == "analytics.advanced"));
        assert!(features.iter().any(|f| f == "budgeting"));
        assert!(features.iter().any(|f| f == "reports.advanced"));

        // Query alias GET /api/v1/subscriptions/status
        let alias_req = Request::builder()
            .uri("/api/v1/subscriptions/status")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let alias_res = ctx.app.clone().oneshot(alias_req).await.unwrap();
        assert_eq!(alias_res.status(), StatusCode::OK);

        let alias_body = alias_res.into_body().collect().await.unwrap().to_bytes();
        let alias_json: Value = serde_json::from_slice(&alias_body).unwrap();
        assert_eq!(alias_json["status"], "trialing");
        assert_eq!(alias_json["days_remaining"], 90);
        assert_eq!(alias_json["remaining_days"], 90);
    }

    // -----------------------------------------------------------------------
    // T6: Lazy Expiration — Transition to Free & Status 'expired', Preserving Data
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_trial_lazy_expiration_falls_back_to_free_preserving_data() {
        let ctx = setup_app().await;
        let (user_id, token) = create_test_user(&ctx, "expiry_user@test.com", "free").await;

        // 1. Activate trial
        let trial_req = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();
        let trial_res = ctx.app.clone().oneshot(trial_req).await.unwrap();
        assert_eq!(trial_res.status(), StatusCode::OK);

        // 2. User creates an account, category, and transaction during their trial
        let acc_repo = SqlxAccountRepository::new(ctx.pool.clone());
        let cat_repo = SqlxCategoryRepository::new(ctx.pool.clone());
        let tx_repo = SqlxTransactionRepository::new(ctx.pool.clone());

        let account = acc_repo
            .create(&NewAccount {
                id: format!("acc_{}", uuid::Uuid::new_v4()),
                user_id: user_id.clone(),
                name: "Bank Jago".to_string(),
                account_type: "savings".to_string(),
                currency: Some("IDR".to_string()),
                initial_balance: Rupiah(1_000_000),
                color: None,
                icon: None,
            })
            .await
            .unwrap();

        let category = cat_repo
            .create(&NewCategory {
                id: format!("cat_{}", uuid::Uuid::new_v4()),
                user_id: Some(user_id.clone()),
                name: "Groceries".to_string(),
                category_type: "expense".to_string(),
                icon: None,
                color: None,
                is_system: false,
                display_name: None,
                normalized_name: None,
                metadata: None,
            })
            .await
            .unwrap();

        let mut tx = ctx.pool.begin().await.unwrap();
        let _transaction = tx_repo
            .create_in_tx(
                &mut tx,
                &NewTransaction {
                    id: format!("tx_{}", uuid::Uuid::new_v4()),
                    user_id: user_id.clone(),
                    account_id: account.id.clone(),
                    to_account_id: None,
                    category_id: Some(category.id.clone()),
                    transaction_type: "expense".to_string(),
                    amount: Rupiah(50_000),
                    date: Utc::now().to_rfc3339(),
                    description: "Supermarket".to_string(),
                    notes: None,
                    is_recurring: false,
                },
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // 3. Simulate trial expiration: set trial_ends_at and current_period_end to past
        let past_dt = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        sqlx::query("UPDATE users SET trial_ends_at = ?1 WHERE id = ?2")
            .bind(&past_dt)
            .bind(&user_id)
            .execute(&ctx.pool)
            .await
            .unwrap();

        sqlx::query("UPDATE subscriptions SET current_period_end = ?1 WHERE user_id = ?2")
            .bind(&past_dt)
            .bind(&user_id)
            .execute(&ctx.pool)
            .await
            .unwrap();

        // 4. User queries subscription status -> triggers lazy expiration
        let status_req = Request::builder()
            .uri("/api/v1/subscription")
            .header(COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();

        let status_res = ctx.app.clone().oneshot(status_req).await.unwrap();
        assert_eq!(status_res.status(), StatusCode::OK);

        let body = status_res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["status"], "expired");
        assert_eq!(json["tier"], "free");
        assert_eq!(json["is_premium"], false);
        assert_eq!(json["days_remaining"], 0);

        // 5. Verify database user tier is now 'free'
        let updated_tier: String =
            sqlx::query_scalar("SELECT subscription_tier FROM users WHERE id = ?1")
                .bind(&user_id)
                .fetch_one(&ctx.pool)
                .await
                .unwrap();
        assert_eq!(updated_tier, "free");

        // 6. Verify subscriptions table status is now 'expired'
        let updated_sub_status: String =
            sqlx::query_scalar("SELECT status FROM subscriptions WHERE user_id = ?1")
                .bind(&user_id)
                .fetch_one(&ctx.pool)
                .await
                .unwrap();
        assert_eq!(updated_sub_status, "expired");

        // 7. Verify audit log for subscription_expired
        let expired_log_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_logs WHERE user_id = ?1 AND action = 'subscription_expired'"
        )
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
        assert_eq!(expired_log_count, 1);

        // 8. CRUCIAL: Verify ZERO user financial data was lost
        let user_accs = acc_repo.list_by_user(&user_id, false).await.unwrap();
        assert_eq!(
            user_accs.len(),
            1,
            "User accounts must remain completely intact"
        );
        assert_eq!(user_accs[0].name, "Bank Jago");

        let user_cats = cat_repo.list_by_user(&user_id).await.unwrap();
        assert!(
            user_cats.iter().any(|c| c.name == "Groceries"),
            "User categories must remain intact"
        );

        let (items, _) = tx_repo
            .list(&user_id, &TransactionFilter::default())
            .await
            .unwrap();
        assert_eq!(
            items.len(),
            1,
            "User transactions must remain completely intact"
        );
        assert_eq!(items[0].description, "Supermarket");

        // 9. Generate fresh token as free user and verify Premium endpoints are locked (HTTP 403)
        let (new_free_token, _) = ctx
            .jwt_engine
            .generate_token(&user_id, "expiry_user@test.com", "user", "free")
            .unwrap();
        let locked_req = Request::builder()
            .uri("/api/v1/analytics/advanced")
            .header(COOKIE, format!("auth_token={}", new_free_token))
            .body(Body::empty())
            .unwrap();

        let locked_res = ctx.app.clone().oneshot(locked_req).await.unwrap();
        assert_eq!(locked_res.status(), StatusCode::FORBIDDEN);
    }

    // -----------------------------------------------------------------------
    // T7: Both Singular and Plural Endpoints Route Correctly
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_trial_plural_and_singular_routes() {
        let ctx = setup_app().await;

        // User A uses singular /api/v1/subscription/trial
        let (_, token_a) = create_test_user(&ctx, "user_a@test.com", "free").await;
        let req_a = Request::builder()
            .uri("/api/v1/subscription/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token_a))
            .body(Body::empty())
            .unwrap();
        let res_a = ctx.app.clone().oneshot(req_a).await.unwrap();
        assert_eq!(res_a.status(), StatusCode::OK);

        // User B uses plural /api/v1/subscriptions/trial
        let (_, token_b) = create_test_user(&ctx, "user_b@test.com", "free").await;
        let req_b = Request::builder()
            .uri("/api/v1/subscriptions/trial")
            .method("POST")
            .header(COOKIE, format!("auth_token={}", token_b))
            .body(Body::empty())
            .unwrap();
        let res_b = ctx.app.clone().oneshot(req_b).await.unwrap();
        assert_eq!(res_b.status(), StatusCode::OK);
    }

    // -----------------------------------------------------------------------
    // T8: Concurrency Stress Test — Race Condition Protection
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_trial_concurrent_activation_race_condition_protection() {
        let ctx = setup_app().await;
        let (user_id, _token) = create_test_user(&ctx, "racer@test.com", "free").await;

        let payment_service = Arc::new(PaymentService::new_with_pool(
            PaymentConfig::default(),
            Arc::new(SqlxSubscriptionRepository::new(ctx.pool.clone())),
            Arc::new(SqlxUserRepository::new(ctx.pool.clone())),
            Arc::new(SqlxAuditRepository::new(ctx.pool.clone())),
            ctx.pool.clone(),
        ));

        // Spawn 10 concurrent activation tasks for the same user
        let mut handles = Vec::new();
        for _ in 0..10 {
            let ps = payment_service.clone();
            let uid = user_id.clone();
            let pool = ctx.pool.clone();
            handles.push(tokio::spawn(async move {
                ps.activate_trial_with_pool(&pool, &uid).await
            }));
        }

        let mut success_count = 0;
        let mut rejected_count = 0;

        for h in handles {
            let res = h.await.unwrap();
            match res {
                Ok(_) => success_count += 1,
                Err(backend::service::payment_service::PaymentError::TrialAlreadyUsed) => {
                    rejected_count += 1;
                }
                Err(other) => panic!(
                    "Unexpected error during concurrent trial activation: {:?}",
                    other
                ),
            }
        }

        assert_eq!(
            success_count, 1,
            "Exactly one concurrent activation must succeed"
        );
        assert_eq!(
            rejected_count, 9,
            "All 9 other concurrent activations must be rejected with TrialAlreadyUsed"
        );

        // Verify database records show exactly 1 trial
        let has_used: i64 = sqlx::query_scalar("SELECT has_used_trial FROM users WHERE id = ?1")
            .bind(&user_id)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
        assert_eq!(has_used, 1);

        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_logs WHERE user_id = ?1 AND action = 'subscription_trial_started'"
        )
        .bind(&user_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
        assert_eq!(audit_count, 1);
    }
}
