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
use backend::domain::accounting::{PostJournalEntryCommand, PostJournalLineCommand};
use backend::domain::money::Rupiah;
use backend::domain::tenant::TenantContext;
use backend::service::accounting_service::AccountingService;
use chrono::Utc;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::tempdir;
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
    let db_path = dir.path().join("accounting_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 5,
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

    let jwt_secret = "m2_accounting_jwt_secret_key_1234567890_super_secret";
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
            email: "owner_a@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_a@business.com", "user", "premium")
        .unwrap();

    // 2. Create Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_a@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_a@business.com", "user", "free")
        .unwrap();

    // 3. Create Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Corp".to_string(),
            slug: "tenant-alpha-corp".to_string(),
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
            email: "user_b@beta.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "User B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "user_b@beta.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Ltd".to_string(),
            slug: "tenant-beta-ltd".to_string(),
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
    let resp = app.clone().oneshot(req).await.expect("Request failed");
    let status = resp.status();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json_val = serde_json::from_slice::<Value>(&body_bytes).unwrap_or(Value::Null);

    (status, json_val)
}

// ============================================================================
// FEATURE 8: Double-Entry Balancing Invariant
// ============================================================================

#[tokio::test]
async fn test_double_entry_balanced_post_success() {
    let harness = setup_harness().await;

    let payload = json!({
        "entry_date": "2026-10-02T00:00:00Z",
        "description": "Setoran Modal Bank",
        "lines": [
            {"account_code": "1100", "debit": 5000000, "credit": 0, "memo": "Debit Bank"},
            {"account_code": "4000", "debit": 0, "credit": 5000000, "memo": "Credit Pendapatan"}
        ]
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["total_debit"], 5000000);
    assert_eq!(body["total_credit"], 5000000);
    assert_eq!(body["status"], "POSTED");
    assert!(body["entry_number"].as_str().unwrap().starts_with("JRN-"));
    assert_eq!(body["lines"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_double_entry_compound_split_journal() {
    let harness = setup_harness().await;

    let split_lines = json!([
        {"account_code": "1000", "debit": 100000, "credit": 0},
        {"account_code": "1100", "debit": 150000, "credit": 0},
        {"account_code": "1200", "debit": 250000, "credit": 0},
        {"account_code": "4000", "debit": 0, "credit": 300000},
        {"account_code": "2000", "debit": 0, "credit": 100000},
        {"account_code": "2100", "debit": 0, "credit": 100000}
    ]);

    let payload = json!({
        "entry_date": "2026-10-03T00:00:00Z",
        "description": "6-Line Split Entry",
        "lines": split_lines
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["total_debit"], 500000);
    assert_eq!(body["total_credit"], 500000);
    assert_eq!(body["lines"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn test_double_entry_unbalanced_rejected_422() {
    let harness = setup_harness().await;

    let payload = json!({
        "entry_date": "2026-10-02T02:00:00Z",
        "description": "Unbalanced by 1 Rupiah",
        "lines": [
            {"account_code": "1000", "debit": 500001, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 500000}
        ]
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "UNBALANCED_JOURNAL_ENTRY");
}

#[tokio::test]
async fn test_double_entry_single_line_rejected_422() {
    let harness = setup_harness().await;

    let payload = json!({
        "entry_date": "2026-10-03T00:00:00Z",
        "description": "Single line attempt",
        "lines": [
            {"account_code": "1000", "debit": 100000, "credit": 0}
        ]
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "UNBALANCED_JOURNAL_ENTRY");
}

#[tokio::test]
async fn test_double_entry_all_zeros_rejected_422() {
    let harness = setup_harness().await;

    let payload = json!({
        "entry_date": "2026-10-03T00:00:00Z",
        "description": "All zeros",
        "lines": [
            {"account_code": "1000", "debit": 0, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 0}
        ]
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "UNBALANCED_JOURNAL_ENTRY");
}

#[tokio::test]
async fn test_double_entry_empty_lines_rejected_400() {
    let harness = setup_harness().await;

    let payload = json!({
        "entry_date": "2026-10-03T00:00:00Z",
        "description": "Empty lines",
        "lines": []
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_JOURNAL_LINES");
}

// ============================================================================
// FEATURE 9: Integer Rupiah Math Invariant
// ============================================================================

#[tokio::test]
async fn test_integer_rupiah_float_decimal_string_rejected_400() {
    let harness = setup_harness().await;

    // String float "100.50"
    let (status1, body1) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": "100.50", "tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status1, StatusCode::BAD_REQUEST);
    assert_eq!(body1["code"], "INVALID_AMOUNT");

    // Float number 100.50
    let (status2, body2) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 100.50, "tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status2, StatusCode::BAD_REQUEST);
    assert_eq!(body2["code"], "INVALID_AMOUNT");
}

#[tokio::test]
async fn test_integer_rupiah_negative_rejected_400() {
    let harness = setup_harness().await;

    let payload = json!({
        "entry_date": "2026-10-02T05:00:00Z",
        "description": "Negative Amount Attempt",
        "lines": [
            {"account_code": "1000", "debit": -50000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": -50000}
        ]
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(payload),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AMOUNT");
}

#[tokio::test]
async fn test_integer_rupiah_quadrillion_without_overflow() {
    let harness = setup_harness().await;

    // 9 Quadrillion IDR: Rp 9.000.000.000.000.000 at 0.5% UMKM = 45.000.000.000.000
    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "amount": 9000000000000000i64,
            "tax_type": "UMKM_05"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tax_amount"], 45000000000000i64);
    assert_eq!(body["base_amount"], 9000000000000000i64);
}

// ============================================================================
// FEATURE 10: Journal Immutability
// ============================================================================

#[tokio::test]
async fn test_journal_immutability_put_delete_patch_405() {
    let harness = setup_harness().await;

    // 1. Post a valid journal
    let post_payload = json!({
        "entry_date": "2026-10-03T00:00:00Z",
        "description": "Immutable Journal Target",
        "lines": [
            {"account_code": "1000", "debit": 250000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 250000}
        ]
    });

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(post_payload),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let j_id = body["id"].as_str().unwrap();

    // 2. Direct PUT must return 405 Method Not Allowed
    let (put_status, put_body) = send_request(
        &harness.app,
        Method::PUT,
        &format!("/api/v1/accounting/journals/{}", j_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"description": "Tampered"})),
    )
    .await;
    assert_eq!(put_status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(put_body["code"], "JOURNAL_IMMUTABLE");
    assert!(put_body["detail"].as_str().unwrap().to_lowercase().contains("reversal"));

    // 3. Direct DELETE must return 405 Method Not Allowed
    let (del_status, del_body) = send_request(
        &harness.app,
        Method::DELETE,
        &format!("/api/v1/accounting/journals/{}", j_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(del_status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(del_body["code"], "JOURNAL_IMMUTABLE");

    // 4. Direct PATCH must return 405 Method Not Allowed
    let (patch_status, patch_body) = send_request(
        &harness.app,
        Method::PATCH,
        &format!("/api/v1/accounting/journals/{}", j_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"status": "DRAFT"})),
    )
    .await;
    assert_eq!(patch_status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(patch_body["code"], "JOURNAL_IMMUTABLE");

    // 5. Verify journal remains intact
    let (get_status, get_body) = send_request(
        &harness.app,
        Method::GET,
        &format!("/api/v1/accounting/journals/{}", j_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["description"], "Immutable Journal Target");
    assert_eq!(get_body["status"], "POSTED");
}

// ============================================================================
// FEATURE 11: Journal Reversal Workflow
// ============================================================================

#[tokio::test]
async fn test_journal_reversal_lifecycle_and_inversion() {
    let harness = setup_harness().await;

    // 1. Post original journal: Debit Kas 350.000, Credit Pendapatan 350.000
    let post_payload = json!({
        "entry_date": "2026-10-03T00:00:00Z",
        "description": "Journal to Reverse",
        "lines": [
            {"account_code": "1000", "debit": 350000, "credit": 0, "memo": "Kas masuk"},
            {"account_code": "4000", "debit": 0, "credit": 350000, "memo": "Pendapatan"}
        ]
    });

    let (post_status, post_body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(post_payload),
    )
    .await;
    assert_eq!(post_status, StatusCode::CREATED);
    let orig_id = post_body["id"].as_str().unwrap();

    // 2. Reverse journal
    let rev_payload = json!({"reason": "Correction of posting error"});
    let (rev_status, rev_body) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(rev_payload),
    )
    .await;

    assert_eq!(rev_status, StatusCode::CREATED);
    assert_eq!(rev_body["source_type"], "REVERSAL");
    assert_eq!(rev_body["source_id"], orig_id);
    assert!(rev_body["entry_number"].as_str().unwrap().starts_with("REV-"));
    assert!(rev_body["description"].as_str().unwrap().contains("Correction of posting error"));

    // Verify swapped lines: Kas now Credit 350.000, Pendapatan now Debit 350.000
    let rev_lines = rev_body["lines"].as_array().unwrap();
    let line_1000 = rev_lines.iter().find(|l| l["account_code"] == "1000").unwrap();
    let line_4000 = rev_lines.iter().find(|l| l["account_code"] == "4000").unwrap();
    assert_eq!(line_1000["credit"], 350000);
    assert_eq!(line_1000["debit"], 0);
    assert_eq!(line_4000["debit"], 350000);
    assert_eq!(line_4000["credit"], 0);

    // 3. Verify original journal marked as is_reversed = 1
    let (get_orig_status, get_orig_body) = send_request(
        &harness.app,
        Method::GET,
        &format!("/api/v1/accounting/journals/{}", orig_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(get_orig_status, StatusCode::OK);
    assert_eq!(get_orig_body["is_reversed"], 1);

    // 4. Duplicate reversal attempt returns 409 ALREADY_REVERSED
    let (dup_status, dup_body) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Duplicate attempt"})),
    )
    .await;
    assert_eq!(dup_status, StatusCode::CONFLICT);
    assert_eq!(dup_body["code"], "ALREADY_REVERSED");

    // 5. Net trial balance impact is balanced to zero
    let (tb_status, tb_body) = send_request(
        &harness.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(tb_status, StatusCode::OK);
    assert_eq!(tb_body["is_balanced"], true);
    assert_eq!(tb_body["net_balance"], 0);
}

#[tokio::test]
async fn test_journal_reversal_staff_forbidden_403() {
    let harness = setup_harness().await;

    // 1. Post journal as Owner
    let (post_status, post_body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "entry_date": "2026-10-03T00:00:00Z",
            "description": "Target Journal",
            "lines": [
                {"account_code": "1000", "debit": 100000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100000}
            ]
        })),
    )
    .await;
    assert_eq!(post_status, StatusCode::CREATED);
    let orig_id = post_body["id"].as_str().unwrap();

    // 2. Staff attempts reversal -> HTTP 403 Forbidden
    let (rev_status, rev_body) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", orig_id),
        &harness.staff_token,
        Some(&harness.tenant_a_id),
        Some(json!({"reason": "Staff attempt"})),
    )
    .await;

    assert_eq!(rev_status, StatusCode::FORBIDDEN);
    assert_eq!(rev_body["code"], "FORBIDDEN");
}

// ============================================================================
// FEATURE 12: Standard Chart of Accounts & Protection
// ============================================================================

#[tokio::test]
async fn test_chart_of_accounts_seeding_and_protection() {
    let harness = setup_harness().await;

    // 1. List accounts for Tenant A: must contain all 8 system accounts
    let (status, body) = send_request(
        &harness.app,
        Method::GET,
        "/api/v1/accounting/accounts",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let accounts = body["accounts"].as_array().unwrap();
    assert!(accounts.len() >= 8);

    let codes: Vec<&str> = accounts
        .iter()
        .map(|a| a["code"].as_str().unwrap())
        .collect();
    for sys_code in ["1000", "1100", "1200", "2000", "2100", "4000", "5000", "6000"] {
        assert!(codes.contains(&sys_code), "Missing system account {}", sys_code);
    }

    // 2. Deleting system accounts 1000, 1200, 4000 returns HTTP 403 SYSTEM_ACCOUNT_PROTECTED
    for protected in ["1000", "1200", "4000"] {
        let (del_status, del_body) = send_request(
            &harness.app,
            Method::DELETE,
            &format!("/api/v1/accounting/accounts/{}", protected),
            &harness.owner_token,
            Some(&harness.tenant_a_id),
            None,
        )
        .await;

        assert_eq!(del_status, StatusCode::FORBIDDEN);
        assert_eq!(del_body["code"], "SYSTEM_ACCOUNT_PROTECTED");
    }
}

#[tokio::test]
async fn test_chart_of_accounts_duplicate_code_409() {
    let harness = setup_harness().await;

    // Attempting to create duplicate code 1000 returns 409
    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "code": "1000",
            "name": "Kas Duplikat",
            "account_type": "asset"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "ACCOUNT_ALREADY_EXISTS");
}

#[tokio::test]
async fn test_chart_of_accounts_custom_create_delete() {
    let harness = setup_harness().await;

    // 1. Create custom account 6999
    let (c_status, c_body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "code": "6999",
            "name": "Biaya Lain-Lain",
            "account_type": "expense"
        })),
    )
    .await;

    assert_eq!(c_status, StatusCode::CREATED);
    assert_eq!(c_body["code"], "6999");
    assert_eq!(c_body["is_system"], false);

    // 2. Delete custom account 6999
    let (d_status, d_body) = send_request(
        &harness.app,
        Method::DELETE,
        "/api/v1/accounting/accounts/6999",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        None,
    )
    .await;

    assert_eq!(d_status, StatusCode::OK);
    assert_eq!(d_body["status"], "deleted");
    assert_eq!(d_body["code"], "6999");
}

// ============================================================================
// FEATURE 13, 14, 15: Indonesian Tax Calculation Engine
// ============================================================================

#[tokio::test]
async fn test_indonesian_tax_golden_vectors() {
    let harness = setup_harness().await;

    // 1. PPN 11% Exclusive: Rp 100.000 -> 11.000 tax, 111.000 gross
    let (s1, b1) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 100000, "tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(b1["tax_amount"], 11000);
    assert_eq!(b1["gross_amount"], 111000);

    // 2. PPN 11% Inclusive: Rp 111.000 -> 11.000 tax, 100.000 net (net + tax == gross)
    let (s2, b2) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 111000, "tax_type": "PPN_11_INCL"})),
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["tax_amount"], 11000);
    assert_eq!(b2["net_amount"], 100000);
    assert_eq!(b2["net_amount"].as_i64().unwrap() + b2["tax_amount"].as_i64().unwrap(), 111000);

    // 3. PPN 12% Exclusive: Rp 100.000 -> 12.000 tax, 112.000 gross
    let (s3, b3) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 100000, "tax_type": "PPN_12_EXCL"})),
    )
    .await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(b3["tax_amount"], 12000);
    assert_eq!(b3["gross_amount"], 112000);

    // 4. PPN 12% Inclusive: Rp 112.000 -> 12.000 tax, 100.000 net
    let (s4, b4) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 112000, "tax_type": "PPN_12_INCL"})),
    )
    .await;
    assert_eq!(s4, StatusCode::OK);
    assert_eq!(b4["tax_amount"], 12000);
    assert_eq!(b4["net_amount"], 100000);

    // 5. Half-up rounding edge: 105 * 11% = 11.55 -> rounds to 12
    let (s5, b5) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 105, "tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(s5, StatusCode::OK);
    assert_eq!(b5["tax_amount"], 12);

    // 6. UMKM 0.5% exact half edge: Rp 100 -> 0.50 rounds up to 1
    let (s6, b6) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 100, "tax_type": "UMKM_05"})),
    )
    .await;
    assert_eq!(s6, StatusCode::OK);
    assert_eq!(b6["tax_amount"], 1);

    // 7. UMKM 0.5% under half edge: Rp 99 -> 0.495 rounds down to 0
    let (s7, b7) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 99, "tax_type": "UMKM_05"})),
    )
    .await;
    assert_eq!(s7, StatusCode::OK);
    assert_eq!(b7["tax_amount"], 0);

    // 8. UMKM statutory ceiling: Rp 4.800.000.000 -> 24.000.000
    let (s8, b8) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 4800000000i64, "tax_type": "UMKM_05"})),
    )
    .await;
    assert_eq!(s8, StatusCode::OK);
    assert_eq!(b8["tax_amount"], 24000000);

    // 9. Exempt: Rp 250.000 -> 0 tax, net == gross == 250.000
    let (s9, b9) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 250000, "tax_type": "EXEMPT"})),
    )
    .await;
    assert_eq!(s9, StatusCode::OK);
    assert_eq!(b9["tax_amount"], 0);
    assert_eq!(b9["net_amount"], 250000);
    assert_eq!(b9["gross_amount"], 250000);

    // 10. Unsupported tax type -> 400 INVALID_TAX_TYPE
    let (s10, b10) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/tax/calculate",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({"amount": 100000, "tax_type": "PPN_99_SUPER"})),
    )
    .await;
    assert_eq!(s10, StatusCode::BAD_REQUEST);
    assert_eq!(b10["code"], "INVALID_TAX_TYPE");
}

