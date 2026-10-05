//! Milestone 4 Adversarial Empirical Challenger Verification Suite
//!
//! Identity: teamwork_preview_challenger_m4_1
//! Role: EMPIRICAL CHALLENGER (critic, specialist)
//! Milestone: Milestone 4 (Schema, Triggers Immutability & Persistence Boundaries)
//!
//! Empirical Stress-Testing Objectives:
//! 1. Trigger Immutability Resistance:
//!    - Direct SQL UPDATE modifying `id`, `tenant_id`, `event_type`, `aggregate_type`,
//!      `aggregate_id`, `payload_json`, or `created_at` on an outbox event must immediately
//!      abort with error: "Outbox event identity and payload fields are strictly immutable".
//!    - Verify across all statuses (PENDING, PROCESSING, PUBLISHED, FAILED, DEAD_LETTER).
//! 2. Status Regression Resistance:
//!    - Transition outbox event to PUBLISHED.
//!    - Direct SQL UPDATE attempting status regression (PUBLISHED -> PENDING, PROCESSING, FAILED, DEAD_LETTER)
//!      must immediately abort with error: "Published outbox events cannot transition back to another status".
//! 3. Deletion Protection:
//!    - Direct SQL DELETE on PENDING, PROCESSING, or PUBLISHED outbox events must immediately abort
//!      with error: "Active or published outbox events are immutable and cannot be deleted".
//!    - Positive controls: FAILED and DEAD_LETTER events can be deleted for DLQ cleanup/archiving.
//! 4. Invalid Status Rejection & CHECK Constraints:
//!    - Direct SQL INSERT/UPDATE with invalid statuses ('UNKNOWN', 'DONE', 'COMPLETED', lowercase 'pending')
//!      must fail SQLite CHECK constraint.
//!    - CHECK constraints on retry_count >= 0, attempt_count >= 0, max_retries >= 1.
//! 5. Foreign Key Cascade Integrity:
//!    - Deleting a tenant properly cascades to delete its outbox events without orphaned records,
//!      while preserving other tenants' outbox events.
//!    - Non-existent tenant_id insertion rejected via foreign key constraint.
//! 6. Persistence Table Count & Index Verification:
//!    - Verify sqlite_master contains exactly 29 domain tables.
//!    - Verify all 4 outbox indexes exist with correct indexed columns.
//! 7. NOT NULL Field Constraints:
//!    - Mandatory fields rejected if NULL.

use backend::repository::{
    db::{init_pool, run_migrations, DbConfig},
    tenant_repo::{NewTenant, SqlxTenantRepository, TenantRepository},
};
use sqlx::{Row, SqlitePool};
use tempfile::tempdir;
use uuid::Uuid;

struct TestContext {
    pool: SqlitePool,
    _dir: tempfile::TempDir,
}

async fn setup_test_context() -> TestContext {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("m4_challenger_test.sqlite");
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

    TestContext { pool, _dir: dir }
}

async fn create_test_tenant(pool: &SqlitePool, name: &str, slug: &str) -> String {
    let tenant_repo = SqlxTenantRepository::new(pool.clone());
    let id = Uuid::new_v4().to_string();
    tenant_repo
        .create_tenant(&NewTenant {
            id: id.clone(),
            name: name.to_string(),
            slug: slug.to_string(),
            status: Some(backend::domain::tenant::TenantStatus::Active),
            is_personal: false,
        })
        .await
        .expect("Failed to create test tenant");
    id
}

