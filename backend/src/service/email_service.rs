//! Enterprise Email Service for FinRep Platform
//! Supports transactional email delivery (Account Recovery, Notifications, Invoices)
//! with multi-sender capability (support@finrep.com, no-reply@finrep.com, recovery@finrep.com).

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub from: String,
    pub from_name: String,
    pub to: String,
    pub reply_to: Option<String>,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub default_sender: String,
    pub default_sender_name: String,
    pub support_email: String,
    pub app_url: String,
    pub is_production: bool,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            default_sender: "no-reply@finrep.com".to_string(),
            default_sender_name: "FinRep Platform".to_string(),
            support_email: "support@finrep.com".to_string(),
            app_url: "http://127.0.0.1:8080".to_string(),
            is_production: false,
        }
    }
}

impl EmailConfig {
    pub fn from_env() -> Self {
        let default_sender = std::env::var("MAIL_FROM")
            .unwrap_or_else(|_| "no-reply@finrep.com".to_string());
        let default_sender_name = std::env::var("MAIL_FROM_NAME")
            .unwrap_or_else(|_| "FinRep Platform".to_string());
        let support_email = std::env::var("MAIL_SUPPORT")
            .unwrap_or_else(|_| "support@finrep.com".to_string());
        let app_url = std::env::var("APP_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let is_production = std::env::var("APP_ENV")
            .map(|v| v.to_lowercase() == "production")
            .unwrap_or(false);

        Self {
            default_sender,
            default_sender_name,
            support_email,
            app_url,
            is_production,
        }
    }
}

/// Enterprise Email Service providing template rendering and delivery
#[derive(Clone)]
pub struct EmailService {
    config: EmailConfig,
    /// In-memory queue for testing and verification
    sent_mailbox: Arc<Mutex<Vec<EmailMessage>>>,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            config,
            sent_mailbox: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(EmailConfig::from_env())
    }

    /// Retrieve the last sent emails (useful for testing or local inspection)
    pub fn get_sent_messages(&self) -> Vec<EmailMessage> {
        self.sent_mailbox.lock().unwrap().clone()
    }

    /// Clears the sent messages buffer
    pub fn clear_sent_messages(&self) {
        self.sent_mailbox.lock().unwrap().clear();
    }

