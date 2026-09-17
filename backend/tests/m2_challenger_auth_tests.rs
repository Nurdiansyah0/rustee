//! Adversarial Test Suite for Milestone 2: Auth & Token Security
//! Authored by: M2 Challenger 1 (Password Hashing, Timing & Token Adversarial Tests)
//!
//! Evaluates:
//! 1. Timing differential mitigation on non-existent users vs existing users with wrong password.
//! 2. JWT token tampering (payload manipulation, signature mutation, alg="none", alg downgrade, expired, future-dated).
//! 3. Password strength policy exact boundaries (0, 7, 8, 128, 129 chars, character class omissions, unicode byte vs char behavior).
//! 4. Case insensitivity matrix across registration, deduplication, login, and JWT claims.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::tempdir;
use tower::ServiceExt;

use backend::api::auth::{auth_routes, AuthState};
use backend::error::Rfc7807Error;
use backend::repository::{
    init_pool, run_migrations, DbConfig, SqlxUserRepository, UserRepository,
};
use backend::service::auth_service::{
    AuthResponse, AuthService, LoginRequest, RegisterRequest, UserProfileResponse,
};
use backend::service::crypto::{Argon2Config, CryptoService};
use backend::service::jwt::{AuthTokenError, Claims, JwtEngine};

/// Self-contained Base64URL encoder (RFC 4648, URL & Filename Safe, No Padding)
fn base64url_encode(data: &[u8]) -> String {
    const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as usize;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as usize
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as usize
        } else {
            0
        };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(B64_CHARS[(triple >> 18) & 0x3F] as char);
        result.push(B64_CHARS[(triple >> 12) & 0x3F] as char);

        if i + 1 < data.len() {
            result.push(B64_CHARS[(triple >> 6) & 0x3F] as char);
        }
        if i + 2 < data.len() {
            result.push(B64_CHARS[triple & 0x3F] as char);
        }

        i += 3;
    }
    result
}

/// Self-contained Base64URL decoder
fn base64url_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut padded = s.to_string();
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }
    let mut out = Vec::new();
    let bytes = padded.as_bytes();
    let decode_char = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' | b'+' => Some(62),
            b'_' | b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    };

    let mut i = 0;
    while i + 3 < bytes.len() {
        let c0 = decode_char(bytes[i]).ok_or("invalid char 0")?;
        let c1 = decode_char(bytes[i + 1]).ok_or("invalid char 1")?;
        let c2 = decode_char(bytes[i + 2]).ok_or("invalid char 2")?;
        let c3 = decode_char(bytes[i + 3]).ok_or("invalid char 3")?;

        let triple = ((c0 as u32) << 18) | ((c1 as u32) << 12) | ((c2 as u32) << 6) | (c3 as u32);

        out.push(((triple >> 16) & 0xFF) as u8);
        if bytes[i + 2] != b'=' {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if bytes[i + 3] != b'=' {
            out.push((triple & 0xFF) as u8);
        }
        i += 4;
    }
    Ok(out)
}

/// Test fixture providing isolated SQLite database, repository, services, and Axum test router
async fn setup_challenger_test_app() -> (
    Router,
    sqlx::SqlitePool,
    tempfile::TempDir,
    Arc<AuthService>,
    Arc<CryptoService>,
    Arc<JwtEngine>,
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
        "challenger_test_secret_key_minimum_32_bytes_long_12345",
        900,
    ));

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo,
        Arc::clone(&crypto_service),
        Arc::clone(&jwt_engine),
    ));

    let auth_state = AuthState {
        auth_service: Arc::clone(&auth_service),
        secure_cookie: false,
    };

    let app = Router::new().nest("/api/v1/auth", auth_routes(auth_state));

    (app, pool, dir, auth_service, crypto_service, jwt_engine)
}

// =========================================================================
// 1. TIMING ATTACK MITIGATION ON NON-EXISTENT USERS
// =========================================================================

