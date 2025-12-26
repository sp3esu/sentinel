//! Error types for Sentinel
//!
//! This module defines custom error types used throughout the application.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// Application-level errors
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Authentication required")]
    Unauthorized,

    #[error("Invalid authentication token")]
    InvalidToken,

    #[error("Access forbidden")]
    Forbidden,

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Rate limit exceeded: {message}")]
    RateLimitExceeded {
        message: String,
        limit: i64,
        used: i64,
        remaining: i64,
        reset_at: Option<String>,
    },

    #[error("Quota exceeded: {message}")]
    QuotaExceeded {
        message: String,
        limit: i64,
        used: i64,
    },

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Upstream error: {0}")]
    UpstreamError(String),

    #[error("Redis error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("HTTP client error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Error response body
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

/// Error details
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ErrorDetails>,
}

/// Additional error details for rate limiting
#[derive(Debug, Serialize)]
pub struct ErrorDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match &self {
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                self.to_string(),
                None,
            ),
            AppError::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "INVALID_TOKEN",
                self.to_string(),
                None,
            ),
            AppError::Forbidden => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                self.to_string(),
                None,
            ),
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                msg.clone(),
                None,
            ),
            AppError::RateLimitExceeded {
                message,
                limit,
                used,
                remaining,
                reset_at,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMIT_EXCEEDED",
                message.clone(),
                Some(ErrorDetails {
                    limit: Some(*limit),
                    used: Some(*used),
                    remaining: Some(*remaining),
                    reset_at: reset_at.clone(),
                }),
            ),
            AppError::QuotaExceeded { message, limit, used } => (
                StatusCode::TOO_MANY_REQUESTS,
                "QUOTA_EXCEEDED",
                message.clone(),
                Some(ErrorDetails {
                    limit: Some(*limit),
                    used: Some(*used),
                    remaining: None,
                    reset_at: None,
                }),
            ),
            AppError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "BAD_REQUEST",
                msg.clone(),
                None,
            ),
            AppError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
                msg.clone(),
                None,
            ),
            AppError::UpstreamError(msg) => (
                StatusCode::BAD_GATEWAY,
                "UPSTREAM_ERROR",
                msg.clone(),
                None,
            ),
            AppError::RedisError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "CACHE_ERROR",
                "Cache service error".to_string(),
                None,
            ),
            AppError::HttpError(_) => (
                StatusCode::BAD_GATEWAY,
                "UPSTREAM_ERROR",
                "Upstream service error".to_string(),
                None,
            ),
            AppError::JsonError(_) => (
                StatusCode::BAD_REQUEST,
                "INVALID_JSON",
                "Invalid JSON in request".to_string(),
                None,
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "Internal server error".to_string(),
                None,
            ),
        };

        let body = ErrorResponse {
            error: ErrorBody {
                code: code.to_string(),
                message,
                details,
            },
        };

        (status, Json(body)).into_response()
    }
}

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;
