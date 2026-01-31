# Architecture Patterns: Provider-Agnostic LLM API

**Domain:** Provider-agnostic LLM API gateway
**Researched:** 2026-01-31
**Confidence:** HIGH (verified with official documentation and industry patterns)

## Executive Summary

This document outlines the architecture for Sentinel's new `/native/*` API layer. The design follows established LLM gateway patterns from industry leaders (LiteLLM, TensorZero, OpenRouter) while leveraging Sentinel's existing Rust/Axum infrastructure and `AiProvider` trait abstraction.

The architecture comprises five core components:

1. **Unified Request/Response Format** - Provider-agnostic message and tool schemas
2. **Request Translator** - Bidirectional translation between unified and provider formats
3. **Session Manager** - Conversation tracking for provider stickiness
4. **Tier Router** - Model selection based on complexity tier
5. **Provider Adapters** - Extended `AiProvider` implementations for each backend

## Recommended Architecture

```
                                    Sentinel Native API
                                           |
    +--------------------------------------+--------------------------------------+
    |                                      |                                      |
    v                                      v                                      v
POST /native/chat              GET /native/models               GET /native/docs
    |                                      |                                      |
    v                                      v                                      v
+-------------------+            +-------------------+            +-------------------+
| Auth Middleware   |            | Auth Middleware   |            | API Key Auth      |
| (existing)        |            | (existing)        |            | (new, simple)     |
+-------------------+            +-------------------+            +-------------------+
    |                                      |                                      |
    v                                      v                                      v
+-------------------+            +-------------------+            +-------------------+
| Rate Limiter      |            | Route Handler     |            | OpenAPI Spec      |
| (existing)        |            +-------------------+            | (static YAML)     |
+-------------------+                                             +-------------------+
    |
    v
+-------------------+
| Session Manager   |  <-- Lookup/create session, determine provider
+-------------------+
    |
    v
+-------------------+
| Tier Router       |  <-- Select model based on tier + provider
+-------------------+
    |
    v
+-------------------+
| Request Translator|  <-- Unified -> Provider-specific format
+-------------------+
    |
    v
+-------------------+
| AiProvider        |  <-- OpenAI, Anthropic, etc. (via trait)
+-------------------+
    |
    v
+-------------------+
| Response Translator| <-- Provider-specific -> Unified format
+-------------------+
    |
    v
Client (streaming or non-streaming response)
```

## Component Boundaries

### Component 1: Unified Request/Response Types

**Location:** `src/native/types.rs`

**Responsibility:** Define provider-agnostic data structures for messages, tools, and responses.

**Communicates With:**
- Route handlers (receives parsed requests)
- Request Translator (provides source format)
- Response Translator (provides target format)

**Key Types:**

```rust
// Unified message format
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

pub struct Message {
    pub role: MessageRole,
    pub content: MessageContent,
    pub name: Option<String>,           // For tool results
    pub tool_call_id: Option<String>,   // Links tool result to call
    pub tool_calls: Option<Vec<ToolCall>>, // Assistant's tool invocations
}

pub enum MessageContent {
    Text(String),
    // Future: Image, Audio, etc.
    Parts(Vec<ContentPart>),
}

pub struct ContentPart {
    pub content_type: ContentType,
    pub data: String, // Text or base64 for binary
}

// Unified tool definition (translatable to OpenAI/Anthropic)
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: JsonSchema,  // JSON Schema object
}

pub struct JsonSchema {
    pub schema_type: String,     // "object"
    pub properties: serde_json::Value,
    pub required: Vec<String>,
}

// Unified tool call (from assistant response)
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

// Request types
pub struct NativeChatRequest {
    pub session_id: Option<String>,  // For stickiness
    pub tier: Tier,                  // simple | moderate | complex
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
    pub tool_choice: Option<ToolChoice>,
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

pub enum Tier {
    Simple,    // Fast, cheap models (GPT-4o-mini, Claude Haiku)
    Moderate,  // Balanced models (GPT-4o, Claude Sonnet)
    Complex,   // Most capable (GPT-4-turbo, Claude Opus)
}

pub enum ToolChoice {
    Auto,
    None,
    Required,
    Specific(String), // Force specific tool
}

// Response types
pub struct NativeChatResponse {
    pub id: String,
    pub session_id: String,
    pub message: Message,
    pub finish_reason: FinishReason,
}

pub enum FinishReason {
    Stop,
    ToolUse,
    Length,
    ContentFilter,
}
```

