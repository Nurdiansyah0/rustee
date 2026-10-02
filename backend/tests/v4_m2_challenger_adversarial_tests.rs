use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::tenant::Role;
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
};
use backend::service::{
    auth_service::AuthService,
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
use uuid::Uuid;

#[allow(dead_code)]
struct TestHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    jwt_engine: Arc<JwtEngine>,
    owner_token: String,
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("accounting_challenger_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 10,
        min_connections: 1,
        busy_timeout_ms: 10_000,
        acquire_timeout_secs: 10,
    };

    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool).await.expect("Failed to run migrations");

    let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));
    let category_repo = Arc::new(SqlxCategoryRepository::new(pool.clone()));
    let audit_repo = Arc::new(SqlxAuditRepository::new(pool.clone()));
    let idempotency_repo = Arc::new(SqlxIdempotencyRepository::new(pool.clone()));
    let transaction_repo = Arc::new(SqlxTransactionRepository::new(pool.clone()));
    let subscription_repo = Arc::new(SqlxSubscriptionRepository::new(pool.clone()));

    let jwt_secret = "m2_challenger_jwt_secret_key_1234567890_adversarial";
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
        auth_service,
        secure_cookie: false,
    };

    let state = AppState {
        auth_state,
        account_repo,
        category_repo,
        user_preferences_repo: Arc::new(backend::repository::SqlxUserPreferencesRepository::new(
            pool.clone(),
        )),
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

    // 1. Create Owner User A
    let owner_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_id.clone(),
            email: "owner_challenger@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner Challenger".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_challenger@business.com", "user", "premium")
        .unwrap();

    // 2. Create Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_challenger@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff Challenger".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_challenger@business.com", "user", "free")
        .unwrap();

    // 3. Create Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Challenger Alpha Enterprise".to_string(),
            slug: "challenger-alpha-enterprise".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    // Seed COA for Tenant A
    let mut tx_a = pool.begin().await.unwrap();
    backend::repository::accounting_repo::SqlxAccountingRepository::seed_default_accounts_tx(
        &mut tx_a,
        &tenant_a_id,
    )
    .await
    .unwrap();
    tx_a.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: owner_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: staff_id.clone(),
            role: Role::Staff,
        })
        .await
        .unwrap();

    // 4. Create Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "user_b_challenger@beta.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "User B Challenger".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "user_b_challenger@beta.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Challenger Beta Enterprise".to_string(),
            slug: "challenger-beta-enterprise".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_b = pool.begin().await.unwrap();
    backend::repository::accounting_repo::SqlxAccountingRepository::seed_default_accounts_tx(
        &mut tx_b,
        &tenant_b_id,
    )
    .await
    .unwrap();
    tx_b.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: user_b_id,
            role: Role::Owner,
        })
        .await
        .unwrap();

    TestHarness {
        app,
        pool,
        _dir: dir,
        jwt_engine,
        owner_token,
        staff_token,
        tenant_a_id,
        tenant_b_id,
        tenant_b_token,
    }
}

async fn send_request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header(CONTENT_TYPE, "application/json");

    if let Some(t_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", t_id);
    }

    let req_body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };

    let req = builder.body(req_body).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body_json: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };
    (status, body_json)
}

async fn send_raw_request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: Option<&str>,
    raw_body: &'static str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header(CONTENT_TYPE, "application/json");

    if let Some(t_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", t_id);
    }

    let req = builder.body(Body::from(raw_body)).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body_json: Value = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };
    (status, body_json)
}

// ============================================================================
// Scope 1: Double-Entry Balancing Edge Cases
// ============================================================================

