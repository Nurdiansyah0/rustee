//! Integration and Contract Verification Test Suite for Milestone 3
//! Features 16-21: Commercial Invoicing, Gapless Numbering, Document Snapshots,
//! Receivables & Aging, Atomic Payment Allocation, and Mutation Idempotency.

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
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    #[allow(dead_code)]
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("invoicing_test.sqlite");
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

    let jwt_secret = "m3_invoicing_jwt_secret_key_1234567890_super_secret";
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

    // 2. Staff User A
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

    // 3. Tenant A
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

    // 4. Tenant B & User B for isolation testing
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
            user_id: user_b_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    TestHarness {
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

// ---------------------------------------------------------------------------
// FEATURE 16: Commercial Invoice Lifecycle & State Machine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_invoice_draft_success_and_integer_arithmetic() {
    let harness = setup_harness().await;

    let payload = json!({
        "customer_name": "PT Sumber Rejeki",
        "customer_address": "Jl. Gatot Subroto No. 42",
        "customer_email": "finance@sumberrejeki.com",
        "due_date": "2026-11-15T00:00:00Z",
        "currency": "IDR",
        "tax_type": "PPN_11_EXCL",
        "items": [
            {
                "description": "Software Engineering Services",
                "quantity": 10,
                "unit_price": 500_000,
                "discount": 100_000
            },
            {
                "description": "DevOps Consulting",
                "quantity": 2,
                "unit_price": 1_000_000,
                "discount": 0
            }
        ]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let res = harness.app.clone().oneshot(req).await.unwrap();
    let (status, body, _) = parse_response(res).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "DRAFT");
    assert!(body["invoice_number"].is_null());

    // Item 1: (10 * 500_000) - 100_000 = 4_900_000
    // Item 2: (2 * 1_000_000) - 0 = 2_000_000
    // Subtotal: 6_900_000
    // PPN 11% Excl: round(6_900_000 * 0.11) = 759_000
    // Total: 7_659_000
    assert_eq!(body["subtotal"], 6_900_000);
    assert_eq!(body["tax_amount"], 759_000);
    assert_eq!(body["total_amount"], 7_659_000);
}

#[tokio::test]
async fn test_create_invoice_empty_items_rejected_400() {
    let harness = setup_harness().await;

    let payload = json!({
        "customer_name": "Test Client",
        "items": []
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let res = harness.app.clone().oneshot(req).await.unwrap();
    let (status, body, _) = parse_response(res).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_ITEMS");
}

#[tokio::test]
async fn test_create_invoice_negative_or_zero_qty_or_price_rejected_400() {
    let harness = setup_harness().await;

    // Zero quantity
    let payload = json!({
        "customer_name": "Test Client",
        "items": [{ "description": "Item", "quantity": 0, "unit_price": 1000 }]
    });
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_QUANTITY");

    // Negative unit price
    let payload_price = json!({
        "customer_name": "Test Client",
        "items": [{ "description": "Item", "quantity": 1, "unit_price": -500 }]
    });
    let req_price = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload_price.to_string()))
        .unwrap();
    let (status_p, body_p, _) = parse_response(harness.app.clone().oneshot(req_price).await.unwrap()).await;
    assert_eq!(status_p, StatusCode::BAD_REQUEST);
    assert_eq!(body_p["code"], "INVALID_PRICE");
}

#[tokio::test]
async fn test_draft_invoice_update_succeeds_but_issued_returns_409() {
    let harness = setup_harness().await;

    // 1. Create draft
    let create_payload = json!({
        "customer_name": "Initial Name",
        "items": [{ "description": "Item A", "quantity": 1, "unit_price": 100_000 }]
    });
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED);
    let invoice_id = body["id"].as_str().unwrap();

    // 2. Update while DRAFT succeeds
    let update_payload = json!({
        "customer_name": "Updated Customer Name",
        "customer_address": "Updated Address"
    });
    let update_req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/v1/invoices/{}", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(update_payload.to_string()))
        .unwrap();
    let (u_status, u_body, _) = parse_response(harness.app.clone().oneshot(update_req).await.unwrap()).await;
    assert_eq!(u_status, StatusCode::OK);
    assert_eq!(u_body["customer_name"], "Updated Customer Name");

    // 3. Issue the invoice
    let issue_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();
    let (i_status, i_body, _) = parse_response(harness.app.clone().oneshot(issue_req).await.unwrap()).await;
    assert_eq!(i_status, StatusCode::OK);
    assert_eq!(i_body["status"], "ISSUED");

    // 4. Attempt to update issued invoice -> HTTP 409 INVOICE_LOCKED
    let locked_payload = json!({ "customer_name": "Hacked After Issue" });
    let locked_req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/v1/invoices/{}", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(locked_payload.to_string()))
        .unwrap();
    let (l_status, l_body, _) = parse_response(harness.app.clone().oneshot(locked_req).await.unwrap()).await;
    assert_eq!(l_status, StatusCode::CONFLICT);
    assert_eq!(l_body["code"], "INVOICE_LOCKED");
}

