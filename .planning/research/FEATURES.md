# Feature Landscape: Provider-Agnostic LLM API

**Domain:** Unified LLM API abstraction layer
**Researched:** 2026-01-31
**Confidence:** HIGH (verified against official documentation)

## Universal Features

Features supported by all three providers (OpenAI, Anthropic, X/Grok). These form the core abstraction layer.

| Feature | OpenAI | Anthropic | X/Grok | Complexity | Notes |
|---------|--------|-----------|--------|------------|-------|
| **Chat/Messages endpoint** | `/v1/chat/completions` | `/v1/messages` | `/v1/chat/completions` | Low | Core functionality |
| **Message roles** | system, user, assistant | user, assistant (system separate) | system, user, assistant | Med | Anthropic uses top-level `system` parameter |
| **Text content** | `content: string` | `content: string \| ContentBlock[]` | `content: string` | Low | Anthropic supports content blocks |
| **Streaming (SSE)** | `stream: true` | `stream: true` | `stream: true` | Med | Different event schemas (see below) |
| **Temperature** | 0-2, default 1 | 0-1, default 1 | 0-2, default 1 | Low | Different ranges |
| **Max tokens** | `max_tokens` | `max_tokens` (required) | `max_tokens` / `max_completion_tokens` | Low | Anthropic requires explicit value |
| **Stop sequences** | `stop: string[]` (up to 4) | `stop_sequences: string[]` | `stop: string[]` | Low | Mostly compatible |
| **Top-p sampling** | `top_p: 0-1` | `top_p: 0-1` | `top_p: 0-1` | Low | Universal |
| **Tool/Function calling** | `tools` array | `tools` array | `tools` array | High | Schema differences (see below) |
| **Model selection** | `model: string` | `model: string` | `model: string` | Low | Universal |
| **Usage reporting** | In response `usage` | In response `usage` | In response `usage` | Low | Token counts available |

## Provider-Specific Features (Require Abstraction)

### Message Format Differences

**OpenAI:**
```json
{
  "messages": [
    {"role": "system", "content": "You are helpful"},
    {"role": "user", "content": "Hello"},
    {"role": "assistant", "content": "Hi there!"}
  ]
}
```

**Anthropic:**
```json
{
  "system": "You are helpful",
  "messages": [
    {"role": "user", "content": "Hello"},
    {"role": "assistant", "content": "Hi there!"}
  ]
}
```
- System prompt is a separate top-level parameter, NOT a message role
- No "system" role in messages array
- Content can be string OR array of content blocks
- Consecutive same-role messages are combined

**X/Grok:**
```json
{
  "messages": [
    {"role": "system", "content": "You are helpful"},
    {"role": "user", "content": "Hello"},
    {"role": "assistant", "content": "Hi there!"}
  ]
}
```
- OpenAI-compatible format

| Abstraction Need | Complexity | Recommendation |
|-----------------|------------|----------------|
| System prompt extraction | Med | Native API accepts inline; translate to Anthropic's top-level `system` |
| Message role mapping | Low | Map directly for OpenAI/Grok; filter system for Anthropic |
| Content block normalization | Med | Native API uses simple strings; translate to Anthropic content blocks if needed |

### Tool/Function Calling Schema Differences

**OpenAI Format:**
```json
{
  "tools": [{
    "type": "function",
    "function": {
      "name": "get_weather",
      "description": "Get current weather",
      "parameters": {
        "type": "object",
        "properties": {
          "location": {"type": "string"}
        },
        "required": ["location"]
      }
    }
  }]
}
```

**Anthropic Format:**
```json
{
  "tools": [{
    "name": "get_weather",
    "description": "Get current weather",
    "input_schema": {
      "type": "object",
      "properties": {
        "location": {"type": "string"}
      },
      "required": ["location"]
    }
  }]
}
```
- No `type: "function"` wrapper
- Uses `input_schema` instead of `parameters`

**X/Grok Format:**
```json
{
  "tools": [{
    "type": "function",
    "function": {
      "name": "get_weather",
      "description": "Get current weather",
      "parameters": {
        "type": "object",
        "properties": {
          "location": {"type": "string"}
        },
        "required": ["location"]
      }
    }
  }]
}
```
- OpenAI-compatible format

