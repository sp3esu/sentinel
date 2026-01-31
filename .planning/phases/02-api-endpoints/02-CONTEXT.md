# Phase 2: API Endpoints - Context

**Gathered:** 2026-02-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Expose /native/* endpoints that accept unified format and return responses. Supports streaming and non-streaming modes. Must not break existing /v1/* endpoints.

</domain>

<decisions>
## Implementation Decisions

### Endpoint paths
- Use versioned path structure: `/native/v1/chat/completions`
- Add `/native/v1/models` endpoint showing native-supported models
- Chat only — no legacy `/native/v1/completions` endpoint (deprecated pattern)
- Separate router module — dedicated `native_routes` module, clean separation from `/v1/*`

### Request validation
- Strict validation with `deny_unknown_fields` — reject unknown fields, catches typos
- Two-layer validation: structural at deserialization, semantic at translation
- Detailed error messages with field path + reason (e.g., "messages[0].content: expected string or array")
- Enforce content limits before sending to provider — faster feedback, protects backend

### Response format
- OpenAI-compatible response structure — clients can switch with minimal changes
- Hide provider in chunks — provider is implementation detail, cleaner abstraction
- Always include usage stats (prompt_tokens, completion_tokens, total_tokens)
- Streaming: emit usage in final chunk before [DONE]

### Authentication behavior
- Same Zion JWT auth as /v1/* — reuse existing middleware
- Native error format for auth errors (not OpenAI format)
- Generic rate limit errors ("Rate limit exceeded") — no specific limit name exposed
- Same usage limits as /v1/* — all endpoints share ai_requests, ai_tokens quotas

### Claude's Discretion
- Exact HTTP status codes for different error types
- Internal routing between native handlers and translation layer
- SSE chunk format details beyond [DONE] terminator
- Content limit thresholds (can defer to Phase 4 tier config)

</decisions>

<specifics>
## Specific Ideas

- Response structure should feel like calling OpenAI directly — same fields, same nesting
- Error messages should be developer-friendly, actionable without being verbose

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 02-api-endpoints*
*Context gathered: 2026-02-01*
