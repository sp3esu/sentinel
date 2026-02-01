---
phase: 05-tool-calling
verified: 2026-02-01T12:30:00Z
status: passed
score: 5/5 must-haves verified
---

# Phase 5: Tool Calling Verification Report

**Phase Goal:** Support function/tool calling through unified schema with provider translation
**Verified:** 2026-02-01T12:30:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Tool definitions accepted in unified format (name, description, parameters JSON schema) | ✓ VERIFIED | ToolDefinition type exists, validates name/schema, used in ChatCompletionRequest.tools field |
| 2 | Tool schemas translate to OpenAI function format correctly | ✓ VERIFIED | OpenAITranslator.translate_request validates and serializes tools array to OpenAI format |
| 3 | Assistant tool calls return in unified format (tool_call_id, function name, arguments) | ✓ VERIFIED | OpenAITranslator.translate_response parses tool_calls and generates Sentinel IDs (call_{uuid}) |
| 4 | Tool results submitted and translated to provider format | ✓ VERIFIED | Tool messages with role:Tool look up function name from history, translate to OpenAI tool message format |
| 5 | Streaming with tool calls accumulates correctly and emits tool_call chunks | ✓ VERIFIED | ToolCallAccumulator accumulates deltas by index, finalizes with JSON parsing |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| src/native/types.rs | ToolDefinition, ToolCall, ToolResult, ToolChoice types | ✓ VERIFIED | 857 lines, contains all types, validate_tool_name, validate_tool_schema, 44 passing tests |
| src/native/request.rs | tools and tool_choice fields on ChatCompletionRequest | ✓ VERIFIED | tools: Option<Vec<ToolDefinition>>, tool_choice: Option<ToolChoice> fields present |
| src/native/response.rs | tool_calls field on ChoiceMessage and Delta | ✓ VERIFIED | ChoiceMessage.tool_calls, Delta.tool_calls, ToolCallDelta types present |
| src/native/translate/openai.rs | Tool translation in request and response | ✓ VERIFIED | 1397 lines, translate_tools in request, tool_calls parsing in response, history lookup for tool results |
| src/native/translate/mod.rs | ToolCallIdMapping for ID translation | ✓ VERIFIED | ToolCallIdMapping with generate_sentinel_id, bidirectional mapping |
| src/native/streaming.rs | ToolCallAccumulator for streaming | ✓ VERIFIED | 838 lines, accumulate/finalize methods, handles deltas by index |
| tests/integration/native_chat.rs | Tool calling integration tests | ✓ VERIFIED | 7 tool-related tests, all passing (22/22 total tests pass) |
| Cargo.toml | jsonschema dependency | ✓ VERIFIED | jsonschema = "0.29" present |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| src/native/request.rs | src/native/types.rs | ToolDefinition import | ✓ WIRED | use super::types::{ToolDefinition, ToolChoice} present |
| src/native/response.rs | src/native/types.rs | ToolCall import | ✓ WIRED | use super::types::ToolCall present |
| OpenAITranslator::translate_request | validate_tool_schema | Schema validation before translation | ✓ WIRED | Calls validate_tool_schema for each tool, returns InvalidToolDefinition on failure |
| OpenAITranslator::translate_request | validate_tool_name | Name validation before translation | ✓ WIRED | Calls validate_tool_name for each tool, returns InvalidToolDefinition on invalid name |
| Tool result translation | conversation history | Function name lookup by tool_call_id | ✓ WIRED | find_function_name_for_tool_call searches backwards through messages, returns MissingToolCallInHistory on failure |
| OpenAITranslator::translate_response | ToolCallIdMapping | Generate Sentinel IDs for tool calls | ✓ WIRED | Calls id_mapping.generate_sentinel_id for each provider tool_call_id |
| chat handler | translate_response | Uses tuple return (response, mapping) | ✓ WIRED | Destructures (native_response, _id_mapping) = translator.translate_response() |
| ToolCallAccumulator | finalize | JSON parsing of accumulated arguments | ✓ WIRED | serde_json::from_str on arguments, returns StreamError::ParseError on malformed JSON |

