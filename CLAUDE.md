# CLAUDE.md - Sentinel AI Proxy

This document provides context for Claude Code when working with the Sentinel codebase.

## Project Overview

Sentinel is a high-performance AI proxy written in Rust that:
- Provides OpenAI-compatible API endpoints for chat applications
- Implements traffic limiting based on user subscriptions from Zion governance system
- Routes requests through Vercel AI Gateway to LLM providers
- Caches user limits in Redis with TTL-based invalidation
- Counts tokens using tiktoken-rs for accurate usage tracking

## Architecture

```
Client (with Zion JWT)
        │
        ▼
┌───────────────────┐
│   Sentinel Proxy  │
│  ┌─────────────┐  │
│  │ Auth Layer  │──┼──▶ Zion API (JWT validation, /api/v1/users/me)
│  ├─────────────┤  │
│  │Rate Limiter │──┼──▶ Redis (sliding window counters)
│  ├─────────────┤  │
│  │Token Counter│  │
│  ├─────────────┤  │
│  │   Proxy     │──┼──▶ Vercel AI Gateway (OpenAI)
│  └─────────────┘  │
│         │         │
│         ▼         │
│      Redis        │◀── User limits cache (5-min TTL)
└───────────────────┘
        │
        ▼
Zion API (usage increment, /api/v1/usage/external/increment)
```

## Key Files and Modules

### Entry Points
- `src/main.rs` - Application entry, server startup, graceful shutdown
- `src/routes/mod.rs` - Router configuration, all endpoint wiring

### API Routes (`src/routes/`)
- `chat.rs` - `POST /v1/chat/completions` (streaming + non-streaming)
- `completions.rs` - `POST /v1/completions` (legacy endpoint)
- `models.rs` - `GET /v1/models`, `GET /v1/models/:id`
- `health.rs` - Health probes: `/health`, `/health/ready`, `/health/live`
- `metrics.rs` - Prometheus metrics endpoint: `GET /metrics`

### Middleware (`src/middleware/`)
- `auth.rs` - JWT validation via Zion, extracts `AuthenticatedUser`
- `rate_limiter.rs` - Sliding window rate limiting using Redis

### External Integrations
- `src/zion/client.rs` - Zion API client for limits and usage
- `src/zion/models.rs` - Zion data types (UserLimit, UserProfile, etc.)
- `src/proxy/vercel_gateway.rs` - Vercel AI Gateway client with streaming

### Caching (`src/cache/`)
- `redis.rs` - Generic Redis cache with TTL
- `subscription.rs` - Subscription-aware cache (limits, JWT validation)

### Core Services
- `src/tokens/counter.rs` - Token counting with tiktoken-rs
- `src/usage/tracker.rs` - Usage tracking and batch increments
- `src/config.rs` - Environment-based configuration
- `src/error.rs` - Error types with proper HTTP status codes

## Common Tasks

### Running Locally

```bash
# Start Redis and build container
docker-compose up -d redis
./run_dev.sh

# Or run directly (requires Redis running)
cargo run
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test tokens::
cargo test zion::

# Run with output
cargo test -- --nocapture
```

### Building for Production

```bash
docker build -t sentinel .
```

## Environment Variables

Required:
- `ZION_API_URL` - Zion governance API base URL
- `ZION_API_KEY` - API key for Zion external endpoints
- `VERCEL_AI_GATEWAY_API_KEY` - Vercel AI Gateway authentication

Optional (with defaults):
- `SENTINEL_HOST` (default: `0.0.0.0`)
- `SENTINEL_PORT` (default: `8080`)
- `REDIS_URL` (default: `redis://localhost:6379`)
- `VERCEL_AI_GATEWAY_URL` (default: `https://api.vercel.ai/v1`)
- `CACHE_TTL_SECONDS` (default: `300`)
- `JWT_CACHE_TTL_SECONDS` (default: `300`)
- `RUST_LOG` (default: `sentinel=info,tower_http=info`)

## API Endpoints

### OpenAI-Compatible
- `POST /v1/chat/completions` - Chat completion (supports streaming)
- `POST /v1/completions` - Text completion (supports streaming)
- `GET /v1/models` - List available models
- `GET /v1/models/:id` - Get specific model

### Health & Monitoring
- `GET /health` - Full health check with dependency status
- `GET /health/ready` - Kubernetes readiness probe
- `GET /health/live` - Kubernetes liveness probe
- `GET /metrics` - Prometheus-compatible metrics

## Authentication Flow

1. Client sends `Authorization: Bearer <zion-jwt>` header
2. Sentinel hashes JWT and checks Redis cache
3. On cache miss, validates via Zion `GET /api/v1/users/me`
4. Extracts `external_id` from user profile
5. Uses `external_id` for all Zion external API calls

## Rate Limiting

Uses sliding window algorithm in Redis:
- Window size configurable per limit
- Atomic operations with MULTI/EXEC
- Returns proper 429 response with `X-RateLimit-*` headers

## Token Counting

- Pre-request: Estimates prompt tokens before forwarding
- Post-response: Uses OpenAI `usage` field when available
- Streaming: Uses `stream_options.include_usage=true`
- Fallback: Counts locally with tiktoken-rs

## Zion Integration

### Required Limits in Zion
| Name | Description |
|------|-------------|
| `ai_input_tokens` | Input/prompt tokens |
| `ai_output_tokens` | Output/completion tokens |
| `ai_requests` | Total API request count |

### API Endpoints Used
- `GET /api/v1/limits/external/{externalId}` - Fetch user limits
- `POST /api/v1/usage/external/increment` - Increment usage
- `GET /api/v1/users/me` - Validate JWT and get user profile

## Code Patterns

### Error Handling
```rust
use crate::error::{AppError, AppResult};

// Return errors using ? operator
let limits = zion_client.get_limits(&external_id).await?;

// Or return specific errors
return Err(AppError::RateLimitExceeded { ... });
```

### State Access
```rust
use std::sync::Arc;
use axum::extract::State;
use crate::AppState;

pub async fn handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Access state.redis, state.config, state.zion_client, etc.
}
```

### Streaming Responses
```rust
use axum::response::sse::{Event, Sse};
use futures::stream::Stream;

pub async fn stream_handler() -> Sse<impl Stream<Item = ...>> {
    // Return SSE stream
}
```

## Testing

### Unit Tests
Located in each module as `#[cfg(test)] mod tests { ... }`

### Integration Tests
Located in `tests/integration/`:
- `health.rs` - Health endpoint tests
- `chat_completions.rs` - Chat API tests
- `models.rs` - Models endpoint tests

### Mocks
Located in `tests/mocks/`:
- `zion.rs` - Mock Zion API server (wiremock)
- `vercel_gateway.rs` - Mock AI Gateway
- `redis.rs` - Redis test helpers

## Performance Notes

- HTTP client uses connection pooling (100 idle connections per host)
- Redis uses connection manager for efficient reuse
- Token counter caches encoders per model
- All I/O is async with Tokio runtime
- Graceful shutdown handles SIGTERM/SIGINT

## Debugging

```bash
# Enable debug logging
RUST_LOG=sentinel=debug,tower_http=trace cargo run

# Check Redis cache
redis-cli GET "limits:user123"
redis-cli GET "jwt:$(echo -n 'your-jwt' | sha256sum | cut -d' ' -f1)"
```
