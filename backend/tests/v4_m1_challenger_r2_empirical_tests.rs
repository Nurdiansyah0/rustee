//! Empirical Adversarial Challenger Suite for Milestone 1 Round 2 (R1)
//!
//! Identity: teamwork_preview_challenger_m1_r2_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Anti-enumeration and tenant isolation: Test cross-tenant access via header
//!    (X-Tenant-ID spoofing), path manipulation, and repository direct calls.
//!    Verify that foreign tenant entities strictly return HTTP 404 Not Found.
//! 2. TenantContext extractor stress test: Test all extractor resolution paths
//!    (header, path, personal fallback, invalid header 400, inactive membership 403/404).
//! 3. Sole owner guard: Verify sole workspace owner cannot remove self or demote self
//!    without transferring ownership (HTTP 409 Conflict with SOLE_OWNER_LEAVE / SOLE_OWNER_DEMOTION).

#[cfg(test)]
mod r2_empirical_tests {
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{
            header::{AUTHORIZATION, CONTENT_TYPE},
            Request, StatusCode,
        },
    };
    use backend::api::{create_app, AppState, AuthState};
    use backend::domain::tenant::{Role, TenantContext, TenantStatus};
    use backend::error::Rfc7807Error;
    use backend::repository::{
        account_repo::SqlxAccountRepository,
        audit_repo::SqlxAuditRepository,
        category_repo::SqlxCategoryRepository,
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        subscription_repo::SqlxSubscriptionRepository,
        tenant_repo::{
            BusinessProfileRepository, MembershipRepository, NewMembership, NewTenant,
            SqlxBusinessProfileRepository, SqlxMembershipRepository, SqlxTenantRepository,
            TenantRepository, UpsertBusinessProfile,
        },
        transaction_repo::SqlxTransactionRepository,
        user_repo::{NewUser, SqlxUserRepository, UserRepository},
        DbError,
    };
    use backend::service::{
        auth_service::AuthService,
        crypto::{Argon2Config, CryptoService},
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{PaymentConfig, PaymentService},
        tenant_service::TenantService,
    };
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tempfile::tempdir;
    use tower::ServiceExt;
    use uuid::Uuid;

    const TENANT_HEADER: &str = "x-tenant-id";

    struct TestFixture {
        app: axum::Router,
        state: AppState,
        jwt: Arc<JwtEngine>,
        pool: sqlx::SqlitePool,
        _dir: tempfile::TempDir,
        tenant_service: Arc<TenantService>,
    }

    async fn setup_fixture() -> TestFixture {
        let dir = tempdir().expect("Failed to create tempdir");
        let db_path = dir.path().join("v4_m1_challenger_r2.sqlite");
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

        let jwt_secret = "m1_r2_challenger_secret_jwt_key_9876543210";
        let jwt = Arc::new(JwtEngine::new(jwt_secret, 3600));

        let crypto_service =
            Arc::new(CryptoService::new(Argon2Config::fast_for_testing()).unwrap());

        let auth = Arc::new(AuthService::new(
            pool.clone(),
            user_repo.clone(),
            crypto_service,
            jwt.clone(),
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
            auth_service: auth.clone(),
            secure_cookie: false,
        };

        let tenant_repo = Arc::new(SqlxTenantRepository::new(pool.clone()));
        let tenant_service = Arc::new(TenantService::new_with_pool(pool.clone()));

        let state = AppState {
            auth_state,
            account_repo,
            category_repo,
            user_preferences_repo: Arc::new(
                backend::repository::SqlxUserPreferencesRepository::new(pool.clone()),
            ),
            ledger_service,
            payment_service,
            tenant_service: tenant_service.clone(),
            tenant_repo,
            pool: pool.clone(),
            rate_limiter: Arc::default(),
        };

        // Context test probe route
        let test_router = axum::Router::new()
            .route(
                "/api/v1/context-echo",
                axum::routing::get(|ctx: TenantContext| async move {
                    axum::Json(json!({
                        "tenant_id": ctx.tenant_id_str(),
                        "actor_id": ctx.actor_id_str(),
                        "role": ctx.role.as_str(),
                    }))
                }),
            )
            .with_state(state.clone());

        let app = create_app(state.clone()).merge(test_router);

        TestFixture {
            app,
            state,
            jwt,
            pool,
            _dir: dir,
            tenant_service,
        }
    }

    async fn create_user(
        fixture: &TestFixture,
        email: &str,
        display_name: &str,
    ) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let user_repo = SqlxUserRepository::new(fixture.pool.clone());
        user_repo
            .create(&NewUser {
                id: user_id.clone(),
                email: email.to_string(),
                password_hash: "pass_hash".to_string(),
                display_name: display_name.to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some("free".to_string()),
            })
            .await
            .expect("Create user failed");

        let (token, _) = fixture
            .jwt
            .generate_token(&user_id, email, "user", "free")
            .expect("Generate token failed");

        (user_id, token)
    }

    async fn create_workspace(
        fixture: &TestFixture,
        token: &str,
        name: &str,
        slug: &str,
    ) -> (String, Value) {
        let res = fixture
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": name, "slug": slug }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .expect("Request failed");

        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let id = body["id"].as_str().unwrap().to_string();
        (id, body)
    }

    // =========================================================================
    // MANDATORY SCOPE 1: Anti-Enumeration & Tenant Isolation
    // =========================================================================

    #[tokio::test]
    async fn challenge_header_x_tenant_id_spoofing_strictly_returns_404() {
        let fix = setup_fixture().await;

        let (_alice_id, token_alice) = create_user(&fix, "alice_s1@a.id", "Alice").await;
        let (_bob_id, token_bob) = create_user(&fix, "bob_s1@b.id", "Bob").await;

        let (tenant_a_id, _) = create_workspace(&fix, &token_alice, "Workspace A", "ws-a").await;
        let non_existent_uuid = Uuid::new_v4().to_string();

        // 1. Bob tries to access context-echo passing Alice's tenant ID
        let res_spoof = fix
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/context-echo")
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .header(TENANT_HEADER, &tenant_a_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            res_spoof.status(),
            StatusCode::NOT_FOUND,
            "Spoofed X-Tenant-ID must strictly return 404 Not Found"
        );
        let body_spoof: Rfc7807Error =
            serde_json::from_slice(&res_spoof.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body_spoof.status, 404);
        assert_eq!(body_spoof.code, "NOT_FOUND");
        assert_eq!(body_spoof.detail, "Workspace not found");

        // 2. Bob tries to access context-echo passing non-existent UUID
        let res_missing = fix
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/context-echo")
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .header(TENANT_HEADER, &non_existent_uuid)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res_missing.status(), StatusCode::NOT_FOUND);
        let body_missing: Rfc7807Error =
            serde_json::from_slice(&res_missing.into_body().collect().await.unwrap().to_bytes())
                .unwrap();

        // Anti-enumeration equivalence: body structure and content must be indistinguishable
        assert_eq!(body_spoof, body_missing);
    }

    #[tokio::test]
    async fn challenge_path_manipulation_anti_enumeration() {
        let fix = setup_fixture().await;

        let (_alice_id, token_alice) = create_user(&fix, "alice_s2@a.id", "Alice").await;
        let (_bob_id, token_bob) = create_user(&fix, "bob_s2@b.id", "Bob").await;

        let (tenant_a_id, _) = create_workspace(&fix, &token_alice, "Workspace A2", "ws-a2").await;

        // Test all tenant endpoints with foreign tenant ID accessed by Bob:
        // Every single endpoint MUST return 404 (NEVER 403, 500, or 200).
        let test_cases = vec![
            (
                "GET",
                format!("/api/v1/tenants/{}", tenant_a_id),
                Body::empty(),
            ),
            (
                "GET",
                format!("/api/v1/tenants/{}/profile", tenant_a_id),
                Body::empty(),
            ),
            (
                "PUT",
                format!("/api/v1/tenants/{}/profile", tenant_a_id),
                Body::from(json!({ "business_name": "Tampered Name" }).to_string()),
            ),
            (
                "GET",
                format!("/api/v1/tenants/{}/members", tenant_a_id),
                Body::empty(),
            ),
            (
                "POST",
                format!("/api/v1/tenants/{}/members", tenant_a_id),
                Body::from(json!({ "email": "attacker@evil.id", "role": "staff" }).to_string()),
            ),
            (
                "PUT",
                format!("/api/v1/tenants/{}/members/{}", tenant_a_id, Uuid::new_v4()),
                Body::from(json!({ "role": "manager" }).to_string()),
            ),
            (
                "DELETE",
                format!("/api/v1/tenants/{}/members/{}", tenant_a_id, Uuid::new_v4()),
                Body::empty(),
            ),
            // Slug based path resolution
            ("GET", "/api/v1/tenants/ws-a2".to_string(), Body::empty()),
            (
                "GET",
                "/api/v1/tenants/ws-a2/members".to_string(),
                Body::empty(),
            ),
            // Path traversal attempts
            (
                "GET",
                "/api/v1/tenants/..%2f..%2fetc%2fpasswd".to_string(),
                Body::empty(),
            ),
            (
                "GET",
                "/api/v1/tenants/'%20OR%201=1--".to_string(),
                Body::empty(),
            ),
            (
                "GET",
                "/api/v1/tenants/nonexistent-random-slug-999".to_string(),
                Body::empty(),
            ),
        ];

        for (method, uri, body) in test_cases {
            let res = fix
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(&uri)
                        .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                        .header(CONTENT_TYPE, "application/json")
                        .body(body)
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::NOT_FOUND,
                "Cross-tenant path {} {} must return 404 Not Found, got {}",
                method,
                uri,
                res.status()
            );

            let body_val: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(body_val["code"], "NOT_FOUND");
        }
    }

    #[tokio::test]
    async fn challenge_repository_direct_calls_isolation() {
        let fix = setup_fixture().await;

        let (user_a_id, _) = create_user(&fix, "repo_a@x.id", "User A").await;
        let (user_b_id, _) = create_user(&fix, "repo_b@x.id", "User B").await;

        let tenant_a_uuid = Uuid::new_v4();
        let tenant_b_uuid = Uuid::new_v4();
        let user_a_uuid = Uuid::parse_str(&user_a_id).unwrap();
        let user_b_uuid = Uuid::parse_str(&user_b_id).unwrap();

        let tenant_repo = SqlxTenantRepository::new(fix.pool.clone());
        let membership_repo = SqlxMembershipRepository::new(fix.pool.clone());
        let profile_repo = SqlxBusinessProfileRepository::new(fix.pool.clone());

        // Provision Tenant A and Tenant B
        tenant_repo
            .create_tenant(&NewTenant {
                id: tenant_a_uuid.to_string(),
                name: "Tenant Alpha Repo".into(),
                slug: "tenant-alpha-repo".into(),
                status: Some(TenantStatus::Active),
                is_personal: false,
            })
            .await
            .unwrap();

        tenant_repo
            .create_tenant(&NewTenant {
                id: tenant_b_uuid.to_string(),
                name: "Tenant Beta Repo".into(),
                slug: "tenant-beta-repo".into(),
                status: Some(TenantStatus::Active),
                is_personal: false,
            })
            .await
            .unwrap();

        membership_repo
            .create_membership(&NewMembership {
                id: format!("mem_{}", Uuid::new_v4()),
                tenant_id: tenant_a_uuid.to_string(),
                user_id: user_a_id.clone(),
                role: Role::Owner,
            })
            .await
            .unwrap();

        membership_repo
            .create_membership(&NewMembership {
                id: format!("mem_{}", Uuid::new_v4()),
                tenant_id: tenant_b_uuid.to_string(),
                user_id: user_b_id.clone(),
                role: Role::Owner,
            })
            .await
            .unwrap();

        let ctx_a = TenantContext::new(tenant_a_uuid, user_a_uuid, Role::Owner);
        let ctx_b = TenantContext::new(tenant_b_uuid, user_b_uuid, Role::Owner);

        // Seed profile for A
        profile_repo
            .upsert_profile(
                &ctx_a,
                &UpsertBusinessProfile {
                    business_name: "Original Alpha Corp".into(),
                    legal_name: None,
                    tax_id: None,
                    address: None,
                    phone: None,
                    email: None,
                    timezone: Some("Asia/Jakarta".into()),
                    currency: Some("IDR".into()),
                    locale: Some("id-ID".into()),
                    invoice_prefix: Some("INV".into()),
                    business_type: Some("general".into()),
                },
            )
            .await
            .unwrap();

        // 1. User A lists tenants: only sees Tenant A
        let user_a_tenants = tenant_repo.list_user_tenants(&user_a_id).await.unwrap();
        assert_eq!(user_a_tenants.len(), 1);
        assert_eq!(user_a_tenants[0].tenant.id, tenant_a_uuid.to_string());

        // 2. Direct membership query across tenants: Context A cannot see User B
        let members_a = membership_repo.list_members(&ctx_a).await.unwrap();
        assert_eq!(members_a.len(), 1);
        assert_eq!(members_a[0].user_id, user_a_id);

        // 3. Direct membership mutation across tenants: Context A cannot mutate User B in Tenant B
        let err_update = membership_repo
            .update_role(&ctx_a, &user_b_id, Role::Staff)
            .await
            .unwrap_err();
        assert!(matches!(err_update, DbError::NotFound));

        // 4. Direct membership deletion across tenants: Context A cannot delete User B
        let err_delete = membership_repo
            .delete_membership(&ctx_a, &user_b_id)
            .await
            .unwrap_err();
        assert!(matches!(err_delete, DbError::NotFound));

        // 5. Context B profile access: Tenant B has no profile yet (Ok(None))
        let profile_b = profile_repo.get_profile(&ctx_b).await.unwrap();
        assert!(
            profile_b.is_none(),
            "Tenant B must not see Tenant A's profile"
        );

        // 6. Direct profile mutation across tenants: Context B upserting does not affect Tenant A
        profile_repo
            .upsert_profile(
                &ctx_b,
                &UpsertBusinessProfile {
                    business_name: "Beta Corp".into(),
                    legal_name: None,
                    tax_id: None,
                    address: None,
                    phone: None,
                    email: None,
                    timezone: Some("Asia/Makassar".into()),
                    currency: Some("IDR".into()),
                    locale: Some("id-ID".into()),
                    invoice_prefix: Some("INV-B".into()),
                    business_type: Some("retail".into()),
                },
            )
            .await
            .unwrap();

        let profile_a_check = profile_repo.get_profile(&ctx_a).await.unwrap().unwrap();
        assert_eq!(profile_a_check.business_name, "Original Alpha Corp");
        assert_eq!(profile_a_check.timezone, "Asia/Jakarta");
    }

    // =========================================================================
    // MANDATORY SCOPE 2: TenantContext Extractor Stress Test
    // =========================================================================

    #[tokio::test]
    async fn challenge_extractor_all_resolution_paths() {
        let fix = setup_fixture().await;

        let (user_id, token) = create_user(&fix, "extractor_user@test.id", "Ext User").await;
        let now = chrono::Utc::now().to_rfc3339();

        // 1. Create a personal workspace for user
        let personal_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?1, 'Personal WS', 'personal-ext', 'ACTIVE', 1, ?2, ?2)",
        )
        .bind(&personal_id)
        .bind(&now)
        .execute(&fix.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'owner', 'ACTIVE', ?4, ?4)",
        )
        .bind(format!("mem_{}", Uuid::new_v4()))
        .bind(&personal_id)
        .bind(&user_id)
        .bind(&now)
        .execute(&fix.pool)
        .await
        .unwrap();

        // 2. Create a corporate workspace for user
        let corporate_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?1, 'Corporate WS', 'corp-ext', 'ACTIVE', 0, ?2, ?2)",
        )
        .bind(&corporate_id)
        .bind(&now)
        .execute(&fix.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'administrator', 'ACTIVE', ?4, ?4)",
        )
        .bind(format!("mem_{}", Uuid::new_v4()))
        .bind(&corporate_id)
        .bind(&user_id)
        .bind(&now)
        .execute(&fix.pool)
        .await
        .unwrap();

        // --- PATH A: Valid X-Tenant-ID header ---
        let req_a = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .header(TENANT_HEADER, &corporate_id)
            .body(Body::empty())
            .unwrap();
        let (mut parts_a, _) = req_a.into_parts();
        let ctx_a = TenantContext::from_request_parts(&mut parts_a, &fix.state)
            .await
            .expect("Path A must resolve corporate workspace");
        assert_eq!(ctx_a.tenant_id_str(), corporate_id);
        assert_eq!(ctx_a.role, Role::Administrator);

        // --- PATH B: Path-based resolution (/api/v1/tenants/{id}/...) ---
        let req_b = Request::builder()
            .uri(format!("/api/v1/tenants/{}/members", corporate_id))
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let (mut parts_b, _) = req_b.into_parts();
        let ctx_b = TenantContext::from_request_parts(&mut parts_b, &fix.state)
            .await
            .expect("Path B must resolve via path URI");
        assert_eq!(ctx_b.tenant_id_str(), corporate_id);

        // --- PATH C: Path-based resolution by slug (/api/v1/tenants/{slug}/...) ---
        let req_c = Request::builder()
            .uri("/api/v1/tenants/corp-ext/members")
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let (mut parts_c, _) = req_c.into_parts();
        let ctx_c = TenantContext::from_request_parts(&mut parts_c, &fix.state)
            .await
            .expect("Path C must resolve via slug");
        assert_eq!(ctx_c.tenant_id_str(), corporate_id);

        // --- PATH D: Personal fallback when header omitted and non-tenant path ---
        let req_d = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let (mut parts_d, _) = req_d.into_parts();
        let ctx_d = TenantContext::from_request_parts(&mut parts_d, &fix.state)
            .await
            .expect("Path D must fall back to personal workspace");
        assert_eq!(ctx_d.tenant_id_str(), personal_id);
        assert_eq!(ctx_d.role, Role::Owner);

        // --- PATH E: Invalid header format strictly returns 400 Bad Request ---
        let invalid_headers = vec![
            "invalid-uuid-string",
            "12345",
            "",
            "   ",
            "'; DROP TABLE memberships; --",
        ];
        for bad_h in invalid_headers {
            let req_e = Request::builder()
                .uri("/api/v1/accounts")
                .header(AUTHORIZATION, format!("Bearer {}", token))
                .header(TENANT_HEADER, bad_h)
                .body(Body::empty())
                .unwrap();
            let (mut parts_e, _) = req_e.into_parts();
            let err_e = TenantContext::from_request_parts(&mut parts_e, &fix.state)
                .await
                .unwrap_err();
            assert_eq!(
                err_e.status_code(),
                StatusCode::BAD_REQUEST,
                "Malformed header '{}' must return 400 Bad Request",
                bad_h
            );
        }

        // --- PATH F: Inactive membership returns 403 Forbidden on explicit header ---
        let suspended_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tenants (id, name, slug, status, is_personal, created_at, updated_at) VALUES (?1, 'Suspended Corp', 'susp-corp', 'ACTIVE', 0, ?2, ?2)",
        )
        .bind(&suspended_id)
        .bind(&now)
        .execute(&fix.pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'staff', 'SUSPENDED', ?4, ?4)",
        )
        .bind(format!("mem_{}", Uuid::new_v4()))
        .bind(&suspended_id)
        .bind(&user_id)
        .bind(&now)
        .execute(&fix.pool)
        .await
        .unwrap();

        let req_f = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .header(TENANT_HEADER, &suspended_id)
            .body(Body::empty())
            .unwrap();
        let (mut parts_f, _) = req_f.into_parts();
        let err_f = TenantContext::from_request_parts(&mut parts_f, &fix.state)
            .await
            .unwrap_err();
        assert_eq!(
            err_f.status_code(),
            StatusCode::FORBIDDEN,
            "Inactive membership on header must return 403 Forbidden"
        );

        // --- PATH G: Inactive membership returns 404 Not Found on path resolution (anti-enumeration) ---
        let req_g = Request::builder()
            .uri(format!("/api/v1/tenants/{}/members", suspended_id))
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let (mut parts_g, _) = req_g.into_parts();
        let err_g = TenantContext::from_request_parts(&mut parts_g, &fix.state)
            .await
            .unwrap_err();
        assert_eq!(
            err_g.status_code(),
            StatusCode::NOT_FOUND,
            "Inactive membership on path must return 404 Not Found to prevent enumeration"
        );

        // --- PATH H: Fallback when user has zero workspaces returns 404 ---
        let (_no_ws_id, token_no_ws) = create_user(&fix, "noworkspace@corp.id", "No WS").await;
        let req_h = Request::builder()
            .uri("/api/v1/accounts")
            .header(AUTHORIZATION, format!("Bearer {}", token_no_ws))
            .body(Body::empty())
            .unwrap();
        let (mut parts_h, _) = req_h.into_parts();
        let err_h = TenantContext::from_request_parts(&mut parts_h, &fix.state)
            .await
            .unwrap_err();
        assert_eq!(err_h.status_code(), StatusCode::NOT_FOUND);
    }

    // =========================================================================
    // MANDATORY SCOPE 3: Sole Owner Guard Stress Test
    // =========================================================================

    #[tokio::test]
    async fn challenge_sole_owner_cannot_leave_or_demote_self() {
        let fix = setup_fixture().await;

        let (owner_id, token_owner) = create_user(&fix, "sole_owner@guard.id", "Sole Owner").await;
        let (admin_id, token_admin) = create_user(&fix, "admin_user@guard.id", "Admin User").await;
        let (_staff_id, token_staff) = create_user(&fix, "staff_user@guard.id", "Staff User").await;

        // Owner creates workspace
        let (tenant_id, _) =
            create_workspace(&fix, &token_owner, "Sole Guard Corp", "sole-guard-corp").await;

        // Owner invites Admin and Staff
        for (email, role) in [
            ("admin_user@guard.id", "administrator"),
            ("staff_user@guard.id", "staff"),
        ] {
            let inv_res = fix
                .app
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
            assert_eq!(inv_res.status(), StatusCode::CREATED);
        }

        // 1. Sole owner attempts self-removal via HTTP DELETE -> Strictly 409 Conflict (SOLE_OWNER_LEAVE)
        let res_self_remove = fix
            .app
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

        assert_eq!(
            res_self_remove.status(),
            StatusCode::CONFLICT,
            "Sole owner self-removal must strictly return 409 Conflict"
        );
        let body_leave: Rfc7807Error = serde_json::from_slice(
            &res_self_remove
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body_leave.code, "SOLE_OWNER_LEAVE");
        assert_eq!(body_leave.status, 409);

        // Verify Owner is still active in database
        let owner_mem: Option<String> = sqlx::query_scalar(
            "SELECT role FROM memberships WHERE tenant_id = ?1 AND user_id = ?2 AND status = 'ACTIVE'",
        )
        .bind(&tenant_id)
        .bind(&owner_id)
        .fetch_optional(&fix.pool)
        .await
        .unwrap();
        assert_eq!(owner_mem.as_deref(), Some("owner"));

        // 2. Sole owner attempts to demote self to administrator -> Strictly 409 Conflict (SOLE_OWNER_DEMOTION)
        let res_demote_admin = fix
            .app
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
                    .body(Body::from(json!({ "role": "administrator" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            res_demote_admin.status(),
            StatusCode::CONFLICT,
            "Sole owner demotion must strictly return 409 Conflict"
        );
        let body_demote: Rfc7807Error = serde_json::from_slice(
            &res_demote_admin
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body_demote.code, "SOLE_OWNER_DEMOTION");

        // 3. Sole owner attempts to demote self to staff -> Strictly 409 Conflict
        let res_demote_staff = fix
            .app
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
        assert_eq!(res_demote_staff.status(), StatusCode::CONFLICT);

        // 4. Admin attempts to remove owner -> Strictly 403 Forbidden
        let res_admin_remove_owner = fix
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, owner_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_admin))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res_admin_remove_owner.status(),
            StatusCode::FORBIDDEN,
            "Admin cannot remove workspace owner"
        );

        // 5. Staff attempts to remove admin -> Strictly 403 Forbidden
        let res_staff_remove_admin = fix
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!(
                        "/api/v1/tenants/{}/members/{}",
                        tenant_id, admin_id
                    ))
                    .header(AUTHORIZATION, format!("Bearer {}", token_staff))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res_staff_remove_admin.status(),
            StatusCode::FORBIDDEN,
            "Staff cannot remove any workspace member"
        );

        // 6. Direct Service Call Stress: Verify domain invariant at service layer
        let owner_uuid = Uuid::parse_str(&owner_id).unwrap();
        let tenant_uuid = Uuid::parse_str(&tenant_id).unwrap();
        let ctx = TenantContext::new(tenant_uuid, owner_uuid, Role::Owner);

        let err_svc_demote = fix
            .tenant_service
            .update_member_role(&ctx, &owner_id, Role::Staff)
            .await
            .unwrap_err();
        assert!(format!("{:?}", err_svc_demote).contains("SOLE_OWNER_DEMOTION"));

        let err_svc_leave = fix
            .tenant_service
            .remove_member(&ctx, &owner_id)
            .await
            .unwrap_err();
        assert!(format!("{:?}", err_svc_leave).contains("SOLE_OWNER_LEAVE"));

        // 7. Multi-Owner Contrast Verification:
        // When a second owner is introduced (e.g. via direct DB record for co-ownership),
        // Owner 1 CAN safely leave without violation.
        let (co_owner_id, _token_co) = create_user(&fix, "co_owner@guard.id", "Co-Owner").await;
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'owner', 'ACTIVE', ?4, ?4)",
        )
        .bind(format!("mem_{}", Uuid::new_v4()))
        .bind(&tenant_id)
        .bind(&co_owner_id)
        .bind(&now)
        .execute(&fix.pool)
        .await
        .unwrap();

        // Now that owner_count == 2, Owner 1 leaving succeeds!
        let res_co_owner_leave = fix
            .app
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
        assert_eq!(
            res_co_owner_leave.status(),
            StatusCode::NO_CONTENT,
            "Non-sole owner can voluntarily leave when another owner exists"
        );
    }
}
