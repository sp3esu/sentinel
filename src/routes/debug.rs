//! Debug endpoints for development
//!
//! These endpoints are only available when SENTINEL_DEBUG=true.
//! They provide introspection into cache state and configuration.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use redis::AsyncCommands;
use serde::Serialize;

use crate::{cache::redis::keys, AppState};

/// Cache overview response
#[derive(Serialize)]
pub struct CacheOverview {
    pub limits_cache: CacheStats,
    pub jwt_cache: CacheStats,
    pub profile_cache: CacheStats,
    pub redis_available: bool,
}

/// Stats for a cache category
#[derive(Serialize)]
pub struct CacheStats {
    pub keys_count: usize,
    pub sample_keys: Vec<String>,
}

/// User auth state response
#[derive(Serialize)]
pub struct UserAuthState {
    pub external_id: String,
    pub cached_limits: Option<serde_json::Value>,
    pub cache_ttl_remaining_seconds: i64,
}

/// Config response (non-sensitive)
#[derive(Serialize)]
pub struct ConfigInfo {
    pub zion_api_url: String,
    pub openai_api_url: String,
    pub cache_ttl_seconds: u64,
    pub jwt_cache_ttl_seconds: u64,
    pub redis_connected: bool,
    pub debug_enabled: bool,
}

/// Scan Redis for keys matching a pattern (limited to 100 keys)
async fn scan_keys(conn: &mut redis::aio::ConnectionManager, pattern: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut cursor = 0u64;

    loop {
        let result: Result<(u64, Vec<String>), _> = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(conn)
            .await;

        match result {
            Ok((next_cursor, batch)) => {
                keys.extend(batch);
                cursor = next_cursor;
                if keys.len() >= 100 || cursor == 0 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    keys.truncate(100);
    keys
}

/// GET /debug/cache - Overview of Redis cache state
pub async fn cache_overview(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if !state.config.debug_enabled {
        return Err(debug_disabled_error());
    }

    let Some(ref redis) = state.redis else {
        return Ok(Json(CacheOverview {
            limits_cache: CacheStats { keys_count: 0, sample_keys: vec![] },
            jwt_cache: CacheStats { keys_count: 0, sample_keys: vec![] },
            profile_cache: CacheStats { keys_count: 0, sample_keys: vec![] },
            redis_available: false,
        }));
    };

    let mut conn = redis.clone();

    let limits_keys = scan_keys(&mut conn, "sentinel:limits:*").await;
    let jwt_keys = scan_keys(&mut conn, "sentinel:jwt:*").await;
    let profile_keys = scan_keys(&mut conn, "sentinel:profile:*").await;

    let response = CacheOverview {
        limits_cache: CacheStats {
            keys_count: limits_keys.len(),
            sample_keys: limits_keys.into_iter().take(10).collect(),
        },
        jwt_cache: CacheStats {
            keys_count: jwt_keys.len(),
            sample_keys: jwt_keys.into_iter().take(10).collect(),
        },
        profile_cache: CacheStats {
            keys_count: profile_keys.len(),
            sample_keys: profile_keys.into_iter().take(10).collect(),
        },
        redis_available: true,
    };

    Ok(Json(response))
}

/// GET /debug/auth/:external_id - Inspect cached auth state for a user
pub async fn user_auth_state(
    State(state): State<Arc<AppState>>,
    Path(external_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if !state.config.debug_enabled {
        return Err(debug_disabled_error());
    }

    let Some(ref redis) = state.redis else {
        return Ok(Json(UserAuthState {
            external_id,
            cached_limits: None,
            cache_ttl_remaining_seconds: -2,
        }));
    };

    let cache_key = keys::user_limits(&external_id);
    let mut conn = redis.clone();

    // Try to get cached limits
    let cached_value: Option<String> = conn.get(&cache_key).await.ok();
    let cached_limits: Option<serde_json::Value> = cached_value
        .and_then(|v| serde_json::from_str(&v).ok());

    // Get TTL
    let ttl: i64 = conn.ttl(&cache_key).await.unwrap_or(-2);

    let response = UserAuthState {
        external_id,
        cached_limits,
        cache_ttl_remaining_seconds: ttl,
    };

    Ok(Json(response))
}

/// GET /debug/config - Return non-sensitive configuration
pub async fn config_info(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if !state.config.debug_enabled {
        return Err(debug_disabled_error());
    }

    // Check Redis connectivity
    let redis_connected = if let Some(ref redis) = state.redis {
        let mut conn = redis.clone();
        let result: Result<String, _> = redis::cmd("PING").query_async(&mut conn).await;
        result.map(|r| r == "PONG").unwrap_or(false)
    } else {
        false
    };

    let response = ConfigInfo {
        zion_api_url: state.config.zion_api_url.clone(),
        openai_api_url: state.config.openai_api_url.clone(),
        cache_ttl_seconds: state.config.cache_ttl_seconds,
        jwt_cache_ttl_seconds: state.config.jwt_cache_ttl_seconds,
        redis_connected,
        debug_enabled: state.config.debug_enabled,
    };

    Ok(Json(response))
}

/// Helper to return a consistent 404 error when debug is disabled
fn debug_disabled_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": {
                "message": "Debug endpoints are disabled. Set SENTINEL_DEBUG=true to enable.",
                "type": "not_found_error",
                "code": "debug_disabled"
            }
        })),
    )
}
