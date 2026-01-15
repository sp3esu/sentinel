//! OpenAI Responses API handler
//!
//! Routes /v1/responses to OpenAI with full token tracking.
//! The Responses API uses `input` array (similar to chat messages) instead of `messages`.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{
    error::AppError,
    middleware::auth::AuthenticatedUser,
    routes::metrics::{
        record_fallback_estimation, record_request, record_sse_parse_error,
        record_token_estimation_diff, record_tokens,
    },
    streaming::SseLineBuffer,
    AppState,
};

/// Responses API request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesRequest {
    pub model: String,
    /// Input items - can be message items (with role) or function call outputs (with call_id)
    #[serde(default)]
    pub input: Vec<serde_json::Value>,
    #[serde(default)]
    pub stream: bool,
    // Pass through all other fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Usage statistics (same as chat completions)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

/// Responses API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesResponse {
    pub id: String,
    #[serde(default)]
    pub output: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    // Pass through all other fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Convert input items to tuples for token counting.
/// Only extracts items that have a "role" field (message items).
/// Function call output items and other non-message items are skipped.
fn input_to_tuples(input: &[serde_json::Value]) -> Vec<(String, String, Option<String>)> {
    input
        .iter()
        .filter_map(|item| {
            // Only process items with "role" field (message items)
            let role = item.get("role")?.as_str()?;
            let content = item
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let name = item
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());

            Some((role.to_string(), content, name))
        })
        .collect()
}

/// Extract text content from output array for token counting
fn extract_output_text(output: &[serde_json::Value]) -> String {
    let mut text = String::new();
    for item in output {
        // Try to extract content from various possible formats
        if let Some(content) = item.get("content") {
            if let Some(s) = content.as_str() {
                text.push_str(s);
            } else if let Some(arr) = content.as_array() {
                for part in arr {
                    if let Some(text_content) = part.get("text").and_then(|t| t.as_str()) {
                        text.push_str(text_content);
                    }
                }
            }
        }
        // Also check for text field directly
        if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
            text.push_str(t);
        }
    }
    text
}

/// Handler for POST /v1/responses
pub async fn responses_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<Response, AppError> {
    let start_time = Instant::now();

    // Extract authenticated user from request extensions (set by auth middleware)
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .cloned()
        .ok_or_else(|| {
            warn!("AuthenticatedUser not found in request extensions");
            AppError::Unauthorized
        })?;

    // Parse the request body
    let body = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read request body: {}", e)))?;

    let responses_request: ResponsesRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("Invalid request body: {}", e)))?;

    let model = responses_request.model.clone();
    let is_streaming = responses_request.stream;

    info!(
        model = %model,
        stream = %is_streaming,
        input_items = %responses_request.input.len(),
        external_id = %user.external_id,
        "Processing responses API request"
    );

    if is_streaming {
        handle_streaming_responses(state, &headers, responses_request, model, start_time, user).await
    } else {
        handle_non_streaming_responses(state, &headers, responses_request, model, start_time, user).await
    }
}