// ============================================================================
// Multi-Tenant Cross-Access Isolation (HTTP 404 Anti-Enumeration)
// ============================================================================

#[tokio::test]
async fn test_cross_tenant_accounting_isolation_strictly_returns_404() {
    let harness = setup_harness().await;

    // 1. Post a journal in Tenant A
    let (post_status, post_body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "entry_date": "2026-10-02T00:00:00Z",
            "description": "Tenant A Secret Journal",
            "lines": [
                {"account_code": "1000", "debit": 1000000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 1000000}
            ]
        })),
    )
    .await;
    assert_eq!(post_status, StatusCode::CREATED);
    let tenant_a_j_id = post_body["id"].as_str().unwrap();

    // 2. Tenant B user attempts to fetch Tenant A's journal by ID
    // Anti-enumeration invariant: MUST strictly return HTTP 404 Not Found (NEVER 403)
    let (cross_status, cross_body) = send_request(
        &harness.app,
        Method::GET,
        &format!("/api/v1/accounting/journals/{}", tenant_a_j_id),
        &harness.tenant_b_token,
        Some(&harness.tenant_b_id),
        None,
    )
    .await;

    assert_eq!(cross_status, StatusCode::NOT_FOUND);
    assert_eq!(cross_body["code"], "NOT_FOUND");

    // 3. Tenant B user attempts to reverse Tenant A's journal
    let (cross_rev_status, cross_rev_body) = send_request(
        &harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", tenant_a_j_id),
        &harness.tenant_b_token,
        Some(&harness.tenant_b_id),
        Some(json!({"reason": "Malicious reversal"})),
    )
    .await;

    assert_eq!(cross_rev_status, StatusCode::NOT_FOUND);
    assert_eq!(cross_rev_body["code"], "NOT_FOUND");
}

