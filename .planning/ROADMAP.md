# Roadmap: Mindsmith Native API

## Overview

This roadmap delivers a provider-agnostic LLM API layer for Mindsmith. Starting with unified types and translation (foundation for everything), we build up through API endpoints, session management, tier routing, tool calling, and documentation. Each phase delivers a coherent capability that can be tested end-to-end. OpenAI is the only provider wired for v1; Anthropic and xAI inform the design but are deferred to v2.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [x] **Phase 1: Types and Translation** - Define unified message format and translate to/from OpenAI
- [x] **Phase 2: API Endpoints** - Create /native/* routes with basic request handling
- [ ] **Phase 3: Session Management** - Track conversations with provider stickiness
- [ ] **Phase 4: Tier Routing** - Map complexity tiers to models with config from Zion
- [ ] **Phase 5: Tool Calling** - Unified tool format with schema translation
- [ ] **Phase 6: Documentation** - OpenAPI spec with protected docs endpoint

## Phase Details

### Phase 1: Types and Translation
**Goal**: Establish the canonical message format that all providers translate to/from
**Depends on**: Nothing (first phase)
**Requirements**: TYPE-01, TYPE-02, TYPE-03, TYPE-04, TRNS-01, TRNS-02, TRNS-03, TRNS-04
**Success Criteria** (what must be TRUE):
  1. Native API accepts messages in unified format (role + content) and rejects malformed input
  2. System prompts in any position translate correctly to OpenAI format
  3. Streaming responses emit normalized SSE chunks regardless of provider format
  4. Errors from providers return unified error response with code, message, and provider hint
  5. Anthropic translation logic exists (validates strict alternation) even though provider not wired
**Plans**: 4 plans in 2 waves

Plans:
- [x] 01-01-PLAN.md - Unified types definition (Wave 1)
- [x] 01-02-PLAN.md - OpenAI translator (Wave 2)
- [x] 01-03-PLAN.md - Streaming normalization (Wave 2)
- [x] 01-04-PLAN.md - Error handling and Anthropic translator scaffold (Wave 2)

### Phase 2: API Endpoints
**Goal**: Expose /native/* endpoints that accept unified format and return responses
**Depends on**: Phase 1
**Requirements**: API-01, API-02, API-03, API-04
**Success Criteria** (what must be TRUE):
  1. POST /native/chat/completions accepts unified request and returns completion
  2. Streaming mode returns SSE chunks ending with [DONE]
  3. Non-streaming mode returns complete response in single JSON body
  4. Existing /v1/* endpoints work unchanged (regression-free)
**Plans**: 2 plans in 2 waves

Plans:
- [x] 02-01-PLAN.md - Chat completions endpoint with streaming + non-streaming (Wave 1)
- [x] 02-02-PLAN.md - Integration tests and /v1/* regression verification (Wave 2)

### Phase 3: Session Management
**Goal**: Track conversations to ensure consistent provider selection within a session
**Depends on**: Phase 2
**Requirements**: SESS-01, SESS-02, SESS-03, SESS-04
**Success Criteria** (what must be TRUE):
  1. Requests with conversation_id use provider stored for that session
  2. First request in a session stores provider selection in Redis
  3. Requests without conversation_id trigger fresh provider selection each time
  4. Session data expires after 24 hours of inactivity
**Plans**: TBD

Plans:
- [ ] 03-01: Session storage and retrieval
- [ ] 03-02: Session-aware routing integration

### Phase 4: Tier Routing
**Goal**: Map complexity tiers to specific models based on configuration from Zion
**Depends on**: Phase 3
**Requirements**: TIER-01, TIER-02, TIER-03, TIER-04, TIER-05, TIER-06
**Success Criteria** (what must be TRUE):
  1. API accepts tier (simple | moderate | complex) and selects appropriate model
  2. Model configuration loads from Zion API with caching
  3. Fallback configuration used when Zion unavailable
  4. Provider selection prefers cheaper option when multiple available for tier
  5. Unavailable providers (rate limited, down) are skipped in selection
**Plans**: TBD

Plans:
- [ ] 04-01: Tier configuration and Zion integration
- [ ] 04-02: Provider selection logic with cost awareness
- [ ] 04-03: Availability checking and fallback handling

### Phase 5: Tool Calling
**Goal**: Support function/tool calling through unified schema with provider translation
**Depends on**: Phase 4
**Requirements**: TOOL-01, TOOL-02, TOOL-03, TOOL-04, TOOL-05, TOOL-06
**Success Criteria** (what must be TRUE):
  1. Tool definitions accepted in unified format (name, description, parameters JSON schema)
  2. Tool schemas translate to OpenAI function format correctly
  3. Assistant tool calls return in unified format (tool_call_id, function name, arguments)
  4. Tool results submitted and translated to provider format
  5. Streaming with tool calls accumulates correctly and emits tool_call chunks
**Plans**: TBD

Plans:
- [ ] 05-01: Tool schema translation
- [ ] 05-02: Tool call/result handling
- [ ] 05-03: Streaming with tool calls

### Phase 6: Documentation
**Goal**: Provide OpenAPI specification for the Native API with protected access
**Depends on**: Phase 5
**Requirements**: DOCS-01, DOCS-02, DOCS-03, DOCS-04
**Success Criteria** (what must be TRUE):
  1. OpenAPI 3.x specification accurately describes all /native/* endpoints
  2. GET /native/docs returns the specification
  3. Docs endpoint requires dedicated API key (not JWT)
  4. Specification includes all request/response schemas with examples
**Plans**: TBD

Plans:
- [ ] 06-01: OpenAPI spec generation with utoipa
- [ ] 06-02: Docs endpoint with API key protection

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Types and Translation | 4/4 | Complete | 2026-01-31 |
| 2. API Endpoints | 2/2 | Complete | 2026-02-01 |
| 3. Session Management | 0/2 | Not started | - |
| 4. Tier Routing | 0/3 | Not started | - |
| 5. Tool Calling | 0/3 | Not started | - |
| 6. Documentation | 0/2 | Not started | - |

---
*Roadmap created: 2026-01-31*
*Total: 6 phases, 16 plans, 32 requirements*
