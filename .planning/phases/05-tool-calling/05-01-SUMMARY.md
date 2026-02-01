---
phase: 05-tool-calling
plan: 01
subsystem: api
tags: [tool-calling, function-calling, json-schema, openai-compatible, serde]

# Dependency graph
requires:
  - phase: 01-types-and-translation
    provides: Native API types (Message, Content, Role)
provides:
  - ToolDefinition, ToolCall, ToolResult, ToolChoice types
  - Tool schema validation with jsonschema
  - Tool name validation (alphanumeric + underscore)
  - ChatCompletionRequest with tools and tool_choice fields
  - ChoiceMessage and Delta with tool_calls for responses
  - ToolCallDelta for streaming tool call fragments
affects:
  - 05-02: Translation layer uses these types
  - 05-03: Streaming accumulator uses ToolCallDelta

# Tech tracking
tech-stack:
  added: [jsonschema 0.29, regex 1]
  patterns: [untagged enum serialization for ToolResultContent, custom serde for ToolChoice]

key-files:
  created: []
  modified:
    - src/native/types.rs
    - src/native/request.rs
    - src/native/response.rs
    - src/native/mod.rs
    - Cargo.toml

key-decisions:
  - "ToolResultContent as untagged enum: Text serializes as string, Json as object"
  - "ToolChoice custom serde: string variants (auto/none/required) plus object for function selection"
  - "Arguments as parsed JSON (serde_json::Value) not string per CONTEXT.md decision"
  - "ToolCallDelta uses index field for parallel tool call accumulation"
  - "Message gets tool_calls field for assistant messages with tool calls"

patterns-established:
  - "Tool call ID format: call_{uuid} for Sentinel-generated IDs"
  - "Validation functions return bool or Result<(), String> for clear error messages"
  - "Schema validation checks type: object requirement for function parameters"

# Metrics
duration: 7min
completed: 2026-02-01
---

# Phase 5 Plan 1: Tool Calling Types Summary

**ToolDefinition, ToolCall, ToolResult, ToolChoice types with JSON Schema validation and streaming delta support**

## Performance

- **Duration:** 7 min
- **Started:** 2026-02-01T11:04:43Z
- **Completed:** 2026-02-01T11:11:46Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments
- Defined complete tool calling type system (ToolDefinition, ToolCall, ToolResult, ToolChoice)
- Added JSON Schema validation for tool parameters with type: object requirement
- Added tool name validation (alphanumeric + underscore pattern)
- Extended ChatCompletionRequest with tools and tool_choice fields
- Extended response types with tool_calls for both streaming and non-streaming
- Added 47 new unit tests for tool type serialization and validation

## Task Commits

Each task was committed atomically:

1. **Task 1: Add jsonschema dependency and tool types** - `575a8b3` (feat)
2. **Task 2: Add tool fields to request** - `c8f9748` (feat)
3. **Task 3: Add tool_calls to response types** - `251d6f3` (feat)

## Files Created/Modified
- `Cargo.toml` - Added jsonschema 0.29 and regex 1 dependencies
- `src/native/types.rs` - Added FunctionDefinition, ToolDefinition, ToolCall, ToolCallFunction, ToolResultContent, ToolResult, ToolChoice types with validation functions and 31 tests
- `src/native/request.rs` - Added tools and tool_choice fields to ChatCompletionRequest with 8 tests
- `src/native/response.rs` - Added tool_calls to ChoiceMessage and Delta, ToolCallDelta and ToolCallFunctionDelta types with 8 tests
- `src/native/mod.rs` - Updated re-exports for all new tool types
- `src/native/streaming.rs` - Updated Delta usages with tool_calls field
- `src/native/translate/openai.rs` - Updated ChoiceMessage construction with tool_calls
- `src/native/translate/anthropic.rs` - Updated Message construction with tool_calls

## Decisions Made
- **ToolResultContent as untagged enum:** Text variant serializes as JSON string, Json variant serializes as object - matches ergonomic API design from CONTEXT.md
- **Custom serde for ToolChoice:** String variants (auto/none/required) plus structured object for function selection - matches OpenAI format exactly
- **Arguments as parsed JSON:** serde_json::Value not string per CONTEXT.md decision for better ergonomics
- **ToolCallDelta index field:** Critical for streaming accumulation of parallel tool calls
- **Message tool_calls field:** Added to Message type for assistant messages that include tool calls

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added tool_calls field to Message type**
- **Found during:** Task 2 (compilation failure)
- **Issue:** Adding tools to ChatCompletionRequest caused other files to fail because Message needed tool_calls field for assistant messages
- **Fix:** Added tool_calls: Option<Vec<ToolCall>> to Message struct
- **Files modified:** src/native/types.rs
- **Verification:** Compilation succeeded
- **Committed in:** 251d6f3 (Task 3 commit)

**2. [Rule 3 - Blocking] Updated existing code with new fields**
- **Found during:** Task 3 (compilation failure)
- **Issue:** Multiple files constructing ChoiceMessage, Delta, and Message structs needed updating for new fields
- **Fix:** Added tool_calls: None to all struct constructions
- **Files modified:** src/native/streaming.rs, src/native/translate/openai.rs, src/native/translate/anthropic.rs
- **Verification:** All 142 native module tests pass
- **Committed in:** 251d6f3 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both auto-fixes were necessary to maintain compilable code. Adding new fields to structs requires updating all construction sites. No scope creep.

## Issues Encountered
None - linter helpfully added some struct field updates, speeding up the process.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Tool types ready for Plan 02 (translation layer)
- ToolCallDelta ready for Plan 03 (streaming accumulator)
- All validation functions (validate_tool_name, validate_tool_schema) ready for use
- Re-exports complete for external module access

---
*Phase: 05-tool-calling*
*Completed: 2026-02-01*
