---
phase: 04-tier-routing
plan: 01b
type: execute
wave: 1
depends_on: ["04-01"]
files_modified:
  - src/zion/client.rs
  - src/tiers/mod.rs
  - src/tiers/config.rs
  - src/tiers/cache.rs
  - src/cache/redis.rs
  - src/config.rs
  - src/lib.rs
autonomous: true

must_haves:
  truths:
    - "ZionClient has get_tier_config method"
    - "TierConfigCache fetches and caches config with 30-minute TTL"
    - "TIER_CONFIG_TTL_SECONDS is configurable via environment"
  artifacts:
    - path: "src/zion/client.rs"
      provides: "get_tier_config method"
      contains: "async fn get_tier_config"
    - path: "src/tiers/config.rs"
      provides: "TierConfigCache service"
      exports: ["TierConfigCache"]
    - path: "src/cache/redis.rs"
      provides: "Tier config cache key function"
      contains: "fn tier_config()"
  key_links:
    - from: "src/tiers/cache.rs"
      to: "src/zion/client.rs"
      via: "Zion tier config fetch"
      pattern: "get_tier_config"
    - from: "src/tiers/cache.rs"
      to: "src/cache/redis.rs"
      via: "Cache key function"
      pattern: "keys::tier_config"
---

<objective>
Wire Zion tier config fetching and caching: ZionClient method, TierConfigCache service, Redis cache key.

Purpose: Enable fetching and caching tier configuration from Zion API. This provides the config data that Plan 02 (TierRouter) will use for model selection.

Output: ZionClient.get_tier_config method, TierConfigCache service with 30-minute TTL, config environment variable.
</objective>

<execution_context>
@/Users/gregor/.claude/get-shit-done/workflows/execute-plan.md
@/Users/gregor/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/phases/04-tier-routing/04-CONTEXT.md
@.planning/phases/04-tier-routing/04-RESEARCH.md
@.planning/phases/04-tier-routing/04-01-SUMMARY.md

@src/zion/client.rs
@src/zion/models.rs
@src/cache/redis.rs
@src/cache/subscription.rs
@src/config.rs
@src/lib.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add get_tier_config to ZionClient</name>
  <files>src/zion/client.rs</files>
  <action>
Add method to fetch tier configuration in `src/zion/client.rs`:

1. Add import for new types:
   ```rust
   use crate::zion::models::TierConfigResponse;
   ```

2. Add the get_tier_config method:
   ```rust
   /// Get tier configuration (global, not per-user)
   ///
   /// Fetches the tier-to-model mapping from Zion. This configuration
   /// is global (same for all users) and changes infrequently.
   #[instrument(skip(self))]
   pub async fn get_tier_config(&self) -> AppResult<TierConfigData> {
       let url = format!("{}/api/v1/tiers/config", self.base_url);

       debug!(url = %url, "Fetching tier config from Zion");

       let response = self
           .client
           .get(&url)
           .headers(self.api_key_headers())
           .send()
           .await?;

       let status = response.status();
       debug!(status = %status, "Zion tier config response status");

       if !status.is_success() {
           let text = response.text().await.unwrap_or_default();
           error!(status = %status, body = %text, "Zion tier config request failed");
           return Err(AppError::UpstreamError(format!(
               "Zion tier config API error {}: {}",
               status, text
           )));
       }

       let body = response.text().await?;
       debug!(body = %body, "Zion tier config response body");

       let result: TierConfigResponse = match serde_json::from_str(&body) {
           Ok(r) => r,
           Err(e) => {
               error!(error = %e, body = %body, "Failed to parse Zion tier config response");
               return Err(AppError::UpstreamError(format!(
                   "Failed to parse Zion tier config response: {}",
                   e
               )));
           }
       };

       debug!(version = %result.data.version, "Successfully fetched tier config");
       Ok(result.data)
   }
   ```

Note: This uses the existing api_key_headers() method for authentication.
  </action>
  <verify>
`cargo check` passes.
Method signature matches expected usage from TierConfigCache.
  </verify>
  <done>
ZionClient can fetch tier configuration.
Method follows existing patterns (api_key_headers, error handling, logging).
  </done>
</task>

<task type="auto">
  <name>Task 2: Add tier config cache key and TTL config</name>
  <files>src/cache/redis.rs, src/config.rs</files>
  <action>
1. Add tier config cache key to the `keys` module in `src/cache/redis.rs`:

```rust
/// Tier configuration cache key (global, not per-user)
pub fn tier_config() -> &'static str {
    "sentinel:tiers:config"
}
```