| Abstraction Need | Complexity | Recommendation |
|-----------------|------------|----------------|
| Tool definition schema | Med | Native API uses OpenAI format; translate for Anthropic |
| Tool call response format | Med | Normalize tool_calls from all providers |
| Tool result format | Med | Normalize tool results back to provider format |

### Streaming Event Format Differences

**OpenAI SSE Events:**
```
data: {"id":"chatcmpl-123","choices":[{"delta":{"role":"assistant"}}]}
data: {"id":"chatcmpl-123","choices":[{"delta":{"content":"Hello"}}]}
data: {"id":"chatcmpl-123","choices":[{"delta":{}}],"finish_reason":"stop"}
data: [DONE]
```
- Single event type with `delta` object
- Role in first chunk, content in subsequent
- Usage in final chunk (with `stream_options.include_usage: true`)

**Anthropic SSE Events:**
```
event: message_start
data: {"type":"message_start","message":{...}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{...}}

event: message_stop
data: {"type":"message_stop"}
```
- Multiple named event types
- Block-based content model
- Usage in `message_delta` event

**X/Grok SSE Events:**
```
data: {"id":"...","choices":[{"delta":{"content":"Hello"}}]}
data: [DONE]
```
- OpenAI-compatible format

| Abstraction Need | Complexity | Recommendation |
|-----------------|------------|----------------|
| Stream event parsing | High | Parse provider-specific; emit unified stream to client |
| Unified stream format | High | Design native stream format inspired by OpenAI (simpler) |
| Usage extraction | Med | Extract from final chunk/event for all providers |

### Provider-Specific Parameters

| Parameter | OpenAI | Anthropic | X/Grok | Abstraction |
|-----------|--------|-----------|--------|-------------|
| `presence_penalty` | 0 to 2 | Not supported | Not on Grok-4 | Pass-through or ignore |
| `frequency_penalty` | 0 to 2 | Not supported | Not on Grok-4 | Pass-through or ignore |
| `seed` | Supported | Not supported | Not documented | Pass-through or ignore |
| `n` (multiple completions) | Supported | Not supported | Limited | Reject or emulate |
| `response_format` | `json_object`, `json_schema` | `output_config.format` | Limited | High complexity |
| `reasoning_effort` | Not applicable | Not supported | Only grok-3-mini | Provider-specific |
| `service_tier` | `auto`, `default`, `flex` | Not applicable | Not documented | Provider-specific |

## Differentiators

Features that provide competitive advantage. Implement selectively.

| Feature | Value Proposition | Complexity | Provider Support | Notes |
|---------|-------------------|------------|------------------|-------|
| **Tier-based model selection** | Abstracts model choice from client | Med | Native API concept | Core differentiator for Mindsmith |
| **Session stickiness** | Consistent behavior in conversations | Med | Native API concept | Prevents mid-conversation switches |
| **Structured outputs** | Guaranteed JSON schema compliance | High | OpenAI (mature), Anthropic (new), Grok (limited) | Defer to v2 |
| **Extended thinking/reasoning** | Better answers for complex tasks | Med | Anthropic, OpenAI (o-series), Grok-3-mini | Expose as complexity tier |
| **Web search** | Real-time information | Low | Grok (native), others (via tools) | Grok-specific advantage |
| **Image input** | Vision capabilities | High | All three | Design for, implement later |
| **Server-side tool execution** | Autonomous agent behavior | High | Grok (Agent Tools API) | Grok-specific, consider for v2 |

## Anti-Features

Features to explicitly NOT expose in the Native API.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Direct model names** | Couples client to provider; blocks optimization | Use tier-based selection (simple/moderate/complex) |
| **Provider-specific parameters** | Leaky abstraction; breaks portability | Support only universal parameters |
| **Multiple completions (n > 1)** | Inconsistent support; complex token accounting | Single completion per request |
| **Silent provider failover** | Unpredictable behavior; debugging nightmare | Error bubbles up; client controls retry UX |
| **Response metadata (provider, cost)** | Client doesn't need; adds coupling | Track server-side only |
| **Embeddings endpoint** | Different use case; separate abstraction | Keep on existing `/v1/embeddings` if needed |
| **Legacy completions** | Deprecated paradigm | Chat/messages only |
| **Assistants API concepts** | OpenAI-specific; being deprecated (Aug 2026) | Stateless message history |
| **Fine-tuned model exposure** | Provider-specific | Map to tiers if used |
| **Logprobs** | Debugging feature; provider-specific | Omit from Native API |

