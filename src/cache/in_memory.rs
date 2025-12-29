//! In-memory cache implementation for testing
//!
//! This module provides an in-memory cache that can be used in place of Redis
//! during integration testing, eliminating the need for a real Redis instance.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::{de::DeserializeOwned, Serialize};

use crate::error::AppResult;

/// Entry in the in-memory cache with expiration
struct CacheEntry {
    value: String,
    expires_at: Option<Instant>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Instant::now() > exp).unwrap_or(false)
    }
}

/// In-memory cache for testing
///
/// This cache stores values in a HashMap and supports TTL-based expiration.
/// It's designed to have the same API as RedisCache for easy substitution.
///
/// # Thread Safety
///
/// Uses RwLock for interior mutability, allowing concurrent reads.
pub struct InMemoryCache {
    data: RwLock<HashMap<String, CacheEntry>>,
    default_ttl: u64,
}

impl InMemoryCache {
    /// Create a new in-memory cache with the specified default TTL
    pub fn new(default_ttl: u64) -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            default_ttl,
        }
    }

    /// Get a value from cache
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> AppResult<Option<T>> {
        let data = self.data.read().unwrap();

        match data.get(key) {
            Some(entry) if !entry.is_expired() => {
                let parsed: T = serde_json::from_str(&entry.value)?;
                Ok(Some(parsed))
            }
            _ => Ok(None),
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
        let serialized = serde_json::to_string(value)?;
        let expires_at = if ttl_seconds > 0 {
            Some(Instant::now() + Duration::from_secs(ttl_seconds))
        } else {
            None
        };

        let mut data = self.data.write().unwrap();
        data.insert(
            key.to_string(),
            CacheEntry {
                value: serialized,
                expires_at,
            },
        );
        Ok(())
    }

    /// Delete a key from cache
    pub async fn delete(&self, key: &str) -> AppResult<()> {
        let mut data = self.data.write().unwrap();
        data.remove(key);
        Ok(())
    }

    /// Check if a key exists (and is not expired)
    pub async fn exists(&self, key: &str) -> AppResult<bool> {
        let data = self.data.read().unwrap();
        match data.get(key) {
            Some(entry) => Ok(!entry.is_expired()),
            None => Ok(false),
        }
    }

    /// Increment a counter
    pub async fn incr(&self, key: &str, delta: i64) -> AppResult<i64> {
        let mut data = self.data.write().unwrap();

        let current: i64 = data
            .get(key)
            .filter(|e| !e.is_expired())
            .and_then(|e| e.value.parse().ok())
            .unwrap_or(0);

        let new_value = current + delta;
        data.insert(
            key.to_string(),
            CacheEntry {
                value: new_value.to_string(),
                expires_at: None,
            },
        );

        Ok(new_value)
    }

    /// Set expiry on a key
    pub async fn expire(&self, key: &str, seconds: u64) -> AppResult<()> {
        let mut data = self.data.write().unwrap();

        if let Some(entry) = data.get_mut(key) {
            entry.expires_at = Some(Instant::now() + Duration::from_secs(seconds));
        }

        Ok(())
    }

    /// Clear all entries (useful for test isolation)
    #[allow(dead_code)]
    pub fn clear(&self) {
        let mut data = self.data.write().unwrap();
        data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get() {
        let cache = InMemoryCache::new(60);

        cache.set("key1", &"value1").await.unwrap();
        let result: Option<String> = cache.get("key1").await.unwrap();

        assert_eq!(result, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_get_missing_key() {
        let cache = InMemoryCache::new(60);

        let result: Option<String> = cache.get("nonexistent").await.unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_delete() {
        let cache = InMemoryCache::new(60);

        cache.set("key1", &"value1").await.unwrap();
        cache.delete("key1").await.unwrap();
        let result: Option<String> = cache.get("key1").await.unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_exists() {
        let cache = InMemoryCache::new(60);

        assert!(!cache.exists("key1").await.unwrap());

        cache.set("key1", &"value1").await.unwrap();
        assert!(cache.exists("key1").await.unwrap());
    }

    #[tokio::test]
    async fn test_incr() {
        let cache = InMemoryCache::new(60);

        let v1 = cache.incr("counter", 1).await.unwrap();
        assert_eq!(v1, 1);

        let v2 = cache.incr("counter", 5).await.unwrap();
        assert_eq!(v2, 6);

        let v3 = cache.incr("counter", -2).await.unwrap();
        assert_eq!(v3, 4);
    }

    #[tokio::test]
    async fn test_struct_serialization() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct TestData {
            name: String,
            count: i32,
        }

        let cache = InMemoryCache::new(60);
        let data = TestData {
            name: "test".to_string(),
            count: 42,
        };

        cache.set("data", &data).await.unwrap();
        let result: Option<TestData> = cache.get("data").await.unwrap();

        assert_eq!(result, Some(data));
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = InMemoryCache::new(60);

        cache.set("key1", &"value1").await.unwrap();
        cache.set("key2", &"value2").await.unwrap();

        cache.clear();

        assert!(!cache.exists("key1").await.unwrap());
        assert!(!cache.exists("key2").await.unwrap());
    }
}