// ============================================================================
// REMEDIATION: Role Harness Setup
// ============================================================================

struct RoleHarness {
    harness: TestHarness,
    admin_token: String,
    accountant_token: String,
    manager_token: String,
}

async fn setup_role_harness() -> RoleHarness {
    let harness = setup_harness().await;
    let pool = harness.pool.clone();
    let jwt_engine = harness.jwt_engine.clone();
    let user_repo = SqlxUserRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    // Create Admin
    let admin_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: admin_id.clone(),
            email: "admin_test@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Admin User".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();
    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: harness.tenant_a_id.clone(),
            user_id: admin_id.clone(),
            role: Role::Administrator,
        })
        .await
        .unwrap();
    let (admin_token, _) = jwt_engine
        .generate_token(&admin_id, "admin_test@business.com", "user", "premium")
        .unwrap();

    // Create Accountant
    let acct_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: acct_id.clone(),
            email: "accountant_test@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Accountant User".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();
    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: harness.tenant_a_id.clone(),
            user_id: acct_id.clone(),
            role: Role::Accountant,
        })
        .await
        .unwrap();
    let (accountant_token, _) = jwt_engine
        .generate_token(&acct_id, "accountant_test@business.com", "user", "premium")
        .unwrap();

    // Create Manager
    let mgr_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: mgr_id.clone(),
            email: "manager_test@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Manager User".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();
    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: harness.tenant_a_id.clone(),
            user_id: mgr_id.clone(),
            role: Role::Manager,
        })
        .await
        .unwrap();
    let (manager_token, _) = jwt_engine
        .generate_token(&mgr_id, "manager_test@business.com", "user", "free")
        .unwrap();

    RoleHarness {
        harness,
        admin_token,
        accountant_token,
        manager_token,
    }
}

