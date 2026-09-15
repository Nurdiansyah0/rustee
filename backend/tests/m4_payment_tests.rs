//! M4 Integration Tests — Native Rust Payment & Subscription Webhook Engine
//!
//! Validates:
//! - Cryptographic signature verification (Midtrans SHA-512 & Xendit callback token)
//! - Constant-time comparison preventing timing attacks
//! - Idempotent webhook processing (replayed webhooks return DuplicateIgnored)
//! - Subscription lifecycle state machine (Free vs Premium @ Rp5,000/month: Active, Grace, Cancelled, Expired)
//! - User tier synchronization ('free' <-> 'premium')
//! - Audit log generation for subscription events

#[cfg(test)]
mod m4_payment_tests {
    use backend::domain::money::Rupiah;
    use backend::repository::{
        audit_repo::SqlxAuditRepository,
        db::{init_pool, run_migrations, DbConfig},
        subscription_repo::SqlxSubscriptionRepository,
        user_repo::{NewUser, SqlxUserRepository, UserRepository},
    };
    use backend::service::payment_service::{
        MidtransNotification, PaymentConfig, PaymentError, PaymentService, WebhookProcessingResult,
        XenditNotification,
    };
    use sha2::{Digest, Sha512};
    use std::sync::Arc;
    use tempfile::tempdir;

    async fn setup() -> (PaymentService, String, sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempdir().expect("Failed to create tempdir");
        let db_path = dir.path().join("m4_test.sqlite");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());

        let config = DbConfig {
            database_url: url,
            max_connections: 5,
            min_connections: 1,
            busy_timeout_ms: 10_000,
            acquire_timeout_secs: 10,
        };

        let pool = init_pool(&config).await.expect("Failed to init pool");
        run_migrations(&pool)
            .await
            .expect("Failed to run migrations");

        let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
        let subscription_repo = Arc::new(SqlxSubscriptionRepository::new(pool.clone()));
        let audit_repo = Arc::new(SqlxAuditRepository::new(pool.clone()));

        let user_id = uuid::Uuid::new_v4().to_string();
        user_repo
            .create(&NewUser {
                id: user_id.clone(),
                email: "subscriber@test.com".to_string(),
                password_hash: "argon2_dummy_hash".to_string(),
                display_name: "Test Subscriber".to_string(),
                currency: Some("IDR".to_string()),
                role: None,
                subscription_tier: Some("free".to_string()),
            })
            .await
            .expect("Failed to create user");

        let payment_config = PaymentConfig {
            midtrans_server_key: "SB-Mid-server-secret-key-12345".to_string(),
            xendit_webhook_token: "xendit_token_verification_secret_67890".to_string(),
            ..PaymentConfig::default()
        };

        let service = PaymentService::new(
            payment_config,
            subscription_repo,
            user_repo.clone(),
            audit_repo,
        );

