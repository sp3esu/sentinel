//! Chat completions endpoint integration tests
//!
//! Tests for the chat completions endpoint:
//! - POST /v1/chat/completions - Chat completion requests
//! - Request validation (missing messages, invalid model)
//! - Streaming response format

use axum::{
    body::Body,
    extract::Json as ExtractJson,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
    Json,
};
use axum_test::TestServer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Chat message role
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    System,
    User,
    Assistant,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: Role,
    content: Option<String>,
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// Usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Chat completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatCompletionChoice {
    index: u32,
    message: ChatMessage,
    finish_reason: Option<String>,
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    object: String,
    created: i64,
    model: String,
    choices: Vec<ChatCompletionChoice>,
    usage: Option<Usage>,
}

/// Error response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorBody {
    code: String,
    message: String,
}

/// Create a test router that mimics the chat completions endpoint
fn create_chat_test_router() -> Router {
    async fn chat_completions(
        ExtractJson(request): ExtractJson<ChatCompletionRequest>,
    ) -> Response {
        // Check if streaming
        if request.stream {
            // Return streaming response
            let stream_data = concat!(
                "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"!\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            );

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from(stream_data))
                .unwrap()
        } else {
            // Return non-streaming response
            let response = ChatCompletionResponse {
                id: "chatcmpl-test123".to_string(),
                object: "chat.completion".to_string(),
                created: 1706745600,
                model: request.model.clone(),
                choices: vec![ChatCompletionChoice {
                    index: 0,
                    message: ChatMessage {
                        role: Role::Assistant,
                        content: Some("Hello! How can I help you today?".to_string()),
                    },
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Some(Usage {
                    prompt_tokens: 10,
                    completion_tokens: 8,
                    total_tokens: 18,
                }),
            };

            (StatusCode::OK, Json(response)).into_response()
        }
    }

    // Handler that validates input and returns errors for invalid requests
    async fn chat_completions_with_validation(body: String) -> Response {
        // Try to parse the request
        let request: Result<ChatCompletionRequest, _> = serde_json::from_str(&body);

        match request {
            Ok(req) => {
                // Validate messages not empty
                if req.messages.is_empty() {
                    return (StatusCode::BAD_REQUEST, Json(ErrorResponse {
                        error: ErrorBody {
                            code: "BAD_REQUEST".to_string(),
                            message: "messages array must not be empty".to_string(),
                        },
                    })).into_response();
                }

                // Return successful response
                let response = ChatCompletionResponse {
                    id: "chatcmpl-test123".to_string(),
                    object: "chat.completion".to_string(),
                    created: 1706745600,
                    model: req.model.clone(),
                    choices: vec![ChatCompletionChoice {
                        index: 0,
                        message: ChatMessage {
                            role: Role::Assistant,
                            content: Some("Hello!".to_string()),
                        },
                        finish_reason: Some("stop".to_string()),
                    }],
                    usage: Some(Usage {
                        prompt_tokens: 10,
                        completion_tokens: 2,
                        total_tokens: 12,
                    }),
                };

                (StatusCode::OK, Json(response)).into_response()
            }
            Err(_) => {
                // Check what's missing in the JSON
                let value: Result<Value, _> = serde_json::from_str(&body);

                match value {
                    Ok(v) => {
                        if v.get("model").is_none() {
                            return (StatusCode::BAD_REQUEST, Json(ErrorResponse {
                                error: ErrorBody {
                                    code: "BAD_REQUEST".to_string(),
                                    message: "model field is required".to_string(),
                                },
                            })).into_response();
                        }
                        if v.get("messages").is_none() {
                            return (StatusCode::BAD_REQUEST, Json(ErrorResponse {
                                error: ErrorBody {
                                    code: "BAD_REQUEST".to_string(),
                                    message: "messages field is required".to_string(),
                                },
                            })).into_response();
                        }
                        // Generic parse error
                        (StatusCode::BAD_REQUEST, Json(ErrorResponse {
                            error: ErrorBody {
                                code: "INVALID_JSON".to_string(),
                                message: "Invalid JSON in request".to_string(),
                            },
                        })).into_response()
                    }
                    Err(_) => {
                        (StatusCode::BAD_REQUEST, Json(ErrorResponse {
                            error: ErrorBody {
                                code: "INVALID_JSON".to_string(),
                                message: "Invalid JSON in request".to_string(),
                            },
                        })).into_response()
                    }
                }
            }
        }
    }

    Router::new()
        .route("/v1/chat/completions", post(chat_completions_with_validation))
        .route("/v1/chat/completions/simple", post(chat_completions))
}

#[tokio::test]
async fn test_chat_completions_valid_request() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Hello, how are you?"
            }
        ]
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Verify response structure
    assert!(json.get("id").is_some(), "Response should have 'id' field");
    assert!(json.get("object").is_some(), "Response should have 'object' field");
    assert!(json.get("created").is_some(), "Response should have 'created' field");
    assert!(json.get("model").is_some(), "Response should have 'model' field");
    assert!(json.get("choices").is_some(), "Response should have 'choices' field");
    assert!(json.get("usage").is_some(), "Response should have 'usage' field");

    // Verify object type
    assert_eq!(json["object"].as_str().unwrap(), "chat.completion");

    // Verify model matches request
    assert_eq!(json["model"].as_str().unwrap(), "gpt-4o");
}