#[tokio::test]
async fn test_adversarial_timing_differential_login_and_dummy_hash() {
    let (app, _, _dir, _, crypto_service, _) = setup_challenger_test_app().await;

    // 1. Register a real user
    let reg_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"email":"registered_victim@example.com","password":"ValidPassword123","display_name":"Target Victim"}"#,
        ))
        .unwrap();
    let reg_res = app.clone().oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);

    // Warm up the DB connection and Tokio threadpool
    for _ in 0..3 {
        let warm_req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"email":"warmup@example.com","password":"SomePassword123"}"#,
            ))
            .unwrap();
        let _ = app.clone().oneshot(warm_req).await.unwrap();
    }

    const SAMPLES: usize = 20;
    let mut wrong_pw_durations = Vec::with_capacity(SAMPLES);
    let mut ghost_user_durations = Vec::with_capacity(SAMPLES);

    // Measure Case A: Existing user with incorrect password
    for _ in 0..SAMPLES {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"email":"registered_victim@example.com","password":"WrongPassword123"}"#,
            ))
            .unwrap();

        let start = Instant::now();
        let res = app.clone().oneshot(req).await.unwrap();
        let dur = start.elapsed();

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        wrong_pw_durations.push(dur);
    }

    // Measure Case B: Non-existent user email (triggers dummy verification)
    for i in 0..SAMPLES {
        let ghost_email = format!("ghost_user_{}@example.com", i);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(
                r#"{{"email":"{}","password":"WrongPassword123"}}"#,
                ghost_email
            )))
            .unwrap();

        let start = Instant::now();
        let res = app.clone().oneshot(req).await.unwrap();
        let dur = start.elapsed();

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        ghost_user_durations.push(dur);
    }

    // Statistical analysis
    let mean_wrong: f64 = wrong_pw_durations
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / SAMPLES as f64;
    let mean_ghost: f64 = ghost_user_durations
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / SAMPLES as f64;

    println!(
        "\n[TIMING CHALLENGE] HTTP Login Endpoints (Fast Test Argon2 Params):\n - Existing User Wrong Password: Mean = {:.3}ms, Min = {:.3}ms, Max = {:.3}ms\n - Non-Existent User:           Mean = {:.3}ms, Min = {:.3}ms, Max = {:.3}ms",
        mean_wrong,
        wrong_pw_durations.iter().map(|d| d.as_secs_f64() * 1000.0).fold(f64::INFINITY, f64::min),
        wrong_pw_durations.iter().map(|d| d.as_secs_f64() * 1000.0).fold(0.0, f64::max),
        mean_ghost,
        ghost_user_durations.iter().map(|d| d.as_secs_f64() * 1000.0).fold(f64::INFINITY, f64::min),
        ghost_user_durations.iter().map(|d| d.as_secs_f64() * 1000.0).fold(0.0, f64::max)
    );

    // CRITICAL ASSERTION:
    // If dummy verification was NOT executed, non-existent user would complete in < 0.1ms (simple DB query).
    // With dummy verification, both must execute Argon2id, taking >= 0.5ms even in fast test mode.
    assert!(
        mean_ghost >= 0.5,
        "Non-existent user mean latency ({:.3}ms) must not bypass password hashing!",
        mean_ghost
    );
    assert!(
        mean_wrong >= 0.5,
        "Existing user wrong password mean latency ({:.3}ms) must execute password hashing!",
        mean_wrong
    );

    // Check direct unit execution of CryptoService::verify_or_dummy
    let real_hash = crypto_service
        .hash_password("RealPassword123".to_string())
        .await
        .unwrap();

    let mut direct_real_durations = Vec::with_capacity(SAMPLES);
    let mut direct_dummy_durations = Vec::with_capacity(SAMPLES);

    for _ in 0..SAMPLES {
        let s1 = Instant::now();
        let _ = crypto_service
            .verify_or_dummy("WrongGuessPassword123".to_string(), Some(&real_hash))
            .await;
        direct_real_durations.push(s1.elapsed());

        let s2 = Instant::now();
        let _ = crypto_service
            .verify_or_dummy("WrongGuessPassword123".to_string(), None)
            .await;
        direct_dummy_durations.push(s2.elapsed());
    }

    let direct_mean_real: f64 = direct_real_durations
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / SAMPLES as f64;
    let direct_mean_dummy: f64 = direct_dummy_durations
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / SAMPLES as f64;

    println!(
        "[TIMING CHALLENGE] Direct CryptoService::verify_or_dummy:\n - Real Hash Verify:  Mean = {:.3}ms\n - Dummy Hash Verify: Mean = {:.3}ms",
        direct_mean_real, direct_mean_dummy
    );

    let diff = (direct_mean_real - direct_mean_dummy).abs();
    let max_mean = direct_mean_real.max(direct_mean_dummy);
    let diff_ratio = diff / max_mean;
    println!(
        "[TIMING CHALLENGE] Direct verification difference ratio: {:.2}%",
        diff_ratio * 100.0
    );

    // Direct Argon2 computation for real vs dummy hash must match within 25% under identical parameters
    assert!(
        diff_ratio < 0.25,
        "Direct Argon2 real vs dummy hash difference ({:.2}%) indicates timing side-channel!",
        diff_ratio * 100.0
    );
}

