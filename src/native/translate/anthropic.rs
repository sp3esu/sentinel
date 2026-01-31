//! Anthropic translator implementation
//!
//! Provides bidirectional translation between Native API format and Anthropic's API format.
//! Handles Anthropic's strict message alternation requirements and system prompt extraction.
//!
//! Note: This is a scaffold for v2. The actual translation is not yet implemented,
//! but validation logic is in place to ensure type design handles Anthropic's constraints.

use super::{MessageTranslator, TranslationError};
use crate::native::request::ChatCompletionRequest;
use crate::native::response::ChatCompletionResponse;
use crate::native::types::{Message, Role};

/// Anthropic API translator
///
/// Translates between Native API format and Anthropic's messages API format.
/// Anthropic has stricter requirements than OpenAI:
/// - System messages go to a separate `system` field, not in messages array
/// - Messages must strictly alternate between user and assistant
/// - First non-system message must be from user
#[derive(Debug, Clone, Default)]
pub struct AnthropicTranslator;

impl AnthropicTranslator {
    /// Create a new Anthropic translator
    pub fn new() -> Self {
        Self
    }
}

/// Validate that messages follow Anthropic's strict alternation rules
///
/// Anthropic requires:
/// 1. At least one user message (after filtering out system messages)
/// 2. First non-system message must be from user role
/// 3. Messages must strictly alternate between user and assistant
///
/// System messages are handled separately (extracted to `system` field in Anthropic API).
pub fn validate_anthropic_alternation(messages: &[Message]) -> Result<(), TranslationError> {
    // Filter out system messages - they go to a separate field in Anthropic API
    let non_system_messages: Vec<&Message> = messages
        .iter()
        .filter(|m| m.role != Role::System)
        .collect();

    // Must have at least one user message
    if non_system_messages.is_empty() {
        return Err(TranslationError::NoUserMessage);
    }

    // First non-system message must be from user
    if non_system_messages[0].role != Role::User {
        return Err(TranslationError::FirstMustBeUser);
    }

    // Check strict alternation
    let mut expect_user = true;
    for message in &non_system_messages {
        let is_user = message.role == Role::User;

        if expect_user && !is_user {
            return Err(TranslationError::MustAlternate);
        }
        if !expect_user && is_user {
            return Err(TranslationError::MustAlternate);
        }

        // Toggle expected role (but Tool messages don't toggle)
        if message.role != Role::Tool {
            expect_user = !expect_user;
        }
    }

    Ok(())
}

/// Extract system prompt from messages
///
/// In the Native API format (OpenAI-compatible), system messages are in the messages array.
/// In Anthropic's API, the system prompt is a separate top-level field.
///
/// This function:
/// 1. Finds all system messages (which should be at the start per validation)
/// 2. Concatenates their text content
/// 3. Returns the remaining non-system messages
///
/// Note: We rely on `validate_message_order` from OpenAI translator ensuring
/// system messages appear first.
pub fn extract_system_prompt(messages: &[Message]) -> (Option<String>, Vec<&Message>) {
    let mut system_texts = Vec::new();
    let mut non_system_messages = Vec::new();

    for message in messages {
        match message.role {
            Role::System => {
                system_texts.push(message.content.as_text());
            }
            _ => {
                non_system_messages.push(message);
            }
        }
    }

    let system_prompt = if system_texts.is_empty() {
        None
    } else {
        Some(system_texts.join("\n"))
    };

    (system_prompt, non_system_messages)
}

impl MessageTranslator for AnthropicTranslator {
    fn translate_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<serde_json::Value, TranslationError> {
        // Validate Anthropic-specific requirements
        validate_anthropic_alternation(&request.messages)?;

        // Actual translation not implemented yet - this is a scaffold for v2
        Err(TranslationError::NotImplemented(
            "Anthropic request translation".to_string(),
        ))
    }

    fn translate_response(
        &self,
        _response: serde_json::Value,
    ) -> Result<ChatCompletionResponse, TranslationError> {
        // Actual translation not implemented yet - this is a scaffold for v2
        Err(TranslationError::NotImplemented(
            "Anthropic response translation".to_string(),
        ))
    }

