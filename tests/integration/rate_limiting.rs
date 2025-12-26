//! Rate limiting integration tests
//!
//! Tests for the rate limiting middleware:
//! - Basic rate limiting behavior (allowed/denied)
//! - Rate limit headers (X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Reset)
//! - 429 Too Many Requests responses with Retry-After header
//! - Sliding window algorithm behavior
//! - Per-user rate limit isolation

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use axum_test::TestServer;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

/// Test helper to connect to Redis (skips test if unavailable)
async fn get_test_redis() -> Option<redis::aio::ConnectionManager> {
    let client = redis::Client::open("redis://127.0.0.1:6379").ok()?;
    client.get_connection_manager().await.ok()
}

/// Clean up rate limit keys for a specific test namespace
async fn cleanup_rate_limit_keys(conn: &mut redis::aio::ConnectionManager, prefix: &str) {
    let pattern = format!("{}*", prefix);
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(&pattern)
        .query_async(conn)
        .await
        .unwrap_or_default();

    for key in keys {
        let _: redis::RedisResult<()> = conn.del(&key).await;
    }
}

/// Generate a unique test prefix to avoid collisions between tests
fn test_prefix(test_name: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("sentinel:test:ratelimit:{}:{}", test_name, timestamp)
}

/// Simple mock state for testing
struct MockAppState {
    redis: redis::aio::ConnectionManager,
}

/// Rate limit configuration for tests
#[derive(Clone)]
struct TestRateLimitConfig {
    max_requests: i64,
    window_seconds: u64,
    key_prefix: String,
}

/// Rate limit result
#[derive(Debug, Clone)]
struct RateLimitResult {
    allowed: bool,
    limit: i64,
    remaining: i64,
    reset_at: i64,
    current: i64,
}

impl RateLimitResult {
    fn headers(&self) -> Vec<(header::HeaderName, header::HeaderValue)> {
        let mut headers = vec![
            (
                header::HeaderName::from_static("x-ratelimit-limit"),
                header::HeaderValue::from_str(&self.limit.to_string()).unwrap(),
            ),
            (
                header::HeaderName::from_static("x-ratelimit-remaining"),
                header::HeaderValue::from_str(&self.remaining.max(0).to_string()).unwrap(),
            ),
            (
                header::HeaderName::from_static("x-ratelimit-reset"),
                header::HeaderValue::from_str(&self.reset_at.to_string()).unwrap(),
            ),
        ];

        if !self.allowed {
            let retry_after = (self.reset_at - chrono::Utc::now().timestamp()).max(1);
            headers.push((
                header::RETRY_AFTER,
                header::HeaderValue::from_str(&retry_after.to_string()).unwrap(),
            ));
        }

        headers
    }
}

/// Check rate limit using sliding window algorithm
async fn check_rate_limit(
    conn: &mut redis::aio::ConnectionManager,
    user_id: &str,
    config: &TestRateLimitConfig,
) -> RateLimitResult {
    let now = chrono::Utc::now().timestamp();
    let window_seconds = config.window_seconds as i64;
    let current_window = now / window_seconds;
    let previous_window = current_window - 1;
    let window_start_time = current_window * window_seconds;
    let elapsed_in_window = now - window_start_time;

    let current_key = format!("{}:{}:{}", config.key_prefix, user_id, current_window);
    let previous_key = format!("{}:{}:{}", config.key_prefix, user_id, previous_window);

    // Get previous window count
    let previous_count: i64 = conn.get(&previous_key).await.unwrap_or(0);

    // Atomically increment current window
    let (current_count,): (i64,) = redis::pipe()
        .atomic()
        .incr(&current_key, 1i64)
        .expire(&current_key, (config.window_seconds * 2) as i64)
        .ignore()
        .query_async(conn)
        .await
        .unwrap();

    // Calculate sliding window count
    let weight = 1.0 - (elapsed_in_window as f64 / window_seconds as f64);
    let weighted_previous = (previous_count as f64 * weight) as i64;
    let total_count = current_count + weighted_previous;

    let allowed = total_count <= config.max_requests;
    let remaining = config.max_requests - total_count;
    let reset_at = window_start_time + window_seconds;

    RateLimitResult {
        allowed,
        limit: config.max_requests,
        remaining,
        reset_at,
        current: total_count,
    }
}

/// Error response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<ErrorDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    used: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remaining: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reset_at: Option<String>,
}

/// Simple response type for tests
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestResponse {
    status: String,
    message: String,
}

