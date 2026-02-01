//! Zion API data models
//!
//! Data structures for Zion API requests and responses.

use serde::{Deserialize, Serialize};

/// Reset period for limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResetPeriod {
    Daily,
    Weekly,
    Monthly,
    Never,
}

/// A single metric within a unified limit (e.g., aiInputTokens, aiOutputTokens, aiRequests)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LimitMetric {
    pub limit: i64,
    pub used: i64,
    pub remaining: i64,
}

/// User limit information from Zion (unified structure with embedded metrics)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLimit {
    pub name: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub ai_input_tokens: LimitMetric,
    pub ai_output_tokens: LimitMetric,
    pub ai_requests: LimitMetric,
    pub reset_period: Option<ResetPeriod>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
}

/// Response from external limits endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLimitsResponse {
    pub success: bool,
    pub data: ExternalLimitsData,
}

/// Data in external limits response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalLimitsData {
    pub user_id: String,
    pub external_id: String,
    pub limits: Vec<UserLimit>,
}

/// Request to increment usage (unified format with all 3 metrics)
/// Note: limit_name is not sent - it's auto-detected from user's subscription plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageRequest {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_requests: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,      // AI model name (e.g., "gpt-4o")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,  // ISO 8601 UTC timestamp
}

/// Response data from single increment endpoint
/// Note: Different from UserLimit - includes canUse, excludes name/displayName/resetPeriod
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageData {
    pub can_use: bool,
    pub ai_input_tokens: LimitMetric,
    pub ai_output_tokens: LimitMetric,
    pub ai_requests: LimitMetric,
}

/// Response from increment usage endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageResponse {
    pub success: bool,
    pub data: IncrementUsageData,
}

/// Single item in a batch increment request
/// Note: limit_name is not sent - it's auto-detected from user's subscription plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementItem {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_requests: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,      // AI model name (e.g., "gpt-4o")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,  // ISO 8601 UTC timestamp
}

/// Batch increment request (up to 1000 items)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementRequest {
    pub increments: Vec<BatchIncrementItem>,
}

/// Result for a single item in batch increment response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementResult {
    pub email: String,
    pub limit_name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_input_tokens: Option<BatchIncrementMetricResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_output_tokens: Option<BatchIncrementMetricResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_requests: Option<BatchIncrementMetricResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Metric result in batch increment response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementMetricResult {
    pub new_value: i64,
    pub limit: i64,
}

/// Response from batch increment endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementResponse {
    pub success: bool,
    pub data: BatchIncrementData,
}

/// Data in batch increment response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIncrementData {
    pub processed: i32,
    pub failed: i32,
    pub results: Vec<BatchIncrementResult>,
}

/// User profile from /api/v1/users/me
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub external_id: Option<String>,
    pub email_verified: bool,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

/// Response from user profile endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileResponse {
    pub success: bool,
    pub data: UserProfile,
}

// ===========================================
// Tier Configuration Types
// ===========================================

/// Model configuration for a single provider/model combination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Model identifier (e.g., "gpt-4o-mini", "gpt-4o")
    pub model: String,
    /// Relative cost score (1-10, lower is cheaper)
    /// Used for weighted selection - lower cost = higher probability
    /// Must be >= 1 to avoid division by zero in weight calculation
    pub relative_cost: u8,
    /// Input token price per million (for cost reporting)
    pub input_price_per_million: f64,
    /// Output token price per million (for cost reporting)
    pub output_price_per_million: f64,
}

/// Tier-to-model mapping configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TierMapping {
    /// Models available for simple tier
    pub simple: Vec<ModelConfig>,
    /// Models available for moderate tier
    pub moderate: Vec<ModelConfig>,
    /// Models available for complex tier
    pub complex: Vec<ModelConfig>,
}

/// Tier configuration data from Zion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TierConfigData {
    /// Config version for cache invalidation
    pub version: String,
    /// When this config was last updated
    pub updated_at: String,
    /// Tier-to-model mappings
    pub tiers: TierMapping,
}

