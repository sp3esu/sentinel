//! Usage tracker implementation
//!
//! Tracks token usage and reports to Zion API.

use std::sync::Arc;

use crate::{error::AppResult, zion::ZionClient, AppState};

/// Usage tracker for AI requests
pub struct UsageTracker {
    zion_client: Arc<ZionClient>,
}

impl UsageTracker {
    /// Create a new usage tracker
    pub fn new(zion_client: Arc<ZionClient>) -> Self {
        Self { zion_client }
    }

    /// Record input tokens usage
    pub async fn record_input_tokens(
        &self,
        external_id: &str,
        tokens: u64,
    ) -> AppResult<()> {
        self.zion_client
            .increment_usage(external_id, "ai_input_tokens", tokens as i64)
            .await?;
        Ok(())
    }

    /// Record output tokens usage
    pub async fn record_output_tokens(
        &self,
        external_id: &str,
        tokens: u64,
    ) -> AppResult<()> {
        self.zion_client
            .increment_usage(external_id, "ai_output_tokens", tokens as i64)
            .await?;
        Ok(())
    }

    /// Record a request
    pub async fn record_request(&self, external_id: &str) -> AppResult<()> {
        self.zion_client
            .increment_usage(external_id, "ai_requests", 1)
            .await?;
        Ok(())
    }

    /// Record all usage at once
    pub async fn record_usage(
        &self,
        external_id: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> AppResult<()> {
        // Record all metrics
        // TODO: Consider batching these calls
        self.record_request(external_id).await?;
        self.record_input_tokens(external_id, input_tokens).await?;
        self.record_output_tokens(external_id, output_tokens).await?;
        Ok(())
    }
}
