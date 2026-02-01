//! Core message types for the Native API
//!
//! Defines the fundamental types used in chat completions: roles, content, messages,
//! and tool calling types.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Role of a message participant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message providing instructions or context
    System,
    /// User message from the human
    User,
    /// Assistant message from the AI
    Assistant,
    /// Tool/function result message
    Tool,
}

/// Image URL reference for multimodal content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageUrl {
    /// URL of the image (can be data URL or HTTP URL)
    pub url: String,
    /// Image detail level: "auto", "low", or "high"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A part of multimodal content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content
    Text {
        /// The text content
        text: String,
    },
    /// Image URL reference
    ImageUrl {
        /// The image URL details
        image_url: ImageUrl,
    },
}

/// Message content - either plain text or multimodal parts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Content {
    /// Simple text content
    Text(String),
    /// Multimodal content with text and/or images
    Parts(Vec<ContentPart>),
}

impl Content {
    /// Extract text content from either variant
    ///
    /// For `Text` variant, returns the string directly.
    /// For `Parts` variant, concatenates all text parts.
    pub fn as_text(&self) -> String {
        match self {
            Content::Text(text) => text.clone(),
            Content::Parts(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } => Some(text.as_str()),
                    ContentPart::ImageUrl { .. } => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

/// A chat message with role and content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// The role of the message author
    pub role: Role,
    /// The content of the message
    pub content: Content,
    /// Optional name of the author (for multi-user scenarios)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool call ID this message is responding to (for tool messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made by the assistant (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Complexity tier for model routing
///
/// Ordered from lowest to highest complexity for upgrade comparison.
/// Implements PartialOrd so tiers can be compared: Simple < Moderate < Complex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Simple tasks - fast, cheap models (e.g., gpt-4o-mini)
    Simple,
    /// Moderate tasks - balanced models (e.g., gpt-4o)
    Moderate,
    /// Complex tasks - most capable models (e.g., gpt-4o)
    Complex,
}

impl Default for Tier {
    fn default() -> Self {
        Tier::Simple
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Simple => write!(f, "simple"),
            Tier::Moderate => write!(f, "moderate"),
            Tier::Complex => write!(f, "complex"),
        }
    }
}

impl Tier {
    /// Check if upgrading from current tier to new tier is allowed.
    ///
    /// Upgrades: simple -> moderate -> complex allowed.
    /// Downgrades: not allowed within session.
    pub fn can_upgrade_to(&self, new_tier: &Tier) -> bool {
        new_tier >= self
    }
}

// =============================================================================
// Tool Calling Types
// =============================================================================

/// Regex pattern for validating tool names: alphanumeric and underscores only
static TOOL_NAME_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_]+$").unwrap());

/// Validate a tool name matches the required pattern (alphanumeric + underscore only)
pub fn validate_tool_name(name: &str) -> bool {
    !name.is_empty() && TOOL_NAME_PATTERN.is_match(name)
}

/// Validate a JSON Schema for tool parameters
///
/// Checks that the schema is valid JSON Schema and has type: "object"
/// as required for function parameters.
pub fn validate_tool_schema(schema: &serde_json::Value) -> Result<(), String> {
    // Check that schema has type: "object"
    match schema.get("type") {
        Some(serde_json::Value::String(t)) if t == "object" => {}
        Some(serde_json::Value::String(t)) => {
            return Err(format!(
                "Tool parameters schema must have type 'object', got '{}'",
                t
            ));
        }
        Some(_) => {
            return Err("Tool parameters schema 'type' must be a string".to_string());
        }
        None => {
            return Err("Tool parameters schema must have 'type' field".to_string());
        }
    }

    // Validate it's a valid JSON Schema by compiling it
    jsonschema::draft202012::new(schema)
        .map_err(|e| format!("Invalid JSON Schema: {}", e))?;

    Ok(())
}

/// Function definition within a tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionDefinition {
    /// Function name (validated: a-zA-Z0-9_ only)
    pub name: String,
    /// Description of what the function does (required for better LLM performance)
    pub description: String,
    /// JSON Schema defining the function parameters
    pub parameters: serde_json::Value,
}

