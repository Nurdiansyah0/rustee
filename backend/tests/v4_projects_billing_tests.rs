//! Integration Test Suite for Milestone 4:
//! Hybrid Progress Billing & Commercial Invoicing Integration
//! (PRD §10, §11, §12, §13, §14, §15, §17, §20, §31, §60, §62, §72).
//!
//! Tests Features 18, 19, 20, 21, 22, 23, 24, 26, 28:
//! 1. `test_fixed_milestone_progress_billing_happy_path`
//! 2. `test_fixed_milestone_duplicate_billing_conflict_guard`
//! 3. `test_milestone_billing_state_invariants_inactive_and_uncompleted`
//! 4. `test_poc_billing_pure_integer_rupiah_and_cumulative_guard`
//! 5. `test_poc_billing_with_verified_progress_record_linkage`
//! 6. `test_tax_snapshotting_variations_ppn11_ppn12_umkm_exempt`
//! 7. `test_multi_tenant_anti_enumeration_cross_tenant_isolation`
//! 8. `test_rbac_matrix_staff_forbidden_vs_manager_owner_allowed`
//! 9. `test_real_time_profitability_reflects_billed_revenue`
//! 10. `test_transactional_atomicity_and_outbox_event_emission`

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
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    #[allow(dead_code)]
    _dir: tempfile::TempDir,
    owner_id: String,
    owner_token: String,
    manager_token: String,
    staff_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    tenant_b_token: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("projects_m4_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 20,
        min_connections: 1,
        busy_timeout_ms: 5_000,
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

    let jwt_secret = "m4_projects_billing_secret_key_1234567890_super_secret";
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
            email: "owner_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_a@contractor.com", "user", "premium")
        .unwrap();

    // 2. Manager User A
    let manager_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: manager_id.clone(),
            email: "manager_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Manager A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (manager_token, _) = jwt_engine
        .generate_token(&manager_id, "manager_a@contractor.com", "user", "premium")
        .unwrap();

    // 3. Staff User A
    let staff_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: staff_id.clone(),
            email: "staff_a@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Staff A".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .unwrap();

    let (staff_token, _) = jwt_engine
        .generate_token(&staff_id, "staff_a@contractor.com", "user", "free")
        .unwrap();

    // 4. Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha Construction".to_string(),
            slug: "tenant-alpha-construction".to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .unwrap();

    // Seed Chart of Accounts for Tenant A
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
            user_id: manager_id.clone(),
            role: Role::Manager,
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

    // 5. Tenant B (Isolation testing)
    let tenant_b_id = Uuid::new_v4().to_string();
    let user_b_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: user_b_id.clone(),
            email: "owner_b@contractor.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner B".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (tenant_b_token, _) = jwt_engine
        .generate_token(&user_b_id, "owner_b@contractor.com", "user", "premium")
        .unwrap();

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta Engineering".to_string(),
            slug: "tenant-beta-engineering".to_string(),
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
        owner_id,
        owner_token,
        manager_token,
        staff_token,
        tenant_a_id,
        tenant_b_id,
        tenant_b_token,
    }
}

async fn send_req(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: &str,
    tenant_id: &str,
    body: Option<Value>,
) -> (StatusCode, Value, HeaderMap) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .header("X-Tenant-ID", tenant_id);

    if body.is_some() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }

    let body_bytes = match body {
        Some(val) => Body::from(serde_json::to_vec(&val).unwrap()),
        None => Body::empty(),
    };

    let req = builder.body(body_bytes).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();

    let status = resp.status();
    let resp_headers = resp.headers().clone();
    let resp_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json_val = if resp_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&resp_bytes).unwrap_or(Value::Null)
    };

    (status, json_val, resp_headers)
}

