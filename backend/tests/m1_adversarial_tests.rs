//! Adversarial Challenger Test Suite: Currency Math & Concurrency Stress Verification
//!
//! Probes:
//! 1. Extreme boundaries (i64::MIN, i64::MAX, 0, -1, 1, boundary off-by-ones).
//! 2. Overflow detection across checked arithmetic, basis points, ratio math, and iterators.
//! 3. Serde JSON float rejection (e.g. 123.45, 0.0, -0.5, 1e5, "123", non-integers).
//! 4. format_idr edge cases and formatting integrity for extreme inputs.
//! 5. High-contention concurrent balance updates (adjust_balance_atomic) under heavy thread contention.

use backend::domain::money::Rupiah;
use backend::repository::{
    init_pool, run_migrations, AccountRepository, DbConfig, NewAccount, NewUser,
    SqlxAccountRepository, SqlxUserRepository, UserRepository,
};
use std::sync::Arc;
use tempfile::tempdir;

/// Helper to spin up a clean, isolated SQLite WAL database
async fn setup_stress_db(max_connections: u32) -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempdir().expect("Failed to create tempdir");
    let db_path = dir.path().join("stress_db.sqlite");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let config = DbConfig {
        database_url: url,
        max_connections,
        min_connections: 1,
        busy_timeout_ms: 5000,
        acquire_timeout_secs: 10,
    };

    let pool = init_pool(&config).await.expect("Failed to init pool");
    run_migrations(&pool)
        .await
        .expect("Failed to run migrations");
    (pool, dir)
}

// ===========================================================================
// 1. Extreme Boundaries & Checked Arithmetic Overflow Probing
// ===========================================================================

#[test]
fn test_adversarial_boundary_constants_and_predicates() {
    // Exact limits
    assert_eq!(Rupiah::MIN.as_i64(), i64::MIN);
    assert_eq!(Rupiah::MAX.as_i64(), i64::MAX);
    assert_eq!(Rupiah::ZERO.as_i64(), 0);

    // Negative boundary predicates
    assert!(Rupiah::MIN.is_negative());
    assert!(!Rupiah::MIN.is_positive());
    assert!(!Rupiah::MIN.is_zero());

    // Positive boundary predicates
    assert!(Rupiah::MAX.is_positive());
    assert!(!Rupiah::MAX.is_negative());
    assert!(!Rupiah::MAX.is_zero());

    // Zero predicates
    assert!(Rupiah::ZERO.is_zero());
    assert!(!Rupiah::ZERO.is_positive());
    assert!(!Rupiah::ZERO.is_negative());

    // Off-by-one near zero
    let plus_one = Rupiah::new(1);
    let minus_one = Rupiah::new(-1);
    assert!(plus_one.is_positive());
    assert!(!plus_one.is_negative());
    assert!(minus_one.is_negative());
    assert!(!minus_one.is_positive());

    // Off-by-one near boundaries
    let max_minus_one = Rupiah::new(i64::MAX - 1);
    let min_plus_one = Rupiah::new(i64::MIN + 1);
    assert!(max_minus_one < Rupiah::MAX);
    assert!(min_plus_one > Rupiah::MIN);
}

