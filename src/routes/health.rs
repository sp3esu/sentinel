//! Health check endpoints
//!
//! Provides endpoints for monitoring and container orchestration:
//! - `/health` - Full health check with dependency status
//! - `/health/ready` - Readiness probe
//! - `/health/live` - Liveness probe

use std::sync::Arc;
use std::time::Instant;

use axum::{extract::State, http::StatusCode, Json};
use redis::AsyncCommands;
use serde::Serialize;

use crate::AppState;

/// Health status enum
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Individual dependency check result
#[derive(Debug, Serialize)]
pub struct DependencyCheck {
    pub status: HealthStatus,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Dependency checks collection
#[derive(Debug, Serialize)]
pub struct DependencyChecks {
    pub redis: DependencyCheck,
}

/// Application statistics
#[derive(Debug, Serialize)]
pub struct HealthStats {
    pub uptime_seconds: u64,
}

/// Full health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub version: String,
    pub uptime_seconds: u64,
    pub timestamp: String,
    pub checks: DependencyChecks,
    pub stats: HealthStats,
}

/// Simple health response for liveness/readiness
#[derive(Debug, Serialize)]
pub struct SimpleHealthResponse {
    pub status: HealthStatus,
}

/// Check Redis connectivity
async fn check_redis(state: &AppState) -> DependencyCheck {
    let start = Instant::now();
    let mut conn = state.redis.clone();

    match redis::cmd("PING")
        .query_async::<_, String>(&mut conn)
        .await
    {
        Ok(_) => DependencyCheck {
            status: HealthStatus::Healthy,
            latency_ms: start.elapsed().as_millis() as u64,
            error: None,
        },
        Err(e) => DependencyCheck {
            status: HealthStatus::Unhealthy,
            latency_ms: start.elapsed().as_millis() as u64,
            error: Some(e.to_string()),
        },
    }
}

/// Full health check endpoint
///
/// Returns comprehensive health information including:
/// - Overall status
/// - Version info
/// - Uptime
/// - Dependency checks (Redis)
/// - Application stats
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<HealthResponse>) {
    let redis_check = check_redis(&state).await;

    // Determine overall status
    let overall_status = if redis_check.status == HealthStatus::Unhealthy {
        HealthStatus::Unhealthy
    } else if redis_check.status == HealthStatus::Degraded {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    };

    let uptime = state.start_time.elapsed().as_secs();

    let response = HealthResponse {
        status: overall_status.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks: DependencyChecks { redis: redis_check },
        stats: HealthStats {
            uptime_seconds: uptime,
        },
    };

    let status_code = match overall_status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK,
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(response))
}

/// Readiness probe endpoint
///
/// Returns 200 OK if the application is ready to receive traffic.
/// Used by Kubernetes readiness probes.
pub async fn readiness_check(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<SimpleHealthResponse>) {
    let redis_check = check_redis(&state).await;

    if redis_check.status == HealthStatus::Unhealthy {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SimpleHealthResponse {
                status: HealthStatus::Unhealthy,
            }),
        );
    }

    (
        StatusCode::OK,
        Json(SimpleHealthResponse {
            status: HealthStatus::Healthy,
        }),
    )
}

/// Liveness probe endpoint
///
/// Returns 200 OK if the application is alive.
/// Used by Kubernetes liveness probes.
pub async fn liveness_check() -> (StatusCode, Json<SimpleHealthResponse>) {
    (
        StatusCode::OK,
        Json(SimpleHealthResponse {
            status: HealthStatus::Healthy,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_serialization() {
        assert_eq!(
            serde_json::to_string(&HealthStatus::Healthy).unwrap(),
            "\"healthy\""
        );
        assert_eq!(
            serde_json::to_string(&HealthStatus::Degraded).unwrap(),
            "\"degraded\""
        );
        assert_eq!(
            serde_json::to_string(&HealthStatus::Unhealthy).unwrap(),
            "\"unhealthy\""
        );
    }
}
