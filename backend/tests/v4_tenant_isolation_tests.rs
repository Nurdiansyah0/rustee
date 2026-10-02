//! Milestone 1 Verification Suite: Multi-Tenant Architecture & Identity Boundaries
//!
//! Validates:
//! - Cross-tenant resource queries strictly return HTTP 404 Not Found (anti-enumeration)
//! - Tenant-internal unauthorized role actions strictly return HTTP 403 Forbidden
//! - Reserved system slug rejection with HTTP 400 Bad Request
//! - Indonesian localization defaults (IDR, id-ID, Asia/Jakarta, INV)
//! - Registration auto-provisioning of default personal workspace
//! - Idempotent backfill and middleware fallback

#[cfg(test)]
mod v4_tenant_isolation_tests {
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
        auth_service::{AuthService, RegisterRequest},
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{PaymentConfig, PaymentService},
    };
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tempfile::tempdir;
    use tower::ServiceExt;

    async fn setup_test_env_with_state() -> (
        axum::Router,
        AppState,
        Arc<JwtEngine>,
        sqlx::SqlitePool,
        tempfile::TempDir,
        Arc<AuthService>,
    ) {
        let dir = tempdir().expect("Failed to create tempdir");
        let db_path = dir.path().join("v4_isolation_test.sqlite");
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

        let jwt_secret = "m1_isolation_testing_jwt_secret_key_1234567890";
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

        let app = create_app(state.clone());
        (app, state, jwt_engine, pool, dir, auth_service)
    }

    async fn setup_test_env() -> (
        axum::Router,
        Arc<JwtEngine>,
        sqlx::SqlitePool,
        tempfile::TempDir,
        Arc<AuthService>,
    ) {
        let (app, _state, jwt_engine, pool, dir, auth_service) = setup_test_env_with_state().await;
        (app, jwt_engine, pool, dir, auth_service)
    }

    async fn create_user(
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

    #[tokio::test]
    async fn test_cross_tenant_isolation_strictly_returns_404() {
        let (app, jwt, pool, _dir, _auth) = setup_test_env().await;

        let (_user_a, token_a) = create_user(&pool, &jwt, "owner_a@example.com", "Alice").await;
        let (_user_b, token_b) = create_user(&pool, &jwt, "owner_b@example.com", "Bob").await;

        // User A creates Tenant A
        let res_a = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_a))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Warung Alice",
                            "slug": "warung-alice"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_a.status(), StatusCode::CREATED);
        let body_a: Value =
            serde_json::from_slice(&res_a.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_a_id = body_a["id"].as_str().unwrap();

        // 1. Cross-Tenant GET /api/v1/tenants/:id by User B strictly returns 404 (NOT 403)
        let res_cross = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}", tenant_a_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_b))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res_cross.status(),
            StatusCode::NOT_FOUND,
            "Cross-tenant access must return 404 to prevent enumeration"
        );
        let body_cross: Value =
            serde_json::from_slice(&res_cross.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body_cross["code"], "NOT_FOUND");

        // 2. Cross-Tenant GET /api/v1/tenants/:id/profile strictly returns 404
        let res_cross_profile = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_a_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_b))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_cross_profile.status(), StatusCode::NOT_FOUND);

        // 3. Cross-Tenant GET /api/v1/tenants/:id/members strictly returns 404
        let res_cross_members = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_a_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_b))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_cross_members.status(), StatusCode::NOT_FOUND);

        // 4. Non-existent UUID also returns 404 with identical error code
        let random_uuid = uuid::Uuid::new_v4().to_string();
        let res_nonexistent = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}", random_uuid))
                    .header(AUTHORIZATION, format!("Bearer {}", token_b))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_nonexistent.status(), StatusCode::NOT_FOUND);
        let body_nonexistent: Value = serde_json::from_slice(
            &res_nonexistent
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body_nonexistent["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn test_tenant_internal_rbac_enforcement() {
        let (app, jwt, pool, _dir, _auth) = setup_test_env().await;

        let (_owner, token_owner) = create_user(&pool, &jwt, "owner@corp.com", "Owner").await;
        let (_staff, token_staff) =
            create_user(&pool, &jwt, "staff@corp.com", "Staff Member").await;

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
                        json!({
                            "name": "Koperasi Bersama",
                            "slug": "koperasi-bersama"
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

        // Owner invites Staff
        let invite_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "staff@corp.com",
                            "role": "staff"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invite_res.status(), StatusCode::CREATED);

        // Staff can READ tenant details and profile
        let staff_read_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_staff))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(staff_read_res.status(), StatusCode::OK);

        let staff_profile_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_staff))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(staff_profile_res.status(), StatusCode::OK);

        // Staff attempting to UPDATE profile strictly returns 403 Forbidden
        let staff_update_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_staff))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "business_name": "Hacked Business Name"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            staff_update_res.status(),
            StatusCode::FORBIDDEN,
            "Unauthorized role action within tenant must return 403"
        );
        let body_forbidden: Value = serde_json::from_slice(
            &staff_update_res
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body_forbidden["code"], "FORBIDDEN");

        // Staff attempting to INVITE member strictly returns 403 Forbidden
        let staff_invite_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_staff))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "other@corp.com",
                            "role": "staff"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(staff_invite_res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_reserved_slugs_and_formatting_rejection() {
        let (app, jwt, pool, _dir, _auth) = setup_test_env().await;
        let (_user, token) =
            create_user(&pool, &jwt, "slug_tester@example.com", "Slug Tester").await;

        let reserved = [
            "admin", "api", "app", "www", "support", "billing", "status", "docs",
        ];

        for slug in reserved {
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
                "Slug '{}' must be rejected",
                slug
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(body["code"], "RESERVED_SLUG");
        }

        // Malformed slugs: starting with hyphen, ending with hyphen, too short, spaces, etc.
        let invalid = [
            "-leading",
            "trailing-",
            "sh",
            "has space",
            "under_score",
            "double--hyphen",
        ];
        for slug in invalid {
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
                                "name": "Invalid Slug",
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
                "Invalid slug '{}' must return 400",
                slug
            );
        }
    }

    #[tokio::test]
    async fn test_indonesian_localization_defaults() {
        let (app, jwt, pool, _dir, _auth) = setup_test_env().await;
        let (_user, token) = create_user(&pool, &jwt, "indo_user@example.com", "Indo User").await;

        // Create tenant without overriding defaults
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
                            "name": "PT Maju Terus",
                            "slug": "pt-maju-terus"
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

        // Fetch business profile and assert Indonesian defaults
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

        assert_eq!(profile["currency"], "IDR");
        assert_eq!(profile["locale"], "id-ID");
        assert_eq!(profile["timezone"], "Asia/Jakarta");
        assert_eq!(profile["invoice_prefix"], "INV");

        // Update to WITA (Asia/Makassar) succeeds
        let update_wita = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "timezone": "Asia/Makassar"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_wita.status(), StatusCode::OK);
        let updated: Value =
            serde_json::from_slice(&update_wita.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(updated["timezone"], "Asia/Makassar");

        // Invalid timezone rejected with 400
        let update_invalid = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "timezone": "Invalid/Zone"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_invalid.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_registration_auto_provisions_default_personal_workspace() {
        let (app, _jwt, pool, _dir, auth_service) = setup_test_env().await;

        // Register new user via AuthService
        let auth_res = auth_service
            .register(RegisterRequest {
                email: "new_registered_user@example.com".to_string(),
                password: "Password123!".to_string(),
                display_name: "Budi Santoso".to_string(),
            })
            .await
            .expect("Registration should succeed");

        let user_id = &auth_res.user.id;
        let token = &auth_res.token;

        // Verify database state: exactly 1 tenant, 1 profile, 1 membership
        let tenant_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM memberships m JOIN tenants t ON t.id = m.tenant_id WHERE m.user_id = ?1 AND t.is_personal = 1"
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            tenant_count, 1,
            "User must have exactly 1 default personal workspace"
        );

        let membership_role: String =
            sqlx::query_scalar("SELECT role FROM memberships WHERE user_id = ?1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            membership_role, "owner",
            "User must be owner of personal workspace"
        );

        // List workspaces via API
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
        assert!(!list.as_array().unwrap().is_empty());
        assert_eq!(list[0]["role"], "owner");
        assert_eq!(list[0]["is_default"], true);

        // Access personal finance endpoint (e.g. GET /api/v1/accounts) without X-Tenant-ID header
        // Invariant: extractor automatically falls back to active personal workspace
        let acc_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/accounts")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            acc_res.status(),
            StatusCode::OK,
            "Legacy endpoints without header must resolve personal workspace"
        );
    }

    #[tokio::test]
    async fn test_migration_idempotent_backfill_for_existing_users() {
        let (_app, _jwt, pool, _dir, _auth) = setup_test_env().await;

        // Insert legacy user directly without workspace
        let legacy_user_id = "legacy_user_123";
        sqlx::query(
            r#"
            INSERT INTO users (id, email, password_hash, display_name, currency, role, subscription_tier, created_at, updated_at)
            VALUES (?1, 'legacy@example.com', 'hash', 'Legacy User', 'IDR', 'user', 'free', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
            "#,
        )
        .bind(legacy_user_id)
        .execute(&pool)
        .await
        .unwrap();

        // Run the backfill logic (identical to migration 0006)
        sqlx::query(
            r#"
            CREATE TEMP TABLE IF NOT EXISTS _test_migration_workspaces AS
            SELECT 
                u.id AS user_id,
                lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS tenant_id,
                lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS profile_id,
                lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS membership_id,
                'personal-' || lower(hex(randomblob(4))) AS slug,
                u.display_name,
                u.created_at,
                u.updated_at
            FROM users u
            WHERE NOT EXISTS (
                SELECT 1 FROM memberships m WHERE m.user_id = u.id
            );

            INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at)
            SELECT tenant_id, display_name || '''s Workspace', slug, 'ACTIVE', 1, created_at, updated_at
            FROM _test_migration_workspaces;

            INSERT INTO business_profiles (id, tenant_id, business_name, legal_name, timezone, currency, invoice_prefix, business_type, created_at, updated_at)
            SELECT profile_id, tenant_id, display_name, display_name, 'Asia/Jakarta', 'IDR', 'INV', 'personal', created_at, updated_at
            FROM _test_migration_workspaces;

            INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at)
            SELECT membership_id, tenant_id, user_id, 'owner', 'ACTIVE', created_at, updated_at
            FROM _test_migration_workspaces;

            DROP TABLE IF EXISTS _test_migration_workspaces;
            "#
        )
        .execute(&pool)
        .await
        .unwrap();

        // Verify legacy user now has workspace
        let legacy_memberships: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM memberships WHERE user_id = ?1 AND role = 'owner'",
        )
        .bind(legacy_user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(legacy_memberships, 1);

        // Re-running the backfill logic is idempotent (0 new rows)
        let before_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants")
            .fetch_one(&pool)
            .await
            .unwrap();

        sqlx::query(
            r#"
            CREATE TEMP TABLE IF NOT EXISTS _test_migration_workspaces2 AS
            SELECT 
                u.id AS user_id,
                lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS tenant_id,
                lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS profile_id,
                lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-a' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(6))) AS membership_id,
                'personal-' || lower(hex(randomblob(4))) AS slug,
                u.display_name,
                u.created_at,
                u.updated_at
            FROM users u
            WHERE NOT EXISTS (
                SELECT 1 FROM memberships m WHERE m.user_id = u.id
            );

            INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at)
            SELECT tenant_id, display_name || '''s Workspace', slug, 'ACTIVE', 1, created_at, updated_at
            FROM _test_migration_workspaces2;

            DROP TABLE IF EXISTS _test_migration_workspaces2;
            "#
        )
        .execute(&pool)
        .await
        .unwrap();

        let after_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            before_count, after_count,
            "Backfill must be completely idempotent"
        );
    }

    #[tokio::test]
    async fn test_tenant_context_extractor_explicit_invocations() {
        use axum::extract::FromRequestParts;
        use backend::api::middleware::tenant_extractor::TENANT_HEADER;
        use backend::domain::tenant::{Role, TenantContext};

        let (_app, state, jwt, pool, _dir, _auth) = setup_test_env_with_state().await;

        let (_alice_id, token_alice) =
            create_user(&pool, &jwt, "alice_extractor@corp.com", "Alice").await;
        let (_bob_id, token_bob) = create_user(&pool, &jwt, "bob_extractor@corp.com", "Bob").await;

        // 1. Create a business workspace for Alice
        let tenant_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?1, 'Alice Corp', 'alice-corp', 'ACTIVE', 0, ?2, ?2)",
        )
        .bind(&tenant_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'owner', 'ACTIVE', ?4, ?4)",
        )
        .bind(format!("mem_{}", uuid::Uuid::new_v4()))
        .bind(&tenant_id)
        .bind(&_alice_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        // --- Scenario A: Valid X-Tenant-ID header ---
        let req_valid = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token_alice))
            .header(TENANT_HEADER, &tenant_id)
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req_valid.into_parts();
        let ctx = TenantContext::from_request_parts(&mut parts, &state)
            .await
            .expect("Valid X-Tenant-ID header must resolve TenantContext");
        assert_eq!(ctx.tenant_id_str(), tenant_id);
        assert_eq!(ctx.role, Role::Owner);
        assert!(ctx.is_owner());

        // --- Scenario B: Non-member X-Tenant-ID header (strictly 404 anti-enumeration) ---
        let req_non_member = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token_bob))
            .header(TENANT_HEADER, &tenant_id)
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req_non_member.into_parts();
        let err_non_member = TenantContext::from_request_parts(&mut parts, &state)
            .await
            .unwrap_err();
        assert_eq!(err_non_member.status_code(), StatusCode::NOT_FOUND);

        // --- Scenario C: Malformed / Non-UUID X-Tenant-ID header (400) ---
        let req_malformed = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token_alice))
            .header(TENANT_HEADER, "not-a-valid-uuid")
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req_malformed.into_parts();
        let err_malformed = TenantContext::from_request_parts(&mut parts, &state)
            .await
            .unwrap_err();
        assert_eq!(err_malformed.status_code(), StatusCode::BAD_REQUEST);

        // --- Scenario D: Path-based resolution fallback without header ---
        let req_path = Request::builder()
            .uri(format!("/api/v1/tenants/{}/members", tenant_id))
            .header(AUTHORIZATION, format!("Bearer {}", token_alice))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req_path.into_parts();
        let ctx_path = TenantContext::from_request_parts(&mut parts, &state)
            .await
            .expect("Path-based /api/v1/tenants/{id} must resolve without header");
        assert_eq!(ctx_path.tenant_id_str(), tenant_id);

        // --- Scenario E: Personal workspace fallback without header on general route ---
        let personal_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?1, 'Alice Personal', 'alice-personal', 'ACTIVE', 1, ?2, ?2)",
        )
        .bind(&personal_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'owner', 'ACTIVE', ?4, ?4)",
        )
        .bind(format!("mem_{}", uuid::Uuid::new_v4()))
        .bind(&personal_id)
        .bind(&_alice_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let req_fallback = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token_alice))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req_fallback.into_parts();
        let ctx_fallback = TenantContext::from_request_parts(&mut parts, &state)
            .await
            .expect("Missing header must fall back to personal workspace");
        assert_eq!(ctx_fallback.tenant_id_str(), personal_id);

        // --- Scenario F: Fallback failure when user has zero workspaces ---
        let (_charlie_id, token_charlie) =
            create_user(&pool, &jwt, "charlie_noworkspace@corp.com", "Charlie").await;
        let req_no_workspace = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token_charlie))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req_no_workspace.into_parts();
        let err_no_ws = TenantContext::from_request_parts(&mut parts, &state)
            .await
            .unwrap_err();
        assert_eq!(err_no_ws.status_code(), StatusCode::NOT_FOUND);

        // --- Scenario G: Inactive membership returns 403 Forbidden on explicit header ---
        let suspended_tenant_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?1, 'Suspended Corp', 'suspended-corp', 'ACTIVE', 0, ?2, ?2)",
        )
        .bind(&suspended_tenant_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'staff', 'SUSPENDED', ?4, ?4)",
        )
        .bind(format!("mem_{}", uuid::Uuid::new_v4()))
        .bind(&suspended_tenant_id)
        .bind(&_bob_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let req_suspended = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token_bob))
            .header(TENANT_HEADER, &suspended_tenant_id)
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req_suspended.into_parts();
        let err_suspended = TenantContext::from_request_parts(&mut parts, &state)
            .await
            .unwrap_err();
        assert_eq!(err_suspended.status_code(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_member_role_update_endpoint_and_rbac_invariants() {
        let (app, jwt, pool, _dir, _auth) = setup_test_env().await;

        let (_owner_id, token_owner) =
            create_user(&pool, &jwt, "owner_updater@corp.com", "Owner User").await;
        let (_admin_id, _token_admin) =
            create_user(&pool, &jwt, "admin_updater@corp.com", "Admin User").await;
        let (staff_id, token_staff) =
            create_user(&pool, &jwt, "staff_target@corp.com", "Staff User").await;
        let (_outsider_id, token_outsider) =
            create_user(&pool, &jwt, "outsider@corp.com", "Outsider User").await;

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
                        json!({
                            "name": "Teamwork Systems",
                            "slug": "teamwork-systems"
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

        // Owner invites Admin and Staff
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "admin_updater@corp.com",
                            "role": "administrator"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "staff_target@corp.com",
                            "role": "staff"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 1. Owner updates staff to accountant -> 200 OK
        let res_owner_update = app
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
                    .body(Body::from(json!({ "role": "accountant" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_owner_update.status(), StatusCode::OK);
        let updated_body: Value = serde_json::from_slice(
            &res_owner_update
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(updated_body["role"], "accountant");

        // 2. Staff attempts to update member role -> 403 Forbidden
        let res_staff_update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, _admin_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_staff))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "role": "staff" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_staff_update.status(), StatusCode::FORBIDDEN);

        // 3. Privilege escalation guard: Cannot promote member to owner -> 400 Bad Request
        let res_promote_owner = app
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
                    .body(Body::from(json!({ "role": "owner" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_promote_owner.status(), StatusCode::BAD_REQUEST);

        // 4. Invalid role string rejected -> 400 Bad Request
        let res_invalid_role = app
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
                    .body(Body::from(json!({ "role": "superadmin" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_invalid_role.status(), StatusCode::BAD_REQUEST);

        // 5. Non-existent target user -> 404 Not Found
        let random_user_id = uuid::Uuid::new_v4().to_string();
        let res_missing_user = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, random_user_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "role": "staff" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_missing_user.status(), StatusCode::NOT_FOUND);

        // 6. Cross-tenant isolation -> 404 Not Found
        let res_cross = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, staff_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_outsider))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "role": "staff" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_cross.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_member_removal_endpoint_and_invariants() {
        let (app, jwt, pool, _dir, _auth) = setup_test_env().await;

        let (_owner_id, token_owner) =
            create_user(&pool, &jwt, "remover_owner@corp.com", "Owner").await;
        let (_admin_id, token_admin) =
            create_user(&pool, &jwt, "remover_admin@corp.com", "Admin").await;
        let (staff1_id, _token_staff1) =
            create_user(&pool, &jwt, "remover_staff1@corp.com", "Staff 1").await;
        let (staff2_id, _token_staff2) =
            create_user(&pool, &jwt, "remover_staff2@corp.com", "Staff 2").await;
        let (_outsider_id, token_outsider) =
            create_user(&pool, &jwt, "remover_outsider@corp.com", "Outsider").await;

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
                        json!({
                            "name": "Removal Corp",
                            "slug": "removal-corp"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let tenant_id = body["id"].as_str().unwrap();

        // Invite Admin, Staff 1, Staff 2
        for (email, role) in [
            ("remover_admin@corp.com", "administrator"),
            ("remover_staff1@corp.com", "staff"),
            ("remover_staff2@corp.com", "staff"),
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

        // 1. Admin removes Staff 1 -> 204 No Content
        let res_admin_remove = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, staff1_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_admin))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_admin_remove.status(), StatusCode::NO_CONTENT);

        // Verify Staff 1 is removed from database
        let staff1_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memberships WHERE tenant_id = ?1 AND user_id = ?2",
        )
        .bind(tenant_id)
        .bind(&staff1_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(staff1_count, 0);

        // 2. Owner removes Staff 2 -> 204 No Content
        let res_owner_remove = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, staff2_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_owner_remove.status(), StatusCode::NO_CONTENT);

        // 3. Voluntary self-removal (Admin leaves workspace) -> 204 No Content
        let res_self_leave = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, _admin_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_admin))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_self_leave.status(), StatusCode::NO_CONTENT);

        // 4. Critical Invariant: Admin CANNOT remove Owner -> 403 Forbidden
        let (admin2_id, token_admin2) =
            create_user(&pool, &jwt, "admin2@corp.com", "Admin 2").await;
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "email": "admin2@corp.com", "role": "administrator" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let res_admin_remove_owner = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, _owner_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_admin2))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_admin_remove_owner.status(), StatusCode::FORBIDDEN);

        // 5. Sole owner cannot leave without transferring ownership -> 409 Conflict
        let res_sole_owner_leave = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, _owner_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_sole_owner_leave.status(), StatusCode::CONFLICT);

        // 6. Cross-tenant isolation -> 404 Not Found
        let res_cross = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, admin2_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_outsider))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_cross.status(), StatusCode::NOT_FOUND);
    }
}
