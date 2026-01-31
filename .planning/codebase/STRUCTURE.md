# Codebase Structure

**Analysis Date:** 2026-01-31

## Directory Layout

```
sentinel/
├── src/                          # Rust source code
│   ├── main.rs                   # Entry point: server startup and graceful shutdown
│   ├── lib.rs                    # Library root: AppState definition, module declarations
│   ├── config.rs                 # Environment-based configuration loading
│   ├── error.rs                  # Unified error types and HTTP status mapping
│   │
│   ├── middleware/               # Request middleware (auth, rate limiting)
│   │   ├── mod.rs                # Middleware module exports
│   │   ├── auth.rs               # JWT validation and AuthenticatedUser extraction
│   │   └── rate_limiter.rs       # Sliding window rate limiting (Redis-based)
│   │
│   ├── routes/                   # HTTP endpoint handlers
│   │   ├── mod.rs                # Router creation and route registration
│   │   ├── chat.rs               # POST /v1/chat/completions (streaming + non-streaming)
│   │   ├── completions.rs        # POST /v1/completions (legacy endpoint)
│   │   ├── embeddings.rs         # POST /v1/embeddings
│   │   ├── models.rs             # GET /v1/models, GET /v1/models/:id
│   │   ├── health.rs             # GET /health, /health/ready, /health/live
│   │   ├── metrics.rs            # GET /metrics (Prometheus format)
│   │   ├── debug.rs              # Debug endpoints (SENTINEL_DEBUG=true only)
│   │   ├── passthrough.rs        # Fallback handler for unmatched /v1/* routes
│   │   ├── responses.rs          # POST /v1/responses (OpenAI Responses API)
│   │   └── responses.rs          # JSON response serialization helpers
│   │
│   ├── proxy/                    # AI provider abstraction and implementation
│   │   ├── mod.rs                # Proxy module exports
│   │   ├── provider.rs           # AiProvider trait definition
│   │   ├── openai.rs             # OpenAI provider implementation
│   │   ├── headers.rs            # Secure header filtering (whitelist-based)
│   │   └── logging.rs            # RequestContext for request correlation
│   │
│   ├── cache/                    # Caching layer (Redis + in-memory)
│   │   ├── mod.rs                # Cache module exports
│   │   ├── redis.rs              # RedisCache: persistent cache implementation
│   │   ├── in_memory.rs          # InMemoryCache: test-only in-memory cache
│   │   └── subscription.rs       # SubscriptionCache: user limits + JWT validation wrapper
│   │
│   ├── zion/                     # External Zion governance API integration
│   │   ├── mod.rs                # Zion module exports
│   │   ├── client.rs             # ZionClient HTTP communication
│   │   └── models.rs             # Zion API data types (UserLimit, UserProfile, etc.)
│   │
│   ├── tokens/                   # Token counting (tiktoken-rs based)
│   │   ├── mod.rs                # Tokens module exports
│   │   └── counter.rs            # TokenCounter: estimate token counts per model
│   │
│   ├── usage/                    # Usage tracking and reporting
│   │   ├── mod.rs                # Usage module exports
│   │   ├── tracker.rs            # UsageTracker: immediate API reporting
│   │   └── batching.rs           # BatchingUsageTracker: fire-and-forget with batching
│   │
│   └── streaming/                # SSE stream processing utilities
│       └── mod.rs                # SseLineBuffer: line boundary handling for SSE chunks
│
├── tests/                        # Integration tests and test utilities
│   ├── integration/              # Integration test suites
│   │   ├── health.rs             # Health endpoint tests
│   │   ├── chat_completions.rs   # Chat API integration tests
│   │   ├── completions.rs        # Completions API integration tests
│   │   ├── embeddings.rs         # Embeddings API integration tests
│   │   ├── models.rs             # Models endpoint tests
│   │   └── auth.rs               # Authentication and rate limiting tests
│   │
│   ├── mocks/                    # Mock servers and test helpers
│   │   ├── zion.rs               # Mock Zion API (wiremock)
│   │   ├── openai.rs             # Mock OpenAI API (wiremock)
│   │   └── redis.rs              # Redis test helpers
│   │
│   ├── common/                   # Shared test utilities
│   │   └── mod.rs                # Common setup and helpers
│   │
│   ├── integration_tests.rs      # Integration test entry point
│   └── mocks_test.rs             # Mock server initialization
│
├── deployment/                   # Deployment configurations
│   ├── terraform/                # Terraform IaC
│   ├── scripts/                  # Deployment scripts
│   └── docker-compose files      # See root directory
│
├── docs/                         # Documentation
│   ├── integrations/             # External service integration docs
│   │   ├── zion.md               # Zion API documentation
│   │   └── zion-batch-endpoint.md # Batch increment endpoint
│   └── plans/                    # Implementation plans
│
├── scripts/                      # Utility scripts
├── target/                       # Build output (generated)
├── Cargo.toml                    # Rust project manifest and dependencies
├── Cargo.lock                    # Locked dependency versions
├── Dockerfile                    # Container build definition
├── docker-compose.yml            # Local development environment
├── docker-compose.prod.yml       # Production environment
├── run_dev.sh                    # Development startup script
├── CLAUDE.md                     # Claude Code context and guidelines
├── README.md                     # Project overview and setup
├── DEPLOYMENT.md                 # Production deployment guide
├── .env                          # Development environment variables (not committed)
├── .env.docker                   # Docker development environment
├── .env.example                  # Example environment template
├── .env.prod                     # Production environment variables (not committed)
├── .env.prod.example             # Production environment template
├── .gitignore                    # Git ignore rules
└── .github/                      # GitHub configuration
```

