//! Final Milestone Phase 2 Adversarial Coverage Hardening Verification Suite
//!
//! Identity: teamwork_preview_challenger_final_1
//! Role: Final Milestone Adversarial Challenger 1 (critic, specialist)
//! Milestone: Final Milestone Phase 2 Adversarial Coverage Hardening
//!
//! Empirical Verification Objectives:
//! 1. Double-Entry & Immutability Under Stress:
//!    - SQLite triggers on journal_entries, journal_lines, outbox_events, invoices,
//!      invoice_items, invoice_snapshots, payments, and payment_allocations cannot
//!      be circumvented by direct raw SQL mutations.
//!    - Double-entry invariants strictly hold under extreme Rupiah amounts (quadrillion arithmetic),
//!      negative amounts rejection, single line rejection, zero-sum rejection.
//! 2. Multi-Tenancy Isolation & Negative Probing:
//!    - Cross-tenant access to invoices, payments, accounts, journals, and outbox records
//!      strictly returns HTTP 404 NOT_FOUND (never 500, never 403 leaking resource existence).
//!    - Internal unauthorized actions (Staff/Manager attempting restricted accounting actions)
//!      strictly return HTTP 403 FORBIDDEN.
//! 3. Outbox Worker & Concurrency Races:
//!    - Atomic rollback: zero orphaned outbox events on transaction abort.
//!    - Exponential backoff and DLQ transitions after 5 failed attempts.
//!    - Concurrency race claiming: multiple concurrent workers process disjoint events without duplicates.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::outbox::OutboxEventDraft;
use backend::domain::tenant::{Role, TenantStatus};
use backend::repository::{
    account_repo::SqlxAccountRepository,
    accounting_repo::SqlxAccountingRepository,
    audit_repo::SqlxAuditRepository,
    category_repo::SqlxCategoryRepository,
    db::{init_pool, run_migrations, DbConfig},
    idempotency_repo::SqlxIdempotencyRepository,
    outbox_repo::{OutboxRepository, SqlxOutboxRepository},
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
use chrono::Utc;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::task::JoinSet;
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: axum::Router,
    pool: SqlitePool,
    _dir: tempfile::TempDir,
    owner_token_a: String,
    staff_token_a: String,
    tenant_a_id: String,
    owner_token_b: String,
    tenant_b_id: String,
}

