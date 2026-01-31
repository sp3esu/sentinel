# Mindsmith Native API

## What This Is

A provider-agnostic LLM API layer for the Mindsmith application. It abstracts differences between LLM providers (OpenAI, Anthropic, X/Grok) behind a unified interface, enabling Mindsmith to interact with any provider without knowing provider-specific details. The API uses tier-based model selection and intelligent provider routing.

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

### Active

- [ ] New `/native/*` API endpoints for Mindsmith
- [ ] Unified message format supporting text (extensible to images)
- [ ] Unified tool/function calling format (translates to provider-specific)
- [ ] Tier-based model selection: `simple | moderate | complex`
- [ ] Session (conversation) tracking for provider stickiness
- [ ] Provider routing based on cost and availability
- [ ] Model configuration loaded from Zion (with caching/fallback)
- [ ] OpenAPI specification for the Native API
- [ ] Protected docs endpoint (`/native/docs`) with dedicated API key
- [ ] Streaming chat completions through unified format

### Out of Scope

- Anthropic provider implementation — design for it, implement later
- X/Grok provider implementation — design for it, implement later
- Silent failover between providers — errors bubble up to client
- Response metadata (tokens, cost, provider) — client doesn't need it
- Server-side conversation history storage — client sends full history
- Vision/image support — text only for v1, but design message format to extend
- File attachments — not needed yet

## Context

**Mindsmith Application:**
- Users create custom assistants by chatting with "Merlin" (a meta-assistant)
- Merlin helps craft system prompts and assigns a complexity tier (simple/moderate/complex)
- Users then chat with their created assistants
- All conversations are streamed for natural feel

**Provider Landscape:**
- OpenAI: Current provider, well-understood API
- Anthropic: Different message format, different tool calling schema
- X/Grok: OpenAI-compatible with extensions
- Each has different pricing, rate limits, and capabilities

**Existing Codebase:**
- Rust/Axum proxy with middleware-based architecture
- `AiProvider` trait already exists for provider abstraction
- Redis for caching (user limits, JWT validation)
- Zion integration for auth and usage tracking

**Design Constraints:**
- API must accommodate differences between providers without leaking abstractions
- Tool definitions vary significantly: OpenAI uses `functions`, Anthropic uses `tools` with different schema
- Message roles and formats differ across providers
- Streaming chunk formats are provider-specific

## Constraints

- **Backwards Compatibility**: Existing `/v1/*` API must remain unchanged — new API is additive
- **Single Provider Implementation**: Only OpenAI wired up initially — others inform design only
- **Tech Stack**: Rust, Axum, Redis — must fit existing architecture
- **Auth**: Zion JWT for API access, separate API key for docs endpoint
- **Zion Dependency**: Model configuration comes from Zion (API not yet created — need fallback)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Tier-based model selection | Abstracts model choices from client; allows server-side optimization | — Pending |
| Session stickiness per conversation | Consistent behavior within a chat; avoids mid-conversation provider switches | — Pending |
| Full history per request | Simpler architecture; no server-side state management for conversations | — Pending |
| Errors bubble up (no silent failover) | Client controls UX; keeps Sentinel simple and predictable | — Pending |
| OpenAPI for docs | Industry standard; machine-readable for AI agents | — Pending |
| Separate docs API key | Docs not public but accessible to development tools | — Pending |

---
*Last updated: 2026-01-31 after initialization*
