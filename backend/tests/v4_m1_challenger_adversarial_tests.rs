//! Empirical Adversarial Challenger Test Suite for Milestone 1 (R1)
//!
//! Identity: teamwork_preview_challenger_m1_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Objectives:
//! 1. Verify cross-tenant isolation: any attempt to access, mutate, or query another tenant's entity
//!    strictly returns HTTP 404 Not Found (RFC 7807 problem details with code `NOT_FOUND`), preventing entity existence enumeration.
//! 2. Verify anti-enumeration equivalence: response from probing an existing foreign tenant ID is
//!    empirically identical in status code, problem details structure, and error code to probing a non-existent UUID.
//! 3. Verify tenant-internal authorization: staff/manager/accountant attempting administrative operations
//!    strictly returns HTTP 403 Forbidden (RFC 7807 problem details with code `FORBIDDEN`).
//! 4. Verify privilege escalation defenses: neither administrator nor owner can invite a member with role "owner".
//! 5. Verify direct service and repository layers: TenantService and Sqlx repositories enforce strict TenantContext isolation.
//! 6. Verify adversarial inputs: SQL injection, path traversal in IDs, malformed X-Tenant-ID headers, slug collisions, and reserved slugs.

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{
            header::{AUTHORIZATION, CONTENT_TYPE},
            Request, StatusCode,
        },
    };
    use backend::api::{create_app, AppState, AuthState};
    use backend::domain::tenant::{Role, Tenant, TenantContext, TenantStatus};
    use backend::error::Rfc7807Error;
    use backend::repository::{
        account_repo::SqlxAccountRepository,
        audit_repo::SqlxAuditRepository,
        category_repo::SqlxCategoryRepository,
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        subscription_repo::SqlxSubscriptionRepository,
        tenant_repo::{
            MembershipRepository, NewMembership, NewTenant, SqlxMembershipRepository,
            SqlxTenantRepository, TenantRepository,
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

    struct TestHarness {
        app: axum::Router,
        jwt: Arc<JwtEngine>,
        pool: sqlx::SqlitePool,
        _dir: tempfile::TempDir,
        _auth: Arc<AuthService>,
        tenant_service: Arc<TenantService>,
    }

    async fn setup_harness() -> TestHarness {
        let dir = tempdir().expect("Failed to create tempdir");
        let db_path = dir.path().join("v4_m1_challenger.sqlite");
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

        let jwt_secret = "m1_challenger_jwt_secret_token_123456789012345";
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

        let tenant_repo = Arc::new(backend::repository::tenant_repo::SqlxTenantRepository::new(
            pool.clone(),
        ));
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

        let test_router = axum::Router::new()
            .route(
                "/api/v1/test-context",
                axum::routing::get(|_ctx: TenantContext| async { StatusCode::OK }),
            )
            .with_state(state.clone());
        let app = create_app(state).merge(test_router);

        TestHarness {
            app,
            jwt,
            pool,
            _dir: dir,
            _auth: auth,
            tenant_service,
        }
    }

    async fn create_test_user(
        harness: &TestHarness,
        email: &str,
        display_name: &str,
    ) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let user_repo = SqlxUserRepository::new(harness.pool.clone());
        user_repo
            .create(&NewUser {
                id: user_id.clone(),
                email: email.to_string(),
                password_hash: "secret_hash".to_string(),
                display_name: display_name.to_string(),
                currency: Some("IDR".to_string()),
                role: Some("user".to_string()),
                subscription_tier: Some("free".to_string()),
            })
            .await
            .expect("Failed to create user");

        let (token, _) = harness
            .jwt
            .generate_token(&user_id, email, "user", "free")
            .expect("Failed to generate token");

        (user_id, token)
    }

    async fn create_workspace_api(
        harness: &TestHarness,
        token: &str,
        name: &str,
        slug: &str,
    ) -> (String, Value) {
        let res = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": name,
                            "slug": slug,
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .expect("Failed to execute request");

        assert_eq!(
            res.status(),
            StatusCode::CREATED,
            "Workspace creation failed"
        );
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let id = body["id"].as_str().expect("id must be string").to_string();
        (id, body)
    }

    // =========================================================================
    // SECTION 1: Cross-Tenant Isolation & Anti-Enumeration Equivalence
    // =========================================================================

    #[tokio::test]
    async fn challenge_cross_tenant_read_and_anti_enumeration_oracle() {
        let harness = setup_harness().await;

        let (_alice_id, token_alice) =
            create_test_user(&harness, "alice@alpha.id", "Alice Alpha").await;
        let (_bob_id, token_bob) = create_test_user(&harness, "bob@beta.id", "Bob Beta").await;

        let (tenant_alpha_id, _) = create_workspace_api(
            &harness,
            &token_alice,
            "Alpha Enterprise",
            "alpha-enterprise",
        )
        .await;
        let non_existent_id = Uuid::new_v4().to_string();

        // 1. Bob queries Alpha's workspace (Cross-Tenant)
        let res_cross = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}", tenant_alpha_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res_cross.status(), StatusCode::NOT_FOUND);
        let bytes_cross = res_cross.into_body().collect().await.unwrap().to_bytes();
        let prob_cross: Rfc7807Error =
            serde_json::from_slice(&bytes_cross).expect("Must parse as RFC 7807 error");

        // 2. Bob queries Non-Existent workspace
        let res_nonexistent = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}", non_existent_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res_nonexistent.status(), StatusCode::NOT_FOUND);
        let bytes_nonexistent = res_nonexistent
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let prob_nonexistent: Rfc7807Error =
            serde_json::from_slice(&bytes_nonexistent).expect("Must parse as RFC 7807 error");

        // ORACLE EMPIRICAL VERIFICATION:
        // Probing existing foreign tenant vs probing non-existent UUID must be indistinguishable
        assert_eq!(prob_cross.status, 404);
        assert_eq!(prob_cross.code, "NOT_FOUND");
        assert_eq!(prob_cross.title, "Not Found");
        assert_eq!(
            prob_cross.r#type,
            "https://api.nurdiansyahlabs.com/errors/not-found"
        );
        assert_eq!(prob_cross.detail, "Workspace not found");
        assert_eq!(
            prob_cross, prob_nonexistent,
            "Anti-enumeration violation: cross-tenant error differs from non-existent error!"
        );
    }

    #[tokio::test]
    async fn challenge_cross_tenant_profile_and_members_endpoints() {
        let harness = setup_harness().await;

        let (_alice_id, token_alice) =
            create_test_user(&harness, "alice2@alpha.id", "Alice Alpha 2").await;
        let (_bob_id, token_bob) = create_test_user(&harness, "bob2@beta.id", "Bob Beta 2").await;

        let (tenant_alpha_id, _) =
            create_workspace_api(&harness, &token_alice, "Alpha Corp", "alpha-corp").await;

        // 1. GET /profile cross-tenant
        let res_profile = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_alpha_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_profile.status(), StatusCode::NOT_FOUND);
        let body: Value =
            serde_json::from_slice(&res_profile.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["code"], "NOT_FOUND");

        // 2. GET /members cross-tenant
        let res_members = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_alpha_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_members.status(), StatusCode::NOT_FOUND);
        let body_m: Value =
            serde_json::from_slice(&res_members.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body_m["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn challenge_cross_tenant_mutations_strictly_return_404() {
        let harness = setup_harness().await;

        let (_alice_id, token_alice) =
            create_test_user(&harness, "alice_mut@alpha.id", "Alice").await;
        let (_bob_id, token_bob) = create_test_user(&harness, "bob_mut@beta.id", "Bob").await;

        let (tenant_alpha_id, _) =
            create_workspace_api(&harness, &token_alice, "Alpha Mutate", "alpha-mutate").await;

        // 1. Cross-tenant PUT /profile (attempt to overwrite company details)
        let res_put = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_alpha_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "business_name": "Tampered By Bob",
                            "tax_id": "00.000.000.0-000.000"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // INVARIANT: Cross-tenant mutation must return 404 NOT FOUND (NOT 403, NOT 200, NOT 400)
        assert_eq!(
            res_put.status(),
            StatusCode::NOT_FOUND,
            "Cross-tenant mutation must return 404"
        );
        let body_put: Value =
            serde_json::from_slice(&res_put.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body_put["code"], "NOT_FOUND");

        // Verify database integrity: Alpha profile was NOT altered
        let current_name: String =
            sqlx::query_scalar("SELECT business_name FROM business_profiles WHERE tenant_id = ?1")
                .bind(&tenant_alpha_id)
                .fetch_one(&harness.pool)
                .await
                .unwrap();
        assert_eq!(
            current_name, "Alpha Mutate",
            "Database was tampered across tenant boundaries!"
        );

        // 2. Cross-tenant POST /members (attempt to invite user into foreign tenant)
        let res_invite = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_alpha_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "attacker@darkweb.org",
                            "role": "staff"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            res_invite.status(),
            StatusCode::NOT_FOUND,
            "Cross-tenant member invitation must return 404"
        );
        let body_inv: Value =
            serde_json::from_slice(&res_invite.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body_inv["code"], "NOT_FOUND");

        // Verify membership count for Alpha is strictly 1 (Alice only)
        let mem_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM memberships WHERE tenant_id = ?1")
                .bind(&tenant_alpha_id)
                .fetch_one(&harness.pool)
                .await
                .unwrap();
        assert_eq!(mem_count, 1);
    }

    #[tokio::test]
    async fn challenge_header_x_tenant_id_spoofing_isolation() {
        let harness = setup_harness().await;

        let (_alice_id, token_alice) =
            create_test_user(&harness, "alice_hdr@alpha.id", "Alice").await;
        let (_bob_id, token_bob) = create_test_user(&harness, "bob_hdr@beta.id", "Bob").await;

        let (tenant_alpha_id, _) =
            create_workspace_api(&harness, &token_alice, "Alpha Scoped", "alpha-scoped").await;

        // Bob passes Alice's tenant ID in X-Tenant-ID header when hitting endpoint requiring TenantContext
        let res_spoof = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/test-context")
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .header("x-tenant-id", &tenant_alpha_id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Extractor rejects with 404 NOT_FOUND to prevent enumeration
        assert_eq!(
            res_spoof.status(),
            StatusCode::NOT_FOUND,
            "Spoofed X-Tenant-ID header must yield 404"
        );
        let body: Value =
            serde_json::from_slice(&res_spoof.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn challenge_inactive_membership_anti_enumeration() {
        let harness = setup_harness().await;

        let (_alice_id, token_alice) =
            create_test_user(&harness, "alice_inact@alpha.id", "Alice").await;
        let (charlie_id, token_charlie) =
            create_test_user(&harness, "charlie@alpha.id", "Charlie").await;

        let (tenant_alpha_id, _) =
            create_workspace_api(&harness, &token_alice, "Alpha Guarded", "alpha-guarded").await;

        // Insert membership for Charlie with status = 'SUSPENDED'
        let mem_id = format!("mem_{}", Uuid::new_v4());
        sqlx::query(
            r#"
            INSERT INTO memberships (id, tenant_id, user_id, role, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, 'staff', 'SUSPENDED', datetime('now'), datetime('now'))
            "#,
        )
        .bind(&mem_id)
        .bind(&tenant_alpha_id)
        .bind(&charlie_id)
        .execute(&harness.pool)
        .await
        .unwrap();

        // Suspended member attempting GET /tenants/:id strictly returns 404 (not active)
        let res_susp = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}", tenant_alpha_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_charlie))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_susp.status(), StatusCode::NOT_FOUND);

        // Suspended member attempting GET /profile strictly returns 404
        let res_susp_prof = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/tenants/{}/profile", tenant_alpha_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_charlie))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_susp_prof.status(), StatusCode::NOT_FOUND);
    }

    // =========================================================================
    // SECTION 2: Tenant-Internal RBAC Authorization (Strict HTTP 403 Forbidden)
    // =========================================================================

    #[tokio::test]
    async fn challenge_comprehensive_rbac_role_matrix() {
        let harness = setup_harness().await;

        let (_owner_id, token_owner) =
            create_test_user(&harness, "owner_mat@corp.id", "Owner").await;
        let (_admin_id, token_admin) =
            create_test_user(&harness, "admin_mat@corp.id", "Admin").await;
        let (_mgr_id, token_mgr) = create_test_user(&harness, "mgr_mat@corp.id", "Manager").await;
        let (_staff_id, token_staff) =
            create_test_user(&harness, "staff_mat@corp.id", "Staff").await;
        let (_acct_id, token_acct) =
            create_test_user(&harness, "acct_mat@corp.id", "Accountant").await;

        let (tenant_id, _) =
            create_workspace_api(&harness, &token_owner, "Matrix Corp", "matrix-corp").await;

        // Owner invites members with each role
        let roles = [
            ("admin_mat@corp.id", "administrator"),
            ("mgr_mat@corp.id", "manager"),
            ("staff_mat@corp.id", "staff"),
            ("acct_mat@corp.id", "accountant"),
        ];

        for (email, role) in roles {
            let inv_res = harness
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "email": email,
                                "role": role,
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                inv_res.status(),
                StatusCode::CREATED,
                "Owner should be able to invite {}",
                role
            );
        }

        // Test Matrix for PUT /profile
        // Roles:
        // - Owner: 200 OK
        // - Administrator: 200 OK
        // - Manager: 403 Forbidden
        // - Staff: 403 Forbidden
        // - Accountant: 403 Forbidden

        let profile_cases = [
            ("Administrator", &token_admin, StatusCode::OK, false),
            ("Manager", &token_mgr, StatusCode::FORBIDDEN, true),
            ("Staff", &token_staff, StatusCode::FORBIDDEN, true),
            ("Accountant", &token_acct, StatusCode::FORBIDDEN, true),
            ("Owner", &token_owner, StatusCode::OK, false),
        ];

        for (role_name, token, expected_status, is_forbidden) in profile_cases {
            let res = harness
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri(format!("/api/v1/tenants/{}/profile", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "legal_name": format!("Matrix Corp Updated by {}", role_name)
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                expected_status,
                "Role {} PUT /profile test failed. Expected {}",
                role_name,
                expected_status
            );

            if is_forbidden {
                let body: Value =
                    serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                        .unwrap();
                assert_eq!(body["code"], "FORBIDDEN");
            }
        }

        // Test Matrix for POST /members (invite member)
        // Roles:
        // - Owner: 201 Created
        // - Administrator: 201 Created
        // - Manager: 403 Forbidden
        // - Staff: 403 Forbidden
        // - Accountant: 403 Forbidden

        let target_users = [
            (
                "Manager",
                &token_mgr,
                "test_target_1@corp.id",
                "Target 1",
                StatusCode::FORBIDDEN,
                true,
            ),
            (
                "Staff",
                &token_staff,
                "test_target_2@corp.id",
                "Target 2",
                StatusCode::FORBIDDEN,
                true,
            ),
            (
                "Accountant",
                &token_acct,
                "test_target_3@corp.id",
                "Target 3",
                StatusCode::FORBIDDEN,
                true,
            ),
            (
                "Administrator",
                &token_admin,
                "test_target_4@corp.id",
                "Target 4",
                StatusCode::CREATED,
                false,
            ),
            (
                "Owner",
                &token_owner,
                "test_target_5@corp.id",
                "Target 5",
                StatusCode::CREATED,
                false,
            ),
        ];

        for (role_name, token, tgt_email, tgt_name, expected_status, is_forbidden) in target_users {
            let (_t_id, _) = create_test_user(&harness, tgt_email, tgt_name).await;

            let res = harness
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            json!({
                                "email": tgt_email,
                                "role": "staff"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                expected_status,
                "Role {} POST /members test failed. Expected {}",
                role_name,
                expected_status
            );

            if is_forbidden {
                let body: Value =
                    serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                        .unwrap();
                assert_eq!(body["code"], "FORBIDDEN");
            }
        }
    }

    #[tokio::test]
    async fn challenge_privilege_escalation_owner_role_invitation_rejection() {
        let harness = setup_harness().await;

        let (_owner_id, token_owner) =
            create_test_user(&harness, "owner_esc@corp.id", "Owner").await;
        let (_admin_id, token_admin) =
            create_test_user(&harness, "admin_esc@corp.id", "Admin").await;
        let (_target_id, _) = create_test_user(&harness, "target_esc@corp.id", "Target").await;

        let (tenant_id, _) =
            create_workspace_api(&harness, &token_owner, "Escalation Corp", "escalation-corp")
                .await;

        // Invite Admin
        harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "admin_esc@corp.id",
                            "role": "administrator"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 1. Owner attempts to invite someone with role: "owner" -> Must return 400 Bad Request
        let res_owner_inv_owner = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_owner))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "target_esc@corp.id",
                            "role": "owner"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_owner_inv_owner.status(), StatusCode::BAD_REQUEST);
        let body: Value = serde_json::from_slice(
            &res_owner_inv_owner
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body["code"], "INVALID_ROLE");

        // 2. Administrator attempts to invite someone with role: "owner" -> Must return 400 Bad Request
        let res_admin_inv_owner = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/tenants/{}/members", tenant_id))
                    .header(AUTHORIZATION, format!("Bearer {}", token_admin))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "target_esc@corp.id",
                            "role": "owner"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_admin_inv_owner.status(), StatusCode::BAD_REQUEST);
        let body2: Value = serde_json::from_slice(
            &res_admin_inv_owner
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body2["code"], "INVALID_ROLE");
    }

    // =========================================================================
    // SECTION 3: Service Layer & Repository Scoping Challenges
    // =========================================================================

    #[tokio::test]
    async fn challenge_tenant_service_role_and_sole_owner_guards() {
        let harness = setup_harness().await;

        let (owner_id, _token_owner) =
            create_test_user(&harness, "owner_svc@corp.id", "Owner").await;
        let (admin_id, _token_admin) =
            create_test_user(&harness, "admin_svc@corp.id", "Admin").await;
        let (staff_id, _token_staff) =
            create_test_user(&harness, "staff_svc@corp.id", "Staff").await;

        let tenant_uuid = Uuid::new_v4();
        let owner_uuid = Uuid::parse_str(&owner_id).unwrap();
        let admin_uuid = Uuid::parse_str(&admin_id).unwrap();
        let staff_uuid = Uuid::parse_str(&staff_id).unwrap();

        // Provision tenant manually
        let tenant_repo = SqlxTenantRepository::new(harness.pool.clone());
        let membership_repo = SqlxMembershipRepository::new(harness.pool.clone());

        tenant_repo
            .create_tenant(&NewTenant {
                id: tenant_uuid.to_string(),
                name: "Service Guard Corp".to_string(),
                slug: "service-guard-corp".to_string(),
                status: Some(TenantStatus::Active),
                is_personal: false,
            })
            .await
            .unwrap();

        membership_repo
            .create_membership(&NewMembership {
                id: format!("mem_{}", Uuid::new_v4()),
                tenant_id: tenant_uuid.to_string(),
                user_id: owner_id.clone(),
                role: Role::Owner,
            })
            .await
            .unwrap();

        membership_repo
            .create_membership(&NewMembership {
                id: format!("mem_{}", Uuid::new_v4()),
                tenant_id: tenant_uuid.to_string(),
                user_id: admin_id.clone(),
                role: Role::Administrator,
            })
            .await
            .unwrap();

        membership_repo
            .create_membership(&NewMembership {
                id: format!("mem_{}", Uuid::new_v4()),
                tenant_id: tenant_uuid.to_string(),
                user_id: staff_id.clone(),
                role: Role::Staff,
            })
            .await
            .unwrap();

        let ctx_admin = TenantContext::new(tenant_uuid, admin_uuid, Role::Administrator);
        let ctx_owner = TenantContext::new(tenant_uuid, owner_uuid, Role::Owner);
        let ctx_staff = TenantContext::new(tenant_uuid, staff_uuid, Role::Staff);

        // 1. Admin attempts to update member role via service -> Must be FORBIDDEN (Owner only)
        let res_admin_update = harness
            .tenant_service
            .update_member_role(&ctx_admin, &staff_id, Role::Manager)
            .await;
        assert!(res_admin_update.is_err());
        let err = res_admin_update.unwrap_err();
        assert!(format!("{:?}", err).contains("FORBIDDEN"));

        // 2. Staff attempts to remove another member -> Must be FORBIDDEN
        let res_staff_remove = harness
            .tenant_service
            .remove_member(&ctx_staff, &admin_id)
            .await;
        assert!(res_staff_remove.is_err());
        let err2 = res_staff_remove.unwrap_err();
        assert!(format!("{:?}", err2).contains("FORBIDDEN"));

        // 3. Sole owner attempts to demote self -> Must return SOLE_OWNER_DEMOTION conflict
        let res_owner_demote = harness
            .tenant_service
            .update_member_role(&ctx_owner, &owner_id, Role::Staff)
            .await;
        assert!(res_owner_demote.is_err());
        let err3 = res_owner_demote.unwrap_err();
        assert!(format!("{:?}", err3).contains("SOLE_OWNER_DEMOTION"));

        // 4. Sole owner attempts to leave workspace -> Must return SOLE_OWNER_LEAVE conflict
        let res_owner_leave = harness
            .tenant_service
            .remove_member(&ctx_owner, &owner_id)
            .await;
        assert!(res_owner_leave.is_err());
        let err4 = res_owner_leave.unwrap_err();
        assert!(format!("{:?}", err4).contains("SOLE_OWNER_LEAVE"));
    }

    #[tokio::test]
    async fn challenge_repository_tenant_context_isolation() {
        let harness = setup_harness().await;

        let (user_a_id, _) = create_test_user(&harness, "repo_a@test.id", "User A").await;
        let (user_b_id, _) = create_test_user(&harness, "repo_b@test.id", "User B").await;

        let user_a_uuid = Uuid::parse_str(&user_a_id).unwrap();
        let user_b_uuid = Uuid::parse_str(&user_b_id).unwrap();
        let tenant_a_uuid = Uuid::new_v4();
        let tenant_b_uuid = Uuid::new_v4();

        let membership_repo = SqlxMembershipRepository::new(harness.pool.clone());
        let tenant_repo = SqlxTenantRepository::new(harness.pool.clone());

        // Create Tenant A
        tenant_repo
            .create_tenant(&NewTenant {
                id: tenant_a_uuid.to_string(),
                name: "Tenant Alpha Repo".to_string(),
                slug: "tenant-alpha-repo".to_string(),
                status: Some(TenantStatus::Active),
                is_personal: false,
            })
            .await
            .unwrap();

        // Create Tenant B
        tenant_repo
            .create_tenant(&NewTenant {
                id: tenant_b_uuid.to_string(),
                name: "Tenant Beta Repo".to_string(),
                slug: "tenant-beta-repo".to_string(),
                status: Some(TenantStatus::Active),
                is_personal: false,
            })
            .await
            .unwrap();

        // Insert Member A in Tenant A
        membership_repo
            .create_membership(&NewMembership {
                id: format!("mem_{}", Uuid::new_v4()),
                tenant_id: tenant_a_uuid.to_string(),
                user_id: user_a_uuid.to_string(),
                role: Role::Staff,
            })
            .await
            .unwrap();

        // Insert Member B in Tenant B
        membership_repo
            .create_membership(&NewMembership {
                id: format!("mem_{}", Uuid::new_v4()),
                tenant_id: tenant_b_uuid.to_string(),
                user_id: user_b_uuid.to_string(),
                role: Role::Staff,
            })
            .await
            .unwrap();

        // CONTEXT OF TENANT A
        let ctx_a = TenantContext::new(tenant_a_uuid, user_a_uuid, Role::Owner);

        // 1. Context A lists members: Must ONLY return user_a, NEVER user_b
        let members = membership_repo.list_members(&ctx_a).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].user_id, user_a_uuid.to_string());

        // 2. Context A attempts to mutate user_b's role: Must fail with DbError::NotFound (0 rows affected)
        let res_mut = membership_repo
            .update_role(&ctx_a, &user_b_uuid.to_string(), Role::Administrator)
            .await;
        assert!(matches!(res_mut, Err(DbError::NotFound)));

        // Verify User B's role in Tenant B is unchanged
        let mem_b = membership_repo
            .get_membership(&tenant_b_uuid.to_string(), &user_b_uuid.to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mem_b.role, "staff");

        // 3. Context A attempts to delete user_b: Must fail with DbError::NotFound (0 rows affected)
        let res_del = membership_repo
            .delete_membership(&ctx_a, &user_b_uuid.to_string())
            .await;
        assert!(matches!(res_del, Err(DbError::NotFound)));

        // Verify User B is still in Tenant B
        let mem_b_still = membership_repo
            .get_membership(&tenant_b_uuid.to_string(), &user_b_uuid.to_string())
            .await
            .unwrap();
        assert!(mem_b_still.is_some());
    }

    // =========================================================================
    // SECTION 4: Adversarial Input / Attack Vectors
    // =========================================================================

    #[tokio::test]
    async fn challenge_malicious_paths_and_sql_injection_attempts() {
        let harness = setup_harness().await;
        let (_user_id, token) =
            create_test_user(&harness, "fuzz_user@example.com", "Fuzz User").await;

        let malicious_uris = [
            "/api/v1/tenants/00000000-0000-0000-0000-000000000000",
            "/api/v1/tenants/not-a-valid-uuid-format",
            "/api/v1/tenants/'%20OR%201=1--",
            "/api/v1/tenants/admin'--",
            "/api/v1/tenants/%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        ];

        for uri in malicious_uris {
            let res = harness
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(uri)
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            // All invalid or unmatched IDs must strictly return 404 NOT_FOUND
            assert_eq!(
                res.status(),
                StatusCode::NOT_FOUND,
                "URI '{}' did not return 404",
                uri
            );
        }
    }

    #[tokio::test]
    async fn challenge_malformed_x_tenant_id_header() {
        let harness = setup_harness().await;
        let (_user_id, token) =
            create_test_user(&harness, "header_fuzz@example.com", "Header Fuzz").await;

        let malformed_headers = [
            "not-a-valid-uuid",
            "12345",
            "../../../etc/passwd",
            "' OR '1'='1",
        ];

        for h in malformed_headers {
            let res = harness
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/test-context")
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header("x-tenant-id", h)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                res.status(),
                StatusCode::BAD_REQUEST,
                "Malformed header '{}' must return 400 Bad Request",
                h
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(body["code"], "INVALID_TENANT_ID");
        }
    }

    #[tokio::test]
    async fn challenge_comprehensive_reserved_slugs_and_collisions() {
        let harness = setup_harness().await;
        let (_user_id, token) =
            create_test_user(&harness, "slug_master@example.com", "Slug Master").await;

        // All 28 system reserved slugs
        for &slug in Tenant::RESERVED_SLUGS {
            let res = harness
                .app
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
                "Reserved slug '{}' must return 400 Bad Request",
                slug
            );
            let body: Value =
                serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            if slug.len() < 3 {
                assert_eq!(
                    body["code"], "INVALID_SLUG",
                    "Slug '{}' (< 3 chars) must return INVALID_SLUG",
                    slug
                );
            } else {
                assert_eq!(
                    body["code"], "RESERVED_SLUG",
                    "Slug '{}' must return RESERVED_SLUG",
                    slug
                );
            }
        }

        // Test slug collision / conflict
        let (_id1, _) =
            create_workspace_api(&harness, &token, "Original Slug Co", "original-slug-co").await;

        // Attempt exact duplicate slug
        let res_dup = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Duplicate Slug Attempt",
                            "slug": "original-slug-co"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res_dup.status(), StatusCode::CONFLICT);
        let body_dup: Value =
            serde_json::from_slice(&res_dup.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body_dup["code"], "SLUG_ALREADY_EXISTS");

        // Attempt uppercase version of duplicate slug -> must be normalized and rejected as CONFLICT
        let res_dup_upper = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "Uppercase Slug Attempt",
                            "slug": "ORIGINAL-SLUG-CO"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res_dup_upper.status(), StatusCode::CONFLICT);
        let body_upper: Value = serde_json::from_slice(
            &res_dup_upper
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body_upper["code"], "SLUG_ALREADY_EXISTS");
    }

    #[tokio::test]
    async fn challenge_tenant_list_isolation() {
        let harness = setup_harness().await;

        let (_alice_id, token_alice) =
            create_test_user(&harness, "alice_lst@corp.id", "Alice").await;
        let (_bob_id, token_bob) = create_test_user(&harness, "bob_lst@corp.id", "Bob").await;

        // Alice creates 2 workspaces
        let (id_a1, _) =
            create_workspace_api(&harness, &token_alice, "Alice Co 1", "alice-co-1").await;
        let (id_a2, _) =
            create_workspace_api(&harness, &token_alice, "Alice Co 2", "alice-co-2").await;

        // Bob creates 1 workspace
        let (id_b1, _) = create_workspace_api(&harness, &token_bob, "Bob Co 1", "bob-co-1").await;

        // Bob lists tenants
        let res_bob = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_bob))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res_bob.status(), StatusCode::OK);
        let list_bob: Vec<Value> =
            serde_json::from_slice(&res_bob.into_body().collect().await.unwrap().to_bytes())
                .unwrap();

        let bob_tenant_ids: Vec<&str> =
            list_bob.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert!(bob_tenant_ids.contains(&id_b1.as_str()));
        assert!(
            !bob_tenant_ids.contains(&id_a1.as_str()),
            "Bob's tenant list leaked Alice's workspace A1"
        );
        assert!(
            !bob_tenant_ids.contains(&id_a2.as_str()),
            "Bob's tenant list leaked Alice's workspace A2"
        );

        // Alice lists tenants
        let res_alice = harness
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/tenants")
                    .header(AUTHORIZATION, format!("Bearer {}", token_alice))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res_alice.status(), StatusCode::OK);
        let list_alice: Vec<Value> =
            serde_json::from_slice(&res_alice.into_body().collect().await.unwrap().to_bytes())
                .unwrap();

        let alice_tenant_ids: Vec<&str> = list_alice
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect();
        assert!(alice_tenant_ids.contains(&id_a1.as_str()));
        assert!(alice_tenant_ids.contains(&id_a2.as_str()));
        assert!(
            !alice_tenant_ids.contains(&id_b1.as_str()),
            "Alice's tenant list leaked Bob's workspace B1"
        );
    }
}
