//! Milestone 1 Adversarial Challenger Suite: Concurrency, Slug Fuzzing & Indonesian Defaults
//!
//! Identity: teamwork_preview_challenger_m1_2
//!
//! Evaluates:
//! 1. Concurrent tenant creation races with colliding slugs (HTTP 409 Conflict vs 500, no panic, no DB corruption)
//! 2. Reserved slug bypass attempts (case variations, unicode homoglyphs, punctuation, length boundaries)
//! 3. Indonesian localization defaults (IDR, id-ID, Asia/Jakarta, INV) and rejection of invalid timezones

#[cfg(test)]
mod m1_challenger_concurrency_slug_tests {
    use axum::{
        body::Body,
        http::{
            header::{AUTHORIZATION, CONTENT_TYPE},
            Request, StatusCode,
        },
        response::Response,
    };
    use backend::api::{create_app, AppState, AuthState};
    use backend::repository::{
        account_repo::SqlxAccountRepository,
        audit_repo::SqlxAuditRepository,
        category_repo::SqlxCategoryRepository,
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        subscription_repo::SqlxSubscriptionRepository,
        transaction_repo::SqlxTransactionRepository,
        user_repo::{NewUser, SqlxUserRepository, UserRepository},
    };
    use backend::service::{
        auth_service::{AuthService, RegisterRequest},
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{PaymentConfig, PaymentService},
    };
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::task::JoinSet;
    use tower::ServiceExt;

    async fn setup_challenger_app() -> (
        axum::Router,
        Arc<JwtEngine>,
        sqlx::SqlitePool,
        tempfile::TempDir,
        Arc<AuthService>,
    ) {
        let dir = tempdir().expect("Failed to create tempdir");
        let db_path = dir.path().join("challenger_m1_test.sqlite");
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

        let jwt_secret = "challenger_jwt_secret_concurrency_and_slugs_12345678";
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

        let payment_service = Arc::new(PaymentService::new(
            PaymentConfig::default(),
            subscription_repo,
            user_repo.clone(),
            audit_repo,
        ));

        let auth_state = AuthState {
            auth_service: auth_service.clone(),
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
        (app, jwt_engine, pool, dir, auth_service)
    }

    async fn create_test_user(
        pool: &sqlx::SqlitePool,
        jwt_engine: &JwtEngine,
        email: &str,
        name: &str,
    ) -> (String, String) {
        let user_id = uuid::Uuid::new_v4().to_string();
        let user_repo = SqlxUserRepository::new(pool.clone());
        user_repo
            .create(&NewUser {
                id: user_id.clone(),
                email: email.to_string(),
                password_hash: "hash".to_string(),
                display_name: name.to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some("free".to_string()),
            })
            .await
            .unwrap();

        let (token, _) = jwt_engine
            .generate_token(&user_id, email, "user", "free")
            .unwrap();
        (user_id, token)
    }

    // =========================================================================
    // SECTION 1: CONCURRENT TENANT CREATION RACES WITH COLLIDING SLUGS
    // =========================================================================

    #[tokio::test]
    async fn challenge_concurrent_colliding_slugs_returns_409_conflict() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_user1, token1) = create_test_user(&pool, &jwt, "race1@test.com", "Racer 1").await;
        let (_user2, token2) = create_test_user(&pool, &jwt, "race2@test.com", "Racer 2").await;

        let colliding_slug = "quantum-corp";

        // Launch two concurrent requests attempting to create a workspace with the exact same slug
        let req1 = {
            let app = app.clone();
            let token = token1.clone();
            tokio::spawn(async move {
                app.oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Quantum Corp Alpha",
                                "slug": colliding_slug
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
            })
        };

        let req2 = {
            let app = app.clone();
            let token = token2.clone();
            tokio::spawn(async move {
                app.oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Quantum Corp Beta",
                                "slug": colliding_slug
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
            })
        };

        let (res1, res2) = tokio::join!(req1, req2);
        let resp1: Response = res1.unwrap();
        let resp2: Response = res2.unwrap();

        let status1 = resp1.status();
        let status2 = resp2.status();

