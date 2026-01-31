# Project Research Summary

**Project:** Sentinel Native API - Provider-Agnostic LLM Gateway
**Domain:** Multi-provider LLM API abstraction layer
**Researched:** 2026-01-31
**Confidence:** HIGH

## Executive Summary

Sentinel's Native API will provide a unified, tier-based abstraction layer over OpenAI, Anthropic, and xAI/Grok. The research confirms that building a custom translation layer on top of Sentinel's existing `AiProvider` trait is the right approach rather than adopting third-party unified LLM libraries. This leverages existing infrastructure while maintaining full control over request/response handling.

The core architectural insight is that while xAI is OpenAI-compatible (requiring minimal new code), Anthropic has fundamental structural differences in message format, tool calling, and streaming that must be handled through explicit translation. The recommended approach is to define a canonical "Native API" format that represents the intersection of all provider capabilities, then build bidirectional translators for each provider. This prevents leaky abstractions and ensures consistent behavior across providers.

The critical risks are message format incompatibility (OpenAI allows system messages anywhere, Anthropic requires strict alternation), tool calling schema mismatches (sequential vs parallel, different wrapper structures), and streaming chunk format divergence. These must be addressed in the foundation layers before building higher-level features. Session stickiness is essential to prevent mid-conversation provider switches that would break context continuity.

## Key Findings

### Recommended Stack

Sentinel should continue using its proven Rust/Axum/reqwest stack rather than adopting unified LLM libraries. The existing `AiProvider` trait provides exactly the abstraction needed, and reqwest gives fine-grained control over HTTP interactions that would be lost with higher-level SDKs.

**Core technologies:**
- **Rust 1.83+ with Axum 0.7**: Keep existing framework, defer Axum 0.8 upgrade to separate effort
- **reqwest 0.12**: HTTP client with connection pooling, already proven in production
- **tiktoken-rs 0.9.1**: OpenAI/xAI token counting (shared tokenizer family)
- **claude-tokenizer 0.3**: Anthropic token counting (offline estimation, accept variance)
- **utoipa 5.x**: OpenAPI documentation with Axum 0.7 support (use utoipa-axum 0.1.x or defer integration)

**Critical decision:** Do NOT adopt rust-genai, llm (graniet), or async-openai. These unified libraries add abstraction layers that duplicate what Sentinel already has, introduce new error types, and reduce debugging clarity. The xAI API is OpenAI-compatible, so only Anthropic requires new provider code.

### Expected Features

**Must have (table stakes for MVP):**
- Message format normalization between providers (system message handling, role alternation)
- Basic chat completion (non-streaming first, then streaming)
- Tier-based model selection (simple/moderate/complex abstracts model names)
- Streaming responses with unified SSE format
- Usage tracking with provider-reported token counts
- Session stickiness (same provider throughout conversation)

**Should have (competitive differentiators):**
- Tool/function calling with schema translation (OpenAI parallel tools → Anthropic sequential)
- Tier-to-model configuration via Zion API (runtime changes without redeployment)
- Provider routing based on availability and priority
- OpenAPI documentation for client discovery

**Defer to v2+:**
- Structured outputs (complex schema differences, Anthropic feature just launched)
- Vision/image inputs (text-only for v1)
- Extended thinking/reasoning exposure (map to complexity tier transparently)
- Multiple simultaneous providers wired (start OpenAI-only, add Anthropic later)
- Provider failover (errors bubble up, client controls retry)

### Architecture Approach

The Native API architecture follows proven LLM gateway patterns (LiteLLM, TensorZero, OpenRouter) with five core components: unified request/response types, request translator, session manager, tier router, and provider registry. This design maintains clean separation between the provider-agnostic API layer and provider-specific implementations.

**Major components:**
1. **Unified Types** (`native/types.rs`) — Canonical message/tool format representing intersection of all providers
2. **Request Translator** (`native/translator/{openai,anthropic}.rs`) — Bidirectional format conversion with provider-specific logic
3. **Session Manager** (`native/session.rs`) — Redis-backed conversation tracking for provider stickiness (24h TTL)
4. **Tier Router** (`native/router.rs`) — Maps simple/moderate/complex to provider+model based on Zion config
5. **Provider Registry** (`native/providers/mod.rs`) — Manages provider instances and dispatches translated requests

**Critical design decision:** The canonical format must be the MOST RESTRICTIVE intersection of all providers (e.g., no parallel tool calls since Anthropic is sequential, strict user/assistant alternation). This prevents provider-specific features from leaking into the unified API.

### Critical Pitfalls

