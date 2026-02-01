# Phase 5: Tool Calling - Research

**Researched:** 2026-02-01
**Domain:** OpenAI Function/Tool Calling with Unified API Translation
**Confidence:** HIGH

## Summary

This research investigates how to implement tool calling support for the Sentinel Native API, focusing on the OpenAI provider (per CONTEXT.md: "OpenAI only for v1"). The implementation requires:

1. **Request-side**: Accepting tool definitions in a unified format, validating JSON schemas, translating to OpenAI's `tools` format
2. **Response-side**: Translating OpenAI's `tool_calls` in assistant messages to a unified format with Sentinel-generated IDs
3. **Result handling**: Accepting tool results with unified format, mapping back to OpenAI's tool message format
4. **Streaming**: Accumulating tool call deltas as they arrive, with proper index tracking for parallel calls

The implementation builds on the existing `native/translate/openai.rs` translation layer and extends the `native/types.rs`, `native/request.rs`, and `native/response.rs` type definitions.

**Primary recommendation:** Use the `jsonschema` crate (v0.40.2) for JSON Schema validation, generate Sentinel tool_call_ids using UUID v4 with `call_` prefix (format: `call_{uuid}`), and store ID mappings in a per-request context for response translation.

## Standard Stack

The established libraries/tools for this domain:

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| jsonschema | 0.40.2 | JSON Schema validation | Fastest Rust validator, 75-400x faster than alternatives, Draft 2020-12 support |
| serde_json | 1.x | JSON parsing | Already in project, handles tool arguments and schema parsing |
| uuid | 1.x (v4) | ID generation | Already in project, used for trace IDs |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| thiserror | 1.x | Error handling | Already in project, for tool validation errors |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| jsonschema | valico | valico is slower, less maintained, but supports coercion |
| jsonschema | serde_valid | Compile-time validation only, not runtime schema validation |

**Installation:**
```bash
cargo add jsonschema
```

## Architecture Patterns

### Recommended Project Structure
```
src/native/
├── types.rs           # Add ToolDefinition, ToolCall, ToolCallFunction
├── request.rs         # Add tools, tool_choice fields to ChatCompletionRequest
├── response.rs        # Add tool_calls to ChoiceMessage, Delta
├── tools/
│   ├── mod.rs         # Tool module exports
│   ├── schema.rs      # Schema validation, ToolDefinition struct
│   ├── mapping.rs     # ID mapping (Sentinel <-> Provider)
│   └── accumulator.rs # Stream delta accumulation for tool calls
└── translate/
    └── openai.rs      # Extend with tool translation
```

### Pattern 1: Unified Tool Definition
**What:** Define tool schema format that closely matches OpenAI but with stricter validation
**When to use:** Request parsing, before translation
**Example:**
```rust
// Source: OpenAI API + CONTEXT.md decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Must be "function" (OpenAI format)
    #[serde(rename = "type")]
    pub tool_type: String,  // Always "function" for v1
    /// Function details
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name: alphanumeric + underscore only (a-zA-Z0-9_)
    pub name: String,
    /// Description is REQUIRED per CONTEXT.md decisions
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}
```

### Pattern 2: Tool Call ID Mapping
**What:** Generate Sentinel-specific IDs and maintain bidirectional mapping
**When to use:** Response translation and result handling
**Example:**
```rust
// Source: CONTEXT.md decision - "Generate Sentinel-specific tool_call_id"
use std::collections::HashMap;
use uuid::Uuid;

/// Maps between Sentinel and provider tool call IDs
#[derive(Debug, Default)]
pub struct ToolCallIdMapping {
    /// Sentinel ID -> Provider ID
    sentinel_to_provider: HashMap<String, String>,
    /// Provider ID -> Sentinel ID
    provider_to_sentinel: HashMap<String, String>,
}

impl ToolCallIdMapping {
    /// Generate a new Sentinel ID and map to provider ID
    pub fn generate_sentinel_id(&mut self, provider_id: &str) -> String {
        let sentinel_id = format!("call_{}", Uuid::new_v4());
        self.sentinel_to_provider.insert(sentinel_id.clone(), provider_id.to_string());
        self.provider_to_sentinel.insert(provider_id.to_string(), sentinel_id.clone());
        sentinel_id
    }

    /// Look up provider ID from Sentinel ID (for submitting results)
    pub fn get_provider_id(&self, sentinel_id: &str) -> Option<&String> {
        self.sentinel_to_provider.get(sentinel_id)
    }
}
```

