//! Prometheus metrics endpoint
//!
//! Exposes application metrics in Prometheus format for monitoring.

use axum::response::IntoResponse;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::Lazy;

/// Global Prometheus handle for metrics export
static PROMETHEUS_HANDLE: Lazy<PrometheusHandle> = Lazy::new(|| {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus recorder")
});

/// Initialize metrics (call once at startup)
pub fn init_metrics() {
    // Force initialization of the lazy static
    let _ = &*PROMETHEUS_HANDLE;

    // Register custom metrics
    register_metrics();
}

/// Register all custom metrics
fn register_metrics() {
    // These metrics are registered by usage, but we can describe them here
    metrics::describe_counter!(
        "sentinel_requests_total",
        "Total number of requests processed"
    );
    metrics::describe_counter!(
        "sentinel_tokens_processed_total",
        "Total tokens processed"
    );
    metrics::describe_counter!(
        "sentinel_cache_operations_total",
        "Total cache operations"
    );
    metrics::describe_histogram!(
        "sentinel_request_duration_seconds",
        "Request duration in seconds"
    );
    metrics::describe_gauge!(
        "sentinel_active_connections",
        "Number of active connections"
    );
    metrics::describe_histogram!(
        "sentinel_token_estimation_diff",
        "Difference between estimated and actual input tokens (actual - estimated)"
    );
    metrics::describe_histogram!(
        "sentinel_token_estimation_diff_pct",
        "Percentage difference between estimated and actual input tokens"
    );
    metrics::describe_counter!(
        "sentinel_sse_parse_errors_total",
        "Total SSE JSON parse errors encountered during streaming"
    );
    metrics::describe_counter!(
        "sentinel_token_estimation_fallback_total",
        "Times token counting fell back to estimation (OpenAI didn't return usage)"
    );

    // Tier routing metrics
    metrics::describe_counter!(
        "sentinel_tier_requests_total",
        "Total requests by tier"
    );
    metrics::describe_counter!(
        "sentinel_model_selections_total",
        "Model selections by tier, provider, and model"
    );
    metrics::describe_counter!(
        "sentinel_provider_failures_total",
        "Provider failures triggering backoff"
    );
    metrics::describe_counter!(
        "sentinel_model_retries_total",
        "Model retry attempts after initial failure"
    );
    metrics::describe_gauge!(
        "sentinel_provider_health",
        "Provider health status (1=healthy, 0=in backoff)"
    );
}

/// Prometheus metrics endpoint handler
///
/// Returns metrics in Prometheus text format for scraping.
pub async fn prometheus_metrics() -> impl IntoResponse {
    PROMETHEUS_HANDLE.render()
}

/// Record a request
pub fn record_request(status: &str, model: &str, duration_secs: f64) {
    metrics::counter!("sentinel_requests_total", "status" => status.to_string(), "model" => model.to_string())
        .increment(1);
    metrics::histogram!("sentinel_request_duration_seconds", "model" => model.to_string())
        .record(duration_secs);
}

/// Record tokens processed
pub fn record_tokens(token_type: &str, count: u64, model: &str) {
    metrics::counter!(
        "sentinel_tokens_processed_total",
        "type" => token_type.to_string(),
        "model" => model.to_string()
    )
    .increment(count);
}

/// Record cache operation
pub fn record_cache_operation(operation: &str, result: &str) {
    metrics::counter!(
        "sentinel_cache_operations_total",
        "operation" => operation.to_string(),
        "result" => result.to_string()
    )
    .increment(1);
}

/// Update active connections gauge
pub fn set_active_connections(count: f64) {
    metrics::gauge!("sentinel_active_connections").set(count);
}

/// Record token estimation accuracy metrics
///
/// Records the difference between our tiktoken-based estimation and OpenAI's
/// actual reported token count. Useful for monitoring estimation accuracy.
pub fn record_token_estimation_diff(model: &str, estimated: u64, actual: u64) {
    let diff = (actual as i64) - (estimated as i64);
    let diff_pct = if estimated > 0 {
        diff as f64 / estimated as f64 * 100.0
    } else {
        0.0
    };

    metrics::histogram!(
        "sentinel_token_estimation_diff",
        "model" => model.to_string()
    )
    .record(diff as f64);

    metrics::histogram!(
        "sentinel_token_estimation_diff_pct",
        "model" => model.to_string()
    )
    .record(diff_pct);
}

/// Record SSE parse error during streaming
///
/// Called when a complete SSE line fails to parse as valid JSON.
/// This helps identify issues with malformed responses from upstream providers.
pub fn record_sse_parse_error(endpoint: &str, model: &str) {
    metrics::counter!(
        "sentinel_sse_parse_errors_total",
        "endpoint" => endpoint.to_string(),
        "model" => model.to_string()
    )
    .increment(1);
}

/// Record when token counting falls back to estimation
///
/// Called when OpenAI doesn't return usage data in streaming response,
/// requiring Sentinel to use tiktoken-based estimation instead.
pub fn record_fallback_estimation(model: &str) {
    metrics::counter!(
        "sentinel_token_estimation_fallback_total",
        "model" => model.to_string()
    )
    .increment(1);
}

// =============================================================================
// Tier Routing Metrics
// =============================================================================

/// Record a tier request
pub fn record_tier_request(tier: &str) {
    metrics::counter!(
        "sentinel_tier_requests_total",
        "tier" => tier.to_string()
    )
    .increment(1);
}

/// Record model selection
pub fn record_model_selection(tier: &str, provider: &str, model: &str) {
    metrics::counter!(
        "sentinel_model_selections_total",
        "tier" => tier.to_string(),
        "provider" => provider.to_string(),
        "model" => model.to_string()
    )
    .increment(1);
}

/// Record provider failure
pub fn record_provider_failure(provider: &str, model: &str) {
    metrics::counter!(
        "sentinel_provider_failures_total",
        "provider" => provider.to_string(),
        "model" => model.to_string()
    )
    .increment(1);
}

/// Record model retry attempt
pub fn record_model_retry(tier: &str, failed_model: &str, retry_model: &str) {
    metrics::counter!(
        "sentinel_model_retries_total",
        "tier" => tier.to_string(),
        "failed_model" => failed_model.to_string(),
        "retry_model" => retry_model.to_string()
    )
    .increment(1);
}

/// Update provider health gauge
pub fn set_provider_health(provider: &str, model: &str, healthy: bool) {
    metrics::gauge!(
        "sentinel_provider_health",
        "provider" => provider.to_string(),
        "model" => model.to_string()
    )
    .set(if healthy { 1.0 } else { 0.0 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        // This should not panic
        init_metrics();
    }
}