#[tokio::test]
async fn test_void_draft_and_void_issued_with_journal_reversal() {
    let harness = setup_harness().await;

    // --- Void a DRAFT invoice ---
    let create_payload = json!({
        "customer_name": "Draft Client",
        "items": [{ "description": "Item", "quantity": 1, "unit_price": 200_000 }]
    });
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(create_payload.to_string()))
        .unwrap();
    let (_, draft_body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    let draft_id = draft_body["id"].as_str().unwrap();

    let void_draft_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", draft_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Draft discarded" }).to_string()))
        .unwrap();
    let (v_status, v_body, _) = parse_response(harness.app.clone().oneshot(void_draft_req).await.unwrap()).await;
    assert_eq!(v_status, StatusCode::OK);
    assert_eq!(v_body["status"], "VOIDED");

    // --- Void an ISSUED invoice (posts journal reversal) ---
    let create_issued_payload = json!({
        "customer_name": "Client To Void",
        "items": [{ "description": "Item", "quantity": 1, "unit_price": 300_000 }]
    });
    let req2 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(create_issued_payload.to_string()))
        .unwrap();
    let (_, issued_create_body, _) = parse_response(harness.app.clone().oneshot(req2).await.unwrap()).await;
    let issued_id = issued_create_body["id"].as_str().unwrap();

    let issue_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", issued_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();
    let (i_status, _, _) = parse_response(harness.app.clone().oneshot(issue_req).await.unwrap()).await;
    assert_eq!(i_status, StatusCode::OK);

    // Verify journal was posted
    let orig_journal: (String, i64) = sqlx::query_as(
        "SELECT id, is_reversed FROM journal_entries WHERE tenant_id = ?1 AND source_type = 'INVOICE' AND source_id = ?2",
    )
    .bind(&harness.tenant_a_id)
    .bind(issued_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(orig_journal.1, 0); // Not reversed yet

    // Void the issued invoice
    let void_issued_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", issued_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Customer cancelled order" }).to_string()))
        .unwrap();
    let (v_iss_status, v_iss_body, _) = parse_response(harness.app.clone().oneshot(void_issued_req).await.unwrap()).await;
    assert_eq!(v_iss_status, StatusCode::OK);
    assert_eq!(v_iss_body["status"], "VOIDED");

    // Verify original journal is now marked reversed
    let updated_journal_is_rev: i64 = sqlx::query_scalar(
        "SELECT is_reversed FROM journal_entries WHERE id = ?1",
    )
    .bind(&orig_journal.0)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(updated_journal_is_rev, 1);

    // Verify reversal journal entry was created
    let rev_journal_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE tenant_id = ?1 AND source_type = 'REVERSAL' AND source_id = ?2",
    )
    .bind(&harness.tenant_a_id)
    .bind(&orig_journal.0)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(rev_journal_count, 1);

    // Verify receivable status is VOIDED with outstanding = 0
    let rec_status: (String, i64) = sqlx::query_as(
        "SELECT status, outstanding_amount FROM receivables WHERE invoice_id = ?1 AND tenant_id = ?2",
    )
    .bind(issued_id)
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(rec_status.0, "VOIDED");
    assert_eq!(rec_status.1, 0);
}