### Requirements Coverage

From ROADMAP.md Phase 5 success criteria:

| Success Criterion | Status | Supporting Evidence |
|------------------|--------|---------------------|
| 1. Tool definitions accepted in unified format (name, description, parameters JSON schema) | ✓ SATISFIED | ToolDefinition type with FunctionDefinition, validate_tool_name (alphanumeric+underscore), validate_tool_schema (type:object requirement) |
| 2. Tool schemas translate to OpenAI function format correctly | ✓ SATISFIED | OpenAITranslator validates and serializes tools array, test_translate_request_with_tools passes |
| 3. Assistant tool calls return in unified format (tool_call_id, function name, arguments) | ✓ SATISFIED | Response parsing generates Sentinel IDs (call_{uuid}), parses arguments to JSON objects, test_native_chat_tool_call_response verifies |
| 4. Tool results submitted and translated to provider format | ✓ SATISFIED | Tool messages search history for function name, translate to OpenAI tool message with name field, test_native_chat_tool_result_submission passes |
| 5. Streaming with tool calls accumulates correctly and emits tool_call chunks | ✓ SATISFIED | ToolCallAccumulator with index-based tracking, finalize parses JSON, test_tool_call_accumulator_* tests pass |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| src/native/translate/anthropic.rs | 121, 131 | "not implemented yet - scaffold for v2" | ℹ️ Info | Expected - Anthropic provider deferred to v2 per ROADMAP |

**No blockers found.** The "not implemented" patterns are in the Anthropic scaffold, which is intentionally incomplete for v1.

### Human Verification Required

None. All success criteria are programmatically verifiable and have been verified through:
- Unit tests (44 tool type tests, 17 streaming tests)
- Integration tests (7 tool calling flow tests)
- Build verification (compiles without errors)

---

## Detailed Verification Evidence

### Truth 1: Tool definitions accepted in unified format

**Verified through:**
1. **Type existence:** ToolDefinition struct at src/native/types.rs:193 with:
   - tool_type: String (serialized as "type")
   - function: FunctionDefinition (name, description, parameters)
2. **Validation functions:**
   - validate_tool_name at line 147: regex ^[a-zA-Z0-9_]+$ 
   - validate_tool_schema at line 155: checks type:object, validates with jsonschema crate
3. **Request integration:** ChatCompletionRequest.tools at src/native/request.rs:58
4. **Tests passing:**
   - test_validate_tool_name_valid_names: accepts valid names
   - test_validate_tool_name_invalid_names: rejects hyphens, spaces, special chars
   - test_validate_tool_schema_valid: accepts valid JSON schema
   - test_validate_tool_schema_wrong_type: rejects non-object schemas
   - test_native_chat_with_tools_request: end-to-end request with tools

### Truth 2: Tool schemas translate to OpenAI function format correctly

**Verified through:**
1. **Translation code:** src/native/translate/openai.rs:150-176
   - Validates each tool (name, description non-empty, schema)
   - Returns TranslationError::InvalidToolDefinition on validation failure
   - Serializes tools array to OpenAI format (already compatible)
2. **ToolChoice translation:** Lines 179-190
   - Auto/None/Required as strings
   - Function variant as object with type:function, function.name
3. **Tests passing:**
   - test_translate_request_with_tools: full translation roundtrip
   - test_native_chat_invalid_tool_name: rejects "invalid-name"
   - test_native_chat_empty_tool_description: rejects empty description
   - test_native_chat_tool_choice_variants: all ToolChoice variants translate

### Truth 3: Assistant tool calls return in unified format

