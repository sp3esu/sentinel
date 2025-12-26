//! Zion API client
//!
//! HTTP client for communicating with the Zion governance API.

use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};

use crate::{
    config::Config,
    error::{AppError, AppResult},
    zion::models::*,
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
    pub async fn get_limits(&self, external_id: &str) -> AppResult<Vec<UserLimit>> {
        let url = format!(
            "{}/api/v1/limits/external/{}",
            self.base_url, external_id
        );

        let response = self
            .client
            .get(&url)
            .headers(self.api_key_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

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

        let result: ExternalLimitsResponse = response.json().await?;
        Ok(result.data.limits)
    }

    /// Increment usage for a limit
    pub async fn increment_usage(
        &self,
        external_id: &str,
        limit_name: &str,
        amount: i64,
    ) -> AppResult<UserLimit> {
        let url = format!("{}/api/v1/usage/external/increment", self.base_url);

        let request = IncrementUsageRequest {
            external_id: external_id.to_string(),
            limit_name: limit_name.to_string(),
            amount: Some(amount),
        };

        let response = self
            .client
            .post(&url)
            .headers(self.api_key_headers())
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::UpstreamError(format!(
                "Zion API error {}: {}",
                status, text
            )));
        }

        let result: IncrementUsageResponse = response.json().await?;
        Ok(result.data)
    }

    /// Validate a JWT and get user profile
    pub async fn validate_jwt(&self, jwt: &str) -> AppResult<UserProfile> {
        let url = format!("{}/api/v1/users/me", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", jwt))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();

            if status.as_u16() == 401 {
                return Err(AppError::InvalidToken);
            }

            let text = response.text().await.unwrap_or_default();
            return Err(AppError::UpstreamError(format!(
                "Zion API error {}: {}",
                status, text
            )));
        }

        let result: UserProfileResponse = response.json().await?;
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
