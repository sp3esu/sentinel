# Technology Stack: Provider-Agnostic LLM API

**Project:** Sentinel Native API
**Researched:** 2026-01-31
**Focus:** Stack dimension for multi-provider LLM abstraction

## Executive Recommendation

**Build your own thin abstraction layer on top of reqwest**, rather than adopting a unified LLM library. Here's why:

1. **Sentinel already has the architecture** - The `AiProvider` trait and `OpenAIProvider` implementation show exactly the pattern needed
2. **xAI/Grok is OpenAI-compatible** - No separate SDK needed; just change base_url
3. **Anthropic is the only truly different API** - One additional provider implementation
4. **Unified libraries add complexity** - They bring their own abstractions, error types, and update cadence
5. **Control over streaming** - You already have precise control over SSE handling

---

## Recommended Stack

### Core Framework (Keep Existing)

| Technology | Version | Purpose | Confidence |
|------------|---------|---------|------------|
| Rust | 1.83+ | Language | HIGH |
| Axum | 0.7.x | Web framework | HIGH (see note) |
| Tokio | 1.x | Async runtime | HIGH |
| reqwest | 0.12.x | HTTP client | HIGH |
| serde/serde_json | 1.x | Serialization | HIGH |

**Note on Axum 0.8:** Axum 0.8.0 was released January 2025 with breaking changes (path syntax `/:id` -> `/{id}`, `Option<T>` extractor changes). The current Sentinel uses 0.7.x. **Recommendation:** Stay on 0.7.x for this milestone; upgrade is a separate effort. utoipa-axum 0.2.0 requires axum ^0.8.0, so use utoipa-axum 0.1.x or defer OpenAPI until Axum upgrade.

### Provider SDKs (NOT Recommended)

| Library | Version | Why NOT |
|---------|---------|---------|
| rust-genai | 0.5.x | Adds abstraction layer over what you already have; different error types |
| llm (graniet) | 1.2.4 | Heavy; includes voice, agents, chains - overkill for proxy |
| async-openai | 0.32.x | Already using reqwest directly; adding typed SDK would require refactoring |

**Why not use unified LLM libraries:**
- Sentinel's `AiProvider` trait already defines the exact interface needed
- Adding a library means adopting its error handling, types, and update schedule
- You lose fine-grained control over request/response handling
- Debugging becomes harder when issues are inside third-party abstractions

### Token Counting

| Technology | Version | Purpose | Confidence |
|------------|---------|---------|------------|
| tiktoken-rs | 0.9.1 | OpenAI token counting | HIGH |
| claude-tokenizer | 0.3.x | Anthropic token counting (offline) | MEDIUM |
| Anthropic API | N/A | Token counting endpoint (online, accurate) | HIGH |

**Token counting strategy:**
- **OpenAI/xAI**: Use tiktoken-rs (same tokenizer, `o200k_base` for GPT-4o/Grok)
- **Anthropic**: Two options:
  1. **Offline (fast)**: `claude-tokenizer` crate - may have slight accuracy variance
  2. **Online (accurate)**: Anthropic's `/v1/messages/count_tokens` endpoint - free, rate-limited
- **Recommendation**: Use offline tokenizers for pre-request estimates, trust provider usage in responses

### OpenAPI Documentation

| Technology | Version | Purpose | Confidence |
|------------|---------|---------|------------|
| utoipa | 5.x | OpenAPI schema generation | HIGH |
| utoipa-axum | 0.1.x | Axum 0.7 integration | MEDIUM |
| utoipa-swagger-ui | 5.x | Swagger UI serving | HIGH |

**Alternative considered:** aide 0.15.x - more declarative, less macro magic. However, utoipa has broader adoption, more examples, and better documentation. Choose utoipa.

**Note:** utoipa-axum 0.2.0 requires Axum 0.8. For Axum 0.7, either:
1. Use utoipa-axum 0.1.x
2. Define OpenAPI manually without the axum integration crate
3. Defer OpenAPI to Axum 0.8 upgrade

---

## Provider Implementation Strategy

### OpenAI (Existing)

Already implemented in `src/proxy/openai.rs`. No changes needed.

### xAI/Grok (New)

**Key finding:** xAI API is OpenAI-compatible. Base URL: `https://api.x.ai/v1`

**Implementation:** Clone `OpenAIProvider`, change:
- `base_url` to `https://api.x.ai/v1`
- `api_key` to xAI key
- `name()` to return `"xai"`

**Confidence:** HIGH - Officially documented as OpenAI-compatible.

