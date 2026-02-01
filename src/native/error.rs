//! Unified error types for the Native API
//!
//! Provides OpenAI-compatible error responses for all Native API endpoints.
//! Errors are wrapped in a consistent JSON format that matches OpenAI's error structure.

use axum::{
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

/// Native API error with OpenAI-compatible structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NativeError {
    /// Human-readable error message
    pub message: String,
    /// Error type category
    #[serde(rename = "type")]
    pub error_type: String,
    /// Error code for programmatic handling
    pub code: String,
    /// Provider hint when error originates from upstream
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// Wrapper for error responses matching OpenAI's format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NativeErrorResponse {
    /// The error details
    pub error: NativeError,
    /// Rate limit information (internal use, not serialized in response body)
    #[serde(skip)]
    pub rate_limit_info: Option<RateLimitInfo>,
}

/// Rate limit information for 429 responses
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimitInfo {
    /// Seconds until retry is allowed
    pub retry_after: Option<u64>,
}

impl NativeErrorResponse {
    /// Create a validation error (400 Bad Request)
    ///
    /// Use for invalid request format, missing fields, or constraint violations.
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            error: NativeError {
                message: message.into(),
                error_type: "invalid_request_error".to_string(),
                code: "invalid_request".to_string(),
                provider: None,
            },
            rate_limit_info: None,
        }
    }

    /// Create a provider error (502 Bad Gateway)
    ///
    /// Use when an upstream provider returns an error. Includes provider hint.
    pub fn provider_error(message: impl Into<String>, provider: &str) -> Self {
        Self {
            error: NativeError {
                message: message.into(),
                error_type: "upstream_error".to_string(),
                code: "provider_error".to_string(),
                provider: Some(provider.to_string()),
            },
            rate_limit_info: None,
        }
    }

    /// Create a rate limit error (429 Too Many Requests)
    ///
    /// Use when request rate exceeds limits. Optionally includes Retry-After header.
    pub fn rate_limited(message: impl Into<String>, retry_after: Option<u64>) -> Self {
        Self {
            error: NativeError {
                message: message.into(),
                error_type: "rate_limit_error".to_string(),
                code: "rate_limit_exceeded".to_string(),
                provider: None,
            },
            rate_limit_info: Some(RateLimitInfo { retry_after }),
        }
    }

    /// Create an internal server error (500 Internal Server Error)
    ///
    /// Use for unexpected errors that are not the client's fault.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            error: NativeError {
                message: message.into(),
                error_type: "server_error".to_string(),
                code: "internal_error".to_string(),
                provider: None,
            },
            rate_limit_info: None,
        }
    }

    /// Create a service unavailable error (503 Service Unavailable)
    ///
    /// Use when the service is temporarily unavailable (e.g., all providers in backoff).
    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            error: NativeError {
                message: message.into(),
                error_type: "service_unavailable".to_string(),
                code: "service_unavailable".to_string(),
                provider: None,
            },
            rate_limit_info: None,
        }
    }

    /// Convert from AppError
    pub fn from_app_error(err: crate::error::AppError) -> Self {
        use crate::error::AppError;
        match err {
            AppError::ServiceUnavailable { message, .. } => Self::service_unavailable(&message),
            AppError::BadRequest(msg) => Self::validation(msg),
            AppError::NotFound(msg) => Self::validation(msg),
            _ => Self::internal(err.to_string()),
        }
    }

    /// Get the HTTP status code for this error
    fn status_code(&self) -> StatusCode {
        match self.error.error_type.as_str() {
            "invalid_request_error" => StatusCode::BAD_REQUEST,
            "upstream_error" => StatusCode::BAD_GATEWAY,
            "rate_limit_error" => StatusCode::TOO_MANY_REQUESTS,
            "server_error" => StatusCode::INTERNAL_SERVER_ERROR,
            "service_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for NativeErrorResponse {
    fn into_response(self) -> Response {
        let status = self.status_code();

        // Build headers
        let mut headers = HeaderMap::new();

        // Add Retry-After header for rate limit errors
        if let Some(ref rate_limit_info) = self.rate_limit_info {
            if let Some(retry_after) = rate_limit_info.retry_after {
                if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
                    headers.insert("Retry-After", value);
                }
            }
        }

        // Create response body (excludes rate_limit_info due to #[serde(skip)])
        let body = Json(&self);

        (status, headers, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[test]
    fn test_validation_error_json() {
        let error = NativeErrorResponse::validation("Invalid model specified");
        let json = serde_json::to_string(&error).unwrap();

        // Verify structure matches OpenAI format
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error"]["message"], "Invalid model specified");
        assert_eq!(parsed["error"]["type"], "invalid_request_error");
        assert_eq!(parsed["error"]["code"], "invalid_request");
        assert!(parsed["error"].get("provider").is_none());
    }

    #[test]
    fn test_provider_error_includes_hint() {
        let error = NativeErrorResponse::provider_error("Model overloaded", "openai");
        let json = serde_json::to_string(&error).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["error"]["provider"], "openai");
        assert_eq!(parsed["error"]["type"], "upstream_error");
        assert_eq!(parsed["error"]["code"], "provider_error");
    }

    #[test]
    fn test_rate_limit_error_status() {
        let error = NativeErrorResponse::rate_limited("Too many requests", Some(60));
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_rate_limit_error_includes_retry_after_header() {
        let error = NativeErrorResponse::rate_limited("Too many requests", Some(30));
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get("Retry-After").unwrap(),
            &HeaderValue::from_static("30")
        );
    }

    #[test]
    fn test_internal_error_status() {
        let error = NativeErrorResponse::internal("Database connection failed");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_validation_error_status() {
        let error = NativeErrorResponse::validation("Missing required field");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_provider_error_status() {
        let error = NativeErrorResponse::provider_error("Upstream timeout", "anthropic");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn test_error_response_body_format() {
        let error = NativeErrorResponse::validation("Test error");
        let response = error.into_response();

        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify OpenAI-compatible structure
        assert!(parsed.get("error").is_some());
        assert_eq!(parsed["error"]["message"], "Test error");
        assert_eq!(parsed["error"]["type"], "invalid_request_error");
        assert_eq!(parsed["error"]["code"], "invalid_request");
    }
}
