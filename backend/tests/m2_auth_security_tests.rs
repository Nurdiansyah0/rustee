use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;
use tower::ServiceExt;

use backend::api::auth::{auth_routes, auth_routes_with_rate_limiter, AuthState};
use backend::api::middleware::rate_limiter::{RateLimiterConfig, SlidingWindowRateLimiter};
use backend::domain::money::Rupiah;
use backend::error::Rfc7807Error;
use backend::repository::{
    init_pool, run_migrations, AccountRepository, CategoryRepository, DbConfig,
    SqlxAccountRepository, SqlxCategoryRepository, SqlxUserRepository, UserRepository,
};
use backend::service::auth_service::{
    AuthResponse, AuthService, RegisterRequest, UserProfileResponse,
};
use backend::service::crypto::{Argon2Config, CryptoService};
use backend::service::jwt::JwtEngine;

/// Test setup fixture: creates isolated SQLite database, repositories, services, and Axum routers
async fn setup_test_app() -> (
    Router,
    Router,
    sqlx::SqlitePool,
    tempfile::TempDir,
    Arc<AuthService>,
    Arc<SlidingWindowRateLimiter>,
) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m2_auth_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 5,
        min_connections: 1,
        busy_timeout_ms: 5000,
        acquire_timeout_secs: 5,
    };

    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
    // Use fast argon2 parameters for tests to keep test suite fast and deterministic
    let crypto_service = Arc::new(CryptoService::new(Argon2Config::fast_for_testing()).unwrap());
    let jwt_engine = Arc::new(JwtEngine::new(
        "test_secret_key_minimum_32_bytes_long_12345",
        900,
    ));

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo,
        crypto_service,
        jwt_engine,
    ));

    let auth_state = AuthState {
        auth_service: Arc::clone(&auth_service),
        secure_cookie: false,
    };

    let rate_limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig::default()));

    // 1. Basic router (no rate limiter)
    let app_basic = Router::new().nest("/api/v1/auth", auth_routes(auth_state.clone()));

    // 2. Rate-limited router (5 attempts / 15 min on /register & /login)
    let app_rate_limited = Router::new().nest(
        "/api/v1/auth",
        auth_routes_with_rate_limiter(auth_state, Arc::clone(&rate_limiter)),
    );

    (
        app_basic,
        app_rate_limited,
        pool,
        dir,
        auth_service,
        rate_limiter,
    )
}

