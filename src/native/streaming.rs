//! Streaming chunk normalization for the Native API
//!
//! Provides utilities for formatting normalized stream chunks as SSE (Server-Sent Events).
//! This module ensures all streaming responses emit in a consistent OpenAI-compatible format,
//! regardless of which provider generated them.

use bytes::Bytes;
use serde::Serialize;
use thiserror::Error;

use super::response::{Delta, StreamChoice, StreamChunk, Usage};

/// Metadata cached across streaming chunks for consistent response generation.
///
/// When processing a stream, the first chunk typically contains metadata (id, model, created)
/// that should persist across all subsequent chunks. This struct caches that information.
#[derive(Debug, Clone)]
pub struct StreamMetadata {
    /// Unique identifier for this completion
    pub id: String,
    /// Model used for completion
    pub model: String,
    /// Unix timestamp of creation
    pub created: u64,
}

/// State accumulated during stream processing.
///
/// Tracks metadata from the first chunk and accumulates content for token counting fallback
/// (when the provider doesn't return usage statistics).
#[derive(Debug, Default)]
pub struct StreamState {
    /// Cached metadata from the first chunk
    metadata: Option<StreamMetadata>,
    /// Accumulated content for token counting fallback
    accumulated_content: String,
}

impl StreamState {
    /// Create a new empty stream state.
    pub fn new() -> Self {
        Self {
            metadata: None,
            accumulated_content: String::new(),
        }
    }

    /// Set the metadata (typically from the first chunk).
    pub fn set_metadata(&mut self, meta: StreamMetadata) {
        self.metadata = Some(meta);
    }

    /// Get the cached metadata, if available.
    pub fn metadata(&self) -> Option<&StreamMetadata> {
        self.metadata.as_ref()
    }

    /// Append content to the accumulator.
    pub fn append_content(&mut self, content: &str) {
        self.accumulated_content.push_str(content);
    }

    /// Get the accumulated content.
    pub fn get_content(&self) -> &str {
        &self.accumulated_content
    }
}

/// Format a stream chunk as an SSE data event.
///
/// Serializes the chunk to JSON and wraps it in SSE format: `data: {json}\n\n`
///
/// # Arguments
/// * `chunk` - The stream chunk to format
///
/// # Returns
/// Bytes containing the SSE-formatted event
pub fn format_sse_chunk(chunk: &StreamChunk) -> Bytes {
    let json = serde_json::to_string(chunk).expect("StreamChunk should always serialize");
    Bytes::from(format!("data: {}\n\n", json))
}

/// Format the SSE done marker.
///
/// Returns the standard OpenAI stream termination marker: `data: [DONE]\n\n`
pub fn format_sse_done() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}

/// Create a stream chunk with consistent metadata.
///
/// Factory function to create a StreamChunk with metadata from the cached state,
/// ensuring all chunks in a stream have consistent id, model, and created fields.
///
/// # Arguments
/// * `metadata` - Cached metadata from the first chunk
/// * `delta` - The delta content for this chunk
/// * `finish_reason` - Optional finish reason (only in final content chunk)
/// * `usage` - Optional usage statistics (only in final chunk when requested)
///
/// # Returns
/// A StreamChunk with consistent metadata
pub fn create_chunk_with_metadata(
    metadata: &StreamMetadata,
    delta: Delta,
    finish_reason: Option<String>,
    usage: Option<Usage>,
) -> StreamChunk {
    StreamChunk {
        id: metadata.id.clone(),
        object: "chat.completion.chunk".to_string(),
        created: metadata.created,
        model: metadata.model.clone(),
        choices: vec![StreamChoice {
            index: 0,
            delta,
            finish_reason,
        }],
        usage,
    }
}

/// SSE error event structure for stream errors.
#[derive(Debug, Serialize)]
struct SseErrorEvent {
    error: SseErrorDetails,
}

