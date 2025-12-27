//! Vercel AI Gateway proxy
//!
//! Handles request forwarding to Vercel's AI Gateway service.

use axum::body::Body;
use axum::http::{header, HeaderName, Method, Response, StatusCode};
use bytes::Bytes;
use futures::Stream;
use http_body_util::BodyExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{de::DeserializeOwned, Serialize};
use std::pin::Pin;
use tracing::{debug, error, info, instrument};

use crate::{config::Config, error::{AppError, AppResult}};

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
    #[instrument(skip(self, request), fields(endpoint = "chat/completions", streaming = false))]
    pub async fn chat_completions<T: Serialize, R: DeserializeOwned>(
        &self,
        request: &T,
    ) -> AppResult<R> {
        let url = format!("{}/chat/completions", self.base_url);
        info!(url = %url, "Forwarding chat completion request to Vercel AI Gateway");
        self.post(&url, request).await
    }

    /// Forward a chat completion request with streaming response
    #[instrument(skip(self, request), fields(endpoint = "chat/completions", streaming = true))]
    pub async fn chat_completions_stream<T: Serialize>(
        &self,
        request: &T,
    ) -> AppResult<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        info!(url = %url, "Forwarding streaming chat completion request to Vercel AI Gateway");
        self.post_stream(&url, request).await
    }

    /// Forward a completion request (non-streaming)
    #[instrument(skip(self, request), fields(endpoint = "completions", streaming = false))]
    pub async fn completions<T: Serialize, R: DeserializeOwned>(
        &self,
        request: &T,
    ) -> AppResult<R> {
        let url = format!("{}/completions", self.base_url);
        info!(url = %url, "Forwarding completion request to Vercel AI Gateway");
        self.post(&url, request).await
    }

    /// Forward a completion request with streaming response
    #[instrument(skip(self, request), fields(endpoint = "completions", streaming = true))]
    pub async fn completions_stream<T: Serialize>(
        &self,
        request: &T,
    ) -> AppResult<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>> {
        let url = format!("{}/completions", self.base_url);
        info!(url = %url, "Forwarding streaming completion request to Vercel AI Gateway");
        self.post_stream(&url, request).await
    }

    /// List available models
    #[instrument(skip(self), fields(endpoint = "models"))]
    pub async fn list_models<R: DeserializeOwned>(&self) -> AppResult<R> {
        let url = format!("{}/models", self.base_url);
        info!(url = %url, "Fetching models from Vercel AI Gateway");
        self.get(&url).await
    }

    /// Forward an embeddings request (non-streaming)
    #[instrument(skip(self, request), fields(endpoint = "embeddings"))]
    pub async fn embeddings<T: Serialize, R: DeserializeOwned>(
        &self,
        request: &T,
    ) -> AppResult<R> {
        let url = format!("{}/embeddings", self.base_url);
        info!(url = %url, "Forwarding embeddings request to Vercel AI Gateway");
        self.post(&url, request).await
    }

    /// Forward a raw request to any endpoint (pass-through proxy)
    ///
    /// This method forwards the request body unchanged without parsing,
    /// allowing any OpenAI API endpoint to be proxied.
    #[instrument(skip(self, incoming_headers, body), fields(method = %method, path = %path))]
    pub async fn forward_raw(
        &self,
        method: Method,
        path: &str,
        incoming_headers: HeaderMap,
        body: Body,
    ) -> AppResult<Response<Body>> {
        let url = format!("{}{}", self.base_url, path);
        info!(url = %url, method = %method, "Forwarding raw request to Vercel AI Gateway");

        // Build headers for the proxy request
        let headers = self.build_proxy_headers(&incoming_headers);

        // Convert axum Body to bytes for reqwest
        // Note: This buffers the body, which is fine for most API requests
        // but for very large uploads we might want to use streaming
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read request body: {}", e)))?
            .to_bytes();

        debug!(
            url = %url,
            method = %method,
            body_len = body_bytes.len(),
            "Sending raw request to Vercel AI Gateway"
        );

        // Build the request
        let mut request_builder = self.client.request(
            reqwest::Method::from_bytes(method.as_str().as_bytes())
                .unwrap_or(reqwest::Method::POST),
            &url,
        );
        request_builder = request_builder.headers(headers);

        // Only add body for methods that support it
        if method != Method::GET && method != Method::HEAD {
            request_builder = request_builder.body(body_bytes.to_vec());
        }

        let response = request_builder.send().await.map_err(|e| {
            error!(url = %url, error = %e, "Failed to send raw request to Vercel AI Gateway");
            e
        })?;

        let status = response.status();
        debug!(
            url = %url,
            status = %status,
            "Received raw response from Vercel AI Gateway"
        );

        // Convert reqwest response to axum response
        self.convert_response(response).await
    }

    /// Build headers for proxy request
    ///
    /// Copies relevant headers from the incoming request and adds authentication.
    fn build_proxy_headers(&self, incoming: &HeaderMap) -> HeaderMap {
        let mut headers = HeaderMap::new();

        // Copy specific headers from incoming request
        let headers_to_copy = [
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::USER_AGENT,
        ];

        for header_name in headers_to_copy {
            if let Some(value) = incoming.get(&header_name) {
                headers.insert(header_name, value.clone());
            }
        }

        // Set authorization with Vercel API key
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .expect("Invalid API key"),
        );

        headers
    }

    /// Convert reqwest Response to axum Response
    async fn convert_response(
        &self,
        response: reqwest::Response,
    ) -> AppResult<Response<Body>> {
        let status = StatusCode::from_u16(response.status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let headers = response.headers().clone();

        // Stream the response body
        let body = Body::from_stream(response.bytes_stream());

        // Build axum response
        let mut builder = Response::builder().status(status);

        // Copy headers, filtering out hop-by-hop headers
        for (name, value) in headers.iter() {
            if !is_hop_by_hop_header(name.as_str()) {
                if let Ok(header_name) = HeaderName::from_bytes(name.as_ref()) {
                    builder = builder.header(header_name, value.as_bytes());
                }
            }
        }

        builder
            .body(body)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build response: {}", e)))
    }

    /// Make a POST request to the gateway (non-streaming)
    async fn post<T: Serialize, R: DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> AppResult<R> {
        debug!(
            url = %url,
            host = %self.base_url,
            "Sending POST request to Vercel AI Gateway"
        );

        let response = self
            .client
            .post(url)
            .headers(self.default_headers())
            .json(body)
            .send()
            .await
            .map_err(|e| {
                error!(url = %url, error = %e, "Failed to send request to Vercel AI Gateway");
                e
            })?;

        let status = response.status();
        debug!(
            url = %url,
            status = %status,
            "Received response from Vercel AI Gateway"
        );

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(
                url = %url,
                status = %status,
                body = %text,
                "Vercel AI Gateway returned error"
            );
            return Err(crate::error::AppError::UpstreamError(format!(
                "Vercel Gateway error {}: {}",
                status, text
            )));
        }

        let body_text = response.text().await?;
        debug!(
            url = %url,
            body_len = body_text.len(),
            "Vercel AI Gateway response body received"
        );

        let result: R = serde_json::from_str(&body_text).map_err(|e| {
            error!(
                url = %url,
                error = %e,
                body = %body_text,
                "Failed to parse Vercel AI Gateway response"
            );
            crate::error::AppError::UpstreamError(format!("Failed to parse response: {}", e))
        })?;

        debug!(url = %url, "Successfully processed Vercel AI Gateway response");
        Ok(result)
    }

    /// Make a POST request to the gateway with streaming response
    async fn post_stream<T: Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> AppResult<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>> {
        debug!(
            url = %url,
            host = %self.base_url,
            "Sending streaming POST request to Vercel AI Gateway"
        );

        let response = self
            .client
            .post(url)
            .headers(self.default_headers())
            .json(body)
            .send()
            .await
            .map_err(|e| {
                error!(url = %url, error = %e, "Failed to send streaming request to Vercel AI Gateway");
                e
            })?;

        let status = response.status();
        debug!(
            url = %url,
            status = %status,
            "Received streaming response from Vercel AI Gateway"
        );

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(
                url = %url,
                status = %status,
                body = %text,
                "Vercel AI Gateway returned error for streaming request"
            );
            return Err(crate::error::AppError::UpstreamError(format!(
                "Vercel Gateway error {}: {}",
                status, text
            )));
        }

        debug!(url = %url, "Starting to stream response from Vercel AI Gateway");
        // Return the byte stream
        Ok(Box::pin(response.bytes_stream()))
    }

    /// Make a GET request to the gateway
    async fn get<R: DeserializeOwned>(&self, url: &str) -> AppResult<R> {
        debug!(
            url = %url,
            host = %self.base_url,
            "Sending GET request to Vercel AI Gateway"
        );

        let response = self
            .client
            .get(url)
            .headers(self.default_headers())
            .send()
            .await
            .map_err(|e| {
                error!(url = %url, error = %e, "Failed to send GET request to Vercel AI Gateway");
                e
            })?;

        let status = response.status();
        debug!(
            url = %url,
            status = %status,
            "Received response from Vercel AI Gateway"
        );

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            error!(
                url = %url,
                status = %status,
                body = %text,
                "Vercel AI Gateway returned error"
            );
            return Err(crate::error::AppError::UpstreamError(format!(
                "Vercel Gateway error {}: {}",
                status, text
            )));
        }

        let body_text = response.text().await?;
        debug!(
            url = %url,
            body_len = body_text.len(),
            "Vercel AI Gateway GET response body received"
        );

        let result: R = serde_json::from_str(&body_text).map_err(|e| {
            error!(
                url = %url,
                error = %e,
                body = %body_text,
                "Failed to parse Vercel AI Gateway GET response"
            );
            crate::error::AppError::UpstreamError(format!("Failed to parse response: {}", e))
        })?;

        debug!(url = %url, "Successfully processed Vercel AI Gateway GET response");
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

/// Check if a header is a hop-by-hop header that should not be forwarded
fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
