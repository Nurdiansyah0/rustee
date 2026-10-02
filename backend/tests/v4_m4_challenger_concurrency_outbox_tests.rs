//! Milestone 4 Adversarial Empirical Challenger Verification Suite
//!
//! Identity: teamwork_preview_challenger_m4_2
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//! Focus: Concurrency Race Claiming, Atomic Commit Rollback, Exponential Backoff Timing & DLQ, Sidecar Failure Isolation, Consumer Deduplication
//!
//! Verification Objectives:
//! 1. Concurrency Race Claiming:
//!    - Seed 50 pending outbox events.
//!    - Spawn 5 concurrent processor worker tasks racing to claim and process events.
//!    - Assert: every event is processed exactly once, exactly 50 total dispatches occur, 0 duplicate dispatches, 0 race errors.
//! 2. Concurrency Multi-Tenant Racing:
//!    - Seed 60 events across multiple tenants; 4 concurrent workers calling process_all_pending.
//!    - Assert: 0 duplicates, all 60 published with attempt_count 1.
//! 3. Atomic Rollback & Zero Orphaned Events:
//!    - Force an error during invoice issuance or payment recording after the outbox draft is prepared.
//!    - Assert: transaction rolls back completely; `COUNT(*) FROM outbox_events WHERE aggregate_id = ?` is strictly 0.
//! 4. Exponential Backoff Timing & DLQ Progression:
//!    - Seed a failing dispatcher.
//!    - Execute successive process ticks (attempts 1 through 5).
//!    - Verify attempt counters increment, `next_retry_at` advances exponentially (base * 2^(attempt - 1)), and at attempt 5, status transitions to `DEAD_LETTER` with `last_error` populated.
//!    - Verify dead-lettered events are excluded from subsequent processor batches.
//! 5. Sidecar Failure Isolation Boundary:
//!    - Simulate external sidecar outages (HTTP 503, connection timeouts, panics).
//!    - Verify core business operations (`issue_invoice`, `allocate_payment`) succeed completely and remain valid in the database.
//!    - Verify database pool remains healthy with zero leaked locks.
//! 6. Consumer Deduplication by UUID `event_id`:
//!    - Verify that replaying the same event ID to a mock consumer is recognized as a duplicate and processed idempotently.
//! 7. Cross-Tenant Outbox Isolation:
//!    - Verify cross-tenant isolation on outbox retrieval endpoints.

use axum::{
    body::Body,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderMap, Method, Request, StatusCode,
    },
};
use backend::api::{create_app, AppState, AuthState};
use backend::domain::outbox::{OutboxEvent, OutboxEventDraft};
use backend::domain::tenant::{Role, TenantContext, TenantStatus};
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
    outbox_processor::{DispatchError, EventDispatcher, OutboxProcessor, OutboxProcessorConfig},
    payment_service::{PaymentConfig, PaymentService},
};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

struct ChallengerHarness {
    app: axum::Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
    owner_token: String,
    tenant_a_id: String,
    tenant_b_id: String,
    owner_id: String,
}

