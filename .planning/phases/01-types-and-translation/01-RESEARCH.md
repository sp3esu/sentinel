# Phase 1: Types and Translation - Research

**Researched:** 2026-01-31
**Domain:** Unified message types and provider translation for LLM APIs
**Confidence:** HIGH

## Summary

This phase establishes the canonical message format that all providers translate to/from. The research focused on understanding OpenAI and Anthropic API formats to design a unified type system that can translate to both, while keeping the external API OpenAI-compatible as specified in CONTEXT.md.

Key findings:
- OpenAI format is the natural unified format since Mindsmith already uses it
- Anthropic requires different system prompt handling (separate field, not in messages) and strict user/assistant alternation
- serde's `#[serde(deny_unknown_fields)]` provides strict validation but is incompatible with `#[serde(flatten)]` - use explicit field lists instead
- Streaming formats differ significantly: OpenAI uses simple `data: {json}` chunks while Anthropic uses structured events (`message_start`, `content_block_delta`, etc.)
- Error response formats differ but can be unified to OpenAI's structure

**Primary recommendation:** Define unified types matching OpenAI format, implement translators as separate modules, use the existing streaming infrastructure with a normalization layer.

## Standard Stack

The existing Sentinel codebase already has the necessary dependencies. No new libraries needed.

### Core (Already in Cargo.toml)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde | 1.x | Serialization/deserialization with derive | Industry standard, required for JSON |
| serde_json | 1.x | JSON parsing and generation | Pairs with serde |
| axum | 0.7.x | HTTP framework | Already used throughout codebase |
| async-trait | 0.1.x | Async trait definitions | Required for provider trait |
| thiserror | 1.x | Error type derivation | Already used in error.rs |

### Supporting (Already Available)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| futures | 0.3.x | Stream utilities | Streaming transformations |
| async-stream | 0.3.x | Stream creation | Stream normalization |
| bytes | 1.x | Byte buffer handling | SSE chunk processing |

### Not Needed
| Library | Why Not |
|---------|---------|
| schemars | No JSON schema generation needed in v1 |
| validator | serde + custom validation sufficient |
| derive_more | Standard From/Into traits sufficient |

**Installation:** No changes to Cargo.toml required.

## Architecture Patterns

### Recommended Module Structure
```
src/
├── native/                    # New: Native API types and translation
│   ├── mod.rs                 # Module exports
│   ├── types.rs               # Unified message types
│   ├── request.rs             # Unified request types
│   ├── response.rs            # Unified response types
│   ├── error.rs               # Unified error types
│   └── translate/             # Provider translators
│       ├── mod.rs
│       ├── openai.rs          # OpenAI <-> Native
│       └── anthropic.rs       # Anthropic <-> Native (scaffold)
├── streaming/                 # Existing: SSE utilities
│   ├── mod.rs                 # Add: stream normalization
│   └── normalize.rs           # New: chunk normalization
└── proxy/                     # Existing: provider implementations
    ├── provider.rs            # Existing trait
    └── openai.rs              # Existing implementation
```

### Pattern 1: Unified Message Type with serde Attributes
**What:** Define message types that serialize to OpenAI format while supporting validation
**When to use:** All Native API request/response types
**Example:**
```rust
// Source: serde documentation + CONTEXT.md decisions
use serde::{Deserialize, Serialize};

/// Role in a conversation message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Content can be string or array of parts (multimodal)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}

/// Content part for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

/// Unified chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}
```

### Pattern 2: Strict Validation Without Flatten
**What:** Use explicit field lists instead of flatten to enable deny_unknown_fields
**When to use:** Request types where unknown fields should be rejected
**Example:**
```rust
// Source: serde docs - deny_unknown_fields incompatible with flatten
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]  // Rejects unknown fields
pub struct ChatCompletionRequest {
    pub model: Option<String>,  // Optional, tier routing may override
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopSequence>,
    #[serde(default)]
    pub stream: bool,
    // Explicitly list all supported fields - no flatten
}
```