/// Handle non-streaming responses
async fn handle_non_streaming_responses(
    state: Arc<AppState>,
    headers: &HeaderMap,
    request: ResponsesRequest,
    model: String,
    start_time: Instant,
    user: AuthenticatedUser,
) -> Result<Response, AppError> {
    // Pre-count input tokens using tiktoken
    let input_tuples = input_to_tuples(&request.input);
    let estimated_input_tokens = state
        .token_counter
        .count_chat_messages(&model, &input_tuples)
        .unwrap_or(0);

    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    let response_value = state
        .ai_provider
        .responses(request_value, headers)
        .await?;

    // Parse the response
    let response: ResponsesResponse = serde_json::from_value(response_value.clone())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse response: {}", e)))?;

    // Record metrics
    let duration = start_time.elapsed().as_secs_f64();
    record_request("success", &model, duration);

    // Get token counts: prefer OpenAI usage, fallback to estimation
    let (input_tokens, output_tokens) = if let Some(ref usage) = response.usage {
        // Log comparison between estimated and actual
        let input_diff = (usage.input_tokens as i64) - (estimated_input_tokens as i64);
        let input_diff_pct = if estimated_input_tokens > 0 {
            input_diff as f64 / estimated_input_tokens as f64 * 100.0
        } else {
            0.0
        };

        debug!(
            estimated_input = estimated_input_tokens,
            actual_input = usage.input_tokens,
            input_diff = input_diff,
            input_diff_pct = %format!("{:.1}%", input_diff_pct),
            actual_output = usage.output_tokens,
            model = %model,
            "Token estimation comparison"
        );

        record_token_estimation_diff(&model, estimated_input_tokens as u64, usage.input_tokens as u64);

        (usage.input_tokens as u64, usage.output_tokens as u64)
    } else {
        // Fallback: use estimated input tokens, estimate output from response text
        let output_text = extract_output_text(&response.output);
        let estimated_output = state
            .token_counter
            .count_tokens(&model, &output_text)
            .unwrap_or(0) as u64;
        debug!(
            estimated_input = estimated_input_tokens,
            estimated_output = estimated_output,
            "Using estimated token counts (OpenAI didn't return usage)"
        );
        (estimated_input_tokens as u64, estimated_output)
    };

    record_tokens("prompt", input_tokens, &model);
    record_tokens("completion", output_tokens, &model);

    // Track usage in Zion (fire-and-forget, never blocks)
    state.batching_tracker.track(user.email.clone(), input_tokens, output_tokens, Some(model.clone()));

    info!(
        model = %model,
        duration_ms = %format!("{:.2}", duration * 1000.0),
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        external_id = %user.external_id,
        "Responses API request completed"
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Response object inside response.completed events
#[derive(Debug, Clone, Deserialize, Default)]
struct ResponseObject {
    #[serde(default)]
    usage: Option<Usage>,
    // Note: output field exists in the API but we don't need it for token counting
}

/// Streaming chunk for parsing content and usage
/// Handles both Chat Completions-style and Responses API-style events
#[derive(Debug, Clone, Deserialize)]
struct StreamChunk {
    /// Event type (e.g., "response.output_text.delta", "response.completed")
    /// Used in tests to verify correct parsing; available for production debugging if needed
    #[serde(default, rename = "type")]
    #[allow(dead_code)]
    event_type: Option<String>,

    /// Delta content - can be a string (response.output_text.delta) or object
    #[serde(default)]
    delta: Option<serde_json::Value>,

    /// Response object inside response.completed events - contains usage
    #[serde(default)]
    response: Option<ResponseObject>,

    /// Direct usage field (legacy/fallback)
    #[serde(default)]
    usage: Option<Usage>,

    /// Output array (for non-streaming or different event types)
    #[serde(default)]
    output: Vec<serde_json::Value>,
}

/// Handle streaming responses
async fn handle_streaming_responses(
    state: Arc<AppState>,
    headers: &HeaderMap,
    request: ResponsesRequest,
    model: String,
    start_time: Instant,
    user: AuthenticatedUser,
) -> Result<Response, AppError> {
    // Pre-count input tokens using tiktoken
    let input_tuples = input_to_tuples(&request.input);
    let estimated_input_tokens = state
        .token_counter
        .count_chat_messages(&model, &input_tuples)
        .unwrap_or(0) as u64;

    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    // Forward streaming request to provider
    let stream = state
        .ai_provider
        .responses_stream(request_value, headers)
        .await?;

    // Clone values for the stream closure
    let model_clone = model.clone();
    let tracker = state.batching_tracker.clone();
    let user_email = user.email.clone();
    let token_counter = state.token_counter.clone();

    // Track accumulated usage from stream (if OpenAI provides it)
    let usage_accumulator = std::sync::Arc::new(std::sync::Mutex::new(Usage::default()));
    let usage_for_stream = usage_accumulator.clone();

    // Track accumulated content for token counting fallback
    let content_accumulator = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let content_for_stream = content_accumulator.clone();

    // Buffer for accumulating incomplete SSE lines across chunk boundaries
    let line_buffer = std::sync::Arc::new(std::sync::Mutex::new(SseLineBuffer::new()));
    let line_buffer_for_stream = line_buffer.clone();

    // Clone model for metrics in stream closure
    let model_for_parse_error = model.clone();

    // Wrap the stream to extract content and usage from chunks
    let tracked_stream = stream.map(move |chunk| {
        match chunk {
            Ok(bytes) => {
                // Use line buffer to handle chunks split across network boundaries
                let complete_lines = line_buffer_for_stream.lock().unwrap().feed(&bytes);

                for line in complete_lines {
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        let json_str = json_str.trim();
                        if json_str != "[DONE]" {
                            match serde_json::from_str::<StreamChunk>(json_str) {
                                Ok(chunk) => {
                                    // Accumulate content from output array (non-streaming or some event types)
                                    if !chunk.output.is_empty() {
                                        let output_text = extract_output_text(&chunk.output);
                                        content_for_stream.lock().unwrap().push_str(&output_text);
                                    }

                                    // Accumulate content from delta field
                                    if let Some(ref delta) = chunk.delta {
                                        // For response.output_text.delta events, delta is a string
                                        if let Some(text) = delta.as_str() {
                                            content_for_stream.lock().unwrap().push_str(text);
                                        }
                                        // For other event types, delta may be an object with text/content fields
                                        if let Some(obj) = delta.as_object() {
                                            if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                                                content_for_stream.lock().unwrap().push_str(text);
                                            }
                                            if let Some(content) = obj.get("content").and_then(|c| c.as_str()) {
                                                content_for_stream.lock().unwrap().push_str(content);
                                            }
                                        }
                                    }

                                    // Extract usage from response.completed event (nested in response object)
                                    if let Some(ref response) = chunk.response {
                                        if let Some(ref usage) = response.usage {
                                            let mut acc = usage_for_stream.lock().unwrap();
                                            acc.input_tokens = usage.input_tokens;
                                            acc.output_tokens = usage.output_tokens;
                                            acc.total_tokens = usage.total_tokens;
                                        }
                                    }

                                    // Fallback: direct usage field (legacy/other formats)
                                    if let Some(usage) = chunk.usage {
                                        let mut acc = usage_for_stream.lock().unwrap();
                                        acc.input_tokens = usage.input_tokens;
                                        acc.output_tokens = usage.output_tokens;
                                        acc.total_tokens = usage.total_tokens;
                                    }
                                }
                                Err(e) => {
                                    // Log and record metric for parse failures on complete lines
                                    warn!(
                                        error = %e,
                                        sse_line = %if json_str.len() > 500 { &json_str[..500] } else { json_str },
                                        line_len = json_str.len(),
                                        "Failed to parse complete SSE line"
                                    );
                                    record_sse_parse_error("responses", &model_for_parse_error);
                                }
                            }
                        }
                    }
                }
                Ok(bytes)
            }
            Err(e) => {
                warn!(model = %model_clone, error = %e, "Stream error");
                Err(e)
            }
        }
    });

    // Create a stream that tracks usage after completion
    let user_email_final = user_email.clone();
    let model_for_metrics = model.clone();
    let model_for_counting = model.clone();
    let usage_final = usage_accumulator.clone();
    let content_final = content_accumulator.clone();
    let tracker_final = tracker.clone();

    let final_stream = async_stream::stream! {
        futures::pin_mut!(tracked_stream);
        while let Some(item) = tracked_stream.next().await {
            yield item;
        }

        // Stream completed - determine token counts
        let openai_usage = usage_final.lock().unwrap().clone();
        let accumulated_content = content_final.lock().unwrap().clone();

        // Prefer OpenAI usage if available, otherwise estimate
        let (input_tokens, output_tokens) = if openai_usage.input_tokens > 0 || openai_usage.output_tokens > 0 {
            // Log comparison between estimated and actual
            let input_diff = (openai_usage.input_tokens as i64) - (estimated_input_tokens as i64);
            let input_diff_pct = if estimated_input_tokens > 0 {
                input_diff as f64 / estimated_input_tokens as f64 * 100.0
            } else {
                0.0
            };

            debug!(
                estimated_input = estimated_input_tokens,
                actual_input = openai_usage.input_tokens,
                input_diff = input_diff,
                input_diff_pct = %format!("{:.1}%", input_diff_pct),
                actual_output = openai_usage.output_tokens,
                model = %model_for_metrics,
                "Token estimation comparison (streaming)"
            );

            record_token_estimation_diff(&model_for_metrics, estimated_input_tokens, openai_usage.input_tokens as u64);

            (openai_usage.input_tokens as u64, openai_usage.output_tokens as u64)
        } else {
            // Fallback to estimation - OpenAI didn't return usage field
            let estimated_output = token_counter
                .count_tokens(&model_for_counting, &accumulated_content)
                .unwrap_or(0) as u64;
            warn!(
                model = %model_for_counting,
                estimated_input = estimated_input_tokens,
                estimated_output = estimated_output,
                content_len = accumulated_content.len(),
                "Using estimated token counts - OpenAI didn't return usage field"
            );
            record_fallback_estimation(&model_for_counting);
            (estimated_input_tokens, estimated_output)
        };

        // Record metrics
        record_tokens("prompt", input_tokens, &model_for_metrics);
        record_tokens("completion", output_tokens, &model_for_metrics);

        // ALWAYS track usage in Zion (fire-and-forget)
        tracker_final.track(user_email_final.clone(), input_tokens, output_tokens, Some(model_for_metrics.clone()));

        info!(
            model = %model_for_metrics,
            input_tokens = input_tokens,
            output_tokens = output_tokens,
            email = %user_email_final,
            "Streaming responses usage tracked"
        );
    };

    // Record that we started streaming
    let duration = start_time.elapsed().as_secs_f64();
    record_request("streaming", &model, duration);

    // Build SSE response
    let body = Body::from_stream(final_stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .header("X-Accel-Buffering", "no")
        .body(body)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build response: {}", e)))?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_input_to_tuples_message_items() {
        let input = vec![
            json!({"role": "user", "content": "Hello"}),
            json!({"role": "assistant", "content": "Hi there!"}),
        ];

        let tuples = input_to_tuples(&input);
        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0], ("user".to_string(), "Hello".to_string(), None));
        assert_eq!(
            tuples[1],
            ("assistant".to_string(), "Hi there!".to_string(), None)
        );
    }

    #[test]
    fn test_input_to_tuples_skips_function_call_output() {
        let input = vec![
            json!({"role": "user", "content": "What's the weather?"}),
            json!({"type": "function_call_output", "call_id": "call_123", "output": "{\"temp\": 72}"}),
            json!({"role": "assistant", "content": "The weather is 72 degrees."}),
        ];

        let tuples = input_to_tuples(&input);
        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0].0, "user");
        assert_eq!(tuples[1].0, "assistant");
    }

    #[test]
    fn test_input_to_tuples_skips_function_call() {
        let input = vec![
            json!({"role": "user", "content": "Get weather"}),
            json!({"type": "function_call", "call_id": "call_123", "name": "get_weather", "arguments": "{}"}),
        ];

        let tuples = input_to_tuples(&input);
        assert_eq!(tuples.len(), 1);
        assert_eq!(tuples[0].0, "user");
    }

    #[test]
    fn test_input_to_tuples_with_name() {
        let input = vec![json!({"role": "user", "content": "Hello", "name": "Alice"})];

        let tuples = input_to_tuples(&input);
        assert_eq!(tuples.len(), 1);
        assert_eq!(
            tuples[0],
            (
                "user".to_string(),
                "Hello".to_string(),
                Some("Alice".to_string())
            )
        );
    }

    #[test]
    fn test_input_to_tuples_empty_content() {
        let input = vec![
            json!({"role": "user"}), // No content field
        ];

        let tuples = input_to_tuples(&input);
        assert_eq!(tuples.len(), 1);
        assert_eq!(tuples[0].1, ""); // Empty string for missing content
    }

    #[test]
    fn test_input_to_tuples_mixed_items() {
        // Real-world scenario: conversation with function calling
        let input = vec![
            json!({"role": "user", "content": "What's the weather in Paris?"}),
            json!({"role": "assistant", "content": null}),
            json!({"type": "function_call", "call_id": "fc_1", "name": "get_weather", "arguments": "{\"city\":\"Paris\"}"}),
            json!({"type": "function_call_output", "call_id": "fc_1", "output": "{\"temp\":\"18C\",\"condition\":\"cloudy\"}"}),
            json!({"role": "assistant", "content": "The weather in Paris is 18C and cloudy."}),
        ];

        let tuples = input_to_tuples(&input);
        // Should only get 3 items: user, assistant (empty), assistant (with content)
        assert_eq!(tuples.len(), 3);
        assert_eq!(tuples[0].0, "user");
        assert_eq!(tuples[1].0, "assistant");
        assert_eq!(tuples[1].1, ""); // null content becomes empty string
        assert_eq!(tuples[2].0, "assistant");
        assert!(tuples[2].1.contains("Paris"));
    }

    // ==========================================
    // Streaming Token Counting Tests
    // ==========================================

    #[test]
    fn test_streaming_token_count_matches_completion_usage() {
        // Simulate a streaming session with multiple delta events and final completion
        let sse_events = vec![
            // Delta events with content (response.output_text.delta has delta as string)
            r#"{"type":"response.output_text.delta","delta":"Hello "}"#,
            r#"{"type":"response.output_text.delta","delta":"world"}"#,
            // Final completion with usage (this is the authoritative source)
            r#"{"type":"response.completed","response":{"usage":{"input_tokens":15,"output_tokens":25,"total_tokens":40}}}"#,
        ];

        let mut content_accumulator = String::new();
        let mut final_usage = Usage::default();

        for json_str in sse_events {
            let chunk: StreamChunk = serde_json::from_str(json_str).unwrap();

            // Accumulate content from deltas (same logic as streaming handler)
            if let Some(ref delta) = chunk.delta {
                // For response.output_text.delta events, delta is a string
                if let Some(text) = delta.as_str() {
                    content_accumulator.push_str(text);
                }
                // For other event types, delta may be an object
                if let Some(obj) = delta.as_object() {
                    if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                        content_accumulator.push_str(text);
                    }
                }
            }

            // Extract final usage from response.completed
            if let Some(ref response) = chunk.response {
                if let Some(ref usage) = response.usage {
                    final_usage = usage.clone();
                }
            }
        }

        // Verify content was accumulated
        assert_eq!(content_accumulator, "Hello world");

        // Verify final usage was extracted from response.completed
        assert_eq!(final_usage.input_tokens, 15);
        assert_eq!(final_usage.output_tokens, 25);
        assert_eq!(final_usage.total_tokens, 40);

        // The key invariant: we use the authoritative usage from response.completed,
        // not a token count estimation from accumulated content
    }

    #[test]
    fn test_fallback_estimation_when_no_usage() {
        // Simulate streaming without usage in final event (edge case)
        let sse_events = vec![
            r#"{"type":"response.output_text.delta","delta":"Hello world"}"#,
            r#"{"type":"response.completed","response":{}}"#, // No usage field
        ];

        let mut content_accumulator = String::new();
        let mut final_usage = Usage::default();

        for json_str in sse_events {
            let chunk: StreamChunk = serde_json::from_str(json_str).unwrap();

            if let Some(ref delta) = chunk.delta {
                if let Some(text) = delta.as_str() {
                    content_accumulator.push_str(text);
                }
            }

            if let Some(ref response) = chunk.response {
                if let Some(ref usage) = response.usage {
                    final_usage = usage.clone();
                }
            }
        }

        // Content should still be accumulated for fallback estimation
        assert_eq!(content_accumulator, "Hello world");

        // Usage remains default (0) - triggers fallback path in production code
        assert_eq!(final_usage.input_tokens, 0);
        assert_eq!(final_usage.output_tokens, 0);
    }

    #[test]
    fn test_object_style_delta_backwards_compat() {
        // Some event types may use object-style delta (e.g., response.content_part.delta)
        let json_str = r#"{"type":"response.content_part.delta","delta":{"text":"Hello"}}"#;
        let chunk: StreamChunk = serde_json::from_str(json_str).unwrap();

        let mut content = String::new();
        if let Some(ref delta) = chunk.delta {
            // First try string
            if let Some(text) = delta.as_str() {
                content.push_str(text);
            }
            // Then try object
            if let Some(obj) = delta.as_object() {
                if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                    content.push_str(text);
                }
            }
        }

        assert_eq!(content, "Hello");
    }

    #[test]
    fn test_response_object_usage_extraction() {
        // Test the ResponseObject struct correctly deserializes usage
        let json_str = r#"{"type":"response.completed","response":{"id":"resp_123","usage":{"input_tokens":100,"output_tokens":200,"total_tokens":300},"output":[]}}"#;
        let chunk: StreamChunk = serde_json::from_str(json_str).unwrap();

        assert!(chunk.response.is_some());
        let response = chunk.response.unwrap();
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.total_tokens, 300);
    }

    #[test]
    fn test_legacy_direct_usage_field() {
        // Test that direct usage field (legacy format) still works
        let json_str = r#"{"usage":{"input_tokens":50,"output_tokens":75,"total_tokens":125}}"#;
        let chunk: StreamChunk = serde_json::from_str(json_str).unwrap();

        assert!(chunk.usage.is_some());
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.output_tokens, 75);
        assert_eq!(usage.total_tokens, 125);
    }

    #[test]
    fn test_stream_chunk_event_type_parsing() {
        // Test that event type is correctly extracted
        let json_str = r#"{"type":"response.output_text.delta","delta":"test"}"#;
        let chunk: StreamChunk = serde_json::from_str(json_str).unwrap();

        assert_eq!(chunk.event_type, Some("response.output_text.delta".to_string()));
    }
}