#[tokio::test]
async fn test_timing_attack_protection_under_full_owasp_parameters() {
    // Challenge with default production OWASP settings (64 MiB, 3 passes)
    let owasp_service =
        CryptoService::new(Argon2Config::default()).expect("failed to init OWASP CryptoService");

    let real_hash = owasp_service
        .hash_password("ProductionPassword123!".to_string())
        .await
        .unwrap();

    // 1. Verify existing user with wrong password under OWASP parameters
    let t1 = Instant::now();
    let res1 = owasp_service
        .verify_or_dummy("WrongPasswordGuess123!".to_string(), Some(&real_hash))
        .await;
    let dur_real = t1.elapsed();
    assert!(!res1);

    // 2. Verify non-existent user (dummy hash) under OWASP parameters
    let t2 = Instant::now();
    let res2 = owasp_service
        .verify_or_dummy("WrongPasswordGuess123!".to_string(), None)
        .await;
    let dur_dummy = t2.elapsed();
    assert!(!res2);

    println!(
        "[TIMING CHALLENGE] Full OWASP Argon2id (64 MiB, 3 passes):\n - Existing user wrong password: {:.2}ms\n - Non-existent user dummy hash:   {:.2}ms",
        dur_real.as_secs_f64() * 1000.0,
        dur_dummy.as_secs_f64() * 1000.0
    );

    // Both operations must take substantial time (> 20ms) confirming dummy hash uses real OWASP workload
    assert!(
        dur_real >= Duration::from_millis(20),
        "OWASP real hash verification should take >= 20ms"
    );
    assert!(
        dur_dummy >= Duration::from_millis(20),
        "OWASP dummy hash verification should take >= 20ms (proves dummy hash is not bypassed)"
    );

    let ratio = (dur_real.as_secs_f64() - dur_dummy.as_secs_f64()).abs()
        / dur_real.as_secs_f64().max(dur_dummy.as_secs_f64());
    assert!(
        ratio < 0.35,
        "OWASP real vs dummy execution time ratio ({:.2}%) too disparate",
        ratio * 100.0
    );
}

// =========================================================================
// 2. JWT TOKEN TAMPERING & CRYPTOGRAPHIC ATTACKS
// =========================================================================

