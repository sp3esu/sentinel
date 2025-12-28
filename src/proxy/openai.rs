//! OpenAI provider implementation
//!
//! Handles request forwarding to OpenAI's API with comprehensive logging
//! and secure header handling.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderName, Method, Response, StatusCode};
use http_body_util::BodyExt;
use reqwest::header::HeaderMap;
use tracing::{debug, instrument};

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::proxy::headers::{build_default_headers, build_proxy_headers, is_hop_by_hop_header};
use crate::proxy::logging::RequestContext;
use crate::proxy::provider::{AiProvider, ByteStream};

/// OpenAI API provider
///
/// Implements the AiProvider trait for OpenAI's API, handling all communication
/// with proper logging and secure header filtering.
pub struct OpenAIProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider
    ///
    /// # Panics
    ///
    /// Panics if OPENAI_API_KEY is not configured.
    pub fn new(client: reqwest::Client, config: &Config) -> Self {
        let api_key = config
            .openai_api_key
            .clone()
            .expect("OPENAI_API_KEY must be configured");

        Self {
            client,
            base_url: config.openai_api_url.clone(),
            api_key,
        }
    }

    /// Check if the provider is configured (always true for OpenAIProvider)
    ///
    /// This method exists for backwards compatibility. The provider always
    /// requires configuration and will panic on creation if not configured.
    #[deprecated(note = "OpenAI API key is now required, this always returns true")]
    pub fn is_configured(&self) -> bool {
        true
    }

    /// Make a POST request (non-streaming)
    async fn post(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
        incoming_headers: &HeaderMap,
        ctx: &RequestContext,
    ) -> AppResult<serde_json::Value> {
        let url = format!("{}{}", self.base_url, endpoint);

        let headers = build_proxy_headers(incoming_headers, &self.api_key);
        ctx.log_headers_prepared(headers.len());
        ctx.log_upstream_request(&url, None);

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                ctx.log_connection_error(&e.to_string(), &url);
                e
            })?;

        let status = response.status();
        let content_length = response.content_length();
        ctx.log_upstream_response(status.as_u16(), content_length);

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            ctx.log_error(&format!("OpenAI error {}: {}", status, text));
            return Err(AppError::UpstreamError(format!(
                "OpenAI error {}: {}",
                status, text
            )));
        }

        let body_text = response.text().await?;
        debug!(
            trace_id = %ctx.trace_id,
            body_len = body_text.len(),
            "Response body received"
        );

        let result: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            ctx.log_error(&format!("Failed to parse response: {}", e));
            AppError::UpstreamError(format!("Failed to parse response: {}", e))
        })?;

        ctx.log_request_complete(None);
        Ok(result)
    }

    /// Make a POST request with streaming response
    async fn post_stream(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
        incoming_headers: &HeaderMap,
        ctx: &RequestContext,
    ) -> AppResult<ByteStream> {
        let url = format!("{}{}", self.base_url, endpoint);

        let headers = build_proxy_headers(incoming_headers, &self.api_key);
        ctx.log_headers_prepared(headers.len());
        ctx.log_upstream_request(&url, None);

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                ctx.log_connection_error(&e.to_string(), &url);
                e
            })?;

        let status = response.status();
        ctx.log_upstream_response(status.as_u16(), None);

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            ctx.log_error(&format!("OpenAI error {}: {}", status, text));
            return Err(AppError::UpstreamError(format!(
                "OpenAI error {}: {}",
                status, text
            )));
        }

        ctx.log_stream_started();
        Ok(Box::pin(response.bytes_stream()))
    }

    /// Make a GET request
    async fn get(&self, endpoint: &str, ctx: &RequestContext) -> AppResult<serde_json::Value> {
        let url = format!("{}{}", self.base_url, endpoint);

        let headers = build_default_headers(&self.api_key);
        ctx.log_headers_prepared(headers.len());
        ctx.log_upstream_request(&url, None);

        let response = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                ctx.log_connection_error(&e.to_string(), &url);
                e
            })?;

        let status = response.status();
        let content_length = response.content_length();
        ctx.log_upstream_response(status.as_u16(), content_length);

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            ctx.log_error(&format!("OpenAI error {}: {}", status, text));
            return Err(AppError::UpstreamError(format!(
                "OpenAI error {}: {}",
                status, text
            )));
        }

        let body_text = response.text().await?;
        let result: serde_json::Value = serde_json::from_str(&body_text).map_err(|e| {
            ctx.log_error(&format!("Failed to parse response: {}", e));
            AppError::UpstreamError(format!("Failed to parse response: {}", e))
        })?;

        ctx.log_request_complete(None);
        Ok(result)
    }

    /// Convert reqwest Response to axum Response
    async fn convert_response(&self, response: reqwest::Response) -> AppResult<Response<Body>> {
        let status = StatusCode::from_u16(response.status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let response_headers = response.headers().clone();

        // Stream the response body
        let body = Body::from_stream(response.bytes_stream());

        // Build axum response
        let mut builder = Response::builder().status(status);

        // Copy headers, filtering out hop-by-hop headers
        for (name, value) in response_headers.iter() {
            if !is_hop_by_hop_header(name) {
                if let Ok(header_name) = HeaderName::from_bytes(name.as_ref()) {
                    builder = builder.header(header_name, value.as_bytes());
                }
            }
        }

        builder
            .body(body)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build response: {}", e)))
    }
}

