//! Integration Test Suite for Milestone 4: Transactional Outbox Pattern & Sidecar Boundary
//! Features 22-25: Transactional Outbox Persistence, At-Least-Once Dispatcher, Deduplication, and Failure Isolation.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::outbox::{now_utc_iso_z, OutboxEventDraft, OutboxStatus};
use backend::domain::tenant::Role;
use backend::repository::{
    account_repo::SqlxAccountRepository,
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
    outbox_processor::{DefaultEventDispatcher, OutboxProcessor, OutboxProcessorConfig},
    payment_service::{PaymentConfig, PaymentService},
};
use chrono::Duration;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    tenant_a_id: String,
}

async fn setup_harness() -> TestHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("outbox_test.sqlite");
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

    let jwt_secret = "m4_outbox_jwt_secret_key_1234567890_super_secret";
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

    let owner_id = Uuid::new_v4().to_string();
    user_repo
        .create(&NewUser {
            id: owner_id.clone(),
            email: "owner_m4@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Owner M4".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "owner_m4@business.com", "user", "premium")
        .unwrap();

    let tenant_a_id = Uuid::new_v4().to_string();
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant M4 Corp".to_string(),
            slug: "tenant-m4-corp".to_string(),
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

    TestHarness {
        app,
        pool,
        _dir: dir,
        owner_token,
        tenant_a_id,
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
// 1. test_01_atomic_outbox_emission_invoice_issue
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_01_atomic_outbox_emission_invoice_issue() {
    let harness = setup_harness().await;

    // 1. Create invoice draft
    let invoice_payload = json!({
        "customer_name": "PT Outbox Alpha",
        "customer_email": "alpha@outbox.com",
        "due_date": "2026-12-31T00:00:00Z",
        "currency": "IDR",
        "tax_type": "EXEMPT",
        "items": [
            {
                "description": "Enterprise Subscription",
                "quantity": 1,
                "unit_price": 5_000_000,
                "discount": 0
            }
        ]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(invoice_payload.to_string()))
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED);
    let invoice_id = resp_json["id"].as_str().unwrap().to_string();

    // 2. Issue invoice
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    let inv_number = resp_json["invoice_number"].as_str().unwrap().to_string();

    // 3. Query Outbox Events via API
    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/outbox/events")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    let events = resp_json["events"].as_array().expect("Expected events array");
    assert!(!events.is_empty(), "Expected at least one outbox event");

    let invoice_event = events
        .iter()
        .find(|e| e["aggregate_id"].as_str() == Some(&invoice_id))
        .expect("Expected InvoiceIssued event for issued invoice");

    assert_eq!(invoice_event["event_type"], "InvoiceIssued");
    assert_eq!(invoice_event["aggregate_type"], "Invoice");
    assert_eq!(invoice_event["status"], "PENDING");
    assert_eq!(invoice_event["attempt_count"], 0);
    assert_eq!(invoice_event["retry_count"], 0);
    assert!(invoice_event["created_at"].as_str().unwrap().ends_with('Z'));

    // Verify structured payload contents
    let payload = &invoice_event["payload"];
    assert_eq!(payload["invoice_id"], invoice_id);
    assert_eq!(payload["invoice_number"], inv_number);
    assert_eq!(payload["total_amount"], 5_000_000);
    assert_eq!(payload["customer_name"], "PT Outbox Alpha");
}

// ---------------------------------------------------------------------------
// 2. test_02_rollback_isolation_zero_orphaned_events
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_02_rollback_isolation_zero_orphaned_events() {
    let harness = setup_harness().await;

    // Demonstrate atomic transaction rollback guarantee:
    // If a domain operation fails and rolls back, exactly ZERO outbox events remain orphaned in DB.
    let target_aggregate_id = Uuid::new_v4().to_string();
    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();

    let mut tx = harness.pool.begin().await.unwrap();

    // Simulate inserting draft within transaction
    let draft = OutboxEventDraft::invoice_issued(
        tenant_uuid,
        &target_aggregate_id,
        "INV-ROLLBACK-001",
        10_000_000,
        "Rollback Customer",
    );
    SqlxOutboxRepository::insert_tx_static(&mut tx, &draft)
        .await
        .unwrap();

    // Intentionally rollback transaction
    tx.rollback().await.unwrap();

    // Verify outbox_events table has strictly 0 rows for target_aggregate_id
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1",
    )
    .bind(&target_aggregate_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(count, 0, "Expected strictly 0 outbox events after transaction rollback");
}

// ---------------------------------------------------------------------------
// 3. test_03_atomic_outbox_emission_payment_allocation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_03_atomic_outbox_emission_payment_allocation() {
    let harness = setup_harness().await;

    // 1. Create and issue invoice
    let invoice_payload = json!({
        "customer_name": "PT Payment Beta",
        "customer_email": "beta@payment.com",
        "due_date": "2026-12-31T00:00:00Z",
        "currency": "IDR",
        "tax_type": "EXEMPT",
        "items": [
            {
                "description": "Cloud Hosting Services",
                "quantity": 1,
                "unit_price": 3_000_000,
                "discount": 0
            }
        ]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(invoice_payload.to_string()))
        .unwrap();

    let (_, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    let invoice_id = resp_json["id"].as_str().unwrap().to_string();

    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, _, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);

    // 2. Allocate payment
    let payment_payload = json!({
        "invoice_id": invoice_id,
        "amount": 3_000_000,
        "payment_method": "BANK_TRANSFER",
        "reference": "REF-PAY-TEST-001"
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payment_payload.to_string()))
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED, "Payment allocation failed: {:?}", resp_json);
    let payment_id = resp_json["id"].as_str().unwrap().to_string();

    // 3. Query Outbox Events filtering by aggregate_type=Payment
    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/outbox/events?aggregate_type=Payment")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);

    let events = resp_json["events"].as_array().expect("Expected events array");
    assert!(!events.is_empty(), "Expected PaymentConfirmed event");

    let payment_event = events
        .iter()
        .find(|e| e["aggregate_id"].as_str() == Some(&payment_id))
        .expect("Expected event for payment");

    assert_eq!(payment_event["event_type"], "PaymentConfirmed");
    assert_eq!(payment_event["aggregate_type"], "Payment");
    assert_eq!(payment_event["status"], "PENDING");
    assert_eq!(payment_event["attempt_count"], 0);

    let payload = &payment_event["payload"];
    assert_eq!(payload["payment_id"], payment_id);
    assert_eq!(payload["invoice_id"], invoice_id);
    assert_eq!(payload["amount"], 3_000_000);
    assert_eq!(payload["reference"], "REF-PAY-TEST-001");
}

// ---------------------------------------------------------------------------
// 4. test_04_polling_and_successful_delivery
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_04_polling_and_successful_delivery() {
    let harness = setup_harness().await;

    // Seed 2 pending events
    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let draft1 = OutboxEventDraft::invoice_issued(
        tenant_uuid,
        &Uuid::new_v4().to_string(),
        "INV-TEST-001",
        1_000_000,
        "Customer 1",
    );
    let draft2 = OutboxEventDraft::invoice_voided(
        tenant_uuid,
        &Uuid::new_v4().to_string(),
        "INV-TEST-002",
        "Voided by test",
    );

    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    repo.insert(&draft1).await.unwrap();
    repo.insert(&draft2).await.unwrap();

    // Trigger process batch
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/outbox/process")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"simulate_sidecar_failure": false}).to_string()))
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp_json["status"], "completed");
    assert_eq!(resp_json["processed"], 2);
    assert_eq!(resp_json["failed"], 0);

    // Verify events transitioned to PUBLISHED with published_at ending in 'Z'
    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/outbox/events")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (_, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    let events = resp_json["events"].as_array().unwrap();
    for e in events {
        assert_eq!(e["status"], "PUBLISHED");
        assert_eq!(e["attempt_count"], 1);
        assert!(e["published_at"].as_str().unwrap().ends_with('Z'));
    }

    // Subsequent process run on empty queue returns 0 processed
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/outbox/process")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp_json["processed"], 0);
    assert_eq!(resp_json["status"], "completed");
}