// ============================================================================
// REMEDIATION: SQLite Immutability Triggers Verification
// ============================================================================

#[tokio::test]
async fn test_sqlite_trigger_prevent_update_on_posted_journal_lines() {
    let harness = setup_harness().await;
    let pool = &harness.pool;

    let j_id = format!("j_trg_{}", Uuid::new_v4());
    let l1_id = format!("l_trg_1_{}", Uuid::new_v4());
    let l2_id = format!("l_trg_2_{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, status, is_reversed, created_at, updated_at)
        VALUES (?1, ?2, 'JRN-2026-TRG001', ?3, 'Direct Trigger Test', 'MANUAL', 'POSTED', 0, ?3, ?3)
        "#,
    )
    .bind(&j_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo, created_at)
        VALUES (?1, ?2, ?3, '1000', 500000, 0, 'Line 1', ?4),
               (?5, ?2, ?3, '4000', 0, 500000, 'Line 2', ?4)
        "#,
    )
    .bind(&l1_id)
    .bind(&j_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .bind(&l2_id)
    .execute(pool)
    .await
    .unwrap();

    // Adversarial Action: Direct SQL UPDATE on debit of posted line
    let res_debit = sqlx::query("UPDATE journal_lines SET debit = 999999 WHERE id = ?1")
        .bind(&l1_id)
        .execute(pool)
        .await;

    assert!(res_debit.is_err(), "Trigger failed! Direct UPDATE on debit succeeded");
    let err_msg = res_debit.unwrap_err().to_string();
    assert!(
        err_msg.contains("Journal lines of posted journals cannot be modified")
            || err_msg.contains("immutable"),
        "Unexpected error: {}",
        err_msg
    );

    // Verify row unaltered
    let row: (i64,) = sqlx::query_as("SELECT debit FROM journal_lines WHERE id = ?1")
        .bind(&l1_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row.0, 500000);
}

