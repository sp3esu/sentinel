# External Integrations

**Analysis Date:** 2026-01-31

## APIs & External Services

**Zion Governance API:**
- Service: Zion user management and quota system
- What it's used for:
  - JWT validation for user authentication
  - Fetching user subscription limits (input tokens, output tokens, request count)
  - Reporting AI usage back to governance system
  - Supporting future multi-model limit management
- SDK/Client: Custom HTTP client in `src/zion/client.rs` using Reqwest
- Auth: Bearer token in `Authorization` header, API key in requests
- Configuration: `ZION_API_URL`, `ZION_API_KEY` env vars
- Endpoints used:
  - `GET /api/v1/users/me` - Validate JWT and get user profile
  - `GET /api/v1/limits/external/{externalId}` - Fetch user limits
  - `POST /api/v1/usage/external/increment` - Report individual usage
  - `POST /api/v1/usage/external/batch-increment` - Report batched usage (bulk)

**OpenAI API:**
- Service: LLM provider for chat completions, text completions, and model information
- What it's used for:
  - Processing `/v1/chat/completions` and `/v1/completions` requests
  - Retrieving available models list
  - Token counting via `usage` field in responses
- SDK/Client: Custom HTTP client in `src/proxy/openai.rs` using Reqwest
- Auth: Bearer token with `Authorization: Bearer {OPENAI_API_KEY}` header
- Configuration: `OPENAI_API_URL` (default: `https://api.openai.com/v1`), `OPENAI_API_KEY` env var
- Features:
  - Streaming support (Server-Sent Events)
  - Non-streaming JSON responses
  - Request forwarding with whitelist-based header filtering
  - Secure header handling (see `src/proxy/headers.rs` - never forwards client JWT)

## Data Storage

**Databases:**
- Redis 7.x
  - Connection: `redis://localhost:6379` (or env var `REDIS_URL`)
  - Client: Redis Tokio AsyncCommands via `redis` crate v0.24
  - Connection pooling: `redis::aio::ConnectionManager` for efficient reuse
  - Usage:
    - Rate limiting counters (sliding window algorithm)
    - User subscription limits cache with TTL
    - JWT validation cache (hashed tokens) with TTL
    - Failed usage increments queue for retry mechanism

**File Storage:**
- Local filesystem only - No S3 or external file storage
- Application binaries and code only (no user data)
- Container volume mounts: `./src:/app/src` (dev), source code mounting

**Caching:**
- Redis (primary) - In-memory distributed cache with TTL
  - Limit cache TTL: `CACHE_TTL_SECONDS` (default: 300s)
  - JWT cache TTL: `JWT_CACHE_TTL_SECONDS` (default: 300s)
- In-memory cache (test only) - `src/cache/in_memory.rs` for unit/integration tests
  - Implementation: `InMemoryCache` struct using Arc<DashMap>
  - No persistence, used only when `test-utils` feature enabled

## Authentication & Identity

**Auth Provider:**
- Type: JWT-based with Zion validation (external provider)
- Implementation approach:
  - Extracts JWT from `Authorization: Bearer {token}` header
  - Hashes JWT with SHA256 for cache key (never stores raw token)
  - Checks Redis cache first (JWT validation results)
  - On cache miss, validates via `GET /api/v1/users/me` to Zion API
  - Caches successful validation with `JWT_CACHE_TTL_SECONDS` TTL
  - Extracts `external_id`, `user_id`, and `email` from user profile
- Middleware: `src/middleware/auth.rs`
  - Validates every request before routing to handlers
  - Returns 401 Unauthorized if JWT missing/invalid
  - Returns 403 Forbidden if JWT validation fails
  - Stores `AuthenticatedUser` in request extensions

**JWT Handling:**
- No key storage in Sentinel - Zion is source of truth
- Cache key format: SHA256(jwt_token) in hex format
- No token refresh logic (Zion handles this)

## Monitoring & Observability

**Error Tracking:**
- Structured logging via `tracing` crate
- Errors logged with context fields (request IDs, user IDs, etc.)
- No external error tracking service (Sentry, Datadog, etc.)
- Error types: Custom `AppError` enum in `src/error.rs` with HTTP status codes

**Logs:**
- Output: Standard output (JSON format via `tracing-subscriber` with `json` feature)
- Levels: Configurable via `RUST_LOG` env var
- Default: `sentinel=info,tower_http=info`
- Destination: Application stdout (collected by container/deployment platform)
- Instrumentation:
  - Request-level: trace_id (UUID), path, method, status
  - Service-level: Zion API calls, OpenAI API calls, Redis operations
  - Async tasks: Batching tracker, circuit breaker decisions

