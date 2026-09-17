use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

use backend::{
    api::{create_app, AppState, AuthState},
    repository::{
        account_repo::SqlxAccountRepository,
        audit_repo::SqlxAuditRepository,
        category_repo::SqlxCategoryRepository,
        db::{init_pool, run_migrations, DbConfig},
        idempotency_repo::SqlxIdempotencyRepository,
        subscription_repo::SqlxSubscriptionRepository,
        transaction_repo::SqlxTransactionRepository,
        user_repo::SqlxUserRepository,
    },
    service::{
        auth_service::AuthService,
        crypto::{Argon2Config, CryptoService},
        jwt::JwtEngine,
        ledger_service::LedgerService,
        payment_service::{PaymentConfig, PaymentService},
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Personal Finance PWA Backend...");

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/personal_finance.db?mode=rwc".to_string());
    let max_connections: u32 = std::env::var("DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let busy_timeout_ms: u64 = std::env::var("DB_BUSY_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);
    let acquire_timeout_secs: u64 = std::env::var("DB_ACQUIRE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let db_config = DbConfig {
        database_url: db_url,
        max_connections,
        min_connections: 1,
        busy_timeout_ms,
        acquire_timeout_secs,
    };

    println!(
        "Connecting to SQLite database at {}...",
        db_config.database_url
    );
    let pool = init_pool(&db_config).await?;

    println!("Executing embedded migrations...");
    run_migrations(&pool).await?;
    println!("Database migrations up to date.");

    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        "default_insecure_jwt_secret_change_in_production_32_bytes".to_string()
    });
    let jwt_ttl: u64 = std::env::var("JWT_TTL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(900);
    let secure_cookie = std::env::var("COOKIE_SECURE")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    let midtrans_key = std::env::var("MIDTRANS_SERVER_KEY")
        .unwrap_or_else(|_| "midtrans_test_server_key".to_string());
    let xendit_token = std::env::var("XENDIT_WEBHOOK_TOKEN")
        .unwrap_or_else(|_| "xendit_test_webhook_token".to_string());
    let dana_merchant_id = std::env::var("DANA_MERCHANT_ID").unwrap_or_default();
    let dana_client_id = std::env::var("DANA_CLIENT_ID").unwrap_or_default();
    let dana_client_secret = std::env::var("DANA_CLIENT_SECRET").unwrap_or_default();
    let dana_public_key_pem = std::env::var("DANA_PUBLIC_KEY").unwrap_or_default();
    let dana_private_key_pem = std::env::var("DANA_PRIVATE_KEY").unwrap_or_default();
    let dana_api_base_url = std::env::var("DANA_API_BASE_URL")
        .unwrap_or_else(|_| "https://api.sandbox.dana.id".to_string());


    let user_repo = Arc::new(SqlxUserRepository::new(pool.clone()));
    let account_repo = Arc::new(SqlxAccountRepository::new(pool.clone()));
    let category_repo = Arc::new(SqlxCategoryRepository::new(pool.clone()));
    let audit_repo = Arc::new(SqlxAuditRepository::new(pool.clone()));
    let idempotency_repo = Arc::new(SqlxIdempotencyRepository::new(pool.clone()));
    let transaction_repo = Arc::new(SqlxTransactionRepository::new(pool.clone()));
    let subscription_repo = Arc::new(SqlxSubscriptionRepository::new(pool.clone()));

    let crypto_service = Arc::new(CryptoService::new(Argon2Config::default())?);
    let jwt_engine = Arc::new(JwtEngine::new(&jwt_secret, jwt_ttl));

    let auth_service = Arc::new(AuthService::new(
        pool.clone(),
        user_repo.clone(),
        crypto_service,
        jwt_engine,
    ));

    let ledger_service = Arc::new(LedgerService::new(
        pool.clone(),
        transaction_repo,
        idempotency_repo,
    ));

    let payment_service = Arc::new(PaymentService::new(
        PaymentConfig {
            midtrans_server_key: midtrans_key,
            xendit_webhook_token: xendit_token,
            dana_merchant_id,
            dana_client_id,
            dana_client_secret,
            dana_public_key_pem,
            dana_private_key_pem,
            dana_api_base_url,
        },
        subscription_repo,
        user_repo,
        audit_repo,
    ));

    let rate_limiter = Arc::new(
        backend::api::middleware::rate_limiter::SlidingWindowRateLimiter::new(
            backend::api::middleware::rate_limiter::RateLimiterConfig {
                max_attempts: std::env::var("RATE_LIMIT_MAX_ATTEMPTS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5),
                window_duration: std::time::Duration::from_secs(
                    std::env::var("RATE_LIMIT_WINDOW_SECS")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(900),
                ),
                cleanup_interval: std::time::Duration::from_secs(300),
            },
        ),
    );
    rate_limiter.clone().spawn_cleanup_task();

    let auth_state = AuthState {
        auth_service,
        secure_cookie,
    };

    let user_preferences_repo = Arc::new(
        backend::repository::SqlxUserPreferencesRepository::new(pool.clone()),
    );

    let app_state = AppState {
        auth_state,
        account_repo,
        category_repo,
        user_preferences_repo,
        ledger_service,
        payment_service,
        pool,
        rate_limiter,
    };

    let app = create_app(app_state);

    let web_dist = std::env::var("WEB_DIST").unwrap_or_else(|_| {
        if std::path::Path::new("./dist").exists() {
            "./dist".to_string()
        } else if std::path::Path::new("frontend/dist").exists() {
            "frontend/dist".to_string()
        } else if std::path::Path::new("../frontend/dist").exists() {
            "../frontend/dist".to_string()
        } else {
            "./dist".to_string()
        }
    });

    let app = if std::path::Path::new(&web_dist).exists() {
        println!("Serving frontend PWA assets from '{}'...", web_dist);
        use tower_http::services::{ServeDir, ServeFile};
        let index_file = format!("{}/index.html", web_dist);
        let serve_dir = ServeDir::new(&web_dist).not_found_service(ServeFile::new(index_file));
        app.fallback_service(serve_dir)
    } else {
        println!(
            "Static directory '{}' not found, running API-only mode.",
            web_dist
        );
        app
    };

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    println!("Server bound to http://{}", addr);
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
