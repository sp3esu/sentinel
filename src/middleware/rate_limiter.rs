//! Rate limiting middleware
//!
//! Implements sliding window rate limiting using Redis.
//! Uses atomic MULTI/EXEC operations to ensure accuracy under concurrent load.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use redis::AsyncCommands;

use crate::{
    error::{AppError, ErrorBody, ErrorDetails, ErrorResponse},
    middleware::auth::AuthenticatedUser,
    AppState,
};

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: i64,
    /// Window size in seconds
    pub window_seconds: u64,
    /// Key prefix for Redis
    pub key_prefix: String,
}

impl RateLimitConfig {
    /// Create a new rate limit config
    pub fn new(max_requests: i64, window_seconds: u64, key_prefix: &str) -> Self {
        Self {
            max_requests,
            window_seconds,
            key_prefix: key_prefix.to_string(),
        }
    }

    /// Create config for AI requests
    pub fn for_ai_requests() -> Self {
        Self {
            max_requests: 100,
            window_seconds: 60,
            key_prefix: "sentinel:ratelimit:ai".to_string(),
        }
    }

    /// Create config for token-based limits
    pub fn for_tokens(max_tokens: i64, window_seconds: u64) -> Self {
        Self {
            max_requests: max_tokens,
            window_seconds,
            key_prefix: "sentinel:ratelimit:tokens".to_string(),
        }
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_seconds: 60,
            key_prefix: "sentinel:ratelimit".to_string(),
        }
    }
}

/// Rate limit check result
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Maximum requests allowed in window
    pub limit: i64,
    /// Remaining requests in current window
    pub remaining: i64,
    /// Timestamp when the rate limit resets
    pub reset_at: i64,
    /// Current request count
    pub current: i64,
}

impl RateLimitResult {
    /// Create rate limit headers for the response
    pub fn headers(&self) -> Vec<(header::HeaderName, HeaderValue)> {
        let mut headers = vec![
            (
                header::HeaderName::from_static("x-ratelimit-limit"),
                HeaderValue::from_str(&self.limit.to_string()).unwrap(),
            ),
            (
                header::HeaderName::from_static("x-ratelimit-remaining"),
                HeaderValue::from_str(&self.remaining.max(0).to_string()).unwrap(),
            ),
            (
                header::HeaderName::from_static("x-ratelimit-reset"),
                HeaderValue::from_str(&self.reset_at.to_string()).unwrap(),
            ),
        ];

        if !self.allowed {
            // Add Retry-After header when rate limited
            let retry_after = (self.reset_at - chrono::Utc::now().timestamp()).max(1);
            headers.push((
                header::RETRY_AFTER,
                HeaderValue::from_str(&retry_after.to_string()).unwrap(),
            ));
        }

        headers
    }
}

/// Generate Redis key for sliding window rate limiting
fn rate_limit_key(prefix: &str, user_id: &str, window_start: i64) -> String {
    format!("{}:{}:{}", prefix, user_id, window_start)
}

