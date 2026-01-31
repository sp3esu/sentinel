//! Streaming chunk normalization for the Native API
//!
//! Provides utilities for formatting normalized stream chunks as SSE (Server-Sent Events).
//! This module ensures all streaming responses emit in a consistent OpenAI-compatible format,
//! regardless of which provider generated them.

use bytes::Bytes;
use serde::Serialize;

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
