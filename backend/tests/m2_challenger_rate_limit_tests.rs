use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use tokio::sync::Barrier;
use tower::ServiceExt;

use backend::api::auth::{auth_routes_with_rate_limiter, AuthState};
use backend::api::middleware::rate_limiter::{
    extract_client_ip, RateLimitDecision, RateLimiterConfig, SlidingWindowRateLimiter,
};
use backend::repository::{init_pool, run_migrations, DbConfig, SqlxUserRepository};
use backend::service::auth_service::{AuthService, RegisterRequest};
use backend::service::crypto::{Argon2Config, CryptoService};
use backend::service::jwt::JwtEngine;

/// Dedicated test fixture for rate limiter adversarial tests.
async fn setup_challenger_app(
    rate_limiter: Arc<SlidingWindowRateLimiter>,
) -> (
    Router,
    sqlx::SqlitePool,
    tempfile::TempDir,
    Arc<AuthService>,
) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m2_challenger_test.sqlite");
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
    let crypto_service = Arc::new(CryptoService::new(Argon2Config::fast_for_testing()).unwrap());
    let jwt_engine = Arc::new(JwtEngine::new(
        "challenger_rate_limiter_secret_minimum_32_bytes_12345",
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

    let app = Router::new().nest(
        "/api/v1/auth",
        auth_routes_with_rate_limiter(auth_state, Arc::clone(&rate_limiter)),
    );

    (app, pool, dir, auth_service)
}

// =========================================================================
// Challenge 1: 100-thread concurrent login race condition over HTTP
// =========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_adversarial_100_thread_concurrent_login_race_exact_5_succeed_95_rejected_http() {
    let rate_limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig::default()));
    let (app, _, _dir, auth_service) = setup_challenger_app(Arc::clone(&rate_limiter)).await;

    // Register valid user
    let user_email = "concurrent_victim@example.com";
    let user_pass = "VictimPass123";
    auth_service
        .register(RegisterRequest {
            email: user_email.to_string(),
            password: user_pass.to_string(),
            display_name: "Concurrent Victim".to_string(),
        })
        .await
        .expect("Registration should succeed");

    let client_ip = "198.51.100.77";
    let total_tasks = 100;
    let barrier = Arc::new(Barrier::new(total_tasks));
    let mut handles = Vec::with_capacity(total_tasks);

    for _ in 0..total_tasks {
        let app_clone = app.clone();
        let barrier_clone = Arc::clone(&barrier);
        let ip_header = client_ip.to_string();
        let email = user_email.to_string();
        let password = "WrongPassword123".to_string();

        handles.push(tokio::spawn(async move {
            let req_body = serde_json::json!({
                "email": email,
                "password": password
            });

            // Synchronize all 100 tasks to fire at the exact same instant
            barrier_clone.wait().await;

            let req = Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header("X-Forwarded-For", ip_header)
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap();

            let response = app_clone.oneshot(req).await.unwrap();
            let status = response.status();
            let headers = response.headers().clone();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();

            (status, headers, bytes)
        }));
    }

    let mut allowed_count = 0;
    let mut rejected_count = 0;
    let mut remaining_values = Vec::new();

    for handle in handles {
        let (status, headers, bytes) = handle.await.unwrap();

        match status {
            StatusCode::UNAUTHORIZED => {
                allowed_count += 1;
                // Verify rate limit headers on allowed responses
                assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "5");
                let rem_str = headers
                    .get("X-RateLimit-Remaining")
                    .unwrap()
                    .to_str()
                    .unwrap();
                let rem: usize = rem_str.parse().unwrap();
                remaining_values.push(rem);
            }
            StatusCode::TOO_MANY_REQUESTS => {
                rejected_count += 1;
                // Verify RFC 7807 and rate limit headers on 429 responses
                assert_eq!(
                    headers.get(header::CONTENT_TYPE).unwrap(),
                    "application/problem+json"
                );
                assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "5");
                assert_eq!(headers.get("X-RateLimit-Remaining").unwrap(), "0");
                assert!(headers.contains_key("Retry-After"));
                assert!(headers.contains_key("X-RateLimit-Reset"));

                let retry_after: u64 = headers
                    .get("Retry-After")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(retry_after > 0 && retry_after <= 900);

                let problem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(problem["status"], 429);
                assert_eq!(problem["code"], "AUTH_RATE_LIMIT_EXCEEDED");
                assert_eq!(
                    problem["type"],
                    "https://api.nurdiansyahlabs.com/errors/too-many-requests"
                );
                assert_eq!(
                    problem["retry_after_seconds"].as_u64().unwrap(),
                    retry_after
                );
            }
            other => panic!("Unexpected HTTP status code under concurrency: {:?}", other),
        }
    }

    // STRICT SLA: Exactly 5 permitted, exactly 95 rejected with HTTP 429
    assert_eq!(
        allowed_count, 5,
        "Empirical Race Condition Failure: Exactly 5 attempts must succeed"
    );
    assert_eq!(
        rejected_count, 95,
        "Empirical Race Condition Failure: Exactly 95 attempts must receive HTTP 429"
    );

    // Remaining quotas across the 5 permitted calls must span 0..=4
    remaining_values.sort();
    assert_eq!(
        remaining_values,
        vec![0, 1, 2, 3, 4],
        "Remaining headers must monotonically decrement across concurrent requests"
    );
}

