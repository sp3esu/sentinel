//! Redis cache implementation
//!
//! Handles caching of user limits and JWT validation results.

use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::{AppError, AppResult};

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
        conn.set_ex(key, serialized, ttl_seconds).await?;
        Ok(())
    }

    /// Delete a key from cache
    pub async fn delete(&self, key: &str) -> AppResult<()> {
        let mut conn = self.conn.clone();
        conn.del(key).await?;
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
        conn.expire(key, seconds as i64).await?;
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::keys;

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
}
