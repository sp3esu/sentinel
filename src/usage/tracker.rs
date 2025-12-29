//! Usage tracker implementation
//!
//! Tracks token usage and reports to Zion API.
//! Uses the unified increment endpoint to send all metrics in one request.

use std::sync::Arc;

use crate::{error::AppResult, zion::ZionClient};

/// Limit names for AI usage tracking
pub mod limits {
    /// Unified AI usage limit name (contains input tokens, output tokens, and requests)
    pub const AI_USAGE: &str = "ai_usage";
}

/// Usage data for a single request
#[derive(Debug, Clone, Default)]
pub struct UsageData {
    /// Number of input tokens (prompt tokens)
    pub input_tokens: u64,
    /// Number of output tokens (completion tokens)
    pub output_tokens: u64,
    /// Whether to count as a request (usually 1)
    pub count_request: bool,
}

impl UsageData {
    /// Create new usage data
    pub fn new(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
            count_request: true,
        }
    }

    /// Create usage data without counting as a request
    /// Useful for streaming where we only want to count tokens
    pub fn tokens_only(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
            count_request: false,
        }
    }

    /// Total tokens used
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Check if there's any usage to record
    pub fn has_usage(&self) -> bool {
        self.input_tokens > 0 || self.output_tokens > 0 || self.count_request
    }
}

/// Usage tracker for AI requests
///
/// Provides methods to track and report token usage to the Zion API.
/// Supports both individual increments and batch operations for efficiency.
pub struct UsageTracker {
    zion_client: Arc<ZionClient>,
}

impl UsageTracker {
    /// Create a new usage tracker
    pub fn new(zion_client: Arc<ZionClient>) -> Self {
        Self { zion_client }
    }

