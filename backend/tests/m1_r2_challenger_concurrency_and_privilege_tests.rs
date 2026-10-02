//! Milestone 1 Round 2 Adversarial Challenger Suite:
//! Empirical Concurrency Stampedes & Privilege Escalation / Member Manipulation
//!
//! Identity: teamwork_preview_challenger_m1_r2_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Evaluates:
//! 1. Concurrency race conditions: High concurrency (20 threads) slug registration stampedes
//!    to guarantee 100% of conflicting requests return HTTP 409 Conflict with code SLUG_ALREADY_EXISTS, never HTTP 500.
//! 2. WAL lock / uncommitted transaction race slug collisions returning 409 Conflict.
//! 3. Auto-generated slug collision stampedes returning 409 Conflict.
//! 4. Privilege escalation & member manipulation: Member role update RBAC matrix (Admin, Staff, Manager, Accountant).
//! 5. Member removal RBAC matrix (Admin cannot remove Owner, Staff cannot remove anyone, Outsider gets 404).
//! 6. Concurrent member deletions (idempotent / clean 204 or 404, never 500).
//! 7. Concurrent member role update vs member deletion race.
//! 8. Sole owner self-demotion and self-removal guards under concurrency.
//! 9. Privilege escalation guards: Assigning 'owner' role via update or invite strictly rejected with 400 INVALID_ROLE.
//! 10. Concurrent unauthorized manipulation stampede (403 for internal unauthorized, 404 for outsiders, 0 500s).

#[cfg(test)]
mod m1_r2_challenger_tests {
    use axum::{
        body::Body,
        http::{
            header::{AUTHORIZATION, CONTENT_TYPE},
            Request, StatusCode,
        },
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
        auth_service::AuthService,
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{PaymentConfig, PaymentService},
        tenant_service::TenantService,
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
        let db_path = dir.path().join("challenger_m1_r2_test.sqlite");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());

        let config = DbConfig {
            database_url: url,
            max_connections: 25,
            min_connections: 1,
            busy_timeout_ms: 15_000,
            acquire_timeout_secs: 15,
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

        let jwt_secret = "m1_r2_challenger_jwt_secret_concurrency_race_tests_12345";
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
            tenant_service: Arc::new(TenantService::new_with_pool(pool.clone())),
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
                password_hash: "secret_hash".to_string(),
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
    // SECTION 1: CONCURRENT SLUG REGISTRATION STAMPEDES (HTTP 409 GUARANTEE)
    // =========================================================================

    #[tokio::test]
    async fn challenge_concurrent_slug_stampede_20_threads_100_percent_409() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let concurrency = 20;
        let colliding_slug = "stampede-alpha-20";

        // Seed 20 different authenticated users
        let mut users = Vec::new();
        for i in 0..concurrency {
            let email = format!("stampede20_{}@test.com", i);
            let name = format!("Stampeder {}", i);
            let (_uid, token) = create_test_user(&pool, &jwt, &email, &name).await;
            users.push(token);
        }

        // Fire all 20 requests concurrently using JoinSet
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
        let mut non_conforming_conflicts = Vec::new();
        let mut unexpected_statuses = Vec::new();

        while let Some(res) = join_set.join_next().await {
            let (status, body) = res.unwrap();
            if status == StatusCode::CREATED {
                created_count += 1;
            } else if status == StatusCode::CONFLICT {
                conflict_count += 1;
                // Verify RFC 7807 problem details and semantic code
                let code = body["code"].as_str().unwrap_or("");
                let status_code = body["status"].as_u64().unwrap_or(0);
                if code != "SLUG_ALREADY_EXISTS" || status_code != 409 {
                    non_conforming_conflicts.push(body);
                }
            } else {
                unexpected_statuses.push((status, body));
            }
        }

        println!(
            "20-Thread Stampede Results: created={}, conflict={}, unexpected={:?}, non_conforming={:?}",
            created_count, conflict_count, unexpected_statuses, non_conforming_conflicts
        );

        // MANDATORY INVARIANT 1: Exactly 1 request must succeed with HTTP 201 Created
        assert_eq!(
            created_count, 1,
            "Exactly 1 request out of {} must succeed with 201 Created",
            concurrency
        );

        // MANDATORY INVARIANT 2: ZERO requests return HTTP 500 or any unexpected error
        assert!(
            unexpected_statuses.is_empty(),
            "No requests should return HTTP 500 or other unexpected status: {:?}",
            unexpected_statuses
        );

        // MANDATORY INVARIANT 3: 100% of conflicting requests return HTTP 409 Conflict
        assert_eq!(
            conflict_count,
            concurrency - 1,
            "All {} conflicting requests must return HTTP 409 Conflict",
            concurrency - 1
        );

        // MANDATORY INVARIANT 4: 100% of conflicting responses contain code 'SLUG_ALREADY_EXISTS'
        assert!(
            non_conforming_conflicts.is_empty(),
            "All 409 Conflict responses must contain code SLUG_ALREADY_EXISTS: {:?}",
            non_conforming_conflicts
        );

        // MANDATORY INVARIANT 5: Database state is strictly intact
        let tenant_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants WHERE slug = ?1")
            .bind(colliding_slug)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(tenant_count, 1);

        let profile_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM business_profiles bp JOIN tenants t ON t.id = bp.tenant_id WHERE t.slug = ?1"
        )
        .bind(colliding_slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(profile_count, 1);

        let membership_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM memberships m JOIN tenants t ON t.id = m.tenant_id WHERE t.slug = ?1"
        )
        .bind(colliding_slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(membership_count, 1);
    }