#[derive(Debug, Serialize)]
struct SseErrorDetails {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

/// Format an error as an SSE error event.
///
/// Creates a structured error JSON and wraps it in SSE format.
/// This allows clients to receive error information before the stream closes.
///
/// # Arguments
/// * `message` - Error message
/// * `code` - Optional error code
///
/// # Returns
/// Bytes containing the SSE-formatted error event
pub fn format_error_event(message: &str, code: Option<&str>) -> Bytes {
    let event = SseErrorEvent {
        error: SseErrorDetails {
            message: message.to_string(),
            error_type: "stream_error".to_string(),
            code: code.map(|c| c.to_string()),
        },
    };
    let json = serde_json::to_string(&event).expect("SseErrorEvent should always serialize");
    Bytes::from(format!("data: {}\n\n", json))
}

// ============================================================================
// Normalized Chunk Types
// ============================================================================

/// A normalized stream chunk that abstracts over different chunk types.
///
/// This enum represents all possible events in a normalized stream:
/// - Content deltas (the actual generated text)
/// - Stream completion (with optional usage statistics)
/// - Keep-alive comments (for connection maintenance)
#[derive(Debug, Clone)]
pub enum NormalizedChunk {
    /// Content delta (text being generated)
    Delta(StreamChunk),
    /// Stream complete with optional usage statistics
    Done(Option<Usage>),
    /// Keep-alive comment (no data, just connection maintenance)
    KeepAlive,
}

/// Format a normalized chunk as SSE bytes.
///
/// Converts any normalized chunk type to the appropriate SSE format:
/// - `Delta` -> `data: {json}\n\n`
/// - `Done` -> `data: [DONE]\n\n`
/// - `KeepAlive` -> `: keep-alive\n\n` (SSE comment)
///
/// # Arguments
/// * `chunk` - The normalized chunk to format
///
/// # Returns
/// Bytes containing the SSE-formatted event
pub fn format_normalized(chunk: &NormalizedChunk) -> Bytes {
    match chunk {
        NormalizedChunk::Delta(stream_chunk) => format_sse_chunk(stream_chunk),
        NormalizedChunk::Done(_) => format_sse_done(),
        NormalizedChunk::KeepAlive => Bytes::from_static(b": keep-alive\n\n"),
    }
}

// ============================================================================
// Stream Errors
// ============================================================================

/// Errors that can occur during stream processing.
#[derive(Debug, Error)]
pub enum StreamError {
    /// Failed to parse a chunk from the provider
    #[error("Failed to parse provider chunk: {0}")]
    ParseError(String),

    /// Stream connection closed unexpectedly
    #[error("Stream connection closed unexpectedly")]
    ConnectionClosed,

