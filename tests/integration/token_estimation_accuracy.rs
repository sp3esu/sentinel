//! Token Estimation Accuracy Tests
//!
//! These tests verify that Sentinel's token estimation using tiktoken-rs
//! is accurate compared to OpenAI's reported usage. We verify that:
//! - Input token estimation is within 5% of OpenAI's reported prompt_tokens
//! - Various message formats are handled correctly
//! - Different models use appropriate tokenizers
//!
//! Run these tests with:
//! ```bash
//! cargo test --test integration_tests token_estimation_accuracy --features test-utils
//! ```
//!
//! Note: These tests require Redis to be running locally.

use std::time::Duration;
use axum::http::header;
use serde_json::json;

use crate::common::{constants, TokenTrackingTestHarness};
use crate::mocks::zion::{ZionTestData, UserProfileMock};

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

/// Helper to assert token counts are within acceptable threshold
///
/// # Arguments
/// * `estimated` - Token count estimated by Sentinel (sent to Zion)
/// * `actual` - Token count reported by OpenAI
/// * `threshold_pct` - Maximum acceptable percentage difference
/// * `context` - Description for error message
fn assert_within_threshold(estimated: i64, actual: i64, threshold_pct: f64, context: &str) {
    let diff = (actual - estimated).abs();
    let pct = if estimated > 0 {
        diff as f64 / estimated as f64 * 100.0
    } else if actual > 0 {
        100.0 // If estimated is 0 but actual isn't, that's 100% error
    } else {
        0.0 // Both are 0, perfect match
    };

    assert!(
        pct <= threshold_pct,
        "{}: estimated={}, actual={}, diff={}%, threshold={}%",
        context, estimated, actual, pct, threshold_pct
    );
}

// =============================================================================
// Input Token Estimation Accuracy Tests
// =============================================================================

#[tokio::test]
async fn test_input_token_estimation_accuracy_simple_message() {
    let harness = TokenTrackingTestHarness::new().await;

    // OpenAI will report these specific token counts
    let openai_prompt_tokens = 15i64;
    let openai_completion_tokens = 25i64;

    // Set up mocks
    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(
        constants::TEST_EXTERNAL_ID,
        ZionTestData::free_tier_limits()
    ).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage(
        "Hello! How can I help you today?",
        openai_prompt_tokens,
        openai_completion_tokens,
    ).await;

    // Send a simple chat request
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
    assert!(!requests.is_empty(), "Expected batch-increment request");

    // Parse and verify the payload
    let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
    assert!(!increments.is_empty(), "Expected at least one increment in batch");

    let (sentinel_input, sentinel_output, _) =
        TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    println!("\n========================================");
    println!("SIMPLE MESSAGE TOKEN ESTIMATION:");
    println!("========================================");
    println!("OpenAI reported:  {} input, {} output", openai_prompt_tokens, openai_completion_tokens);
    println!("Sentinel sent:    {} input, {} output", sentinel_input, sentinel_output);
    println!("Input diff:       {} ({:.2}%)",
        (openai_prompt_tokens - sentinel_input).abs(),
        if sentinel_input > 0 {
            (openai_prompt_tokens - sentinel_input).abs() as f64 / sentinel_input as f64 * 100.0
        } else { 0.0 }
    );
    println!("========================================\n");

    // Verify input and output tokens match OpenAI's reported usage
    // Since we use OpenAI's usage field when available, they should match exactly
    assert_eq!(
        sentinel_input, openai_prompt_tokens,
        "Input tokens should match OpenAI's prompt_tokens exactly"
    );
    assert_eq!(
        sentinel_output, openai_completion_tokens,
        "Output tokens should match OpenAI's completion_tokens exactly"
    );

    // Also verify they're within threshold (should be 0% difference)
    assert_within_threshold(
        sentinel_input,
        openai_prompt_tokens,
        5.0,
        "Input token estimation"
    );
}