// ---------------------------------------------------------------------------
// 5. test_05_exponential_backoff_retry_calculation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_05_exponential_backoff_retry_calculation() {
    let harness = setup_harness().await;

    // Verify backoff algorithm formula: base_secs * 2^(retry_count - 1)
    assert_eq!(OutboxStatus::calculate_backoff(1, 2, 60), Duration::seconds(2));
    assert_eq!(OutboxStatus::calculate_backoff(2, 2, 60), Duration::seconds(4));
    assert_eq!(OutboxStatus::calculate_backoff(3, 2, 60), Duration::seconds(8));
    assert_eq!(OutboxStatus::calculate_backoff(4, 2, 60), Duration::seconds(16));
    assert_eq!(OutboxStatus::calculate_backoff(5, 2, 60), Duration::seconds(32));

    // Seed 1 pending event
    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let draft = OutboxEventDraft::invoice_issued(
        tenant_uuid,
        &Uuid::new_v4().to_string(),
        "INV-RETRY-001",
        2_500_000,
        "Retry Customer",
    );
    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    let ev = repo.insert(&draft).await.unwrap();

    // Process with simulated failure
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/outbox/process")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"simulate_sidecar_failure": true}).to_string()))
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp_json["status"], "retry_scheduled");
    assert_eq!(resp_json["failed"], 1);

    // Verify event in database is FAILED, attempt_count is 1, last_error captures simulated failure
    let updated_ev = repo.get_by_id(&backend::domain::tenant::TenantContext {
        tenant_id: tenant_uuid,
        actor_id: Uuid::new_v4(),
        role: Role::Owner,
    }, &ev.id).await.unwrap().unwrap();

    assert_eq!(updated_ev.status, "FAILED");
    assert_eq!(updated_ev.attempt_count, 1);
    assert_eq!(updated_ev.retry_count, 1);
    assert!(updated_ev.last_error.as_deref().unwrap().contains("SIMULATED_SIDECAR_ERROR"));
    assert!(updated_ev.next_retry_at.ends_with('Z'));
}