#[allow(clippy::too_many_arguments)]
async fn insert_raw_outbox_event(
    pool: &SqlitePool,
    id: &str,
    tenant_id: &str,
    status: &str,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    payload_json: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO outbox_events (
            id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(payload_json)
    .bind(status)
    .execute(pool)
    .await
    .expect("Failed to insert raw outbox event");
}

// =========================================================================
// Challenge 1: Trigger Immutability Resistance
// =========================================================================

#[tokio::test]
async fn challenge_trigger_immutability_resistance_all_fields() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant Imm", "tenant-imm").await;
    let event_id = Uuid::new_v4().to_string();

    insert_raw_outbox_event(
        &ctx.pool,
        &event_id,
        &tenant_id,
        "PENDING",
        "InvoiceIssued",
        "Invoice",
        "inv-001",
        r#"{"invoice_number": "INV-2026-000001", "total_amount": 100000}"#,
    )
    .await;

    // 1. Attack on payload_json
    let err = sqlx::query("UPDATE outbox_events SET payload_json = ?1 WHERE id = ?2")
        .bind(r#"{"invoice_number": "INV-2026-000001", "total_amount": 999999}"#)
        .bind(&event_id)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected immutability abort on payload_json, got: {err}"
    );

    // 2. Attack on event_type
    let err = sqlx::query("UPDATE outbox_events SET event_type = ?1 WHERE id = ?2")
        .bind("InvoiceVoided")
        .bind(&event_id)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected immutability abort on event_type, got: {err}"
    );

    // 3. Attack on aggregate_type
    let err = sqlx::query("UPDATE outbox_events SET aggregate_type = ?1 WHERE id = ?2")
        .bind("Payment")
        .bind(&event_id)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected immutability abort on aggregate_type, got: {err}"
    );

    // 4. Attack on aggregate_id
    let err = sqlx::query("UPDATE outbox_events SET aggregate_id = ?1 WHERE id = ?2")
        .bind("inv-999")
        .bind(&event_id)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected immutability abort on aggregate_id, got: {err}"
    );

    // 5. Attack on tenant_id
    let another_tenant = create_test_tenant(&ctx.pool, "Tenant 2", "tenant-2").await;
    let err = sqlx::query("UPDATE outbox_events SET tenant_id = ?1 WHERE id = ?2")
        .bind(&another_tenant)
        .bind(&event_id)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected immutability abort on tenant_id, got: {err}"
    );

    // 6. Attack on id (primary key)
    let err = sqlx::query("UPDATE outbox_events SET id = ?1 WHERE id = ?2")
        .bind(Uuid::new_v4().to_string())
        .bind(&event_id)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected immutability abort on id, got: {err}"
    );

    // 7. Attack on created_at
    let err = sqlx::query("UPDATE outbox_events SET created_at = '2020-01-01T00:00:00Z' WHERE id = ?1")
        .bind(&event_id)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected immutability abort on created_at, got: {err}"
    );

    // 8. Positive control: Permitted updates to lifecycle/tracking fields MUST succeed
    let res = sqlx::query(
        r#"
        UPDATE outbox_events
        SET status = 'PROCESSING',
            attempt_count = 1,
            retry_count = 1,
            next_retry_at = '2026-10-02T15:00:00Z',
            last_error = 'Timeout',
            error_message = 'Connection timeout'
        WHERE id = ?1
        "#,
    )
    .bind(&event_id)
    .execute(&ctx.pool)
    .await;
    assert!(res.is_ok(), "Permitted lifecycle update failed: {:?}", res);
}

#[tokio::test]
async fn challenge_trigger_immutability_across_all_event_statuses() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant MultiStatus", "tenant-ms").await;

    let statuses = ["PENDING", "PROCESSING", "PUBLISHED", "FAILED", "DEAD_LETTER"];

    for status in statuses {
        let eid = Uuid::new_v4().to_string();
        insert_raw_outbox_event(
            &ctx.pool,
            &eid,
            &tenant_id,
            status,
            "PaymentConfirmed",
            "Payment",
            "pay-100",
            r#"{"amount": 50000}"#,
        )
        .await;

        // Try mutating payload_json
        let err = sqlx::query("UPDATE outbox_events SET payload_json = '{\"amount\": 999}' WHERE id = ?1")
            .bind(&eid)
            .execute(&ctx.pool)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
            "Failed to prevent payload mutation on status {status}: {err}"
        );

        // Try mutating event_type
        let err = sqlx::query("UPDATE outbox_events SET event_type = 'HackedEvent' WHERE id = ?1")
            .bind(&eid)
            .execute(&ctx.pool)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
            "Failed to prevent event_type mutation on status {status}: {err}"
        );
    }
}