/// Helper: creates and activates a project for testing
async fn create_active_project(h: &TestHarness, name: &str, contract_amount: i64) -> String {
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": name,
            "description": "Commercial building contract",
            "customer_name": "PT Mega Konstruksi",
            "billing_type": "HYBRID",
            "budget_amount": contract_amount * 80 / 100,
            "contract_amount": contract_amount
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let project_id = resp["id"].as_str().unwrap().to_string();

    // Transition from DRAFT -> ACTIVE
    let (status, update_resp, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "status": "ACTIVE"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(update_resp["status"], "ACTIVE");

    project_id
}

/// Helper: creates a milestone on a project
async fn create_milestone(h: &TestHarness, project_id: &str, title: &str, billable_amount: i64) -> String {
    let (status, resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "title": title,
            "description": format!("Milestone: {}", title),
            "target_date": "2026-11-01",
            "billable_amount": billable_amount
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    resp["id"].as_str().unwrap().to_string()
}

// ============================================================================
// Test 1: Fixed Milestone Progress Billing Happy Path
// ============================================================================
#[tokio::test]
async fn test_fixed_milestone_progress_billing_happy_path() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Gedung Perkantoran Menara 1", 100_000_000).await;
    let milestone_id = create_milestone(&h, &project_id, "Pondasi & Struktur Bawah", 25_000_000).await;

    // 1. Complete milestone
    let (status, comp_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Failed to complete milestone: {:?}", comp_resp);
    assert_eq!(comp_resp["status"], "COMPLETED");
    assert!(!comp_resp["completed_at"].is_null());
    assert_eq!(comp_resp["is_billed"], false);

    // Verify MilestoneCompleted outbox event emitted
    let count_completed: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?1 AND event_type = 'MilestoneCompleted' AND aggregate_id = ?2;",
    )
    .bind(&h.tenant_a_id)
    .bind(&milestone_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(count_completed.0, 1, "MilestoneCompleted event was not emitted");

    // 2. Bill milestone with 11% PPN exclusive tax
    let (status, bill_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "tax_type": "PPN_11_EXCL",
            "due_date": "2026-12-01T00:00:00Z",
            "notes": "Tagihan Pembayaran Tahap 1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Failed to bill milestone: {:?}", bill_resp);

    // Verify milestone updated
    assert_eq!(bill_resp["milestone"]["is_billed"], true);
    let invoice_id = bill_resp["invoice_id"].as_str().unwrap();
    assert_eq!(bill_resp["milestone"]["invoice_id"], invoice_id);

    // Verify commercial invoice structure
    let inv = &bill_resp["invoice"];
    assert_eq!(inv["status"], "ISSUED");
    let inv_number = inv["invoice_number"].as_str().unwrap();
    assert!(inv_number.starts_with("INV-"), "Invoice number should start with INV-: {}", inv_number);
    assert_eq!(inv["subtotal"], 25_000_000);
    assert_eq!(inv["tax_type"], "PPN_11_EXCL");
    assert_eq!(inv["tax_amount"], 2_750_000); // 11% of 25,000,000
    assert_eq!(inv["total_amount"], 27_750_000);
    assert_eq!(inv["created_by"], h.owner_id);
    assert_eq!(inv["issued_by"], h.owner_id);

    // Verify frozen snapshot in database
    let snap_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoice_snapshots WHERE tenant_id = ?1 AND invoice_id = ?2;",
    )
    .bind(&h.tenant_a_id)
    .bind(invoice_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(snap_count.0, 1, "Snapshot not recorded");

    // Verify open Receivable record
    let rec_row: (i64, i64, String) = sqlx::query_as(
        "SELECT total_amount, outstanding_amount, status FROM receivables WHERE tenant_id = ?1 AND invoice_id = ?2;",
    )
    .bind(&h.tenant_a_id)
    .bind(invoice_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(rec_row.0, 27_750_000);
    assert_eq!(rec_row.1, 27_750_000);
    assert_eq!(rec_row.2, "OPEN");

    // Verify balanced GL journal entry: Debit 1200 = Credit 4000 + Credit 2100
    let journal_rows: Vec<(String, i64, i64)> = sqlx::query_as(
        r#"
        SELECT l.account_code, l.debit, l.credit
        FROM journal_lines l
        JOIN journal_entries e ON l.journal_id = e.id
        WHERE e.tenant_id = ?1 AND e.source_type = 'INVOICE' AND e.source_id = ?2;
        "#,
    )
    .bind(&h.tenant_a_id)
    .bind(invoice_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();

    let total_debit: i64 = journal_rows.iter().map(|r| r.1).sum();
    let total_credit: i64 = journal_rows.iter().map(|r| r.2).sum();
    assert_eq!(total_debit, total_credit, "Journal debit does not equal credit!");
    assert_eq!(total_debit, 27_750_000);

    let ar_line = journal_rows.iter().find(|r| r.0 == "1200").expect("Missing AR line 1200");
    assert_eq!(ar_line.1, 27_750_000);
    assert_eq!(ar_line.2, 0);

    let rev_line = journal_rows.iter().find(|r| r.0 == "4000").expect("Missing Revenue line 4000");
    assert_eq!(rev_line.1, 0);
    assert_eq!(rev_line.2, 25_000_000);

    let tax_line = journal_rows.iter().find(|r| r.0 == "2100").expect("Missing Tax line 2100");
    assert_eq!(tax_line.1, 0);
    assert_eq!(tax_line.2, 2_750_000);

    // Verify ProgressBilled outbox event
    let outbox_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_type, payload_json FROM outbox_events WHERE tenant_id = ?1 AND event_type = 'ProgressBilled' AND aggregate_id = ?2;",
    )
    .bind(&h.tenant_a_id)
    .bind(&milestone_id)
    .fetch_all(&h.pool)
    .await
    .unwrap();
    assert_eq!(outbox_rows.len(), 1, "ProgressBilled event was not emitted");
    let payload: Value = serde_json::from_str(&outbox_rows[0].1).unwrap();
    assert_eq!(payload["billing_type"], "MILESTONE");
    assert_eq!(payload["amount"], 25_000_000);
    assert_eq!(payload["total_amount"], 27_750_000);
    assert_eq!(payload["invoice_id"], invoice_id);
}

// ============================================================================
// Test 2: Fixed Milestone Duplicate Billing Conflict Guard
// ============================================================================
#[tokio::test]
async fn test_fixed_milestone_duplicate_billing_conflict_guard() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Gedung Kantor Dua Lantai", 80_000_000).await;
    let milestone_id = create_milestone(&h, &project_id, "Struktur Atap", 20_000_000).await;

    // Complete milestone
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Bill first time -> OK
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Bill second time -> strictly 409 CONFLICT with MILESTONE_ALREADY_BILLED
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Re-billing must return 409 Conflict: {:?}", err_resp);
    assert_eq!(err_resp["code"], "MILESTONE_ALREADY_BILLED");

    // Verify database has only 1 invoice and 1 receivable for this milestone
    let inv_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM invoices WHERE tenant_id = ?1 AND id IN (SELECT invoice_id FROM milestones WHERE id = ?2);",
    )
    .bind(&h.tenant_a_id)
    .bind(&milestone_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(inv_count.0, 1, "There should only be 1 invoice issued");
}

// ============================================================================
// Test 3: Milestone Billing State Invariants (Inactive Project & Uncompleted)
// ============================================================================
#[tokio::test]
async fn test_milestone_billing_state_invariants_inactive_and_uncompleted() {
    let h = setup_harness().await;

    // Create DRAFT project (do not activate)
    let (status, proj_resp, _) = send_req(
        &h.app,
        Method::POST,
        "/api/v1/projects",
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "name": "Proyek Rumah Tinggal",
            "contract_amount": 50_000_000
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let draft_project_id = proj_resp["id"].as_str().unwrap().to_string();
    assert_eq!(proj_resp["status"], "DRAFT");

    let milestone_id = create_milestone(&h, &draft_project_id, "Tahap Awal", 10_000_000).await;

    // 1. Attempt to complete milestone on DRAFT project -> HTTP 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", draft_project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // 2. Attempt to bill milestone on DRAFT project -> HTTP 422 PROJECT_NOT_ACTIVE
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", draft_project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");

    // Now activate project
    let (status, _, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", draft_project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"status": "ACTIVE"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 3. Attempt to bill milestone while status is PENDING (not completed) -> HTTP 422 MILESTONE_NOT_COMPLETED
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", draft_project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "MILESTONE_NOT_COMPLETED");

    // 4. Milestone with zero billable amount completed and attempted to bill -> HTTP 422 INVALID_BILLABLE_AMOUNT
    let zero_milestone_id = create_milestone(&h, &draft_project_id, "Milestone Non-Tagihan", 0).await;
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", draft_project_id, zero_milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", draft_project_id, zero_milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "INVALID_BILLABLE_AMOUNT");

    // 5. Complete first milestone, put project ON_HOLD, attempt to bill -> HTTP 422 PROJECT_NOT_ACTIVE
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", draft_project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = send_req(
        &h.app,
        Method::PATCH,
        &format!("/api/v1/projects/{}/status", draft_project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"status": "ON_HOLD"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", draft_project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_resp["code"], "PROJECT_NOT_ACTIVE");
}

// ============================================================================
// Test 4: Percentage of Completion (PoC) Pure Integer Math & Cumulative Guard
// ============================================================================
#[tokio::test]
async fn test_poc_billing_pure_integer_rupiah_and_cumulative_guard() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Proyek Konstruksi Pabrik Semen", 100_000_000).await;

    // Stage 1: Bill 30% -> subtotal = (100,000,000 * 30) / 100 = 30,000,000
    let (status, resp1, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 30,
            "tax_type": "PPN_11_EXCL",
            "notes": "Tagihan Progress 30%"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Stage 1 failed: {:?}", resp1);
    assert_eq!(resp1["billed_percentage"], 30);
    assert_eq!(resp1["cumulative_percentage"], 30);
    assert_eq!(resp1["invoice"]["subtotal"], 30_000_000);
    assert_eq!(resp1["invoice"]["tax_amount"], 3_300_000);
    assert_eq!(resp1["invoice"]["total_amount"], 33_300_000);

    // Stage 2: Bill 40% -> cumulative = 70%
    let (status, resp2, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 40,
            "tax_type": "PPN_11_EXCL",
            "notes": "Tagihan Progress 40%"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Stage 2 failed: {:?}", resp2);
    assert_eq!(resp2["billed_percentage"], 40);
    assert_eq!(resp2["cumulative_percentage"], 70);
    assert_eq!(resp2["invoice"]["subtotal"], 40_000_000);
    assert_eq!(resp2["invoice"]["total_amount"], 44_400_000);

    // Stage 3: Bill 30% -> cumulative = 100%
    let (status, resp3, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 30,
            "tax_type": "PPN_11_EXCL",
            "notes": "Tagihan Progress 30% Akhir"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Stage 3 failed: {:?}", resp3);
    assert_eq!(resp3["billed_percentage"], 30);
    assert_eq!(resp3["cumulative_percentage"], 100);
    assert_eq!(resp3["invoice"]["subtotal"], 30_000_000);

    // Stage 4: Attempt to bill 10% more (would reach 110%) -> HTTP 409 Conflict EXCEEDS_CUMULATIVE_PROGRESS
    let (status, err_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 10,
            "tax_type": "PPN_11_EXCL"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Exceeding 100% must return 409 Conflict: {:?}", err_resp);
    assert_eq!(err_resp["code"], "EXCEEDS_CUMULATIVE_PROGRESS");

    // Percentage boundary checks: 0% and 101%
    let (status, err_zero, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"percentage": 0})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_zero["code"], "INVALID_PERCENTAGE");

    let (status, err_overflow, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"percentage": 101})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err_overflow["code"], "INVALID_PERCENTAGE");
}

// ============================================================================
// Test 5: PoC Billing with Verified Progress Record Linkage
// ============================================================================
#[tokio::test]
async fn test_poc_billing_with_verified_progress_record_linkage() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Pembangunan Jembatan Beton", 80_000_000).await;

    // 1. Create physical site progress record (50%)
    let (status, rec_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 50,
            "record_date": "2026-10-06",
            "notes": "Pengecoran pilar jembatan selesai 100%",
            "evidence_url": "https://storage.invinite.id/photos/pilar-1.jpg"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Failed to create progress record: {:?}", rec_resp);
    let record_id = rec_resp["id"].as_str().unwrap().to_string();
    assert_eq!(rec_resp["percentage"], 50);
    assert_eq!(rec_resp["is_billed"], false);

    // 2. List progress records
    let (status, list_resp, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_resp["count"], 1);
    assert_eq!(list_resp["records"][0]["id"], record_id);

    // 3. Bill progress using progress_record_id
    let (status, bill_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "progress_record_id": record_id,
            "tax_type": "PPN_11_EXCL",
            "notes": "Invoicing verified 50% milestone"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Failed to bill progress record: {:?}", bill_resp);
    assert_eq!(bill_resp["billed_percentage"], 50);
    assert_eq!(bill_resp["cumulative_percentage"], 50);
    assert_eq!(bill_resp["invoice"]["subtotal"], 40_000_000); // 50% of 80,000,000
    assert_eq!(bill_resp["progress_record"]["is_billed"], true);
    let invoice_id = bill_resp["invoice_id"].as_str().unwrap();

    // Verify record in database updated
    let db_rec: (i64, Option<String>) = sqlx::query_as(
        "SELECT is_billed, invoice_id FROM progress_records WHERE id = ?1;",
    )
    .bind(&record_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();
    assert_eq!(db_rec.0, 1);
    assert_eq!(db_rec.1.as_deref(), Some(invoice_id));

    // 4. Duplicate billing on same progress_record_id -> 409 Conflict PROGRESS_RECORD_ALREADY_BILLED
    let (status, dup_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "progress_record_id": record_id,
            "tax_type": "PPN_11_EXCL"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(dup_resp["code"], "PROGRESS_RECORD_ALREADY_BILLED");
}

// ============================================================================
// Test 6: Tax Snapshotting Variations (PPN 11%, PPN 12%, UMKM 0.5%, EXEMPT)
// ============================================================================
#[tokio::test]
async fn test_tax_snapshotting_variations_ppn11_ppn12_umkm_exempt() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Kompleks Pergudangan Logistik", 200_000_000).await;

    // Helper to test tax calculation and GL entry balance
    let test_tax_regime = |tax_type: &'static str, expected_tax: i64, expected_total: i64| {
        let h_ref = &h;
        let proj_ref = &project_id;
        async move {
            let m_id = create_milestone(h_ref, proj_ref, &format!("Tahap {}", tax_type), 10_000_000).await;
            let (status, _, _) = send_req(
                &h_ref.app,
                Method::POST,
                &format!("/api/v1/projects/{}/milestones/{}/complete", proj_ref, m_id),
                &h_ref.owner_token,
                &h_ref.tenant_a_id,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::OK);

            let (status, resp, _) = send_req(
                &h_ref.app,
                Method::POST,
                &format!("/api/v1/projects/{}/milestones/{}/bill", proj_ref, m_id),
                &h_ref.owner_token,
                &h_ref.tenant_a_id,
                Some(json!({"tax_type": tax_type})),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "Failed for tax type {}: {:?}", tax_type, resp);

            let inv = &resp["invoice"];
            assert_eq!(inv["subtotal"], 10_000_000);
            assert_eq!(inv["tax_amount"], expected_tax);
            assert_eq!(inv["total_amount"], expected_total);

            let invoice_id = resp["invoice_id"].as_str().unwrap();

            // Verify balanced GL entries
            let lines: Vec<(String, i64, i64)> = sqlx::query_as(
                r#"
                SELECT l.account_code, l.debit, l.credit
                FROM journal_lines l
                JOIN journal_entries e ON l.journal_id = e.id
                WHERE e.tenant_id = ?1 AND e.source_id = ?2;
                "#,
            )
            .bind(&h_ref.tenant_a_id)
            .bind(invoice_id)
            .fetch_all(&h_ref.pool)
            .await
            .unwrap();

            let total_debit: i64 = lines.iter().map(|l| l.1).sum();
            let total_credit: i64 = lines.iter().map(|l| l.2).sum();
            assert_eq!(total_debit, total_credit, "Unbalanced journal for {}", tax_type);
            assert_eq!(total_debit, expected_total);
        }
    };

    // 1. PPN 11% Excl: 10,000,000 * 11% = 1,100,000 -> total 11,100,000
    test_tax_regime("PPN_11_EXCL", 1_100_000, 11_100_000).await;

    // 2. PPN 12% Excl: 10,000,000 * 12% = 1,200,000 -> total 11,200,000
    test_tax_regime("PPN_12_EXCL", 1_200_000, 11_200_000).await;

    // 3. UMKM 0.5% (Final PPh): 10,000,000 * 0.5% = 50,000 -> total 10,050,000
    test_tax_regime("UMKM_05", 50_000, 10_050_000).await;

    // 4. EXEMPT: 0% tax -> total 10,000,000
    test_tax_regime("EXEMPT", 0, 10_000_000).await;
}

// ============================================================================
// Test 7: Multi-Tenant Anti-Enumeration Cross-Tenant Isolation (Strict HTTP 404)
// ============================================================================
#[tokio::test]
async fn test_multi_tenant_anti_enumeration_cross_tenant_isolation() {
    let h = setup_harness().await;

    // Tenant A creates project & milestone & progress record
    let project_a_id = create_active_project(&h, "Tenant A Exclusive Project", 50_000_000).await;
    let milestone_a_id = create_milestone(&h, &project_a_id, "Milestone A", 10_000_000).await;

    let (status, rec_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/progress", project_a_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 20,
            "record_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let record_a_id = rec_resp["id"].as_str().unwrap().to_string();

    // Tenant B attempts to access Tenant A resources: MUST strictly return HTTP 404 NOT_FOUND
    // 1. Cross-tenant complete milestone -> 404
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", project_a_id, milestone_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant complete milestone must return 404");

    // 2. Cross-tenant bill milestone -> 404
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", project_a_id, milestone_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant bill milestone must return 404");

    // 3. Cross-tenant bill progress -> 404
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({"percentage": 20})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant bill progress must return 404");

    // 4. Cross-tenant bill progress with progress_record_id -> 404
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({"progress_record_id": record_a_id})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant bill progress with record must return 404");

    // 5. Cross-tenant list progress -> 404
    let (status, _, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/progress", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant list progress must return 404");

    // 6. Cross-tenant create progress -> 404
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/progress", project_a_id),
        &h.tenant_b_token,
        &h.tenant_b_id,
        Some(json!({
            "percentage": 30,
            "record_date": "2026-10-06"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Cross-tenant create progress must return 404");
}

// ============================================================================
// Test 8: RBAC Matrix (Staff Forbidden vs Manager/Owner Allowed)
// ============================================================================
#[tokio::test]
async fn test_rbac_matrix_staff_forbidden_vs_manager_owner_allowed() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Gedung Sekolah Swasta", 60_000_000).await;
    let milestone_id = create_milestone(&h, &project_id, "Pondasi Dasar", 15_000_000).await;

    // Staff attempts to complete milestone -> HTTP 403 FORBIDDEN
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", project_id, milestone_id),
        &h.staff_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff cannot complete milestone");

    // Staff attempts to bill milestone -> HTTP 403 FORBIDDEN
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", project_id, milestone_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff cannot bill milestone");

    // Staff attempts to bill progress -> HTTP 403 FORBIDDEN
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({"percentage": 25})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Staff cannot bill progress");

    // Staff CAN record physical progress -> HTTP 201 CREATED
    let (status, staff_rec, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/progress", project_id),
        &h.staff_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 25,
            "record_date": "2026-10-06",
            "notes": "Pengecekan lapangan oleh staf lapangan"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Staff should be able to log physical progress");
    assert_eq!(staff_rec["percentage"], 25);

    // Manager role CAN complete milestone -> HTTP 200 OK
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", project_id, milestone_id),
        &h.manager_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Manager should be allowed to complete milestone");

    // Manager role CAN bill milestone -> HTTP 200 OK (with automated internal GL elevation)
    let (status, bill_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", project_id, milestone_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Manager should be allowed to bill milestone: {:?}", bill_resp);

    // Manager role CAN bill progress -> HTTP 200 OK
    let (status, poc_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.manager_token,
        &h.tenant_a_id,
        Some(json!({"percentage": 25, "tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Manager should be allowed to bill progress: {:?}", poc_resp);
}

// ============================================================================
// Test 9: Real-Time Profitability Engine Reflects Billed Revenue
// ============================================================================
#[tokio::test]
async fn test_real_time_profitability_reflects_billed_revenue() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Gedung Pusat Riset Teknologi", 100_000_000).await;
    let milestone_id = create_milestone(&h, &project_id, "Tahap Struktur", 25_000_000).await;

    // 1. Initial profitability: billed revenue = 0
    let (status, init_prof, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(init_prof["total_billed_revenue"], 0);

    // 2. Complete and bill milestone (25,000,000 + 11% PPN = 27,750,000 total invoice)
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, bill_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/bill", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let m_invoice_total = bill_resp["invoice"]["total_amount"].as_i64().unwrap();
    assert_eq!(m_invoice_total, 27_750_000);

    // Profitability reflects milestone invoice total
    let (status, prof_after_m, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prof_after_m["total_billed_revenue"], 27_750_000);

    // 3. Bill 20% PoC: subtotal 20,000,000 + 11% PPN = 22,200,000 total invoice
    let (status, poc_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({"percentage": 20, "tax_type": "PPN_11_EXCL"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let poc_invoice_total = poc_resp["invoice"]["total_amount"].as_i64().unwrap();
    assert_eq!(poc_invoice_total, 22_200_000);

    // Profitability reflects combined revenue: 27,750,000 + 22_200_000 = 49,950_000
    let (status, final_prof, _) = send_req(
        &h.app,
        Method::GET,
        &format!("/api/v1/projects/{}/profitability", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(final_prof["total_billed_revenue"], 49_950_000);
}

// ============================================================================
// Test 10: Transactional Atomicity and Outbox Event Emission
// ============================================================================
#[tokio::test]
async fn test_transactional_atomicity_and_outbox_event_emission() {
    let h = setup_harness().await;
    let project_id = create_active_project(&h, "Gedung Bioskop Modern", 90_000_000).await;
    let milestone_id = create_milestone(&h, &project_id, "Tahap Finishing Akustik", 30_000_000).await;

    // 1. Complete milestone and check outbox event
    let (status, _, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/milestones/{}/complete", project_id, milestone_id),
        &h.owner_token,
        &h.tenant_a_id,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let m_event: (String, String, String, String) = sqlx::query_as(
        r#"
        SELECT event_type, aggregate_type, aggregate_id, payload_json
        FROM outbox_events
        WHERE tenant_id = ?1 AND event_type = 'MilestoneCompleted' AND aggregate_id = ?2;
        "#,
    )
    .bind(&h.tenant_a_id)
    .bind(&milestone_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(m_event.0, "MilestoneCompleted");
    assert_eq!(m_event.1, "Milestone");
    assert_eq!(m_event.2, milestone_id);
    let m_payload: Value = serde_json::from_str(&m_event.3).unwrap();
    assert_eq!(m_payload["milestone_id"], milestone_id);
    assert_eq!(m_payload["project_id"], project_id);
    assert_eq!(m_payload["title"], "Tahap Finishing Akustik");
    assert!(!m_payload["completed_at"].is_null());

    // 2. Bill progress and check outbox event
    let (status, poc_resp, _) = send_req(
        &h.app,
        Method::POST,
        &format!("/api/v1/projects/{}/billing/progress", project_id),
        &h.owner_token,
        &h.tenant_a_id,
        Some(json!({
            "percentage": 15,
            "tax_type": "PPN_11_EXCL"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let invoice_id = poc_resp["invoice_id"].as_str().unwrap();

    let p_event: (String, String, String, String) = sqlx::query_as(
        r#"
        SELECT event_type, aggregate_type, aggregate_id, payload_json
        FROM outbox_events
        WHERE tenant_id = ?1 AND event_type = 'ProgressBilled' AND aggregate_id = ?2;
        "#,
    )
    .bind(&h.tenant_a_id)
    .bind(&project_id)
    .fetch_one(&h.pool)
    .await
    .unwrap();

    assert_eq!(p_event.0, "ProgressBilled");
    assert_eq!(p_event.1, "Project");
    assert_eq!(p_event.2, project_id);
    let p_payload: Value = serde_json::from_str(&p_event.3).unwrap();
    assert_eq!(p_payload["billing_type"], "PERCENTAGE_OF_COMPLETION");
    assert_eq!(p_payload["project_id"], project_id);
    assert_eq!(p_payload["invoice_id"], invoice_id);
    assert_eq!(p_payload["billed_percentage"], 15);
    assert_eq!(p_payload["cumulative_percentage"], 15);
    assert_eq!(p_payload["amount"], 13_500_000); // 15% of 90,000,000
    assert_eq!(p_payload["total_amount"], 14_985_000); // 13,500,000 + 11% PPN
}
