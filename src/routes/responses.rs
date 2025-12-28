//! OpenAI Responses API handler
//!
//! Routes /v1/responses directly to OpenAI since this endpoint
//! is specific to OpenAI and not available through all providers.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::State,
    http::{header::HeaderMap, Method},
    response::Response,
    Extension,
};
use tracing::info;

use crate::{
    error::AppError,
    middleware::auth::AuthenticatedUser,
    routes::metrics::record_request,
    AppState,
};

/// Handler for POST /v1/responses
///
/// This endpoint routes directly to OpenAI API.
pub async fn responses_handler(
    State(state): State<Arc<AppState>>,
    method: Method,
    headers: HeaderMap,
    Extension(user): Extension<AuthenticatedUser>,
    request: axum::extract::Request,
) -> Result<Response, AppError> {
    let start_time = Instant::now();
    let path = "/responses";

    info!(
        method = %method,
        path = %path,
        external_id = %user.external_id,
        "Processing OpenAI Responses API request"
    );

    // Extract body from request
    let body = request.into_body();

    // Forward the request using the AI provider
    let response = state
        .ai_provider
        .forward_raw(method.clone(), path, headers, body)
        .await?;

    // Record metrics
    let duration = start_time.elapsed().as_secs_f64();
    let status_label = if response.status().is_success() {
        "success"
    } else {
        "error"
    };
    record_request(status_label, "/v1/responses", duration);

    // Track request count only (no token tracking for responses endpoint)
    state
        .batching_tracker
        .track_request_only(user.external_id.clone());

    info!(
        method = %method,
        path = %path,
        status = %response.status(),
        duration_ms = %format!("{:.2}", duration * 1000.0),
        external_id = %user.external_id,
        "OpenAI Responses API request completed"
    );

    Ok(response)
}
