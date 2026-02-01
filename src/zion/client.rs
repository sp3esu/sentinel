//! Zion API client
//!
//! HTTP client for communicating with the Zion governance API.

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use tracing::{debug, error, instrument, warn};

use crate::{
    config::Config,
    error::{AppError, AppResult},
    zion::models::{
        BatchIncrementData, BatchIncrementItem, BatchIncrementRequest, BatchIncrementResponse,
        ExternalLimitsResponse, IncrementUsageData, IncrementUsageRequest, IncrementUsageResponse,
        TierConfigData, TierConfigResponse, UserLimit, UserProfile, UserProfileResponse,
    },
};

/// Zion API client
pub struct ZionClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl ZionClient {
    /// Create a new Zion client
    pub fn new(client: reqwest::Client, config: &Config) -> Self {
        Self {
            client,
            base_url: config.zion_api_url.clone(),
            api_key: config.zion_api_key.clone(),
        }
    }

    /// Get user limits by external ID
    #[instrument(skip(self), fields(external_id = %external_id))]
    pub async fn get_limits(&self, external_id: &str) -> AppResult<Vec<UserLimit>> {
        let url = format!(
            "{}/api/v1/limits/external/{}",
            self.base_url, external_id
        );

        debug!(url = %url, "Fetching user limits from Zion");

        let response = self
            .client
            .get(&url)
            .headers(self.api_key_headers())
            .send()
            .await?;

        let status = response.status();
        debug!(status = %status, "Zion limits response status");

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "Zion limits request failed");

            if status.as_u16() == 404 {
                return Err(AppError::NotFound(format!(
                    "User not found: {}",
                    external_id
                )));
            }

