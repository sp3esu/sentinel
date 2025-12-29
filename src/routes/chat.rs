//! Chat completions endpoint
//!
//! OpenAI-compatible chat completions API endpoint.
//! Handles both streaming and non-streaming responses.

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
    proxy::AiProvider,
    routes::metrics::{record_request, record_tokens},
    AppState,
};

/// Convert ChatMessage to tuple format for token counting
fn messages_to_tuples(messages: &[ChatMessage]) -> Vec<(String, String, Option<String>)> {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
                Role::Function => "function",
            };
            (
                role.to_string(),
                msg.content.clone().unwrap_or_default(),
                msg.name.clone(),
            )
        })
        .collect()
}

/// Chat message role
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
    Function,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Stream options for including usage in streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    #[serde(default)]
    pub include_usage: bool,
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    // Pass through any extra fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Chat completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
}

/// Extract bearer token from Authorization header
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Handle chat completion requests
///
/// This endpoint is compatible with OpenAI's chat completions API.
/// It proxies requests to the AI provider after checking user quotas.
pub async fn chat_completions(
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

    let chat_request: ChatCompletionRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("Invalid request body: {}", e)))?;

    let model = chat_request.model.clone();
    let is_streaming = chat_request.stream;

    // Extract authorization token (kept for potential future use)
    let _token = extract_bearer_token(&headers);

    info!(
        model = %model,
        stream = %is_streaming,
        messages = %chat_request.messages.len(),
        external_id = %user.external_id,
        "Processing chat completion request"
    );

    if is_streaming {
        // Handle streaming response
        handle_streaming_chat(state, &headers, chat_request, model, start_time, user).await
    } else {
        // Handle non-streaming response
        handle_non_streaming_chat(state, &headers, chat_request, model, start_time, user).await
    }
}

/// Handle non-streaming chat completion
async fn handle_non_streaming_chat(
    state: Arc<AppState>,
    headers: &HeaderMap,
    request: ChatCompletionRequest,
    model: String,
    start_time: Instant,
    user: AuthenticatedUser,
) -> Result<Response, AppError> {
    // Pre-count input tokens using tiktoken (for fallback if OpenAI doesn't return usage)
    let message_tuples = messages_to_tuples(&request.messages);
    let estimated_input_tokens = state
        .token_counter
        .count_chat_messages(&model, &message_tuples)
        .unwrap_or(0);

    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    let response_value = state
        .ai_provider
        .chat_completions(request_value, headers)
        .await?;

    // Parse the response
    let response: ChatCompletionResponse = serde_json::from_value(response_value.clone())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse response: {}", e)))?;

    // Record metrics
    let duration = start_time.elapsed().as_secs_f64();
    record_request("success", &model, duration);

    // Get token counts: prefer OpenAI usage, fallback to estimation
    let (input_tokens, output_tokens) = if let Some(ref usage) = response.usage {
        (usage.prompt_tokens as u64, usage.completion_tokens as u64)
    } else {
        // Fallback: use estimated input tokens, estimate output from response text
        let output_text = response
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("");
        let estimated_output = state
            .token_counter
            .count_tokens(&model, output_text)
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
    state.batching_tracker.track(user.email.clone(), input_tokens, output_tokens);

    info!(
        model = %model,
        duration_ms = %format!("{:.2}", duration * 1000.0),
        input_tokens = input_tokens,
        output_tokens = output_tokens,
        external_id = %user.external_id,
        "Chat completion request completed"
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Streaming chunk for parsing content and usage
#[derive(Debug, Clone, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

/// Streaming choice with delta
#[derive(Debug, Clone, Deserialize, Default)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

/// Streaming delta content
#[derive(Debug, Clone, Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

/// Handle streaming chat completion
async fn handle_streaming_chat(
    state: Arc<AppState>,
    headers: &HeaderMap,
    mut request: ChatCompletionRequest,
    model: String,
    start_time: Instant,
    user: AuthenticatedUser,
) -> Result<Response, AppError> {
    // Pre-count input tokens using tiktoken
    let message_tuples = messages_to_tuples(&request.messages);
    let estimated_input_tokens = state
        .token_counter
        .count_chat_messages(&model, &message_tuples)
        .unwrap_or(0) as u64;

    // Ensure stream_options.include_usage is set to get token counts from OpenAI
    request.stream_options = Some(StreamOptions { include_usage: true });

    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    // Forward streaming request to provider
    let stream = state
        .ai_provider
        .chat_completions_stream(request_value, headers)
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

    // Wrap the stream to extract content and usage from chunks
    let tracked_stream = stream.map(move |chunk| {
        match chunk {
            Ok(bytes) => {
                // Try to parse content and usage from SSE chunks
                // Format: "data: {...}\n\n" or "data: [DONE]\n\n"
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    for line in text.lines() {
                        if let Some(json_str) = line.strip_prefix("data: ") {
                            if json_str != "[DONE]" {
                                match serde_json::from_str::<StreamChunk>(json_str) {
                                    Ok(chunk) => {
                                        // Accumulate content from delta
                                        if let Some(choice) = chunk.choices.first() {
                                            if let Some(ref content) = choice.delta.content {
                                                content_for_stream.lock().unwrap().push_str(content);
                                            }
                                        }
                                        // Capture usage if provided (usually in final chunk)
                                        if let Some(usage) = chunk.usage {
                                            let mut acc = usage_for_stream.lock().unwrap();
                                            acc.prompt_tokens = usage.prompt_tokens;
                                            acc.completion_tokens = usage.completion_tokens;
                                            acc.total_tokens = usage.total_tokens;
                                        }
                                    }
                                    Err(e) => {
                                        debug!(error = %e, chunk = %json_str, "Failed to parse SSE chunk");
                                    }
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
        let (input_tokens, output_tokens) = if openai_usage.prompt_tokens > 0 || openai_usage.completion_tokens > 0 {
            (openai_usage.prompt_tokens as u64, openai_usage.completion_tokens as u64)
        } else {
            // Fallback to estimation
            let estimated_output = token_counter
                .count_tokens(&model_for_counting, &accumulated_content)
                .unwrap_or(0) as u64;
            debug!(
                estimated_input = estimated_input_tokens,
                estimated_output = estimated_output,
                content_len = accumulated_content.len(),
                "Using estimated token counts for streaming (OpenAI didn't return usage)"
            );
            (estimated_input_tokens, estimated_output)
        };

        // Record metrics
        record_tokens("prompt", input_tokens, &model_for_metrics);
        record_tokens("completion", output_tokens, &model_for_metrics);

        // ALWAYS track usage in Zion (fire-and-forget)
        tracker_final.track(user_email_final.clone(), input_tokens, output_tokens);

        info!(
            model = %model_for_metrics,
            input_tokens = input_tokens,
            output_tokens = output_tokens,
            email = %user_email_final,
            "Streaming usage tracked"
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
