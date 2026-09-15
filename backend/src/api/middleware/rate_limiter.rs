use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Configuration for sliding window rate limiting.
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    /// Maximum allowed attempts within the window (default: 5).
    pub max_attempts: usize,
    /// Duration of the sliding window (default: 15 minutes = 900s).
    pub window_duration: Duration,
    /// Interval between background sweeps (default: 5 minutes = 300s).
    pub cleanup_interval: Duration,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            window_duration: Duration::from_secs(900),
            cleanup_interval: Duration::from_secs(300),
        }
    }
}

/// Decision returned by the rate limiter evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allowed {
        remaining: usize,
        reset_after: Duration,
    },
    Denied {
        retry_after: Duration,
        remaining: usize,
        reset_after: Duration,
    },
}

/// Thread-safe in-memory sliding-window rate limiter.
#[derive(Debug, Clone)]
pub struct SlidingWindowRateLimiter {
    pub attempts: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    pub config: RateLimiterConfig,
}

impl Default for SlidingWindowRateLimiter {
    fn default() -> Self {
        Self::new(RateLimiterConfig::default())
    }
}

impl SlidingWindowRateLimiter {
    /// Creates a new rate limiter with the specified configuration.
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            attempts: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Evaluates rate limit for a key, recording the attempt if allowed.
    pub async fn check_and_record(&self, key: &str) -> RateLimitDecision {
        let mut map = self.attempts.write().await;
        let now = Instant::now();
        let window = self.config.window_duration;

        let timestamps = map.entry(key.to_string()).or_default();

        // Tier 1: Lazy inline prune of expired entries
        timestamps.retain(|&t| now.duration_since(t) < window);

        if timestamps.len() >= self.config.max_attempts {
            // Quota exceeded: calculate cooldown until oldest attempt expires
            let oldest = timestamps.first().copied().unwrap_or(now);
            let elapsed = now.duration_since(oldest);
            let retry_after = if elapsed < window {
                window - elapsed
            } else {
                Duration::from_secs(1)
            };

            RateLimitDecision::Denied {
                retry_after,
                remaining: 0,
                reset_after: retry_after,
            }
        } else {
            // Quota available: record attempt
            timestamps.push(now);
            let remaining = self.config.max_attempts - timestamps.len();
            let oldest = *timestamps.first().unwrap();
            let elapsed = now.duration_since(oldest);
            let reset_after = if elapsed < window {
                window - elapsed
            } else {
                window
            };

            RateLimitDecision::Allowed {
                remaining,
                reset_after,
            }
        }
    }

    /// Resets recorded attempts for a key (called upon successful authentication).
    pub async fn reset(&self, key: &str) {
        let mut map = self.attempts.write().await;
        map.remove(key);
    }

    /// Tier 2: Sweeps all keys, removing expired records and empty keys.
    pub async fn prune_stale_entries(&self) -> usize {
        let mut map = self.attempts.write().await;
        let now = Instant::now();
        let window = self.config.window_duration;
        let initial_len = map.len();

        map.retain(|_key, timestamps| {
            // Fast path: if newest timestamp is expired, all are expired
            if let Some(&newest) = timestamps.last() {
                if now.duration_since(newest) >= window {
                    return false;
                }
            }
            timestamps.retain(|&t| now.duration_since(t) < window);
            !timestamps.is_empty()
        });

        initial_len - map.len()
    }

    /// Spawns a Tokio background task to periodically purge stale entries.
    pub fn spawn_cleanup_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval_duration = self.config.cleanup_interval;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval_duration);
            loop {
                interval.tick().await;
                let _ = self.prune_stale_entries().await;
            }
        })
    }
}

/// RFC 7807 Problem Details representation for rate limit exceeded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemDetails {
    pub r#type: &'static str,
    pub title: &'static str,
    pub status: u16,
    pub detail: String,
    pub code: &'static str,
    pub retry_after_seconds: u64,
}

impl ProblemDetails {
    pub fn rate_limit_exceeded(retry_after: Duration) -> Self {
        let retry_after_seconds = retry_after.as_secs().max(1);
        Self {
            r#type: "https://api.nurdiansyahlabs.com/errors/too-many-requests",
            title: "Too Many Requests",
            status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
            detail: format!(
                "Too many authentication attempts. Rate limit is 5 attempts per 15 minutes. Please retry in {} seconds.",
                retry_after_seconds
            ),
            code: "AUTH_RATE_LIMIT_EXCEEDED",
            retry_after_seconds,
        }
    }
}