#[tokio::test]
async fn test_registration_success_seeds_wallet_and_categories() {
    let (app, _, pool, _dir, _, _) = setup_test_app().await;
    let account_repo = SqlxAccountRepository::new(pool.clone());
    let category_repo = SqlxCategoryRepository::new(pool.clone());

    let payload = RegisterRequest {
        email: "alice@example.com".to_string(),
        password: "Password123".to_string(),
        display_name: "Alice Liddell".to_string(),
    };

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Verify Set-Cookie header attributes
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("Missing Set-Cookie header")
        .to_str()
        .unwrap();
    assert!(cookie.contains("auth_token="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Max-Age=900"));
    assert!(cookie.contains("Path=/"));

    // Verify JSON response payload
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let auth_res: AuthResponse = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(auth_res.user.email, "alice@example.com");
    assert_eq!(auth_res.user.display_name, "Alice Liddell");
    assert_eq!(auth_res.user.currency, "IDR");
    assert_eq!(auth_res.user.role, "user");
    assert_eq!(auth_res.user.subscription_tier, "free");
    assert_eq!(auth_res.permissions, vec!["transactions.basic"]);
    assert!(!auth_res.token.is_empty());

    // Verify default "Cash" wallet account was atomically seeded
    let accounts = account_repo
        .list_by_user(&auth_res.user.id, false)
        .await
        .unwrap();
    assert_eq!(accounts.len(), 1, "Exactly 1 default wallet must be seeded");
    assert_eq!(accounts[0].name, "Cash");
    assert_eq!(accounts[0].account_type, "cash");
    assert_eq!(accounts[0].currency, "IDR");
    assert_eq!(accounts[0].current_balance, Rupiah::ZERO);

    // Verify 10 starter categories were atomically seeded
    let categories = category_repo.list_by_user(&auth_res.user.id).await.unwrap();
    assert_eq!(categories.len(), 10, "Expected 10 starter categories");

    let income_count = categories
        .iter()
        .filter(|c| c.category_type == "income")
        .count();
    let expense_count = categories
        .iter()
        .filter(|c| c.category_type == "expense")
        .count();
    assert_eq!(income_count, 3, "Expected 3 income categories");
    assert_eq!(expense_count, 7, "Expected 7 expense categories");

    // Starter categories must be owned by user (is_system = false) so user can customize/delete them
    for cat in &categories {
        assert!(!cat.is_system, "Starter categories must have is_system = 0");
        assert_eq!(cat.user_id.as_deref(), Some(auth_res.user.id.as_str()));
    }
}

#[tokio::test]
async fn test_registration_validation_and_duplicate_rejection() {
    let (app, _, _, _dir, _, _) = setup_test_app().await;

    // 1. Weak password: too short (< 8 chars)
    let short_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"bob@test.com","password":"short","display_name":"Bob"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(short_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "PASSWORD_TOO_SHORT");

    // 2. Weak password: missing numbers
    let no_num_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"bob@test.com","password":"NoNumbersHere","display_name":"Bob"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(no_num_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "PASSWORD_TOO_WEAK");

    // 3. Invalid email format
    let bad_email_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"invalid-email","password":"Password123","display_name":"Bob"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(bad_email_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "INVALID_EMAIL_FORMAT");

    // 4. Blank display name
    let blank_name_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"bob@test.com","password":"Password123","display_name":"   "}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(blank_name_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "DISPLAY_NAME_REQUIRED");

    // 5. Register valid user
    let valid_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"bob@test.com","password":"Password123","display_name":"Bob"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(valid_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 6. Duplicate registration with same email (case-insensitive)
    let dup_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"BOB@test.com","password":"Password123","display_name":"Bob 2"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(dup_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "EMAIL_ALREADY_EXISTS");
}

#[tokio::test]
async fn test_login_flow_and_invalid_credentials_timing_protection() {
    let (app, _, _, _dir, _, _) = setup_test_app().await;

    // Register user first
    let reg_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"charlie@test.com","password":"SecurePass123","display_name":"Charlie"}"#,
        ))
        .unwrap();
    let _ = app.clone().oneshot(reg_req).await.unwrap();

    // 1. Successful login
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"charlie@test.com","password":"SecurePass123"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(login_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().contains_key(header::SET_COOKIE));

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let auth_res: AuthResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(auth_res.user.email, "charlie@test.com");
    assert!(!auth_res.token.is_empty());

    // 2. Incorrect password on existing user
    let start_wrong = Instant::now();
    let bad_pw_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"charlie@test.com","password":"WrongPassword123"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(bad_pw_req).await.unwrap();
    let _dur_wrong = start_wrong.elapsed();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "INVALID_CREDENTIALS");

    // 3. Non-existent email (triggers dummy Argon2 verification)
    let start_ghost = Instant::now();
    let ghost_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"ghost@test.com","password":"SomePassword123"}"#,
        ))
        .unwrap();
    let res = app.clone().oneshot(ghost_req).await.unwrap();
    let _dur_ghost = start_ghost.elapsed();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "INVALID_CREDENTIALS");
}