#[tokio::test]
async fn test_adversarial_jwt_claims_tampering_attacks() {
    let (app, _, _dir, _, _, jwt_engine) = setup_challenger_test_app().await;

    // Generate legitimate user token
    let (valid_token, _) = jwt_engine
        .generate_token("usr_legit_101", "alice@example.com", "user", "free")
        .unwrap();

    let parts: Vec<&str> = valid_token.split('.').collect();
    assert_eq!(parts.len(), 3, "Valid JWT must have 3 segments");
    let header_b64 = parts[0];
    let original_payload_b64 = parts[1];
    let signature_b64 = parts[2];

    // Decode original claims JSON using self-contained base64url decoder
    let payload_bytes = base64url_decode(original_payload_b64).expect("decode payload");
    let mut claims_json: serde_json::Value =
        serde_json::from_slice(&payload_bytes).expect("parse claims JSON");

    // Adversarial vector 1: Privilege Escalation - Modify "role" from "user" to "admin"
    claims_json["role"] = serde_json::json!("admin");
    let tampered_payload_role = base64url_encode(&serde_json::to_vec(&claims_json).unwrap());
    let token_tampered_role = format!("{}.{}.{}", header_b64, tampered_payload_role, signature_b64);

    // Direct engine verification MUST fail
    let res = jwt_engine.verify_token(&token_tampered_role);
    assert!(
        matches!(res, Err(AuthTokenError::Invalid(_))),
        "Privilege-escalated role tampering must be rejected by JwtEngine"
    );

    // API endpoint /me MUST return 401 Unauthorized
    let req_role = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(
            header::COOKIE,
            format!("auth_token={}", token_tampered_role),
        )
        .body(Body::empty())
        .unwrap();
    let res_role = app.clone().oneshot(req_role).await.unwrap();
    assert_eq!(res_role.status(), StatusCode::UNAUTHORIZED);
    let bytes = res_role.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(err.code, "AUTH_TOKEN_INVALID");

    // Adversarial vector 2: Identity Impersonation - Modify "sub" to victim user ID
    claims_json["role"] = serde_json::json!("user");
    claims_json["sub"] = serde_json::json!("usr_victim_root_001");
    let tampered_payload_sub = base64url_encode(&serde_json::to_vec(&claims_json).unwrap());
    let token_tampered_sub = format!("{}.{}.{}", header_b64, tampered_payload_sub, signature_b64);

    assert!(matches!(
        jwt_engine.verify_token(&token_tampered_sub),
        Err(AuthTokenError::Invalid(_))
    ));
    let req_sub = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", token_tampered_sub),
        )
        .body(Body::empty())
        .unwrap();
    let res_sub = app.clone().oneshot(req_sub).await.unwrap();
    assert_eq!(res_sub.status(), StatusCode::UNAUTHORIZED);

    // Adversarial vector 3: Subscription Bypassing - Modify "tier" from "free" to "premium"
    claims_json["sub"] = serde_json::json!("usr_legit_101");
    claims_json["tier"] = serde_json::json!("premium");
    let tampered_payload_tier = base64url_encode(&serde_json::to_vec(&claims_json).unwrap());
    let token_tampered_tier = format!("{}.{}.{}", header_b64, tampered_payload_tier, signature_b64);

    assert!(matches!(
        jwt_engine.verify_token(&token_tampered_tier),
        Err(AuthTokenError::Invalid(_))
    ));
    let req_tier = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(
            header::COOKIE,
            format!("auth_token={}", token_tampered_tier),
        )
        .body(Body::empty())
        .unwrap();
    let res_tier = app.clone().oneshot(req_tier).await.unwrap();
    assert_eq!(res_tier.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_adversarial_jwt_signature_mutation_and_stripping() {
    let (app, _, _dir, _, _, jwt_engine) = setup_challenger_test_app().await;

    let (valid_token, _) = jwt_engine
        .generate_token("usr_sig_test", "sig@example.com", "user", "free")
        .unwrap();

    let parts: Vec<&str> = valid_token.split('.').collect();
    let header_payload = format!("{}.{}", parts[0], parts[1]);
    let sig = parts[2];

    // Vector 1: Mutated last byte of signature
    let mutated_sig = format!("{}_", &sig[..sig.len() - 1]);
    let token_mutated = format!("{}.{}", header_payload, mutated_sig);
    assert!(matches!(
        jwt_engine.verify_token(&token_mutated),
        Err(AuthTokenError::Invalid(_))
    ));

    // Vector 2: Truncated signature
    let truncated_sig = &sig[..10];
    let token_truncated = format!("{}.{}", header_payload, truncated_sig);
    assert!(matches!(
        jwt_engine.verify_token(&token_truncated),
        Err(AuthTokenError::Invalid(_))
    ));

    // Vector 3: Signature completely stripped with trailing dot (header.payload.)
    let token_stripped_dot = format!("{}.", header_payload);
    assert!(matches!(
        jwt_engine.verify_token(&token_stripped_dot),
        Err(AuthTokenError::Invalid(_))
    ));

    // Vector 4: Signature completely stripped without dot (header.payload)
    let token_no_dot = header_payload.clone();
    assert!(matches!(
        jwt_engine.verify_token(&token_no_dot),
        Err(AuthTokenError::Invalid(_))
    ));

    // Vector 5: Appended extraneous bytes to valid signature
    let token_garbage_sig = format!("{}{}EXTRA_PADDING_GARBAGE", header_payload, sig);
    assert!(matches!(
        jwt_engine.verify_token(&token_garbage_sig),
        Err(AuthTokenError::Invalid(_))
    ));

    // Vector 6: Empty signature between dots (header..sig)
    let token_empty_payload = format!("{}..{}", parts[0], sig);
    assert!(matches!(
        jwt_engine.verify_token(&token_empty_payload),
        Err(AuthTokenError::Invalid(_))
    ));

    // Verify all vectors return 401 via HTTP extractor
    for token in &[
        &token_mutated,
        &token_truncated,
        &token_stripped_dot,
        &token_no_dot,
        &token_garbage_sig,
        &token_empty_payload,
    ] {
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/auth/me")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "Tampered token must be rejected with 401: {}",
            token
        );
    }
}