// ---------------------------------------------------------------------------
// FEATURE 17: Gapless Sequential Numbering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gapless_sequential_numbering_and_tenant_isolation() {
    let harness = setup_harness().await;

    let year = Utc::now().format("%Y").to_string();

    // Tenant A creates 2 invoices
    let inv1_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 100_000).await;
    let inv2_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 200_000).await;

    // Issue invoice 1 -> INV-YYYY-000001
    let num1 = issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv1_id).await;
    assert_eq!(num1, format!("INV-{}-000001", year));

    // Issue invoice 2 -> INV-YYYY-000002
    let num2 = issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv2_id).await;
    assert_eq!(num2, format!("INV-{}-000002", year));

    // Tenant B issues first invoice -> INV-YYYY-000001 (isolated counter!)
    let inv_b_id = create_test_draft(&harness, &harness.tenant_b_token, &harness.tenant_b_id, 500_000).await;
    let num_b = issue_test_invoice(&harness, &harness.tenant_b_token, &harness.tenant_b_id, &inv_b_id).await;
    assert_eq!(num_b, format!("INV-{}-000001", year));
}

#[tokio::test]
async fn test_custom_invoice_prefix_from_business_profile() {
    let harness = setup_harness().await;
    let year = Utc::now().format("%Y").to_string();

    // Set custom invoice prefix 'FAK' for Tenant A
    sqlx::query(
        "INSERT INTO business_profiles (id, tenant_id, business_name, invoice_prefix, created_at, updated_at) VALUES ('bp1', ?1, 'Tenant Alpha Corp', 'FAK', '2026-01-01', '2026-01-01') ON CONFLICT(tenant_id) DO UPDATE SET invoice_prefix = 'FAK'",
    )
    .bind(&harness.tenant_a_id)
    .execute(&harness.pool)
    .await
    .unwrap();

    let inv_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 500_000).await;
    let num = issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    assert!(num.starts_with(&format!("FAK-{}-", year)));
}

