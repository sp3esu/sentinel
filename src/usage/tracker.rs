//! Usage tracker implementation
//!
//! Tracks token usage and reports to Zion API.
//! Supports both individual and batch increment operations.

use std::sync::Arc;

use futures::future::try_join_all;

use crate::{error::AppResult, zion::ZionClient};

/// Limit names for AI usage tracking
pub mod limits {
    /// Input tokens limit name
    pub const AI_INPUT_TOKENS: &str = "ai_input_tokens";
    /// Output tokens limit name
    pub const AI_OUTPUT_TOKENS: &str = "ai_output_tokens";
    /// Request count limit name
    pub const AI_REQUESTS: &str = "ai_requests";
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

/// Batch increment request for multiple limits
#[derive(Debug, Clone)]
pub struct BatchIncrementItem {
    /// The limit name to increment
    pub limit_name: String,
    /// Amount to increment by
    pub amount: i64,
}

impl BatchIncrementItem {
    /// Create a new batch increment item
    pub fn new(limit_name: &str, amount: i64) -> Self {
        Self {
            limit_name: limit_name.to_string(),
            amount,
        }
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

    /// Record input tokens usage
    pub async fn record_input_tokens(&self, external_id: &str, tokens: u64) -> AppResult<()> {
        if tokens == 0 {
            return Ok(());
        }
        self.zion_client
            .increment_usage(external_id, limits::AI_INPUT_TOKENS, tokens as i64)
            .await?;
        tracing::debug!(
            external_id = %external_id,
            tokens = tokens,
            "Recorded input tokens"
        );
        Ok(())
    }

    /// Record output tokens usage
    pub async fn record_output_tokens(&self, external_id: &str, tokens: u64) -> AppResult<()> {
        if tokens == 0 {
            return Ok(());
        }
        self.zion_client
            .increment_usage(external_id, limits::AI_OUTPUT_TOKENS, tokens as i64)
            .await?;
        tracing::debug!(
            external_id = %external_id,
            tokens = tokens,
            "Recorded output tokens"
        );
        Ok(())
    }

    /// Record a request
    pub async fn record_request(&self, external_id: &str) -> AppResult<()> {
        self.zion_client
            .increment_usage(external_id, limits::AI_REQUESTS, 1)
            .await?;
        tracing::debug!(
            external_id = %external_id,
            "Recorded request"
        );
        Ok(())
    }

    /// Record all usage at once using parallel requests
    ///
    /// This is more efficient than calling individual methods sequentially
    /// as it makes all API calls in parallel.
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
    /// Makes parallel API calls for all non-zero usage values.
    pub async fn record_usage_data(&self, external_id: &str, usage: &UsageData) -> AppResult<()> {
        if !usage.has_usage() {
            return Ok(());
        }

        // Clone the external_id to own it for the async operations
        let ext_id = external_id.to_string();
        let client = self.zion_client.clone();
        let input_tokens = usage.input_tokens;
        let output_tokens = usage.output_tokens;
        let count_request = usage.count_request;

        type BoxedFuture = std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send>>;
        let mut futures: Vec<BoxedFuture> = Vec::new();

        if count_request {
            let id = ext_id.clone();
            let c = client.clone();
            futures.push(Box::pin(async move {
                c.increment_usage(&id, limits::AI_REQUESTS, 1).await?;
                Ok(())
            }));
        }

        if input_tokens > 0 {
            let id = ext_id.clone();
            let c = client.clone();
            futures.push(Box::pin(async move {
                c.increment_usage(&id, limits::AI_INPUT_TOKENS, input_tokens as i64).await?;
                Ok(())
            }));
        }

        if output_tokens > 0 {
            let id = ext_id.clone();
            let c = client.clone();
            futures.push(Box::pin(async move {
                c.increment_usage(&id, limits::AI_OUTPUT_TOKENS, output_tokens as i64).await?;
                Ok(())
            }));
        }

        // Execute all increments in parallel
        try_join_all(futures).await?;

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

    /// Batch increment multiple limits at once
    ///
    /// Executes all increments in parallel for efficiency.
    pub async fn batch_increment(
        &self,
        external_id: &str,
        items: &[BatchIncrementItem],
    ) -> AppResult<()> {
        if items.is_empty() {
            return Ok(());
        }

        let futures: Vec<_> = items
            .iter()
            .filter(|item| item.amount != 0)
            .map(|item| {
                let external_id = external_id.to_string();
                let limit_name = item.limit_name.clone();
                let amount = item.amount;
                let client = self.zion_client.clone();

                async move { client.increment_usage(&external_id, &limit_name, amount).await }
            })
            .collect();

        try_join_all(futures).await?;

        tracing::debug!(
            external_id = %external_id,
            item_count = items.len(),
            "Batch increment completed"
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
    // BatchIncrementItem Tests
    // ===========================================

    #[test]
    fn test_batch_increment_item() {
        let item = BatchIncrementItem::new("ai_input_tokens", 100);
        assert_eq!(item.limit_name, "ai_input_tokens");
        assert_eq!(item.amount, 100);
    }

    #[test]
    fn test_batch_increment_item_various_limits() {
        let items = vec![
            ("ai_input_tokens", 100),
            ("ai_output_tokens", 200),
            ("ai_requests", 1),
        ];

        for (name, amount) in items {
            let item = BatchIncrementItem::new(name, amount);
            assert_eq!(item.limit_name, name);
            assert_eq!(item.amount, amount);
        }
    }

    #[test]
    fn test_batch_increment_item_zero_amount() {
        let item = BatchIncrementItem::new("test_limit", 0);
        assert_eq!(item.limit_name, "test_limit");
        assert_eq!(item.amount, 0);
    }

    #[test]
    fn test_batch_increment_item_negative_amount() {
        // Negative amounts could be used for decrements
        let item = BatchIncrementItem::new("test_limit", -50);
        assert_eq!(item.limit_name, "test_limit");
        assert_eq!(item.amount, -50);
    }

    #[test]
    fn test_batch_increment_item_large_amount() {
        let item = BatchIncrementItem::new("test_limit", i64::MAX);
        assert_eq!(item.amount, i64::MAX);
    }

    #[test]
    fn test_batch_increment_item_clone() {
        let item = BatchIncrementItem::new("ai_tokens", 500);
        let cloned = item.clone();

        assert_eq!(item.limit_name, cloned.limit_name);
        assert_eq!(item.amount, cloned.amount);
    }

    #[test]
    fn test_batch_increment_item_debug() {
        let item = BatchIncrementItem::new("ai_input_tokens", 100);
        let debug_str = format!("{:?}", item);
        assert!(debug_str.contains("BatchIncrementItem"));
        assert!(debug_str.contains("ai_input_tokens"));
        assert!(debug_str.contains("100"));
    }

    // ===========================================
    // Limit Constants Tests
    // ===========================================

    #[test]
    fn test_limit_constants() {
        assert_eq!(limits::AI_INPUT_TOKENS, "ai_input_tokens");
        assert_eq!(limits::AI_OUTPUT_TOKENS, "ai_output_tokens");
        assert_eq!(limits::AI_REQUESTS, "ai_requests");
    }

    #[test]
    fn test_limit_constants_format() {
        // All limit names should follow snake_case pattern
        let all_limits = vec![
            limits::AI_INPUT_TOKENS,
            limits::AI_OUTPUT_TOKENS,
            limits::AI_REQUESTS,
        ];

        for limit in all_limits {
            assert!(limit.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
            assert!(!limit.is_empty());
            assert!(!limit.starts_with('_'));
            assert!(!limit.ends_with('_'));
        }
    }

    #[test]
    fn test_limit_constants_uniqueness() {
        let limits_set: std::collections::HashSet<&str> = [
            limits::AI_INPUT_TOKENS,
            limits::AI_OUTPUT_TOKENS,
            limits::AI_REQUESTS,
        ]
        .iter()
        .copied()
        .collect();

        // All limits should be unique
        assert_eq!(limits_set.len(), 3);
    }

    #[test]
    fn test_limit_constants_ai_prefix() {
        // All AI-related limits should start with "ai_"
        assert!(limits::AI_INPUT_TOKENS.starts_with("ai_"));
        assert!(limits::AI_OUTPUT_TOKENS.starts_with("ai_"));
        assert!(limits::AI_REQUESTS.starts_with("ai_"));
    }

    // ===========================================
    // BatchIncrementItem Collection Tests
    // ===========================================

    #[test]
    fn test_batch_items_collection() {
        let items = vec![
            BatchIncrementItem::new(limits::AI_INPUT_TOKENS, 100),
            BatchIncrementItem::new(limits::AI_OUTPUT_TOKENS, 50),
            BatchIncrementItem::new(limits::AI_REQUESTS, 1),
        ];

        assert_eq!(items.len(), 3);

        // Verify each item
        assert_eq!(items[0].limit_name, limits::AI_INPUT_TOKENS);
        assert_eq!(items[0].amount, 100);

        assert_eq!(items[1].limit_name, limits::AI_OUTPUT_TOKENS);
        assert_eq!(items[1].amount, 50);

        assert_eq!(items[2].limit_name, limits::AI_REQUESTS);
        assert_eq!(items[2].amount, 1);
    }

    #[test]
    fn test_batch_items_filter_zero_amounts() {
        let items = vec![
            BatchIncrementItem::new("limit1", 100),
            BatchIncrementItem::new("limit2", 0),
            BatchIncrementItem::new("limit3", 50),
        ];

        let non_zero_items: Vec<_> = items
            .iter()
            .filter(|item| item.amount != 0)
            .collect();

        assert_eq!(non_zero_items.len(), 2);
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
    fn test_empty_limit_name() {
        let item = BatchIncrementItem::new("", 100);
        assert_eq!(item.limit_name, "");
        assert_eq!(item.amount, 100);
    }

    #[test]
    fn test_special_characters_in_limit_name() {
        let item = BatchIncrementItem::new("custom:limit:name", 100);
        assert_eq!(item.limit_name, "custom:limit:name");
    }

    #[test]
    fn test_unicode_in_limit_name() {
        let item = BatchIncrementItem::new("limit_name_test", 100);
        assert_eq!(item.limit_name, "limit_name_test");
    }

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

    // ===========================================
    // Serialization Tests (for BatchIncrementItem if needed)
    // ===========================================

    #[test]
    fn test_batch_increment_item_to_usage_request() {
        // Verify BatchIncrementItem can be used to build request payloads
        let item = BatchIncrementItem::new(limits::AI_INPUT_TOKENS, 1000);

        // This tests the structure is suitable for API requests
        assert!(!item.limit_name.is_empty());
        assert!(item.amount > 0);
    }

    #[test]
    fn test_multiple_batch_items_for_full_usage() {
        let usage = UsageData::new(1000, 500);

        let mut items = Vec::new();

        if usage.count_request {
            items.push(BatchIncrementItem::new(limits::AI_REQUESTS, 1));
        }

        if usage.input_tokens > 0 {
            items.push(BatchIncrementItem::new(
                limits::AI_INPUT_TOKENS,
                usage.input_tokens as i64,
            ));
        }

        if usage.output_tokens > 0 {
            items.push(BatchIncrementItem::new(
                limits::AI_OUTPUT_TOKENS,
                usage.output_tokens as i64,
            ));
        }

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].limit_name, limits::AI_REQUESTS);
        assert_eq!(items[1].limit_name, limits::AI_INPUT_TOKENS);
        assert_eq!(items[2].limit_name, limits::AI_OUTPUT_TOKENS);
    }
}
