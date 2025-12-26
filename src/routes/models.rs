//! Models endpoint
//!
//! Lists available models through the proxy.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{error::AppResult, AppState};

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

/// Models list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}

/// List available models
///
/// Returns a list of models available through the Vercel AI Gateway.
pub async fn list_models(
    State(_state): State<Arc<AppState>>,
) -> AppResult<(StatusCode, Json<ModelsResponse>)> {
    // TODO: Phase 2 - Fetch actual models from Vercel AI Gateway
    // For now, return commonly used OpenAI models

    let models = vec![
        Model {
            id: "gpt-4o".to_string(),
            object: "model".to_string(),
            created: 1706745600,
            owned_by: "openai".to_string(),
        },
        Model {
            id: "gpt-4o-mini".to_string(),
            object: "model".to_string(),
            created: 1706745600,
            owned_by: "openai".to_string(),
        },
        Model {
            id: "gpt-4-turbo".to_string(),
            object: "model".to_string(),
            created: 1706745600,
            owned_by: "openai".to_string(),
        },
        Model {
            id: "gpt-4".to_string(),
            object: "model".to_string(),
            created: 1687882411,
            owned_by: "openai".to_string(),
        },
        Model {
            id: "gpt-3.5-turbo".to_string(),
            object: "model".to_string(),
            created: 1677610602,
            owned_by: "openai".to_string(),
        },
    ];

    let response = ModelsResponse {
        object: "list".to_string(),
        data: models,
    };

    Ok((StatusCode::OK, Json(response)))
}