#[test]
fn test_adversarial_checked_arithmetic_overflow_detection() {
    // Addition overflow
    assert_eq!(Rupiah::MAX.checked_add(Rupiah::new(1)), None);
    assert_eq!(Rupiah::MAX.checked_add(Rupiah::new(100)), None);
    assert_eq!(Rupiah::MAX.checked_add(Rupiah::MAX), None);
    assert_eq!(
        Rupiah::new(i64::MAX - 10).checked_add(Rupiah::new(11)),
        None
    );
    assert_eq!(
        Rupiah::new(i64::MAX - 10).checked_add(Rupiah::new(10)),
        Some(Rupiah::MAX)
    );

    // Addition underflow
    assert_eq!(Rupiah::MIN.checked_add(Rupiah::new(-1)), None);
    assert_eq!(Rupiah::MIN.checked_add(Rupiah::MIN), None);
    assert_eq!(
        Rupiah::new(i64::MIN + 10).checked_add(Rupiah::new(-11)),
        None
    );
    assert_eq!(
        Rupiah::new(i64::MIN + 10).checked_add(Rupiah::new(-10)),
        Some(Rupiah::MIN)
    );

    // Subtraction overflow & underflow
    assert_eq!(Rupiah::MIN.checked_sub(Rupiah::new(1)), None);
    assert_eq!(Rupiah::MIN.checked_sub(Rupiah::MAX), None);
    assert_eq!(Rupiah::MAX.checked_sub(Rupiah::new(-1)), None);
    assert_eq!(Rupiah::MAX.checked_sub(Rupiah::MIN), None);
    assert_eq!(Rupiah::new(i64::MIN + 5).checked_sub(Rupiah::new(6)), None);
    assert_eq!(
        Rupiah::new(i64::MIN + 5).checked_sub(Rupiah::new(5)),
        Some(Rupiah::MIN)
    );

    // Subtraction of self always yields ZERO without overflow
    assert_eq!(Rupiah::MAX.checked_sub(Rupiah::MAX), Some(Rupiah::ZERO));
    assert_eq!(Rupiah::MIN.checked_sub(Rupiah::MIN), Some(Rupiah::ZERO));

    // Multiplication overflow
    assert_eq!(Rupiah::MAX.checked_mul(2), None);
    assert_eq!(Rupiah::MAX.checked_mul(-2), None);
    assert_eq!(Rupiah::MIN.checked_mul(2), None);
    assert_eq!(Rupiah::MIN.checked_mul(-1), None); // i64::MIN * -1 overflows
    assert_eq!(Rupiah::new(i64::MAX / 2 + 1).checked_mul(2), None);
    assert_eq!(
        Rupiah::new(i64::MAX / 2).checked_mul(2),
        Some(Rupiah::new((i64::MAX / 2) * 2))
    );

    // Multiplication by 0 and 1
    assert_eq!(Rupiah::MAX.checked_mul(0), Some(Rupiah::ZERO));
    assert_eq!(Rupiah::MIN.checked_mul(0), Some(Rupiah::ZERO));
    assert_eq!(Rupiah::MAX.checked_mul(1), Some(Rupiah::MAX));
    assert_eq!(Rupiah::MIN.checked_mul(1), Some(Rupiah::MIN));

    // Division edge cases
    assert_eq!(Rupiah::MAX.checked_div(0), None);
    assert_eq!(Rupiah::MIN.checked_div(0), None);
    assert_eq!(Rupiah::ZERO.checked_div(0), None);
    assert_eq!(Rupiah::MIN.checked_div(-1), None); // i64::MIN / -1 overflows
    assert_eq!(Rupiah::MAX.checked_div(1), Some(Rupiah::MAX));
    assert_eq!(Rupiah::MIN.checked_div(1), Some(Rupiah::MIN));
    assert_eq!(Rupiah::MAX.checked_div(-1), Some(Rupiah::new(-i64::MAX)));
    assert_eq!(Rupiah::ZERO.checked_div(100), Some(Rupiah::ZERO));

    // Checked div_rem edge cases
    assert_eq!(Rupiah::MAX.checked_div_rem(0), None);
    assert_eq!(Rupiah::MIN.checked_div_rem(0), None);
    assert_eq!(Rupiah::MIN.checked_div_rem(-1), None); // division overflow

    let (q, r) = Rupiah::MAX.checked_div_rem(7).unwrap();
    assert_eq!(q.as_i64() * 7 + r.as_i64(), i64::MAX);

    let (q_neg, r_neg) = Rupiah::new(-100_000).checked_div_rem(3).unwrap();
    assert_eq!(q_neg.as_i64() * 3 + r_neg.as_i64(), -100_000);

    // Absolute value and Negation on i64::MIN
    assert_eq!(Rupiah::MIN.abs(), None);
    assert_eq!(Rupiah::MIN.checked_neg(), None);
    assert_eq!(Rupiah::MAX.abs(), Some(Rupiah::MAX));
    assert_eq!(Rupiah::new(-42).abs(), Some(Rupiah::new(42)));
    assert_eq!(Rupiah::ZERO.abs(), Some(Rupiah::ZERO));
    assert_eq!(Rupiah::ZERO.checked_neg(), Some(Rupiah::ZERO));
    assert_eq!(Rupiah::MAX.checked_neg(), Some(Rupiah::new(-i64::MAX)));

    // Saturating arithmetic clamping
    assert_eq!(Rupiah::MAX.saturating_add(Rupiah::new(100)), Rupiah::MAX);
    assert_eq!(Rupiah::MIN.saturating_sub(Rupiah::new(100)), Rupiah::MIN);
    assert_eq!(Rupiah::MAX.saturating_sub(Rupiah::new(-100)), Rupiah::MAX);
    assert_eq!(Rupiah::MIN.saturating_add(Rupiah::new(-100)), Rupiah::MIN);

    // Basis points and ratios
    assert_eq!(Rupiah::MAX.checked_mul_bps(10_000), None); // intermediate mul overflows
    assert_eq!(
        Rupiah::new(1_000_000).checked_mul_bps(0),
        Some(Rupiah::ZERO)
    );
    assert_eq!(
        Rupiah::new(10_000_000).checked_mul_bps(1_100),
        Some(Rupiah::new(1_100_000))
    ); // 11% PPN
    assert_eq!(Rupiah::new(500_000).checked_mul_ratio(1, 0), None); // div by 0
    assert_eq!(Rupiah::MAX.checked_mul_ratio(2, 1), None); // mul overflow

    // Sum iterator overflow
    let list_overflow = vec![Rupiah::MAX, Rupiah::new(1)];
    assert_eq!(Rupiah::checked_sum(list_overflow), None);

    let list_underflow = vec![Rupiah::MIN, Rupiah::new(-1)];
    assert_eq!(Rupiah::checked_sum(list_underflow), None);

    let list_ok = vec![
        Rupiah::new(100_000),
        Rupiah::new(-50_000),
        Rupiah::new(250_000),
    ];
    assert_eq!(Rupiah::checked_sum(list_ok), Some(Rupiah::new(300_000)));

    // Empty list sum is ZERO
    let empty_list: Vec<Rupiah> = vec![];
    assert_eq!(Rupiah::checked_sum(empty_list), Some(Rupiah::ZERO));
}

