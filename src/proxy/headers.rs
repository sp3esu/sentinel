//! Header utilities for AI provider proxying
//!
//! Provides secure header filtering to ensure internal authentication tokens
//! are never forwarded to external AI providers.

use axum::http::header::{self, HeaderName};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

/// Headers that are safe to forward to AI providers
///
/// SECURITY: This is a whitelist approach - only these headers are forwarded.
/// The Authorization header is explicitly NOT included because:
/// - Client requests use internal JWTs for Sentinel authentication
/// - AI providers require their own API keys
/// - We MUST replace the Authorization header, never forward it
pub const SAFE_HEADERS_TO_FORWARD: &[HeaderName] = &[
    header::CONTENT_TYPE,
    header::ACCEPT,
    header::USER_AGENT,
    header::ACCEPT_ENCODING,
];

/// Custom headers that are safe to forward (as string names)
pub const SAFE_CUSTOM_HEADERS: &[&str] = &[
    "x-request-id", // For request correlation/tracing
];

/// Hop-by-hop headers that must never be forwarded
const HOP_BY_HOP_HEADERS: &[HeaderName] = &[
    header::CONNECTION,
    header::PROXY_AUTHENTICATE,
    header::PROXY_AUTHORIZATION,
    header::TE,
    header::TRAILER,
    header::TRANSFER_ENCODING,
    header::UPGRADE,
];

/// Build headers for proxying to an AI provider
///
/// # Security
///
/// This function:
/// - Only copies whitelisted headers from the incoming request
/// - Replaces the Authorization header with the provider's API key
/// - Never forwards client JWTs or other sensitive headers
///
/// # Arguments
///
/// * `incoming` - Headers from the client request
/// * `api_key` - The API key for the target AI provider
pub fn build_proxy_headers(incoming: &HeaderMap, api_key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    // Copy whitelisted standard headers
    for header_name in SAFE_HEADERS_TO_FORWARD {
        if let Some(value) = incoming.get(header_name) {
            headers.insert(header_name.clone(), value.clone());
        }
    }

    // Copy whitelisted custom headers
    for header_name in SAFE_CUSTOM_HEADERS {
        if let Some(value) = incoming.get(*header_name) {
            if let Ok(name) = HeaderName::from_bytes(header_name.as_bytes()) {
                headers.insert(name, value.clone());
            }
        }
    }

    // Set provider-specific Authorization (REPLACES any incoming Authorization)
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", api_key)).expect("Invalid API key format"),
    );

    // Ensure Content-Type is set
    headers
        .entry(CONTENT_TYPE)
        .or_insert(HeaderValue::from_static("application/json"));

    headers
}

/// Build default headers for requests without incoming headers
///
/// Used for internal API calls (like listing models) that don't originate
/// from a client request.
pub fn build_default_headers(api_key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", api_key)).expect("Invalid API key format"),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    headers
}

/// Check if a header is a hop-by-hop header that should not be forwarded
pub fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    HOP_BY_HOP_HEADERS.contains(name)
}

/// Filter hop-by-hop headers from a response
///
/// Used when converting provider responses back to client responses.
pub fn filter_response_headers(response_headers: &HeaderMap) -> HeaderMap {
    let mut filtered = HeaderMap::new();

    for (name, value) in response_headers {
        if !is_hop_by_hop_header(name) {
            filtered.insert(name.clone(), value.clone());
        }
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn test_build_proxy_headers_replaces_authorization() {
        let mut incoming = HeaderMap::new();
        incoming.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer client-jwt-token"),
        );
        incoming.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let result = build_proxy_headers(&incoming, "provider-api-key");

        // Authorization should be replaced, not forwarded
        assert_eq!(
            result.get(AUTHORIZATION).unwrap().to_str().unwrap(),
            "Bearer provider-api-key"
        );
    }

    #[test]
    fn test_build_proxy_headers_copies_safe_headers() {
        let mut incoming = HeaderMap::new();
        incoming.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        incoming.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        incoming.insert(
            header::USER_AGENT,
            HeaderValue::from_static("TestClient/1.0"),
        );

        let result = build_proxy_headers(&incoming, "api-key");

        assert!(result.contains_key(CONTENT_TYPE));
        assert!(result.contains_key(header::ACCEPT));
        assert!(result.contains_key(header::USER_AGENT));
    }

    #[test]
    fn test_build_proxy_headers_ignores_unsafe_headers() {
        let mut incoming = HeaderMap::new();
        incoming.insert("x-internal-secret", HeaderValue::from_static("secret"));
        incoming.insert(header::COOKIE, HeaderValue::from_static("session=abc"));

        let result = build_proxy_headers(&incoming, "api-key");

        assert!(!result.contains_key("x-internal-secret"));
        assert!(!result.contains_key(header::COOKIE));
    }

    #[test]
    fn test_is_hop_by_hop_header() {
        assert!(is_hop_by_hop_header(&header::CONNECTION));
        assert!(is_hop_by_hop_header(&header::TRANSFER_ENCODING));
        assert!(!is_hop_by_hop_header(&header::CONTENT_TYPE));
        assert!(!is_hop_by_hop_header(&header::ACCEPT));
    }
}