#[tokio::test]
async fn test_adversarial_jwt_algorithm_none_and_downgrade_attacks() {
    let (app, _, _dir, _, _, jwt_engine) = setup_challenger_test_app().await;

    let claims = Claims {
        sub: "usr_root_admin".to_string(),
        email: "root@admin.com".to_string(),
        role: "admin".to_string(),
        tier: "premium".to_string(),
        exp: 2_000_000_000,
        iat: 1_700_000_000,
    };
    let payload_b64 = base64url_encode(&serde_json::to_vec(&claims).unwrap());

    // Vector 1: Alg "none" in standard lowercase {"alg":"none","typ":"JWT"}
    let alg_none_header = base64url_encode(br#"{"alg":"none","typ":"JWT"}"#);

    // Variation A: Alg "none" with trailing dot
    let token_none_dot = format!("{}.{}.", alg_none_header, payload_b64);
    assert!(
        matches!(
            jwt_engine.verify_token(&token_none_dot),
            Err(AuthTokenError::Invalid(_))
        ),
        "alg=none attack with trailing dot must be rejected"
    );

    // Variation B: Alg "none" without trailing dot
    let token_none_no_dot = format!("{}.{}", alg_none_header, payload_b64);
    assert!(
        matches!(
            jwt_engine.verify_token(&token_none_no_dot),
            Err(AuthTokenError::Invalid(_))
        ),
        "alg=none attack without trailing dot must be rejected"
    );

    // Variation C: Alg "none" with arbitrary dummy signature
    let token_none_dummy_sig = format!("{}.{}.dummysignature", alg_none_header, payload_b64);
    assert!(
        matches!(
            jwt_engine.verify_token(&token_none_dummy_sig),
            Err(AuthTokenError::Invalid(_))
        ),
        "alg=none attack with dummy signature must be rejected"
    );

    // Vector 2: Alg "None" (casing bypass variation {"alg":"None","typ":"JWT"})
    let alg_none_capped = base64url_encode(br#"{"alg":"None","typ":"JWT"}"#);
    let token_none_capped = format!("{}.{}.", alg_none_capped, payload_b64);
    assert!(matches!(
        jwt_engine.verify_token(&token_none_capped),
        Err(AuthTokenError::Invalid(_))
    ));

    // Vector 3: Alg "HS384" / "HS512" algorithm confusion
    let hs512_header = Header::new(Algorithm::HS512);
    let key = EncodingKey::from_secret(b"challenger_test_secret_key_minimum_32_bytes_long_12345");
    let token_hs512 = encode(&hs512_header, &claims, &key).expect("encode hs512");
    assert!(
        matches!(
            jwt_engine.verify_token(&token_hs512),
            Err(AuthTokenError::Invalid(_))
        ),
        "HS512 token must be rejected when HS256 is strictly enforced"
    );

    // Test on Axum HTTP /api/v1/auth/me
    for token in &[
        &token_none_dot,
        &token_none_no_dot,
        &token_none_dummy_sig,
        &token_none_capped,
        &token_hs512,
    ] {
        let req = Request::builder()
            .method("GET")
            .uri("/api/v1/auth/me")
            .header(header::COOKIE, format!("auth_token={}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "Alg attack token must yield 401: {}",
            token
        );
    }
}

#[tokio::test]
async fn test_adversarial_jwt_expiration_and_clock_skew_boundaries() {
    let (app, _, _dir, _, _, jwt_engine) = setup_challenger_test_app().await;

    let secret = b"challenger_test_secret_key_minimum_32_bytes_long_12345";
    let encoding_key = EncodingKey::from_secret(secret);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Boundary: Expired by 11 seconds (strictly outside 10s leeway)
    let claims_expired_11s = Claims {
        sub: "usr_exp_11s".to_string(),
        email: "exp11s@example.com".to_string(),
        role: "user".to_string(),
        tier: "free".to_string(),
        exp: (now - 11) as usize,
        iat: (now - 100) as usize,
    };
    let token_expired_11s = encode(
        &Header::new(Algorithm::HS256),
        &claims_expired_11s,
        &encoding_key,
    )
    .unwrap();

    let res = jwt_engine.verify_token(&token_expired_11s);
    assert!(
        matches!(res, Err(AuthTokenError::Expired)),
        "Token expired by 11s must yield AuthTokenError::Expired"
    );

    let req_exp_11s = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::COOKIE, format!("auth_token={}", token_expired_11s))
        .body(Body::empty())
        .unwrap();
    let res_exp_11s = app.clone().oneshot(req_exp_11s).await.unwrap();
    assert_eq!(res_exp_11s.status(), StatusCode::UNAUTHORIZED);
    let bytes = res_exp_11s.into_body().collect().await.unwrap().to_bytes();
    let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        err.code, "AUTH_TOKEN_EXPIRED",
        "Expired token must have code AUTH_TOKEN_EXPIRED"
    );

    // 2. Boundary: Expired 1 hour ago
    let claims_expired_1h = Claims {
        sub: "usr_exp_1h".to_string(),
        email: "exp1h@example.com".to_string(),
        role: "user".to_string(),
        tier: "free".to_string(),
        exp: (now - 3600) as usize,
        iat: (now - 7200) as usize,
    };
    let token_expired_1h = encode(
        &Header::new(Algorithm::HS256),
        &claims_expired_1h,
        &encoding_key,
    )
    .unwrap();
    assert!(matches!(
        jwt_engine.verify_token(&token_expired_1h),
        Err(AuthTokenError::Expired)
    ));

    // 3. Boundary: Token expired by 5 seconds (WITHIN 10s leeway)
    let claims_within_leeway = Claims {
        sub: "usr_leeway".to_string(),
        email: "leeway@example.com".to_string(),
        role: "user".to_string(),
        tier: "free".to_string(),
        exp: (now - 5) as usize,
        iat: (now - 60) as usize,
    };
    let token_within_leeway = encode(
        &Header::new(Algorithm::HS256),
        &claims_within_leeway,
        &encoding_key,
    )
    .unwrap();
    // 10s clock skew leeway accepts this:
    let res_leeway = jwt_engine.verify_token(&token_within_leeway);
    assert!(
        res_leeway.is_ok(),
        "Token within 10s leeway should be accepted"
    );

    // 4. Boundary: Token with exp = 0 (Unix Epoch 1970)
    let claims_epoch_exp = Claims {
        sub: "usr_epoch".to_string(),
        email: "epoch@example.com".to_string(),
        role: "user".to_string(),
        tier: "free".to_string(),
        exp: 0,
        iat: 0,
    };
    let token_epoch_exp = encode(
        &Header::new(Algorithm::HS256),
        &claims_epoch_exp,
        &encoding_key,
    )
    .unwrap();
    assert!(matches!(
        jwt_engine.verify_token(&token_epoch_exp),
        Err(AuthTokenError::Expired)
    ));

    // 5. Boundary: Future iat (issued 1 day in the future) with future exp
    let claims_future_iat = Claims {
        sub: "usr_future_iat".to_string(),
        email: "future@example.com".to_string(),
        role: "user".to_string(),
        tier: "free".to_string(),
        exp: (now + 86400 + 900) as usize,
        iat: (now + 86400) as usize,
    };
    let token_future_iat = encode(
        &Header::new(Algorithm::HS256),
        &claims_future_iat,
        &encoding_key,
    )
    .unwrap();
    let res_future = jwt_engine.verify_token(&token_future_iat);
    println!(
        "[TOKEN CHALLENGE] Future iat verification result: is_ok = {}",
        res_future.is_ok()
    );
}

#[tokio::test]
async fn test_adversarial_malformed_tokens_robustness() {
    let (app, _, _dir, _, _, jwt_engine) = setup_challenger_test_app().await;

    let giant_payload = "A".repeat(100_000);
    let malformed_inputs: Vec<&str> = vec![
        "",                                            // Empty token
        "   ",                                         // Whitespace only
        "not.a.token",                                 // 3 invalid words
        "Bearer",                                      // Bare keyword
        "Bearer ",                                     // Trailing space
        "header.payload",                              // Missing signature segment
        "a.b.c.d",                                     // 4 segments
        "header.payload.signature.extra.segment",      // 5 segments
        "header.payload\0.signature",                  // Embedded null byte
        "../../../etc/passwd",                         // Directory traversal attempt
        "' OR '1'='1",                                 // SQL injection payload
        "<script>alert(1)</script>",                   // XSS payload
        "eyJhbGciOiJIUzI1NiJ9.INVALID_JSON.signature", // Invalid JSON payload
        &giant_payload,                                // Giant 100KB payload (DoS)
    ];

    for input in &malformed_inputs {
        // Direct engine check
        let engine_res = jwt_engine.verify_token(input);
        assert!(
            engine_res.is_err(),
            "JwtEngine must reject malformed token: {:?}",
            input
        );

        // HTTP Extractor check via Cookie (if HTTP header parser accepts the characters)
        let cookie_str = format!("auth_token={}", input);
        if let Ok(cookie_val) = header::HeaderValue::from_str(&cookie_str) {
            let req_cookie = Request::builder()
                .method("GET")
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, cookie_val)
                .body(Body::empty())
                .unwrap();
            let res_cookie = app.clone().oneshot(req_cookie).await.unwrap();
            assert_eq!(
                res_cookie.status(),
                StatusCode::UNAUTHORIZED,
                "Cookie with malformed token must return 401: {:?}",
                input
            );
        }

        // HTTP Extractor check via Bearer header
        let bearer_str = format!("Bearer {}", input);
        if let Ok(bearer_val) = header::HeaderValue::from_str(&bearer_str) {
            let req_bearer = Request::builder()
                .method("GET")
                .uri("/api/v1/auth/me")
                .header(header::AUTHORIZATION, bearer_val)
                .body(Body::empty())
                .unwrap();
            let res_bearer = app.clone().oneshot(req_bearer).await.unwrap();
            assert_eq!(
                res_bearer.status(),
                StatusCode::UNAUTHORIZED,
                "Bearer header with malformed token must return 401: {:?}",
                input
            );
        }
    }
}

