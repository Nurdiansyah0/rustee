//! Milestone 3 Adversarial Empirical Challenger Verification Suite
//!
//! Identity: teamwork_preview_challenger_m3_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//!
//! Empirical Verification Objectives:
//! 1. Draft Invariants & HTTP 409 Semantics:
//!    - Verify draft invoices strictly have `invoice_number = NULL` in DB and JSON response.
//!    - Verify attempting to update an issued invoice returns HTTP 409 Conflict with code "INVOICE_LOCKED".
//!    - Verify attempting to update a paid or voided invoice returns HTTP 409 Conflict with code "INVOICE_LOCKED".
//! 2. SQLite Immutability Triggers Stress-Testing:
//!    - Direct SQL UPDATE on financial columns (subtotal, tax_amount, total_amount, discount, tax_type) of issued invoices fails with trigger abort.
//!    - Direct SQL UPDATE on identity/audit fields (invoice_number, issue_date, customer_name, customer_address, tenant_id) fails with trigger abort.
//!    - Direct SQL DELETE on issued, partially paid, or paid invoices fails with trigger abort.
//!    - Direct SQL UPDATE or DELETE on invoice_items belonging to issued invoices fails with trigger abort.
//!    - Direct SQL UPDATE or DELETE on invoice_snapshots fails with trigger abort.
//!    - Direct SQL UPDATE or DELETE on confirmed payments fails with trigger abort.
//!    - Direct SQL UPDATE or DELETE on payment_allocations fails with trigger abort.
//!    - Direct SQL illegal status transitions (PAID -> VOIDED, PARTIALLY_PAID -> VOIDED, VOIDED -> ISSUED) fail with trigger abort.
//! 3. Concurrent Invoice Issuing Races & Gapless Monotonic Numbering:
//!    - High concurrency (10 concurrent racing issue requests on distinct drafts) produces gapless, monotonic, duplicate-free numbering INV-YYYY-XXXXXX.
//!    - Simultaneous racing issue requests on the EXACT SAME draft yields exactly 1 winner (HTTP 200) and preserves single-snapshot/single-receivable invariants.
//!    - Concurrent payment allocations strictly prevent overpayment (cumulative allocated <= total_amount).
//! 4. Cross-Tenant Boundaries:
//!    - Cross-tenant issue, update, void, and payment requests strictly return HTTP 404 NOT_FOUND.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
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
use chrono::Utc;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::task::JoinSet;
use tower::ServiceExt;
use uuid::Uuid;

#[allow(dead_code)]
struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m3_challenger_test.sqlite");
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

    let jwt_secret = "m3_challenger_jwt_secret_key_1234567890_adversarial";
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

    // 1. Owner User A
    let owner_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_id.clone(),
            email: "challenger_owner_a@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Challenger Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "challenger_owner_a@business.com", "user", "premium")
        .unwrap();

    // 2. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "challenger_staff_a@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Challenger Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "challenger_staff_a@business.com", "user", "free")
        .unwrap();

    // 3. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Challenger Alpha Enterprise".to_string(),
            slug: "challenger-alpha-ent".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

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

    // 4. Tenant B & User B for isolation testing
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "challenger_user_b@beta.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Challenger User B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "challenger_user_b@beta.com", "user", "free")
        .unwrap();

    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Challenger Beta Enterprise".to_string(),
            slug: "challenger-beta-ent".to_string(),
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
            user_id: user_b_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        owner_token,
        staff_token,
        tenant_a_id,
        tenant_b_id,
        tenant_b_token,
    }
}

async fn parse_response(res: axum::response::Response) -> (StatusCode, Value, HeaderMap) {
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json, headers)
}

async fn create_draft(
    harness: &ChallengerHarness,
    token: &str,
    tenant_id: &str,
    customer_name: &str,
    amount: i64,
) -> (String, Value) {
    let payload = json!({
        "customer_name": customer_name,
        "due_date": "2026-12-31T00:00:00Z",
        "currency": "IDR",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Standard Professional Services",
                "quantity": 1,
                "unit_price": amount,
                "discount": 0
            }
        ]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let res = harness.app.clone().oneshot(req).await.unwrap();
    let (status, body, _) = parse_response(res).await;
    assert_eq!(status, StatusCode::CREATED, "Draft creation should return 201 Created");
    let id = body["id"].as_str().unwrap().to_string();
    (id, body)
}

async fn issue_invoice_api(
    harness: &ChallengerHarness,
    token: &str,
    tenant_id: &str,
    invoice_id: &str,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id)
        .body(Body::empty())
        .unwrap();

    let res = harness.app.clone().oneshot(req).await.unwrap();
    let (status, body, _) = parse_response(res).await;
    (status, body)
}

// ===========================================================================
// OBJECTIVE 1: Draft Invariants & HTTP 409 Semantics
// ===========================================================================