// ===========================================================================
// 2. Serde JSON Float Rejection & Deserialization Adversarial Probing
// ===========================================================================

#[test]
fn test_adversarial_serde_float_rejection() {
    // Standard floating-point values
    assert!(serde_json::from_str::<Rupiah>("123.45").is_err());
    assert!(serde_json::from_str::<Rupiah>("0.0").is_err());
    assert!(serde_json::from_str::<Rupiah>("-0.0").is_err());
    assert!(serde_json::from_str::<Rupiah>("-0.5").is_err());
    assert!(serde_json::from_str::<Rupiah>("50000.0").is_err());
    assert!(serde_json::from_str::<Rupiah>("0.0001").is_err());

    // Scientific notation floating-point values
    assert!(serde_json::from_str::<Rupiah>("1e5").is_err());
    assert!(serde_json::from_str::<Rupiah>("1.5e3").is_err());
    assert!(serde_json::from_str::<Rupiah>("1e0").is_err());
    assert!(serde_json::from_str::<Rupiah>("-2.5e4").is_err());

    // Non-numeric JSON types
    assert!(serde_json::from_str::<Rupiah>("\"12345\"").is_err());
    assert!(serde_json::from_str::<Rupiah>("true").is_err());
    assert!(serde_json::from_str::<Rupiah>("false").is_err());
    assert!(serde_json::from_str::<Rupiah>("null").is_err());
    assert!(serde_json::from_str::<Rupiah>("[]").is_err());
    assert!(serde_json::from_str::<Rupiah>("{}").is_err());

    // Integer overflow in JSON (exceeding i64::MAX / i64::MIN)
    // 9223372036854775808 = i64::MAX + 1
    assert!(serde_json::from_str::<Rupiah>("9223372036854775808").is_err());
    // -9223372036854775809 = i64::MIN - 1
    assert!(serde_json::from_str::<Rupiah>("-9223372036854775809").is_err());

    // Valid integer JSON numbers must deserialize with exact fidelity
    assert_eq!(serde_json::from_str::<Rupiah>("0").unwrap(), Rupiah::ZERO);
    assert_eq!(serde_json::from_str::<Rupiah>("1").unwrap(), Rupiah::new(1));
    assert_eq!(
        serde_json::from_str::<Rupiah>("-1").unwrap(),
        Rupiah::new(-1)
    );
    assert_eq!(
        serde_json::from_str::<Rupiah>("9223372036854775807").unwrap(),
        Rupiah::MAX
    );
    assert_eq!(
        serde_json::from_str::<Rupiah>("-9223372036854775808").unwrap(),
        Rupiah::MIN
    );
}

