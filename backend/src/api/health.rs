use axum::{
    extract::State,
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sqlx::SqlitePool;

pub const CACHE_CONTROL_VALUE: &str = "private, no-store, must-revalidate";

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

#[derive(Serialize)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub database: &'static str,
    pub wal_mode: bool,
}

pub async fn health_handler() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));
    (
        StatusCode::OK,
        headers,
        Json(HealthResponse {
            status: "pass",
            service: "personal_finance_pwa",
            version: "1.0.0",
        }),
    )
}

pub async fn ready_handler(State(pool): State<SqlitePool>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL_VALUE));

    let wal = sqlx::query_scalar::<_, String>("PRAGMA journal_mode;")
        .fetch_one(&pool)
        .await
        .map(|s| s.to_lowercase() == "wal")
        .unwrap_or(false);

    (
        StatusCode::OK,
        headers,
        Json(ReadyResponse {
            status: "pass",
            database: "connected",
            wal_mode: wal,
        }),
    )
}

pub async fn manifest_handler() -> impl IntoResponse {
    let dist_manifest = std::env::var("WEB_DIST")
        .map(|d| format!("{}/manifest.json", d))
        .unwrap_or_else(|_| "dist/manifest.json".to_string());

    if let Ok(content) = tokio::fs::read_to_string(&dist_manifest).await {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/manifest+json"),
        );
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400"),
        );
        return (StatusCode::OK, headers, content);
    }

    let manifest = serde_json::json!({
        "name": "Personal Finance PWA",
        "short_name": "FinancePWA",
        "start_url": "/",
        "display": "standalone",
        "background_color": "#ffffff",
        "theme_color": "#059669",
        "icons": [
            {"src": "/icons/icon-192.png", "sizes": "192x192", "type": "image/png"},
            {"src": "/icons/icon-512.png", "sizes": "512x512", "type": "image/png"}
        ]
    });
    let body = serde_json::to_string(&manifest).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/manifest+json"),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    (StatusCode::OK, headers, body)
}

pub async fn service_worker_handler() -> impl IntoResponse {
    let dist_sw = std::env::var("WEB_DIST")
        .map(|d| format!("{}/sw.js", d))
        .unwrap_or_else(|_| "dist/sw.js".to_string());

    if let Ok(content) = tokio::fs::read_to_string(&dist_sw).await {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/javascript"),
        );
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return (StatusCode::OK, headers, content);
    }

    let sw_code = r#"
const CACHE_NAME = 'pwa-cache-v1';
self.addEventListener('install', event => {
    event.waitUntil(caches.open(CACHE_NAME).then(c => c.addAll(['/', '/manifest.json', '/index.html'])));
    self.skipWaiting();
});
self.addEventListener('fetch', event => {
    if (event.request.url.includes('/api/v1/')) {
        event.respondWith(fetch(event.request)); // NetworkOnly for financial endpoints
    } else {
        event.respondWith(caches.match(event.request).then(res => res || fetch(event.request)));
    }
});
"#
    .trim();

    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/javascript"),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, sw_code.to_string())
}

pub async fn index_html_handler() -> impl IntoResponse {
    let dist_index = std::env::var("WEB_DIST")
        .map(|d| format!("{}/index.html", d))
        .unwrap_or_else(|_| "dist/index.html".to_string());

    if let Ok(content) = tokio::fs::read_to_string(&dist_index).await {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=3600"),
        );
        return (StatusCode::OK, headers, content);
    }

    let html = r#"<!DOCTYPE html>
<html lang="id">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover">
  <link rel="manifest" href="/manifest.json">
  <title>Finance PWA</title>
</head>
<body>
  <div id="app"></div>
</body>
</html>"#;

    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    (StatusCode::OK, headers, html.to_string())
}

pub fn health_router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/manifest.json", get(manifest_handler))
        .route("/service-worker.js", get(service_worker_handler))
        .route("/", get(index_html_handler))
        .route("/index.html", get(index_html_handler))
        .with_state(pool)
}