    /// Provider returned an error in the stream
    #[error("Provider error: {message}")]
    ProviderError {
        /// Error message from the provider
        message: String,
        /// Optional error code
        code: Option<String>,
    },
}

/// Format a stream error as an SSE error event.
///
/// Converts a StreamError to a structured JSON error event that clients can parse.
/// This allows error information to be transmitted before the stream closes.
///
/// # Arguments
/// * `error` - The stream error to format
///
/// # Returns
/// Bytes containing the SSE-formatted error event
pub fn format_error_chunk(error: &StreamError) -> Bytes {
    match error {
        StreamError::ParseError(msg) => format_error_event(msg, Some("parse_error")),
        StreamError::ConnectionClosed => {
            format_error_event("Stream connection closed unexpectedly", Some("connection_closed"))
        }
        StreamError::ProviderError { message, code } => {
            format_error_event(message, code.as_deref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::types::Role;

    /// Helper to create a test StreamChunk with given content
    fn make_test_chunk(content: &str) -> StreamChunk {
        StreamChunk {
            id: "chatcmpl-test123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "gpt-4".to_string(),
            choices: vec![StreamChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some(content.to_string()),
                },
                finish_reason: None,
            }],
            usage: None,
        }
    }

    #[test]
    fn test_format_sse_chunk() {
        let chunk = make_test_chunk("Hello");
        let bytes = format_sse_chunk(&chunk);
        let output = std::str::from_utf8(&bytes).unwrap();

        // Verify SSE format: starts with "data: " and ends with "\n\n"
        assert!(output.starts_with("data: "), "Should start with 'data: '");
        assert!(output.ends_with("\n\n"), "Should end with double newline");

        // Extract and parse the JSON
        let json_str = output.trim_start_matches("data: ").trim_end();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // Verify expected fields
        assert_eq!(parsed["id"], "chatcmpl-test123");
        assert_eq!(parsed["object"], "chat.completion.chunk");
        assert_eq!(parsed["created"], 1234567890);
        assert_eq!(parsed["model"], "gpt-4");
        assert_eq!(parsed["choices"][0]["delta"]["content"], "Hello");
    }

    #[test]
    fn test_format_sse_done() {
        let bytes = format_sse_done();
        let output = std::str::from_utf8(&bytes).unwrap();

        // Must be exactly "data: [DONE]\n\n"
        assert_eq!(output, "data: [DONE]\n\n");
    }

    #[test]
    fn test_stream_state_accumulation() {
        let mut state = StreamState::new();

        // Initially empty
        assert_eq!(state.get_content(), "");
        assert!(state.metadata().is_none());

        // Append content
        state.append_content("Hello");
        assert_eq!(state.get_content(), "Hello");

        state.append_content(" world");
        assert_eq!(state.get_content(), "Hello world");

        // Set and get metadata
        let meta = StreamMetadata {
            id: "test-id".to_string(),
            model: "gpt-4".to_string(),
            created: 12345,
        };
        state.set_metadata(meta);

        let stored = state.metadata().unwrap();
        assert_eq!(stored.id, "test-id");
        assert_eq!(stored.model, "gpt-4");
        assert_eq!(stored.created, 12345);
    }

    #[test]
    fn test_create_chunk_with_metadata() {
        let metadata = StreamMetadata {
            id: "chatcmpl-abc123".to_string(),
            model: "gpt-4-turbo".to_string(),
            created: 1700000000,
        };

        let delta = Delta {
            role: Some(Role::Assistant),
            content: Some("Test content".to_string()),
        };

        let chunk = create_chunk_with_metadata(&metadata, delta, None, None);

        // Verify metadata propagation
        assert_eq!(chunk.id, "chatcmpl-abc123");
        assert_eq!(chunk.model, "gpt-4-turbo");
        assert_eq!(chunk.created, 1700000000);
        assert_eq!(chunk.object, "chat.completion.chunk");

        // Verify choice structure
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].index, 0);
        assert_eq!(chunk.choices[0].delta.role, Some(Role::Assistant));
        assert_eq!(
            chunk.choices[0].delta.content,
            Some("Test content".to_string())
        );
        assert!(chunk.choices[0].finish_reason.is_none());
        assert!(chunk.usage.is_none());
    }

    #[test]
    fn test_create_chunk_with_finish_reason_and_usage() {
        let metadata = StreamMetadata {
            id: "chatcmpl-xyz".to_string(),
            model: "gpt-4".to_string(),
            created: 1234567890,
        };

        let delta = Delta::default();
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };

        let chunk = create_chunk_with_metadata(
            &metadata,
            delta,
            Some("stop".to_string()),
            Some(usage),
        );