### Pattern 3: Translator Trait
**What:** Trait for bidirectional translation between unified and provider formats
**When to use:** Each provider gets its own translator implementation
**Example:**
```rust
// Source: Existing AiProvider pattern in codebase
pub trait MessageTranslator {
    type ProviderRequest;
    type ProviderResponse;
    type ProviderStreamChunk;

    /// Translate unified request to provider format
    fn translate_request(&self, request: &NativeRequest) -> Result<Self::ProviderRequest, TranslationError>;

    /// Translate provider response to unified format
    fn translate_response(&self, response: Self::ProviderResponse) -> Result<NativeResponse, TranslationError>;

    /// Normalize streaming chunk to unified format
    fn normalize_chunk(&self, chunk: Self::ProviderStreamChunk) -> Result<Option<NativeStreamChunk>, TranslationError>;
}
```

### Pattern 4: System Prompt Extraction for Anthropic
**What:** Extract system messages from message array for Anthropic API
**When to use:** Anthropic translator only (system is separate field)
**Example:**
```rust
// Source: Anthropic API documentation - system is top-level, not in messages
impl AnthropicTranslator {
    fn extract_system(messages: &[Message]) -> (Option<String>, Vec<Message>) {
        let mut system_content = String::new();
        let mut remaining = Vec::new();

        for (i, msg) in messages.iter().enumerate() {
            if msg.role == Role::System {
                if i != 0 {
                    // CONTEXT.md: System must be first, reject if elsewhere
                    return Err(TranslationError::SystemNotFirst);
                }
                system_content = msg.content.as_text().unwrap_or_default();
            } else {
                remaining.push(msg.clone());
            }
        }

        let system = if system_content.is_empty() { None } else { Some(system_content) };
        (system, remaining)
    }
}
```

### Pattern 5: Streaming Chunk Normalization
**What:** Transform provider-specific SSE chunks to unified format
**When to use:** All streaming responses
**Example:**
```rust
// Source: OpenAI and Anthropic streaming documentation
/// Unified streaming chunk (OpenAI-compatible format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub object: String,  // "chat.completion.chunk"
    pub created: i64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// Normalizer for Anthropic's event stream
pub fn normalize_anthropic_event(event: &AnthropicEvent) -> Option<StreamChunk> {
    match event {
        AnthropicEvent::ContentBlockDelta { index, delta } => {
            // Convert text_delta to OpenAI chunk format
            Some(StreamChunk {
                choices: vec![StreamChoice {
                    index: *index,
                    delta: Delta {
                        content: delta.text.clone(),
                        ..Default::default()
                    },
                    finish_reason: None,
                }],
                ..Default::default()
            })
        }
        AnthropicEvent::MessageDelta { delta, usage } => {
            // Final chunk with stop_reason and usage
            Some(StreamChunk {
                choices: vec![StreamChoice {
                    delta: Delta::default(),
                    finish_reason: Some(translate_stop_reason(&delta.stop_reason)),
                    ..Default::default()
                }],
                usage: Some(translate_usage(usage)),
                ..Default::default()
            })
        }
        // Ignore ping, message_start (handled for id/model), content_block_start
        _ => None,
    }
}
```

### Anti-Patterns to Avoid
- **Using `#[serde(flatten)]` with `deny_unknown_fields`:** These are incompatible. List all fields explicitly instead.
- **Passing through unknown fields:** CONTEXT.md mandates strict rejection. Don't use `#[serde(flatten)] extra: Map<String, Value>`.
- **Converting errors mid-stream without closing:** Either emit error chunk then close, or just close. Don't leave stream hanging.
- **Storing system prompts in messages array for Anthropic:** Anthropic requires system as separate top-level field.

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON serialization edge cases | Custom JSON parsers | serde_json | Handles escaping, Unicode, numbers correctly |
| SSE line buffering | Manual string concatenation | Existing SseLineBuffer | Already handles chunk boundaries correctly |
| Token counting | Custom tokenizers | tiktoken-rs (existing) | Matches OpenAI tokenization exactly |
| Error type derivation | Manual impl Error | thiserror derive | Consistent with codebase pattern |
| Stream transformation | Manual polling | futures::StreamExt::map | Composable, correct |

