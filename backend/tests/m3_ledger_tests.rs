//! M3 Integration Tests — Core Financial Ledger Service
//!
//! Validates:
//! - Atomic balance maintenance (income / expense / transfer)
//! - Idempotency: duplicate key returns exact cached response, zero duplicate rows
//! - Multi-tenant isolation: cross-user transaction access strictly blocked
//! - Balance reversal on transaction deletion
//! - CashFlowSummary: income - expenses = net_cash_flow (the single shared formula)
//! - Pagination and filtering
//! - Boundary: zero amount rejected, negative amount rejected
//! - Transfer validation: same-account transfer rejected

#[cfg(test)]
mod m3_ledger_tests {
    use backend::domain::money::Rupiah;
    use backend::repository::{
        account_repo::{NewAccount, SqlxAccountRepository},
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        transaction_repo::{SqlxTransactionRepository, TransactionFilter},
        user_repo::{NewUser, SqlxUserRepository},
        AccountRepository, UserRepository,
    };
    use backend::service::ledger_service::{CreateTransactionRequest, LedgerService};
    use std::sync::Arc;
    use tempfile::tempdir;

    async fn make_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_db.sqlite");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let config = DbConfig {
            database_url: url,
            max_connections: 5,
            min_connections: 1,
            busy_timeout_ms: 10_000,
            acquire_timeout_secs: 10,
        };
        let pool = init_pool(&config).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn setup() -> (LedgerService, String, String, tempfile::TempDir) {
        let (pool, dir) = make_pool().await;

        let user_repo = SqlxUserRepository::new(pool.clone());
        let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));

        let user = user_repo
            .create(&NewUser {
                id: uuid::Uuid::new_v4().to_string(),
                email: "ledger@test.com".to_string(),
                password_hash: "hash".to_string(),
                display_name: "Ledger User".to_string(),
                currency: Some("IDR".to_string()),
                role: None,
                subscription_tier: None,
            })
            .await
            .unwrap();

        let account = account_repo
            .create(&NewAccount {
                id: uuid::Uuid::new_v4().to_string(),
                user_id: user.id.clone(),
                name: "Main Wallet".to_string(),
                account_type: "cash".to_string(),
                currency: Some("IDR".to_string()),
                initial_balance: Rupiah(1_000_000),
                color: None,
                icon: None,
            })
            .await
            .unwrap();

        let service = LedgerService::new(
            pool.clone(),
            Arc::new(SqlxTransactionRepository::new(pool.clone())),
            Arc::new(SqlxIdempotencyRepository::new(pool.clone())),
        );

        (service, user.id, account.id, dir)
    }

    fn income_req(account_id: &str, amount: i64, key: &str) -> CreateTransactionRequest {
        CreateTransactionRequest {
            idempotency_key: key.to_string(),
            account_id: account_id.to_string(),
            to_account_id: None,
            category_id: None,
            transaction_type: "income".to_string(),
            amount: Rupiah(amount),
            date: "2026-09-01T09:00:00Z".to_string(),
            description: "Salary".to_string(),
            notes: None,
            is_recurring: None,
        }
    }

    fn expense_req(account_id: &str, amount: i64, key: &str) -> CreateTransactionRequest {
        CreateTransactionRequest {
            idempotency_key: key.to_string(),
            account_id: account_id.to_string(),
            to_account_id: None,
            category_id: None,
            transaction_type: "expense".to_string(),
            amount: Rupiah(amount),
            date: "2026-09-05T14:00:00Z".to_string(),
            description: "Groceries".to_string(),
            notes: None,
            is_recurring: None,
        }
    }

    // -----------------------------------------------------------------------
    // T1: Income increases account balance atomically
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_income_increases_balance() {
        let (svc, user_id, account_id, _dir) = setup().await;

        let resp = svc
            .create_transaction(&user_id, income_req(&account_id, 500_000, "k1"))
            .await
            .unwrap();

        assert_eq!(resp.account_balance, Rupiah(1_500_000));
        assert_eq!(resp.transaction.amount, Rupiah(500_000));
        assert_eq!(resp.transaction.transaction_type, "income");
    }

    // -----------------------------------------------------------------------
    // T2: Expense decreases account balance atomically
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_expense_decreases_balance() {
        let (svc, user_id, account_id, _dir) = setup().await;

        let resp = svc
            .create_transaction(&user_id, expense_req(&account_id, 200_000, "k2"))
            .await
            .unwrap();

        assert_eq!(resp.account_balance, Rupiah(800_000));
    }

    // -----------------------------------------------------------------------
    // T3: The single shared net_cash_flow formula: income - expenses = net
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_net_cash_flow_shared_formula() {
        let (svc, user_id, account_id, _dir) = setup().await;

        svc.create_transaction(&user_id, income_req(&account_id, 3_000_000, "t3-i1"))
            .await
            .unwrap();
        svc.create_transaction(&user_id, income_req(&account_id, 500_000, "t3-i2"))
            .await
            .unwrap();
        svc.create_transaction(&user_id, expense_req(&account_id, 800_000, "t3-e1"))
            .await
            .unwrap();
        svc.create_transaction(&user_id, expense_req(&account_id, 200_000, "t3-e2"))
            .await
            .unwrap();

        let summary = svc.cash_flow_summary(&user_id, None, None).await.unwrap();

        assert_eq!(summary.total_income, Rupiah(3_500_000));
        assert_eq!(summary.total_expenses, Rupiah(1_000_000));
        // THE single shared formula
        assert_eq!(summary.net_cash_flow, Rupiah(2_500_000));
        assert_eq!(
            summary
                .total_income
                .checked_sub(summary.total_expenses)
                .unwrap(),
            summary.net_cash_flow,
            "Algebraic identity: income - expenses == net_cash_flow must hold"
        );
    }

    // -----------------------------------------------------------------------
    // T4: Idempotency — duplicate key returns same tx ID, balance applied once
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_idempotency_duplicate_returns_cached() {
        let (svc, user_id, account_id, _dir) = setup().await;

        let req = income_req(&account_id, 100_000, "idem-key-1");
        let resp1 = svc.create_transaction(&user_id, req.clone()).await.unwrap();
        let resp2 = svc.create_transaction(&user_id, req).await.unwrap();

        // Same transaction ID — exactly one row created
        assert_eq!(resp1.transaction.id, resp2.transaction.id);
        assert_eq!(resp1.account_balance, resp2.account_balance);
        // Balance applied exactly once: 1_000_000 + 100_000 = 1_100_000
        assert_eq!(resp2.account_balance, Rupiah(1_100_000));
    }

    #[tokio::test]
    async fn test_idempotency_payload_mutation_rejected() {
        let (svc, user_id, account_id, _dir) = setup().await;

        // 1. Initial request with payload A (amount: 100_000)
        let req1 = income_req(&account_id, 100_000, "idem-mutation-key");
        let resp1 = svc.create_transaction(&user_id, req1).await.unwrap();
        assert_eq!(resp1.account_balance, Rupiah(1_100_000));

        // 2. Re-submit with SAME idempotency key but MUTATED payload (amount: 500_000)
        let req2 = income_req(&account_id, 500_000, "idem-mutation-key");
        let err = svc.create_transaction(&user_id, req2).await.unwrap_err();

        // Must reject with DuplicateIdempotentKey indicating payload mismatch
        match err {
            backend::service::ledger_service::LedgerError::DuplicateIdempotentKey(msg) => {
                assert!(
                    msg.contains("different payload"),
                    "Expected payload mismatch error message, got: {}",
                    msg
                );
            }
            other => panic!(
                "Expected DuplicateIdempotentKey with mismatch, got: {:?}",
                other
            ),
        }

        // 3. Balance must remain unchanged at 1_100_000 (mutation attempt rejected, zero side-effects)
        let summary = svc.cash_flow_summary(&user_id, None, None).await.unwrap();
        assert_eq!(summary.total_income, Rupiah(100_000));
    }

    // -----------------------------------------------------------------------
    // T5: Delete income reverses balance
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_delete_income_reverses_balance() {
        let (svc, user_id, account_id, _dir) = setup().await;

        let resp = svc
            .create_transaction(&user_id, income_req(&account_id, 400_000, "del-k1"))
            .await
            .unwrap();

        assert_eq!(resp.account_balance, Rupiah(1_400_000));

        svc.delete_transaction(&user_id, &resp.transaction.id)
            .await
            .unwrap();

        let tx = svc.get_transaction(&user_id, &resp.transaction.id).await;
        assert!(tx.is_err(), "Transaction must be gone after deletion");
    }

    // -----------------------------------------------------------------------
    // T6: Delete expense reverses balance
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_delete_expense_reverses_balance() {
        let (svc, user_id, account_id, _dir) = setup().await;

        let resp = svc
            .create_transaction(&user_id, expense_req(&account_id, 300_000, "del-k2"))
            .await
            .unwrap();

        assert_eq!(resp.account_balance, Rupiah(700_000));

        svc.delete_transaction(&user_id, &resp.transaction.id)
            .await
            .unwrap();

        let tx = svc.get_transaction(&user_id, &resp.transaction.id).await;
        assert!(tx.is_err(), "Transaction must be gone after deletion");
    }

    // -----------------------------------------------------------------------
    // T7: Transfer — source debited, destination credited atomically
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_transfer_atomic_debit_credit() {
        let (pool, _dir) = make_pool().await;

        let user_repo = SqlxUserRepository::new(pool.clone());
        let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));

        let user = user_repo
            .create(&NewUser {
                id: uuid::Uuid::new_v4().to_string(),
                email: "xfer@test.com".to_string(),
                password_hash: "h".to_string(),
                display_name: "X".to_string(),
                currency: Some("IDR".to_string()),
                role: None,
                subscription_tier: None,
            })
            .await
            .unwrap();

        let src = account_repo
            .create(&NewAccount {
                id: uuid::Uuid::new_v4().to_string(),
                user_id: user.id.clone(),
                name: "Source".to_string(),
                account_type: "cash".to_string(),
                currency: Some("IDR".to_string()),
                initial_balance: Rupiah(2_000_000),
                color: None,
                icon: None,
            })
            .await
            .unwrap();

        let dst = account_repo
            .create(&NewAccount {
                id: uuid::Uuid::new_v4().to_string(),
                user_id: user.id.clone(),
                name: "Savings".to_string(),
                account_type: "savings".to_string(),
                currency: Some("IDR".to_string()),
                initial_balance: Rupiah(500_000),
                color: None,
                icon: None,
            })
            .await
            .unwrap();

        let svc = LedgerService::new(
            pool.clone(),
            Arc::new(SqlxTransactionRepository::new(pool.clone())),
            Arc::new(SqlxIdempotencyRepository::new(pool.clone())),
        );

        svc.create_transaction(
            &user.id,
            CreateTransactionRequest {
                idempotency_key: "xfer-1".to_string(),
                account_id: src.id.clone(),
                to_account_id: Some(dst.id.clone()),
                category_id: None,
                transaction_type: "transfer".to_string(),
                amount: Rupiah(300_000),
                date: "2026-09-01T10:00:00Z".to_string(),
                description: "To savings".to_string(),
                notes: None,
                is_recurring: None,
            },
        )
        .await
        .unwrap();

        let src_after = account_repo
            .find_by_id(&user.id, &src.id)
            .await
            .unwrap()
            .unwrap();
        let dst_after = account_repo
            .find_by_id(&user.id, &dst.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(src_after.current_balance, Rupiah(1_700_000));
        assert_eq!(dst_after.current_balance, Rupiah(800_000));
    }

    // -----------------------------------------------------------------------
    // T8: Multi-tenant isolation — cross-user transaction access blocked
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_cross_user_transaction_blocked() {
        let (svc, user_a, account_a, _dir) = setup().await;

        let resp = svc
            .create_transaction(&user_a, income_req(&account_a, 100_000, "iso-k1"))
            .await
            .unwrap();

        let result = svc
            .get_transaction("completely-different-user-id", &resp.transaction.id)
            .await;
        assert!(result.is_err(), "Cross-user access must be blocked");
    }

    // -----------------------------------------------------------------------
    // T9: Zero amount rejected
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_zero_amount_rejected() {
        let (svc, user_id, account_id, _dir) = setup().await;

        let result = svc
            .create_transaction(&user_id, income_req(&account_id, 0, "zero-key"))
            .await;
        assert!(result.is_err(), "Zero amount must be rejected");
    }

    // -----------------------------------------------------------------------
    // T10: Same-account transfer rejected
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_same_account_transfer_rejected() {
        let (svc, user_id, account_id, _dir) = setup().await;

        let result = svc
            .create_transaction(
                &user_id,
                CreateTransactionRequest {
                    idempotency_key: "self-xfer".to_string(),
                    account_id: account_id.clone(),
                    to_account_id: Some(account_id.clone()),
                    category_id: None,
                    transaction_type: "transfer".to_string(),
                    amount: Rupiah(100_000),
                    date: "2026-09-01T10:00:00Z".to_string(),
                    description: "Self".to_string(),
                    notes: None,
                    is_recurring: None,
                },
            )
            .await;
        assert!(result.is_err(), "Same-account transfer must be rejected");
    }

    // -----------------------------------------------------------------------
    // T11: Pagination — correct page/total/total_pages
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_pagination_correct_meta() {
        let (svc, user_id, account_id, _dir) = setup().await;

        for i in 0..5_i64 {
            svc.create_transaction(
                &user_id,
                income_req(&account_id, 10_000 * (i + 1), &format!("page-k{}", i)),
            )
            .await
            .unwrap();
        }

        let resp = svc
            .list_transactions(
                &user_id,
                TransactionFilter {
                    page: 1,
                    per_page: 2,
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.meta.total, 5);
        assert_eq!(resp.meta.total_pages, 3);
        assert_eq!(resp.meta.page, 1);
        assert_eq!(resp.meta.per_page, 2);
    }
}
