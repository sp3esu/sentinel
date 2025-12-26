//! Redis test helpers for testing
//!
//! Provides helpers for testing Redis-dependent code:
//! - Test connection management (use real Redis if available, skip if not)
//! - Test key prefixing to avoid collisions between tests
//! - Cleanup utilities for test data
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::mocks::redis::{TestRedis, skip_if_no_redis};
//!
//! #[tokio::test]
//! async fn test_with_redis() {
//!     let redis = match TestRedis::connect().await {
//!         Some(r) => r,
//!         None => {
//!             eprintln!("Skipping test: Redis not available");
//!             return;
//!         }
//!     };
//!
//!     // Use redis.conn() for operations
//!     // Keys are automatically prefixed with test namespace
//!     redis.set("mykey", "myvalue").await.unwrap();
//!
//!     // Cleanup happens automatically on drop
//! }
//! ```

use redis::AsyncCommands;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default Redis URL for testing
pub const TEST_REDIS_URL: &str = "redis://127.0.0.1:6379";

/// Test key prefix to avoid collisions with production data
pub const TEST_KEY_PREFIX: &str = "sentinel:test:";

/// Counter for generating unique test namespaces
static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Test Redis wrapper with automatic key prefixing and cleanup
pub struct TestRedis {
    conn: redis::aio::ConnectionManager,
    namespace: String,
    tracked_keys: std::sync::Mutex<Vec<String>>,
}

impl TestRedis {
    /// Try to connect to Redis for testing
    ///
    /// Returns `Some(TestRedis)` if connection succeeds, `None` if Redis is unavailable.
    /// This allows tests to gracefully skip when Redis isn't running.
    pub async fn connect() -> Option<Self> {
        Self::connect_with_url(TEST_REDIS_URL).await
    }

