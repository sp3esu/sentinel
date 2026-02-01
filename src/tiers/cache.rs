//! Tier configuration cache
//!
//! Caches tier configuration from Zion with TTL.

use std::sync::Arc;

use tracing::{debug, instrument};

use crate::{
    cache::redis::{keys, RedisCache},
    error::AppResult,
    zion::{models::TierConfigData, ZionClient},
};

#[cfg(any(test, feature = "test-utils"))]
use crate::cache::InMemoryCache;

/// Cache backend abstraction for TierConfigCache
///
/// Follows the pattern from SubscriptionCache for consistency.
pub enum TierConfigCacheBackend {
    /// Redis-based cache for production use
    Redis(Arc<RedisCache>),
    /// In-memory cache for testing
    #[cfg(any(test, feature = "test-utils"))]
    InMemory(Arc<InMemoryCache>),
}

impl TierConfigCacheBackend {
    async fn get(&self, key: &str) -> AppResult<Option<TierConfigData>> {
        match self {
            TierConfigCacheBackend::Redis(cache) => cache.get(key).await,
            #[cfg(any(test, feature = "test-utils"))]
            TierConfigCacheBackend::InMemory(cache) => cache.get(key).await,
        }
    }

    async fn set_with_ttl(
        &self,
        key: &str,
        value: &TierConfigData,
        ttl_seconds: u64,
    ) -> AppResult<()> {
        match self {
            TierConfigCacheBackend::Redis(cache) => cache.set_with_ttl(key, value, ttl_seconds).await,
            #[cfg(any(test, feature = "test-utils"))]
            TierConfigCacheBackend::InMemory(cache) => {
                cache.set_with_ttl(key, value, ttl_seconds).await
            }
        }
    }
}

/// Tier configuration cache service
///
/// Fetches tier configuration from Zion and caches it with a 30-minute TTL.
/// This is a global configuration (same for all users).
pub struct TierConfigCache {
    cache: TierConfigCacheBackend,
    zion_client: Arc<ZionClient>,
    ttl: u64,
}

impl TierConfigCache {
    /// Create a new tier config cache with Redis backend
    pub fn new(cache: Arc<RedisCache>, zion_client: Arc<ZionClient>, ttl: u64) -> Self {
        Self {
            cache: TierConfigCacheBackend::Redis(cache),
            zion_client,
            ttl,
        }
    }

    /// Create for testing with in-memory backend
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_testing(
        cache: Arc<InMemoryCache>,
        zion_client: Arc<ZionClient>,
        ttl: u64,
    ) -> Self {
        Self {
            cache: TierConfigCacheBackend::InMemory(cache),
            zion_client,
            ttl,
        }
    }

    /// Get tier configuration, using cache if available
    ///
    /// Returns cached config if present, otherwise fetches from Zion
    /// and caches the result with configured TTL.
    #[instrument(skip(self))]
    pub async fn get_config(&self) -> AppResult<TierConfigData> {
        let cache_key = keys::tier_config();

        // Try cache first
        if let Some(config) = self.cache.get(cache_key).await? {
            debug!(version = %config.version, "Tier config cache hit");
            return Ok(config);
        }

        debug!("Tier config cache miss, fetching from Zion");

        // Fetch from Zion
        let config = self.zion_client.get_tier_config().await?;

        // Cache the result
        self.cache
            .set_with_ttl(cache_key, &config, self.ttl)
            .await?;

        debug!(version = %config.version, "Tier config cached");
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_config_cache_backend_enum() {
        // Compile-time check that enum variants exist
        fn _type_check(_backend: TierConfigCacheBackend) {}
    }
}
