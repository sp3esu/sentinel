---
phase: 02-api-endpoints
plan: 01
subsystem: api
tags: [native-api, chat-completions, streaming, sse, axum]

dependency-graph:
  requires: [01-types-and-translation]
  provides: [/native/v1/chat/completions endpoint]
  affects: [02-02-embeddings, 03-middleware-enhancements]

tech-stack:
  added: []
  patterns: [nested-routers, middleware-from-fn-with-state, sse-streaming]

key-files:
  created:
    - src/native_routes/mod.rs
    - src/native_routes/chat.rs
  modified:
    - src/lib.rs
    - src/routes/mod.rs

decisions:
  - id: extract-user-from-extensions
    choice: "Extract AuthenticatedUser from request.extensions()"
    reason: "Auth middleware stores user in extensions, not as parameter"
  - id: router-state-type
    choice: "Return Router<Arc<AppState>> without .with_state()"
    reason: "Parent router provides state; avoids type mismatch"
  - id: model-required-phase2
    choice: "Require model field in Phase 2"
    reason: "Phase 4 adds tier routing which makes model optional"
  - id: stream-pass-through
    choice: "OpenAI stream chunks pass through unchanged"
    reason: "Native API is OpenAI-compatible; minimal transformation needed"

metrics:
  duration: 4 min
  completed: 2026-02-01
---

# Phase 02 Plan 01: Chat Completions Endpoint Summary

Native API chat completions endpoint with streaming and non-streaming support.

## One-liner

POST /native/v1/chat/completions handler using OpenAITranslator with SSE streaming passthrough.

## What Was Built

### Task 1: Create native_routes module with router
**Commit:** ab296c6

Created `src/native_routes/mod.rs`:
- `create_native_router(state)` function returning `Router<Arc<AppState>>`
- Route: POST `/v1/chat/completions` -> `chat::native_chat_completions`
- Applied existing auth/rate-limit middleware in correct order
- No `.with_state()` call - parent router provides state

Added module to `src/lib.rs`:
- `pub mod native_routes;`

### Task 2: Implement native chat completions handler
**Commit:** 08a66cb

Created `src/native_routes/chat.rs` with:
- `native_chat_completions` handler accepting Native API format
- Extracts `AuthenticatedUser` from request extensions (set by auth middleware)
- Requires `model` field (Phase 4 enables tier routing)
- Uses `OpenAITranslator` for request/response translation
- Returns `NativeErrorResponse` for all error cases

**Non-streaming flow:**
- Forward translated request to `ai_provider.chat_completions()`
- Translate response back with `translator.translate_response()`
- Track usage via `batching_tracker.track()`
- Return JSON response

**Streaming flow:**
- Inject `stream_options.include_usage: true` for accurate token tracking
- Call `ai_provider.chat_completions_stream()`
- Pass through OpenAI SSE chunks (format matches Native API)
- Accumulate content and usage for tracking
- Return SSE response with proper headers

### Task 3: Wire native routes into main router
**Commit:** 448c568

Modified `src/routes/mod.rs`:
- Added `use crate::native_routes;`
- Added `.nest("/native", native_routes::create_native_router(state.clone()))`
- Updated fallback message to mention `/native/` endpoints

## Key Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| User extraction | From extensions | Auth middleware stores in extensions, not handler param |
| Router return type | `Router<Arc<AppState>>` | Allows nesting without state type mismatch |
| Model field | Required in Phase 2 | Phase 4 tier routing makes it optional |
| Stream handling | Pass-through | Native API is OpenAI-compatible, minimal transformation |
| Usage tracking | stream_options injection | Critical for accurate token counting in streams |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Handler signature fix**
- **Found during:** Task 2 verification
- **Issue:** `user: AuthenticatedUser` parameter didn't work as Axum handler param
- **Fix:** Extract from `request.extensions().get::<AuthenticatedUser>()`
- **Files modified:** src/native_routes/chat.rs
- **Commit:** 08a66cb (included in task commit)

**2. [Rule 3 - Blocking] Router type mismatch**
- **Found during:** Task 3 verification
- **Issue:** `Router` vs `Router<Arc<AppState>>` type mismatch when nesting
- **Fix:** Return `Router<Arc<AppState>>` and remove `.with_state(state)` call
- **Files modified:** src/native_routes/mod.rs
- **Commit:** 448c568 (included in task commit)

## Verification Results

- `cargo check`: Pass (only pre-existing warning about unused redis import)
- `cargo test`: 288 tests pass, 0 failures
- `cargo build`: Success

### Code Review Checklist

- [x] Native routes use existing middleware (not duplicated)
- [x] Error responses use NativeErrorResponse (not AppError)
- [x] Streaming injects stream_options.include_usage: true
- [x] Model field required (with comment about Phase 4)

## Files Changed

| File | Action | Purpose |
|------|--------|---------|
| src/lib.rs | Modified | Add native_routes module |
| src/native_routes/mod.rs | Created | Native API router with auth/rate-limit |
| src/native_routes/chat.rs | Created | Chat completions handler |
| src/routes/mod.rs | Modified | Wire /native routes into main router |

## Next Phase Readiness

### Immediate Next Steps
- Plan 02-02: Add /native/v1/embeddings endpoint (follows same pattern)

### Dependencies Unblocked
- Phase 3 middleware can now enhance native routes
- Phase 4 tier routing can hook into model selection

### No Blockers
- Authentication and rate limiting work out of the box
- Usage tracking integrates with existing batching tracker