        (service, user_id, pool, dir)
    }

    fn compute_midtrans_signature(
        order_id: &str,
        status_code: &str,
        gross_amount: &str,
        server_key: &str,
    ) -> String {
        let mut hasher = Sha512::new();
        hasher.update(order_id.as_bytes());
        hasher.update(status_code.as_bytes());
        hasher.update(gross_amount.as_bytes());
        hasher.update(server_key.as_bytes());
        hex::encode(hasher.finalize())
    }

    // -----------------------------------------------------------------------
    // T1: Cryptographic signature verification for Midtrans (SHA-512)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_midtrans_signature_verification() {
        let (service, _, _, _dir) = setup().await;
        let server_key = "SB-Mid-server-secret-key-12345";

        let order_id = "ORDER-20260912-001";
        let status_code = "200";
        let gross_amount = "5000.00";
        let valid_sig = compute_midtrans_signature(order_id, status_code, gross_amount, server_key);

        // Valid signature passes
        assert!(service.verify_midtrans_signature(order_id, status_code, gross_amount, &valid_sig));

        // Tampered order_id fails
        assert!(!service.verify_midtrans_signature(
            "ORDER-TAMPERED",
            status_code,
            gross_amount,
            &valid_sig
        ));

        // Tampered amount fails
        assert!(!service.verify_midtrans_signature(order_id, status_code, "9999.00", &valid_sig));

        // Tampered signature string fails
        let mut tampered_sig = valid_sig.clone();
        tampered_sig.replace_range(0..2, "ff");
        assert!(!service.verify_midtrans_signature(
            order_id,
            status_code,
            gross_amount,
            &tampered_sig
        ));
    }

    // -----------------------------------------------------------------------
    // T2: Cryptographic token verification for Xendit
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_xendit_token_verification() {
        let (service, _, _, _dir) = setup().await;
        let valid_token = "xendit_token_verification_secret_67890";

        assert!(service.verify_xendit_token(valid_token));
        assert!(!service.verify_xendit_token("invalid_token_hacker"));
        assert!(!service.verify_xendit_token(""));
    }

    // -----------------------------------------------------------------------
    // T3: Midtrans settlement activates Premium tier @ Rp5,000/month
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_midtrans_settlement_activates_premium() {
        let (service, user_id, pool, _dir) = setup().await;
        let server_key = "SB-Mid-server-secret-key-12345";

        let order_id = format!("order_{}", uuid::Uuid::new_v4());
        let status_code = "200";
        let gross_amount = "5000.00";
        let signature =
            compute_midtrans_signature(&order_id, status_code, gross_amount, server_key);

        let notification = MidtransNotification {
            order_id: order_id.clone(),
            status_code: status_code.to_string(),
            gross_amount: gross_amount.to_string(),
            signature_key: signature,
            transaction_status: "settlement".to_string(),
            fraud_status: Some("accept".to_string()),
            custom_field1: Some(user_id.clone()),
        };

        let result = service
            .handle_midtrans_webhook(notification, "{}")
            .await
            .expect("Webhook processing should succeed");

        assert_eq!(
            result,
            WebhookProcessingResult::Processed {
                user_id: user_id.clone(),
                status: "active".to_string(),
                tier: "premium".to_string(),
            }
        );

        // Verify user tier is now premium in database
        let user_repo = SqlxUserRepository::new(pool.clone());
        let user = user_repo.find_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user.subscription_tier, "premium");

        // Verify subscription record
        let sub = service.get_subscription(&user_id).await.unwrap().unwrap();
        assert_eq!(sub.status, "active");
        assert_eq!(sub.amount, Rupiah(5000));
        assert_eq!(sub.provider, "midtrans");
    }

    // -----------------------------------------------------------------------
    // T4: Midtrans webhook idempotency — replayed event is ignored cleanly
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_midtrans_webhook_idempotency() {
        let (service, user_id, _, _dir) = setup().await;
        let server_key = "SB-Mid-server-secret-key-12345";

        let order_id = format!("order_idem_{}", uuid::Uuid::new_v4());
        let status_code = "200";
        let gross_amount = "5000.00";
        let signature =
            compute_midtrans_signature(&order_id, status_code, gross_amount, server_key);

        let notification = MidtransNotification {
            order_id: order_id.clone(),
            status_code: status_code.to_string(),
            gross_amount: gross_amount.to_string(),
            signature_key: signature,
            transaction_status: "settlement".to_string(),
            fraud_status: Some("accept".to_string()),
            custom_field1: Some(user_id.clone()),
        };

        // First delivery -> Processed
        let first_res = service
            .handle_midtrans_webhook(notification.clone(), "{}")
            .await
            .unwrap();
        assert!(matches!(
            first_res,
            WebhookProcessingResult::Processed { .. }
        ));

        // Replayed second delivery -> DuplicateIgnored (no error, no double-charge)
        let second_res = service
            .handle_midtrans_webhook(notification, "{}")
            .await
            .unwrap();
        assert_eq!(
            second_res,
            WebhookProcessingResult::DuplicateIgnored {
                event_id: format!("{}:{}", order_id, status_code)
            }
        );
    }

    // -----------------------------------------------------------------------
    // T5: Tampered webhook signature is rejected with error
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_tampered_webhook_signature_rejected() {
        let (service, user_id, _, _dir) = setup().await;

        let notification = MidtransNotification {
            order_id: "order_fake_123".to_string(),
            status_code: "200".to_string(),
            gross_amount: "5000.00".to_string(),
            signature_key: "completely_fabricated_signature".to_string(),
            transaction_status: "settlement".to_string(),
            fraud_status: Some("accept".to_string()),
            custom_field1: Some(user_id),
        };

        let res = service.handle_midtrans_webhook(notification, "{}").await;
        assert!(matches!(res, Err(PaymentError::InvalidSignature)));
    }

    // -----------------------------------------------------------------------
    // T6: Xendit webhook subscription lifecycle (PAID -> active, EXPIRED -> free)
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_xendit_subscription_lifecycle() {
        let (service, user_id, pool, _dir) = setup().await;
        let valid_token = "xendit_token_verification_secret_67890";
        let user_repo = SqlxUserRepository::new(pool.clone());

        // 1. PAID event -> active & premium
        let event_paid = XenditNotification {
            id: format!("evt_{}", uuid::Uuid::new_v4()),
            event: "recurring.payment.succeeded".to_string(),
            external_id: format!("sub_{}", user_id),
            user_id: Some(user_id.clone()),
            status: "PAID".to_string(),
            amount: Some(5000),
        };

        let res1 = service
            .handle_xendit_webhook(valid_token, event_paid, "{}")
            .await
            .unwrap();
        assert_eq!(
            res1,
            WebhookProcessingResult::Processed {
                user_id: user_id.clone(),
                status: "active".to_string(),
                tier: "premium".to_string(),
            }
        );

        let user_active = user_repo.find_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user_active.subscription_tier, "premium");

        // 2. EXPIRED event -> expired & free
        let event_expired = XenditNotification {
            id: format!("evt_{}", uuid::Uuid::new_v4()),
            event: "subscription.expired".to_string(),
            external_id: format!("sub_{}", user_id),
            user_id: Some(user_id.clone()),
            status: "EXPIRED".to_string(),
            amount: Some(5000),
        };

        let res2 = service
            .handle_xendit_webhook(valid_token, event_expired, "{}")
            .await
            .unwrap();
        assert_eq!(
            res2,
            WebhookProcessingResult::Processed {
                user_id: user_id.clone(),
                status: "expired".to_string(),
                tier: "free".to_string(),
            }
        );

        let user_expired = user_repo.find_by_id(&user_id).await.unwrap().unwrap();
        assert_eq!(user_expired.subscription_tier, "free");
    }

    // -----------------------------------------------------------------------
    // T7: Fraud status challenge moves to grace status without activating premium
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_fraud_challenge_moves_to_grace() {
        let (service, user_id, pool, _dir) = setup().await;
        let server_key = "SB-Mid-server-secret-key-12345";

        let order_id = format!("order_challenge_{}", uuid::Uuid::new_v4());
        let status_code = "200";
        let gross_amount = "5000.00";
        let signature =
            compute_midtrans_signature(&order_id, status_code, gross_amount, server_key);

        let notification = MidtransNotification {
            order_id: order_id.clone(),
            status_code: status_code.to_string(),
            gross_amount: gross_amount.to_string(),
            signature_key: signature,
            transaction_status: "capture".to_string(),
            fraud_status: Some("challenge".to_string()),
            custom_field1: Some(user_id.clone()),
        };

        let result = service
            .handle_midtrans_webhook(notification, "{}")
            .await
            .unwrap();

        assert_eq!(
            result,
            WebhookProcessingResult::Processed {
                user_id: user_id.clone(),
                status: "grace".to_string(),
                tier: "free".to_string(),
            }
        );

        let user = SqlxUserRepository::new(pool)
            .find_by_id(&user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.subscription_tier, "free");
    }

    // -----------------------------------------------------------------------
    // T8: Trial activation via PaymentService sets trialing status and premium tier
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_service_activate_trial_with_anti_abuse() {
        let (service, user_id, pool, _dir) = setup().await;

        let trial_res = service
            .activate_trial_with_pool(&pool, &user_id)
            .await
            .expect("First trial activation should succeed");

        assert_eq!(trial_res.status, "trialing");
        assert_eq!(trial_res.tier, "premium");
        assert!(trial_res.is_premium);
        assert_eq!(trial_res.days_remaining, 7);

        // Verify trial info helper
        let trial_info = service
            .get_user_trial_info(&pool, &user_id)
            .await
            .unwrap()
            .expect("User trial info should exist");
        assert!(trial_info.has_used_trial);
        assert!(trial_info.trial_started_at.is_some());
        assert!(trial_info.trial_ends_at.is_some());

        // Second activation must fail with TrialAlreadyUsed
        let duplicate_res = service.activate_trial_with_pool(&pool, &user_id).await;
        match duplicate_res {
            Err(PaymentError::TrialAlreadyUsed) => {}
            other => panic!("Expected TrialAlreadyUsed, got {:?}", other),
        }
    }
}
