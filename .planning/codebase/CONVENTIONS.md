# Coding Conventions

**Analysis Date:** 2026-01-31

## Naming Patterns

**Files:**
- Modules use snake_case: `auth.rs`, `rate_limiter.rs`, `token_counter.rs`
- Test files in `tests/` directory or inline with `#[cfg(test)]` modules
- Main files: `main.rs` (entry), `lib.rs` (library root), `mod.rs` (module aggregators)

**Functions:**
- Async functions prefixed with `async`: `pub async fn get_limits()`
- Middleware functions use `_middleware` suffix: `auth_middleware`, `rate_limit_middleware`
- Constructor functions conventionally named `new()`: `pub fn new()`
- Builder pattern with `with_*` methods: `.with_model()`, `.with_streaming()`, `.with_external_id()`
- Result-returning functions often use verb patterns: `check_rate_limit()`, `increment_rate_limit()`, `validate_jwt()`

**Variables:**
- Local variables use snake_case: `response`, `token_count`, `external_id`
- Configuration constants in SHOUTY_SNAKE_CASE: `AI_USAGE`, `TEST_ZION_API_KEY`
- Private fields in structs use snake_case: `base_url`, `api_key`, `default_ttl`

**Types:**
- Structs use PascalCase: `AppError`, `RateLimitConfig`, `TokenCounter`, `AuthenticatedUser`
- Enums use PascalCase: `Role`, `CacheBackend`, `AppError` (enum variants)
- Type aliases (generics) use snake_case for traits: `ByteStream = Pin<Box<dyn Stream<...> + Send>>`
- Traits use PascalCase: `AiProvider`

## Code Style

**Formatting:**
- No explicit formatter configured (no `.rustfmt.toml`)
- Standard Rust formatting conventions apply (4-space indentation)
- Uses `cargo fmt` implicitly

**Linting:**
- No explicit linter configuration detected
- Code follows Rust idioms and standard clippy warnings
- Error handling via `?` operator is preferred pattern

**Module Documentation:**
- All modules have `//!` doc comments explaining purpose: "Chat completions endpoint"
- Markdown formatting in doc comments for structure
- Example code blocks using ` ```rust,ignore ` for non-compiled examples
- Design notes for complex modules (e.g., `AiProvider` trait documentation)

## Import Organization

**Order:**
1. Standard library imports (`std::*`)
2. External crate imports (alphabetically): `axum`, `anyhow`, `tokio`, `tracing`, `serde`
3. Relative crate imports: `use crate::error::AppError`

**Path Organization Example:**
```rust
use std::sync::Arc;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use crate::{
    error::AppError,
    middleware::auth::AuthenticatedUser,
    AppState,
};
```

**Path Aliases:**
- No path aliases in Cargo.toml
- Relative imports from crate root using `crate::` prefix throughout
- Common re-exports in `lib.rs`: `pub use crate::cache::{RedisCache, SubscriptionCache}`

## Error Handling

**Patterns:**
- Custom error enum: `AppError` with variants for each error type
- Error conversion via `#[from]` attributes: `RedisError(#[from] redis::RedisError)`
- Result type alias for convenience: `pub type AppResult<T> = Result<T, AppError>`
- Use `?` operator for early returns: `let limits = zion_client.get_limits(&external_id).await?`
- Contextual errors with `anyhow::Context`: `.context("Invalid SENTINEL_PORT")?`
- Status code mapping in `IntoResponse` impl for HTTP responses
- Detailed error responses with error body containing code, message, and optional details

**Error Response Format:**
```rust
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Rate limit exceeded: ...",
    "details": {
      "limit": 100,
      "used": 95,
      "remaining": 5,
      "reset_at": "2024-01-31T12:30:00Z"
    }
  }
}
```

## Logging

**Framework:** `tracing` crate with `tracing-subscriber` for initialization

**Configuration:**
- Initialized in `main.rs` with `tracing_subscriber::fmt()`
- Env filter: `RUST_LOG=sentinel=info,tower_http=info` (default)
- Features: target, thread IDs
- Log levels: `info`, `debug`, `warn`, `error`

**Patterns:**
- `#[instrument]` macro for structured logging on async functions
- Example: `#[instrument(skip_all, fields(path = %request.uri().path()))]`
- Skip sensitive data with `skip` in `#[instrument]`
- Use specific log levels appropriately:
  - `info!()` for high-level operations (startup, shutdown)
  - `debug!()` for request/response details
  - `warn!()` for non-fatal issues (fallback encoder, retries)
  - `error!()` for failures that need attention

