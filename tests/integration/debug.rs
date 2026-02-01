//! Debug endpoints integration tests
//!
//! Tests to ensure debug endpoints are properly gated by SENTINEL_DEBUG flag.
//! Critical: These tests verify debug routes are NOT accessible in production
//! (when debug_enabled is false).

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::Value;
use std::sync::Arc;

use sentinel::{
    routes, AiProvider, AppState, BatchingUsageTracker, Config, OpenAIProvider,
    ZionClient,
};

use crate::common::constants;
use crate::mocks::{MockOpenAI, MockZionServer};

/// Test harness for debug endpoint tests
///
/// Creates a minimal test environment with configurable debug_enabled flag.
/// Uses in-memory cache - no Redis required.
struct DebugTestHarness {
    server: TestServer,
    #[allow(dead_code)]
    openai: MockOpenAI,
    #[allow(dead_code)]
    zion: MockZionServer,
}

impl DebugTestHarness {
    /// Create a new test harness with specified debug_enabled setting
    async fn new(debug_enabled: bool) -> Self {
        // Start mock servers
        let openai = MockOpenAI::start().await;
        let zion = MockZionServer::start().await;

        // Create config with specified debug_enabled
        let config = Config {
            host: "127.0.0.1".to_string(),
            port: 0,
            redis_url: "redis://localhost:6379".to_string(), // Not used in test mode
            zion_api_url: zion.uri(),
            zion_api_key: constants::TEST_ZION_API_KEY.to_string(),
            openai_api_url: format!("{}/v1", openai.uri()),
            openai_api_key: Some(constants::TEST_OPENAI_API_KEY.to_string()),
            cache_ttl_seconds: 60,
            jwt_cache_ttl_seconds: 60,
            session_ttl_seconds: 86400,
            debug_enabled,
        };

        // Create HTTP client
        let http_client = reqwest::Client::new();

        // Create Zion client pointing to mock
        let zion_client = Arc::new(ZionClient::new(http_client.clone(), &config));

        // Create batching tracker (test version without Redis retry)
        let batching_tracker = Arc::new(BatchingUsageTracker::new_for_testing(zion_client.clone()));

        // Create AI provider pointing to mock
        let ai_provider: Arc<dyn AiProvider> = Arc::new(OpenAIProvider::new(http_client, &config));

        // Create app state with in-memory cache (no Redis required)
        let state = Arc::new(
            AppState::new_for_testing(config, zion_client, ai_provider, batching_tracker).await,
        );

        // Create router
        let app = routes::create_router(state);

        // Create test server
        let server = TestServer::new(app).expect("Failed to create test server");

        Self { server, openai, zion }
    }
}

// =============================================================================
// Tests: Debug endpoints return 404 when disabled (production behavior)
// =============================================================================

#[tokio::test]
async fn test_debug_cache_returns_404_when_disabled() {
    let harness = DebugTestHarness::new(false).await;

    let response = harness.server.get("/debug/cache").await;

    response.assert_status(StatusCode::NOT_FOUND);

    let json: Value = response.json();
    assert!(
        json.get("error").is_some(),
        "Response should have 'error' field"
    );
    assert_eq!(
        json["error"]["code"].as_str().unwrap(),
        "debug_disabled",
        "Error code should be 'debug_disabled'"
    );
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("SENTINEL_DEBUG"),
        "Error message should mention SENTINEL_DEBUG"
    );
}

#[tokio::test]
async fn test_debug_auth_returns_404_when_disabled() {
    let harness = DebugTestHarness::new(false).await;

    let response = harness.server.get("/debug/auth/test_user_123").await;

    response.assert_status(StatusCode::NOT_FOUND);

    let json: Value = response.json();
    assert_eq!(json["error"]["code"].as_str().unwrap(), "debug_disabled");
}

#[tokio::test]
async fn test_debug_config_returns_404_when_disabled() {
    let harness = DebugTestHarness::new(false).await;

    let response = harness.server.get("/debug/config").await;

    response.assert_status(StatusCode::NOT_FOUND);

    let json: Value = response.json();
    assert_eq!(json["error"]["code"].as_str().unwrap(), "debug_disabled");
}

// =============================================================================
// Tests: Debug endpoints return data when enabled (development behavior)
// =============================================================================

