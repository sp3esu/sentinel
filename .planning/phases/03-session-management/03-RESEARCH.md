# Phase 3: Session Management - Research

**Researched:** 2026-02-01
**Domain:** Redis-based session tracking for provider stickiness
**Confidence:** HIGH

## Summary

Phase 3 implements session management to ensure consistent provider selection within a conversation. The core mechanism is simple: store the provider and model selection when a session starts, retrieve it for subsequent requests with the same conversation_id.

The research reveals that this phase has straightforward requirements with well-understood patterns. Sentinel already has robust Redis infrastructure (`RedisCache` with TTL support), UUID generation (`uuid` crate with v4), and established key patterns (`sentinel:*`). The main work is:

1. Adding `conversation_id` field to `ChatCompletionRequest`
2. Creating a `Session` struct to store provider/model binding
3. Implementing a `SessionManager` service using existing `RedisCache` patterns
4. Integrating session lookup into the native chat handler

**Primary recommendation:** Follow the existing `SubscriptionCache` pattern - create `SessionManager` struct that wraps Redis operations with session-specific logic. Use the established `sentinel:session:{conversation_id}` key pattern.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| redis | 0.24 | Session storage | Already in Cargo.toml, async with tokio-comp |
| uuid | 1.x | Session ID generation | Already in Cargo.toml with v4 feature |
| serde/serde_json | 1.x | Session serialization | Already in use for all cache values |
| chrono | 0.4 | Timestamp handling | Already in Cargo.toml with serde feature |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing | 0.1 | Debug logging | Already in use, add session_id to spans |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Redis TTL | Manual cleanup task | Redis TTL is simpler, automatic, already used |
| UUID v4 | UUIDv7/ULID | v4 already available, ordering not needed |
| JSON storage | MessagePack | JSON works with existing cache patterns |

**No new dependencies required** - all needed libraries are already in Cargo.toml.

## Architecture Patterns

### Recommended Project Structure

```
src/
  native/
    session.rs         # NEW: SessionManager + Session struct
    request.rs         # UPDATE: Add conversation_id field
  native_routes/
    chat.rs           # UPDATE: Integrate session lookup
  config.rs           # UPDATE: Add SESSION_TTL_SECONDS config
```

### Pattern 1: Session as Cache Value (following SubscriptionCache pattern)

**What:** Store session data as JSON in Redis with TTL-based expiration.

**When to use:** Always - this is the established pattern in Sentinel.

**Example:**

```rust
// src/native/session.rs
use serde::{Deserialize, Serialize};

/// Session data stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub id: String,
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Model identifier used for this session
    pub model: String,
    /// User's external ID (for debugging/cleanup)
    pub external_id: String,
    /// Unix timestamp when session was created
    pub created_at: i64,
}

/// Session manager for provider stickiness
pub struct SessionManager {
    cache: Arc<RedisCache>,
    session_ttl: u64,  // 24 hours default
}

impl SessionManager {
    pub fn new(cache: Arc<RedisCache>, session_ttl: u64) -> Self {
        Self { cache, session_ttl }
    }

    /// Get existing session or return None
    pub async fn get(&self, conversation_id: &str) -> AppResult<Option<Session>> {
        let key = format!("sentinel:session:{}", conversation_id);
        self.cache.get::<Session>(&key).await
    }

    /// Store session for new conversation
    pub async fn create(
        &self,
        conversation_id: &str,
        provider: &str,
        model: &str,
        external_id: &str,
    ) -> AppResult<Session> {
        let session = Session {
            id: conversation_id.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            external_id: external_id.to_string(),
            created_at: chrono::Utc::now().timestamp(),
        };
        let key = format!("sentinel:session:{}", conversation_id);
        self.cache.set_with_ttl(&key, &session, self.session_ttl).await?;
        Ok(session)
    }

    /// Touch session to refresh TTL (activity-based expiration)
    pub async fn touch(&self, conversation_id: &str) -> AppResult<()> {
        let key = format!("sentinel:session:{}", conversation_id);
        self.cache.expire(&key, self.session_ttl).await
    }
}
```

### Pattern 2: Request Field Extension

