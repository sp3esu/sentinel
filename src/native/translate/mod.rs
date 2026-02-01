//! Translation layer for converting between Native API and provider formats
//!
//! This module provides the `MessageTranslator` trait and implementations for
//! translating chat completion requests/responses between the unified Native API
//! format and provider-specific formats (OpenAI, Anthropic, etc.).

pub mod anthropic;
pub mod openai;

use thiserror::Error;

use super::request::ChatCompletionRequest;
use super::response::ChatCompletionResponse;

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
    /// the unified `ChatCompletionResponse` format.
    ///
    /// # Errors
    ///
    /// Returns `TranslationError` if:
    /// - Response format is unexpected
    /// - Required fields are missing from the response
    /// - JSON deserialization fails
    fn translate_response(
        &self,
        response: serde_json::Value,
    ) -> Result<ChatCompletionResponse, TranslationError>;

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