// ---------------------------------------------------------------------------
// FEATURE 18: Issued Document Snapshotting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_issued_document_snapshot_immutability() {
    let harness = setup_harness().await;

    let payload = json!({
        "customer_name": "Snapshot Customer Ltd",
        "customer_address": "Jl. Merdeka No. 17",
        "customer_email": "tax@snapshot.com",
        "items": [
            { "description": "Widget Pro", "quantity": 5, "unit_price": 200_000, "discount": 0 }
        ]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let (_, draft, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    let invoice_id = draft["id"].as_str().unwrap();

    // Issue invoice
    let issue_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();
    let (_, issued_body, _) = parse_response(harness.app.clone().oneshot(issue_req).await.unwrap()).await;

    assert!(issued_body["snapshot"].is_object());
    assert_eq!(issued_body["snapshot"]["customer_name"], "Snapshot Customer Ltd");
    assert_eq!(issued_body["snapshot"]["total_amount"], 1_110_000); // 1M + 11% PPN

    // GET /api/v1/invoices/:id includes snapshot
    let get_req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/invoices/{}", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();
    let (g_status, g_body, _) = parse_response(harness.app.clone().oneshot(get_req).await.unwrap()).await;
    assert_eq!(g_status, StatusCode::OK);
    assert_eq!(g_body["snapshot"]["customer_name"], "Snapshot Customer Ltd");
}

// ---------------------------------------------------------------------------
// FEATURE 19: Commercial Receivables & Aging Calculation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_receivables_listing_and_aging_calculation() {
    let harness = setup_harness().await;

    // Issue an invoice
    let inv_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;
    issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // 1. List receivables
    let list_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/receivables")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();
    let (l_status, l_body, _) = parse_response(harness.app.clone().oneshot(list_req).await.unwrap()).await;
    assert_eq!(l_status, StatusCode::OK);
    assert_eq!(l_body["count"], 1);
    assert_eq!(l_body["receivables"][0]["status"], "OPEN");

    // 2. Insert aged receivables directly into DB to test all 4 buckets
    let rec_35_days = Uuid::new_v4().to_string();
    let inv_fake_1 = Uuid::new_v4().to_string();
    let due_35_days = (Utc::now() - chrono::Duration::days(35)).format("%Y-%m-%d").to_string();

    let rec_75_days = Uuid::new_v4().to_string();
    let inv_fake_2 = Uuid::new_v4().to_string();
    let due_75_days = (Utc::now() - chrono::Duration::days(75)).format("%Y-%m-%d").to_string();

    let rec_100_days = Uuid::new_v4().to_string();
    let inv_fake_3 = Uuid::new_v4().to_string();
    let due_100_days = (Utc::now() - chrono::Duration::days(100)).format("%Y-%m-%d").to_string();

    for (inv_id, amount) in [(&inv_fake_1, 500_000), (&inv_fake_2, 750_000), (&inv_fake_3, 1_000_000)] {
        sqlx::query(
            "INSERT INTO invoices (id, tenant_id, customer_name, due_date, currency, tax_type, subtotal, discount, tax_amount, total_amount, balance_due, status, created_at, updated_at) VALUES (?1, ?2, 'Aged Client', '2026-01-01', 'IDR', 'NONE', ?3, 0, 0, ?3, ?3, 'ISSUED', '2026-01-01', '2026-01-01')"
        )
        .bind(inv_id)
        .bind(&harness.tenant_a_id)
        .bind(amount)
        .execute(&harness.pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at) VALUES (?1, ?2, ?3, 500000, 0, 500000, ?4, 'OPEN', '2026-01-01', '2026-01-01')"
    )
    .bind(&rec_35_days)
    .bind(&harness.tenant_a_id)
    .bind(&inv_fake_1)
    .bind(&due_35_days)
    .execute(&harness.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at) VALUES (?1, ?2, ?3, 750000, 0, 750000, ?4, 'OPEN', '2026-01-01', '2026-01-01')"
    )
    .bind(&rec_75_days)
    .bind(&harness.tenant_a_id)
    .bind(&inv_fake_2)
    .bind(&due_75_days)
    .execute(&harness.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO receivables (id, tenant_id, invoice_id, total_amount, allocated_amount, outstanding_amount, due_date, status, created_at, updated_at) VALUES (?1, ?2, ?3, 1000000, 0, 1000000, ?4, 'OPEN', '2026-01-01', '2026-01-01')"
    )
    .bind(&rec_100_days)
    .bind(&harness.tenant_a_id)
    .bind(&inv_fake_3)
    .bind(&due_100_days)
    .execute(&harness.pool)
    .await
    .unwrap();

    // 3. Query Aging Report
    let aging_req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/receivables/aging")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();
    let (a_status, a_body, _) = parse_response(harness.app.clone().oneshot(aging_req).await.unwrap()).await;
    assert_eq!(a_status, StatusCode::OK);

    assert_eq!(a_body["current_0_30"], 1_110_000); // 1M + 11% PPN
    assert_eq!(a_body["overdue_31_60"], 500_000);
    assert_eq!(a_body["overdue_61_90"], 750_000);
    assert_eq!(a_body["overdue_90_plus"], 1_000_000);
    assert_eq!(a_body["total_outstanding"], 3_360_000);
}

