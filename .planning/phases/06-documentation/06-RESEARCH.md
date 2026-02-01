# Phase 6: Documentation - Research

**Researched:** 2026-02-01
**Domain:** OpenAPI documentation with utoipa for Rust/Axum
**Confidence:** HIGH

## Summary

This phase implements OpenAPI 3.x documentation for the Native API using the utoipa ecosystem, the established standard for compile-time OpenAPI generation in Rust. The utoipa crate provides derive macros (`ToSchema`, `ToResponse`, `#[utoipa::path]`) that generate OpenAPI specifications from code annotations, ensuring documentation always matches the actual implementation.

The documentation endpoints (`/native/docs` for Swagger UI, `/native/docs/openapi.json` for raw spec) will be protected by an API key (`X-Docs-Key` header) with a 404 response for unauthorized requests to hide endpoint existence. In development, when `DOCS_API_KEY` is unset, the endpoints will be accessible without authentication.

**Primary recommendation:** Use utoipa 5.x with utoipa-swagger-ui 9.x and the axum feature flag. Annotate existing types with `#[derive(ToSchema)]` and handlers with `#[utoipa::path]`. Create a dedicated docs module with custom middleware for API key protection.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| utoipa | 5.4+ | OpenAPI spec generation | De facto standard for Rust OpenAPI, compile-time generation, code-first approach |
| utoipa-swagger-ui | 9.0+ | Swagger UI serving | Official companion crate, pre-built axum integration |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| utoipa-axum | 0.2+ | Axum router integration | Optional - simplifies route+doc composition with OpenApiRouter |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| utoipa-swagger-ui | utoipa-redoc, utoipa-scalar | Swagger UI is most widely recognized; alternatives offer different UI styles |
| utoipa-axum | Manual route+spec composition | OpenApiRouter is more ergonomic but adds dependency; manual is simpler for small APIs |

**Installation (Cargo.toml):**
```toml
[dependencies]
utoipa = { version = "5.4", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "9.0", features = ["axum", "vendored"] }
```

**Feature flag notes:**
- `axum_extras`: Enables IntoParams without explicit `parameter_in` attribute
- `vendored`: Bundles Swagger UI assets (recommended for reproducible builds, no network dependency)

## Architecture Patterns

### Recommended Project Structure
```
src/
├── native/
│   ├── types.rs          # Add #[derive(ToSchema)] to existing types
│   ├── request.rs        # Add #[derive(ToSchema)] to ChatCompletionRequest
│   ├── response.rs       # Add #[derive(ToSchema)] to response types
│   └── error.rs          # Add #[derive(ToSchema)] to NativeErrorResponse
├── native_routes/
│   ├── mod.rs            # Router setup
│   ├── chat.rs           # Add #[utoipa::path] to handler
│   └── docs.rs           # NEW: Docs routes and API key middleware
└── docs/
    └── openapi.rs        # NEW: OpenApi struct definition and security schemes
```

### Pattern 1: Annotate Existing Types with ToSchema

**What:** Add `#[derive(utoipa::ToSchema)]` to existing serde types
**When to use:** All request/response types that should appear in OpenAPI spec
**Example:**
```rust
// Source: https://docs.rs/utoipa/latest/utoipa/derive.ToSchema.html
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Message {
    /// The role of the message author
    pub role: Role,
    /// The content of the message
    #[schema(example = "Hello, how can I help you today?")]
    pub content: Content,
    // ... other fields
}
```

### Pattern 2: Document Handlers with utoipa::path

**What:** Add detailed endpoint documentation with the `#[utoipa::path]` attribute
**When to use:** Every handler that should be documented
**Example:**
```rust
// Source: https://docs.rs/utoipa/latest/utoipa/attr.path.html
use utoipa::path;

/// Handle native chat completion requests
#[utoipa::path(
    post,
    path = "/native/v1/chat/completions",
    tag = "Chat",
    request_body(
        content = ChatCompletionRequest,
        description = "Chat completion request with messages and optional settings",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "Successful completion", body = ChatCompletionResponse),
        (status = 400, description = "Invalid request", body = NativeErrorResponse,
            example = json!({"error": {"message": "Invalid request body", "type": "invalid_request_error", "code": "invalid_request"}})),
        (status = 401, description = "Missing or invalid authentication"),
        (status = 403, description = "Insufficient permissions or quota exceeded"),
        (status = 429, description = "Rate limit exceeded", body = NativeErrorResponse),
        (status = 500, description = "Internal server error", body = NativeErrorResponse),
        (status = 503, description = "Service unavailable", body = NativeErrorResponse)
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn native_chat_completions(/* ... */) -> Result<Response, NativeErrorResponse> {
    // ...
}
```

### Pattern 3: Define OpenAPI Struct with Security Schemes

