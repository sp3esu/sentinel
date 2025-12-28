//! Common test utilities for Sentinel
//!
//! This module provides shared test fixtures, mock servers, and helper functions
//! used across both unit and integration tests.

#![allow(dead_code)]

use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, path_regex, header};

/// Test configuration constants
pub mod constants {
    /// Default test API key for Zion
    pub const TEST_ZION_API_KEY: &str = "test-zion-api-key";
    /// Default test API key for OpenAI
    pub const TEST_OPENAI_API_KEY: &str = "test-openai-api-key";
    /// Default test JWT token
    pub const TEST_JWT_TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyXzEyMyIsImVtYWlsIjoidGVzdEB0ZXN0LmNvbSJ9.test";
    /// Test user ID
    pub const TEST_USER_ID: &str = "user_123";
    /// Test external ID
    pub const TEST_EXTERNAL_ID: &str = "ext_123";
    /// Test email
    pub const TEST_EMAIL: &str = "test@test.com";
}

/// Test configuration that mirrors the real Config
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub host: String,
    pub port: u16,
    pub redis_url: String,
    pub zion_api_url: String,
    pub zion_api_key: String,
    pub openai_api_url: String,
    pub openai_api_key: String,
    pub cache_ttl_seconds: u64,
    pub jwt_cache_ttl_seconds: u64,
}

impl TestConfig {
    /// Create a test config with mock server URLs
    pub fn new(zion_url: &str, openai_url: &str) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 0, // Let OS assign port
            redis_url: "redis://localhost:6379".to_string(),
            zion_api_url: zion_url.to_string(),
            zion_api_key: constants::TEST_ZION_API_KEY.to_string(),
            openai_api_url: openai_url.to_string(),
            openai_api_key: constants::TEST_OPENAI_API_KEY.to_string(),
            cache_ttl_seconds: 300,
            jwt_cache_ttl_seconds: 300,
        }
    }
}

/// Mock Zion API responses
pub mod zion_mocks {
    use super::*;
    use serde_json::json;

    /// Create a mock for successful JWT validation
    pub async fn mock_jwt_validation(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/api/v1/users/me"))
            .and(header("Authorization", format!("Bearer {}", constants::TEST_JWT_TOKEN).as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "data": {
                    "id": constants::TEST_USER_ID,
                    "email": constants::TEST_EMAIL,
                    "name": "Test User",
                    "externalId": constants::TEST_EXTERNAL_ID,
                    "emailVerified": true,
                    "createdAt": "2024-01-01T00:00:00Z",
                    "lastLoginAt": "2024-01-15T12:00:00Z"
                }
            })))
            .mount(server)
            .await;
    }

    /// Create a mock for invalid JWT
    pub async fn mock_jwt_validation_invalid(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/api/v1/users/me"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "success": false,
                "error": {
                    "code": "UNAUTHORIZED",
                    "message": "Invalid token"
                }
            })))
            .mount(server)
            .await;
    }

    /// Create a mock for user limits
    pub async fn mock_user_limits(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path_regex(r"/api/v1/limits/external/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "data": {
                    "userId": constants::TEST_USER_ID,
                    "externalId": constants::TEST_EXTERNAL_ID,
                    "limits": [
                        {
                            "name": "ai_usage",
                            "displayName": "AI Usage",
                            "description": "AI usage limits",
                            "aiInputTokens": {
                                "limit": 100000,
                                "used": 1000,
                                "remaining": 99000
                            },
                            "aiOutputTokens": {
                                "limit": 50000,
                                "used": 500,
                                "remaining": 49500
                            },
                            "aiRequests": {
                                "limit": 1000,
                                "used": 10,
                                "remaining": 990
                            },
                            "resetPeriod": "MONTHLY",
                            "periodStart": "2024-01-01T00:00:00Z",
                            "periodEnd": "2024-01-31T23:59:59Z"
                        }
                    ]
                }
            })))
            .mount(server)
            .await;
    }
}

/// Mock OpenAI API responses
pub mod openai_mocks {
    use super::*;
    use serde_json::json;

    /// Create a mock for listing models
    pub async fn mock_list_models(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "object": "list",
                "data": [
                    {
                        "id": "gpt-4o",
                        "object": "model",
                        "created": 1706745600,
                        "owned_by": "openai"
                    },
                    {
                        "id": "gpt-4o-mini",
                        "object": "model",
                        "created": 1706745600,
                        "owned_by": "openai"
                    },
                    {
                        "id": "claude-3-5-sonnet-20241022",
                        "object": "model",
                        "created": 1729555200,
                        "owned_by": "anthropic"
                    }
                ]
            })))
            .mount(server)
            .await;
    }

    /// Create a mock for chat completions (non-streaming)
    pub async fn mock_chat_completions(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-test123",
                "object": "chat.completion",
                "created": 1706745600,
                "model": "gpt-4o",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello! How can I help you today?"
                        },
                        "finish_reason": "stop"
                    }
                ],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 8,
                    "total_tokens": 18
                }
            })))
            .mount(server)
            .await;
    }

    /// Create a mock for streaming chat completions
    pub async fn mock_chat_completions_streaming(server: &MockServer) {
        // SSE format streaming response
        let stream_data = concat!(
            "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"!\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-test123\",\"object\":\"chat.completion.chunk\",\"created\":1706745600,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(stream_data)
                    .insert_header("content-type", "text/event-stream")
                    .insert_header("cache-control", "no-cache")
            )
            .mount(server)
            .await;
    }
}

/// Sample request/response data for tests
pub mod test_data {
    use serde_json::json;

    /// Valid chat completion request
    pub fn valid_chat_request() -> serde_json::Value {
        json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": "Hello, how are you?"
                }
            ]
        })
    }

    /// Chat completion request with streaming
    pub fn streaming_chat_request() -> serde_json::Value {
        json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": "Hello!"
                }
            ],
            "stream": true
        })
    }

    /// Chat completion request missing messages
    pub fn invalid_chat_request_no_messages() -> serde_json::Value {
        json!({
            "model": "gpt-4o"
        })
    }

    /// Chat completion request missing model
    pub fn invalid_chat_request_no_model() -> serde_json::Value {
        json!({
            "messages": [
                {
                    "role": "user",
                    "content": "Hello"
                }
            ]
        })
    }
}
