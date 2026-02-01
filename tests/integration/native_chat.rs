//! Native Chat Completions Integration Tests
//!
//! Tests for the native chat completions endpoint:
//! - POST /native/v1/chat/completions - Chat completions in Native API format
//! - Request validation (tier, unknown fields)
//! - Streaming response format
//! - Error response format (NativeErrorResponse)
//! - Tier routing integration
//!
//! These tests verify that the Native API endpoints work correctly alongside
//! the existing /v1/* endpoints without breaking them.

use std::time::Duration;

use axum::http::{header, StatusCode};
use serde_json::json;

use crate::common::{constants, TokenTrackingTestHarness};
use crate::mocks::openai::OpenAITestData;
use crate::mocks::zion::{UserProfileMock, ZionTestData};

// =============================================================================
// Test Helpers
// =============================================================================

/// Helper to create authorization header value
fn auth_header() -> String {
    format!("Bearer {}", constants::TEST_JWT_TOKEN)
}

/// Create a test user profile
fn make_test_profile() -> UserProfileMock {
    UserProfileMock {
        id: constants::TEST_USER_ID.to_string(),
        email: constants::TEST_EMAIL.to_string(),
        name: Some("Test User".to_string()),
        external_id: Some(constants::TEST_EXTERNAL_ID.to_string()),
        email_verified: true,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        last_login_at: Some("2024-01-15T12:00:00Z".to_string()),
    }
}

// =============================================================================
// Non-Streaming Tests
// =============================================================================

#[tokio::test]
async fn test_native_chat_completions_non_streaming() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Hello! How can I help you today?", 15, 25)
        .await;

    // Send request with tier: moderate and stream: false
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "moderate",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "stream": false
        }))
        .await;

    response.assert_status_ok();

    // Verify X-Sentinel-* headers
    assert!(
        response.headers().get("X-Sentinel-Model").is_some(),
        "Should have X-Sentinel-Model header"
    );
    assert!(
        response.headers().get("X-Sentinel-Tier").is_some(),
        "Should have X-Sentinel-Tier header"
    );

    // Verify response structure
    let body: serde_json::Value = response.json();
    assert!(body.get("id").is_some(), "Response should have 'id' field");
    assert!(
        body.get("object").is_some(),
        "Response should have 'object' field"
    );
    assert!(
        body.get("choices").is_some(),
        "Response should have 'choices' field"
    );
    assert!(
        body.get("usage").is_some(),
        "Response should have 'usage' field"
    );

    // Verify usage structure
    let usage = body.get("usage").unwrap();
    assert!(
        usage.get("prompt_tokens").is_some(),
        "Usage should have prompt_tokens"
    );
    assert!(
        usage.get("completion_tokens").is_some(),
        "Usage should have completion_tokens"
    );
    assert!(
        usage.get("total_tokens").is_some(),
        "Usage should have total_tokens"
    );
}

#[tokio::test]
async fn test_native_chat_completions_default_non_streaming() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Hello!", 10, 5)
        .await;

    // Send request without tier/stream fields (tier defaults to simple, stream to false)
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }))
        .await;

    response.assert_status_ok();

    // Verify tier defaults to simple
    let tier_header = response.headers().get("X-Sentinel-Tier");
    assert!(tier_header.is_some(), "Should have X-Sentinel-Tier header");
    assert_eq!(tier_header.unwrap().to_str().unwrap(), "simple");

    let body: serde_json::Value = response.json();
    assert!(body.get("choices").is_some());
    assert!(body.get("usage").is_some());
}

#[tokio::test]
async fn test_native_chat_completions_with_system_message() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("I am a helpful assistant!", 20, 10)
        .await;

    // Send request with system message first (correct ordering)
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "moderate",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Who are you?"}
            ]
        }))
        .await;

    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert!(body.get("choices").is_some());
}

// =============================================================================
// Validation Error Tests
// =============================================================================

#[tokio::test]
async fn test_native_chat_completions_invalid_tier() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up auth mock (needed before validation)
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;

    // Send request with invalid tier value
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "invalid_tier",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);

    // Verify NativeErrorResponse format
    let body: serde_json::Value = response.json();
    let error = body.get("error").expect("Response should have 'error' field");
    assert!(
        error.get("message").is_some(),
        "Error should have 'message' field"
    );
    assert!(
        error.get("type").is_some(),
        "Error should have 'type' field"
    );
    assert_eq!(
        error.get("type").unwrap().as_str().unwrap(),
        "invalid_request_error",
        "Error type should be 'invalid_request_error'"
    );
}

#[tokio::test]
async fn test_native_chat_completions_unknown_field_rejected() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up auth mock
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;

    // Send request with unknown field (should be rejected due to deny_unknown_fields)
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "unknown_field": "should_be_rejected"
        }))
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);

    // Verify NativeErrorResponse format
    let body: serde_json::Value = response.json();
    let error = body.get("error").expect("Response should have 'error' field");
    assert_eq!(
        error.get("type").unwrap().as_str().unwrap(),
        "invalid_request_error"
    );
}

