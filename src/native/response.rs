//! Response types for the Native API
//!
//! Defines chat completion response and streaming chunk structures.

use serde::{Deserialize, Serialize};

use super::types::Role;

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    /// Number of tokens in the prompt
    pub prompt_tokens: u32,
    /// Number of tokens in the completion
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

/// Message in a completion choice
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChoiceMessage {
    /// Role of the message author
    pub role: Role,
    /// Content of the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// A completion choice
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Choice {
    /// Index of this choice
    pub index: u32,
    /// The generated message
    pub message: ChoiceMessage,
    /// Reason the generation stopped
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Chat completion response (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatCompletionResponse {
    /// Unique identifier for this completion
    pub id: String,
    /// Object type (always "chat.completion")
    pub object: String,
    /// Unix timestamp of creation
    pub created: u64,
    /// Model used for completion
    pub model: String,
    /// List of completion choices
    pub choices: Vec<Choice>,
    /// Token usage statistics
    pub usage: Usage,
}

/// Delta content in a streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Delta {
    /// Role (only present in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Content fragment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// A choice in a streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamChoice {
    /// Index of this choice
    pub index: u32,
    /// Delta content
    pub delta: Delta,
    /// Reason the generation stopped (only in final chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Streaming chunk for chat completion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamChunk {
    /// Unique identifier for this completion
    pub id: String,
    /// Object type (always "chat.completion.chunk")
    pub object: String,
    /// Unix timestamp of creation
    pub created: u64,
    /// Model used for completion
    pub model: String,
    /// List of choices with delta content
    pub choices: Vec<StreamChoice>,
    /// Token usage (only in final chunk when requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}
