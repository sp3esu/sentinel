//! Authentication middleware
//!
//! Validates Zion JWTs and caches validation results.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

use crate::{error::AppError, AppState};

/// Extract user ID from request
///
/// This struct is used to pass authenticated user info to handlers.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub external_id: String,
    pub email: String,
}

/// Extract the Authorization header and return the bearer token
pub fn extract_bearer_token(auth_header: &str) -> Option<&str> {
    if auth_header.starts_with("Bearer ") {
        Some(&auth_header[7..])
    } else {
        None
    }
}

/// Hash a JWT for cache key
pub fn hash_jwt(jwt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(jwt.as_bytes());
    hex::encode(hasher.finalize())
}

/// Authentication middleware
///
/// This middleware:
/// 1. Extracts JWT from Authorization header
/// 2. Checks JWT cache (Redis) for existing validation
/// 3. If not cached, validates with Zion API
/// 4. Caches successful validation
/// 5. Adds AuthenticatedUser to request extensions
pub async fn auth_middleware(
    State(_state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    // Extract bearer token
    let token = extract_bearer_token(auth_header).ok_or(AppError::InvalidToken)?;

    // Hash the token for cache lookup
    let _token_hash = hash_jwt(token);

    // TODO: Phase 3 - Implement full JWT validation:
    // 1. Check cache for existing validation
    // 2. If miss, call Zion /api/v1/users/me with the JWT
    // 3. Cache the result
    // 4. Extract user info and add to extensions

    // Placeholder for Phase 1 - just pass through
    let user = AuthenticatedUser {
        user_id: "placeholder".to_string(),
        external_id: "placeholder".to_string(),
        email: "placeholder@example.com".to_string(),
    };

    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(
            extract_bearer_token("Bearer abc123"),
            Some("abc123")
        );
        assert_eq!(extract_bearer_token("bearer abc123"), None);
        assert_eq!(extract_bearer_token("abc123"), None);
        assert_eq!(extract_bearer_token(""), None);
    }

    #[test]
    fn test_hash_jwt() {
        let hash = hash_jwt("test-jwt-token");
        assert_eq!(hash.len(), 64); // SHA256 produces 32 bytes = 64 hex chars
    }
}
