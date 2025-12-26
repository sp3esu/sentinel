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

/// User limit information from Zion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserLimit {
    pub limit_id: String,
    pub name: String,
    pub display_name: String,
    pub unit: Option<String>,
    pub limit: i64,
    pub used: i64,
    pub remaining: i64,
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

/// Request to increment usage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageRequest {
    pub external_id: String,
    pub limit_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
}

/// Response from increment usage endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementUsageResponse {
    pub success: bool,
    pub data: UserLimit,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_user_limit() {
        let json = r#"{
            "limitId": "clx123abc",
            "name": "ai_input_tokens",
            "displayName": "AI Input Tokens",
            "unit": "tokens",
            "limit": 10000,
            "used": 500,
            "remaining": 9500,
            "resetPeriod": "MONTHLY",
            "periodStart": "2024-01-01T00:00:00Z",
            "periodEnd": "2024-01-31T23:59:59Z"
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.name, "ai_input_tokens");
        assert_eq!(limit.limit, 10000);
        assert_eq!(limit.used, 500);
        assert_eq!(limit.remaining, 9500);
        assert_eq!(limit.reset_period, Some(ResetPeriod::Monthly));
    }

    #[test]
    fn test_serialize_increment_request() {
        let request = IncrementUsageRequest {
            external_id: "user123".to_string(),
            limit_name: "ai_input_tokens".to_string(),
            amount: Some(100),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("externalId"));
        assert!(json.contains("limitName"));
        assert!(json.contains("100"));
    }
}
