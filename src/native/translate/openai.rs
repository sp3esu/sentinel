//! OpenAI translator implementation
//!
//! Provides bidirectional translation between Native API format and OpenAI's API format.
//! Since the Native API is designed to be OpenAI-compatible, translation is minimal.

use serde_json::json;

use super::{MessageTranslator, ToolCallIdMapping, TranslationError};
use crate::native::request::ChatCompletionRequest;
use crate::native::response::{ChatCompletionResponse, Choice, ChoiceMessage, Usage};
use crate::native::types::{
    validate_tool_name, validate_tool_schema, Message, Role, ToolCall, ToolCallFunction, ToolChoice,
};

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

/// Find function name for a tool_call_id by searching conversation history.
///
/// Searches backwards through messages for an assistant message containing
/// tool_calls with a matching tool_call_id, then extracts the function name
/// from that tool_call.
///
/// This is needed because OpenAI's tool message format requires a `name` field,
/// but our unified format doesn't store the name on tool result messages.
fn find_function_name_for_tool_call(
    messages: &[Message],
    tool_call_id: &str,
    current_index: usize,
) -> Option<String> {
    // Search backwards from current_index
    for i in (0..current_index).rev() {
        let msg = &messages[i];
        if msg.role == Role::Assistant {
            if let Some(ref tool_calls) = msg.tool_calls {
                for tc in tool_calls {
                    if tc.id == tool_call_id {
                        return Some(tc.function.name.clone());
                    }
                }
            }
        }
    }
    None
}