// ---------------------------------------------------------------------------
// 6. test_06_dead_letter_queue_dlq_threshold
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_06_dead_letter_queue_dlq_threshold() {
    let harness = setup_harness().await;

    // Seed 1 pending event with max_retries = 5
    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let draft = OutboxEventDraft::invoice_issued(
        tenant_uuid,
        &Uuid::new_v4().to_string(),
        "INV-DLQ-001",
        7_000_000,
        "DLQ Customer",
    );
    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    let ev = repo.insert(&draft).await.unwrap();

    let processor = OutboxProcessor::new(
        harness.pool.clone(),
        Arc::new(DefaultEventDispatcher),
        OutboxProcessorConfig {
            poll_interval: std::time::Duration::from_millis(10),
            batch_size: 10,
            max_retries: 5,
            base_backoff_secs: 0, // 0 backoff so immediately claimable for next loop
        },
    );

    // Execute 4 failed attempts
    for attempt in 1..=4 {
        let res = processor.process_tenant_batch(&harness.tenant_a_id, true).await.unwrap();
        assert_eq!(res.failed, 1, "Attempt {} should fail", attempt);
        assert_eq!(res.dead_lettered, 0, "Attempt {} should not be DLQ yet", attempt);

        // Reset next_retry_at to past so it's immediately picked up in test
        sqlx::query("UPDATE outbox_events SET next_retry_at = '2000-01-01T00:00:00.000000Z' WHERE id = ?1")
            .bind(&ev.id)
            .execute(&harness.pool)
            .await
            .unwrap();
    }

    // 5th attempt: Reaches max_retries (5) -> Transitions to DEAD_LETTER
    let res = processor.process_tenant_batch(&harness.tenant_a_id, true).await.unwrap();
    assert_eq!(res.dead_lettered, 1, "5th attempt must transition to DEAD_LETTER");

    let final_ev = repo.get_by_id(&backend::domain::tenant::TenantContext {
        tenant_id: tenant_uuid,
        actor_id: Uuid::new_v4(),
        role: Role::Owner,
    }, &ev.id).await.unwrap().unwrap();

    assert_eq!(final_ev.status, "DEAD_LETTER");
    assert_eq!(final_ev.attempt_count, 5);

    // Subsequent process batch ignores DEAD_LETTER events
    let res = processor.process_tenant_batch(&harness.tenant_a_id, false).await.unwrap();
    assert_eq!(res.processed, 0, "DEAD_LETTER events must not be processed");
}