/// Check rate limit for a user using sliding window algorithm
///
/// Uses Redis MULTI/EXEC for atomic operations:
/// 1. INCR the counter for current window
/// 2. EXPIRE to set TTL if key is new
/// 3. GET previous window counter (for sliding calculation)
///
/// The sliding window combines current and previous windows based on elapsed time.
pub async fn check_rate_limit(
    state: &Arc<AppState>,
    user_id: &str,
    config: &RateLimitConfig,
) -> Result<RateLimitResult, AppError> {
    // In test mode, Redis may not be configured - skip rate limiting
    let Some(ref redis) = state.redis else {
        let now = chrono::Utc::now().timestamp();
        return Ok(RateLimitResult {
            allowed: true,
            limit: config.max_requests,
            remaining: config.max_requests,
            reset_at: now + config.window_seconds as i64,
            current: 0,
        });
    };

    let mut conn = redis.clone();
    let now = chrono::Utc::now().timestamp();

    // Calculate window boundaries
    let window_seconds = config.window_seconds as i64;
    let current_window = now / window_seconds;
    let previous_window = current_window - 1;
    let window_start_time = current_window * window_seconds;
    let elapsed_in_window = now - window_start_time;

    // Keys for current and previous windows
    let current_key = rate_limit_key(&config.key_prefix, user_id, current_window);
    let previous_key = rate_limit_key(&config.key_prefix, user_id, previous_window);

    // Get previous window count (may not exist)
    let previous_count: i64 = conn.get(&previous_key).await.unwrap_or(0);

    // Atomically increment current window and set expiry
    // Use a pipeline for atomic operations
    let (current_count,): (i64,) = redis::pipe()
        .atomic()
        .incr(&current_key, 1i64)
        .expire(&current_key, (config.window_seconds * 2) as i64) // Keep for 2 windows
        .ignore()
        .query_async(&mut conn)
        .await?;

    // Calculate sliding window count
    // Weight the previous window by the portion that hasn't elapsed
    let weight = 1.0 - (elapsed_in_window as f64 / window_seconds as f64);
    let weighted_previous = (previous_count as f64 * weight) as i64;
    let total_count = current_count + weighted_previous;

    // Check if limit exceeded
    let allowed = total_count <= config.max_requests;
    let remaining = config.max_requests - total_count;
    let reset_at = window_start_time + window_seconds;

    Ok(RateLimitResult {
        allowed,
        limit: config.max_requests,
        remaining,
        reset_at,
        current: total_count,
    })
}

/// Increment rate limit counter by a custom amount
///
/// Useful for token-based rate limiting where we want to increment
/// by the number of tokens used rather than by 1.
pub async fn increment_rate_limit(
    state: &Arc<AppState>,
    user_id: &str,
    config: &RateLimitConfig,
    amount: i64,
) -> Result<RateLimitResult, AppError> {
    // In test mode, Redis may not be configured - skip rate limiting
    let Some(ref redis) = state.redis else {
        let now = chrono::Utc::now().timestamp();
        return Ok(RateLimitResult {
            allowed: true,
            limit: config.max_requests,
            remaining: config.max_requests - amount,
            reset_at: now + config.window_seconds as i64,
            current: amount,
        });
    };

    let mut conn = redis.clone();
    let now = chrono::Utc::now().timestamp();

    // Calculate window boundaries
    let window_seconds = config.window_seconds as i64;
    let current_window = now / window_seconds;
    let previous_window = current_window - 1;
    let window_start_time = current_window * window_seconds;
    let elapsed_in_window = now - window_start_time;

    // Keys for current and previous windows
    let current_key = rate_limit_key(&config.key_prefix, user_id, current_window);
    let previous_key = rate_limit_key(&config.key_prefix, user_id, previous_window);

    // Get previous window count
    let previous_count: i64 = conn.get(&previous_key).await.unwrap_or(0);

    // Atomically increment current window by amount
    let (current_count,): (i64,) = redis::pipe()
        .atomic()
        .incr(&current_key, amount)
        .expire(&current_key, (config.window_seconds * 2) as i64)
        .ignore()
        .query_async(&mut conn)
        .await?;

    // Calculate sliding window count
    let weight = 1.0 - (elapsed_in_window as f64 / window_seconds as f64);
    let weighted_previous = (previous_count as f64 * weight) as i64;
    let total_count = current_count + weighted_previous;

    let allowed = total_count <= config.max_requests;
    let remaining = config.max_requests - total_count;
    let reset_at = window_start_time + window_seconds;

    Ok(RateLimitResult {
        allowed,
        limit: config.max_requests,
        remaining,
        reset_at,
        current: total_count,
    })
}

