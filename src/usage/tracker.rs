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
    fn test_batch_increment_item() {
        let item = BatchIncrementItem::new("ai_input_tokens", 100);
        assert_eq!(item.limit_name, "ai_input_tokens");
        assert_eq!(item.amount, 100);
    }

    #[test]
    fn test_limit_constants() {
        assert_eq!(limits::AI_INPUT_TOKENS, "ai_input_tokens");
        assert_eq!(limits::AI_OUTPUT_TOKENS, "ai_output_tokens");
        assert_eq!(limits::AI_REQUESTS, "ai_requests");
    }
}
