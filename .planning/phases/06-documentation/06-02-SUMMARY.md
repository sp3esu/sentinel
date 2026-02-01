---
phase: 06-documentation
plan: 02
subsystem: api
tags: [swagger-ui, openapi, documentation, middleware, api-key]

# Dependency graph
requires:
  - phase: 06-01
    provides: OpenAPI specification and NativeApiDoc struct
provides:
  - Protected Swagger UI at /native/docs
  - Raw OpenAPI JSON at /native/docs/openapi.json
  - X-Docs-Key middleware for API key protection
  - Dev mode (no auth when DOCS_API_KEY not set)
affects: [api-clients, sdk-generation, developer-onboarding]

# Tech tracking
tech-stack:
  added: []
  patterns: [CDN-hosted Swagger UI, API key middleware]

key-files:
  created:
    - src/native_routes/docs.rs
  modified:
    - src/native_routes/mod.rs
    - src/routes/mod.rs

key-decisions:
  - "CDN-hosted Swagger UI assets - avoids bundling large static files, simpler build"
  - "404 on unauthorized (not 401/403) - hides endpoint existence from unauthorized users"
  - "Generic router over state type S - enables merging with Arc<AppState> routers"
  - "Static mutex for env var tests - ensures parallel test isolation"

patterns-established:
  - "API key middleware pattern with env var check and 404 response"
  - "Generic router functions for stateless route modules"

# Metrics
duration: 6min
completed: 2026-02-01
---

# Phase 06 Plan 02: Docs Endpoints Summary

**Protected Swagger UI and OpenAPI JSON endpoints with X-Docs-Key middleware using CDN-hosted assets**

## Performance

- **Duration:** 6 min
- **Started:** 2026-02-01T14:47:08Z
- **Completed:** 2026-02-01T14:54:06Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- Swagger UI accessible at /native/docs for interactive API exploration
- Raw OpenAPI JSON at /native/docs/openapi.json for tooling
- X-Docs-Key middleware returns 404 on unauthorized access (hides endpoint)
- Dev mode allows access when DOCS_API_KEY not set
- Full test coverage for all auth scenarios

## Task Commits

Each task was committed atomically:

1. **Task 1: Create docs routes with API key middleware** - `19f163b` (feat)
2. **Task 2: Wire docs router into main application** - `e9f85dc` (feat)
3. **Task 3: Add tests for docs endpoints** - `fa6abd5` (test)

## Files Created/Modified
- `src/native_routes/docs.rs` - Docs routes, API key middleware, Swagger UI HTML template
- `src/native_routes/mod.rs` - Export create_docs_router
- `src/routes/mod.rs` - Merge docs router into main application router

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| CDN-hosted Swagger UI assets | Avoids bundling large static files (~3MB), simpler build, easier updates |
| Return 404 on unauthorized | Hides endpoint existence from unauthorized users (security through obscurity) |
| Generic router function | Router<S> where S: Clone + Send + Sync enables merging with stateful routers |
| Static mutex for tests | Environment variable manipulation requires serialization in parallel tests |
| No catch-all route | CDN assets load directly from HTML, no need for redirect handler |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] axum version mismatch with utoipa-swagger-ui**
- **Found during:** Task 1 (Create docs routes)
- **Issue:** utoipa-swagger-ui 9.x uses axum 0.8, project uses axum 0.7, causing `From<SwaggerUi>` trait bound failure
- **Fix:** Implemented manual Swagger UI serving with CDN-hosted assets instead of utoipa-swagger-ui's Router conversion
- **Files modified:** src/native_routes/docs.rs
- **Verification:** cargo check passes, Swagger UI loads correctly
- **Committed in:** 19f163b (Task 1 commit)

**2. [Rule 1 - Bug] Invalid catch-all route syntax**
- **Found during:** Task 3 (Tests)
- **Issue:** Route `/native/docs/{*file}` panicked with "catch-all parameters are only allowed at the end"
- **Fix:** Removed catch-all route - unnecessary since HTML loads CDN assets directly
- **Files modified:** src/native_routes/docs.rs
- **Verification:** All tests pass
- **Committed in:** fa6abd5 (Task 3 commit)

**3. [Rule 1 - Bug] Environment variable race conditions in tests**
- **Found during:** Task 3 (Tests)
- **Issue:** Tests manipulating DOCS_API_KEY failed when run in parallel
- **Fix:** Added static mutex to serialize environment variable access across tests
- **Files modified:** src/native_routes/docs.rs
- **Verification:** Full test suite passes (414 tests)
- **Committed in:** fa6abd5 (Task 3 commit)

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs)
**Impact on plan:** All auto-fixes necessary for functionality. CDN approach is actually cleaner than bundled assets.

## Issues Encountered
- axum version mismatch required alternative implementation approach
- Resolved by using CDN-hosted Swagger UI which is lighter and easier to maintain

## User Setup Required

None - no external service configuration required.

**Optional:** Set `DOCS_API_KEY` environment variable in production to protect docs endpoints.

## Next Phase Readiness
- Documentation phase complete
- All verification criteria met
- Ready for production deployment

---
*Phase: 06-documentation*
*Completed: 2026-02-01*