// ===========================================================================
// 3. Indonesian Currency Formatting Edge Cases
// ===========================================================================

#[test]
fn test_adversarial_format_idr_edge_cases() {
    // 0 and small integers
    assert_eq!(Rupiah::new(0).format_idr(), "Rp 0");
    assert_eq!(Rupiah::new(1).format_idr(), "Rp 1");
    assert_eq!(Rupiah::new(-1).format_idr(), "-Rp 1");
    assert_eq!(Rupiah::new(10).format_idr(), "Rp 10");
    assert_eq!(Rupiah::new(-10).format_idr(), "-Rp 10");
    assert_eq!(Rupiah::new(99).format_idr(), "Rp 99");
    assert_eq!(Rupiah::new(100).format_idr(), "Rp 100");
    assert_eq!(Rupiah::new(-100).format_idr(), "-Rp 100");
    assert_eq!(Rupiah::new(999).format_idr(), "Rp 999");
    assert_eq!(Rupiah::new(-999).format_idr(), "-Rp 999");

    // Thousands transitions
    assert_eq!(Rupiah::new(1_000).format_idr(), "Rp 1.000");
    assert_eq!(Rupiah::new(-1_000).format_idr(), "-Rp 1.000");
    assert_eq!(Rupiah::new(1_001).format_idr(), "Rp 1.001");
    assert_eq!(Rupiah::new(9_999).format_idr(), "Rp 9.999");
    assert_eq!(Rupiah::new(10_000).format_idr(), "Rp 10.000");
    assert_eq!(Rupiah::new(-10_000).format_idr(), "-Rp 10.000");
    assert_eq!(Rupiah::new(99_999).format_idr(), "Rp 99.999");
    assert_eq!(Rupiah::new(100_000).format_idr(), "Rp 100.000");
    assert_eq!(Rupiah::new(-100_000).format_idr(), "-Rp 100.000");
    assert_eq!(Rupiah::new(999_999).format_idr(), "Rp 999.999");

    // Millions and Billions
    assert_eq!(Rupiah::new(1_000_000).format_idr(), "Rp 1.000.000");
    assert_eq!(Rupiah::new(-1_000_000).format_idr(), "-Rp 1.000.000");
    assert_eq!(Rupiah::new(10_000_000).format_idr(), "Rp 10.000.000");
    assert_eq!(Rupiah::new(100_000_000).format_idr(), "Rp 100.000.000");
    assert_eq!(Rupiah::new(1_000_000_000).format_idr(), "Rp 1.000.000.000");
    assert_eq!(
        Rupiah::new(-1_000_000_000).format_idr(),
        "-Rp 1.000.000.000"
    );

    // Extreme boundaries (must be panic-free and accurate)
    assert_eq!(Rupiah::MAX.format_idr(), "Rp 9.223.372.036.854.775.807");
    assert_eq!(Rupiah::MIN.format_idr(), "-Rp 9.223.372.036.854.775.808");
    assert_eq!(
        Rupiah::new(i64::MAX - 1).format_idr(),
        "Rp 9.223.372.036.854.775.806"
    );
    assert_eq!(
        Rupiah::new(i64::MIN + 1).format_idr(),
        "-Rp 9.223.372.036.854.775.807"
    );

    // Display trait formatting verification
    assert_eq!(format!("{}", Rupiah::ZERO), "Rp 0");
    assert_eq!(format!("{}", Rupiah::new(50_000)), "Rp 50.000");
    assert_eq!(format!("{}", Rupiah::new(-15_000)), "-Rp 15.000");
    assert_eq!(format!("{}", Rupiah::MAX), "Rp 9.223.372.036.854.775.807");
    assert_eq!(format!("{}", Rupiah::MIN), "-Rp 9.223.372.036.854.775.808");
}

// ===========================================================================
// 4. High-Contention Concurrency Stress Testing
// ===========================================================================