// ---------------------------------------------------------------------------
// FEATURE 20: Atomic Payment Allocation & Double-Entry Integrity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_payment_allocation_partial_full_and_overpayment_rejected() {
    let harness = setup_harness().await;

    // Issue invoice: 1,000,000 + 11% PPN = 1,110,000 total
    let inv_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;
    issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // 1. Overpayment rejected
    let overpay_payload = json!({
        "invoice_id": inv_id,
        "amount": 1_200_000,
        "payment_method": "BANK_TRANSFER"
    });
    let over_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(overpay_payload.to_string()))
        .unwrap();
    let (o_status, o_body, _) = parse_response(harness.app.clone().oneshot(over_req).await.unwrap()).await;
    assert_eq!(o_status, StatusCode::BAD_REQUEST);
    assert_eq!(o_body["code"], "OVERPAYMENT_NOT_ALLOWED");

    // 2. Zero or negative amount rejected
    let zero_payload = json!({ "invoice_id": inv_id, "amount": 0 });
    let zero_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(zero_payload.to_string()))
        .unwrap();
    let (z_status, z_body, _) = parse_response(harness.app.clone().oneshot(zero_req).await.unwrap()).await;
    assert_eq!(z_status, StatusCode::BAD_REQUEST);
    assert_eq!(z_body["code"], "INVALID_AMOUNT");

    // 3. Partial payment of 500,000
    let pay_partial_payload = json!({
        "invoice_id": inv_id,
        "amount": 500_000,
        "payment_method": "BANK_TRANSFER",
        "reference": "TRX-PARTIAL-1"
    });
    let pay_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_partial_payload.to_string()))
        .unwrap();
    let (p_status, p_body, _) = parse_response(harness.app.clone().oneshot(pay_req).await.unwrap()).await;
    assert_eq!(p_status, StatusCode::CREATED);
    assert_eq!(p_body["amount"], 500_000);
    assert_eq!(p_body["outstanding_balance"], 610_000);
    assert_eq!(p_body["status"], "CONFIRMED");

    // Verify invoice & receivable status is PARTIALLY_PAID
    let (inv_status, inv_balance): (String, i64) = sqlx::query_as(
        "SELECT status, balance_due FROM invoices WHERE id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(inv_status, "PARTIALLY_PAID");
    assert_eq!(inv_balance, 610_000);

    // Verify GL payment entry was posted: Debit 1100 (500k), Credit 1200 (500k)
    let payment_journal: (i64, i64) = sqlx::query_as(
        r#"
        SELECT SUM(jl.debit), SUM(jl.credit)
        FROM journal_lines jl
        JOIN journal_entries je ON je.id = jl.journal_id
        WHERE je.source_type = 'PAYMENT' AND je.tenant_id = ?1
        "#,
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(payment_journal.0, 500_000);
    assert_eq!(payment_journal.1, 500_000);

    // 4. Final settlement payment of 610,000 -> PAID
    let pay_full_payload = json!({
        "invoice_id": inv_id,
        "amount": 610_000,
        "payment_method": "CASH",
        "reference": "TRX-FINAL-SETTLE"
    });
    let full_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_full_payload.to_string()))
        .unwrap();
    let (f_status, f_body, _) = parse_response(harness.app.clone().oneshot(full_req).await.unwrap()).await;
    assert_eq!(f_status, StatusCode::CREATED);
    assert_eq!(f_body["outstanding_balance"], 0);

    // Verify invoice & receivable status is PAID
    let (final_inv_status, final_inv_balance): (String, i64) = sqlx::query_as(
        "SELECT status, balance_due FROM invoices WHERE id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(final_inv_status, "PAID");
    assert_eq!(final_inv_balance, 0);

    // 5. Attempting to void PAID invoice returns HTTP 409 CANNOT_VOID_PAID_INVOICE
    let void_paid_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Late void" }).to_string()))
        .unwrap();
    let (vp_status, vp_body, _) = parse_response(harness.app.clone().oneshot(void_paid_req).await.unwrap()).await;
    assert_eq!(vp_status, StatusCode::CONFLICT);
    assert_eq!(vp_body["code"], "CANNOT_VOID_PAID_INVOICE");
}