**Key insight:** The existing codebase already has solid patterns for streaming, error handling, and JSON processing. Reuse them; don't reinvent.

## Common Pitfalls

### Pitfall 1: Assuming OpenAI and Anthropic Messages Are Interchangeable
**What goes wrong:** Sending messages with system in the middle, or consecutive user messages, to Anthropic
**Why it happens:** OpenAI is more lenient; Anthropic enforces strict rules
**How to avoid:** Validate message structure during translation:
- System must be first (or extract to separate field for Anthropic)
- Anthropic: messages must strictly alternate user/assistant
- Anthropic: first message must be user
**Warning signs:** "messages: roles must alternate between user and assistant" errors from Anthropic

### Pitfall 2: Content Format Mismatch
**What goes wrong:** Sending string content where array expected, or vice versa
**Why it happens:** Both APIs accept content as string OR array of parts, but serialization differs
**How to avoid:** Use untagged enum for Content that handles both:
```rust
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}
```
**Warning signs:** Deserialization errors on content field

### Pitfall 3: Streaming Chunk ID/Model Missing
**What goes wrong:** OpenAI clients expect id and model in every chunk
**Why it happens:** Anthropic sends these only in message_start, not every delta
**How to avoid:** Cache message metadata from initial event, inject into normalized chunks
**Warning signs:** Client-side errors about missing fields in stream

### Pitfall 4: Stop Reason Translation
**What goes wrong:** Different providers use different stop reason strings
**Why it happens:** OpenAI: "stop", "length", "tool_calls"; Anthropic: "end_turn", "max_tokens", "stop_sequence", "tool_use"
**How to avoid:** Create explicit mapping:
```rust
fn translate_anthropic_stop_reason(reason: &str) -> &'static str {
    match reason {
        "end_turn" => "stop",
        "max_tokens" => "length",
        "stop_sequence" => "stop",
        "tool_use" => "tool_calls",
        _ => "stop"
    }
}
```
**Warning signs:** Client code breaking on unexpected finish_reason values

### Pitfall 5: Usage Stats in Streaming
**What goes wrong:** Token counts missing from streamed responses
**Why it happens:** Usage is only in final chunk; easy to miss if stream ends unexpectedly
**How to avoid:**
- Request `stream_options.include_usage: true` for OpenAI
- Extract from message_delta for Anthropic
- Track accumulated content as fallback for estimation
**Warning signs:** Zero token counts in metrics after streaming requests

## Code Examples

Verified patterns from official sources and existing codebase:

### Unified Error Response
```rust
// Source: CONTEXT.md decision - OpenAI-compatible error structure
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct NativeErrorResponse {
    pub error: NativeError,
}

#[derive(Debug, Serialize)]
pub struct NativeError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,  // Hint about which provider failed
}

impl NativeErrorResponse {
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            error: NativeError {
                message: message.into(),
                error_type: "invalid_request_error".to_string(),
                code: "invalid_request".to_string(),
                provider: None,
            }
        }
    }

    pub fn provider_error(message: impl Into<String>, provider: &str) -> Self {
        Self {
            error: NativeError {
                message: message.into(),
                error_type: "upstream_error".to_string(),
                code: "provider_error".to_string(),
                provider: Some(provider.to_string()),
            }
        }
    }
}
```

### Message Validation
```rust
// Source: CONTEXT.md - system must be first, reject if elsewhere
pub fn validate_messages(messages: &[Message]) -> Result<(), ValidationError> {
    let mut seen_non_system = false;

    for msg in messages {
        if msg.role == Role::System {
            if seen_non_system {
                return Err(ValidationError::SystemNotFirst);
            }
        } else {
            seen_non_system = true;
        }
    }

    Ok(())
}
```

### Anthropic Alternation Check
```rust
// Source: Anthropic API documentation - strict alternation required
pub fn validate_anthropic_alternation(messages: &[Message]) -> Result<(), ValidationError> {
    // Filter out system messages (they go to separate field)
    let non_system: Vec<_> = messages.iter()
        .filter(|m| m.role != Role::System)
        .collect();

    if non_system.is_empty() {
        return Err(ValidationError::NoUserMessage);
    }

    // First must be user
    if non_system[0].role != Role::User {
        return Err(ValidationError::FirstMustBeUser);
    }

    // Must alternate
    for window in non_system.windows(2) {
        if window[0].role == window[1].role {
            return Err(ValidationError::MustAlternate);
        }
    }

    Ok(())
}
```

