# Phase 1: Types and Translation - Context

**Gathered:** 2026-01-31
**Status:** Ready for planning

<domain>
## Phase Boundary

Establish the canonical message format that all providers translate to/from. Define unified types for requests/responses, translate to OpenAI format, normalize streaming chunks, and standardize error responses. Anthropic translation validates design but isn't wired to a live provider in v1.

</domain>

<decisions>
## Implementation Decisions

### Message format
- Use OpenAI-style roles: user, assistant, system, tool
- Content can be string OR array of parts (text/image_url) for multimodal support
- System messages must be first — reject if system appears elsewhere
- Metadata fields (name, tool_call_id) added only when relevant

### Error contract
- Unified errors only — hide provider-specific details from callers
- Use OpenAI-compatible error structure: `{error: {message, type, code}}`
- Rate limit errors include Retry-After header and value in body
- Validation error reporting: Claude's discretion on single vs. all errors

### Streaming format
- OpenAI-compatible SSE chunk format — existing client libraries work unchanged
- Final chunk always includes usage stats (input/output token counts)
- Mid-stream error handling: Claude's discretion on error chunk vs. close
- Heartbeat/keep-alive: Claude's discretion based on infrastructure needs

### Validation strictness
- Strict validation — reject requests with unknown fields
- Empty content handling: Claude's discretion based on provider acceptance
- Message order validation: Claude's discretion based on provider requirements
- Parameter range validation: Claude's discretion on upfront vs. passthrough

### Claude's Discretion
- Message metadata: which optional fields to support based on OpenAI compatibility
- Validation error aggregation: single first error or collect all
- Stream error handling: error chunk before close vs. abrupt close
- Keep-alive mechanism: SSE comments vs. TCP keep-alive only
- Empty content: allow or reject based on provider behavior
- Message alternation: validate upfront or let provider handle
- Parameter ranges: check upfront or pass through to provider

</decisions>

<specifics>
## Specific Ideas

- "OpenAI-compatible" is the guiding principle — developers shouldn't have to learn a new format
- Mindsmith's assistants already speak OpenAI format, so translation layer should be invisible to them
- Strict unknown field rejection catches typos and enforces the contract clearly

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-types-and-translation*
*Context gathered: 2026-01-31*
