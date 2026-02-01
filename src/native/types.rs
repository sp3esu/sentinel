//! Core message types for the Native API
//!
//! Defines the fundamental types used in chat completions: roles, content, and messages.

use serde::{Deserialize, Serialize};

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
    /// Tool call ID this message is responding to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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
}