1. **Message Format Incompatibility** — OpenAI allows system messages anywhere and consecutive same-role messages; Anthropic requires strict user/assistant alternation and hoists system messages. Prevention: Define canonical format first, validate at API boundary, test multi-turn conversations across all providers. Address in Phase 1 (foundation).

2. **Tool Calling Schema Mismatch** — OpenAI wraps tools in `{type: "function", function: {...}}` with `parameters`; Anthropic uses flat `{name, input_schema}`. OpenAI supports parallel calls; Anthropic is sequential. Prevention: Build sequential-only tool calling in unified API, translate for each provider, validate tool_call_id correlation. Address in Phase 2 (after basic messages work).

3. **Streaming Chunk Format Divergence** — OpenAI uses single event type with `delta` object; Anthropic uses named events (`content_block_delta`, `message_delta`). Prevention: NEVER pass through raw chunks, always normalize to canonical format, test with short/long/tool-call/error streams. Address in Phase 1 (critical path).

4. **Session Stickiness Violation** — Mid-conversation provider switches break context due to format incompatibility. Prevention: Track session_id explicitly, enforce sticky routing within conversations, only switch on explicit new session or expiry. Address in Phase 4 (routing).

5. **Token Counting Discrepancies** — Different tokenizers produce 10-20% variance. Prevention: Use provider-reported tokens for billing/quota, tolerate estimation errors for pre-flight checks, track actual vs estimated per provider. Address in Phase 3 (usage tracking).

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Foundation - Types and Message Translation
**Rationale:** Types and translation must exist before any other component can function. Streaming is critical path for user experience.
**Delivers:** Unified message format, OpenAI translator, basic non-streaming chat, streaming SSE normalization
**Addresses:** Message format incompatibility (Pitfall #1), Streaming divergence (Pitfall #4)
**Stack:** Uses existing Axum/reqwest, defines canonical types in `native/types.rs`
**Avoids:** Building routing or sessions before having working translation layer

### Phase 2: Session Management and Routing
**Rationale:** Session stickiness must work before adding multiple providers or complex routing logic.
**Delivers:** Redis-backed session tracking, tier-based routing with hardcoded fallback config
**Implements:** SessionManager component (Architecture #3), TierRouter component (Architecture #4)
**Addresses:** Session stickiness violation (Pitfall #5)
**Stack:** Uses existing Redis infrastructure from rate limiting

### Phase 3: OpenAI Provider Integration
**Rationale:** Wire complete flow with single provider before adding multi-provider complexity.
**Delivers:** End-to-end OpenAI chat completion (streaming + non-streaming), usage tracking
**Implements:** ProviderRegistry component (Architecture #5)
**Addresses:** Token counting with tiktoken-rs (existing), usage reporting to Zion
**Avoids:** Multi-provider routing logic before single provider proven

### Phase 4: Tool Calling Support
**Rationale:** Tool calling is required for Mindsmith but complex enough to isolate after core flow works.
**Delivers:** Tool schema translation (OpenAI format), tool call/result handling, streaming with tools
**Addresses:** Tool calling schema mismatch (Pitfall #2), Strict validation gaps (Pitfall #8)
**Uses:** Extends existing translator with tool-specific logic
**Note:** Sequential-only for MVP; no parallel tool calls

### Phase 5: Anthropic Provider (Future)
**Rationale:** Defer second provider until OpenAI flow is stable and proven. High complexity due to format differences.
**Delivers:** AnthropicProvider implementation, Anthropic translator, Claude token counting
**Implements:** Alternative provider path using same architecture
**Addresses:** Required parameter injection (Pitfall #7), extended thinking leakage (Pitfall #3)
**Stack:** Add claude-tokenizer or use Anthropic's `/v1/messages/count_tokens` endpoint

### Phase 6: OpenAPI Documentation
**Rationale:** Documentation can be added after API is stable; non-blocking for functionality.
**Delivers:** utoipa-generated OpenAPI schema, Swagger UI endpoint at `/native/docs`
**Stack:** utoipa 5.x with utoipa-axum 0.1.x (Axum 0.7 compatibility)
**Note:** API key auth for docs endpoint (separate from JWT)

### Phase Ordering Rationale

- **Types first** because everything depends on canonical format definition
- **Translation before routing** because routing decisions require translated requests
- **Single provider before multi-provider** to prove architecture without routing complexity
- **Tool calling after basic messages** because tools depend on message translation working
- **Anthropic deferred** because it's the most complex provider and OpenAI proves the pattern
- **Documentation last** because API surface must stabilize first

**Dependency chain:**
```
Types → Translation → Session/Routing → Provider Integration → Tool Calling
                                              ↓
                                     Additional Providers (Anthropic, xAI)
                                              ↓
                                        Documentation
```

### Research Flags

**Phases needing deeper research during planning:**
- **Phase 4 (Tool Calling):** Complex schema translation, parallel vs sequential differences, validation strategies - will need focused research during implementation
- **Phase 5 (Anthropic):** Content blocks vs simple strings, system message hoisting, extended thinking handling - official docs thorough but needs careful testing

**Phases with standard patterns (skip research-phase):**
- **Phase 1 (Types/Translation):** Well-understood pattern from ARCHITECTURE.md, similar to existing proxy code
- **Phase 2 (Session/Routing):** Redis session tracking similar to existing limits cache, routing is config lookup
- **Phase 3 (OpenAI):** Provider already implemented, just needs wiring through new translation layer
- **Phase 6 (Documentation):** utoipa is well-documented, standard integration pattern

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Existing Rust/Axum stack proven, reqwest approach verified by research, clear rationale for rejecting unified libraries |
| Features | HIGH | Official API docs for all three providers verified, feature intersection well-documented |
| Architecture | HIGH | Pattern verified against industry implementations (LiteLLM, TensorZero, OpenRouter), matches existing Sentinel patterns |
| Pitfalls | MEDIUM | Critical pitfalls sourced from official compatibility docs and real bug reports, but some edge cases may surface in implementation |

**Overall confidence:** HIGH

### Gaps to Address

**Token counting accuracy:** claude-tokenizer is unofficial and may have 10-20% variance vs Anthropic's actual tokenizer. Strategy: Accept variance for pre-flight estimates, always use provider-reported tokens for billing/quota. Consider switching to Anthropic's `/v1/messages/count_tokens` endpoint if accuracy becomes critical.

**xAI API stability:** xAI is newest provider with evolving API (Anthropic SDK compatibility deprecated, Live Search being replaced). Strategy: Treat xAI as OpenAI-compatible but build provider-specific test suite, monitor changelog actively, only use stable documented features in production.

**Prompt engineering variance:** Prompts optimized for GPT-4 may perform poorly on Claude. Gap: Research doesn't cover prompt optimization strategies. Strategy: Test system prompts on all providers, consider provider-specific variants if quality variance significant, document during Phase 5.

**Streaming chunk accumulation:** How to handle very long streaming responses for token counting? Gap: Research confirms need to accumulate but doesn't cover memory limits. Strategy: Implement streaming chunk buffer with size limit, fall back to estimation if buffer exceeded.

**Error translation completeness:** Error formats differ across providers. Gap: Research identifies the issue but doesn't provide complete error mapping. Strategy: Define unified error enum during Phase 1, expand mappings as errors discovered in testing.

## Sources

### Primary (HIGH confidence)
- [xAI REST API Reference](https://docs.x.ai/docs/api-reference) - Confirmed OpenAI compatibility
- [Anthropic Messages API](https://docs.anthropic.com/en/api/messages) - Message format, streaming, tool use
- [Anthropic OpenAI SDK Compatibility](https://platform.claude.com/docs/en/api/openai-sdk) - System message hoisting, strict parameter ignored, extended thinking
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat) - Message format, streaming, function calling
- [tiktoken-rs GitHub](https://github.com/zurawiki/tiktoken-rs) - v0.9.1, model encoders
- [utoipa GitHub](https://github.com/juhaku/utoipa) - v5.x with Axum support
- [Axum 0.8.0 Announcement](https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0) - Breaking changes documented

### Secondary (MEDIUM confidence)
- [LiteLLM Anthropic Provider](https://docs.litellm.ai/docs/providers/anthropic) - Real-world translation challenges, max_tokens requirement
- [LiteLLM GitHub Issue #16215](https://github.com/BerriAI/litellm/issues/16215) - Tool calling translation bugs
- [TensorZero Gateway](https://www.tensorzero.com/docs/gateway) - Rust-based LLM gateway architecture
- [RouteLLM Framework](https://lmsys.org/blog/2024-07-01-routellm/) - Routing algorithms
- [OpenAI vs Anthropic API Guide](https://www.eesel.ai/blog/openai-api-vs-anthropic-api) - Format comparison
- [Portkey Token Tracking](https://portkey.ai/blog/tracking-llm-token-usage-across-providers-teams-and-workloads/) - Tokenizer variance
- [claude-tokenizer crate](https://crates.io/crates/claude-tokenizer) - v0.3.x, unofficial implementation

### Tertiary (LOW confidence)
- [rust-genai](https://github.com/jeremychone/rust-genai) - v0.5.x, not recommended but verified as alternative
- [llm (graniet)](https://github.com/graniet/llm) - v1.2.4, comprehensive but overkill for proxy use case

---
*Research completed: 2026-01-31*
*Ready for roadmap: yes*