#[tokio::test]
async fn challenge_draft_invoice_number_strictly_null_in_db_and_api() {
    let harness = setup_challenger_harness().await;

    // Create 3 drafts
    for i in 1..=3 {
        let (id, body) = create_draft(
            &harness,
            &harness.owner_token,
            &harness.tenant_a_id,
            &format!("Customer Draft {}", i),
            1_000_000 * i,
        )
        .await;

        // 1. Verify API response
        assert_eq!(body["status"], "DRAFT");
        assert!(body["invoice_number"].is_null(), "Draft invoice_number in API response must be null");
        assert!(body["issue_date"].is_null(), "Draft issue_date in API response must be null");

        // 2. Verify direct SQLite persistence
        let row: (Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT invoice_number, issue_date, status FROM invoices WHERE id = ?1 AND tenant_id = ?2",
        )
        .bind(&id)
        .bind(&harness.tenant_a_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

        assert!(row.0.is_none(), "Database column invoice_number must be strictly NULL for draft invoices");
        assert!(row.1.is_none(), "Database column issue_date must be strictly NULL for draft invoices");
        assert_eq!(row.2, "DRAFT", "Database status must be DRAFT");
    }
}

#[tokio::test]
async fn challenge_update_issued_invoice_returns_http_409_invoice_locked() {
    let harness = setup_challenger_harness().await;

    let (inv_id, _) = create_draft(
        &harness,
        &harness.owner_token,
        &harness.tenant_a_id,
        "Customer Locked Test",
        2_000_000,
    )
    .await;

    // Update while draft -> succeeds 200 OK
    let draft_update_payload = json!({
        "customer_name": "Customer Name Updated While Draft",
        "customer_address": "Jl. Sudirman 100"
    });
    let req_draft = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/v1/invoices/{}", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(draft_update_payload.to_string()))
        .unwrap();

    let (s_draft, b_draft, _) = parse_response(harness.app.clone().oneshot(req_draft).await.unwrap()).await;
    assert_eq!(s_draft, StatusCode::OK);
    assert_eq!(b_draft["customer_name"], "Customer Name Updated While Draft");

    // Issue invoice
    let (s_issue, b_issue) = issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;
    assert_eq!(s_issue, StatusCode::OK);
    assert_eq!(b_issue["status"], "ISSUED");
    assert!(b_issue["invoice_number"].is_string());

    // Attempt update on issued invoice -> HTTP 409 CONFLICT with code INVOICE_LOCKED
    let issued_update_payload = json!({
        "customer_name": "Tampered Customer After Issue"
    });
    let req_issued = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/v1/invoices/{}", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(issued_update_payload.to_string()))
        .unwrap();

    let (s_lock, b_lock, _) = parse_response(harness.app.clone().oneshot(req_issued).await.unwrap()).await;
    assert_eq!(s_lock, StatusCode::CONFLICT, "Updating an issued invoice must return 409 Conflict");
    assert_eq!(b_lock["code"], "INVOICE_LOCKED", "Error code must be INVOICE_LOCKED");
}

#[tokio::test]
async fn challenge_update_paid_and_voided_invoices_return_http_409_invoice_locked() {
    let harness = setup_challenger_harness().await;

    // 1. Test update on PAID invoice
    let (inv_paid_id, _) = create_draft(
        &harness,
        &harness.owner_token,
        &harness.tenant_a_id,
        "Customer Paid Settle",
        1_000_000,
    )
    .await;
    let (_, b_iss) = issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_paid_id).await;
    let total_amount = b_iss["total_amount"].as_i64().unwrap();

    // Pay full amount -> status becomes PAID
    let pay_payload = json!({
        "invoice_id": inv_paid_id,
        "amount": total_amount,
        "payment_method": "BANK_TRANSFER"
    });
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_payload.to_string()))
        .unwrap();
    let (s_pay, _, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(s_pay, StatusCode::CREATED);

    // Attempt PUT on PAID invoice -> 409 INVOICE_LOCKED
    let req_paid_update = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/v1/invoices/{}", inv_paid_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "customer_name": "Modify Paid Invoice" }).to_string()))
        .unwrap();
    let (s_paid_up, b_paid_up, _) = parse_response(harness.app.clone().oneshot(req_paid_update).await.unwrap()).await;
    assert_eq!(s_paid_up, StatusCode::CONFLICT);
    assert_eq!(b_paid_up["code"], "INVOICE_LOCKED");

    // 2. Test update on VOIDED invoice
    let (inv_void_id, _) = create_draft(
        &harness,
        &harness.owner_token,
        &harness.tenant_a_id,
        "Customer Voided Test",
        500_000,
    )
    .await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_void_id).await;

    // Void the issued invoice
    let void_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_void_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Customer cancelled order" }).to_string()))
        .unwrap();
    let (s_void, _, _) = parse_response(harness.app.clone().oneshot(void_req).await.unwrap()).await;
    assert_eq!(s_void, StatusCode::OK);

    // Attempt PUT on VOIDED invoice -> 409 INVOICE_LOCKED
    let req_void_update = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/v1/invoices/{}", inv_void_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "customer_name": "Modify Voided Invoice" }).to_string()))
        .unwrap();
    let (s_void_up, b_void_up, _) = parse_response(harness.app.clone().oneshot(req_void_update).await.unwrap()).await;
    assert_eq!(s_void_up, StatusCode::CONFLICT);
    assert_eq!(b_void_up["code"], "INVOICE_LOCKED");
}

// ===========================================================================
// OBJECTIVE 2: Direct SQLite Immutability Triggers Stress-Testing
// ===========================================================================

