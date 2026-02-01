# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-31)

**Core value:** Mindsmith can build and chat with assistants using any LLM provider through a single, stable API
**Current focus:** Phase 6 - Documentation

## Current Position

Phase: 6 of 6 (Documentation)
Plan: 1 of 2 in current phase
Status: In progress
Last activity: 2026-02-01 - Completed 06-01-PLAN.md (OpenAPI specification)

Progress: [====================] 97%

## Performance Metrics

**Velocity:**
- Total plans completed: 16
- Average duration: 6 min
- Total execution time: 101 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-types-and-translation | 4 | 19 min | 5 min |
| 02-api-endpoints | 2 | 7 min | 4 min |
| 03-session-management | 2 | 12 min | 6 min |
| 04-tier-routing | 4 | 29 min | 7 min |
| 05-tool-calling | 3 | 25 min | 8 min |
| 06-documentation | 1 | 9 min | 9 min |

**Recent Trend:**
- Last 5 plans: 05-01 (7min), 05-02 (11min), 05-03 (7min), 06-01 (9min)
- Trend: Steady at 7-11 min

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

| Decision | Plan | Rationale |
|----------|------|-----------|
| Content as untagged enum | 01-01 | Text serializes as string, Parts as array - matches OpenAI |
| ContentPart tagged by "type" | 01-01 | OpenAI-compatible `{"type": "text", "text": "..."}` format |
| deny_unknown_fields on request | 01-01 | Strict validation catches typos, enforces API contract |
| Model field optional | 01-01 | Tier routing may override model selection |
| OpenAI format passes through | 01-02 | Native API is OpenAI-compatible, minimal transformation needed |
| System message validation at translation | 01-02 | Catch ordering errors before API calls |
| serde_json::Value for response parsing | 01-02 | Flexible handling of optional fields |
| Error chunks before stream close | 01-03 | Emit structured error JSON so clients receive error info |
| NormalizedChunk abstracts stream events | 01-03 | Delta/Done/KeepAlive unified for provider-agnostic handling |
| OpenAI-compatible error format | 01-04 | Error response: {error: {message, type, code, provider?}} |
| Scaffold pattern for Anthropic | 01-04 | Validate now, implement translation in v2 |
| Extract user from extensions | 02-01 | Auth middleware stores user in extensions, not as handler param |
| Router state type | 02-01 | Return Router<Arc<AppState>> without .with_state() for nesting |
| Model required in Phase 2 | 02-01 | Phase 4 adds tier routing which makes model optional |
| Stream pass-through | 02-01 | Native API is OpenAI-compatible; minimal transformation needed |
| Regression test in native_chat module | 02-02 | Single module for all native chat tests including regression |
| Session stored as JSON in Redis | 03-01 | Follow SubscriptionCache pattern for consistency |
| Activity-based TTL refresh | 03-01 | touch() refreshes TTL on each request, not fixed from creation |
| Session model takes precedence | 03-02 | Session model overrides request model for stickiness |
| SessionCacheBackend abstraction | 03-02 | Redis/InMemory enum follows SubscriptionCache pattern |
| Tier enum with ordering | 04-01 | PartialOrd enables upgrade-only session logic |
| Replace model with tier | 04-01 | Native API uses tier abstraction, not direct model names |
| Model injection pattern | 04-01 | Handler determines model, injected into provider request |
| Temporary tier mapping | 04-01 | Hardcoded tier->model until TierRouter in Plan 02/03 |
| Static cache key for tier config | 04-01b | Global config, not per-user, uses static string |
| 30-minute tier config TTL | 04-01b | Balance between freshness and Zion API load |
| Cost-weighted selection | 04-02 | Probabilistic selection favors cheaper models |
| Exponential backoff for health | 04-02 | 30s initial, 2x multiplier, 5min max per decisions |
| weight = 1/relative_cost | 04-02 | Simple inverse weighting favors cheaper models |
| Preferred provider first | 04-02 | Session continuity takes precedence over cost |
| Fail explicit when Zion unavailable | 04-03 | Return 503, don't use hardcoded fallback per decisions |
| ToolResultContent as untagged enum | 05-01 | Text serializes as string, Json as object |
| Custom serde for ToolChoice | 05-01 | String variants plus object for function selection |
| Arguments as parsed JSON | 05-01 | serde_json::Value not string for ergonomics |
| ToolCallDelta index field | 05-01 | Critical for streaming accumulation of parallel tool calls |
| Sentinel ID format call_{uuid} | 05-02 | Consistent across providers, decoupled from provider IDs |
| Parse arguments to JSON object | 05-02 | More ergonomic than string, catch malformed JSON early |
| History lookup for function names | 05-02 | Tool results look up name from assistant message tool_calls |
| Tuple return (response, mapping) | 05-02 | translate_response returns ID mapping for tool results |
| Streaming uses provider IDs | 05-03 | V1 limitation - ID translation would require buffering stream |
| Handler discards mapping | 05-03 | Tool results use history lookup, no persistence needed |
| Accumulator sorts by index | 05-03 | Maintains order when deltas arrive interleaved |
| Use schema(as) for custom serde | 06-01 | ToolChoice/ToolResultContent use serde_json::Value representation |
| Schema examples at struct level | 06-01 | utoipa 5.x doesn't support variant-level examples on enums |
| SecurityAddon modifier pattern | 06-01 | Extensible security scheme definition via Modify trait |

### Pending Todos

None.

### Blockers/Concerns

None.

## Session Continuity

Last session: 2026-02-01
Stopped at: Completed 06-01-PLAN.md
Resume file: .planning/phases/06-documentation/06-02-PLAN.md
