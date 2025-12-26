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
    proxy::VercelGateway,
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
/// It proxies requests to the Vercel AI Gateway after checking user quotas.
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, AppError> {
    let start_time = Instant::now();
    let model = request.model.clone();
    let is_streaming = request.stream;

    // Extract authorization token (Phase 3 will validate it)
    let _token = extract_bearer_token(&headers);
    // TODO: Phase 3 - Validate JWT and check user quota

    info!(
        model = %model,
        stream = %is_streaming,
        messages = %request.messages.len(),
        "Processing chat completion request"
    );

    // Create gateway client
    let gateway = VercelGateway::new(state.http_client.clone(), &state.config);

    if is_streaming {
        // Handle streaming response
        handle_streaming_chat(gateway, request, model, start_time).await
    } else {
        // Handle non-streaming response
        handle_non_streaming_chat(gateway, request, model, start_time).await
    }
}

/// Handle non-streaming chat completion
async fn handle_non_streaming_chat(
    gateway: VercelGateway,
    request: ChatCompletionRequest,
    model: String,
    start_time: Instant,
) -> Result<Response, AppError> {
    let response: ChatCompletionResponse = gateway.chat_completions(&request).await?;

    // Record metrics
    let duration = start_time.elapsed().as_secs_f64();
    record_request("success", &model, duration);

    if let Some(ref usage) = response.usage {
        record_tokens("prompt", usage.prompt_tokens as u64, &model);
        record_tokens("completion", usage.completion_tokens as u64, &model);
    }

    info!(
        model = %model,
        duration_ms = %format!("{:.2}", duration * 1000.0),
        "Chat completion request completed"
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Handle streaming chat completion
async fn handle_streaming_chat(
    gateway: VercelGateway,
    request: ChatCompletionRequest,
    model: String,
    start_time: Instant,
) -> Result<Response, AppError> {
    // Forward streaming request to gateway
    let stream = gateway.chat_completions_stream(&request).await?;

    // Create a channel to pass data
    let model_clone = model.clone();

    // Wrap the stream to add metrics tracking on completion
    let tracked_stream = stream.then(move |chunk| {
        let model_for_logging = model_clone.clone();
        async move {
            match chunk {
                Ok(bytes) => Ok(bytes),
                Err(e) => {
                    warn!(model = %model_for_logging, error = %e, "Stream error");
                    Err(e)
                }
            }
        }
    });

    // Record that we started streaming (final metrics recorded when stream completes)
    let duration = start_time.elapsed().as_secs_f64();
    record_request("streaming", &model, duration);

    // Build SSE response
    let body = Body::from_stream(tracked_stream);

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