**Metrics:**
- Framework: Prometheus metrics via `metrics` crate v0.22
- Exporter: `metrics-exporter-prometheus` v0.13
- Endpoint: `GET /metrics` in Prometheus text format
- Metrics tracked:
  - `sentinel_requests_total` (counter) - Total requests
  - `sentinel_tokens_processed_total` (counter) - Input + output tokens
  - `sentinel_cache_operations_total` (counter) - Cache hits/misses
  - `sentinel_request_duration_seconds` (histogram) - Request latency
  - `sentinel_active_connections` (gauge) - Current connections
  - `sentinel_token_estimation_diff` (histogram) - Estimated vs actual token diff
  - `sentinel_token_estimation_diff_pct` (histogram) - Percentage difference
  - `sentinel_sse_parse_errors_total` (counter) - Streaming parse errors
  - Custom metrics for rate limiting, circuit breaker state

## CI/CD & Deployment

**Hosting:**
- Docker container (primary deployment method)
- Multi-stage build: builder stage (Rust 1.83), runtime stage (Debian Bookworm)
- Container orchestration: Kubernetes-ready with health check endpoints
- VPS/Cloud platforms: Compatible with any Docker-capable environment

**CI Pipeline:**
- Not detected in repository (no GitHub Actions, GitLab CI, etc.)
- Build process: Manual Docker build via `docker build` command
- Deployment: Manual via deployment scripts (`deployment/scripts/`)

**Deployment Targets:**
- Docker container deployments
- Kubernetes (via health endpoints: `/health`, `/health/ready`, `/health/live`)
- Traditional VPS with Docker runtime
- Cloudflare as reverse proxy (from README architecture diagram)

## Environment Configuration

**Required env vars:**
- `ZION_API_URL` - Zion API base URL (no default)
- `ZION_API_KEY` - Zion API authentication key (no default)
- `OPENAI_API_KEY` - OpenAI API key (required, panics if missing on startup)

**Critical optional env vars:**
- `REDIS_URL` - Redis connection string (default: `redis://localhost:6379`)
- `SENTINEL_HOST` - Bind address (default: `0.0.0.0`)
- `SENTINEL_PORT` - Listen port (default: `8080`)

**Secrets location:**
- Environment variables (sourced from):
  - `.env` file (local development, not committed)
  - Docker secrets/ConfigMaps (Kubernetes)
  - Docker environment variables (Docker Compose)
  - Shell exports (direct deployment)
- Sentinel does NOT store secrets in files
- API keys passed via env vars only

**Config Management:**
- Loaded at startup via `Config::from_env()` in `src/config.rs`
- Using `dotenvy` crate to load `.env` files
- Validated at application initialization
- Immutable after server start (Arc<Config> shared across all requests)

## Webhooks & Callbacks

**Incoming Webhooks:**
- Not detected - Sentinel only makes outbound requests
- All authentication is request-based (Authorization header with Zion JWT)

**Outgoing Callbacks:**
- Zion usage reporting: `POST /api/v1/usage/external/increment` (blocking)
- Zion batch usage reporting: `POST /api/v1/usage/external/batch-increment` (batched, fire-and-forget)
- No other outbound webhooks

**Usage Reporting Flow:**
1. Request processed successfully
2. Tokens counted (input + output)
3. Non-streaming: Sync report via `UsageTracker` to Zion (blocking)
4. Streaming: Accumulate during stream, report on completion
5. Batching tracker (background): Aggregates multiple user increments
   - Flushes when: batch reaches 100 items OR 500ms elapsed
   - Rate limited: Max 20 requests/second to Zion
   - Circuit breaker: Opens after 3 consecutive failures, resets after 30s
   - Retry: Failed increments persisted in Redis, retried every 60s

## Integration Patterns

**HTTP Client Pooling:**
- Single shared `reqwest::Client` across all handlers
- Connection pooling: 100 idle connections per host
- Request timeout: 300 seconds (5 minutes)
- TLS: rustls (no OpenSSL dependency)

**Rate Limiting (Internal):**
- Algorithm: Sliding window via Redis
- Enforced in `src/middleware/rate_limiter.rs`
- Returns 429 Too Many Requests with `X-RateLimit-*` headers
- Limits fetched from Zion and cached with TTL

**Circuit Breaker:**
- Location: `src/usage/batching.rs`
- Used for: Zion API calls (rate limiting + failure handling)
- Threshold: 3 consecutive failures opens circuit
- Reset time: 30 seconds before attempting recovery
- Fallback: Failed increments queued in Redis for retry

**Caching Strategy:**
- Limits: 5 minutes (configurable via `CACHE_TTL_SECONDS`)
- JWT validation: 5 minutes (configurable via `JWT_CACHE_TTL_SECONDS`)
- Cache keys:
  - Limits: `limits:{externalId}`
  - JWT validation: `jwt:{sha256(token)}`

---

*Integration audit: 2026-01-31*