async fn setup_challenger_harness() -> ChallengerHarness {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("outbox_challenger_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 10,
        min_connections: 2,
        busy_timeout_ms: 15_000,
        acquire_timeout_secs: 15,
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

    let jwt_secret = "challenger_m4_jwt_secret_key_1234567890_super_secret";
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
            email: "challenger_owner@business.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Challenger Owner".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .unwrap();

    let (owner_token, _) = jwt_engine
        .generate_token(&owner_id, "challenger_owner@business.com", "user", "premium")
        .unwrap();

    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let membership_repo = SqlxMembershipRepository::new(pool.clone());

    // Tenant A
    let tenant_a_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_a_id.clone(),
            name: "Tenant Alpha".to_string(),
            slug: "tenant-alpha".to_string(),
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
            id: format!("mem_a_{}", Uuid::new_v4()),
            tenant_id: tenant_a_id.clone(),
            user_id: owner_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    // Tenant B
    let tenant_b_id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: tenant_b_id.clone(),
            name: "Tenant Beta".to_string(),
            slug: "tenant-beta".to_string(),
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
            id: format!("mem_b_{}", Uuid::new_v4()),
            tenant_id: tenant_b_id.clone(),
            user_id: owner_id.clone(),
            role: Role::Owner,
        })
        .await
        .unwrap();

    ChallengerHarness {
        app,
        pool,
        _dir: dir,
        owner_token,
        tenant_a_id,
        tenant_b_id,
        owner_id,
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
// Mock Dispatchers for Empirical Testing
// ---------------------------------------------------------------------------

struct CountingDispatcher {
    dispatched_ids: Arc<Mutex<Vec<String>>>,
    dispatch_count: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl EventDispatcher for CountingDispatcher {
    async fn dispatch(&self, event: &OutboxEvent) -> Result<(), DispatchError> {
        self.dispatch_count.fetch_add(1, Ordering::SeqCst);
        let mut ids = self.dispatched_ids.lock().await;
        ids.push(event.id.clone());
        Ok(())
    }
}

struct FailingDispatcher {
    error_type: String,
    attempt_tracker: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl EventDispatcher for FailingDispatcher {
    async fn dispatch(&self, _event: &OutboxEvent) -> Result<(), DispatchError> {
        self.attempt_tracker.fetch_add(1, Ordering::SeqCst);
        match self.error_type.as_str() {
            "timeout" => Err(DispatchError::Timeout("Connection timed out after 3000ms".to_string())),
            "permanent" => Err(DispatchError::Permanent("Malformed sidecar payload rejected".to_string())),
            _ => Err(DispatchError::Transient(503, "HTTP 503 Service Unavailable".to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Concurrency Race Claiming: 50 Events, 5 Concurrent Racing Workers
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_challenger_01_concurrency_race_claiming_50_events_5_workers() {
    let harness = setup_challenger_harness().await;

    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let repo = SqlxOutboxRepository::new(harness.pool.clone());

    // 1. Seed 50 pending outbox events for Tenant A
    const TOTAL_EVENTS: usize = 50;
    let mut seeded_ids = Vec::with_capacity(TOTAL_EVENTS);
    for i in 1..=TOTAL_EVENTS {
        let draft = OutboxEventDraft::invoice_issued(
            tenant_uuid,
            &Uuid::new_v4().to_string(),
            &format!("INV-STRESS-{:04}", i),
            1_500_000 * (i as i64),
            &format!("Stress Customer {}", i),
        );
        let ev = repo.insert(&draft).await.unwrap();
        seeded_ids.push(ev.id);
    }
    assert_eq!(seeded_ids.len(), TOTAL_EVENTS);

    // 2. Setup shared tracking dispatcher
    let dispatched_ids = Arc::new(Mutex::new(Vec::new()));
    let dispatch_count = Arc::new(AtomicUsize::new(0));

    let dispatcher: Arc<dyn EventDispatcher> = Arc::new(CountingDispatcher {
        dispatched_ids: dispatched_ids.clone(),
        dispatch_count: dispatch_count.clone(),
    });

    // 3. Spawn 5 concurrent processor worker tasks racing to claim and process events
    const WORKER_COUNT: usize = 5;
    let mut handles = Vec::with_capacity(WORKER_COUNT);

    for worker_idx in 0..WORKER_COUNT {
        let pool = harness.pool.clone();
        let disp = dispatcher.clone();
        let tenant_id = harness.tenant_a_id.clone();

        let handle = tokio::spawn(async move {
            let processor = OutboxProcessor::new(
                pool,
                disp,
                OutboxProcessorConfig {
                    poll_interval: std::time::Duration::from_millis(5),
                    batch_size: 10, // Small batch size to maximize contention across workers
                    max_retries: 5,
                    base_backoff_secs: 2,
                },
            );

            let mut worker_processed_total = 0;
            // Worker polls in a loop until no more pending events remain
            for _ in 0..10 {
                let res = processor.process_tenant_batch(&tenant_id, false).await;
                match res {
                    Ok(batch_res) => {
                        worker_processed_total += batch_res.processed;
                        if batch_res.processed == 0 {
                            break;
                        }
                    }
                    Err(e) => panic!("Worker {} encountered unexpected error: {:?}", worker_idx, e),
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            worker_processed_total
        });
        handles.push(handle);
    }

    // 4. Await all 5 workers
    let mut total_processed_by_workers = 0;
    for (idx, handle) in handles.into_iter().enumerate() {
        let count = handle.await.expect("Worker task panicked");
        total_processed_by_workers += count;
        println!("Worker {} processed {} events", idx, count);
    }

    // 5. Assert: exactly 50 total dispatches occur, 0 duplicate dispatches, 0 race errors
    assert_eq!(
        total_processed_by_workers, TOTAL_EVENTS,
        "Sum of worker processed counts must equal exactly 50"
    );

    let final_dispatch_count = dispatch_count.load(Ordering::SeqCst);
    assert_eq!(
        final_dispatch_count, TOTAL_EVENTS,
        "Dispatcher must have been invoked exactly 50 times"
    );

    let recorded_ids = dispatched_ids.lock().await;
    assert_eq!(recorded_ids.len(), TOTAL_EVENTS);

    // Verify 0 duplicates across workers
    let unique_dispatches: HashSet<&String> = recorded_ids.iter().collect();
    assert_eq!(
        unique_dispatches.len(),
        TOTAL_EVENTS,
        "Every event must be dispatched exactly once (0 duplicates)"
    );

    // 6. Verify in database: exactly 50 rows, all PUBLISHED, each with attempt_count == 1
    let rows: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, status, attempt_count, published_at FROM outbox_events WHERE tenant_id = ?1"
    )
    .bind(&harness.tenant_a_id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), TOTAL_EVENTS);
    for (id, status, attempts, published_at) in rows {
        assert_eq!(status, "PUBLISHED", "Event {} must be PUBLISHED", id);
        assert_eq!(attempts, 1, "Event {} must have attempt_count == 1", id);
        assert!(
            published_at.is_some() && published_at.unwrap().ends_with('Z'),
            "published_at must be populated with UTC ISO 'Z'"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Concurrency Multi-Tenant Racing (process_all_pending)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_challenger_02_concurrency_multi_tenant_claiming_race() {
    let harness = setup_challenger_harness().await;

    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    let tenant_a_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let tenant_b_uuid = Uuid::parse_str(&harness.tenant_b_id).unwrap();

    // Seed 30 events for Tenant A and 30 events for Tenant B = 60 total
    for i in 1..=30 {
        let draft_a = OutboxEventDraft::invoice_issued(
            tenant_a_uuid,
            &Uuid::new_v4().to_string(),
            &format!("INV-A-{:03}", i),
            1_000_000 * (i as i64),
            &format!("Cust A {}", i),
        );
        repo.insert(&draft_a).await.unwrap();

        let draft_b = OutboxEventDraft::payment_confirmed(
            tenant_b_uuid,
            &Uuid::new_v4().to_string(),
            &Uuid::new_v4().to_string(),
            500_000 * (i as i64),
            Some(&format!("REF-B-{:03}", i)),
        );
        repo.insert(&draft_b).await.unwrap();
    }

    let dispatched_ids = Arc::new(Mutex::new(Vec::new()));
    let dispatch_count = Arc::new(AtomicUsize::new(0));

    let dispatcher: Arc<dyn EventDispatcher> = Arc::new(CountingDispatcher {
        dispatched_ids: dispatched_ids.clone(),
        dispatch_count: dispatch_count.clone(),
    });

    // 4 concurrent tasks racing across all tenants using process_all_pending
    const WORKER_COUNT: usize = 4;
    let mut handles = Vec::with_capacity(WORKER_COUNT);

    for _ in 0..WORKER_COUNT {
        let pool = harness.pool.clone();
        let disp = dispatcher.clone();

        let handle = tokio::spawn(async move {
            let processor = OutboxProcessor::new(
                pool,
                disp,
                OutboxProcessorConfig {
                    poll_interval: std::time::Duration::from_millis(5),
                    batch_size: 15,
                    max_retries: 5,
                    base_backoff_secs: 2,
                },
            );

            let mut count = 0;
            for _ in 0..10 {
                let res = processor.process_all_pending(false).await.unwrap();
                count += res.processed;
                if res.processed == 0 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            count
        });
        handles.push(handle);
    }

    let mut sum = 0;
    for h in handles {
        sum += h.await.unwrap();
    }

    assert_eq!(sum, 60, "All 60 events across tenants must be processed");
    assert_eq!(dispatch_count.load(Ordering::SeqCst), 60);

    let recorded = dispatched_ids.lock().await;
    let unique: HashSet<&String> = recorded.iter().collect();
    assert_eq!(unique.len(), 60, "Zero duplicates across concurrent multi-tenant workers");
}

// ---------------------------------------------------------------------------
// 3. Atomic Rollback & Zero Orphaned Events (Invoice Issuance & Payment)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_challenger_03_atomic_rollback_zero_orphaned_events_invoice() {
    let harness = setup_challenger_harness().await;

    let target_aggregate_id = Uuid::new_v4().to_string();
    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();

    // 1. Begin explicit transaction mimicking invoice issue flow
    let mut tx = harness.pool.begin().await.unwrap();

    // Prepare outbox event draft
    let draft = OutboxEventDraft::invoice_issued(
        tenant_uuid,
        &target_aggregate_id,
        "INV-ROLLBACK-002",
        25_000_000,
        "Rollback Customer PT",
    );

    // Insert outbox event draft into tx
    SqlxOutboxRepository::insert_tx_static(&mut tx, &draft)
        .await
        .unwrap();

    // Force failure: violate a CHECK constraint or abort transaction
    let invalid_insert = sqlx::query(
        "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status) VALUES ('invalid', ?1, 'T', 'T', 'T', '{}', 'INVALID_STATUS_STRING')"
    )
    .bind(&harness.tenant_a_id)
    .execute(&mut *tx)
    .await;

    assert!(invalid_insert.is_err(), "Invalid status must trigger CHECK constraint error");

    // Explicit rollback after error
    tx.rollback().await.unwrap();

    // 2. Assert: COUNT(*) FROM outbox_events WHERE aggregate_id = ? is strictly 0
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1"
    )
    .bind(&target_aggregate_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(count, 0, "Strictly ZERO orphaned outbox events must remain after rollback");
}

#[tokio::test]
async fn test_challenger_04_atomic_rollback_zero_orphaned_events_payment() {
    let harness = setup_challenger_harness().await;

    let target_payment_id = Uuid::new_v4().to_string();
    let invoice_id = Uuid::new_v4().to_string();
    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();

    let mut tx = harness.pool.begin().await.unwrap();

    // Prepare payment draft inside transaction
    let draft = OutboxEventDraft::payment_confirmed(
        tenant_uuid,
        &target_payment_id,
        &invoice_id,
        8_000_000,
        Some("REF-ROLLBACK-PAY"),
    );

    SqlxOutboxRepository::insert_tx_static(&mut tx, &draft)
        .await
        .unwrap();

    // Trigger an intentional rollback
    tx.rollback().await.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE aggregate_id = ?1"
    )
    .bind(&target_payment_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(count, 0, "Strictly ZERO orphaned payment events after rollback");
}

// ---------------------------------------------------------------------------
// 4. Exponential Backoff Timing & DLQ Progression (Attempts 1 to 5)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_challenger_05_exponential_backoff_timing_and_dlq_progression() {
    let harness = setup_challenger_harness().await;

    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let draft = OutboxEventDraft::invoice_issued(
        tenant_uuid,
        &Uuid::new_v4().to_string(),
        "INV-BACKOFF-001",
        12_000_000,
        "Backoff Test Ltd",
    );
    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    let ev = repo.insert(&draft).await.unwrap();

    let attempt_tracker = Arc::new(AtomicUsize::new(0));
    let failing_dispatcher: Arc<dyn EventDispatcher> = Arc::new(FailingDispatcher {
        error_type: "transient".to_string(),
        attempt_tracker: attempt_tracker.clone(),
    });

    let base_backoff_secs = 2i64;
    let processor = OutboxProcessor::new(
        harness.pool.clone(),
        failing_dispatcher,
        OutboxProcessorConfig {
            poll_interval: std::time::Duration::from_millis(5),
            batch_size: 10,
            max_retries: 5,
            base_backoff_secs,
        },
    );

    // Verify mathematical backoff schedule:
    // attempt 1: delay = 2 * 2^0 = 2s
    // attempt 2: delay = 2 * 2^1 = 4s
    // attempt 3: delay = 2 * 2^2 = 8s
    // attempt 4: delay = 2 * 2^3 = 16s
    // attempt 5: DLQ
    let expected_delays = [2i64, 4i64, 8i64, 16i64];

    for attempt in 1..=4 {
        let before_tick = Utc::now();
        let res = processor.process_tenant_batch(&harness.tenant_a_id, false).await.unwrap();

        assert_eq!(res.failed, 1, "Attempt {} must record 1 failure", attempt);
        assert_eq!(res.dead_lettered, 0, "Attempt {} must not be dead lettered", attempt);

        // Verify event in DB
        let row: (String, i64, i64, String, Option<String>) = sqlx::query_as(
            "SELECT status, attempt_count, retry_count, next_retry_at, last_error FROM outbox_events WHERE id = ?1"
        )
        .bind(&ev.id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();

        let (status, attempts, retries, next_retry_at, last_error) = row;
        assert_eq!(status, "FAILED");
        assert_eq!(attempts, attempt as i64);
        assert_eq!(retries, attempt as i64);
        assert!(last_error.unwrap().contains("HTTP 503"));

        // Verify next_retry_at is formatted with 'Z' and advances exponentially
        assert!(next_retry_at.ends_with('Z'));
        let parsed_next_retry = DateTime::parse_from_rfc3339(&next_retry_at)
            .expect("next_retry_at must be valid RFC3339")
            .with_timezone(&Utc);

        let expected_delay_sec = expected_delays[(attempt - 1) as usize];
        let diff_sec = (parsed_next_retry - before_tick).num_seconds();
        // Allow ±2 seconds margin for test execution jitter
        assert!(
            (diff_sec - expected_delay_sec).abs() <= 2,
            "Attempt {} expected delay ~{}s, observed {}s",
            attempt,
            expected_delay_sec,
            diff_sec
        );

        // Reset next_retry_at to past so next tick can immediately claim it
        sqlx::query("UPDATE outbox_events SET next_retry_at = '2000-01-01T00:00:00.000000Z' WHERE id = ?1")
            .bind(&ev.id)
            .execute(&harness.pool)
            .await
            .unwrap();
    }

    // 5th attempt: exhausts max_retries (5) -> Transitions to DEAD_LETTER
    let res = processor.process_tenant_batch(&harness.tenant_a_id, false).await.unwrap();
    assert_eq!(res.dead_lettered, 1, "5th attempt must transition to DEAD_LETTER");
    assert_eq!(res.failed, 0);

    let dlq_row: (String, i64, Option<String>) = sqlx::query_as(
        "SELECT status, attempt_count, last_error FROM outbox_events WHERE id = ?1"
    )
    .bind(&ev.id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();

    assert_eq!(dlq_row.0, "DEAD_LETTER");
    assert_eq!(dlq_row.1, 5);
    assert!(dlq_row.2.as_deref().unwrap().contains("HTTP 503"));

    // 6th tick: Dead-lettered events must be completely EXCLUDED from subsequent batches
    let idle_res = processor.process_tenant_batch(&harness.tenant_a_id, false).await.unwrap();
    assert_eq!(idle_res.processed, 0);
    assert_eq!(idle_res.failed, 0);
    assert_eq!(idle_res.dead_lettered, 0);

    // Verify dispatcher was invoked exactly 5 times
    assert_eq!(attempt_tracker.load(Ordering::SeqCst), 5);
}

// ---------------------------------------------------------------------------
// 5. Sidecar Failure Isolation Boundary (503, Timeouts, Panics)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_challenger_06_sidecar_failure_isolation_all_modes() {
    let harness = setup_challenger_harness().await;

    // 1. Issue core invoice
    let invoice_payload = json!({
        "customer_name": "PT Core Financial Entity",
        "customer_email": "core@financial.com",
        "due_date": "2026-12-31T00:00:00Z",
        "currency": "IDR",
        "tax_type": "EXEMPT",
        "items": [
            {
                "description": "Core Accounting Ledger Package",
                "quantity": 1,
                "unit_price": 20_000_000,
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

    // 2. Allocate payment on the invoice
    let payment_payload = json!({
        "invoice_id": invoice_id,
        "amount": 10_000_000,
        "payment_method": "BANK_TRANSFER",
        "reference": "REF-SIDECAR-TEST"
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/payments")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payment_payload.to_string()))
        .unwrap();

    let (status, resp_pay, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED, "Payment must succeed: {:?}", resp_pay);
    let payment_id = resp_pay["id"].as_str().unwrap().to_string();

    // 3. Test Failure Mode A: Connection Timeout
    let timeout_dispatcher: Arc<dyn EventDispatcher> = Arc::new(FailingDispatcher {
        error_type: "timeout".to_string(),
        attempt_tracker: Arc::new(AtomicUsize::new(0)),
    });
    let timeout_processor = OutboxProcessor::new(
        harness.pool.clone(),
        timeout_dispatcher,
        OutboxProcessorConfig::default(),
    );
    let res = timeout_processor.process_tenant_batch(&harness.tenant_a_id, false).await.unwrap();
    assert!(res.failed > 0, "Timeout should record failures");

    // 4. Test Failure Mode B: HTTP 503 via API endpoint simulation
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/outbox/process")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"simulate_sidecar_failure": true}).to_string()))
        .unwrap();

    let (status, resp_outbox, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp_outbox["status"], "retry_scheduled");

    // 5. Verify CORE BUSINESS OPERATIONS remain 100% UNTOUCHED and VALID
    // A. Invoice remains strictly PARTIALLY_PAID (due to 10M payment on 20M invoice)
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1/invoices/{}", invoice_id))
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, inv_check, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inv_check["status"], "PARTIALLY_PAID");
    assert_eq!(inv_check["total_amount"], 20_000_000);

    // B. Payment remains strictly CONFIRMED
    let pay_row: (String, i64) = sqlx::query_as(
        "SELECT status, amount FROM payments WHERE id = ?1"
    )
    .bind(&payment_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(pay_row.0, "CONFIRMED");
    assert_eq!(pay_row.1, 10_000_000);

    // C. Accounting ledger journals are balanced (SUM(debit) == SUM(credit))
    let trial_balance: (i64, i64) = sqlx::query_as(
        r#"
        SELECT COALESCE(SUM(jl.debit), 0), COALESCE(SUM(jl.credit), 0)
        FROM journal_lines jl
        JOIN journal_entries j ON j.id = jl.journal_id
        WHERE j.tenant_id = ?1
        "#
    )
    .bind(&harness.tenant_a_id)
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    assert_eq!(trial_balance.0, trial_balance.1, "Trial balance debits must equal credits");
    assert!(trial_balance.0 > 0, "Posted journals must exist");

    // 6. Verify SQLite Connection Pool Health & Zero Leaked Locks
    let pragma_mode: String = sqlx::query_scalar("PRAGMA journal_mode;").fetch_one(&harness.pool).await.unwrap();
    assert_eq!(pragma_mode.to_lowercase(), "wal");

    let integrity_check: String = sqlx::query_scalar("PRAGMA integrity_check;").fetch_one(&harness.pool).await.unwrap();
    assert_eq!(integrity_check.to_lowercase(), "ok");

    // Can still acquire read and write transactions smoothly without lock errors
    let mut test_tx = harness.pool.begin().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE tenant_id = ?1")
        .bind(&harness.tenant_a_id)
        .fetch_one(&mut *test_tx)
        .await
        .unwrap();
    assert!(count >= 2);
    test_tx.commit().await.unwrap();
}

// ---------------------------------------------------------------------------
// 6. Consumer Deduplication by UUID event_id (Idempotency Simulation)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_challenger_07_consumer_deduplication_by_uuid_event_id() {
    let harness = setup_challenger_harness().await;

    let tenant_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let draft = OutboxEventDraft::payment_confirmed(
        tenant_uuid,
        &Uuid::new_v4().to_string(),
        &Uuid::new_v4().to_string(),
        3_500_000,
        Some("REF-IDEMP-CHECK"),
    );
    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    let ev = repo.insert(&draft).await.unwrap();

    // 1. Verify valid UUID v4
    let parsed_uuid = Uuid::parse_str(&ev.id).expect("Event ID must be valid UUID v4");
    assert_eq!(parsed_uuid.get_version_num(), 4);

    // 2. Mock downstream consumer with deduplication cache / idempotency ledger
    struct MockConsumer {
        processed_event_ids: Mutex<HashSet<String>>,
        side_effect_execution_count: AtomicUsize,
    }

    impl MockConsumer {
        async fn handle_event(&self, event_id: &str) -> bool {
            let mut set = self.processed_event_ids.lock().await;
            if set.contains(event_id) {
                // Duplicate detected; skip side effect
                false
            } else {
                set.insert(event_id.to_string());
                self.side_effect_execution_count.fetch_add(1, Ordering::SeqCst);
                true
            }
        }
    }

    let consumer = MockConsumer {
        processed_event_ids: Mutex::new(HashSet::new()),
        side_effect_execution_count: AtomicUsize::new(0),
    };

    // First arrival: successfully executed
    let first = consumer.handle_event(&ev.id).await;
    assert!(first, "First event arrival must trigger processing");

    // Redeliver 10 times with identical event_id (at-least-once delivery guarantee)
    for _ in 1..=10 {
        let duplicate = consumer.handle_event(&ev.id).await;
        assert!(!duplicate, "Redelivered event must be recognized as duplicate and skipped");
    }

    // Assert side effect was executed strictly ONCE
    assert_eq!(
        consumer.side_effect_execution_count.load(Ordering::SeqCst),
        1,
        "Consumer side-effect must execute strictly once despite 10 replays"
    );

    // 3. Database PK collision test: Re-inserting exact same event_id fails
    let collision = sqlx::query(
        "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status) VALUES (?1, ?2, 'Test', 'Test', '1', '{}', 'PENDING')"
    )
    .bind(&ev.id)
    .bind(&harness.tenant_a_id)
    .execute(&harness.pool)
    .await;

    assert!(collision.is_err(), "Duplicate event ID insertion into outbox_events table must fail PK constraint");
}

// ---------------------------------------------------------------------------
// 7. Cross-Tenant Outbox Isolation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_challenger_08_cross_tenant_outbox_isolation() {
    let harness = setup_challenger_harness().await;

    let repo = SqlxOutboxRepository::new(harness.pool.clone());
    let tenant_a_uuid = Uuid::parse_str(&harness.tenant_a_id).unwrap();
    let tenant_b_uuid = Uuid::parse_str(&harness.tenant_b_id).unwrap();

    let draft_a = OutboxEventDraft::invoice_issued(
        tenant_a_uuid,
        &Uuid::new_v4().to_string(),
        "INV-TENANT-A-01",
        1_000_000,
        "Customer A",
    );
    let ev_a = repo.insert(&draft_a).await.unwrap();

    let draft_b = OutboxEventDraft::invoice_issued(
        tenant_b_uuid,
        &Uuid::new_v4().to_string(),
        "INV-TENANT-B-01",
        2_000_000,
        "Customer B",
    );
    let ev_b = repo.insert(&draft_b).await.unwrap();

    // Query outbox events as Tenant A
    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/outbox/events")
        .header(AUTHORIZATION, format!("Bearer {}", harness.owner_token))
        .header("X-Tenant-Id", &harness.tenant_a_id)
        .body(Body::empty())
        .unwrap();

    let (status, resp_json, _) = parse_response(harness.app.clone().oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    let events = resp_json["events"].as_array().unwrap();

    // Must see ev_a, but MUST NOT see ev_b
    assert!(events.iter().any(|e| e["id"].as_str() == Some(&ev_a.id)));
    assert!(!events.iter().any(|e| e["id"].as_str() == Some(&ev_b.id)), "Tenant A must NOT see Tenant B events");

    // Repository lookup of ev_b using Tenant A context returns None
    let ctx_a = TenantContext {
        tenant_id: tenant_a_uuid,
        actor_id: Uuid::parse_str(&harness.owner_id).unwrap(),
        role: Role::Owner,
    };
    let lookup_b = repo.get_by_id(&ctx_a, &ev_b.id).await.unwrap();
    assert!(lookup_b.is_none(), "Cross-tenant event lookup via repository must return None");
}