```rust
// Pseudocode - nearly identical to OpenAIProvider
pub struct XaiProvider {
    client: reqwest::Client,
    base_url: String,  // "https://api.x.ai/v1"
    api_key: String,   // XAI_API_KEY
}
```

### Anthropic (New)

**Key finding:** Anthropic API is NOT OpenAI-compatible. Different request/response format.

**Implementation options:**

| Option | Effort | Confidence | Recommendation |
|--------|--------|------------|----------------|
| Raw reqwest | Medium | HIGH | **Recommended** |
| anthropic-sdk-rust | Low | MEDIUM | Feature-complete but adds dependency |
| anthropic-rs | Low | LOW | Streaming incomplete |

**Recommended approach:** Implement `AnthropicProvider` using raw reqwest, following the pattern in `openai.rs`. Reasons:
1. Full control over request/response handling
2. Matches existing codebase patterns
3. No third-party abstraction layer

**Anthropic API differences to handle:**
- Different auth header: `x-api-key` instead of `Authorization: Bearer`
- Different request format (messages array structure differs)
- Different streaming format (SSE but different event names)
- Model names: `claude-3-opus-20240229`, `claude-3-5-sonnet-20241022`, etc.

---

## Dependencies to Add

```toml
[dependencies]
# Token counting for Anthropic (choose one)
claude-tokenizer = "0.3"  # Offline, may have variance
# OR use Anthropic API endpoint for accurate counts

# OpenAPI documentation
utoipa = { version = "5", features = ["axum_extras"] }
utoipa-swagger-ui = { version = "5", features = ["axum"] }
# Note: utoipa-axum 0.1.x for Axum 0.7 compatibility, or defer
```

---

## What NOT to Use

| Technology | Why Not |
|------------|---------|
| rust-genai | Unnecessary abstraction; you already have `AiProvider` trait |
| llm (graniet) | Overkill for proxy use case; designed for agents/chains |
| async-openai | Would require refactoring existing reqwest-based code |
| xai-sdk (gRPC) | xAI has OpenAI-compatible REST API; gRPC is unnecessary complexity |
| aide | Less adoption than utoipa; stick with established solution |

---

## Environment Variables (New)

```bash
# Existing
OPENAI_API_KEY=sk-...
OPENAI_API_URL=https://api.openai.com/v1

# Add for xAI
XAI_API_KEY=xai-...
XAI_API_URL=https://api.x.ai/v1  # Optional, has sensible default

# Add for Anthropic
ANTHROPIC_API_KEY=sk-ant-...
ANTHROPIC_API_URL=https://api.anthropic.com  # Optional, has sensible default
```

---

## Confidence Assessment

| Area | Level | Rationale |
|------|-------|-----------|
| Keep reqwest approach | HIGH | Existing pattern works; unified libs add complexity |
| xAI is OpenAI-compatible | HIGH | Official documentation states this explicitly |
| Anthropic needs custom impl | HIGH | Different API format, well-documented |
| tiktoken-rs for OpenAI/xAI | HIGH | Same tokenizer family |
| claude-tokenizer accuracy | MEDIUM | Unofficial; may have variance vs Anthropic API |
| utoipa for OpenAPI | HIGH | Well-adopted, good Axum support |
| utoipa-axum 0.7 compat | MEDIUM | May need 0.1.x version; verify before implementing |

---

## Sources

**Verified (HIGH confidence):**
- [xAI REST API Reference](https://docs.x.ai/docs/api-reference) - Confirms OpenAI compatibility
- [async-openai GitHub](https://github.com/64bit/async-openai) - v0.32.4, January 2025
- [tiktoken-rs GitHub](https://github.com/zurawiki/tiktoken-rs) - v0.9.1, November 2025
- [utoipa GitHub](https://github.com/juhaku/utoipa) - v5.x with Axum support
- [Axum 0.8.0 Announcement](https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0) - Breaking changes documented

**WebSearch findings (MEDIUM confidence):**
- [rust-genai](https://github.com/jeremychone/rust-genai) - v0.5.x, multi-provider support
- [llm (graniet)](https://github.com/graniet/llm) - v1.2.4, comprehensive but heavy
- [claude-tokenizer](https://crates.io/crates/claude-tokenizer) - v0.3.x, unofficial

**Official API documentation:**
- [Anthropic Token Counting](https://docs.anthropic.com/en/api/messages-count-tokens) - Free endpoint
- [Anthropic Messages API](https://docs.anthropic.com/en/api/messages) - Request/response format