#[tokio::test]
async fn test_sqlite_trigger_prevent_delete_on_posted_journal_lines() {
    let harness = setup_harness().await;
    let pool = &harness.pool;

    let j_id = format!("j_trg_del_{}", Uuid::new_v4());
    let l_id = format!("l_trg_del_{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, status, is_reversed, created_at, updated_at) VALUES (?1, ?2, 'JRN-2026-TRG002', ?3, 'Delete Test', 'MANUAL', 'POSTED', 0, ?3, ?3)"
    )
    .bind(&j_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo, created_at) VALUES (?1, ?2, ?3, '1000', 100000, 0, 'Line', ?4)"
    )
    .bind(&l_id)
    .bind(&j_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    // Adversarial Action: Direct SQL DELETE on posted line
    let res_del = sqlx::query("DELETE FROM journal_lines WHERE id = ?1")
        .bind(&l_id)
        .execute(pool)
        .await;

    assert!(res_del.is_err(), "Trigger failed! Direct DELETE on posted line succeeded");
    let err_msg = res_del.unwrap_err().to_string();
    assert!(
        err_msg.contains("Journal lines of posted journals cannot be deleted")
            || err_msg.contains("immutable"),
        "Unexpected error: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_sqlite_trigger_prevent_status_downgrade_to_draft() {
    let harness = setup_harness().await;
    let pool = &harness.pool;

    let j_id = format!("j_trg_st_{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, status, is_reversed, created_at, updated_at) VALUES (?1, ?2, 'JRN-2026-TRG003', ?3, 'Status Test', 'MANUAL', 'POSTED', 0, ?3, ?3)"
    )
    .bind(&j_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    // Adversarial Action: Status Downgrade to DRAFT
    let res_status = sqlx::query("UPDATE journal_entries SET status = 'DRAFT' WHERE id = ?1")
        .bind(&j_id)
        .execute(pool)
        .await;

    assert!(res_status.is_err(), "Trigger failed! Updating status from POSTED to DRAFT succeeded");
    let err_msg = res_status.unwrap_err().to_string();
    assert!(
        err_msg.contains("Posted journals are immutable and cannot be modified"),
        "Unexpected error: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_sqlite_trigger_draft_journal_mutations_succeed() {
    let harness = setup_harness().await;
    let pool = &harness.pool;

    let j_id = format!("j_draft_{}", Uuid::new_v4());
    let l_id = format!("l_draft_{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, status, is_reversed, created_at, updated_at) VALUES (?1, ?2, 'JRN-2026-DRAFT01', ?3, 'Draft Entry', 'MANUAL', 'DRAFT', 0, ?3, ?3)"
    )
    .bind(&j_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO journal_lines (id, journal_id, tenant_id, account_code, debit, credit, memo, created_at) VALUES (?1, ?2, ?3, '1000', 50000, 0, 'Draft Line', ?4)"
    )
    .bind(&l_id)
    .bind(&j_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    let upd_res = sqlx::query("UPDATE journal_lines SET debit = 75000 WHERE id = ?1")
        .bind(&l_id)
        .execute(pool)
        .await;
    assert!(upd_res.is_ok());

    let del_res = sqlx::query("DELETE FROM journal_lines WHERE id = ?1")
        .bind(&l_id)
        .execute(pool)
        .await;
    assert!(del_res.is_ok());

    let j_del_res = sqlx::query("DELETE FROM journal_entries WHERE id = ?1")
        .bind(&j_id)
        .execute(pool)
        .await;
    assert!(j_del_res.is_ok());
}

#[tokio::test]
async fn test_sqlite_trigger_legitimate_reversal_update_succeeds() {
    let harness = setup_harness().await;
    let pool = &harness.pool;

    let orig_id = format!("j_orig_{}", Uuid::new_v4());
    let rev_id = format!("j_rev_{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, status, is_reversed, created_at, updated_at) VALUES (?1, ?2, 'JRN-2026-REVORIG', ?3, 'Original Entry', 'MANUAL', 'POSTED', 0, ?3, ?3)"
    )
    .bind(&orig_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO journal_entries (id, tenant_id, entry_number, entry_date, description, source_type, source_id, status, is_reversed, created_at, updated_at) VALUES (?1, ?2, 'REV-2026-000001', ?3, 'Reversal', 'REVERSAL', ?4, 'POSTED', 0, ?3, ?3)"
    )
    .bind(&rev_id)
    .bind(&harness.tenant_a_id)
    .bind(&now)
    .bind(&orig_id)
    .execute(pool)
    .await
    .unwrap();

    let upd_res = sqlx::query(
        "UPDATE journal_entries SET is_reversed = 1, reversal_entry_id = ?1, updated_at = ?2 WHERE id = ?3"
    )
    .bind(&rev_id)
    .bind(&now)
    .bind(&orig_id)
    .execute(pool)
    .await;

    assert!(upd_res.is_ok(), "Legitimate reversal update must succeed");
}

// ============================================================================
// REMEDIATION: RBAC Authorization Matrix Tests
// ============================================================================

#[tokio::test]
async fn test_rbac_post_journal_authorization_matrix() {
    let rh = setup_role_harness().await;

    let payload = json!({
        "description": "RBAC Posting Test",
        "lines": [
            {"account_code": "1000", "debit": 100000, "credit": 0},
            {"account_code": "4000", "debit": 0, "credit": 100000}
        ]
    });

    // 1. Staff -> FORBIDDEN (403)
    let (s_staff, b_staff) = send_request(
        &rh.harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &rh.harness.staff_token,
        Some(&rh.harness.tenant_a_id),
        Some(payload.clone()),
    ).await;
    assert_eq!(s_staff, StatusCode::FORBIDDEN);
    assert_eq!(b_staff["code"], "FORBIDDEN");

    // 2. Manager -> FORBIDDEN (403)
    let (s_mgr, b_mgr) = send_request(
        &rh.harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &rh.manager_token,
        Some(&rh.harness.tenant_a_id),
        Some(payload.clone()),
    ).await;
    assert_eq!(s_mgr, StatusCode::FORBIDDEN);
    assert_eq!(b_mgr["code"], "FORBIDDEN");

    // 3. Accountant -> ALLOWED (201)
    let (s_acct, _) = send_request(
        &rh.harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &rh.accountant_token,
        Some(&rh.harness.tenant_a_id),
        Some(payload.clone()),
    ).await;
    assert_eq!(s_acct, StatusCode::CREATED);

    // 4. Administrator -> ALLOWED (201)
    let (s_admin, _) = send_request(
        &rh.harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &rh.admin_token,
        Some(&rh.harness.tenant_a_id),
        Some(payload.clone()),
    ).await;
    assert_eq!(s_admin, StatusCode::CREATED);

    // 5. Owner -> ALLOWED (201)
    let (s_owner, _) = send_request(
        &rh.harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &rh.harness.owner_token,
        Some(&rh.harness.tenant_a_id),
        Some(payload),
    ).await;
    assert_eq!(s_owner, StatusCode::CREATED);
}

#[tokio::test]
async fn test_rbac_reverse_journal_authorization_matrix() {
    let rh = setup_role_harness().await;

    let post_journal = |desc: &str| {
        let app = rh.harness.app.clone();
        let token = rh.harness.owner_token.clone();
        let tenant_id = rh.harness.tenant_a_id.clone();
        let desc = desc.to_string();
        async move {
            let (status, body) = send_request(
                &app,
                Method::POST,
                "/api/v1/accounting/journals",
                &token,
                Some(&tenant_id),
                Some(json!({
                    "description": desc,
                    "lines": [
                        {"account_code": "1000", "debit": 50000, "credit": 0},
                        {"account_code": "4000", "debit": 0, "credit": 50000}
                    ]
                })),
            ).await;
            assert_eq!(status, StatusCode::CREATED);
            body["id"].as_str().unwrap().to_string()
        }
    };

    // 1. Staff -> FORBIDDEN (403)
    let j1 = post_journal("For Staff Reversal").await;
    let (s_staff, b_staff) = send_request(
        &rh.harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j1),
        &rh.harness.staff_token,
        Some(&rh.harness.tenant_a_id),
        Some(json!({"reason": "Staff attempt"})),
    ).await;
    assert_eq!(s_staff, StatusCode::FORBIDDEN);
    assert_eq!(b_staff["code"], "FORBIDDEN");

    // 2. Manager -> FORBIDDEN (403)
    let j2 = post_journal("For Manager Reversal").await;
    let (s_mgr, b_mgr) = send_request(
        &rh.harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j2),
        &rh.manager_token,
        Some(&rh.harness.tenant_a_id),
        Some(json!({"reason": "Manager attempt"})),
    ).await;
    assert_eq!(s_mgr, StatusCode::FORBIDDEN);
    assert_eq!(b_mgr["code"], "FORBIDDEN");

    // 3. Administrator -> FORBIDDEN (403) (Role::can_reverse_journal() allows Owner | Accountant)
    let j3 = post_journal("For Admin Reversal").await;
    let (s_admin, b_admin) = send_request(
        &rh.harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j3),
        &rh.admin_token,
        Some(&rh.harness.tenant_a_id),
        Some(json!({"reason": "Admin attempt"})),
    ).await;
    assert_eq!(s_admin, StatusCode::FORBIDDEN);
    assert_eq!(b_admin["code"], "FORBIDDEN");

    // 4. Accountant -> ALLOWED (201)
    let j4 = post_journal("For Accountant Reversal").await;
    let (s_acct, b_acct) = send_request(
        &rh.harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j4),
        &rh.accountant_token,
        Some(&rh.harness.tenant_a_id),
        Some(json!({"reason": "Accountant correction"})),
    ).await;
    assert_eq!(s_acct, StatusCode::CREATED);
    assert_eq!(b_acct["source_type"], "REVERSAL");

    // 5. Owner -> ALLOWED (201)
    let j5 = post_journal("For Owner Reversal").await;
    let (s_owner, b_owner) = send_request(
        &rh.harness.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", j5),
        &rh.harness.owner_token,
        Some(&rh.harness.tenant_a_id),
        Some(json!({"reason": "Owner correction"})),
    ).await;
    assert_eq!(s_owner, StatusCode::CREATED);
    assert_eq!(b_owner["source_type"], "REVERSAL");
}

