---
phase: 06-documentation
plan: 01
subsystem: api
tags: [utoipa, openapi, documentation, swagger]

# Dependency graph
requires:
  - phase: 05-tool-calling
    provides: Native API types with tool calling support
provides:
  - OpenAPI specification for Native API
  - ToSchema derives on all native types
  - utoipa::path annotation on chat handler
  - Static export binary for CI/tooling
affects: [06-02, sdk-generation, api-linting]

# Tech tracking
tech-stack:
  added: [utoipa 5.4, utoipa-swagger-ui 9.0]
  patterns: [OpenAPI-first documentation]

key-files:
  created:
    - docs/openapi.json
    - src/docs/mod.rs
    - src/docs/openapi.rs
    - src/bin/export_openapi.rs
  modified:
    - Cargo.toml
    - src/lib.rs
    - src/native/types.rs
    - src/native/request.rs
    - src/native/response.rs
    - src/native/error.rs
    - src/native_routes/chat.rs

key-decisions:
  - "Use schema(as) for custom serde types - ToolChoice and ToolResultContent use serde_json::Value representation"
  - "Schema examples at struct level for enums - utoipa 5.x doesn't support variant-level examples"
  - "SecurityAddon modifier for bearer_auth - more explicit than inline security scheme"

patterns-established:
  - "ToSchema derive pattern for all API types"
  - "utoipa::path annotation pattern for handlers"
  - "export_openapi binary for static spec generation"

# Metrics
duration: 9min
completed: 2026-02-01
---

# Phase 6 Plan 1: OpenAPI Specification Generation Summary

**utoipa 5.4 integration with ToSchema derives on all native types, comprehensive path annotation on chat handler, and static export binary generating docs/openapi.json**

## Performance

- **Duration:** 9 min
- **Started:** 2026-02-01T14:35:31Z
- **Completed:** 2026-02-01T14:44:35Z
- **Tasks:** 3
- **Files modified:** 11

## Accomplishments
- Added utoipa 5.4 and utoipa-swagger-ui 9.0 dependencies
- All 28 native types have ToSchema derive with realistic examples
- Chat handler has comprehensive utoipa::path with full documentation
- NativeApiDoc struct aggregates paths, schemas, and security
- Static export binary generates valid OpenAPI 3.1.0 JSON

## Task Commits

Each task was committed atomically:

1. **Task 1: Add utoipa dependencies and create docs module** - `52f95e2` (feat)
2. **Task 2: Add ToSchema derives to all native types** - `bfc0426` (feat)
3. **Task 3: Add utoipa::path to chat handler and create export binary** - `5ef892e` (feat)

## Files Created/Modified

**Created:**
- `docs/openapi.json` - Generated OpenAPI 3.1.0 specification
- `src/docs/mod.rs` - Docs module declaration with NativeApiDoc export
- `src/docs/openapi.rs` - OpenAPI struct with paths, schemas, security
- `src/bin/export_openapi.rs` - Binary for static spec generation

**Modified:**
- `Cargo.toml` - Added utoipa dependencies and export_openapi binary
- `src/lib.rs` - Added pub mod docs declaration
- `src/native/types.rs` - ToSchema on Role, Tier, Message, Content, ToolCall, etc.
- `src/native/request.rs` - ToSchema on ChatCompletionRequest, StopSequence
- `src/native/response.rs` - ToSchema on all response and stream types
- `src/native/error.rs` - ToSchema on NativeError, NativeErrorResponse
- `src/native_routes/chat.rs` - utoipa::path annotation with full docs

## Decisions Made

1. **Use schema(as) for custom serde types** - ToolChoice and ToolResultContent have custom serde implementations that serialize to string or object. Using `#[schema(as = serde_json::Value)]` allows OpenAPI to represent them correctly without manual ToSchema impl.

2. **Schema examples at struct level for enums** - utoipa 5.x no longer supports per-variant examples on simple enums. Moved examples to struct-level `#[schema(example = "value")]` attribute.

3. **SecurityAddon modifier pattern** - Rather than inline security definition, used a Modify trait implementation to add bearer_auth security scheme. This pattern is more extensible for future security additions.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

1. **utoipa 5.x API changes** - Initial ToSchema derives used syntax from older utoipa versions. Fixed by removing variant-level examples on enums and using `schema(as)` instead of `schema(value_type)` for custom serde types.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- OpenAPI spec ready for Plan 02 (docs endpoint with Swagger UI)
- All schemas documented with realistic examples
- Security scheme defined for authentication
- Static export available for CI/tooling integration

---
*Phase: 06-documentation*
*Completed: 2026-02-01*