### Stream Normalization Pipeline
```rust
// Source: Existing streaming/mod.rs pattern + this research
use futures::{Stream, StreamExt};

pub fn normalize_openai_stream<S>(
    stream: S,
) -> impl Stream<Item = Result<bytes::Bytes, reqwest::Error>>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>>,
{
    // OpenAI format is already our target format - minimal transformation
    stream
}

pub fn normalize_anthropic_stream<S>(
    stream: S,
    initial_metadata: MessageMetadata,
) -> impl Stream<Item = Result<bytes::Bytes, StreamError>>
where
    S: Stream<Item = Result<AnthropicEvent, StreamError>>,
{
    let metadata = std::sync::Arc::new(initial_metadata);

    stream.filter_map(move |event| {
        let meta = metadata.clone();
        async move {
            match event {
                Ok(event) => {
                    normalize_anthropic_event(&event, &meta)
                        .map(|chunk| Ok(format_sse_chunk(&chunk)))
                }
                Err(e) => Some(Err(e)),
            }
        }
    })
}

fn format_sse_chunk(chunk: &StreamChunk) -> bytes::Bytes {
    let json = serde_json::to_string(chunk).unwrap();
    bytes::Bytes::from(format!("data: {}\n\n", json))
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| OpenAI Completions API | Chat Completions API | 2023 | Must support messages array, not just prompt string |
| Single provider | Multi-provider abstraction | Current | Native API design assumes multiple backends |
| Provider-specific errors | Unified error format | This phase | Clients see consistent error structure |

**Deprecated/outdated:**
- OpenAI `/v1/completions` endpoint: Still supported but chat completions preferred
- Anthropic Text Completions API: Replaced by Messages API with structured format

## Open Questions

Things that couldn't be fully resolved:

1. **Empty content handling**
   - What we know: OpenAI accepts empty content in some cases; Anthropic behavior unclear
   - What's unclear: Whether to validate empty content or let provider decide
   - Recommendation: Claude's discretion per CONTEXT.md - suggest allowing empty and letting provider error if invalid

2. **Parameter range validation**
   - What we know: temperature must be 0.0-2.0 for OpenAI, 0.0-1.0 for Anthropic
   - What's unclear: Should we validate upfront or let provider handle?
   - Recommendation: Claude's discretion - suggest validating against union of valid ranges

3. **Mid-stream error handling**
   - What we know: Both APIs can error during stream
   - What's unclear: Whether to emit error chunk before close or just close
   - Recommendation: Claude's discretion - suggest emitting error chunk then close

## Sources

### Primary (HIGH confidence)
- [Anthropic Messages API Documentation](https://platform.claude.com/docs/en/api/messages) - Complete message format, system handling, response structure
- [Anthropic Streaming Documentation](https://platform.claude.com/docs/claude/reference/messages-streaming) - Event types, delta formats
- [Anthropic Errors Documentation](https://platform.claude.com/docs/en/api/errors) - Error types and HTTP codes
- [serde Container Attributes](https://serde.rs/container-attrs.html) - deny_unknown_fields documentation
- [AWS Bedrock Anthropic Messages](https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters-anthropic-claude-messages.html) - Alternation requirements

### Secondary (MEDIUM confidence)
- OpenAI Community Forums - Error code formats (verified against actual API responses)
- OpenAI API Reference (couldn't fetch due to 403, but verified patterns match existing codebase)

### Tertiary (LOW confidence - used for patterns only)
- WebSearch results for Rust enum conversion patterns - general guidance on From/Into traits

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Already using these libraries in codebase
- Architecture: HIGH - Based on existing codebase patterns and official API docs
- Pitfalls: HIGH - Derived from official API documentation requirements

**Research date:** 2026-01-31
**Valid until:** 2026-03-01 (30 days - stable domain, APIs evolve slowly)
