# Domain Pitfalls: Provider-Agnostic LLM API

**Domain:** Unified LLM API layer over OpenAI, Anthropic, xAI/Grok
**Researched:** 2026-01-31
**Confidence:** MEDIUM (verified with official documentation where available)

---

## Critical Pitfalls

Mistakes that cause rewrites or major issues.

---

### Pitfall 1: Message Format Incompatibility

**What goes wrong:** OpenAI and Anthropic have fundamentally different message structures. OpenAI allows system messages anywhere in conversation and back-to-back messages from the same role. Anthropic requires strict user/assistant alternation and only supports a single initial system message.

**Why it happens:** Teams build translation assuming formats are similar with minor differences. They discover mid-implementation that the differences are structural, not cosmetic.

**Consequences:**
- Multi-turn conversations break when switching providers
- System prompts placed mid-conversation get lost or concatenated unexpectedly
- Context injection patterns that work on OpenAI fail silently on Anthropic

**Prevention:**
1. Define a canonical internal message format that is the INTERSECTION of all providers (most restrictive)
2. Validate messages against this format at API boundary, before routing
3. Never store provider-specific formats; always convert to/from canonical format
4. Test multi-turn conversations with 10+ messages across all providers

**Detection (warning signs):**
- "Works on OpenAI but not Anthropic" bug reports
- System prompts appearing in wrong locations in Claude responses
- Conversation context appearing corrupted after provider switches

**Phase to address:** Phase 1 (Message Translation Layer) - must be foundational