#[tokio::test]
async fn test_me_extractor_via_cookie_and_bearer_header() {
    let (app, _, pool, _dir, auth_service, _) = setup_test_app().await;

    // Register user to obtain token
    let reg_res = auth_service
        .register(RegisterRequest {
            email: "dan@test.com".to_string(),
            password: "DanPassword123".to_string(),
            display_name: "Dan".to_string(),
        })
        .await
        .unwrap();

    let valid_token = reg_res.token;

    // 1. Access /me via Cookie header
    let cookie_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, format!("auth_token={}", valid_token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(cookie_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let profile: UserProfileResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(profile.user.email, "dan@test.com");
    assert_eq!(profile.permissions, vec!["transactions.basic"]);

    // 2. Access /me via Authorization: Bearer header fallback
    let bearer_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {}", valid_token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(bearer_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Access /me without any credentials -> 401 Unauthorized
    let unauth_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(unauth_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "AUTH_TOKEN_MISSING");

    // 4. Access /me with invalid/tampered token -> 401 Unauthorized
    let tampered_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::AUTHORIZATION, "Bearer invalid.jwt.token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(tampered_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "AUTH_TOKEN_INVALID");

    // 5. Update user tier to premium in database -> verify real-time permissions reflection
    let user_repo = SqlxUserRepository::new(pool);
    user_repo
        .update_tier(&reg_res.user.id, "premium")
        .await
        .unwrap();

    let premium_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, format!("auth_token={}", valid_token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(premium_req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let updated_profile: UserProfileResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(updated_profile.user.subscription_tier, "premium");
    assert_eq!(
        updated_profile.permissions,
        vec![
            "transactions.basic",
            "analytics.advanced",
            "budgeting",
            "reports.advanced"
        ]
    );
}

#[tokio::test]
async fn test_logout_clears_cookie() {
    let (app, _, _, _dir, _, _) = setup_test_app().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("Missing Set-Cookie header on logout")
        .to_str()
        .unwrap();
    assert!(cookie.contains("auth_token="));
    assert!(cookie.contains("Max-Age=0"));
    assert!(cookie.contains("Expires=Thu, 01 Jan 1970"));
}

#[tokio::test]
async fn test_sliding_window_rate_limiter_5_attempt_sla_and_429() {
    let (_, app_rate_limited, _, _dir, _, _) = setup_test_app().await;
    let client_ip = "203.0.113.195";

    // 1. Send 5 failed login attempts from client_ip -> all should pass through middleware
    for i in 1..=5 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Forwarded-For", client_ip)
            .body(Body::from(
                r#"{"email":"target@test.com","password":"WrongPassword123"}"#,
            ))
            .unwrap();

        let res = app_rate_limited.clone().oneshot(req).await.unwrap();
        // Since credentials are wrong, handler returns 401, but rate limit allowed it
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let rem = res
            .headers()
            .get("X-RateLimit-Remaining")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(rem, (5 - i).to_string());
    }

    // 2. 6th Attempt from SAME client_ip MUST be blocked with HTTP 429 Too Many Requests
    let req6 = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", client_ip)
        .body(Body::from(
            r#"{"email":"target@test.com","password":"WrongPassword123"}"#,
        ))
        .unwrap();

    let res6 = app_rate_limited.clone().oneshot(req6).await.unwrap();
    assert_eq!(res6.status(), StatusCode::TOO_MANY_REQUESTS);

    // Verify rate limiting response headers
    let headers = res6.headers();
    assert_eq!(
        headers.get("Content-Type").unwrap(),
        "application/problem+json"
    );
    assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "5");
    assert_eq!(headers.get("X-RateLimit-Remaining").unwrap(), "0");
    assert!(headers.contains_key("Retry-After"));
    assert!(headers.contains_key("X-RateLimit-Reset"));

    // Verify RFC 7807 JSON problem details
    let bytes = res6.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], 429);
    assert_eq!(json["code"], "AUTH_RATE_LIMIT_EXCEEDED");
    assert_eq!(
        json["type"],
        "https://api.nurdiansyahlabs.com/errors/too-many-requests"
    );
    assert_eq!(json["title"], "Too Many Requests");
    assert!(json["retry_after_seconds"].as_u64().unwrap() > 0);

    // 3. Verify that a DIFFERENT IP address is NOT blocked (Partition isolation)
    let req_other_ip = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", "198.51.100.99")
        .body(Body::from(
            r#"{"email":"target@test.com","password":"WrongPassword123"}"#,
        ))
        .unwrap();

    let res_other = app_rate_limited.oneshot(req_other_ip).await.unwrap();
    assert_eq!(
        res_other.status(),
        StatusCode::UNAUTHORIZED,
        "Different IP must not be rate limited"
    );
    assert_eq!(
        res_other
            .headers()
            .get("X-RateLimit-Remaining")
            .unwrap()
            .to_str()
            .unwrap(),
        "4"
    );
}