// =========================================================================
// 3. PASSWORD STRENGTH BOUNDARY VIOLATIONS
// =========================================================================

#[test]
fn test_adversarial_password_strength_exact_boundaries() {
    // 1. Length 0 (Empty password)
    let res = CryptoService::validate_password_strength("");
    assert!(matches!(
        res,
        Err(backend::error::AppError::BadRequest(
            _,
            "PASSWORD_TOO_SHORT"
        ))
    ));

    // 2. Length 1 to 7 (Too short)
    for len in 1..=7usize {
        let pw = format!("P1{}", "a".repeat(len.saturating_sub(2)));
        let res = CryptoService::validate_password_strength(&pw);
        assert!(
            matches!(
                res,
                Err(backend::error::AppError::BadRequest(
                    _,
                    "PASSWORD_TOO_SHORT"
                ))
            ),
            "Length {} should be rejected as PASSWORD_TOO_SHORT: {}",
            len,
            pw
        );
    }

    // Exact 7 chars with both letter and digit
    assert!(matches!(
        CryptoService::validate_password_strength("Pass123"),
        Err(backend::error::AppError::BadRequest(
            _,
            "PASSWORD_TOO_SHORT"
        ))
    ));

    // 3. Length 8: Boundary Valid vs Invalid
    // Valid 8 chars
    assert!(CryptoService::validate_password_strength("Pass1234").is_ok());
    assert!(CryptoService::validate_password_strength("a1b2c3d4").is_ok());
    assert!(CryptoService::validate_password_strength("P@ssw0rd").is_ok());
    assert!(CryptoService::validate_password_strength("1P!!!!!!").is_ok());

    // 8 chars invalid: Digits only (no letters)
    assert!(matches!(
        CryptoService::validate_password_strength("12345678"),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));

    // 8 chars invalid: Letters only (no digits)
    assert!(matches!(
        CryptoService::validate_password_strength("abcdefgh"),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));
    assert!(matches!(
        CryptoService::validate_password_strength("ABCDEFGH"),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));

    // 8 chars invalid: Special characters only (no letters, no digits)
    assert!(matches!(
        CryptoService::validate_password_strength("!@#$%^&*"),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));

    // 8 chars invalid: Letters and symbols (no digits)
    assert!(matches!(
        CryptoService::validate_password_strength("Pass!!!!"),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));

    // 8 chars invalid: Digits and symbols (no letters)
    assert!(matches!(
        CryptoService::validate_password_strength("1234!!!!"),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));

    // 4. Length 128: Maximum Boundary
    let valid_128 = "a1".repeat(64);
    assert_eq!(valid_128.len(), 128);
    assert!(
        CryptoService::validate_password_strength(&valid_128).is_ok(),
        "128 character password must be accepted"
    );

    let no_digits_128 = "a".repeat(128);
    assert!(matches!(
        CryptoService::validate_password_strength(&no_digits_128),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));

    let no_letters_128 = "1".repeat(128);
    assert!(matches!(
        CryptoService::validate_password_strength(&no_letters_128),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
    ));

    // 5. Length 129: Exceeds Maximum Boundary
    let invalid_129 = format!("{}a", "a1".repeat(64));
    assert_eq!(invalid_129.len(), 129);
    assert!(matches!(
        CryptoService::validate_password_strength(&invalid_129),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_LONG"))
    ));

    // 6. Huge length (10,000 chars) DoS protection
    let huge_password = "a1".repeat(5000);
    assert_eq!(huge_password.len(), 10_000);
    assert!(matches!(
        CryptoService::validate_password_strength(&huge_password),
        Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_LONG"))
    ));

    // 7. Whitespace handling in password
    // Leading/trailing spaces count toward password entropy (not trimmed)
    let pw_with_spaces = "   P1   "; // 8 chars
    assert!(CryptoService::validate_password_strength(pw_with_spaces).is_ok());

    let short_with_spaces = " P1 "; // 4 chars
    assert!(matches!(
        CryptoService::validate_password_strength(short_with_spaces),
        Err(backend::error::AppError::BadRequest(
            _,
            "PASSWORD_TOO_SHORT"
        ))
    ));

    // 8. Unicode multi-byte characters investigation
    // Note: Rust String::len() measures bytes, while chars().count() measures unicode scalars.
    // Emoji 🔑 is 4 bytes. "🔑Pass123" is 11 bytes and 8 unicode characters.
    let unicode_pw = "🔑Pass123";
    assert_eq!(unicode_pw.chars().count(), 8);
    assert_eq!(unicode_pw.len(), 11);
    assert!(CryptoService::validate_password_strength(unicode_pw).is_ok());

    // Non-ASCII alphabetic characters: e.g. "ééééééé1" (has 7 accented e's and 1 digit)
    // is_ascii_alphabetic() strictly checks ASCII [a-zA-Z]!
    let non_ascii_letters = "ééééééé1";
    assert!(
        matches!(
            CryptoService::validate_password_strength(non_ascii_letters),
            Err(backend::error::AppError::BadRequest(_, "PASSWORD_TOO_WEAK"))
        ),
        "Non-ASCII letters are rejected by is_ascii_alphabetic policy"
    );
}

