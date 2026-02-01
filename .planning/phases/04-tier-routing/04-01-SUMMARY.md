---
phase: 04-tier-routing
plan: 01
subsystem: api
tags: [tier, routing, model-selection, serde]

# Dependency graph
requires:
  - phase: 03-session-management
    provides: Session types for tier stickiness
provides:
  - Tier enum with Simple, Moderate, Complex variants
  - ChatCompletionRequest with tier field instead of model
  - TierConfigData types for Zion API integration
affects: [04-02, 04-03]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Tier enum with PartialOrd for upgrade-only logic
    - Temporary tier_to_default_model mapping for transition

key-files:
  created: []
  modified:
    - src/native/types.rs
    - src/native/request.rs
    - src/zion/models.rs
    - src/native_routes/chat.rs
    - src/native/translate/openai.rs

key-decisions:
  - "Tier enum ordering enables upgrade-only session logic via PartialOrd"
  - "Replace model field entirely with tier field in native API"
  - "Model is injected by handler, not included in translate_request"
  - "Temporary hardcoded tier->model mapping until TierRouter in Plan 02/03"

patterns-established:
  - "Tier routing: Request specifies tier, handler selects model"
  - "TierConfigData: Zion provides tier-to-model mappings with cost weights"

# Metrics
duration: 6min
completed: 2026-02-01
---

# Phase 4 Plan 01: Tier Routing Foundation Types Summary

**Tier enum with ordering (Simple < Moderate < Complex), tier field in ChatCompletionRequest, and Zion tier config types for model selection**

## Performance

- **Duration:** 6 min
- **Started:** 2026-02-01
- **Completed:** 2026-02-01
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments
- Added Tier enum with Simple, Moderate, Complex variants and PartialOrd for upgrade logic
- Replaced model field with tier field in ChatCompletionRequest (breaking API change)
- Added TierConfigData, TierMapping, ModelConfig types for Zion tier configuration
- Updated native_routes/chat.rs with temporary tier-to-model mapping
- Updated translate/openai.rs to not include model in request (injected by handler)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Tier enum to native types** - `d7b1ec6` (feat)
2. **Task 2: Replace model with tier in ChatCompletionRequest** - `7cb9b7f` (feat)
3. **Task 3: Add tier config types to Zion models** - `36835d8` (feat)

## Files Created/Modified
- `src/native/types.rs` - Added Tier enum with ordering, Default, Display
- `src/native/request.rs` - Changed model field to tier field
- `src/zion/models.rs` - Added ModelConfig, TierMapping, TierConfigData, TierConfigResponse
- `src/native_routes/chat.rs` - Added tier_to_default_model temporary mapping, updated handler
- `src/native/translate/openai.rs` - Removed model from translate_request

## Decisions Made
- **Tier enum ordering:** PartialOrd derives automatically give Simple < Moderate < Complex, enabling can_upgrade_to logic
- **Model field removal:** Completely replaced with tier - no backward compatibility for model field
- **Model injection pattern:** Handler determines model via tier routing, injects into provider request
- **Temporary mapping:** Used hardcoded tier->model mapping until TierRouter in Plan 02/03

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed compilation errors in dependent files**
- **Found during:** Task 2 (Replace model with tier)
- **Issue:** native_routes/chat.rs and native/translate/openai.rs referenced request.model which no longer exists
- **Fix:** Updated chat.rs to use tier with temporary tier_to_default_model mapping; updated openai.rs to not include model in translate_request
- **Files modified:** src/native_routes/chat.rs, src/native/translate/openai.rs
- **Verification:** cargo check passes, all tests pass
- **Committed in:** 7cb9b7f (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary to make codebase compile. The plan expected these errors but they blocked test execution. Fixes are minimal and correct - TierRouter in Plan 02/03 will replace the temporary mapping.

## Issues Encountered
None - all issues were handled via deviation rules.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Tier enum and types ready for TierRouter implementation (Plan 02)
- Zion tier config types ready for client integration (Plan 01b)
- Temporary tier-to-model mapping in place until full tier routing

---
*Phase: 04-tier-routing*
*Completed: 2026-02-01*
