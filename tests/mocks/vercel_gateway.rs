//! Mock Vercel AI Gateway for testing
//!
//! Provides wiremock-based mocks for Vercel AI Gateway endpoints:
//! - POST /v1/chat/completions - Chat completions (streaming and non-streaming)
//! - POST /v1/completions - Text completions
//! - GET /v1/models - List available models
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::mocks::vercel_gateway::{MockVercelGateway, VercelTestData};
//!
//! #[tokio::test]
//! async fn test_with_vercel_mock() {
//!     let mock_gateway = MockVercelGateway::start().await;
//!
//!     // Set up successful chat completion response
//!     mock_gateway.mock_chat_completion_success(VercelTestData::simple_response()).await;
//!
//!     // Use mock_gateway.uri() as the Vercel Gateway URL
//!     // ...
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use wiremock::{
    matchers::{header, header_exists, method, path},
    Mock, MockServer, ResponseTemplate,
};

/// Counter for generating unique IDs
static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique ID for mock responses
fn generate_id(prefix: &str) -> String {
    let counter = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{}-{}-{}", prefix, timestamp, counter)
}

/// Get current Unix timestamp
fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Mock Vercel AI Gateway server wrapper
pub struct MockVercelGateway {
    server: MockServer,
}

impl MockVercelGateway {
    /// Start a new mock Vercel Gateway server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        Self { server }
    }

    /// Get the mock server URI
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// Get the mock server address (host:port)
    pub fn address(&self) -> String {
        self.server.address().to_string()
    }

    // =========================================================================
    // POST /v1/chat/completions - Chat Completions
    // =========================================================================

    /// Mock successful chat completion response (non-streaming)
    pub async fn mock_chat_completion_success(&self, response: ChatCompletionResponseMock) {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header_exists("Authorization"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock successful chat completion streaming response (SSE format)
    pub async fn mock_chat_completion_stream(&self, chunks: Vec<ChatCompletionChunkMock>) {
        let sse_body = Self::format_sse_stream(&chunks);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header_exists("Authorization"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_body)
                    .insert_header("Content-Type", "text/event-stream")
                    .insert_header("Cache-Control", "no-cache")
                    .insert_header("Connection", "keep-alive"),
            )
            .mount(&self.server)
            .await;
    }

    /// Mock chat completion with custom token usage
    pub async fn mock_chat_completion_with_usage(
        &self,
        content: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
    ) {
        let response = ChatCompletionResponseMock {
            id: generate_id("chatcmpl"),
            object: "chat.completion".to_string(),
            created: current_timestamp(),
            model: "gpt-4".to_string(),
            choices: vec![ChatChoiceMock {
                index: 0,
                message: ChatMessageMock {
                    role: "assistant".to_string(),
                    content: Some(content.to_string()),
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(UsageMock {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }),
        };

        self.mock_chat_completion_success(response).await;
    }

    /// Mock 401 Unauthorized for chat completions
    pub async fn mock_chat_completion_unauthorized(&self) {
        let response = OpenAIErrorResponseMock {
            error: OpenAIErrorMock {
                message: "Invalid API key provided".to_string(),
                error_type: "invalid_request_error".to_string(),
                param: None,
                code: Some("invalid_api_key".to_string()),
            },
        };

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 429 Rate Limited for chat completions
    pub async fn mock_chat_completion_rate_limited(&self) {
        let response = OpenAIErrorResponseMock {
            error: OpenAIErrorMock {
                message: "Rate limit exceeded. Please retry after 60 seconds.".to_string(),
                error_type: "rate_limit_error".to_string(),
                param: None,
                code: Some("rate_limit_exceeded".to_string()),
            },
        };

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_json(&response)
                    .insert_header("Retry-After", "60"),
            )
            .mount(&self.server)
            .await;
    }

    /// Mock 500 Internal Server Error for chat completions
    pub async fn mock_chat_completion_server_error(&self) {
        let response = OpenAIErrorResponseMock {
            error: OpenAIErrorMock {
                message: "The server had an error while processing your request".to_string(),
                error_type: "server_error".to_string(),
                param: None,
                code: Some("internal_error".to_string()),
            },
        };

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 503 Service Unavailable for chat completions
    pub async fn mock_chat_completion_service_unavailable(&self) {
        let response = OpenAIErrorResponseMock {
            error: OpenAIErrorMock {
                message: "The engine is currently overloaded, please try again later".to_string(),
                error_type: "service_unavailable".to_string(),
                param: None,
                code: Some("overloaded".to_string()),
            },
        };

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(503).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    // =========================================================================
    // POST /v1/completions - Text Completions
    // =========================================================================

    /// Mock successful completion response (non-streaming)
    pub async fn mock_completion_success(&self, response: CompletionResponseMock) {
        Mock::given(method("POST"))
            .and(path("/v1/completions"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock completion with custom token usage
    pub async fn mock_completion_with_usage(
        &self,
        text: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
    ) {
        let response = CompletionResponseMock {
            id: generate_id("cmpl"),
            object: "text_completion".to_string(),
            created: current_timestamp(),
            model: "gpt-3.5-turbo-instruct".to_string(),
            choices: vec![CompletionChoiceMock {
                index: 0,
                text: text.to_string(),
                logprobs: None,
                finish_reason: Some("stop".to_string()),
            }],
            usage: UsageMock {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        };

        self.mock_completion_success(response).await;
    }

    /// Mock 401 Unauthorized for completions
    pub async fn mock_completion_unauthorized(&self) {
        let response = OpenAIErrorResponseMock {
            error: OpenAIErrorMock {
                message: "Invalid API key provided".to_string(),
                error_type: "invalid_request_error".to_string(),
                param: None,
                code: Some("invalid_api_key".to_string()),
            },
        };

        Mock::given(method("POST"))
            .and(path("/v1/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 500 Internal Server Error for completions
    pub async fn mock_completion_server_error(&self) {
        let response = OpenAIErrorResponseMock {
            error: OpenAIErrorMock {
                message: "The server had an error while processing your request".to_string(),
                error_type: "server_error".to_string(),
                param: None,
                code: Some("internal_error".to_string()),
            },
        };

        Mock::given(method("POST"))
            .and(path("/v1/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    // =========================================================================
    // GET /v1/models - List Models
    // =========================================================================

    /// Mock successful models list response
    pub async fn mock_list_models_success(&self, models: Vec<ModelMock>) {
        let response = ModelsListResponseMock {
            object: "list".to_string(),
            data: models,
        };

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 401 Unauthorized for models list
    pub async fn mock_list_models_unauthorized(&self) {
        let response = OpenAIErrorResponseMock {
            error: OpenAIErrorMock {
                message: "Invalid API key provided".to_string(),
                error_type: "invalid_request_error".to_string(),
                param: None,
                code: Some("invalid_api_key".to_string()),
            },
        };

        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Format chunks into SSE stream format
    fn format_sse_stream(chunks: &[ChatCompletionChunkMock]) -> String {
        let mut result = String::new();

        for chunk in chunks {
            let json = serde_json::to_string(chunk).unwrap();
            result.push_str(&format!("data: {}\n\n", json));
        }

        // Add the final [DONE] message
        result.push_str("data: [DONE]\n\n");

        result
    }
}

// =============================================================================
// Mock Data Types (matching OpenAI API response formats)
// =============================================================================

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageMock {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCallMock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallMock>>,
}

/// Function call in message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallMock {
    pub name: String,
    pub arguments: String,
}

/// Tool call in message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallMock {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCallMock,
}

/// Chat completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoiceMock {
    pub index: i32,
    pub message: ChatMessageMock,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMock {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponseMock {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoiceMock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageMock>,
}

/// Delta content for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaMock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCallMock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallMock>>,
}

/// Chat completion chunk choice (for streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChunkChoiceMock {
    pub index: i32,
    pub delta: DeltaMock,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Chat completion chunk (for streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunkMock {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChunkChoiceMock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageMock>,
}

/// Completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChoiceMock {
    pub index: i32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Text completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponseMock {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<CompletionChoiceMock>,
    pub usage: UsageMock,
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMock {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

/// Models list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListResponseMock {
    pub object: String,
    pub data: Vec<ModelMock>,
}

/// OpenAI error detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIErrorMock {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// OpenAI error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIErrorResponseMock {
    pub error: OpenAIErrorMock,
}

// =============================================================================
// Test Data Factories
// =============================================================================

/// Factory for creating test data
pub struct VercelTestData;

impl VercelTestData {
    /// Create a simple chat completion response
    pub fn simple_chat_response(content: &str) -> ChatCompletionResponseMock {
        ChatCompletionResponseMock {
            id: generate_id("chatcmpl"),
            object: "chat.completion".to_string(),
            created: current_timestamp(),
            model: "gpt-4".to_string(),
            choices: vec![ChatChoiceMock {
                index: 0,
                message: ChatMessageMock {
                    role: "assistant".to_string(),
                    content: Some(content.to_string()),
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(UsageMock {
                prompt_tokens: 50,
                completion_tokens: 100,
                total_tokens: 150,
            }),
        }
    }

    /// Create a chat response with function call
    pub fn function_call_response(name: &str, arguments: &str) -> ChatCompletionResponseMock {
        ChatCompletionResponseMock {
            id: generate_id("chatcmpl"),
            object: "chat.completion".to_string(),
            created: current_timestamp(),
            model: "gpt-4".to_string(),
            choices: vec![ChatChoiceMock {
                index: 0,
                message: ChatMessageMock {
                    role: "assistant".to_string(),
                    content: None,
                    function_call: Some(FunctionCallMock {
                        name: name.to_string(),
                        arguments: arguments.to_string(),
                    }),
                    tool_calls: None,
                },
                finish_reason: Some("function_call".to_string()),
            }],
            usage: Some(UsageMock {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            }),
        }
    }

    /// Create streaming chunks for a simple response
    pub fn streaming_chunks(content: &str) -> Vec<ChatCompletionChunkMock> {
        let id = generate_id("chatcmpl");
        let created = current_timestamp();
        let model = "gpt-4".to_string();

        let mut chunks = Vec::new();

        // Initial chunk with role
        chunks.push(ChatCompletionChunkMock {
            id: id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model.clone(),
            choices: vec![ChatChunkChoiceMock {
                index: 0,
                delta: DeltaMock {
                    role: Some("assistant".to_string()),
                    content: None,
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            usage: None,
        });

        // Content chunks (split by words for realism)
        for word in content.split_whitespace() {
            chunks.push(ChatCompletionChunkMock {
                id: id.clone(),
                object: "chat.completion.chunk".to_string(),
                created,
                model: model.clone(),
                choices: vec![ChatChunkChoiceMock {
                    index: 0,
                    delta: DeltaMock {
                        role: None,
                        content: Some(format!("{} ", word)),
                        function_call: None,
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
                usage: None,
            });
        }

        // Final chunk with finish reason and usage
        chunks.push(ChatCompletionChunkMock {
            id: id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model.clone(),
            choices: vec![ChatChunkChoiceMock {
                index: 0,
                delta: DeltaMock {
                    role: None,
                    content: None,
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(UsageMock {
                prompt_tokens: 50,
                completion_tokens: content.split_whitespace().count() as i64 * 2,
                total_tokens: 50 + content.split_whitespace().count() as i64 * 2,
            }),
        });

        chunks
    }

    /// Create a simple text completion response
    pub fn simple_completion_response(text: &str) -> CompletionResponseMock {
        CompletionResponseMock {
            id: generate_id("cmpl"),
            object: "text_completion".to_string(),
            created: current_timestamp(),
            model: "gpt-3.5-turbo-instruct".to_string(),
            choices: vec![CompletionChoiceMock {
                index: 0,
                text: text.to_string(),
                logprobs: None,
                finish_reason: Some("stop".to_string()),
            }],
            usage: UsageMock {
                prompt_tokens: 20,
                completion_tokens: 50,
                total_tokens: 70,
            },
        }
    }

    /// Create default list of models
    pub fn default_models() -> Vec<ModelMock> {
        vec![
            ModelMock {
                id: "gpt-4".to_string(),
                object: "model".to_string(),
                created: 1687882410,
                owned_by: "openai".to_string(),
            },
            ModelMock {
                id: "gpt-4-turbo".to_string(),
                object: "model".to_string(),
                created: 1706037612,
                owned_by: "openai".to_string(),
            },
            ModelMock {
                id: "gpt-3.5-turbo".to_string(),
                object: "model".to_string(),
                created: 1677610602,
                owned_by: "openai".to_string(),
            },
            ModelMock {
                id: "gpt-3.5-turbo-instruct".to_string(),
                object: "model".to_string(),
                created: 1692901427,
                owned_by: "openai".to_string(),
            },
            ModelMock {
                id: "text-embedding-ada-002".to_string(),
                object: "model".to_string(),
                created: 1671217299,
                owned_by: "openai-internal".to_string(),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_starts() {
        let mock = MockVercelGateway::start().await;
        assert!(!mock.uri().is_empty());
    }

    #[tokio::test]
    async fn test_mock_chat_completion_success() {
        let mock = MockVercelGateway::start().await;
        let response = VercelTestData::simple_chat_response("Hello, world!");
        mock.mock_chat_completion_success(response).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/chat/completions", mock.uri()))
            .header("Authorization", "Bearer test-key")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": "gpt-4",
                "messages": [{"role": "user", "content": "Hi"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: ChatCompletionResponseMock = response.json().await.unwrap();
        assert_eq!(body.choices[0].message.content, Some("Hello, world!".to_string()));
    }

    #[tokio::test]
    async fn test_mock_streaming_response() {
        let mock = MockVercelGateway::start().await;
        let chunks = VercelTestData::streaming_chunks("Hello world test");
        mock.mock_chat_completion_stream(chunks).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/chat/completions", mock.uri()))
            .header("Authorization", "Bearer test-key")
            .json(&serde_json::json!({
                "model": "gpt-4",
                "messages": [{"role": "user", "content": "Hi"}],
                "stream": true
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.text().await.unwrap();
        assert!(body.contains("data:"));
        assert!(body.contains("[DONE]"));
    }

    #[tokio::test]
    async fn test_mock_list_models() {
        let mock = MockVercelGateway::start().await;
        mock.mock_list_models_success(VercelTestData::default_models()).await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/v1/models", mock.uri()))
            .header("Authorization", "Bearer test-key")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: ModelsListResponseMock = response.json().await.unwrap();
        assert!(!body.data.is_empty());
        assert!(body.data.iter().any(|m| m.id == "gpt-4"));
    }

    #[tokio::test]
    async fn test_test_data_factories() {
        let chat_response = VercelTestData::simple_chat_response("Test");
        assert_eq!(chat_response.choices.len(), 1);
        assert!(chat_response.usage.is_some());

        let chunks = VercelTestData::streaming_chunks("Hello world");
        assert!(chunks.len() >= 3); // At least: role, content, finish

        let models = VercelTestData::default_models();
        assert!(!models.is_empty());
    }
}