### Pattern 3: Stream Delta Accumulation
**What:** Accumulate tool call deltas across streaming chunks using index tracking
**When to use:** Streaming responses with tool calls
**Example:**
```rust
// Source: OpenAI streaming format research
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    /// Index -> accumulated tool call
    tool_calls: HashMap<u32, AccumulatedToolCall>,
}

#[derive(Debug, Default)]
struct AccumulatedToolCall {
    id: Option<String>,
    function_name: String,
    arguments: String,
}

impl ToolCallAccumulator {
    /// Process a streaming delta for tool calls
    pub fn accumulate_delta(&mut self, delta: &ToolCallDelta) {
        let entry = self.tool_calls.entry(delta.index).or_default();

        if let Some(ref id) = delta.id {
            entry.id = Some(id.clone());
        }
        if let Some(ref name) = delta.function.name {
            entry.function_name = name.clone();
        }
        if let Some(ref args) = delta.function.arguments {
            entry.arguments.push_str(args);
        }
    }

    /// Finalize and validate accumulated tool calls
    pub fn finalize(self) -> Result<Vec<ToolCall>, ToolCallError> {
        // Parse accumulated arguments as JSON, return error if malformed
        // per CONTEXT.md: "Malformed arguments return error"
    }
}
```

### Pattern 4: Unified Tool Result Format
**What:** Custom format for tool results that differs from OpenAI's raw tool message
**When to use:** Handling tool result submissions
**Example:**
```rust
// Source: CONTEXT.md decision - unified result format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The Sentinel-generated tool_call_id this result corresponds to
    pub tool_call_id: String,
    /// Result content - string or JSON (we serialize to string for provider)
    pub content: ToolResultContent,
    /// Optional flag to indicate this result is an error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Json(serde_json::Value),
}

impl ToolResultContent {
    /// Convert to string for OpenAI
    pub fn to_string(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Json(v) => serde_json::to_string(v).unwrap_or_default(),
        }
    }
}
```

### Anti-Patterns to Avoid
- **Storing ID mappings globally:** Use per-request/session context instead. Global state creates race conditions.
- **Parsing arguments in streaming:** Don't parse JSON until stream completes. Intermediate chunks contain partial JSON.
- **Trusting provider IDs in results:** Always validate tool_call_id exists in mapping before sending to provider.
- **Ignoring tool_choice "required":** Treat all tool_choice values equally; don't special-case behavior.

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON Schema validation | Custom schema walker | `jsonschema` crate | Edge cases: $ref, allOf/anyOf, formats, regex patterns |
| UUID generation | Manual random IDs | `uuid::Uuid::new_v4()` | Collision resistance, proper entropy |
| JSON string accumulation | String concatenation | `String::push_str()` | Already efficient, no custom buffer needed |
| Tool name validation | Custom regex | Static compiled regex | Pattern: `^[a-zA-Z0-9_]+$` per OpenAI spec |

**Key insight:** JSON Schema validation is deceptively complex. The `jsonschema` crate handles Draft 2020-12, recursive refs, format validation, and returns structured errors. Hand-rolling would miss edge cases.

## Common Pitfalls

### Pitfall 1: Streaming Index Confusion
**What goes wrong:** Tool call deltas use `index` to identify which tool call in the parallel set is being updated. Ignoring this causes deltas to overwrite each other.
**Why it happens:** Developers assume deltas arrive in order or one tool at a time.
**How to avoid:** Always key accumulator by `index` field, not insertion order.
**Warning signs:** Tool arguments appear garbled or mixed between calls.

### Pitfall 2: Premature JSON Parsing
**What goes wrong:** Attempting to parse `arguments` as JSON before stream completes fails with parse errors.
**Why it happens:** Arguments stream incrementally as string fragments like `{"arg` then `1": "val` then `ue"}`.
**How to avoid:** Accumulate argument strings, parse only on `finish_reason: "tool_calls"`.
**Warning signs:** JSON parse errors mid-stream, missing tool calls in final result.

### Pitfall 3: ID Mapping Lifetime
**What goes wrong:** Tool call ID mapping lost between request and result submission.
**Why it happens:** Mapping stored in request handler scope, not persisted.
**How to avoid:** For multi-turn conversations, store mapping in session alongside provider/model. For stateless, include mapping hint in response.
**Warning signs:** 400 errors on tool result submission with "unknown tool_call_id".

### Pitfall 4: Missing Description Validation
**What goes wrong:** Tools without descriptions get poor LLM performance.
**Why it happens:** OpenAI makes description optional, but context says we require it.
**How to avoid:** Validate description is non-empty before accepting tool definition.
**Warning signs:** LLM fails to call appropriate tools, makes wrong selections.

### Pitfall 5: Arguments as Object vs String
**What goes wrong:** Client expects arguments as parsed JSON object, but OpenAI returns JSON string.
**Why it happens:** Per CONTEXT.md: "Arguments returned as parsed JSON object" but OpenAI sends string.
**How to avoid:** Parse the JSON string in response translation, error if malformed (per CONTEXT.md).
**Warning signs:** Client receives escaped JSON strings instead of objects.

## Code Examples

Verified patterns from official sources:

