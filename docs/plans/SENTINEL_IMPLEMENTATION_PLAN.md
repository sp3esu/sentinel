# Sentinel AI Proxy Implementation Plan

> **Note**: This plan will be saved to `docs/plans/SENTINEL_IMPLEMENTATION_PLAN.md` during Phase 1, and archived to `docs/archive/plans/` after completion.

## Overview
Build a high-performance AI proxy application that implements traffic limiting based on user subscriptions managed in an external system (Zion).

## Technology Choices (Confirmed)
- **Language**: Rust
- **Web Framework**: Axum (Tower middleware ecosystem)
- **Cache**: Redis (for local caching of user limits)
- **Initial AI Provider**: OpenAI via Vercel AI Gateway
- **Containerization**: Docker from day one
- **Deployment**: VPS behind Cloudflare

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Cloudflare                               │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Docker Host (VPS)                           │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Sentinel Proxy                        │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐ │   │
│  │  │  Auth    │  │  Rate    │  │  Token   │  │ Request │ │   │
│  │  │Middleware│─▶│ Limiter  │─▶│ Counter  │─▶│ Forward │ │   │
│  │  └──────────┘  └──────────┘  └──────────┘  └─────────┘ │   │
│  │        │              │                          │      │   │
│  │        ▼              ▼                          ▼      │   │
│  │  ┌──────────────────────────────────────────────────┐  │   │
│  │  │              Redis (Cache Layer)                  │  │   │
│  │  │  - User subscription cache                        │  │   │
│  │  │  - Rate limit counters                            │  │   │
│  │  │  - Token usage tracking                           │  │   │
│  │  └──────────────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────┘   │
│                           │                                      │
└───────────────────────────┼──────────────────────────────────────┘
                            │
            ┌───────────────┴───────────────┐
            ▼                               ▼
    ┌──────────────┐               ┌──────────────────┐
    │  Zion API    │               │ Vercel AI Gateway│
    │ (Governance) │               │    (OpenAI)      │
    └──────────────┘               └──────────────────┘
