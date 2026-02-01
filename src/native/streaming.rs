//! Streaming chunk normalization for the Native API
//!
//! Provides utilities for formatting normalized stream chunks as SSE (Server-Sent Events).
//! This module ensures all streaming responses emit in a consistent OpenAI-compatible format,
//! regardless of which provider generated them.

use bytes::Bytes;
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;

use super::response::{Delta, StreamChoice, StreamChunk, ToolCallDelta, Usage};
use super::types::{ToolCall, ToolCallFunction};

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

// ============================================================================
// Tool Call Accumulator
// ============================================================================

/// Internal struct for accumulating tool call data across deltas
#[derive(Debug, Default)]
struct AccumulatedToolCall {
    /// Provider ID from first delta
    id: Option<String>,
    /// Accumulated function name from deltas
    function_name: String,
    /// Accumulated argument string fragments
    arguments: String,
}

/// Accumulates tool call deltas across streaming chunks.
///
/// OpenAI sends tool calls incrementally with `index` identifying which
/// tool call in a parallel set is being updated. This accumulator tracks
/// each tool call separately and finalizes them when streaming completes.
#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    /// Index -> accumulated tool call data
    tool_calls: HashMap<u32, AccumulatedToolCall>,
}

impl ToolCallAccumulator {
    /// Create a new empty accumulator
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a streaming delta for tool calls.
    ///
    /// Call this for each ToolCallDelta received in a streaming chunk.
    pub fn accumulate(&mut self, delta: &ToolCallDelta) {
        let entry = self.tool_calls.entry(delta.index).or_default();

        if let Some(ref id) = delta.id {
            entry.id = Some(id.clone());
        }
        if let Some(ref func) = delta.function {
            if let Some(ref name) = func.name {
                entry.function_name = name.clone();
            }
            if let Some(ref args) = func.arguments {
                entry.arguments.push_str(args);
            }
        }
    }

    /// Check if any tool calls have been accumulated.
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Finalize accumulated tool calls, parsing arguments as JSON.
    ///
    /// Returns ToolCalls with PROVIDER IDs (not Sentinel IDs).
    /// Caller must use ToolCallIdMapping to generate Sentinel IDs.
    ///
    /// Returns error if any arguments are not valid JSON.
    pub fn finalize(self) -> Result<Vec<(String, ToolCall)>, StreamError> {
        let mut result = Vec::with_capacity(self.tool_calls.len());

        // Sort by index to maintain order
        let mut entries: Vec<_> = self.tool_calls.into_iter().collect();
        entries.sort_by_key(|(idx, _)| *idx);

        for (index, acc) in entries {
            let provider_id = acc.id.ok_or_else(|| {
                StreamError::ParseError(format!("Tool call at index {} missing ID", index))
            })?;

            // Parse arguments as JSON (per CONTEXT.md - fail on malformed)
            let arguments: serde_json::Value =
                serde_json::from_str(&acc.arguments).map_err(|e| {
                    StreamError::ParseError(format!(
                        "Malformed tool arguments at index {}: {}",
                        index, e
                    ))
                })?;

            let tool_call = ToolCall {
                id: provider_id.clone(), // Temporary - caller replaces with Sentinel ID
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: acc.function_name,
                    arguments,
                },
            };

            result.push((provider_id, tool_call));
        }

        Ok(result)
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
                    tool_calls: None,
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
            tool_calls: None,
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

    // =============================================================================
    // ToolCallAccumulator Tests
    // =============================================================================

    use super::ToolCallAccumulator;
    use crate::native::response::ToolCallFunctionDelta;

