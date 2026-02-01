//! Translation layer for converting between Native API and provider formats
//!
//! This module provides the `MessageTranslator` trait and implementations for
//! translating chat completion requests/responses between the unified Native API
//! format and provider-specific formats (OpenAI, Anthropic, etc.).

pub mod anthropic;
pub mod openai;

use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

use super::request::ChatCompletionRequest;
use super::response::ChatCompletionResponse;

/// Maps between Sentinel and provider tool call IDs
///
/// When translating tool call responses, we generate Sentinel-specific IDs
/// (format: `call_{uuid}`) and maintain a bidirectional mapping to the
/// provider's original IDs. This enables:
/// - Consistent ID format across providers
/// - Tool result submission using Sentinel IDs
/// - Translation back to provider IDs when sending tool results
#[derive(Debug, Default, Clone)]
pub struct ToolCallIdMapping {
    /// Sentinel ID -> Provider ID
    sentinel_to_provider: HashMap<String, String>,
    /// Provider ID -> Sentinel ID
    provider_to_sentinel: HashMap<String, String>,
}

impl ToolCallIdMapping {
    /// Create a new empty mapping
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a Sentinel ID and map it to a provider ID
    ///
    /// Creates a new Sentinel-format tool call ID (`call_{uuid}`) and
    /// establishes bidirectional mapping with the provider's ID.
    pub fn generate_sentinel_id(&mut self, provider_id: &str) -> String {
        let sentinel_id = format!("call_{}", Uuid::new_v4());
        self.sentinel_to_provider
            .insert(sentinel_id.clone(), provider_id.to_string());
        self.provider_to_sentinel
            .insert(provider_id.to_string(), sentinel_id.clone());
        sentinel_id
    }

    /// Get provider ID from Sentinel ID (for tool result submission)
    pub fn get_provider_id(&self, sentinel_id: &str) -> Option<&String> {
        self.sentinel_to_provider.get(sentinel_id)
    }

    /// Get Sentinel ID from provider ID (for response translation)
    pub fn get_sentinel_id(&self, provider_id: &str) -> Option<&String> {
        self.provider_to_sentinel.get(provider_id)
    }

    /// Check if the mapping is empty
    pub fn is_empty(&self) -> bool {
        self.sentinel_to_provider.is_empty()
    }
}

/// Errors that can occur during message translation
#[derive(Debug, Error)]
pub enum TranslationError {
    /// Message format is invalid for the target provider
    #[error("Invalid message format: {0}")]
    InvalidMessageFormat(String),

    /// System message must be first in the message array
    #[error("System messages must appear before any non-system messages")]
    SystemNotFirst,

    /// A required field is missing from the input
    #[error("Missing required field: {0}")]
    MissingRequiredField(String),

    /// JSON serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// No user message in the conversation (required by Anthropic)
    #[error("Conversation must contain at least one user message")]
    NoUserMessage,

    /// First non-system message must be from user (required by Anthropic)
    #[error("First non-system message must be from user role")]
    FirstMustBeUser,

    /// Messages must alternate between user and assistant (required by Anthropic)
    #[error("Messages must alternate between user and assistant roles")]
    MustAlternate,

    /// Feature not yet implemented (used for scaffold methods)
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Invalid tool definition
    #[error("Invalid tool definition: {0}")]
    InvalidToolDefinition(String),

    /// Malformed arguments in tool call response
    #[error("Malformed tool call arguments: {0}")]
    MalformedArguments(String),

    /// Tool call ID not found in conversation history
    #[error("No tool call found in history for tool_call_id: {0}")]
    MissingToolCallInHistory(String),
}

/// Trait for translating between Native API format and provider-specific formats
///
/// Implementations of this trait handle the bidirectional conversion between
/// the unified Native API types and provider-specific JSON formats. This enables
/// Sentinel to communicate with different AI providers using a single internal
/// representation.
pub trait MessageTranslator {
    /// Translate unified request to provider-specific JSON
    ///
    /// Takes a `ChatCompletionRequest` in Native API format and converts it
    /// to the JSON format expected by the target provider's API.
    ///
    /// # Errors
    ///
    /// Returns `TranslationError` if:
    /// - Message format is invalid for the target provider
    /// - System messages are not positioned correctly
    /// - Required fields are missing
    /// - JSON serialization fails
    fn translate_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<serde_json::Value, TranslationError>;

    /// Translate provider response JSON to unified format
    ///
    /// Takes a JSON response from the provider's API and converts it to
    /// the unified `ChatCompletionResponse` format, along with a mapping
    /// of tool call IDs if the response contains tool calls.
    ///
    /// # Returns
    ///
    /// A tuple of (response, id_mapping) where:
    /// - `response` is the translated `ChatCompletionResponse`
    /// - `id_mapping` contains Sentinel->Provider ID mappings for any tool calls
    ///
    /// # Errors
    ///
    /// Returns `TranslationError` if:
    /// - Response format is unexpected
    /// - Required fields are missing from the response
    /// - JSON deserialization fails
    /// - Tool call arguments are malformed JSON
    fn translate_response(
        &self,
        response: serde_json::Value,
    ) -> Result<(ChatCompletionResponse, ToolCallIdMapping), TranslationError>;

    /// Map provider stop reason to unified format
    ///
    /// Converts a provider-specific finish reason string to the unified format
    /// used in Native API responses. This enables consistent handling of
    /// completion reasons across different providers.
    fn translate_stop_reason(&self, reason: &str) -> String;
}

// Re-export key types for convenience
pub use anthropic::AnthropicTranslator;
pub use openai::OpenAITranslator;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_id_mapping_generate() {
        let mut mapping = ToolCallIdMapping::new();
        let sentinel_id = mapping.generate_sentinel_id("call_openai_123");

        assert!(sentinel_id.starts_with("call_"));
        assert_eq!(
            mapping.get_provider_id(&sentinel_id),
            Some(&"call_openai_123".to_string())
        );
        assert_eq!(
            mapping.get_sentinel_id("call_openai_123"),
            Some(&sentinel_id)
        );
    }

    #[test]
    fn test_tool_call_id_mapping_empty() {
        let mapping = ToolCallIdMapping::new();
        assert!(mapping.is_empty());
    }

    #[test]
    fn test_tool_call_id_mapping_multiple() {
        let mut mapping = ToolCallIdMapping::new();
        let id1 = mapping.generate_sentinel_id("provider_1");
        let id2 = mapping.generate_sentinel_id("provider_2");

        assert_ne!(id1, id2);
        assert!(!mapping.is_empty());
        assert_eq!(mapping.get_provider_id(&id1), Some(&"provider_1".to_string()));
        assert_eq!(mapping.get_provider_id(&id2), Some(&"provider_2".to_string()));
    }

    #[test]
    fn test_tool_call_id_mapping_not_found() {
        let mapping = ToolCallIdMapping::new();
        assert!(mapping.get_provider_id("nonexistent").is_none());
        assert!(mapping.get_sentinel_id("nonexistent").is_none());
    }
}
