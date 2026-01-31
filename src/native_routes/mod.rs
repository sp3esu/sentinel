//! Native API routes for Sentinel
//!
//! This module provides routes for the Native API endpoints under `/native/*`.
//! These endpoints accept the unified Native API format and translate to
//! provider-specific formats internally.

pub mod chat;

use std::sync::Arc;

use axum::{middleware, routing::post, Router};

use crate::{
    middleware::{auth::auth_middleware, rate_limiter::rate_limit_middleware},
    AppState,
};

/// Create the native API router
///
/// Routes:
/// - POST /v1/chat/completions - Chat completions (streaming + non-streaming)
///
/// All routes require authentication and rate limiting.
/// Middleware is applied in reverse order (last applied runs first):
/// - auth_middleware runs first
/// - rate_limit_middleware runs second
///
/// Returns a `Router<Arc<AppState>>` to be nested into the main router.
/// Do not call `.with_state()` on the returned router - the parent router
/// will provide the state.
pub fn create_native_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/chat/completions", post(chat::native_chat_completions))
        // Apply rate limiting (runs after auth)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        // Apply authentication (runs first)
        .layer(middleware::from_fn_with_state(state, auth_middleware))
}
