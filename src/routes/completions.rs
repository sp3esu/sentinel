//! Legacy completions endpoint
//!
//! OpenAI-compatible completions API endpoint (legacy).
//! Most modern applications should use chat completions instead.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{error::AppResult, AppState};

/// Completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: String,
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
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// Usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub usage: Usage,
}

/// Handle legacy completion requests
///
/// This endpoint is compatible with OpenAI's completions API.
/// It proxies requests to the Vercel AI Gateway after checking user quotas.
pub async fn completions(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CompletionRequest>,
) -> AppResult<(StatusCode, Json<CompletionResponse>)> {
    // TODO: Phase 2 - Implement full flow similar to chat completions

    let _ = headers;
    let response = CompletionResponse {
        id: format!("cmpl-{}", uuid::Uuid::new_v4()),
        object: "text_completion".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: request.model.clone(),
        choices: vec![CompletionChoice {
            text: "Sentinel proxy is running. Full implementation pending.".to_string(),
            index: 0,
            logprobs: None,
            finish_reason: Some("stop".to_string()),
        }],
        usage: Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
    };

    Ok((StatusCode::OK, Json(response)))
}