    #[tokio::test]
    async fn challenge_concurrent_uncommitted_tx_slug_collision_stampede() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let colliding_slug = "uncommitted-race-target";
        let num_requests = 8;

        // Pre-create users before acquiring SQLite write lock
        let mut tokens = Vec::new();
        for i in 0..num_requests {
            let email = format!("uncommitted_racer_{}@test.com", i);
            let (_uid, token) = create_test_user(&pool, &jwt, &email, "Racer").await;
            tokens.push(token);
        }

        // 1. Begin a raw transaction that inserts colliding_slug into the tenants table
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES ('t_uncommitted', 'Pre-existing Corp', ?1, 'ACTIVE', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        )
        .bind(colliding_slug)
        .execute(&mut *tx)
        .await
        .unwrap();

        // 2. Concurrently spawn 8 HTTP requests trying to register a workspace with the colliding slug
        let mut join_set: JoinSet<(StatusCode, Value)> = JoinSet::new();
        for (i, token) in tokens.into_iter().enumerate() {
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
                                    "name": format!("Racer Corp {}", i),
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

        // Give the spawned tasks a moment to start and attempt to acquire lock
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 3. Commit the raw transaction
        tx.commit().await.unwrap();

        // 4. Collect all responses
        let mut conflict_count = 0;
        let mut unexpected = Vec::new();

        while let Some(res) = join_set.join_next().await {
            let (status, body) = res.unwrap();
            if status == StatusCode::CONFLICT {
                let code = body["code"].as_str().unwrap_or("");
                if code == "SLUG_ALREADY_EXISTS" {
                    conflict_count += 1;
                } else {
                    unexpected.push((status, body));
                }
            } else {
                unexpected.push((status, body));
            }
        }

        println!(
            "Uncommitted TX Race Results: conflict={}, unexpected={:?}",
            conflict_count, unexpected
        );

        assert_eq!(
            conflict_count, num_requests,
            "100% of conflicting requests must receive HTTP 409 Conflict with SLUG_ALREADY_EXISTS"
        );
        assert!(
            unexpected.is_empty(),
            "No requests should fail with 500 or any non-409 status: {:?}",
            unexpected
        );
    }

    #[tokio::test]
    async fn challenge_concurrent_auto_slug_generation_stampede() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let num_racers = 8;
        let identical_name = "Nusantara Financial Hub";
        // Auto-generated slug will be "nusantara-financial-hub"