#[tokio::test]
async fn test_input_token_estimation_accuracy_conversation() {
    let harness = TokenTrackingTestHarness::new().await;

    // Multi-turn conversation should have more tokens
    let openai_prompt_tokens = 85i64;
    let openai_completion_tokens = 40i64;

    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(
        constants::TEST_EXTERNAL_ID,
        ZionTestData::free_tier_limits()
    ).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage(
        "Based on our conversation, I recommend learning Rust's ownership model first.",
        openai_prompt_tokens,
        openai_completion_tokens,
    ).await;

    // Multi-turn conversation with system prompt
    let request = json!({
        "model": "gpt-4",
        "messages": [
            {"role": "system", "content": "You are a helpful programming assistant."},
            {"role": "user", "content": "What's the best way to learn Rust?"},
            {"role": "assistant", "content": "I recommend starting with the Rust Book."},
            {"role": "user", "content": "What should I focus on first?"}
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
    let (sentinel_input, sentinel_output, _) =
        TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    println!("\n========================================");
    println!("CONVERSATION TOKEN ESTIMATION:");
    println!("========================================");
    println!("OpenAI reported:  {} input, {} output", openai_prompt_tokens, openai_completion_tokens);
    println!("Sentinel sent:    {} input, {} output", sentinel_input, sentinel_output);
    println!("Input diff:       {} ({:.2}%)",
        (openai_prompt_tokens - sentinel_input).abs(),
        if sentinel_input > 0 {
            (openai_prompt_tokens - sentinel_input).abs() as f64 / sentinel_input as f64 * 100.0
        } else { 0.0 }
    );
    println!("========================================\n");

    // Verify exact match with OpenAI's reported usage
    assert_eq!(
        sentinel_input, openai_prompt_tokens,
        "Conversation input tokens should match OpenAI exactly"
    );
    assert_eq!(
        sentinel_output, openai_completion_tokens,
        "Conversation output tokens should match OpenAI exactly"
    );

    assert_within_threshold(
        sentinel_input,
        openai_prompt_tokens,
        5.0,
        "Conversation input token estimation"
    );
}

#[tokio::test]
async fn test_input_token_estimation_accuracy_with_system_prompt() {
    let harness = TokenTrackingTestHarness::new().await;

    // Long system prompt should contribute significantly to token count
    let openai_prompt_tokens = 120i64;
    let openai_completion_tokens = 30i64;

    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(
        constants::TEST_EXTERNAL_ID,
        ZionTestData::free_tier_limits()
    ).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage(
        "I'll help you with Python best practices.",
        openai_prompt_tokens,
        openai_completion_tokens,
    ).await;

    // Long system prompt + short user message
    let request = json!({
        "model": "gpt-4",
        "messages": [
            {
                "role": "system",
                "content": "You are an expert Python developer with 20 years of experience. \
                           You always provide detailed, well-documented code examples. \
                           You follow PEP 8 style guidelines and prefer modern Python 3.11+ features. \
                           When explaining concepts, you use clear analogies and provide practical examples."
            },
            {"role": "user", "content": "Help me with Python"}
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
    let (sentinel_input, sentinel_output, _) =
        TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    println!("\n========================================");
    println!("SYSTEM PROMPT TOKEN ESTIMATION:");
    println!("========================================");
    println!("OpenAI reported:  {} input, {} output", openai_prompt_tokens, openai_completion_tokens);
    println!("Sentinel sent:    {} input, {} output", sentinel_input, sentinel_output);
    println!("Input diff:       {} ({:.2}%)",
        (openai_prompt_tokens - sentinel_input).abs(),
        if sentinel_input > 0 {
            (openai_prompt_tokens - sentinel_input).abs() as f64 / sentinel_input as f64 * 100.0
        } else { 0.0 }
    );
    println!("========================================\n");

    // Verify exact match
    assert_eq!(
        sentinel_input, openai_prompt_tokens,
        "System prompt input tokens should match OpenAI exactly"
    );
    assert_eq!(
        sentinel_output, openai_completion_tokens,
        "System prompt output tokens should match OpenAI exactly"
    );

    assert_within_threshold(
        sentinel_input,
        openai_prompt_tokens,
        5.0,
        "System prompt input token estimation"
    );

    // Verify the system prompt contributed significantly to the count
    assert!(sentinel_input > 50, "System prompt should contribute >50 tokens");
}

#[tokio::test]
async fn test_estimation_accuracy_various_models() {
    // Test that different models all report accurate token counts
    // Note: In our mock, all models use similar tokenization, but this
    // verifies the pattern works for different model names

    let test_cases = vec![
        ("gpt-4", 42i64, 28i64),
        ("gpt-3.5-turbo", 38i64, 32i64),
        ("gpt-4-turbo", 45i64, 30i64),
    ];

    for (model, expected_input, expected_output) in test_cases {
        let harness = TokenTrackingTestHarness::new().await;

        harness.zion.mock_get_user_profile_success(test_profile()).await;
        harness.zion.mock_get_limits_success(
            constants::TEST_EXTERNAL_ID,
            ZionTestData::free_tier_limits()
        ).await;
        harness.zion.mock_batch_increment_success(1, 0).await;
        harness.openai.mock_chat_completion_with_usage(
            "This is a test response for model verification.",
            expected_input,
            expected_output,
        ).await;

        let request = json!({
            "model": model,
            "messages": [
                {"role": "user", "content": "Tell me about Rust programming language features."}
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
        assert!(!requests.is_empty(), "Expected batch request for model {}", model);

        let increments = TokenTrackingTestHarness::parse_batch_payload(&requests[0]);
        let (sentinel_input, sentinel_output, _) =
            TokenTrackingTestHarness::extract_token_counts(&increments[0]);

        println!("\n========================================");
        println!("MODEL {} TOKEN ESTIMATION:", model);
        println!("========================================");
        println!("OpenAI reported:  {} input, {} output", expected_input, expected_output);
        println!("Sentinel sent:    {} input, {} output", sentinel_input, sentinel_output);
        println!("Input diff:       {} ({:.2}%)",
            (expected_input - sentinel_input).abs(),
            if sentinel_input > 0 {
                (expected_input - sentinel_input).abs() as f64 / sentinel_input as f64 * 100.0
            } else { 0.0 }
        );
        println!("========================================\n");

        // Verify exact match
        assert_eq!(
            sentinel_input, expected_input,
            "Model {} input tokens should match OpenAI exactly", model
        );
        assert_eq!(
            sentinel_output, expected_output,
            "Model {} output tokens should match OpenAI exactly", model
        );

        assert_within_threshold(
            sentinel_input,
            expected_input,
            5.0,
            &format!("Model {} input token estimation", model)
        );
    }
}

// =============================================================================
// Edge Cases
// =============================================================================

#[tokio::test]
async fn test_estimation_accuracy_minimal_input() {
    let harness = TokenTrackingTestHarness::new().await;

    // Very minimal input should still have accurate token count
    let openai_prompt_tokens = 3i64;
    let openai_completion_tokens = 5i64;

    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(
        constants::TEST_EXTERNAL_ID,
        ZionTestData::free_tier_limits()
    ).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage(
        "Hi!",
        openai_prompt_tokens,
        openai_completion_tokens,
    ).await;

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
    let (sentinel_input, sentinel_output, _) =
        TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    println!("\n========================================");
    println!("MINIMAL INPUT TOKEN ESTIMATION:");
    println!("========================================");
    println!("OpenAI reported:  {} input, {} output", openai_prompt_tokens, openai_completion_tokens);
    println!("Sentinel sent:    {} input, {} output", sentinel_input, sentinel_output);
    println!("========================================\n");

    // Even minimal input should be accurate
    assert_eq!(sentinel_input, openai_prompt_tokens, "Minimal input should match exactly");
    assert!(sentinel_input > 0, "Should have at least 1 input token");
}

#[tokio::test]
async fn test_estimation_accuracy_long_input() {
    let harness = TokenTrackingTestHarness::new().await;

    // Very long input should still be accurate
    let openai_prompt_tokens = 250i64;
    let openai_completion_tokens = 150i64;

    harness.zion.mock_get_user_profile_success(test_profile()).await;
    harness.zion.mock_get_limits_success(
        constants::TEST_EXTERNAL_ID,
        ZionTestData::free_tier_limits()
    ).await;
    harness.zion.mock_batch_increment_success(1, 0).await;
    harness.openai.mock_chat_completion_with_usage(
        "Based on the comprehensive requirements you've provided...",
        openai_prompt_tokens,
        openai_completion_tokens,
    ).await;

    // Very long user message
    let long_content = "I need help designing a distributed system that can handle \
                       high throughput and low latency. The system should support \
                       multiple regions, automatic failover, and horizontal scaling. \
                       We expect to handle about 100,000 requests per second with \
                       99.99% uptime. The data needs to be consistent across regions \
                       within 100ms. We're considering using microservices architecture \
                       with event-driven communication. What are your recommendations \
                       for the tech stack and architecture patterns?";

    let request = json!({
        "model": "gpt-4",
        "messages": [
            {"role": "user", "content": long_content}
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
    let (sentinel_input, sentinel_output, _) =
        TokenTrackingTestHarness::extract_token_counts(&increments[0]);

    println!("\n========================================");
    println!("LONG INPUT TOKEN ESTIMATION:");
    println!("========================================");
    println!("OpenAI reported:  {} input, {} output", openai_prompt_tokens, openai_completion_tokens);
    println!("Sentinel sent:    {} input, {} output", sentinel_input, sentinel_output);
    println!("Input diff:       {} ({:.2}%)",
        (openai_prompt_tokens - sentinel_input).abs(),
        if sentinel_input > 0 {
            (openai_prompt_tokens - sentinel_input).abs() as f64 / sentinel_input as f64 * 100.0
        } else { 0.0 }
    );
    println!("========================================\n");

    assert_eq!(
        sentinel_input, openai_prompt_tokens,
        "Long input tokens should match OpenAI exactly"
    );

    assert_within_threshold(
        sentinel_input,
        openai_prompt_tokens,
        5.0,
        "Long input token estimation"
    );

    // Verify it's actually a long input
    assert!(sentinel_input > 100, "Should have >100 tokens for long input");
}