```

## Zion API Integration (External Service Endpoints)

**Authentication**: API Key via `x-api-key` header

### Key Endpoints for Sentinel:

1. **Get User Limits** - `GET /api/v1/limits/external/{identifier}`
   - Returns: `{ userId, externalId, limits: UserLimit[] }`

2. **Increment Usage** - `POST /api/v1/usage/external/increment`
   - Body: `{ externalId: string, limitName: string, amount?: number }`
   - Returns: `UserLimit`

### UserLimit Schema:
```json
{
  "limitId": "string",
  "name": "string",           // e.g., "api_tokens", "api_requests"
  "displayName": "string",
  "unit": "string|null",      // e.g., "tokens", "requests"
  "limit": 10000,             // max allowed
  "used": 500,                // current usage
  "remaining": 9500,          // limit - used
  "resetPeriod": "DAILY|WEEKLY|MONTHLY|NEVER",
  "periodStart": "ISO8601",
  "periodEnd": "ISO8601"
}
```

### Authentication: Zion JWT Passthrough
Users authenticate to Sentinel using their Zion JWT. Sentinel validates the JWT and extracts user info.

**JWT Validation Flow:**
1. Client sends `Authorization: Bearer <zion-jwt>` header
2. Sentinel caches validated JWTs in Redis (keyed by JWT hash, 5-min TTL)
3. On cache miss, validate via `GET /api/v1/users/me` with the JWT
4. Extract `externalId` from user profile for Zion external API calls

### Integration Flow:
1. Client sends request to Sentinel with Zion JWT in Authorization header
2. Sentinel validates JWT (cache or Zion API call)
3. Extract user's externalId from cached user profile
4. Check Redis cache for user limits (cache hit = fast path)
5. If cache miss, fetch from Zion `/api/v1/limits/external/{externalId}`
6. Cache result in Redis with 5-minute TTL
7. Check if user has remaining quota for the requested operation
8. If quota exceeded, return 429 with limit info
9. Forward request to Vercel AI Gateway if allowed
10. Count tokens from response (streaming or non-streaming)
11. Increment usage in Zion via `/api/v1/usage/external/increment`
12. Update local Redis cache with new usage

## Implementation Phases

### Phase 1: Project Setup & Infrastructure
**Agent**: general-purpose
**Tasks**:
1. Initialize Rust project with Cargo
2. Set up project structure
3. Create Docker configuration (Dockerfile, docker-compose.yml)
4. Create `run_dev.sh` script
5. Set up Redis container for development
6. Configure CI basics

**Files to create**:
- `Cargo.toml`
- `src/main.rs`
- `Dockerfile`
- `docker-compose.yml`
- `run_dev.sh`
- `.gitignore`
- `.env.example`

### Phase 2: Core Proxy Infrastructure
**Agent**: general-purpose
**Tasks**:
1. Implement Axum server with health endpoints:
   - `GET /health` - Basic health with dependency status
   - `GET /health/ready` - Readiness probe
   - `GET /health/live` - Liveness probe
   - `GET /metrics` - Prometheus-compatible metrics
2. Set up Tower middleware stack
3. Implement request/response logging with tracing
4. Set up Prometheus metrics collection
5. Create OpenAI-compatible API routes:
   - `POST /v1/chat/completions`
   - `POST /v1/completions`
   - `GET /v1/models`
6. Implement request forwarding to Vercel AI Gateway

**Files to create**:
- `src/routes/mod.rs`
- `src/routes/health.rs`
- `src/routes/metrics.rs`
- `src/routes/chat.rs`
- `src/routes/completions.rs`
- `src/routes/models.rs`
- `src/middleware/mod.rs`
- `src/proxy/mod.rs`
- `src/proxy/vercel_gateway.rs`

### Phase 3: Zion Integration & Caching
**Agent**: general-purpose
**Tasks**:
1. Implement Zion API client
2. Create user subscription models
3. Implement Redis caching layer
4. Set up cache invalidation strategy (TTL-based)
5. Create background sync for subscription updates

**Files to create**:
- `src/zion/mod.rs`
- `src/zion/client.rs`
- `src/zion/models.rs`
- `src/cache/mod.rs`
- `src/cache/redis.rs`
- `src/cache/subscription.rs`

### Phase 4: Rate Limiting & Token Counting
**Agent**: general-purpose
**Tasks**:
1. Implement token counting using tiktoken-rs
2. Handle streaming response token counting
3. Implement rate limiting middleware
4. Create usage tracking per user
5. Implement limit enforcement based on subscription tiers

**Files to create**:
- `src/tokens/mod.rs`
- `src/tokens/counter.rs`
- `src/middleware/rate_limiter.rs`
- `src/middleware/auth.rs`
- `src/usage/mod.rs`
- `src/usage/tracker.rs`

### Phase 5: Testing
**Agent**: general-purpose
**Tasks**:
1. Unit tests for all modules
2. Integration tests for API endpoints
3. Load testing setup
4. Mock Zion API for testing
5. Mock Vercel AI Gateway for testing

**Target**: High code coverage (>80%)

**Files to create**:
- `tests/integration/mod.rs`
- `tests/integration/chat_completions.rs`
- `tests/integration/rate_limiting.rs`
- `src/*/tests.rs` (inline unit tests)
- `tests/mocks/mod.rs`

### Phase 6: Documentation & Finalization
**Agent**: general-purpose
**Tasks**:
1. Create `CLAUDE.md` for Claude Code
2. Write comprehensive `README.md`
3. Document Zion integration in `docs/integrations/zion.md`
4. Document Vercel AI Gateway integration in `docs/integrations/vercel-ai-gateway.md`
5. Archive plan to `docs/archive/plans/`

**Files to create**:
- `CLAUDE.md`
- `README.md`
- `docs/integrations/zion.md`
- `docs/integrations/vercel-ai-gateway.md`
- `docs/archive/plans/SENTINEL_IMPLEMENTATION_PLAN.md`

## Key Dependencies (Rust Crates)
```toml
[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace", "compression"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json", "stream"] }
redis = { version = "0.24", features = ["tokio-comp"] }
tiktoken-rs = "0.5"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
thiserror = "1"
anyhow = "1"
dotenvy = "0.15"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }

# Metrics & Monitoring
metrics = "0.22"
metrics-exporter-prometheus = "0.13"

# Async utilities
futures = "0.3"
async-trait = "0.1"

[dev-dependencies]
tokio-test = "0.4"
wiremock = "0.6"
```

## Token Counting Strategy
1. **Pre-request**: Estimate prompt tokens using tiktoken-rs before forwarding
2. **Post-response**:
   - For non-streaming: Use `usage` field from OpenAI response
   - For streaming: Use `stream_options={"include_usage": true}` to get usage in final chunk
3. **Fallback**: Count completion tokens locally if API doesn't return usage

## Caching Strategy
- **Subscription data**: Cache with 5-minute TTL
- **Rate limit counters**: Sliding window in Redis
- **Cache invalidation**: TTL-based (simple) + webhook from Zion (if available)

## Git Workflow
- Each phase will be committed separately
- Meaningful commit messages
- All changes committed before moving to next phase

## Commit Checkpoints
1. `feat: initialize project structure and Docker setup`
2. `feat: implement core Axum server and OpenAI-compatible routes`
3. `feat: add Zion API integration and Redis caching`
4. `feat: implement rate limiting and token counting`
5. `test: add comprehensive test suite`
6. `docs: add CLAUDE.md, README, and integration documentation`

## Required Zion Configuration

The following limit definitions should exist in Zion for token tracking:

| Limit Name | Display Name | Unit | Description |
|------------|--------------|------|-------------|
| `ai_input_tokens` | AI Input Tokens | tokens | Prompt/input tokens consumed |
| `ai_output_tokens` | AI Output Tokens | tokens | Completion/output tokens consumed |
| `ai_requests` | AI Requests | requests | Total API requests made |

## Environment Variables

```bash
# Sentinel Configuration
SENTINEL_HOST=0.0.0.0
SENTINEL_PORT=8080
RUST_LOG=sentinel=debug,tower_http=debug

# Redis
REDIS_URL=redis://localhost:6379

# Zion Integration
ZION_API_URL=http://localhost:3000
ZION_API_KEY=your-api-key-here

# Vercel AI Gateway
VERCEL_AI_GATEWAY_API_KEY=your-vercel-key
VERCEL_AI_GATEWAY_URL=https://api.vercel.com/ai-gateway
```

## Docker Configuration

### Development (docker-compose.yml)
```yaml
services:
  sentinel:
    build: .
    ports:
      - "8080:8080"
    environment:
      - REDIS_URL=redis://redis:6379
      - ZION_API_URL=${ZION_API_URL}
      - ZION_API_KEY=${ZION_API_KEY}
    depends_on:
      - redis

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data

volumes:
  redis_data:
```

### Production (Dockerfile)
```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/sentinel /usr/local/bin/
EXPOSE 8080
CMD ["sentinel"]
```

## Project Structure

```
sentinel/
├── Cargo.toml
├── Cargo.lock
├── Dockerfile
├── docker-compose.yml
├── run_dev.sh
├── .env.example
├── .gitignore
├── CLAUDE.md
├── README.md
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs           # Configuration management
│   ├── error.rs            # Error types
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── health.rs       # /health, /health/ready, /health/live
│   │   ├── metrics.rs      # GET /metrics (Prometheus)
│   │   ├── chat.rs         # POST /v1/chat/completions
│   │   ├── completions.rs  # POST /v1/completions
│   │   └── models.rs       # GET /v1/models
│   ├── middleware/
│   │   ├── mod.rs
│   │   ├── auth.rs         # Authentication middleware
│   │   └── rate_limiter.rs # Rate limiting middleware
│   ├── proxy/
│   │   ├── mod.rs
│   │   └── vercel_gateway.rs
│   ├── zion/
│   │   ├── mod.rs
│   │   ├── client.rs       # Zion API client
│   │   └── models.rs       # Zion data models
│   ├── cache/
│   │   ├── mod.rs
│   │   └── redis.rs        # Redis cache implementation
│   ├── tokens/
│   │   ├── mod.rs
│   │   └── counter.rs      # Token counting (tiktoken-rs)
│   └── usage/
│       ├── mod.rs
│       └── tracker.rs      # Usage tracking service
├── tests/
│   ├── integration/
│   │   ├── mod.rs
│   │   ├── chat_completions.rs
│   │   └── rate_limiting.rs
│   └── common/
│       └── mod.rs          # Test utilities
└── docs/
    ├── integrations/
    │   ├── zion.md
    │   └── vercel-ai-gateway.md
    └── archive/
        └── plans/
```

## Health & Monitoring Endpoints

### Health Check Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `GET /health` | GET | Basic liveness check (for k8s/docker) |
| `GET /health/ready` | GET | Readiness check (dependencies ready) |
| `GET /health/live` | GET | Liveness check (app is running) |
| `GET /metrics` | GET | Prometheus-compatible metrics |

### Health Response Schema

```json
{
  "status": "healthy|degraded|unhealthy",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "timestamp": "2024-01-15T10:30:00Z",
  "checks": {
    "redis": {
      "status": "healthy",
      "latency_ms": 2
    },
    "zion_api": {
      "status": "healthy",
      "latency_ms": 45
    },
    "vercel_gateway": {
      "status": "healthy",
      "latency_ms": 120
    }
  },
  "stats": {
    "total_requests": 150000,
    "active_connections": 42,
    "requests_per_minute": 250,
    "cache_hit_rate": 0.95,
    "avg_response_time_ms": 180
  }
}
```

### Prometheus Metrics (GET /metrics)

```
# HELP sentinel_requests_total Total number of requests processed
# TYPE sentinel_requests_total counter
sentinel_requests_total{status="success"} 149000
sentinel_requests_total{status="rate_limited"} 800
sentinel_requests_total{status="error"} 200

# HELP sentinel_request_duration_seconds Request duration histogram
# TYPE sentinel_request_duration_seconds histogram
sentinel_request_duration_seconds_bucket{le="0.1"} 50000
sentinel_request_duration_seconds_bucket{le="0.5"} 120000
sentinel_request_duration_seconds_bucket{le="1.0"} 145000

# HELP sentinel_tokens_processed_total Total tokens processed
# TYPE sentinel_tokens_processed_total counter
sentinel_tokens_processed_total{type="input"} 5000000
sentinel_tokens_processed_total{type="output"} 3000000

# HELP sentinel_cache_hits_total Cache hit/miss counter
# TYPE sentinel_cache_hits_total counter
sentinel_cache_hits_total{result="hit"} 142000
sentinel_cache_hits_total{result="miss"} 8000

# HELP sentinel_active_connections Current active connections
# TYPE sentinel_active_connections gauge
sentinel_active_connections 42

# HELP sentinel_redis_connection_pool_size Redis connection pool size
# TYPE sentinel_redis_connection_pool_size gauge
sentinel_redis_connection_pool_size 10
```

### Monitoring Integration

The health endpoints enable integration with:
- **Prometheus + Grafana**: Scrape `/metrics` endpoint
- **Docker Healthcheck**: Use `/health/live` endpoint
- **Kubernetes**: Liveness probe on `/health/live`, readiness on `/health/ready`
- **Cloudflare Health Checks**: Configure on `/health` endpoint
- **Uptime monitoring** (Uptime Robot, Pingdom): Use `/health` endpoint

### Additional Rust Dependencies for Monitoring

```toml
metrics = "0.22"
metrics-exporter-prometheus = "0.13"
```

## Performance Considerations

1. **Connection Pooling**: Use connection pools for Redis and HTTP clients
2. **Async Everything**: Full async/await with Tokio runtime
3. **Zero-Copy**: Minimize memory allocations in hot paths
4. **Request Pipelining**: Pipeline Redis commands where possible
5. **Streaming**: Support streaming responses for chat completions
6. **Graceful Shutdown**: Handle SIGTERM for clean container restarts

## Security Considerations

1. **API Key Validation**: Validate all incoming API keys
2. **Rate Limiting**: Protect against abuse at multiple layers
3. **Input Validation**: Sanitize all user inputs
4. **Secrets Management**: Never log API keys or sensitive data
5. **TLS**: Cloudflare handles TLS termination

## Agents to Execute Work

| Phase | Agent Type | Reason |
|-------|------------|--------|
| 1 | `general-purpose` | Project setup, Docker configuration |
| 2 | `general-purpose` | Core Axum implementation |
| 3 | `general-purpose` | Zion client, Redis integration |
| 4 | `general-purpose` | Token counting, rate limiting |
| 5 | `general-purpose` | Test suite implementation |
| 6 | `general-purpose` | Documentation
