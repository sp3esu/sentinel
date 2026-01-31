# Testing Patterns

**Analysis Date:** 2026-01-31

## Test Framework

**Runner:**
- Framework: `tokio` test runtime
- Test attribute: `#[tokio::test]` for async tests
- Config file: `Cargo.toml` (no separate test config)
- Features: `test-utils` feature gate enables integration test constructors

**Assertion Library:**
- `assert!()` and `assert_eq!()` macros for basic assertions
- `pretty_assertions` crate for detailed comparison output (dev-dependency)
- `axum_test` for HTTP handler testing with `TestServer`
- `wiremock` for HTTP mock server assertions

**Run Commands:**
```bash
cargo test                                      # Run all tests
cargo test --lib                               # Run unit tests only
cargo test --test integration_tests            # Run integration tests
cargo test --test integration_tests --features test-utils  # With test utilities
cargo test --lib -- --nocapture               # Show stdout in unit tests
cargo test token_tracking                      # Run specific test by name
cargo test -- --test-threads=1                # Run serially (useful for Redis tests)
```

## Test File Organization

**Location:**
- Unit tests: Inline in source files with `#[cfg(test)] mod tests { ... }`
- Integration tests: `tests/integration_tests.rs` (entry), `tests/integration/` (modules)
- Common utilities: `tests/common/mod.rs`
- Mock servers: `tests/mocks/` (zion.rs, openai.rs, redis.rs)

**Naming:**
- Unit test modules: `#[cfg(test)] mod tests`
- Test functions: `test_<feature>_<scenario>` (e.g., `test_default_values`, `test_chat_completion_non_streaming_tracks_tokens`)
- Integration test files: `<endpoint>.rs` (e.g., `chat_completions.rs`, `health.rs`)

**Structure:**
```
tests/
├── integration_tests.rs        # Entry point - declares modules
├── common/
│   └── mod.rs                  # Test constants and utilities
├── mocks/
│   ├── mod.rs                  # Mock exports
│   ├── zion.rs                 # Mock Zion API (wiremock)
│   ├── openai.rs               # Mock OpenAI API (wiremock)
│   └── redis.rs                # Redis test helpers
├── integration/
│   ├── mod.rs                  # Integration test aggregator
│   ├── health.rs               # Health endpoint tests
│   ├── chat_completions.rs     # Chat API tests
│   ├── models.rs               # Models endpoint tests
│   ├── rate_limiting.rs        # Rate limit tests
│   ├── token_tracking.rs       # Token usage tracking tests
│   ├── token_estimation_accuracy.rs  # Token count accuracy
│   └── debug.rs                # Debug endpoint tests
└── mocks_test.rs               # Mock validation tests
```

## Test Structure

**Suite Organization:**

```rust
#[tokio::test]
async fn test_feature_scenario() {
    // Setup: Create test harness with mocks
    let harness = TokenTrackingTestHarness::new().await;

    // Configure mocks
    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.openai.mock_chat_completion_with_usage("Response", 15, 25).await;

    // Execute: Make HTTP request
    let request = json!({"model": "gpt-4", "messages": [...]});
    let response = harness.server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .json(&request)
        .await;

    // Assert: Verify response and side effects
    response.assert_status_ok();
    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(2)).await;
    assert!(!requests.is_empty(), "Expected batch request");
}
```

**Patterns:**
- **Setup pattern:** `let harness = TestHarness::new().await;`
- **Configuration pattern:** `harness.zion.mock_*().await;`
- **Execution pattern:** `harness.server.post(...).await;`
- **Assertion pattern:** `response.assert_status_ok();` or `assert_eq!(value, expected);`
- **Teardown:** Automatic via test harness `Drop` impl

## Mocking

**Framework:** `wiremock` crate for HTTP mocking

**Patterns:**

Mock server creation:
```rust
use wiremock::MockServer;

let mock_server = MockServer::start().await;
let uri = mock_server.uri();  // e.g., http://127.0.0.1:12345
```

