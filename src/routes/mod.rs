//! HTTP routes for Sentinel
//!
//! This module defines all HTTP endpoints exposed by the proxy.
//!
//! ## Route Architecture
//!
//! Sentinel uses a hybrid routing approach:
//! - **Typed handlers** for endpoints that need token tracking (chat, completions, embeddings)
//! - **Pass-through handler** for all other /v1/* endpoints (audio, images, moderations, etc.)

pub mod chat;
pub mod completions;
pub mod debug;
pub mod embeddings;
pub mod health;
pub mod metrics;
pub mod models;
pub mod passthrough;
pub mod responses;

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::warn;

use crate::{
    middleware::{auth::auth_middleware, rate_limiter::rate_limit_middleware},
    native_routes,
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
    //
    // Using nest() so that the fallback works correctly for /v1/* routes.
    // Routes are defined without /v1 prefix since nest() adds it.
    let protected_routes = Router::new()
        // Typed handlers with token tracking
        .route("/chat/completions", post(chat::chat_completions))
        .route("/completions", post(completions::completions))
        .route("/embeddings", post(embeddings::embeddings))
        .route("/models", get(models::list_models))
        .route("/models/{model_id}", get(models::get_model))
        // OpenAI Responses API - routes directly to OpenAI (not supported by Vercel AI Gateway)
        .route("/responses", post(responses::responses_handler))
        // Pass-through handler for all other /v1/* endpoints
        // Handles: audio, images, moderations, assistants, etc.
        .fallback(passthrough::passthrough_handler)
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

    // Debug routes (only available when SENTINEL_DEBUG=true)
    let debug_routes = Router::new()
        .route("/debug/cache", get(debug::cache_overview))
        .route("/debug/auth/:external_id", get(debug::user_auth_state))
        .route("/debug/config", get(debug::config_info));

    Router::new()
        .merge(public_routes)
        .merge(debug_routes)
        // Nest protected routes under /v1 - this makes fallback work correctly
        .nest("/v1", protected_routes)
        // Nest native API routes under /native - unified format with translation
        .nest("/native", native_routes::create_native_router(state.clone()))
        // Fallback for non-/v1 routes
        .fallback(fallback_handler)
        // Global middleware (applied to all routes)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// Fallback handler for unmatched routes
///
/// Logs the request details and returns a helpful 404 response.
/// Note: All /v1/* routes are handled by the pass-through handler,
/// so this only catches non-API routes.
async fn fallback_handler(request: Request<Body>) -> impl IntoResponse {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path();

    warn!(
        method = %method,
        path = %path,
        uri = %uri,
        "Unmatched route - not an API endpoint"
    );

    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "message": format!("Endpoint {} {} not found. API endpoints are under /v1/ and /native/", method, path),
                "type": "not_found_error",
                "code": "endpoint_not_found"
            }
        })),
    )
}
