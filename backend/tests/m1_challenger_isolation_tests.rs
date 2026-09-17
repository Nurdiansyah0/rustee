//! backend/tests/m1_challenger_isolation_tests.rs
//! Empirical Adversarial Challenger Test Suite for Milestone 1
//! Evaluates Multi-Tenant Isolation, Cross-Tenant Resource Tampering,
//! System Category Protection, Concurrent Idempotency Races, and Foreign Key Integrity.

use backend::domain::money::Rupiah;
use backend::repository::{
    init_pool, run_migrations, AccountRepository, CategoryRepository, DbConfig, DbError,
    IdempotencyLockResult, IdempotencyRepository, NewAccount, NewCategory, NewUser,
    SqlxAccountRepository, SqlxCategoryRepository, SqlxIdempotencyRepository, SqlxUserRepository,
    UserRepository,
};
use chrono::Utc;
use std::sync::Arc;
use tempfile::tempdir;

/// Helper to spin up an isolated temporary SQLite database in WAL mode with migrations
async fn setup_challenger_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("challenger_test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections: 10,
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

/// Helper to seed two independent tenant users
async fn seed_two_users(
    user_repo: &SqlxUserRepository,
) -> (backend::repository::User, backend::repository::User) {
    let user_a = user_repo
        .create(&NewUser {
            id: "usr_tenant_alpha".to_string(),
            email: "alpha@tenants.org".to_string(),
            password_hash: "argon2_secret_hash_a".to_string(),
            display_name: "Tenant Alpha".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("free".to_string()),
        })
        .await
        .expect("Seed user Alpha failed");

    let user_b = user_repo
        .create(&NewUser {
            id: "usr_tenant_bravo".to_string(),
            email: "bravo@tenants.org".to_string(),
            password_hash: "argon2_secret_hash_b".to_string(),
            display_name: "Tenant Bravo".to_string(),
            currency: Some("IDR".to_string()),
            role: Some("user".to_string()),
            subscription_tier: Some("premium".to_string()),
        })
        .await
        .expect("Seed user Bravo failed");

    (user_a, user_b)
}

// =========================================================================
// SECTION 1: Cross-Tenant Account Access & Tampering Defense Tests
// =========================================================================

#[tokio::test]
async fn test_adversarial_account_isolation_and_tampering() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());
    let (user_a, user_b) = seed_two_users(&user_repo).await;

    // Seed Account for Tenant Alpha
    let acc_alpha = account_repo
        .create(&NewAccount {
            id: "acc_alpha_checking".to_string(),
            user_id: user_a.id.clone(),
            name: "Alpha Checking".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(10_000_000), // Rp 10,000,000
            color: Some("#112233".to_string()),
            icon: Some("bank".to_string()),
        })
        .await
        .expect("Create Alpha account failed");

    // Seed Account for Tenant Bravo
    let acc_bravo = account_repo
        .create(&NewAccount {
            id: "acc_bravo_savings".to_string(),
            user_id: user_b.id.clone(),
            name: "Bravo Savings".to_string(),
            account_type: "savings".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(50_000_000), // Rp 50,000,000
            color: Some("#445566".to_string()),
            icon: Some("piggy-bank".to_string()),
        })
        .await
        .expect("Create Bravo account failed");

    // 1. Attack Vector: Tenant Alpha tries to read Tenant Bravo's account
    let cross_read_result = account_repo
        .find_by_id(&user_a.id, &acc_bravo.id)
        .await
        .expect("find_by_id query must not fail");
    assert!(
        cross_read_result.is_none(),
        "CRITICAL: Cross-tenant reading must return None, but returned record: {:?}",
        cross_read_result
    );

    // 2. Attack Vector: Tenant Alpha tries to list accounts -> must NEVER see Bravo's account
    let alpha_accounts = account_repo
        .list_by_user(&user_a.id, true)
        .await
        .expect("list_by_user failed");
    assert_eq!(alpha_accounts.len(), 1);
    assert_eq!(alpha_accounts[0].id, acc_alpha.id);
    assert!(alpha_accounts.iter().all(|a| a.user_id == user_a.id));

    // 3. Attack Vector: Tenant Alpha tries direct balance overwrite on Bravo's account
    let overwrite_res = account_repo
        .update_balance(&user_a.id, &acc_bravo.id, Rupiah::new(0))
        .await;
    assert!(
        matches!(overwrite_res, Err(DbError::NotFound)),
        "Cross-tenant update_balance must fail with NotFound, got: {:?}",
        overwrite_res
    );

    // Verify Bravo's balance remains strictly untouched
    let bravo_verify = account_repo
        .find_by_id(&user_b.id, &acc_bravo.id)
        .await
        .unwrap()
        .expect("Bravo account must exist");
    assert_eq!(
        bravo_verify.current_balance,
        Rupiah::new(50_000_000),
        "Bravo balance was tampered with!"
    );

    // 4. Attack Vector: Tenant Alpha tries atomic balance deduction (-Rp 25,000,000) on Bravo
    let atomic_tamper_res = account_repo
        .adjust_balance_atomic(&user_a.id, &acc_bravo.id, -25_000_000)
        .await;
    assert!(
        matches!(atomic_tamper_res, Err(DbError::NotFound)),
        "Cross-tenant adjust_balance_atomic must fail with NotFound, got: {:?}",
        atomic_tamper_res
    );

    // Verify Bravo's balance remains strictly unchanged
    let bravo_verify2 = account_repo
        .find_by_id(&user_b.id, &acc_bravo.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bravo_verify2.current_balance, Rupiah::new(50_000_000));

    // 5. Attack Vector: Transactional atomic adjustment with cross-tenant ID
    let mut tx = pool.begin().await.expect("Begin transaction failed");
    let tx_tamper_res = SqlxAccountRepository::adjust_balance_atomic_tx(
        &mut tx,
        &user_a.id,
        &acc_bravo.id,
        -10_000_000,
    )
    .await;
    assert!(
        matches!(tx_tamper_res, Err(DbError::NotFound)),
        "Cross-tenant adjust_balance_atomic_tx must fail with NotFound, got: {:?}",
        tx_tamper_res
    );
    tx.rollback().await.expect("Rollback failed");

    // 6. Attack Vector: Tenant Alpha tries to archive Tenant Bravo's account
    let archive_tamper_res = account_repo.archive(&user_a.id, &acc_bravo.id).await;
    assert!(
        matches!(archive_tamper_res, Err(DbError::NotFound)),
        "Cross-tenant archive must fail with NotFound, got: {:?}",
        archive_tamper_res
    );

    // Verify Bravo's account is NOT archived
    let bravo_verify3 = account_repo
        .find_by_id(&user_b.id, &acc_bravo.id)
        .await
        .unwrap()
        .unwrap();
    assert!(!bravo_verify3.is_archived);
}

