//! Pass-through proxy handler
//!
//! Generic handler that forwards all unmatched /v1/* requests to the Vercel AI Gateway
//! without parsing the request body. Used for endpoints that don't require token tracking
//! (audio, images, moderations, etc.).

use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{OriginalUri, State},
    http::{header::HeaderMap, Method},
    response::Response,
    Extension,
};
use tracing::info;

use crate::{
    error::AppError,
    middleware::auth::AuthenticatedUser,
    proxy::VercelGateway,
    routes::metrics::record_request,
    AppState,
};

/// Pass-through handler for all /v1/* requests not handled by specific routes
///
/// This handler:
/// 1. Authenticates the user (via middleware)
/// 2. Forwards the request body unchanged to Vercel AI Gateway
/// 3. Streams the response back to the client
/// 4. Tracks usage (request count only, no token tracking)
pub async fn passthrough_handler(
    State(state): State<Arc<AppState>>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    Extension(user): Extension<AuthenticatedUser>,
    request: axum::extract::Request,
) -> Result<Response, AppError> {
    let start_time = Instant::now();
    let path = uri.path().to_string();

    // Strip /v1 prefix since the gateway base URL already includes it
    let forward_path = path
        .strip_prefix("/v1")
        .unwrap_or(&path)
        .to_string();

    info!(
        method = %method,
        path = %path,
        forward_path = %forward_path,
        external_id = %user.external_id,
        "Processing pass-through request"
    );

    // Create gateway client
    let gateway = VercelGateway::new(state.http_client.clone(), &state.config);

    // Extract body from request
    let body = request.into_body();

    // Forward the request
    let response = gateway
        .forward_raw(method.clone(), &forward_path, headers, body)
        .await?;

    // Record metrics
    let duration = start_time.elapsed().as_secs_f64();
    let status_label = if response.status().is_success() {
        "success"
    } else {
        "error"
    };
    record_request(status_label, &path, duration);

    // Track request count only (no token tracking for pass-through endpoints)
    state
        .batching_tracker
        .track_request_only(user.external_id.clone());

    info!(
        method = %method,
        path = %path,
        status = %response.status(),
        duration_ms = %format!("{:.2}", duration * 1000.0),
        external_id = %user.external_id,
        "Pass-through request completed"
    );

    Ok(response)
}
