pub mod accounting;
pub mod accounts;
pub mod analytics;
pub mod auth;
pub mod categories;
pub mod handlers;
pub mod health;
pub mod invoices;
pub mod middleware;
pub mod outbox;
pub mod payments;
pub mod receivables;
pub mod router;
pub mod sync;
pub mod tenants;
pub mod transactions;
pub mod users;
pub mod webhooks;
pub mod ws;

pub use accounting::accounting_router;
pub use accounts::accounts_router;
pub use analytics::analytics_router;
pub use auth::{auth_routes, auth_routes_with_rate_limiter, AuthState};
pub use categories::categories_router;
pub use health::health_router;
pub use invoices::invoices_router;
pub use middleware::*;
pub use outbox::outbox_router;
pub use payments::payments_router;
pub use receivables::receivables_router;
pub use router::{projects_router, register_ingestion_routes};
pub use tenants::tenants_router;
pub use transactions::transactions_router;
pub use users::users_router;
pub use webhooks::webhooks_router;

use crate::repository::{
    account_repo::AccountRepository, category_repo::CategoryRepository,
    tenant_repo::TenantRepository, user_preferences_repo::UserPreferencesRepository,
};
use crate::service::{
    jwt::JwtEngine, ledger_service::LedgerService, payment_service::PaymentService,
    tenant_service::TenantService,
};
use axum::Router;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub auth_state: AuthState,
    pub account_repo: Arc<dyn AccountRepository>,
    pub category_repo: Arc<dyn CategoryRepository>,
    pub user_preferences_repo: Arc<dyn UserPreferencesRepository>,
    pub ledger_service: Arc<LedgerService>,
    pub payment_service: Arc<PaymentService>,
    pub tenant_service: Arc<TenantService>,
    pub tenant_repo: Arc<dyn TenantRepository>,
    pub pool: SqlitePool,
    pub rate_limiter: Arc<SlidingWindowRateLimiter>,
}

impl AppState {
    pub fn accounting_service(&self) -> Arc<crate::service::accounting_service::AccountingService> {
        Arc::new(crate::service::accounting_service::AccountingService::new_with_pool(self.pool.clone()))
    }

    pub fn accounting_repo(&self) -> Arc<dyn crate::repository::accounting_repo::AccountingRepository> {
        Arc::new(crate::repository::accounting_repo::SqlxAccountingRepository::new(self.pool.clone()))
    }

    pub fn invoice_service(&self) -> Arc<crate::service::invoice_service::InvoiceService> {
        Arc::new(crate::service::invoice_service::InvoiceService::new(
            self.pool.clone(),
            self.accounting_service(),
        ))
    }

    pub fn outbox_repo(&self) -> Arc<dyn crate::repository::outbox_repo::OutboxRepository> {
        Arc::new(crate::repository::outbox_repo::SqlxOutboxRepository::new(self.pool.clone()))
    }

    pub fn outbox_processor(&self) -> Arc<crate::service::outbox_processor::OutboxProcessor> {
        Arc::new(crate::service::outbox_processor::OutboxProcessor::with_default_config(self.pool.clone()))
    }

    pub fn project_service(&self) -> Arc<crate::service::project_service::ProjectService> {
        Arc::new(crate::service::project_service::ProjectService::new(self.pool.clone()))
    }
}

impl HasJwtEngine for AppState {
    fn jwt_engine(&self) -> &JwtEngine {
        self.auth_state.jwt_engine()
    }
}

async fn payment_success_handler() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!DOCTYPE html>
<html lang="id">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Pembayaran Berhasil — Invinite</title>
  <style>
    *{margin:0;padding:0;box-sizing:border-box}
    body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;
         background:#0f172a;color:#f8fafc;min-height:100vh;display:flex;
         align-items:center;justify-content:center;text-align:center;padding:24px}
    .card{background:#1e293b;border:1px solid #334155;border-radius:16px;
          padding:40px 32px;max-width:420px;width:100%}
    .icon{font-size:56px;margin-bottom:16px}
    h1{font-size:22px;font-weight:700;margin-bottom:8px;color:#34d399}
    p{color:#94a3b8;font-size:15px;line-height:1.6;margin-bottom:24px}
    a{display:inline-block;background:#10b981;color:#fff;text-decoration:none;
      padding:12px 28px;border-radius:8px;font-weight:600;font-size:15px}
    a:hover{background:#059669}
    .note{margin-top:16px;font-size:12px;color:#475569}
  </style>
  <script>setTimeout(()=>location.href='/',4000)</script>
</head>
<body>
  <div class="card">
    <div class="icon">✅</div>
    <h1>Pembayaran Berhasil!</h1>
    <p>Akun Premium Invinite Anda telah aktif.<br>Nikmati akses penuh ke semua fitur analitik dan laporan keuangan.</p>
    <a href="/">Kembali ke Aplikasi</a>
    <p class="note">Anda akan diarahkan otomatis dalam beberapa detik…</p>
  </div>
</body>
</html>"#,
    )
}

pub fn create_app(state: AppState) -> Router {
    let auth_router =
        auth::auth_routes_with_rate_limiter(state.auth_state.clone(), state.rate_limiter.clone());
    let health_router = health::health_router(state.pool.clone());

    let protected_router = Router::new()
        .nest("/tenants", tenants::tenants_router())
        .nest("/accounting", accounting::accounting_router())
        .nest("/invoices", invoices::invoices_router())
        .nest("/receivables", receivables::receivables_router())
        .nest("/payments", payments::payments_router())
        .nest("/outbox", outbox::outbox_router())
        .nest("/accounts", accounts::accounts_router())
        .nest("/categories", categories::categories_router())
        .nest("/transactions", transactions::transactions_router())
        .nest("/users", users::users_router())
        .route("/sync", axum::routing::get(sync::sync_handler))
        .route("/ws", axum::routing::get(ws::ws_handler))
        .merge(analytics::analytics_router())
        .merge(webhooks::webhooks_router());

    let protected_router =
        router::register_ingestion_routes(protected_router).with_state(state.clone());

    Router::new()
        .merge(health_router)
        .route("/ws", axum::routing::get(ws::ws_handler))
        .route(
            "/payment/success",
            axum::routing::get(payment_success_handler),
        )
        .route(
            "/v1.0/debit/notify",
            axum::routing::post(webhooks::dana_webhook_handler).with_state(state.clone()),
        )
        .route(
            "/payment-gateway/v1.0/debit/notify",
            axum::routing::post(webhooks::dana_webhook_handler).with_state(state.clone()),
        )
        .nest("/api/v1/auth", auth_router)
        .nest("/api/v1", protected_router)
}