// ---------------------------------------------------------------------------
// FEATURE 21: Mutation Idempotency Engine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mutation_idempotency_engine_payments() {
    let harness = setup_harness().await;

    let inv_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;
    issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    let idempotency_key = "idemp_test_payment_uuid_123456";
    let pay_payload = json!({
        "invoice_id": inv_id,
        "amount": 300_000,
        "payment_method": "BANK_TRANSFER"
    });

    // First payment request
    let req1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", idempotency_key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_payload.to_string()))
        .unwrap();
    let (s1, b1, h1) = parse_response(harness.app.clone().oneshot(req1).await.unwrap()).await;
    assert_eq!(s1, StatusCode::CREATED);
    assert!(h1.get("x-cache-replay").is_none());
    let payment_id_1 = b1["id"].as_str().unwrap().to_string();

    // Replay identical request with same key -> cached 201 with X-Cache-Replay
    let req2 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", idempotency_key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_payload.to_string()))
        .unwrap();
    let (s2, b2, h2) = parse_response(harness.app.clone().oneshot(req2).await.unwrap()).await;
    assert_eq!(s2, StatusCode::CREATED);
    assert_eq!(h2.get("x-cache-replay").unwrap(), "true");
    assert_eq!(b2["id"].as_str().unwrap(), payment_id_1);

    // Verify balance was NOT decremented twice!
    let current_outstanding: i64 = sqlx::query_scalar(
        "SELECT outstanding_amount FROM receivables WHERE invoice_id = ?1",
    )
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(current_outstanding, 810_000); // 1,110,000 - 300,000 = 810,000 exactly once

    // Replay same key with mismatched payload -> HTTP 409 IDEMPOTENCY_KEY_MISMATCH
    let mismatched_payload = json!({
        "invoice_id": inv_id,
        "amount": 400_000,
        "payment_method": "BANK_TRANSFER"
    });
    let req_mismatch = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", idempotency_key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(mismatched_payload.to_string()))
        .unwrap();
    let (sm, bm, _) = parse_response(harness.app.clone().oneshot(req_mismatch).await.unwrap()).await;
    assert_eq!(sm, StatusCode::CONFLICT);
    assert_eq!(bm["code"], "IDEMPOTENCY_KEY_MISMATCH");
}

// ---------------------------------------------------------------------------
// SQLITE IMMUTABILITY TRIGGERS INTEGRITY VERIFICATION
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sqlite_triggers_prevent_tampering_on_issued_invoices_and_payments() {
    let harness = setup_harness().await;

    // Issue invoice
    let inv_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 1_000_000).await;
    issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // 1. Direct SQL update on financial field of issued invoice must be aborted by trigger
    let trigger_err = sqlx::query(
        "UPDATE invoices SET subtotal = 999999 WHERE id = ?1",
    )
    .bind(&inv_id)
    .execute(&harness.pool)
    .await;
    assert!(trigger_err.is_err(), "Trigger must abort direct update of issued invoice subtotal");

    // 2. Direct SQL delete on issued invoice must be aborted by trigger
    let del_err = sqlx::query(
        "DELETE FROM invoices WHERE id = ?1",
    )
    .bind(&inv_id)
    .execute(&harness.pool)
    .await;
    assert!(del_err.is_err(), "Trigger must abort direct delete of issued invoice");

    // 3. Direct SQL update/delete on invoice items of issued invoice must be aborted
    let item_update_err = sqlx::query(
        "UPDATE invoice_items SET unit_price = 100 WHERE invoice_id = ?1",
    )
    .bind(&inv_id)
    .execute(&harness.pool)
    .await;
    assert!(item_update_err.is_err(), "Trigger must abort line item updates on issued invoice");

    // 4. Direct SQL update/delete on snapshot must be aborted
    let snap_update_err = sqlx::query(
        "UPDATE invoice_snapshots SET snapshot_json = '{}' WHERE invoice_id = ?1",
    )
    .bind(&inv_id)
    .execute(&harness.pool)
    .await;
    assert!(snap_update_err.is_err(), "Trigger must abort snapshot updates");

    // 5. Record a payment
    let rec_id: String = sqlx::query_scalar("SELECT id FROM receivables WHERE invoice_id = ?1")
        .bind(&inv_id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    let pay_res = sqlx::query(
        "INSERT INTO payments (id, tenant_id, invoice_id, receivable_id, payment_number, payment_date, amount, payment_method, status, created_at, updated_at) VALUES ('p1', ?1, ?2, ?3, 'PAY-2026-000001', '2026-10-01', 500000, 'BANK_TRANSFER', 'CONFIRMED', '2026-10-01', '2026-10-01')"
    )
    .bind(&harness.tenant_a_id)
    .bind(&inv_id)
    .bind(&rec_id)
    .execute(&harness.pool)
    .await;
    assert!(pay_res.is_ok());

    // Direct SQL update on confirmed payment amount must be aborted
    let pay_mod_err = sqlx::query(
        "UPDATE payments SET amount = 999999 WHERE id = 'p1'",
    )
    .execute(&harness.pool)
    .await;
    assert!(pay_mod_err.is_err(), "Trigger must abort update of confirmed payment amount");

    // Direct SQL delete on confirmed payment must be aborted
    let pay_del_err = sqlx::query(
        "DELETE FROM payments WHERE id = 'p1'",
    )
    .execute(&harness.pool)
    .await;
    assert!(pay_del_err.is_err(), "Trigger must abort delete of confirmed payment");
}

