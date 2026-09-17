use backend::domain::money::Rupiah;
use backend::repository::{
    init_pool, run_migrations, verify_pragmas, AccountRepository, AuditRepository,
    CategoryRepository, DbConfig, DbError, IdempotencyLockResult, IdempotencyRepository,
    NewAccount, NewAuditLog, NewCategory, NewUser, SqlxAccountRepository, SqlxAuditRepository,
    SqlxCategoryRepository, SqlxIdempotencyRepository, SqlxUserRepository, UserRepository,
};
use std::sync::Arc;
use tempfile::tempdir;

/// Helper to create an isolated file-based database for a test with WAL mode
async fn setup_test_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("test_db.sqlite");
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
    (pool, dir)
}

#[tokio::test]
async fn test_pool_pragmas_and_migration_execution() {
    let (pool, _dir) = setup_test_db().await;

    // 1. Verify PRAGMAs
    let pragmas = verify_pragmas(&pool)
        .await
        .expect("Failed to verify pragmas");
    assert_eq!(pragmas.journal_mode.to_lowercase(), "wal");
    assert_eq!(pragmas.busy_timeout, 5000);
    assert!(pragmas.foreign_keys);
    assert_eq!(pragmas.synchronous, 1); // 1 = NORMAL

    // 2. Verify all 15 tables exist (10 baseline + 4 v3.1.0 support tables + 1 user_preferences)
    let table_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%';"
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to query tables");

    assert_eq!(table_count, 15, "Expected 15 domain tables after v3.1.0 migration");

    // 3. Verify mandatory composite and acceleration indexes
    let required_indexes = [
        "idx_transactions_user_date",
        "idx_transactions_account",
        "idx_transactions_to_account",
        "idx_transactions_user_category",
        "idx_accounts_user",
        "idx_categories_user",
        "idx_categories_system",
        "idx_budgets_user",
        "idx_budgets_user_category",
        "idx_goals_user",
        "idx_subscriptions_user",
        "idx_subscriptions_provider_id",
        "idx_audit_logs_user_date",
        "idx_idempotency_expires",
        "idx_webhook_events_status",
        // v3.1.0 additions:
        "idx_categories_user_norm",
        "idx_transactions_external_ref",
        "idx_transactions_ingestion",
        "idx_ingestion_user_status",
        "idx_subscription_events_user",
        "idx_device_installations_user",
        "idx_user_preferences_user",
    ];

    for idx_name in required_indexes {
        let exists: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name = ?1;",
        )
        .bind(idx_name)
        .fetch_one(&pool)
        .await
        .expect("Failed to query index");

        assert_eq!(exists, 1, "Expected index {} to exist", idx_name);
    }
}