// =========================================================================
// Challenge 2: Multi-IP concurrent partition isolation
// =========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_adversarial_concurrent_multi_ip_isolation_100_tasks() {
    let rate_limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig::default()));
    let (app, _, _dir, auth_service) = setup_challenger_app(Arc::clone(&rate_limiter)).await;

    // Register user
    auth_service
        .register(RegisterRequest {
            email: "multi_ip@example.com".to_string(),
            password: "MultiIpPassword123".to_string(),
            display_name: "Multi IP User".to_string(),
        })
        .await
        .unwrap();

    // 10 distinct IPs, each sending 10 concurrent requests = 100 concurrent tasks total
    let total_ips = 10;
    let reqs_per_ip = 10;
    let total_tasks = total_ips * reqs_per_ip;
    let barrier = Arc::new(Barrier::new(total_tasks));
    let mut handles = Vec::with_capacity(total_tasks);

    for ip_idx in 1..=total_ips {
        let ip = format!("192.0.2.{}", ip_idx);

        for _ in 0..reqs_per_ip {
            let app_clone = app.clone();
            let barrier_clone = Arc::clone(&barrier);
            let ip_str = ip.clone();

            handles.push(tokio::spawn(async move {
                let req_body = serde_json::json!({
                    "email": "multi_ip@example.com",
                    "password": "WrongPassword123"
                });

                barrier_clone.wait().await;

                let req = Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("X-Forwarded-For", &ip_str)
                    .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap();

                let res = app_clone.oneshot(req).await.unwrap();
                (ip_str, res.status())
            }));
        }
    }

    let mut per_ip_allowed: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut per_ip_denied: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for handle in handles {
        let (ip, status) = handle.await.unwrap();
        if status == StatusCode::UNAUTHORIZED {
            *per_ip_allowed.entry(ip).or_default() += 1;
        } else if status == StatusCode::TOO_MANY_REQUESTS {
            *per_ip_denied.entry(ip).or_default() += 1;
        } else {
            panic!("Unexpected status: {:?}", status);
        }
    }

    // Every single IP must have exactly 5 allowed and 5 denied
    for ip_idx in 1..=total_ips {
        let ip = format!("192.0.2.{}", ip_idx);
        let allowed = per_ip_allowed.get(&ip).copied().unwrap_or(0);
        let denied = per_ip_denied.get(&ip).copied().unwrap_or(0);

        assert_eq!(
            allowed, 5,
            "IP {} must have strictly 5 allowed attempts under concurrency",
            ip
        );
        assert_eq!(
            denied, 5,
            "IP {} must have strictly 5 denied attempts under concurrency",
            ip
        );
    }
}

// =========================================================================
// Challenge 3: Boundary timing cooldown & sliding window rollover
// =========================================================================

