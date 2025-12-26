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

use crate::config::Config;

/// Application state shared across all request handlers
pub struct AppState {
    pub config: Config,
    pub redis: redis::aio::ConnectionManager,
    pub http_client: reqwest::Client,
    pub start_time: Instant,
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

        Ok(Self {
            config,
            redis,
            http_client,
            start_time: Instant::now(),
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