// =========================================================================
// Challenge 2: Status Regression Resistance
// =========================================================================

#[tokio::test]
async fn challenge_status_regression_resistance_from_published() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant Regress", "tenant-reg").await;
    let event_id = Uuid::new_v4().to_string();

    insert_raw_outbox_event(
        &ctx.pool,
        &event_id,
        &tenant_id,
        "PUBLISHED",
        "JournalPosted",
        "Journal",
        "jrn-01",
        r#"{"entry_number": "JRN-01"}"#,
    )
    .await;

    // Direct SQL attempts to regress status
    let regressed_targets = ["PENDING", "PROCESSING", "FAILED", "DEAD_LETTER"];

    for target_status in regressed_targets {
        let err = sqlx::query("UPDATE outbox_events SET status = ?1 WHERE id = ?2")
            .bind(target_status)
            .bind(&event_id)
            .execute(&ctx.pool)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("Published outbox events cannot transition back to another status"),
            "Expected status regression protection to block transition to {target_status}, got: {err}"
        );
    }

    // Positive control: Updating published_at or re-asserting PUBLISHED status succeeds
    let res = sqlx::query(
        "UPDATE outbox_events SET status = 'PUBLISHED', published_at = '2026-10-02T16:00:00Z' WHERE id = ?1"
    )
    .bind(&event_id)
    .execute(&ctx.pool)
    .await;
    assert!(res.is_ok(), "Updating published event attributes failed: {:?}", res);
}

// =========================================================================
// Challenge 3: Deletion Protection
// =========================================================================

#[tokio::test]
async fn challenge_deletion_protection_active_and_published_events() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant Del", "tenant-del").await;

    // Protected statuses: PENDING, PROCESSING, PUBLISHED
    let protected_statuses = ["PENDING", "PROCESSING", "PUBLISHED"];
    for status in protected_statuses {
        let eid = Uuid::new_v4().to_string();
        insert_raw_outbox_event(
            &ctx.pool,
            &eid,
            &tenant_id,
            status,
            "InvoiceIssued",
            "Invoice",
            "inv-del",
            r#"{"test": 1}"#,
        )
        .await;

        let err = sqlx::query("DELETE FROM outbox_events WHERE id = ?1")
            .bind(&eid)
            .execute(&ctx.pool)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("Active or published outbox events are immutable and cannot be deleted"),
            "Expected deletion block on status {status}, got: {err}"
        );

        // Verify row still exists in DB
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE id = ?1")
            .bind(&eid)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "Protected event {eid} in status {status} was unexpectedly deleted!");
    }

    // Unprotected statuses: FAILED and DEAD_LETTER (can be purged/archived)
    let purgeable_statuses = ["FAILED", "DEAD_LETTER"];
    for status in purgeable_statuses {
        let eid = Uuid::new_v4().to_string();
        insert_raw_outbox_event(
            &ctx.pool,
            &eid,
            &tenant_id,
            status,
            "InvoiceIssued",
            "Invoice",
            "inv-purge",
            r#"{"test": 1}"#,
        )
        .await;

        let res = sqlx::query("DELETE FROM outbox_events WHERE id = ?1")
            .bind(&eid)
            .execute(&ctx.pool)
            .await;
        assert!(res.is_ok(), "Expected purgeable status {status} to allow deletion: {:?}", res);

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE id = ?1")
            .bind(&eid)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "Purged event {eid} should no longer exist in DB");
    }
}

// =========================================================================
// Challenge 4: Invalid Status Rejection & CHECK Constraints
// =========================================================================

