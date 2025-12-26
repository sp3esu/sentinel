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
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    middleware::{auth::auth_middleware, rate_limiter::rate_limit_middleware},
    AppState,
};

/// Create the main application router
pub fn create_router(state: Arc<AppState>) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Routes that require authentication and rate limiting
    // Middleware is applied in reverse order (last applied runs first)
    // So: auth runs first, then rate limiting
    let protected_routes = Router::new()
        .route("/v1/chat/completions", post(chat::chat_completions))
        .route("/v1/completions", post(completions::completions))
        .route("/v1/models", get(models::list_models))
        .route("/v1/models/:model_id", get(models::get_model))
        // Apply rate limiting (runs after auth)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        // Apply authentication (runs first)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Public routes (health checks, metrics) - no auth required
    let public_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/health/ready", get(health::readiness_check))
        .route("/health/live", get(health::liveness_check))
        .route("/metrics", get(metrics::prometheus_metrics));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        // Global middleware (applied to all routes)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