#[tokio::test]
async fn test_adversarial_boundary_timing_cooldown_and_sliding_window_rollover() {
    let window_ms = 250;
    let config = RateLimiterConfig {
        max_attempts: 5,
        window_duration: Duration::from_millis(window_ms),
        cleanup_interval: Duration::from_secs(60),
    };
    let limiter = SlidingWindowRateLimiter::new(config);
    let ip = "203.0.113.44";

    // 1. Send 5 rapid requests at t=0ms -> All must be Allowed
    for i in 1..=5 {
        match limiter.check_and_record(ip).await {
            RateLimitDecision::Allowed { remaining, .. } => {
                assert_eq!(remaining, 5 - i);
            }
            RateLimitDecision::Denied { .. } => {
                panic!("Attempt {} at t=0 must be allowed", i);
            }
        }
    }

    // 2. 6th attempt rejected at t=15ms
    tokio::time::sleep(Duration::from_millis(15)).await;
    match limiter.check_and_record(ip).await {
        RateLimitDecision::Denied {
            retry_after,
            remaining,
            ..
        } => {
            assert_eq!(remaining, 0);
            assert!(retry_after > Duration::ZERO);
            assert!(retry_after <= Duration::from_millis(window_ms));
        }
        RateLimitDecision::Allowed { .. } => panic!("6th attempt must be rejected"),
    }

    // 3. 7th and 8th attempts rejected at t=50ms and t=100ms
    tokio::time::sleep(Duration::from_millis(35)).await;
    assert!(matches!(
        limiter.check_and_record(ip).await,
        RateLimitDecision::Denied { .. }
    ));

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(matches!(
        limiter.check_and_record(ip).await,
        RateLimitDecision::Denied { .. }
    ));

    // 4. CRITICAL: Repeated denied requests MUST NOT extend the sliding window duration!
    // If the rate limiter mistakenly added timestamps on denied requests, the lockout
    // would be extended by another 250ms from t=100ms (until t=350ms).
    // With correct behavior, the initial 5 attempts expire at t=250ms from start.
    // We wait until t=280ms total (current t ~ 100ms, sleep 180ms).
    tokio::time::sleep(Duration::from_millis(180)).await;

    // Window has rolled over: 9th attempt MUST BE ALLOWED!
    match limiter.check_and_record(ip).await {
        RateLimitDecision::Allowed { remaining, .. } => {
            assert_eq!(
                remaining, 4,
                "All 5 previous attempts expired; fresh attempt should leave remaining = 4"
            );
        }
        RateLimitDecision::Denied { retry_after, .. } => {
            panic!(
                "Sliding window failed to roll over! Denied attempts improperly extended lockout. Retry after: {:?}",
                retry_after
            );
        }
    }
}

#[tokio::test]
async fn test_adversarial_sliding_window_partial_rollover_granularity() {
    let window_ms = 300;
    let config = RateLimiterConfig {
        max_attempts: 5,
        window_duration: Duration::from_millis(window_ms),
        cleanup_interval: Duration::from_secs(60),
    };
    let limiter = SlidingWindowRateLimiter::new(config);
    let ip = "198.51.100.88";

    // 1. Group A: 3 attempts at t=0ms
    for _ in 0..3 {
        assert!(matches!(
            limiter.check_and_record(ip).await,
            RateLimitDecision::Allowed { .. }
        ));
    }

    // 2. Wait 150ms
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 3. Group B: 2 attempts at t=150ms
    for _ in 0..2 {
        assert!(matches!(
            limiter.check_and_record(ip).await,
            RateLimitDecision::Allowed { .. }
        ));
    }

    // Total active attempts = 5. Attempt 6 at t=160ms MUST be Denied
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(matches!(
        limiter.check_and_record(ip).await,
        RateLimitDecision::Denied { .. }
    ));

    // 4. Advance time to t=330ms (sleep 160ms)
    // At t=330ms:
    // - Group A attempts (t=0) have elapsed 330ms > 300ms -> EXPIRED
    // - Group B attempts (t=150ms) have elapsed 180ms < 300ms -> STILL ACTIVE
    tokio::time::sleep(Duration::from_millis(160)).await;

    // Exactly 3 slots must be reopened!
    // Attempt 1 of 3:
    match limiter.check_and_record(ip).await {
        RateLimitDecision::Allowed { remaining, .. } => assert_eq!(remaining, 2),
        RateLimitDecision::Denied { .. } => panic!("Slot 1 must be open"),
    }
    // Attempt 2 of 3:
    match limiter.check_and_record(ip).await {
        RateLimitDecision::Allowed { remaining, .. } => assert_eq!(remaining, 1),
        RateLimitDecision::Denied { .. } => panic!("Slot 2 must be open"),
    }
    // Attempt 3 of 3:
    match limiter.check_and_record(ip).await {
        RateLimitDecision::Allowed { remaining, .. } => assert_eq!(remaining, 0),
        RateLimitDecision::Denied { .. } => panic!("Slot 3 must be open"),
    }
    // Attempt 4 (Quota full again: 2 from Group B + 3 fresh = 5 active):
    match limiter.check_and_record(ip).await {
        RateLimitDecision::Denied { remaining, .. } => assert_eq!(remaining, 0),
        RateLimitDecision::Allowed { .. } => panic!("Quota should be exhausted again"),
    }
}