    fn translate_stop_reason(&self, reason: &str) -> String {
        // Map Anthropic stop reasons to unified format
        // Anthropic uses: end_turn, max_tokens, stop_sequence, tool_use
        // Unified format (OpenAI-based): stop, length, tool_calls
        match reason {
            "end_turn" => "stop".to_string(),
            "max_tokens" => "length".to_string(),
            "stop_sequence" => "stop".to_string(),
            "tool_use" => "tool_calls".to_string(),
            _ => "stop".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::types::Content;

    fn make_message(role: Role, text: &str) -> Message {
        Message {
            role,
            content: Content::Text(text.to_string()),
            name: None,
            tool_call_id: None,
        }
    }

    #[test]
    fn test_alternation_valid() {
        let messages = vec![
            make_message(Role::System, "You are helpful."),
            make_message(Role::User, "Hello"),
            make_message(Role::Assistant, "Hi there!"),
            make_message(Role::User, "How are you?"),
        ];

        assert!(validate_anthropic_alternation(&messages).is_ok());
    }

    #[test]
    fn test_consecutive_user_rejected() {
        let messages = vec![
            make_message(Role::User, "Hello"),
            make_message(Role::User, "Are you there?"),
        ];

        let result = validate_anthropic_alternation(&messages);
        assert!(matches!(result, Err(TranslationError::MustAlternate)));
    }

    #[test]
    fn test_first_must_be_user() {
        let messages = vec![
            make_message(Role::Assistant, "I'm ready to help!"),
            make_message(Role::User, "Hello"),
        ];

        let result = validate_anthropic_alternation(&messages);
        assert!(matches!(result, Err(TranslationError::FirstMustBeUser)));
    }

    #[test]
    fn test_no_user_message_rejected() {
        let messages = vec![make_message(Role::System, "You are helpful.")];

        let result = validate_anthropic_alternation(&messages);
        assert!(matches!(result, Err(TranslationError::NoUserMessage)));
    }

    #[test]
    fn test_extract_system_prompt() {
        let messages = vec![
            make_message(Role::System, "You are helpful."),
            make_message(Role::User, "Hello"),
        ];

        let (system, remaining) = extract_system_prompt(&messages);

        assert_eq!(system, Some("You are helpful.".to_string()));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].role, Role::User);
    }

    #[test]
    fn test_extract_multiple_system_prompts() {
        let messages = vec![
            make_message(Role::System, "First instruction."),
            make_message(Role::System, "Second instruction."),
            make_message(Role::User, "Hello"),
        ];

        let (system, remaining) = extract_system_prompt(&messages);

        assert_eq!(
            system,
            Some("First instruction.\nSecond instruction.".to_string())
        );
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn test_extract_no_system_prompt() {
        let messages = vec![
            make_message(Role::User, "Hello"),
            make_message(Role::Assistant, "Hi!"),
        ];

        let (system, remaining) = extract_system_prompt(&messages);

        assert!(system.is_none());
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn test_stop_reason_end_turn() {
        let translator = AnthropicTranslator::new();
        assert_eq!(translator.translate_stop_reason("end_turn"), "stop");
    }

    #[test]
    fn test_stop_reason_max_tokens() {
        let translator = AnthropicTranslator::new();
        assert_eq!(translator.translate_stop_reason("max_tokens"), "length");
    }

    #[test]
    fn test_stop_reason_tool_use() {
        let translator = AnthropicTranslator::new();
        assert_eq!(translator.translate_stop_reason("tool_use"), "tool_calls");
    }

    #[test]
    fn test_stop_reason_stop_sequence() {
        let translator = AnthropicTranslator::new();
        assert_eq!(translator.translate_stop_reason("stop_sequence"), "stop");
    }

    #[test]
    fn test_stop_reason_unknown_defaults_to_stop() {
        let translator = AnthropicTranslator::new();
        assert_eq!(translator.translate_stop_reason("unknown_reason"), "stop");
    }
}
