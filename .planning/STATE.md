# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-31)

**Core value:** Mindsmith can build and chat with assistants using any LLM provider through a single, stable API
**Current focus:** Phase 4 - Tier Routing

## Current Position

Phase: 4 of 6 (Tier Routing)
Plan: 2 of 4 in current phase (01 and 01b complete)
Status: In progress
Last activity: 2026-02-01 - Completed 04-01b-PLAN.md (Zion Tier Config)

Progress: [============        ] 60%

## Performance Metrics

**Velocity:**
- Total plans completed: 10
- Average duration: 5 min
- Total execution time: 49 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-types-and-translation | 4 | 19 min | 5 min |
| 02-api-endpoints | 2 | 7 min | 4 min |
| 03-session-management | 2 | 12 min | 6 min |
| 04-tier-routing | 2 | 11 min | 6 min |

**Recent Trend:**
- Last 5 plans: 03-01 (4min), 03-02 (8min), 04-01 (6min), 04-01b (5min)
- Trend: Steady at 4-8 min

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
| Fail explicit when Zion unavailable | 04-03 | Return 503, don't use hardcoded fallback per decisions |

### Pending Todos

None.

### Blockers/Concerns

None.

## Session Continuity

Last session: 2026-02-01
Stopped at: Completed 04-01b-PLAN.md (Zion Tier Config)
Resume file: .planning/phases/04-tier-routing/04-02-PLAN.md
