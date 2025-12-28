//! Request logging utilities for AI provider proxying
//!
//! Provides structured logging with correlation IDs for tracing requests
//! through the system, especially useful for debugging desktop app integration.

use std::time::Instant;
use tracing::{debug, error, info, warn, Span};
use uuid::Uuid;

/// Context for tracking a request through the system
///
/// Provides correlation IDs and timing information for debugging
/// and observability.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Unique identifier for this request (for log correlation)
    pub trace_id: String,
    /// When the request started
    pub start_time: Instant,
    /// AI provider handling this request
    pub provider: String,
    /// API endpoint being called
    pub endpoint: String,
    /// Model being used (if applicable)
    pub model: Option<String>,
    /// Whether this is a streaming request
    pub streaming: bool,
    /// User's external ID (for correlation with Zion)
    pub external_id: Option<String>,
}

impl RequestContext {
    /// Create a new request context
    pub fn new(provider: &str, endpoint: &str) -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string()[..8].to_string(), // Short ID for readability
            start_time: Instant::now(),
            provider: provider.to_string(),
            endpoint: endpoint.to_string(),
            model: None,
            streaming: false,
            external_id: None,
        }
    }

    /// Set the model for this request
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Mark this as a streaming request
    pub fn with_streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Set the user's external ID
    pub fn with_external_id(mut self, external_id: impl Into<String>) -> Self {
        self.external_id = Some(external_id.into());
        self
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u128 {
        self.start_time.elapsed().as_millis()
    }

    /// Log request initiation
    pub fn log_request_start(&self) {
        info!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            model = ?self.model,
            streaming = %self.streaming,
            external_id = ?self.external_id,
            "Request started"
        );
    }

    /// Log headers being sent (debug level)
    pub fn log_headers_prepared(&self, header_count: usize) {
        debug!(
            trace_id = %self.trace_id,
            header_count = %header_count,
            "Headers prepared for upstream request"
        );
    }

    /// Log request being sent to upstream
    pub fn log_upstream_request(&self, url: &str, body_size: Option<usize>) {
        debug!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            url = %url,
            body_size = ?body_size,
            elapsed_ms = %self.elapsed_ms(),
            "Sending request to upstream"
        );
    }

    /// Log response received from upstream
    pub fn log_upstream_response(&self, status: u16, content_length: Option<u64>) {
        info!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            status = %status,
            content_length = ?content_length,
            elapsed_ms = %self.elapsed_ms(),
            "Response received from upstream"
        );
    }

    /// Log successful request completion
    pub fn log_request_complete(&self, tokens: Option<u64>) {
        info!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            model = ?self.model,
            streaming = %self.streaming,
            tokens = ?tokens,
            elapsed_ms = %self.elapsed_ms(),
            external_id = ?self.external_id,
            "Request completed successfully"
        );
    }

    /// Log stream started (for streaming requests)
    pub fn log_stream_started(&self) {
        info!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            elapsed_ms = %self.elapsed_ms(),
            "Streaming response started"
        );
    }

    /// Log stream ended
    pub fn log_stream_ended(&self, chunks: Option<usize>) {
        info!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            chunks = ?chunks,
            elapsed_ms = %self.elapsed_ms(),
            "Streaming response ended"
        );
    }

    /// Log a warning condition
    pub fn log_warning(&self, message: &str) {
        warn!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            elapsed_ms = %self.elapsed_ms(),
            message = %message,
            "Warning during request"
        );
    }

    /// Log request failure
    pub fn log_error(&self, error: &str) {
        error!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            model = ?self.model,
            streaming = %self.streaming,
            elapsed_ms = %self.elapsed_ms(),
            external_id = ?self.external_id,
            error = %error,
            "Request failed"
        );
    }

    /// Log connection error (specific for debugging connectivity issues)
    pub fn log_connection_error(&self, error: &str, url: &str) {
        error!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            url = %url,
            elapsed_ms = %self.elapsed_ms(),
            error = %error,
            "Connection to upstream failed"
        );
    }

    /// Log timeout
    pub fn log_timeout(&self, timeout_ms: u64) {
        error!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            timeout_ms = %timeout_ms,
            elapsed_ms = %self.elapsed_ms(),
            "Request timed out"
        );
    }

    /// Log retry attempt
    pub fn log_retry(&self, attempt: u32, reason: &str) {
        warn!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            attempt = %attempt,
            reason = %reason,
            elapsed_ms = %self.elapsed_ms(),
            "Retrying request"
        );
    }

    /// Create a tracing span for this request
    pub fn create_span(&self) -> Span {
        tracing::info_span!(
            "ai_request",
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            model = ?self.model,
            streaming = %self.streaming,
        )
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new("unknown", "unknown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_creation() {
        let ctx = RequestContext::new("openai", "/v1/chat/completions")
            .with_model("gpt-4")
            .with_streaming(true)
            .with_external_id("user-123");

        assert_eq!(ctx.provider, "openai");
        assert_eq!(ctx.endpoint, "/v1/chat/completions");
        assert_eq!(ctx.model, Some("gpt-4".to_string()));
        assert!(ctx.streaming);
        assert_eq!(ctx.external_id, Some("user-123".to_string()));
        assert_eq!(ctx.trace_id.len(), 8);
    }

    #[test]
    fn test_elapsed_time() {
        let ctx = RequestContext::new("openai", "/test");
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(ctx.elapsed_ms() >= 10);
    }
}
