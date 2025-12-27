//! OpenAI direct proxy
//!
//! Handles request forwarding directly to OpenAI API for endpoints
//! not supported by Vercel AI Gateway (e.g., /responses).

use axum::body::Body;
use axum::http::{header, HeaderName, Method, Response, StatusCode};
use http_body_util::BodyExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use tracing::{debug, error, info, instrument};

use crate::{config::Config, error::{AppError, AppResult}};

/// OpenAI direct client for endpoints not supported by Vercel AI Gateway
pub struct OpenAIClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl OpenAIClient {
    /// Create a new OpenAI client
    pub fn new(client: reqwest::Client, config: &Config) -> Self {
        Self {
            client,
            base_url: config.openai_api_url.clone(),
            api_key: config.openai_api_key.clone(),
        }
    }

    /// Check if the client is configured with an API key
    pub fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    /// Forward a raw request to OpenAI
    #[instrument(skip(self, incoming_headers, body), fields(method = %method, path = %path))]
    pub async fn forward_raw(
        &self,
        method: Method,
        path: &str,
        incoming_headers: HeaderMap,
        body: Body,
    ) -> AppResult<Response<Body>> {
        let api_key = self.api_key.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable("OPENAI_API_KEY is not configured".to_string())
        })?;

        let url = format!("{}{}", self.base_url, path);
        info!(url = %url, method = %method, "Forwarding request directly to OpenAI");

        // Build headers for the proxy request
        let headers = self.build_proxy_headers(&incoming_headers, api_key);

        // Convert axum Body to bytes for reqwest
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read request body: {}", e)))?
            .to_bytes();

        debug!(
            url = %url,
            method = %method,
            body_len = body_bytes.len(),
            "Sending request to OpenAI"
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
            error!(url = %url, error = %e, "Failed to send request to OpenAI");
            e
        })?;

        let status = response.status();
        debug!(
            url = %url,
            status = %status,
            "Received response from OpenAI"
        );

        // Convert reqwest response to axum response
        self.convert_response(response).await
    }

    /// Build headers for proxy request
    fn build_proxy_headers(&self, incoming: &HeaderMap, api_key: &str) -> HeaderMap {
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

        // Set authorization with OpenAI API key
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .expect("Invalid API key"),
        );

        headers
    }

    /// Convert reqwest response to axum response
    async fn convert_response(
        &self,
        response: reqwest::Response,
    ) -> AppResult<Response<Body>> {
        let status = StatusCode::from_u16(response.status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let mut builder = Response::builder().status(status);

        // Copy headers, filtering out hop-by-hop headers
        let hop_by_hop: &[HeaderName] = &[
            header::CONNECTION,
            header::PROXY_AUTHENTICATE,
            header::PROXY_AUTHORIZATION,
            header::TE,
            header::TRAILER,
            header::TRANSFER_ENCODING,
            header::UPGRADE,
        ];

        for (name, value) in response.headers() {
            if !hop_by_hop.contains(name) {
                builder = builder.header(name.clone(), value.clone());
            }
        }

        // Stream the body
        let body = Body::from_stream(response.bytes_stream());

        builder
            .body(body)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build response: {}", e)))
    }
}