#[tokio::test]
async fn test_native_chat_completions_unauthorized() {
    let harness = TokenTrackingTestHarness::new().await;

    // No auth mock - request should fail at auth middleware
    harness.zion.mock_get_user_profile_unauthorized().await;

    // Send request with invalid token
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, "Bearer invalid-token".parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }))
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);

    // Verify error response format
    let body: serde_json::Value = response.json();
    let error = body.get("error").expect("Response should have 'error' field");
    assert!(error.get("message").is_some());
}

#[tokio::test]
async fn test_native_chat_completions_no_auth_header() {
    let harness = TokenTrackingTestHarness::new().await;

    // Send request without Authorization header
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }))
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// =============================================================================
// Streaming Tests
// =============================================================================

#[tokio::test]
async fn test_native_chat_completions_streaming() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;

    // Mock streaming response with usage in final chunk
    let chunks = OpenAITestData::streaming_chunks("Hello world this is a streaming response");
    harness.openai.mock_chat_completion_stream(chunks).await;

    // Send streaming request
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "stream": true
        }))
        .await;

    response.assert_status_ok();

    // Verify content type is text/event-stream
    let content_type = response.headers().get(header::CONTENT_TYPE);
    assert!(content_type.is_some(), "Should have Content-Type header");
    assert!(
        content_type
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/event-stream"),
        "Content-Type should be text/event-stream"
    );

    // Verify X-Sentinel-* headers are present in streaming response
    assert!(
        response.headers().get("X-Sentinel-Model").is_some(),
        "Streaming should have X-Sentinel-Model header"
    );
    assert!(
        response.headers().get("X-Sentinel-Tier").is_some(),
        "Streaming should have X-Sentinel-Tier header"
    );

    // Consume the stream and verify format
    let body = response.text();
    assert!(body.contains("data:"), "Response should be SSE format");
    assert!(body.contains("[DONE]"), "Stream should end with [DONE]");
}

#[tokio::test]
async fn test_native_chat_completions_streaming_usage_tracked() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;

    // Mock streaming response with usage
    let chunks = OpenAITestData::streaming_chunks("Test response");
    harness.openai.mock_chat_completion_stream(chunks).await;

    // Send streaming request
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "moderate",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "stream": true
        }))
        .await;

    response.assert_status_ok();

    // Consume stream to trigger usage tracking
    let _ = response.text();

    // Wait for Zion batch-increment (streaming may take longer)
    let requests = harness
        .wait_for_batch_requests(1, Duration::from_secs(3))
        .await;
    assert!(
        !requests.is_empty(),
        "Expected batch-increment request after streaming"
    );

    // Verify tokens were tracked
    let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
    let (input, output, req_count) = TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    assert!(
        input > 0 || output > 0,
        "Should have tracked some tokens for streaming"
    );
    assert_eq!(req_count, 1, "Request count should be 1");
}

// =============================================================================
// Regression Test
// =============================================================================

/// Regression test: Verify /v1/* endpoints are unaffected by native routes
///
/// This test exists as explicit documentation that adding /native/* routes
/// MUST NOT break existing /v1/* functionality. If this test fails after
/// changes to native_routes, the router configuration is wrong.
#[tokio::test]
async fn test_v1_endpoints_regression_check() {
    let harness = TokenTrackingTestHarness::new().await;

    // Test 1: Verify /v1/chat/completions still works
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Hello from v1!", 10, 5)
        .await;

    let v1_response = harness
        .server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Hello"}]
        }))
        .await;

    v1_response.assert_status_ok();

    // Test 2: Verify /native/v1/chat/completions also works (both coexist)
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Hello from native!", 10, 5)
        .await;

    let native_response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [{"role": "user", "content": "World"}]
        }))
        .await;

    native_response.assert_status_ok();

    // Verify both responses have expected structure
    let v1_body: serde_json::Value = v1_response.json();
    let native_body: serde_json::Value = native_response.json();

    assert!(v1_body.get("choices").is_some(), "/v1/* should return choices");
    assert!(
        native_body.get("choices").is_some(),
        "/native/* should return choices"
    );
}

// =============================================================================
// Session Management Tests
// =============================================================================

/// Test that a new session is created for a new conversation_id
#[tokio::test]
async fn test_session_creation_and_retrieval() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks for first request
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Hello! I'm here to help.", 15, 20)
        .await;

    // First request with conversation_id creates session
    let response1 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "conversation_id": "test-session-1"
        }))
        .await;

    response1.assert_status_ok();

    // Set up mocks for second request (session should exist now)
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("I remember our conversation!", 20, 25)
        .await;

    // Second request with same conversation_id reuses session
    let response2 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"},
                {"role": "assistant", "content": "Hello! I'm here to help."},
                {"role": "user", "content": "Thanks!"}
            ],
            "conversation_id": "test-session-1"
        }))
        .await;

    response2.assert_status_ok();

    // Both should have valid response structure
    let body1: serde_json::Value = response1.json();
    let body2: serde_json::Value = response2.json();
    assert!(body1.get("choices").is_some());
    assert!(body2.get("choices").is_some());
}