#[tokio::test]
async fn test_adversarial_concurrent_balance_updates_symmetric_stress() {
    // 5 connection pool under 100 concurrent tasks (20x oversubscription)
    let (pool, _dir) = setup_stress_db(5).await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));

    // Create user
    user_repo
        .create(&NewUser {
            id: "user_stress_1".to_string(),
            email: "stress1@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Stress User 1".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    let initial_amount = 5_000_000i64;
    let acc = account_repo
        .create(&NewAccount {
            id: "acc_stress_1".to_string(),
            user_id: "user_stress_1".to_string(),
            name: "Contention Wallet".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(initial_amount),
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    // 100 concurrent tasks:
    // 50 tasks add +50,000
    // 50 tasks subtract -50,000
    // Total net delta MUST be exactly 0.
    const TASK_COUNT: usize = 100;
    const DELTA_AMOUNT: i64 = 50_000;

    let mut tasks = Vec::with_capacity(TASK_COUNT);
    for i in 0..TASK_COUNT {
        let repo = Arc::clone(&account_repo);
        let acc_id = acc.id.clone();
        let delta = if i % 2 == 0 {
            DELTA_AMOUNT
        } else {
            -DELTA_AMOUNT
        };

        tasks.push(tokio::spawn(async move {
            repo.adjust_balance_atomic("user_stress_1", &acc_id, delta)
                .await
        }));
    }

    let mut success_count = 0;
    for task in tasks {
        let res = task.await.expect("Tokio task panicked");
        assert!(res.is_ok(), "Atomic balance adjustment failed: {:?}", res);
        success_count += 1;
    }
    assert_eq!(success_count, 100, "All 100 tasks must succeed");

    // Final balance check: zero lost updates
    let final_acc = account_repo
        .find_by_id("user_stress_1", &acc.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_acc.current_balance,
        Rupiah::new(initial_amount),
        "Symmetric concurrency must result in exact initial balance (zero lost updates)"
    );
}

#[tokio::test]
async fn test_adversarial_concurrent_balance_updates_asymmetric_stress() {
    // 100 concurrent tasks each applying a unique increment
    let (pool, _dir) = setup_stress_db(5).await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));

    user_repo
        .create(&NewUser {
            id: "user_stress_2".to_string(),
            email: "stress2@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Stress User 2".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    let initial_amount = 100_000i64;
    let acc = account_repo
        .create(&NewAccount {
            id: "acc_stress_2".to_string(),
            user_id: "user_stress_2".to_string(),
            name: "Asymmetric Wallet".to_string(),
            account_type: "savings".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(initial_amount),
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    // 100 tasks, task `i` increments by `(i + 1) * 10`
    // Sum of 1..=100 is 5050.
    // Total expected increment = 5050 * 10 = 50,500.
    const TASK_COUNT: i64 = 100;
    let expected_delta: i64 = (1..=TASK_COUNT).sum::<i64>() * 10;
    assert_eq!(expected_delta, 50_500);

    let mut tasks = Vec::with_capacity(TASK_COUNT as usize);
    for i in 1..=TASK_COUNT {
        let repo = Arc::clone(&account_repo);
        let acc_id = acc.id.clone();
        let delta = i * 10;

        tasks.push(tokio::spawn(async move {
            repo.adjust_balance_atomic("user_stress_2", &acc_id, delta)
                .await
        }));
    }

    for task in tasks {
        let res = task.await.expect("Tokio task panicked");
        assert!(res.is_ok(), "Task failed: {:?}", res);
    }

    let final_acc = account_repo
        .find_by_id("user_stress_2", &acc.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_acc.current_balance,
        Rupiah::new(initial_amount + expected_delta),
        "Final balance must match exact expected cumulative sum"
    );
}

#[tokio::test]
async fn test_adversarial_concurrent_multi_account_cross_contention() {
    // 4 accounts under simultaneous cross-cutting concurrency (25 tasks each = 100 concurrent tasks)
    let (pool, _dir) = setup_stress_db(5).await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));

    user_repo
        .create(&NewUser {
            id: "user_stress_3".to_string(),
            email: "stress3@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Multi Account User".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    let account_ids = ["acc_multi_1", "acc_multi_2", "acc_multi_3", "acc_multi_4"];
    for (idx, id) in account_ids.iter().enumerate() {
        account_repo
            .create(&NewAccount {
                id: id.to_string(),
                user_id: "user_stress_3".to_string(),
                name: format!("Wallet {}", idx),
                account_type: "checking".to_string(),
                currency: Some("IDR".to_string()),
                initial_balance: Rupiah::new(1_000_000),
                color: None,
                icon: None,
            })
            .await
            .unwrap();
    }

    // Spawn 25 tasks per account (100 total), each task adding (acc_idx + 1) * 1,000
    let mut tasks = Vec::with_capacity(100);
    for (acc_idx, id) in account_ids.iter().enumerate() {
        let delta = ((acc_idx + 1) * 1_000) as i64;
        for _ in 0..25 {
            let repo = Arc::clone(&account_repo);
            let acc_id = id.to_string();
            tasks.push(tokio::spawn(async move {
                repo.adjust_balance_atomic("user_stress_3", &acc_id, delta)
                    .await
            }));
        }
    }

    for task in tasks {
        let res = task.await.expect("Task panicked");
        assert!(res.is_ok(), "Multi-account update failed: {:?}", res);
    }

    // Verify each account isolated balance integrity
    for (acc_idx, id) in account_ids.iter().enumerate() {
        let acc = account_repo
            .find_by_id("user_stress_3", id)
            .await
            .unwrap()
            .unwrap();

        let expected_delta = 25 * ((acc_idx + 1) * 1_000) as i64;
        let expected_balance = 1_000_000 + expected_delta;
        assert_eq!(
            acc.current_balance,
            Rupiah::new(expected_balance),
            "Account {} balance mismatch: expected {}, got {}",
            id,
            expected_balance,
            acc.current_balance.as_i64()
        );
    }
}

#[tokio::test]
async fn test_adversarial_sqlite_integer_overflow_protection() {
    let (pool, _dir) = setup_stress_db(1).await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = SqlxAccountRepository::new(pool.clone());

    user_repo
        .create(&NewUser {
            id: "user_overflow".to_string(),
            email: "overflow@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Overflow User".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    let acc = account_repo
        .create(&NewAccount {
            id: "acc_overflow".to_string(),
            user_id: "user_overflow".to_string(),
            name: "Max Limit Wallet".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::MAX,
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    // Adding 1 to i64::MAX causes SQLite to convert integer to real, which SQLx rejects as a decode error
    let res = account_repo
        .adjust_balance_atomic("user_overflow", &acc.id, 1)
        .await;

    assert!(
        res.is_err(),
        "Overflowing i64 in SQLite must be rejected, got {:?}",
        res
    );
}

#[tokio::test]
async fn test_adversarial_heavy_contention_200_tasks() {
    // 200 concurrent tasks against a 5-connection pool (40x pool oversubscription)
    let (pool, _dir) = setup_stress_db(5).await;
    let user_repo = SqlxUserRepository::new(pool.clone());
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));

    user_repo
        .create(&NewUser {
            id: "user_stress_200".to_string(),
            email: "stress200@test.com".to_string(),
            password_hash: "hash".to_string(),
            display_name: "Heavy Stress User".to_string(),
            currency: None,
            role: None,
            subscription_tier: None,
        })
        .await
        .unwrap();

    let initial_balance = 10_000_000i64;
    let acc = account_repo
        .create(&NewAccount {
            id: "acc_stress_200".to_string(),
            user_id: "user_stress_200".to_string(),
            name: "High Load Wallet".to_string(),
            account_type: "checking".to_string(),
            currency: Some("IDR".to_string()),
            initial_balance: Rupiah::new(initial_balance),
            color: None,
            icon: None,
        })
        .await
        .unwrap();

    const TASK_COUNT: usize = 200;
    const DELTA_AMOUNT: i64 = 25_000;

    let mut tasks = Vec::with_capacity(TASK_COUNT);
    for i in 0..TASK_COUNT {
        let repo = Arc::clone(&account_repo);
        let acc_id = acc.id.clone();
        let delta = if i % 2 == 0 {
            DELTA_AMOUNT
        } else {
            -DELTA_AMOUNT
        };

        tasks.push(tokio::spawn(async move {
            repo.adjust_balance_atomic("user_stress_200", &acc_id, delta)
                .await
        }));
    }

    let mut completed = 0;
    for task in tasks {
        let res = task.await.expect("Tokio task panicked");
        assert!(res.is_ok(), "Task failed under heavy contention: {:?}", res);
        completed += 1;
    }
    assert_eq!(completed, 200);

    let final_acc = account_repo
        .find_by_id("user_stress_200", &acc.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_acc.current_balance,
        Rupiah::new(initial_balance),
        "Final balance must exactly match initial balance under 200 concurrent tasks"
    );
}