### OpenAI Tool Definition (Request)
```json
// Source: OpenAI API cookbook
{
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get current weather for a location",
        "parameters": {
          "type": "object",
          "properties": {
            "location": {
              "type": "string",
              "description": "City name"
            }
          },
          "required": ["location"]
        }
      }
    }
  ]
}
```

### OpenAI Tool Call (Response)
```json
// Source: OpenAI API cookbook
{
  "choices": [{
    "message": {
      "role": "assistant",
      "content": null,
      "tool_calls": [
        {
          "id": "call_abc123",
          "type": "function",
          "function": {
            "name": "get_weather",
            "arguments": "{\"location\": \"Boston\"}"
          }
        }
      ]
    },
    "finish_reason": "tool_calls"
  }]
}
```

### OpenAI Tool Result (Next Request)
```json
// Source: OpenAI API cookbook
{
  "role": "tool",
  "tool_call_id": "call_abc123",
  "name": "get_weather",
  "content": "{\"temperature\": 72, \"condition\": \"sunny\"}"
}
```

### OpenAI Streaming Tool Call Delta
```json
// Source: OpenAI community forum examples
// First chunk with ID and name
{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc123","type":"function","function":{"name":"get_weather","arguments":""}}]}}]}

// Subsequent chunks with argument fragments
{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"loc"}}]}}]}
{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\":"}}]}}]}
{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"Boston\"}"}}]}}]}

// Final chunk with finish_reason
{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}
```

### JSON Schema Validation in Rust
```rust
// Source: jsonschema docs.rs
use jsonschema::validator_for;
use serde_json::json;

fn validate_tool_parameters(schema: &serde_json::Value) -> Result<(), String> {
    // First, validate that the schema itself is a valid JSON Schema
    let meta_schema = json!({
        "type": "object",
        "properties": {
            "type": {"const": "object"},
            "properties": {"type": "object"},
            "required": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["type"]
    });

    let validator = validator_for(&meta_schema)
        .map_err(|e| format!("Failed to compile meta-schema: {}", e))?;

    if let Err(errors) = validator.validate(schema) {
        let error_messages: Vec<_> = errors
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        return Err(format!("Invalid schema: {}", error_messages.join(", ")));
    }

    Ok(())
}
```

### Tool Choice Translation
```rust
// Source: OpenAI API documentation + CONTEXT.md
use serde_json::{json, Value};

/// Translate tool_choice from unified format to OpenAI format
fn translate_tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Function { name } => json!({
            "type": "function",
            "function": {"name": name}
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    #[serde(rename = "function")]
    Function { name: String },
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `functions` parameter | `tools` parameter | Dec 2023 | CONTEXT.md says "tools format only" |
| `function_call` parameter | `tool_choice` parameter | Dec 2023 | Full support per CONTEXT.md |
| Single function call | Parallel tool calls | Nov 2023 | Must handle array with index |
| String arguments only | Structured outputs optional | 2024 | Skip for v1 per CONTEXT.md |

**Deprecated/outdated:**
- `functions` parameter: Replaced by `tools` array, don't support per CONTEXT.md
- `function_call` parameter: Replaced by `tool_choice`, don't support
- Strict mode: Per CONTEXT.md "No strict mode for v1"

## Open Questions

Things that couldn't be fully resolved:

1. **ID Mapping Persistence for Multi-Turn**
   - What we know: Need to map Sentinel IDs to provider IDs for result submission
   - What's unclear: Should mapping be stored in session (Redis) or returned in response?
   - Recommendation: Store in session for conversation_id requests; include in response header for stateless

2. **Parallel Tool Calls Limit**
   - What we know: OpenAI supports parallel tool calls with index tracking
   - What's unclear: Should Sentinel impose a limit on parallel calls?
   - Recommendation: No limit in v1, pass through provider's limit behavior

3. **Tool Name Validation Timing**
   - What we know: Names must match `^[a-zA-Z0-9_]+$` per OpenAI
   - What's unclear: Validate on request receipt or let provider error?
   - Recommendation: Validate early per CONTEXT.md "reject tools with invalid schemas before sending"

## Sources

### Primary (HIGH confidence)
- OpenAI API Cookbook (GitHub) - Tool calling examples and JSON formats
- jsonschema crate docs.rs - Validation API and usage patterns
- OpenAI Community Forums - Streaming tool call delta format

### Secondary (MEDIUM confidence)
- OpenAI platform documentation (accessed via WebSearch summaries) - tool_choice options

### Tertiary (LOW confidence)
- Community posts on streaming edge cases - May not reflect current API behavior

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - jsonschema is well-documented, actively maintained
- Architecture: HIGH - Follows existing codebase patterns (translate/, types.rs)
- Pitfalls: MEDIUM - Based on community reports, not direct testing

**Research date:** 2026-02-01
**Valid until:** 2026-02-15 (OpenAI API relatively stable, but streaming format may update)