**Verified through:**
1. **Response parsing:** src/native/translate/openai.rs:269-330
   - Extracts tool_calls array from OpenAI response
   - For each tool_call:
     - Generates Sentinel ID: id_mapping.generate_sentinel_id(provider_id)
     - Parses arguments from JSON string to JSON object
     - Returns TranslationError::MalformedArguments on parse failure
   - Returns (response, id_mapping) tuple
2. **ToolCall type:** src/native/types.rs:212
   - id: String (Sentinel format: call_{uuid})
   - call_type: "function"
   - function: ToolCallFunction with name and arguments (JSON object)
3. **Tests passing:**
   - test_native_chat_tool_call_response: verifies Sentinel ID format, JSON arguments
   - test_translate_response_with_tool_calls: unit test for response parsing
   - test_translate_response_malformed_tool_arguments: rejects invalid JSON

### Truth 4: Tool results submitted and translated to provider format

**Verified through:**
1. **History lookup:** src/native/translate/openai.rs:61-80
   - find_function_name_for_tool_call searches backwards through messages
   - Finds assistant message with matching tool_call.id
   - Extracts function.name from that tool_call
2. **Tool message translation:** Lines 94-124
   - Checks Role::Tool
   - Extracts tool_call_id (required field)
   - Calls find_function_name_for_tool_call
   - Returns MissingToolCallInHistory if not found
   - Builds OpenAI tool message with role, tool_call_id, name (from history), content
3. **Tests passing:**
   - test_native_chat_tool_result_submission: full flow with history lookup
   - test_translate_request_tool_result_with_history: unit test for lookup
   - test_translate_request_tool_result_not_found_in_history: errors correctly
   - test_translate_request_tool_message_missing_tool_call_id: requires tool_call_id
   - test_translate_request_multiple_tool_results: each finds correct name

### Truth 5: Streaming with tool calls accumulates correctly

**Verified through:**
1. **ToolCallAccumulator:** src/native/streaming.rs:271-347
   - HashMap<u32, AccumulatedToolCall> indexed by delta.index
   - accumulate method: builds up id, function_name, arguments string
   - finalize method:
     - Sorts by index to maintain order
     - Parses accumulated arguments as JSON
     - Returns StreamError::ParseError on malformed JSON
     - Returns Vec<(provider_id, ToolCall)> tuples
2. **ToolCallDelta type:** src/native/response.rs:75
   - index: u32 (critical for parallel tool calls)
   - id: Option<String> (only in first delta)
   - function: Option<ToolCallFunctionDelta> with name and arguments fragments
3. **Tests passing:**
   - test_tool_call_accumulator_finalize_parses_json: accumulates and parses
   - test_tool_call_accumulator_multiple_parallel_calls: handles multiple indices
   - test_tool_call_accumulator_malformed_arguments: rejects bad JSON
   - test_tool_call_accumulator_missing_id: requires ID
   - test_tool_call_accumulator_empty: handles empty case

---

## Summary

**All 5 success criteria verified.**

Phase 5 successfully implements tool calling for the Native API:

1. **Types foundation (Plan 05-01):** Complete type system with validation
2. **Translation (Plan 05-02):** Bidirectional OpenAI translation with ID mapping and history lookup
3. **Streaming (Plan 05-03):** Accumulator for streaming tool call deltas
4. **Integration:** All components wired, 22/22 integration tests passing

**Key accomplishments:**
- Tool definitions validated (name pattern, JSON schema structure)
- Sentinel-generated tool call IDs (call_{uuid} format) decouple from provider
- Arguments parsed to JSON objects (more ergonomic than string)
- Tool result translation uses history lookup (no name field on ToolResult)
- Streaming accumulation handles parallel tool calls by index

**Documented limitations (acceptable for v1):**
- Streaming tool calls use provider IDs (Sentinel ID translation would require buffering entire stream)
- Anthropic provider scaffold not implemented (deferred to v2 per ROADMAP)

**No gaps found.** Phase goal achieved.

---

_Verified: 2026-02-01T12:30:00Z_
_Verifier: Claude (gsd-verifier)_