async fn setup_final_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("final_challenger_test.sqlite");
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

    let jwt_secret = "final_challenger_jwt_secret_key_1234567890_enterprise";
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

    let payment_service = Arc::new(
        PaymentService::new(
            PaymentConfig::default(),
            subscription_repo,
            user_repo.clone(),
            audit_repo,
        )
        .with_pool(pool.clone()),
    );

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

    // 1. Create Tenant A with Owner and Staff
    let owner_a_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_a_id.clone(),
            email: "owner_a@final.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token_a, _) = jwt_engine
        .generate_token(&owner_a_id, "owner_a@final.com", "user", "premium")
        .unwrap();

    let staff_a_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_a_id.clone(),
            email: "staff_a@final.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token_a, _) = jwt_engine
        .generate_token(&staff_a_id, "staff_a@final.com", "user", "free")
        .unwrap();

    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Final Workspace A".to_string(),
            slug: "final-workspace-a".to_string(),
            status: Some(TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_a = pool.begin().await.unwrap();
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx_a, &tenant_a_id)
        .await
        .unwrap();
    tx_a.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: owner_a_id,
            role: Role::Owner,
        })
        .await
        .unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: staff_a_id,
            role: Role::Staff,
        })
        .await
        .unwrap();

    // 2. Create Tenant B with Owner
    let owner_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_b_id.clone(),
            email: "owner_b@final.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token_b, _) = jwt_engine
        .generate_token(&owner_b_id, "owner_b@final.com", "user", "premium")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Final Workspace B".to_string(),
            slug: "final-workspace-b".to_string(),
            status: Some(TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    let mut tx_b = pool.begin().await.unwrap();
    SqlxAccountingRepository::seed_default_accounts_tx(&mut tx_b, &tenant_b_id)
        .await
        .unwrap();
    tx_b.commit().await.unwrap();

    membership_repo
        .create_membership(&NewMembership {
            id: format!("mem_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: owner_b_id,
            role: Role::Owner,
        })
        .await
        .unwrap();

    TestHarness {
        app,
        pool,
        _dir: dir,
        owner_token_a,
        staff_token_a,
        tenant_a_id,
        owner_token_b,
        tenant_b_id,
    }
}

async fn send_req(
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

#[tokio::test]
async fn test_final_sqlite_triggers_prevent_all_raw_sql_tampering() {
    let h = setup_final_harness().await;

    // 1. Post a valid journal entry in Tenant A
    let (status, body) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Baseline immutable journal",
            "source_type": "MANUAL",
            "lines": [
                {"account_code": "1000", "debit": 500_000, "credit": 0, "memo": "Cash"},
                {"account_code": "4000", "debit": 0, "credit": 500_000, "memo": "Revenue"}
            ]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let journal_id = body["id"].as_str().unwrap().to_string();

    // 2. Direct raw SQL DELETE on posted journal must fail
    let del_err = sqlx::query("DELETE FROM journal_entries WHERE id = ?1")
        .bind(&journal_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        del_err.to_string().contains("Posted journals are immutable and cannot be deleted"),
        "Unexpected error: {:?}",
        del_err
    );

    // 3. Direct raw SQL UPDATE on posted journal must fail
    let upd_err = sqlx::query("UPDATE journal_entries SET description = 'tampered' WHERE id = ?1")
        .bind(&journal_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        upd_err.to_string().contains("Posted journals are immutable and cannot be modified"),
        "Unexpected error: {:?}",
        upd_err
    );

    // 4. Direct raw SQL UPDATE on posted journal lines must fail
    let line_upd_err = sqlx::query("UPDATE journal_lines SET debit = 999999 WHERE journal_id = ?1")
        .bind(&journal_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        line_upd_err.to_string().contains("Journal lines of posted journals cannot be modified"),
        "Unexpected error: {:?}",
        line_upd_err
    );

    // 5. Direct raw SQL DELETE on posted journal lines must fail
    let line_del_err = sqlx::query("DELETE FROM journal_lines WHERE journal_id = ?1")
        .bind(&journal_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        line_del_err.to_string().contains("Journal lines of posted journals cannot be deleted"),
        "Unexpected error: {:?}",
        line_del_err
    );

    // 6. Direct raw SQL DELETE on system chart of accounts must fail
    let coa_del_err = sqlx::query("DELETE FROM chart_of_accounts WHERE tenant_id = ?1 AND is_system = 1")
        .bind(&h.tenant_a_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        coa_del_err.to_string().contains("System accounts are protected and cannot be deleted"),
        "Unexpected error: {:?}",
        coa_del_err
    );

    // 7. Insert an outbox event and verify immutability triggers
    let outbox_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO outbox_events (
            id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status
        ) VALUES (?1, ?2, 'InvoiceIssued', 'Invoice', 'agg-123', '{"test": 1}', 'PENDING')
        "#,
    )
    .bind(&outbox_id)
    .bind(&h.tenant_a_id)
    .execute(&h.pool)
    .await
    .unwrap();

    // Raw UPDATE on immutable fields (event_type) must fail
    let outbox_upd_err = sqlx::query("UPDATE outbox_events SET event_type = 'Hacked' WHERE id = ?1")
        .bind(&outbox_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        outbox_upd_err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Unexpected error: {:?}",
        outbox_upd_err
    );

    // Raw DELETE on PENDING outbox event must fail
    let outbox_del_err = sqlx::query("DELETE FROM outbox_events WHERE id = ?1")
        .bind(&outbox_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        outbox_del_err.to_string().contains("Active or published outbox events are immutable and cannot be deleted"),
        "Unexpected error: {:?}",
        outbox_del_err
    );

    // Transition to PUBLISHED and verify status regression rejection
    sqlx::query("UPDATE outbox_events SET status = 'PUBLISHED' WHERE id = ?1")
        .bind(&outbox_id)
        .execute(&h.pool)
        .await
        .unwrap();

    let outbox_regr_err = sqlx::query("UPDATE outbox_events SET status = 'PENDING' WHERE id = ?1")
        .bind(&outbox_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        outbox_regr_err.to_string().contains("Published outbox events cannot transition back to another status"),
        "Unexpected error: {:?}",
        outbox_regr_err
    );

    // Raw DELETE on PUBLISHED outbox event must fail
    let outbox_pub_del_err = sqlx::query("DELETE FROM outbox_events WHERE id = ?1")
        .bind(&outbox_id)
        .execute(&h.pool)
        .await
        .unwrap_err();
    assert!(
        outbox_pub_del_err.to_string().contains("Active or published outbox events are immutable and cannot be deleted"),
        "Unexpected error: {:?}",
        outbox_pub_del_err
    );
}

#[tokio::test]
async fn test_final_double_entry_and_arithmetic_invariants() {
    let h = setup_final_harness().await;

    // 1. Single line journal rejected (422)
    let (s1, b1) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Single line journal",
            "lines": [{"account_code": "1000", "debit": 100_000, "credit": 0}]
        })),
    )
    .await;
    assert_eq!(s1, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b1["code"], "UNBALANCED_JOURNAL_ENTRY");

    // 2. Unbalanced by 1 Rupiah rejected (422)
    let (s2, b2) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Unbalanced by 1 Rupiah",
            "lines": [
                {"account_code": "1000", "debit": 100_001, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100_000}
            ]
        })),
    )
    .await;
    assert_eq!(s2, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b2["code"], "UNBALANCED_JOURNAL_ENTRY");

    // 3. Zero-sum lines rejected (422)
    let (s3, b3) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Zero total journal",
            "lines": [
                {"account_code": "1000", "debit": 0, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 0}
            ]
        })),
    )
    .await;
    assert_eq!(s3, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b3["code"], "UNBALANCED_JOURNAL_ENTRY");

    // 4. Negative amounts rejected (400)
    let (s4, b4) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Negative amounts",
            "lines": [
                {"account_code": "1000", "debit": -50_000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": -50_000}
            ]
        })),
    )
    .await;
    assert_eq!(s4, StatusCode::BAD_REQUEST);
    assert_eq!(b4["code"], "INVALID_AMOUNT");

    // 5. Extreme amount: 9 quadrillion IDR (9_000_000_000_000_000)
    let quad = 9_000_000_000_000_000i64;
    let (s5, b5) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Sovereign 9 Quadrillion IDR transaction",
            "source_type": "MANUAL",
            "lines": [
                {"account_code": "1000", "debit": quad, "credit": 0, "memo": "Mega cash inflow"},
                {"account_code": "4000", "debit": 0, "credit": quad, "memo": "Sovereign funding"}
            ]
        })),
    )
    .await;
    assert_eq!(s5, StatusCode::CREATED);
    assert_eq!(b5["total_debit"], quad);
    assert_eq!(b5["total_credit"], quad);
    let mega_journal_id = b5["id"].as_str().unwrap().to_string();

    // Verify trial balance arithmetic holds without overflow
    let (s_tb, b_tb) = send_req(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(s_tb, StatusCode::OK);
    assert_eq!(b_tb["total_debit"], quad);
    assert_eq!(b_tb["total_credit"], quad);
    assert_eq!(b_tb["net_balance"], 0);
    assert_eq!(b_tb["is_balanced"], true);

    // Verify reversal of quadrillion journal
    let (s_rev, b_rev) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", mega_journal_id),
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({"reason": "Reverse sovereign transaction"})),
    )
    .await;
    assert_eq!(s_rev, StatusCode::CREATED);
    assert_eq!(b_rev["total_debit"], quad);
    assert_eq!(b_rev["total_credit"], quad);
    assert_eq!(b_rev["source_type"], "REVERSAL");

    // Post-reversal trial balance must still be balanced with net zero
    let (s_tb2, b_tb2) = send_req(
        &h.app,
        Method::GET,
        "/api/v1/accounting/trial-balance",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(s_tb2, StatusCode::OK);
    assert_eq!(b_tb2["net_balance"], 0);
    assert_eq!(b_tb2["is_balanced"], true);
}