**What:** Create central OpenApi struct that aggregates all paths and schemas
**When to use:** Once per API documentation scope
**Example:**
```rust
// Source: https://docs.rs/utoipa/latest/utoipa/derive.OpenApi.html
use utoipa::{openapi::security::{Http, HttpAuthScheme, SecurityScheme}, Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Sentinel Native API",
        version = "1.0.0",
        description = "Native API for Sentinel AI Proxy - unified format with tier routing and session management",
        contact(name = "Sentinel Team")
    ),
    paths(
        crate::native_routes::chat::native_chat_completions
    ),
    components(
        schemas(
            ChatCompletionRequest,
            ChatCompletionResponse,
            Message,
            Role,
            Content,
            // ... all other types
        ),
        responses(
            NativeErrorResponse
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Chat", description = "Chat completion endpoints")
    )
)]
pub struct NativeApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    Http::new(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .description(Some("Zion JWT token in Authorization header"))
                ),
            );
        }
    }
}
```

### Pattern 4: API Key Protection Middleware

**What:** Custom middleware to protect docs endpoints with API key
**When to use:** For the docs routes specifically
**Example:**
```rust
// Source: https://docs.rs/axum/latest/axum/middleware/fn.from_fn.html
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

pub async fn docs_auth_middleware(
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    // Get API key from environment
    let expected_key = std::env::var("DOCS_API_KEY").ok();

    // If no key configured, allow access (dev mode)
    if expected_key.is_none() {
        return Ok(next.run(request).await);
    }

    // Check X-Docs-Key header
    let provided_key = request
        .headers()
        .get("X-Docs-Key")
        .and_then(|v| v.to_str().ok());

    match (expected_key, provided_key) {
        (Some(expected), Some(provided)) if expected == provided => {
            Ok(next.run(request).await)
        }
        _ => {
            // Return 404 to hide endpoint existence
            Err(StatusCode::NOT_FOUND.into_response())
        }
    }
}
```

### Pattern 5: Serve Swagger UI and OpenAPI JSON

**What:** Configure routes for docs UI and raw spec
**When to use:** Setting up the docs router
**Example:**
```rust
// Source: https://docs.rs/utoipa-swagger-ui/latest/utoipa_swagger_ui/
use axum::{middleware, routing::get, Json, Router};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub fn create_docs_router<S>(state: Arc<AppState>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let openapi = NativeApiDoc::openapi();

    Router::new()
        // Swagger UI at /native/docs
        .merge(
            SwaggerUi::new("/native/docs")
                .url("/native/docs/openapi.json", openapi.clone())
        )
        // Also serve raw JSON at /native/docs/openapi.json (handled by SwaggerUi)
        .layer(middleware::from_fn(docs_auth_middleware))
}
```

### Pattern 6: Static File Export for CI/Tooling

**What:** Export OpenAPI spec to static JSON file
**When to use:** For SDK generation, API linting, or CI validation
**Example (binary approach):**
```rust
// src/bin/export_openapi.rs
use sentinel::docs::NativeApiDoc;
use utoipa::OpenApi;

fn main() {
    let spec = NativeApiDoc::openapi();
    let json = spec.to_pretty_json().expect("Failed to serialize OpenAPI spec");
    std::fs::write("docs/openapi.json", json).expect("Failed to write openapi.json");
    println!("Exported OpenAPI spec to docs/openapi.json");
}
```

**Run with:** `cargo run --bin export_openapi`

### Anti-Patterns to Avoid

- **Inline recursive types:** Never use `#[schema(inline)]` on recursive data types - causes infinite loops
- **Forgetting schema registration:** All types referenced in paths must be registered in `components(schemas(...))`
- **Documenting implementation details:** Only document the public API contract, not internal types
- **Hardcoding examples:** Use `#[schema(example = ...)]` with realistic but not production data

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| OpenAPI spec generation | Manual JSON construction | utoipa derive macros | Compile-time validation, stays in sync with code |
| Swagger UI serving | Static file embedding | utoipa-swagger-ui | Pre-built, handles asset serving correctly |
| Schema validation | Manual validation | utoipa's ToSchema | Automatic from type definition |
| Security scheme definition | Manual JSON | utoipa's Modify trait | Type-safe, documented pattern |

**Key insight:** utoipa turns compile-time type information into OpenAPI specs. Hand-rolling loses this guarantee and creates drift between code and documentation.

## Common Pitfalls

### Pitfall 1: Schema Not Found at Runtime

**What goes wrong:** Types used in responses aren't registered, causing empty schemas
**Why it happens:** Forgot to add type to `components(schemas(...))` in OpenApi derive
**How to avoid:** Register all types explicitly; use utoipauto crate for automatic detection if needed
**Warning signs:** Empty `$ref` in generated spec, missing type definitions

### Pitfall 2: Serde/Schema Attribute Conflicts

**What goes wrong:** Field names or structure don't match between serde and schema
**Why it happens:** Serde's `rename_all` takes precedence; `#[schema(rename)]` is overridden
**How to avoid:** Let serde control naming, use doc comments for descriptions
**Warning signs:** Field names in spec don't match what API actually accepts