        let mut join_set: JoinSet<(StatusCode, Value)> = JoinSet::new();
        for i in 0..num_racers {
            let email = format!("auto_slug_{}@test.com", i);
            let (_uid, token) = create_test_user(&pool, &jwt, &email, "Auto Slugger").await;
            let app_clone = app.clone();

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
                                    "name": identical_name
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

        let mut created = 0;
        let mut conflict = 0;
        let mut unexpected = Vec::new();

        while let Some(res) = join_set.join_next().await {
            let (status, body) = res.unwrap();
            if status == StatusCode::CREATED {
                created += 1;
            } else if status == StatusCode::CONFLICT {
                if body["code"] == "SLUG_ALREADY_EXISTS" {
                    conflict += 1;
                } else {
                    unexpected.push((status, body));
                }
            } else {
                unexpected.push((status, body));
            }
        }

        assert_eq!(created, 1, "Exactly 1 request must win the auto-slug race");
        assert_eq!(
            conflict,
            num_racers - 1,
            "All losing auto-slug requests must return 409 SLUG_ALREADY_EXISTS"
        );
        assert!(unexpected.is_empty(), "No 500s: {:?}", unexpected);
    }

    // =========================================================================
    // SECTION 2: PRIVILEGE ESCALATION & MEMBER MANIPULATION UNDER CONCURRENCY & UNAUTHORIZED CONDITIONS
    // =========================================================================

