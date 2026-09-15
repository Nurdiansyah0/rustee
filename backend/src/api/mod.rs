pub mod accounts;
pub mod analytics;
pub mod auth;
pub mod categories;
pub mod health;
pub mod middleware;
pub mod transactions;
pub mod webhooks;

pub use accounts::accounts_router;
pub use analytics::analytics_router;
pub use auth::{auth_routes, auth_routes_with_rate_limiter, AuthState};
pub use categories::categories_router;
pub use health::health_router;
pub use middleware::*;
pub use transactions::transactions_router;
pub use webhooks::webhooks_router;

use axum::Router;
use sqlx::SqlitePool;
use std::sync::Arc;
use crate::repository::{account_repo::AccountRepository, category_repo::CategoryRepository};
use crate::service::{
    jwt::JwtEngine, ledger_service::LedgerService, payment_service::PaymentService,
};

#[derive(Clone)]
pub struct AppState {
    pub auth_state: AuthState,
    pub account_repo: Arc<dyn AccountRepository>,
    pub category_repo: Arc<dyn CategoryRepository>,
    pub ledger_service: Arc<LedgerService>,
    pub payment_service: Arc<PaymentService>,
    pub pool: SqlitePool,
    pub rate_limiter: Arc<SlidingWindowRateLimiter>,
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
    let auth_router = auth::auth_routes_with_rate_limiter(
        state.auth_state.clone(),
        state.rate_limiter.clone(),
    );
    let health_router = health::health_router(state.pool.clone());

    let protected_router = Router::new()
        .nest("/accounts", accounts::accounts_router())
        .nest("/categories", categories::categories_router())
        .nest("/transactions", transactions::transactions_router())
        .merge(analytics::analytics_router())
        .merge(webhooks::webhooks_router())
        .with_state(state);

    Router::new()
        .merge(health_router)
        .route("/payment/success", axum::routing::get(payment_success_handler))
        .nest("/api/v1/auth", auth_router)
        .nest("/api/v1", protected_router)
}