        let body1_bytes = resp1.into_body().collect().await.unwrap().to_bytes();
        let body2_bytes = resp2.into_body().collect().await.unwrap().to_bytes();
        let body1: Value = serde_json::from_slice(&body1_bytes).unwrap_or(json!({}));
        let body2: Value = serde_json::from_slice(&body2_bytes).unwrap_or(json!({}));

        println!(
            "Concurrent Race Results: Resp1 = {:?} (body: {}), Resp2 = {:?} (body: {})",
            status1, body1, status2, body2
        );

        // Invariant: Exactly ONE request must succeed with 201 CREATED
        let success_count =
            (status1 == StatusCode::CREATED) as usize + (status2 == StatusCode::CREATED) as usize;
        assert_eq!(
            success_count, 1,
            "Exactly one concurrent tenant creation must succeed with 201"
        );

        // Invariant: The failing request MUST receive HTTP 409 CONFLICT (never 500 or panic)
        let conflict_count =
            (status1 == StatusCode::CONFLICT) as usize + (status2 == StatusCode::CONFLICT) as usize;
        assert_eq!(
            conflict_count, 1,
            "The losing racer must receive HTTP 409 Conflict, but got status1={:?}, status2={:?}",
            status1, status2
        );

        // Database integrity check: exactly 1 tenant record in DB with that slug
        let tenant_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants WHERE slug = ?1")
            .bind(colliding_slug)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            tenant_count, 1,
            "Database must contain exactly 1 tenant with the colliding slug"
        );

        // Database integrity check: exactly 1 business profile for that tenant
        let profile_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM business_profiles bp JOIN tenants t ON t.id = bp.tenant_id WHERE t.slug = ?1"
        )
        .bind(colliding_slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            profile_count, 1,
            "Database must contain exactly 1 business profile for the winning tenant"
        );

        // Database integrity check: exactly 1 membership for that tenant
        let membership_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM memberships m JOIN tenants t ON t.id = m.tenant_id WHERE t.slug = ?1"
        )
        .bind(colliding_slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            membership_count, 1,
            "Database must contain exactly 1 membership for the winning tenant"
        );
    }

    #[tokio::test]
    async fn challenge_high_concurrency_colliding_slug_stress() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let concurrency = 8;
        let colliding_slug = "stampede-corp";

        // Seed 8 different users
        let mut users = Vec::new();
        for i in 0..concurrency {
            let email = format!("stampede{}@test.com", i);
            let name = format!("Stampeder {}", i);
            let (_uid, token) = create_test_user(&pool, &jwt, &email, &name).await;
            users.push(token);
        }

        // Fire all 8 requests concurrently using JoinSet
        let mut join_set: JoinSet<(StatusCode, Value)> = JoinSet::new();
        for (i, token) in users.into_iter().enumerate() {
            let app_clone = app.clone();
            let slug = colliding_slug.to_string();
            join_set.spawn(async move {
                let resp = app_clone
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/v1/tenants")
                            .header(AUTHORIZATION, format!("Bearer {}", token))
                            .header(CONTENT_TYPE, "application/json")
                            .body(Body::from(
                                json!({
                                    "name": format!("Stampede Corp {}", i),
                                    "slug": slug
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();

                let status = resp.status();
                let bytes = resp.into_body().collect().await.unwrap().to_bytes();
                let body: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
                (status, body)
            });
        }

        let mut created_count = 0;
        let mut conflict_count = 0;
        let mut unexpected_statuses = Vec::new();

        while let Some(res) = join_set.join_next().await {
            let (status, body) = res.unwrap();
            if status == StatusCode::CREATED {
                created_count += 1;
            } else if status == StatusCode::CONFLICT {
                conflict_count += 1;
            } else {
                unexpected_statuses.push((status, body));
            }
        }

        println!(
            "Stampede Results: created={}, conflict={}, unexpected={:?}",
            created_count, conflict_count, unexpected_statuses
        );

        assert_eq!(
            created_count, 1,
            "Exactly 1 request out of {} must succeed with 201",
            concurrency
        );
        assert!(
            unexpected_statuses.is_empty(),
            "No requests should return unexpected errors (e.g. 500 or DB panic): {:?}",
            unexpected_statuses
        );
        assert_eq!(
            conflict_count,
            concurrency - 1,
            "All losing requests must return 409 Conflict"
        );

        // Verify DB integrity
        let final_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants WHERE slug = ?1")
            .bind(colliding_slug)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(final_count, 1);
    }

    #[tokio::test]
    async fn challenge_sequential_slug_collision_returns_409() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_user1, token1) = create_test_user(&pool, &jwt, "seq1@test.com", "Seq 1").await;
        let (_user2, token2) = create_test_user(&pool, &jwt, "seq2@test.com", "Seq 2").await;

        let slug = "reused-slug-target";

        // First creation succeeds
        let res1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token1))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "First Tenant",
                            "slug": slug
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res1.status(), StatusCode::CREATED);

        // Second creation with identical slug must return 409 CONFLICT with code SLUG_ALREADY_EXISTS
        let res2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token2))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Second Tenant",
                            "slug": slug
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::CONFLICT);
        let body2: Value =
            serde_json::from_slice(&res2.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(body2["code"], "SLUG_ALREADY_EXISTS");
    }

    #[tokio::test]
    async fn challenge_true_race_condition_unique_constraint_violation() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) =
            create_test_user(&pool, &jwt, "race_victim@test.com", "Race Victim").await;

        let race_slug = "race-collision-target";

        // 1. Begin a raw transaction tx1 that inserts the race_slug
        let mut tx1 = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES ('t_race1', 'Racer 1', ?1, 'ACTIVE', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        )
        .bind(race_slug)
        .execute(&mut *tx1)
        .await
        .unwrap();

        // 2. While tx1 is uncommitted, spawn the HTTP request that tries to create a workspace with race_slug.
        // In SQLite WAL mode, readers query the DB snapshot before tx1, so SELECT slug returns None!
        // The handler will pass the application-level slug check and attempt tx.begin() / INSERT INTO tenants.
        let app_clone = app.clone();
        let token_clone = token.clone();
        let slug_str = race_slug.to_string();
        let handle = tokio::spawn(async move {
            app_clone
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token_clone))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Race Victim Tenant",
                                "slug": slug_str
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
        });

        // Give the spawned task 50ms to perform the SELECT check and begin waiting on the write lock
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 3. Commit tx1, releasing the lock so the spawned task proceeds with its INSERT
        tx1.commit().await.unwrap();

        // 4. Await response from the spawned HTTP request
        let resp = handle.await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));

        println!(
            "True Race Condition Response: status = {:?}, body = {}",
            status, body
        );

        // REQUIREMENT FROM DISPATCH:
        // "Concurrent tenant creation races with colliding slugs (verify HTTP 409 conflict, no panic or corrupted DB)."
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "Concurrent colliding slug race must return HTTP 409 Conflict, got status {:?} and body {}",
            status, body
        );
        let code = body["code"].as_str().unwrap_or("");
        assert!(
            code == "SLUG_ALREADY_EXISTS" || code == "UNIQUE_VIOLATION",
            "Error code must be SLUG_ALREADY_EXISTS or UNIQUE_VIOLATION, got: {}",
            code
        );
        println!(
            "\n✅ Clean conflict handling: returned HTTP 409 Conflict with code '{}'!",
            code
        );
    }

    // =========================================================================
    // SECTION 2: RESERVED SLUG BYPASS ATTEMPTS & ADVERSARIAL FUZZING
    // =========================================================================

    #[tokio::test]
    async fn challenge_reserved_slug_case_variations() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) = create_test_user(&pool, &jwt, "fuzzer1@test.com", "Fuzzer 1").await;

        let variations = [
            "ADMIN",
            "Admin",
            "aDmiN",
            "AdMiN",
            "API",
            "Api",
            "aPi",
            "APP",
            "App",
            "BILLING",
            "Billing",
            "WWW",
            "Www",
            "SETTINGS",
            "Settings",
            "SUPPORT",
            "Support",
            "DASHBOARD",
            "Dashboard",
            "LOGIN",
            "Login",
            "LOGOUT",
            "Logout",
            "REGISTER",
            "Register",
            "SYSTEM",
            "System",
            "ROOT",
            "Root",
            "HEALTH",
            "Health",
            "DOCS",
            "Docs",
        ];

        for slug in variations {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": format!("Tenant {}", slug),
                                "slug": slug
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "Case variation '{}' of reserved slug must be rejected",
                slug
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(
                body["code"], "RESERVED_SLUG",
                "Error code must be RESERVED_SLUG for '{}'",
                slug
            );
        }
    }

    #[tokio::test]
    async fn challenge_reserved_slug_unicode_and_homoglyphs() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) = create_test_user(&pool, &jwt, "fuzzer2@test.com", "Fuzzer 2").await;

        // Cyrillic homoglyphs and unicode lookalikes
        let unicode_slugs = [
            "аdmin",         // Cyrillic 'а' (U+0430)
            "арi",           // Cyrillic 'а' and 'р'
            "ɑdmin",         // Latin alpha (U+0251)
            "admiո",         // Armenian small letter (U+0578)
            "admin\u{200B}", // Zero-width space
            "admin\u{00A0}", // Non-breaking space
            "ａｄｍｉｎ",    // Fullwidth Latin
            "tést-slug",     // Accented Latin e
            "инвините",      // Cyrillic
            "🏢-workspace",  // Emoji
        ];

        for slug in unicode_slugs {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Unicode Slug Test",
                                "slug": slug
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "Unicode slug '{}' must be rejected with 400",
                slug
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            let code = body["code"].as_str().unwrap_or("");
            assert!(
                code == "INVALID_SLUG" || code == "RESERVED_SLUG",
                "Rejection code must be INVALID_SLUG or RESERVED_SLUG for '{}', got '{}'",
                slug,
                code
            );
        }
    }

    #[tokio::test]
    async fn challenge_slug_punctuation_and_special_chars() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) = create_test_user(&pool, &jwt, "fuzzer3@test.com", "Fuzzer 3").await;

        let malformed = [
            "admin.corp",
            "api/v1",
            "foo$bar",
            "tenant@work",
            "tenant#1",
            "tenant%20",
            "tenant\0null",
            "tenant*corp",
            "tenant+plus",
            "tenant=equal",
            "tenant;semi",
            "tenant:colon",
            "tenant<angle>",
            "tenant?query",
            "tenant!excl",
            "tenant(paren)",
        ];

        for slug in malformed {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Malformed Slug Test",
                                "slug": slug
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "Malformed slug '{}' must be rejected with 400",
                slug
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(body["code"], "INVALID_SLUG");
        }
    }

    #[tokio::test]
    async fn challenge_slug_hyphen_boundaries_and_consecutive() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) = create_test_user(&pool, &jwt, "fuzzer4@test.com", "Fuzzer 4").await;

        let invalid_hyphens = [
            "-leading",
            "trailing-",
            "-both-",
            "--double-leading",
            "double-trailing--",
            "consecutive--hyphen",
            "triple---hyphen",
            "many----hyphens",
            "a--b",
            "a--b--c",
        ];

        for slug in invalid_hyphens {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Hyphen Slug Test",
                                "slug": slug
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "Hyphen boundary slug '{}' must be rejected with 400",
                slug
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(body["code"], "INVALID_SLUG");
        }
    }

    #[tokio::test]
    async fn challenge_slug_length_boundaries() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) = create_test_user(&pool, &jwt, "fuzzer5@test.com", "Fuzzer 5").await;

        // Under min boundary (< 3 chars): explicit short slugs must be rejected with 400
        let too_short = ["a", "ab", "1", "12"];
        for slug in too_short {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Short Slug Workspace",
                                "slug": slug
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "Slug '{}' (len < 3) must be rejected",
                slug
            );
        }

        // Empty slug with short name (auto-generation produces < 3 chars) -> rejected
        let res_short_name = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "ab",
                            "slug": ""
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res_short_name.status(),
            StatusCode::BAD_REQUEST,
            "Auto-generated slug < 3 chars must be rejected"
        );

        // Min boundary valid: 3 chars (e.g. "xyz")
        let min_valid = "xyz";
        let res_min = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Min Valid Slug",
                            "slug": min_valid
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res_min.status(),
            StatusCode::CREATED,
            "Slug 'xyz' (len 3) must be accepted"
        );

        // Max boundary valid: exactly 63 chars
        let max_valid = "a".repeat(63);
        let res_max = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Max Valid Slug",
                            "slug": max_valid
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res_max.status(),
            StatusCode::CREATED,
            "Slug of len 63 must be accepted"
        );

        // Over max boundary: 64 chars
        let too_long = "b".repeat(64);
        let res_long = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Too Long Slug",
                            "slug": too_long
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res_long.status(),
            StatusCode::BAD_REQUEST,
            "Slug of len 64 must be rejected"
        );

        // Extreme length: 1000 chars
        let extreme = "c".repeat(1000);
        let res_extreme = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Extreme Slug",
                            "slug": extreme
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_extreme.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn challenge_auto_slug_generation_and_reserved_word_collision() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) = create_test_user(&pool, &jwt, "fuzzer6@test.com", "Fuzzer 6").await;

        // When slug is omitted but name is "Admin", auto-generated slug is "admin", which must be rejected as reserved
        let res_admin = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Admin"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_admin.status(), StatusCode::BAD_REQUEST);
        let body: Value =
            serde_json::from_slice(&res_admin.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["code"], "RESERVED_SLUG");

        // When name has special symbols like "Toko Kopi 100%", generated slug should be clean
        let res_clean = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Toko Kopi 100%"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_clean.status(), StatusCode::CREATED);
        let body_clean: Value =
            serde_json::from_slice(&res_clean.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body_clean["slug"], "toko-kopi-100");
    }

    // =========================================================================
    // SECTION 3: INDONESIAN LOCALIZATION DEFAULTS & TIMEZONE REJECTION
    // =========================================================================

    #[tokio::test]
    async fn challenge_indonesian_localization_defaults_on_creation() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) =
            create_test_user(&pool, &jwt, "indo_default@test.com", "Indo Defaults").await;

        // Create tenant without supplying timezone, currency, or other overrides
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "CV Berkah Abadi",
                            "slug": "cv-berkah-abadi"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Query business profile directly from DB and via API
        let profile_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(profile_res.status(), StatusCode::OK);
        let profile: Value =
            serde_json::from_slice(&profile_res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();

        assert_eq!(profile["currency"], "IDR", "Default currency must be IDR");
        assert_eq!(profile["locale"], "id-ID", "Default locale must be id-ID");
        assert_eq!(
            profile["timezone"], "Asia/Jakarta",
            "Default timezone must be Asia/Jakarta"
        );
        assert_eq!(
            profile["invoice_prefix"], "INV",
            "Default invoice prefix must be INV"
        );
        assert_eq!(profile["business_type"], "general");
    }

    #[tokio::test]
    async fn challenge_valid_indonesian_timezones_accepted() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) = create_test_user(&pool, &jwt, "tz_tester@test.com", "TZ Tester").await;

        let valid_zones = [
            ("Asia/Jakarta", "WIB"),
            ("Asia/Pontianak", "WIB"),
            ("Asia/Makassar", "WITA"),
            ("Asia/Jayapura", "WIT"),
            ("UTC", "UTC"),
            // Case insensitivity checks
            ("asia/jakarta", "WIB lowercase"),
            ("ASIA/MAKASSAR", "WITA uppercase"),
            ("Asia/jayapura", "WIT mixed"),
        ];

        for (i, (tz, label)) in valid_zones.iter().enumerate() {
            let slug = format!("tz-workspace-{}", i);
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": format!("TZ Workspace {}", label),
                                "slug": slug,
                                "timezone": tz
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::CREATED,
                "Valid Indonesian timezone '{}' ({}) must be accepted on creation",
                tz,
                label
            );

            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            let tenant_id = body["id"].as_str().unwrap();

            let prof_res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(prof_res.status(), StatusCode::OK);
            let profile: Value =
                serde_json::from_slice(&prof_res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert!(
                profile["timezone"]
                    .as_str()
                    .unwrap()
                    .eq_ignore_ascii_case(tz),
                "Stored timezone '{}' must match requested '{}'",
                profile["timezone"],
                tz
            );
        }
    }

    #[tokio::test]
    async fn challenge_invalid_timezones_strictly_rejected() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;
        let (_user, token) =
            create_test_user(&pool, &jwt, "bad_tz_user@test.com", "Bad TZ User").await;

        let invalid_timezones = [
            "America/New_York",
            "Europe/London",
            "Asia/Tokyo",
            "Asia/Singapore",
            "Australia/Sydney",
            "Pacific/Auckland",
            "GMT",
            "GMT+7",
            "WIB", // Shorthand must be rejected; full IANA identifier required
            "WITA",
            "WIT",
            "Bogus/Timezone",
            "Invalid/Zone",
            "Asia/Bali", // Non-canonical (canonical is Asia/Makassar)
            "",
            "   ",
        ];

        // 1. Test rejection during POST /api/v1/tenants
        for tz in invalid_timezones {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/tenants")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "name": "Invalid TZ Workspace",
                                "slug": "invalid-tz-slug",
                                "timezone": tz
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "Invalid timezone '{}' must be rejected during tenant creation",
                tz
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(
                body["code"], "INVALID_TIMEZONE",
                "Error code must be INVALID_TIMEZONE for '{}'",
                tz
            );
        }

        // 2. Test rejection during PUT /api/v1/tenants/:id/profile
        // First create a valid workspace
        let valid_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Valid Profile Workspace",
                            "slug": "valid-profile-ws"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(valid_res.status(), StatusCode::CREATED);
        let valid_body: Value =
            serde_json::from_slice(&valid_res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let tenant_id = valid_body["id"].as_str().unwrap();

        for tz in invalid_timezones {
            let put_res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "timezone": tz
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                put_res.status(),
                StatusCode::BAD_REQUEST,
                "Invalid timezone '{}' must be rejected during profile update",
                tz
            );
            let body: Value =
                serde_json::from_slice(&put_res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(body["code"], "INVALID_TIMEZONE");
        }
    }

    #[tokio::test]
    async fn challenge_registration_auto_provisioning_indonesian_defaults() {
        let (app, _jwt, pool, _dir, auth_service) = setup_challenger_app().await;

        let reg_email = "auto_indo_reg@example.com";
        let reg_name = "Dewi Lestari";

        let auth_res = auth_service
            .register(RegisterRequest {
                email: reg_email.to_string(),
                password: "SecurePassword123!".to_string(),
                display_name: reg_name.to_string(),
            })
            .await
            .expect("Registration must succeed");

        let user_id = &auth_res.user.id;
        let token = &auth_res.token;

        // Fetch personal workspace details
        let list_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_res.status(), StatusCode::OK);
        let list: Value =
            serde_json::from_slice(&list_res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            list.as_array().unwrap().len(),
            1,
            "User must have exactly 1 workspace"
        );

        let personal_ws = &list[0];
        let tenant_id = personal_ws["id"].as_str().unwrap();
        assert_eq!(personal_ws["is_default"], true);
        assert_eq!(personal_ws["role"], "owner");

        // Verify Indonesian defaults on auto-provisioned profile in DB
        #[derive(sqlx::FromRow)]
        struct ProfileCheck {
            timezone: String,
            currency: String,
            locale: String,
            invoice_prefix: String,
            business_type: String,
        }

        let db_profile = sqlx::query_as::<_, ProfileCheck>(
            "SELECT timezone, currency, locale, invoice_prefix, business_type FROM business_profiles WHERE tenant_id = ?1"
        )
        .bind(tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(db_profile.timezone, "Asia/Jakarta");
        assert_eq!(db_profile.currency, "IDR");
        assert_eq!(db_profile.locale, "id-ID");
        assert_eq!(db_profile.invoice_prefix, "INV");
        assert_eq!(db_profile.business_type, "personal");

        // Ensure user has owner membership
        let owner_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM memberships WHERE tenant_id = ?1 AND user_id = ?2 AND role = 'owner' AND status = 'ACTIVE'"
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(owner_count, 1);
    }
}
