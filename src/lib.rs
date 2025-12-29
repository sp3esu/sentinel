//! Sentinel - High-performance AI proxy with traffic limiting
//!
//! This library provides the core functionality for the Sentinel proxy server.
//! It handles AI request proxying with user authentication, rate limiting,
//! and token tracking.

pub mod cache;
pub mod config;
pub mod error;
pub mod middleware;
pub mod proxy;
pub mod routes;
pub mod tokens;
pub mod usage;
pub mod zion;

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

pub use crate::cache::{RedisCache, SubscriptionCache};
pub use crate::config::Config;
pub use crate::proxy::{AiProvider, OpenAIProvider};
pub use crate::tokens::SharedTokenCounter;
pub use crate::usage::{BatchingUsageTracker, UsageTracker};
pub use crate::zion::ZionClient;

/// Application state shared across all request handlers
pub struct AppState {
    pub config: Config,
    pub redis: redis::aio::ConnectionManager,
    pub http_client: reqwest::Client,
    pub start_time: Instant,
    pub zion_client: Arc<ZionClient>,
    pub subscription_cache: Arc<SubscriptionCache>,
    /// Synchronous usage tracker for immediate tracking (used for streaming)
    pub usage_tracker: Arc<UsageTracker>,
    /// Batching usage tracker for fire-and-forget tracking (protects Zion from floods)
    pub batching_tracker: Arc<BatchingUsageTracker>,
    /// AI provider for forwarding requests to LLM backends
    pub ai_provider: Arc<dyn AiProvider>,
    /// Token counter for estimating token usage with tiktoken-rs
    pub token_counter: SharedTokenCounter,
}

impl AppState {
    /// Create a new application state
    pub async fn new(config: Config) -> Result<Self> {
        // Initialize Redis connection
        let redis_client = redis::Client::open(config.redis_url.as_str())?;
        let redis = redis::aio::ConnectionManager::new(redis_client).await?;

        // Initialize HTTP client with connection pooling
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(100)
            .timeout(std::time::Duration::from_secs(300))
            .build()?;

        // Initialize Zion client
        let zion_client = Arc::new(ZionClient::new(http_client.clone(), &config));

        // Initialize Redis cache
        let redis_cache = Arc::new(RedisCache::new(redis.clone(), config.cache_ttl_seconds));

        // Initialize subscription cache
        let subscription_cache = Arc::new(SubscriptionCache::new(
            redis_cache,
            zion_client.clone(),
            config.cache_ttl_seconds,
            config.jwt_cache_ttl_seconds,
        ));

        // Initialize usage tracker (synchronous, for streaming)
        let usage_tracker = Arc::new(UsageTracker::new(zion_client.clone()));

        // Initialize batching usage tracker (fire-and-forget, protects Zion)
        let batching_tracker = Arc::new(BatchingUsageTracker::with_defaults(
            zion_client.clone(),
            redis.clone(),
        ));

        // Initialize AI provider (OpenAI by default)
        // Note: Will panic if OPENAI_API_KEY is not set - this is intentional
        // as the proxy cannot function without an AI provider
        let ai_provider: Arc<dyn AiProvider> = Arc::new(OpenAIProvider::new(
            http_client.clone(),
            &config,
        ));

        // Initialize token counter for tiktoken-based token estimation
        let token_counter = SharedTokenCounter::new();

        Ok(Self {
            config,
            redis,
            http_client,
            start_time: Instant::now(),
            zion_client,
            subscription_cache,
            usage_tracker,
            batching_tracker,
            ai_provider,
            token_counter,
        })
    }

    /// Create a new application state for testing with real Redis but mocked HTTP services
    ///
    /// This constructor creates a real AppState that uses:
    /// - Real Redis connection (required for caching)
    /// - Mock Zion client (pointing to wiremock server)
    /// - Mock AI provider (pointing to wiremock server)
    /// - Test batching tracker (without Redis retry)
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn new_for_testing(
        config: Config,
        redis: redis::aio::ConnectionManager,
        zion_client: Arc<ZionClient>,
        ai_provider: Arc<dyn AiProvider>,
        batching_tracker: Arc<BatchingUsageTracker>,
    ) -> Self {
        let http_client = reqwest::Client::new();
        let token_counter = SharedTokenCounter::new();
        let usage_tracker = Arc::new(UsageTracker::new(zion_client.clone()));

        // Create Redis cache with short TTL for testing
        let redis_cache = Arc::new(RedisCache::new(redis.clone(), 60));

        // Create subscription cache with short TTLs for testing
        let subscription_cache = Arc::new(SubscriptionCache::new(
            redis_cache,
            zion_client.clone(),
            60, // 1 minute TTL for limits
            60, // 1 minute TTL for JWT
        ));

        Self {
            config,
            redis,
            http_client,
            start_time: Instant::now(),
            zion_client,
            subscription_cache,
            usage_tracker,
            batching_tracker,
            ai_provider,
            token_counter,
        }
    }
}