**Design Rationale:**
- Message format is a superset of OpenAI and Anthropic
- Tool schema uses JSON Schema (common denominator)
- Session ID enables provider stickiness
- Tier abstracts model selection from client

---

### Component 2: Request Translator

**Location:** `src/native/translator/mod.rs` with submodules per provider

**Responsibility:** Bidirectional translation between unified format and provider-specific formats.

**Communicates With:**
- Route handlers (receives unified request)
- AiProvider implementations (provides translated request)
- Response Translator (inverse direction)

**Structure:**

```rust
// src/native/translator/mod.rs
pub mod openai;
pub mod anthropic;  // Future

pub trait RequestTranslator: Send + Sync {
    /// Translate unified request to provider-specific JSON
    fn translate_request(
        &self,
        request: &NativeChatRequest,
        model: &str,
    ) -> Result<serde_json::Value, TranslationError>;

    /// Translate provider response to unified format
    fn translate_response(
        &self,
        response: serde_json::Value,
        session_id: &str,
    ) -> Result<NativeChatResponse, TranslationError>;

    /// Translate streaming chunk to unified format
    fn translate_stream_chunk(
        &self,
        chunk: &str,
    ) -> Result<Option<StreamChunk>, TranslationError>;
}
```

**OpenAI Translation (src/native/translator/openai.rs):**

| Unified | OpenAI |
|---------|--------|
| `Message.role: System` | `role: "system"` |
| `Message.role: User` | `role: "user"` |
| `Message.role: Assistant` | `role: "assistant"` |
| `Message.role: Tool` | `role: "tool"` |
| `Tool.parameters` | `function.parameters` (wrapped in `tools[]`) |
| `ToolCall` | `tool_calls[].function` |

**Anthropic Translation (src/native/translator/anthropic.rs) - Future:**

| Unified | Anthropic |
|---------|-----------|
| `Message.role: System` | First message in `system` parameter (hoisted) |
| `Message.role: User` | `role: "user"` |
| `Message.role: Assistant` | `role: "assistant"` |
| `Message.role: Tool` | `role: "user"` with `tool_result` content block |
| `Tool.parameters` | `input_schema` (direct, no wrapper) |
| `ToolCall` | `content[type: "tool_use"]` |

**Tool Schema Translation Details:**

OpenAI format:
```json
{
  "type": "function",
  "function": {
    "name": "get_weather",
    "description": "Get weather for a location",
    "parameters": {
      "type": "object",
      "properties": { "location": { "type": "string" } },
      "required": ["location"]
    }
  }
}
```

Anthropic format:
```json
{
  "name": "get_weather",
  "description": "Get weather for a location",
  "input_schema": {
    "type": "object",
    "properties": { "location": { "type": "string" } },
    "required": ["location"]
  }
}
```

**Streaming Translation:**
- OpenAI: SSE with `data: {...}` lines, `choices[0].delta.content`
- Anthropic: SSE with `event: content_block_delta`, `delta.text`
- Both converted to unified `StreamChunk { content: String, tool_calls: Option<Vec<ToolCall>> }`

---

### Component 3: Session Manager

**Location:** `src/native/session.rs`

**Responsibility:** Track conversations to ensure provider stickiness within a session.

**Communicates With:**
- Route handlers (provides/creates session)
- Redis (persistent session storage)
- Tier Router (session determines provider)

**Design:**

```rust
pub struct Session {
    pub id: String,
    pub provider: String,       // "openai", "anthropic", etc.
    pub model: String,          // Specific model used
    pub tier: Tier,
    pub created_at: i64,
    pub last_used: i64,
    pub external_id: String,    // User identifier for cleanup
}

pub struct SessionManager {
    redis: redis::aio::ConnectionManager,
    session_ttl: u64,  // e.g., 24 hours
}

impl SessionManager {
    /// Get or create session for a conversation
    pub async fn get_or_create(
        &self,
        session_id: Option<&str>,
        tier: Tier,
        external_id: &str,
    ) -> Result<Session, AppError>;

    /// Update last_used timestamp
    pub async fn touch(&self, session_id: &str) -> Result<(), AppError>;

    /// Explicit cleanup (user ends conversation)
    pub async fn end(&self, session_id: &str) -> Result<(), AppError>;
}
```