// =========================================================================
// Challenge 4: IP Spoofing header precedence and sanitization
// =========================================================================

#[test]
fn test_adversarial_ip_extraction_precedence_and_spoofing() {
    let mut headers = HeaderMap::new();
    let socket: SocketAddr = "172.20.0.10:38492".parse().unwrap();

    // 1. Cloudflare header precedence over all others
    headers.insert("cf-connecting-ip", "1.1.1.1".parse().unwrap());
    headers.insert("x-real-ip", "2.2.2.2".parse().unwrap());
    headers.insert(
        "x-forwarded-for",
        "3.3.3.3, 4.4.4.4, 5.5.5.5".parse().unwrap(),
    );
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "1.1.1.1",
        "CF-Connecting-IP must take precedence over X-Real-IP, XFF, and Socket"
    );

    // 2. X-Real-IP precedence over XFF and Socket when CF header is missing
    headers.remove("cf-connecting-ip");
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "2.2.2.2",
        "X-Real-IP must take precedence over XFF and Socket"
    );

    // 3. X-Forwarded-For leftmost IP precedence over Socket when CF and X-Real-IP missing
    headers.remove("x-real-ip");
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "3.3.3.3",
        "XFF leftmost IP must take precedence over Socket"
    );

    // 4. SocketAddr fallback when no proxy headers exist
    headers.remove("x-forwarded-for");
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "172.20.0.10",
        "SocketAddr must be used when no headers present"
    );

    // 5. Default fallback to 127.0.0.1 when both headers and socket are absent
    assert_eq!(
        extract_client_ip(&headers, None),
        "127.0.0.1",
        "Fallback to 127.0.0.1 when nothing provided"
    );

    // 6. Adversarial malformed inputs:
    // Malformed CF-Connecting-IP (SQL injection or junk string) must NOT be trusted as IP;
    // it must fall through to valid X-Real-IP
    headers.insert(
        "cf-connecting-ip",
        "' OR '1'='1' -- not an ip".parse().unwrap(),
    );
    headers.insert("x-real-ip", "8.8.8.8".parse().unwrap());
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "8.8.8.8",
        "Malformed CF-Connecting-IP must safely fall through to X-Real-IP"
    );

    // Malformed X-Real-IP must fall through to X-Forwarded-For
    headers.remove("cf-connecting-ip");
    headers.insert("x-real-ip", "999.999.999.999".parse().unwrap());
    headers.insert("x-forwarded-for", "192.0.2.55, 10.0.0.1".parse().unwrap());
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "192.0.2.55",
        "Malformed X-Real-IP must safely fall through to valid XFF"
    );

    // XFF with whitespace and trimming
    headers.remove("x-real-ip");
    headers.insert(
        "x-forwarded-for",
        "   198.51.100.99   , 10.0.0.1".parse().unwrap(),
    );
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "198.51.100.99",
        "XFF must trim surrounding whitespace"
    );

    // Valid IPv6 addresses
    headers.insert("cf-connecting-ip", "2001:db8::1".parse().unwrap());
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "2001:db8::1",
        "Valid IPv6 address in CF-Connecting-IP must be recognized"
    );

    headers.remove("cf-connecting-ip");
    headers.insert("x-real-ip", "::1".parse().unwrap());
    assert_eq!(
        extract_client_ip(&headers, Some(&socket)),
        "::1",
        "Valid IPv6 loopback in X-Real-IP must be recognized"
    );
}

