//! Response types for the Native API
//!
//! Defines chat completion response and streaming chunk structures.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::types::{Role, ToolCall};

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct Usage {
    /// Number of tokens in the prompt
    #[schema(example = 50)]
    pub prompt_tokens: u32,
    /// Number of tokens in the completion
    #[schema(example = 100)]
    pub completion_tokens: u32,
    /// Total tokens used
    #[schema(example = 150)]
    pub total_tokens: u32,
}

/// Message in a completion choice
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ChoiceMessage {
    /// Role of the message author
    pub role: Role,
    /// Content of the message
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "Hello! I'm an AI assistant. How can I help you today?")]
    pub content: Option<String>,
    /// Tool calls made by the assistant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// A completion choice
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct Choice {
    /// Index of this choice
    #[schema(example = 0)]
    pub index: u32,
    /// The generated message
    pub message: ChoiceMessage,
    /// Reason the generation stopped
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "stop")]
    pub finish_reason: Option<String>,
}

/// Chat completion response (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ChatCompletionResponse {
    /// Unique identifier for this completion
    #[schema(example = "chatcmpl-abc123")]
    pub id: String,
    /// Object type (always "chat.completion")
    #[schema(example = "chat.completion")]
    pub object: String,
    /// Unix timestamp of creation
    #[schema(example = 1677858242)]
    pub created: u64,
    /// Model used for completion
    #[schema(example = "gpt-4o-mini")]
    pub model: String,
    /// List of completion choices
    pub choices: Vec<Choice>,
    /// Token usage statistics
    pub usage: Usage,
}

/// Function call delta in streaming tool calls
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, ToSchema)]
pub struct ToolCallFunctionDelta {
    /// Function name (only in first delta for this tool call)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "get_weather")]
    pub name: Option<String>,
    /// Argument string fragment (accumulated across deltas)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "{\"location\":\"Lon")]
    pub arguments: Option<String>,
}

/// Tool call delta in streaming responses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ToolCallDelta {
    /// Index of this tool call in the parallel set
    #[schema(example = 0)]
    pub index: u32,
    /// Tool call ID (only in first delta for this index)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "call_abc123xyz")]
    pub id: Option<String>,
    /// Type of tool call (only in first delta)
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[schema(rename = "type", example = "function")]
    pub call_type: Option<String>,
    /// Function call details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolCallFunctionDelta>,
}

/// Delta content in a streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, ToSchema)]
pub struct Delta {
    /// Role (only present in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Content fragment
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "Hello")]
    pub content: Option<String>,
    /// Tool call deltas
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// A choice in a streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct StreamChoice {
    /// Index of this choice
    #[schema(example = 0)]
    pub index: u32,
    /// Delta content
    pub delta: Delta,
    /// Reason the generation stopped (only in final chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "stop")]
    pub finish_reason: Option<String>,
}

/// Streaming chunk for chat completion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct StreamChunk {
    /// Unique identifier for this completion
    #[schema(example = "chatcmpl-abc123")]
    pub id: String,
    /// Object type (always "chat.completion.chunk")
    #[schema(example = "chat.completion.chunk")]
    pub object: String,
    /// Unix timestamp of creation
    #[schema(example = 1677858242)]
    pub created: u64,
    /// Model used for completion
    #[schema(example = "gpt-4o-mini")]
    pub model: String,
    /// List of choices with delta content
    pub choices: Vec<StreamChoice>,
    /// Token usage (only in final chunk when requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::types::{ToolCall, ToolCallFunction};

    // =============================================================================
    // ChoiceMessage Tool Calls Tests
    // =============================================================================

    #[test]
    fn test_choice_message_with_tool_calls_serializes() {
        let message = ChoiceMessage {
            role: Role::Assistant,
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_abc123".to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: "get_weather".to_string(),
                    arguments: serde_json::json!({"location": "London"}),
                },
            }]),
        };
        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"tool_calls\""));
        assert!(json.contains("\"id\":\"call_abc123\""));
        assert!(json.contains("\"type\":\"function\""));
        assert!(json.contains("\"get_weather\""));
    }

    #[test]
    fn test_choice_message_without_tool_calls_omits_field() {
        let message = ChoiceMessage {
            role: Role::Assistant,
            content: Some("Hello".to_string()),
            tool_calls: None,
        };
        let json = serde_json::to_string(&message).unwrap();
        assert!(!json.contains("tool_calls"));
    }

    #[test]
    fn test_choice_message_roundtrip_with_tool_calls() {
        let message = ChoiceMessage {
            role: Role::Assistant,
            content: None,
            tool_calls: Some(vec![
                ToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction {
                        name: "func1".to_string(),
                        arguments: serde_json::json!({"a": 1}),
                    },
                },
                ToolCall {
                    id: "call_2".to_string(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction {
                        name: "func2".to_string(),
                        arguments: serde_json::json!({"b": 2}),
                    },
                },
            ]),
        };
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: ChoiceMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(message, deserialized);
    }

    // =============================================================================
    // Delta Tool Calls Tests
    // =============================================================================

    #[test]
    fn test_delta_with_tool_calls_serializes() {
        let delta = Delta {
            role: Some(Role::Assistant),
            content: None,
            tool_calls: Some(vec![ToolCallDelta {
                index: 0,
                id: Some("call_xyz".to_string()),
                call_type: Some("function".to_string()),
                function: Some(ToolCallFunctionDelta {
                    name: Some("get_weather".to_string()),
                    arguments: None,
                }),
            }]),
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("\"tool_calls\""));
        assert!(json.contains("\"index\":0"));
        assert!(json.contains("\"id\":\"call_xyz\""));
    }

    #[test]
    fn test_delta_without_tool_calls_omits_field() {
        let delta = Delta {
            role: Some(Role::Assistant),
            content: Some("Hello".to_string()),
            tool_calls: None,
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(!json.contains("tool_calls"));
    }

    // =============================================================================
    // ToolCallDelta Tests
    // =============================================================================

    #[test]
    fn test_tool_call_delta_with_partial_fields() {
        // Subsequent deltas only have index and arguments fragment
        let delta = ToolCallDelta {
            index: 0,
            id: None,
            call_type: None,
            function: Some(ToolCallFunctionDelta {
                name: None,
                arguments: Some("{\"loc".to_string()),
            }),
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("\"index\":0"));
        assert!(json.contains("\"arguments\":\"{\\\"loc\""));
        assert!(!json.contains("\"id\""));
        assert!(!json.contains("\"type\""));
        assert!(!json.contains("\"name\""));
    }

    #[test]
    fn test_tool_call_delta_first_chunk() {
        // First delta has id, type, and function name
        let delta = ToolCallDelta {
            index: 0,
            id: Some("call_abc".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("search".to_string()),
                arguments: Some("".to_string()),
            }),
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("\"id\":\"call_abc\""));
        assert!(json.contains("\"type\":\"function\""));
        assert!(json.contains("\"name\":\"search\""));
    }

    #[test]
    fn test_tool_call_delta_roundtrip() {
        let delta = ToolCallDelta {
            index: 1,
            id: Some("call_test".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("test_func".to_string()),
                arguments: Some("{\"key\": \"value\"}".to_string()),
            }),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let deserialized: ToolCallDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, deserialized);
    }
}