    #[test]
    fn test_tool_call_accumulator_single_call() {
        let mut acc = ToolCallAccumulator::new();

        // First delta with id and function name
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: Some("call_abc123".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("get_weather".to_string()),
                arguments: Some("{\"loc".to_string()),
            }),
        });

        // Subsequent deltas with argument fragments
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: None,
            call_type: None,
            function: Some(ToolCallFunctionDelta {
                name: None,
                arguments: Some("ation\":".to_string()),
            }),
        });

        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: None,
            call_type: None,
            function: Some(ToolCallFunctionDelta {
                name: None,
                arguments: Some("\"Boston\"}".to_string()),
            }),
        });

        assert!(acc.has_tool_calls());

        let result = acc.finalize().unwrap();
        assert_eq!(result.len(), 1);

        let (provider_id, tool_call) = &result[0];
        assert_eq!(provider_id, "call_abc123");
        assert_eq!(tool_call.function.name, "get_weather");
        assert_eq!(
            tool_call.function.arguments,
            serde_json::json!({"location": "Boston"})
        );
    }

    #[test]
    fn test_tool_call_accumulator_multiple_parallel_calls() {
        let mut acc = ToolCallAccumulator::new();

        // First tool call
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: Some("call_weather".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("get_weather".to_string()),
                arguments: Some("{\"city\":\"NYC\"}".to_string()),
            }),
        });

        // Second tool call (parallel, different index)
        acc.accumulate(&ToolCallDelta {
            index: 1,
            id: Some("call_time".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("get_time".to_string()),
                arguments: Some("{\"tz\":\"EST\"}".to_string()),
            }),
        });

        assert!(acc.has_tool_calls());

        let result = acc.finalize().unwrap();
        assert_eq!(result.len(), 2);

        // Should be sorted by index
        assert_eq!(result[0].0, "call_weather");
        assert_eq!(result[0].1.function.name, "get_weather");
        assert_eq!(result[1].0, "call_time");
        assert_eq!(result[1].1.function.name, "get_time");
    }

    #[test]
    fn test_tool_call_accumulator_finalize_parses_json() {
        let mut acc = ToolCallAccumulator::new();

        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: Some("call_test".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("search".to_string()),
                arguments: Some("{\"query\":\"rust\",\"limit\":10}".to_string()),
            }),
        });

        let result = acc.finalize().unwrap();
        let args = &result[0].1.function.arguments;

        assert!(args.is_object());
        assert_eq!(args["query"], "rust");
        assert_eq!(args["limit"], 10);
    }

    #[test]
    fn test_tool_call_accumulator_malformed_arguments() {
        let mut acc = ToolCallAccumulator::new();

        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: Some("call_bad".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("some_func".to_string()),
                arguments: Some("{invalid json".to_string()),
            }),
        });

        let result = acc.finalize();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, StreamError::ParseError(msg) if msg.contains("index 0")));
    }

    #[test]
    fn test_tool_call_accumulator_missing_id() {
        let mut acc = ToolCallAccumulator::new();

        // Delta without ID
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: None,
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("test".to_string()),
                arguments: Some("{}".to_string()),
            }),
        });

        let result = acc.finalize();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, StreamError::ParseError(msg) if msg.contains("missing ID")));
    }

    #[test]
    fn test_tool_call_accumulator_empty() {
        let acc = ToolCallAccumulator::new();
        assert!(!acc.has_tool_calls());

        let result = acc.finalize().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_tool_call_accumulator_interleaved_deltas() {
        let mut acc = ToolCallAccumulator::new();

        // Deltas for different indices arriving interleaved
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: Some("call_a".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("func_a".to_string()),
                arguments: Some("{\"a\":".to_string()),
            }),
        });

        acc.accumulate(&ToolCallDelta {
            index: 1,
            id: Some("call_b".to_string()),
            call_type: Some("function".to_string()),
            function: Some(ToolCallFunctionDelta {
                name: Some("func_b".to_string()),
                arguments: Some("{\"b\":".to_string()),
            }),
        });

        // More arguments for index 0
        acc.accumulate(&ToolCallDelta {
            index: 0,
            id: None,
            call_type: None,
            function: Some(ToolCallFunctionDelta {
                name: None,
                arguments: Some("1}".to_string()),
            }),
        });

        // More arguments for index 1
        acc.accumulate(&ToolCallDelta {
            index: 1,
            id: None,
            call_type: None,
            function: Some(ToolCallFunctionDelta {
                name: None,
                arguments: Some("2}".to_string()),
            }),
        });

        let result = acc.finalize().unwrap();
        assert_eq!(result.len(), 2);

        assert_eq!(result[0].1.function.arguments, serde_json::json!({"a": 1}));
        assert_eq!(result[1].1.function.arguments, serde_json::json!({"b": 2}));
    }
}
