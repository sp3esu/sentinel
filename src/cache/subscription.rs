//! Subscription cache service
//!
//! Provides caching for user limits and JWT validation results.

use std::sync::Arc;

use tracing::{debug, instrument};

use crate::{
    cache::redis::{keys, RedisCache},
    error::AppResult,
    zion::{UserLimit, UserProfile, ZionClient},
};

/// Subscription cache service
///
/// This service provides a caching layer on top of the Zion API,
/// caching user limits and JWT validation results in Redis.
pub struct SubscriptionCache {
    cache: Arc<RedisCache>,
    zion_client: Arc<ZionClient>,
    limits_ttl: u64,
    jwt_ttl: u64,
}

impl SubscriptionCache {
    /// Create a new subscription cache
    pub fn new(
        cache: Arc<RedisCache>,
        zion_client: Arc<ZionClient>,
        limits_ttl: u64,
        jwt_ttl: u64,
    ) -> Self {
        Self {
            cache,
            zion_client,
            limits_ttl,
            jwt_ttl,
        }
    }

    /// Get user limits, using cache if available
    ///
    /// Returns cached limits if present, otherwise fetches from Zion API
    /// and caches the result.
    #[instrument(skip(self), fields(external_id = %external_id))]
    pub async fn get_user_limits(&self, external_id: &str) -> AppResult<Vec<UserLimit>> {
        let cache_key = keys::user_limits(external_id);

        // Try cache first
        if let Some(limits) = self.cache.get::<Vec<UserLimit>>(&cache_key).await? {
            debug!("Cache hit for user limits");
            return Ok(limits);
        }

        debug!("Cache miss for user limits, fetching from Zion");

        // Fetch from Zion API
        let limits = self.zion_client.get_limits(external_id).await?;

        // Cache the result
        self.cache
            .set_with_ttl(&cache_key, &limits, self.limits_ttl)
            .await?;

        Ok(limits)
    }

    /// Set user limits in cache
    ///
    /// Useful for updating cache after usage increment.
    #[instrument(skip(self, limits), fields(external_id = %external_id))]
    pub async fn set_user_limits(
        &self,
        external_id: &str,
        limits: &[UserLimit],
    ) -> AppResult<()> {
        let cache_key = keys::user_limits(external_id);
        self.cache
            .set_with_ttl(&cache_key, &limits, self.limits_ttl)
            .await
    }

    /// Invalidate user limits cache
    ///
    /// Call this after modifying usage to ensure fresh data on next request.
    #[instrument(skip(self), fields(external_id = %external_id))]
    pub async fn invalidate_user_limits(&self, external_id: &str) -> AppResult<()> {
        let cache_key = keys::user_limits(external_id);
        debug!("Invalidating user limits cache");
        self.cache.delete(&cache_key).await
    }

    /// Validate JWT and get user profile, using cache if available
    ///
    /// The jwt_hash should be a SHA256 hash of the JWT token.
    #[instrument(skip(self, jwt), fields(jwt_hash = %jwt_hash))]
    pub async fn validate_jwt(
        &self,
        jwt: &str,
        jwt_hash: &str,
    ) -> AppResult<UserProfile> {
        let cache_key = keys::user_profile(jwt_hash);

        // Try cache first
        if let Some(profile) = self.cache.get::<UserProfile>(&cache_key).await? {
            debug!("Cache hit for JWT validation");
            return Ok(profile);
        }

        debug!("Cache miss for JWT validation, validating with Zion");

        // Validate with Zion API
        let profile = self.zion_client.validate_jwt(jwt).await?;

        // Cache the result
        self.cache
            .set_with_ttl(&cache_key, &profile, self.jwt_ttl)
            .await?;

        Ok(profile)
    }

    /// Get cached user profile by JWT hash
    ///
    /// Returns None if not in cache (does not fetch from Zion).
    #[instrument(skip(self), fields(jwt_hash = %jwt_hash))]
    pub async fn get_cached_profile(&self, jwt_hash: &str) -> AppResult<Option<UserProfile>> {
        let cache_key = keys::user_profile(jwt_hash);
        self.cache.get::<UserProfile>(&cache_key).await
    }

    /// Set user profile in cache
    #[instrument(skip(self, profile), fields(jwt_hash = %jwt_hash))]
    pub async fn set_profile(&self, jwt_hash: &str, profile: &UserProfile) -> AppResult<()> {
        let cache_key = keys::user_profile(jwt_hash);
        self.cache
            .set_with_ttl(&cache_key, profile, self.jwt_ttl)
            .await
    }

    /// Invalidate JWT validation cache
    #[instrument(skip(self), fields(jwt_hash = %jwt_hash))]
    pub async fn invalidate_jwt(&self, jwt_hash: &str) -> AppResult<()> {
        let cache_key = keys::user_profile(jwt_hash);
        debug!("Invalidating JWT cache");
        self.cache.delete(&cache_key).await
    }

    /// Increment usage and invalidate cache
    ///
    /// This helper method increments usage via Zion and then invalidates
    /// the cached limits to ensure fresh data on next request.
    #[instrument(skip(self), fields(external_id = %external_id, limit_name = %limit_name, amount = %amount))]
    pub async fn increment_usage(
        &self,
        external_id: &str,
        limit_name: &str,
        amount: i64,
    ) -> AppResult<UserLimit> {
        // Increment via Zion API
        let updated_limit = self
            .zion_client
            .increment_usage(external_id, limit_name, amount)
            .await?;

        // Invalidate cached limits
        self.invalidate_user_limits(external_id).await?;

        Ok(updated_limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would require a mock Redis and Zion client
    // For now, we just ensure the module compiles correctly
}
