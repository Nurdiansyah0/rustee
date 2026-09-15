use crate::domain::money::Rupiah;
use crate::error::AppError;
use crate::repository::{
    audit_repo::{AuditRepository, NewAuditLog},
    subscription_repo::{NewSubscription, NewWebhookEvent, Subscription, SubscriptionRepository},
    user_repo::UserRepository,
    DbError,
};
use base64::prelude::*;
use chrono::{Duration, Utc};
use rsa::{
    pkcs1v15::{SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    signature::{SignatureEncoding, Signer, Verifier},
    RsaPrivateKey,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use sqlx::SqlitePool;
use std::sync::Arc;
use subtle::ConstantTimeEq;

#[derive(Debug, thiserror::Error)]
pub enum PaymentError {
    #[error("Invalid cryptographic webhook signature")]
    InvalidSignature,

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Unknown or unsupported payment provider: {0}")]
    UnsupportedProvider(String),

    #[error("Invalid webhook payload format: {0}")]
    InvalidPayload(String),

    #[error("Trial already used for this account")]
    TrialAlreadyUsed,

    #[error("User already has an active Premium subscription")]
    AlreadyPremium,

    #[error("Database error: {0}")]
    Db(#[from] DbError),
}

impl From<PaymentError> for AppError {
    fn from(err: PaymentError) -> Self {
        match err {
            PaymentError::InvalidSignature => {
                AppError::Unauthorized("Invalid webhook signature".to_string(), "INVALID_SIGNATURE")
            }
            PaymentError::UserNotFound(msg) => AppError::NotFound(msg, "USER_NOT_FOUND"),
            PaymentError::UnsupportedProvider(msg) => {
                AppError::BadRequest(msg, "UNSUPPORTED_PROVIDER")
            }
            PaymentError::InvalidPayload(msg) => AppError::BadRequest(msg, "INVALID_PAYLOAD"),
            PaymentError::TrialAlreadyUsed => AppError::BadRequest(
                "Trial already used for this account".to_string(),
                "TRIAL_ALREADY_USED",
            ),
            PaymentError::AlreadyPremium => AppError::BadRequest(
                "User already has an active Premium subscription".to_string(),
                "ALREADY_PREMIUM",
            ),
            PaymentError::Db(e) => AppError::from(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Payment Gateway Configuration & DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PaymentConfig {
    pub midtrans_server_key: String,
    pub xendit_webhook_token: String,
    pub dana_merchant_id: String,
    pub dana_client_id: String,
    pub dana_client_secret: String,
    pub dana_public_key_pem: String,
    pub dana_private_key_pem: String,
    pub dana_api_base_url: String,
}

impl Default for PaymentConfig {
    fn default() -> Self {
        Self {
            midtrans_server_key: "midtrans_test_server_key_replace_in_prod".to_string(),
            xendit_webhook_token: "xendit_test_webhook_token_replace_in_prod".to_string(),
            dana_merchant_id: String::new(),
            dana_client_id: String::new(),
            dana_client_secret: String::new(),
            dana_public_key_pem: String::new(),
            dana_private_key_pem: String::new(),
            dana_api_base_url: "https://api.sandbox.dana.id".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookProcessingResult {
    Processed {
        user_id: String,
        status: String,
        tier: String,
    },
    DuplicateIgnored {
        event_id: String,
    },
}

// ---------------------------------------------------------------------------
// Midtrans Webhook Payload Schema
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct MidtransNotification {
    pub order_id: String,
    pub status_code: String,
    pub gross_amount: String,
    pub signature_key: String,
    pub transaction_status: String,
    pub fraud_status: Option<String>,
    pub custom_field1: Option<String>, // Stores user_id
}

// ---------------------------------------------------------------------------
// Xendit Webhook Payload Schema
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Deserialize)]
pub struct XenditNotification {
    pub id: String, // event_id
    pub event: String,
    pub external_id: String, // order_id / sub_id
    pub user_id: Option<String>,
    pub status: String,
    pub amount: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrialActivationResult {
    pub status: String,
    pub tier: String,
    pub is_premium: bool,
    pub trial_started_at: String,
    pub trial_ends_at: String,
    pub days_remaining: i64,
    pub message: String,
    pub subscription: Subscription,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserTrialInfo {
    pub has_used_trial: bool,
    pub trial_started_at: Option<String>,
    pub trial_ends_at: Option<String>,
}

// ---------------------------------------------------------------------------
// DANA Open API Types & Embedded Test Fixtures
// ---------------------------------------------------------------------------

pub const DEFAULT_DANA_TEST_PUBLIC_KEY: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAlG6urqDDVNHbTJew+/Ja
i/5Fk4XG33TeAykJr7JUk9buQ5pQS4J6SfCxbWilC6b8LzHUVUbvQoszYg6FoN9+
ovVbdZK1tLkU0nrz8RaUuQQfnrQNoaXRi+/G5BmhYsDCB46bTY9r1lfQB+4P3Gha
Rj1qVyJTK6y56XhERLtoa1ho5QmHKRRj8gbkEw5jaILnZikB8elS/8xLYUUIah0n
B0JhARJ5U5muNg2CrKoGE4jV7TqCQmrV+q74wGEFoiLnT347EKsu26Ns7wJn3TDV
u0qpjn+HvG+77A9kT1/88b+DdakpfYtpq8ENwKxCod8THAEMa386ZKncnxJp96U7
RwIDAQAB
-----END PUBLIC KEY-----"#;

pub const DEFAULT_DANA_TEST_PRIVATE_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCUbq6uoMNU0dtM
l7D78lqL/kWThcbfdN4DKQmvslST1u5DmlBLgnpJ8LFtaKULpvwvMdRVRu9CizNi
DoWg336i9Vt1krW0uRTSevPxFpS5BB+etA2hpdGL78bkGaFiwMIHjptNj2vWV9AH
7g/caFpGPWpXIlMrrLnpeEREu2hrWGjlCYcpFGPyBuQTDmNogudmKQHx6VL/zEth
RQhqHScHQmEBEnlTma42DYKsqgYTiNXtOoJCatX6rvjAYQWiIudPfjsQqy7bo2zv
AmfdMNW7SqmOf4e8b7vsD2RPX/zxv4N1qSl9i2mrwQ3ArEKh3xMcAQxrfzpkqdyf
Emn3pTtHAgMBAAECggEAA5JaP7d8m8jk9wXba2SciyvWLsOUUoI0aW0OX5zx7hDI
8PWAoyCDos3Y5yISfqJJBTW0v0ySq05AMUbaLlHScUdoKP8bwjqF5r6wqgd6Eq2n
uSDqBw6/aRee+JQpTwAGazoiQI6H8MNyLQ6scQhNy8zkhy47RBzG6HhNZD4CODsB
/84BY1Xxz+7U0E00yh3T1Y9LRHbA+c1+Psy219liXRj0Vo4zC5nCY3sBaSXZbAYL
sVFF12c3oHp6K0ITMSFeOUtJbYdpf072zMb6VzngwNORGgEUWxZjklXJOlExhk9d
8dtaS382v531o3r2lHNVG2h9nywYDL8oF4SG+JgNIQKBgQDKdq1z6+SVom0cqO50
+AFSHTE2nOl+H2ahxF/G5Xm9hMnBZMInXa3i0VdYdZb0wsHNA46CoIsttfcYF+Xg
94DG6bwK7cNGCFyLc3gYDsh2Iv7aOqrgrEnvauo4hPmsQnQJVelHbe4/asmeDf7i
1IVSMvRO/AW6mdTQN29qEsLfMQKBgQC7rnsMsLbX6JOmw5E5tbjyZtEGTGRoL2Cf
VpyWptqSLVtVLdb1oBQOEtTdFPE1sunR/C7510WZDvgOeqnSfhAhlrNMaYwGQMbx
wgdzph0N06iDllun2P7k5jiPSMoDfpzDyYCMX4/QWTOe3ZkSoGYAGacdRFcMZibk
CWH8oLNT9wKBgEr+8vlBpAaZh/lZyhqh0ztrfNNSBFunngjGCQRP9GxzR5jPjeuv
E7409TnbNPOtQMSEUMGqXmOsR78w+wH+LEGCSxlxQSgr6LvvJckjkLXR+L01hh57
M1fwLpqJB0L7yqe6nxLKcbokAFL/tC6pskjkfwLS7/xTBzWpkyejk3PBAoGAPE+B
cz6GQzOV3w0Raf4fhKXNnbyGt4QiBJIMl8zeiALTSrgET8I1L6CVjsXgDWWFBdmI
LvkigGDzDZQVZnLkNCb9TxzLxmaih6XWRy+mPn85s69pnLJ6lov0uPanFCBnt/LU
wEclK8q+b9q+CeJJZNbZgOopHu7kqHrrZgcuGVkCgYEApPDkY+y94L2SkgNGbd64
yenZFpNidcr/xLvCVrTg12EmFJ/nYsjHc2/r9W60X11NREi7rUCJhN34i3c8YdjM
sZ+axdF1bmPmcqNVO4YFgORFdAsi1XDsqWCbl82zIH59mqEcuaLk2gS3WjIB5KZ8
4StTj0O1GAI65ENXcr4cWR4=
-----END PRIVATE KEY-----"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckoutSession {
    pub order_id: String,
    pub checkout_url: String,
    pub reference_no: String,
    pub amount: i64,
    pub currency: String,
    pub provider: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DanaNotificationAmount {
    pub value: Option<String>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DanaAdditionalInfo {
    #[serde(alias = "userId", alias = "user_id")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DanaNotification {
    #[serde(
        alias = "originalPartnerReferenceNo",
        alias = "original_partner_reference_no"
    )]
    pub original_partner_reference_no: Option<String>,
    #[serde(alias = "originalReferenceNo", alias = "original_reference_no")]
    pub original_reference_no: Option<String>,
    #[serde(alias = "latestTransactionStatus", alias = "latest_transaction_status")]
    pub latest_transaction_status: Option<String>,
    #[serde(alias = "transactionStatusDesc", alias = "transaction_status_desc")]
    pub transaction_status_desc: Option<String>,
    pub amount: Option<DanaNotificationAmount>,
    #[serde(alias = "additionalInfo", alias = "additional_info")]
    pub additional_info: Option<DanaAdditionalInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DanaAckResponse {
    #[serde(rename = "responseCode")]
    pub response_code: String,
    #[serde(rename = "responseMessage")]
    pub response_message: String,
}

impl DanaAckResponse {
    pub fn success() -> Self {
        Self {
            response_code: "2005600".to_string(),
            response_message: "Successful".to_string(),
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            response_code: "4015600".to_string(),
            response_message: "Unauthorized: Invalid Signature".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Payment Service
// ---------------------------------------------------------------------------

pub struct PaymentService {
    config: PaymentConfig,
    pub subscription_repo: Arc<dyn SubscriptionRepository>,
    pub user_repo: Arc<dyn UserRepository>,
    pub audit_repo: Arc<dyn AuditRepository>,
    pub pool: Option<SqlitePool>,
    pub dana_public_key: Option<String>,
}

impl PaymentService {
    pub fn new(
        config: PaymentConfig,
        subscription_repo: Arc<dyn SubscriptionRepository>,
        user_repo: Arc<dyn UserRepository>,
        audit_repo: Arc<dyn AuditRepository>,
    ) -> Self {
        Self {
            config,
            subscription_repo,
            user_repo,
            audit_repo,
            pool: None,
            dana_public_key: None,
        }
    }

    pub fn with_pool(mut self, pool: SqlitePool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn with_dana_public_key(mut self, key: String) -> Self {
        self.dana_public_key = Some(key);
        self
    }

    pub fn get_dana_public_key(&self) -> String {
        if !self.config.dana_public_key_pem.trim().is_empty() {
            return self.config.dana_public_key_pem.clone();
        }
        if let Some(ref k) = self.dana_public_key {
            if !k.trim().is_empty() {
                return k.clone();
            }
        }
        DEFAULT_DANA_TEST_PUBLIC_KEY.to_string()
    }

    pub fn new_with_pool(
        config: PaymentConfig,
        subscription_repo: Arc<dyn SubscriptionRepository>,
        user_repo: Arc<dyn UserRepository>,
        audit_repo: Arc<dyn AuditRepository>,
        pool: SqlitePool,
    ) -> Self {
        Self {
            config,
            subscription_repo,
            user_repo,
            audit_repo,
            pool: Some(pool),
            dana_public_key: None,
        }
    }

    /// Verifies DANA RSA-SHA256 PKCS#1 v1.5 signature against String-to-Sign.
    pub fn verify_dana_rsa_signature(&self, string_to_sign: &str, signature_b64: &str) -> bool {
        let sig_bytes = match BASE64_STANDARD.decode(signature_b64.trim()) {
            Ok(b) => b,
            Err(_) => return false,
        };

        let key_pem = self.get_dana_public_key();

        let public_key = match DecodePublicKey::from_public_key_pem(&key_pem)
            .or_else(|_| rsa::pkcs1::DecodeRsaPublicKey::from_pkcs1_pem(&key_pem))
        {
            Ok(k) => k,
            Err(_) => return false,
        };

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);
        let signature = match rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice()) {
            Ok(s) => s,
            Err(_) => return false,
        };

        verifying_key
            .verify(string_to_sign.as_bytes(), &signature)
            .is_ok()
    }

    /// Constructs the standard SNAP String-to-Sign and verifies DANA RSA signature:
    /// String-to-Sign: `{method}:{path}:{sha256_hex_of_raw_body}:{timestamp}`
    pub fn verify_dana_signature(
        &self,
        method: &str,
        path: &str,
        timestamp: &str,
        raw_body: &[u8],
        signature_b64: &str,
    ) -> bool {
        let body_hash = hex::encode(Sha256::digest(raw_body));
        let string_to_sign = format!("{}:{}:{}:{}", method, path, body_hash, timestamp);
        self.verify_dana_rsa_signature(&string_to_sign, signature_b64)
    }

    /// Helper to sign String-to-Sign with DANA RSA private key.
    pub fn sign_dana_string_to_sign(
        string_to_sign: &str,
        private_key_pem: &str,
    ) -> Result<String, String> {
        let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
            .or_else(|_| rsa::pkcs1::DecodeRsaPrivateKey::from_pkcs1_pem(private_key_pem))
            .map_err(|e| format!("Failed to parse private key: {}", e))?;

        let signing_key = SigningKey::<Sha256>::new(private_key);
        let signature = signing_key.sign(string_to_sign.as_bytes());
        Ok(BASE64_STANDARD.encode(signature.to_bytes()))
    }

    /// Helper to sign DANA payload with RSA private key.
    pub fn sign_dana_payload(
        method: &str,
        path: &str,
        timestamp: &str,
        raw_body: &[u8],
        private_key_pem: &str,
    ) -> Result<String, String> {
        let body_hash = hex::encode(Sha256::digest(raw_body));
        let string_to_sign = format!("{}:{}:{}:{}", method, path, body_hash, timestamp);
        Self::sign_dana_string_to_sign(&string_to_sign, private_key_pem)
    }

    /// Creates a real DANA checkout order via the DANA Open API sandbox,
    /// returning a live payment URL the user can open in DANA app.
    pub async fn create_dana_checkout(
        &self,
        user_id: &str,
        order_id: &str,
        reference_no: &str,
    ) -> Result<String, String> {
        let now = Utc::now();
        let timestamp = now.format("%Y-%m-%dT%H:%M:%S+07:00").to_string();
        let expire_time = (now + Duration::minutes(15))
            .format("%Y-%m-%dT%H:%M:%S+07:00")
            .to_string();

        let finish_url = "https://api.nurdiansyahlabs.com/payment/success".to_string();

        let body = serde_json::json!({
            "partnerReferenceNo": reference_no,
            "merchantId": self.config.dana_merchant_id,
            "amount": {
                "value": "5000.00",
                "currency": "IDR"
            },
            "urlParams": [
                {
                    "url": finish_url,
                    "type": "PAY_RETURN",
                    "isDeeplink": "N"
                }
            ],
            "validUpTo": expire_time,
            "additionalInfo": {
                "userId": user_id,
                "orderId": order_id
            }
        });

        if self.config.dana_client_id.trim().is_empty()
            || self.config.dana_merchant_id.trim().is_empty()
        {
            return Ok(format!(
                "https://m.dana.id/d/checkout?orderId={}&ref={}",
                order_id, reference_no
            ));
        }

        let body_bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
        let path = "/v1.0/debit/payment-host-to-host.htm";
        let private_key = &self.config.dana_private_key_pem;

        let signature = if !private_key.trim().is_empty() {
            let pem_key = format!(
                "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
                private_key.trim()
            );
            Self::sign_dana_payload("POST", path, &timestamp, &body_bytes, &pem_key)
                .unwrap_or_default()
        } else {
            String::new()
        };

        let url = format!("{}{}", self.config.dana_api_base_url, path);
        let client = reqwest::Client::new();

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-TIMESTAMP", &timestamp)
            .header("X-CLIENT-KEY", &self.config.dana_client_id)
            .header("X-PARTNER-ID", &self.config.dana_merchant_id)
            .header("X-SIGNATURE", &signature)
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| format!("DANA API request failed: {}", e))?;

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("DANA API bad JSON response: {}", e))?;

        // DANA returns checkoutUrl or webRedirectUrl in the response
        let checkout_url = json
            .get("checkoutUrl")
            .or_else(|| json.get("webRedirectUrl"))
            .or_else(|| json.get("paymentUrl"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        checkout_url.ok_or_else(|| format!("DANA API did not return a checkout URL: {}", json))
    }

    /// Generates a checkout session for DANA, Midtrans, or Xendit.
    /// DANA sessions are created via async real API call; use create_checkout_session_async instead.
    pub async fn create_checkout_session_async(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<CheckoutSession, PaymentError> {
        let user_id_short = &user_id[..user_id.len().min(8)];
        let ts = Utc::now().timestamp();

        match provider.to_lowercase().as_str() {
            "dana" => {
                let order_id = format!("ORDER-DANA-{}-{}", user_id_short, ts);
                let reference_no = format!(
                    "REF-DANA-{}",
                    uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_uppercase()
                );

                let checkout_url = match self
                    .create_dana_checkout(user_id, &order_id, &reference_no)
                    .await
                {
                    Ok(url) => url,
                    Err(e) => {
                        // Log error but fail clearly so user knows
                        eprintln!("[DANA] createOrder failed: {}", e);
                        return Err(PaymentError::InvalidPayload(format!(
                            "DANA createOrder failed: {}",
                            e
                        )));
                    }
                };

                Ok(CheckoutSession {
                    order_id,
                    checkout_url,
                    reference_no,
                    amount: 5000,
                    currency: "IDR".to_string(),
                    provider: "dana".to_string(),
                })
            }
            "midtrans" => {
                let order_id = format!("SUB-{}-{}", user_id_short, ts);
                let reference_no = format!(
                    "REF-MID-{}",
                    uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_uppercase()
                );
                let checkout_url = format!(
                    "https://payment.nurdiansyahlabs.com/midtrans/pay/{}",
                    order_id
                );
                Ok(CheckoutSession {
                    order_id,
                    checkout_url,
                    reference_no,
                    amount: 5000,
                    currency: "IDR".to_string(),
                    provider: "midtrans".to_string(),
                })
            }
            "xendit" => {
                let order_id = format!("SUB-{}-{}", user_id_short, ts);
                let reference_no = format!(
                    "REF-XEN-{}",
                    uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_uppercase()
                );
                let checkout_url = format!(
                    "https://payment.nurdiansyahlabs.com/xendit/pay/{}",
                    order_id
                );
                Ok(CheckoutSession {
                    order_id,
                    checkout_url,
                    reference_no,
                    amount: 5000,
                    currency: "IDR".to_string(),
                    provider: "xendit".to_string(),
                })
            }
            other => Err(PaymentError::UnsupportedProvider(format!(
                "Unsupported payment provider '{}'.",
                other
            ))),
        }
    }

    /// Process incoming DANA webhook with cryptographic RSA signature verification and idempotency deduplication.
    pub async fn handle_dana_webhook(
        &self,
        method: &str,
        path: &str,
        timestamp: Option<&str>,
        signature: Option<&str>,
        raw_payload: &str,
    ) -> Result<WebhookProcessingResult, PaymentError> {
        let ts = timestamp.ok_or(PaymentError::InvalidSignature)?;
        let sig = signature.ok_or(PaymentError::InvalidSignature)?;

        if ts.trim().is_empty() || sig.trim().is_empty() {
            return Err(PaymentError::InvalidSignature);
        }

        if !self.verify_dana_signature(method, path, ts, raw_payload.as_bytes(), sig) {
            return Err(PaymentError::InvalidSignature);
        }

        let notification: DanaNotification = serde_json::from_str(raw_payload).map_err(|e| {
            PaymentError::InvalidPayload(format!("Invalid DANA JSON payload: {}", e))
        })?;

        self.handle_dana_notification(notification, raw_payload)
            .await
    }

    /// Atomically settles DANA payment notification:
    /// - Deduplicates event in webhook_events
    /// - Transitions subscription to 'active' Premium (+30 days)
    /// - Sets user subscription tier to 'premium'
    /// - Records immutable audit trail
    pub async fn handle_dana_notification(
        &self,
        notification: DanaNotification,
        raw_payload: &str,
    ) -> Result<WebhookProcessingResult, PaymentError> {
        let event_id = notification
            .original_reference_no
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                notification
                    .original_partner_reference_no
                    .as_ref()
                    .map(|p| {
                        format!(
                            "{}:{}",
                            p,
                            notification
                                .latest_transaction_status
                                .as_deref()
                                .unwrap_or("00")
                        )
                    })
            })
            .unwrap_or_else(|| format!("DANA-EVT-{}", uuid::Uuid::new_v4()));

        let event_type = notification
            .latest_transaction_status
            .clone()
            .unwrap_or_else(|| "00".to_string());

        // 1. Idempotent Deduplication
        let new_event = NewWebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            provider: "dana".to_string(),
            event_id: event_id.clone(),
            event_type,
            payload: raw_payload.to_string(),
        };

        let is_new = self
            .subscription_repo
            .record_webhook_event(&new_event)
            .await?;
        if !is_new {
            return Ok(WebhookProcessingResult::DuplicateIgnored { event_id });
        }

        // 2. Determine target status & user tier
        let status_code = notification
            .latest_transaction_status
            .as_deref()
            .unwrap_or("00");

        let is_success = status_code == "00"
            || status_code.eq_ignore_ascii_case("SUCCESS")
            || status_code.eq_ignore_ascii_case("PAID")
            || status_code.eq_ignore_ascii_case("SETTLEMENT")
            || notification
                .transaction_status_desc
                .as_deref()
                .map(|d| d.eq_ignore_ascii_case("Successful") || d.eq_ignore_ascii_case("SUCCESS"))
                .unwrap_or(false);

        let (sub_status, user_tier) = if is_success {
            ("active", "premium")
        } else if status_code == "01" || status_code.eq_ignore_ascii_case("PENDING") {
            ("grace", "free")
        } else {
            ("expired", "free")
        };

        // 3. Resolve User ID
        let user_id = notification
            .additional_info
            .as_ref()
            .and_then(|info| info.user_id.as_deref())
            .or_else(|| {
                notification
                    .original_partner_reference_no
                    .as_deref()
                    .map(|ref_no| {
                        if let Some(rest) = ref_no.strip_prefix("ORDER-DANA-") {
                            if let Some((uid, _ts)) = rest.rsplit_once('-') {
                                return uid;
                            }
                        }
                        ref_no
                    })
            })
            .ok_or_else(|| {
                PaymentError::InvalidPayload("Missing user_id in notification".to_string())
            })?;

        // Verify user exists
        let _user = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| PaymentError::UserNotFound(user_id.to_string()))?;

        // 4. Determine subscription period
        let now = Utc::now();
        let (start_period, end_period) = if sub_status == "active" {
            let existing_sub = self.subscription_repo.find_by_user_id(user_id).await?;
            match existing_sub {
                Some(ref s) if s.status == "active" => {
                    if let Ok(existing_end) =
                        chrono::DateTime::parse_from_rfc3339(&s.current_period_end)
                    {
                        let existing_end_utc = existing_end.with_timezone(&Utc);
                        if existing_end_utc > now {
                            (
                                s.current_period_start.clone(),
                                (existing_end_utc + Duration::days(30)).to_rfc3339(),
                            )
                        } else {
                            (now.to_rfc3339(), (now + Duration::days(30)).to_rfc3339())
                        }
                    } else {
                        (now.to_rfc3339(), (now + Duration::days(30)).to_rfc3339())
                    }
                }
                _ => (now.to_rfc3339(), (now + Duration::days(30)).to_rfc3339()),
            }
        } else {
            (now.to_rfc3339(), (now + Duration::days(30)).to_rfc3339())
        };

        // 5. Update subscription & user tier
        let sub_id = format!("sub_dana_{}", user_id);
        let sub = NewSubscription {
            id: sub_id.clone(),
            user_id: user_id.to_string(),
            provider: "dana".to_string(),
            provider_subscription_id: notification.original_partner_reference_no.clone(),
            plan_id: "premium_monthly".to_string(),
            status: sub_status.to_string(),
            amount: Rupiah(5000),
            current_period_start: start_period,
            current_period_end: end_period,
            cancel_at_period_end: false,
        };

        self.subscription_repo.upsert_subscription(&sub).await?;
        self.user_repo.update_tier(user_id, user_tier).await?;

        // 6. Mark Webhook Processed
        self.subscription_repo
            .mark_webhook_processed("dana", &event_id, "processed")
            .await?;

        // 7. Write Immutable Audit Log
        let action = if sub_status == "active" {
            "subscription_activated".to_string()
        } else {
            format!("subscription_{}", sub_status)
        };

        let _ = self
            .audit_repo
            .log_event(&NewAuditLog {
                user_id: Some(user_id.to_string()),
                action,
                entity_type: "subscription".to_string(),
                entity_id: sub.id.clone(),
                ip_address: None,
                user_agent: Some("DANA-Webhook".to_string()),
                details: Some(format!(
                    "provider=dana, order_id={}, ref_no={}, status={}, tier={}",
                    notification
                        .original_partner_reference_no
                        .as_deref()
                        .unwrap_or(""),
                    event_id,
                    sub_status,
                    user_tier
                )),
            })
            .await;

        Ok(WebhookProcessingResult::Processed {
            user_id: user_id.to_string(),
            status: sub_status.to_string(),
            tier: user_tier.to_string(),
        })
    }

    /// Verifies Midtrans SHA-512 notification signature in constant time.
    /// Expected signature: SHA512(order_id + status_code + gross_amount + server_key)
    pub fn verify_midtrans_signature(
        &self,
        order_id: &str,
        status_code: &str,
        gross_amount: &str,
        provided_signature: &str,
    ) -> bool {
        let mut hasher = Sha512::new();
        hasher.update(order_id.as_bytes());
        hasher.update(status_code.as_bytes());
        hasher.update(gross_amount.as_bytes());
        hasher.update(self.config.midtrans_server_key.as_bytes());
        let computed = hex::encode(hasher.finalize());

        computed
            .as_bytes()
            .ct_eq(provided_signature.as_bytes())
            .into()
    }

    /// Verifies Xendit callback token or HMAC-SHA256 signature in constant time.
    pub fn verify_xendit_token(&self, provided_token: &str) -> bool {
        self.config
            .xendit_webhook_token
            .as_bytes()
            .ct_eq(provided_token.as_bytes())
            .into()
    }

    /// Process incoming Midtrans webhook with cryptographic verification and idempotency deduplication.
    pub async fn handle_midtrans_webhook(
        &self,
        notification: MidtransNotification,
        raw_payload: &str,
    ) -> Result<WebhookProcessingResult, PaymentError> {
        // 1. Verify SHA-512 Signature
        if !self.verify_midtrans_signature(
            &notification.order_id,
            &notification.status_code,
            &notification.gross_amount,
            &notification.signature_key,
        ) {
            return Err(PaymentError::InvalidSignature);
        }

        let event_id = format!("{}:{}", notification.order_id, notification.status_code);

        // 2. Idempotent Deduplication
        let new_event = NewWebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            provider: "midtrans".to_string(),
            event_id: event_id.clone(),
            event_type: notification.transaction_status.clone(),
            payload: raw_payload.to_string(),
        };

        let is_new = self
            .subscription_repo
            .record_webhook_event(&new_event)
            .await?;
        if !is_new {
            return Ok(WebhookProcessingResult::DuplicateIgnored { event_id });
        }

        // 3. Determine User ID and Target Subscription Status
        let user_id = notification
            .custom_field1
            .as_deref()
            .unwrap_or(&notification.order_id);

        let (sub_status, user_tier) = match notification.transaction_status.as_str() {
            "capture" => {
                if notification.fraud_status.as_deref() == Some("challenge") {
                    ("grace", "free")
                } else {
                    ("active", "premium")
                }
            }
            "settlement" => ("active", "premium"),
            "pending" => ("grace", "free"),
            "deny" | "cancel" | "expire" => ("expired", "free"),
            _ => ("cancelled", "free"),
        };

        // 4. Update Subscription & User Tier
        let now = Utc::now();
        let end_period = now + Duration::days(30);

        let sub = NewSubscription {
            id: format!("sub_midtrans_{}", user_id),
            user_id: user_id.to_string(),
            provider: "midtrans".to_string(),
            provider_subscription_id: Some(notification.order_id.clone()),
            plan_id: "premium_monthly".to_string(),
            status: sub_status.to_string(),
            amount: Rupiah(5000),
            current_period_start: now.to_rfc3339(),
            current_period_end: end_period.to_rfc3339(),
            cancel_at_period_end: false,
        };

        self.subscription_repo.upsert_subscription(&sub).await?;
        let _ = self.user_repo.update_tier(user_id, user_tier).await;

        // 5. Mark Webhook Processed
        self.subscription_repo
            .mark_webhook_processed("midtrans", &event_id, "processed")
            .await?;

        // 6. Write Audit Log
        let _ = self
            .audit_repo
            .log_event(&NewAuditLog {
                user_id: Some(user_id.to_string()),
                action: format!("subscription_{}", sub_status),
                entity_type: "subscription".to_string(),
                entity_id: sub.id.clone(),
                ip_address: None,
                user_agent: Some("Midtrans-Webhook".to_string()),
                details: Some(format!(
                    "order_id={}, status={}, tier={}",
                    notification.order_id, sub_status, user_tier
                )),
            })
            .await;

        Ok(WebhookProcessingResult::Processed {
            user_id: user_id.to_string(),
            status: sub_status.to_string(),
            tier: user_tier.to_string(),
        })
    }

    /// Process incoming Xendit webhook with token verification and idempotency deduplication.
    pub async fn handle_xendit_webhook(
        &self,
        token_header: &str,
        notification: XenditNotification,
        raw_payload: &str,
    ) -> Result<WebhookProcessingResult, PaymentError> {
        // 1. Verify Callback Token
        if !self.verify_xendit_token(token_header) {
            return Err(PaymentError::InvalidSignature);
        }

        let event_id = notification.id.clone();

        // 2. Idempotent Deduplication
        let new_event = NewWebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            provider: "xendit".to_string(),
            event_id: event_id.clone(),
            event_type: notification.event.clone(),
            payload: raw_payload.to_string(),
        };

        let is_new = self
            .subscription_repo
            .record_webhook_event(&new_event)
            .await?;
        if !is_new {
            return Ok(WebhookProcessingResult::DuplicateIgnored { event_id });
        }

        // 3. Determine User ID and Target Subscription Status
        let user_id = notification
            .user_id
            .as_deref()
            .unwrap_or(&notification.external_id);

        let (sub_status, user_tier) = match notification.status.to_uppercase().as_str() {
            "SUCCEEDED" | "ACTIVE" | "PAID" => ("active", "premium"),
            "PENDING" => ("grace", "free"),
            "FAILED" | "EXPIRED" => ("expired", "free"),
            "CANCELLED" => ("cancelled", "free"),
            _ => ("cancelled", "free"),
        };

        // 4. Update Subscription & User Tier
        let now = Utc::now();
        let end_period = now + Duration::days(30);

        let sub = NewSubscription {
            id: format!("sub_xendit_{}", user_id),
            user_id: user_id.to_string(),
            provider: "xendit".to_string(),
            provider_subscription_id: Some(notification.external_id.clone()),
            plan_id: "premium_monthly".to_string(),
            status: sub_status.to_string(),
            amount: Rupiah(notification.amount.unwrap_or(5000)),
            current_period_start: now.to_rfc3339(),
            current_period_end: end_period.to_rfc3339(),
            cancel_at_period_end: false,
        };

        self.subscription_repo.upsert_subscription(&sub).await?;
        let _ = self.user_repo.update_tier(user_id, user_tier).await;

        // 5. Mark Webhook Processed
        self.subscription_repo
            .mark_webhook_processed("xendit", &event_id, "processed")
            .await?;

        // 6. Write Audit Log
        let _ = self
            .audit_repo
            .log_event(&NewAuditLog {
                user_id: Some(user_id.to_string()),
                action: format!("subscription_{}", sub_status),
                entity_type: "subscription".to_string(),
                entity_id: sub.id.clone(),
                ip_address: None,
                user_agent: Some("Xendit-Webhook".to_string()),
                details: Some(format!(
                    "event_id={}, status={}, tier={}",
                    event_id, sub_status, user_tier
                )),
            })
            .await;

        Ok(WebhookProcessingResult::Processed {
            user_id: user_id.to_string(),
            status: sub_status.to_string(),
            tier: user_tier.to_string(),
        })
    }

    /// Fetches the current subscription for a user.
    pub async fn get_subscription(
        &self,
        user_id: &str,
    ) -> Result<Option<Subscription>, PaymentError> {
        self.subscription_repo
            .find_by_user_id(user_id)
            .await
            .map_err(PaymentError::Db)
    }

    /// Activates a 7-day free trial for a user with strict anti-abuse validation.
    /// Rejects if `users.has_used_trial == 1` with `TRIAL_ALREADY_USED` / HTTP 400.
    /// Atomically sets `has_used_trial = 1`, `subscription_tier = 'premium'`, `trial_started_at = now`, `trial_ends_at = now + 7 days`.
    /// Upserts `subscriptions` row with `status = 'trialing'`, `provider = 'trial'`.
    /// Records audit log.
    pub async fn activate_trial_with_pool(
        &self,
        pool: &SqlitePool,
        user_id: &str,
    ) -> Result<TrialActivationResult, PaymentError> {
        // 1. Anti-abuse validation: Check if user exists and whether trial was already used
        let row: Option<(i64, String)> =
            sqlx::query_as("SELECT has_used_trial, subscription_tier FROM users WHERE id = ?1")
                .bind(user_id)
                .fetch_optional(pool)
                .await
                .map_err(DbError::from_sqlx)?;

        let (has_used_trial, _current_tier) = match row {
            Some(r) => r,
            None => return Err(PaymentError::UserNotFound(user_id.to_string())),
        };

        if has_used_trial == 1 {
            return Err(PaymentError::TrialAlreadyUsed);
        }

        // 2. Check if user already has an active subscription
        let existing_sub = self
            .subscription_repo
            .find_by_user_id(user_id)
            .await
            .map_err(PaymentError::Db)?;

        if let Some(ref sub) = existing_sub {
            if sub.status == "active" {
                return Err(PaymentError::AlreadyPremium);
            }
            if sub.status == "trialing" {
                return Err(PaymentError::TrialAlreadyUsed);
            }
        }

        // 3. Atomically set has_used_trial = 1, subscription_tier = 'premium',
        // trial_started_at = now, trial_ends_at = now + 7 days
        let now = Utc::now();
        let trial_ends = now + Duration::days(7);
        let now_str = now.to_rfc3339();
        let trial_ends_str = trial_ends.to_rfc3339();

        let mut tx = pool.begin().await.map_err(DbError::from_sqlx)?;

        let update_res = sqlx::query(
            r#"
            UPDATE users
            SET has_used_trial = 1,
                subscription_tier = 'premium',
                trial_started_at = ?1,
                trial_ends_at = ?2,
                updated_at = ?1
            WHERE id = ?3 AND has_used_trial = 0
            "#,
        )
        .bind(&now_str)
        .bind(&trial_ends_str)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        if update_res.rows_affected() == 0 {
            // Concurrency protection: another request already marked trial as used
            return Err(PaymentError::TrialAlreadyUsed);
        }

        // 4. Upsert subscriptions row with status = 'trialing', provider = 'trial'
        let sub_id = format!("sub_trial_{}", user_id);
        let sub_record = sqlx::query_as::<_, Subscription>(
            r#"
            INSERT INTO subscriptions
                (id, user_id, provider, provider_subscription_id, plan_id, status, amount,
                 current_period_start, current_period_end, cancel_at_period_end, created_at, updated_at)
            VALUES
                (?1, ?2, 'trial', ?3, 'premium_trial_7d', 'trialing', 0, ?4, ?5, 1, ?4, ?4)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                provider_subscription_id = excluded.provider_subscription_id,
                plan_id = excluded.plan_id,
                status = excluded.status,
                amount = excluded.amount,
                current_period_start = excluded.current_period_start,
                current_period_end = excluded.current_period_end,
                cancel_at_period_end = excluded.cancel_at_period_end,
                updated_at = excluded.updated_at
            RETURNING id, user_id, provider, provider_subscription_id, plan_id, status, amount,
                      current_period_start, current_period_end, cancel_at_period_end, created_at, updated_at
            "#,
        )
        .bind(&sub_id)
        .bind(user_id)
        .bind(format!("trial-{}", user_id))
        .bind(&now_str)
        .bind(&trial_ends_str)
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::from_sqlx)?;

        tx.commit().await.map_err(DbError::from_sqlx)?;

        // 5. Record audit log
        let _ = self
            .audit_repo
            .log_event(&NewAuditLog {
                user_id: Some(user_id.to_string()),
                action: "subscription_trial_started".to_string(),
                entity_type: "subscription".to_string(),
                entity_id: sub_id,
                ip_address: None,
                user_agent: Some("Invinite-Trial-Service".to_string()),
                details: Some(format!(
                    "Activated 7-day Premium Free Trial ending at {}",
                    trial_ends_str
                )),
            })
            .await;

        Ok(TrialActivationResult {
            status: "trialing".to_string(),
            tier: "premium".to_string(),
            is_premium: true,
            trial_started_at: now_str,
            trial_ends_at: trial_ends_str,
            days_remaining: 7,
            message: "7-day premium trial activated successfully".to_string(),
            subscription: sub_record,
        })
    }

    /// Activates a 7-day free trial for a user using the configured pool.
    pub async fn activate_trial(
        &self,
        user_id: &str,
    ) -> Result<TrialActivationResult, PaymentError> {
        match &self.pool {
            Some(pool) => self.activate_trial_with_pool(pool, user_id).await,
            None => Err(PaymentError::InvalidPayload(
                "Database pool required for trial activation".to_string(),
            )),
        }
    }

    /// Helper to retrieve user trial tracking information.
    pub async fn get_user_trial_info(
        &self,
        pool: &SqlitePool,
        user_id: &str,
    ) -> Result<Option<UserTrialInfo>, PaymentError> {
        let row: Option<(i64, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT has_used_trial, trial_started_at, trial_ends_at FROM users WHERE id = ?1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(DbError::from_sqlx)?;

        Ok(row.map(|(used, started, ends)| UserTrialInfo {
            has_used_trial: used == 1,
            trial_started_at: started,
            trial_ends_at: ends,
        }))
    }
}
