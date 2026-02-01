# Requirements: Mindsmith Native API

**Defined:** 2026-01-31
**Core Value:** Mindsmith can build and chat with assistants using any LLM provider through a single, stable API

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Unified Types

- [x] **TYPE-01**: Native API accepts unified message format (role, content)
- [x] **TYPE-02**: System prompts handled uniformly regardless of provider
- [x] **TYPE-03**: Common parameters supported (temperature, max_tokens, top_p, stop)
- [x] **TYPE-04**: Unified error response format across all providers

### Message Translation

- [x] **TRNS-01**: Messages translated to OpenAI format
- [x] **TRNS-02**: Messages translated to Anthropic format (strict alternation enforced)
- [x] **TRNS-03**: Streaming chunks normalized to unified format
- [x] **TRNS-04**: Provider-specific response fields stripped from output

### Tool Calling

- [ ] **TOOL-01**: Unified tool definition format (name, description, parameters)
- [ ] **TOOL-02**: Tool schemas translated to OpenAI function format
- [ ] **TOOL-03**: Tool schemas translated to Anthropic tool format
- [ ] **TOOL-04**: Tool call responses normalized to unified format
- [ ] **TOOL-05**: Tool results accepted and translated to provider format
- [ ] **TOOL-06**: Streaming with tool calls handled correctly

### Session Management

- [x] **SESS-01**: Conversation ID tracks session
- [x] **SESS-02**: Provider selection stored per session in Redis
- [x] **SESS-03**: Same provider used for all requests in a session
- [x] **SESS-04**: New session triggers fresh provider selection

### Tier Routing

- [x] **TIER-01**: API accepts tier level (simple, moderate, complex)
- [x] **TIER-02**: Tier maps to available models from configuration
- [x] **TIER-03**: Provider selected based on cost (prefer cheaper)
- [x] **TIER-04**: Unavailable providers skipped (rate limited, down)
- [x] **TIER-05**: Model configuration loaded from Zion API
- [x] **TIER-06**: Fallback configuration when Zion unavailable

### API Endpoints

- [x] **API-01**: POST /native/chat/completions for chat requests
- [x] **API-02**: Streaming response via SSE
- [x] **API-03**: Non-streaming response option
- [x] **API-04**: Existing /v1/* endpoints unchanged

### OpenAPI Documentation

- [ ] **DOCS-01**: OpenAPI 3.x specification for Native API
- [ ] **DOCS-02**: GET /native/docs returns OpenAPI spec
- [ ] **DOCS-03**: Docs endpoint protected by dedicated API key
- [ ] **DOCS-04**: Spec includes all request/response schemas

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Additional Providers

- **PROV-01**: Anthropic provider implementation
- **PROV-02**: xAI/Grok provider implementation

### Extended Features

- **EXT-01**: Vision/image support in messages
- **EXT-02**: File attachment support
- **EXT-03**: Extended thinking token handling (Anthropic)
- **EXT-04**: Structured output support

### Advanced Routing

- **ROUT-01**: Latency-based provider selection
- **ROUT-02**: Load balancing across providers
- **ROUT-03**: Automatic failover with session migration

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Direct model names in API | Abstracted behind tiers — prevents client-provider coupling |
| Provider-specific parameters | Leaky abstraction — use unified params only |
| Multiple completions (n > 1) | Not universally supported, adds complexity |
| Silent failover | Client should control UX — errors bubble up |
| Server-side conversation storage | Client sends full history — keeps Sentinel stateless |
| Response metadata (tokens, cost) | Client doesn't need it — usage tracked internally |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| TYPE-01 | Phase 1 | Complete |
| TYPE-02 | Phase 1 | Complete |
| TYPE-03 | Phase 1 | Complete |
| TYPE-04 | Phase 1 | Complete |
| TRNS-01 | Phase 1 | Complete |
| TRNS-02 | Phase 1 | Complete |
| TRNS-03 | Phase 1 | Complete |
| TRNS-04 | Phase 1 | Complete |
| TOOL-01 | Phase 5 | Pending |
| TOOL-02 | Phase 5 | Pending |
| TOOL-03 | Phase 5 | Pending |
| TOOL-04 | Phase 5 | Pending |
| TOOL-05 | Phase 5 | Pending |
| TOOL-06 | Phase 5 | Pending |
| SESS-01 | Phase 3 | Complete |
| SESS-02 | Phase 3 | Complete |
| SESS-03 | Phase 3 | Complete |
| SESS-04 | Phase 3 | Complete |
| TIER-01 | Phase 4 | Complete |
| TIER-02 | Phase 4 | Complete |
| TIER-03 | Phase 4 | Complete |
| TIER-04 | Phase 4 | Complete |
| TIER-05 | Phase 4 | Complete |
| TIER-06 | Phase 4 | Complete |
| API-01 | Phase 2 | Complete |
| API-02 | Phase 2 | Complete |
| API-03 | Phase 2 | Complete |
| API-04 | Phase 2 | Complete |
| DOCS-01 | Phase 6 | Pending |
| DOCS-02 | Phase 6 | Pending |
| DOCS-03 | Phase 6 | Pending |
| DOCS-04 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 32 total
- Mapped to phases: 32
- Unmapped: 0

---
*Requirements defined: 2026-01-31*
*Last updated: 2026-01-31 after roadmap creation*