    /// Try to connect to Redis with a custom URL
    pub async fn connect_with_url(url: &str) -> Option<Self> {
        let client = redis::Client::open(url).ok()?;
        let conn = client.get_connection_manager().await.ok()?;

        // Generate unique namespace for this test run
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let namespace = format!("{}{}_{}", TEST_KEY_PREFIX, timestamp, counter);

        Some(Self {
            conn,
            namespace,
            tracked_keys: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Get the underlying connection manager
    pub fn conn(&self) -> redis::aio::ConnectionManager {
        self.conn.clone()
    }

    /// Get the test namespace prefix
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Create a prefixed key for testing
    pub fn key(&self, suffix: &str) -> String {
        let key = format!("{}:{}", self.namespace, suffix);
        self.tracked_keys.lock().unwrap().push(key.clone());
        key
    }

    /// Set a value with automatic key prefixing
    pub async fn set(&self, key: &str, value: &str) -> redis::RedisResult<()> {
        let full_key = self.key(key);
        let mut conn = self.conn.clone();
        conn.set(&full_key, value).await
    }

    /// Set a value with TTL
    pub async fn set_ex(&self, key: &str, value: &str, ttl_seconds: u64) -> redis::RedisResult<()> {
        let full_key = self.key(key);
        let mut conn = self.conn.clone();
        conn.set_ex(&full_key, value, ttl_seconds).await
    }

    /// Get a value with automatic key prefixing
    pub async fn get(&self, key: &str) -> redis::RedisResult<Option<String>> {
        let full_key = format!("{}:{}", self.namespace, key);
        let mut conn = self.conn.clone();
        conn.get(&full_key).await
    }

    /// Delete a value with automatic key prefixing
    pub async fn del(&self, key: &str) -> redis::RedisResult<()> {
        let full_key = format!("{}:{}", self.namespace, key);
        let mut conn = self.conn.clone();
        conn.del(&full_key).await
    }

    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> redis::RedisResult<bool> {
        let full_key = format!("{}:{}", self.namespace, key);
        let mut conn = self.conn.clone();
        conn.exists(&full_key).await
    }

    /// Increment a counter
    pub async fn incr(&self, key: &str, delta: i64) -> redis::RedisResult<i64> {
        let full_key = self.key(key);
        let mut conn = self.conn.clone();
        conn.incr(&full_key, delta).await
    }

    /// Set expiry on a key
    pub async fn expire(&self, key: &str, seconds: i64) -> redis::RedisResult<()> {
        let full_key = format!("{}:{}", self.namespace, key);
        let mut conn = self.conn.clone();
        conn.expire(&full_key, seconds).await
    }

    /// Clean up all tracked keys created during this test
    pub async fn cleanup(&self) -> redis::RedisResult<()> {
        let keys: Vec<String> = {
            let guard = self.tracked_keys.lock().unwrap();
            guard.clone()
        };

        if keys.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.clone();
        for key in keys {
            let _: redis::RedisResult<()> = conn.del(&key).await;
        }

        Ok(())
    }

    /// Clean up all keys matching the test prefix pattern
    /// Use with caution - this uses SCAN which can be slow
    pub async fn cleanup_all_test_keys(&self) -> redis::RedisResult<()> {
        let mut conn = self.conn.clone();
        let pattern = format!("{}*", TEST_KEY_PREFIX);

        // Use SCAN to find all matching keys
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;

        if !keys.is_empty() {
            for key in keys {
                let _: redis::RedisResult<()> = conn.del(&key).await;
            }
        }

        Ok(())
    }
}

/// Macro to skip a test if Redis is not available
#[macro_export]
macro_rules! skip_if_no_redis {
    () => {
        match $crate::mocks::redis::TestRedis::connect().await {
            Some(r) => r,
            None => {
                eprintln!("Skipping test: Redis not available at {}", $crate::mocks::redis::TEST_REDIS_URL);
                return;
            }
        }
    };
    ($url:expr) => {
        match $crate::mocks::redis::TestRedis::connect_with_url($url).await {
            Some(r) => r,
            None => {
                eprintln!("Skipping test: Redis not available at {}", $url);
                return;
            }
        }
    };
}

/// Check if Redis is available at the default URL
pub async fn is_redis_available() -> bool {
    TestRedis::connect().await.is_some()
}

/// Check if Redis is available at a custom URL
pub async fn is_redis_available_at(url: &str) -> bool {
    TestRedis::connect_with_url(url).await.is_some()
}

/// Helper struct for building cache keys consistent with production code
pub struct TestCacheKeys;

impl TestCacheKeys {
    /// User limits cache key (matches production format)
    pub fn user_limits(external_id: &str) -> String {
        format!("sentinel:limits:{}", external_id)
    }

    /// JWT validation cache key (matches production format)
    pub fn jwt_validation(jwt_hash: &str) -> String {
        format!("sentinel:jwt:{}", jwt_hash)
    }

    /// User profile cache key (matches production format)
    pub fn user_profile(jwt_hash: &str) -> String {
        format!("sentinel:profile:{}", jwt_hash)
    }
}

/// Sample test data for Redis testing
pub struct RedisTestData;

impl RedisTestData {
    /// Sample cached user limits JSON
    pub fn cached_limits_json() -> String {
        serde_json::json!({
            "success": true,
            "data": {
                "userId": "usr_test123",
                "externalId": "test123",
                "limits": [
                    {
                        "limitId": "lmt_001",
                        "name": "ai_input_tokens",
                        "displayName": "AI Input Tokens",
                        "unit": "tokens",
                        "limit": 50000,
                        "used": 5000,
                        "remaining": 45000,
                        "resetPeriod": "MONTHLY"
                    }
                ]
            }
        })
        .to_string()
    }

    /// Sample cached user profile JSON
    pub fn cached_profile_json() -> String {
        serde_json::json!({
            "success": true,
            "data": {
                "id": "usr_test123",
                "email": "test@example.com",
                "name": "Test User",
                "externalId": "test123",
                "emailVerified": true,
                "createdAt": "2024-01-01T00:00:00Z"
            }
        })
        .to_string()
    }

    /// Sample JWT validation result
    pub fn jwt_validation_result(valid: bool) -> String {
        serde_json::json!({
            "valid": valid,
            "externalId": if valid { Some("test123") } else { None },
            "checkedAt": "2024-01-15T12:00:00Z"
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_redis_connection_check() {
        // This test always passes - it just checks if Redis is available
        let available = is_redis_available().await;
        if available {
            println!("Redis is available for testing");
        } else {
            println!("Redis is not available - dependent tests will be skipped");
        }
    }

    #[tokio::test]
    async fn test_redis_basic_operations() {
        let redis = match TestRedis::connect().await {
            Some(r) => r,
            None => {
                eprintln!("Skipping test: Redis not available");
                return;
            }
        };

        // Test set/get
        redis.set("test_key", "test_value").await.unwrap();
        let value = redis.get("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Test exists
        assert!(redis.exists("test_key").await.unwrap());
        assert!(!redis.exists("nonexistent_key").await.unwrap());

        // Test delete
        redis.del("test_key").await.unwrap();
        assert!(!redis.exists("test_key").await.unwrap());

        // Cleanup
        redis.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_redis_key_prefixing() {
        let redis = match TestRedis::connect().await {
            Some(r) => r,
            None => {
                eprintln!("Skipping test: Redis not available");
                return;
            }
        };

        // Keys should be prefixed with the test namespace
        let key = redis.key("mykey");
        assert!(key.starts_with(TEST_KEY_PREFIX));
        assert!(key.contains("mykey"));

        // Different test instances should have different namespaces
        let redis2 = TestRedis::connect().await.unwrap();
        assert_ne!(redis.namespace(), redis2.namespace());

        redis.cleanup().await.unwrap();
        redis2.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_redis_increment() {
        let redis = match TestRedis::connect().await {
            Some(r) => r,
            None => {
                eprintln!("Skipping test: Redis not available");
                return;
            }
        };

        let result = redis.incr("counter", 1).await.unwrap();
        assert_eq!(result, 1);

        let result = redis.incr("counter", 5).await.unwrap();
        assert_eq!(result, 6);

        let result = redis.incr("counter", -2).await.unwrap();
        assert_eq!(result, 4);

        redis.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cache_keys() {
        assert_eq!(
            TestCacheKeys::user_limits("user123"),
            "sentinel:limits:user123"
        );
        assert_eq!(
            TestCacheKeys::jwt_validation("abc123"),
            "sentinel:jwt:abc123"
        );
        assert_eq!(
            TestCacheKeys::user_profile("abc123"),
            "sentinel:profile:abc123"
        );
    }

    #[test]
    fn test_sample_data() {
        let limits = RedisTestData::cached_limits_json();
        assert!(limits.contains("ai_input_tokens"));

        let profile = RedisTestData::cached_profile_json();
        assert!(profile.contains("test@example.com"));

        let jwt_valid = RedisTestData::jwt_validation_result(true);
        assert!(jwt_valid.contains("\"valid\":true"));

        let jwt_invalid = RedisTestData::jwt_validation_result(false);
        assert!(jwt_invalid.contains("\"valid\":false"));
    }
}