## Feature Dependencies

```
┌─────────────────────────────────────────────────────────────┐
│                     CORE (Required)                         │
├─────────────────────────────────────────────────────────────┤
│  Message Format Normalization                               │
│         │                                                   │
│         ▼                                                   │
│  Basic Chat Completion ──────────────────────────────────── │
│         │                                                   │
│         ├──▶ Streaming ──▶ Usage Tracking                  │
│         │                                                   │
│         └──▶ Tier-Based Model Selection                    │
│                    │                                        │
│                    ▼                                        │
│              Provider Routing                               │
│                    │                                        │
│                    ▼                                        │
│              Session Stickiness                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    TOOL CALLING                             │
├─────────────────────────────────────────────────────────────┤
│  Tool Schema Translation                                    │
│         │                                                   │
│         ├──▶ Tool Call Response Parsing                    │
│         │                                                   │
│         └──▶ Tool Result Formatting                        │
│                    │                                        │
│                    ▼                                        │
│         Streaming with Tool Calls (complex)                 │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                     FUTURE (v2+)                            │
├─────────────────────────────────────────────────────────────┤
│  Structured Outputs ◄── Complex, provider differences       │
│  Vision/Images ◄── Design message format now                │
│  Extended Thinking ◄── Map to complexity tiers              │
└─────────────────────────────────────────────────────────────┘
```

## MVP Recommendation

### Phase 1: Core Chat (Required for v1)

1. **Message format normalization** - Translate between Native API format and OpenAI/Anthropic/Grok
2. **Basic chat completion** - Non-streaming first
3. **Streaming responses** - Parse provider SSE, emit unified format
4. **Tier-based model selection** - simple/moderate/complex mapping
5. **Usage tracking** - Token counting for Zion

### Phase 2: Tool Calling (Required for Mindsmith)

1. **Tool schema translation** - Native format to provider-specific
2. **Tool call parsing** - Normalize responses from all providers
3. **Tool result handling** - Format results for each provider

### Defer to Post-MVP

| Feature | Reason to Defer |
|---------|-----------------|
| Structured outputs | Complex schema differences; Anthropic just launched |
| Vision/images | Text-only for v1 per requirements |
| Extended thinking | Maps to complexity tier; transparent to client |
| Provider failover | "Errors bubble up" per design decision |
| Multiple providers wired | Only OpenAI implemented initially |

## Sources

### Official Documentation (HIGH confidence)
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [OpenAI Function Calling Guide](https://platform.openai.com/docs/guides/function-calling)
- [OpenAI Structured Outputs](https://platform.openai.com/docs/guides/structured-outputs)
- [OpenAI Streaming Events](https://platform.openai.com/docs/api-reference/responses-streaming)
- [Anthropic Messages API](https://docs.anthropic.com/en/api/messages)
- [Anthropic Streaming Messages](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [Anthropic Tool Use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/implement-tool-use)
- [Anthropic Structured Outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)
- [xAI Grok API Overview](https://docs.x.ai/docs/overview)
- [xAI Function Calling](https://docs.x.ai/docs/guides/function-calling)
- [xAI Streaming Response](https://docs.x.ai/docs/guides/streaming-response)

### Community/Comparative Sources (MEDIUM confidence)
- [OpenRouter: Unified LLM APIs Guide](https://medium.com/@milesk_33/a-practical-guide-to-openrouter-unified-llm-apis-model-routing-and-real-world-use-d3c4c07ed170)
- [Comparing LLM API Streaming Structures](https://medium.com/percolation-labs/comparing-the-streaming-response-structure-for-different-llm-apis-2b8645028b41)
- [eesel.ai: OpenAI vs Anthropic API](https://www.eesel.ai/blog/openai-api-vs-anthropic-api)