impl MessageTranslator for OpenAITranslator {
    fn translate_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<serde_json::Value, TranslationError> {
        // Validate message ordering
        validate_message_order(&request.messages)?;

        // Transform messages for OpenAI format
        // Most messages serialize directly, but Tool messages need function name lookup
        let mut translated_messages = Vec::with_capacity(request.messages.len());
        for (idx, msg) in request.messages.iter().enumerate() {
            if msg.role == Role::Tool {
                // Tool message needs function name from history
                let tool_call_id = msg.tool_call_id.as_ref().ok_or_else(|| {
                    TranslationError::MissingRequiredField(
                        "tool_call_id is required for tool messages".to_string(),
                    )
                })?;

                let function_name =
                    find_function_name_for_tool_call(&request.messages, tool_call_id, idx)
                        .ok_or_else(|| {
                            TranslationError::MissingToolCallInHistory(tool_call_id.clone())
                        })?;

                // Build OpenAI tool message format with name field
                let content_str = msg.content.as_text();
                translated_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "name": function_name,
                    "content": content_str
                }));
            } else {
                // Other message types serialize directly
                translated_messages.push(serde_json::to_value(msg)?);
            }
        }

        // Build the request JSON
        // Note: model is not included here - it's injected by the handler after tier routing
        let mut obj = json!({
            "messages": translated_messages,
        });

        // Add optional fields if present
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

        // Add tools if present and non-empty
        if let Some(ref tools) = request.tools {
            if !tools.is_empty() {
                // Validate each tool definition
                for tool in tools {
                    if !validate_tool_name(&tool.function.name) {
                        return Err(TranslationError::InvalidToolDefinition(format!(
                            "Invalid tool name '{}': must contain only alphanumeric characters and underscores",
                            tool.function.name
                        )));
                    }
                    if tool.function.description.is_empty() {
                        return Err(TranslationError::InvalidToolDefinition(format!(
                            "Tool '{}' has empty description",
                            tool.function.name
                        )));
                    }
                    if let Err(e) = validate_tool_schema(&tool.function.parameters) {
                        return Err(TranslationError::InvalidToolDefinition(format!(
                            "Tool '{}' has invalid schema: {}",
                            tool.function.name, e
                        )));
                    }
                }
                // Tools are already OpenAI-compatible, serialize directly
                obj["tools"] = serde_json::to_value(tools)?;
            }
        }

        // Add tool_choice if present
        if let Some(ref tool_choice) = request.tool_choice {
            let choice_value = match tool_choice {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::None => json!("none"),
                ToolChoice::Required => json!("required"),
                ToolChoice::Function { name } => json!({
                    "type": "function",
                    "function": { "name": name }
                }),
            };
            obj["tool_choice"] = choice_value;
        }

        Ok(obj)
    }

    fn translate_response(
        &self,
        response: serde_json::Value,
    ) -> Result<(ChatCompletionResponse, ToolCallIdMapping), TranslationError> {
        let mut id_mapping = ToolCallIdMapping::new();

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

            // Parse tool_calls if present
            let tool_calls = if let Some(tc_array) = message_value.get("tool_calls") {
                let tc_array = tc_array.as_array().ok_or_else(|| {
                    TranslationError::InvalidMessageFormat("tool_calls is not an array".to_string())
                })?;

                let mut calls = Vec::with_capacity(tc_array.len());
                for tc in tc_array {
                    // Extract provider's tool call ID
                    let provider_id = tc
                        .get("id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            TranslationError::MissingRequiredField("tool_call.id".to_string())
                        })?;

                    // Generate Sentinel ID and map it
                    let sentinel_id = id_mapping.generate_sentinel_id(provider_id);

                    // Extract function details
                    let function = tc.get("function").ok_or_else(|| {
                        TranslationError::MissingRequiredField("tool_call.function".to_string())
                    })?;

                    let name = function
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            TranslationError::MissingRequiredField(
                                "tool_call.function.name".to_string(),
                            )
                        })?
                        .to_string();

                    // Arguments come as a JSON string from OpenAI - parse to JSON object
                    let arguments_str = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            TranslationError::MissingRequiredField(
                                "tool_call.function.arguments".to_string(),
                            )
                        })?;

                    let arguments: serde_json::Value =
                        serde_json::from_str(arguments_str).map_err(|e| {
                            TranslationError::MalformedArguments(format!(
                                "Failed to parse arguments for tool call '{}': {}",
                                name, e
                            ))
                        })?;

                    calls.push(ToolCall {
                        id: sentinel_id,
                        call_type: "function".to_string(),
                        function: ToolCallFunction { name, arguments },
                    });
                }
                Some(calls)
            } else {
                None
            };

            let finish_reason = choice_value
                .get("finish_reason")
                .and_then(|v| v.as_str())
                .map(|s| self.translate_stop_reason(s));

            choices.push(Choice {
                index,
                message: ChoiceMessage {
                    role,
                    content,
                    tool_calls,
                },
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

        Ok((
            ChatCompletionResponse {
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
            },
            id_mapping,
        ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::types::Content;
    use serde_json::json;

    fn make_message(role: Role, content: &str) -> Message {
        Message {
            role,
            content: Content::Text(content.to_string()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    #[test]
    fn test_translate_simple_request() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::System, "You are a helpful assistant."),
                make_message(Role::User, "Hello!"),
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request).unwrap();

        // Verify messages array exists with correct roles
        let messages = result.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].get("role").unwrap(), "system");
        assert_eq!(messages[1].get("role").unwrap(), "user");
        assert_eq!(messages[0].get("content").unwrap(), "You are a helpful assistant.");
        assert_eq!(messages[1].get("content").unwrap(), "Hello!");
        // Model is not included in translated request - injected by handler
        assert!(result.get("model").is_none());
    }

    #[test]
    fn test_translate_request_with_params() {
        use crate::native::types::Tier;
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: Some(Tier::Moderate),
            messages: vec![make_message(Role::User, "Hi")],
            temperature: Some(0.7),
            max_tokens: Some(500),
            top_p: Some(0.95),
            stop: None,
            stream: true,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request).unwrap();

        // Model is not included - tier routing handles model selection
        assert!(result.get("model").is_none());
        assert_eq!(result.get("temperature").unwrap(), 0.7);
        assert_eq!(result.get("max_tokens").unwrap(), 500);
        assert_eq!(result.get("top_p").unwrap(), 0.95);
        assert_eq!(result.get("stream").unwrap(), true);
    }

    #[test]
    fn test_system_not_first_error() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::User, "Hello!"),
                make_message(Role::System, "You are an assistant."), // System AFTER user - invalid!
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request);

        assert!(matches!(result, Err(TranslationError::SystemNotFirst)));
    }

    #[test]
    fn test_translate_response() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4-0613",
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
                "prompt_tokens": 9,
                "completion_tokens": 12,
                "total_tokens": 21
            }
        });

        let (result, mapping) = translator.translate_response(response).unwrap();

        assert_eq!(result.id, "chatcmpl-123");
        assert_eq!(result.object, "chat.completion");
        assert_eq!(result.created, 1677652288);
        assert_eq!(result.model, "gpt-4-0613");
        assert_eq!(result.choices.len(), 1);
        assert_eq!(result.choices[0].index, 0);
        assert_eq!(result.choices[0].message.role, Role::Assistant);
        assert_eq!(
            result.choices[0].message.content,
            Some("Hello! How can I help you today?".to_string())
        );
        assert_eq!(result.choices[0].finish_reason, Some("stop".to_string()));
        assert_eq!(result.usage.prompt_tokens, 9);
        assert_eq!(result.usage.completion_tokens, 12);
        assert_eq!(result.usage.total_tokens, 21);
        // No tool calls, so mapping should be empty
        assert!(mapping.is_empty());
    }

    #[test]
    fn test_translate_stop_reason() {
        let translator = OpenAITranslator::new();

        // All OpenAI stop reasons should pass through unchanged
        assert_eq!(translator.translate_stop_reason("stop"), "stop");
        assert_eq!(translator.translate_stop_reason("length"), "length");
        assert_eq!(translator.translate_stop_reason("tool_calls"), "tool_calls");
        assert_eq!(
            translator.translate_stop_reason("content_filter"),
            "content_filter"
        );
        assert_eq!(
            translator.translate_stop_reason("function_call"),
            "function_call"
        );
    }

    #[test]
    fn test_empty_messages_valid() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        // Empty messages should translate without error
        // Let the provider reject if invalid
        let result = translator.translate_request(&request);
        assert!(result.is_ok());

        let json = result.unwrap();
        let messages = json.get("messages").unwrap().as_array().unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_multiple_system_messages_at_start() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::System, "First system message"),
                make_message(Role::System, "Second system message"),
                make_message(Role::User, "Hello!"),
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        // Multiple system messages at start should be valid
        let result = translator.translate_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_translate_response_missing_field_error() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-123",
            // Missing "object" field
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [],
            "usage": {
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0
            }
        });

        let result = translator.translate_response(response);
        assert!(matches!(
            result,
            Err(TranslationError::MissingRequiredField(field)) if field == "object"
        ));
    }

    #[test]
    fn test_translate_response_with_null_content() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null
                    },
                    "finish_reason": "tool_calls"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 0,
                "total_tokens": 10
            }
        });

        let (result, _mapping) = translator.translate_response(response).unwrap();
        assert_eq!(result.choices[0].message.content, None);
        assert_eq!(result.choices[0].finish_reason, Some("tool_calls".to_string()));
    }

    // =============================================================================
    // Tool Translation Tests
    // =============================================================================

    #[test]
    fn test_translate_request_with_valid_tools() {
        use crate::native::types::{FunctionDefinition, ToolDefinition};

        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "What's the weather?")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: Some(vec![ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "get_weather".to_string(),
                    description: "Get the current weather".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"]
                    }),
                },
            }]),
            tool_choice: None,
        };

        let result = translator.translate_request(&request).unwrap();
        let tools = result.get("tools").unwrap().as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn test_translate_request_with_invalid_tool_name() {
        use crate::native::types::{FunctionDefinition, ToolDefinition};

        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: Some(vec![ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "invalid-name".to_string(), // Invalid: contains hyphen
                    description: "Some description".to_string(),
                    parameters: json!({"type": "object"}),
                },
            }]),
            tool_choice: None,
        };

        let result = translator.translate_request(&request);
        assert!(matches!(
            result,
            Err(TranslationError::InvalidToolDefinition(msg)) if msg.contains("invalid-name")
        ));
    }

    #[test]
    fn test_translate_request_with_empty_tool_description() {
        use crate::native::types::{FunctionDefinition, ToolDefinition};

        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: Some(vec![ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "my_tool".to_string(),
                    description: "".to_string(), // Empty description
                    parameters: json!({"type": "object"}),
                },
            }]),
            tool_choice: None,
        };

        let result = translator.translate_request(&request);
        assert!(matches!(
            result,
            Err(TranslationError::InvalidToolDefinition(msg)) if msg.contains("empty description")
        ));
    }

    #[test]
    fn test_translate_request_with_invalid_tool_schema() {
        use crate::native::types::{FunctionDefinition, ToolDefinition};

        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: Some(vec![ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "my_tool".to_string(),
                    description: "A tool".to_string(),
                    parameters: json!({"type": "string"}), // Invalid: must be object
                },
            }]),
            tool_choice: None,
        };

        let result = translator.translate_request(&request);
        assert!(matches!(
            result,
            Err(TranslationError::InvalidToolDefinition(msg)) if msg.contains("invalid schema")
        ));
    }

    #[test]
    fn test_translate_request_with_empty_tools_array() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: Some(vec![]),
            tool_choice: None,
        };

        let result = translator.translate_request(&request).unwrap();
        // Empty tools array should not add tools key
        assert!(result.get("tools").is_none());
    }

    #[test]
    fn test_translate_request_tool_choice_auto() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: Some(ToolChoice::Auto),
        };

        let result = translator.translate_request(&request).unwrap();
        assert_eq!(result.get("tool_choice").unwrap(), "auto");
    }

    #[test]
    fn test_translate_request_tool_choice_none() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: Some(ToolChoice::None),
        };

        let result = translator.translate_request(&request).unwrap();
        assert_eq!(result.get("tool_choice").unwrap(), "none");
    }

    #[test]
    fn test_translate_request_tool_choice_required() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: Some(ToolChoice::Required),
        };

        let result = translator.translate_request(&request).unwrap();
        assert_eq!(result.get("tool_choice").unwrap(), "required");
    }

    #[test]
    fn test_translate_request_tool_choice_function() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![make_message(Role::User, "Hi")],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: Some(ToolChoice::Function {
                name: "get_weather".to_string(),
            }),
        };

        let result = translator.translate_request(&request).unwrap();
        let choice = result.get("tool_choice").unwrap();
        assert_eq!(choice["type"], "function");
        assert_eq!(choice["function"]["name"], "get_weather");
    }

    // =============================================================================
    // Tool Call Response Translation Tests
    // =============================================================================

    #[test]
    fn test_translate_response_with_tool_calls() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-456",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4-0613",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_openai_abc123",
                                "type": "function",
                                "function": {
                                    "name": "get_weather",
                                    "arguments": "{\"location\": \"London\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let (result, mapping) = translator.translate_response(response).unwrap();

        assert_eq!(result.choices[0].message.content, None);
        assert_eq!(result.choices[0].finish_reason, Some("tool_calls".to_string()));

        let tool_calls = result.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert!(tool_calls[0].id.starts_with("call_"));
        assert_eq!(tool_calls[0].call_type, "function");
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(tool_calls[0].function.arguments, json!({"location": "London"}));

        // Mapping should link Sentinel ID to provider ID
        assert!(!mapping.is_empty());
        assert_eq!(
            mapping.get_provider_id(&tool_calls[0].id),
            Some(&"call_openai_abc123".to_string())
        );
    }

    #[test]
    fn test_translate_response_with_multiple_parallel_tool_calls() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-789",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4-0613",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_provider_1",
                                "type": "function",
                                "function": {
                                    "name": "get_weather",
                                    "arguments": "{\"location\": \"London\"}"
                                }
                            },
                            {
                                "id": "call_provider_2",
                                "type": "function",
                                "function": {
                                    "name": "get_time",
                                    "arguments": "{\"timezone\": \"UTC\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ],
            "usage": {
                "prompt_tokens": 15,
                "completion_tokens": 10,
                "total_tokens": 25
            }
        });

        let (result, mapping) = translator.translate_response(response).unwrap();

        let tool_calls = result.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 2);

        // Each tool call should have unique Sentinel ID
        assert_ne!(tool_calls[0].id, tool_calls[1].id);
        assert!(tool_calls[0].id.starts_with("call_"));
        assert!(tool_calls[1].id.starts_with("call_"));

        // Verify mappings
        assert_eq!(
            mapping.get_provider_id(&tool_calls[0].id),
            Some(&"call_provider_1".to_string())
        );
        assert_eq!(
            mapping.get_provider_id(&tool_calls[1].id),
            Some(&"call_provider_2".to_string())
        );
    }

    #[test]
    fn test_translate_response_malformed_arguments() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-error",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4-0613",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_bad",
                                "type": "function",
                                "function": {
                                    "name": "some_func",
                                    "arguments": "not valid json {"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let result = translator.translate_response(response);
        assert!(matches!(
            result,
            Err(TranslationError::MalformedArguments(msg)) if msg.contains("some_func")
        ));
    }

    #[test]
    fn test_translate_response_without_tool_calls_empty_mapping() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-normal",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4-0613",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello! I'm a helpful assistant."
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 8,
                "total_tokens": 13
            }
        });

        let (result, mapping) = translator.translate_response(response).unwrap();

        assert_eq!(result.choices[0].message.content, Some("Hello! I'm a helpful assistant.".to_string()));
        assert!(result.choices[0].message.tool_calls.is_none());
        assert!(mapping.is_empty());
    }

    #[test]
    fn test_translate_response_arguments_parsed_as_json_object() {
        let translator = OpenAITranslator::new();
        let response = json!({
            "id": "chatcmpl-obj",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_xyz",
                                "type": "function",
                                "function": {
                                    "name": "search",
                                    "arguments": "{\"query\": \"rust programming\", \"limit\": 10}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let (result, _mapping) = translator.translate_response(response).unwrap();

        let tool_calls = result.choices[0].message.tool_calls.as_ref().unwrap();
        // Arguments should be parsed JSON, not a string
        assert!(tool_calls[0].function.arguments.is_object());
        assert_eq!(tool_calls[0].function.arguments["query"], "rust programming");
        assert_eq!(tool_calls[0].function.arguments["limit"], 10);
    }

    // =============================================================================
    // Tool Result Request Translation Tests
    // =============================================================================

    #[test]
    fn test_translate_request_with_tool_result_message() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::User, "What's the weather in London?"),
                Message {
                    role: Role::Assistant,
                    content: Content::Text("".to_string()),
                    name: None,
                    tool_call_id: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_abc123".to_string(),
                        call_type: "function".to_string(),
                        function: ToolCallFunction {
                            name: "get_weather".to_string(),
                            arguments: json!({"location": "London"}),
                        },
                    }]),
                },
                Message {
                    role: Role::Tool,
                    content: Content::Text("The weather in London is sunny, 22C".to_string()),
                    name: None,
                    tool_call_id: Some("call_abc123".to_string()),
                    tool_calls: None,
                },
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request).unwrap();
        let messages = result.get("messages").unwrap().as_array().unwrap();

        assert_eq!(messages.len(), 3);

        // Check the tool result message has the name field
        let tool_msg = &messages[2];
        assert_eq!(tool_msg["role"], "tool");
        assert_eq!(tool_msg["tool_call_id"], "call_abc123");
        assert_eq!(tool_msg["name"], "get_weather");
        assert_eq!(tool_msg["content"], "The weather in London is sunny, 22C");
    }

    #[test]
    fn test_translate_request_tool_result_not_found_in_history() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::User, "Hi"),
                Message {
                    role: Role::Tool,
                    content: Content::Text("Some result".to_string()),
                    name: None,
                    tool_call_id: Some("call_nonexistent".to_string()),
                    tool_calls: None,
                },
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request);
        assert!(matches!(
            result,
            Err(TranslationError::MissingToolCallInHistory(id)) if id == "call_nonexistent"
        ));
    }

    #[test]
    fn test_translate_request_tool_message_missing_tool_call_id() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::User, "Hi"),
                Message {
                    role: Role::Tool,
                    content: Content::Text("Some result".to_string()),
                    name: None,
                    tool_call_id: None, // Missing tool_call_id
                    tool_calls: None,
                },
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request);
        assert!(matches!(
            result,
            Err(TranslationError::MissingRequiredField(msg)) if msg.contains("tool_call_id")
        ));
    }

    #[test]
    fn test_translate_request_multiple_tool_results() {
        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::User, "What's the weather and time?"),
                Message {
                    role: Role::Assistant,
                    content: Content::Text("".to_string()),
                    name: None,
                    tool_call_id: None,
                    tool_calls: Some(vec![
                        ToolCall {
                            id: "call_weather".to_string(),
                            call_type: "function".to_string(),
                            function: ToolCallFunction {
                                name: "get_weather".to_string(),
                                arguments: json!({"location": "NYC"}),
                            },
                        },
                        ToolCall {
                            id: "call_time".to_string(),
                            call_type: "function".to_string(),
                            function: ToolCallFunction {
                                name: "get_time".to_string(),
                                arguments: json!({"timezone": "EST"}),
                            },
                        },
                    ]),
                },
                Message {
                    role: Role::Tool,
                    content: Content::Text("Sunny, 25C".to_string()),
                    name: None,
                    tool_call_id: Some("call_weather".to_string()),
                    tool_calls: None,
                },
                Message {
                    role: Role::Tool,
                    content: Content::Text("3:00 PM".to_string()),
                    name: None,
                    tool_call_id: Some("call_time".to_string()),
                    tool_calls: None,
                },
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request).unwrap();
        let messages = result.get("messages").unwrap().as_array().unwrap();

        assert_eq!(messages.len(), 4);

        // First tool result
        let tool_msg1 = &messages[2];
        assert_eq!(tool_msg1["name"], "get_weather");
        assert_eq!(tool_msg1["tool_call_id"], "call_weather");
        assert_eq!(tool_msg1["content"], "Sunny, 25C");

        // Second tool result
        let tool_msg2 = &messages[3];
        assert_eq!(tool_msg2["name"], "get_time");
        assert_eq!(tool_msg2["tool_call_id"], "call_time");
        assert_eq!(tool_msg2["content"], "3:00 PM");
    }

    #[test]
    fn test_translate_request_tool_result_with_json_content() {
        use crate::native::types::ContentPart;

        let translator = OpenAITranslator::new();
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![
                make_message(Role::User, "Search for something"),
                Message {
                    role: Role::Assistant,
                    content: Content::Text("".to_string()),
                    name: None,
                    tool_call_id: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_search".to_string(),
                        call_type: "function".to_string(),
                        function: ToolCallFunction {
                            name: "search".to_string(),
                            arguments: json!({"query": "test"}),
                        },
                    }]),
                },
                Message {
                    role: Role::Tool,
                    // Content parts with text (will be joined)
                    content: Content::Parts(vec![
                        ContentPart::Text { text: "Result: ".to_string() },
                        ContentPart::Text { text: "found it!".to_string() },
                    ]),
                    name: None,
                    tool_call_id: Some("call_search".to_string()),
                    tool_calls: None,
                },
            ],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
            tools: None,
            tool_choice: None,
        };

        let result = translator.translate_request(&request).unwrap();
        let messages = result.get("messages").unwrap().as_array().unwrap();

        let tool_msg = &messages[2];
        assert_eq!(tool_msg["name"], "search");
        assert_eq!(tool_msg["content"], "Result: found it!");
    }
}