#[tokio::test]
async fn test_me_extractor_expired_token_rejection_http_401() {
    let (app, _, _, _dir, auth_service, _) = setup_test_app().await;

    // Generate expired token using the new JwtEngine helper
    let (expired_token, _) = auth_service
        .jwt_engine()
        .generate_expired_token("usr_test_expired", "expired@test.com", "user", "free")
        .expect("generate expired token");

    // 1. Expired token via Cookie header -> HTTP 401 AUTH_TOKEN_EXPIRED
    let cookie_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, format!("auth_token={}", expired_token))
        .body(Body::empty())
        .unwrap();

    let res_cookie = app.clone().oneshot(cookie_req).await.unwrap();
    assert_eq!(res_cookie.status(), StatusCode::UNAUTHORIZED);
    let bytes_cookie = res_cookie.into_body().collect().await.unwrap().to_bytes();
    let err_cookie: Rfc7807Error = serde_json::from_slice(&bytes_cookie).unwrap();
    assert_eq!(err_cookie.code, "AUTH_TOKEN_EXPIRED");

    // 2. Expired token via Authorization: Bearer header -> HTTP 401 AUTH_TOKEN_EXPIRED
    let bearer_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {}", expired_token))
        .body(Body::empty())
        .unwrap();

    let res_bearer = app.oneshot(bearer_req).await.unwrap();
    assert_eq!(res_bearer.status(), StatusCode::UNAUTHORIZED);
    let bytes_bearer = res_bearer.into_body().collect().await.unwrap().to_bytes();
    let err_bearer: Rfc7807Error = serde_json::from_slice(&bytes_bearer).unwrap();
    assert_eq!(err_bearer.code, "AUTH_TOKEN_EXPIRED");
}

#[tokio::test]
async fn test_auth_endpoints_declare_cache_control_headers() {
    let (app_basic, _app_rate_limited, _, _dir, _, _) = setup_test_app().await;

    // 1. Register endpoint must return Cache-Control
    let reg_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"cache_test@example.com","password":"Password123","display_name":"Cache Test"}"#,
        ))
        .unwrap();
    let reg_res = app_basic.clone().oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);
    assert_eq!(
        reg_res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|h| h.to_str().unwrap()),
        Some("private, no-store, must-revalidate")
    );

    // 2. Login endpoint must return Cache-Control
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"cache_test@example.com","password":"Password123"}"#,
        ))
        .unwrap();
    let login_res = app_basic.clone().oneshot(login_req).await.unwrap();
    assert_eq!(login_res.status(), StatusCode::OK);
    assert_eq!(
        login_res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|h| h.to_str().unwrap()),
        Some("private, no-store, must-revalidate")
    );

    // 3. Logout endpoint must return Cache-Control
    let logout_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .body(Body::empty())
        .unwrap();
    let logout_res = app_basic.clone().oneshot(logout_req).await.unwrap();
    assert_eq!(logout_res.status(), StatusCode::OK);
    assert_eq!(
        logout_res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|h| h.to_str().unwrap()),
        Some("private, no-store, must-revalidate")
    );

    // 4. Me endpoint must return Cache-Control
    let login_bytes = login_res.into_body().collect().await.unwrap().to_bytes();
    let auth_res: AuthResponse = serde_json::from_slice(&login_bytes).unwrap();

    let me_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, format!("auth_token={}", auth_res.token))
        .body(Body::empty())
        .unwrap();
    let me_res = app_basic.clone().oneshot(me_req).await.unwrap();
    assert_eq!(me_res.status(), StatusCode::OK);
    assert_eq!(
        me_res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|h| h.to_str().unwrap()),
        Some("private, no-store, must-revalidate")
    );

    // 5. Unauthenticated rejection (401) must also carry Cache-Control via middleware
    let unauth_req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .body(Body::empty())
        .unwrap();
    let unauth_res = app_basic.oneshot(unauth_req).await.unwrap();
    assert_eq!(unauth_res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        unauth_res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|h| h.to_str().unwrap()),
        Some("private, no-store, must-revalidate")
    );
}

#[tokio::test]
async fn test_me_extractor_http2_multi_cookie_headers() {
    let (app, _, _, _dir, auth_service, _) = setup_test_app().await;

    let reg_res = auth_service
        .register(RegisterRequest {
            email: "http2_cookies@test.com".to_string(),
            password: "Http2Password123".to_string(),
            display_name: "HTTP2 User".to_string(),
        })
        .await
        .unwrap();

    // Simulate HTTP/2 multi-header request where auth_token is NOT the first header
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, "theme=dark; layout=mobile")
        .header(header::COOKIE, format!("auth_token={}", reg_res.token))
        .header(header::COOKIE, "analytics_opt_out=true")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "HTTP/2 split Cookie headers must successfully authenticate"
    );

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let profile: UserProfileResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(profile.user.email, "http2_cookies@test.com");
}
