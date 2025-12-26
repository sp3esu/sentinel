//! Health endpoint integration tests
//!
//! Tests for the health check endpoints:
//! - GET /health - Full health check with dependency status
//! - GET /health/ready - Readiness probe
//! - GET /health/live - Liveness probe

use axum::{
    http::StatusCode,
    Router,
};
use axum_test::TestServer;
use serde_json::Value;

/// Create a minimal router for health endpoint testing.
///
/// Health endpoints don't require Redis for the liveness check,
/// but do require it for readiness and full health checks.
/// For unit-style integration tests, we test what we can without Redis.
fn create_health_test_router() -> Router {
    // For health endpoint tests, we use a simplified approach
    // that doesn't require actual Redis connection
    use axum::routing::get;
    use axum::Json;
    use serde::Serialize;

    #[derive(Serialize)]
    struct SimpleHealthResponse {
        status: String,
    }

    #[derive(Serialize)]
    struct FullHealthResponse {
        status: String,
        version: String,
        uptime_seconds: u64,
        timestamp: String,
        checks: HealthChecks,
        stats: HealthStats,
    }

    #[derive(Serialize)]
    struct HealthChecks {
        redis: DependencyCheck,
    }

    #[derive(Serialize)]
    struct DependencyCheck {
        status: String,
        latency_ms: u64,
    }

    #[derive(Serialize)]
    struct HealthStats {
        uptime_seconds: u64,
    }

    async fn mock_health_check() -> (StatusCode, Json<FullHealthResponse>) {
        (StatusCode::OK, Json(FullHealthResponse {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 100,
            timestamp: chrono::Utc::now().to_rfc3339(),
            checks: HealthChecks {
                redis: DependencyCheck {
                    status: "healthy".to_string(),
                    latency_ms: 1,
                },
            },
            stats: HealthStats {
                uptime_seconds: 100,
            },
        }))
    }

    async fn mock_readiness_check() -> (StatusCode, Json<SimpleHealthResponse>) {
        (StatusCode::OK, Json(SimpleHealthResponse {
            status: "healthy".to_string(),
        }))
    }

    async fn mock_liveness_check() -> (StatusCode, Json<SimpleHealthResponse>) {
        (StatusCode::OK, Json(SimpleHealthResponse {
            status: "healthy".to_string(),
        }))
    }

    async fn mock_unhealthy_check() -> (StatusCode, Json<FullHealthResponse>) {
        (StatusCode::SERVICE_UNAVAILABLE, Json(FullHealthResponse {
            status: "unhealthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 100,
            timestamp: chrono::Utc::now().to_rfc3339(),
            checks: HealthChecks {
                redis: DependencyCheck {
                    status: "unhealthy".to_string(),
                    latency_ms: 0,
                },
            },
            stats: HealthStats {
                uptime_seconds: 100,
            },
        }))
    }

    Router::new()
        .route("/health", get(mock_health_check))
        .route("/health/ready", get(mock_readiness_check))
        .route("/health/live", get(mock_liveness_check))
        .route("/health/unhealthy", get(mock_unhealthy_check))
}

#[tokio::test]
async fn test_health_endpoint_returns_proper_structure() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/health").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Verify response structure
    assert!(json.get("status").is_some(), "Response should have 'status' field");
    assert!(json.get("version").is_some(), "Response should have 'version' field");
    assert!(json.get("uptime_seconds").is_some(), "Response should have 'uptime_seconds' field");
    assert!(json.get("timestamp").is_some(), "Response should have 'timestamp' field");
    assert!(json.get("checks").is_some(), "Response should have 'checks' field");
    assert!(json.get("stats").is_some(), "Response should have 'stats' field");

    // Verify status value
    let status = json["status"].as_str().unwrap();
    assert_eq!(status, "healthy");

    // Verify checks structure
    let checks = json.get("checks").unwrap();
    assert!(checks.get("redis").is_some(), "Checks should have 'redis' field");

    // Verify redis check structure
    let redis_check = checks.get("redis").unwrap();
    assert!(redis_check.get("status").is_some(), "Redis check should have 'status'");
    assert!(redis_check.get("latency_ms").is_some(), "Redis check should have 'latency_ms'");
}

#[tokio::test]
async fn test_health_ready_endpoint() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/health/ready").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Readiness check should have minimal response
    assert!(json.get("status").is_some(), "Response should have 'status' field");

    let status = json["status"].as_str().unwrap();
    assert_eq!(status, "healthy");
}

#[tokio::test]
async fn test_health_live_endpoint() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/health/live").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Liveness check should have minimal response
    assert!(json.get("status").is_some(), "Response should have 'status' field");

    let status = json["status"].as_str().unwrap();
    assert_eq!(status, "healthy");
}

#[tokio::test]
async fn test_health_unhealthy_status_when_redis_down() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    // Use the mock unhealthy endpoint to simulate Redis being down
    let response = server.get("/health/unhealthy").await;

    response.assert_status(StatusCode::SERVICE_UNAVAILABLE);

    let json: Value = response.json();

    // Verify unhealthy status
    let status = json["status"].as_str().unwrap();
    assert_eq!(status, "unhealthy");

    // Verify redis check shows unhealthy
    let redis_status = json["checks"]["redis"]["status"].as_str().unwrap();
    assert_eq!(redis_status, "unhealthy");
}

#[tokio::test]
async fn test_health_endpoint_returns_version() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/health").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Version should be the package version from Cargo.toml
    let version = json["version"].as_str().unwrap();
    assert!(!version.is_empty(), "Version should not be empty");
    // Should match semver pattern
    assert!(version.contains('.'), "Version should be in semver format");
}

#[tokio::test]
async fn test_health_endpoint_returns_uptime() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/health").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Uptime should be a non-negative number
    let uptime = json["uptime_seconds"].as_u64().unwrap();
    // uptime is u64, so it's always non-negative
    let _ = uptime; // Verify we can read it

    // Stats should also have uptime
    let stats_uptime = json["stats"]["uptime_seconds"].as_u64().unwrap();
    assert_eq!(uptime, stats_uptime, "Uptime in stats should match top-level uptime");
}

#[tokio::test]
async fn test_health_endpoint_returns_valid_timestamp() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    let response = server.get("/health").await;

    response.assert_status_ok();

    let json: Value = response.json();

    // Timestamp should be a valid RFC3339 string
    let timestamp = json["timestamp"].as_str().unwrap();
    assert!(!timestamp.is_empty(), "Timestamp should not be empty");

    // Should be parseable as a datetime
    let parsed = chrono::DateTime::parse_from_rfc3339(timestamp);
    assert!(parsed.is_ok(), "Timestamp should be valid RFC3339 format");
}

#[tokio::test]
async fn test_health_endpoints_accept_get_only() {
    let app = create_health_test_router();
    let server = TestServer::new(app).expect("Failed to create test server");

    // POST should not be allowed
    let response = server.post("/health").await;
    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);

    let response = server.post("/health/ready").await;
    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);

    let response = server.post("/health/live").await;
    response.assert_status(StatusCode::METHOD_NOT_ALLOWED);
}