/// Response wrapper from tier config endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierConfigResponse {
    pub success: bool,
    pub data: TierConfigData,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // ResetPeriod Serialization Tests
    // ===========================================

    #[test]
    fn test_reset_period_daily_serialization() {
        let period = ResetPeriod::Daily;
        let serialized = serde_json::to_string(&period).unwrap();
        assert_eq!(serialized, "\"DAILY\"");

        let deserialized: ResetPeriod = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, ResetPeriod::Daily);
    }

    #[test]
    fn test_reset_period_weekly_serialization() {
        let period = ResetPeriod::Weekly;
        let serialized = serde_json::to_string(&period).unwrap();
        assert_eq!(serialized, "\"WEEKLY\"");

        let deserialized: ResetPeriod = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, ResetPeriod::Weekly);
    }

    #[test]
    fn test_reset_period_monthly_serialization() {
        let period = ResetPeriod::Monthly;
        let serialized = serde_json::to_string(&period).unwrap();
        assert_eq!(serialized, "\"MONTHLY\"");

        let deserialized: ResetPeriod = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, ResetPeriod::Monthly);
    }

    #[test]
    fn test_reset_period_never_serialization() {
        let period = ResetPeriod::Never;
        let serialized = serde_json::to_string(&period).unwrap();
        assert_eq!(serialized, "\"NEVER\"");

        let deserialized: ResetPeriod = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, ResetPeriod::Never);
    }

    #[test]
    fn test_reset_period_all_variants() {
        let variants = vec![
            (ResetPeriod::Daily, "\"DAILY\""),
            (ResetPeriod::Weekly, "\"WEEKLY\""),
            (ResetPeriod::Monthly, "\"MONTHLY\""),
            (ResetPeriod::Never, "\"NEVER\""),
        ];

        for (period, expected_json) in variants {
            let serialized = serde_json::to_string(&period).unwrap();
            assert_eq!(serialized, expected_json, "Failed for {:?}", period);
        }
    }

    #[test]
    fn test_reset_period_clone() {
        let period = ResetPeriod::Monthly;
        let cloned = period.clone();
        assert_eq!(period, cloned);
    }

    #[test]
    fn test_reset_period_debug() {
        let period = ResetPeriod::Daily;
        let debug_str = format!("{:?}", period);
        assert_eq!(debug_str, "Daily");
    }

    // ===========================================
    // LimitMetric Tests
    // ===========================================

    #[test]
    fn test_limit_metric_deserialize() {
        let json = r#"{"limit": 10000, "used": 500, "remaining": 9500}"#;
        let metric: LimitMetric = serde_json::from_str(json).unwrap();
        assert_eq!(metric.limit, 10000);
        assert_eq!(metric.used, 500);
        assert_eq!(metric.remaining, 9500);
    }

    #[test]
    fn test_limit_metric_serialize() {
        let metric = LimitMetric {
            limit: 1000,
            used: 100,
            remaining: 900,
        };
        let json = serde_json::to_string(&metric).unwrap();
        assert!(json.contains("\"limit\":1000"));
        assert!(json.contains("\"used\":100"));
        assert!(json.contains("\"remaining\":900"));
    }

    #[test]
    fn test_limit_metric_negative_remaining() {
        let json = r#"{"limit": 100, "used": 150, "remaining": -50}"#;
        let metric: LimitMetric = serde_json::from_str(json).unwrap();
        assert_eq!(metric.remaining, -50);
    }

    // ===========================================
    // UserLimit Tests (Unified Structure)
    // ===========================================

    #[test]
    fn test_deserialize_user_limit() {
        let json = r#"{
            "name": "ai_usage",
            "displayName": "AI Usage",
            "unit": "tokens",
            "aiInputTokens": {"limit": 100000, "used": 1000, "remaining": 99000},
            "aiOutputTokens": {"limit": 50000, "used": 500, "remaining": 49500},
            "aiRequests": {"limit": 1000, "used": 10, "remaining": 990},
            "resetPeriod": "MONTHLY",
            "periodStart": "2024-01-01T00:00:00Z",
            "periodEnd": "2024-01-31T23:59:59Z"
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.name, "ai_usage");
        assert_eq!(limit.display_name, "AI Usage");
        assert_eq!(limit.ai_input_tokens.limit, 100000);
        assert_eq!(limit.ai_input_tokens.used, 1000);
        assert_eq!(limit.ai_output_tokens.limit, 50000);
        assert_eq!(limit.ai_requests.limit, 1000);
        assert_eq!(limit.reset_period, Some(ResetPeriod::Monthly));
    }

    #[test]
    fn test_deserialize_user_limit_minimal() {
        let json = r#"{
            "name": "ai_usage",
            "displayName": "AI Usage",
            "aiInputTokens": {"limit": 100, "used": 0, "remaining": 100},
            "aiOutputTokens": {"limit": 50, "used": 0, "remaining": 50},
            "aiRequests": {"limit": 10, "used": 0, "remaining": 10},
            "resetPeriod": null,
            "periodStart": null,
            "periodEnd": null
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.name, "ai_usage");
        assert!(limit.unit.is_none());
        assert!(limit.description.is_none());
        assert!(limit.reset_period.is_none());
    }

    #[test]
    fn test_deserialize_user_limit_all_reset_periods() {
        let periods = vec![
            ("DAILY", ResetPeriod::Daily),
            ("WEEKLY", ResetPeriod::Weekly),
            ("MONTHLY", ResetPeriod::Monthly),
            ("NEVER", ResetPeriod::Never),
        ];

        for (json_period, expected) in periods {
            let json = format!(r#"{{
                "name": "ai_usage",
                "displayName": "AI Usage",
                "aiInputTokens": {{"limit": 100, "used": 0, "remaining": 100}},
                "aiOutputTokens": {{"limit": 50, "used": 0, "remaining": 50}},
                "aiRequests": {{"limit": 10, "used": 0, "remaining": 10}},
                "resetPeriod": "{}",
                "periodStart": null,
                "periodEnd": null
            }}"#, json_period);

            let limit: UserLimit = serde_json::from_str(&json).unwrap();
            assert_eq!(limit.reset_period, Some(expected), "Failed for {}", json_period);
        }
    }

    #[test]
    fn test_serialize_user_limit() {
        let limit = UserLimit {
            name: "ai_usage".to_string(),
            display_name: "AI Usage".to_string(),
            description: None,
            unit: Some("tokens".to_string()),
            ai_input_tokens: LimitMetric { limit: 1000, used: 100, remaining: 900 },
            ai_output_tokens: LimitMetric { limit: 500, used: 50, remaining: 450 },
            ai_requests: LimitMetric { limit: 100, used: 10, remaining: 90 },
            reset_period: Some(ResetPeriod::Daily),
            period_start: Some("2024-01-01T00:00:00Z".to_string()),
            period_end: Some("2024-01-01T23:59:59Z".to_string()),
        };

        let json = serde_json::to_string(&limit).unwrap();
        assert!(json.contains("\"name\":\"ai_usage\""));
        assert!(json.contains("\"displayName\":\"AI Usage\""));
        assert!(json.contains("\"resetPeriod\":\"DAILY\""));
        assert!(json.contains("\"aiInputTokens\""));
        assert!(json.contains("\"aiOutputTokens\""));
        assert!(json.contains("\"aiRequests\""));
    }

    #[test]
    fn test_user_limit_clone() {
        let limit = UserLimit {
            name: "ai_usage".to_string(),
            display_name: "AI Usage".to_string(),
            description: None,
            unit: None,
            ai_input_tokens: LimitMetric { limit: 100, used: 50, remaining: 50 },
            ai_output_tokens: LimitMetric { limit: 50, used: 25, remaining: 25 },
            ai_requests: LimitMetric { limit: 10, used: 5, remaining: 5 },
            reset_period: None,
            period_start: None,
            period_end: None,
        };

        let cloned = limit.clone();
        assert_eq!(limit.name, cloned.name);
        assert_eq!(limit.ai_input_tokens.limit, cloned.ai_input_tokens.limit);
    }

    #[test]
    fn test_user_limit_negative_remaining() {
        let json = r#"{
            "name": "ai_usage",
            "displayName": "AI Usage",
            "aiInputTokens": {"limit": 100, "used": 150, "remaining": -50},
            "aiOutputTokens": {"limit": 50, "used": 0, "remaining": 50},
            "aiRequests": {"limit": 10, "used": 0, "remaining": 10},
            "resetPeriod": null,
            "periodStart": null,
            "periodEnd": null
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.ai_input_tokens.used, 150);
        assert_eq!(limit.ai_input_tokens.remaining, -50);
    }

    // ===========================================
    // IncrementUsageRequest Tests (Unified Format)
    // ===========================================

    #[test]
    fn test_serialize_increment_request() {
        let request = IncrementUsageRequest {
            email: "user123@example.com".to_string(),
            ai_input_tokens: Some(100),
            ai_output_tokens: Some(50),
            ai_requests: Some(1),
            model: None,
            timestamp: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"email\":\"user123@example.com\""));
        // limitName is no longer sent - auto-detected by API
        assert!(!json.contains("limitName"));
        assert!(json.contains("\"aiInputTokens\":100"));
        assert!(json.contains("\"aiOutputTokens\":50"));
        assert!(json.contains("\"aiRequests\":1"));
    }

    #[test]
    fn test_serialize_increment_request_partial() {
        let request = IncrementUsageRequest {
            email: "user123@example.com".to_string(),
            ai_input_tokens: Some(100),
            ai_output_tokens: None,
            ai_requests: None,
            model: None,
            timestamp: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"aiInputTokens\":100"));
        assert!(!json.contains("aiOutputTokens"));
        assert!(!json.contains("aiRequests"));
    }

    #[test]
    fn test_deserialize_increment_request() {
        let json = r#"{
            "email": "user@example.com",
            "aiInputTokens": 1000,
            "aiOutputTokens": 500,
            "aiRequests": 1
        }"#;

        let request: IncrementUsageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.email, "user@example.com");
        assert_eq!(request.ai_input_tokens, Some(1000));
        assert_eq!(request.ai_output_tokens, Some(500));
        assert_eq!(request.ai_requests, Some(1));
    }

    #[test]
    fn test_increment_request_clone() {
        let request = IncrementUsageRequest {
            email: "user123@example.com".to_string(),
            ai_input_tokens: Some(100),
            ai_output_tokens: Some(50),
            ai_requests: Some(1),
            model: Some("gpt-4o".to_string()),
            timestamp: Some("2024-01-15T10:30:00Z".to_string()),
        };

        let cloned = request.clone();
        assert_eq!(request.email, cloned.email);
        assert_eq!(request.ai_input_tokens, cloned.ai_input_tokens);
        assert_eq!(request.model, cloned.model);
        assert_eq!(request.timestamp, cloned.timestamp);
    }

    #[test]
    fn test_serialize_increment_request_with_model_and_timestamp() {
        let request = IncrementUsageRequest {
            email: "user123@example.com".to_string(),
            ai_input_tokens: Some(100),
            ai_output_tokens: Some(50),
            ai_requests: Some(1),
            model: Some("gpt-4o".to_string()),
            timestamp: Some("2024-01-15T10:30:00Z".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"email\":\"user123@example.com\""));
        assert!(json.contains("\"aiInputTokens\":100"));
        assert!(json.contains("\"aiOutputTokens\":50"));
        assert!(json.contains("\"aiRequests\":1"));
        assert!(json.contains("\"model\":\"gpt-4o\""));
        assert!(json.contains("\"timestamp\":\"2024-01-15T10:30:00Z\""));
    }

    #[test]
    fn test_deserialize_increment_request_with_model_and_timestamp() {
        let json = r#"{
            "email": "user@example.com",
            "aiInputTokens": 1000,
            "aiOutputTokens": 500,
            "aiRequests": 1,
            "model": "gpt-4o",
            "timestamp": "2024-01-15T10:30:00Z"
        }"#;

        let request: IncrementUsageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.email, "user@example.com");
        assert_eq!(request.ai_input_tokens, Some(1000));
        assert_eq!(request.ai_output_tokens, Some(500));
        assert_eq!(request.ai_requests, Some(1));
        assert_eq!(request.model, Some("gpt-4o".to_string()));
        assert_eq!(request.timestamp, Some("2024-01-15T10:30:00Z".to_string()));
    }

    // ===========================================
    // BatchIncrementItem Tests
    // ===========================================

    #[test]
    fn test_batch_increment_item_serialize() {
        let item = BatchIncrementItem {
            email: "user123@example.com".to_string(),
            ai_input_tokens: Some(1000),
            ai_output_tokens: Some(500),
            ai_requests: Some(1),
            model: None,
            timestamp: None,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"email\":\"user123@example.com\""));
        // limitName is no longer sent - auto-detected by API
        assert!(!json.contains("limitName"));
        assert!(json.contains("\"aiInputTokens\":1000"));
    }

    #[test]
    fn test_batch_increment_request_serialize() {
        let request = BatchIncrementRequest {
            increments: vec![
                BatchIncrementItem {
                    email: "user1@example.com".to_string(),
                    ai_input_tokens: Some(1000),
                    ai_output_tokens: Some(500),
                    ai_requests: Some(1),
                    model: Some("gpt-4o".to_string()),
                    timestamp: Some("2024-01-15T10:30:00Z".to_string()),
                },
                BatchIncrementItem {
                    email: "user2@example.com".to_string(),
                    ai_input_tokens: Some(2000),
                    ai_output_tokens: None,
                    ai_requests: Some(1),
                    model: None,
                    timestamp: None,
                },
            ],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"increments\""));
        assert!(json.contains("\"user1@example.com\""));
        assert!(json.contains("\"user2@example.com\""));
        // limitName is no longer sent
        assert!(!json.contains("limitName"));
        // Verify token data is included with correct camelCase names
        assert!(json.contains("\"aiInputTokens\":1000"), "aiInputTokens should be present");
        assert!(json.contains("\"aiOutputTokens\":500"), "aiOutputTokens should be present");
        assert!(json.contains("\"aiRequests\":1"), "aiRequests should be present");
        // Verify model and timestamp are included for first item
        assert!(json.contains("\"model\":\"gpt-4o\""), "model should be present");
        assert!(json.contains("\"timestamp\":\"2024-01-15T10:30:00Z\""), "timestamp should be present");
    }

    #[test]
    fn test_batch_increment_response_deserialize() {
        let json = r#"{
            "success": true,
            "data": {
                "processed": 2,
                "failed": 1,
                "results": [
                    {
                        "email": "user1@example.com",
                        "limitName": "ai_usage",
                        "success": true,
                        "aiInputTokens": {"newValue": 1100, "limit": 100000},
                        "aiOutputTokens": {"newValue": 550, "limit": 50000},
                        "aiRequests": {"newValue": 11, "limit": 1000}
                    },
                    {
                        "email": "user2@example.com",
                        "limitName": "ai_usage",
                        "success": false,
                        "error": "User not found"
                    }
                ]
            }
        }"#;

        let response: BatchIncrementResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data.processed, 2);
        assert_eq!(response.data.failed, 1);
        assert_eq!(response.data.results.len(), 2);
        assert!(response.data.results[0].success);
        assert!(!response.data.results[1].success);
        assert_eq!(response.data.results[1].error, Some("User not found".to_string()));
    }

    // ===========================================
    // ExternalLimitsResponse Tests (Unified Structure)
    // ===========================================

    #[test]
    fn test_deserialize_external_limits_response() {
        let json = r#"{
            "success": true,
            "data": {
                "userId": "user_123",
                "externalId": "ext_456",
                "limits": [
                    {
                        "name": "ai_usage",
                        "displayName": "AI Usage",
                        "unit": "tokens",
                        "aiInputTokens": {"limit": 100000, "used": 1000, "remaining": 99000},
                        "aiOutputTokens": {"limit": 50000, "used": 500, "remaining": 49500},
                        "aiRequests": {"limit": 1000, "used": 10, "remaining": 990},
                        "resetPeriod": "MONTHLY",
                        "periodStart": "2024-01-01T00:00:00Z",
                        "periodEnd": "2024-01-31T23:59:59Z"
                    }
                ]
            }
        }"#;

        let response: ExternalLimitsResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data.user_id, "user_123");
        assert_eq!(response.data.external_id, "ext_456");
        assert_eq!(response.data.limits.len(), 1);
        assert_eq!(response.data.limits[0].name, "ai_usage");
        assert_eq!(response.data.limits[0].ai_input_tokens.limit, 100000);
        assert_eq!(response.data.limits[0].ai_output_tokens.limit, 50000);
        assert_eq!(response.data.limits[0].ai_requests.limit, 1000);
    }

    #[test]
    fn test_deserialize_external_limits_response_empty_limits() {
        let json = r#"{
            "success": true,
            "data": {
                "userId": "user_123",
                "externalId": "ext_456",
                "limits": []
            }
        }"#;

        let response: ExternalLimitsResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert!(response.data.limits.is_empty());
    }

    #[test]
    fn test_deserialize_external_limits_response_with_description() {
        let json = r#"{
            "success": true,
            "data": {
                "userId": "user_123",
                "externalId": "ext_456",
                "limits": [
                    {
                        "name": "ai_usage",
                        "displayName": "AI Usage",
                        "description": "AI token and request limits",
                        "unit": "tokens/requests",
                        "aiInputTokens": {"limit": 100000, "used": 0, "remaining": 100000},
                        "aiOutputTokens": {"limit": 50000, "used": 0, "remaining": 50000},
                        "aiRequests": {"limit": 1000, "used": 0, "remaining": 1000},
                        "resetPeriod": "MONTHLY",
                        "periodStart": null,
                        "periodEnd": null
                    }
                ]
            }
        }"#;

        let response: ExternalLimitsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.limits.len(), 1);
        assert_eq!(response.data.limits[0].description, Some("AI token and request limits".to_string()));
    }

    // ===========================================
    // IncrementUsageResponse Tests (IncrementUsageData)
    // ===========================================

    #[test]
    fn test_deserialize_increment_usage_response() {
        // Actual response format from Zion API
        let json = r#"{
            "success": true,
            "data": {
                "canUse": true,
                "aiInputTokens": {"limit": 100000, "used": 1100, "remaining": 98900},
                "aiOutputTokens": {"limit": 50000, "used": 550, "remaining": 49450},
                "aiRequests": {"limit": 1000, "used": 11, "remaining": 989}
            }
        }"#;

        let response: IncrementUsageResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert!(response.data.can_use);
        assert_eq!(response.data.ai_input_tokens.used, 1100);
        assert_eq!(response.data.ai_input_tokens.remaining, 98900);
        assert_eq!(response.data.ai_output_tokens.used, 550);
        assert_eq!(response.data.ai_requests.used, 11);
    }

    #[test]
    fn test_deserialize_increment_usage_response_limit_exceeded() {
        let json = r#"{
            "success": true,
            "data": {
                "canUse": false,
                "aiInputTokens": {"limit": 1000, "used": 1000, "remaining": 0},
                "aiOutputTokens": {"limit": 500, "used": 500, "remaining": 0},
                "aiRequests": {"limit": 10, "used": 10, "remaining": 0}
            }
        }"#;

        let response: IncrementUsageResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert!(!response.data.can_use);
        assert_eq!(response.data.ai_input_tokens.remaining, 0);
    }

    // ===========================================
    // UserProfile Tests
    // ===========================================

    #[test]
    fn test_deserialize_user_profile() {
        let json = r#"{
            "id": "user_123abc",
            "email": "user@example.com",
            "name": "John Doe",
            "externalId": "ext_456",
            "emailVerified": true,
            "createdAt": "2024-01-01T00:00:00Z",
            "lastLoginAt": "2024-06-15T10:30:00Z"
        }"#;

        let profile: UserProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.id, "user_123abc");
        assert_eq!(profile.email, "user@example.com");
        assert_eq!(profile.name, Some("John Doe".to_string()));
        assert_eq!(profile.external_id, Some("ext_456".to_string()));
        assert!(profile.email_verified);
        assert_eq!(profile.created_at, "2024-01-01T00:00:00Z");
        assert_eq!(profile.last_login_at, Some("2024-06-15T10:30:00Z".to_string()));
    }

    #[test]
    fn test_deserialize_user_profile_minimal() {
        let json = r#"{
            "id": "user_123",
            "email": "user@example.com",
            "name": null,
            "externalId": null,
            "emailVerified": false,
            "createdAt": "2024-01-01T00:00:00Z",
            "lastLoginAt": null
        }"#;

        let profile: UserProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.id, "user_123");
        assert_eq!(profile.email, "user@example.com");
        assert!(profile.name.is_none());
        assert!(profile.external_id.is_none());
        assert!(!profile.email_verified);
        assert!(profile.last_login_at.is_none());
    }

    #[test]
    fn test_deserialize_user_profile_unverified_email() {
        let json = r#"{
            "id": "user_123",
            "email": "unverified@example.com",
            "name": "Test User",
            "externalId": null,
            "emailVerified": false,
            "createdAt": "2024-01-01T00:00:00Z",
            "lastLoginAt": null
        }"#;

        let profile: UserProfile = serde_json::from_str(json).unwrap();
        assert!(!profile.email_verified);
    }

    #[test]
    fn test_user_profile_clone() {
        let profile = UserProfile {
            id: "user_123".to_string(),
            email: "user@example.com".to_string(),
            name: Some("Test".to_string()),
            external_id: Some("ext_456".to_string()),
            email_verified: true,
            created_at: "2024-01-01".to_string(),
            last_login_at: None,
        };

        let cloned = profile.clone();
        assert_eq!(profile.id, cloned.id);
        assert_eq!(profile.email, cloned.email);
        assert_eq!(profile.name, cloned.name);
        assert_eq!(profile.external_id, cloned.external_id);
    }

    // ===========================================
    // UserProfileResponse Tests
    // ===========================================

    #[test]
    fn test_deserialize_user_profile_response() {
        let json = r#"{
            "success": true,
            "data": {
                "id": "user_123",
                "email": "user@example.com",
                "name": "Test User",
                "externalId": "ext_456",
                "emailVerified": true,
                "createdAt": "2024-01-01T00:00:00Z",
                "lastLoginAt": "2024-06-15T10:30:00Z"
            }
        }"#;

        let response: UserProfileResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data.id, "user_123");
        assert_eq!(response.data.email, "user@example.com");
    }

    #[test]
    fn test_deserialize_user_profile_response_failure() {
        // Note: The current struct doesn't have error fields, but success can be false
        let json = r#"{
            "success": false,
            "data": {
                "id": "",
                "email": "",
                "name": null,
                "externalId": null,
                "emailVerified": false,
                "createdAt": "",
                "lastLoginAt": null
            }
        }"#;

        let response: UserProfileResponse = serde_json::from_str(json).unwrap();
        assert!(!response.success);
    }

    // ===========================================
    // Edge Cases and Error Handling
    // ===========================================

    #[test]
    fn test_invalid_reset_period_fails() {
        let json = r#"{
            "name": "ai_usage",
            "displayName": "AI Usage",
            "aiInputTokens": {"limit": 100, "used": 0, "remaining": 100},
            "aiOutputTokens": {"limit": 50, "used": 0, "remaining": 50},
            "aiRequests": {"limit": 10, "used": 0, "remaining": 10},
            "resetPeriod": "INVALID",
            "periodStart": null,
            "periodEnd": null
        }"#;

        let result: Result<UserLimit, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Invalid reset period should fail deserialization");
    }

    #[test]
    fn test_large_limit_values() {
        let json = r#"{
            "name": "ai_usage",
            "displayName": "AI Usage",
            "aiInputTokens": {"limit": 9223372036854775807, "used": 1000000000000, "remaining": 9223372035854775807},
            "aiOutputTokens": {"limit": 50000, "used": 0, "remaining": 50000},
            "aiRequests": {"limit": 1000, "used": 0, "remaining": 1000},
            "resetPeriod": null,
            "periodStart": null,
            "periodEnd": null
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.ai_input_tokens.limit, i64::MAX);
    }

    #[test]
    fn test_user_limit_roundtrip() {
        let original = UserLimit {
            name: "ai_usage".to_string(),
            display_name: "AI Usage".to_string(),
            description: Some("AI token and request limits".to_string()),
            unit: Some("tokens".to_string()),
            ai_input_tokens: LimitMetric { limit: 100000, used: 1000, remaining: 99000 },
            ai_output_tokens: LimitMetric { limit: 50000, used: 500, remaining: 49500 },
            ai_requests: LimitMetric { limit: 1000, used: 10, remaining: 990 },
            reset_period: Some(ResetPeriod::Monthly),
            period_start: Some("2024-01-01".to_string()),
            period_end: Some("2024-01-31".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: UserLimit = serde_json::from_str(&json).unwrap();

        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.ai_input_tokens.limit, deserialized.ai_input_tokens.limit);
        assert_eq!(original.ai_input_tokens.used, deserialized.ai_input_tokens.used);
        assert_eq!(original.ai_output_tokens.limit, deserialized.ai_output_tokens.limit);
        assert_eq!(original.ai_requests.limit, deserialized.ai_requests.limit);
        assert_eq!(original.reset_period, deserialized.reset_period);
    }

    #[test]
    fn test_increment_request_roundtrip() {
        let original = IncrementUsageRequest {
            email: "user@example.com".to_string(),
            ai_input_tokens: Some(1000),
            ai_output_tokens: Some(500),
            ai_requests: Some(1),
            model: Some("gpt-4o".to_string()),
            timestamp: Some("2024-01-15T10:30:00Z".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: IncrementUsageRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(original.email, deserialized.email);
        assert_eq!(original.ai_input_tokens, deserialized.ai_input_tokens);
        assert_eq!(original.ai_output_tokens, deserialized.ai_output_tokens);
        assert_eq!(original.ai_requests, deserialized.ai_requests);
        assert_eq!(original.model, deserialized.model);
        assert_eq!(original.timestamp, deserialized.timestamp);
    }

    #[test]
    fn test_user_profile_roundtrip() {
        let original = UserProfile {
            id: "user_123".to_string(),
            email: "test@example.com".to_string(),
            name: Some("Test User".to_string()),
            external_id: Some("ext_456".to_string()),
            email_verified: true,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            last_login_at: Some("2024-06-15T10:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: UserProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.email, deserialized.email);
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.external_id, deserialized.external_id);
        assert_eq!(original.email_verified, deserialized.email_verified);
    }

    // ===========================================
    // Debug Trait Tests
    // ===========================================

    #[test]
    fn test_user_limit_debug() {
        let limit = UserLimit {
            name: "ai_usage".to_string(),
            display_name: "AI Usage".to_string(),
            description: None,
            unit: None,
            ai_input_tokens: LimitMetric { limit: 100, used: 50, remaining: 50 },
            ai_output_tokens: LimitMetric { limit: 50, used: 25, remaining: 25 },
            ai_requests: LimitMetric { limit: 10, used: 5, remaining: 5 },
            reset_period: None,
            period_start: None,
            period_end: None,
        };

        let debug_str = format!("{:?}", limit);
        assert!(debug_str.contains("UserLimit"));
        assert!(debug_str.contains("ai_usage"));
    }

    #[test]
    fn test_user_profile_debug() {
        let profile = UserProfile {
            id: "user_123".to_string(),
            email: "test@example.com".to_string(),
            name: None,
            external_id: None,
            email_verified: false,
            created_at: "2024-01-01".to_string(),
            last_login_at: None,
        };

        let debug_str = format!("{:?}", profile);
        assert!(debug_str.contains("UserProfile"));
        assert!(debug_str.contains("user_123"));
    }

    // ===========================================
    // Tier Configuration Tests
    // ===========================================

    #[test]
    fn test_model_config_serialization_roundtrip() {
        let config = ModelConfig {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            relative_cost: 1,
            input_price_per_million: 0.15,
            output_price_per_million: 0.60,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
        assert_eq!(deserialized.provider, "openai");
        assert_eq!(deserialized.model, "gpt-4o-mini");
        assert_eq!(deserialized.relative_cost, 1);
    }

    #[test]
    fn test_model_config_camel_case_serialization() {
        let config = ModelConfig {
            provider: "anthropic".to_string(),
            model: "claude-3-5-sonnet".to_string(),
            relative_cost: 5,
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"relativeCost\":5"));
        assert!(json.contains("\"inputPricePerMillion\":3.0"));
        assert!(json.contains("\"outputPricePerMillion\":15.0"));
    }

    #[test]
    fn test_tier_config_data_deserialization() {
        let json = r#"{
            "version": "1.0.0",
            "updatedAt": "2024-01-15T10:30:00Z",
            "tiers": {
                "simple": [
                    {
                        "provider": "openai",
                        "model": "gpt-4o-mini",
                        "relativeCost": 1,
                        "inputPricePerMillion": 0.15,
                        "outputPricePerMillion": 0.60
                    }
                ],
                "moderate": [
                    {
                        "provider": "openai",
                        "model": "gpt-4o",
                        "relativeCost": 5,
                        "inputPricePerMillion": 2.50,
                        "outputPricePerMillion": 10.0
                    }
                ],
                "complex": [
                    {
                        "provider": "openai",
                        "model": "gpt-4o",
                        "relativeCost": 5,
                        "inputPricePerMillion": 2.50,
                        "outputPricePerMillion": 10.0
                    },
                    {
                        "provider": "anthropic",
                        "model": "claude-3-5-sonnet",
                        "relativeCost": 6,
                        "inputPricePerMillion": 3.0,
                        "outputPricePerMillion": 15.0
                    }
                ]
            }
        }"#;

        let config: TierConfigData = serde_json::from_str(json).unwrap();

        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.updated_at, "2024-01-15T10:30:00Z");
        assert_eq!(config.tiers.simple.len(), 1);
        assert_eq!(config.tiers.moderate.len(), 1);
        assert_eq!(config.tiers.complex.len(), 2);

        // Check simple tier
        assert_eq!(config.tiers.simple[0].provider, "openai");
        assert_eq!(config.tiers.simple[0].model, "gpt-4o-mini");
        assert_eq!(config.tiers.simple[0].relative_cost, 1);

        // Check complex tier has multiple models
        assert_eq!(config.tiers.complex[0].provider, "openai");
        assert_eq!(config.tiers.complex[1].provider, "anthropic");
    }

    #[test]
    fn test_tier_config_response_deserialization() {
        let json = r#"{
            "success": true,
            "data": {
                "version": "1.0.0",
                "updatedAt": "2024-01-15T10:30:00Z",
                "tiers": {
                    "simple": [],
                    "moderate": [],
                    "complex": []
                }
            }
        }"#;

        let response: TierConfigResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data.version, "1.0.0");
    }

    #[test]
    fn test_tier_config_data_roundtrip() {
        let original = TierConfigData {
            version: "2.0.0".to_string(),
            updated_at: "2024-06-01T00:00:00Z".to_string(),
            tiers: TierMapping {
                simple: vec![
                    ModelConfig {
                        provider: "openai".to_string(),
                        model: "gpt-4o-mini".to_string(),
                        relative_cost: 1,
                        input_price_per_million: 0.15,
                        output_price_per_million: 0.60,
                    },
                ],
                moderate: vec![
                    ModelConfig {
                        provider: "openai".to_string(),
                        model: "gpt-4o".to_string(),
                        relative_cost: 5,
                        input_price_per_million: 2.50,
                        output_price_per_million: 10.0,
                    },
                ],
                complex: vec![],
            },
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TierConfigData = serde_json::from_str(&json).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_model_config_relative_cost_must_be_positive() {
        // Document that relative_cost should be >= 1 to avoid division by zero
        // The weight calculation uses 1.0 / relative_cost
        let json = r#"{
            "provider": "openai",
            "model": "gpt-4o",
            "relativeCost": 0,
            "inputPricePerMillion": 2.50,
            "outputPricePerMillion": 10.0
        }"#;

        // Deserialization succeeds, but callers must validate relative_cost >= 1
        // This test documents the constraint
        let config: ModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.relative_cost, 0);
        // NOTE: TierRouter must validate relative_cost >= 1 before use
    }

    #[test]
    fn test_tier_mapping_empty_tiers_valid() {
        // Empty tier arrays are valid (means no models available for that tier)
        let json = r#"{
            "simple": [],
            "moderate": [],
            "complex": []
        }"#;

        let mapping: TierMapping = serde_json::from_str(json).unwrap();
        assert!(mapping.simple.is_empty());
        assert!(mapping.moderate.is_empty());
        assert!(mapping.complex.is_empty());
    }
}