#[tokio::test]
async fn test_debug_cache_returns_data_when_enabled() {
    let harness = DebugTestHarness::new(true).await;

    let response = harness.server.get("/debug/cache").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Verify response structure
    assert!(
        json.get("limits_cache").is_some(),
        "Response should have 'limits_cache' field"
    );
    assert!(
        json.get("jwt_cache").is_some(),
        "Response should have 'jwt_cache' field"
    );
    assert!(
        json.get("profile_cache").is_some(),
        "Response should have 'profile_cache' field"
    );
    assert!(
        json.get("redis_available").is_some(),
        "Response should have 'redis_available' field"
    );

    // In test mode without Redis, redis_available should be false
    assert_eq!(
        json["redis_available"].as_bool().unwrap(),
        false,
        "Redis should not be available in test mode"
    );
}

#[tokio::test]
async fn test_debug_auth_returns_data_when_enabled() {
    let harness = DebugTestHarness::new(true).await;

    let response = harness.server.get("/debug/auth/test_user_123").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Verify response structure
    assert!(
        json.get("external_id").is_some(),
        "Response should have 'external_id' field"
    );
    assert!(
        json.get("cached_limits").is_some(),
        "Response should have 'cached_limits' field"
    );
    assert!(
        json.get("cache_ttl_remaining_seconds").is_some(),
        "Response should have 'cache_ttl_remaining_seconds' field"
    );

    // Verify external_id matches the path parameter
    assert_eq!(
        json["external_id"].as_str().unwrap(),
        "test_user_123",
        "external_id should match path parameter"
    );

    // Without Redis, cached_limits should be null and TTL should be -2
    assert!(
        json["cached_limits"].is_null(),
        "cached_limits should be null without Redis"
    );
    assert_eq!(
        json["cache_ttl_remaining_seconds"].as_i64().unwrap(),
        -2,
        "TTL should be -2 without Redis"
    );
}

#[tokio::test]
async fn test_debug_config_returns_data_when_enabled() {
    let harness = DebugTestHarness::new(true).await;

    let response = harness.server.get("/debug/config").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Verify response structure
    assert!(
        json.get("zion_api_url").is_some(),
        "Response should have 'zion_api_url' field"
    );
    assert!(
        json.get("openai_api_url").is_some(),
        "Response should have 'openai_api_url' field"
    );
    assert!(
        json.get("cache_ttl_seconds").is_some(),
        "Response should have 'cache_ttl_seconds' field"
    );
    assert!(
        json.get("jwt_cache_ttl_seconds").is_some(),
        "Response should have 'jwt_cache_ttl_seconds' field"
    );
    assert!(
        json.get("redis_connected").is_some(),
        "Response should have 'redis_connected' field"
    );
    assert!(
        json.get("debug_enabled").is_some(),
        "Response should have 'debug_enabled' field"
    );

    // debug_enabled should be true since we enabled it
    assert_eq!(
        json["debug_enabled"].as_bool().unwrap(),
        true,
        "debug_enabled should be true"
    );

    // redis_connected should be false in test mode
    assert_eq!(
        json["redis_connected"].as_bool().unwrap(),
        false,
        "redis_connected should be false in test mode"
    );
}

// =============================================================================
// Edge case tests
// =============================================================================

#[tokio::test]
async fn test_debug_auth_with_special_characters_in_id() {
    let harness = DebugTestHarness::new(true).await;

    // Test with URL-encoded special characters
    let response = harness.server.get("/debug/auth/user%40example.com").await;

    response.assert_status_ok();

    let json: Value = response.json();
    assert_eq!(
        json["external_id"].as_str().unwrap(),
        "user@example.com",
        "external_id should decode URL-encoded characters"
    );
}

#[tokio::test]
async fn test_debug_endpoints_not_under_v1_prefix() {
    let harness = DebugTestHarness::new(true).await;

    // Debug endpoints should NOT be under /v1/ prefix
    // /v1/* routes require authentication, so they return 401 (not 404)
    // This confirms debug endpoints are NOT registered under /v1/
    let response = harness.server.get("/v1/debug/cache").await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // Debug endpoints should be at root level (not /v1/)
    let response = harness.server.get("/debug/cache").await;
    response.assert_status_ok();
}