/// Test that session tier cannot be downgraded (stickiness)
#[tokio::test]
async fn test_session_tier_downgrade_prevented() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks for first request (creates session with moderate tier)
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Response from moderate tier", 10, 15)
        .await;

    // First request creates session with moderate tier
    let response1 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "moderate",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "conversation_id": "test-session-2"
        }))
        .await;

    response1.assert_status_ok();

    // Verify first response has moderate tier
    let tier1 = response1.headers().get("X-Sentinel-Tier")
        .expect("Should have X-Sentinel-Tier header")
        .to_str().unwrap();
    assert_eq!(tier1, "moderate");

    // Set up mocks for second request
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Still using moderate tier from session", 15, 20)
        .await;

    // Second request tries to use simple tier but session should use moderate
    let response2 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"},
                {"role": "assistant", "content": "Response from moderate tier"},
                {"role": "user", "content": "Continue"}
            ],
            "conversation_id": "test-session-2"
        }))
        .await;

    // Request should succeed - session tier (moderate) is used, not requested simple
    response2.assert_status_ok();

    // Verify session tier was preserved (downgrade prevented)
    let tier2 = response2.headers().get("X-Sentinel-Tier")
        .expect("Should have X-Sentinel-Tier header")
        .to_str().unwrap();
    assert_eq!(tier2, "moderate", "Tier downgrade should be prevented");

    let body: serde_json::Value = response2.json();
    assert!(body.get("choices").is_some());
}

/// Test stateless mode - requests without conversation_id work independently
#[tokio::test]
async fn test_no_conversation_id_stateless() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks for first request
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("First response", 10, 10)
        .await;

    // First request without conversation_id
    let response1 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }))
        .await;

    response1.assert_status_ok();

    // Set up mocks for second request
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Second response", 10, 10)
        .await;

    // Second request without conversation_id (completely independent)
    let response2 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "moderate",
            "messages": [
                {"role": "user", "content": "World!"}
            ]
        }))
        .await;

    response2.assert_status_ok();

    // Both should succeed independently
    let body1: serde_json::Value = response1.json();
    let body2: serde_json::Value = response2.json();
    assert!(body1.get("choices").is_some());
    assert!(body2.get("choices").is_some());
}

/// Test backward compatibility - tier defaults to simple when omitted
#[tokio::test]
async fn test_tier_defaults_to_simple() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Response with default tier", 10, 15)
        .await;

    // Request without tier field should default to simple
    let response = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "temperature": 0.7,
            "max_tokens": 100
        }))
        .await;

    response.assert_status_ok();

    // Verify tier defaults to simple
    let tier = response.headers().get("X-Sentinel-Tier")
        .expect("Should have X-Sentinel-Tier header")
        .to_str().unwrap();
    assert_eq!(tier, "simple", "Tier should default to simple");

    let body: serde_json::Value = response.json();
    assert!(body.get("choices").is_some());
    assert!(body.get("usage").is_some());
}

/// Test session tier upgrade works (simple -> moderate)
#[tokio::test]
async fn test_session_tier_upgrade() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks for first request (creates session with simple tier)
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Response from simple tier", 10, 15)
        .await;

    // First request creates session with simple tier
    let response1 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "conversation_id": "test-session-upgrade"
        }))
        .await;

    response1.assert_status_ok();
    assert_eq!(
        response1.headers().get("X-Sentinel-Tier").unwrap().to_str().unwrap(),
        "simple"
    );

    // Set up mocks for second request (upgrade to moderate)
    harness
        .zion
        .mock_get_user_profile_success(make_test_profile())
        .await;
    harness
        .zion
        .mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits())
        .await;
    harness.zion.mock_tier_config_success().await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness
        .openai
        .mock_chat_completion_with_usage("Response from moderate tier", 15, 20)
        .await;

    // Second request upgrades tier to moderate
    let response2 = harness
        .server
        .post("/native/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&json!({
            "tier": "moderate",
            "messages": [
                {"role": "user", "content": "Hello!"},
                {"role": "assistant", "content": "Response from simple tier"},
                {"role": "user", "content": "Need more complex answer"}
            ],
            "conversation_id": "test-session-upgrade"
        }))
        .await;

    response2.assert_status_ok();

    // Verify tier was upgraded
    assert_eq!(
        response2.headers().get("X-Sentinel-Tier").unwrap().to_str().unwrap(),
        "moderate",
        "Tier should be upgraded to moderate"
    );

    let body: serde_json::Value = response2.json();
    assert!(body.get("choices").is_some());
}