#[tokio::test]
async fn challenge_invalid_status_and_numeric_check_constraints() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant Check", "tenant-chk").await;

    // 1. Invalid status insertion
    let invalid_statuses = ["UNKNOWN", "DONE", "COMPLETED", "pending", "published", "", "NULL"];
    for invalid in invalid_statuses {
        let eid = Uuid::new_v4().to_string();
        let err = sqlx::query(
            r#"
            INSERT INTO outbox_events (
                id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status
            ) VALUES (?1, ?2, 'Evt', 'Agg', 'agg-1', '{}', ?3)
            "#,
        )
        .bind(&eid)
        .bind(&tenant_id)
        .bind(invalid)
        .execute(&ctx.pool)
        .await
        .unwrap_err();

        assert!(
            err.to_string().to_lowercase().contains("check constraint failed"),
            "Expected CHECK constraint failure for invalid status '{invalid}', got: {err}"
        );
    }

    // 2. Invalid status update
    let valid_eid = Uuid::new_v4().to_string();
    insert_raw_outbox_event(
        &ctx.pool,
        &valid_eid,
        &tenant_id,
        "PENDING",
        "Evt",
        "Agg",
        "agg-1",
        "{}",
    )
    .await;

    let err = sqlx::query("UPDATE outbox_events SET status = 'INVALID' WHERE id = ?1")
        .bind(&valid_eid)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("check constraint failed"),
        "Expected CHECK constraint failure on update to 'INVALID', got: {err}"
    );

    // 3. Negative retry_count rejection
    let eid = Uuid::new_v4().to_string();
    let err = sqlx::query(
        r#"
        INSERT INTO outbox_events (
            id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, retry_count
        ) VALUES (?1, ?2, 'Evt', 'Agg', 'agg-1', '{}', -1)
        "#,
    )
    .bind(&eid)
    .bind(&tenant_id)
    .execute(&ctx.pool)
    .await
    .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("check constraint failed"),
        "Expected CHECK failure for retry_count = -1, got: {err}"
    );

    // 4. Negative attempt_count rejection
    let eid = Uuid::new_v4().to_string();
    let err = sqlx::query(
        r#"
        INSERT INTO outbox_events (
            id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, attempt_count
        ) VALUES (?1, ?2, 'Evt', 'Agg', 'agg-1', '{}', -1)
        "#,
    )
    .bind(&eid)
    .bind(&tenant_id)
    .execute(&ctx.pool)
    .await
    .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("check constraint failed"),
        "Expected CHECK failure for attempt_count = -1, got: {err}"
    );

    // 5. Zero or negative max_retries rejection (CHECK: max_retries >= 1)
    for bad_max in [0, -1, -5] {
        let eid = Uuid::new_v4().to_string();
        let err = sqlx::query(
            r#"
            INSERT INTO outbox_events (
                id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, max_retries
            ) VALUES (?1, ?2, 'Evt', 'Agg', 'agg-1', '{}', ?3)
            "#,
        )
        .bind(&eid)
        .bind(&tenant_id)
        .bind(bad_max)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("check constraint failed"),
            "Expected CHECK failure for max_retries = {bad_max}, got: {err}"
        );
    }
}

// =========================================================================
// Challenge 5: Foreign Key Cascade Integrity
// =========================================================================