## Directory Purposes

**src/**
- Purpose: All Rust source code
- Contains: Main application, libraries, modules
- Key files: main.rs (entry), lib.rs (library root), config.rs, error.rs

**src/middleware/**
- Purpose: Request preprocessing middleware
- Contains: Authentication, rate limiting
- Key files: `auth.rs`, `rate_limiter.rs`

**src/routes/**
- Purpose: HTTP endpoint handlers
- Contains: All /v1/* API endpoints, health checks, metrics
- Key files: `mod.rs` (router setup), `chat.rs` (main endpoint), `passthrough.rs` (fallback)

**src/proxy/**
- Purpose: AI provider abstraction and implementation
- Contains: Provider trait, OpenAI implementation, header security
- Key files: `provider.rs` (trait), `openai.rs` (implementation), `headers.rs` (security)

**src/cache/**
- Purpose: Caching layer for user limits and JWT validation
- Contains: Redis cache, in-memory cache, subscription cache service
- Key files: `redis.rs` (production), `in_memory.rs` (test), `subscription.rs` (wrapper)

**src/zion/**
- Purpose: External Zion governance API integration
- Contains: API client, data models
- Key files: `client.rs` (HTTP communication), `models.rs` (data types)

**src/tokens/**
- Purpose: Token counting using tiktoken-rs
- Contains: Token counter with model-specific encoders
- Key files: `counter.rs` (implementation)

**src/usage/**
- Purpose: Usage tracking and reporting to Zion
- Contains: Immediate tracker, batching tracker
- Key files: `tracker.rs` (immediate), `batching.rs` (fire-and-forget)

**src/streaming/**
- Purpose: SSE stream parsing utilities
- Contains: Line buffer for chunk boundary handling
- Key files: `mod.rs` (SseLineBuffer implementation)

**tests/**
- Purpose: Integration tests and test utilities
- Contains: Mock servers, test suites, shared helpers
- Key files: `integration/` (test suites), `mocks/` (wiremock servers)

## Key File Locations

**Entry Points:**
- `src/main.rs`: Server startup, config loading, state initialization, graceful shutdown
- `src/routes/mod.rs::create_router()`: Route registration, middleware wiring

**Configuration:**
- `src/config.rs`: Environment variable loading with defaults
- `.env`, `.env.docker`, `.env.prod`: Environment variables (examples provided)
- `Dockerfile`: Container image definition
- `docker-compose.yml`, `docker-compose.prod.yml`: Service orchestration

**Core Logic:**
- `src/lib.rs`: AppState definition, module organization
- `src/middleware/auth.rs`: JWT validation and user extraction
- `src/middleware/rate_limiter.rs`: Sliding window rate limiting
- `src/routes/chat.rs`: Main chat completions endpoint (streaming + non-streaming)
- `src/proxy/provider.rs`: AI provider trait definition
- `src/proxy/openai.rs`: OpenAI implementation
- `src/zion/client.rs`: Zion API communication

**Testing:**
- `tests/integration/`: Test suites for each endpoint
- `tests/mocks/`: Mock server implementations using wiremock
- `tests/common/`: Shared test utilities

**Error Handling:**
- `src/error.rs`: Unified error types and HTTP status mapping

## Naming Conventions

**Files:**
- `.rs` extension for Rust source files
- Snake_case for filenames: `auth.rs`, `rate_limiter.rs`, `chat_completions.rs`
- Trait definitions in `<trait_name>.rs`: `provider.rs` for AiProvider trait
- Implementation details in descriptive files: `openai.rs`, `redis.rs`

**Directories:**
- Lowercase snake_case for module directories: `middleware/`, `routes/`, `proxy/`
- Functional grouping by concern: `cache/`, `zion/`, `tokens/`, `usage/`

**Modules:**
- Each directory has `mod.rs` with public exports
- Module names match directory names
- Private submodules re-exported through `pub use`

**Rust Identifiers:**
- Traits: PascalCase with suffix "Trait" or descriptive (AiProvider, SubscriptionCache)
- Structs: PascalCase (AppState, RequestContext, ChatCompletionRequest)
- Enums: PascalCase with variants (AppError::RateLimitExceeded)
- Functions: snake_case (chat_completions, rate_limit_middleware)
- Constants: UPPER_SNAKE_CASE (HOP_BY_HOP_HEADERS)
- Type aliases: PascalCase (AppResult, ByteStream)

## Where to Add New Code

**New Endpoint (e.g., POST /v1/images/generations):**
1. Create `src/routes/images.rs` with handler function
2. Add route in `src/routes/mod.rs::create_router()`: `.route("/images/generations", post(images::handler))`
3. Create test in `tests/integration/images.rs`
4. Create mock in `tests/mocks/openai.rs` if needed
5. Add to passthrough if token tracking not needed, otherwise implement typed handler

**New Middleware:**
1. Create file in `src/middleware/`
2. Implement as `async fn middleware_name(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Result<Response>`
3. Register in `src/routes/mod.rs::create_router()` using `.layer(middleware::from_fn_with_state(state, middleware_name))`

**New External Service Integration:**
1. Create new module directory: `src/<service_name>/`
2. Create `client.rs` for HTTP communication
3. Create `models.rs` for data types
4. Export from `src/<service_name>/mod.rs`
5. Add client initialization to `AppState::new()`

**New Cache Backend:**
1. Implement struct in `src/cache/<backend_name>.rs`
2. Implement trait: `get()`, `set_with_ttl()`, `delete()`
3. Add variant to `CacheBackend` enum in `src/cache/subscription.rs`
4. Add match arm in CacheBackend methods

**Utilities and Helpers:**
- Shared utilities: `src/lib.rs` or dedicated module if substantial
- Streaming utilities: `src/streaming/mod.rs`
- Error utilities: `src/error.rs`
- Token utilities: `src/tokens/`

**Tests:**
- Integration tests: `tests/integration/<feature>.rs`
- Unit tests: `#[cfg(test)] mod tests { ... }` in same file as code
- Test mocks: `tests/mocks/<service>.rs`

## Special Directories

**src/proxy/:**
- Purpose: AI provider abstraction layer
- Generated: No
- Committed: Yes
- Notes: Headers.rs is critical for security (whitelist-based filtering)

**tests/mocks/:**
- Purpose: Mock servers for testing
- Generated: No
- Committed: Yes
- Notes: Uses wiremock for HTTP mocking, enables isolated integration tests

**tests/integration/:**
- Purpose: Integration test suites
- Generated: No
- Committed: Yes
- Notes: Tests run against mock Zion and OpenAI servers (no real API calls)

**target/:**
- Purpose: Build artifacts and compiled binaries
- Generated: Yes (by cargo build)
- Committed: No (in .gitignore)
- Size: ~500MB-1GB depending on build profile

**.planning/codebase/:**
- Purpose: Generated documentation for Claude Code
- Generated: Yes (by GSD mapper)
- Committed: Yes
- Contains: ARCHITECTURE.md, STRUCTURE.md, CONVENTIONS.md, TESTING.md, CONCERNS.md, STACK.md, INTEGRATIONS.md

---

*Structure analysis: 2026-01-31*
