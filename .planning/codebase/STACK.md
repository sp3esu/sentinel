# Technology Stack

**Analysis Date:** 2026-01-31

## Languages

**Primary:**
- Rust 1.83 - Core proxy implementation, all business logic and API handlers

**Configuration & Scripts:**
- TOML - Cargo manifest and configuration
- Shell (Bash) - Deployment and development helper scripts

## Runtime

**Environment:**
- Tokio 1.x - Async runtime for handling concurrent requests
- Rust standard library (Edition 2021)

**Package Manager:**
- Cargo - Rust's dependency manager and build system
- Lockfile: `Cargo.lock` present (committed for reproducible builds)

## Frameworks

**Core Web Framework:**
- Axum 0.7 - High-performance async HTTP framework with middleware support
  - Features: macros, routing, middleware, graceful shutdown
  - Used for: All API endpoints (`/v1/chat/completions`, `/v1/completions`, `/v1/models`, `/health`, `/metrics`)

**Async & Concurrency:**
- Tokio 1 - Async runtime (full features: all runtime types, signal handling, timers)
- Async-trait 0.1 - Trait support for async methods
- Async-stream 0.3 - Stream utilities for SSE responses
- Futures 0.3 - Future combinators and utilities

**HTTP & Networking:**
- Reqwest 0.12 - HTTP client with connection pooling (100 idle conns per host)
  - Features: JSON serialization, streaming responses, rustls-tls, no default features
  - Used for: Zion API calls, OpenAI API calls, external service communication
- Tower 0.4 - Middleware and composable services framework
- Tower-HTTP 0.5 - HTTP-specific middleware
  - Features: CORS, request tracing, gzip compression, timeout handling

**Testing:**
- Wiremock 0.6 - Mock HTTP server for integration testing
- Axum-test 14 - Testing utilities for Axum applications
- Tokio-test 0.4 - Tokio runtime testing helpers
- Pretty_assertions 1 - Enhanced assertion output

## Key Dependencies

**Critical:**
- Redis 0.24 - Session/state caching and rate limiting with connection manager
  - Features: tokio async support, connection pooling, Lua scripting support
  - Why: Caches user subscription limits and JWT validation (5-min TTL), stores rate limit counters

- Tiktoken-rs 0.5 - Token counting for OpenAI models
  - Why: Accurate token estimation before/after OpenAI requests, used for quota tracking and usage reporting

**Serialization & Data:**
- Serde 1 - Data serialization framework with derive macros
- Serde_json 1 - JSON serialization/deserialization
- Chrono 0.4 - DateTime handling with serde support (ISO 8601 timestamps)
- Bytes 1 - Efficient byte buffer handling for streaming responses
- HTTP-body-util 0.1 - HTTP body utilities

**Infrastructure & Observability:**
- Metrics 0.22 - Prometheus metrics collection
- Metrics-exporter-prometheus 0.13 - Prometheus export format (HTTP endpoint at `/metrics`)
- Tracing 0.1 - Structured logging and distributed tracing framework
- Tracing-subscriber 0.3 - Tracing output formatting
  - Features: JSON format output, env-filter for log levels, thread IDs
  - Log levels configurable via `RUST_LOG` env var

**Error Handling & Utilities:**
- Thiserror 1 - Custom error type derivation
- Anyhow 1 - Flexible error handling for main application
- Once_cell 1 - Lazy static initialization (used for Prometheus metrics singleton)
- UUID 1 - Unique request IDs with v4 generation and serde support
- Hex 0.4 - Hex encoding for JWT hashing (SHA256 cache keys)
- Sha2 0.10 - Cryptographic hashing (SHA256 for JWT caching)

**Rate Limiting & Resilience:**
- Governor 0.6 - Efficient rate limiting (sliding window algorithm in Zion API calls)
- Failsafe 1.2 - Circuit breaker pattern for Zion API calls
- Nonzero_ext 0.3 - NonZero type utilities

**Configuration:**
- Dotenvy 0.15 - Environment variable loading from `.env` files
  - Used at startup: `dotenvy::dotenv().ok()` in `src/main.rs`

## Configuration

**Environment Variables:**

Required:
- `ZION_API_URL` - Zion governance API base URL (e.g., `http://localhost:3000`)
- `ZION_API_KEY` - API key for Zion external service endpoints

Optional (with defaults):
- `SENTINEL_HOST` (default: `0.0.0.0`)
- `SENTINEL_PORT` (default: `8080`)
- `REDIS_URL` (default: `redis://localhost:6379`)
- `OPENAI_API_URL` (default: `https://api.openai.com/v1`)
- `OPENAI_API_KEY` (required for production, optional for testing)
- `CACHE_TTL_SECONDS` (default: `300` - user limits cache duration)
- `JWT_CACHE_TTL_SECONDS` (default: `300` - JWT validation cache duration)
- `RUST_LOG` (default: `sentinel=info,tower_http=info` - logging levels)
- `SENTINEL_DEBUG` (default: `false` - enables debug endpoints)

**Environment Files:**
- `.env` - Local development (Redis on localhost)
- `.env.docker` - Docker Compose development (Redis service name resolution)
- `.env.prod` - Production configuration template
- `.env.example` - Template for new environments

**Build Configuration:**

Release profile optimizations (`Cargo.toml`):
```
[profile.release]
lto = true                # Link-time optimization
codegen-units = 1         # Single codegen unit for max optimization
panic = "abort"           # Abort on panic (smaller binary)
strip = true              # Strip debug symbols
```

## Platform Requirements

**Development:**
- Rust 1.83+ (specified in Dockerfile)
- Cargo (comes with Rust)
- Redis 7.x (for local testing)
- Docker & Docker Compose (optional, for containerized development)
- Linux/macOS (development tested on these; Windows would need WSL2 for Docker)

**Production:**
- Debian Bookworm or compatible Linux distribution
- Docker container runtime (multi-stage build generates `debian:bookworm-slim` runtime)
- Redis 7.x instance (separate from application)
- 512MB+ RAM minimum (Rust binary with full runtime)
- Network access to:
  - Zion API (`ZION_API_URL`)
  - OpenAI API (`https://api.openai.com/v1` or custom endpoint)
  - Redis instance (`REDIS_URL`)

**Container Requirements:**
- Docker base: `rust:1.83-slim` for builder, `debian:bookworm-slim` for runtime
- Non-root user: `sentinel` (UID/GID auto-created)
- Exposed port: `8080` (configurable via env var)
- Health check: Curl probe on `http://localhost:8080/health/live` (30s intervals)

## Language-Specific Features

**Async/Await:**
- All I/O is non-blocking (Redis, HTTP clients, streaming responses)
- Uses Tokio task spawning for background work (batching tracker, circuit breaker)
- Graceful shutdown handling via SIGTERM/SIGINT signals

**Error Handling:**
- Custom `AppError` enum in `src/error.rs` with HTTP status code mapping
- Question mark operator (`?`) for propagating errors
- Structured error context with tracing instrumentation

**Macros:**
- `#[tokio::main]` - Main async entrypoint
- `#[instrument]` - Tracing instrumentation with field extraction
- `#[derive]` - Serde serialization, custom error types
- `#[cfg(test)]` - Test module conditional compilation

---

*Stack analysis: 2026-01-31*
