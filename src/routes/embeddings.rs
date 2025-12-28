//! Embeddings endpoint
//!
//! OpenAI-compatible embeddings API endpoint.
//! Tracks input token usage for billing.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::{
    error::AppError,
    middleware::auth::AuthenticatedUser,
    routes::metrics::{record_request, record_tokens},
    AppState,
};

/// Embedding input - can be a string, array of strings, array of integers, or array of arrays of integers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    /// Single text string
    String(String),
    /// Array of text strings
    StringArray(Vec<String>),
    /// Array of token integers
    Tokens(Vec<i64>),
    /// Array of arrays of token integers
    TokenArrays(Vec<Vec<i64>>),
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    /// The input text to embed
    pub input: EmbeddingInput,
    /// ID of the model to use
    pub model: String,
    /// The format to return the embeddings in (float or base64)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<String>,
    /// The number of dimensions the resulting output embeddings should have
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<i32>,
    /// A unique identifier representing your end-user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Pass through any additional fields
    #[serde(flatten)]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Embedding object in the response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The index of the embedding in the list of embeddings
    pub index: i32,
    /// The embedding vector or base64-encoded string
    pub embedding: serde_json::Value,
    /// The object type, which is always "embedding"
    pub object: String,
}

/// Usage information for embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    /// Number of tokens in the prompt
    pub prompt_tokens: i64,
    /// Total number of tokens used (same as prompt_tokens for embeddings)
    pub total_tokens: i64,
}

/// Embedding response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// The object type, which is always "list"
    pub object: String,
    /// List of embedding objects
    pub data: Vec<Embedding>,
    /// The model used for the embedding
    pub model: String,
    /// Usage statistics for the request
    pub usage: EmbeddingUsage,
}

/// Handle embedding requests
///
/// This endpoint:
/// 1. Forwards the request to the AI provider
/// 2. Tracks token usage (input tokens only, no output tokens for embeddings)
/// 3. Returns the embedding response
pub async fn embeddings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Response, AppError> {
    let start_time = Instant::now();
    let model = request.model.clone();

    debug!(
        model = %model,
        external_id = %user.external_id,
        "Processing embeddings request"
    );

    // Convert request to Value for the provider
    let request_value = serde_json::to_value(&request)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize request: {}", e)))?;

    // Forward request to provider
    let response_value = state
        .ai_provider
        .embeddings(request_value, &headers)
        .await?;

    // Parse the response
    let response: EmbeddingResponse = serde_json::from_value(response_value)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse response: {}", e)))?;

    // Record metrics
    let duration = start_time.elapsed().as_secs_f64();
    record_request("success", &model, duration);
    record_tokens("prompt", response.usage.prompt_tokens as u64, &model);

    // Track usage in Zion (fire-and-forget)
    // Embeddings only have input tokens, no output tokens
    state.batching_tracker.track(
        user.email.clone(),
        response.usage.prompt_tokens as u64,
        0, // No output tokens for embeddings
    );

    info!(
        model = %model,
        prompt_tokens = response.usage.prompt_tokens,
        duration_ms = %format!("{:.2}", duration * 1000.0),
        external_id = %user.external_id,
        "Embeddings request completed"
    );

    Ok((StatusCode::OK, Json(response)).into_response())
}