Setting up mocks:
```rust
use wiremock::{Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

Mock::given(method("GET"))
    .and(path("/api/v1/users/me"))
    .and(header("Authorization", format!("Bearer {}", token).as_str()))
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "data": { "id": "user_123", "email": "test@test.com" }
    })))
    .mount(&mock_server)
    .await;
```

Request verification:
```rust
let requests = harness.zion.received_requests().await;
assert!(!requests.is_empty(), "Expected Zion to receive requests");

// Filter to specific endpoint
let batch_requests = harness.zion.batch_increment_requests().await;
```

**What to Mock:**
- External HTTP APIs (Zion, OpenAI)
- HTTP responses with specific status codes and bodies
- Error responses (401 Unauthorized, 404 Not Found, 500 Internal Server Error)

**What NOT to Mock:**
- Redis connection (use real Redis for integration tests)
- Application state and handlers (test with real router)
- Internal service logic (test via public APIs)

## Fixtures and Factories

**Test Data:**

```rust
// Test constants in common/mod.rs
pub mod constants {
    pub const TEST_ZION_API_KEY: &str = "test-zion-api-key";
    pub const TEST_OPENAI_API_KEY: &str = "test-openai-api-key";
    pub const TEST_JWT_TOKEN: &str = "eyJhbGc...";
    pub const TEST_USER_ID: &str = "user_123";
    pub const TEST_EXTERNAL_ID: &str = "ext_123";
    pub const TEST_EMAIL: &str = "test@test.com";
}

// Factory functions for test objects
pub struct TestConfig {
    pub host: String,
    pub port: u16,
    // ...
}

impl TestConfig {
    pub fn new(zion_url: &str, openai_url: &str) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 0,  // Let OS assign
            // ...
        }
    }
}

// Zion test data builders
pub mod zion_mocks {
    pub async fn mock_jwt_validation(server: &MockServer) { ... }
    pub async fn mock_user_limits(server: &MockServer) { ... }
}
```

**Location:**
- Constants: `tests/common/mod.rs`
- Mock helpers: `tests/mocks/*.rs`
- Test data builders: `mocks/zion.rs`, `mocks/openai.rs`

**Helper Functions:**

```rust
// In test file
fn auth_header() -> String {
    format!("Bearer {}", constants::TEST_JWT_TOKEN)
}

fn test_profile() -> UserProfileMock {
    UserProfileMock {
        id: constants::TEST_USER_ID.to_string(),
        email: constants::TEST_EMAIL.to_string(),
        // ...
    }
}
```

## Coverage

**Requirements:** None enforced in CI

**View Coverage:**
```bash
cargo tarpaulin --out Html              # Generate HTML report
cargo tarpaulin --exclude-files tests/* # Exclude test code
```

**Target:** Implicit - prioritize integration tests for critical paths (token tracking, rate limiting)

## Test Types

**Unit Tests:**
- Scope: Individual functions and modules in isolation
- Approach: `#[cfg(test)] mod tests` within source files
- Example: `src/config.rs` tests default configuration values
- Pattern: Test inputs and outputs, no external dependencies

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        env::set_var("ZION_API_URL", "http://localhost:3000");
        env::set_var("ZION_API_KEY", "test-key");

        let config = Config::from_env().unwrap();

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);

        // Cleanup
        env::remove_var("ZION_API_URL");
        env::remove_var("ZION_API_KEY");
    }
}
```

**Integration Tests:**
- Scope: Full request/response cycle with real dependencies
- Approach: `tests/integration/` with test harness
- Dependencies: Redis (required), Mock HTTP servers (wiremock)
- Pattern: Test complete workflows (auth → rate limit → proxy → token tracking)

```rust
#[tokio::test]
async fn test_chat_completion_non_streaming_tracks_tokens() {
    let harness = TokenTrackingTestHarness::new().await;

    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits()).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage("Hello!", 15, 25).await;

    let request = json!({"model": "gpt-4", "messages": [...]});
    let response = harness.server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .json(&request)
        .await;

    response.assert_status_ok();

    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(2)).await;
    assert!(!requests.is_empty());
}
```

**E2E Tests:**
- Framework: Not used (integration tests serve this role)
- Alternative: Docker Compose setup in `docker-compose.yml` for manual testing

## Common Patterns

**Async Testing:**

All tests use `#[tokio::test]` for async runtime:
```rust
#[tokio::test]
async fn test_async_operation() {
    let result = some_async_function().await;
    assert!(result.is_ok());
}
```

