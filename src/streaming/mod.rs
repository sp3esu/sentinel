//! SSE (Server-Sent Events) streaming utilities
//!
//! Provides buffering and parsing helpers for processing SSE streams
//! from AI providers like OpenAI.

/// Buffer for accumulating incomplete SSE lines across chunk boundaries.
///
/// SSE data arrives as byte chunks that may not align with line boundaries.
/// This buffer accumulates incomplete lines until a complete line (ending with \n)
/// is available for processing.
///
/// # Example
/// ```
/// use sentinel::streaming::SseLineBuffer;
///
/// let mut buffer = SseLineBuffer::new();
///
/// // First chunk contains partial line
/// let lines1 = buffer.feed(b"data: {\"content\":\"hel");
/// assert!(lines1.is_empty()); // No complete lines yet
///
/// // Second chunk completes the line
/// let lines2 = buffer.feed(b"lo\"}\n");
/// assert_eq!(lines2, vec!["data: {\"content\":\"hello\"}"]);
/// ```
#[derive(Debug, Default)]
pub struct SseLineBuffer {
    /// Accumulated incomplete line data
    incomplete: String,
}

impl SseLineBuffer {
    /// Create a new empty buffer
    pub fn new() -> Self {
        Self {
            incomplete: String::new(),
        }
    }

    /// Feed bytes into the buffer and return any complete lines.
    ///
    /// Complete lines are those ending with `\n`. The newline character
    /// is stripped from returned lines. Incomplete trailing data is
    /// retained in the buffer for the next call.
    ///
    /// # Arguments
    /// * `bytes` - Raw bytes from the SSE stream chunk
    ///
    /// # Returns
    /// Vector of complete lines (without trailing newlines)
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        // Convert bytes to string, replacing invalid UTF-8 with replacement char
        let text = String::from_utf8_lossy(bytes);

        // Append to any incomplete data from previous chunk
        self.incomplete.push_str(&text);

        // Split into lines and collect complete ones
        let mut complete_lines = Vec::new();

        // Find complete lines (those followed by \n)
        while let Some(newline_pos) = self.incomplete.find('\n') {
            let line = self.incomplete[..newline_pos].to_string();
            self.incomplete = self.incomplete[newline_pos + 1..].to_string();

            // Skip empty lines (SSE uses double newlines as separators)
            if !line.is_empty() {
                complete_lines.push(line);
            }
        }

        complete_lines
    }

    /// Check if there's any incomplete data remaining in the buffer.
    ///
    /// Useful for detecting truncated streams at end of response.
    pub fn has_incomplete(&self) -> bool {
        !self.incomplete.is_empty()
    }

    /// Get any remaining incomplete data.
    ///
    /// Call this at end of stream to check for truncated data.
    pub fn remaining(&self) -> &str {
        &self.incomplete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let mut buffer = SseLineBuffer::new();
        let lines = buffer.feed(b"");
        assert!(lines.is_empty());
        assert!(!buffer.has_incomplete());
    }

    #[test]
    fn test_single_complete_line() {
        let mut buffer = SseLineBuffer::new();
        let lines = buffer.feed(b"data: hello\n");
        assert_eq!(lines, vec!["data: hello"]);
        assert!(!buffer.has_incomplete());
    }

    #[test]
    fn test_multiple_complete_lines() {
        let mut buffer = SseLineBuffer::new();
        let lines = buffer.feed(b"data: first\ndata: second\n");
        assert_eq!(lines, vec!["data: first", "data: second"]);
        assert!(!buffer.has_incomplete());
    }

    #[test]
    fn test_incomplete_line_buffered() {
        let mut buffer = SseLineBuffer::new();
        let lines = buffer.feed(b"data: incomp");
        assert!(lines.is_empty());
        assert!(buffer.has_incomplete());
        assert_eq!(buffer.remaining(), "data: incomp");
    }

    #[test]
    fn test_split_line_across_chunks() {
        let mut buffer = SseLineBuffer::new();

        // First chunk: partial line
        let lines1 = buffer.feed(b"data: {\"content\":\"hel");
        assert!(lines1.is_empty());
        assert!(buffer.has_incomplete());

        // Second chunk: completes the line
        let lines2 = buffer.feed(b"lo\"}\n");
        assert_eq!(lines2, vec!["data: {\"content\":\"hello\"}"]);
        assert!(!buffer.has_incomplete());
    }

    #[test]
    fn test_line_split_at_newline() {
        let mut buffer = SseLineBuffer::new();

        // Chunk ends right before newline
        let lines1 = buffer.feed(b"data: test");
        assert!(lines1.is_empty());

        // Next chunk starts with newline
        let lines2 = buffer.feed(b"\ndata: next\n");
        assert_eq!(lines2, vec!["data: test", "data: next"]);
    }

    #[test]
    fn test_sse_double_newline_separator() {
        let mut buffer = SseLineBuffer::new();
        // SSE uses \n\n between events - empty lines should be skipped
        let lines = buffer.feed(b"data: first\n\ndata: second\n");
        assert_eq!(lines, vec!["data: first", "data: second"]);
    }

    #[test]
    fn test_realistic_openai_stream() {
        let mut buffer = SseLineBuffer::new();

        // Simulate realistic OpenAI SSE chunks
        let chunk1 = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n";
        let lines1 = buffer.feed(chunk1);
        assert_eq!(
            lines1,
            vec!["data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}"]
        );

        // Split chunk
        let chunk2 = b"data: {\"choices\":[{\"delta\":{\"con";
        let lines2 = buffer.feed(chunk2);
        assert!(lines2.is_empty());

        let chunk3 = b"tent\":\" world\"}}]}\n\n";
        let lines3 = buffer.feed(chunk3);
        assert_eq!(
            lines3,
            vec!["data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}"]
        );

        // Done marker
        let chunk4 = b"data: [DONE]\n\n";
        let lines4 = buffer.feed(chunk4);
        assert_eq!(lines4, vec!["data: [DONE]"]);
    }

    #[test]
    fn test_carriage_return_handling() {
        let mut buffer = SseLineBuffer::new();
        // Some systems send \r\n - we only split on \n, \r remains in line
        let lines = buffer.feed(b"data: test\r\n");
        assert_eq!(lines, vec!["data: test\r"]);
    }

    #[test]
    fn test_invalid_utf8() {
        let mut buffer = SseLineBuffer::new();
        // Invalid UTF-8 bytes should be replaced with replacement character
        let invalid_utf8 = b"data: hello \xff world\n";
        let lines = buffer.feed(invalid_utf8);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("hello"));
        assert!(lines[0].contains("world"));
    }
}
