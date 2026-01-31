//! OpenAI translator implementation
//!
//! Provides bidirectional translation between Native API format and OpenAI's API format.
//! Since the Native API is designed to be OpenAI-compatible, translation is minimal.

use super::{MessageTranslator, TranslationError};
use crate::native::request::ChatCompletionRequest;
use crate::native::response::ChatCompletionResponse;

/// OpenAI API translator
///
/// Translates between Native API format and OpenAI's chat completion API format.
/// Since the Native API is designed to be OpenAI-compatible, most fields pass
/// through unchanged, with validation ensuring message ordering requirements.
#[derive(Debug, Clone, Default)]
pub struct OpenAITranslator;

impl OpenAITranslator {
    /// Create a new OpenAI translator
    pub fn new() -> Self {
        Self
    }
}

impl MessageTranslator for OpenAITranslator {
    fn translate_request(
        &self,
        _request: &ChatCompletionRequest,
    ) -> Result<serde_json::Value, TranslationError> {
        // TODO: Implement in Task 2
        unimplemented!()
    }

    fn translate_response(
        &self,
        _response: serde_json::Value,
    ) -> Result<ChatCompletionResponse, TranslationError> {
        // TODO: Implement in Task 2
        unimplemented!()
    }

    fn translate_stop_reason(&self, _reason: &str) -> String {
        // TODO: Implement in Task 2
        unimplemented!()
    }
}