**What:** Add `conversation_id` as optional field on `ChatCompletionRequest`.

**When to use:** For session tracking in Phase 3.

**Example:**

```rust
// src/native/request.rs - add to existing struct
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChatCompletionRequest {
    // ... existing fields ...

    /// Conversation ID for session stickiness (optional)
    /// When provided, uses the provider/model from the first request in this conversation.
    /// When absent, triggers fresh provider selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}
```

### Pattern 3: Handler Integration

**What:** Integrate session lookup into chat handler before provider selection.

**When to use:** In the native_chat_completions handler.

**Example flow:**

```rust
// src/native_routes/chat.rs - conceptual flow
async fn native_chat_completions(...) {
    // 1. Parse request (existing)
    let native_request: ChatCompletionRequest = ...;

    // 2. Session handling (NEW)
    let (provider, model) = if let Some(ref conv_id) = native_request.conversation_id {
        // Try to get existing session
        if let Some(session) = state.session_manager.get(conv_id).await? {
            // Refresh TTL on activity
            state.session_manager.touch(conv_id).await?;
            (session.provider, session.model)
        } else {
            // Session expired or invalid - treat as new conversation
            // This triggers fresh selection (Phase 4 implements tier routing)
            // For Phase 3, model comes from request
            let model = native_request.model.clone().ok_or_else(|| ...)?;
            let provider = "openai".to_string();  // Hardcoded until Phase 4

            // Store new session
            state.session_manager.create(conv_id, &provider, &model, &user.external_id).await?;
            (provider, model)
        }
    } else {
        // No conversation_id - fresh selection each time
        let model = native_request.model.clone().ok_or_else(|| ...)?;
        ("openai".to_string(), model)
    };

    // 3. Continue with provider/model (existing flow)
}
```

### Anti-Patterns to Avoid

- **Storing session in request headers:** Use request body field, not headers. Headers are for transport metadata, body is for business logic.

- **Generating conversation_id server-side:** The *client* generates and manages conversation_id. Server only stores provider binding. This keeps Sentinel stateless from client's perspective.

- **Validating conversation_id format:** Accept any string. Client controls the format (could be UUID, could be custom). Just use it as an opaque key.

- **Session per user instead of per conversation:** Sessions are per conversation_id, not per user. One user can have many concurrent sessions.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TTL-based expiration | Background cleanup task | Redis TTL (EXPIRE command) | Built into Redis, automatic, no code to maintain |
| Session ID generation | Custom ID generator | Client-provided conversation_id | Client manages session lifecycle |
| JSON serialization | Custom format | serde_json (existing) | Already used for all cache values |
| Cache abstraction | Direct redis commands | RedisCache wrapper (existing) | Error handling, connection management already done |

**Key insight:** The existing `RedisCache` already handles everything needed for session storage. `SessionManager` is a thin wrapper adding session-specific key naming and business logic.

## Common Pitfalls

### Pitfall 1: Session Leak on Model Change

**What goes wrong:** Client sends `conversation_id` but with a different `model` than stored in session. Code uses new model, breaking provider stickiness.

**Why it happens:** Handler uses request.model instead of session.model when session exists.

**How to avoid:** When session exists, ALWAYS use session.model, ignore request.model. Log if they differ (for debugging) but don't change behavior.

**Warning signs:** Provider/model changes mid-conversation in logs.

### Pitfall 2: No TTL Refresh on Activity

**What goes wrong:** User has long conversation with pauses. Session expires mid-conversation despite activity.

**Why it happens:** TTL only set at creation, not refreshed on subsequent requests.

**How to avoid:** Call `touch()` (refresh TTL) on every request that uses the session. The 24-hour TTL resets with each activity.

**Warning signs:** "Session not found" errors for active conversations.

### Pitfall 3: Race Condition on Session Creation

**What goes wrong:** Two concurrent requests with same new conversation_id create duplicate sessions with different provider/model selections.

**Why it happens:** Check-then-create pattern without atomicity.

**How to avoid:** For Phase 3 (only OpenAI), this is not critical. For Phase 4 (multiple providers), use Redis SETNX or Lua script for atomic create-if-not-exists. Document this as a known limitation for Phase 3.