/// Extracts client IP with proxy-aware fallback and validation.
pub fn extract_client_ip(headers: &HeaderMap, connect_info: Option<&SocketAddr>) -> String {
    // 1. Cloudflare header
    if let Some(cf_ip) = headers
        .get("cf-connecting-ip")
        .and_then(|h| h.to_str().ok())
    {
        let trimmed = cf_ip.trim();
        if trimmed.parse::<IpAddr>().is_ok() {
            return trimmed.to_string();
        }
    }

    // 2. Nginx Real IP header
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|h| h.to_str().ok()) {
        let trimmed = real_ip.trim();
        if trimmed.parse::<IpAddr>().is_ok() {
            return trimmed.to_string();
        }
    }

    // 3. Standard X-Forwarded-For (leftmost client IP)
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok()) {
        if let Some(client_ip) = xff.split(',').next().map(|s| s.trim()) {
            if client_ip.parse::<IpAddr>().is_ok() {
                return client_ip.to_string();
            }
        }
    }

    // 4. Direct socket address from Axum ConnectInfo
    if let Some(addr) = connect_info {
        return addr.ip().to_string();
    }

    // 5. Test runner fallback
    "127.0.0.1".to_string()
}

/// Axum middleware applying the sliding window rate limiter.
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<SlidingWindowRateLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    let connect_info = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let client_ip = extract_client_ip(request.headers(), connect_info.as_ref());
    let is_login = request.uri().path().ends_with("/login");

    match limiter.check_and_record(&client_ip).await {
        RateLimitDecision::Allowed {
            remaining,
            reset_after,
        } => {
            let mut response = next.run(request).await;

            // Reset rate limiter on successful authentication to prevent locking out legitimate users
            let is_login_success = is_login && response.status().is_success();
            if is_login_success {
                limiter.reset(&client_ip).await;
            }

            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", HeaderValue::from_static("5"));
            let rem_header = if is_login_success {
                limiter.config.max_attempts
            } else {
                remaining
            };
            headers.insert("X-RateLimit-Remaining", HeaderValue::from(rem_header));
            headers.insert(
                "X-RateLimit-Reset",
                HeaderValue::from(reset_after.as_secs().max(1)),
            );
            response
        }
        RateLimitDecision::Denied {
            retry_after,
            reset_after,
            ..
        } => {
            let retry_secs = retry_after.as_secs().max(1);
            let reset_secs = reset_after.as_secs().max(1);

            let problem = ProblemDetails::rate_limit_exceeded(retry_after);

            let mut headers = HeaderMap::new();
            headers.insert("Retry-After", HeaderValue::from(retry_secs));
            headers.insert("X-RateLimit-Limit", HeaderValue::from_static("5"));
            headers.insert("X-RateLimit-Remaining", HeaderValue::from_static("0"));
            headers.insert("X-RateLimit-Reset", HeaderValue::from(reset_secs));
            headers.insert(
                "Content-Type",
                HeaderValue::from_static("application/problem+json"),
            );

            (StatusCode::TOO_MANY_REQUESTS, headers, Json(problem)).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sliding_window_exact_five_attempt_barrier() {
        let limiter = SlidingWindowRateLimiter::new(RateLimiterConfig::default());
        let ip = "192.168.1.10";

        for i in 1..=5 {
            let decision = limiter.check_and_record(ip).await;
            match decision {
                RateLimitDecision::Allowed { remaining, .. } => {
                    assert_eq!(
                        remaining,
                        5 - i,
                        "Attempt {} should have remaining {}",
                        i,
                        5 - i
                    );
                }
                RateLimitDecision::Denied { .. } => panic!("Attempt {} must be allowed!", i),
            }
        }

        // 6th Attempt MUST be denied
        let decision = limiter.check_and_record(ip).await;
        match decision {
            RateLimitDecision::Denied {
                retry_after,
                remaining,
                ..
            } => {
                assert_eq!(remaining, 0);
                assert!(retry_after.as_secs() > 0 && retry_after.as_secs() <= 900);
            }
            RateLimitDecision::Allowed { .. } => panic!("Attempt 6 must be blocked!"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_heavy_concurrent_race_condition_exact_quota() {
        let limiter = Arc::new(SlidingWindowRateLimiter::new(RateLimiterConfig::default()));
        let ip = "10.0.0.99";
        let total_tasks = 100;
        let barrier = Arc::new(tokio::sync::Barrier::new(total_tasks));
        let mut handles = Vec::new();

        for _ in 0..total_tasks {
            let lim = limiter.clone();
            let bar = barrier.clone();
            handles.push(tokio::spawn(async move {
                bar.wait().await;
                lim.check_and_record(ip).await
            }));
        }

        let mut allowed = 0;
        let mut denied = 0;
        for handle in handles {
            match handle.await.unwrap() {
                RateLimitDecision::Allowed { .. } => allowed += 1,
                RateLimitDecision::Denied { .. } => denied += 1,
            }
        }

        assert_eq!(allowed, 5, "Strictly 5 tasks must be granted permission");
        assert_eq!(denied, 95, "Strictly 95 tasks must be rejected");
    }

    #[tokio::test]
    async fn test_multi_ip_partition_isolation() {
        let limiter = SlidingWindowRateLimiter::new(RateLimiterConfig::default());
        let ip_a = "1.1.1.1";
        let ip_b = "2.2.2.2";

        // Lock out IP A
        for _ in 0..5 {
            assert!(matches!(
                limiter.check_and_record(ip_a).await,
                RateLimitDecision::Allowed { .. }
            ));
        }
        assert!(matches!(
            limiter.check_and_record(ip_a).await,
            RateLimitDecision::Denied { .. }
        ));

        // IP B must still have all 5 attempts available
        for i in 1..=5 {
            match limiter.check_and_record(ip_b).await {
                RateLimitDecision::Allowed { remaining, .. } => assert_eq!(remaining, 5 - i),
                RateLimitDecision::Denied { .. } => {
                    panic!("IP B must not be affected by IP A lockout")
                }
            }
        }
    }

    #[tokio::test]
    async fn test_sliding_window_cooldown_recovery() {
        let config = RateLimiterConfig {
            max_attempts: 2,
            window_duration: Duration::from_millis(50),
            cleanup_interval: Duration::from_secs(60),
        };
        let limiter = SlidingWindowRateLimiter::new(config);
        let ip = "172.16.0.1";

        assert!(matches!(
            limiter.check_and_record(ip).await,
            RateLimitDecision::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check_and_record(ip).await,
            RateLimitDecision::Allowed { .. }
        ));
        assert!(matches!(
            limiter.check_and_record(ip).await,
            RateLimitDecision::Denied { .. }
        ));

        // Wait for window to expire
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Slot has reopened
        match limiter.check_and_record(ip).await {
            RateLimitDecision::Allowed { remaining, .. } => assert_eq!(remaining, 1),
            RateLimitDecision::Denied { .. } => {
                panic!("Quota should have recovered after cooldown")
            }
        }
    }

    #[tokio::test]
    async fn test_automatic_pruning_memory_leak_prevention() {
        let limiter = SlidingWindowRateLimiter::new(RateLimiterConfig {
            max_attempts: 5,
            window_duration: Duration::from_millis(50),
            cleanup_interval: Duration::from_millis(100),
        });

        // Populate 50 ephemeral keys
        for i in 0..50 {
            limiter.check_and_record(&format!("10.0.0.{}", i)).await;
        }

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Add 1 fresh key
        limiter.check_and_record("10.0.0.254").await;

        let purged = limiter.prune_stale_entries().await;
        assert_eq!(purged, 50, "All 50 expired keys must be pruned");

        let map = limiter.attempts.read().await;
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("10.0.0.254"));
    }

    #[tokio::test]
    async fn test_successful_auth_counter_reset() {
        let limiter = SlidingWindowRateLimiter::new(RateLimiterConfig::default());
        let ip = "192.168.1.55";

        for _ in 0..4 {
            limiter.check_and_record(ip).await;
        }

        limiter.reset(ip).await;

        match limiter.check_and_record(ip).await {
            RateLimitDecision::Allowed { remaining, .. } => assert_eq!(remaining, 4),
            RateLimitDecision::Denied { .. } => panic!("Reset key must allow fresh attempts"),
        }
    }

    #[test]
    fn test_extract_client_ip_precedence() {
        let mut headers = HeaderMap::new();

        // 1. Empty headers fallback
        assert_eq!(extract_client_ip(&headers, None), "127.0.0.1");

        // 2. ConnectInfo fallback
        let socket: SocketAddr = "192.168.10.5:8080".parse().unwrap();
        assert_eq!(extract_client_ip(&headers, Some(&socket)), "192.168.10.5");

        // 3. X-Forwarded-For precedence over socket
        headers.insert(
            "x-forwarded-for",
            "203.0.113.195, 70.41.3.18".parse().unwrap(),
        );
        assert_eq!(extract_client_ip(&headers, Some(&socket)), "203.0.113.195");

        // 4. X-Real-IP precedence over XFF
        headers.insert("x-real-ip", "198.51.100.1".parse().unwrap());
        assert_eq!(extract_client_ip(&headers, Some(&socket)), "198.51.100.1");

        // 5. CF-Connecting-IP precedence over Real-IP
        headers.insert("cf-connecting-ip", "1.1.1.1".parse().unwrap());
        assert_eq!(extract_client_ip(&headers, Some(&socket)), "1.1.1.1");
    }
}