**Example Usage:**
```rust
#[instrument(skip(self), fields(external_id = %external_id))]
pub async fn get_limits(&self, external_id: &str) -> AppResult<Vec<UserLimit>> {
    debug!(url = %url, "Fetching user limits from Zion");
    // ... operation ...
    debug!(limits_count = result.data.limits.len(), "Successfully fetched user limits");
}
```

## Comments

**When to Comment:**
- Module-level docs: `//!` for every module explaining purpose
- Item-level docs: `///` for public functions explaining inputs, outputs, and behavior
- Complex logic: inline comments for non-obvious decision points
- Avoid comments that restate code; explain the "why"

**JSDoc/TSDoc:**
- Rust uses `///` for doc comments (JSDoc-equivalent)
- Supports markdown in doc comments
- Examples in doc comments with ` ```rust ` blocks
- Security notes and design decisions documented

**Example:**
```rust
/// Get the provider name for logging and metrics
fn name(&self) -> &'static str;

/// Chat completions (streaming)
///
/// Sends a chat completion request and returns a stream of response chunks.
///
/// # Security
///
/// Implementations MUST never forward client Authorization headers to upstream providers
async fn chat_completions_stream(
    &self,
    request: serde_json::Value,
    incoming_headers: &HeaderMap,
) -> AppResult<ByteStream>;
```

## Function Design

**Size:** No explicit limit, but typical functions 20-50 lines

**Parameters:**
- Use struct types for multiple related parameters
- Derive `Clone` on config/parameter structs for ease of passing
- Accept references (`&str`, `&T`) for borrowed data
- Accept owned values for data that will be stored
- State passed via `State<Arc<AppState>>` in handlers

**Return Values:**
- Use `AppResult<T>` (alias for `Result<T, AppError>`) throughout
- Return early with `?` operator
- Async functions return `impl Future` or directly return `async fn`
- Handlers return `impl IntoResponse` from axum handlers

**Example:**
```rust
pub async fn increment_usage(
    &self,
    email: &str,
    input_tokens: i64,
    output_tokens: i64,
    requests: i64,
) -> AppResult<()> {
    // ... validation and operation ...
    Ok(())
}
```

## Module Design

**Exports:**
- Public items marked with `pub` keyword
- Commonly re-exported in `lib.rs` for consumer convenience
- Module structure mirrors file structure (mod.rs files aggregate submodules)

**Barrel Files:**
- Each directory with multiple related files has `mod.rs` (implicit or explicit)
- Example: `src/routes/mod.rs` aggregates and wires all route handlers
- `lib.rs` aggregates and re-exports main module interfaces

**Example Module Structure:**
```rust
// src/cache/mod.rs
pub mod in_memory;
pub mod redis;
pub mod subscription;

pub use self::redis::RedisCache;
pub use self::subscription::SubscriptionCache;
```

## Trait and Interface Patterns

**Trait Design:**
- Used for provider abstraction: `AiProvider` trait allows pluggable backends
- Uses `async_trait` macro for async trait methods
- `dyn` objects enabled by avoiding generics (use `serde_json::Value` instead)
- Implemented with `#[async_trait]` macro for async function support

**Example:**
```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn chat_completions(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value>;
}
```

## Derive Macros

**Common Derives:**
- `Debug` - for debugging output
- `Clone` - for owned copies (especially configuration)
- `Serialize` - for JSON serialization (serde)
- `Deserialize` - for JSON deserialization (serde)
- `Default` - for default constructor

**Serde Attributes:**
- `#[serde(rename_all = "lowercase")]` for enum variants matching JSON
- `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields
- `#[serde(default)]` for optional request fields with defaults

**Example:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
```

## Configuration Management

**Pattern:** Environment-based configuration in `Config` struct

**Implementation:**
- Single `Config::from_env()` method loads all vars at startup
- Uses `env::var()` with fallback defaults
- `context()` for validation errors
- Optional values use `.ok()` conversion
- Configuration loaded once and shared via `Arc<AppState>`

**Example:**
```rust
pub struct Config {
    pub host: String,
    pub port: u16,
    pub redis_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            host: env::var("SENTINEL_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("SENTINEL_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("Invalid SENTINEL_PORT")?,
        })
    }
}
```

---

*Convention analysis: 2026-01-31*