#[tokio::test]
async fn test_adversarial_ip_spoofing_bypass_resistance_http() {
    let rate_limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig::default()));
    let (app, _, _dir, auth_service) = setup_challenger_app(Arc::clone(&rate_limiter)).await;

    auth_service
        .register(RegisterRequest {
            email: "spoof_target@example.com".to_string(),
            password: "TargetPassword123".to_string(),
            display_name: "Spoof Target".to_string(),
        })
        .await
        .unwrap();

    let genuine_cf_ip = "198.51.100.200";

    // Attacker tries to bypass rate limiting by cycling forged X-Forwarded-For headers
    // while their real reverse proxy IP (CF-Connecting-IP) remains constant.
    for i in 1..=5 {
        let fake_xff = format!("10.99.{}.{}", i, i);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header("CF-Connecting-IP", genuine_cf_ip)
            .header("X-Forwarded-For", fake_xff)
            .body(Body::from(
                r#"{"email":"spoof_target@example.com","password":"WrongPassword123"}"#,
            ))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "Attempt {} must be allowed by rate limiter (returns 401)",
            i
        );
    }

    // 6th Attempt with yet another forged X-Forwarded-For:
    // MUST BE BLOCKED because CF-Connecting-IP takes precedence!
    let req6 = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("CF-Connecting-IP", genuine_cf_ip)
        .header("X-Forwarded-For", "172.31.255.254")
        .body(Body::from(
            r#"{"email":"spoof_target@example.com","password":"WrongPassword123"}"#,
        ))
        .unwrap();

    let res6 = app.clone().oneshot(req6).await.unwrap();
    assert_eq!(
        res6.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Spoofing X-Forwarded-For while CF-Connecting-IP is constant must NOT bypass rate limiter"
    );
}

// =========================================================================
// Challenge 5: Memory pruning under 10,000 synthetic IPs
// =========================================================================

#[tokio::test]
async fn test_adversarial_memory_boundedness_10000_synthetic_ips() {
    let window_ms = 60;
    let limiter = SlidingWindowRateLimiter::new(RateLimiterConfig {
        max_attempts: 5,
        window_duration: Duration::from_millis(window_ms),
        cleanup_interval: Duration::from_millis(100),
    });

    let total_ips = 10_000;

    // 1. Ingest 10,000 distinct synthetic IP addresses
    let start_insert = Instant::now();
    for i in 0..total_ips {
        let ip = format!("10.{}.{}.{}", (i >> 16) & 0xFF, (i >> 8) & 0xFF, i & 0xFF);
        limiter.check_and_record(&ip).await;
    }
    let insert_dur = start_insert.elapsed();
    println!("Inserted 10,000 synthetic IPs in {:?}", insert_dur);

    // Verify all 10,000 entries exist in the map
    {
        let map = limiter.attempts.read().await;
        assert_eq!(map.len(), total_ips, "Map must hold exactly 10,000 entries");
    }

    // 2. Wait for the window to expire
    tokio::time::sleep(Duration::from_millis(window_ms + 10)).await;

    // 3. Trigger sweeping prune
    let start_prune = Instant::now();
    let purged = limiter.prune_stale_entries().await;
    let prune_dur = start_prune.elapsed();
    println!("Pruned 10,000 entries in {:?}", prune_dur);

    assert_eq!(
        purged, total_ips,
        "All 10,000 expired entries must be pruned from memory"
    );

    // 4. Verify map is strictly empty and memory is bounded
    {
        let map = limiter.attempts.read().await;
        assert_eq!(
            map.len(),
            0,
            "Memory leak detected: map must be completely empty after pruning stale entries"
        );
    }

    // 5. Test partial interleaved workload: 5,000 expired + 5,000 fresh
    for i in 0..5_000 {
        let ip = format!("172.16.{}.{}", (i >> 8) & 0xFF, i & 0xFF);
        limiter.check_and_record(&ip).await;
    }

    // Wait for first 5,000 to expire
    tokio::time::sleep(Duration::from_millis(window_ms + 10)).await;

    // Insert 5,000 fresh IPs
    for i in 5_000..10_000 {
        let ip = format!("172.16.{}.{}", (i >> 8) & 0xFF, i & 0xFF);
        limiter.check_and_record(&ip).await;
    }

    // Run pruning: exactly 5,000 expired should be removed; 5,000 fresh retained
    let purged_partial = limiter.prune_stale_entries().await;
    assert_eq!(
        purged_partial, 5_000,
        "Exactly the 5,000 expired IPs must be purged"
    );

    {
        let map = limiter.attempts.read().await;
        assert_eq!(
            map.len(),
            5_000,
            "Exactly the 5,000 active IPs must be retained"
        );
    }
}

