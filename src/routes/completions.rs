//! Legacy completions endpoint
//!
//! OpenAI-compatible completions API endpoint (legacy).
//! Most modern applications should use chat completions instead.

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
    routes::metrics::{record_request, record_tokens},
    AppState,
};

/// Completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: serde_json::Value, // Can be string or array of strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_of: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
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

/// Completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChoice {
    pub text: String,
    pub index: u32,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: Option<String>,
}

/// Completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
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

/// Handle legacy completion requests
///
/// This endpoint is compatible with OpenAI's completions API.
/// It proxies requests to the AI provider after checking user quotas.
pub async fn completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CompletionRequest>,
) -> Result<Response, AppError> {
    let start_time = Instant::now();
    let model = request.model.clone();
    let is_streaming = request.stream;

    // Extract authorization token (kept for potential future use)
    let _token = extract_bearer_token(&headers);

    info!(
        model = %model,
        stream = %is_streaming,
        "Processing completion request"
    );

    if is_streaming {
        // Handle streaming response
        handle_streaming_completion(state, &headers, request, model, start_time).await
    } else {
        // Handle non-streaming response
        handle_non_streaming_completion(state, &headers, request, model, start_time).await
    }
}

/// Handle non-streaming completion
async fn handle_non_streaming_completion(
    state: Arc<AppState>,
    headers: &HeaderMap,
    request: CompletionRequest,
    model: String,
    start_time: Instant,
) -> Result<Response, AppError> {
    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    let response_value = state
        .ai_provider
        .completions(request_value, headers)
        .await?;

    // Parse the response
    let response: CompletionResponse = serde_json::from_value(response_value)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse response: {}", e)))?;

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
        "Completion request completed"
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}

/// Handle streaming completion
async fn handle_streaming_completion(
    state: Arc<AppState>,
    headers: &HeaderMap,
    request: CompletionRequest,
    model: String,
    start_time: Instant,
) -> Result<Response, AppError> {
    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    // Forward streaming request to provider
    let stream = state
        .ai_provider
        .completions_stream(request_value, headers)
        .await?;

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

    // Record that we started streaming
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
