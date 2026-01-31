# Phase 2: API Endpoints - Research

**Researched:** 2026-02-01
**Domain:** Axum HTTP routing and request handling for unified AI API
**Confidence:** HIGH

## Summary

This phase exposes `/native/*` endpoints that accept the unified format from Phase 1 and return responses. Research focused on Axum routing patterns, middleware composition, SSE streaming implementation, and integration testing patterns already established in the codebase.

Key findings:
- Axum's router nesting (`Router::nest`) provides clean separation between `/v1/*` (existing) and `/native/*` (new) endpoints
- The existing middleware stack (auth + rate limiting) can be reused directly via `middleware::from_fn_with_state`
- Phase 1 already provides all necessary types: `ChatCompletionRequest`, `ChatCompletionResponse`, `StreamChunk`, `NativeErrorResponse`
- The OpenAI translator (`OpenAITranslator`) handles bidirectional conversion between unified and provider formats
- Streaming uses standard Axum SSE patterns with `Body::from_stream` - same as existing `/v1/chat/completions`
- Testing follows established patterns: `TokenTrackingTestHarness` provides mock servers for integration tests

**Primary recommendation:** Create a dedicated `native_routes` module with handlers that use Phase 1 types and translators. Reuse existing middleware and streaming infrastructure. Test with the established harness pattern.

## Standard Stack

All dependencies already exist in the codebase. No new libraries needed.

### Core (Already in Cargo.toml)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| axum | 0.7.x | HTTP framework with routing and middleware | Already used throughout, well-documented patterns |
| serde | 1.x | Request/response serialization | Already used, pairs with Phase 1 types |
| serde_json | 1.x | JSON parsing | Already used for API request/response handling |
| futures | 0.3.x | Stream utilities for SSE | Already used in streaming handlers |
| bytes | 1.x | Byte buffer handling for SSE chunks | Already used in streaming module |
| tracing | 0.1.x | Logging and instrumentation | Already used, follows codebase conventions |