**Warning signs:** First message in conversation gets different model than subsequent messages.

### Pitfall 4: Expired Session Treated as Error

**What goes wrong:** Session expires, client sends same conversation_id, server returns error instead of starting fresh.

**Why it happens:** Code treats "session not found" as error instead of "create new session".

**How to avoid:** Missing session with valid conversation_id should trigger fresh provider selection, not error. Log it but continue.

**Warning signs:** "Session not found" errors returned to client.

## Code Examples

### Cache Key Pattern (following existing convention)

```rust
// src/cache/redis.rs - add to existing keys module
pub mod keys {
    // ... existing keys ...

    /// Session cache key
    pub fn session(conversation_id: &str) -> String {
        format!("sentinel:session:{}", conversation_id)
    }
}
```

### Config Extension

```rust
// src/config.rs - add new field
pub struct Config {
    // ... existing fields ...

    /// Session TTL (default: 24 hours = 86400 seconds)
    pub session_ttl_seconds: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            // ... existing ...
            session_ttl_seconds: env::var("SESSION_TTL_SECONDS")
                .unwrap_or_else(|_| "86400".to_string())
                .parse()
                .context("Invalid SESSION_TTL_SECONDS")?,
        })
    }
}
```

### AppState Extension

```rust
// src/lib.rs - add to AppState
pub struct AppState {
    // ... existing fields ...

    /// Session manager for provider stickiness
    pub session_manager: Arc<SessionManager>,
}
```

### Test Pattern

```rust
// Test session creation and retrieval
#[tokio::test]
async fn test_session_stickiness() {
    let manager = SessionManager::new(cache, 86400);
    let conv_id = "test-conv-123";

    // First request creates session
    assert!(manager.get(conv_id).await.unwrap().is_none());
    manager.create(conv_id, "openai", "gpt-4", "user123").await.unwrap();

    // Second request retrieves session
    let session = manager.get(conv_id).await.unwrap().unwrap();
    assert_eq!(session.provider, "openai");
    assert_eq!(session.model, "gpt-4");
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Server-generated session IDs | Client-provided conversation_id | 2025 | Simpler, client controls lifecycle |
| Memory-based sessions | Redis-based with TTL | Established | Survives restarts, scales horizontally |
| Fixed expiration time | Activity-based TTL refresh | Best practice | Better user experience |

**Deprecated/outdated:**
- In-memory session storage: Not suitable for multi-instance deployment

## Open Questions

1. **What if client sends invalid/malicious conversation_id?**
   - What we know: Redis key is generated from client input
   - What's unclear: Maximum length, character restrictions
   - Recommendation: Validate length (max 256 chars), use as-is otherwise. Redis keys handle special characters fine.

2. **Should we return session info in response?**
   - What we know: Phase 3 requirements don't specify
   - What's unclear: Whether client needs session metadata
   - Recommendation: Keep responses minimal. Client already has conversation_id. Add metadata if explicitly requested later.

3. **How to handle session for Tier changes?**
   - What we know: SESSION-01 says same provider for all requests in session
   - What's unclear: What if client wants to change tier mid-conversation?
   - Recommendation: Defer to Phase 4. For Phase 3, session locks provider/model regardless of request.

## Sources

### Primary (HIGH confidence)

- Sentinel codebase `/src/cache/redis.rs` - Existing Redis cache patterns
- Sentinel codebase `/src/cache/subscription.rs` - SubscriptionCache as reference pattern
- Sentinel codebase `/src/config.rs` - Configuration pattern
- ARCHITECTURE.md Session Manager section - Design already specified

### Secondary (MEDIUM confidence)

- Redis documentation on TTL/EXPIRE - Standard behavior
- UUID v4 specification - Standard format

### Tertiary (LOW confidence)

- None - all findings verified against codebase

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries already in Cargo.toml
- Architecture: HIGH - Follows established patterns in codebase
- Pitfalls: HIGH - Based on analysis of existing code and common Redis patterns

**Research date:** 2026-02-01
**Valid until:** 60 days (stable domain, established patterns)