#[tokio::test]
async fn test_foreign_key_and_cascade_enforcement() {
    let (pool, _dir) = setup_test_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());

    // 1. Inserting account with non-existent user_id must fail foreign key check
    let bad_acc = NewAccount {
        id: "acc_bad_1".to_string(),
        user_id: "non_existent_user".to_string(),
        name: "Orphan Account".to_string(),
        account_type: "checking".to_string(),
        currency: Some("IDR".to_string()),
        initial_balance: Rupiah::new(100_000),
        color: None,
        icon: None,
    };

    let err = account_repo.create(&bad_acc).await.unwrap_err();
    assert!(
        matches!(err, DbError::ForeignKeyViolation(_)),
        "Expected ForeignKeyViolation, got {:?}",
        err
    );

    // 2. Create valid user and valid account
    let user = user_repo
        .create(&NewUser {
            id: "user_cascade_test".to_string(),
            email: "cascade@test.com".to_string(),
            password_hash: "hash123".to_string(),
            display_name: "Cascade User".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .expect("Failed to create user");

    let acc = account_repo
        .create(&NewAccount {
            id: "acc_cascade_test".to_string(),
            user_id: user.id.clone(),
            name: "Wallet".to_string(),
            account_type: "e_wallet".to_string(),
            currency: None,
            initial_balance: Rupiah::new(50_000),
            color: None,
            icon: None,
        })
        .await
        .expect("Failed to create account");

    assert_eq!(acc.user_id, user.id);

    // 3. Delete user -> cascade should delete account
    sqlx::query("DELETE FROM users WHERE id = ?1")
        .bind(&user.id)
        .execute(&pool)
        .await
        .expect("Failed to delete user");

    let found_acc = account_repo
        .find_by_id(&user.id, &acc.id)
        .await
        .expect("Query failed");
    assert!(
        found_acc.is_none(),
        "Account should have been cascade-deleted"
    );
}

#[tokio::test]
async fn test_rupiah_value_object_invariants_and_zero_floats() {
    // 1. Basic arithmetic and constants
    let zero = Rupiah::ZERO;
    assert!(zero.is_zero());
    assert!(!zero.is_positive());
    assert!(!zero.is_negative());

    let r1 = Rupiah::new(50_000);
    let r2 = Rupiah::new(25_000);

    assert_eq!(r1.checked_add(r2), Some(Rupiah::new(75_000)));
    assert_eq!(r1.checked_sub(r2), Some(Rupiah::new(25_000)));
    assert_eq!(r2.checked_sub(r1), Some(Rupiah::new(-25_000)));

    // 2. Checked multiplication and division
    assert_eq!(
        Rupiah::new(5_000).checked_mul(12),
        Some(Rupiah::new(60_000))
    );
    assert_eq!(
        Rupiah::new(100_000).checked_div(3),
        Some(Rupiah::new(33_333))
    );

    let (q, r) = Rupiah::new(100_000).checked_div_rem(3).unwrap();
    assert_eq!(q, Rupiah::new(33_333));
    assert_eq!(r, Rupiah::new(1));
    assert_eq!(q.as_i64() * 3 + r.as_i64(), 100_000);

    // 3. Overflow and boundary detection
    assert_eq!(Rupiah::MAX.checked_add(Rupiah::new(1)), None);
    assert_eq!(Rupiah::MIN.checked_sub(Rupiah::new(1)), None);
    assert_eq!(Rupiah::MAX.checked_mul(2), None);
    assert_eq!(Rupiah::new(10_000).checked_div(0), None);
    assert_eq!(Rupiah::MIN.checked_div(-1), None);

    // 4. Basis points and ratios
    // 11% PPN (Indonesian VAT) = 1,100 bps
    let amount = Rupiah::new(200_000);
    assert_eq!(amount.checked_mul_bps(1_100), Some(Rupiah::new(22_000)));
    // 3/4 split
    assert_eq!(amount.checked_mul_ratio(3, 4), Some(Rupiah::new(150_000)));

    // 5. Formatting
    assert_eq!(Rupiah::new(0).format_idr(), "Rp 0");
    assert_eq!(Rupiah::new(5_000).format_idr(), "Rp 5.000");
    assert_eq!(Rupiah::new(1_500_000).format_idr(), "Rp 1.500.000");
    assert_eq!(Rupiah::new(-50_000).format_idr(), "-Rp 50.000");
    assert_eq!(Rupiah::MAX.format_idr(), "Rp 9.223.372.036.854.775.807");
    assert_eq!(Rupiah::MIN.format_idr(), "-Rp 9.223.372.036.854.775.808");

    // 6. Serde transparent serialization / rejection of float
    let json_int = serde_json::to_string(&r1).unwrap();
    assert_eq!(json_int, "50000");

    let parsed: Rupiah = serde_json::from_str("50000").unwrap();
    assert_eq!(parsed, r1);

    let parsed_neg: Rupiah = serde_json::from_str("-25000").unwrap();
    assert_eq!(parsed_neg, Rupiah::new(-25_000));

    // Reject floating-point JSON numbers
    let float_parse: Result<Rupiah, _> = serde_json::from_str("50000.50");
    assert!(float_parse.is_err(), "Must reject floating-point numbers");
}

#[tokio::test]
async fn test_user_repository_crud_and_uniqueness() {
    let (pool, _dir) = setup_test_db().await;
    let user_repo = SqlxUserRepository::new(pool);

    let new_user = NewUser {
        id: "usr_alice".to_string(),
        email: "Alice@Example.com".to_string(),
        password_hash: "hashed_pwd_secret".to_string(),
        display_name: "Alice Wonderland".to_string(),
        currency: Some("IDR".to_string()),
        role: Some("user".to_string()),
        subscription_tier: Some("free".to_string()),
    };

    let user = user_repo
        .create(&new_user)
        .await
        .expect("User creation failed");
    assert_eq!(user.id, "usr_alice");
    assert_eq!(user.email, "Alice@Example.com");
    assert_eq!(user.subscription_tier, "free");

    // Lookup by ID
    let found_by_id = user_repo.find_by_id("usr_alice").await.unwrap();
    assert!(found_by_id.is_some());
    assert_eq!(found_by_id.unwrap().display_name, "Alice Wonderland");

    // Case-insensitive lookup by email
    let found_by_email = user_repo.find_by_email("alice@example.com").await.unwrap();
    assert!(found_by_email.is_some());
    assert_eq!(found_by_email.unwrap().id, "usr_alice");

    // Unique email enforcement (case-sensitive check in DB or conflict)
    let duplicate_user = NewUser {
        id: "usr_duplicate".to_string(),
        email: "Alice@Example.com".to_string(),
        password_hash: "other_hash".to_string(),
        display_name: "Clone".to_string(),
        currency: None,
        role: None,
        subscription_tier: None,
    };
    let err = user_repo.create(&duplicate_user).await.unwrap_err();
    assert!(matches!(err, DbError::UniqueViolation { .. }));

    // Update tier
    user_repo
        .update_tier("usr_alice", "premium")
        .await
        .expect("Update tier failed");
    let updated = user_repo.find_by_id("usr_alice").await.unwrap().unwrap();
    assert_eq!(updated.subscription_tier, "premium");

    // Non-existent user update returns NotFound
    let bad_update = user_repo.update_tier("usr_ghost", "premium").await;
    assert!(matches!(bad_update, Err(DbError::NotFound)));
}

#[tokio::test]
async fn test_account_multi_tenant_isolation_and_atomic_balance() {
    let (pool, _dir) = setup_test_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());

    // Create User A and User B
    user_repo
        .create(&NewUser {
            id: "user_a".to_string(),
            email: "a@test.com".to_string(),
            password_hash: "h".to_string(),
            display_name: "User A".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    user_repo
        .create(&NewUser {
            id: "user_b".to_string(),
            email: "b@test.com".to_string(),
            password_hash: "h".to_string(),
            display_name: "User B".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    // User A creates Account A
    let acc_a = account_repo
        .create(&NewAccount {
            id: "acc_a_1".to_string(),
            user_id: "user_a".to_string(),
            name: "Alice BCA".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(1_000_000),
            color: Some("#0055ff".to_string()),
            icon: Some("bank".to_string()),
        })
        .await
        .expect("Account creation failed");

    // Multi-tenant isolation test:
    // 1. User A can find Account A
    let found_a = account_repo.find_by_id("user_a", &acc_a.id).await.unwrap();
    assert!(found_a.is_some());
    assert_eq!(found_a.unwrap().name, "Alice BCA");

    // 2. User B trying to find Account A receives Ok(None) (mapped to HTTP 404)
    let found_cross = account_repo.find_by_id("user_b", &acc_a.id).await.unwrap();
    assert!(
        found_cross.is_none(),
        "Cross-tenant access must return None"
    );

    // 3. User B trying to update balance of Account A returns Err(DbError::NotFound)
    let bad_update = account_repo
        .update_balance("user_b", &acc_a.id, Rupiah::new(0))
        .await;
    assert!(
        matches!(bad_update, Err(DbError::NotFound)),
        "Cross-tenant update must return NotFound"
    );

    // 4. User B trying to archive Account A returns Err(DbError::NotFound)
    let bad_archive = account_repo.archive("user_b", &acc_a.id).await;
    assert!(
        matches!(bad_archive, Err(DbError::NotFound)),
        "Cross-tenant archive must return NotFound"
    );

    // 5. Atomic balance adjustment by User A
    let post_adj = account_repo
        .adjust_balance_atomic("user_a", &acc_a.id, 250_000)
        .await
        .expect("Adjustment failed");
    assert_eq!(post_adj, Rupiah::new(1_250_000));

    let updated_acc = account_repo
        .find_by_id("user_a", &acc_a.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_acc.current_balance, Rupiah::new(1_250_000));

    // 6. High-concurrency atomic balance maintenance test
    let repo_arc = Arc::new(account_repo);
    let mut tasks = Vec::new();
    // 20 concurrent tasks, each adding Rp 10,000
    for _ in 0..20 {
        let repo = Arc::clone(&repo_arc);
        let acc_id = acc_a.id.clone();
        tasks.push(tokio::spawn(async move {
            repo.adjust_balance_atomic("user_a", &acc_id, 10_000).await
        }));
    }

    for task in tasks {
        let res = task.await.expect("Task panicked");
        assert!(res.is_ok());
    }

    let final_acc = repo_arc
        .find_by_id("user_a", &acc_a.id)
        .await
        .unwrap()
        .unwrap();
    // 1,250,000 + (20 * 10,000) = 1,450,000
    assert_eq!(final_acc.current_balance, Rupiah::new(1_450_000));
}

#[tokio::test]
async fn test_category_system_protection_and_soft_delete() {
    let (pool, _dir) = setup_test_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let cat_repo = SqlxCategoryRepository::new(pool);

    // Setup users
    user_repo
        .create(&NewUser {
            id: "u_cat_1".to_string(),
            email: "cat1@test.com".to_string(),
            password_hash: "p".to_string(),
            display_name: "Cat 1".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    user_repo
        .create(&NewUser {
            id: "u_cat_2".to_string(),
            email: "cat2@test.com".to_string(),
            password_hash: "p".to_string(),
            display_name: "Cat 2".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    // 1. Create global system category
    let sys_cat = cat_repo
        .create(&NewCategory {
            id: "cat_sys_salary".to_string(),
            user_id: None,
            name: "Salary".to_string(),
            category_type: "income".to_string(),
            icon: Some("briefcase".to_string()),
            color: Some("#00aa00".to_string()),
            is_system: true,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .expect("Failed to create system category");

    // 2. Create custom category for User 1
    let u1_cat = cat_repo
        .create(&NewCategory {
            id: "cat_u1_crypto".to_string(),
            user_id: Some("u_cat_1".to_string()),
            name: "Crypto Mining".to_string(),
            category_type: "income".to_string(),
            icon: None,
            color: None,
            is_system: false,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .expect("Failed to create user category");

    // 3. Create custom category for User 2
    let _u2_cat = cat_repo
        .create(&NewCategory {
            id: "cat_u2_hobby".to_string(),
            user_id: Some("u_cat_2".to_string()),
            name: "Model Trains".to_string(),
            category_type: "expense".to_string(),
            icon: None,
            color: None,
            is_system: false,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .expect("Failed to create user category");

    // 4. Listing for User 1: must include system category + User 1 category, NEVER User 2
    let u1_list = cat_repo.list_by_user("u_cat_1").await.unwrap();
    assert_eq!(u1_list.len(), 2);
    let names: Vec<String> = u1_list.into_iter().map(|c| c.name).collect();
    assert!(names.contains(&"Salary".to_string()));
    assert!(names.contains(&"Crypto Mining".to_string()));
    assert!(!names.contains(&"Model Trains".to_string()));

    // 5. Soft-delete protection on system category: must return CannotDeleteSystemEntity
    let sys_del_err = cat_repo
        .soft_delete("u_cat_1", &sys_cat.id)
        .await
        .unwrap_err();
    assert!(
        matches!(sys_del_err, DbError::CannotDeleteSystemEntity),
        "Expected CannotDeleteSystemEntity, got {:?}",
        sys_del_err
    );

    // 6. User 2 trying to soft-delete User 1's category: must return NotFound
    let cross_del_err = cat_repo
        .soft_delete("u_cat_2", &u1_cat.id)
        .await
        .unwrap_err();
    assert!(
        matches!(cross_del_err, DbError::NotFound),
        "Expected NotFound, got {:?}",
        cross_del_err
    );

    // 7. User 1 soft-deletes their own category: succeeds
    cat_repo
        .soft_delete("u_cat_1", &u1_cat.id)
        .await
        .expect("Soft delete failed");

    // Listing for User 1 now only contains the system category
    let u1_list_after = cat_repo.list_by_user("u_cat_1").await.unwrap();
    assert_eq!(u1_list_after.len(), 1);
    assert_eq!(u1_list_after[0].name, "Salary");
}

#[tokio::test]
async fn test_idempotency_repository_lifecycle_and_tenant_isolation() {
    let (pool, _dir) = setup_test_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let idemp_repo = SqlxIdempotencyRepository::new(pool);

    // Setup users
    user_repo
        .create(&NewUser {
            id: "usr_idem_1".to_string(),
            email: "idem1@test.com".to_string(),
            password_hash: "p".to_string(),
            display_name: "Idem 1".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    user_repo
        .create(&NewUser {
            id: "usr_idem_2".to_string(),
            email: "idem2@test.com".to_string(),
            password_hash: "p".to_string(),
            display_name: "Idem 2".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    let shared_key = "key-uuid-12345";
    let hash_1 = "sha256_hash_payload_1";
    let hash_2 = "sha256_hash_payload_2";

    // 1. User 1 acquires lock
    let res1 = idemp_repo
        .acquire_lock("usr_idem_1", shared_key, hash_1, 60)
        .await
        .unwrap();
    assert_eq!(res1, IdempotencyLockResult::Acquired);

    // 2. User 2 acquires lock with the SAME key string (tenant isolation verification)
    let res2 = idemp_repo
        .acquire_lock("usr_idem_2", shared_key, hash_1, 60)
        .await
        .unwrap();
    assert_eq!(
        res2,
        IdempotencyLockResult::Acquired,
        "Different tenants must have independent idempotency namespaces"
    );

    // 3. User 1 re-submits with SAME key and SAME payload while in progress -> InProgress
    let res1_repeat = idemp_repo
        .acquire_lock("usr_idem_1", shared_key, hash_1, 60)
        .await
        .unwrap();
    assert_eq!(res1_repeat, IdempotencyLockResult::InProgress);

    // 4. User 1 submits with SAME key but DIFFERENT payload -> MismatchedPayload
    let res1_mismatch = idemp_repo
        .acquire_lock("usr_idem_1", shared_key, hash_2, 60)
        .await
        .unwrap();
    assert_eq!(res1_mismatch, IdempotencyLockResult::MismatchedPayload);

    // 5. User 1 completes operation and saves response
    idemp_repo
        .save_response(
            "usr_idem_1",
            shared_key,
            201,
            r#"{"id":"tx_123","status":"created"}"#,
        )
        .await
        .expect("Failed to save response");

    // 6. User 1 calls acquire_lock again -> receives Cached response
    let res1_cached = idemp_repo
        .acquire_lock("usr_idem_1", shared_key, hash_1, 60)
        .await
        .unwrap();
    match res1_cached {
        IdempotencyLockResult::Cached {
            response_code,
            response_body,
        } => {
            assert_eq!(response_code, 201);
            assert!(response_body.contains("tx_123"));
        }
        other => panic!("Expected Cached response, got {:?}", other),
    }

    // 7. Lock release on failure
    let fail_key = "key-fail-test";
    idemp_repo
        .acquire_lock("usr_idem_1", fail_key, hash_1, 60)
        .await
        .unwrap();
    idemp_repo
        .release_lock_on_failure("usr_idem_1", fail_key)
        .await
        .unwrap();
    // Re-acquire should succeed immediately
    let re_acquire = idemp_repo
        .acquire_lock("usr_idem_1", fail_key, hash_1, 60)
        .await
        .unwrap();
    assert_eq!(re_acquire, IdempotencyLockResult::Acquired);
}

#[tokio::test]
async fn test_audit_repository_logging_and_pagination() {
    let (pool, _dir) = setup_test_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let audit_repo = SqlxAuditRepository::new(pool);

    user_repo
        .create(&NewUser {
            id: "u_audit".to_string(),
            email: "audit@test.com".to_string(),
            password_hash: "p".to_string(),
            display_name: "Audit User".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    // Log 5 events
    for i in 1..=5 {
        audit_repo
            .log_event(&NewAuditLog {
                user_id: Some("u_audit".to_string()),
                action: format!("action_{}", i),
                entity_type: "transaction".to_string(),
                entity_id: format!("tx_{}", i),
                ip_address: Some("127.0.0.1".to_string()),
                user_agent: Some("Mozilla/5.0".to_string()),
                details: Some(format!("{{\"step\":{}}}", i)),
            })
            .await
            .expect("Failed to log audit event");
    }

    // Page 1: limit 3, offset 0
    let page1 = audit_repo.list_by_user("u_audit", 3, 0).await.unwrap();
    assert_eq!(page1.len(), 3);

    // Page 2: limit 3, offset 3
    let page2 = audit_repo.list_by_user("u_audit", 3, 3).await.unwrap();
    assert_eq!(page2.len(), 2);

    // Different user has 0 logs
    let empty = audit_repo.list_by_user("u_other", 10, 0).await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn test_migration_0003_v3_1_0_schema_upgrade() {
    let (pool, _dir) = setup_test_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());

    // Create a test user
    user_repo
        .create(&NewUser {
            id: "u_v310".to_string(),
            email: "v310@test.com".to_string(),
            password_hash: "p".to_string(),
            display_name: "v3.1.0 User".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    // 1. Test vocabulary columns in categories
    sqlx::query(
        r#"
        INSERT INTO categories (id, user_id, name, display_name, normalized_name, metadata, category_type, is_system, created_at, updated_at)
        VALUES ('cat_v310_1', 'u_v310', 'Ngopi Santai', 'Ngopi Santai', 'ngopi santai', '{"icon_variant":"coffee"}', 'expense', 0, '2026-09-17T00:00:00Z', '2026-09-17T00:00:00Z')
        "#
    )
    .execute(&pool)
    .await
    .expect("Failed to insert category with custom vocabulary columns");

    let (name, disp, norm, meta): (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT name, display_name, normalized_name, metadata FROM categories WHERE id = 'cat_v310_1'"
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to query vocabulary columns");

    assert_eq!(name, "Ngopi Santai");
    assert_eq!(disp, Some("Ngopi Santai".to_string()));
    assert_eq!(norm, Some("ngopi santai".to_string()));
    assert_eq!(meta, Some("{\"icon_variant\":\"coffee\"}".to_string()));

    // 2. Test transactions table ingestion columns
    let account_repo = SqlxAccountRepository::new(pool.clone());
    account_repo
        .create(&NewAccount {
            id: "acc_v310".to_string(),
            user_id: "u_v310".to_string(),
            name: "Main Wallet".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(500_000),
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    sqlx::query(
        r#"
        INSERT INTO transactions (
            id, user_id, account_id, category_id, transaction_type, amount, date, description,
            source, external_reference, merchant, confidence, ingestion_id, created_at, updated_at
        )
        VALUES (
            'tx_v310_1', 'u_v310', 'acc_v310', 'cat_v310_1', 'expense', 25000, '2026-09-17T01:00:00Z', 'Kopi Pagi',
            'notification', 'REF-BCA-12345', 'Kopi Kenangan', 'HIGH', 'ing_event_1', '2026-09-17T01:00:00Z', '2026-09-17T01:00:00Z'
        )
        "#
    )
    .execute(&pool)
    .await
    .expect("Failed to insert transaction with ingestion signals");

    let (src, ext_ref, merch, conf, ing_id): (String, Option<String>, Option<String>, String, Option<String>) = sqlx::query_as(
        "SELECT source, external_reference, merchant, confidence, ingestion_id FROM transactions WHERE id = 'tx_v310_1'"
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to query transaction ingestion columns");

    assert_eq!(src, "notification");
    assert_eq!(ext_ref, Some("REF-BCA-12345".to_string()));
    assert_eq!(merch, Some("Kopi Kenangan".to_string()));
    assert_eq!(conf, "HIGH");
    assert_eq!(ing_id, Some("ing_event_1".to_string()));

    // 3. Test ingestion_events table
    sqlx::query(
        r#"
        INSERT INTO ingestion_events (id, user_id, source, raw_payload_hash, status, confidence, parsed_candidate, created_at, processed_at)
        VALUES ('ing_event_1', 'u_v310', 'notification', 'hash_abc123', 'auto_created', 'HIGH', '{"amount":25000}', '2026-09-17T01:00:00Z', '2026-09-17T01:00:05Z')
        "#
    )
    .execute(&pool)
    .await
    .expect("Failed to insert ingestion_event");

    // 4. Test subscription_events table
    sqlx::query(
        r#"
        INSERT INTO subscription_events (id, user_id, event_type, provider, event_id, payload, created_at)
        VALUES ('sub_evt_1', 'u_v310', 'payment.success', 'dana', 'dana_ord_99', '{"status":"SUCCESS"}', '2026-09-17T01:00:00Z')
        "#
    )
    .execute(&pool)
    .await
    .expect("Failed to insert subscription_event");

    // 5. Test device_installations table
    sqlx::query(
        r#"
        INSERT INTO device_installations (id, user_id, device_id, platform, app_version, push_token, last_active_at, created_at)
        VALUES ('dev_inst_1', 'u_v310', 'android_hw_123', 'android_webview', '1.0.0', 'push_tok_abc', '2026-09-17T01:00:00Z', '2026-09-17T01:00:00Z')
        "#
    )
    .execute(&pool)
    .await
    .expect("Failed to insert device_installation");

    // 6. Test sync_cursors table
    sqlx::query(
        r#"
        INSERT INTO sync_cursors (user_id, last_cursor, updated_at)
        VALUES ('u_v310', 42, '2026-09-17T01:00:00Z')
        "#
    )
    .execute(&pool)
    .await
    .expect("Failed to insert sync_cursor");

    let last_cursor: i64 = sqlx::query_scalar("SELECT last_cursor FROM sync_cursors WHERE user_id = 'u_v310'")
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch sync_cursor");
    assert_eq!(last_cursor, 42);
}
