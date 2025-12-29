//! Token Tracking Blackbox Tests
//!
//! These tests verify that token counts are always sent to the Zion API
//! when processing AI requests. They use a complete test harness with:
//! - Mock OpenAI server (wiremock)
//! - Mock Zion server (wiremock) with request capture
//! - Real Redis connection for caching
//! - Real app router with all middleware
//!
//! Run these tests with:
//! ```bash
//! cargo test --test integration_tests token_tracking --features test-utils
//! ```
//!
//! Note: These tests require Redis to be running locally.

use std::time::Duration;
use axum::http::header;
use serde_json::json;

use crate::common::{constants, TokenTrackingTestHarness};
use crate::mocks::zion::{ZionTestData, UserProfileMock};
use crate::mocks::openai::OpenAITestData;

/// Helper to create authorization header value
fn auth_header() -> String {
    format!("Bearer {}", constants::TEST_JWT_TOKEN)
}

/// Create a test user profile
fn test_profile() -> UserProfileMock {
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
// Non-Streaming Chat Completions Tests
// =============================================================================

#[tokio::test]
async fn test_chat_completion_non_streaming_tracks_tokens() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits()).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage("Hello! How can I help?", 15, 25).await;

    // Send chat completion request
    let request = json!({
        "model": "gpt-4",
        "messages": [
            {"role": "user", "content": "Hello, how are you?"}
        ]
    });

    let response = harness.server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&request)
        .await;

    response.assert_status_ok();

    // Wait for Zion batch-increment
    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(2)).await;
    assert!(!requests.is_empty(), "Expected at least one batch-increment request");

    // Parse and verify the payload
    let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
    assert!(!increments.is_empty(), "Expected at least one increment in batch");

    // Verify token counts
    let (input, output, req_count) = TokenTrackingTestHarness::extract_token_counts(&increments[0]);
    assert!(input > 0, "Input tokens should be > 0, got {}", input);
    assert!(output > 0, "Output tokens should be > 0, got {}", output);
    assert_eq!(req_count, 1, "Request count should be 1");

    // Verify email
    let email = increments[0]["email"].as_str().unwrap_or("");
    assert_eq!(email, constants::TEST_EMAIL, "Email should match authenticated user");
}

#[tokio::test]
async fn test_chat_completion_token_counts_match_openai_usage() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks with specific token counts
    let expected_input = 42;
    let expected_output = 128;

    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits()).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage(
        "This is a longer response with more tokens.",
        expected_input,
        expected_output,
    ).await;

    let request = json!({
        "model": "gpt-4",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Tell me about Rust programming."}
        ]
    });

    let response = harness.server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&request)
        .await;

    response.assert_status_ok();

    // Wait for Zion batch-increment
    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(2)).await;
    let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
    let (input, output, _) = TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    // Token counts should match what OpenAI returned
    assert_eq!(input, expected_input as i64, "Input tokens should match OpenAI usage");
    assert_eq!(output, expected_output as i64, "Output tokens should match OpenAI usage");
}

// =============================================================================
// Streaming Chat Completions Tests
// =============================================================================

#[tokio::test]
async fn test_chat_completion_streaming_tracks_tokens() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits()).await;
    harness.zion.mock_batch_increment_success(1, 0).await;

    // Mock streaming response with usage in final chunk
    let chunks = OpenAITestData::streaming_chunks("Hello world this is a streaming response");
    harness.openai.mock_chat_completion_stream(chunks).await;

    let request = json!({
        "model": "gpt-4",
        "messages": [
            {"role": "user", "content": "Hello!"}
        ],
        "stream": true
    });

    let response = harness.server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&request)
        .await;

    response.assert_status_ok();

    // Consume the stream by reading the body
    let body = response.text();
    assert!(body.contains("data:"), "Response should be SSE format");
    assert!(body.contains("[DONE]"), "Stream should complete");

    // Wait for Zion batch-increment (streaming may take a bit longer)
    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(3)).await;
    assert!(!requests.is_empty(), "Expected batch-increment request after streaming");

    // Parse and verify the payload
    let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
    let (input, output, req_count) = TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    assert!(input > 0, "Input tokens should be > 0 for streaming, got {}", input);
    assert!(output > 0, "Output tokens should be > 0 for streaming, got {}", output);
    assert_eq!(req_count, 1, "Request count should be 1");
}

// =============================================================================
// Completions Endpoint Tests
// =============================================================================

#[tokio::test]
async fn test_completions_endpoint_tracks_tokens() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits()).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_completion_with_usage("The quick brown fox", 10, 20).await;

    let request = json!({
        "model": "gpt-3.5-turbo-instruct",
        "prompt": "Complete this: The quick brown"
    });

    let response = harness.server
        .post("/v1/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&request)
        .await;

    response.assert_status_ok();

    // Wait for Zion batch-increment
    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(2)).await;
    assert!(!requests.is_empty(), "Expected batch-increment request");

    let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
    let (input, output, req_count) = TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    assert!(input > 0, "Input tokens should be > 0");
    assert!(output > 0, "Output tokens should be > 0");
    assert_eq!(req_count, 1, "Request count should be 1");
}

// =============================================================================
// Token Count Validation Tests
// =============================================================================

#[tokio::test]
async fn test_token_counts_never_zero_for_valid_requests() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks with minimal but non-zero response
    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits()).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage("OK", 3, 1).await;

    // Minimal valid request
    let request = json!({
        "model": "gpt-4",
        "messages": [
            {"role": "user", "content": "Hi"}
        ]
    });

    let response = harness.server
        .post("/v1/chat/completions")
        .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
        .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
        .json(&request)
        .await;

    response.assert_status_ok();

    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(2)).await;
    let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
    let (input, output, req_count) = TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    // Even with minimal input, tokens should never be zero
    assert!(input > 0, "Input tokens should never be zero");
    assert!(output > 0, "Output tokens should never be zero");
    assert!(req_count > 0, "Request count should never be zero");
}

// =============================================================================
// Multiple Requests Tests
// =============================================================================

#[tokio::test]
async fn test_multiple_requests_tracked_correctly() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks for multiple requests
    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(constants::TEST_EXTERNAL_ID, ZionTestData::free_tier_limits()).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage("Response", 10, 5).await;

    let request = json!({
        "model": "gpt-4",
        "messages": [{"role": "user", "content": "Hello"}]
    });

    // Send 3 rapid requests
    for _ in 0..3 {
        let response = harness.server
            .post("/v1/chat/completions")
            .add_header(header::AUTHORIZATION, auth_header().parse().unwrap())
            .add_header(header::CONTENT_TYPE, "application/json".parse().unwrap())
            .json(&request)
            .await;
        response.assert_status_ok();
    }

    // Wait for batch to be sent (may combine into single batch)
    tokio::time::sleep(Duration::from_millis(100)).await;
    let requests = harness.wait_for_batch_requests(1, Duration::from_secs(3)).await;

    // Count total requests tracked
    let mut total_requests = 0;
    for req in &requests {
        let increments = TokenTrackingTestHarness::parse_batch_payload(req);
        for inc in &increments {
            let (_, _, req_count) = TokenTrackingTestHarness::extract_token_counts(inc);
            total_requests += req_count;
        }
    }

    assert_eq!(total_requests, 3, "All 3 requests should be tracked");
}