#[tokio::test]
async fn test_cross_tenant_invoice_access_strictly_returns_404() {
    let harness = setup_harness().await;

    // Tenant A creates invoice
    let inv_a = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 300_000).await;

    // Tenant B attempts to read Tenant A's invoice
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/invoices/{}", inv_a))
        .header(AUTHORIZATION, format!("Bearer {}", harness.tenant_b_token))
        .header("X-Tenant-ID", &harness.tenant_b_id)
        .body(Body::empty())
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

#[tokio::test]
async fn test_staff_role_can_issue_invoice_and_post_ledger() {
    let harness = setup_harness().await;

    // Staff creates draft
    let inv_id = create_test_draft(&harness, &harness.staff_token, &harness.tenant_a_id, 450_000).await;

    // Staff issues invoice -> system posting context ensures ledger entry is posted without 403 Forbidden
    let issue_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.staff_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(issue_req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ISSUED");

    // Verify journal was posted
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM journal_entries WHERE tenant_id = ?1 AND source_type = 'INVOICE' AND source_id = ?2",
    )
    .bind(&harness.tenant_a_id)
    .bind(&inv_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_idempotent_invoice_creation_and_issue_replay() {
    let harness = setup_harness().await;

    let payload = json!({
        "customer_name": "Idempotent Corp",
        "items": [{ "description": "Consulting", "quantity": 1, "unit_price": 500_000 }]
    });

    // 1. Create with Idempotency-Key
    let key = "inv_create_key_unique_999";
    let req1 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let (s1, b1, h1) = parse_response(harness.app.clone().oneshot(req1).await.unwrap()).await;
    assert_eq!(s1, StatusCode::CREATED);
    assert!(h1.get("x-cache-replay").is_none());
    let inv_id = b1["id"].as_str().unwrap().to_string();

    // Replay creation -> cached response
    let req2 = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", key)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let (s2, b2, h2) = parse_response(harness.app.clone().oneshot(req2).await.unwrap()).await;
    assert_eq!(s2, StatusCode::CREATED);
    assert_eq!(h2.get("x-cache-replay").unwrap(), "true");
    assert_eq!(b2["id"].as_str().unwrap(), inv_id);

    // 2. Issue with Idempotency-Key
    let issue_key = "inv_issue_key_unique_888";
    let req_issue_1 = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", issue_key)
        .body(Body::empty())
        .unwrap();

    let (si1, bi1, hi1) = parse_response(harness.app.clone().oneshot(req_issue_1).await.unwrap()).await;
    assert_eq!(si1, StatusCode::OK);
    assert!(hi1.get("x-cache-replay").is_none());
    let inv_number = bi1["invoice_number"].as_str().unwrap().to_string();

    // Replay issue -> cached response with x-cache-replay
    let req_issue_2 = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", issue_key)
        .body(Body::empty())
        .unwrap();

    let (si2, bi2, hi2) = parse_response(harness.app.clone().oneshot(req_issue_2).await.unwrap()).await;
    assert_eq!(si2, StatusCode::OK);
    assert_eq!(hi2.get("x-cache-replay").unwrap(), "true");
    assert_eq!(bi2["invoice_number"].as_str().unwrap(), inv_number);
}

#[tokio::test]
async fn test_empty_idempotency_key_ignored_and_user_scoping() {
    let harness = setup_harness().await;

    // Issue invoice
    let inv_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 600_000).await;
    issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // Empty key is ignored
    let pay_payload = json!({
        "invoice_id": inv_id,
        "amount": 100_000,
        "payment_method": "CASH"
    });
    let req_empty = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header("Idempotency-Key", "   ")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(pay_payload.to_string()))
        .unwrap();

    let (se, be, he) = parse_response(harness.app.clone().oneshot(req_empty).await.unwrap()).await;
    assert_eq!(se, StatusCode::CREATED);
    assert!(he.get("x-cache-replay").is_none());
    assert_eq!(be["amount"], 100_000);
}