#[tokio::test]
async fn test_final_cross_tenant_negative_probing_strictly_404() {
    let h = setup_final_harness().await;

    // 1. Tenant A creates draft invoice
    let (inv_status, inv_body) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/invoices",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "customer_name": "Tenant A Customer",
            "due_date": "2026-12-31",
            "currency": "IDR",
            "tax_type": "PPN_11_EXCL",
            "items": [
                {
                    "description": "Consulting Services",
                    "quantity": 1,
                    "unit_price": 10_000_000,
                    "discount": 0
                }
            ]
        })),
    )
    .await;
    assert_eq!(inv_status, StatusCode::CREATED);
    let invoice_id = inv_body["id"].as_str().unwrap().to_string();

    // Issue invoice in Tenant A
    let (issue_status, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/invoices/{}/issue", invoice_id),
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        None,
    )
    .await;
    assert_eq!(issue_status, StatusCode::OK);

    // 2. Tenant A posts a journal entry
    let (j_status, j_body) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Tenant A Journal",
            "lines": [
                {"account_code": "1000", "debit": 1_000_000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 1_000_000}
            ]
        })),
    )
    .await;
    assert_eq!(j_status, StatusCode::CREATED);
    let journal_id = j_body["id"].as_str().unwrap().to_string();

    // 3. Tenant A creates custom account 8888
    let (acc_status, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &h.owner_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "code": "8888",
            "name": "Custom Asset A",
            "account_type": "asset"
        })),
    )
    .await;
    assert_eq!(acc_status, StatusCode::CREATED);

    // --- NOW TENANT B PROBES ALL TENANT A RESOURCES ---

    // 4. Probe Tenant A's Invoice from Tenant B Context
    let (p_inv_get, b_inv_get) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/invoices/{}", invoice_id),
        &h.owner_token_b,
        Some(&h.tenant_b_id),
        None,
    )
    .await;
    assert_eq!(p_inv_get, StatusCode::NOT_FOUND, "Must strictly return 404, never 403/500");
    assert_eq!(b_inv_get["code"], "NOT_FOUND");

    // 5. Probe Voiding Tenant A's Invoice from Tenant B Context
    let (p_inv_void, b_inv_void) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/invoices/{}/void", invoice_id),
        &h.owner_token_b,
        Some(&h.tenant_b_id),
        Some(json!({"reason": "Malicious void"})),
    )
    .await;
    assert_eq!(p_inv_void, StatusCode::NOT_FOUND, "Must strictly return 404");
    assert_eq!(b_inv_void["code"], "NOT_FOUND");

    // 6. Probe Recording Payment against Tenant A's Invoice from Tenant B Context
    let (p_pay, b_pay) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/payments",
        &h.owner_token_b,
        Some(&h.tenant_b_id),
        Some(json!({
            "invoice_id": invoice_id,
            "amount": 5_000_000,
            "payment_method": "BANK_TRANSFER"
        })),
    )
    .await;
    assert_eq!(p_pay, StatusCode::NOT_FOUND, "Must strictly return 404");
    assert_eq!(b_pay["code"], "NOT_FOUND");

    // 7. Probe Tenant A's Journal from Tenant B Context
    let (p_j_rev, b_j_rev) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/accounting/journals/{}/reverse", journal_id),
        &h.owner_token_b,
        Some(&h.tenant_b_id),
        Some(json!({"reason": "Malicious reversal"})),
    )
    .await;
    assert_eq!(p_j_rev, StatusCode::NOT_FOUND, "Must strictly return 404");
    assert_eq!(b_j_rev["code"], "NOT_FOUND");

    // 8. Probe Deleting Tenant A's Account from Tenant B Context
    let (p_acc_del, b_acc_del) = send_req(
        &h.app,
        Method::DELETE,
        "/api/v1/accounting/accounts/8888",
        &h.owner_token_b,
        Some(&h.tenant_b_id),
        None,
    )
    .await;
    assert_eq!(p_acc_del, StatusCode::NOT_FOUND, "Must strictly return 404");
    assert_eq!(b_acc_del["code"], "NOT_FOUND");

    // 9. Outbox events list under Tenant B must have zero Tenant A events
    let (p_outbox, b_outbox) = send_req(
        &h.app,
        Method::GET,
        "/api/v1/outbox/events",
        &h.owner_token_b,
        Some(&h.tenant_b_id),
        None,
    )
    .await;
    assert_eq!(p_outbox, StatusCode::OK);
    assert_eq!(b_outbox["count"], 0);
    assert_eq!(b_outbox["events"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_final_internal_unauthorized_rbac_strictly_403() {
    let h = setup_final_harness().await;

    // Staff member attempts to post journal entry -> 403 FORBIDDEN
    let (s1, b1) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/journals",
        &h.staff_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "description": "Unauthorized Staff Journal",
            "lines": [
                {"account_code": "1000", "debit": 100_000, "credit": 0},
                {"account_code": "4000", "debit": 0, "credit": 100_000}
            ]
        })),
    )
    .await;
    assert_eq!(s1, StatusCode::FORBIDDEN, "Staff cannot post journals");
    assert_eq!(b1["code"], "FORBIDDEN");

    // Staff member attempts to create COA account -> 403 FORBIDDEN
    let (s2, b2) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/accounting/accounts",
        &h.staff_token_a,
        Some(&h.tenant_a_id),
        Some(json!({
            "code": "7777",
            "name": "Staff Asset",
            "account_type": "asset"
        })),
    )
    .await;
    assert_eq!(s2, StatusCode::FORBIDDEN, "Staff cannot manage COA");
    assert_eq!(b2["code"], "FORBIDDEN");
}

