//! OpenAI Responses API handler
//!
//! Routes /v1/responses directly to OpenAI since Vercel AI Gateway
//! doesn't support the Responses API endpoint.

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
    proxy::OpenAIClient,
    routes::metrics::record_request,
    AppState,
};

/// Handler for POST /v1/responses
///
/// This endpoint routes directly to OpenAI API since Vercel AI Gateway
/// doesn't support the Responses API.
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

    // Create OpenAI client
    let openai = OpenAIClient::new(state.http_client.clone(), &state.config);

    // Check if OpenAI is configured
    if !openai.is_configured() {
        return Err(AppError::ServiceUnavailable(
            "OpenAI API key not configured. Set OPENAI_API_KEY to use the Responses API.".to_string()
        ));
    }

    // Extract body from request
    let body = request.into_body();

    // Forward the request directly to OpenAI
    let response = openai
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