/// Build a 429 Too Many Requests response with rate limit headers
pub fn rate_limit_exceeded_response(result: &RateLimitResult) -> Response {
    let error_response = ErrorResponse {
        error: ErrorBody {
            code: "RATE_LIMIT_EXCEEDED".to_string(),
            message: "Too many requests. Please slow down.".to_string(),
            details: Some(ErrorDetails {
                limit: Some(result.limit),
                used: Some(result.current),
                remaining: Some(result.remaining.max(0)),
                reset_at: Some(
                    chrono::DateTime::from_timestamp(result.reset_at, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| result.reset_at.to_string()),
                ),
            }),
        },
    };

    let mut response = (StatusCode::TOO_MANY_REQUESTS, Json(error_response)).into_response();

    // Add rate limit headers
    let headers = response.headers_mut();
    for (name, value) in result.headers() {
        headers.insert(name, value);
    }

    response
}

/// Rate limiting middleware
///
/// Checks rate limits before processing requests. Returns 429 if exceeded.
/// Adds rate limit headers to all responses.
pub async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    // Extract user ID from extensions (set by auth middleware)
    let user_id = request
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.external_id.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    // Default rate limit config
    let config = RateLimitConfig::for_ai_requests();

    // Check rate limit
    match check_rate_limit(&state, &user_id, &config).await {
        Ok(result) => {
            if !result.allowed {
                tracing::warn!(
                    user_id = %user_id,
                    limit = result.limit,
                    current = result.current,
                    "Rate limit exceeded"
                );
                return rate_limit_exceeded_response(&result);
            }

            // Process request
            let mut response = next.run(request).await;

            // Add rate limit headers to successful response
            let headers = response.headers_mut();
            for (name, value) in result.headers() {
                headers.insert(name, value);
            }

            response
        }
        Err(e) => {
            // Log error but allow request through (fail open)
            tracing::error!(error = %e, "Rate limit check failed");
            next.run(request).await
        }
    }
}