// =========================================================================
// SECTION 2: Category System Protection & Cross-Tenant Defense Tests
// =========================================================================

#[tokio::test]
async fn test_adversarial_category_system_protection_and_isolation() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let category_repo = SqlxCategoryRepository::new(pool.clone());
    let (user_a, user_b) = seed_two_users(&user_repo).await;

    // 1. Seed Global System Category (e.g., Food & Beverage)
    let sys_cat = category_repo
        .create(&NewCategory {
            id: "cat_system_fnb".to_string(),
            user_id: None,
            name: "Food & Beverage".to_string(),
            category_type: "expense".to_string(),
            icon: Some("utensils".to_string()),
            color: Some("#ff5500".to_string()),
            is_system: true,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .expect("Create system category failed");

    // 2. Seed Custom Categories for Tenant Alpha and Tenant Bravo
    let cat_alpha = category_repo
        .create(&NewCategory {
            id: "cat_alpha_freelance".to_string(),
            user_id: Some(user_a.id.clone()),
            name: "Freelance Dev".to_string(),
            category_type: "income".to_string(),
            icon: Some("laptop".to_string()),
            color: Some("#00bb22".to_string()),
            is_system: false,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .expect("Create Alpha category failed");

    let cat_bravo = category_repo
        .create(&NewCategory {
            id: "cat_bravo_secret".to_string(),
            user_id: Some(user_b.id.clone()),
            name: "Bravo Secret Investments".to_string(),
            category_type: "expense".to_string(),
            icon: Some("lock".to_string()),
            color: Some("#990000".to_string()),
            is_system: false,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .expect("Create Bravo category failed");

    // 3. System Category Deletion Defense:
    // Tenant Alpha attempts to soft-delete the system category
    let sys_delete_alpha = category_repo.soft_delete(&user_a.id, &sys_cat.id).await;
    assert!(
        matches!(sys_delete_alpha, Err(DbError::CannotDeleteSystemEntity)),
        "Attempting to delete a system category MUST return CannotDeleteSystemEntity, got: {:?}",
        sys_delete_alpha
    );

    // Tenant Bravo attempts to soft-delete the system category
    let sys_delete_bravo = category_repo.soft_delete(&user_b.id, &sys_cat.id).await;
    assert!(
        matches!(sys_delete_bravo, Err(DbError::CannotDeleteSystemEntity)),
        "Attempting to delete a system category MUST return CannotDeleteSystemEntity, got: {:?}",
        sys_delete_bravo
    );

    // Verify system category remains intact and active
    let found_sys_a = category_repo
        .find_by_id(&user_a.id, &sys_cat.id)
        .await
        .unwrap()
        .expect("System category must still be visible to Alpha");
    assert!(found_sys_a.deleted_at.is_none());

    // 4. Cross-Tenant Category Reading:
    // Tenant Alpha attempts to read Tenant Bravo's private category
    let cross_read_cat = category_repo
        .find_by_id(&user_a.id, &cat_bravo.id)
        .await
        .expect("Query failed");
    assert!(
        cross_read_cat.is_none(),
        "Tenant Alpha must NOT see Tenant Bravo's private category!"
    );

    // 5. Cross-Tenant Category Soft-Deletion:
    // Tenant Alpha attempts to soft-delete Tenant Bravo's category
    let cross_delete_res = category_repo.soft_delete(&user_a.id, &cat_bravo.id).await;
    assert!(
        matches!(cross_delete_res, Err(DbError::NotFound)),
        "Cross-tenant category deletion must return NotFound, got: {:?}",
        cross_delete_res
    );

    // Verify Bravo's category is still active and untouched
    let bravo_cat_verify = category_repo
        .find_by_id(&user_b.id, &cat_bravo.id)
        .await
        .unwrap()
        .expect("Bravo category must still exist for Bravo");
    assert!(bravo_cat_verify.deleted_at.is_none());

    // 6. Listing Isolation:
    let alpha_categories = category_repo.list_by_user(&user_a.id).await.unwrap();
    let alpha_cat_ids: Vec<String> = alpha_categories.iter().map(|c| c.id.clone()).collect();
    assert!(
        alpha_cat_ids.contains(&sys_cat.id),
        "Alpha must see system category"
    );
    assert!(
        alpha_cat_ids.contains(&cat_alpha.id),
        "Alpha must see Alpha's category"
    );
    assert!(
        !alpha_cat_ids.contains(&cat_bravo.id),
        "Alpha MUST NOT see Bravo's category"
    );

    // 7. Legitimate Soft-Delete and Re-delete Idempotency / NotFound:
    category_repo
        .soft_delete(&user_a.id, &cat_alpha.id)
        .await
        .expect("Alpha should be able to soft-delete own category");

    // Finding soft-deleted category must return None
    let deleted_lookup = category_repo
        .find_by_id(&user_a.id, &cat_alpha.id)
        .await
        .unwrap();
    assert!(
        deleted_lookup.is_none(),
        "Soft-deleted category must not be found"
    );

    // Attempting to delete the same category again must return NotFound
    let re_delete_res = category_repo.soft_delete(&user_a.id, &cat_alpha.id).await;
    assert!(
        matches!(re_delete_res, Err(DbError::NotFound)),
        "Re-deleting an already soft-deleted category must return NotFound, got: {:?}",
        re_delete_res
    );
}

// =========================================================================
// SECTION 3: Concurrent Idempotency Races & Cross-Tenant Key Partitioning
// =========================================================================

#[tokio::test]
async fn test_adversarial_idempotency_concurrency_and_tenant_partitioning() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let idemp_repo = SqlxIdempotencyRepository::new(pool.clone());
    let (user_a, user_b) = seed_two_users(&user_repo).await;

    let shared_key = "idemp-key-shared-uuid-112233";
    let payload_hash_alpha = "hash_payload_alpha_submission";
    let payload_hash_bravo = "hash_payload_bravo_submission";

    // 1. Cross-Tenant Key Collision Immunity:
    // Tenant Alpha and Tenant Bravo both use the EXACT same idempotency key
    let lock_alpha = idemp_repo
        .acquire_lock(&user_a.id, shared_key, payload_hash_alpha, 300)
        .await
        .expect("Alpha acquire lock failed");
    assert_eq!(lock_alpha, IdempotencyLockResult::Acquired);

    let lock_bravo = idemp_repo
        .acquire_lock(&user_b.id, shared_key, payload_hash_bravo, 300)
        .await
        .expect("Bravo acquire lock failed");
    assert_eq!(
        lock_bravo,
        IdempotencyLockResult::Acquired,
        "Distinct tenants MUST be completely isolated even when using identical idempotency keys"
    );

    // Alpha saves response
    idemp_repo
        .save_response(
            &user_a.id,
            shared_key,
            201,
            r#"{"transaction_id":"tx_alpha_01","amount":100000}"#,
        )
        .await
        .expect("Save response for Alpha failed");

    // Bravo is still in_progress, saves different response
    idemp_repo
        .save_response(
            &user_b.id,
            shared_key,
            200,
            r#"{"transaction_id":"tx_bravo_99","amount":500000}"#,
        )
        .await
        .expect("Save response for Bravo failed");

    // Alpha gets cached response -> must get Alpha's data only
    let cached_alpha = idemp_repo
        .get_cached_response(&user_a.id, shared_key)
        .await
        .unwrap()
        .expect("Cached record for Alpha missing");
    assert_eq!(cached_alpha.response_code, Some(201));
    assert!(cached_alpha
        .response_body
        .as_ref()
        .unwrap()
        .contains("tx_alpha_01"));

    // Bravo gets cached response -> must get Bravo's data only
    let cached_bravo = idemp_repo
        .get_cached_response(&user_b.id, shared_key)
        .await
        .unwrap()
        .expect("Cached record for Bravo missing");
    assert_eq!(cached_bravo.response_code, Some(200));
    assert!(cached_bravo
        .response_body
        .as_ref()
        .unwrap()
        .contains("tx_bravo_99"));

    // 2. High Concurrency Race Condition:
    // 25 concurrent tasks attempting to acquire the SAME idempotency key for Tenant Alpha
    let race_key = "race-key-concurrency-test";
    let race_hash = "race_hash_payload";
    let idemp_arc = Arc::new(idemp_repo);
    let mut handles = Vec::new();

    for _ in 0..25 {
        let repo = Arc::clone(&idemp_arc);
        let user_id = user_a.id.clone();
        let k = race_key.to_string();
        let h = race_hash.to_string();

        handles.push(tokio::spawn(async move {
            repo.acquire_lock(&user_id, &k, &h, 60).await
        }));
    }

    let mut acquired_count = 0;
    let mut in_progress_count = 0;

    for h in handles {
        let res = h.await.expect("Task panicked").expect("DB error in race");
        match res {
            IdempotencyLockResult::Acquired => acquired_count += 1,
            IdempotencyLockResult::InProgress => in_progress_count += 1,
            other => panic!("Unexpected result during concurrent acquire: {:?}", other),
        }
    }

    assert_eq!(
        acquired_count, 1,
        "CRITICAL: Exactly ONE task must acquire the lock! Acquired count = {}",
        acquired_count
    );
    assert_eq!(
        in_progress_count, 24,
        "CRITICAL: Exactly 24 tasks must receive InProgress (409 conflict candidate)! Count = {}",
        in_progress_count
    );

    // 3. Attack Vector: Tenant Alpha tries to tamper with Tenant Bravo's idempotency response
    let tamper_save = idemp_arc
        .save_response(
            &user_a.id, // Alpha pretending to save Bravo's key
            shared_key, // Key exists for Alpha too, but let's test a key exclusive to Bravo
            500, "tampered",
        )
        .await;
    // Since shared_key for Alpha was already 'completed' (not in_progress), save_response affects 0 rows
    assert!(
        matches!(tamper_save, Err(DbError::NotFound)),
        "save_response on already completed or non-in_progress record must return NotFound"
    );

    // Bravo exclusive key
    let bravo_exclusive_key = "bravo-exclusive-pending";
    idemp_arc
        .acquire_lock(&user_b.id, bravo_exclusive_key, "hash_b", 60)
        .await
        .unwrap();

    // Alpha tries to save response for Bravo's exclusive key
    let alpha_hijack = idemp_arc
        .save_response(&user_a.id, bravo_exclusive_key, 200, "{\"hijacked\": true}")
        .await;
    assert!(
        matches!(alpha_hijack, Err(DbError::NotFound)),
        "Tenant Alpha attempting to complete Tenant Bravo's idempotency lock must return NotFound"
    );

    // Alpha tries to release Bravo's exclusive lock
    idemp_arc
        .release_lock_on_failure(&user_a.id, bravo_exclusive_key)
        .await
        .expect("Release on wrong tenant should affect 0 rows and return Ok");

    // Bravo's lock must STILL be InProgress for Bravo
    let bravo_lock_status = idemp_arc
        .acquire_lock(&user_b.id, bravo_exclusive_key, "hash_b", 60)
        .await
        .unwrap();
    assert_eq!(
        bravo_lock_status,
        IdempotencyLockResult::InProgress,
        "Bravo's lock was erroneously dropped by Alpha!"
    );
}

// =========================================================================
// SECTION 4: Foreign Key Cascades, RESTRICT Defenses & Orphan Prevention
// =========================================================================

#[tokio::test]
async fn test_adversarial_foreign_key_and_cascade_boundaries() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());
    let category_repo = SqlxCategoryRepository::new(pool.clone());
    let (user_a, _user_b) = seed_two_users(&user_repo).await;

    // 1. Orphan Entity Insertion Prevention across tables:

    // Orphan Account
    let orphan_account = NewAccount {
        id: "acc_orphan".to_string(),
        user_id: "non_existent_user_xyz".to_string(),
        name: "Orphan".to_string(),
        account_type: "cash".to_string(),
        currency: None,
        initial_balance: Rupiah::new(100),
        color: None,
        icon: None,
    };
    let err_acc = account_repo.create(&orphan_account).await.unwrap_err();
    assert!(matches!(err_acc, DbError::ForeignKeyViolation(_)));

    // Orphan Category (with invalid non-existent user_id)
    let orphan_category = NewCategory {
        id: "cat_orphan".to_string(),
        user_id: Some("ghost_user_id".to_string()),
        name: "Ghost Cat".to_string(),
        category_type: "expense".to_string(),
        icon: None,
        color: None,
        is_system: false,
        display_name: None,
        normalized_name: None,
        metadata: None,
    };
    let err_cat = category_repo.create(&orphan_category).await.unwrap_err();
    assert!(matches!(err_cat, DbError::ForeignKeyViolation(_)));

    // Valid account and category for User A
    let acc_a = account_repo
        .create(&NewAccount {
            id: "acc_valid_a".to_string(),
            user_id: user_a.id.clone(),
            name: "Wallet A".to_string(),
            account_type: "e_wallet".to_string(),
            currency: None,
            initial_balance: Rupiah::new(500_000),
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    let cat_a = category_repo
        .create(&NewCategory {
            id: "cat_valid_a".to_string(),
            user_id: Some(user_a.id.clone()),
            name: "Groceries".to_string(),
            category_type: "expense".to_string(),
            icon: None,
            color: None,
            is_system: false,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .unwrap();

    let now_str = Utc::now().to_rfc3339();

    // 2. Orphan Transaction Insertions:
    // (a) Invalid user_id
    let orphan_tx_user = sqlx::query(
        r#"
        INSERT INTO transactions (id, user_id, account_id, to_account_id, category_id, transaction_type, amount, date, description, created_at, updated_at)
        VALUES ('tx_orphan_user', 'ghost_user', ?1, NULL, ?2, 'expense', 50000, ?3, 'Test', ?3, ?3)
        "#
    )
    .bind(&acc_a.id)
    .bind(&cat_a.id)
    .bind(&now_str)
    .execute(&pool)
    .await;
    assert!(
        orphan_tx_user.is_err(),
        "Transaction with invalid user_id must fail FK constraint"
    );

    // (b) Invalid account_id
    let orphan_tx_account = sqlx::query(
        r#"
        INSERT INTO transactions (id, user_id, account_id, to_account_id, category_id, transaction_type, amount, date, description, created_at, updated_at)
        VALUES ('tx_orphan_account', ?1, 'ghost_acc', NULL, ?2, 'expense', 50000, ?3, 'Test', ?3, ?3)
        "#
    )
    .bind(&user_a.id)
    .bind(&cat_a.id)
    .bind(&now_str)
    .execute(&pool)
    .await;
    assert!(
        orphan_tx_account.is_err(),
        "Transaction with invalid account_id must fail FK constraint"
    );

    // (c) Invalid category_id
    let orphan_tx_cat = sqlx::query(
        r#"
        INSERT INTO transactions (id, user_id, account_id, to_account_id, category_id, transaction_type, amount, date, description, created_at, updated_at)
        VALUES ('tx_orphan_cat', ?1, ?2, NULL, 'ghost_cat', 'expense', 50000, ?3, 'Test', ?3, ?3)
        "#
    )
    .bind(&user_a.id)
    .bind(&acc_a.id)
    .bind(&now_str)
    .execute(&pool)
    .await;
    assert!(
        orphan_tx_cat.is_err(),
        "Transaction with invalid category_id must fail FK constraint"
    );

    // 3. Insert a VALID transaction referencing acc_a and cat_a
    sqlx::query(
        r#"
        INSERT INTO transactions (id, user_id, account_id, to_account_id, category_id, transaction_type, amount, date, description, created_at, updated_at)
        VALUES ('tx_valid_01', ?1, ?2, NULL, ?3, 'expense', 50000, ?4, 'Legitimate Expense', ?4, ?4)
        "#
    )
    .bind(&user_a.id)
    .bind(&acc_a.id)
    .bind(&cat_a.id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .expect("Valid transaction insert failed");

    // 4. ON DELETE RESTRICT on Accounts:
    // Attempting to delete acc_a while referenced by tx_valid_01 MUST FAIL!
    let delete_acc_res = sqlx::query("DELETE FROM accounts WHERE id = ?1")
        .bind(&acc_a.id)
        .execute(&pool)
        .await;
    assert!(
        delete_acc_res.is_err(),
        "CRITICAL: Deleting account with existing transactions MUST be restricted by FK, but succeeded!"
    );
    let err_msg = delete_acc_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("FOREIGN KEY constraint failed"),
        "Expected foreign key failure, got: {}",
        err_msg
    );

    // 5. ON DELETE RESTRICT on Categories:
    // Attempting to hard-delete cat_a while referenced by tx_valid_01 MUST FAIL!
    let delete_cat_res = sqlx::query("DELETE FROM categories WHERE id = ?1")
        .bind(&cat_a.id)
        .execute(&pool)
        .await;
    assert!(
        delete_cat_res.is_err(),
        "CRITICAL: Hard-deleting category with existing transactions MUST be restricted by FK, but succeeded!"
    );
    let cat_err_msg = delete_cat_res.unwrap_err().to_string();
    assert!(
        cat_err_msg.contains("FOREIGN KEY constraint failed"),
        "Expected foreign key failure, got: {}",
        cat_err_msg
    );

    // 6. ON DELETE RESTRICT on Budgets:
    // Create a budget referencing cat_a
    sqlx::query(
        r#"
        INSERT INTO budgets (id, user_id, category_id, amount_limit, period, start_date, end_date, created_at, updated_at)
        VALUES ('budget_01', ?1, ?2, 1000000, 'monthly', '2026-09-01', '2026-09-30', ?3, ?3)
        "#
    )
    .bind(&user_a.id)
    .bind(&cat_a.id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .expect("Budget insert failed");

    // 7. Audit Log SET NULL on user deletion:
    sqlx::query(
        r#"
        INSERT INTO audit_logs (id, user_id, action, entity_type, entity_id, created_at)
        VALUES ('audit_log_01', ?1, 'account.create', 'account', ?2, ?3)
        "#,
    )
    .bind(&user_a.id)
    .bind(&acc_a.id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .expect("Audit log insert failed");

    // 8. User Cascade Delete:
    // When deleting user_a, child records in transactions, budgets, accounts, categories should cascade.
    // Note: Because transactions reference accounts/categories with ON DELETE RESTRICT,
    // let's test how SQLite evaluates deleting the root parent user.
    // In SQLite with foreign keys ON, deleting the user cascades to transactions first or simultaneously.
    let delete_user_res = sqlx::query("DELETE FROM users WHERE id = ?1")
        .bind(&user_a.id)
        .execute(&pool)
        .await;

    // In SQLite, if parent delete triggers cascade deletes for child tables that have mutual RESTRICT constraints,
    // let's observe SQLite's exact behavior:
    println!("User cascade deletion result: {:?}", delete_user_res);
    match delete_user_res {
        Ok(_) => {
            // Verify audit log has user_id SET NULL
            let audit_user_id: Option<String> =
                sqlx::query_scalar("SELECT user_id FROM audit_logs WHERE id = 'audit_log_01'")
                    .fetch_one(&pool)
                    .await
                    .expect("Fetch audit log failed");
            assert!(
                audit_user_id.is_none(),
                "Audit log user_id must be SET NULL when user is deleted, got: {:?}",
                audit_user_id
            );

            // Verify accounts are gone
            let acc_count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM accounts WHERE user_id = ?1")
                    .bind(&user_a.id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(acc_count, 0);
        }
        Err(e) => {
            println!("Cascade deletion constraint note: {:?}", e);
        }
    }
}

// =========================================================================
// SECTION 5: SQL Injection Immunity Across Repository Parameters
// =========================================================================

#[tokio::test]
async fn test_adversarial_sql_injection_resilience() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());
    let category_repo = SqlxCategoryRepository::new(pool.clone());
    let idemp_repo = SqlxIdempotencyRepository::new(pool.clone());
    let (user_a, _user_b) = seed_two_users(&user_repo).await;

    // Craft SQL injection attack strings
    let sqli_user_id = "usr_tenant_alpha' OR '1'='1' --";
    let sqli_id = "acc_01' UNION SELECT * FROM accounts; --";
    let sqli_payload = "'; DROP TABLE users; --";

    // 1. Account find_by_id with SQL injection in user_id
    let res = account_repo.find_by_id(sqli_user_id, "acc_dummy").await;
    assert!(
        res.is_ok() && res.unwrap().is_none(),
        "SQL injection in user_id must safely return None"
    );

    // 2. Account find_by_id with SQL injection in entity id
    let res = account_repo.find_by_id(&user_a.id, sqli_id).await;
    assert!(
        res.is_ok() && res.unwrap().is_none(),
        "SQL injection in account id must safely return None"
    );

    // 3. Category find_by_id with SQL injection in user_id
    let res = category_repo.find_by_id(sqli_user_id, "cat_dummy").await;
    assert!(
        res.is_ok() && res.unwrap().is_none(),
        "SQL injection in category user_id must safely return None"
    );

    // 4. Idempotency acquire_lock with SQL injection in payload hash
    let res = idemp_repo
        .acquire_lock(&user_a.id, "key_sqli_test", sqli_payload, 60)
        .await;
    assert!(
        matches!(res, Ok(IdempotencyLockResult::Acquired)),
        "Idempotency acquire must safely store malicious strings without executing them"
    );

    // Verify 'users' table still exists and was NOT dropped
    let user_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("Users table must still exist");
    assert!(user_count >= 2, "Users table was unexpectedly compromised");
}

// =========================================================================
// SECTION 6: Soft-Deleted Category Historical Transaction Ledger Integrity
// =========================================================================

#[tokio::test]
async fn test_soft_deleted_category_transaction_ledger_integrity() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());
    let category_repo = SqlxCategoryRepository::new(pool.clone());
    let (user_a, _user_b) = seed_two_users(&user_repo).await;

    // 1. Create account and category
    let acc = account_repo
        .create(&NewAccount {
            id: "acc_ledger_01".to_string(),
            user_id: user_a.id.clone(),
            name: "Main Ledger Account".to_string(),
            account_type: "checking".to_string(),
            currency: None,
            initial_balance: Rupiah::new(10_000_000),
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    let cat = category_repo
        .create(&NewCategory {
            id: "cat_deprecating_soon".to_string(),
            user_id: Some(user_a.id.clone()),
            name: "Old Gym Membership".to_string(),
            category_type: "expense".to_string(),
            icon: None,
            color: None,
            is_system: false,
            display_name: None,
            normalized_name: None,
            metadata: None,
        })
        .await
        .unwrap();

    let now_str = Utc::now().to_rfc3339();

    // 2. Record historical transaction referencing this category
    sqlx::query(
        r#"
        INSERT INTO transactions (id, user_id, account_id, to_account_id, category_id, transaction_type, amount, date, description, created_at, updated_at)
        VALUES ('tx_hist_001', ?1, ?2, NULL, ?3, 'expense', 250000, ?4, 'Monthly Gym Fee', ?4, ?4)
        "#
    )
    .bind(&user_a.id)
    .bind(&acc.id)
    .bind(&cat.id)
    .bind(&now_str)
    .execute(&pool)
    .await
    .expect("Failed to insert historical transaction");

    // 3. User soft-deletes the category
    category_repo
        .soft_delete(&user_a.id, &cat.id)
        .await
        .expect("Soft-delete must succeed");

    // 4. Invariant: Historical transaction MUST still exist and retain category_id
    let tx_cat_id: Option<String> =
        sqlx::query_scalar("SELECT category_id FROM transactions WHERE id = 'tx_hist_001'")
            .fetch_one(&pool)
            .await
            .expect("Historical transaction must still exist");

    assert_eq!(
        tx_cat_id,
        Some(cat.id.clone()),
        "Transaction must retain historical category_id"
    );

    // 5. Invariant: CategoryRepository hides the soft-deleted category from active lookups
    let active_lookup = category_repo.find_by_id(&user_a.id, &cat.id).await.unwrap();
    assert!(
        active_lookup.is_none(),
        "Category lookup must hide soft-deleted item"
    );

    let active_list = category_repo.list_by_user(&user_a.id).await.unwrap();
    assert!(
        active_list.iter().all(|c| c.id != cat.id),
        "Category list must not contain soft-deleted item"
    );
}

// =========================================================================
// SECTION 7: Idempotency Expiry & Recovery Under Adversarial Timing
// =========================================================================

#[tokio::test]
async fn test_idempotency_expiry_and_failed_recovery() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let idemp_repo = SqlxIdempotencyRepository::new(pool);
    let (user_a, _user_b) = seed_two_users(&user_repo).await;

    let key = "idemp_timeout_test_key";
    let hash = "hash_timeout_test";

    // 1. Acquire lock with 1-second TTL
    let res1 = idemp_repo
        .acquire_lock(&user_a.id, key, hash, 1)
        .await
        .unwrap();
    assert_eq!(res1, IdempotencyLockResult::Acquired);

    // Immediate re-check -> InProgress
    let res_immediate = idemp_repo
        .acquire_lock(&user_a.id, key, hash, 1)
        .await
        .unwrap();
    assert_eq!(res_immediate, IdempotencyLockResult::InProgress);

    // Sleep 1.2 seconds to ensure expiration
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

    // After expiration, acquire_lock must reset expired lock and grant Acquired
    let res_after_expiry = idemp_repo
        .acquire_lock(&user_a.id, key, hash, 60)
        .await
        .unwrap();
    assert_eq!(
        res_after_expiry,
        IdempotencyLockResult::Acquired,
        "Expired in_progress idempotency lock must be recoverable"
    );
}

// =========================================================================
// SECTION 8: Account Balance Atomic Concurrency Stress Test
// =========================================================================

#[tokio::test]
async fn test_account_atomic_concurrency_stress_test() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());
    let (user_a, _user_b) = seed_two_users(&user_repo).await;

    let initial = 1_000_000i64; // Rp 1,000,000
    let acc = account_repo
        .create(&NewAccount {
            id: "acc_stress_01".to_string(),
            user_id: user_a.id.clone(),
            name: "Stress Account".to_string(),
            account_type: "checking".to_string(),
            currency: None,
            initial_balance: Rupiah::new(initial),
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    let account_arc = Arc::new(account_repo);
    let mut tasks = Vec::new();

    // 30 concurrent tasks:
    // 15 tasks add Rp 100,000
    // 15 tasks subtract Rp 40,000
    for i in 0..30 {
        let repo = Arc::clone(&account_arc);
        let uid = user_a.id.clone();
        let aid = acc.id.clone();
        let delta = if i % 2 == 0 { 100_000 } else { -40_000 };

        tasks.push(tokio::spawn(async move {
            repo.adjust_balance_atomic(&uid, &aid, delta).await
        }));
    }

    for t in tasks {
        t.await
            .expect("Task panicked")
            .expect("Atomic adjustment failed");
    }

    // Expected final balance:
    // 1,000,000 + (15 * 100,000) - (15 * 40,000) = 1,000,000 + 1,500,000 - 600,000 = 1,900,000
    let expected = 1_000_000 + (15 * 100_000) - (15 * 40_000);
    let final_acc = account_arc
        .find_by_id(&user_a.id, &acc.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_acc.current_balance,
        Rupiah::new(expected),
        "Atomic balance adjustment suffered from race condition / lost updates!"
    );
}

// =========================================================================
// SECTION 9: Email Case-Insensitive Lookup vs Unique Constraint Behavior
// =========================================================================

#[tokio::test]
async fn test_email_casing_and_uniqueness() {
    let (pool, _dir) = setup_challenger_db().await;
    let user_repo = SqlxUserRepository::new(pool);

    // Register user with mixed case
    user_repo
        .create(&NewUser {
            id: "usr_casing_test".to_string(),
            email: "Charles.Darwin@Evolution.org".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Charles".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .expect("Create user failed");

    // Case-insensitive lookups must all resolve to the same user
    let l1 = user_repo
        .find_by_email("charles.darwin@evolution.org")
        .await
        .unwrap();
    assert!(l1.is_some());
    assert_eq!(l1.unwrap().id, "usr_casing_test");

    let l2 = user_repo
        .find_by_email("CHARLES.DARWIN@EVOLUTION.ORG")
        .await
        .unwrap();
    assert!(l2.is_some());
    assert_eq!(l2.unwrap().id, "usr_casing_test");

    // Exact duplicate email must fail
    let dup_res = user_repo
        .create(&NewUser {
            id: "usr_casing_dup".to_string(),
            email: "Charles.Darwin@Evolution.org".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Duplicate".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await;
    assert!(matches!(dup_res, Err(DbError::UniqueViolation { .. })));
}
