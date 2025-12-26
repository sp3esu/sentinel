# Sentinel AI Proxy

High-performance AI proxy with traffic limiting based on user subscriptions.

## Overview

Sentinel is a Rust-based proxy server that:

- Exposes **OpenAI-compatible API endpoints** (`/v1/chat/completions`, `/v1/completions`, `/v1/models`)
- **Enforces usage limits** based on user subscriptions from Zion governance system
- **Counts tokens** accurately using tiktoken-rs
- **Caches user limits** in Redis for low-latency enforcement
- **Supports streaming** responses via Server-Sent Events (SSE)
- Routes requests through **Vercel AI Gateway** to upstream LLM providers

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
│  │  └──────────────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                            │
            ┌───────────────┴───────────────┐
            ▼                               ▼
    ┌──────────────┐               ┌──────────────────┐
    │  Zion API    │               │ Vercel AI Gateway│
    │ (Governance) │               │    (OpenAI)      │
    └──────────────┘               └──────────────────┘
```

## Quick Start

### Prerequisites

- Docker and Docker Compose
- Zion API credentials
- Vercel AI Gateway API key

### Running with Docker

```bash
# Copy environment file
cp .env.example .env

# Edit .env with your credentials
vim .env

# Start all services
docker-compose up -d

# View logs
docker-compose logs -f sentinel
```

### Running Locally

```bash
# Start Redis
docker-compose up -d redis

# Run the proxy
./run_dev.sh

# Or with cargo directly
cargo run
```

## Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ZION_API_URL` | Yes | - | Zion governance API URL |
| `ZION_API_KEY` | Yes | - | API key for Zion external endpoints |
| `VERCEL_AI_GATEWAY_API_KEY` | Yes | - | Vercel AI Gateway API key |
| `SENTINEL_HOST` | No | `0.0.0.0` | Host to bind to |
| `SENTINEL_PORT` | No | `8080` | Port to listen on |
| `REDIS_URL` | No | `redis://localhost:6379` | Redis connection URL |
| `VERCEL_AI_GATEWAY_URL` | No | `https://gateway.ai.vercel.com/v1` | Gateway URL |
| `CACHE_TTL_SECONDS` | No | `300` | User limits cache TTL |
| `JWT_CACHE_TTL_SECONDS` | No | `300` | JWT validation cache TTL |
| `RUST_LOG` | No | `sentinel=info` | Log level |

## API Endpoints

### OpenAI-Compatible

#### Chat Completions
```bash
POST /v1/chat/completions
Authorization: Bearer <zion-jwt>
Content-Type: application/json

{
  "model": "gpt-4",
  "messages": [
    {"role": "user", "content": "Hello!"}
  ],
  "stream": true
}
```

#### Completions (Legacy)
```bash
POST /v1/completions
Authorization: Bearer <zion-jwt>
Content-Type: application/json

{
  "model": "gpt-3.5-turbo-instruct",
  "prompt": "Hello",
  "max_tokens": 100
}
```

#### Models
```bash
GET /v1/models
GET /v1/models/gpt-4
```

### Health & Monitoring

```bash
# Full health check with dependency status
GET /health

# Kubernetes readiness probe
GET /health/ready

# Kubernetes liveness probe
GET /health/live

# Prometheus metrics
GET /metrics
```

### Health Response

```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "timestamp": "2024-01-15T10:30:00Z",
  "checks": {
    "redis": {
      "status": "healthy",
      "latency_ms": 2
    }
  },
  "stats": {
    "uptime_seconds": 3600
  }
}
```

## Authentication

Sentinel uses **Zion JWT passthrough** authentication:

1. Client authenticates with Zion and obtains a JWT
2. Client sends requests to Sentinel with `Authorization: Bearer <jwt>`
3. Sentinel validates the JWT via Zion API (with caching)
4. User's `external_id` is extracted for limit lookups

## Rate Limiting

Sentinel enforces rate limits using a **sliding window algorithm**:

- Limits are fetched from Zion and cached in Redis
- Window-based counters track usage per user
- When limits are exceeded, returns `429 Too Many Requests` with details

### Rate Limit Headers

```
X-RateLimit-Limit: 10000
X-RateLimit-Remaining: 9500
X-RateLimit-Reset: 1705312800
```

## Token Counting

Tokens are counted accurately using `tiktoken-rs`:

- **Pre-request**: Estimates prompt tokens before forwarding
- **Post-response**: Uses OpenAI's `usage` field when available
- **Streaming**: Uses `stream_options.include_usage=true`
- **Fallback**: Counts completion tokens locally

## Zion Integration

### Required Limits

Configure these limits in your Zion instance:

| Name | Display Name | Unit |
|------|--------------|------|
| `ai_input_tokens` | AI Input Tokens | tokens |
| `ai_output_tokens` | AI Output Tokens | tokens |
| `ai_requests` | AI Requests | requests |

### API Endpoints Used

- `GET /api/v1/limits/external/{externalId}` - Fetch user limits
- `POST /api/v1/usage/external/increment` - Increment usage
- `GET /api/v1/users/me` - Validate JWT

## Development

### Project Structure

```
sentinel/
├── src/
│   ├── main.rs           # Entry point
│   ├── config.rs         # Configuration
│   ├── error.rs          # Error types
│   ├── routes/           # HTTP endpoints
│   ├── middleware/       # Auth & rate limiting
│   ├── proxy/            # Vercel Gateway client
│   ├── zion/             # Zion API client
│   ├── cache/            # Redis caching
│   ├── tokens/           # Token counting
│   └── usage/            # Usage tracking
├── tests/
│   ├── common/           # Test utilities
│   ├── integration/      # Integration tests
│   └── mocks/            # Mock servers
├── docs/
│   └── integrations/     # Integration guides
├── Dockerfile
├── docker-compose.yml
└── Cargo.toml
```

### Running Tests

```bash
# All tests
cargo test

# Specific module
cargo test tokens::

# With output
cargo test -- --nocapture
```

### Building for Production

```bash
# Build optimized binary
cargo build --release

# Build Docker image
docker build -t sentinel .
```

## Deployment

### Docker

```bash
docker run -d \
  -p 8080:8080 \
  -e ZION_API_URL=https://your-zion.com \
  -e ZION_API_KEY=your-key \
  -e VERCEL_AI_GATEWAY_API_KEY=your-vercel-key \
  -e REDIS_URL=redis://redis:6379 \
  sentinel
```

### Kubernetes

Use the health endpoints for probes:

```yaml
livenessProbe:
  httpGet:
    path: /health/live
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 30

readinessProbe:
  httpGet:
    path: /health/ready
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 10
```

## Monitoring

### Prometheus Metrics

Scrape the `/metrics` endpoint for:

- `sentinel_requests_total` - Total requests by status
- `sentinel_request_duration_seconds` - Request latency histogram
- `sentinel_tokens_processed_total` - Tokens by type (input/output)
- `sentinel_cache_hits_total` - Cache hit/miss ratio

### Grafana

Import dashboards for:
- Request rate and latency
- Token usage over time
- Cache performance
- Error rates

## License

MIT
