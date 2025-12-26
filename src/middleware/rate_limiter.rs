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
    let mut conn = state.redis.clone();
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
    let mut conn = state.redis.clone();
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

    #[test]
    fn test_default_config() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window_seconds, 60);
    }

    #[test]
    fn test_rate_limit_key() {
        let key = rate_limit_key("sentinel:ratelimit", "user123", 1234567890);
        assert_eq!(key, "sentinel:ratelimit:user123:1234567890");
    }

    #[test]
    fn test_rate_limit_result_headers() {
        let result = RateLimitResult {
            allowed: true,
            limit: 100,
            remaining: 95,
            reset_at: 1234567890,
            current: 5,
        };

        let headers = result.headers();
        assert_eq!(headers.len(), 3); // limit, remaining, reset

        let result_exceeded = RateLimitResult {
            allowed: false,
            limit: 100,
            remaining: -5,
            reset_at: chrono::Utc::now().timestamp() + 30,
            current: 105,
        };

        let headers = result_exceeded.headers();
        assert_eq!(headers.len(), 4); // includes Retry-After
    }

    #[test]
    fn test_ai_requests_config() {
        let config = RateLimitConfig::for_ai_requests();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window_seconds, 60);
        assert!(config.key_prefix.contains("ai"));
    }
}
