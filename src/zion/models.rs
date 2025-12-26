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
    // UserLimit Tests
    // ===========================================

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
    fn test_deserialize_user_limit_minimal() {
        // Test with only required fields and nulls for optional
        let json = r#"{
            "limitId": "clx123",
            "name": "test_limit",
            "displayName": "Test Limit",
            "unit": null,
            "limit": 100,
            "used": 0,
            "remaining": 100,
            "resetPeriod": null,
            "periodStart": null,
            "periodEnd": null
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.limit_id, "clx123");
        assert_eq!(limit.name, "test_limit");
        assert_eq!(limit.display_name, "Test Limit");
        assert!(limit.unit.is_none());
        assert_eq!(limit.limit, 100);
        assert_eq!(limit.used, 0);
        assert_eq!(limit.remaining, 100);
        assert!(limit.reset_period.is_none());
        assert!(limit.period_start.is_none());
        assert!(limit.period_end.is_none());
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
                "limitId": "clx123",
                "name": "test",
                "displayName": "Test",
                "unit": null,
                "limit": 100,
                "used": 0,
                "remaining": 100,
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
            limit_id: "clx123".to_string(),
            name: "ai_tokens".to_string(),
            display_name: "AI Tokens".to_string(),
            unit: Some("tokens".to_string()),
            limit: 1000,
            used: 100,
            remaining: 900,
            reset_period: Some(ResetPeriod::Daily),
            period_start: Some("2024-01-01T00:00:00Z".to_string()),
            period_end: Some("2024-01-01T23:59:59Z".to_string()),
        };

        let json = serde_json::to_string(&limit).unwrap();
        assert!(json.contains("\"limitId\":\"clx123\""));
        assert!(json.contains("\"name\":\"ai_tokens\""));
        assert!(json.contains("\"displayName\":\"AI Tokens\""));
        assert!(json.contains("\"resetPeriod\":\"DAILY\""));
    }

    #[test]
    fn test_user_limit_clone() {
        let limit = UserLimit {
            limit_id: "clx123".to_string(),
            name: "test".to_string(),
            display_name: "Test".to_string(),
            unit: None,
            limit: 100,
            used: 50,
            remaining: 50,
            reset_period: None,
            period_start: None,
            period_end: None,
        };

        let cloned = limit.clone();
        assert_eq!(limit.limit_id, cloned.limit_id);
        assert_eq!(limit.name, cloned.name);
    }

    #[test]
    fn test_user_limit_negative_values() {
        // Test that negative values are allowed (remaining can be negative if overused)
        let json = r#"{
            "limitId": "clx123",
            "name": "test",
            "displayName": "Test",
            "unit": null,
            "limit": 100,
            "used": 150,
            "remaining": -50,
            "resetPeriod": null,
            "periodStart": null,
            "periodEnd": null
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.used, 150);
        assert_eq!(limit.remaining, -50);
    }

    // ===========================================
    // IncrementUsageRequest Tests
    // ===========================================

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

    #[test]
    fn test_serialize_increment_request_no_amount() {
        let request = IncrementUsageRequest {
            external_id: "user123".to_string(),
            limit_name: "ai_requests".to_string(),
            amount: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"externalId\":\"user123\""));
        assert!(json.contains("\"limitName\":\"ai_requests\""));
        // amount should be skipped when None
        assert!(!json.contains("amount"));
    }

    #[test]
    fn test_deserialize_increment_request() {
        let json = r#"{
            "externalId": "ext_abc123",
            "limitName": "ai_output_tokens",
            "amount": 500
        }"#;

        let request: IncrementUsageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.external_id, "ext_abc123");
        assert_eq!(request.limit_name, "ai_output_tokens");
        assert_eq!(request.amount, Some(500));
    }

    #[test]
    fn test_deserialize_increment_request_null_amount() {
        let json = r#"{
            "externalId": "ext_abc123",
            "limitName": "ai_requests",
            "amount": null
        }"#;

        let request: IncrementUsageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.external_id, "ext_abc123");
        assert_eq!(request.limit_name, "ai_requests");
        assert!(request.amount.is_none());
    }

    #[test]
    fn test_increment_request_clone() {
        let request = IncrementUsageRequest {
            external_id: "user123".to_string(),
            limit_name: "ai_tokens".to_string(),
            amount: Some(100),
        };

        let cloned = request.clone();
        assert_eq!(request.external_id, cloned.external_id);
        assert_eq!(request.limit_name, cloned.limit_name);
        assert_eq!(request.amount, cloned.amount);
    }

    // ===========================================
    // ExternalLimitsResponse Tests
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
                        "limitId": "lim_1",
                        "name": "ai_input_tokens",
                        "displayName": "AI Input Tokens",
                        "unit": "tokens",
                        "limit": 10000,
                        "used": 500,
                        "remaining": 9500,
                        "resetPeriod": "MONTHLY",
                        "periodStart": "2024-01-01",
                        "periodEnd": "2024-01-31"
                    }
                ]
            }
        }"#;

        let response: ExternalLimitsResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data.user_id, "user_123");
        assert_eq!(response.data.external_id, "ext_456");
        assert_eq!(response.data.limits.len(), 1);
        assert_eq!(response.data.limits[0].name, "ai_input_tokens");
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
    fn test_deserialize_external_limits_response_multiple_limits() {
        let json = r#"{
            "success": true,
            "data": {
                "userId": "user_123",
                "externalId": "ext_456",
                "limits": [
                    {
                        "limitId": "lim_1",
                        "name": "ai_input_tokens",
                        "displayName": "AI Input Tokens",
                        "unit": "tokens",
                        "limit": 10000,
                        "used": 500,
                        "remaining": 9500,
                        "resetPeriod": "MONTHLY",
                        "periodStart": null,
                        "periodEnd": null
                    },
                    {
                        "limitId": "lim_2",
                        "name": "ai_output_tokens",
                        "displayName": "AI Output Tokens",
                        "unit": "tokens",
                        "limit": 20000,
                        "used": 1000,
                        "remaining": 19000,
                        "resetPeriod": "MONTHLY",
                        "periodStart": null,
                        "periodEnd": null
                    },
                    {
                        "limitId": "lim_3",
                        "name": "ai_requests",
                        "displayName": "AI Requests",
                        "unit": "requests",
                        "limit": 100,
                        "used": 10,
                        "remaining": 90,
                        "resetPeriod": "DAILY",
                        "periodStart": null,
                        "periodEnd": null
                    }
                ]
            }
        }"#;

        let response: ExternalLimitsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.limits.len(), 3);
        assert_eq!(response.data.limits[0].name, "ai_input_tokens");
        assert_eq!(response.data.limits[1].name, "ai_output_tokens");
        assert_eq!(response.data.limits[2].name, "ai_requests");
    }

    // ===========================================
    // IncrementUsageResponse Tests
    // ===========================================

    #[test]
    fn test_deserialize_increment_usage_response() {
        let json = r#"{
            "success": true,
            "data": {
                "limitId": "lim_1",
                "name": "ai_input_tokens",
                "displayName": "AI Input Tokens",
                "unit": "tokens",
                "limit": 10000,
                "used": 600,
                "remaining": 9400,
                "resetPeriod": "MONTHLY",
                "periodStart": null,
                "periodEnd": null
            }
        }"#;

        let response: IncrementUsageResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data.name, "ai_input_tokens");
        assert_eq!(response.data.used, 600);
        assert_eq!(response.data.remaining, 9400);
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
            "limitId": "clx123",
            "name": "test",
            "displayName": "Test",
            "unit": null,
            "limit": 100,
            "used": 0,
            "remaining": 100,
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
            "limitId": "clx123",
            "name": "test",
            "displayName": "Test",
            "unit": null,
            "limit": 9223372036854775807,
            "used": 1000000000000,
            "remaining": 9223372035854775807,
            "resetPeriod": null,
            "periodStart": null,
            "periodEnd": null
        }"#;

        let limit: UserLimit = serde_json::from_str(json).unwrap();
        assert_eq!(limit.limit, i64::MAX);
    }

    #[test]
    fn test_user_limit_roundtrip() {
        let original = UserLimit {
            limit_id: "clx123".to_string(),
            name: "ai_tokens".to_string(),
            display_name: "AI Tokens".to_string(),
            unit: Some("tokens".to_string()),
            limit: 10000,
            used: 500,
            remaining: 9500,
            reset_period: Some(ResetPeriod::Monthly),
            period_start: Some("2024-01-01".to_string()),
            period_end: Some("2024-01-31".to_string()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: UserLimit = serde_json::from_str(&json).unwrap();

        assert_eq!(original.limit_id, deserialized.limit_id);
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.limit, deserialized.limit);
        assert_eq!(original.used, deserialized.used);
        assert_eq!(original.remaining, deserialized.remaining);
        assert_eq!(original.reset_period, deserialized.reset_period);
    }

    #[test]
    fn test_increment_request_roundtrip() {
        let original = IncrementUsageRequest {
            external_id: "ext_123".to_string(),
            limit_name: "ai_tokens".to_string(),
            amount: Some(100),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: IncrementUsageRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(original.external_id, deserialized.external_id);
        assert_eq!(original.limit_name, deserialized.limit_name);
        assert_eq!(original.amount, deserialized.amount);
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
            limit_id: "clx123".to_string(),
            name: "test".to_string(),
            display_name: "Test".to_string(),
            unit: None,
            limit: 100,
            used: 50,
            remaining: 50,
            reset_period: None,
            period_start: None,
            period_end: None,
        };

        let debug_str = format!("{:?}", limit);
        assert!(debug_str.contains("UserLimit"));
        assert!(debug_str.contains("clx123"));
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
}
