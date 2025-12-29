//! Sentinel - High-performance AI proxy with traffic limiting
//!
//! This is the main entry point for the Sentinel proxy server.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tokio::signal;
use tracing::{info, warn};

mod cache;
mod config;
mod error;
mod middleware;
mod proxy;
mod routes;
mod tokens;
mod usage;
mod zion;

use crate::cache::{RedisCache, SubscriptionCache};
use crate::config::Config;
use crate::proxy::{AiProvider, OpenAIProvider};
use crate::tokens::SharedTokenCounter;
use crate::usage::{BatchingUsageTracker, UsageTracker};
use crate::zion::ZionClient;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sentinel=info,tower_http=info".into()),
        )
        .with_target(true)
        .with_thread_ids(true)
        .init();

    info!("Starting Sentinel AI Proxy");

    // Load configuration
    let config = Config::from_env()?;
    info!("Configuration loaded successfully");

    // Initialize metrics
    routes::metrics::init_metrics();
    info!("Metrics initialized");

    // Initialize application state
    let state = Arc::new(AppState::new(config.clone()).await?);
    info!("Application state initialized");

    // Build the router
    let app = routes::create_router(state.clone());

    // Bind to address
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!("Listening on {}", addr);

    // Create listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Start server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Sentinel shutdown complete");
    Ok(())
}

/// Handle graceful shutdown signals
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            warn!("Received Ctrl+C, initiating shutdown");
        }
        _ = terminate => {
            warn!("Received SIGTERM, initiating shutdown");
        }
    }
}