#[tokio::test]
async fn test_payment_against_voided_and_nonexistent_invoices() {
    let harness = setup_harness().await;

    // 1. Payment against non-existent invoice returns 404
    let fake_id = Uuid::new_v4().to_string();
    let req_fake = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": fake_id, "amount": 100_000 }).to_string()))
        .unwrap();
    let (sf, bf, _) = parse_response(harness.app.clone().oneshot(req_fake).await.unwrap()).await;
    assert_eq!(sf, StatusCode::NOT_FOUND);
    assert_eq!(bf["code"], "NOT_FOUND");

    // 2. Payment against voided invoice returns 409 INVOICE_VOIDED
    let inv_id = create_test_draft(&harness, &harness.owner_token, &harness.tenant_a_id, 300_000).await;
    issue_test_invoice(&harness, &harness.owner_token, &harness.tenant_a_id, &inv_id).await;

    // Void it
    let void_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/void", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "reason": "Customer cancelled" }).to_string()))
        .unwrap();
    let (sv, _, _) = parse_response(harness.app.clone().oneshot(void_req).await.unwrap()).await;
    assert_eq!(sv, StatusCode::OK);

    // Attempt payment
    let pay_void_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "invoice_id": inv_id, "amount": 100_000 }).to_string()))
        .unwrap();
    let (spv, bpv, _) = parse_response(harness.app.clone().oneshot(pay_void_req).await.unwrap()).await;
    assert_eq!(spv, StatusCode::CONFLICT);
    assert_eq!(bpv["code"], "INVOICE_VOIDED");
}

#[tokio::test]
async fn test_inclusive_tax_pricing_and_revenue_breakdown() {
    let harness = setup_harness().await;

    let payload = json!({
        "customer_name": "Tax Inclusive Client",
        "tax_type": "PPN_11_INCL",
        "items": [{ "description": "Consulting Package", "quantity": 1, "unit_price": 1_110_000 }]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["subtotal"], 1_110_000);
    assert_eq!(body["tax_amount"], 110_000);
    assert_eq!(body["total_amount"], 1_110_000);

    let inv_id = body["id"].as_str().unwrap();

    // Issue invoice
    let issue_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", inv_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-ID", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();
    let (is_status, _, _) = parse_response(harness.app.clone().oneshot(issue_req).await.unwrap()).await;
    assert_eq!(is_status, StatusCode::OK);

    // Verify journal posted:
    // Debit 1200: 1,110,000
    // Credit 4000: 1,000,000 (net revenue)
    // Credit 2100: 110,000 (tax)
    let journal_lines: Vec<(String, i64, i64)> = sqlx::query_as(
        r#"
        SELECT jl.account_code, jl.debit, jl.credit
        FROM journal_lines jl
        JOIN journal_entries je ON je.id = jl.journal_id
        WHERE je.source_type = 'INVOICE' AND je.source_id = ?1
        ORDER BY jl.account_code ASC
        "#,
    )
    .bind(inv_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    assert_eq!(journal_lines.len(), 3);
    assert_eq!(journal_lines[0], ("1200".to_string(), 1_110_000, 0)); // Piutang Usaha
    assert_eq!(journal_lines[1], ("2100".to_string(), 0, 110_000));   // Utang Pajak
    assert_eq!(journal_lines[2], ("4000".to_string(), 0, 1_000_000)); // Pendapatan Usaha
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn create_test_draft(harness: &TestHarness, token: &str, tenant_id: &str, amount: i64) -> String {
    let payload = json!({
        "customer_name": "Test Customer",
        "items": [{ "description": "Consulting", "quantity": 1, "unit_price": amount }]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED);
    body["id"].as_str().unwrap().to_string()
}

async fn issue_test_invoice(harness: &TestHarness, token: &str, tenant_id: &str, invoice_id: &str) -> String {
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id)
        .body(Body::empty())
        .unwrap();

    let (status, body, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    body["invoice_number"].as_str().unwrap().to_string()
}