// =========================================================================
// Challenge 6: Register & Login rate limiting + Logout exemption
// =========================================================================

#[tokio::test]
async fn test_adversarial_register_login_protection_and_logout_exemption() {
    let rate_limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig::default()));
    let (app, _, _dir, _) = setup_challenger_app(Arc::clone(&rate_limiter)).await;
    let ip = "198.51.100.99";

    // 1. Exhaust quota on /register (5 attempts)
    for i in 1..=5 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Forwarded-For", ip)
            .body(Body::from(format!(
                r#"{{"email":"reg_{}@test.com","password":"Password123","display_name":"User {}"}}"#,
                i, i
            )))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // 6th Attempt on /register must receive HTTP 429
    let req6_reg = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", ip)
        .body(Body::from(
            r#"{"email":"reg_6@test.com","password":"Password123","display_name":"User 6"}"#,
        ))
        .unwrap();
    let res6_reg = app.clone().oneshot(req6_reg).await.unwrap();
    assert_eq!(res6_reg.status(), StatusCode::TOO_MANY_REQUESTS);

    // 7th Attempt on /login from SAME IP must ALSO receive HTTP 429 (shared quota on sensitive auth)
    let req7_login = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", ip)
        .body(Body::from(
            r#"{"email":"reg_1@test.com","password":"Password123"}"#,
        ))
        .unwrap();
    let res7_login = app.clone().oneshot(req7_login).await.unwrap();
    assert_eq!(res7_login.status(), StatusCode::TOO_MANY_REQUESTS);

    // 8. Non-sensitive endpoint /logout must NOT be rate limited!
    // Send 10 rapid requests to /logout from the SAME IP
    for _ in 0..10 {
        let req_logout = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/logout")
            .header("X-Forwarded-For", ip)
            .body(Body::empty())
            .unwrap();
        let res_logout = app.clone().oneshot(req_logout).await.unwrap();
        assert_eq!(
            res_logout.status(),
            StatusCode::OK,
            "/logout must be exempted from rate limiting"
        );
    }
}

#[tokio::test]
async fn test_successful_login_resets_rate_limiter_http() {
    let rate_limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig::default()));
    let (app, _, _dir, auth_service) = setup_challenger_app(Arc::clone(&rate_limiter)).await;

    auth_service
        .register(RegisterRequest {
            email: "reset_user@example.com".to_string(),
            password: "CorrectPassword123".to_string(),
            display_name: "Reset User".to_string(),
        })
        .await
        .unwrap();

    let client_ip = "198.51.100.50";

    // 1. Send 4 failed login attempts -> returns 401 Unauthorized, remaining decrements to 1
    for i in 1..=4 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Forwarded-For", client_ip)
            .body(Body::from(
                r#"{"email":"reset_user@example.com","password":"WrongPassword123"}"#,
            ))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let rem: usize = res
            .headers()
            .get("X-RateLimit-Remaining")
            .unwrap()
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(rem, 5 - i);
    }

    // 2. 5th Attempt: Successful login -> returns 200 OK, resets remaining to 5
    let req_success = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", client_ip)
        .body(Body::from(
            r#"{"email":"reset_user@example.com","password":"CorrectPassword123"}"#,
        ))
        .unwrap();

    let res_success = app.clone().oneshot(req_success).await.unwrap();
    assert_eq!(res_success.status(), StatusCode::OK);
    let rem: usize = res_success
        .headers()
        .get("X-RateLimit-Remaining")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(rem, 5, "Successful login must reset remaining quota to 5");

    // 3. 6th Attempt: Another successful login from the same IP -> MUST SUCCEED (not 429)
    let req_repeat = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", client_ip)
        .body(Body::from(
            r#"{"email":"reset_user@example.com","password":"CorrectPassword123"}"#,
        ))
        .unwrap();

    let res_repeat = app.clone().oneshot(req_repeat).await.unwrap();
    assert_eq!(
        res_repeat.status(),
        StatusCode::OK,
        "Legitimate user must not be blocked after previous successful logins"
    );
}