#[tokio::test]
async fn challenge_trigger_issued_invoice_financial_mutation_abort() {
    let harness = setup_challenger_harness().await;

    let (inv_id, _) = create_draft(
        &harness,
        &harness.owner_token,
        &harness.tenant_a_id,
        "Trigger Financial Test",
        3_000_000,
    )
    .await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // Vector 1: Tamper subtotal
    let res = sqlx::query("UPDATE invoices SET subtotal = subtotal + 500000 WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct update of subtotal");

    // Vector 2: Tamper tax_amount
    let res = sqlx::query("UPDATE invoices SET tax_amount = 0 WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct update of tax_amount");

    // Vector 3: Tamper total_amount
    let res = sqlx::query("UPDATE invoices SET total_amount = total_amount - 100000 WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct update of total_amount");

    // Vector 4: Tamper discount
    let res = sqlx::query("UPDATE invoices SET discount = 200000 WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct update of discount");

    // Vector 5: Tamper tax_type
    let res = sqlx::query("UPDATE invoices SET tax_type = 'PPN_12_EXCL' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct update of tax_type");

    // Vector 6: Tamper invoice_number
    let res = sqlx::query("UPDATE invoices SET invoice_number = 'INV-HACKED-000001' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct alteration of invoice_number");

    // Vector 7: Tamper issue_date
    let res = sqlx::query("UPDATE invoices SET issue_date = '2099-01-01T00:00:00Z' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct alteration of issue_date");

    // Vector 8: Tamper customer_name
    let res = sqlx::query("UPDATE invoices SET customer_name = 'Fraudulent Entity' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort direct alteration of customer_name");

    // Vector 9: Tamper tenant_id (cross-tenant hijacking)
    let res = sqlx::query("UPDATE invoices SET tenant_id = ?1 WHERE id = ?2")
        .bind(&harness.tenant_b_id)
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(res.is_err(), "Trigger must abort tenant_id reassignment on issued invoices");

    // Positive control: Non-financial update (notes) is permitted for operational notes
    let ok_res = sqlx::query("UPDATE invoices SET notes = 'Verified with customer' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(ok_res.is_ok(), "Non-financial annotations should be allowed on issued invoices");
}

#[tokio::test]
async fn challenge_trigger_prevent_issued_invoice_delete_abort() {
    let harness = setup_challenger_harness().await;

    // 1. Issued invoice deletion must abort
    let (inv_iss, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Del Iss", 1_000_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_iss).await;

    let del_iss_res = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_iss)
        .execute(&harness.pool)
        .await;
    assert!(del_iss_res.is_err(), "Trigger must abort direct DELETE on ISSUED invoice");

    // 2. Draft invoice deletion must succeed (positive control)
    let (inv_draft, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Del Draft", 500_000).await;
    let del_draft_res = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_draft)
        .execute(&harness.pool)
        .await;
    assert!(del_draft_res.is_ok(), "Direct DELETE on DRAFT invoice must be permitted");
}

#[tokio::test]
async fn challenge_trigger_invoice_items_mutation_and_deletion_abort() {
    let harness = setup_challenger_harness().await;

    let (inv_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Items Lock", 750_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // Attempt direct line item UPDATE on issued invoice
    let update_item_res = sqlx::query("UPDATE invoice_items SET unit_price = 100 WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(update_item_res.is_err(), "Trigger must abort direct UPDATE on invoice_items of issued invoice");

    // Attempt direct line item DELETE on issued invoice
    let delete_item_res = sqlx::query("DELETE FROM invoice_items WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(delete_item_res.is_err(), "Trigger must abort direct DELETE on invoice_items of issued invoice");
}

#[tokio::test]
async fn challenge_trigger_invoice_snapshots_immutability_abort() {
    let harness = setup_challenger_harness().await;

    let (inv_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Snap Lock", 1_200_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // Verify snapshot exists
    let snap_count: i64 = sqlx::query_scalar("SELECT count(*) FROM invoice_snapshots WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(snap_count, 1);

    // Attempt direct snapshot UPDATE -> must fail
    let snap_up_res = sqlx::query("UPDATE invoice_snapshots SET snapshot_json = '{\"hacked\": true}' WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(snap_up_res.is_err(), "Trigger must abort direct UPDATE on invoice_snapshots");

    // Attempt direct snapshot DELETE -> must fail
    let snap_del_res = sqlx::query("DELETE FROM invoice_snapshots WHERE invoice_id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(snap_del_res.is_err(), "Trigger must abort direct DELETE on invoice_snapshots");
}

#[tokio::test]
async fn challenge_trigger_payments_and_allocations_immutability_abort() {
    let harness = setup_challenger_harness().await;

    let (inv_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Pay Lock", 1_000_000).await;
    let (_, b_iss) = issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;
    assert_eq!(b_iss["total_amount"].as_i64().unwrap(), 1_110_000);

    // Make payment
    let pay_payload = json!({
        "invoice_id": inv_id,
        "amount": 500_000,
        "payment_method": "BANK_TRANSFER"
    });
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_payload.to_string()))
        .unwrap();
    let (s_pay, b_pay, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(s_pay, StatusCode::CREATED);
    let pay_id = b_pay["id"].as_str().unwrap();

    // 1. Direct SQL UPDATE on confirmed payment amount -> must fail
    let pay_amount_up = sqlx::query("UPDATE payments SET amount = 9999999 WHERE id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(pay_amount_up.is_err(), "Trigger must abort direct UPDATE of confirmed payment amount");

    // 2. Direct SQL UPDATE on confirmed payment date -> must fail
    let pay_date_up = sqlx::query("UPDATE payments SET payment_date = '2099-01-01' WHERE id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(pay_date_up.is_err(), "Trigger must abort direct UPDATE of confirmed payment date");

    // 3. Direct SQL DELETE on confirmed payment -> must fail
    let pay_del = sqlx::query("DELETE FROM payments WHERE id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(pay_del.is_err(), "Trigger must abort direct DELETE of confirmed payment");

    // 4. Direct SQL UPDATE on payment allocations -> must fail
    let alloc_up = sqlx::query("UPDATE payment_allocations SET amount = 1 WHERE payment_id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(alloc_up.is_err(), "Trigger must abort direct UPDATE of payment allocations");

    // 5. Direct SQL DELETE on payment allocations -> must fail
    let alloc_del = sqlx::query("DELETE FROM payment_allocations WHERE payment_id = ?1")
        .bind(pay_id)
        .execute(&harness.pool)
        .await;
    assert!(alloc_del.is_err(), "Trigger must abort direct DELETE of payment allocations");
}

#[tokio::test]
async fn challenge_trigger_illegal_status_transitions_abort() {
    let harness = setup_challenger_harness().await;

    let (inv_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Illegal Trans", 800_000).await;
    let (_, b_iss) = issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;
    let total_amount = b_iss["total_amount"].as_i64().unwrap();

    // Settle to PAID
    let pay_payload = json!({
        "invoice_id": inv_id,
        "amount": total_amount,
        "payment_method": "CASH"
    });
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_payload.to_string()))
        .unwrap();
    let (s_pay, _, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(s_pay, StatusCode::CREATED);

    // 1. Direct SQL transition PAID -> VOIDED must be aborted by trigger 8.1
    let void_paid_sql = sqlx::query("UPDATE invoices SET status = 'VOIDED' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(void_paid_sql.is_err(), "Trigger must abort direct transition from PAID to VOIDED");

    // 2. Direct SQL transition PAID -> DRAFT must be aborted by trigger 8.1
    let draft_paid_sql = sqlx::query("UPDATE invoices SET status = 'DRAFT' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;
    assert!(draft_paid_sql.is_err(), "Trigger must abort direct transition from PAID to DRAFT");

    // 3. Direct SQL transition from VOIDED to any other status must be aborted by trigger 8.1
    let (inv_void_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Void Trans", 300_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_void_id).await;
    let _ = sqlx::query("UPDATE invoices SET status = 'VOIDED' WHERE id = ?1").bind(&inv_void_id).execute(&harness.pool).await;

    let void_to_issued = sqlx::query("UPDATE invoices SET status = 'ISSUED' WHERE id = ?1")
        .bind(&inv_void_id)
        .execute(&harness.pool)
        .await;
    assert!(void_to_issued.is_err(), "Trigger must abort transition from VOIDED to ISSUED");

    // 4. Direct SQL transition from ISSUED to DRAFT must be aborted by trigger 8.1
    let (inv_iss_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Iss Trans", 200_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_iss_id).await;

    let iss_to_draft = sqlx::query("UPDATE invoices SET status = 'DRAFT' WHERE id = ?1")
        .bind(&inv_iss_id)
        .execute(&harness.pool)
        .await;
    assert!(iss_to_draft.is_err(), "Trigger must abort transition from ISSUED to DRAFT");
}

#[tokio::test]
async fn challenge_status_downgrade_vulnerability_observation() {
    let harness = setup_challenger_harness().await;

    let (inv_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Downgrade Target", 400_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // Observe whether trigger 8.1 prevents direct status downgrade to DRAFT
    let downgrade_res = sqlx::query("UPDATE invoices SET status = 'DRAFT' WHERE id = ?1")
        .bind(&inv_id)
        .execute(&harness.pool)
        .await;

    // Trigger 8.1 strictly aborts status downgrade from ISSUED to DRAFT
    assert!(downgrade_res.is_err(), "Trigger 8.1 must strictly abort status downgrade from ISSUED to DRAFT");
}

// ===========================================================================
// OBJECTIVE 3: Concurrent Invoice Issuing Races & Gapless Monotonic Numbering
// ===========================================================================

#[tokio::test]
async fn challenge_concurrent_invoice_issuing_gapless_monotonic_sequence() {
    let harness = setup_challenger_harness().await;
    let current_year = Utc::now().format("%Y").to_string();

    let concurrency_count = 10;
    let mut draft_ids = Vec::with_capacity(concurrency_count);

    // Create 10 distinct draft invoices
    for i in 1..=concurrency_count {
        let (id, _) = create_draft(
            &harness,
            &harness.owner_token,
            &harness.tenant_a_id,
            &format!("Concurrent Client {}", i),
            100_000 * i as i64,
        )
        .await;
        draft_ids.push(id);
    }

    // Concurrently issue all 10 invoices using JoinSet with backoff retry
    let mut join_set = JoinSet::new();

    for id in draft_ids {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_a_id.clone();

        join_set.spawn(async move {
            let mut attempts = 0;
            loop {
                attempts += 1;
                let req = Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/invoices/{}/issue", id))
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .header("X-Tenant-ID", &tenant_id)
                    .body(Body::empty())
                    .unwrap();

                let (status, body, _) = parse_response(app.clone().oneshot(req).await.unwrap()).await;

                if status == StatusCode::OK {
                    let num = body["invoice_number"].as_str().unwrap().to_string();
                    return (id, num);
                }

                if status == StatusCode::CONFLICT && body["code"] == "ALREADY_ISSUED" {
                    // Invoice was already committed as ISSUED in a previous racing attempt
                    let get_req = Request::builder()
                        .method(Method::GET)
                        .uri(format!("/api/v1/invoices/{}", id))
                        .header(AUTHORIZATION, format!("Bearer {}", token))
                        .header("X-Tenant-ID", &tenant_id)
                        .body(Body::empty())
                        .unwrap();
                    let (get_s, get_b, _) = parse_response(app.clone().oneshot(get_req).await.unwrap()).await;
                    if get_s == StatusCode::OK {
                        if let Some(num) = get_b["invoice_number"].as_str() {
                            return (id, num.to_string());
                        }
                    }
                }

                if attempts >= 25 {
                    panic!("Task failed to issue invoice {} after {} attempts: status {:?}, body: {:?}", id, attempts, status, body);
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(50 * attempts)).await;
            }
        });
    }

    let mut issued_numbers = Vec::new();
    while let Some(res) = join_set.join_next().await {
        let (_id, num) = res.expect("Join task panicked");
        issued_numbers.push(num);
    }

    assert_eq!(issued_numbers.len(), concurrency_count);

    // 1. Verify all numbers have format INV-YYYY-XXXXXX
    let expected_prefix = format!("INV-{}-", current_year);
    for num in &issued_numbers {
        assert!(
            num.starts_with(&expected_prefix),
            "Invoice number {} must start with {}",
            num,
            expected_prefix
        );
    }

    // 2. Verify all numbers are unique (zero duplicates)
    let unique_set: HashSet<String> = issued_numbers.iter().cloned().collect();
    assert_eq!(
        unique_set.len(),
        concurrency_count,
        "Every issued invoice must have a strictly unique invoice number"
    );

    // 3. Extract numeric suffixes and verify strictly gapless monotonic sequence 1..=10
    let mut sequence_numbers: Vec<u64> = issued_numbers
        .iter()
        .map(|num| {
            let suffix = num.strip_prefix(&expected_prefix).unwrap();
            suffix.parse::<u64>().expect("Suffix must be numeric")
        })
        .collect();

    sequence_numbers.sort_unstable();

    let expected_sequence: Vec<u64> = (1..=concurrency_count as u64).collect();
    assert_eq!(
        sequence_numbers, expected_sequence,
        "Invoice sequence must be strictly monotonic and gapless without missing or skipped numbers"
    );
}

#[tokio::test]
async fn challenge_simultaneous_double_issue_race_same_invoice() {
    let harness = setup_challenger_harness().await;

    // Create a single draft invoice
    let (draft_id, _) = create_draft(
        &harness,
        &harness.owner_token,
        &harness.tenant_a_id,
        "Double Issue Target",
        500_000,
    )
    .await;

    // Spawn 10 concurrent tasks all attempting to issue this SAME invoice simultaneously
    let mut join_set = JoinSet::new();

    for _ in 0..10 {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_a_id.clone();
        let id = draft_id.clone();

        join_set.spawn(async move {
            let req = Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/invoices/{}/issue", id))
                .header(AUTHORIZATION, format!("Bearer {}", token))
                .header("X-Tenant-ID", &tenant_id)
                .body(Body::empty())
                .unwrap();

            let (status, body, _) = parse_response(app.oneshot(req).await.unwrap()).await;
            (status, body)
        });
    }

    let mut ok_count = 0;
    let mut conflict_count = 0;

    while let Some(res) = join_set.join_next().await {
        let (status, body) = res.expect("Join task panicked");
        if status == StatusCode::OK {
            ok_count += 1;
            assert_eq!(body["status"], "ISSUED");
        } else if status == StatusCode::CONFLICT {
            conflict_count += 1;
            let code = body["code"].as_str().unwrap_or("");
            assert!(
                code == "ALREADY_ISSUED" || code == "INVOICE_LOCKED" || code == "CONFLICT" || code == "UNIQUE_VIOLATION" || code == "LOCK_CONTENTION",
                "Conflict code should denote conflict, got: {}",
                code
            );
        } else {
            panic!("Unexpected status code in double-issue race: {:?}, body: {:?}", status, body);
        }
    }

    // Invariant 1: Exactly 1 winner succeeds
    assert_eq!(ok_count, 1, "Exactly one concurrent issue request must succeed");
    // Invariant 2: All 9 competing tasks fail with HTTP 409 Conflict
    assert_eq!(conflict_count, 9, "All 9 competing issue requests must return HTTP 409 Conflict");

    println!(
        "[ADVERSARIAL_OBSERVATION] Double-issue race result: 1 OK, {} Conflict (409)",
        conflict_count
    );

    // Verify database state: exactly 1 snapshot, exactly 1 receivable, exactly 1 journal entry
    let snapshot_count: i64 = sqlx::query_scalar("SELECT count(*) FROM invoice_snapshots WHERE invoice_id = ?1")
        .bind(&draft_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(snapshot_count, 1, "Exactly 1 invoice snapshot must exist in database");

    let receivable_count: i64 = sqlx::query_scalar("SELECT count(*) FROM receivables WHERE invoice_id = ?1")
        .bind(&draft_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(receivable_count, 1, "Exactly 1 receivable record must exist in database");

    let journal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM journal_entries WHERE source_type = 'INVOICE' AND source_id = ?1")
        .bind(&draft_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(journal_count, 1, "Exactly 1 GL journal entry must be posted");
}

#[tokio::test]
async fn challenge_concurrent_payment_allocations_race_preventing_overpayment() {
    let harness = setup_challenger_harness().await;

    // Issue invoice: 1,000,000 + 11% PPN = 1,110,000 total
    let (inv_id, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Race Overpay Target", 1_000_000).await;
    let (_, b_iss) = issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;
    let total_amount = b_iss["total_amount"].as_i64().unwrap();
    assert_eq!(total_amount, 1_110_000);

    // 5 concurrent tasks each attempting to pay 400,000 IDR
    // Total attempted = 2,000,000 > 1,110,000
    // At most 2 can succeed (400k + 400k = 800k), 3rd would make 1.2M > 1.11M
    let mut join_set = JoinSet::new();

    for i in 1..=5 {
        let app = harness.app.clone();
        let token = harness.owner_token.clone();
        let tenant_id = harness.tenant_a_id.clone();
        let invoice_id = inv_id.clone();

        join_set.spawn(async move {
            let payload = json!({
                "invoice_id": invoice_id,
                "amount": 400_000,
                "payment_method": "BANK_TRANSFER",
                "reference": format!("RACE-REF-{}", i)
            });

            let req = Request::builder()
                .method(Method::POST)
                .uri("/api/v1/payments")
                .header(AUTHORIZATION, format!("Bearer {}", token))
                .header("X-Tenant-ID", &tenant_id)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();

            let (status, body, _) = parse_response(app.oneshot(req).await.unwrap()).await;
            (status, body)
        });
    }

    let mut successful_allocations = 0;
    let mut rejected_overpayments = 0;
    let mut conflict_errors = 0;

    while let Some(res) = join_set.join_next().await {
        let (status, body) = res.expect("Join task panicked");
        if status == StatusCode::CREATED {
            successful_allocations += 1;
            assert_eq!(body["status"], "CONFIRMED");
        } else if status == StatusCode::BAD_REQUEST {
            rejected_overpayments += 1;
            assert_eq!(body["code"], "OVERPAYMENT_NOT_ALLOWED");
        } else if status == StatusCode::CONFLICT {
            conflict_errors += 1;
        } else {
            panic!("Unexpected status in payment concurrency test: {:?}, body: {:?}", status, body);
        }
    }

    assert!(
        successful_allocations <= 2,
        "At most 2 payments of 400k could succeed without exceeding 1,110,000 balance"
    );
    assert_eq!(successful_allocations + rejected_overpayments + conflict_errors, 5);

    println!(
        "[ADVERSARIAL_OBSERVATION] Payment race: {} Created, {} Overpayment Rejected, {} Conflicts",
        successful_allocations, rejected_overpayments, conflict_errors
    );

    // Verify database state: outstanding_amount >= 0 and allocated_amount <= 1,110,000
    let rec: (i64, i64) = sqlx::query_as(
        "SELECT allocated_amount, outstanding_amount FROM receivables WHERE invoice_id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert!(rec.0 <= total_amount, "Allocated amount {} must not exceed total amount {}", rec.0, total_amount);
    assert!(rec.1 >= 0, "Outstanding amount {} must never be negative", rec.1);
    assert_eq!(rec.0 + rec.1, total_amount, "allocated + outstanding must strictly equal total amount");
}

#[tokio::test]
async fn challenge_cross_tenant_invoice_mutations_strictly_return_404() {
    let harness = setup_challenger_harness().await;

    // Tenant A creates and issues invoice
    let (inv_a, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Tenant A Invoice", 1_000_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_a).await;

    // 1. Tenant B attempts to read Tenant A invoice -> 404
    let req_get = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/invoices/{}", inv_a))
        .header(AUTHORIZATION, format!("Bearer {}", harness.tenant_b_token))
        .header("X-Tenant-ID", &harness.tenant_b_id)
        .body(Body::empty())
        .unwrap();
    let (s_get, b_get, _) = parse_response(harness.app.clone().oneshot(req_get).await.unwrap()).await;
    assert_eq!(s_get, StatusCode::NOT_FOUND);
    assert_eq!(b_get["code"], "NOT_FOUND");

    // 2. Tenant B attempts to void Tenant A invoice -> 404
    let req_void = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_a))
        .header(AUTHORIZATION, format!("Bearer {}", harness.tenant_b_token))
        .header("X-Tenant-ID", &harness.tenant_b_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Malicious void" }).to_string()))
        .unwrap();
    let (s_void, b_void, _) = parse_response(harness.app.clone().oneshot(req_void).await.unwrap()).await;
    assert_eq!(s_void, StatusCode::NOT_FOUND);
    assert_eq!(b_void["code"], "NOT_FOUND");

    // 3. Tenant B attempts to allocate payment to Tenant A invoice -> 404
    let req_pay = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.tenant_b_token))
        .header("X-Tenant-ID", &harness.tenant_b_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({
            "invoice_id": inv_a,
            "amount": 100_000,
            "payment_method": "CASH"
        }).to_string()))
        .unwrap();
    let (s_pay, b_pay, _) = parse_response(harness.app.clone().oneshot(req_pay).await.unwrap()).await;
    assert_eq!(s_pay, StatusCode::NOT_FOUND);
    assert_eq!(b_pay["code"], "NOT_FOUND");
}

// ===========================================================================
// OBJECTIVE 5: Round 2 Adversarial Stress-Testing Matrix for SQLite Triggers
// ===========================================================================

#[tokio::test]
async fn challenge_r2_status_regression_to_draft_exhaustive_matrix() {
    let harness = setup_challenger_harness().await;

    // Helper to create an issued invoice
    async fn setup_invoice(harness: &ChallengerHarness, name: &str, amount: i64) -> (String, i64) {
        let (inv_id, _) = create_draft(harness, &harness.owner_token, &harness.tenant_a_id, name, amount).await;
        let (_, b_iss) = issue_invoice_api(harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;
        let total = b_iss["total_amount"].as_i64().unwrap();
        (inv_id, total)
    }

    // 1. Status regression on ISSUED invoice -> DRAFT must be aborted
    let (inv_iss, _) = setup_invoice(&harness, "Regression Iss", 500_000).await;
    let iss_to_draft = sqlx::query("UPDATE invoices SET status = 'DRAFT' WHERE id = ?1")
        .bind(&inv_iss)
        .execute(&harness.pool)
        .await;
    assert!(iss_to_draft.is_err(), "Trigger 8.1 must abort ISSUED -> DRAFT status regression");
    let err_str = iss_to_draft.unwrap_err().to_string();
    assert!(
        err_str.contains("Issued, paid, or voided invoices cannot have financial terms modified or invalid status transitions"),
        "Unexpected error: {}",
        err_str
    );

    // 2. Status regression on PARTIALLY_PAID invoice -> DRAFT must be aborted
    let (inv_part, _) = setup_invoice(&harness, "Regression Part", 1_000_000).await;
    let pay_part_payload = json!({
        "invoice_id": inv_part,
        "amount": 300_000,
        "payment_method": "BANK_TRANSFER"
    });
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_part_payload.to_string()))
        .unwrap();
    let (s_pay, _, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(s_pay, StatusCode::CREATED);
    let st: String = sqlx::query_scalar("SELECT status FROM invoices WHERE id = ?1")
        .bind(&inv_part)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(st, "PARTIALLY_PAID");

    let part_to_draft = sqlx::query("UPDATE invoices SET status = 'DRAFT' WHERE id = ?1")
        .bind(&inv_part)
        .execute(&harness.pool)
        .await;
    assert!(part_to_draft.is_err(), "Trigger 8.1 must abort PARTIALLY_PAID -> DRAFT status regression");

    // Also verify PARTIALLY_PAID -> ISSUED is aborted
    let part_to_iss = sqlx::query("UPDATE invoices SET status = 'ISSUED' WHERE id = ?1")
        .bind(&inv_part)
        .execute(&harness.pool)
        .await;
    assert!(part_to_iss.is_err(), "Trigger 8.1 must abort PARTIALLY_PAID -> ISSUED regression");

    // Also verify PARTIALLY_PAID -> VOIDED is aborted
    let part_to_void = sqlx::query("UPDATE invoices SET status = 'VOIDED' WHERE id = ?1")
        .bind(&inv_part)
        .execute(&harness.pool)
        .await;
    assert!(part_to_void.is_err(), "Trigger 8.1 must abort PARTIALLY_PAID -> VOIDED regression");

    // 3. Status regression on PAID invoice -> DRAFT must be aborted
    let (inv_paid, total_paid) = setup_invoice(&harness, "Regression Paid", 500_000).await;
    let pay_full_payload = json!({
        "invoice_id": inv_paid,
        "amount": total_paid,
        "payment_method": "BANK_TRANSFER"
    });
    let pay_full_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_full_payload.to_string()))
        .unwrap();
    let (s_pay_full, _, _) = parse_response(harness.app.clone().oneshot(pay_full_req).await.unwrap()).await;
    assert_eq!(s_pay_full, StatusCode::CREATED);
    let st_paid: String = sqlx::query_scalar("SELECT status FROM invoices WHERE id = ?1")
        .bind(&inv_paid)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(st_paid, "PAID");

    let paid_to_draft = sqlx::query("UPDATE invoices SET status = 'DRAFT' WHERE id = ?1")
        .bind(&inv_paid)
        .execute(&harness.pool)
        .await;
    assert!(paid_to_draft.is_err(), "Trigger 8.1 must abort PAID -> DRAFT status regression");

    let paid_to_iss = sqlx::query("UPDATE invoices SET status = 'ISSUED' WHERE id = ?1")
        .bind(&inv_paid)
        .execute(&harness.pool)
        .await;
    assert!(paid_to_iss.is_err(), "Trigger 8.1 must abort PAID -> ISSUED regression");

    let paid_to_part = sqlx::query("UPDATE invoices SET status = 'PARTIALLY_PAID' WHERE id = ?1")
        .bind(&inv_paid)
        .execute(&harness.pool)
        .await;
    assert!(paid_to_part.is_err(), "Trigger 8.1 must abort PAID -> PARTIALLY_PAID regression");

    // 4. Status regression on VOIDED invoice -> DRAFT must be aborted
    let (inv_void, _) = setup_invoice(&harness, "Regression Void", 400_000).await;
    let void_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_void))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Customer cancellation" }).to_string()))
        .unwrap();
    let (s_void, _, _) = parse_response(harness.app.clone().oneshot(void_req).await.unwrap()).await;
    assert_eq!(s_void, StatusCode::OK);
    let st_void: String = sqlx::query_scalar("SELECT status FROM invoices WHERE id = ?1")
        .bind(&inv_void)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(st_void, "VOIDED");

    let void_to_draft = sqlx::query("UPDATE invoices SET status = 'DRAFT' WHERE id = ?1")
        .bind(&inv_void)
        .execute(&harness.pool)
        .await;
    assert!(void_to_draft.is_err(), "Trigger 8.1 must abort VOIDED -> DRAFT status regression");

    let void_to_iss = sqlx::query("UPDATE invoices SET status = 'ISSUED' WHERE id = ?1")
        .bind(&inv_void)
        .execute(&harness.pool)
        .await;
    assert!(void_to_iss.is_err(), "Trigger 8.1 must abort VOIDED -> ISSUED regression");
}

#[tokio::test]
async fn challenge_r2_deletion_prevention_on_non_draft_exhaustive_matrix() {
    let harness = setup_challenger_harness().await;

    // Positive control: DRAFT invoice can be deleted
    let (inv_draft, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Delete Draft", 300_000).await;
    let del_draft = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_draft)
        .execute(&harness.pool)
        .await;
    assert!(del_draft.is_ok(), "Positive control: DRAFT invoice deletion must succeed");

    // 1. ISSUED invoice deletion must be aborted by Trigger 8.2
    let (inv_iss, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Delete Iss", 400_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_iss).await;
    let del_iss = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_iss)
        .execute(&harness.pool)
        .await;
    assert!(del_iss.is_err(), "Trigger 8.2 must abort DELETE on ISSUED invoice");
    let err_str = del_iss.unwrap_err().to_string();
    assert!(
        err_str.contains("Non-draft invoices cannot be deleted"),
        "Unexpected error: {}",
        err_str
    );

    // 2. PARTIALLY_PAID invoice deletion must be aborted by Trigger 8.2
    let (inv_part, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Delete Part", 1_000_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_part).await;
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({
            "invoice_id": inv_part,
            "amount": 250_000,
            "payment_method": "BANK_TRANSFER"
        }).to_string()))
        .unwrap();
    let (s_pay, _, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(s_pay, StatusCode::CREATED);
    let del_part = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_part)
        .execute(&harness.pool)
        .await;
    assert!(del_part.is_err(), "Trigger 8.2 must abort DELETE on PARTIALLY_PAID invoice");

    // 3. PAID invoice deletion must be aborted by Trigger 8.2
    let (inv_paid, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Delete Paid", 500_000).await;
    let (_, b_iss) = issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_paid).await;
    let total = b_iss["total_amount"].as_i64().unwrap();
    let pay_full_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({
            "invoice_id": inv_paid,
            "amount": total,
            "payment_method": "CASH"
        }).to_string()))
        .unwrap();
    let (s_full, _, _) = parse_response(harness.app.clone().oneshot(pay_full_req).await.unwrap()).await;
    assert_eq!(s_full, StatusCode::CREATED);
    let del_paid = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_paid)
        .execute(&harness.pool)
        .await;
    assert!(del_paid.is_err(), "Trigger 8.2 must abort DELETE on PAID invoice");

    // 4. VOIDED invoice deletion must be aborted by Trigger 8.2
    let (inv_void, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Delete Void", 600_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_void).await;
    let void_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_void))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Voiding" }).to_string()))
        .unwrap();
    let (s_void, _, _) = parse_response(harness.app.clone().oneshot(void_req).await.unwrap()).await;
    assert_eq!(s_void, StatusCode::OK);
    let del_void = sqlx::query("DELETE FROM invoices WHERE id = ?1")
        .bind(&inv_void)
        .execute(&harness.pool)
        .await;
    assert!(del_void.is_err(), "Trigger 8.2 must abort DELETE on VOIDED invoice");
}

#[tokio::test]
async fn challenge_r2_invoice_items_insertion_prevention_on_non_draft_exhaustive_matrix() {
    let harness = setup_challenger_harness().await;

    // Helper to attempt line item insert
    async fn try_insert_item(pool: &sqlx::SqlitePool, invoice_id: &str, tenant_id: &str) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
        let item_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO invoice_items (id, invoice_id, tenant_id, description, quantity, unit_price, discount, tax_amount, line_total, created_at)
             VALUES (?1, ?2, ?3, 'Malicious Insert Attempt', 1, 100000, 0, 11000, 111000, '2026-10-02T12:00:00Z')"
        )
        .bind(&item_id)
        .bind(invoice_id)
        .bind(tenant_id)
        .execute(pool)
        .await
    }

    // 0. Positive control: Line item insert on DRAFT invoice must succeed
    let (inv_draft, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Item Draft", 200_000).await;
    let draft_ins = try_insert_item(&harness.pool, &inv_draft, &harness.tenant_a_id).await;
    assert!(draft_ins.is_ok(), "Line item insert on DRAFT invoice must be allowed");

    // 1. Line item insert on ISSUED invoice must be aborted by Trigger 8.2b
    let (inv_iss, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Item Iss", 300_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_iss).await;
    let iss_ins = try_insert_item(&harness.pool, &inv_iss, &harness.tenant_a_id).await;
    assert!(iss_ins.is_err(), "Trigger 8.2b must abort INSERT INTO invoice_items on ISSUED invoice");
    let err_str = iss_ins.unwrap_err().to_string();
    assert!(
        err_str.contains("Line items cannot be added to issued or finalized invoices"),
        "Unexpected error: {}",
        err_str
    );

    // 2. Line item insert on PARTIALLY_PAID invoice must be aborted by Trigger 8.2b
    let (inv_part, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Item Part", 1_000_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_part).await;
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({
            "invoice_id": inv_part,
            "amount": 200_000,
            "payment_method": "BANK_TRANSFER"
        }).to_string()))
        .unwrap();
    let (s_pay, _, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(s_pay, StatusCode::CREATED);
    let part_ins = try_insert_item(&harness.pool, &inv_part, &harness.tenant_a_id).await;
    assert!(part_ins.is_err(), "Trigger 8.2b must abort INSERT INTO invoice_items on PARTIALLY_PAID invoice");

    // 3. Line item insert on PAID invoice must be aborted by Trigger 8.2b
    let (inv_paid, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Item Paid", 500_000).await;
    let (_, b_iss) = issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_paid).await;
    let total = b_iss["total_amount"].as_i64().unwrap();
    let pay_full = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({
            "invoice_id": inv_paid,
            "amount": total,
            "payment_method": "CASH"
        }).to_string()))
        .unwrap();
    let (s_full, _, _) = parse_response(harness.app.clone().oneshot(pay_full).await.unwrap()).await;
    assert_eq!(s_full, StatusCode::CREATED);
    let paid_ins = try_insert_item(&harness.pool, &inv_paid, &harness.tenant_a_id).await;
    assert!(paid_ins.is_err(), "Trigger 8.2b must abort INSERT INTO invoice_items on PAID invoice");

    // 4. Line item insert on VOIDED invoice must be aborted by Trigger 8.2b
    let (inv_void, _) = create_draft(&harness, &harness.owner_token, &harness.tenant_a_id, "Item Void", 400_000).await;
    issue_invoice_api(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_void).await;
    let void_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_void))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Customer changed mind" }).to_string()))
        .unwrap();
    let (s_void, _, _) = parse_response(harness.app.clone().oneshot(void_req).await.unwrap()).await;
    assert_eq!(s_void, StatusCode::OK);
    let void_ins = try_insert_item(&harness.pool, &inv_void, &harness.tenant_a_id).await;
    assert!(void_ins.is_err(), "Trigger 8.2b must abort INSERT INTO invoice_items on VOIDED invoice");
}