// =========================================================================
// 4. CASE INSENSITIVITY MATRIX ACROSS REGISTRATION & LOGIN
// =========================================================================

#[tokio::test]
async fn test_adversarial_case_insensitivity_matrix() {
    let (app, pool, _dir, _, _, _) = setup_challenger_test_app().await;

    let canonical_email = "victim.user@example.com";
    let password = "SecretPassword123";

    // 1. Register with mixed-case email: "Victim.User@EXAMPLE.COM"
    let mixed_email = "Victim.User@EXAMPLE.COM";
    let reg_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&RegisterRequest {
                email: mixed_email.to_string(),
                password: password.to_string(),
                display_name: "Victim User".to_string(),
            })
            .unwrap(),
        ))
        .unwrap();

    let reg_res = app.clone().oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::CREATED);

    let body_bytes = reg_res.into_body().collect().await.unwrap().to_bytes();
    let auth_res: AuthResponse = serde_json::from_slice(&body_bytes).unwrap();

    // Verify response contains canonical lowercase email
    assert_eq!(auth_res.user.email, canonical_email);

    // Verify SQLite database strictly stores canonical lowercase email
    let user_repo = SqlxUserRepository::new(pool.clone());
    let db_user = user_repo
        .find_by_id(&auth_res.user.id)
        .await
        .unwrap()
        .expect("user must exist in db");
    assert_eq!(
        db_user.email, canonical_email,
        "Database must persist normalized lowercase email"
    );

    // 2. Case Insensitivity Collision Attacks: All permutations MUST be rejected with 409 Conflict
    let duplicate_candidates = vec![
        "victim.user@example.com",     // exact lowercase
        "VICTIM.USER@EXAMPLE.COM",     // all uppercase
        "Victim.User@Example.Com",     // title case
        "vIcTiM.uSeR@eXaMpLe.CoM",     // alternating case
        "  victim.user@example.com  ", // leading/trailing spaces
        "  VICTIM.USER@EXAMPLE.COM  ", // spaces + uppercase
    ];

    for dup_email in duplicate_candidates {
        let dup_req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&RegisterRequest {
                    email: dup_email.to_string(),
                    password: "AnotherPassword123".to_string(),
                    display_name: "Imposter".to_string(),
                })
                .unwrap(),
            ))
            .unwrap();

        let res = app.clone().oneshot(dup_req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::CONFLICT,
            "Registration with casing permutation {:?} must be rejected with 409 Conflict",
            dup_email
        );
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let err: Rfc7807Error = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(err.code, "EMAIL_ALREADY_EXISTS");
    }

    // 3. Case Insensitivity Login Matrix: All permutations MUST succeed with 200 OK
    let login_permutations = vec![
        "victim.user@example.com",
        "VICTIM.USER@EXAMPLE.COM",
        "Victim.User@Example.Com",
        "vIcTiM.uSeR@eXaMpLe.CoM",
        "  victim.user@example.com  ",
        "  VICTIM.USER@EXAMPLE.COM  ",
    ];

    for login_email in login_permutations {
        let login_req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&LoginRequest {
                    email: login_email.to_string(),
                    password: password.to_string(),
                })
                .unwrap(),
            ))
            .unwrap();

        let res = app.clone().oneshot(login_req).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "Login with email {:?} must succeed",
            login_email
        );

        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let login_res: AuthResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            login_res.user.email, canonical_email,
            "Returned user email must be canonical"
        );

        // Verify that the issued JWT token's claims also contain the canonical lowercase email
        let me_req = Request::builder()
            .method("GET")
            .uri("/api/v1/auth/me")
            .header(header::AUTHORIZATION, format!("Bearer {}", login_res.token))
            .body(Body::empty())
            .unwrap();
        let me_res = app.clone().oneshot(me_req).await.unwrap();
        assert_eq!(me_res.status(), StatusCode::OK);
        let me_bytes = me_res.into_body().collect().await.unwrap().to_bytes();
        let profile: UserProfileResponse = serde_json::from_slice(&me_bytes).unwrap();
        assert_eq!(profile.user.email, canonical_email);
    }

    // 4. Case permutation with incorrect password MUST return 401 Unauthorized
    let bad_pw_cases = vec!["VICTIM.USER@EXAMPLE.COM", "victim.user@example.com"];
    for email in bad_pw_cases {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&LoginRequest {
                    email: email.to_string(),
                    password: "WrongPassword123".to_string(),
                })
                .unwrap(),
            ))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