    /// Generates and sends a Password Recovery email
    pub async fn send_password_recovery(
        &self,
        recipient_email: &str,
        recipient_name: &str,
        reset_token: &str,
    ) -> Result<(), String> {
        let reset_url = format!("{}/?mode=reset&token={}", self.config.app_url, reset_token);

        let subject = "FinRep — Pemulihan Kata Sandi Akun Anda".to_string();

        let text_body = format!(
            "Halo {},\n\n\
            Kami menerima permintaan untuk mengatur ulang kata sandi akun FinRep Anda.\n\n\
            Untuk membuat kata sandi baru, buka tautan berikut di browser Anda:\n\
            {}\n\n\
            Tautan ini berlaku selama 1 jam.\n\
            Jika Anda tidak merasa meminta pengaturan ulang kata sandi, abaikan pesan ini. Akun Anda tetap aman.\n\n\
            Salam,\n\
            Tim FinRep\n\
            Bantuan: {}",
            recipient_name, reset_url, self.config.support_email
        );

        let html_body = format!(
            r#"<!DOCTYPE html>
<html lang="id">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Pemulihan Kata Sandi FinRep</title>
</head>
<body style="margin: 0; padding: 0; background-color: #f1f5f9; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; color: #0f172a;">
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background-color: #f1f5f9; padding: 40px 16px;">
    <tr>
      <td align="center">
        <table role="presentation" width="100%" max-width="520" cellspacing="0" cellpadding="0" border="0" style="max-width: 520px; background-color: #ffffff; border-radius: 16px; border: 1px solid #e2e8f0; overflow: hidden; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.05);">
          <!-- Header -->
          <tr>
            <td style="background-color: #0f172a; padding: 28px 32px; text-align: left;">
              <span style="font-size: 20px; font-weight: 800; color: #ffffff; letter-spacing: -0.5px;">FinRep</span>
              <span style="font-size: 11px; font-weight: 700; color: #10b981; background-color: rgba(16, 185, 129, 0.15); border: 1px solid rgba(16, 185, 129, 0.3); padding: 3px 8px; border-radius: 6px; margin-left: 8px;">Security</span>
            </td>
          </tr>
          <!-- Body -->
          <tr>
            <td style="padding: 32px;">
              <h1 style="font-size: 18px; font-weight: 800; color: #0f172a; margin-top: 0; margin-bottom: 12px;">Pemulihan Kata Sandi</h1>
              <p style="font-size: 13px; line-height: 20px; color: #475569; margin-bottom: 20px;">
                Halo <strong>{name}</strong>,<br>
                Kami menerima permintaan pengaturan ulang kata sandi untuk akun yang terhubung ke <strong>{email}</strong>.
              </p>
              
              <div style="text-align: center; margin: 28px 0;">
                <a href="{url}" style="background-color: #10b981; color: #ffffff; padding: 12px 28px; border-radius: 10px; font-size: 13px; font-weight: 700; text-decoration: none; display: inline-block;">
                  Atur Ulang Kata Sandi
                </a>
              </div>

              <p style="font-size: 12px; line-height: 18px; color: #64748b; margin-top: 24px;">
                Atau salin tautan berikut ke peramban web Anda:<br>
                <a href="{url}" style="color: #0284c7; word-break: break-all; font-size: 11px;">{url}</a>
              </p>

              <div style="background-color: #f8fafc; border-left: 3px solid #f59e0b; padding: 12px 14px; border-radius: 6px; margin-top: 24px;">
                <p style="font-size: 11px; line-height: 16px; color: #475569; margin: 0;">
                  ⚠️ <strong>Pemberitahuan Keamanan:</strong> Tautan ini hanya berlaku selama <strong>1 jam</strong>. Jika Anda tidak meminta pengaturan ulang kata sandi, abaikan email ini. Akun Anda tetap aman.
                </p>
              </div>
            </td>
          </tr>
          <!-- Footer -->
          <tr>
            <td style="background-color: #f8fafc; padding: 20px 32px; border-top: 1px solid #e2e8f0; font-size: 11px; color: #94a3b8; text-align: center;">
              FinRep by Invinite.id — Digital Creative Solutions<br>
              Butuh bantuan? Hubungi tim dukungan kami di <a href="mailto:{support}" style="color: #64748b; text-decoration: underline;">{support}</a>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#,
            name = recipient_name,
            email = recipient_email,
            url = reset_url,
            support = self.config.support_email
        );

        let message = EmailMessage {
            from: self.config.default_sender.clone(),
            from_name: self.config.default_sender_name.clone(),
            to: recipient_email.to_string(),
            reply_to: Some(self.config.support_email.clone()),
            subject,
            html_body,
            text_body,
        };

        self.deliver(message).await
    }

    /// Central dispatch / delivery abstraction
    pub async fn deliver(&self, message: EmailMessage) -> Result<(), String> {
        println!(
            "Enterprise Mailer dispatching from {} ({}) to {} | Subject: '{}'",
            message.from, message.from_name, message.to, message.subject
        );

        // Store into internal buffer
        self.sent_mailbox.lock().unwrap().push(message.clone());

        // In production, when SMTP or an HTTP transactional mailer (Resend/SendGrid/SES) is configured:
        // Lettre or reqwest client can be engaged here.
        // In local/test mode, we log cleanly:
        println!(
            "\n📧 =================== [OUTGOING EMAIL DISPATCH] ===================\n\
            FROM    : {} <{}>\n\
            TO      : {}\n\
            REPLY-TO: {}\n\
            SUBJECT : {}\n\
            PREVIEW : {}\n\
            ===================================================================\n",
            message.from_name,
            message.from,
            message.to,
            message.reply_to.as_deref().unwrap_or("None"),
            message.subject,
            message.text_body.lines().take(4).collect::<Vec<_>>().join("\n")
        );

        Ok(())
    }
}
