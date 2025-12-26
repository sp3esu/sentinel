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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        // This should not panic
        init_metrics();
    }
}
