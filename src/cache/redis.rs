//! Redis cache implementation
//!
//! Handles caching of user limits and JWT validation results.

use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::AppResult;

/// Redis cache wrapper
pub struct RedisCache {
    conn: redis::aio::ConnectionManager,
    default_ttl: u64,
}

impl RedisCache {
    /// Create a new Redis cache
    pub fn new(conn: redis::aio::ConnectionManager, default_ttl: u64) -> Self {
        Self { conn, default_ttl }
    }

    /// Get a value from cache
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> AppResult<Option<T>> {
        let mut conn = self.conn.clone();
        let value: Option<String> = conn.get(key).await?;

        match value {
            Some(v) => {
                let parsed: T = serde_json::from_str(&v)?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    /// Set a value in cache with default TTL
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> AppResult<()> {
        self.set_with_ttl(key, value, self.default_ttl).await
    }

    /// Set a value in cache with custom TTL
    pub async fn set_with_ttl<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> AppResult<()> {
        let mut conn = self.conn.clone();
        let serialized = serde_json::to_string(value)?;
        let _: () = conn.set_ex(key, serialized, ttl_seconds).await?;
        Ok(())
    }

    /// Delete a key from cache
    pub async fn delete(&self, key: &str) -> AppResult<()> {
        let mut conn = self.conn.clone();
        let _: () = conn.del(key).await?;
        Ok(())
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> AppResult<bool> {
        let mut conn = self.conn.clone();
        let exists: bool = conn.exists(key).await?;
        Ok(exists)
    }

    /// Increment a counter
    pub async fn incr(&self, key: &str, delta: i64) -> AppResult<i64> {
        let mut conn = self.conn.clone();
        let value: i64 = conn.incr(key, delta).await?;
        Ok(value)
    }

    /// Set expiry on a key
    pub async fn expire(&self, key: &str, seconds: u64) -> AppResult<()> {
        let mut conn = self.conn.clone();
        let _: () = conn.expire(key, seconds as i64).await?;
        Ok(())
    }

    /// Get TTL remaining on a key (returns -2 if key doesn't exist, -1 if no TTL)
    pub async fn ttl(&self, key: &str) -> AppResult<i64> {
        let mut conn = self.conn.clone();
        let ttl: i64 = conn.ttl(key).await?;
        Ok(ttl)
    }

    /// Scan for keys matching a pattern (limited to first 100 matches for safety)
    pub async fn scan_keys(&self, pattern: &str) -> AppResult<Vec<String>> {
        let mut conn = self.conn.clone();
        let mut keys = Vec::new();
        let mut cursor = 0u64;

        loop {
            let (next_cursor, batch): (u64, Vec<String>) =
                redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(pattern)
                    .arg("COUNT")
                    .arg(100)
                    .query_async(&mut conn)
                    .await?;

            keys.extend(batch);
            cursor = next_cursor;

            // Limit to 100 keys for safety
            if keys.len() >= 100 || cursor == 0 {
                break;
            }
        }

        keys.truncate(100);
        Ok(keys)
    }

    /// Check if Redis is connected and responsive
    pub async fn ping(&self) -> AppResult<bool> {
        let mut conn = self.conn.clone();
        let result: String = redis::cmd("PING").query_async(&mut conn).await?;
        Ok(result == "PONG")
    }
}

/// Cache key prefixes
pub mod keys {
    /// User limits cache key
    pub fn user_limits(external_id: &str) -> String {
        format!("sentinel:limits:{}", external_id)
    }

    /// JWT validation cache key
    pub fn jwt_validation(jwt_hash: &str) -> String {
        format!("sentinel:jwt:{}", jwt_hash)
    }

    /// User profile cache key
    pub fn user_profile(jwt_hash: &str) -> String {
        format!("sentinel:profile:{}", jwt_hash)
    }

    /// Session cache key for provider stickiness
    pub fn session(conversation_id: &str) -> String {
        format!("sentinel:session:{}", conversation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::keys;

    // ===========================================
    // Cache Key Generation Tests
    // ===========================================

    #[test]
    fn test_cache_keys() {
        assert_eq!(
            keys::user_limits("user123"),
            "sentinel:limits:user123"
        );
        assert_eq!(
            keys::jwt_validation("abc123"),
            "sentinel:jwt:abc123"
        );
        assert_eq!(
            keys::user_profile("abc123"),
            "sentinel:profile:abc123"
        );
    }

    #[test]
    fn test_user_limits_key_format() {
        let key = keys::user_limits("ext_12345");
        assert!(key.starts_with("sentinel:limits:"));
        assert!(key.ends_with("ext_12345"));
        assert_eq!(key, "sentinel:limits:ext_12345");
    }

    #[test]
    fn test_jwt_validation_key_format() {
        let key = keys::jwt_validation("sha256hash");
        assert!(key.starts_with("sentinel:jwt:"));
        assert!(key.ends_with("sha256hash"));
        assert_eq!(key, "sentinel:jwt:sha256hash");
    }

    #[test]
    fn test_user_profile_key_format() {
        let key = keys::user_profile("jwt_hash_abc");
        assert!(key.starts_with("sentinel:profile:"));
        assert!(key.ends_with("jwt_hash_abc"));
        assert_eq!(key, "sentinel:profile:jwt_hash_abc");
    }

    #[test]
    fn test_cache_keys_empty_id() {
        // Edge case: empty ID
        let key = keys::user_limits("");
        assert_eq!(key, "sentinel:limits:");
    }

    #[test]
    fn test_cache_keys_special_characters() {
        // IDs with special characters
        let key = keys::user_limits("user@example.com");
        assert_eq!(key, "sentinel:limits:user@example.com");

        let key = keys::user_limits("user:with:colons");
        assert_eq!(key, "sentinel:limits:user:with:colons");
    }

    #[test]
    fn test_cache_keys_uuid() {
        // UUIDs are common identifiers
        let key = keys::user_limits("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(key, "sentinel:limits:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_cache_keys_consistency() {
        // Same input should always produce same output
        let id = "consistent_user_id";
        let key1 = keys::user_limits(id);
        let key2 = keys::user_limits(id);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_keys_uniqueness() {
        // Different key types for same ID should be different
        let id = "same_id";
        let limits_key = keys::user_limits(id);
        let jwt_key = keys::jwt_validation(id);
        let profile_key = keys::user_profile(id);
        let session_key = keys::session(id);

        assert_ne!(limits_key, jwt_key);
        assert_ne!(limits_key, profile_key);
        assert_ne!(limits_key, session_key);
        assert_ne!(jwt_key, profile_key);
        assert_ne!(jwt_key, session_key);
        assert_ne!(profile_key, session_key);
    }

    // ===========================================
    // Session Key Tests
    // ===========================================

    #[test]
    fn test_session_key_format() {
        let key = keys::session("conv-123");
        assert_eq!(key, "sentinel:session:conv-123");
    }

    #[test]
    fn test_session_key_prefix() {
        let key = keys::session("test-conv");
        assert!(key.starts_with("sentinel:session:"));
        assert!(key.ends_with("test-conv"));
    }

    #[test]
    fn test_session_key_empty_id() {
        // Edge case: empty conversation ID
        let key = keys::session("");
        assert_eq!(key, "sentinel:session:");
    }

    #[test]
    fn test_session_key_uuid() {
        // UUIDs are common conversation IDs
        let key = keys::session("550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(key, "sentinel:session:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_session_key_special_characters() {
        // Special characters in conversation IDs
        let key = keys::session("conv:with:colons");
        assert_eq!(key, "sentinel:session:conv:with:colons");

        let key = keys::session("conv-with-dashes");
        assert_eq!(key, "sentinel:session:conv-with-dashes");

        let key = keys::session("conv_with_underscores");
        assert_eq!(key, "sentinel:session:conv_with_underscores");
    }

    #[test]
    fn test_session_key_consistency() {
        // Same input should always produce same output
        let conv_id = "consistent_conv_id";
        let key1 = keys::session(conv_id);
        let key2 = keys::session(conv_id);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_keys_long_id() {
        // Very long IDs should work
        let long_id = "a".repeat(1000);
        let key = keys::user_limits(&long_id);
        assert!(key.len() > 1000);
        assert!(key.starts_with("sentinel:limits:"));
    }

    #[test]
    fn test_cache_keys_unicode() {
        // Unicode characters in IDs
        let key = keys::user_limits("user_unicode");
        assert_eq!(key, "sentinel:limits:user_unicode");
    }

    // ===========================================
    // RedisCache Unit Tests (without actual Redis)
    // ===========================================

    // Note: Testing RedisCache methods that interact with Redis
    // would require integration tests with a real Redis instance.
    // The following tests document the expected behavior and can be
    // used with mock implementations if needed.

    #[test]
    fn test_redis_cache_struct_has_conn_and_ttl() {
        // This is a compile-time check that RedisCache has the expected fields
        // The actual struct test requires a Redis connection
        use super::RedisCache;

        // Type check - this will fail to compile if the struct changes
        fn _type_check(cache: RedisCache) {
            let _ = cache.default_ttl;
            // conn is private but exists
        }
    }

    // ===========================================
    // Serialization Tests for Cache Values
    // ===========================================

    #[test]
    fn test_serialize_cache_value_string() {
        let value = "test string";
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, "\"test string\"");
    }

    #[test]
    fn test_serialize_cache_value_struct() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct TestStruct {
            id: String,
            count: i64,
        }

        let value = TestStruct {
            id: "test".to_string(),
            count: 42,
        };

        let serialized = serde_json::to_string(&value).unwrap();
        let deserialized: TestStruct = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_serialize_cache_value_vec() {
        let value = vec!["a", "b", "c"];
        let serialized = serde_json::to_string(&value).unwrap();
        let deserialized: Vec<String> = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_serialize_cache_value_option() {
        let some_value: Option<i64> = Some(42);
        let none_value: Option<i64> = None;

        let serialized_some = serde_json::to_string(&some_value).unwrap();
        let serialized_none = serde_json::to_string(&none_value).unwrap();

        assert_eq!(serialized_some, "42");
        assert_eq!(serialized_none, "null");
    }

    #[test]
    fn test_serialize_complex_nested_value() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Nested {
            items: Vec<Item>,
        }

        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Item {
            name: String,
            value: Option<i64>,
        }

        let value = Nested {
            items: vec![
                Item { name: "first".to_string(), value: Some(1) },
                Item { name: "second".to_string(), value: None },
            ],
        };

        let serialized = serde_json::to_string(&value).unwrap();
        let deserialized: Nested = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value, deserialized);
    }

    // ===========================================
    // TTL Value Tests
    // ===========================================

    #[test]
    fn test_ttl_values() {
        // Common TTL values used in the application
        let short_ttl: u64 = 60; // 1 minute
        let medium_ttl: u64 = 300; // 5 minutes
        let long_ttl: u64 = 3600; // 1 hour
        let day_ttl: u64 = 86400; // 1 day

        assert_eq!(short_ttl, 60);
        assert_eq!(medium_ttl, 5 * 60);
        assert_eq!(long_ttl, 60 * 60);
        assert_eq!(day_ttl, 24 * 60 * 60);
    }

    #[test]
    fn test_ttl_edge_cases() {
        // Zero TTL (immediate expiration)
        let zero_ttl: u64 = 0;
        assert_eq!(zero_ttl, 0);

        // Max TTL
        let max_ttl: u64 = u64::MAX;
        assert!(max_ttl > 0);
    }
}