### Pitfall 3: Missing Security in Paths

**What goes wrong:** Swagger UI doesn't show auth requirements or "Authorize" button
**Why it happens:** Security scheme defined but not applied to paths
**How to avoid:** Add `security(("bearer_auth" = []))` to each protected endpoint
**Warning signs:** No lock icon on endpoints in Swagger UI

### Pitfall 4: Untagged Enum Documentation

**What goes wrong:** Enums with `#[serde(untagged)]` generate complex oneOf schemas
**Why it happens:** utoipa correctly models untagged as anyOf/oneOf per OpenAPI
**How to avoid:** Accept the generated schema or add discriminator hints
**Warning signs:** Complex schema in docs for simple-seeming types

### Pitfall 5: Dev vs Prod Auth Behavior

**What goes wrong:** Docs accessible in prod without API key
**Why it happens:** Forgot to set `DOCS_API_KEY` environment variable in production
**How to avoid:** Make it a deployment checklist item; consider requiring key in prod
**Warning signs:** Docs endpoint returns content instead of 404 in production

## Code Examples

### Complete Type Annotation Example
```rust
// Source: https://docs.rs/utoipa/latest/utoipa/derive.ToSchema.html

/// Chat completion request
///
/// Uses `deny_unknown_fields` to ensure strict validation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatCompletionRequest {
    /// Complexity tier for model routing
    #[schema(example = "simple")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<Tier>,

    /// Messages in the conversation
    pub messages: Vec<Message>,

    /// Sampling temperature (0.0 to 2.0)
    #[schema(minimum = 0.0, maximum = 2.0, example = 0.7)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Maximum tokens to generate
    #[schema(minimum = 1, example = 1000)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Whether to stream the response
    #[serde(default)]
    pub stream: bool,

    /// Conversation ID for session stickiness
    #[schema(example = "conv-550e8400-e29b-41d4-a716-446655440000")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,

    /// Tool definitions available to the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,

    /// How the model should use the provided tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}
```

### Error Response Documentation
```rust
/// Wrapper for error responses matching OpenAI's format
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NativeErrorResponse {
    /// The error details
    pub error: NativeError,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NativeError {
    /// Human-readable error message
    #[schema(example = "Invalid request body: missing required field 'messages'")]
    pub message: String,

    /// Error type category
    #[serde(rename = "type")]
    #[schema(example = "invalid_request_error")]
    pub error_type: String,

    /// Error code for programmatic handling
    #[schema(example = "invalid_request")]
    pub code: String,

    /// Provider hint when error originates from upstream
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "openai")]
    pub provider: Option<String>,
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual OpenAPI YAML | Derive macros from code | utoipa 4.x+ (2023) | Documentation always matches code |
| Separate spec file | Runtime-generated spec | utoipa 5.x | Single source of truth |
| Paperclip crate | utoipa | 2022 | utoipa is now dominant |

**Deprecated/outdated:**
- `paperclip`: Less maintained, utoipa preferred
- Manual OpenAPI file maintenance: Error-prone, drifts from implementation

## Open Questions

### 1. Docs Route Nesting

**What we know:** Docs need to be at `/native/docs` and `/native/docs/openapi.json`
**What's unclear:** Whether to nest under existing `/native` router or create separate top-level
**Recommendation:** Create separate router merged at root level, as docs shouldn't have the auth/rate-limit middleware of the main API

### 2. Streaming Response Documentation

**What we know:** OpenAPI 3.1 supports streaming via `content-type: text/event-stream`
**What's unclear:** How well utoipa documents SSE streaming responses
**Recommendation:** Document streaming with a note in description; focus on the SSE chunk schema

## Sources

### Primary (HIGH confidence)
- [utoipa docs.rs](https://docs.rs/utoipa/latest/utoipa/) - Version 5.4.0, derive macros, OpenApi struct
- [utoipa-swagger-ui docs.rs](https://docs.rs/utoipa-swagger-ui/latest/utoipa_swagger_ui/) - Version 9.0.2, axum integration
- [utoipa GitHub examples](https://github.com/juhaku/utoipa/blob/master/examples/todo-axum/src/main.rs) - Working axum example

### Secondary (MEDIUM confidence)
- [utoipa-axum README](https://github.com/juhaku/utoipa/blob/master/utoipa-axum/README.md) - OpenApiRouter usage
- [Axum middleware docs](https://docs.rs/axum/latest/axum/middleware/fn.from_fn.html) - API key middleware pattern

### Tertiary (LOW confidence)
- Web search results for static file export - Limited official documentation on build.rs approach

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - utoipa is the clear standard, versions verified on docs.rs
- Architecture: HIGH - Patterns derived from official examples and documentation
- Pitfalls: MEDIUM - Based on documentation warnings and common issues

**Research date:** 2026-02-01
**Valid until:** 2026-03-01 (30 days - stable ecosystem)
