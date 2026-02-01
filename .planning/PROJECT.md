# Mindsmith Native API

## What This Is

A provider-agnostic LLM API layer for Mindsmith. It abstracts differences between LLM providers (OpenAI, Anthropic, X/Grok) behind a unified interface with tier-based model selection, session stickiness, and tool calling support. OpenAI is the only provider wired for v1; Anthropic and xAI scaffolds ready for v2.

## Core Value

Mindsmith can build and chat with assistants using any LLM provider through a single, stable API — provider changes and optimizations happen in Sentinel without touching the client.

## Requirements

### Validated

- ✓ OpenAI-compatible `/v1/*` endpoints — existing
- ✓ JWT authentication via Zion — existing
- ✓ Rate limiting with Redis sliding window — existing
- ✓ Token counting and usage tracking — existing
- ✓ Streaming responses (SSE) — existing
- ✓ Health and metrics endpoints — existing
- ✓ Unified message format (role, content) — v1.0
- ✓ System prompts handled uniformly — v1.0
- ✓ Common parameters (temperature, max_tokens, top_p, stop) — v1.0
- ✓ Unified error response format — v1.0
- ✓ Messages translated to OpenAI format — v1.0
- ✓ Anthropic translation scaffold (strict alternation) — v1.0
- ✓ Streaming chunks normalized — v1.0
- ✓ POST /native/chat/completions endpoint — v1.0
- ✓ Streaming and non-streaming modes — v1.0
- ✓ Conversation ID tracks session — v1.0
- ✓ Provider selection stored per session — v1.0
- ✓ API accepts tier level (simple/moderate/complex) — v1.0
- ✓ Tier maps to models from Zion config — v1.0
- ✓ Cost-weighted provider selection — v1.0
- ✓ Unavailable providers skipped with backoff — v1.0
- ✓ Unified tool definition format — v1.0
- ✓ Tool schemas translated to OpenAI — v1.0
- ✓ Tool call responses normalized — v1.0
- ✓ Streaming with tool calls handled — v1.0
- ✓ OpenAPI 3.x specification — v1.0
- ✓ Protected docs endpoint — v1.0

### Active

(None — v1 complete, define v2 requirements with next milestone)

### Out of Scope

- Anthropic provider implementation — design complete, implement in v2
- X/Grok provider implementation — design complete, implement in v2
- Silent failover between providers — errors bubble up to client for UX control
- Response metadata (tokens, cost, provider) — client doesn't need it
- Server-side conversation history storage — client sends full history
- Vision/image support — text only for v1, message format supports extension
- File attachments — not needed yet
- Multiple completions (n > 1) — not universally supported, adds complexity
- Direct model names in API — abstracted behind tiers

## Context

**Current State (v1.0 shipped 2026-02-01):**
- 25,764 lines of Rust
- Tech stack: Rust, Axum, Redis, OpenAI API
- 6 phases completed, 17 plans executed
- 130+ tests passing
- OpenAPI spec at /native/docs

**Architecture:**
- Unified types in `src/native/` — canonical format for all providers
- Translation layer in `src/native/translate/` — OpenAI implemented, Anthropic scaffold
- Tier routing in `src/tiers/` — config cache, health tracker, router
- Session management in `src/native/session.rs` — Redis-backed with TTL refresh
- Documentation in `src/docs/` — utoipa-generated OpenAPI 3.1

**Mindsmith Integration:**
- POST /native/v1/chat/completions — tier-based model selection
- Session stickiness via conversation_id
- Tool calling for assistant capabilities
- Protected docs at GET /native/docs (API key required)

## Constraints

- **Backwards Compatibility**: Existing `/v1/*` API unchanged
- **Single Provider**: Only OpenAI wired in v1
- **Tech Stack**: Rust, Axum, Redis — existing infrastructure
- **Auth**: Zion JWT for API access, separate API key for docs
- **Zion Dependency**: Tier config from Zion with 30-min cache

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Tier-based model selection | Abstracts model choices from client | ✓ Good — clean API |
| Session stickiness per conversation | Consistent behavior within a chat | ✓ Good — predictable UX |
| Full history per request | Simpler architecture, stateless Sentinel | ✓ Good — scales well |
| Errors bubble up (no failover) | Client controls UX | ✓ Good — explicit errors |
| OpenAPI for docs | Industry standard, machine-readable | ✓ Good — SDK generation ready |
| Separate docs API key | Docs not public but accessible to tools | ✓ Good — security + usability |
| Cost-weighted selection | Favor cheaper models probabilistically | ✓ Good — cost optimization |
| Exponential backoff for health | 30s initial, 5min max | ✓ Good — provider protection |
| Content as untagged enum | Text serializes as string, Parts as array | ✓ Good — OpenAI compatible |
| deny_unknown_fields on request | Strict validation catches typos | ✓ Good — API contract enforcement |
| Arguments as parsed JSON | serde_json::Value not string | ✓ Good — ergonomic tool results |
| CDN-hosted Swagger UI | Avoids bundling 3MB static files | ✓ Good — simpler build |

---
*Last updated: 2026-02-01 after v1.0 milestone*
