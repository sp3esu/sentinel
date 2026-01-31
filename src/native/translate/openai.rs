//! OpenAI translator implementation
//!
//! Provides bidirectional translation between Native API format and OpenAI's API format.
//! Since the Native API is designed to be OpenAI-compatible, translation is minimal.

use serde_json::json;

use super::{MessageTranslator, TranslationError};
use crate::native::request::ChatCompletionRequest;
use crate::native::response::{ChatCompletionResponse, Choice, ChoiceMessage, Usage};
use crate::native::types::{Message, Role};

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

/// Validate that system messages appear before non-system messages
///
/// OpenAI requires system messages to be at the beginning of the messages array.
/// This function ensures that constraint is met.
fn validate_message_order(messages: &[Message]) -> Result<(), TranslationError> {
    let mut seen_non_system = false;

    for message in messages {
        match message.role {
            Role::System => {
                if seen_non_system {
                    return Err(TranslationError::SystemNotFirst);
                }
            }
            _ => {
                seen_non_system = true;
            }
        }
    }

    Ok(())
}

impl MessageTranslator for OpenAITranslator {
    fn translate_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<serde_json::Value, TranslationError> {
        // Validate message ordering
        validate_message_order(&request.messages)?;

        // Build the request JSON
        // Since Native API is OpenAI-compatible, we can serialize messages directly
        let mut obj = json!({
            "messages": request.messages,
        });

        // Add optional fields if present
        if let Some(ref model) = request.model {
            obj["model"] = json!(model);
        }

        if let Some(temperature) = request.temperature {
            obj["temperature"] = json!(temperature);
        }

        if let Some(max_tokens) = request.max_tokens {
            obj["max_tokens"] = json!(max_tokens);
        }

        if let Some(top_p) = request.top_p {
            obj["top_p"] = json!(top_p);
        }

        if let Some(ref stop) = request.stop {
            obj["stop"] = serde_json::to_value(stop)?;
        }

        if request.stream {
            obj["stream"] = json!(true);
        }

        Ok(obj)
    }

    fn translate_response(
        &self,
        response: serde_json::Value,
    ) -> Result<ChatCompletionResponse, TranslationError> {
        // Extract required fields from OpenAI response
        let id = response
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TranslationError::MissingRequiredField("id".to_string()))?
            .to_string();

        let object = response
            .get("object")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TranslationError::MissingRequiredField("object".to_string()))?
            .to_string();

        let created = response
            .get("created")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| TranslationError::MissingRequiredField("created".to_string()))?;

        let model = response
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TranslationError::MissingRequiredField("model".to_string()))?
            .to_string();

        // Parse choices
        let choices_value = response
            .get("choices")
            .ok_or_else(|| TranslationError::MissingRequiredField("choices".to_string()))?;

        let choices_array = choices_value
            .as_array()
            .ok_or_else(|| TranslationError::InvalidMessageFormat("choices is not an array".to_string()))?;

        let mut choices = Vec::with_capacity(choices_array.len());
        for choice_value in choices_array {
            let index = choice_value
                .get("index")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| TranslationError::MissingRequiredField("choice.index".to_string()))?
                as u32;

            let message_value = choice_value
                .get("message")
                .ok_or_else(|| TranslationError::MissingRequiredField("choice.message".to_string()))?;

            let role_str = message_value
                .get("role")
                .and_then(|v| v.as_str())
                .ok_or_else(|| TranslationError::MissingRequiredField("message.role".to_string()))?;

            let role = match role_str {
                "system" => Role::System,
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool" => Role::Tool,
                other => {
                    return Err(TranslationError::InvalidMessageFormat(format!(
                        "Unknown role: {}",
                        other
                    )))
                }
            };

            let content = message_value
                .get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let finish_reason = choice_value
                .get("finish_reason")
                .and_then(|v| v.as_str())
                .map(|s| self.translate_stop_reason(s));

            choices.push(Choice {
                index,
                message: ChoiceMessage { role, content },
                finish_reason,
            });
        }

        // Parse usage
        let usage_value = response
            .get("usage")
            .ok_or_else(|| TranslationError::MissingRequiredField("usage".to_string()))?;

        let prompt_tokens = usage_value
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| TranslationError::MissingRequiredField("usage.prompt_tokens".to_string()))?
            as u32;

        let completion_tokens = usage_value
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| TranslationError::MissingRequiredField("usage.completion_tokens".to_string()))?
            as u32;

        let total_tokens = usage_value
            .get("total_tokens")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| TranslationError::MissingRequiredField("usage.total_tokens".to_string()))?
            as u32;

        Ok(ChatCompletionResponse {
            id,
            object,
            created,
            model,
            choices,
            usage: Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
            },
        })
    }

    fn translate_stop_reason(&self, reason: &str) -> String {
        // OpenAI stop reasons pass through unchanged since our unified format
        // is based on OpenAI's format. Supported reasons:
        // - "stop": Reached natural stop point or stop sequence
        // - "length": Maximum token limit reached
        // - "tool_calls": Model made a tool/function call
        // - "content_filter": Content was filtered
        // - "function_call": Legacy function call (deprecated)
        reason.to_string()
    }
}
