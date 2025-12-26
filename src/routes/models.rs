//! Models endpoint
//!
//! Lists available models through the proxy.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{error::AppError, proxy::VercelGateway, AppState};

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// Models list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}

/// Static list of commonly used models as fallback
fn get_static_models() -> Vec<Model> {
    vec![
        Model {
            id: "gpt-4o".to_string(),
            object: "model".to_string(),
            created: 1706745600,
            owned_by: "openai".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
        Model {
            id: "gpt-4o-mini".to_string(),
            object: "model".to_string(),
            created: 1706745600,
            owned_by: "openai".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
        Model {
            id: "gpt-4-turbo".to_string(),
            object: "model".to_string(),
            created: 1706745600,
            owned_by: "openai".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
        Model {
            id: "gpt-4".to_string(),
            object: "model".to_string(),
            created: 1687882411,
            owned_by: "openai".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
        Model {
            id: "gpt-3.5-turbo".to_string(),
            object: "model".to_string(),
            created: 1677610602,
            owned_by: "openai".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
        Model {
            id: "claude-3-5-sonnet-20241022".to_string(),
            object: "model".to_string(),
            created: 1729555200,
            owned_by: "anthropic".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
        Model {
            id: "claude-3-5-haiku-20241022".to_string(),
            object: "model".to_string(),
            created: 1729555200,
            owned_by: "anthropic".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
        Model {
            id: "claude-3-opus-20240229".to_string(),
            object: "model".to_string(),
            created: 1709164800,
            owned_by: "anthropic".to_string(),
            permission: None,
            root: None,
            parent: None,
        },
    ]
}

/// List available models
///
/// Attempts to fetch models from Vercel AI Gateway, falls back to static list on error.
pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    info!("Fetching available models");

    // Create gateway client
    let gateway = VercelGateway::new(state.http_client.clone(), &state.config);

    // Try to fetch from gateway, fall back to static list
    let response = match gateway.list_models::<ModelsResponse>().await {
        Ok(models) => {
            info!(count = %models.data.len(), "Fetched models from gateway");
            models
        }
        Err(e) => {
            warn!(error = %e, "Failed to fetch models from gateway, using static list");
            ModelsResponse {
                object: "list".to_string(),
                data: get_static_models(),
            }
        }
    };

    Ok((StatusCode::OK, Json(response)))
}

/// Get a specific model by ID
///
/// Returns model details if found.
pub async fn get_model(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!(model_id = %model_id, "Fetching model details");

    // Create gateway client
    let gateway = VercelGateway::new(state.http_client.clone(), &state.config);

    // Try to fetch all models and find the requested one
    let models = match gateway.list_models::<ModelsResponse>().await {
        Ok(response) => response.data,
        Err(_) => get_static_models(),
    };

    // Find the model
    let model = models
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| AppError::NotFound(format!("Model '{}' not found", model_id)))?;

    Ok((StatusCode::OK, Json(model)))
}
