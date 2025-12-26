//! Mock Zion API server for testing
//!
//! Provides wiremock-based mocks for the Zion API endpoints:
//! - GET /api/v1/limits/external/{id} - Get user limits
//! - POST /api/v1/usage/external/increment - Increment usage
//! - GET /api/v1/users/me - Get user profile
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::mocks::zion::{MockZionServer, ZionTestData};
//!
//! #[tokio::test]
//! async fn test_with_zion_mock() {
//!     let mock_server = MockZionServer::start().await;
//!
//!     // Set up successful limits response
//!     mock_server.mock_get_limits_success("user123", ZionTestData::default_limits()).await;
//!
//!     // Use mock_server.uri() as the Zion API URL
//!     // ...
//! }
//! ```

use serde::{Deserialize, Serialize};
use wiremock::{
    matchers::{body_json, header, header_exists, method, path, path_regex},
    Mock, MockServer, ResponseTemplate,
};

/// Mock Zion API server wrapper
pub struct MockZionServer {
    server: MockServer,
}

impl MockZionServer {
    /// Start a new mock Zion server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        Self { server }
    }

    /// Get the mock server URI
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// Get the mock server address (host:port)
    pub fn address(&self) -> String {
        self.server.address().to_string()
    }

    // =========================================================================
    // GET /api/v1/limits/external/{id} - User Limits
    // =========================================================================

    /// Mock successful GET limits response
    pub async fn mock_get_limits_success(&self, external_id: &str, limits: Vec<UserLimitMock>) {
        let response = ExternalLimitsResponseMock {
            success: true,
            data: ExternalLimitsDataMock {
                user_id: format!("usr_{}", external_id),
                external_id: external_id.to_string(),
                limits,
            },
        };

        Mock::given(method("GET"))
            .and(path(format!("/api/v1/limits/external/{}", external_id)))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 404 Not Found response for limits
    pub async fn mock_get_limits_not_found(&self, external_id: &str) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "NOT_FOUND".to_string(),
                message: format!("User with external ID '{}' not found", external_id),
            },
        };

        Mock::given(method("GET"))
            .and(path(format!("/api/v1/limits/external/{}", external_id)))
            .respond_with(ResponseTemplate::new(404).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 401 Unauthorized response for limits
    pub async fn mock_get_limits_unauthorized(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "UNAUTHORIZED".to_string(),
                message: "Invalid or expired authentication token".to_string(),
            },
        };

        Mock::given(method("GET"))
            .and(path_regex(r"/api/v1/limits/external/.*"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 500 Internal Server Error for limits
    pub async fn mock_get_limits_server_error(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "INTERNAL_ERROR".to_string(),
                message: "An unexpected error occurred".to_string(),
            },
        };

        Mock::given(method("GET"))
            .and(path_regex(r"/api/v1/limits/external/.*"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    // =========================================================================
    // POST /api/v1/usage/external/increment - Increment Usage
    // =========================================================================

    /// Mock successful increment usage response
    pub async fn mock_increment_usage_success(&self, updated_limit: UserLimitMock) {
        let response = IncrementUsageResponseMock {
            success: true,
            data: updated_limit,
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/increment"))
            .and(header_exists("Authorization"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock increment usage with specific request matching
    pub async fn mock_increment_usage_for_request(
        &self,
        external_id: &str,
        limit_name: &str,
        updated_limit: UserLimitMock,
    ) {
        let request = IncrementUsageRequestMock {
            external_id: external_id.to_string(),
            limit_name: limit_name.to_string(),
            amount: None,
        };

        let response = IncrementUsageResponseMock {
            success: true,
            data: updated_limit,
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/increment"))
            .and(header_exists("Authorization"))
            .and(body_json(&request))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock increment usage limit exceeded (still returns 200 but with updated limit showing 0 remaining)
    pub async fn mock_increment_usage_limit_exceeded(&self, limit: UserLimitMock) {
        let response = IncrementUsageResponseMock {
            success: true,
            data: limit,
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/increment"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 404 for increment usage (user not found)
    pub async fn mock_increment_usage_not_found(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "NOT_FOUND".to_string(),
                message: "User or limit not found".to_string(),
            },
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/increment"))
            .respond_with(ResponseTemplate::new(404).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 401 Unauthorized for increment usage
    pub async fn mock_increment_usage_unauthorized(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "UNAUTHORIZED".to_string(),
                message: "Invalid or expired authentication token".to_string(),
            },
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/increment"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 500 Internal Server Error for increment usage
    pub async fn mock_increment_usage_server_error(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "INTERNAL_ERROR".to_string(),
                message: "Failed to increment usage".to_string(),
            },
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/increment"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    // =========================================================================
    // GET /api/v1/users/me - User Profile
    // =========================================================================

    /// Mock successful user profile response
    pub async fn mock_get_user_profile_success(&self, profile: UserProfileMock) {
        let response = UserProfileResponseMock {
            success: true,
            data: profile,
        };

        Mock::given(method("GET"))
            .and(path("/api/v1/users/me"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 401 Unauthorized for user profile
    pub async fn mock_get_user_profile_unauthorized(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "UNAUTHORIZED".to_string(),
                message: "Invalid or expired authentication token".to_string(),
            },
        };

        Mock::given(method("GET"))
            .and(path("/api/v1/users/me"))
            .respond_with(ResponseTemplate::new(401).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 500 Internal Server Error for user profile
    pub async fn mock_get_user_profile_server_error(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "INTERNAL_ERROR".to_string(),
                message: "Failed to fetch user profile".to_string(),
            },
        };

        Mock::given(method("GET"))
            .and(path("/api/v1/users/me"))
            .respond_with(ResponseTemplate::new(500).set_body_json(&response))
            .mount(&self.server)
            .await;
    }
}

// =============================================================================
// Mock Data Types (matching Zion API response formats)
// =============================================================================

/// Reset period for limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResetPeriodMock {
    Daily,
    Weekly,
    Monthly,
    Never,
}

/// User limit mock data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLimitMock {
    pub limit_id: String,
    pub name: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub limit: i64,
    pub used: i64,
    pub remaining: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_period: Option<ResetPeriodMock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_end: Option<String>,
}

/// External limits response data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLimitsDataMock {
    pub user_id: String,
    pub external_id: String,
    pub limits: Vec<UserLimitMock>,
}

/// External limits response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLimitsResponseMock {
    pub success: bool,
    pub data: ExternalLimitsDataMock,
}

/// Increment usage request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageRequestMock {
    pub external_id: String,
    pub limit_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
}

/// Increment usage response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageResponseMock {
    pub success: bool,
    pub data: UserLimitMock,
}

/// User profile mock data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileMock {
    pub id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    pub email_verified: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
}

/// User profile response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileResponseMock {
    pub success: bool,
    pub data: UserProfileMock,
}

/// Error detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetailMock {
    pub code: String,
    pub message: String,
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponseMock {
    pub success: bool,
    pub error: ErrorDetailMock,
}

// =============================================================================
// Test Data Factories
// =============================================================================

/// Factory for creating test data
pub struct ZionTestData;

impl ZionTestData {
    /// Create a default user limit for AI input tokens
    pub fn input_tokens_limit(used: i64, limit: i64) -> UserLimitMock {
        UserLimitMock {
            limit_id: "lmt_input_tokens_001".to_string(),
            name: "ai_input_tokens".to_string(),
            display_name: "AI Input Tokens".to_string(),
            unit: Some("tokens".to_string()),
            limit,
            used,
            remaining: limit - used,
            reset_period: Some(ResetPeriodMock::Monthly),
            period_start: Some("2024-01-01T00:00:00Z".to_string()),
            period_end: Some("2024-01-31T23:59:59Z".to_string()),
        }
    }

    /// Create a default user limit for AI output tokens
    pub fn output_tokens_limit(used: i64, limit: i64) -> UserLimitMock {
        UserLimitMock {
            limit_id: "lmt_output_tokens_001".to_string(),
            name: "ai_output_tokens".to_string(),
            display_name: "AI Output Tokens".to_string(),
            unit: Some("tokens".to_string()),
            limit,
            used,
            remaining: limit - used,
            reset_period: Some(ResetPeriodMock::Monthly),
            period_start: Some("2024-01-01T00:00:00Z".to_string()),
            period_end: Some("2024-01-31T23:59:59Z".to_string()),
        }
    }

    /// Create a default user limit for API requests
    pub fn api_requests_limit(used: i64, limit: i64) -> UserLimitMock {
        UserLimitMock {
            limit_id: "lmt_api_requests_001".to_string(),
            name: "api_requests".to_string(),
            display_name: "API Requests".to_string(),
            unit: Some("requests".to_string()),
            limit,
            used,
            remaining: limit - used,
            reset_period: Some(ResetPeriodMock::Daily),
            period_start: Some("2024-01-15T00:00:00Z".to_string()),
            period_end: Some("2024-01-15T23:59:59Z".to_string()),
        }
    }

    /// Create default limits for a typical free tier user
    pub fn free_tier_limits() -> Vec<UserLimitMock> {
        vec![
            Self::input_tokens_limit(5000, 50000),
            Self::output_tokens_limit(2000, 20000),
            Self::api_requests_limit(50, 100),
        ]
    }

    /// Create default limits for a typical pro tier user
    pub fn pro_tier_limits() -> Vec<UserLimitMock> {
        vec![
            Self::input_tokens_limit(100000, 1000000),
            Self::output_tokens_limit(50000, 500000),
            Self::api_requests_limit(500, 10000),
        ]
    }

    /// Create limits where tokens are almost exhausted
    pub fn nearly_exhausted_limits() -> Vec<UserLimitMock> {
        vec![
            Self::input_tokens_limit(49900, 50000),
            Self::output_tokens_limit(19900, 20000),
            Self::api_requests_limit(99, 100),
        ]
    }

    /// Create limits that are completely exhausted
    pub fn exhausted_limits() -> Vec<UserLimitMock> {
        vec![
            Self::input_tokens_limit(50000, 50000),
            Self::output_tokens_limit(20000, 20000),
            Self::api_requests_limit(100, 100),
        ]
    }

    /// Create a default user profile
    pub fn default_profile(external_id: &str) -> UserProfileMock {
        UserProfileMock {
            id: format!("usr_{}", external_id),
            email: format!("{}@example.com", external_id),
            name: Some("Test User".to_string()),
            external_id: Some(external_id.to_string()),
            email_verified: true,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            last_login_at: Some("2024-01-15T12:00:00Z".to_string()),
        }
    }

    /// Create a profile for an unverified user
    pub fn unverified_profile(external_id: &str) -> UserProfileMock {
        UserProfileMock {
            id: format!("usr_{}", external_id),
            email: format!("{}@example.com", external_id),
            name: None,
            external_id: Some(external_id.to_string()),
            email_verified: false,
            created_at: "2024-01-15T00:00:00Z".to_string(),
            last_login_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_starts() {
        let mock = MockZionServer::start().await;
        assert!(!mock.uri().is_empty());
    }

    #[tokio::test]
    async fn test_mock_get_limits_success() {
        let mock = MockZionServer::start().await;
        mock.mock_get_limits_success("user123", ZionTestData::free_tier_limits())
            .await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/api/v1/limits/external/user123", mock.uri()))
            .header("Authorization", "Bearer test-token")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: ExternalLimitsResponseMock = response.json().await.unwrap();
        assert!(body.success);
        assert_eq!(body.data.external_id, "user123");
        assert_eq!(body.data.limits.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_get_limits_not_found() {
        let mock = MockZionServer::start().await;
        mock.mock_get_limits_not_found("nonexistent").await;

        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/api/v1/limits/external/nonexistent",
                mock.uri()
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn test_test_data_factories() {
        let free_limits = ZionTestData::free_tier_limits();
        assert_eq!(free_limits.len(), 3);
        assert_eq!(free_limits[0].name, "ai_input_tokens");

        let exhausted = ZionTestData::exhausted_limits();
        assert!(exhausted.iter().all(|l| l.remaining == 0));

        let profile = ZionTestData::default_profile("test123");
        assert_eq!(profile.external_id, Some("test123".to_string()));
    }
}
