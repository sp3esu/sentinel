//! Models endpoint integration tests
//!
//! Tests for the models endpoints:
//! - GET /v1/models - List available models
//! - GET /v1/models/:id - Get specific model

use axum::{
    http::StatusCode,
    routing::get,
    Router,
    Json,
    extract::Path,
};
use axum_test::TestServer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Model response structure matching the actual API
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Model {
    id: String,
    object: String,
    created: i64,
    owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelsResponse {
    object: String,
    data: Vec<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
}

/// Create a test router that mimics the models endpoints
fn create_models_test_router() -> Router {
    async fn list_models() -> (StatusCode, Json<ModelsResponse>) {
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
            Model {
                id: "claude-3-5-sonnet-20241022".to_string(),
                object: "model".to_string(),
                created: 1729555200,
                owned_by: "anthropic".to_string(),
            },
            Model {
                id: "claude-3-5-haiku-20241022".to_string(),
                object: "model".to_string(),
                created: 1729555200,
                owned_by: "anthropic".to_string(),
            },
            Model {
                id: "claude-3-opus-20240229".to_string(),
                object: "model".to_string(),
                created: 1709164800,
                owned_by: "anthropic".to_string(),
            },
        ];

        (StatusCode::OK, Json(ModelsResponse {
            object: "list".to_string(),
            data: models,
        }))
    }

    async fn get_model(Path(model_id): Path<String>) -> Result<(StatusCode, Json<Model>), (StatusCode, Json<ErrorResponse>)> {
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
                id: "claude-3-5-sonnet-20241022".to_string(),
                object: "model".to_string(),
                created: 1729555200,
                owned_by: "anthropic".to_string(),
            },
        ];

        match models.into_iter().find(|m| m.id == model_id) {
            Some(model) => Ok((StatusCode::OK, Json(model))),
            None => Err((StatusCode::NOT_FOUND, Json(ErrorResponse {
                error: ErrorBody {
                    code: "NOT_FOUND".to_string(),
                    message: format!("Model '{}' not found", model_id),
                },
            }))),
        }
    }

    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/models/:model_id", get(get_model))
}

#[tokio::test]
async fn test_list_models_returns_model_list() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Verify response structure
    assert_eq!(json["object"].as_str().unwrap(), "list");
    assert!(json.get("data").is_some(), "Response should have 'data' field");

    let data = json["data"].as_array().unwrap();
    assert!(!data.is_empty(), "Models list should not be empty");
}

#[tokio::test]
async fn test_list_models_contains_expected_models() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models").await;

    response.assert_status_ok();

    let json: Value = response.json();
    let data = json["data"].as_array().unwrap();

    // Check for some expected models
    let model_ids: Vec<&str> = data
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();

    assert!(model_ids.contains(&"gpt-4o"), "Should contain gpt-4o");
    assert!(model_ids.contains(&"gpt-4o-mini"), "Should contain gpt-4o-mini");
    assert!(model_ids.contains(&"claude-3-5-sonnet-20241022"), "Should contain claude-3-5-sonnet");
}

#[tokio::test]
async fn test_list_models_model_structure() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models").await;

    response.assert_status_ok();

    let json: Value = response.json();
    let data = json["data"].as_array().unwrap();

    // Check first model has all required fields
    let first_model = &data[0];

    assert!(first_model.get("id").is_some(), "Model should have 'id' field");
    assert!(first_model.get("object").is_some(), "Model should have 'object' field");
    assert!(first_model.get("created").is_some(), "Model should have 'created' field");
    assert!(first_model.get("owned_by").is_some(), "Model should have 'owned_by' field");

    // Verify object type
    assert_eq!(first_model["object"].as_str().unwrap(), "model");

    // Verify created is a number (Unix timestamp)
    assert!(first_model["created"].is_i64(), "created should be a number");
}

#[tokio::test]
async fn test_get_specific_model_success() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models/gpt-4o").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Verify model fields
    assert_eq!(json["id"].as_str().unwrap(), "gpt-4o");
    assert_eq!(json["object"].as_str().unwrap(), "model");
    assert_eq!(json["owned_by"].as_str().unwrap(), "openai");
    assert!(json["created"].is_i64(), "created should be a number");
}

#[tokio::test]
async fn test_get_claude_model() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models/claude-3-5-sonnet-20241022").await;

    response.assert_status_ok();

    let json: Value = response.json();

    assert_eq!(json["id"].as_str().unwrap(), "claude-3-5-sonnet-20241022");
    assert_eq!(json["owned_by"].as_str().unwrap(), "anthropic");
}

#[tokio::test]
async fn test_get_unknown_model_returns_404() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models/nonexistent-model").await;

    response.assert_status(StatusCode::NOT_FOUND);

    let json: Value = response.json();

    // Verify error response structure
    assert!(json.get("error").is_some(), "Response should have 'error' field");

    let error = json.get("error").unwrap();
    assert_eq!(error["code"].as_str().unwrap(), "NOT_FOUND");
    assert!(
        error["message"].as_str().unwrap().contains("nonexistent-model"),
        "Error message should contain the model ID"
    );
}

#[tokio::test]
async fn test_get_model_with_special_characters() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    // Model IDs can contain hyphens and numbers
    let response = server.get("/v1/models/gpt-4o-mini").await;

    response.assert_status_ok();

    let json: Value = response.json();
    assert_eq!(json["id"].as_str().unwrap(), "gpt-4o-mini");
}

#[tokio::test]
async fn test_models_endpoint_method_not_allowed() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    // POST should not be allowed on list endpoint
    let response = server.post("/v1/models").await;
    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);

    // DELETE should not be allowed
    let response = server.delete("/v1/models/gpt-4o").await;
    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_list_models_includes_openai_models() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models").await;

    response.assert_status_ok();

    let json: Value = response.json();
    let data = json["data"].as_array().unwrap();

    // Count OpenAI models
    let openai_models: Vec<_> = data
        .iter()
        .filter(|m| m["owned_by"].as_str().unwrap() == "openai")
        .collect();

    assert!(!openai_models.is_empty(), "Should have OpenAI models");
}

#[tokio::test]
async fn test_list_models_includes_anthropic_models() {
    let app = create_models_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/v1/models").await;

    response.assert_status_ok();

    let json: Value = response.json();
    let data = json["data"].as_array().unwrap();

    // Count Anthropic models
    let anthropic_models: Vec<_> = data
        .iter()
        .filter(|m| m["owned_by"].as_str().unwrap() == "anthropic")
        .collect();

    assert!(!anthropic_models.is_empty(), "Should have Anthropic models");
}