// ---------------------------------------------------------------------------
// 7. test_07_sidecar_failure_isolation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_07_sidecar_failure_isolation() {
    let harness = setup_harness().await;

    // 1. Issue an invoice
    let invoice_payload = json!({
        "customer_name": "PT Resilience Corp",
        "customer_email": "resilience@corp.com",
        "due_date": "2026-12-31T00:00:00Z",
        "currency": "IDR",
        "tax_type": "EXEMPT",
        "items": [
            {
                "description": "Critical Core Operation",
                "quantity": 1,
                "unit_price": 10_000_000,
                "discount": 0
            }
        ]
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/invoices")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(invoice_payload.to_string()))
        .unwrap();

    let (_, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    let invoice_id = resp_json["id"].as_str().unwrap().to_string();

    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/invoices/{}/issue", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, _, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);

    // 2. Simulate catastrophic external sidecar outage (e.g. WhatsApp / Email / PDF engine down)
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/outbox/process")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"simulate_sidecar_failure": true}).to_string()))
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp_json["status"], "retry_scheduled");
    assert_eq!(resp_json["message"], "Sidecar failure isolated; core transaction unaffected");

    // 3. Verify CORE INVOICE is 100% UNTOUCHED and strictly ISSUED
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/invoices/{}", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp_json["status"], "ISSUED");
    assert_eq!(resp_json["total_amount"], 10_000_000);

    // 4. Verify SQLite database health and WAL pragma remain completely unaffected
    let pragma_mode: String = sqlx::query_scalar("PRAGMA journal_mode;").fetch_one(&harness.pool).await.unwrap();
    assert_eq!(pragma_mode.to_lowercase(), "wal");
}

// ---------------------------------------------------------------------------
// 8. test_08_deduplication_and_idempotent_consumer
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_08_deduplication_and_idempotent_consumer() {
    let harness = setup_harness().await;

    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let draft = OutboxEventDraft::invoice_issued(
        tenant_uuid,
        &Uuid::new_v4().to_string(),
        "INV-IDEMP-001",
        4_000_000,
        "Idempotency Customer",
    );
    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    let ev = repo.insert(&draft).await.unwrap();

    // Verify valid UUID v4
    let parsed_uuid = Uuid::parse_str(&ev.id).expect("Event ID must be valid UUID v4");
    assert_eq!(parsed_uuid.get_version_num(), 4);

    // Consumer idempotency simulation
    let mut processed_event_ids: HashSet<String> = HashSet::new();

    // First arrival: successfully processed
    let is_first_processed = processed_event_ids.insert(ev.id.clone());
    assert!(is_first_processed, "First delivery must be processed");

    // Redelivery / duplicate delivery: consumer detects duplicate event ID and skips
    let is_second_processed = processed_event_ids.insert(ev.id.clone());
    assert!(!is_second_processed, "Duplicate delivery must be deduplicated by consumer");

    // Primary key constraint collision test: inserting duplicate ID directly into table fails
    let now_iso = now_utc_iso_z();
    let collision_err = sqlx::query(
        "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status, created_at, next_retry_at) VALUES (?1, ?2, 'Test', 'Test', '1', '{}', 'PENDING', ?3, ?3)"
    )
    .bind(&ev.id)
    .bind(&harness.tenant_a_id)
    .bind(&now_iso)
    .execute(&harness.pool)
    .await;

    assert!(collision_err.is_err(), "Duplicate event ID primary key insertion must fail");
}

// ---------------------------------------------------------------------------
// 9. test_09_concurrency_racing_double_publish_prevention
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_09_concurrency_racing_double_publish_prevention() {
    let harness = setup_harness().await;

    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let repo = SqlxOutboxRepository::new(harness.pool.clone());

    // Seed 20 pending events
    for i in 1..=20 {
        let draft = OutboxEventDraft::invoice_issued(
            tenant_uuid,
            &Uuid::new_v4().to_string(),
            &format!("INV-RACE-{:03}", i),
            1_000_000 * i,
            &format!("Customer {}", i),
        );
        repo.insert(&draft).await.unwrap();
    }

    let processor1 = Arc::new(OutboxProcessor::with_default_config(harness.pool.clone()));
    let processor2 = Arc::new(OutboxProcessor::with_default_config(harness.pool.clone()));

    let tenant_id_1 = harness.tenant_a_id.clone();
    let tenant_id_2 = harness.tenant_a_id.clone();

    // Spawn 2 racing concurrent workers
    let task1 = tokio::spawn(async move {
        processor1.process_tenant_batch(&tenant_id_1, false).await
    });

    let task2 = tokio::spawn(async move {
        processor2.process_tenant_batch(&tenant_id_2, false).await
    });

    let (res1, res2) = tokio::join!(task1, task2);
    let r1 = res1.unwrap().unwrap();
    let r2 = res2.unwrap().unwrap();

    let total_processed = r1.processed + r2.processed;
    assert_eq!(total_processed, 20, "Exact total of 20 events must be processed");
    assert_eq!(r1.failed + r2.failed, 0, "No events should fail");

    // Verify all 20 events are PUBLISHED with exactly attempt_count == 1
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, attempt_count FROM outbox_events WHERE tenant_id = ?1"
    )
    .bind(&harness.tenant_a_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 20);
    for (status, attempts) in rows {
        assert_eq!(status, "PUBLISHED");
        assert_eq!(attempts, 1, "Each event must be claimed and published exactly once");
    }
}