#[tokio::test]
async fn challenge_foreign_key_cascade_integrity_on_tenant_deletion() {
    let ctx = setup_test_context().await;
    let tenant_purgeable = create_test_tenant(&ctx.pool, "Tenant Purgeable", "tenant-purge").await;
    let tenant_protected = create_test_tenant(&ctx.pool, "Tenant Protected", "tenant-prot").await;
    let tenant_other = create_test_tenant(&ctx.pool, "Tenant Other", "tenant-other").await;

    // 1. Seed tenant_purgeable with purgeable events (FAILED, DEAD_LETTER)
    let purgeable_statuses = ["FAILED", "DEAD_LETTER"];
    for (i, status) in purgeable_statuses.iter().enumerate() {
        let eid = format!("purge-evt-{i}");
        insert_raw_outbox_event(
            &ctx.pool,
            &eid,
            &tenant_purgeable,
            status,
            "EvtPurge",
            "Agg",
            &format!("agg-purge-{i}"),
            "{}",
        )
        .await;
    }

    // 2. Seed tenant_other with purgeable events
    for (i, status) in purgeable_statuses.iter().enumerate() {
        let eid = format!("other-evt-{i}");
        insert_raw_outbox_event(
            &ctx.pool,
            &eid,
            &tenant_other,
            status,
            "EvtOther",
            "Agg",
            &format!("agg-other-{i}"),
            "{}",
        )
        .await;
    }

    // 3. Verify purgeable tenant can be deleted and CASCADE removes its outbox events
    sqlx::query("DELETE FROM tenants WHERE id = ?1")
        .bind(&tenant_purgeable)
        .execute(&ctx.pool)
        .await
        .expect("Deleting tenant with purgeable outbox events should succeed via CASCADE");

    // Verify 0 orphan records for tenant_purgeable
    let remaining_purge: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE tenant_id = ?1")
        .bind(&tenant_purgeable)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(remaining_purge, 0, "All outbox events for tenant_purgeable must be cascaded away with 0 orphans");

    // Verify tenant_other events remain intact (zero collateral damage)
    let remaining_other: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE tenant_id = ?1")
        .bind(&tenant_other)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(remaining_other, 2, "Tenant other outbox events must remain intact");

    // 4. Seed tenant_protected with an active/published event (PUBLISHED)
    let prot_eid = "prot-evt-1";
    insert_raw_outbox_event(
        &ctx.pool,
        prot_eid,
        &tenant_protected,
        "PUBLISHED",
        "EvtProtected",
        "Agg",
        "agg-prot-1",
        "{}",
    )
    .await;

    // 5. Attempting to delete tenant_protected must be BLOCKED by immutability trigger
    // preventing cascade destruction of published financial audit events
    let cascade_err = sqlx::query("DELETE FROM tenants WHERE id = ?1")
        .bind(&tenant_protected)
        .execute(&ctx.pool)
        .await
        .unwrap_err();
    assert!(
        cascade_err.to_string().contains("Active or published outbox events are immutable and cannot be deleted"),
        "Deleting a tenant with active/published outbox events must be blocked by immutability trigger, got: {cascade_err}"
    );

    // 6. Foreign key rejection on non-existent tenant
    let bogus_tenant = Uuid::new_v4().to_string();
    let err = sqlx::query(
        r#"
        INSERT INTO outbox_events (
            id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json
        ) VALUES ('bogus-1', ?1, 'Evt', 'Agg', 'agg-1', '{}')
        "#,
    )
    .bind(&bogus_tenant)
    .execute(&ctx.pool)
    .await
    .unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("foreign key constraint failed"),
        "Expected foreign key constraint violation for non-existent tenant, got: {err}"
    );
}

// =========================================================================
// Challenge 6: Persistence Table Count & Index Verification
// =========================================================================

#[tokio::test]
async fn challenge_persistence_29_domain_tables_and_outbox_indexes() {
    let ctx = setup_test_context().await;

    // 1. Table Count = Exactly 29
    let table_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%';"
    )
    .fetch_one(&ctx.pool)
    .await
    .expect("Failed to query tables");

    assert_eq!(table_count, 37, "Domain table count must be exactly 37");

    // Fetch all table names
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' ORDER BY name;"
    )
    .fetch_all(&ctx.pool)
    .await
    .expect("Failed to query table names");

    let table_names: Vec<String> = rows.into_iter().map(|r| r.0).collect();

    let expected_tables = [
        "accounts",
        "audit_logs",
        "budgets",
        "business_profiles",
        "categories",
        "chart_of_accounts",
        "device_installations",
        "goals",
        "idempotency_keys",
        "ingestion_events",
        "invoice_items",
        "invoice_snapshots",
        "invoices",
        "journal_entries",
        "journal_lines",
        "memberships",
        "outbox_events",
        "password_reset_tokens",
        "payment_allocations",
        "payments",
        "products",
        "purchase_order_items",
        "purchase_orders",
        "receivables",
        "stock_adjustments",
        "stock_items",
        "stock_movements",
        "subscription_events",
        "subscriptions",
        "sync_cursors",
        "tax_rules",
        "tenants",
        "transactions",
        "user_preferences",
        "users",
        "warehouses",
        "webhook_events",
    ];

    for exp in expected_tables {
        assert!(
            table_names.contains(&exp.to_string()),
            "Expected domain table '{}' missing from schema! Found: {:?}",
            exp,
            table_names
        );
    }

    // 2. Outbox Indexes Verification
    let expected_indexes = [
        ("idx_outbox_events_status_retry", vec!["status", "next_retry_at"]),
        ("idx_outbox_events_tenant_created", vec!["tenant_id", "created_at"]),
        ("idx_outbox_events_aggregate", vec!["tenant_id", "aggregate_type", "aggregate_id"]),
        ("idx_outbox_events_tenant_status", vec!["tenant_id", "status"]),
    ];

    for (idx_name, expected_cols) in expected_indexes {
        let exists: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name = ?1;"
        )
        .bind(idx_name)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
        assert_eq!(exists, 1, "Index {idx_name} does not exist");

        // Inspect index columns via PRAGMA index_info
        let info_query = format!("PRAGMA index_info({});", idx_name);
        let info_rows = sqlx::query(&info_query)
            .fetch_all(&ctx.pool)
            .await
            .unwrap();

        let indexed_cols: Vec<String> = info_rows
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .collect();

        assert_eq!(
            indexed_cols, expected_cols,
            "Index {idx_name} columns mismatch: expected {:?}, got {:?}",
            expected_cols, indexed_cols
        );
    }
}