**Sources:**
- [Anthropic OpenAI SDK Compatibility](https://platform.claude.com/docs/en/api/openai-sdk) - System/developer messages are hoisted and concatenated
- [LiteLLM Anthropic Docs](https://docs.litellm.ai/docs/providers/anthropic) - Documents translation challenges

---

### Pitfall 2: Tool Calling Schema Mismatch

**What goes wrong:** OpenAI function calling and Anthropic tool use have different structures. OpenAI supports parallel tool calls; Anthropic is sequential. OpenAI uses `function_call` objects; Anthropic uses `tool_use` and `tool_result` blocks inline with content.

**Why it happens:** Both are "tool calling" conceptually, so teams assume a simple field renaming will work. The structural differences in how tools are invoked and results returned are discovered late.

**Consequences:**
- Tool calls fail silently on Anthropic when translating from OpenAI format
- Parallel tool call patterns break entirely on Anthropic
- Tool results get mismatched with wrong tool call IDs
- Complex agent workflows become provider-specific

**Prevention:**
1. Build tool calling as a separate abstraction layer, not inline translation
2. For multi-provider support, accept only SEQUENTIAL tool calling in unified API (lowest common denominator)
3. If parallel tools needed, make it an OpenAI-specific enhancement that gracefully degrades
4. Test tool calling with: single tool, multiple sequential tools, tool with complex JSON input
5. Validate tool_call_id correlation explicitly

**Detection (warning signs):**
- "Requests with tools fail silently" - classic symptom
- Tool results not appearing in subsequent model responses
- Agent loops hanging after tool calls

**Phase to address:** Phase 2 (Tool Calling Unification) - after basic message format works

**Sources:**
- [LiteLLM GitHub Issue #16215](https://github.com/BerriAI/litellm/issues/16215) - "Requests with tools fail silently (Responses API tool call format is not translated to Anthropic format)"
- [OpenAI vs Anthropic API Guide](https://www.eesel.ai/blog/openai-api-vs-anthropic-api) - Documents structural differences

---

### Pitfall 3: Extended Thinking / Reasoning Token Leakage

**What goes wrong:** Claude's extended thinking produces `thinking` blocks that have no OpenAI equivalent. If you expose OpenAI-compatible API and route to Claude with extended thinking enabled, clients receive unexpected response structures.

**Why it happens:** Extended thinking is enabled for quality improvement, but the output format change isn't handled. The OpenAI SDK compatibility layer notes: "the OpenAI SDK won't return Claude's detailed thought process."

**Consequences:**
- Clients parsing OpenAI-format responses crash on unexpected fields
- Token counts are wrong (thinking tokens counted differently)
- Streaming chunk format changes unexpectedly
- Cost calculations are incorrect

**Prevention:**
1. If exposing OpenAI-compatible API, STRIP thinking blocks from responses before returning
2. If you want thinking exposed, create a custom response format (not OpenAI-compatible)
3. Handle thinking tokens separately in usage tracking
4. Document clearly whether extended thinking is enabled per model/tier

**Detection (warning signs):**
- Unexpected fields in API responses
- Token counts from proxy don't match provider bills
- Streaming responses have different chunk structures than expected

**Phase to address:** Phase 3 (Response Normalization) - after basic routing works

**Sources:**
- [Anthropic Extended Thinking Docs](https://platform.claude.com/docs/en/build-with-claude/extended-thinking)
- [Anthropic OpenAI SDK Compatibility](https://platform.claude.com/docs/en/api/openai-sdk) - "While this will improve Claude's reasoning for complex tasks, the OpenAI SDK won't return Claude's detailed thought process"

---

### Pitfall 4: Streaming Chunk Format Divergence

**What goes wrong:** OpenAI, Anthropic, and xAI all use SSE for streaming but with different chunk structures. OpenAI provides usage in the final chunk; Anthropic provides input tokens at stream start and output at end; content deltas have different field names.

**Why it happens:** Teams implement streaming pass-through without chunk transformation, assuming SSE is SSE. Clients break when chunk format doesn't match expected provider.

**Consequences:**
- Clients fail to parse streaming responses from non-primary provider
- Token counting fails mid-stream
- Stream termination not detected correctly
- UI shows raw chunks instead of rendered content

**Prevention:**
1. NEVER pass through raw chunks; always normalize to canonical chunk format
2. Define your own streaming chunk format that captures all provider semantics
3. Accumulate content for token counting; don't rely on mid-stream token fields
4. Test streaming with: short responses, very long responses, tool calls in stream, error mid-stream

**Detection (warning signs):**
- "Works non-streaming but breaks streaming" reports
- Streaming responses truncated or malformed
- Token counts missing for streaming requests

**Phase to address:** Phase 1 (Core Streaming) - critical path, must work early

**Sources:**
- [Streaming LLM Responses Guide](https://dev.to/pockit_tools/the-complete-guide-to-streaming-llm-responses-in-web-applications-from-sse-to-real-time-ui-3534) - "Handle Fragmentation Gracefully - always anticipate that AI model responses may come in multiple fragments"
- [Simon Willison - How Streaming LLM APIs Work](https://til.simonwillison.net/llms/streaming-llm-apis)

---

### Pitfall 5: Session Stickiness Violation

**What goes wrong:** User starts conversation with Claude, mid-conversation gets routed to GPT-4 due to load balancing or tier change. Context format incompatibility causes conversation to break.

**Why it happens:** Routing logic treats each request independently without considering conversation continuity. Load balancing or failover logic ignores session affinity.

**Consequences:**
- Conversations lose context mid-stream
- Users experience jarring behavior changes
- Tool call IDs from previous provider don't match
- Cached prompts invalid for new provider

**Prevention:**
1. Track conversation_id or session_id explicitly
2. Route MUST be sticky within a conversation unless explicitly requested
3. If provider must change mid-conversation, translate ENTIRE conversation history to new format
4. Failover should only activate for new conversations, not mid-conversation

**Detection (warning signs):**
- "AI forgot what we were talking about" user reports
- Tool call mismatches after what seems like a normal response
- Inconsistent personality/behavior within single conversation

**Phase to address:** Phase 4 (Routing & Session Management) - after individual providers work

**Sources:**
- [Multi-provider LLM Orchestration Guide](https://dev.to/ash_dubai/multi-provider-llm-orchestration-in-production-a-2026-guide-1g10) - "A bug in routing logic could accidentally use the most expensive model for everything"
- [Portkey Failover Strategies](https://portkey.ai/blog/failover-routing-strategies-for-llms-in-production/)

---

## Moderate Pitfalls

Mistakes that cause delays or technical debt.

---

### Pitfall 6: Token Counting Discrepancies

**What goes wrong:** Different providers use different tokenizers. Tiktoken (OpenAI) produces different counts than Claude's tokenizer. Pre-flight token estimates don't match actual usage.

**Why it happens:** Teams use a single tokenizer (typically tiktoken) for all providers, assuming close-enough accuracy.

**Consequences:**
- Quota enforcement is inaccurate (over or under by 10-20%)
- Cost estimates don't match bills
- Context window calculations fail near limits
- Rate limiting triggers incorrectly

**Prevention:**
1. For quota enforcement, use provider-reported tokens (post-response), not estimates
2. For pre-flight estimation, use provider-specific tokenizers or accept 15% error margin
3. Track estimated vs actual per provider to calibrate
4. Never block requests based on estimates; warn instead

**Detection (warning signs):**
- User quotas depleting faster/slower than expected
- Requests failing with context length errors despite passing pre-check
- Monthly bills significantly different from tracked usage

**Phase to address:** Phase 3 (Usage & Quota) - after basic flow works

**Sources:**
- [Portkey Token Tracking](https://portkey.ai/blog/tracking-llm-token-usage-across-providers-teams-and-workloads/) - "Different models use different tokenizers. This means the same text can create different token counts"
- Sentinel codebase already uses tiktoken-rs, will need per-provider calibration

---

### Pitfall 7: Required Parameter Injection

**What goes wrong:** Anthropic requires `max_tokens` on every request; OpenAI doesn't. Teams forward requests verbatim and get validation errors from Anthropic.

**Why it happens:** Assumption that optional parameters in one API are optional in all.

**Consequences:**
- Requests fail with cryptic validation errors
- Default values injected don't match user intent (e.g., 4096 tokens when user wanted unlimited)
- Inconsistent behavior across providers

**Prevention:**
1. Document required vs optional parameters per provider
2. Inject sensible defaults where required, log when doing so
3. Expose unified API that makes required params explicit
4. Test with minimal request bodies to find required-param gaps

**Detection (warning signs):**
- "Works on OpenAI, validation error on Anthropic"
- Responses truncated unexpectedly
- Different default behaviors when same request sent to different providers

**Phase to address:** Phase 1 (Request Translation) - foundational

**Sources:**
- [LiteLLM Anthropic Docs](https://docs.litellm.ai/docs/providers/anthropic) - "Anthropic API fails requests when max_tokens are not passed"

---

### Pitfall 8: Strict Schema Validation Gaps

**What goes wrong:** OpenAI's `strict: true` for function calling guarantees JSON schema conformance. Anthropic ignores `strict` and may return non-conformant JSON.

**Why it happens:** Teams rely on OpenAI's strict validation, then route to Anthropic and get schema violations.

**Consequences:**
- JSON parsing failures in downstream code
- Schema validation errors in typed languages
- Inconsistent data shapes across providers

**Prevention:**
1. If using strict mode, document that it only works on OpenAI
2. Add post-response schema validation layer that works for all providers
3. For Anthropic, consider adding retry-with-correction logic for schema violations
4. Test tool responses against actual schemas, not just happy paths

**Detection (warning signs):**
- JSON parse errors on tool responses
- Missing required fields in structured outputs
- Type errors when processing tool responses

**Phase to address:** Phase 2 (Tool Calling) - alongside tool implementation

**Sources:**
- [Anthropic OpenAI SDK Compatibility](https://platform.claude.com/docs/en/api/openai-sdk) - "The strict parameter for function calling is ignored"

---

### Pitfall 9: xAI/Grok API Drift

**What goes wrong:** xAI advertises "OpenAI compatible" but has subtle differences. Anthropic SDK compatibility was deprecated. Features like Live Search are being replaced.

**Why it happens:** xAI is newer, API is still evolving. What works today may break tomorrow.

**Consequences:**
- Integration breaks on xAI API updates
- Features that worked stop working
- Migration burden when deprecated features sunset

**Prevention:**
1. Treat xAI as "OpenAI-like but verify everything"
2. Build xAI-specific test suite, run on schedule
3. Monitor xAI changelog actively
4. Have abstraction layer that can absorb breaking changes
5. Use only stable, documented features; avoid beta features in production

**Detection (warning signs):**
- xAI requests failing after working previously
- Deprecation warnings in API responses
- New required parameters appearing

**Phase to address:** Phase 5 (xAI Integration) - after core providers stable

**Sources:**
- [xAI Provider Docs (Promptfoo)](https://www.promptfoo.dev/docs/providers/xai/) - Documents OpenAI compatibility
- [xAI API Docs](https://docs.x.ai/docs/overview) - "Anthropic SDK compatibility is fully deprecated"

---

### Pitfall 10: Prompt Engineering Provider Variance

**What goes wrong:** Prompts optimized for GPT-4 perform poorly on Claude, and vice versa. Teams ship prompts tested on one provider, users on another tier get bad results.

**Why it happens:** Different models have different strengths, formatting preferences, and instruction-following patterns.

**Consequences:**
- Degraded quality when not using "home" provider
- User complaints about inconsistent behavior
- Support burden from provider-specific issues

**Prevention:**
1. Test all system prompts on ALL providers before shipping
2. Consider provider-specific prompt variants if quality variance is significant
3. Use Anthropic's prompt improver or similar tools when porting prompts
4. Document which prompts are provider-tested vs provider-optimized

**Detection (warning signs):**
- Quality complaints correlating with provider routing
- A/B test showing provider-dependent results
- Same prompt giving dramatically different outputs

**Phase to address:** Phase 5 (Quality Assurance) - ongoing concern

**Sources:**
- [Anthropic OpenAI SDK Compatibility](https://platform.claude.com/docs/en/api/openai-sdk) - "If you've done lots of tweaking to your prompt, it's likely to be well-tuned to OpenAI specifically"

---

## Minor Pitfalls

Mistakes that cause annoyance but are fixable.

---

### Pitfall 11: Response Format Field Differences

**What goes wrong:** Response objects have different field names and structures. `choices[0].message.content` (OpenAI) vs `content[0].text` (Anthropic native). Teams miss field mappings.

**Prevention:**
1. Define canonical response struct
2. Write explicit mapping functions, not inline field access
3. Unit test each field mapping with real response samples

---

### Pitfall 12: Header Handling Inconsistencies

**What goes wrong:** Different providers return different headers. Rate limit headers have different names. Some headers should not be forwarded.

**Prevention:**
1. Normalize rate limit headers to canonical names
2. Strip provider-identifying headers if needed
3. Sentinel already has `is_hop_by_hop_header`; extend for provider-specific headers

---

### Pitfall 13: Error Response Translation

**What goes wrong:** Error formats differ. OpenAI returns structured error JSON; Anthropic has different error codes and messages. Error handling becomes provider-specific.

**Prevention:**
1. Map all provider errors to unified error enum
2. Log original error for debugging, return normalized error to client
3. Test error paths: rate limit, auth failure, model not found, context too long

---

### Pitfall 14: Model Name Mapping Confusion

**What goes wrong:** Users request "gpt-4" but you need to route to "gpt-4-turbo-preview" or map "claude-3" to "claude-3-sonnet-20240229". Model aliases drift.

**Prevention:**
1. Maintain explicit model alias map
2. Validate model names at API boundary
3. Return clear error when model not found/supported
4. Update alias map when providers release new models

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Message Translation | Format incompatibility (#1) | Define canonical format first, test multi-turn |
| Streaming | Chunk format divergence (#4) | Never pass-through raw; always normalize |
| Tool Calling | Schema mismatch (#2), strict mode gaps (#8) | Sequential-only in unified API, post-validate |
| Response Handling | Extended thinking leakage (#3) | Strip provider-specific blocks |
| Routing | Session stickiness (#5), cost routing bugs | Explicit session affinity, routing tests |
| Usage Tracking | Token counting discrepancies (#6) | Use provider-reported tokens |
| xAI Integration | API drift (#9) | Defensive coding, scheduled integration tests |

---

## Sentinel-Specific Considerations

Based on the existing codebase:

1. **AiProvider trait is well-designed** - Uses `serde_json::Value` for flexibility, which is correct for multi-provider support. Keep this pattern.

2. **Header handling is solid** - `build_default_headers` and `is_hop_by_hop_header` in `headers.rs` provide good foundation. Extend for provider-specific headers.

3. **Token counting exists** - `SharedTokenCounter` uses tiktoken-rs. Need to add calibration/correction for non-OpenAI providers.

4. **Streaming implemented** - `ByteStream` type exists. Need chunk transformation layer, not just passthrough.

5. **Missing: Message translation layer** - No conversion between provider formats yet. This is the critical first addition.

6. **Missing: Session tracking** - No conversation_id handling. Need this for routing stickiness.

---

## Sources

**HIGH confidence (official documentation):**
- [Anthropic OpenAI SDK Compatibility](https://platform.claude.com/docs/en/api/openai-sdk)
- [Anthropic Extended Thinking](https://platform.claude.com/docs/en/build-with-claude/extended-thinking)
- [LiteLLM Anthropic Provider](https://docs.litellm.ai/docs/providers/anthropic)
- [xAI API Documentation](https://docs.x.ai/docs/overview)

**MEDIUM confidence (verified community sources):**
- [LiteLLM GitHub Issues](https://github.com/BerriAI/litellm/issues/16215) - Real bug reports
- [OpenAI vs Anthropic API Guide](https://www.eesel.ai/blog/openai-api-vs-anthropic-api)
- [Multi-provider LLM Orchestration Guide](https://dev.to/ash_dubai/multi-provider-llm-orchestration-in-production-a-2026-guide-1g10)
- [Portkey Token Tracking](https://portkey.ai/blog/tracking-llm-token-usage-across-providers-teams-and-workloads/)
- [Streaming LLM Responses Guide](https://dev.to/pockit_tools/the-complete-guide-to-streaming-llm-responses-in-web-applications-from-sse-to-real-time-ui-3534)

**LOW confidence (single source, needs validation):**
- xAI deprecation timelines (API may have changed since research)
