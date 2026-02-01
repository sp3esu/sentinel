//! Session management for provider stickiness
//!
//! This module provides session storage to ensure consistent provider/model
//! selection within a conversation. Sessions are stored in Redis with TTL-based
//! expiration that refreshes on activity.

use std::sync::Arc;

use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::{
    cache::redis::{keys, RedisCache},
    error::AppResult,
};

#[cfg(any(test, feature = "test-utils"))]
use crate::cache::InMemoryCache;

/// Cache backend abstraction for SessionManager
///
/// This enum allows SessionManager to work with either Redis or in-memory
/// caching, enabling fully isolated integration tests.
pub enum SessionCacheBackend {
    /// Redis-based cache for production use
    Redis(Arc<RedisCache>),
    /// In-memory cache for testing (only available with test-utils feature)
    #[cfg(any(test, feature = "test-utils"))]
    InMemory(Arc<InMemoryCache>),
}

impl SessionCacheBackend {
    async fn get<T: DeserializeOwned>(&self, key: &str) -> AppResult<Option<T>> {
        match self {
            SessionCacheBackend::Redis(cache) => cache.get(key).await,
            #[cfg(any(test, feature = "test-utils"))]
            SessionCacheBackend::InMemory(cache) => cache.get(key).await,
        }
    }

    async fn set_with_ttl<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> AppResult<()> {
        match self {
            SessionCacheBackend::Redis(cache) => cache.set_with_ttl(key, value, ttl_seconds).await,
            #[cfg(any(test, feature = "test-utils"))]
            SessionCacheBackend::InMemory(cache) => cache.set_with_ttl(key, value, ttl_seconds).await,
        }
    }

    async fn expire(&self, key: &str, seconds: u64) -> AppResult<()> {
        match self {
            SessionCacheBackend::Redis(cache) => cache.expire(key, seconds).await,
            #[cfg(any(test, feature = "test-utils"))]
            SessionCacheBackend::InMemory(cache) => cache.expire(key, seconds).await,
        }
    }
}

/// Session data stored in Redis
///
/// Represents a conversation's provider/model binding for stickiness.
/// Once a session is created, all subsequent requests in the same
/// conversation use the same provider and model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    /// Unique session identifier (same as conversation_id)
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
///
/// Wraps Redis operations with session-specific logic.
/// Follows the SubscriptionCache pattern for consistency.
pub struct SessionManager {
    cache: SessionCacheBackend,
    session_ttl: u64,
}

impl SessionManager {
    /// Create a new session manager with Redis backend
    ///
    /// # Arguments
    /// * `cache` - Redis cache reference
    /// * `session_ttl` - TTL in seconds (typically 24 hours = 86400)
    pub fn new(cache: Arc<RedisCache>, session_ttl: u64) -> Self {
        Self {
            cache: SessionCacheBackend::Redis(cache),
            session_ttl,
        }
    }

    /// Create a new session manager with in-memory backend for testing
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_testing(cache: Arc<InMemoryCache>, session_ttl: u64) -> Self {
        Self {
            cache: SessionCacheBackend::InMemory(cache),
            session_ttl,
        }
    }

    /// Get existing session by conversation ID
    ///
    /// Returns None if session doesn't exist (not an error).
    /// Caller should handle missing session by creating a new one.
    #[instrument(skip(self), fields(conversation_id = %conversation_id))]
    pub async fn get(&self, conversation_id: &str) -> AppResult<Option<Session>> {
        let key = keys::session(conversation_id);
        let result = self.cache.get::<Session>(&key).await?;

        if result.is_some() {
            debug!("Session cache hit");
        } else {
            debug!("Session cache miss");
        }

        Ok(result)
    }