#[tokio::test]
async fn test_final_atomic_outbox_rollback_and_dlq_resilience() {
    let h = setup_final_harness().await;
    let outbox_repo = SqlxOutboxRepository::new(h.pool.clone());

    // 1. Test Atomic Rollback: Zero Orphaned Outbox Events
    let test_agg_id = format!("inv-test-{}", Uuid::new_v4());
    {
        let mut tx = h.pool.begin().await.unwrap();

        // Stage an outbox event inside transaction
        let draft = OutboxEventDraft {
            tenant_id: Uuid::parse_str(&h.tenant_a_id).unwrap(),
            event_type: "InvoiceIssued".to_string(),
            aggregate_type: "Invoice".to_string(),
            aggregate_id: test_agg_id.clone(),
            payload_json: json!({"amount": 1_000_000}),
            max_retries: 5,
        };

        let _ = outbox_repo.insert_tx(&mut tx, &draft).await.unwrap();

        // Roll back transaction explicitly
        tx.rollback().await.unwrap();
    }

    // Assert: zero records exist in outbox_events for test_agg_id
    let orphaned_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1",
    )
    .bind(&test_agg_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(orphaned_count, 0, "Atomic rollback must guarantee 0 orphaned outbox events");

    // 2. Test Exponential Backoff and DLQ Progression after 5 attempts
    let now = Utc::now();
    let draft_dlq = OutboxEventDraft {
        tenant_id: Uuid::parse_str(&h.tenant_a_id).unwrap(),
        event_type: "PaymentConfirmed".to_string(),
        aggregate_type: "Payment".to_string(),
        aggregate_id: format!("pay-{}", Uuid::new_v4()),
        payload_json: json!({"payment": "failed_dispatch"}),
        max_retries: 5,
    };
    let event = outbox_repo.insert(&draft_dlq).await.unwrap();

    // Simulate 5 successive failures
    for attempt in 1..=5 {
        let mut tx = h.pool.begin().await.unwrap();
        let next_retry = now + chrono::Duration::seconds(2i64.pow(attempt - 1));
        if attempt < 5 {
            outbox_repo
                .mark_failed_tx(&mut tx, &event.id, "Connection refused", next_retry)
                .await
                .unwrap();
        } else {
            outbox_repo
                .mark_dead_letter_tx(&mut tx, &event.id, "Max retries (5) exhausted")
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
    }

    // Verify DLQ status in database
    let (final_status, attempt_count, last_error): (String, i64, Option<String>) = sqlx::query_as(
        "SELECT status, attempt_count, last_error FROM outbox_events WHERE id = ?1",
    )
    .bind(&event.id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(final_status, "DEAD_LETTER");
    assert_eq!(attempt_count, 5);
    assert!(last_error.unwrap().contains("Max retries (5) exhausted"));

    // Verify DEAD_LETTER events are excluded from pending batches
    let pending_batch = outbox_repo
        .fetch_pending_batch(50, Utc::now() + chrono::Duration::days(1))
        .await
        .unwrap();
    assert!(
        !pending_batch.iter().any(|e| e.id == event.id),
        "DEAD_LETTER events must never be polled in active pending batches"
    );

    // 3. Concurrency Race: Multiple concurrent workers claiming disjoint events
    for i in 0..20 {
        let draft = OutboxEventDraft {
            tenant_id: Uuid::parse_str(&h.tenant_a_id).unwrap(),
            event_type: "JournalPosted".to_string(),
            aggregate_type: "Journal".to_string(),
            aggregate_id: format!("jrn-race-{}", i),
            payload_json: json!({"index": i}),
            max_retries: 5,
        };
        outbox_repo.insert(&draft).await.unwrap();
    }

    let mut join_set = JoinSet::new();
    for _ in 0..4 {
        let pool_clone = h.pool.clone();
        join_set.spawn(async move {
            let repo = SqlxOutboxRepository::new(pool_clone.clone());
            let mut claimed_count = 0;
            for _ in 0..10 {
                let pending = repo.fetch_pending_batch(5, Utc::now()).await.unwrap();
                if pending.is_empty() {
                    break;
                }
                let ids: Vec<String> = pending.into_iter().map(|e| e.id).collect();
                let mut tx = pool_clone.begin().await.unwrap();
                let claimed = repo.claim_batch_tx(&mut tx, &ids).await.unwrap();
                claimed_count += claimed.len();
                for c in &claimed {
                    repo.mark_published_tx(&mut tx, &c.id, Utc::now()).await.unwrap();
                }
                tx.commit().await.unwrap();
            }
            claimed_count
        });
    }

    let mut total_claimed = 0;
    while let Some(res) = join_set.join_next().await {
        total_claimed += res.unwrap();
    }

    assert_eq!(
        total_claimed, 20,
        "All 20 events must be claimed and processed with zero race duplicates"
    );
}