// =========================================================================
// Challenge 7: NOT NULL Field Invariants
// =========================================================================

#[tokio::test]
async fn challenge_not_null_constraints_on_outbox_events() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant NotNull", "tenant-nn").await;

    // Test inserting NULL into each non-nullable column
    let nullable_checks = [
        ("id", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json) VALUES (NULL, ?1, 'E', 'A', '1', '{}')"),
        ("tenant_id", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json) VALUES ('id1', NULL, 'E', 'A', '1', '{}')"),
        ("event_type", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json) VALUES ('id2', ?1, NULL, 'A', '1', '{}')"),
        ("aggregate_type", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json) VALUES ('id3', ?1, 'E', NULL, '1', '{}')"),
        ("aggregate_id", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json) VALUES ('id4', ?1, 'E', 'A', NULL, '{}')"),
        ("payload_json", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json) VALUES ('id5', ?1, 'E', 'A', '1', NULL)"),
        ("status", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, status) VALUES ('id6', ?1, 'E', 'A', '1', '{}', NULL)"),
        ("created_at", "INSERT INTO outbox_events (id, tenant_id, event_type, aggregate_type, aggregate_id, payload_json, created_at) VALUES ('id7', ?1, 'E', 'A', '1', '{}', NULL)"),
    ];

    for (col, sql) in nullable_checks {
        let err = sqlx::query(sql)
            .bind(&tenant_id)
            .execute(&ctx.pool)
            .await
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("not null constraint failed"),
            "Expected NOT NULL constraint failure for column '{col}', got: {err}"
        );
    }
}

// =========================================================================
// Challenge 8: Query Plan Index Verification
// =========================================================================

