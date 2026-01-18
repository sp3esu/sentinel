//! Request logging utilities for AI provider proxying
//!
//! Provides structured logging with correlation IDs for tracing requests
//! through the system, especially useful for debugging desktop app integration.

use std::time::Instant;
use tracing::{debug, error, info, warn, Span};
use uuid::Uuid;

/// Truncate a string to at most `max_bytes` bytes, ensuring we don't split UTF-8 characters.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last valid char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

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
            "➡ OUTGOING to {provider}: {url}",
            provider = self.provider,
            url = url,
        );
    }

    /// Log response received from upstream
    pub fn log_upstream_response(&self, status: u16, content_length: Option<u64>) {
        let direction = "⬅ UPSTREAM RESPONSE";
        if status >= 400 {
            warn!(
                trace_id = %self.trace_id,
                provider = %self.provider,
                endpoint = %self.endpoint,
                status = %status,
                content_length = ?content_length,
                elapsed_ms = %self.elapsed_ms(),
                "{direction}: error status from {provider}",
                direction = direction,
                provider = self.provider,
            );
        } else {
            info!(
                trace_id = %self.trace_id,
                provider = %self.provider,
                endpoint = %self.endpoint,
                status = %status,
                content_length = ?content_length,
                elapsed_ms = %self.elapsed_ms(),
                "{direction}: success",
                direction = direction,
            );
        }
    }

    /// Log error response body from upstream provider
    pub fn log_upstream_error_body(&self, status: u16, body: &str) {
        error!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            status = %status,
            elapsed_ms = %self.elapsed_ms(),
            error_body = %body,
            "⬅ UPSTREAM ERROR from {provider}: {body}",
            provider = self.provider,
            body = truncate_utf8(body, 500),
        );
    }

    /// Log JSON parse failure with response body for debugging
    ///
    /// Use this when parsing a successful (2xx) response fails.
    /// The body is truncated to 1000 bytes to prevent log flooding.
    pub fn log_parse_failure(&self, parse_error: &str, body: &str) {
        const MAX_BODY_LEN: usize = 1000;
        let truncated = truncate_utf8(body, MAX_BODY_LEN);

        error!(
            trace_id = %self.trace_id,
            provider = %self.provider,
            endpoint = %self.endpoint,
            elapsed_ms = %self.elapsed_ms(),
            error = %parse_error,
            body_len = body.len(),
            body = %truncated,
            "Failed to parse response body"
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

    #[test]
    fn test_truncate_utf8_short_string() {
        // String shorter than limit should be returned unchanged
        let s = "hello";
        assert_eq!(truncate_utf8(s, 10), "hello");
    }

    #[test]
    fn test_truncate_utf8_exact_length() {
        // String exactly at limit should be returned unchanged
        let s = "hello";
        assert_eq!(truncate_utf8(s, 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_ascii() {
        // ASCII string should truncate at exact byte position
        let s = "hello world";
        assert_eq!(truncate_utf8(s, 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_emoji_boundary() {
        // Emoji "😀" is 4 bytes. Truncating at byte 2 should give empty string
        // or truncating at byte 5 of "a😀" should give just "a"
        let s = "a😀b";  // 'a' (1 byte) + '😀' (4 bytes) + 'b' (1 byte) = 6 bytes
        assert_eq!(truncate_utf8(s, 1), "a");
        assert_eq!(truncate_utf8(s, 2), "a");  // Can't include partial emoji
        assert_eq!(truncate_utf8(s, 3), "a");
        assert_eq!(truncate_utf8(s, 4), "a");
        assert_eq!(truncate_utf8(s, 5), "a😀");  // Now emoji fits
        assert_eq!(truncate_utf8(s, 6), "a😀b");
    }

    #[test]
    fn test_truncate_utf8_chinese_characters() {
        // Chinese character "中" is 3 bytes
        let s = "中文测试";  // Each char is 3 bytes = 12 bytes total
        assert_eq!(truncate_utf8(s, 3), "中");
        assert_eq!(truncate_utf8(s, 4), "中");  // Can't include partial char
        assert_eq!(truncate_utf8(s, 5), "中");
        assert_eq!(truncate_utf8(s, 6), "中文");
    }

    #[test]
    fn test_truncate_utf8_mixed_content() {
        // Mix of ASCII and multi-byte chars
        let s = "Hello, 世界!";  // "Hello, " (7 bytes) + "世" (3) + "界" (3) + "!" (1) = 14 bytes
        assert_eq!(truncate_utf8(s, 7), "Hello, ");
        assert_eq!(truncate_utf8(s, 8), "Hello, ");  // Can't fit partial 世
        assert_eq!(truncate_utf8(s, 10), "Hello, 世");
    }

    #[test]
    fn test_truncate_utf8_empty_string() {
        assert_eq!(truncate_utf8("", 10), "");
        assert_eq!(truncate_utf8("", 0), "");
    }

    #[test]
    fn test_truncate_utf8_zero_limit() {
        assert_eq!(truncate_utf8("hello", 0), "");
        assert_eq!(truncate_utf8("😀", 0), "");
    }
}
