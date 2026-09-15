use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub busy_timeout_ms: u64,
    pub acquire_timeout_secs: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite://data/personal_finance.db?mode=rwc".to_string(),
            max_connections: 10,
            min_connections: 1,
            busy_timeout_ms: 5000,
            acquire_timeout_secs: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PragmaStatus {
    pub journal_mode: String,
    pub busy_timeout: i64,
    pub foreign_keys: bool,
    pub synchronous: i64,
}

/// Helper to ensure the parent directory of an SQLite file exists
pub fn ensure_parent_dir_exists(database_url: &str) {
    let clean_path = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(database_url);

    let file_path = clean_path.split('?').next().unwrap_or(clean_path);
    if file_path != ":memory:" && !file_path.is_empty() {
        let path = Path::new(file_path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
    }
}

/// Builds and initializes the SQLite connection pool with mandatory PRAGMAs
pub async fn init_pool(config: &DbConfig) -> Result<SqlitePool, sqlx::Error> {
    ensure_parent_dir_exists(&config.database_url);

    let connect_opts = SqliteConnectOptions::from_str(&config.database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_millis(config.busy_timeout_ms))
        .foreign_keys(true)
        .synchronous(SqliteSynchronous::Normal);

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect_with(connect_opts)
        .await?;

    Ok(pool)
}

/// Runs embedded SQLx migrations against the pool
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Verifies that all required PRAGMAs are active on the connection
pub async fn verify_pragmas(pool: &SqlitePool) -> Result<PragmaStatus, sqlx::Error> {
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode;")
        .fetch_one(pool)
        .await?;
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout;")
        .fetch_one(pool)
        .await?;
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(pool)
        .await?;
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous;")
        .fetch_one(pool)
        .await?;

    Ok(PragmaStatus {
        journal_mode,
        busy_timeout,
        foreign_keys: foreign_keys == 1,
        synchronous,
    })
}