**Stickiness Logic:**

1. **New Conversation (no session_id):**
   - Tier Router selects provider + model
   - Create session with `provider` and `model` locked in
   - Return session_id to client

2. **Continuing Conversation (session_id provided):**
   - Lookup session from Redis
   - Use stored provider + model (ignore current tier if different)
   - Touch session to extend TTL

3. **Session Expiry:**
   - Sessions expire after TTL (e.g., 24 hours)
   - Client can start new conversation if session expired
   - Expired session ID treated as new conversation

**Redis Key Structure:**
- Key: `session:{session_id}`
- Value: JSON-serialized Session
- TTL: Configurable (default 86400 seconds = 24h)

---

### Component 4: Tier Router

**Location:** `src/native/router.rs`

**Responsibility:** Select provider and model based on tier, availability, and configuration.

**Communicates With:**
- Session Manager (provides routing decision for new sessions)
- Zion API (fetches model configuration)
- Redis (caches model configuration)

**Design:**

```rust
pub struct TierConfig {
    pub simple: Vec<ModelOption>,
    pub moderate: Vec<ModelOption>,
    pub complex: Vec<ModelOption>,
}

pub struct ModelOption {
    pub provider: String,     // "openai", "anthropic"
    pub model: String,        // "gpt-4o-mini", "claude-3-haiku"
    pub priority: u8,         // Lower = preferred
    pub enabled: bool,
}

pub struct TierRouter {
    config_cache: Arc<RwLock<TierConfig>>,
    zion_client: Arc<ZionClient>,
    redis: redis::aio::ConnectionManager,
    fallback_config: TierConfig,  // Hardcoded fallback
}

impl TierRouter {
    /// Select provider and model for a tier
    pub async fn route(&self, tier: Tier) -> Result<(String, String), AppError>;

    /// Refresh configuration from Zion
    pub async fn refresh_config(&self) -> Result<(), AppError>;
}
```

**Configuration Source Priority:**

1. **Zion API** (primary): `GET /api/v1/config/models`
   - Cached in Redis with 5-minute TTL
   - Allows runtime configuration changes

2. **Fallback Config** (if Zion unavailable):
   ```rust
   TierConfig {
       simple: vec![
           ModelOption { provider: "openai", model: "gpt-4o-mini", priority: 1, enabled: true },
       ],
       moderate: vec![
           ModelOption { provider: "openai", model: "gpt-4o", priority: 1, enabled: true },
       ],
       complex: vec![
           ModelOption { provider: "openai", model: "gpt-4-turbo", priority: 1, enabled: true },
       ],
   }
   ```

**Routing Algorithm:**
1. Get enabled models for tier
2. Sort by priority (ascending)
3. Return first available option
4. If none available, return error (no silent failover)

---

### Component 5: Provider Registry

**Location:** `src/native/providers/mod.rs`

**Responsibility:** Manage provider instances and dispatch translated requests.

**Communicates With:**
- Tier Router (receives routing decision)
- Request Translator (for format conversion)
- Existing AiProvider trait (for actual API calls)

**Design:**

```rust
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn AiProvider>>,
    translators: HashMap<String, Arc<dyn RequestTranslator>>,
}

impl ProviderRegistry {
    pub fn new(config: &Config) -> Self {
        let mut providers = HashMap::new();
        let mut translators = HashMap::new();

        // OpenAI (always available if configured)
        if let Some(api_key) = &config.openai_api_key {
            let client = reqwest::Client::new();
            providers.insert(
                "openai".to_string(),
                Arc::new(OpenAIProvider::new(client, config)) as Arc<dyn AiProvider>
            );
            translators.insert(
                "openai".to_string(),
                Arc::new(OpenAITranslator::new()) as Arc<dyn RequestTranslator>
            );
        }

        // Anthropic (future)
        // if let Some(api_key) = &config.anthropic_api_key { ... }

        Self { providers, translators }
    }

    pub fn get(&self, provider: &str) -> Option<(Arc<dyn AiProvider>, Arc<dyn RequestTranslator>)>;

    pub fn available_providers(&self) -> Vec<&str>;
}
```