### Supporting (Already Available)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| async-stream | 0.3.x | Stream creation macros | Wrapping provider streams |
| axum-test | 0.19.x | Integration testing | Already used for /v1/* tests |
| wiremock | 0.6.x | Mock HTTP servers | Already used for Zion/OpenAI mocks |

### Not Needed
| Library | Why Not |
|---------|---------|
| tower-http (new layers) | Already configured globally in `create_router` |
| axum-extra | Standard axum features sufficient |
| actix-web | Codebase uses Axum |

**Installation:** No changes to Cargo.toml required.

## Architecture Patterns

### Recommended Module Structure
```
src/
├── routes/
│   ├── mod.rs                 # Existing: add native_routes integration
│   ├── chat.rs                # Existing: /v1/chat/completions
│   └── ...
├── native_routes/             # New: dedicated module for /native/* endpoints
│   ├── mod.rs                 # Router creation and exports
│   └── chat.rs                # POST /native/v1/chat/completions handler
├── native/                    # Existing (Phase 1): types and translation
│   ├── request.rs             # ChatCompletionRequest
│   ├── response.rs            # ChatCompletionResponse, StreamChunk
│   ├── error.rs               # NativeErrorResponse
│   ├── streaming.rs           # format_sse_chunk, format_sse_done
│   └── translate/
│       └── openai.rs          # OpenAITranslator
└── tests/
    └── integration/
        └── native_chat.rs     # New: integration tests for /native/*
```

### Pattern 1: Separate Router Module with Middleware Reuse
**What:** Create `native_routes` module that builds its own Router, then nest under `/native`
**When to use:** All /native/* endpoint setup
**Example:**
```rust
// src/native_routes/mod.rs
pub mod chat;

use std::sync::Arc;
use axum::{middleware, routing::post, Router};
use crate::{
    middleware::{auth::auth_middleware, rate_limiter::rate_limit_middleware},
    AppState,
};

/// Create the native API router with authentication and rate limiting
pub fn create_native_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Native API endpoints (versioned under /v1)
        .route("/v1/chat/completions", post(chat::native_chat_completions))
        // Apply rate limiting (runs after auth)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        // Apply authentication (runs first)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

// src/routes/mod.rs - integrate native routes
pub fn create_router(state: Arc<AppState>) -> Router {
    // ... existing public and protected routes ...

    Router::new()
        .merge(public_routes)
        .merge(debug_routes)
        .nest("/v1", protected_routes)
        // Add native API routes under /native
        .nest("/native", native_routes::create_native_router(state.clone()))
        .fallback(fallback_handler)
        // ... global middleware ...
}
```

### Pattern 2: Handler with Translation Layer
**What:** Handler receives native request, translates to provider format, calls provider, translates response back
**When to use:** All native API handlers
**Example:**
```rust
// src/native_routes/chat.rs
use std::sync::Arc;
use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};
use crate::{
    middleware::auth::AuthenticatedUser,
    native::{
        error::NativeErrorResponse,
        request::ChatCompletionRequest,
        response::ChatCompletionResponse,
        translate::{MessageTranslator, OpenAITranslator, TranslationError},
    },
    AppState,
};

pub async fn native_chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Result<impl IntoResponse, NativeErrorResponse> {
    // Extract authenticated user (set by auth middleware)
    let user = request
        .extensions()
        .get::<AuthenticatedUser>()
        .cloned()
        .ok_or_else(|| NativeErrorResponse::internal("Authentication required"))?;

    // Parse request body with strict validation (deny_unknown_fields)
    let body = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| NativeErrorResponse::validation(format!("Failed to read body: {}", e)))?;

    let native_request: ChatCompletionRequest = serde_json::from_slice(&body)
        .map_err(|e| NativeErrorResponse::validation(format!("Invalid request: {}", e)))?;

    // Translate to OpenAI format
    let translator = OpenAITranslator::new();
    let provider_request = translator
        .translate_request(&native_request)
        .map_err(|e| NativeErrorResponse::validation(e.to_string()))?;

    // Call provider
    let provider_response = state
        .ai_provider
        .chat_completions(provider_request, &headers)
        .await
        .map_err(|e| NativeErrorResponse::provider_error(e.to_string(), "openai"))?;

    // Translate response back
    let native_response = translator
        .translate_response(provider_response)
        .map_err(|e| NativeErrorResponse::internal(e.to_string()))?;

    // Track usage
    state.batching_tracker.track(
        user.email.clone(),
        native_response.usage.prompt_tokens as u64,
        native_response.usage.completion_tokens as u64,
        native_request.model.clone(),
    );

    Ok(Json(native_response))
}
```

### Pattern 3: Streaming with SSE Format
**What:** Stream provider response, normalize chunks to unified format, emit as SSE
**When to use:** When `stream: true` in request
**Example:**
```rust
// src/native_routes/chat.rs - streaming variant
use axum::{body::Body, http::header, response::Response};
use futures::StreamExt;
use crate::native::streaming::{format_sse_chunk, format_sse_done, StreamMetadata};

async fn handle_streaming_native_chat(
    state: Arc<AppState>,
    headers: &HeaderMap,
    native_request: ChatCompletionRequest,
    user: AuthenticatedUser,
) -> Result<Response, NativeErrorResponse> {
    let translator = OpenAITranslator::new();
    let mut provider_request = translator
        .translate_request(&native_request)
        .map_err(|e| NativeErrorResponse::validation(e.to_string()))?;

    // Ensure stream_options.include_usage for token tracking
    provider_request["stream_options"] = serde_json::json!({"include_usage": true});

    // Get streaming response from provider
    let stream = state
        .ai_provider
        .chat_completions_stream(provider_request, headers)
        .await
        .map_err(|e| NativeErrorResponse::provider_error(e.to_string(), "openai"))?;

    // Transform stream to emit normalized chunks
    // (OpenAI format is already our target format, minimal transformation needed)
    let final_stream = async_stream::stream! {
        futures::pin_mut!(stream);
        while let Some(chunk) = stream.next().await {
            yield chunk;
        }
        // Usage tracking happens at stream end (same as /v1/* pattern)
    };

    let body = Body::from_stream(final_stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .header("X-Accel-Buffering", "no")
        .body(body)
        .map_err(|e| NativeErrorResponse::internal(format!("Failed to build response: {}", e)))
}
```

### Pattern 4: Native Error Response Integration
**What:** Use `NativeErrorResponse` (from Phase 1) as handler return error type
**When to use:** All native route handlers
**Example:**
```rust
// NativeErrorResponse already implements IntoResponse from Phase 1
// Handler signature returns Result<impl IntoResponse, NativeErrorResponse>

// For validation errors:
NativeErrorResponse::validation("messages[0].content: expected string or array")

// For provider errors:
NativeErrorResponse::provider_error("Rate limit exceeded", "openai")

// For internal errors:
NativeErrorResponse::internal("Failed to process request")

// For rate limit errors (from middleware):
NativeErrorResponse::rate_limited("Rate limit exceeded", Some(60))
```

### Pattern 5: Integration Testing with Harness
**What:** Use `TokenTrackingTestHarness` pattern for full integration tests
**When to use:** All native endpoint integration tests
**Example:**
```rust
// tests/integration/native_chat.rs
use crate::common::TokenTrackingTestHarness;
use serde_json::json;

#[tokio::test]
async fn test_native_chat_completions_non_streaming() {
    let harness = TokenTrackingTestHarness::new().await;

    // Set up mocks
    harness.zion.mock_get_user_profile_success(make_test_profile()).await;
    harness.openai.mock_chat_completion_with_usage("Hello!", 10, 5).await;

    // Make request to native endpoint
    let response = harness.server
        .post("/native/v1/chat/completions")
        .add_header("Authorization", format!("Bearer {}", TEST_JWT_TOKEN))
        .json(&json!({
            "messages": [{"role": "user", "content": "Hello"}]
        }))
        .await;

    response.assert_status_ok();

    let body: serde_json::Value = response.json();
    assert!(body.get("choices").is_some());
    assert!(body.get("usage").is_some());
}
```

### Anti-Patterns to Avoid
- **Duplicating middleware logic:** Reuse existing `auth_middleware` and `rate_limit_middleware` - don't create native-specific versions.
- **Mixing error formats:** Native endpoints should return `NativeErrorResponse`, not `AppError`. Use `map_err` to convert at handler boundary.
- **Creating new provider abstraction:** Use existing `AiProvider` trait and `OpenAIProvider`. Native routes are about the API format, not new providers.
- **Copying streaming logic:** Reuse existing streaming patterns from `/v1/chat/completions`. Only the request/response format differs.
- **Hardcoding model selection:** Phase 4 will add tier routing. For now, pass model through to provider.

## Don't Hand-Roll

Problems that look simple but have existing solutions:

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SSE formatting | Custom string building | `format_sse_chunk`, `format_sse_done` | Already handles edge cases correctly |
| Request validation | Custom validators | serde `deny_unknown_fields` | Already in `ChatCompletionRequest` from Phase 1 |
| Authentication | Custom JWT parsing | `auth_middleware` | Already validates with Zion, caches results |
| Rate limiting | Custom counter logic | `rate_limit_middleware` | Already implements sliding window in Redis |
| Token counting | Custom tokenizer | `SharedTokenCounter` | Uses tiktoken-rs, matches OpenAI |
| Usage tracking | Direct Zion calls | `batching_tracker.track()` | Batches and deduplicates automatically |
| Test mocks | Custom mock servers | `TokenTrackingTestHarness` | Already configures Zion + OpenAI mocks |

**Key insight:** Phase 2 is primarily about routing and format translation. All the heavy lifting (auth, rate limiting, provider communication, streaming, usage tracking) already exists. Focus on wiring, not reimplementing.

## Common Pitfalls

### Pitfall 1: Forgetting to Wire Native Routes
**What goes wrong:** Native routes exist but aren't accessible (404)
**Why it happens:** Forgot to call `nest("/native", ...)` in `create_router`
**How to avoid:** Follow Pattern 1 - explicitly integrate native router in `routes/mod.rs`
**Warning signs:** 404 for all `/native/*` requests, routes not showing in route list

### Pitfall 2: Wrong Error Response Type
**What goes wrong:** Native endpoints return OpenAI-format errors instead of native format
**Why it happens:** Using `AppError` instead of `NativeErrorResponse`
**How to avoid:** Handler signature must be `Result<impl IntoResponse, NativeErrorResponse>`; convert AppError at boundary with `map_err`
**Warning signs:** Error responses have `error.code` like "UNAUTHORIZED" instead of `error.type` like "invalid_request_error"

### Pitfall 3: Breaking /v1/* Endpoints
**What goes wrong:** Existing clients fail after native routes added
**Why it happens:** Router configuration interferes with existing routes
**How to avoid:**
- Use `nest("/native", ...)` not `merge` for native routes
- Keep existing `/v1` nest unchanged
- Run regression tests on all `/v1/*` endpoints
**Warning signs:** Existing tests fail, `/v1/chat/completions` returns 404 or wrong format

### Pitfall 4: Missing stream_options for Token Tracking
**What goes wrong:** Streaming requests show 0 tokens in usage metrics
**Why it happens:** OpenAI only returns usage in stream if `stream_options.include_usage: true`
**How to avoid:** Inject `stream_options` into translated request before calling provider (existing pattern in `/v1/chat/completions`)
**Warning signs:** Streaming requests always fall back to estimation

### Pitfall 5: Model Field Handling
**What goes wrong:** Requests without model field fail at provider
**Why it happens:** Native request has optional `model` (for tier routing in Phase 4)
**How to avoid:** For Phase 2, require model or provide sensible default; document that model becomes optional in Phase 4
**Warning signs:** "model is required" errors from OpenAI

### Pitfall 6: Middleware Order Issues
**What goes wrong:** Rate limiting runs before auth, or neither runs
**Why it happens:** Axum middleware layers apply in reverse order of how they're listed
**How to avoid:** Follow existing pattern - add rate_limit layer first, then auth layer (auth runs first at runtime)
**Warning signs:** Requests bypassing auth, rate limits not applying to users

## Code Examples

Verified patterns from existing codebase:

### Router Integration Point
```rust
// Source: src/routes/mod.rs - existing pattern
// Add native routes alongside existing structure

pub fn create_router(state: Arc<AppState>) -> Router {
    // ... existing code ...

    Router::new()
        .merge(public_routes)
        .merge(debug_routes)
        .nest("/v1", protected_routes)
        // NEW: Add native API routes
        .nest("/native", native_routes::create_native_router(state.clone()))
        .fallback(fallback_handler)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
```

### Body Parsing with Validation Error
```rust
// Source: Existing chat.rs pattern adapted for native errors
let body = axum::body::to_bytes(request.into_body(), usize::MAX)
    .await
    .map_err(|e| NativeErrorResponse::validation(
        format!("Failed to read request body: {}", e)
    ))?;

let native_request: ChatCompletionRequest = serde_json::from_slice(&body)
    .map_err(|e| {
        // Parse serde error to extract field path
        let msg = e.to_string();
        NativeErrorResponse::validation(msg)
    })?;
```

### Streaming Response Headers
```rust
// Source: src/routes/chat.rs lines 509-516 - exact headers to use
Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, "text/event-stream")
    .header(header::CACHE_CONTROL, "no-cache")
    .header(header::CONNECTION, "keep-alive")
    .header("X-Accel-Buffering", "no")  // For nginx proxy buffering
    .body(body)
```

### Usage Tracking Call
```rust
// Source: src/routes/chat.rs line 292 - existing pattern
state.batching_tracker.track(
    user.email.clone(),
    input_tokens,
    output_tokens,
    Some(model.clone()),
);
```

### Integration Test Request
```rust
// Source: tests/common/mod.rs - TokenTrackingTestHarness usage
let response = harness.server
    .post("/native/v1/chat/completions")
    .add_header("Authorization", format!("Bearer {}", constants::TEST_JWT_TOKEN))
    .json(&json!({
        "messages": [
            {"role": "user", "content": "Hello!"}
        ],
        "model": "gpt-4o"
    }))
    .await;

response.assert_status_ok();
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual request validation | serde derive with deny_unknown_fields | Phase 1 | Strict validation at deserialization |
| Per-endpoint middleware | Layer composition on Router | Axum 0.7 | Cleaner, reusable middleware |
| Manual SSE formatting | StreamChunk + format_sse_chunk | Phase 1 | Consistent SSE output |
| Direct error returns | NativeErrorResponse with IntoResponse | Phase 1 | OpenAI-compatible error format |

**Deprecated/outdated:**
- Using `axum::Json` for request parsing: Less control over error messages; use `to_bytes` + `serde_json::from_slice` for custom error formatting
- Creating per-route AppState clones: Use `State<Arc<AppState>>` extractor

## Open Questions

Things that couldn't be fully resolved:

1. **Model field requirement for Phase 2**
   - What we know: Native request has optional model (designed for Phase 4 tier routing)
   - What's unclear: Whether to require model in Phase 2 or allow default
   - Recommendation: Claude's discretion - require model for Phase 2, document that Phase 4 makes it optional via tier routing

2. **Content limit validation**
   - What we know: CONTEXT.md mentions "Enforce content limits before sending to provider"
   - What's unclear: What specific limits to enforce (max message count, max content length)
   - Recommendation: Claude's discretion per CONTEXT.md - defer to Phase 4 tier config; for now, let provider enforce limits

3. **Regression test scope**
   - What we know: Success criteria requires "regression-free" /v1/* endpoints
   - What's unclear: Which specific /v1/* tests constitute the regression suite
   - Recommendation: Run existing tests in `tests/integration/` for chat_completions, models, health; add explicit regression marker test

## Sources

### Primary (HIGH confidence)
- Existing codebase: `src/routes/mod.rs`, `src/routes/chat.rs` - Axum routing patterns
- Existing codebase: `tests/common/mod.rs` - Integration test harness
- Existing codebase: `src/middleware/auth.rs`, `src/middleware/rate_limiter.rs` - Middleware patterns
- Phase 1 code: `src/native/` - Types and translation already implemented

### Secondary (MEDIUM confidence)
- Axum documentation (from training data, verified against codebase usage)
- CONTEXT.md decisions for Phase 2

### Tertiary (LOW confidence)
- None - all patterns verified against existing codebase

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries already in use in codebase
- Architecture: HIGH - Patterns directly from existing codebase, verified working
- Pitfalls: HIGH - Derived from codebase review and existing test coverage

**Research date:** 2026-02-01
**Valid until:** 2026-03-01 (30 days - stable domain, established patterns)
