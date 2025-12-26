//! Vercel AI Gateway proxy
//!
//! Handles request forwarding to Vercel's AI Gateway service.

use bytes::Bytes;
use futures::Stream;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{de::DeserializeOwned, Serialize};
use std::pin::Pin;

use crate::{config::Config, error::AppResult};

/// Vercel AI Gateway client
pub struct VercelGateway {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl VercelGateway {
    /// Create a new Vercel Gateway client
    pub fn new(client: reqwest::Client, config: &Config) -> Self {
        Self {
            client,
            base_url: config.vercel_gateway_url.clone(),
            api_key: config.vercel_gateway_api_key.clone(),
        }
    }

    /// Forward a chat completion request (non-streaming)
    pub async fn chat_completions<T: Serialize, R: DeserializeOwned>(
        &self,
        request: &T,
    ) -> AppResult<R> {
        let url = format!("{}/chat/completions", self.base_url);
        self.post(&url, request).await
    }

    /// Forward a chat completion request with streaming response
    pub async fn chat_completions_stream<T: Serialize>(
        &self,
        request: &T,
    ) -> AppResult<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        self.post_stream(&url, request).await
    }

    /// Forward a completion request (non-streaming)
    pub async fn completions<T: Serialize, R: DeserializeOwned>(
        &self,
        request: &T,
    ) -> AppResult<R> {
        let url = format!("{}/completions", self.base_url);
        self.post(&url, request).await
    }

    /// Forward a completion request with streaming response
    pub async fn completions_stream<T: Serialize>(
        &self,
        request: &T,
    ) -> AppResult<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>> {
        let url = format!("{}/completions", self.base_url);
        self.post_stream(&url, request).await
    }

    /// List available models
    pub async fn list_models<R: DeserializeOwned>(&self) -> AppResult<R> {
        let url = format!("{}/models", self.base_url);
        self.get(&url).await
    }

    /// Make a POST request to the gateway (non-streaming)
    async fn post<T: Serialize, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> AppResult<R> {
        let response = self
            .client
            .post(url)
            .headers(self.default_headers())
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(crate::error::AppError::UpstreamError(format!(
                "Vercel Gateway error {}: {}",
                status, text
            )));
        }

        let result = response.json().await?;
        Ok(result)
    }

    /// Make a POST request to the gateway with streaming response
    async fn post_stream<T: Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> AppResult<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>> {
        let response = self
            .client
            .post(url)
            .headers(self.default_headers())
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(crate::error::AppError::UpstreamError(format!(
                "Vercel Gateway error {}: {}",
                status, text
            )));
        }

        // Return the byte stream
        Ok(Box::pin(response.bytes_stream()))
    }

    /// Make a GET request to the gateway
    async fn get<R: DeserializeOwned>(&self, url: &str) -> AppResult<R> {
        let response = self
            .client
            .get(url)
            .headers(self.default_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(crate::error::AppError::UpstreamError(format!(
                "Vercel Gateway error {}: {}",
                status, text
            )));
        }

        let result = response.json().await?;
        Ok(result)
    }

    /// Build default headers for gateway requests
    fn default_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .expect("Invalid API key"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers
    }
}