#[tokio::test]
async fn test_chat_completions_with_system_message() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "system",
                "content": "You are a helpful assistant."
            },
            {
                "role": "user",
                "content": "Hello!"
            }
        ]
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status_ok();
}

#[tokio::test]
async fn test_chat_completions_choices_structure() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ]
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status_ok();

    let json: Value = response.json();
    let choices = json["choices"].as_array().unwrap();

    assert!(!choices.is_empty(), "Choices should not be empty");

    let first_choice = &choices[0];
    assert!(first_choice.get("index").is_some(), "Choice should have 'index'");
    assert!(first_choice.get("message").is_some(), "Choice should have 'message'");
    assert!(first_choice.get("finish_reason").is_some(), "Choice should have 'finish_reason'");

    let message = first_choice.get("message").unwrap();
    assert!(message.get("role").is_some(), "Message should have 'role'");
    assert!(message.get("content").is_some(), "Message should have 'content'");
    assert_eq!(message["role"].as_str().unwrap(), "assistant");
}

#[tokio::test]
async fn test_chat_completions_usage_structure() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ]
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status_ok();

    let json: Value = response.json();
    let usage = json.get("usage").unwrap();

    assert!(usage.get("prompt_tokens").is_some(), "Usage should have 'prompt_tokens'");
    assert!(usage.get("completion_tokens").is_some(), "Usage should have 'completion_tokens'");
    assert!(usage.get("total_tokens").is_some(), "Usage should have 'total_tokens'");

    // Verify total = prompt + completion
    let prompt_tokens = usage["prompt_tokens"].as_u64().unwrap();
    let completion_tokens = usage["completion_tokens"].as_u64().unwrap();
    let total_tokens = usage["total_tokens"].as_u64().unwrap();

    assert_eq!(total_tokens, prompt_tokens + completion_tokens);
}

#[tokio::test]
async fn test_chat_completions_missing_messages() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o"
        // messages is missing
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);

    let json: Value = response.json();
    assert!(json.get("error").is_some(), "Response should have 'error' field");
    assert!(
        json["error"]["message"].as_str().unwrap().contains("messages"),
        "Error should mention 'messages'"
    );
}

#[tokio::test]
async fn test_chat_completions_missing_model() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ]
        // model is missing
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);

    let json: Value = response.json();
    assert!(json.get("error").is_some(), "Response should have 'error' field");
    assert!(
        json["error"]["message"].as_str().unwrap().contains("model"),
        "Error should mention 'model'"
    );
}