#[tokio::test]
async fn challenge_single_line_journal_rejection_422() {
    let h = setup_harness().await;

    // Subcase 1: Single line debit only
    let payload1 = json!({
        "description": "Adversarial single line debit",
        "lines": [
            {"account_code": "1000", "debit": 100_000, "credit": 0}
        ]
    });
    let (status1, body1) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload1),
    )
    .await;
    assert_eq!(status1, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body1["code"], "UNBALANCED_JOURNAL_ENTRY");

    // Subcase 2: Single line credit only
    let payload2 = json!({
        "description": "Adversarial single line credit",
        "lines": [
            {"account_code": "4000", "debit": 0, "credit": 100_000}
        ]
    });
    let (status2, body2) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload2),
    )
    .await;
    assert_eq!(status2, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body2["code"], "UNBALANCED_JOURNAL_ENTRY");

    // Subcase 3: Single line with both debit and credit (violates minimum line count rule)
    let payload3 = json!({
        "description": "Adversarial single line both debit and credit",
        "lines": [
            {"account_code": "1000", "debit": 100_000, "credit": 100_000}
        ]
    });
    let (status3, body3) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload3),
    )
    .await;
    assert_eq!(status3, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body3["code"], "UNBALANCED_JOURNAL_ENTRY");
}

#[tokio::test]
async fn challenge_unbalanced_by_one_rupiah_rejection_422() {
    let h = setup_harness().await;

    // Subcase 1: Small amounts unbalanced by 1 Rupiah (1000 vs 999)
    let p1 = json!({
        "description": "Off by 1 IDR - small",
        "lines": [
            {"account_code": "1000", "debit": 1000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 999}
        ]
    });
    let (s1, b1) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(p1),
    )
    .await;
    assert_eq!(s1, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b1["code"], "UNBALANCED_JOURNAL_ENTRY");

    // Subcase 2: Small amounts unbalanced by 1 Rupiah inverted (999 vs 1000)
    let p2 = json!({
        "description": "Off by 1 IDR - inverted",
        "lines": [
            {"account_code": "1000", "debit": 999, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 1000}
        ]
    });
    let (s2, b2) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(p2),
    )
    .await;
    assert_eq!(s2, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b2["code"], "UNBALANCED_JOURNAL_ENTRY");

    // Subcase 3: Quadrillion level unbalanced by 1 Rupiah
    let p3 = json!({
        "description": "Off by 1 IDR - quadrillion level",
        "lines": [
            {"account_code": "1000", "debit": 1_000_000_000_000_001i64, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 1_000_000_000_000_000i64}
        ]
    });
    let (s3, b3) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(p3),
    )
    .await;
    assert_eq!(s3, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b3["code"], "UNBALANCED_JOURNAL_ENTRY");

    // Subcase 4: Multi-line compound split off by 1 Rupiah (3 debits = 300,001, 3 credits = 300,000)
    let p4 = json!({
        "description": "Compound off by 1 IDR",
        "lines": [
            {"account_code": "1000", "debit": 100_000, "credit": 0},
            {"account_code": "1100", "debit": 100_000, "credit": 0},
            {"account_code": "1200", "debit": 100_001, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 100_000},
            {"account_code": "2000", "debit": 0, "credit": 100_000},
            {"account_code": "2100", "debit": 0, "credit": 100_000}
        ]
    });
    let (s4, b4) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(p4),
    )
    .await;
    assert_eq!(s4, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b4["code"], "UNBALANCED_JOURNAL_ENTRY");
}

