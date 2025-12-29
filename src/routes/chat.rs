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
use tracing::{info, warn};

use crate::{
    error::AppError,
    middleware::auth::AuthenticatedUser,
    proxy::AiProvider,
    routes::metrics::{record_request, record_tokens},
    AppState,
};

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
    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    let response_value = state
        .ai_provider
        .chat_completions(request_value, headers)
        .await?;

    // Parse the response
    let response: ChatCompletionResponse = serde_json::from_value(response_value)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse response: {}", e)))?;

    // Record metrics
    let duration = start_time.elapsed().as_secs_f64();
    record_request("success", &model, duration);

    if let Some(ref usage) = response.usage {
        record_tokens("prompt", usage.prompt_tokens as u64, &model);
        record_tokens("completion", usage.completion_tokens as u64, &model);

        // Track usage in Zion (fire-and-forget, never blocks)
        state.batching_tracker.track(
            user.email.clone(),
            usage.prompt_tokens as u64,
            usage.completion_tokens as u64,
        );
    }

    info!(
        model = %model,
        duration_ms = %format!("{:.2}", duration * 1000.0),
        external_id = %user.external_id,
        "Chat completion request completed"
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Streaming chunk delta for parsing usage
#[derive(Debug, Clone, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    usage: Option<Usage>,
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
    // Ensure stream_options.include_usage is set to get token counts
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

    // Track accumulated usage from stream
    let usage_accumulator = std::sync::Arc::new(std::sync::Mutex::new(Usage::default()));
    let usage_for_stream = usage_accumulator.clone();

    // Wrap the stream to extract usage from chunks and track on completion
    let tracked_stream = stream.map(move |chunk| {
        match chunk {
            Ok(bytes) => {
                // Try to parse usage from SSE chunks
                // Format: "data: {...}\n\n" or "data: [DONE]\n\n"
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    for line in text.lines() {
                        if let Some(json_str) = line.strip_prefix("data: ") {
                            if json_str != "[DONE]" {
                                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(json_str) {
                                    if let Some(usage) = chunk.usage {
                                        // Update accumulated usage (final chunk has totals)
                                        let mut acc = usage_for_stream.lock().unwrap();
                                        acc.prompt_tokens = usage.prompt_tokens;
                                        acc.completion_tokens = usage.completion_tokens;
                                        acc.total_tokens = usage.total_tokens;
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
    let usage_final = usage_accumulator.clone();
    let tracker_final = tracker.clone();

    let final_stream = async_stream::stream! {
        futures::pin_mut!(tracked_stream);
        while let Some(item) = tracked_stream.next().await {
            yield item;
        }

        // Stream completed - record metrics and track usage
        let usage = usage_final.lock().unwrap().clone();
        if usage.prompt_tokens > 0 || usage.completion_tokens > 0 {
            record_tokens("prompt", usage.prompt_tokens as u64, &model_for_metrics);
            record_tokens("completion", usage.completion_tokens as u64, &model_for_metrics);

            // Track usage in Zion (fire-and-forget)
            tracker_final.track(
                user_email_final.clone(),
                usage.prompt_tokens as u64,
                usage.completion_tokens as u64,
            );

            info!(
                model = %model_for_metrics,
                prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                email = %user_email_final,
                "Streaming usage tracked"
            );
        }
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