// ============================================================================
// REMEDIATION: Account Validation & Simultaneous Line Error Tests
// ============================================================================

#[tokio::test]
async fn test_post_journal_strictly_rejects_nonexistent_and_foreign_accounts() {
    let harness = setup_harness().await;

    // 1. Tenant B creates custom account "7777"
    let (ca_s, ca_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &harness.tenant_b_token,
        Some(&harness.tenant_b_id),
        Some(json!({
            "code": "7777",
            "name": "Tenant B Exclusive Vault",
            "account_type": "asset"
        })),
    ).await;
    assert_eq!(ca_s, StatusCode::CREATED);
    assert_eq!(ca_b["code"], "7777");

    // Scenario A: Cross-tenant account "7777"
    let (s_a, b_a) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Cross-Tenant Attack",
            "lines": [
                {"account_code": "7777", "debit": 250000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 250000}
            ]
        })),
    ).await;
    assert_eq!(s_a, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b_a["code"], "ACCOUNT_NOT_FOUND");

    // Scenario B: Non-existent account "9999"
    let (s_b, b_b) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Bogus Account",
            "lines": [
                {"account_code": "9999", "debit": 100000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100000}
            ]
        })),
    ).await;
    assert_eq!(s_b, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b_b["code"], "ACCOUNT_NOT_FOUND");

    // Scenario C: Compound entry with 1 invalid line
    let (s_c, b_c) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Compound Partial Bogus",
            "lines": [
                {"account_code": "1000", "debit": 300000, "credit": 0},
                {"account_code": "1100", "debit": 200000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 400000},
                {"account_code": "8888", "debit": 0, "credit": 100000}
            ]
        })),
    ).await;
    assert_eq!(s_c, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b_c["code"], "ACCOUNT_NOT_FOUND");
}