#[tokio::test]
async fn challenge_compound_multiline_balanced_journals_8_lines() {
    let h = setup_harness().await;

    // 8-line compound journal with 4 debits and 4 credits across all 8 standard system accounts
    // Debits:
    // 1000 Kas: 1,500,000
    // 1100 Bank: 2,500,000
    // 1200 Piutang: 1,000,000
    // 5000 Beban Pokok Penjualan: 3,000,000
    // Total Debits = 8,000,000
    //
    // Credits:
    // 4000 Pendapatan Usaha: 4,000,000
    // 2000 Utang Usaha: 2,000,000
    // 2100 Utang Pajak: 1,000,000
    // 6000 Beban Operasional: 1,000,000
    // Total Credits = 8,000,000
    let payload = json!({
        "description": "Compound 8-line complex split journal",
        "source_type": "MANUAL",
        "lines": [
            {"account_code": "1000", "debit": 1_500_000, "credit": 0, "memo": "Cash portion"},
            {"account_code": "1100", "debit": 2_500_000, "credit": 0, "memo": "Bank transfer portion"},
            {"account_code": "1200", "debit": 1_000_000, "credit": 0, "memo": "Receivable portion"},
            {"account_code": "5000", "debit": 3_000_000, "credit": 0, "memo": "COGS portion"},
            {"account_code": "4000", "debit": 0, "credit": 4_000_000, "memo": "Revenue portion"},
            {"account_code": "2000", "debit": 0, "credit": 2_000_000, "memo": "Vendor payable portion"},
            {"account_code": "2100", "debit": 0, "credit": 1_000_000, "memo": "Tax payable portion"},
            {"account_code": "6000", "debit": 0, "credit": 1_000_000, "memo": "Opex credit portion"}
        ]
    });

    let (status, body) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["total_debit"], 8_000_000);
    assert_eq!(body["total_credit"], 8_000_000);
    assert_eq!(body["lines"].as_array().unwrap().len(), 8);

    // Verify trial balance reflects this 8-line split and net_balance == 0
    let (tb_status, tb_body) = send_request(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token,
        Some(&h.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb_status, StatusCode::OK);
    assert_eq!(tb_body["total_debit"], 8_000_000);
    assert_eq!(tb_body["total_credit"], 8_000_000);
    assert_eq!(tb_body["net_balance"], 0);
    assert_eq!(tb_body["is_balanced"], true);
}

#[tokio::test]
async fn challenge_zero_sum_lines_rejection_422() {
    let h = setup_harness().await;

    // Subcase 1: 2 lines with 0 debit and 0 credit
    let p1 = json!({
        "description": "Zero sum lines (2 lines)",
        "lines": [
            {"account_code": "1000", "debit": 0, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 0}
        ]
    });
    let (s1, b1) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(p1),
    )
    .await;
    assert_eq!(s1, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b1["code"], "UNBALANCED_JOURNAL_ENTRY");

    // Subcase 2: 4 lines all zero
    let p2 = json!({
        "description": "Zero sum lines (4 lines)",
        "lines": [
            {"account_code": "1000", "debit": 0, "credit": 0},
            {"account_code": "1100", "debit": 0, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 0},
            {"account_code": "2000", "debit": 0, "credit": 0}
        ]
    });
    let (s2, b2) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(p2),
    )
    .await;
    assert_eq!(s2, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b2["code"], "UNBALANCED_JOURNAL_ENTRY");
}

#[tokio::test]
async fn challenge_large_amount_arithmetic_quadrillion() {
    let h = setup_harness().await;

    // 9 quadrillion IDR = 9_000_000_000_000_000 IDR
    let quad = 9_000_000_000_000_000i64;
    let payload = json!({
        "description": "National infrastructure mega project 9 quadrillion IDR",
        "source_type": "MANUAL",
        "lines": [
            {"account_code": "1000", "debit": quad, "credit": 0, "memo": "Mega cash inflow"},
            {"account_code": "4000", "debit": 0, "credit": quad, "memo": "Sovereign funding"}
        ]
    });

    let (status, body) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["total_debit"], quad);
    assert_eq!(body["total_credit"], quad);
    let journal_id = body["id"].as_str().unwrap().to_string();

    // Verify trial balance handles 9 quadrillion without integer overflow
    let (tb_status, tb_body) = send_request(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token,
        Some(&h.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb_status, StatusCode::OK);
    assert_eq!(tb_body["total_debit"], quad);
    assert_eq!(tb_body["total_credit"], quad);
    assert_eq!(tb_body["net_balance"], 0);
    assert_eq!(tb_body["is_balanced"], true);

    // Verify reversal of 9 quadrillion journal entry
    let (rev_status, rev_body) = send_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", journal_id),
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(json!({"reason": "Reverse mega project"})),
    )
    .await;

    assert_eq!(rev_status, StatusCode::CREATED);
    assert_eq!(rev_body["total_debit"], quad);
    assert_eq!(rev_body["total_credit"], quad);
    assert_eq!(rev_body["source_type"], "REVERSAL");

    // Verify trial balance returns back to 0 net balance
    let (tb2_status, tb2_body) = send_request(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token,
        Some(&h.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb2_status, StatusCode::OK);
    assert_eq!(tb2_body["total_debit"], quad * 2);
    assert_eq!(tb2_body["total_credit"], quad * 2);
    assert_eq!(tb2_body["net_balance"], 0);
    assert_eq!(tb2_body["is_balanced"], true);
}