    #[tokio::test]
    async fn challenge_privilege_escalation_role_update_rejection() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_owner_id, token_owner) =
            create_test_user(&pool, &jwt, "owner_priv@test.com", "Owner").await;
        let (staff_id, _token_staff) =
            create_test_user(&pool, &jwt, "staff_priv@test.com", "Staff").await;

        // Owner creates workspace
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Privilege Workspace", "slug": "priv-ws" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Invite Staff
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "email": "staff_priv@test.com", "role": "staff" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Adversarial attempts to elevate staff to 'owner'
        let forbidden_roles = [
            "owner",
            "Owner",
            "OWNER",
            "oWnEr",
            " owner ",
            "superadmin",
            "root",
            "admin_root",
            "' OR '1'='1",
            "<script>alert(1)</script>",
            "",
            "   ",
        ];

        for bad_role in forbidden_roles {
            let update_res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!(
                            "/api/v1/tenants/{}/members/{}",
                            tenant_id, staff_id
                        ))
                        .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(json!({ "role": bad_role }).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                update_res.status(),
                StatusCode::BAD_REQUEST,
                "Attempt to update role to '{}' must be rejected with 400 Bad Request",
                bad_role
            );
            let err_body: Value =
                serde_json::from_slice(&update_res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(
                err_body["code"], "INVALID_ROLE",
                "Rejection code must be INVALID_ROLE for role '{}'",
                bad_role
            );
        }

        // Verify staff role in DB remains 'staff'
        let role_in_db: String = sqlx::query_scalar(
            "SELECT role FROM memberships WHERE tenant_id = ?1 AND user_id = ?2",
        )
        .bind(tenant_id)
        .bind(&staff_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(role_in_db, "staff");
    }

    #[tokio::test]
    async fn challenge_unauthorized_member_role_update_matrix() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_owner_id, token_owner) =
            create_test_user(&pool, &jwt, "owner_mat@test.com", "Owner").await;
        let (_admin_id, token_admin) =
            create_test_user(&pool, &jwt, "admin_mat@test.com", "Admin").await;
        let (_manager_id, token_manager) =
            create_test_user(&pool, &jwt, "manager_mat@test.com", "Manager").await;
        let (_accountant_id, token_accountant) =
            create_test_user(&pool, &jwt, "accountant_mat@test.com", "Accountant").await;
        let (_staff_id, token_staff) =
            create_test_user(&pool, &jwt, "staff_mat@test.com", "Staff").await;
        let (target_id, _token_target) =
            create_test_user(&pool, &jwt, "target_mat@test.com", "Target Staff").await;
        let (_outsider_id, token_outsider) =
            create_test_user(&pool, &jwt, "outsider_mat@test.com", "Outsider").await;

        // Owner creates workspace
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Matrix Workspace", "slug": "matrix-ws" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Invite members
        for (email, role) in [
            ("admin_mat@test.com", "administrator"),
            ("manager_mat@test.com", "manager"),
            ("accountant_mat@test.com", "accountant"),
            ("staff_mat@test.com", "staff"),
            ("target_mat@test.com", "staff"),
        ] {
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({ "email": email, "role": role }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        // Test non-owner roles attempting to update target member's role -> 403 Forbidden
        let non_owner_callers = [
            ("administrator", token_admin),
            ("manager", token_manager),
            ("accountant", token_accountant),
            ("staff", token_staff),
        ];

        for (role_name, token) in non_owner_callers {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!(
                            "/api/v1/tenants/{}/members/{}",
                            tenant_id, target_id
                        ))
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(json!({ "role": "manager" }).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::FORBIDDEN,
                "Role '{}' must be forbidden (403) from updating member roles",
                role_name
            );
            let err_body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(err_body["code"], "FORBIDDEN");
        }

        // Outsider attempting to update member role -> 404 Not Found (anti-enumeration)
        let outsider_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, target_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_outsider))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "role": "manager" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            outsider_res.status(),
            StatusCode::NOT_FOUND,
            "Outsider must receive 404 Not Found to prevent workspace enumeration"
        );
        let err_body: Value =
            serde_json::from_slice(&outsider_res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(err_body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn challenge_unauthorized_member_removal_matrix() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_owner_id, token_owner) =
            create_test_user(&pool, &jwt, "owner_del@test.com", "Owner").await;
        let (_admin_id, token_admin) =
            create_test_user(&pool, &jwt, "admin_del@test.com", "Admin").await;
        let (_manager_id, token_manager) =
            create_test_user(&pool, &jwt, "manager_del@test.com", "Manager").await;
        let (_staff_id, token_staff) =
            create_test_user(&pool, &jwt, "staff_del@test.com", "Staff").await;
        let (target_id, _token_target) =
            create_test_user(&pool, &jwt, "target_del@test.com", "Target").await;
        let (_outsider_id, token_outsider) =
            create_test_user(&pool, &jwt, "outsider_del@test.com", "Outsider").await;

        // Owner creates workspace
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Removal Matrix Workspace", "slug": "rem-mat-ws" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Invite members
        for (email, role) in [
            ("admin_del@test.com", "administrator"),
            ("manager_del@test.com", "manager"),
            ("staff_del@test.com", "staff"),
            ("target_del@test.com", "staff"),
        ] {
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({ "email": email, "role": role }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        // 1. Critical Invariant: Admin CANNOT remove Owner -> 403 Forbidden
        let admin_rem_owner = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, _owner_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_admin))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            admin_rem_owner.status(),
            StatusCode::FORBIDDEN,
            "Admin attempting to remove owner must return 403"
        );

        // 2. Staff cannot remove Target -> 403 Forbidden
        let staff_rem_target = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, target_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_staff))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(staff_rem_target.status(), StatusCode::FORBIDDEN);

        // 3. Manager cannot remove Target -> 403 Forbidden
        let manager_rem_target = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, target_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_manager))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(manager_rem_target.status(), StatusCode::FORBIDDEN);

        // 4. Outsider cannot remove anyone -> 404 Not Found (anti-enumeration)
        let outsider_rem = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, target_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_outsider))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(outsider_rem.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn challenge_sole_owner_self_demotion_and_removal_blocked() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (owner_id, token_owner) =
            create_test_user(&pool, &jwt, "sole_owner@test.com", "Sole Owner").await;

        // Owner creates workspace
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Sole Owner Corp", "slug": "sole-owner-corp" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // 1. Sole owner attempts to demote self to staff -> 409 Conflict (SOLE_OWNER_DEMOTION)
        let demote_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, owner_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "role": "staff" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(demote_res.status(), StatusCode::CONFLICT);
        let demote_body: Value =
            serde_json::from_slice(&demote_res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(demote_body["code"], "SOLE_OWNER_DEMOTION");

        // 2. Sole owner attempts to leave / delete self -> 409 Conflict (SOLE_OWNER_LEAVE)
        let leave_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, owner_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(leave_res.status(), StatusCode::CONFLICT);
        let leave_body: Value =
            serde_json::from_slice(&leave_res.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(leave_body["code"], "SOLE_OWNER_LEAVE");

        // Verify owner is still active owner in DB
        let current_role: String = sqlx::query_scalar(
            "SELECT role FROM memberships WHERE tenant_id = ?1 AND user_id = ?2",
        )
        .bind(tenant_id)
        .bind(&owner_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(current_role, "owner");
    }

    #[tokio::test]
    async fn challenge_concurrent_member_deletions_race() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_owner_id, token_owner) =
            create_test_user(&pool, &jwt, "del_owner@test.com", "Owner").await;
        let (_admin1_id, token_admin1) =
            create_test_user(&pool, &jwt, "del_admin1@test.com", "Admin 1").await;
        let (_admin2_id, token_admin2) =
            create_test_user(&pool, &jwt, "del_admin2@test.com", "Admin 2").await;
        let (target_id, _token_target) =
            create_test_user(&pool, &jwt, "del_target@test.com", "Target Staff").await;

        // Owner creates workspace
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Concurrent Deletion WS", "slug": "conc-del-ws" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Invite Admin 1, Admin 2, Target
        for (email, role) in [
            ("del_admin1@test.com", "administrator"),
            ("del_admin2@test.com", "administrator"),
            ("del_target@test.com", "staff"),
        ] {
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({ "email": email, "role": role }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        // Concurrently fire 3 delete requests for Target (Owner, Admin 1, Admin 2)
        let tokens = vec![token_owner, token_admin1, token_admin2];
        let mut join_set: JoinSet<StatusCode> = JoinSet::new();

        for token in tokens {
            let app_clone = app.clone();
            let tid = tenant_id.to_string();
            let uid = target_id.clone();
            join_set.spawn(async move {
                let resp = app_clone
                    .oneshot(
                        Request::builder()
                            .method("DELETE")
                            .uri(format!("/api/v1/tenants/{}/members/{}", tid, uid))
                            .header(AUTHORIZATION, format!("Bearer {}", token))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                resp.status()
            });
        }

        let mut no_content_count = 0;
        let mut not_found_count = 0;
        let mut unexpected = Vec::new();

        while let Some(res) = join_set.join_next().await {
            let status = res.unwrap();
            if status == StatusCode::NO_CONTENT {
                no_content_count += 1;
            } else if status == StatusCode::NOT_FOUND {
                not_found_count += 1;
            } else {
                unexpected.push(status);
            }
        }

        println!(
            "Concurrent Deletion Results: no_content={}, not_found={}, unexpected={:?}",
            no_content_count, not_found_count, unexpected
        );

        assert!(
            unexpected.is_empty(),
            "Concurrent deletions must only return 204 or 404, never 500: {:?}",
            unexpected
        );
        assert!(
            no_content_count >= 1,
            "At least one delete request must succeed with 204 No Content"
        );

        // Verify Target is completely removed from DB
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM memberships WHERE tenant_id = ?1 AND user_id = ?2",
        )
        .bind(tenant_id)
        .bind(&target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0, "Target member must be removed from DB");
    }

    #[tokio::test]
    async fn challenge_concurrent_member_update_vs_deletion_race() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_owner_id, token_owner) =
            create_test_user(&pool, &jwt, "race_owner@test.com", "Owner").await;
        let (_admin_id, token_admin) =
            create_test_user(&pool, &jwt, "race_admin@test.com", "Admin").await;
        let (target_id, _token_target) =
            create_test_user(&pool, &jwt, "race_target@test.com", "Target Staff").await;

        // Owner creates workspace
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Race Update ws", "slug": "race-upd-ws" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Invite Admin and Target Staff
        for (email, role) in [
            ("race_admin@test.com", "administrator"),
            ("race_target@test.com", "staff"),
        ] {
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({ "email": email, "role": role }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        // Fire concurrent Update (Owner updating target to manager) vs Delete (Admin deleting target)
        let app1 = app.clone();
        let tid1 = tenant_id.to_string();
        let uid1 = target_id.clone();
        let tok1 = token_owner.clone();
        let handle_update = tokio::spawn(async move {
            app1.oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/tenants/{}/members/{}", tid1, uid1))
                    .header(AUTHORIZATION, format!("Bearer {}", tok1))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "role": "manager" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
        });

        let app2 = app.clone();
        let tid2 = tenant_id.to_string();
        let uid2 = target_id.clone();
        let tok2 = token_admin.clone();
        let handle_delete = tokio::spawn(async move {
            app2.oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/tenants/{}/members/{}", tid2, uid2))
                    .header(AUTHORIZATION, format!("Bearer {}", tok2))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
        });

        let (upd_status, del_status) = tokio::join!(handle_update, handle_delete);
        let upd_status = upd_status.unwrap();
        let del_status = del_status.unwrap();

        println!(
            "Update vs Delete Race Results: Update={:?}, Delete={:?}",
            upd_status, del_status
        );

        // Update can be OK (200) or NOT_FOUND (404) if delete completed first
        assert!(
            upd_status == StatusCode::OK || upd_status == StatusCode::NOT_FOUND,
            "Update must return 200 or 404, got {:?}",
            upd_status
        );

        // Delete can be NO_CONTENT (204) or NOT_FOUND (404)
        assert!(
            del_status == StatusCode::NO_CONTENT || del_status == StatusCode::NOT_FOUND,
            "Delete must return 204 or 404, got {:?}",
            del_status
        );
    }

    #[tokio::test]
    async fn challenge_concurrent_unauthorized_manipulation_stampede() {
        let (app, jwt, pool, _dir, _auth) = setup_challenger_app().await;

        let (_owner_id, token_owner) =
            create_test_user(&pool, &jwt, "stamp_owner@test.com", "Owner").await;
        let (target_id, _token_target) =
            create_test_user(&pool, &jwt, "stamp_target@test.com", "Target Staff").await;

        // Owner creates workspace
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Unauthorized Stampede WS", "slug": "unauth-stamp-ws" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Invite Target Staff and 5 regular Staff members
        let mut staff_tokens = Vec::new();
        for i in 0..5 {
            let email = format!("internal_staff_{}@test.com", i);
            let (_sid, token) = create_test_user(&pool, &jwt, &email, "Staff Member").await;
            staff_tokens.push(token);

            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({ "email": email, "role": "staff" }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        // Create 5 Outsider users (non-members)
        let mut outsider_tokens = Vec::new();
        for i in 0..5 {
            let email = format!("outsider_{}@test.com", i);
            let (_oid, token) = create_test_user(&pool, &jwt, &email, "Outsider").await;
            outsider_tokens.push(token);
        }

        // Fire 10 concurrent requests: 5 staff attempts (expect 403), 5 outsider attempts (expect 404)
        let mut join_set: JoinSet<(String, StatusCode)> = JoinSet::new();

        // Staff attempts: PUT /members (role update)
        for token in staff_tokens {
            let app_clone = app.clone();
            let tid = tenant_id.to_string();
            let uid = target_id.clone();
            join_set.spawn(async move {
                let resp = app_clone
                    .oneshot(
                        Request::builder()
                            .method("PUT")
                            .uri(format!("/api/v1/tenants/{}/members/{}", tid, uid))
                            .header(AUTHORIZATION, format!("Bearer {}", token))
                            .header(CONTENT_TYPE, "application/json")
                            .body(Body::from(json!({ "role": "manager" }).to_string()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                ("staff".to_string(), resp.status())
            });
        }

        // Outsider attempts: DELETE /members
        for token in outsider_tokens {
            let app_clone = app.clone();
            let tid = tenant_id.to_string();
            let uid = target_id.clone();
            join_set.spawn(async move {
                let resp = app_clone
                    .oneshot(
                        Request::builder()
                            .method("DELETE")
                            .uri(format!("/api/v1/tenants/{}/members/{}", tid, uid))
                            .header(AUTHORIZATION, format!("Bearer {}", token))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                ("outsider".to_string(), resp.status())
            });
        }

        let mut staff_forbidden = 0;
        let mut outsider_not_found = 0;
        let mut failures = Vec::new();

        while let Some(res) = join_set.join_next().await {
            let (caller_type, status) = res.unwrap();
            match (caller_type.as_str(), status) {
                ("staff", StatusCode::FORBIDDEN) => staff_forbidden += 1,
                ("outsider", StatusCode::NOT_FOUND) => outsider_not_found += 1,
                _ => failures.push((caller_type, status)),
            }
        }

        println!(
            "Unauthorized Stampede Results: staff_403={}, outsider_404={}, failures={:?}",
            staff_forbidden, outsider_not_found, failures
        );

        assert_eq!(
            staff_forbidden, 5,
            "100% of internal unauthorized requests must return 403 Forbidden"
        );
        assert_eq!(
            outsider_not_found, 5,
            "100% of outsider requests must return 404 Not Found"
        );
        assert!(
            failures.is_empty(),
            "No unexpected failures or 500s: {:?}",
            failures
        );
    }
}