/// Tool definition for the Native API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    /// Type of tool (always "function")
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition
    pub function: FunctionDefinition,
}

/// Function call details within a tool call
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFunction {
    /// Name of the function being called
    pub name: String,
    /// Arguments as parsed JSON object (not string, for ergonomics)
    pub arguments: serde_json::Value,
}

/// A tool call from the assistant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// Unique identifier for this tool call (format: call_{uuid})
    pub id: String,
    /// Type of tool call (always "function")
    #[serde(rename = "type")]
    pub call_type: String,
    /// Function call details
    pub function: ToolCallFunction,
}

impl ToolCall {
    /// Generate a new tool call ID
    pub fn generate_id() -> String {
        format!("call_{}", uuid::Uuid::new_v4())
    }
}

/// Content of a tool result - can be plain text or JSON
#[derive(Debug, Clone, PartialEq)]
pub enum ToolResultContent {
    /// Plain string content
    Text(String),
    /// JSON content (serialized to string for provider)
    Json(serde_json::Value),
}

impl ToolResultContent {
    /// Convert to string for provider serialization
    pub fn to_string(&self) -> String {
        match self {
            ToolResultContent::Text(s) => s.clone(),
            ToolResultContent::Json(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
        }
    }
}

impl Serialize for ToolResultContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ToolResultContent::Text(s) => serializer.serialize_str(s),
            ToolResultContent::Json(v) => v.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ToolResultContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(s) => Ok(ToolResultContent::Text(s)),
            other => Ok(ToolResultContent::Json(other)),
        }
    }
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    /// ID of the tool call this result is for
    pub tool_call_id: String,
    /// Content of the result
    pub content: ToolResultContent,
    /// Optional flag indicating this is an error result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Tool choice for controlling tool usage
#[derive(Debug, Clone, PartialEq)]
pub enum ToolChoice {
    /// Let the model decide whether to call tools
    Auto,
    /// Do not call any tools
    None,
    /// Require the model to call at least one tool
    Required,
    /// Require a specific function to be called
    Function { name: String },
}

