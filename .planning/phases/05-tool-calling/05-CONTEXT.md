# Phase 5: Tool Calling - Context

**Gathered:** 2026-02-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Support function/tool calling through unified schema with provider translation. Accept tool definitions in unified format, translate to OpenAI function format, return tool calls in unified format, handle tool results. OpenAI is the only provider wired for v1.

</domain>

<decisions>
## Implementation Decisions

### Tool schema format
- Strict JSON Schema validation — reject tools with invalid schemas before sending to provider
- Tool descriptions required — force descriptions on tools and parameters for better LLM performance
- No strict mode for v1 — skip OpenAI's structured outputs 'strict: true' flag, add later if needed
- Naming convention: alphanumeric + underscore — matches OpenAI's validation (a-zA-Z0-9_)

### Tool call response
- Generate Sentinel-specific tool_call_id — consistent across providers, requires ID mapping
- Parallel tool calls as single message with array — matches OpenAI format, one assistant message with tool_calls array
- Arguments returned as parsed JSON object — more ergonomic than OpenAI's JSON string
- Malformed arguments return error — fail the request with clear error rather than returning raw string

### Tool result handling
- Unified result format — custom format with structured fields (tool_call_id, content, is_error) rather than OpenAI's tool message
- Accept string or JSON for content — we serialize to string for provider
- Reject unknown tool_call_id with 400 error — strict validation catches client bugs early
- Include optional 'is_error' flag — allows marking results as errors so LLM handles failures appropriately

### Provider translation
- Tools format only — use OpenAI's newer 'tools' format, don't support deprecated 'function' format
- OpenAI only for v1 — no Anthropic scaffolding, add when needed
- Stream tool deltas — emit tool_call deltas as they arrive, matches OpenAI streaming exactly
- Full tool_choice support — 'auto', 'none', 'required', and specific tool selection

### Claude's Discretion
- Exact ID format and generation strategy for tool_call_id
- JSON Schema validation library choice
- Internal mapping structure for tool_call_id translation
- Delta accumulation strategy for streaming

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 05-tool-calling*
*Context gathered: 2026-02-01*