            return Err(AppError::UpstreamError(format!(
                "Zion API error {}: {}",
                status, text
            )));
        }

        let body = response.text().await?;
        debug!(body = %body, "Zion limits response body");

        let result: ExternalLimitsResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, body = %body, "Failed to parse Zion limits response");
                return Err(AppError::UpstreamError(format!(
                    "Failed to parse Zion response: {}",
                    e
                )));
            }
        };

        debug!(limits_count = result.data.limits.len(), "Successfully fetched user limits");
        Ok(result.data.limits)
    }

    /// Increment AI usage (unified format with all 3 metrics)
    ///
    /// Sends a single request to increment input tokens, output tokens, and request count.
    /// The limit is auto-detected from the user's subscription plan by the Zion API.
    #[instrument(skip(self), fields(email = %email, input_tokens, output_tokens, requests))]
    pub async fn increment_usage(
        &self,
        email: &str,
        input_tokens: i64,
        output_tokens: i64,
        requests: i64,
        model: Option<&str>,
        timestamp: Option<&str>,
    ) -> AppResult<IncrementUsageData> {
        let url = format!("{}/api/v1/usage/external/increment", self.base_url);

        let request = IncrementUsageRequest {
            email: email.to_string(),
            ai_input_tokens: if input_tokens > 0 { Some(input_tokens) } else { None },
            ai_output_tokens: if output_tokens > 0 { Some(output_tokens) } else { None },
            ai_requests: if requests > 0 { Some(requests) } else { None },
            model: model.map(|s| s.to_string()),
            timestamp: timestamp.map(|s| s.to_string()),
        };

        debug!(url = %url, "Incrementing usage via Zion");

        let response = self
            .client
            .post(&url)
            .headers(self.api_key_headers())
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        debug!(status = %status, "Zion increment response status");

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "Zion increment request failed");
            return Err(AppError::UpstreamError(format!(
                "Zion API error {}: {}",
                status, text
            )));
        }

        let body = response.text().await?;
        debug!(body = %body, "Zion increment response body");

        let result: IncrementUsageResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, body = %body, "Failed to parse Zion increment response");
                return Err(AppError::UpstreamError(format!(
                    "Failed to parse Zion response: {}",
                    e
                )));
            }
        };

        debug!("Successfully incremented usage");
        Ok(result.data)
    }

    /// Batch increment usage for multiple users
    ///
    /// Sends up to 1000 increments in a single request.
    /// Returns partial success - individual failures don't fail the entire batch.
    #[instrument(skip(self, items), fields(items_count = items.len()))]
    pub async fn batch_increment(
        &self,
        items: Vec<BatchIncrementItem>,
    ) -> AppResult<BatchIncrementData> {
        if items.is_empty() {
            debug!("Empty batch, skipping API call");
            return Ok(BatchIncrementData {
                processed: 0,
                failed: 0,
                results: vec![],
            });
        }

        if items.len() > 1000 {
            warn!(items_count = items.len(), "Batch increment exceeds limit");
            return Err(AppError::BadRequest(
                "Batch increment limited to 1000 items".to_string(),
            ));
        }

        let url = format!("{}/api/v1/usage/external/batch-increment", self.base_url);

        let request = BatchIncrementRequest { increments: items };

        // Log the full request payload for debugging
        if let Ok(payload) = serde_json::to_string(&request) {
            debug!(url = %url, payload = %payload, "Sending batch increment to Zion");
        } else {
            debug!(url = %url, "Sending batch increment to Zion");
        }

        let response = self
            .client
            .post(&url)
            .headers(self.api_key_headers())
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        debug!(status = %status, "Zion batch increment response status");

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "Zion batch increment request failed");
            return Err(AppError::UpstreamError(format!(
                "Zion batch API error {}: {}",
                status, text
            )));
        }

        let body = response.text().await?;
        debug!(body = %body, "Zion batch increment response body");

        let result: BatchIncrementResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, body = %body, "Failed to parse Zion batch response");
                return Err(AppError::UpstreamError(format!(
                    "Failed to parse Zion response: {}",
                    e
                )));
            }
        };

        debug!(processed = result.data.processed, failed = result.data.failed, "Batch increment completed");
        Ok(result.data)
    }

    /// Validate a JWT and get user profile
    #[instrument(skip(self, jwt), fields(jwt_prefix = %jwt.chars().take(20).collect::<String>()))]
    pub async fn validate_jwt(&self, jwt: &str) -> AppResult<UserProfile> {
        let url = format!("{}/api/v1/users/me", self.base_url);

        debug!(url = %url, jwt_len = jwt.len(), "Validating JWT with Zion");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", jwt))
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to send request to Zion");
                e
            })?;

        let status = response.status();
        debug!(status = %status, "Zion JWT validation response status");

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();

            if status.as_u16() == 401 {
                warn!(status = %status, body = %text, "JWT validation failed - unauthorized");
                return Err(AppError::InvalidToken);
            }

            error!(status = %status, body = %text, "Zion JWT validation request failed");
            return Err(AppError::UpstreamError(format!(
                "Zion API error {}: {}",
                status, text
            )));
        }

        let body = response.text().await?;
        debug!(body = %body, "Zion JWT validation response body");

        let result: UserProfileResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, body = %body, "Failed to parse Zion user profile response");
                return Err(AppError::UpstreamError(format!(
                    "Failed to parse Zion response: {}",
                    e
                )));
            }
        };

        debug!(
            user_id = %result.data.id,
            email = %result.data.email,
            external_id = ?result.data.external_id,
            "JWT validated successfully"
        );
        Ok(result.data)
    }

    /// Get tier configuration (global, not per-user)
    ///
    /// Fetches the tier-to-model mapping from Zion. This configuration
    /// is global (same for all users) and changes infrequently.
    #[instrument(skip(self))]
    pub async fn get_tier_config(&self) -> AppResult<TierConfigData> {
        let url = format!("{}/api/v1/tiers/config", self.base_url);

        debug!(url = %url, "Fetching tier config from Zion");

        let response = self
            .client
            .get(&url)
            .headers(self.api_key_headers())
            .send()
            .await?;

        let status = response.status();
        debug!(status = %status, "Zion tier config response status");

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "Zion tier config request failed");
            return Err(AppError::UpstreamError(format!(
                "Zion tier config API error {}: {}",
                status, text
            )));
        }

        let body = response.text().await?;
        debug!(body = %body, "Zion tier config response body");

        let result: TierConfigResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, body = %body, "Failed to parse Zion tier config response");
                return Err(AppError::UpstreamError(format!(
                    "Failed to parse Zion tier config response: {}",
                    e
                )));
            }
        };

        debug!(version = %result.data.version, "Successfully fetched tier config");
        Ok(result.data)
    }

    /// Build headers with API key authentication
    fn api_key_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key).expect("Invalid API key"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }
}