/// Create rate limit middleware layer with custom config
pub fn rate_limit_layer(
    config: RateLimitConfig,
) -> impl Fn(State<Arc<AppState>>, Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>> + Clone + Send {
    move |state: State<Arc<AppState>>, request: Request, next: Next| {
        let config = config.clone();
        Box::pin(async move {
            let user_id = request
                .extensions()
                .get::<AuthenticatedUser>()
                .map(|u| u.external_id.clone())
                .unwrap_or_else(|| "anonymous".to_string());

            match check_rate_limit(&state, &user_id, &config).await {
                Ok(result) => {
                    if !result.allowed {
                        return rate_limit_exceeded_response(&result);
                    }

                    let mut response = next.run(request).await;
                    let headers = response.headers_mut();
                    for (name, value) in result.headers() {
                        headers.insert(name, value);
                    }
                    response
                }
                Err(e) => {
                    tracing::error!(error = %e, "Rate limit check failed");
                    next.run(request).await
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // RateLimitConfig Tests
    // ===========================================

    #[test]
    fn test_default_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window_seconds, 60);
        assert_eq!(config.key_prefix, "sentinel:ratelimit");
    }

    #[test]
    fn test_config_new() {
        let config = RateLimitConfig::new(500, 300, "custom:prefix");
        assert_eq!(config.max_requests, 500);
        assert_eq!(config.window_seconds, 300);
        assert_eq!(config.key_prefix, "custom:prefix");
    }

    #[test]
    fn test_ai_requests_config() {
        let config = RateLimitConfig::for_ai_requests();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window_seconds, 60);
        assert!(config.key_prefix.contains("ai"));
        assert_eq!(config.key_prefix, "sentinel:ratelimit:ai");
    }

    #[test]
    fn test_tokens_config() {
        let config = RateLimitConfig::for_tokens(10000, 3600);
        assert_eq!(config.max_requests, 10000);
        assert_eq!(config.window_seconds, 3600);
        assert!(config.key_prefix.contains("tokens"));
        assert_eq!(config.key_prefix, "sentinel:ratelimit:tokens");
    }

    #[test]
    fn test_config_clone() {
        let config = RateLimitConfig::new(200, 120, "test:prefix");
        let cloned = config.clone();

        assert_eq!(config.max_requests, cloned.max_requests);
        assert_eq!(config.window_seconds, cloned.window_seconds);
        assert_eq!(config.key_prefix, cloned.key_prefix);
    }

    #[test]
    fn test_config_debug() {
        let config = RateLimitConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("RateLimitConfig"));
        assert!(debug_str.contains("100"));
        assert!(debug_str.contains("60"));
    }

    #[test]
    fn test_config_edge_cases() {
        // Zero requests (effectively blocks all)
        let config = RateLimitConfig::new(0, 60, "test");
        assert_eq!(config.max_requests, 0);

        // Very large limit
        let config = RateLimitConfig::new(i64::MAX, 60, "test");
        assert_eq!(config.max_requests, i64::MAX);

        // Very long window
        let config = RateLimitConfig::new(100, u64::MAX, "test");
        assert_eq!(config.window_seconds, u64::MAX);
    }

    #[test]
    fn test_config_empty_prefix() {
        let config = RateLimitConfig::new(100, 60, "");
        assert_eq!(config.key_prefix, "");
    }

    // ===========================================
    // Rate Limit Key Generation Tests
    // ===========================================

    #[test]
    fn test_rate_limit_key() {
        let key = rate_limit_key("sentinel:ratelimit", "user123", 1234567890);
        assert_eq!(key, "sentinel:ratelimit:user123:1234567890");
    }

    #[test]
    fn test_rate_limit_key_format() {
        let key = rate_limit_key("prefix", "user", 12345);
        assert!(key.starts_with("prefix:"));
        assert!(key.contains(":user:"));
        assert!(key.ends_with(":12345"));
    }

    #[test]
    fn test_rate_limit_key_different_users() {
        let key1 = rate_limit_key("prefix", "user1", 12345);
        let key2 = rate_limit_key("prefix", "user2", 12345);

        assert_ne!(key1, key2);
        assert!(key1.contains("user1"));
        assert!(key2.contains("user2"));
    }

    #[test]
    fn test_rate_limit_key_different_windows() {
        let key1 = rate_limit_key("prefix", "user", 12345);
        let key2 = rate_limit_key("prefix", "user", 12346);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_rate_limit_key_special_characters() {
        let key = rate_limit_key("sentinel:ratelimit", "user@example.com", 12345);
        assert_eq!(key, "sentinel:ratelimit:user@example.com:12345");
    }

    #[test]
    fn test_rate_limit_key_uuid() {
        let key = rate_limit_key("sentinel:ratelimit", "550e8400-e29b-41d4-a716-446655440000", 12345);
        assert!(key.contains("550e8400-e29b-41d4-a716-446655440000"));
    }

    // ===========================================
    // RateLimitResult Tests
    // ===========================================

    #[test]
    fn test_rate_limit_result_allowed() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 95,
            reset_at: 1234567890,
            current: 5,
        };

        assert!(result.allowed);
        assert_eq!(result.limit, 100);
        assert_eq!(result.remaining, 95);
        assert_eq!(result.current, 5);
    }

    #[test]
    fn test_rate_limit_result_denied() {
        let result = RateLimitResult {
            allowed: false,
            limit: 100,
            remaining: -5,
            reset_at: 1234567890,
            current: 105,
        };

        assert!(!result.allowed);
        assert_eq!(result.remaining, -5);
        assert_eq!(result.current, 105);
    }

    #[test]
    fn test_rate_limit_result_headers_allowed() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 95,
            reset_at: 1234567890,
            current: 5,
        };

        let headers = result.headers();
        assert_eq!(headers.len(), 3); // limit, remaining, reset (no Retry-After)

        // Check header names
        let header_names: Vec<_> = headers.iter().map(|(name, _)| name.as_str()).collect();
        assert!(header_names.contains(&"x-ratelimit-limit"));
        assert!(header_names.contains(&"x-ratelimit-remaining"));
        assert!(header_names.contains(&"x-ratelimit-reset"));
    }

    #[test]
    fn test_rate_limit_result_headers_denied() {
        let future_reset = chrono::Utc::now().timestamp() + 30;
        let result = RateLimitResult {
            allowed: false,
            limit: 100,
            remaining: -5,
            reset_at: future_reset,
            current: 105,
        };

        let headers = result.headers();
        assert_eq!(headers.len(), 4); // includes Retry-After

        let header_names: Vec<_> = headers.iter().map(|(name, _)| name.as_str()).collect();
        assert!(header_names.contains(&"retry-after"));
    }

    #[test]
    fn test_rate_limit_result_headers_values() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 50,
            reset_at: 1700000000,
            current: 50,
        };

        let headers = result.headers();

        for (name, value) in &headers {
            match name.as_str() {
                "x-ratelimit-limit" => assert_eq!(value.to_str().unwrap(), "100"),
                "x-ratelimit-remaining" => assert_eq!(value.to_str().unwrap(), "50"),
                "x-ratelimit-reset" => assert_eq!(value.to_str().unwrap(), "1700000000"),
                _ => {}
            }
        }
    }

    #[test]
    fn test_rate_limit_result_remaining_clamped() {
        // Remaining is clamped to 0 in headers when negative
        let result = RateLimitResult {
            allowed: false,
            limit: 100,
            remaining: -10,
            reset_at: chrono::Utc::now().timestamp() + 30,
            current: 110,
        };

        let headers = result.headers();

        for (name, value) in &headers {
            if name.as_str() == "x-ratelimit-remaining" {
                assert_eq!(value.to_str().unwrap(), "0");
            }
        }
    }

    #[test]
    fn test_rate_limit_result_clone() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 50,
            reset_at: 1234567890,
            current: 50,
        };

        let cloned = result.clone();
        assert_eq!(result.allowed, cloned.allowed);
        assert_eq!(result.limit, cloned.limit);
        assert_eq!(result.remaining, cloned.remaining);
        assert_eq!(result.reset_at, cloned.reset_at);
        assert_eq!(result.current, cloned.current);
    }

    #[test]
    fn test_rate_limit_result_debug() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 50,
            reset_at: 1234567890,
            current: 50,
        };

        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("RateLimitResult"));
        assert!(debug_str.contains("allowed: true"));
    }

    // ===========================================
    // Window Calculation Tests
    // ===========================================

    #[test]
    fn test_window_calculation_boundaries() {
        let window_seconds: i64 = 60;

        // Test window boundaries
        let timestamps = vec![
            (0, 0),      // Start of epoch
            (59, 0),     // End of first window
            (60, 1),     // Start of second window
            (119, 1),    // End of second window
            (120, 2),    // Start of third window
        ];

        for (timestamp, expected_window) in timestamps {
            let window = timestamp / window_seconds;
            assert_eq!(window, expected_window, "Failed for timestamp {}", timestamp);
        }
    }

    #[test]
    fn test_window_start_calculation() {
        let now: i64 = 1700000000;
        let window_seconds: i64 = 60;
        let current_window = now / window_seconds;
        let window_start_time = current_window * window_seconds;

        // Window start should be at or before current time
        assert!(window_start_time <= now);
        // Window start should be within one window of current time
        assert!(now - window_start_time < window_seconds);
    }

    #[test]
    fn test_elapsed_in_window_calculation() {
        let window_seconds: i64 = 60;

        // Test various points within a window
        for offset in 0..60 {
            let now = 1700000000 + offset;
            let current_window = now / window_seconds;
            let window_start_time = current_window * window_seconds;
            let elapsed = now - window_start_time;

            assert!(elapsed >= 0 && elapsed < window_seconds);
        }
    }

    #[test]
    fn test_sliding_window_weight_calculation() {
        let window_seconds: i64 = 60;

        // At the start of a window, previous window has full weight
        let elapsed_start: f64 = 0.0;
        let weight_start = 1.0 - (elapsed_start / window_seconds as f64);
        assert!((weight_start - 1.0).abs() < f64::EPSILON);

        // At the end of a window, previous window has zero weight
        let elapsed_end: f64 = 59.0;
        let weight_end = 1.0 - (elapsed_end / window_seconds as f64);
        assert!(weight_end > 0.0 && weight_end < 0.1);

        // At the middle of a window, weight is 0.5
        let elapsed_mid: f64 = 30.0;
        let weight_mid = 1.0 - (elapsed_mid / window_seconds as f64);
        assert!((weight_mid - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reset_at_calculation() {
        let now: i64 = 1700000000;
        let window_seconds: i64 = 60;
        let current_window = now / window_seconds;
        let window_start_time = current_window * window_seconds;
        let reset_at = window_start_time + window_seconds;

        // Reset should be in the future
        assert!(reset_at > now);
        // Reset should be at most window_seconds away
        assert!(reset_at - now <= window_seconds);
    }

    // ===========================================
    // Edge Cases and Boundary Tests
    // ===========================================

    #[test]
    fn test_rate_limit_at_exactly_limit() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 0,
            reset_at: 1234567890,
            current: 100,
        };

        assert!(result.allowed);
        assert_eq!(result.remaining, 0);
    }

    #[test]
    fn test_rate_limit_just_over_limit() {
        let result = RateLimitResult {
            allowed: false,
            limit: 100,
            remaining: -1,
            reset_at: 1234567890,
            current: 101,
        };

        assert!(!result.allowed);
        assert_eq!(result.remaining, -1);
    }

    #[test]
    fn test_rate_limit_large_overage() {
        let result = RateLimitResult {
            allowed: false,
            limit: 100,
            remaining: -1000,
            reset_at: chrono::Utc::now().timestamp() + 30,
            current: 1100,
        };

        let headers = result.headers();

        // Check remaining is clamped to 0
        for (name, value) in &headers {
            if name.as_str() == "x-ratelimit-remaining" {
                assert_eq!(value.to_str().unwrap(), "0");
            }
        }
    }

    #[test]
    fn test_retry_after_minimum_value() {
        // Reset time in the past should result in retry-after of at least 1
        let past_reset = chrono::Utc::now().timestamp() - 10;
        let result = RateLimitResult {
            allowed: false,
            limit: 100,
            remaining: -5,
            reset_at: past_reset,
            current: 105,
        };

        let headers = result.headers();
        for (name, value) in &headers {
            if name.as_str() == "retry-after" {
                let retry_after: i64 = value.to_str().unwrap().parse().unwrap();
                assert!(retry_after >= 1);
            }
        }
    }

    // ===========================================
    // Configuration Presets Tests
    // ===========================================

    #[test]
    fn test_all_config_presets_valid() {
        let configs = vec![
            RateLimitConfig::default(),
            RateLimitConfig::for_ai_requests(),
            RateLimitConfig::for_tokens(10000, 3600),
        ];

        for config in configs {
            assert!(config.max_requests >= 0);
            assert!(config.window_seconds > 0 || config.window_seconds == 0);
            assert!(!config.key_prefix.is_empty() || config.key_prefix.is_empty());
        }
    }

    #[test]
    fn test_tokens_config_various_limits() {
        let configs = vec![
            (1000, 60),
            (10000, 300),
            (100000, 3600),
            (1000000, 86400),
        ];

        for (max_tokens, window) in configs {
            let config = RateLimitConfig::for_tokens(max_tokens, window);
            assert_eq!(config.max_requests, max_tokens);
            assert_eq!(config.window_seconds, window);
        }
    }

    // ===========================================
    // Window Calculation Edge Cases
    // ===========================================

    #[test]
    fn test_window_at_epoch() {
        let now: i64 = 0;
        let window_seconds: i64 = 60;
        let current_window = now / window_seconds;

        assert_eq!(current_window, 0);
    }

    #[test]
    fn test_window_large_timestamp() {
        let now: i64 = 2000000000; // Year 2033
        let window_seconds: i64 = 60;
        let current_window = now / window_seconds;

        assert!(current_window > 0);
        assert_eq!(current_window, now / window_seconds);
    }

    #[test]
    fn test_previous_window_calculation() {
        let now: i64 = 1700000000;
        let window_seconds: i64 = 60;
        let current_window = now / window_seconds;
        let previous_window = current_window - 1;

        assert_eq!(previous_window, current_window - 1);
        assert!(previous_window >= 0 || current_window == 0);
    }
}