/// Create a test router with rate limiting middleware
fn create_rate_limit_test_router(
    redis: redis::aio::ConnectionManager,
    config: TestRateLimitConfig,
) -> Router {
    let state = Arc::new(MockAppState { redis });

    async fn handler() -> (StatusCode, Json<TestResponse>) {
        (
            StatusCode::OK,
            Json(TestResponse {
                status: "ok".to_string(),
                message: "Request processed successfully".to_string(),
            }),
        )
    }

    // Create rate limiting middleware
    let rate_limit_middleware = {
        let config = config.clone();
        move |State(state): State<Arc<MockAppState>>,
              request: Request,
              next: Next| {
            let config = config.clone();
            async move {
                // Extract user ID from header (for testing)
                let user_id = request
                    .headers()
                    .get("x-user-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("anonymous")
                    .to_string();

                let mut conn = state.redis.clone();

                let result = check_rate_limit(&mut conn, &user_id, &config).await;

                if !result.allowed {
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

                    let mut response =
                        (StatusCode::TOO_MANY_REQUESTS, Json(error_response)).into_response();

                    let headers = response.headers_mut();
                    for (name, value) in result.headers() {
                        headers.insert(name, value);
                    }

                    return response;
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
        }
    };

    Router::new()
        .route("/api/test", post(handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .with_state(state)
}

// =============================================================================
// Basic Rate Limiting Behavior Tests
// =============================================================================

#[tokio::test]
async fn test_request_allowed_when_under_limit() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("under_limit");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 10,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user123".parse().unwrap())
        .json(&json!({"message": "hello"}))
        .await;

    response.assert_status_ok();

    let json: Value = response.json();
    assert_eq!(json["status"].as_str().unwrap(), "ok");

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_rate_limit_headers_present_in_response() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("headers_present");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 100,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user456".parse().unwrap())
        .json(&json!({"message": "hello"}))
        .await;

    response.assert_status_ok();

    // Check that rate limit headers are present
    let headers = response.headers();

    assert!(
        headers.get("x-ratelimit-limit").is_some(),
        "Should have X-RateLimit-Limit header"
    );
    assert!(
        headers.get("x-ratelimit-remaining").is_some(),
        "Should have X-RateLimit-Remaining header"
    );
    assert!(
        headers.get("x-ratelimit-reset").is_some(),
        "Should have X-RateLimit-Reset header"
    );

    // Verify header values
    let limit = headers
        .get("x-ratelimit-limit")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(limit, "100", "Limit should be 100");

    let remaining = headers
        .get("x-ratelimit-remaining")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert!(remaining < 100, "Remaining should be less than limit after first request");

    let reset = headers
        .get("x-ratelimit-reset")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert!(reset > 0, "Reset timestamp should be positive");

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_429_returned_when_rate_limit_exceeded() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("limit_exceeded");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    // Very low limit to easily exceed
    let config = TestRateLimitConfig {
        max_requests: 3,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Make requests up to the limit
    for i in 0..3 {
        let response = server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_limit".parse().unwrap())
            .json(&json!({"message": format!("request {}", i)}))
            .await;
        response.assert_status_ok();
    }

    // This request should be rate limited
    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_limit".parse().unwrap())
        .json(&json!({"message": "should be rate limited"}))
        .await;

    response.assert_status(StatusCode::TOO_MANY_REQUESTS);

    let json: Value = response.json();
    assert!(json.get("error").is_some(), "Should have error field");
    assert_eq!(
        json["error"]["code"].as_str().unwrap(),
        "RATE_LIMIT_EXCEEDED"
    );

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_retry_after_header_present_on_429() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("retry_after");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 2,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Exhaust the limit
    for _ in 0..2 {
        server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_retry".parse().unwrap())
            .json(&json!({"message": "hello"}))
            .await;
    }

    // This should be rate limited
    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_retry".parse().unwrap())
        .json(&json!({"message": "rate limited"}))
        .await;

    response.assert_status(StatusCode::TOO_MANY_REQUESTS);

    // Check Retry-After header
    let headers = response.headers();
    assert!(
        headers.get("retry-after").is_some(),
        "Should have Retry-After header on 429 response"
    );

    let retry_after = headers
        .get("retry-after")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert!(
        retry_after >= 1,
        "Retry-After should be at least 1 second"
    );
    assert!(
        retry_after <= 60,
        "Retry-After should not exceed window size"
    );

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_rate_limit_error_response_structure() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("error_structure");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 1,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Use up the limit
    server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_error".parse().unwrap())
        .json(&json!({"message": "hello"}))
        .await;

    // Get rate limited response
    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_error".parse().unwrap())
        .json(&json!({"message": "rate limited"}))
        .await;

    response.assert_status(StatusCode::TOO_MANY_REQUESTS);

    let json: Value = response.json();

    // Verify error structure
    assert!(json.get("error").is_some(), "Should have error field");

    let error = &json["error"];
    assert!(error.get("code").is_some(), "Error should have code");
    assert!(error.get("message").is_some(), "Error should have message");
    assert!(error.get("details").is_some(), "Error should have details");

    let details = &error["details"];
    assert!(details.get("limit").is_some(), "Details should have limit");
    assert!(details.get("used").is_some(), "Details should have used");
    assert!(details.get("remaining").is_some(), "Details should have remaining");
    assert!(details.get("reset_at").is_some(), "Details should have reset_at");

    // Verify values make sense
    let limit = details["limit"].as_i64().unwrap();
    let used = details["used"].as_i64().unwrap();
    let remaining = details["remaining"].as_i64().unwrap();

    assert_eq!(limit, 1, "Limit should be 1");
    assert!(used > limit, "Used should exceed limit when rate limited");
    assert_eq!(remaining, 0, "Remaining should be 0 (clamped)");

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

// =============================================================================
// Sliding Window Algorithm Tests
// =============================================================================

#[tokio::test]
async fn test_multiple_requests_within_window_are_counted() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("window_count");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 10,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Make 5 requests
    for i in 0..5 {
        let response = server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_count".parse().unwrap())
            .json(&json!({"message": format!("request {}", i)}))
            .await;
        response.assert_status_ok();
    }

    // Check the remaining count after 5 requests
    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_count".parse().unwrap())
        .json(&json!({"message": "check remaining"}))
        .await;

    response.assert_status_ok();

    let headers = response.headers();
    let remaining = headers
        .get("x-ratelimit-remaining")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();

    // After 6 requests with limit of 10, remaining should be around 4
    // (allowing for sliding window calculation variance)
    assert!(
        remaining <= 4,
        "Remaining should be at most 4 after 6 requests (got {})",
        remaining
    );
    assert!(
        remaining >= 0,
        "Remaining should be non-negative"
    );

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_remaining_decrements_with_each_request() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("decrement");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 20,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    let mut previous_remaining: Option<i64> = None;

    // Make several requests and verify remaining decrements
    for i in 0..5 {
        let response = server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_dec".parse().unwrap())
            .json(&json!({"message": format!("request {}", i)}))
            .await;

        response.assert_status_ok();

        let headers = response.headers();
        let remaining = headers
            .get("x-ratelimit-remaining")
            .unwrap()
            .to_str()
            .unwrap()
            .parse::<i64>()
            .unwrap();

        if let Some(prev) = previous_remaining {
            assert!(
                remaining < prev,
                "Remaining should decrease: {} should be less than {}",
                remaining,
                prev
            );
        }

        previous_remaining = Some(remaining);
    }

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

// =============================================================================
// Per-User Isolation Tests
// =============================================================================

#[tokio::test]
async fn test_different_users_have_separate_rate_limits() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("user_isolation");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 5,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Exhaust rate limit for user_a
    for _ in 0..5 {
        server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_a".parse().unwrap())
            .json(&json!({"message": "hello"}))
            .await;
    }

    // user_a should be rate limited
    let response_a = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_a".parse().unwrap())
        .json(&json!({"message": "rate limited"}))
        .await;

    response_a.assert_status(StatusCode::TOO_MANY_REQUESTS);

    // user_b should NOT be rate limited (separate counter)
    let response_b = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_b".parse().unwrap())
        .json(&json!({"message": "should work"}))
        .await;

    response_b.assert_status_ok();

    let json: Value = response_b.json();
    assert_eq!(json["status"].as_str().unwrap(), "ok");

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_one_user_hitting_limit_does_not_affect_another() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("no_cross_impact");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 3,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Interleave requests from two users
    for i in 0..3 {
        // User X makes a request
        server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_x".parse().unwrap())
            .json(&json!({"message": format!("user_x request {}", i)}))
            .await;

        // User Y makes a request
        server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_y".parse().unwrap())
            .json(&json!({"message": format!("user_y request {}", i)}))
            .await;
    }

    // Both users should now be at their limit
    // User X should be rate limited
    let response_x = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_x".parse().unwrap())
        .json(&json!({"message": "user_x rate limited"}))
        .await;

    response_x.assert_status(StatusCode::TOO_MANY_REQUESTS);

    // User Y should also be rate limited
    let response_y = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_y".parse().unwrap())
        .json(&json!({"message": "user_y rate limited"}))
        .await;

    response_y.assert_status(StatusCode::TOO_MANY_REQUESTS);

    // User Z (new user) should NOT be rate limited
    let response_z = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_z".parse().unwrap())
        .json(&json!({"message": "user_z should work"}))
        .await;

    response_z.assert_status_ok();

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_anonymous_users_share_rate_limit() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("anonymous");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 3,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Make requests without x-user-id header (anonymous)
    for _ in 0..3 {
        let response = server
            .post("/api/test")
            .json(&json!({"message": "anonymous"}))
            .await;
        response.assert_status_ok();
    }

    // Next anonymous request should be rate limited
    let response = server
        .post("/api/test")
        .json(&json!({"message": "should be rate limited"}))
        .await;

    response.assert_status(StatusCode::TOO_MANY_REQUESTS);

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

// =============================================================================
// Edge Cases Tests
// =============================================================================

#[tokio::test]
async fn test_rate_limit_at_exactly_limit() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("exact_limit");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 5,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // Make exactly 5 requests (the limit)
    for i in 0..5 {
        let response = server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_exact".parse().unwrap())
            .json(&json!({"message": format!("request {}", i)}))
            .await;

        if i < 5 {
            response.assert_status_ok();
        }
    }

    // The 6th request should be rate limited
    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_exact".parse().unwrap())
        .json(&json!({"message": "6th request"}))
        .await;

    response.assert_status(StatusCode::TOO_MANY_REQUESTS);

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_rate_limit_with_single_request_limit() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("single_limit");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 1,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    // First request should succeed
    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_single".parse().unwrap())
        .json(&json!({"message": "first"}))
        .await;

    response.assert_status_ok();

    // Second request should be rate limited
    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_single".parse().unwrap())
        .json(&json!({"message": "second"}))
        .await;

    response.assert_status(StatusCode::TOO_MANY_REQUESTS);

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_rate_limit_headers_format() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("headers_format");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 100,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server
        .post("/api/test")
        .add_header("x-user-id".parse().unwrap(), "user_format".parse().unwrap())
        .json(&json!({"message": "hello"}))
        .await;

    response.assert_status_ok();

    let headers = response.headers();

    // Verify all rate limit headers are numeric strings
    let limit = headers.get("x-ratelimit-limit").unwrap().to_str().unwrap();
    assert!(
        limit.parse::<i64>().is_ok(),
        "X-RateLimit-Limit should be numeric"
    );

    let remaining = headers.get("x-ratelimit-remaining").unwrap().to_str().unwrap();
    assert!(
        remaining.parse::<i64>().is_ok(),
        "X-RateLimit-Remaining should be numeric"
    );

    let reset = headers.get("x-ratelimit-reset").unwrap().to_str().unwrap();
    assert!(
        reset.parse::<i64>().is_ok(),
        "X-RateLimit-Reset should be a Unix timestamp"
    );

    // Reset should be in the future
    let reset_ts = reset.parse::<i64>().unwrap();
    let now = chrono::Utc::now().timestamp();
    assert!(
        reset_ts >= now,
        "Reset timestamp should be in the future (reset: {}, now: {})",
        reset_ts,
        now
    );

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}

#[tokio::test]
async fn test_many_rapid_requests() {
    let redis = match get_test_redis().await {
        Some(r) => r,
        None => {
            eprintln!("Skipping test: Redis not available");
            return;
        }
    };

    let prefix = test_prefix("rapid");
    let mut conn = redis.clone();
    cleanup_rate_limit_keys(&mut conn, &prefix).await;

    let config = TestRateLimitConfig {
        max_requests: 50,
        window_seconds: 60,
        key_prefix: prefix.clone(),
    };

    let app = create_rate_limit_test_router(redis, config);
    let server = TestServer::new(app).expect("Failed to create test server");

    let mut success_count = 0;
    let mut rate_limited_count = 0;

    // Make many requests rapidly
    for _ in 0..60 {
        let response = server
            .post("/api/test")
            .add_header("x-user-id".parse().unwrap(), "user_rapid".parse().unwrap())
            .json(&json!({"message": "rapid"}))
            .await;

        if response.status_code() == StatusCode::OK {
            success_count += 1;
        } else if response.status_code() == StatusCode::TOO_MANY_REQUESTS {
            rate_limited_count += 1;
        }
    }

    // Should have approximately 50 successes and 10 rate limited
    assert!(
        success_count <= 50,
        "Should not have more than 50 successes (got {})",
        success_count
    );
    assert!(
        rate_limited_count >= 10,
        "Should have at least 10 rate limited (got {})",
        rate_limited_count
    );

    // Cleanup
    cleanup_rate_limit_keys(&mut conn, &prefix).await;
}