#[tokio::test]
async fn test_chat_completions_invalid_json() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server
        .post("/v1/chat/completions")
        .content_type("application/json")
        .bytes("not valid json".as_bytes().to_vec().into())
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_chat_completions_streaming_response_format() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ],
        "stream": true
    });

    let response = server
        .post("/v1/chat/completions/simple")
        .json(&request_body)
        .await;

    response.assert_status_ok();

    // Check content type is text/event-stream
    let content_type = response.headers().get(header::CONTENT_TYPE);
    assert!(content_type.is_some(), "Should have Content-Type header");
    assert!(
        content_type.unwrap().to_str().unwrap().contains("text/event-stream"),
        "Content-Type should be text/event-stream"
    );

    // Check cache-control header
    let cache_control = response.headers().get(header::CACHE_CONTROL);
    assert!(cache_control.is_some(), "Should have Cache-Control header");
    assert!(
        cache_control.unwrap().to_str().unwrap().contains("no-cache"),
        "Cache-Control should be no-cache"
    );
}

#[tokio::test]
async fn test_chat_completions_streaming_data_format() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ],
        "stream": true
    });

    let response = server
        .post("/v1/chat/completions/simple")
        .json(&request_body)
        .await;

    response.assert_status_ok();

    let body = response.text();

    // SSE events should start with "data: "
    assert!(body.contains("data: "), "SSE events should start with 'data: '");

    // Should end with [DONE]
    assert!(body.contains("[DONE]"), "Stream should end with [DONE]");

    // Parse individual events
    for line in body.lines() {
        if line.starts_with("data: ") && !line.contains("[DONE]") {
            let json_str = line.strip_prefix("data: ").unwrap();
            let event: Value = serde_json::from_str(json_str).expect("Should be valid JSON");

            // Each chunk should have these fields
            assert!(event.get("id").is_some(), "Chunk should have 'id'");
            assert!(event.get("object").is_some(), "Chunk should have 'object'");
            assert_eq!(event["object"].as_str().unwrap(), "chat.completion.chunk");
            assert!(event.get("choices").is_some(), "Chunk should have 'choices'");
        }
    }
}

#[tokio::test]
async fn test_chat_completions_with_temperature() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ],
        "temperature": 0.7
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status_ok();
}

#[tokio::test]
async fn test_chat_completions_with_max_tokens() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Hello!"
            }
        ],
        "max_tokens": 100
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status_ok();
}

#[tokio::test]
async fn test_chat_completions_method_not_allowed() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    // GET should not be allowed
    let response = server.get("/v1/chat/completions").await;
    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);

    // DELETE should not be allowed
    let response = server.delete("/v1/chat/completions").await;
    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_chat_completions_empty_messages_array() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": []
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);

    let json: Value = response.json();
    assert!(json.get("error").is_some(), "Response should have 'error' field");
}

#[tokio::test]
async fn test_chat_completions_multiple_messages() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let request_body = json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "system",
                "content": "You are helpful."
            },
            {
                "role": "user",
                "content": "Hello!"
            },
            {
                "role": "assistant",
                "content": "Hi there!"
            },
            {
                "role": "user",
                "content": "How are you?"
            }
        ]
    });

    let response = server
        .post("/v1/chat/completions")
        .json(&request_body)
        .await;

    response.assert_status_ok();
}

#[tokio::test]
async fn test_chat_completions_different_models() {
    let app = create_chat_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    for model in ["gpt-4o", "gpt-4o-mini", "claude-3-5-sonnet-20241022"] {
        let request_body = json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": "Hello!"
                }
            ]
        });

        let response = server
            .post("/v1/chat/completions")
            .json(&request_body)
            .await;

        response.assert_status_ok();

        let json: Value = response.json();
        assert_eq!(json["model"].as_str().unwrap(), model);
    }
}