---

## Data Flow

### Non-Streaming Request Flow

```
1. POST /native/chat arrives
   |
2. Auth middleware validates JWT (existing)
   |
3. Rate limiter checks limits (existing)
   |
4. Native chat handler parses NativeChatRequest
   |
5. SessionManager.get_or_create(session_id, tier, external_id)
   |-- New session? -> TierRouter.route(tier) -> creates Session
   |-- Existing session? -> returns stored Session
   |
6. ProviderRegistry.get(session.provider)
   |-- Returns (AiProvider, RequestTranslator)
   |
7. RequestTranslator.translate_request(native_request, session.model)
   |-- Produces serde_json::Value in provider format
   |
8. AiProvider.chat_completions(translated_request, headers)
   |-- Sends to OpenAI/Anthropic/etc
   |
9. RequestTranslator.translate_response(provider_response, session.id)
   |-- Produces NativeChatResponse in unified format
   |
10. Return JSON response to client
```

### Streaming Request Flow

```
1-7. Same as non-streaming
   |
8. AiProvider.chat_completions_stream(translated_request, headers)
   |-- Returns ByteStream
   |
9. For each chunk in stream:
   |-- RequestTranslator.translate_stream_chunk(chunk)
   |-- Yield unified SSE format to client
   |
10. On stream completion:
    |-- Track tokens with BatchingUsageTracker
    |-- SessionManager.touch(session_id)
```

### Unified SSE Format

Client receives consistent SSE regardless of provider:

```
data: {"type": "content", "content": "Hello"}

data: {"type": "content", "content": " there!"}

data: {"type": "tool_call_start", "id": "call_123", "name": "get_weather"}

data: {"type": "tool_call_args", "id": "call_123", "args_delta": "{\"loc"}

data: {"type": "tool_call_args", "id": "call_123", "args_delta": "ation\": \"SF\"}"}

data: {"type": "done", "finish_reason": "tool_use"}

data: [DONE]
```

---

## File Structure

```
src/
  native/
    mod.rs              # Module exports
    types.rs            # Unified request/response types
    session.rs          # Session management
    router.rs           # Tier-based routing
    providers/
      mod.rs            # ProviderRegistry
    translator/
      mod.rs            # RequestTranslator trait
      openai.rs         # OpenAI translation
      anthropic.rs      # Anthropic translation (future)
    routes/
      mod.rs            # Route registration
      chat.rs           # POST /native/chat
      models.rs         # GET /native/models
      docs.rs           # GET /native/docs (OpenAPI)
    error.rs            # Native API errors
```

---

## Suggested Build Order

Based on dependencies between components:

### Phase 1: Foundation
**Build order rationale:** Types must exist before anything can use them.

1. `native/types.rs` - Unified request/response types
2. `native/error.rs` - Native-specific error types

### Phase 2: Translation Layer
**Build order rationale:** Translator needed before routes can work.

3. `native/translator/mod.rs` - RequestTranslator trait
4. `native/translator/openai.rs` - OpenAI implementation (only provider for v1)

### Phase 3: Session and Routing
**Build order rationale:** Router needs Zion integration design, Session needs Redis.

5. `native/router.rs` - TierRouter with hardcoded fallback config
6. `native/session.rs` - SessionManager with Redis

### Phase 4: Provider Integration
**Build order rationale:** Registry ties everything together.

7. `native/providers/mod.rs` - ProviderRegistry

### Phase 5: API Endpoints
**Build order rationale:** Routes are the public interface, need all components ready.

8. `native/routes/chat.rs` - Main chat endpoint
9. `native/routes/models.rs` - Model listing
10. `native/routes/docs.rs` - OpenAPI documentation

### Phase 6: Integration
**Build order rationale:** Wire into existing infrastructure.

11. Update `src/routes/mod.rs` to mount `/native/*` routes
12. Update `AppState` to include native components
13. Integration tests

---

## Integration with Existing Architecture

### AppState Extensions

Add to `src/lib.rs`:

```rust
pub struct AppState {
    // ... existing fields ...

    // Native API components
    pub session_manager: Arc<SessionManager>,
    pub tier_router: Arc<TierRouter>,
    pub provider_registry: Arc<ProviderRegistry>,
}
```

### Route Registration

Add to `src/routes/mod.rs`:

```rust
// Existing protected routes under /v1
let v1_routes = Router::new()
    .route("/chat/completions", post(chat::chat_completions))
    // ... existing routes ...
    .layer(/* existing middleware */);

// New native routes under /native
let native_routes = Router::new()
    .route("/chat", post(native::routes::chat::chat))
    .route("/models", get(native::routes::models::list_models))
    .layer(/* same auth + rate limit middleware */);

// Docs route with separate API key auth
let docs_routes = Router::new()
    .route("/native/docs", get(native::routes::docs::openapi))
    .layer(middleware::from_fn(docs_api_key_auth));

Router::new()
    .merge(public_routes)
    .nest("/v1", v1_routes)
    .nest("/native", native_routes)
    .merge(docs_routes)
    .with_state(state)
```

### Middleware Reuse

The native API reuses existing middleware:
- `auth_middleware` - Same JWT validation via Zion
- `rate_limit_middleware` - Same Redis sliding window

No new middleware needed for MVP.

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Leaky Abstraction
**What:** Exposing provider-specific fields in unified format
**Why bad:** Clients become dependent on provider internals
**Instead:** Define clean unified types; translate at boundaries

### Anti-Pattern 2: Stateful Session Logic in Handlers
**What:** Managing session state inline in route handlers
**Why bad:** Duplicated logic, harder to test, inconsistent behavior
**Instead:** SessionManager encapsulates all session logic

### Anti-Pattern 3: Hardcoded Model Lists
**What:** Embedding model names directly in code
**Why bad:** Requires code deploy to add/remove models
**Instead:** TierRouter with Zion configuration + fallback

### Anti-Pattern 4: Silent Provider Failover
**What:** Automatically switching providers mid-conversation on error
**Why bad:** Inconsistent behavior, debugging nightmare, billing confusion
**Instead:** Errors bubble up; client can retry or start new session

### Anti-Pattern 5: Synchronous Config Fetching
**What:** Blocking request handling to fetch Zion config
**Why bad:** Latency spike, single point of failure
**Instead:** Async refresh with cached fallback

---

## Scalability Considerations

| Concern | Current (100 users) | Growth (10K users) | Scale (1M users) |
|---------|---------------------|--------------------|--------------------|
| Session Storage | Redis single instance | Redis single instance | Redis Cluster |
| Config Cache | In-memory per instance | In-memory per instance | Shared Redis cache |
| Provider Connections | HTTP client pooling | Connection pool tuning | Per-provider pools |
| Translation Overhead | Negligible | Negligible | Consider caching common patterns |

---

## Sources

### Official Documentation
- [Anthropic Tool Use Overview](https://platform.claude.com/docs/en/docs/build-with-claude/tool-use/overview) - Tool definition format with `input_schema`
- [OpenAI Function Calling](https://platform.openai.com/docs/guides/function-calling) - Tool definition format with `parameters`
- [Axum Documentation](https://docs.rs/axum/latest/axum/) - Router and middleware patterns

### Industry Patterns
- [LiteLLM Proxy](https://docs.litellm.ai/docs/simple_proxy) - Unified API for 100+ providers
- [TensorZero Gateway](https://www.tensorzero.com/docs/gateway) - Rust-based LLM gateway with <1ms overhead
- [RouteLLM Framework](https://lmsys.org/blog/2024-07-01-routellm/) - Cost-quality routing algorithms
- [LLM Gateway Comparison 2025](https://dev.to/debmckinney/5-llm-gateways-compared-choosing-the-right-infrastructure-2025-3h1p) - Architecture patterns

### API Translation References
- [Anthropic OpenAI SDK Compatibility](https://docs.anthropic.com/en/api/openai-sdk) - Official translation notes
- [LLMGateway Anthropic Endpoint](https://docs.llmgateway.io/features/anthropic-endpoint) - Format transformation

---

*Architecture research: 2026-01-31*