// ============================================================================
// Scope 2: Type Coercion & Injection
// ============================================================================

#[tokio::test]
async fn challenge_float_decimal_string_and_scientific_notation_injection_400() {
    let h = setup_harness().await;

    // Subcase 1: Raw JSON float 100.5
    let raw_payload_100_5 = r#"{
        "description": "Float injection 100.5",
        "lines": [
            {"account_code": "1000", "debit": 100.5, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 100.5}
        ]
    }"#;
    let (s1, b1) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_100_5,
    )
    .await;
    assert_eq!(s1, StatusCode::BAD_REQUEST);
    assert_eq!(b1["code"], "INVALID_AMOUNT");

    // Subcase 2: Raw JSON float 0.1
    let raw_payload_0_1 = r#"{
        "description": "Float injection 0.1",
        "lines": [
            {"account_code": "1000", "debit": 0.1, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 0.1}
        ]
    }"#;
    let (s2, b2) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_0_1,
    )
    .await;
    assert_eq!(s2, StatusCode::BAD_REQUEST);
    assert_eq!(b2["code"], "INVALID_AMOUNT");

    // Subcase 3: Raw JSON float 1.0 (even whole float must be rejected)
    let raw_payload_1_0 = r#"{
        "description": "Float injection 1.0",
        "lines": [
            {"account_code": "1000", "debit": 1.0, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 1.0}
        ]
    }"#;
    let (s3, b3) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_1_0,
    )
    .await;
    assert_eq!(s3, StatusCode::BAD_REQUEST);
    assert_eq!(b3["code"], "INVALID_AMOUNT");

    // Subcase 4: Decimal string "100.50"
    let raw_payload_dec_str = r#"{
        "description": "Decimal string injection",
        "lines": [
            {"account_code": "1000", "debit": "100.50", "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": "100.50"}
        ]
    }"#;
    let (s4, b4) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_dec_str,
    )
    .await;
    assert_eq!(s4, StatusCode::BAD_REQUEST);
    assert_eq!(b4["code"], "INVALID_AMOUNT");

    // Subcase 5: Comma decimal string "100,50"
    let raw_payload_comma = r#"{
        "description": "Comma decimal string injection",
        "lines": [
            {"account_code": "1000", "debit": "100,50", "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": "100,50"}
        ]
    }"#;
    let (s5, b5) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_comma,
    )
    .await;
    assert_eq!(s5, StatusCode::BAD_REQUEST);
    assert_eq!(b5["code"], "INVALID_AMOUNT");

    // Subcase 6: Integer string "100" (strict integer, no strings allowed)
    let raw_payload_str_int = r#"{
        "description": "Integer string injection",
        "lines": [
            {"account_code": "1000", "debit": "100", "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": "100"}
        ]
    }"#;
    let (s6, b6) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_str_int,
    )
    .await;
    assert_eq!(s6, StatusCode::BAD_REQUEST);
    assert_eq!(b6["code"], "INVALID_AMOUNT");

    // Subcase 7: Scientific notation 1e6
    let raw_payload_sci_1 = r#"{
        "description": "Scientific notation 1e6",
        "lines": [
            {"account_code": "1000", "debit": 1e6, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 1e6}
        ]
    }"#;
    let (s7, b7) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_sci_1,
    )
    .await;
    assert_eq!(s7, StatusCode::BAD_REQUEST);
    assert_eq!(b7["code"], "INVALID_AMOUNT");

    // Subcase 8: Scientific notation 1.5e3
    let raw_payload_sci_2 = r#"{
        "description": "Scientific notation 1.5e3",
        "lines": [
            {"account_code": "1000", "debit": 1.5e3, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 1.5e3}
        ]
    }"#;
    let (s8, b8) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_sci_2,
    )
    .await;
    assert_eq!(s8, StatusCode::BAD_REQUEST);
    assert_eq!(b8["code"], "INVALID_AMOUNT");

    // Subcase 9: Negative debit amount
    let payload_neg_debit = json!({
        "description": "Negative debit injection",
        "lines": [
            {"account_code": "1000", "debit": -50_000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": -50_000}
        ]
    });
    let (s9, b9) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload_neg_debit),
    )
    .await;
    assert_eq!(s9, StatusCode::BAD_REQUEST);
    assert_eq!(b9["code"], "INVALID_AMOUNT");

    // Subcase 10: Negative credit amount
    let payload_neg_credit = json!({
        "description": "Negative credit injection",
        "lines": [
            {"account_code": "1000", "debit": 50_000, "credit": -50_000},
            {"account_code": "4000", "debit": 0, "credit": 50_000}
        ]
    });
    let (s10, b10) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload_neg_credit),
    )
    .await;
    assert_eq!(s10, StatusCode::BAD_REQUEST);
    assert_eq!(b10["code"], "INVALID_AMOUNT");

    // Subcase 11: Boolean injection
    let raw_payload_bool = r#"{
        "description": "Boolean injection",
        "lines": [
            {"account_code": "1000", "debit": true, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 100}
        ]
    }"#;
    let (s11, b11) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        raw_payload_bool,
    )
    .await;
    assert_eq!(s11, StatusCode::BAD_REQUEST);
    assert_eq!(b11["code"], "INVALID_AMOUNT");

    // Subcase 12: Tax calculate endpoint float & string rejection
    let raw_tax_float = r#"{"amount": 100.5, "tax_type": "PPN_11_EXCL"}"#;
    let (s12, b12) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &h.owner_token,
        None,
        raw_tax_float,
    )
    .await;
    assert_eq!(s12, StatusCode::BAD_REQUEST);
    assert_eq!(b12["code"], "INVALID_AMOUNT");

    let raw_tax_sci = r#"{"amount": 1e6, "tax_type": "PPN_11_EXCL"}"#;
    let (s13, b13) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &h.owner_token,
        None,
        raw_tax_sci,
    )
    .await;
    assert_eq!(s13, StatusCode::BAD_REQUEST);
    assert_eq!(b13["code"], "INVALID_AMOUNT");

    let raw_tax_neg = r#"{"amount": -5000, "tax_type": "PPN_11_EXCL"}"#;
    let (s14, b14) = send_raw_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &h.owner_token,
        None,
        raw_tax_neg,
    )
    .await;
    assert_eq!(s14, StatusCode::BAD_REQUEST);
    assert_eq!(b14["code"], "INVALID_AMOUNT");
}