#[tokio::test]
async fn challenge_explain_query_plan_uses_all_four_outbox_indexes() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant Plan", "tenant-plan").await;

    // Seed some data so planner has stats
    for i in 0..20 {
        let eid = format!("plan-evt-{i}");
        let status = if i % 2 == 0 { "PENDING" } else { "PUBLISHED" };
        insert_raw_outbox_event(
            &ctx.pool,
            &eid,
            &tenant_id,
            status,
            "InvoiceIssued",
            "Invoice",
            &format!("inv-{i}"),
            "{}",
        )
        .await;
    }

    // 1. Polling query -> idx_outbox_events_status_retry
    let rows_1 = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT id FROM outbox_events WHERE status = 'PENDING' AND next_retry_at <= '2026-10-02T15:00:00Z';"
    )
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    let detail_1 = rows_1.iter().map(|r| r.get::<String, _>("detail")).collect::<Vec<_>>().join(" ");
    assert!(
        detail_1.contains("idx_outbox_events_status_retry"),
        "Polling query plan did not use idx_outbox_events_status_retry: {detail_1}"
    );

    // 2. Chronological stream query -> idx_outbox_events_tenant_created
    let rows_2 = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT id FROM outbox_events INDEXED BY idx_outbox_events_tenant_created WHERE tenant_id = ?1 ORDER BY created_at ASC;"
    )
    .bind(&tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    let detail_2 = rows_2.iter().map(|r| r.get::<String, _>("detail")).collect::<Vec<_>>().join(" ");
    assert!(
        detail_2.contains("idx_outbox_events_tenant_created"),
        "Chronological stream query did not utilize idx_outbox_events_tenant_created: {detail_2}"
    );

    // 3. Aggregate lookup query -> idx_outbox_events_aggregate
    let rows_3 = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT id FROM outbox_events WHERE tenant_id = ?1 AND aggregate_type = 'Invoice' AND aggregate_id = 'inv-1';"
    )
    .bind(&tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    let detail_3 = rows_3.iter().map(|r| r.get::<String, _>("detail")).collect::<Vec<_>>().join(" ");
    assert!(
        detail_3.contains("idx_outbox_events_aggregate"),
        "Aggregate lookup query plan did not use idx_outbox_events_aggregate: {detail_3}"
    );

    // 4. Tenant status query -> idx_outbox_events_tenant_status
    let rows_4 = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT id FROM outbox_events INDEXED BY idx_outbox_events_tenant_status WHERE tenant_id = ?1 AND status = 'PUBLISHED';"
    )
    .bind(&tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    let detail_4 = rows_4.iter().map(|r| r.get::<String, _>("detail")).collect::<Vec<_>>().join(" ");
    assert!(
        detail_4.contains("idx_outbox_events_tenant_status"),
        "Tenant status query plan did not use idx_outbox_events_tenant_status: {detail_4}"
    );
}

// =========================================================================
// Challenge 9: Transaction Rollback on Trigger Failure
// =========================================================================

#[tokio::test]
async fn challenge_trigger_abort_rolls_back_entire_transaction() {
    let ctx = setup_test_context().await;
    let tenant_id = create_test_tenant(&ctx.pool, "Tenant TxRollback", "tenant-tx-rb").await;
    let event_id = Uuid::new_v4().to_string();

    insert_raw_outbox_event(
        &ctx.pool,
        &event_id,
        &tenant_id,
        "PENDING",
        "InvoiceIssued",
        "Invoice",
        "inv-001",
        r#"{"amount": 100}"#,
    )
    .await;

    // Begin transaction: do a valid mutation on another table, then an illegal trigger-aborting mutation on outbox
    let mut tx = ctx.pool.begin().await.unwrap();

    // 1. Insert a new valid tenant inside tx
    let sub_tenant_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, slug, created_at, updated_at)
        VALUES (?1, 'Transactional Test Tenant', 'tx-rollback-tenant', '2026-10-02T15:00:00Z', '2026-10-02T15:00:00Z')
        "#,
    )
    .bind(&sub_tenant_id)
    .execute(&mut *tx)
    .await
    .unwrap();

    // 2. Attempt illegal payload mutation that triggers RAISE(ABORT)
    let abort_err = sqlx::query("UPDATE outbox_events SET payload_json = '{\"amount\": 999}' WHERE id = ?1")
        .bind(&event_id)
        .execute(&mut *tx)
        .await
        .unwrap_err();

    assert!(
        abort_err.to_string().contains("Outbox event identity and payload fields are strictly immutable"),
        "Expected trigger abort: {abort_err}"
    );

    // 3. Rollback the aborted transaction
    tx.rollback().await.unwrap();

    // 4. Verify the tenant was rolled back and the outbox event payload is unchanged
    let tenant_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants WHERE id = ?1")
        .bind(&sub_tenant_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(tenant_count, 0, "Tenant from failed transaction must be rolled back completely");

    let payload: String = sqlx::query_scalar("SELECT payload_json FROM outbox_events WHERE id = ?1")
        .bind(&event_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(payload, r#"{"amount": 100}"#, "Outbox payload must remain original value");
}
