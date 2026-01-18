//! Header utilities for AI provider proxying
//!
//! Provides secure header filtering to ensure internal authentication tokens
//! are never forwarded to external AI providers.

use axum::http::header::{self, HeaderName};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

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

/// Build default headers for AI provider requests
///
/// This function creates a minimal set of headers for all requests to AI providers.
/// Client headers are intentionally NOT forwarded to ensure Sentinel acts as a
/// complete barrier between clients and AI providers.
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

    #[test]
    fn test_build_default_headers_sets_authorization_and_content_type() {
        let result = build_default_headers("test-api-key");

        assert_eq!(
            result.get(AUTHORIZATION).unwrap().to_str().unwrap(),
            "Bearer test-api-key"
        );
        assert_eq!(
            result.get(CONTENT_TYPE).unwrap().to_str().unwrap(),
            "application/json"
        );
        // Should only have these two headers
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_is_hop_by_hop_header() {
        assert!(is_hop_by_hop_header(&header::CONNECTION));
        assert!(is_hop_by_hop_header(&header::TRANSFER_ENCODING));
        assert!(!is_hop_by_hop_header(&header::CONTENT_TYPE));
        assert!(!is_hop_by_hop_header(&header::ACCEPT));
    }
}