Waiting for async side effects:
```rust
let requests = harness.wait_for_batch_requests(
    expected_count,
    Duration::from_secs(2)
).await;
```

**Error Testing:**

```rust
#[test]
fn test_invalid_config() {
    // Don't set required env vars
    let result = Config::from_env();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_upstream_error_handling() {
    let harness = TokenTrackingTestHarness::new().await;

    // Mock error response
    harness.openai.mock_chat_completion_error(500, "Internal Server Error").await;

    let response = harness.server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .json(&json!({"model": "gpt-4", "messages": [...]}))
        .await;

    response.assert_status(StatusCode::BAD_GATEWAY);
}
```

**Request Assertion Helper Pattern:**

```rust
// axum_test provides convenient assertion methods
response.assert_status_ok();                    // 200 OK
response.assert_status(StatusCode::UNAUTHORIZED); // Specific status
let body = response.json::<ChatCompletionResponse>();  // Parse response
```

**Mock Assertion Pattern:**

```rust
// Capture and verify requests sent to mock servers
let zion_requests = harness.zion.received_requests().await;
let batch_requests = harness.zion.batch_increment_requests().await;

// Extract and parse request body
let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
let (input_tokens, output_tokens, request_count) =
    TokenTrackingTestHarness::extract_token_counts(&increments[0]);
```

## Test Harness

**TokenTrackingTestHarness Pattern:**

Comprehensive test harness for integration tests:
```rust
pub struct TokenTrackingTestHarness {
    pub server: TestServer,        // Real axum router
    pub zion: MockZionServer,      // wiremock Zion API mock
    pub openai: MockOpenAIServer,  // wiremock OpenAI API mock
    pub redis: RedisConnection,    // Real Redis
}

impl TokenTrackingTestHarness {
    pub async fn new() -> Self {
        // Initialize all dependencies
    }

    pub async fn wait_for_batch_requests(
        &self,
        expected: usize,
        timeout: Duration
    ) -> Vec<wiremock::Request> {
        // Poll until requests received or timeout
    }

    pub fn parse_batch_payload(request: &wiremock::Request) -> Vec<serde_json::Value> {
        // Extract increments from batch request body
    }

    pub fn extract_token_counts(increment: &Value) -> (i64, i64, i64) {
        // Return (input_tokens, output_tokens, request_count)
    }
}
```

## Test Execution Requirements

**Dependencies Required:**
- Redis running on `localhost:6379` (for integration tests)
- No external internet access needed (all external APIs mocked)
- Tokio runtime provided by `#[tokio::test]`

**Environment Setup:**
- Test env vars set within tests (cleanup after)
- No `.env` file needed for tests
- Default config values provide safe test defaults

**Special Flags:**
- `--features test-utils` - Enable test constructor feature gate
- `--test-threads=1` - Run tests serially if Redis conflicts occur

## Mock Server Patterns

**Zion Mock (`tests/mocks/zion.rs`):**
- Provides builders for common responses
- Tracks received requests for assertion
- Filters requests by endpoint for validation

```rust
let mock_server = MockZionServer::start().await;
mock_server.mock_get_user_profile_success(profile).await;
mock_server.mock_get_limits_success(external_id, limits).await;
mock_server.mock_batch_increment_success(count, delay_ms).await;

let requests = mock_server.batch_increment_requests().await;
```

**OpenAI Mock (`tests/mocks/openai.rs`):**
- Simulates chat completion responses with usage fields
- Supports streaming and non-streaming
- Configurable delay for timeout testing

```rust
let mock_server = MockOpenAIServer::start().await;
mock_server.mock_chat_completion_with_usage(
    content: "Response text",
    input_tokens: 15,
    output_tokens: 25
).await;
```

---

*Testing analysis: 2026-01-31*