Add tests:
```rust
#[test]
fn test_tier_config_key_format() {
    let key = keys::tier_config();
    assert_eq!(key, "sentinel:tiers:config");
}

#[test]
fn test_tier_config_key_is_static() {
    // Key is constant (global config, not per-user)
    let key1 = keys::tier_config();
    let key2 = keys::tier_config();
    assert_eq!(key1, key2);
    assert!(std::ptr::eq(key1, key2)); // Same memory location (static)
}
```

2. Add tier config TTL to Config struct in `src/config.rs`:

Add field to Config struct:
```rust
/// Cache TTL for tier configuration (in seconds, default: 30 minutes)
pub tier_config_ttl_seconds: u64,
```

Add to from_env():
```rust
tier_config_ttl_seconds: env::var("TIER_CONFIG_TTL_SECONDS")
    .unwrap_or_else(|_| "1800".to_string())
    .parse()
    .context("Invalid TIER_CONFIG_TTL_SECONDS")?,
```

Add test:
```rust
#[test]
fn test_tier_config_ttl_default() {
    // Set required env vars
    env::set_var("ZION_API_URL", "http://localhost:3000");
    env::set_var("ZION_API_KEY", "test-key");

    let config = Config::from_env().unwrap();

    // Default tier config TTL is 30 minutes (1800 seconds)
    assert_eq!(config.tier_config_ttl_seconds, 1800);
    assert_eq!(config.tier_config_ttl_seconds, 30 * 60);

    // Clean up
    env::remove_var("ZION_API_URL");
    env::remove_var("ZION_API_KEY");
}
```
  </action>
  <verify>
`cargo test cache::redis::tests` passes with new tier config key tests.
`cargo test config::tests` passes with tier config TTL test.
`cargo check` passes.
  </verify>
  <done>
Tier config cache key exists and is a static string.
Config includes tier_config_ttl_seconds with 30-minute default.
TIER_CONFIG_TTL_SECONDS environment variable is supported.
  </done>
</task>

<task type="auto">
  <name>Task 3: Create tiers module with TierConfigCache</name>
  <files>src/tiers/mod.rs, src/tiers/config.rs, src/tiers/cache.rs, src/lib.rs</files>
  <action>
Create the tiers module structure:

1. Create `src/tiers/mod.rs`:
   ```rust
   //! Tier routing module
   //!
   //! Handles mapping complexity tiers to AI models based on configuration from Zion.

   pub mod cache;
   pub mod config;

   pub use cache::TierConfigCache;
   pub use config::TierConfig;
   ```

2. Create `src/tiers/config.rs`:
   ```rust
   //! Tier configuration types
   //!
   //! Re-exports and utilities for tier configuration.

   use crate::zion::models::{ModelConfig, TierConfigData, TierMapping};
   use crate::native::types::Tier;

   /// Type alias for tier configuration
   pub type TierConfig = TierConfigData;

   impl TierConfig {
       /// Get models for a specific tier
       pub fn models_for_tier(&self, tier: Tier) -> &[ModelConfig] {
           match tier {
               Tier::Simple => &self.tiers.simple,
               Tier::Moderate => &self.tiers.moderate,
               Tier::Complex => &self.tiers.complex,
           }
       }
   }
   ```

3. Create `src/tiers/cache.rs`:
   ```rust
   //! Tier configuration cache
   //!
   //! Caches tier configuration from Zion with TTL.

   use std::sync::Arc;

   use tracing::{debug, instrument};

   use crate::{
       cache::redis::{keys, RedisCache},
       error::AppResult,
       zion::{ZionClient, models::TierConfigData},
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
               TierConfigCacheBackend::InMemory(cache) => cache.set_with_ttl(key, value, ttl_seconds).await,
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
       pub fn new(
           cache: Arc<RedisCache>,
           zion_client: Arc<ZionClient>,
           ttl: u64,
       ) -> Self {
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
           self.cache.set_with_ttl(cache_key, &config, self.ttl).await?;

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
   ```

4. Update `src/lib.rs` to add the tiers module:
   Add `pub mod tiers;` after the existing module declarations.
   Add export: `pub use crate::tiers::TierConfigCache;`
  </action>
  <verify>
`cargo check` passes.
`cargo test tiers::` runs and passes.
  </verify>
  <done>
tiers module created with TierConfigCache service.
Follows SubscriptionCache pattern for cache backend abstraction.
  </done>
</task>

</tasks>

<verification>
1. `cargo check` - No compilation errors
2. `cargo test cache::redis::tests` - Cache key tests pass
3. `cargo test config::tests` - Config tests pass
4. `cargo test tiers::` - Tier module tests pass
</verification>

<success_criteria>
- ZionClient has get_tier_config method
- TierConfigCache fetches and caches with TTL
- TIER_CONFIG_TTL_SECONDS config with 1800 default
- All existing tests pass
</success_criteria>

<output>
After completion, create `.planning/phases/04-tier-routing/04-01b-SUMMARY.md`
</output>