    /// Create a new session for a conversation
    ///
    /// Stores the provider/model binding in Redis with TTL.
    /// The TTL resets on each activity via `touch()`.
    #[instrument(skip(self), fields(conversation_id = %conversation_id, provider = %provider, model = %model))]
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
            created_at: Utc::now().timestamp(),
        };

        let key = keys::session(conversation_id);
        self.cache
            .set_with_ttl(&key, &session, self.session_ttl)
            .await?;

        debug!("Session created");
        Ok(session)
    }

    /// Refresh session TTL on activity
    ///
    /// Called on each request to implement activity-based expiration.
    /// The 24-hour (or configured) TTL resets from the last activity,
    /// not from session creation.
    #[instrument(skip(self), fields(conversation_id = %conversation_id))]
    pub async fn touch(&self, conversation_id: &str) -> AppResult<()> {
        let key = keys::session(conversation_id);
        self.cache.expire(&key, self.session_ttl).await?;
        debug!("Session TTL refreshed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // Session Struct Tests
    // ===========================================

    #[test]
    fn test_session_serialization_roundtrip() {
        let session = Session {
            id: "conv-123".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            external_id: "user-456".to_string(),
            created_at: 1700000000,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&session).unwrap();

        // Deserialize back
        let deserialized: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(session, deserialized);
    }

    #[test]
    fn test_session_json_format() {
        let session = Session {
            id: "test-id".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-3-opus".to_string(),
            external_id: "ext-123".to_string(),
            created_at: 1700000000,
        };

        let json = serde_json::to_string(&session).unwrap();

        // Verify JSON structure
        assert!(json.contains("\"id\":\"test-id\""));
        assert!(json.contains("\"provider\":\"anthropic\""));
        assert!(json.contains("\"model\":\"claude-3-opus\""));
        assert!(json.contains("\"external_id\":\"ext-123\""));
        assert!(json.contains("\"created_at\":1700000000"));
    }

    #[test]
    fn test_session_deserialize_from_json() {
        let json = r#"{
            "id": "conv-abc",
            "provider": "openai",
            "model": "gpt-4-turbo",
            "external_id": "user-xyz",
            "created_at": 1700000000
        }"#;

        let session: Session = serde_json::from_str(json).unwrap();

        assert_eq!(session.id, "conv-abc");
        assert_eq!(session.provider, "openai");
        assert_eq!(session.model, "gpt-4-turbo");
        assert_eq!(session.external_id, "user-xyz");
        assert_eq!(session.created_at, 1700000000);
    }

    #[test]
    fn test_session_clone() {
        let session = Session {
            id: "clone-test".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            external_id: "user-1".to_string(),
            created_at: 1700000000,
        };

        let cloned = session.clone();

        assert_eq!(session, cloned);
    }

    #[test]
    fn test_session_debug_format() {
        let session = Session {
            id: "debug-test".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            external_id: "user-1".to_string(),
            created_at: 1700000000,
        };

        let debug_str = format!("{:?}", session);

        assert!(debug_str.contains("Session"));
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("openai"));
    }

    // ===========================================
    // SessionManager Struct Tests
    // ===========================================

    // Note: Full integration tests require Redis connection.
    // These tests verify the struct construction and type safety.

    #[test]
    fn test_session_manager_has_expected_fields() {
        // This is a compile-time check that SessionManager has the expected structure
        // The actual creation requires a Redis connection

        fn _type_check(manager: SessionManager) {
            let _ = manager.session_ttl;
            // cache is private but exists
        }
    }

    #[test]
    fn test_session_ttl_values() {
        // Common TTL values
        let one_hour: u64 = 3600;
        let twenty_four_hours: u64 = 86400;
        let one_week: u64 = 604800;

        assert_eq!(one_hour, 60 * 60);
        assert_eq!(twenty_four_hours, 24 * 60 * 60);
        assert_eq!(one_week, 7 * 24 * 60 * 60);
    }

    // ===========================================
    // Edge Case Tests
    // ===========================================

    #[test]
    fn test_session_with_special_characters() {
        // Session IDs can have various formats (UUIDs, custom IDs, etc.)
        let session = Session {
            id: "conv_550e8400-e29b-41d4-a716-446655440000".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-2024-05-13".to_string(),
            external_id: "user@example.com".to_string(),
            created_at: 1700000000,
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(session, deserialized);
    }

    #[test]
    fn test_session_with_empty_strings() {
        // Edge case: empty strings (should not happen in practice but handle gracefully)
        let session = Session {
            id: "".to_string(),
            provider: "".to_string(),
            model: "".to_string(),
            external_id: "".to_string(),
            created_at: 0,
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(session, deserialized);
    }

    #[test]
    fn test_session_with_unicode() {
        // Unicode in model names (though unlikely in practice)
        let session = Session {
            id: "conv-unicode".to_string(),
            provider: "custom".to_string(),
            model: "model-name".to_string(),
            external_id: "user-unicode".to_string(),
            created_at: 1700000000,
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(session, deserialized);
    }

    #[test]
    fn test_session_timestamp_edge_cases() {
        // Timestamp at epoch
        let session_epoch = Session {
            id: "epoch".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            external_id: "user".to_string(),
            created_at: 0,
        };

        // Far future timestamp
        let session_future = Session {
            id: "future".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            external_id: "user".to_string(),
            created_at: i64::MAX,
        };

        // Both should serialize/deserialize correctly
        let json_epoch = serde_json::to_string(&session_epoch).unwrap();
        let json_future = serde_json::to_string(&session_future).unwrap();

        let _: Session = serde_json::from_str(&json_epoch).unwrap();
        let _: Session = serde_json::from_str(&json_future).unwrap();
    }
}