#[async_trait]
impl AiProvider for OpenAIProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    #[instrument(skip(self, request, incoming_headers), fields(provider = "openai", endpoint = "chat/completions"))]
    async fn chat_completions(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value> {
        let ctx = RequestContext::new(self.name(), "/v1/chat/completions");
        ctx.log_request_start();
        self.post("/chat/completions", &request, incoming_headers, &ctx)
            .await
    }

    #[instrument(skip(self, request, incoming_headers), fields(provider = "openai", endpoint = "chat/completions", streaming = true))]
    async fn chat_completions_stream(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<ByteStream> {
        let ctx = RequestContext::new(self.name(), "/v1/chat/completions").with_streaming(true);
        ctx.log_request_start();
        self.post_stream("/chat/completions", &request, incoming_headers, &ctx)
            .await
    }

    #[instrument(skip(self, request, incoming_headers), fields(provider = "openai", endpoint = "completions"))]
    async fn completions(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value> {
        let ctx = RequestContext::new(self.name(), "/v1/completions");
        ctx.log_request_start();
        self.post("/completions", &request, incoming_headers, &ctx)
            .await
    }

    #[instrument(skip(self, request, incoming_headers), fields(provider = "openai", endpoint = "completions", streaming = true))]
    async fn completions_stream(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<ByteStream> {
        let ctx = RequestContext::new(self.name(), "/v1/completions").with_streaming(true);
        ctx.log_request_start();
        self.post_stream("/completions", &request, incoming_headers, &ctx)
            .await
    }

    #[instrument(skip(self, request, incoming_headers), fields(provider = "openai", endpoint = "embeddings"))]
    async fn embeddings(
        &self,
        request: serde_json::Value,
        incoming_headers: &HeaderMap,
    ) -> AppResult<serde_json::Value> {
        let ctx = RequestContext::new(self.name(), "/v1/embeddings");
        ctx.log_request_start();
        self.post("/embeddings", &request, incoming_headers, &ctx)
            .await
    }

    #[instrument(skip(self), fields(provider = "openai", endpoint = "models"))]
    async fn list_models(&self) -> AppResult<serde_json::Value> {
        let ctx = RequestContext::new(self.name(), "/v1/models");
        ctx.log_request_start();
        self.get("/models", &ctx).await
    }

    #[instrument(skip(self), fields(provider = "openai", endpoint = "models"))]
    async fn get_model(&self, model_id: &str) -> AppResult<serde_json::Value> {
        let ctx = RequestContext::new(self.name(), &format!("/v1/models/{}", model_id));
        ctx.log_request_start();
        self.get(&format!("/models/{}", model_id), &ctx).await
    }

    #[instrument(skip(self, incoming_headers, body), fields(provider = "openai", method = %method, path = %path))]
    async fn forward_raw(
        &self,
        method: Method,
        path: &str,
        incoming_headers: HeaderMap,
        body: Body,
    ) -> AppResult<Response<Body>> {
        let ctx = RequestContext::new(self.name(), path);
        ctx.log_request_start();

        let url = format!("{}{}", self.base_url, path);

        // Build headers using the secure filtering
        let headers = build_proxy_headers(&incoming_headers, &self.api_key);
        ctx.log_headers_prepared(headers.len());

        // Convert axum Body to bytes for reqwest
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| {
                ctx.log_error(&format!("Failed to read request body: {}", e));
                AppError::Internal(anyhow::anyhow!("Failed to read request body: {}", e))
            })?
            .to_bytes();

        ctx.log_upstream_request(&url, Some(body_bytes.len()));

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
            ctx.log_connection_error(&e.to_string(), &url);
            e
        })?;

        let status = response.status();
        let content_length = response.content_length();
        ctx.log_upstream_response(status.as_u16(), content_length);

        // If error status, log the error body and forward it to client
        if status.is_client_error() || status.is_server_error() {
            let response_headers = response.headers().clone();

            // Read the error body to log it
            let error_body = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            ctx.log_upstream_error_body(status.as_u16(), &error_body);

            // Reconstruct the response with the body we already read, preserving relevant headers
            let axum_status = StatusCode::from_u16(status.as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

            let mut builder = Response::builder().status(axum_status);

            // Copy relevant headers from the upstream error response
            for (name, value) in response_headers.iter() {
                if !is_hop_by_hop_header(name) {
                    if let Ok(header_name) = HeaderName::from_bytes(name.as_ref()) {
                        builder = builder.header(header_name, value.as_bytes());
                    }
                }
            }

            debug!(
                trace_id = %ctx.trace_id,
                status = %axum_status,
                body_len = error_body.len(),
                "Forwarding error response to client"
            );

            let axum_response = builder
                .body(Body::from(error_body))
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build error response: {}", e)))?;

            return Ok(axum_response);
        }

        // Convert and return the response
        let axum_response = self.convert_response(response).await?;
        ctx.log_request_complete(None);

        Ok(axum_response)
    }
}

// Keep the old OpenAIClient for backwards compatibility during migration
// TODO: Remove after all routes are migrated to use AiProvider trait
pub use OpenAIProvider as OpenAIClient;