#[tokio::test]
async fn test_production_router_create_app_enforces_rate_limiter() {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m2_prod_router_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 5,
        min_connections: 1,
        busy_timeout_ms: 5000,
        acquire_timeout_secs: 5,
    };

    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool).await.expect("Failed to run migrations");

    let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
    let account_repo = Arc::new(backend::repository::account_repo::SqlxAccountRepository::new(pool.clone()));
    let category_repo = Arc::new(backend::repository::category_repo::SqlxCategoryRepository::new(pool.clone()));
    let audit_repo = Arc::new(backend::repository::audit_repo::SqlxAuditRepository::new(pool.clone()));
    let idempotency_repo = Arc::new(backend::repository::idempotency_repo::SqlxIdempotencyRepository::new(pool.clone()));
    let transaction_repo = Arc::new(backend::repository::transaction_repo::SqlxTransactionRepository::new(pool.clone()));
    let subscription_repo = Arc::new(backend::repository::subscription_repo::SqlxSubscriptionRepository::new(pool.clone()));

    let crypto_service = Arc::new(CryptoService::new(Argon2Config::fast_for_testing()).unwrap());
    let jwt_engine = Arc::new(JwtEngine::new("test_secret_for_prod_router_test_123456", 900));

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo.clone(),
        crypto_service,
        jwt_engine,
    ));

    let ledger_service = Arc::new(backend::service::ledger_service::LedgerService::new(
        pool.clone(),
        transaction_repo,
        idempotency_repo,
    ));

    let payment_service = Arc::new(backend::service::payment_service::PaymentService::new(
        backend::service::payment_service::PaymentConfig::default(),
        subscription_repo,
        user_repo,
        audit_repo,
    ));

    let rate_limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig {
        max_attempts: 5,
        window_duration: Duration::from_secs(900),
        cleanup_interval: Duration::from_secs(300),
    }));

    let state = backend::api::AppState {
        auth_state: AuthState {
            auth_service,
            secure_cookie: false,
        },
        account_repo,
        category_repo,
        user_preferences_repo: Arc::new(backend::repository::SqlxUserPreferencesRepository::new(pool.clone())),
        ledger_service,
        payment_service,
        pool,
        rate_limiter,
    };

    // PROD ROUTER CREATED VIA create_app
    let app = backend::api::create_app(state);

    let client_ip = "203.0.113.88";

    // First 5 attempts from this IP: handled by auth handler (401 invalid creds), RateLimit headers present
    for i in 1..=5 {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Forwarded-For", client_ip)
            .body(Body::from(r#"{"email":"nonexistent@test.com","password":"WrongPassword123"}"#))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            res.headers().get("X-RateLimit-Remaining").unwrap().to_str().unwrap(),
            &(5 - i).to_string()
        );
    }

    // 6th attempt from the same IP: Rate limiter MUST block and return HTTP 429 Too Many Requests
    let req6 = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", client_ip)
        .body(Body::from(r#"{"email":"nonexistent@test.com","password":"WrongPassword123"}"#))
        .unwrap();

    let res6 = app.clone().oneshot(req6).await.unwrap();
    assert_eq!(res6.status(), StatusCode::TOO_MANY_REQUESTS, "Production router MUST enforce rate limiter");
    assert_eq!(
        res6.headers().get("X-RateLimit-Remaining").unwrap().to_str().unwrap(),
        "0"
    );
    assert!(res6.headers().contains_key("Retry-After"));
}
