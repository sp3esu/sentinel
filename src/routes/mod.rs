//! HTTP routes for Sentinel
//!
//! This module defines all HTTP endpoints exposed by the proxy.

pub mod chat;
pub mod completions;
pub mod health;
pub mod metrics;
pub mod models;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::AppState;

/// Create the main application router
pub fn create_router(state: Arc<AppState>) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health & monitoring endpoints
        .route("/health", get(health::health_check))
        .route("/health/ready", get(health::readiness_check))
        .route("/health/live", get(health::liveness_check))
        .route("/metrics", get(metrics::prometheus_metrics))
        // OpenAI-compatible API endpoints
        .route("/v1/chat/completions", post(chat::chat_completions))
        .route("/v1/completions", post(completions::completions))
        .route("/v1/models", get(models::list_models))
        // Middleware
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
