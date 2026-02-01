//! Mock Zion API server for testing
//!
//! Provides wiremock-based mocks for the Zion API endpoints:
//! - GET /api/v1/limits/external/{id} - Get user limits
//! - POST /api/v1/usage/external/increment - Increment usage (unified format)
//! - POST /api/v1/usage/external/batch-increment - Batch increment usage
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
//!     // Set up successful limits response with unified ai_usage limit
//!     mock_server.mock_get_limits_success("user123", ZionTestData::default_limits()).await;
//!
//!     // Use mock_server.uri() as the Zion API URL
//!     // ...
//! }
//! ```

use serde::{Deserialize, Serialize};
use wiremock::{
    matchers::{header, header_exists, method, path, path_regex},
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

    /// Get all received requests (for assertion in tests)
    ///
    /// Use this to verify what requests were actually sent to the mock.
    pub async fn received_requests(&self) -> Vec<wiremock::Request> {
        self.server.received_requests().await.unwrap_or_default()
    }

    /// Get only batch-increment requests from all received requests
    pub async fn batch_increment_requests(&self) -> Vec<wiremock::Request> {
        self.received_requests()
            .await
            .into_iter()
            .filter(|r| r.url.path() == "/api/v1/usage/external/batch-increment")
            .collect()
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

    /// Mock successful increment usage response (unified format)
    pub async fn mock_increment_usage_success(&self, updated_limit: UserLimitMock) {
        let response = IncrementUsageResponseMock {
            success: true,
            data: updated_limit,
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/increment"))
            .and(header_exists("x-api-key"))
            .and(header("Content-Type", "application/json"))
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
    // POST /api/v1/usage/external/batch-increment - Batch Increment Usage
    // =========================================================================

    /// Mock successful batch increment response
    pub async fn mock_batch_increment_success(&self, processed: i32, failed: i32) {
        let response = BatchIncrementResponseMock {
            success: true,
            data: BatchIncrementDataMock {
                processed,
                failed,
                results: vec![],
            },
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/batch-increment"))
            .and(header_exists("x-api-key"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock batch increment with partial failures
    pub async fn mock_batch_increment_partial_failure(
        &self,
        processed: i32,
        failed: i32,
        failed_emails: Vec<&str>,
    ) {
        let results: Vec<BatchIncrementResultMock> = failed_emails
            .into_iter()
            .map(|email| BatchIncrementResultMock {
                email: email.to_string(),
                success: false,
                error: Some("Failed to increment".to_string()),
            })
            .collect();

        let response = BatchIncrementResponseMock {
            success: true,
            data: BatchIncrementDataMock {
                processed,
                failed,
                results,
            },
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/batch-increment"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 500 Internal Server Error for batch increment
    pub async fn mock_batch_increment_server_error(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "INTERNAL_ERROR".to_string(),
                message: "Failed to process batch increment".to_string(),
            },
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/usage/external/batch-increment"))
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

    // =========================================================================
    // GET /api/v1/tiers/config - Tier Configuration
    // =========================================================================

    /// Mock successful tier config response with default configuration
    pub async fn mock_tier_config_success(&self) {
        let response = TierConfigResponseMock {
            success: true,
            data: ZionTestData::default_tier_config(),
        };

        Mock::given(method("GET"))
            .and(path("/api/v1/tiers/config"))
            .and(header_exists("x-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock successful tier config response with custom configuration
    pub async fn mock_tier_config_success_with(&self, config: TierConfigDataMock) {
        let response = TierConfigResponseMock {
            success: true,
            data: config,
        };

        Mock::given(method("GET"))
            .and(path("/api/v1/tiers/config"))
            .and(header_exists("x-api-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .mount(&self.server)
            .await;
    }

    /// Mock 500 error for tier config
    pub async fn mock_tier_config_server_error(&self) {
        let response = ErrorResponseMock {
            success: false,
            error: ErrorDetailMock {
                code: "INTERNAL_ERROR".to_string(),
                message: "Failed to fetch tier config".to_string(),
            },
        };

        Mock::given(method("GET"))
            .and(path("/api/v1/tiers/config"))
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

/// Limit metric mock data (for unified limit structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitMetricMock {
    pub limit: i64,
    pub used: i64,
    pub remaining: i64,
}

impl LimitMetricMock {
    pub fn new(limit: i64, used: i64) -> Self {
        Self {
            limit,
            used,
            remaining: limit - used,
        }
    }
}

/// User limit mock data (unified structure with embedded metrics)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLimitMock {
    pub name: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub ai_input_tokens: LimitMetricMock,
    pub ai_output_tokens: LimitMetricMock,
    pub ai_requests: LimitMetricMock,
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

/// Increment usage request (unified format)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageRequestMock {
    pub email: String,
    pub limit_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_requests: Option<i64>,
}

/// Increment usage response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageResponseMock {
    pub success: bool,
    pub data: UserLimitMock,
}

/// Batch increment item
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementItemMock {
    pub email: String,
    pub limit_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_requests: Option<i64>,
}

/// Batch increment request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementRequestMock {
    pub increments: Vec<BatchIncrementItemMock>,
}

/// Batch increment result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementResultMock {
    pub email: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Batch increment data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementDataMock {
    pub processed: i32,
    pub failed: i32,
    pub results: Vec<BatchIncrementResultMock>,
}

/// Batch increment response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementResponseMock {
    pub success: bool,
    pub data: BatchIncrementDataMock,
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
// Tier Configuration Mock Types
// =============================================================================

/// Model configuration for a tier
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigMock {
    pub provider: String,
    pub model: String,
    pub relative_cost: u8,
    pub input_price_per_million: f64,
    pub output_price_per_million: f64,
}

/// Tier-to-model mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierMappingMock {
    pub simple: Vec<ModelConfigMock>,
    pub moderate: Vec<ModelConfigMock>,
    pub complex: Vec<ModelConfigMock>,
}

/// Tier configuration data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierConfigDataMock {
    pub version: String,
    pub updated_at: String,
    pub tiers: TierMappingMock,
}

/// Tier configuration response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierConfigResponseMock {
    pub success: bool,
    pub data: TierConfigDataMock,
}

// =============================================================================
// Test Data Factories
// =============================================================================

/// Factory for creating test data
pub struct ZionTestData;

impl ZionTestData {
    /// Create a unified AI usage limit with custom values
    pub fn ai_usage_limit(
        input_used: i64,
        input_limit: i64,
        output_used: i64,
        output_limit: i64,
        requests_used: i64,
        requests_limit: i64,
    ) -> UserLimitMock {
        UserLimitMock {
            name: "ai_usage".to_string(),
            display_name: "AI Usage".to_string(),
            description: Some("AI usage limits for input tokens, output tokens, and requests".to_string()),
            unit: None,
            ai_input_tokens: LimitMetricMock::new(input_limit, input_used),
            ai_output_tokens: LimitMetricMock::new(output_limit, output_used),
            ai_requests: LimitMetricMock::new(requests_limit, requests_used),
            reset_period: Some(ResetPeriodMock::Monthly),
            period_start: Some("2024-01-01T00:00:00Z".to_string()),
            period_end: Some("2024-01-31T23:59:59Z".to_string()),
        }
    }

    /// Create default limits for a typical free tier user (single unified limit)
    pub fn free_tier_limits() -> Vec<UserLimitMock> {
        vec![Self::ai_usage_limit(
            5000,   // input_used
            50000,  // input_limit
            2000,   // output_used
            20000,  // output_limit
            50,     // requests_used
            100,    // requests_limit
        )]
    }

    /// Create default limits for a typical pro tier user (single unified limit)
    pub fn pro_tier_limits() -> Vec<UserLimitMock> {
        vec![Self::ai_usage_limit(
            100000,   // input_used
            1000000,  // input_limit
            50000,    // output_used
            500000,   // output_limit
            500,      // requests_used
            10000,    // requests_limit
        )]
    }

    /// Create limits where tokens are almost exhausted
    pub fn nearly_exhausted_limits() -> Vec<UserLimitMock> {
        vec![Self::ai_usage_limit(
            49900,  // input_used
            50000,  // input_limit
            19900,  // output_used
            20000,  // output_limit
            99,     // requests_used
            100,    // requests_limit
        )]
    }

    /// Create limits that are completely exhausted
    pub fn exhausted_limits() -> Vec<UserLimitMock> {
        vec![Self::ai_usage_limit(
            50000,  // input_used
            50000,  // input_limit
            20000,  // output_used
            20000,  // output_limit
            100,    // requests_used
            100,    // requests_limit
        )]
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

    /// Create default tier configuration for testing
    ///
    /// Provides a realistic tier config with:
    /// - Simple: gpt-4o-mini (cost 1)
    /// - Moderate: gpt-4o (cost 5)
    /// - Complex: gpt-4o (cost 5)
    pub fn default_tier_config() -> TierConfigDataMock {
        TierConfigDataMock {
            version: "1.0.0".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            tiers: TierMappingMock {
                simple: vec![ModelConfigMock {
                    provider: "openai".to_string(),
                    model: "gpt-4o-mini".to_string(),
                    relative_cost: 1,
                    input_price_per_million: 0.15,
                    output_price_per_million: 0.60,
                }],
                moderate: vec![ModelConfigMock {
                    provider: "openai".to_string(),
                    model: "gpt-4o".to_string(),
                    relative_cost: 5,
                    input_price_per_million: 2.50,
                    output_price_per_million: 10.0,
                }],
                complex: vec![ModelConfigMock {
                    provider: "openai".to_string(),
                    model: "gpt-4o".to_string(),
                    relative_cost: 5,
                    input_price_per_million: 2.50,
                    output_price_per_million: 10.0,
                }],
            },
        }
    }

    /// Create custom tier config with specified models
    pub fn tier_config_with(
        simple_model: &str,
        moderate_model: &str,
        complex_model: &str,
    ) -> TierConfigDataMock {
        TierConfigDataMock {
            version: "1.0.0".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            tiers: TierMappingMock {
                simple: vec![ModelConfigMock {
                    provider: "openai".to_string(),
                    model: simple_model.to_string(),
                    relative_cost: 1,
                    input_price_per_million: 0.15,
                    output_price_per_million: 0.60,
                }],
                moderate: vec![ModelConfigMock {
                    provider: "openai".to_string(),
                    model: moderate_model.to_string(),
                    relative_cost: 5,
                    input_price_per_million: 2.50,
                    output_price_per_million: 10.0,
                }],
                complex: vec![ModelConfigMock {
                    provider: "openai".to_string(),
                    model: complex_model.to_string(),
                    relative_cost: 5,
                    input_price_per_million: 2.50,
                    output_price_per_million: 10.0,
                }],
            },
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
        // Now we have a single unified ai_usage limit
        assert_eq!(body.data.limits.len(), 1);
        assert_eq!(body.data.limits[0].name, "ai_usage");
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
        // Now we have a single unified limit
        assert_eq!(free_limits.len(), 1);
        assert_eq!(free_limits[0].name, "ai_usage");
        // Verify the embedded metrics
        assert_eq!(free_limits[0].ai_input_tokens.limit, 50000);
        assert_eq!(free_limits[0].ai_output_tokens.limit, 20000);
        assert_eq!(free_limits[0].ai_requests.limit, 100);

        let exhausted = ZionTestData::exhausted_limits();
        assert_eq!(exhausted.len(), 1);
        assert_eq!(exhausted[0].ai_input_tokens.remaining, 0);
        assert_eq!(exhausted[0].ai_output_tokens.remaining, 0);
        assert_eq!(exhausted[0].ai_requests.remaining, 0);

        let profile = ZionTestData::default_profile("test123");
        assert_eq!(profile.external_id, Some("test123".to_string()));
    }

    #[test]
    fn test_limit_metric_mock() {
        let metric = LimitMetricMock::new(1000, 250);
        assert_eq!(metric.limit, 1000);
        assert_eq!(metric.used, 250);
        assert_eq!(metric.remaining, 750);
    }

    #[test]
    fn test_ai_usage_limit_factory() {
        let limit = ZionTestData::ai_usage_limit(100, 1000, 50, 500, 10, 100);
        assert_eq!(limit.name, "ai_usage");
        assert_eq!(limit.ai_input_tokens.used, 100);
        assert_eq!(limit.ai_input_tokens.limit, 1000);
        assert_eq!(limit.ai_input_tokens.remaining, 900);
        assert_eq!(limit.ai_output_tokens.used, 50);
        assert_eq!(limit.ai_output_tokens.limit, 500);
        assert_eq!(limit.ai_output_tokens.remaining, 450);
        assert_eq!(limit.ai_requests.used, 10);
        assert_eq!(limit.ai_requests.limit, 100);
        assert_eq!(limit.ai_requests.remaining, 90);
    }
}
