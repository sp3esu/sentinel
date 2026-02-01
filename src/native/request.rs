//! Request types for the Native API
//!
//! Defines the chat completion request structure with strict validation.

use serde::{Deserialize, Serialize};

use super::types::{Message, Tier};

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
    /// Complexity tier for model routing (optional, defaults to simple)
    ///
    /// Controls which model is selected for this request:
    /// - `simple`: Fast, cost-effective models for straightforward tasks
    /// - `moderate`: Balanced models for typical assistant interactions
    /// - `complex`: Most capable models for difficult reasoning tasks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<Tier>,
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
    /// Conversation ID for session stickiness (optional)
    /// When provided, uses the provider/model from the first request in this conversation.
    /// When absent, triggers fresh provider selection each time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::types::{Content, Role};

    // =============================================================================
    // Tier Field Tests
    // =============================================================================

    #[test]
    fn test_tier_simple_deserializes() {
        let json = r#"{
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tier, Some(Tier::Simple));
        assert_eq!(request.messages.len(), 1);
    }

    #[test]
    fn test_tier_moderate_deserializes() {
        let json = r#"{
            "tier": "moderate",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tier, Some(Tier::Moderate));
    }

    #[test]
    fn test_tier_complex_deserializes() {
        let json = r#"{
            "tier": "complex",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tier, Some(Tier::Complex));
    }

    #[test]
    fn test_tier_invalid_rejected() {
        let json = r#"{
            "tier": "invalid",
            "messages": []
        }"#;
        let result: Result<ChatCompletionRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_tier_optional_defaults_to_none() {
        let json = r#"{"messages": []}"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tier, None);
    }

    #[test]
    fn test_valid_request_with_tier_deserializes() {
        let json = r#"{
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "temperature": 0.7,
            "max_tokens": 100
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tier, Some(Tier::Simple));
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, Role::User);
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(100));
        assert!(!request.stream);
    }

    #[test]
    fn test_unknown_field_rejected() {
        let json = r#"{"messages": [], "unknown_field": true}"#;
        let result: Result<ChatCompletionRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn test_model_field_rejected() {
        // model field is no longer valid - use tier instead
        let json = r#"{"model": "gpt-4", "messages": []}"#;
        let result: Result<ChatCompletionRequest, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn test_stream_defaults_to_false() {
        let json = r#"{"messages": []}"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(!request.stream);
    }

    #[test]
    fn test_stream_can_be_true() {
        let json = r#"{"messages": [], "stream": true}"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(request.stream);
    }

    #[test]
    fn test_stop_sequence_single_string() {
        let json = r#"{"messages": [], "stop": "STOP"}"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.stop, Some(StopSequence::Single("STOP".to_string())));
    }

    #[test]
    fn test_stop_sequence_array() {
        let json = r#"{"messages": [], "stop": ["STOP", "END"]}"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.stop,
            Some(StopSequence::Multiple(vec![
                "STOP".to_string(),
                "END".to_string()
            ]))
        );
    }

    #[test]
    fn test_minimal_request() {
        let json = r#"{"messages": []}"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.tier, None);
        assert!(request.messages.is_empty());
        assert_eq!(request.temperature, None);
        assert_eq!(request.max_tokens, None);
        assert_eq!(request.top_p, None);
        assert_eq!(request.stop, None);
        assert!(!request.stream);
    }

    #[test]
    fn test_request_with_all_optional_fields() {
        let request = ChatCompletionRequest {
            tier: Some(Tier::Moderate),
            messages: vec![Message {
                role: Role::User,
                content: Content::Text("Hi".to_string()),
                name: None,
                tool_call_id: None,
            }],
            temperature: Some(0.8),
            max_tokens: Some(500),
            top_p: Some(0.95),
            stop: Some(StopSequence::Single("END".to_string())),
            stream: true,
            conversation_id: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: ChatCompletionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_tier_omitted_from_serialization_when_none() {
        let request = ChatCompletionRequest {
            tier: None,
            messages: vec![],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        // tier should not appear in serialized output when None
        assert!(!json.contains("tier"));
    }

    #[test]
    fn test_tier_included_in_serialization_when_some() {
        let request = ChatCompletionRequest {
            tier: Some(Tier::Complex),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"tier\":\"complex\""));
    }

    // =============================================================================
    // Conversation ID Tests
    // =============================================================================

    #[test]
    fn test_request_without_conversation_id() {
        let json = r#"{
            "tier": "simple",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.conversation_id, None);
        assert_eq!(request.tier, Some(Tier::Simple));
    }

    #[test]
    fn test_request_with_conversation_id_deserializes() {
        let json = r#"{
            "tier": "moderate",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "conversation_id": "conv-12345"
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.conversation_id, Some("conv-12345".to_string()));
        assert_eq!(request.tier, Some(Tier::Moderate));
    }

    #[test]
    fn test_conversation_id_omitted_from_serialization_when_none() {
        let request = ChatCompletionRequest {
            tier: Some(Tier::Simple),
            messages: vec![Message {
                role: Role::User,
                content: Content::Text("Hi".to_string()),
                name: None,
                tool_call_id: None,
            }],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        // conversation_id should not appear in serialized output when None
        assert!(!json.contains("conversation_id"));
    }

    #[test]
    fn test_conversation_id_included_in_serialization_when_some() {
        let request = ChatCompletionRequest {
            tier: Some(Tier::Simple),
            messages: vec![Message {
                role: Role::User,
                content: Content::Text("Hi".to_string()),
                name: None,
                tool_call_id: None,
            }],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            stream: false,
            conversation_id: Some("conv-uuid-123".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("conversation_id"));
        assert!(json.contains("conv-uuid-123"));
    }

    #[test]
    fn test_conversation_id_with_uuid_format() {
        let json = r#"{
            "messages": [],
            "conversation_id": "550e8400-e29b-41d4-a716-446655440000"
        }"#;
        let request: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.conversation_id,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }
}
