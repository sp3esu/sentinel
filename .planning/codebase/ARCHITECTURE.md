# Architecture

**Analysis Date:** 2026-01-31

## Pattern Overview

**Overall:** Layered proxy architecture with middleware-based request processing

**Key Characteristics:**
- Middleware-first request pipeline (authentication → rate limiting → handler)
- Provider abstraction trait enabling pluggable AI backends (currently OpenAI)
- Cache abstraction supporting Redis (production) and in-memory (testing)
- Async-first design using Tokio runtime and Axum web framework
- Trait-based polymorphism for extensibility (AiProvider, CacheBackend)

## Layers

**Middleware Layer:**
- Purpose: Request preprocessing before handler execution
- Location: `src/middleware/`
- Contains: Authentication validation, rate limiting enforcement
- Depends on: AppState, Redis, Zion API
- Used by: All protected /v1/* routes

**Handler/Route Layer:**
- Purpose: Process typed API requests with token tracking
- Location: `src/routes/`
- Contains: Chat completions, text completions, embeddings, models endpoints
- Depends on: AiProvider, TokenCounter, UsageTracker, Middleware layers
- Used by: HTTP clients via Axum router

**AI Provider Abstraction Layer:**
- Purpose: Define unified interface for AI backend communication
- Location: `src/proxy/`
- Contains: AiProvider trait, OpenAI implementation, header filtering, request logging
- Depends on: HTTP client, configuration
- Used by: All route handlers

**Caching Layer:**
- Purpose: Cache user limits and JWT validation results
- Location: `src/cache/`
- Contains: RedisCache implementation, InMemoryCache for testing, SubscriptionCache service
- Depends on: Redis client (optional in test mode)
- Used by: Middleware and rate limiting

**External API Layer:**
- Purpose: Communicate with Zion governance system
- Location: `src/zion/`
- Contains: ZionClient (API communication), data models
- Depends on: HTTP client, configuration
- Used by: SubscriptionCache, UsageTracker, Middleware

**Token Counting Layer:**
- Purpose: Estimate and track token usage
- Location: `src/tokens/`
- Contains: SharedTokenCounter wrapping tiktoken-rs encoders
- Depends on: tiktoken-rs library
- Used by: Chat and completion handlers

**Usage Tracking Layer:**
- Purpose: Accumulate and report token usage to Zion
- Location: `src/usage/`
- Contains: UsageTracker (immediate), BatchingUsageTracker (fire-and-forget)
- Depends on: ZionClient
- Used by: Route handlers for usage reporting

**Streaming Layer:**
- Purpose: Parse and buffer SSE streams from AI providers
- Location: `src/streaming/`
- Contains: SseLineBuffer for line-boundary handling
- Depends on: None (utility)
- Used by: OpenAI streaming handlers

**Error Handling Layer:**
- Purpose: Unified error types and HTTP status mapping
- Location: `src/error.rs`
- Contains: AppError enum with IntoResponse implementation
- Depends on: Axum for HTTP integration
- Used by: All layers

## Data Flow

**Standard Request Flow (Chat Completions):**

1. HTTP Request arrives at Axum router
2. Authentication middleware extracts JWT, checks cache, validates with Zion
3. AuthenticatedUser added to request extensions
4. Rate limiting middleware checks Redis sliding window for this user
5. Route handler (chat_completions) receives request
6. Token counter estimates input tokens from messages
7. Request forwarded to OpenAI via AiProvider trait
8. OpenAI response received and parsed
9. Output tokens extracted from response or estimated
10. UsageTracker or BatchingUsageTracker reports to Zion
11. Response sent to client

**Streaming Request Flow:**

1. Same auth/rate limit middleware as above
2. Handler requests streaming from AiProvider
3. AiProvider returns ByteStream (futures::Stream<Result<Bytes>>)
4. SseLineBuffer parses incoming chunks into complete lines
5. Each SSE line parsed for content and token usage
6. Accumulated tokens reported to Zion at stream completion
7. Streaming response sent to client in real-time

**State Management:**

- **AppState** (`src/lib.rs`): Central state container with:
  - Redis connection manager
  - HTTP client (connection pooling)
  - ZionClient (API communication)
  - SubscriptionCache (limits + JWT validation)
  - UsageTracker & BatchingUsageTracker
  - AiProvider (OpenAI implementation)
  - SharedTokenCounter (tiktoken encoders)
- State shared across all handlers via Arc<AppState>
- All state components are thread-safe (Arc, RwLock, ConnectionManager)

## Key Abstractions

**AiProvider Trait:**
- Purpose: Enable pluggable AI backends (OpenAI, Anthropic, Azure, etc.)
- File: `src/proxy/provider.rs`
- Methods: chat_completions, chat_completions_stream, completions, completions_stream, embeddings, list_models, get_model, responses, responses_stream, forward_raw
- Implementation: OpenAIProvider (`src/proxy/openai.rs`)
- Pattern: Uses serde_json::Value for request/response to remain dyn-compatible

**CacheBackend Enum:**
- Purpose: Abstract cache implementation (Redis vs in-memory)
- File: `src/cache/subscription.rs`
- Pattern: SubscriptionCache wraps CacheBackend, tests use InMemoryCache without Redis
- Implementations: Redis (production), InMemory (test-only feature)

**RequestContext:**
- Purpose: Correlate logs and track request metadata
- File: `src/proxy/logging.rs`
- Contains: trace_id, request timing, error tracking
- Pattern: Passed through all OpenAI provider calls for rich logging

**Middleware as Functions:**
- Pattern: auth_middleware and rate_limit_middleware are tower::middleware functions
- Applied in reverse order: auth runs first, rate limiting runs second
- Use State<Arc<AppState>> extractor for state access

## Entry Points

**Main Server:**
- Location: `src/main.rs`
- Triggers: Application startup
- Responsibilities: Load config, initialize state, bind socket, start Axum server with graceful shutdown

**Router Creation:**
- Location: `src/routes/mod.rs::create_router()`
- Triggers: During AppState initialization
- Responsibilities: Assemble protected routes (with middleware), public routes, debug routes, apply global middleware

**Chat Completions Handler:**
- Location: `src/routes/chat.rs::chat_completions()`
- Triggers: POST /v1/chat/completions
- Responsibilities: Parse request, count tokens, forward to provider, track usage, handle streaming

**Passthrough Handler:**
- Location: `src/routes/passthrough.rs::passthrough_handler()`
- Triggers: Fallback for unmatched /v1/* routes
- Responsibilities: Forward raw request body to provider, stream response, track request count only

## Error Handling

**Strategy:** Typed error enum (AppError) with automatic HTTP status code mapping

**Patterns:**

1. **Result Type Alias:**
   - `AppResult<T> = Result<T, AppError>`
   - Used throughout codebase for consistent error handling

2. **Error Conversion:**
   - `impl From<redis::RedisError> for AppError`
   - `impl From<reqwest::Error> for AppError`
   - Automatic conversion using `?` operator

3. **HTTP Response Mapping:**
   - AppError implements IntoResponse trait
   - Converts to appropriate HTTP status code + JSON error body
   - RateLimitExceeded → 429 with X-RateLimit-* headers
   - Unauthorized → 401
   - BadRequest → 400
   - QuotaExceeded → 403 (custom status)

4. **Error Details:**
   - ErrorResponse contains: code, message, optional details
   - Details include rate limit info (limit, used, remaining, reset_at) when applicable

## Cross-Cutting Concerns

**Logging:**
- Framework: `tracing` crate with `tracing_subscriber`
- Pattern: Use #[instrument] macro on functions to auto-capture parameters
- Levels: info (general flow), debug (detailed operations), warn (recoverable issues), error (failures)
- Context: trace_id in RequestContext for request correlation

**Validation:**
- Location: Middleware layer (auth_middleware checks JWT, rate_limit_middleware checks limits)
- Upstream validation: OpenAI provider validates request format before forwarding
- No explicit input validation layer; relies on serde deserialization + upstream validation

**Authentication:**
- Location: `src/middleware/auth.rs::auth_middleware`
- Process: Extract JWT → hash for cache lookup → validate with Zion → cache result
- Caching: JWT validation cached in Redis with ttl from JWT_CACHE_TTL_SECONDS
- User identification: external_id from Zion used for all subsequent Zion API calls

**Rate Limiting:**
- Location: `src/middleware/rate_limiter.rs`
- Algorithm: Sliding window in Redis with atomic MULTI/EXEC
- Configuration: Default 100 requests/60 seconds per user
- Response: 429 with X-RateLimit-* headers and retry-after

**Token Counting:**
- Framework: tiktoken-rs (OpenAI's token encoding)
- Strategy: Estimate input tokens before request, prefer OpenAI's usage field in response
- Streaming: Accumulate content chunks, count at stream completion
- Fallback: If OpenAI doesn't return usage, estimate with tiktoken-rs

**Usage Reporting:**
- Two-tier system:
  - UsageTracker: Immediate reporting (blocks handler completion)
  - BatchingUsageTracker: Fire-and-forget with batch deduplication
- Pattern: Handlers use batching tracker (protects Zion from floods)
- Batching: Uses Redis for dedup, configurable flush interval

---

*Architecture analysis: 2026-01-31*