// ============================================================================
// Scope 3: Immutability & Concurrency
// ============================================================================

#[tokio::test]
async fn challenge_journal_immutability_put_delete_patch_across_roles_405() {
    let h = setup_harness().await;

    // 1. Post a valid journal entry
    let post_payload = json!({
        "description": "Posted journal to test immutability",
        "lines": [
            {"account_code": "1000", "debit": 500_000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 500_000}
        ]
    });
    let (post_status, post_body) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(post_payload),
    )
    .await;
    assert_eq!(post_status, StatusCode::CREATED);
    let journal_id = post_body["id"].as_str().unwrap().to_string();

    let uri = format!("/api/v1/accounting/journals/{}", journal_id);

    // Test PUT across Owner and Staff roles -> strictly 405 JOURNAL_IMMUTABLE
    let (s_put_owner, b_put_owner) = send_request(
        &h.app,
        Method::PUT,
        &uri,
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(json!({"description": "Tampered description"})),
    )
    .await;
    assert_eq!(s_put_owner, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(b_put_owner["code"], "JOURNAL_IMMUTABLE");

    let (s_put_staff, b_put_staff) = send_request(
        &h.app,
        Method::PUT,
        &uri,
        &h.staff_token,
        Some(&h.tenant_a_id),
        Some(json!({"description": "Staff tampered description"})),
    )
    .await;
    assert_eq!(s_put_staff, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(b_put_staff["code"], "JOURNAL_IMMUTABLE");

    // Test DELETE across Owner and Staff roles -> strictly 405 JOURNAL_IMMUTABLE
    let (s_del_owner, b_del_owner) = send_request(
        &h.app,
        Method::DELETE,
        &uri,
        &h.owner_token,
        Some(&h.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(s_del_owner, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(b_del_owner["code"], "JOURNAL_IMMUTABLE");

    let (s_del_staff, b_del_staff) = send_request(
        &h.app,
        Method::DELETE,
        &uri,
        &h.staff_token,
        Some(&h.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(s_del_staff, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(b_del_staff["code"], "JOURNAL_IMMUTABLE");

    // Test PATCH across Owner and Staff roles -> strictly 405 JOURNAL_IMMUTABLE
    let (s_patch_owner, b_patch_owner) = send_request(
        &h.app,
        Method::PATCH,
        &uri,
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(json!({"status": "ARCHIVED"})),
    )
    .await;
    assert_eq!(s_patch_owner, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(b_patch_owner["code"], "JOURNAL_IMMUTABLE");

    let (s_patch_staff, b_patch_staff) = send_request(
        &h.app,
        Method::PATCH,
        &uri,
        &h.staff_token,
        Some(&h.tenant_a_id),
        Some(json!({"status": "ARCHIVED"})),
    )
    .await;
    assert_eq!(s_patch_staff, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(b_patch_staff["code"], "JOURNAL_IMMUTABLE");

    // Verify direct SQL mutation triggers enforce immutability at SQLite DB layer
    // 1. Direct SQL DELETE attempt on posted journal
    let direct_del_res = sqlx::query("DELETE FROM journal_entries WHERE id = ?1")
        .bind(&journal_id)
        .execute(&h.pool)
        .await;
    assert!(direct_del_res.is_err());
    let del_err = direct_del_res.unwrap_err().to_string();
    assert!(
        del_err.contains("Posted journals are immutable and cannot be deleted"),
        "Expected trigger message but got: {}",
        del_err
    );

    // 2. Direct SQL UPDATE attempt on entry_number
    let direct_upd_res = sqlx::query(
        "UPDATE journal_entries SET entry_number = 'JRN-TAMPERED-001' WHERE id = ?1",
    )
    .bind(&journal_id)
    .execute(&h.pool)
    .await;
    assert!(direct_upd_res.is_err());
    let upd_err = direct_upd_res.unwrap_err().to_string();
    assert!(
        upd_err.contains("Posted journals are immutable and cannot be modified"),
        "Expected trigger message but got: {}",
        upd_err
    );

    // 3. Direct SQL DELETE attempt on system account
    let direct_coa_del = sqlx::query(
        "DELETE FROM chart_of_accounts WHERE tenant_id = ?1 AND code = '1000' AND is_system = 1",
    )
    .bind(&h.tenant_a_id)
    .execute(&h.pool)
    .await;
    assert!(direct_coa_del.is_err());
    let coa_err = direct_coa_del.unwrap_err().to_string();
    assert!(
        coa_err.contains("System accounts are protected and cannot be deleted"),
        "Expected COA trigger message but got: {}",
        coa_err
    );
}

#[tokio::test]
async fn challenge_reversal_lifecycle_and_double_reversal_conflict() {
    let h = setup_harness().await;

    // 1. Post journal
    let post_payload = json!({
        "description": "Original journal to reverse",
        "lines": [
            {"account_code": "1000", "debit": 200_000, "credit": 0, "memo": "Cash out"},
            {"account_code": "6000", "debit": 0, "credit": 200_000, "memo": "Expense in"}
        ]
    });
    let (status, body) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(post_payload),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let orig_id = body["id"].as_str().unwrap().to_string();

    // 2. Staff cannot reverse (RBAC 403 Forbidden)
    let (s_staff, b_staff) = send_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &h.staff_token,
        Some(&h.tenant_a_id),
        Some(json!({"reason": "Staff attempt"})),
    )
    .await;
    assert_eq!(s_staff, StatusCode::FORBIDDEN);
    assert_eq!(b_staff["code"], "FORBIDDEN");

    // 3. Owner reverses successfully (201 Created)
    let (s_rev, b_rev) = send_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(json!({"reason": "Audit adjustment"})),
    )
    .await;
    assert_eq!(s_rev, StatusCode::CREATED);
    assert_eq!(b_rev["source_type"], "REVERSAL");
    assert_eq!(b_rev["source_id"], orig_id);
    let rev_id = b_rev["id"].as_str().unwrap().to_string();

    // Verify reversal lines are exactly inverted
    let rev_lines = b_rev["lines"].as_array().unwrap();
    let kas_line = rev_lines.iter().find(|l| l["account_code"] == "1000").unwrap();
    assert_eq!(kas_line["debit"], 0);
    assert_eq!(kas_line["credit"], 200_000); // was debit 200k, now credit 200k

    let exp_line = rev_lines.iter().find(|l| l["account_code"] == "6000").unwrap();
    assert_eq!(exp_line["debit"], 200_000); // was credit 200k, now debit 200k
    assert_eq!(exp_line["credit"], 0);

    // 4. Second reversal attempt on already reversed journal -> strictly 409 Conflict
    let (s_double_rev, b_double_rev) = send_request(
        &h.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(json!({"reason": "Second reversal attempt"})),
    )
    .await;
    assert_eq!(s_double_rev, StatusCode::CONFLICT);
    assert_eq!(b_double_rev["code"], "ALREADY_REVERSED");

    // 5. Reversal journal itself is immutable
    let (s_put_rev, b_put_rev) = send_request(
        &h.app,
        Method::PUT,
        &format!("/api/v1/accounting/journals/{}", rev_id),
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(json!({"description": "Tamper reversal"})),
    )
    .await;
    assert_eq!(s_put_rev, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(b_put_rev["code"], "JOURNAL_IMMUTABLE");
}

#[tokio::test]
async fn challenge_concurrency_stress_journal_posting() {
    let h = setup_harness().await;

    // Concurrently post 15 journals to the same tenant
    let mut set = JoinSet::new();
    for i in 0..15 {
        let app = h.app.clone();
        let token = h.owner_token.clone();
        let tenant_id = h.tenant_a_id.clone();
        let amount = (i + 1) * 10_000;

        set.spawn(async move {
            let payload = json!({
                "description": format!("Concurrent journal #{}", i),
                "lines": [
                    {"account_code": "1000", "debit": amount, "credit": 0},
                    {"account_code": "4000", "debit": 0, "credit": amount}
                ]
            });

            send_request(
                &app,
                Method::POST,
                "/api/v1/accounting/journals",
                &token,
                Some(&tenant_id),
                Some(payload),
            )
            .await
        });
    }

    let mut successful_entry_numbers = Vec::new();
    let mut conflicts = 0;

    while let Some(res) = set.join_next().await {
        let (status, body): (StatusCode, Value) = res.unwrap();
        if status == StatusCode::CREATED {
            let entry_num = body["entry_number"].as_str().unwrap().to_string();
            successful_entry_numbers.push(entry_num);
        } else if status == StatusCode::CONFLICT {
            conflicts += 1;
        } else {
            panic!("Unexpected status during concurrent post: {}", status);
        }
    }

    // In SQLite WAL mode, either all succeed sequentially with gapless unique numbers,
    // or any simultaneous lock collision safely returns 409 Conflict.
    assert!(!successful_entry_numbers.is_empty(), "At least some journals must have succeeded");

    // Crucial Invariant: ALL successful journals MUST have strictly distinct entry numbers
    let mut deduped = successful_entry_numbers.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(
        successful_entry_numbers.len(),
        deduped.len(),
        "Duplicate entry numbers detected under concurrency!"
    );

    // Verify trial balance is strictly balanced
    let (tb_status, tb_body) = send_request(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token,
        Some(&h.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(tb_status, StatusCode::OK);
    assert_eq!(tb_body["net_balance"], 0);
    assert_eq!(tb_body["is_balanced"], true);
    println!(
        "Concurrency test results: {} created, {} conflicts",
        successful_entry_numbers.len(),
        conflicts
    );
}

#[tokio::test]
async fn challenge_empirical_account_validation_worker_claim() {
    let h = setup_harness().await;

    // Worker claim test:
    // Worker M2 handoff section 1.3 claims:
    // "Referenced accounts must exist within the active TenantContext; non-existent or foreign accounts return HTTP 422 Unprocessable Entity (ACCOUNT_NOT_FOUND)."
    // Let's test whether posting to a non-existent account code returns 422 ACCOUNT_NOT_FOUND or is accepted!
    let payload = json!({
        "description": "Journal with non-existent account code",
        "lines": [
            {"account_code": "NON_EXISTENT_9999", "debit": 100_000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 100_000}
        ]
    });

    let (status, body) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload),
    )
    .await;

    println!("Non-existent account post returned: status={}, body={:?}", status, body);

    // Now query trial balance:
    let (tb_status, tb_body) = send_request(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token,
        Some(&h.tenant_a_id),
        None,
    )
    .await;

    // Remediated invariant:
    // 1. Post is rejected with 422 ACCOUNT_NOT_FOUND
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "Non-existent account code must be rejected with 422");
    assert_eq!(body["code"], "ACCOUNT_NOT_FOUND");
    
    // 2. Trial balance remains balanced and untouched
    assert_eq!(tb_status, StatusCode::OK);
    assert_eq!(tb_body["is_balanced"], true, "Trial balance must remain balanced");
    assert_eq!(tb_body["net_balance"], 0, "Trial balance net_balance must be 0");
}

#[tokio::test]
async fn challenge_line_with_simultaneous_debit_and_credit() {
    let h = setup_harness().await;

    // Both debit and credit positive on the same line
    let payload = json!({
        "description": "Simultaneous debit and credit on same line",
        "lines": [
            {"account_code": "1000", "debit": 100_000, "credit": 100_000},
            {"account_code": "4000", "debit": 100_000, "credit": 100_000}
        ]
    });

    let (status, body) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload),
    )
    .await;

    // Remediated invariant: Returns 400 Bad Request with code INVALID_JOURNAL_LINES
    assert_eq!(status, StatusCode::BAD_REQUEST, "Simultaneous debit and credit must be rejected with 400");
    assert_eq!(body["code"], "INVALID_JOURNAL_LINES");
}

#[tokio::test]
async fn challenge_invalid_source_type_unhandled_500() {
    let h = setup_harness().await;

    // SQLite CHECK constraint on source_type IN ('MANUAL', 'INVOICE', 'PAYMENT', 'REVERSAL', 'SYSTEM')
    let payload = json!({
        "description": "Journal with invalid source type",
        "source_type": "ARBITRARY_UNVALIDATED_SOURCE",
        "lines": [
            {"account_code": "1000", "debit": 100_000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 100_000}
        ]
    });

    let (status, body) = send_request(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token,
        Some(&h.tenant_a_id),
        Some(payload),
    )
    .await;

    // Remediated invariant: Invalid source_type is validated at service layer with 400 INVALID_SOURCE_TYPE
    assert_eq!(status, StatusCode::BAD_REQUEST, "Invalid source_type must be rejected with 400");
    assert_eq!(body["code"], "INVALID_SOURCE_TYPE");
}