    /// Record all usage at once using a single API call
    ///
    /// Uses the unified increment endpoint to send all metrics in one request.
    pub async fn record_usage(
        &self,
        external_id: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> AppResult<()> {
        self.record_usage_data(external_id, &UsageData::new(input_tokens, output_tokens))
            .await
    }

    /// Record usage from UsageData struct
    ///
    /// Makes a single API call with all usage values.
    pub async fn record_usage_data(&self, external_id: &str, usage: &UsageData) -> AppResult<()> {
        if !usage.has_usage() {
            return Ok(());
        }

        let input_tokens = usage.input_tokens as i64;
        let output_tokens = usage.output_tokens as i64;
        let requests = if usage.count_request { 1 } else { 0 };

        self.zion_client
            .increment_usage(external_id, input_tokens, output_tokens, requests, None, None)
            .await?;

        tracing::info!(
            external_id = %external_id,
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            total_tokens = usage.total_tokens(),
            counted_request = usage.count_request,
            "Recorded usage"
        );

        Ok(())
    }

    /// Record usage for streaming responses
    ///
    /// For streaming, we typically:
    /// - Count input tokens once at the start
    /// - Accumulate output tokens as they come in
    /// - Count the request once
    ///
    /// This method is called at the end of a stream with the final counts.
    pub async fn record_streaming_usage(
        &self,
        external_id: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> AppResult<()> {
        // For streaming, we record all at once when the stream completes
        self.record_usage(external_id, input_tokens, output_tokens)
            .await
    }

    /// Record usage with explicit request counting control
    ///
    /// Useful when you want to record tokens without counting a request,
    /// for example when combining multiple API calls into one logical request.
    pub async fn record_tokens_only(
        &self,
        external_id: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> AppResult<()> {
        self.record_usage_data(
            external_id,
            &UsageData::tokens_only(input_tokens, output_tokens),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // UsageData Creation Tests
    // ===========================================

    #[test]
    fn test_usage_data_new() {
        let usage = UsageData::new(100, 50);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert!(usage.count_request);
        assert_eq!(usage.total_tokens(), 150);
        assert!(usage.has_usage());
    }

    #[test]
    fn test_usage_data_tokens_only() {
        let usage = UsageData::tokens_only(100, 50);
        assert!(!usage.count_request);
        assert!(usage.has_usage());
    }

    #[test]
    fn test_usage_data_empty() {
        let usage = UsageData::default();
        assert!(!usage.has_usage());

        let usage_with_request = UsageData {
            input_tokens: 0,
            output_tokens: 0,
            count_request: true,
        };
        assert!(usage_with_request.has_usage());
    }

    #[test]
    fn test_usage_data_new_zero_tokens() {
        let usage = UsageData::new(0, 0);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(usage.count_request);
        assert_eq!(usage.total_tokens(), 0);
        // Still has usage because count_request is true
        assert!(usage.has_usage());
    }

    #[test]
    fn test_usage_data_tokens_only_zero() {
        let usage = UsageData::tokens_only(0, 0);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(!usage.count_request);
        // No usage when all values are zero
        assert!(!usage.has_usage());
    }

    #[test]
    fn test_usage_data_large_values() {
        let usage = UsageData::new(u64::MAX, u64::MAX);
        assert_eq!(usage.input_tokens, u64::MAX);
        assert_eq!(usage.output_tokens, u64::MAX);
        // Note: total_tokens will overflow, but struct accepts large values
    }

    #[test]
    fn test_usage_data_input_only() {
        let usage = UsageData::new(1000, 0);
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens(), 1000);
        assert!(usage.has_usage());
    }

    #[test]
    fn test_usage_data_output_only() {
        let usage = UsageData::new(0, 500);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 500);
        assert_eq!(usage.total_tokens(), 500);
        assert!(usage.has_usage());
    }

    // ===========================================
    // UsageData Total Tokens Tests
    // ===========================================

    #[test]
    fn test_total_tokens_calculation() {
        let test_cases = vec![
            (0, 0, 0),
            (100, 0, 100),
            (0, 100, 100),
            (100, 100, 200),
            (1000, 500, 1500),
            (999, 1, 1000),
        ];

        for (input, output, expected_total) in test_cases {
            let usage = UsageData::new(input, output);
            assert_eq!(
                usage.total_tokens(),
                expected_total,
                "Failed for input={}, output={}",
                input,
                output
            );
        }
    }

    // ===========================================
    // UsageData has_usage Tests
    // ===========================================

    #[test]
    fn test_has_usage_scenarios() {
        // Only input tokens
        let usage = UsageData {
            input_tokens: 100,
            output_tokens: 0,
            count_request: false,
        };
        assert!(usage.has_usage());

        // Only output tokens
        let usage = UsageData {
            input_tokens: 0,
            output_tokens: 100,
            count_request: false,
        };
        assert!(usage.has_usage());

        // Only count_request
        let usage = UsageData {
            input_tokens: 0,
            output_tokens: 0,
            count_request: true,
        };
        assert!(usage.has_usage());

        // Nothing
        let usage = UsageData {
            input_tokens: 0,
            output_tokens: 0,
            count_request: false,
        };
        assert!(!usage.has_usage());

        // Everything
        let usage = UsageData {
            input_tokens: 100,
            output_tokens: 50,
            count_request: true,
        };
        assert!(usage.has_usage());
    }

    // ===========================================
    // UsageData Default Tests
    // ===========================================

    #[test]
    fn test_usage_data_default() {
        let usage = UsageData::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(!usage.count_request);
        assert_eq!(usage.total_tokens(), 0);
        assert!(!usage.has_usage());
    }

    // ===========================================
    // UsageData Clone Tests
    // ===========================================

    #[test]
    fn test_usage_data_clone() {
        let usage = UsageData::new(100, 50);
        let cloned = usage.clone();

        assert_eq!(usage.input_tokens, cloned.input_tokens);
        assert_eq!(usage.output_tokens, cloned.output_tokens);
        assert_eq!(usage.count_request, cloned.count_request);
    }

    // ===========================================
    // UsageData Debug Tests
    // ===========================================

    #[test]
    fn test_usage_data_debug() {
        let usage = UsageData::new(100, 50);
        let debug_str = format!("{:?}", usage);
        assert!(debug_str.contains("UsageData"));
        assert!(debug_str.contains("100"));
        assert!(debug_str.contains("50"));
    }

    // ===========================================
    // Limit Constants Tests
    // ===========================================

    #[test]
    fn test_limit_constant_ai_usage() {
        assert_eq!(limits::AI_USAGE, "ai_usage");
    }

    #[test]
    fn test_limit_constant_format() {
        // Limit name should follow snake_case pattern
        let limit = limits::AI_USAGE;
        assert!(limit.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        assert!(!limit.is_empty());
        assert!(!limit.starts_with('_'));
        assert!(!limit.ends_with('_'));
    }

    #[test]
    fn test_limit_constant_ai_prefix() {
        // AI usage limit should start with "ai_"
        assert!(limits::AI_USAGE.starts_with("ai_"));
    }

    // ===========================================
    // UsageData Typical Usage Patterns
    // ===========================================

    #[test]
    fn test_typical_chat_completion_usage() {
        // Typical chat completion: input prompt + output response
        let usage = UsageData::new(500, 200);
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.output_tokens, 200);
        assert!(usage.count_request);
        assert_eq!(usage.total_tokens(), 700);
    }

    #[test]
    fn test_streaming_response_final_usage() {
        // For streaming, tokens are counted at the end
        let usage = UsageData::new(1000, 2000);
        assert_eq!(usage.total_tokens(), 3000);
        assert!(usage.count_request);
    }

    #[test]
    fn test_embedding_request_usage() {
        // Embeddings typically have only input tokens
        let usage = UsageData::new(500, 0);
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.output_tokens, 0);
        assert!(usage.count_request);
        assert!(usage.has_usage());
    }

    #[test]
    fn test_incremental_token_tracking() {
        // When tracking tokens incrementally without counting request
        let usage = UsageData::tokens_only(100, 0);
        assert!(!usage.count_request);
        assert!(usage.has_usage());

        let usage2 = UsageData::tokens_only(0, 50);
        assert!(!usage2.count_request);
        assert!(usage2.has_usage());
    }

    // ===========================================
    // Edge Cases
    // ===========================================

    #[test]
    fn test_usage_data_max_u64_tokens() {
        let usage = UsageData {
            input_tokens: u64::MAX,
            output_tokens: 0,
            count_request: false,
        };
        assert_eq!(usage.input_tokens, u64::MAX);
        assert!(usage.has_usage());
    }
}