impl Serialize for ToolChoice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ToolChoice::Auto => serializer.serialize_str("auto"),
            ToolChoice::None => serializer.serialize_str("none"),
            ToolChoice::Required => serializer.serialize_str("required"),
            ToolChoice::Function { name } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "function")?;
                map.serialize_entry("function", &serde_json::json!({ "name": name }))?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ToolChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(s) => match s.as_str() {
                "auto" => Ok(ToolChoice::Auto),
                "none" => Ok(ToolChoice::None),
                "required" => Ok(ToolChoice::Required),
                other => Err(serde::de::Error::custom(format!(
                    "unknown tool_choice value: {}",
                    other
                ))),
            },
            serde_json::Value::Object(obj) => {
                // Check for { "type": "function", "function": { "name": "..." } }
                if obj.get("type").and_then(|v| v.as_str()) == Some("function") {
                    if let Some(func) = obj.get("function") {
                        if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                            return Ok(ToolChoice::Function {
                                name: name.to_string(),
                            });
                        }
                    }
                }
                Err(serde::de::Error::custom(
                    "invalid tool_choice object format",
                ))
            }
            _ => Err(serde::de::Error::custom("tool_choice must be string or object")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_serializes_to_lowercase() {
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), "\"system\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), "\"tool\"");
    }

    #[test]
    fn test_content_text_serializes_as_string() {
        let content = Content::Text("Hello, world!".to_string());
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(json, "\"Hello, world!\"");
    }

    #[test]
    fn test_content_parts_serializes_as_array() {
        let content = Content::Parts(vec![
            ContentPart::Text {
                text: "Check this image:".to_string(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: "https://example.com/image.png".to_string(),
                    detail: Some("high".to_string()),
                },
            },
        ]);
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.starts_with('['));
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"type\":\"image_url\""));
    }

    #[test]
    fn test_message_roundtrip_serialization() {
        let message = Message {
            role: Role::User,
            content: Content::Text("Hello!".to_string()),
            name: Some("Alice".to_string()),
            tool_call_id: None,
            tool_calls: None,
        };
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(message, deserialized);
    }

    #[test]
    fn test_content_as_text_simple() {
        let content = Content::Text("Hello".to_string());
        assert_eq!(content.as_text(), "Hello");
    }

    #[test]
    fn test_content_as_text_parts() {
        let content = Content::Parts(vec![
            ContentPart::Text {
                text: "Hello ".to_string(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: "https://example.com/img.png".to_string(),
                    detail: None,
                },
            },
            ContentPart::Text {
                text: "world!".to_string(),
            },
        ]);
        assert_eq!(content.as_text(), "Hello world!");
    }

    // =============================================================================
    // Tier Tests
    // =============================================================================

    #[test]
    fn test_tier_serializes_to_snake_case() {
        assert_eq!(serde_json::to_string(&Tier::Simple).unwrap(), "\"simple\"");
        assert_eq!(
            serde_json::to_string(&Tier::Moderate).unwrap(),
            "\"moderate\""
        );
        assert_eq!(
            serde_json::to_string(&Tier::Complex).unwrap(),
            "\"complex\""
        );
    }

    #[test]
    fn test_tier_deserializes_from_snake_case() {
        let simple: Tier = serde_json::from_str("\"simple\"").unwrap();
        assert_eq!(simple, Tier::Simple);

        let moderate: Tier = serde_json::from_str("\"moderate\"").unwrap();
        assert_eq!(moderate, Tier::Moderate);

        let complex: Tier = serde_json::from_str("\"complex\"").unwrap();
        assert_eq!(complex, Tier::Complex);
    }

    #[test]
    fn test_tier_default_is_simple() {
        assert_eq!(Tier::default(), Tier::Simple);
    }

    #[test]
    fn test_tier_ordering() {
        // Simple < Moderate < Complex
        assert!(Tier::Simple < Tier::Moderate);
        assert!(Tier::Moderate < Tier::Complex);
        assert!(Tier::Simple < Tier::Complex);

        // Reverse comparisons
        assert!(Tier::Complex > Tier::Moderate);
        assert!(Tier::Moderate > Tier::Simple);
        assert!(Tier::Complex > Tier::Simple);

        // Equality
        assert!(Tier::Simple == Tier::Simple);
        assert!(Tier::Moderate == Tier::Moderate);
        assert!(Tier::Complex == Tier::Complex);
    }

    #[test]
    fn test_tier_can_upgrade_to_allows_upgrades() {
        // Same tier is allowed (no change)
        assert!(Tier::Simple.can_upgrade_to(&Tier::Simple));
        assert!(Tier::Moderate.can_upgrade_to(&Tier::Moderate));
        assert!(Tier::Complex.can_upgrade_to(&Tier::Complex));

        // Upgrades are allowed
        assert!(Tier::Simple.can_upgrade_to(&Tier::Moderate));
        assert!(Tier::Simple.can_upgrade_to(&Tier::Complex));
        assert!(Tier::Moderate.can_upgrade_to(&Tier::Complex));
    }

    #[test]
    fn test_tier_can_upgrade_to_rejects_downgrades() {
        // Downgrades are not allowed
        assert!(!Tier::Complex.can_upgrade_to(&Tier::Moderate));
        assert!(!Tier::Complex.can_upgrade_to(&Tier::Simple));
        assert!(!Tier::Moderate.can_upgrade_to(&Tier::Simple));
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(format!("{}", Tier::Simple), "simple");
        assert_eq!(format!("{}", Tier::Moderate), "moderate");
        assert_eq!(format!("{}", Tier::Complex), "complex");
    }

    #[test]
    fn test_tier_invalid_value_rejected() {
        let result: Result<Tier, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_tier_roundtrip_serialization() {
        for tier in [Tier::Simple, Tier::Moderate, Tier::Complex] {
            let json = serde_json::to_string(&tier).unwrap();
            let deserialized: Tier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, deserialized);
        }
    }

    // =============================================================================
    // Tool Name Validation Tests
    // =============================================================================

    #[test]
    fn test_validate_tool_name_valid_names() {
        assert!(validate_tool_name("get_weather"));
        assert!(validate_tool_name("searchWeb"));
        assert!(validate_tool_name("tool123"));
        assert!(validate_tool_name("UPPERCASE"));
        assert!(validate_tool_name("_underscore_start"));
        assert!(validate_tool_name("mixed_Case_123"));
    }

    #[test]
    fn test_validate_tool_name_invalid_names() {
        assert!(!validate_tool_name("")); // Empty
        assert!(!validate_tool_name("get-weather")); // Hyphen
        assert!(!validate_tool_name("get weather")); // Space
        assert!(!validate_tool_name("tool.name")); // Dot
        assert!(!validate_tool_name("func()")); // Parentheses
        assert!(!validate_tool_name("special@char")); // @ symbol
        assert!(!validate_tool_name("emoji🎉")); // Emoji
    }

    // =============================================================================
    // Tool Schema Validation Tests
    // =============================================================================

    #[test]
    fn test_validate_tool_schema_valid() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city name"
                }
            },
            "required": ["location"]
        });
        assert!(validate_tool_schema(&schema).is_ok());
    }

    #[test]
    fn test_validate_tool_schema_empty_object() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        assert!(validate_tool_schema(&schema).is_ok());
    }

    #[test]
    fn test_validate_tool_schema_missing_type() {
        let schema = serde_json::json!({
            "properties": {
                "name": { "type": "string" }
            }
        });
        let err = validate_tool_schema(&schema).unwrap_err();
        assert!(err.contains("must have 'type' field"));
    }

    #[test]
    fn test_validate_tool_schema_wrong_type() {
        let schema = serde_json::json!({
            "type": "array",
            "items": { "type": "string" }
        });
        let err = validate_tool_schema(&schema).unwrap_err();
        assert!(err.contains("must have type 'object'"));
    }

    #[test]
    fn test_validate_tool_schema_type_not_string() {
        let schema = serde_json::json!({
            "type": 123
        });
        let err = validate_tool_schema(&schema).unwrap_err();
        assert!(err.contains("must be a string"));
    }

    // =============================================================================
    // FunctionDefinition Tests
    // =============================================================================

    #[test]
    fn test_function_definition_roundtrip() {
        let func = FunctionDefinition {
            name: "get_weather".to_string(),
            description: "Get weather for a location".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": { "type": "string" }
                }
            }),
        };
        let json = serde_json::to_string(&func).unwrap();
        let deserialized: FunctionDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(func, deserialized);
    }

    // =============================================================================
    // ToolDefinition Tests
    // =============================================================================

    #[test]
    fn test_tool_definition_roundtrip() {
        let tool = ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_web".to_string(),
                description: "Search the web".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    }
                }),
            },
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"type\":\"function\""));
        let deserialized: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(tool, deserialized);
    }

    // =============================================================================
    // ToolCall Tests
    // =============================================================================

    #[test]
    fn test_tool_call_roundtrip() {
        let call = ToolCall {
            id: "call_abc123".to_string(),
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name: "get_weather".to_string(),
                arguments: serde_json::json!({ "location": "London" }),
            },
        };
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("\"type\":\"function\""));
        let deserialized: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(call, deserialized);
    }

    #[test]
    fn test_tool_call_generate_id_format() {
        let id = ToolCall::generate_id();
        assert!(id.starts_with("call_"));
        assert!(id.len() > 5); // "call_" + uuid
    }

    // =============================================================================
    // ToolResultContent Tests
    // =============================================================================

    #[test]
    fn test_tool_result_content_text_serialization() {
        let content = ToolResultContent::Text("Weather is sunny".to_string());
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(json, "\"Weather is sunny\"");
    }

    #[test]
    fn test_tool_result_content_json_serialization() {
        let content = ToolResultContent::Json(serde_json::json!({
            "temperature": 72,
            "condition": "sunny"
        }));
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"temperature\":72"));
    }

    #[test]
    fn test_tool_result_content_text_deserialization() {
        let content: ToolResultContent = serde_json::from_str("\"Hello world\"").unwrap();
        assert_eq!(content, ToolResultContent::Text("Hello world".to_string()));
    }

    #[test]
    fn test_tool_result_content_json_deserialization() {
        let content: ToolResultContent = serde_json::from_str(r#"{"key": "value"}"#).unwrap();
        match content {
            ToolResultContent::Json(v) => {
                assert_eq!(v.get("key").unwrap(), "value");
            }
            _ => panic!("Expected Json variant"),
        }
    }

    #[test]
    fn test_tool_result_content_to_string() {
        let text = ToolResultContent::Text("plain text".to_string());
        assert_eq!(text.to_string(), "plain text");

        let json = ToolResultContent::Json(serde_json::json!({"a": 1}));
        assert_eq!(json.to_string(), r#"{"a":1}"#);
    }

    // =============================================================================
    // ToolResult Tests
    // =============================================================================

    #[test]
    fn test_tool_result_roundtrip() {
        let result = ToolResult {
            tool_call_id: "call_123".to_string(),
            content: ToolResultContent::Text("Success".to_string()),
            is_error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }

    #[test]
    fn test_tool_result_with_error_flag() {
        let result = ToolResult {
            tool_call_id: "call_456".to_string(),
            content: ToolResultContent::Text("Error occurred".to_string()),
            is_error: Some(true),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"is_error\":true"));
    }

    #[test]
    fn test_tool_result_error_flag_omitted_when_none() {
        let result = ToolResult {
            tool_call_id: "call_789".to_string(),
            content: ToolResultContent::Text("OK".to_string()),
            is_error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("is_error"));
    }

    // =============================================================================
    // ToolChoice Tests
    // =============================================================================

    #[test]
    fn test_tool_choice_auto_serialization() {
        let choice = ToolChoice::Auto;
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(json, "\"auto\"");
    }

    #[test]
    fn test_tool_choice_none_serialization() {
        let choice = ToolChoice::None;
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(json, "\"none\"");
    }

    #[test]
    fn test_tool_choice_required_serialization() {
        let choice = ToolChoice::Required;
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(json, "\"required\"");
    }

    #[test]
    fn test_tool_choice_function_serialization() {
        let choice = ToolChoice::Function {
            name: "get_weather".to_string(),
        };
        let json = serde_json::to_string(&choice).unwrap();
        assert!(json.contains("\"type\":\"function\""));
        assert!(json.contains("\"function\":{\"name\":\"get_weather\"}"));
    }

    #[test]
    fn test_tool_choice_auto_deserialization() {
        let choice: ToolChoice = serde_json::from_str("\"auto\"").unwrap();
        assert_eq!(choice, ToolChoice::Auto);
    }

    #[test]
    fn test_tool_choice_none_deserialization() {
        let choice: ToolChoice = serde_json::from_str("\"none\"").unwrap();
        assert_eq!(choice, ToolChoice::None);
    }

    #[test]
    fn test_tool_choice_required_deserialization() {
        let choice: ToolChoice = serde_json::from_str("\"required\"").unwrap();
        assert_eq!(choice, ToolChoice::Required);
    }

    #[test]
    fn test_tool_choice_function_deserialization() {
        let json = r#"{"type": "function", "function": {"name": "search"}}"#;
        let choice: ToolChoice = serde_json::from_str(json).unwrap();
        assert_eq!(
            choice,
            ToolChoice::Function {
                name: "search".to_string()
            }
        );
    }

    #[test]
    fn test_tool_choice_invalid_string_rejected() {
        let result: Result<ToolChoice, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_choice_roundtrip() {
        let choices = vec![
            ToolChoice::Auto,
            ToolChoice::None,
            ToolChoice::Required,
            ToolChoice::Function {
                name: "test_func".to_string(),
            },
        ];
        for choice in choices {
            let json = serde_json::to_string(&choice).unwrap();
            let deserialized: ToolChoice = serde_json::from_str(&json).unwrap();
            assert_eq!(choice, deserialized);
        }
    }
}