        assert_eq!(chunk.choices[0].finish_reason, Some("stop".to_string()));
        assert!(chunk.usage.is_some());
        let u = chunk.usage.unwrap();
        assert_eq!(u.prompt_tokens, 10);
        assert_eq!(u.completion_tokens, 20);
        assert_eq!(u.total_tokens, 30);
    }

    #[test]
    fn test_format_normalized_variants() {
        // Test Delta variant
        let chunk = make_test_chunk("Hi");
        let delta_bytes = format_normalized(&NormalizedChunk::Delta(chunk.clone()));
        let delta_output = std::str::from_utf8(&delta_bytes).unwrap();
        assert!(delta_output.starts_with("data: "));
        assert!(delta_output.ends_with("\n\n"));
        assert!(delta_output.contains("\"content\":\"Hi\""));

        // Test Done variant
        let done_bytes = format_normalized(&NormalizedChunk::Done(None));
        let done_output = std::str::from_utf8(&done_bytes).unwrap();
        assert_eq!(done_output, "data: [DONE]\n\n");

        // Test Done with usage (still emits [DONE], usage is carried separately)
        let usage = Usage {
            prompt_tokens: 5,
            completion_tokens: 10,
            total_tokens: 15,
        };
        let done_with_usage = format_normalized(&NormalizedChunk::Done(Some(usage)));
        let done_usage_output = std::str::from_utf8(&done_with_usage).unwrap();
        assert_eq!(done_usage_output, "data: [DONE]\n\n");

        // Test KeepAlive variant (SSE comment)
        let keepalive_bytes = format_normalized(&NormalizedChunk::KeepAlive);
        let keepalive_output = std::str::from_utf8(&keepalive_bytes).unwrap();
        assert_eq!(keepalive_output, ": keep-alive\n\n");
    }

    #[test]
    fn test_format_error_chunk() {
        // Test ProviderError with code
        let error = StreamError::ProviderError {
            message: "Rate limit exceeded".to_string(),
            code: Some("rate_limit_error".to_string()),
        };
        let bytes = format_error_chunk(&error);
        let output = std::str::from_utf8(&bytes).unwrap();

        assert!(output.starts_with("data: "));
        assert!(output.ends_with("\n\n"));

        let json_str = output.trim_start_matches("data: ").trim_end();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        assert_eq!(parsed["error"]["message"], "Rate limit exceeded");
        assert_eq!(parsed["error"]["type"], "stream_error");
        assert_eq!(parsed["error"]["code"], "rate_limit_error");
    }

    #[test]
    fn test_format_error_chunk_without_code() {
        let error = StreamError::ProviderError {
            message: "Something went wrong".to_string(),
            code: None,
        };
        let bytes = format_error_chunk(&error);
        let output = std::str::from_utf8(&bytes).unwrap();

        let json_str = output.trim_start_matches("data: ").trim_end();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        assert_eq!(parsed["error"]["message"], "Something went wrong");
        assert_eq!(parsed["error"]["type"], "stream_error");
        assert!(parsed["error"]["code"].is_null());
    }

    #[test]
    fn test_format_error_chunk_parse_error() {
        let error = StreamError::ParseError("Invalid JSON at position 42".to_string());
        let bytes = format_error_chunk(&error);
        let output = std::str::from_utf8(&bytes).unwrap();

        let json_str = output.trim_start_matches("data: ").trim_end();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        assert_eq!(parsed["error"]["message"], "Invalid JSON at position 42");
        assert_eq!(parsed["error"]["code"], "parse_error");
    }

    #[test]
    fn test_format_error_chunk_connection_closed() {
        let error = StreamError::ConnectionClosed;
        let bytes = format_error_chunk(&error);
        let output = std::str::from_utf8(&bytes).unwrap();

        let json_str = output.trim_start_matches("data: ").trim_end();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        assert_eq!(
            parsed["error"]["message"],
            "Stream connection closed unexpectedly"
        );
        assert_eq!(parsed["error"]["code"], "connection_closed");
    }

    #[test]
    fn test_stream_error_display() {
        // Test Display trait implementations
        let parse_err = StreamError::ParseError("bad json".to_string());
        assert_eq!(
            parse_err.to_string(),
            "Failed to parse provider chunk: bad json"
        );

        let conn_err = StreamError::ConnectionClosed;
        assert_eq!(
            conn_err.to_string(),
            "Stream connection closed unexpectedly"
        );

        let provider_err = StreamError::ProviderError {
            message: "API error".to_string(),
            code: Some("api_error".to_string()),
        };
        assert_eq!(provider_err.to_string(), "Provider error: API error");
    }
}