#[tokio::test]
async fn test_post_journal_rejects_simultaneous_debit_credit_400() {
    let harness = setup_harness().await;

    let (status, body) = send_request(
        &harness.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &harness.owner_token,
        Some(&harness.tenant_a_id),
        Some(json!({
            "description": "Simultaneous debit and credit line",
            "lines": [
                {"account_code": "1000", "debit": 100000, "credit": 100000},
                {"account_code": "4000", "debit": 0, "credit": 0}
            ]
        })),
    ).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_JOURNAL_LINES");
}

// ============================================================================
// REMEDIATION: Inter-Milestone Contract Test (post_journal_command)
// ============================================================================

#[tokio::test]
async fn test_post_journal_command_inter_milestone_contract() {
    let harness = setup_harness().await;
    let service = AccountingService::new_with_pool(harness.pool.clone());

    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let actor_id = Uuid::new_v4();
    let ctx = TenantContext::new(tenant_uuid, actor_id, Role::Owner);

    // 1. Success posting with PostJournalEntryCommand
    let cmd = PostJournalEntryCommand {
        tenant_id: tenant_uuid,
        entry_date: Utc::now(),
        description: "Programmatic Invoice Journal".to_string(),
        source_type: "INVOICE".to_string(),
        source_id: Some(Uuid::new_v4()),
        lines: vec![
            PostJournalLineCommand {
                account_code: "1200".to_string(),
                debit: Rupiah::new(1110000),
                credit: Rupiah::new(0),
                memo: Some("Receivable from Customer".to_string()),
            },
            PostJournalLineCommand {
                account_code: "4000".to_string(),
                debit: Rupiah::new(0),
                credit: Rupiah::new(1000000),
                memo: Some("Revenue".to_string()),
            },
            PostJournalLineCommand {
                account_code: "2100".to_string(),
                debit: Rupiah::new(0),
                credit: Rupiah::new(110000),
                memo: Some("PPN 11%".to_string()),
            },
        ],
    };

    let result = service.post_journal_command(&ctx, cmd).await;
    assert!(result.is_ok(), "Programmatic journal posting failed: {:?}", result.err());
    let entry = result.unwrap();
    assert_eq!(entry.status, "POSTED");
    assert_eq!(entry.source_type, "INVOICE");
    assert_eq!(entry.total_debit, 1110000);
    assert_eq!(entry.total_credit, 1110000);
    assert_eq!(entry.lines.len(), 3);

    // 2. Reject on tenant ID mismatch
    let foreign_uuid = Uuid::new_v4();
    let bad_tenant_cmd = PostJournalEntryCommand {
        tenant_id: foreign_uuid,
        entry_date: Utc::now(),
        description: "Tenant mismatch".to_string(),
        source_type: "MANUAL".to_string(),
        source_id: None,
        lines: vec![
            PostJournalLineCommand {
                account_code: "1000".to_string(),
                debit: Rupiah::new(50000),
                credit: Rupiah::new(0),
                memo: None,
            },
            PostJournalLineCommand {
                account_code: "4000".to_string(),
                debit: Rupiah::new(0),
                credit: Rupiah::new(50000),
                memo: None,
            },
        ],
    };
    let bad_t_res = service.post_journal_command(&ctx, bad_tenant_cmd).await;
    assert!(bad_t_res.is_err());

    // 3. Reject on unauthorized role (Staff)
    let staff_ctx = TenantContext::new(tenant_uuid, actor_id, Role::Staff);
    let valid_cmd = PostJournalEntryCommand {
        tenant_id: tenant_uuid,
        entry_date: Utc::now(),
        description: "Staff unauthorized attempt".to_string(),
        source_type: "MANUAL".to_string(),
        source_id: None,
        lines: vec![
            PostJournalLineCommand {
                account_code: "1000".to_string(),
                debit: Rupiah::new(50000),
                credit: Rupiah::new(0),
                memo: None,
            },
            PostJournalLineCommand {
                account_code: "4000".to_string(),
                debit: Rupiah::new(0),
                credit: Rupiah::new(50000),
                memo: None,
            },
        ],
    };
    let staff_res = service.post_journal_command(&staff_ctx, valid_cmd).await;
    assert!(staff_res.is_err());

    // 4. Reject on non-existent account code
    let bogus_acc_cmd = PostJournalEntryCommand {
        tenant_id: tenant_uuid,
        entry_date: Utc::now(),
        description: "Bogus account command".to_string(),
        source_type: "MANUAL".to_string(),
        source_id: None,
        lines: vec![
            PostJournalLineCommand {
                account_code: "9999".to_string(),
                debit: Rupiah::new(50000),
                credit: Rupiah::new(0),
                memo: None,
            },
            PostJournalLineCommand {
                account_code: "4000".to_string(),
                debit: Rupiah::new(0),
                credit: Rupiah::new(50000),
                memo: None,
            },
        ],
    };
    let bogus_res = service.post_journal_command(&ctx, bogus_acc_cmd).await;
    assert!(bogus_res.is_err());
}

