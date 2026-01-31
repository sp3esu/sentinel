//! Request types for the Native API
//!
//! Defines the chat completion request structure with strict validation.

use serde::{Deserialize, Serialize};

use super::types::Message;

/// Stop sequence - can be a single string or array of strings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StopSequence {
    /// Single stop sequence
    Single(String),
    /// Multiple stop sequences
    Multiple(Vec<String>),
}

/// Chat completion request
///
/// Uses `deny_unknown_fields` to ensure strict validation - requests with
/// unexpected fields will be rejected. This catches typos and enforces the API contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChatCompletionRequest {
    /// Model to use (optional - tier routing may override)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// Sampling temperature (0.0 to 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Nucleus sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopSequence>,
    /// Whether to stream the response
    #[serde(default)]
    pub stream: bool,
}
