//! Documentation endpoints for the Native API
//!
//! Serves Swagger UI and raw OpenAPI spec with API key protection.
//! Protected by X-Docs-Key header; returns 404 when unauthorized to hide endpoint existence.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use utoipa::OpenApi;

use crate::docs::NativeApiDoc;

/// Middleware to protect docs endpoints with API key
///
/// Checks for X-Docs-Key header matching DOCS_API_KEY environment variable.
/// Returns 404 (not 401/403) when unauthorized to hide endpoint existence.
/// Allows access when DOCS_API_KEY is not set (dev mode).
pub async fn docs_auth_middleware(request: Request, next: Next) -> Result<Response, Response> {
    // Get expected API key from environment
    let expected_key = std::env::var("DOCS_API_KEY").ok();

    // If no key configured, allow access (dev mode)
    if expected_key.is_none() {
        return Ok(next.run(request).await);
    }

    // Check X-Docs-Key header
    let provided_key = request
        .headers()
        .get("X-Docs-Key")
        .and_then(|v| v.to_str().ok());

    match (expected_key, provided_key) {
        (Some(expected), Some(provided)) if expected == provided => Ok(next.run(request).await),
        _ => {
            // Return 404 to hide endpoint existence
            Err(StatusCode::NOT_FOUND.into_response())
        }
    }
}

/// Handler for OpenAPI JSON endpoint
async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(NativeApiDoc::openapi())
}

/// Handler for Swagger UI HTML
///
/// Serves a standalone Swagger UI page that loads the OpenAPI spec
/// from the /native/docs/openapi.json endpoint.
async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

/// Create the docs router
///
/// Routes:
/// - GET /native/docs - Swagger UI
/// - GET /native/docs/ - Swagger UI (with trailing slash)
/// - GET /native/docs/openapi.json - Raw OpenAPI spec
///
/// Uses CDN-hosted Swagger UI assets to avoid bundling large static files.
/// The HTML page loads assets directly from unpkg CDN.
///
/// The router is generic over state type S, allowing it to be merged
/// into routers with any state (e.g., Arc<AppState>).
pub fn create_docs_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/native/docs", get(swagger_ui))
        .route("/native/docs/", get(swagger_ui))
        .route("/native/docs/openapi.json", get(openapi_json))
        .layer(axum::middleware::from_fn(docs_auth_middleware))
}

/// Swagger UI HTML template
///
/// Uses unpkg CDN for Swagger UI assets, configurable to load
/// the OpenAPI spec from the local endpoint.
const SWAGGER_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Sentinel Native API - Documentation</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
    <style>
        html { box-sizing: border-box; overflow-y: scroll; }
        *, *:before, *:after { box-sizing: inherit; }
        body { margin: 0; background: #fafafa; }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {
            const ui = SwaggerUIBundle({
                url: "/native/docs/openapi.json",
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIStandalonePreset
                ],
                plugins: [
                    SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout",
                persistAuthorization: true
            });
            window.ui = ui;
        };
    </script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest};
    use std::sync::Mutex;
    use tower::ServiceExt;

    // Static mutex to serialize access to DOCS_API_KEY environment variable
    // This prevents race conditions when tests run in parallel
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    // Helper to run test with environment variable control
    async fn with_env<F, Fut>(key_value: Option<&str>, test_fn: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _guard = ENV_MUTEX.lock().unwrap();
        match key_value {
            Some(val) => std::env::set_var("DOCS_API_KEY", val),
            None => std::env::remove_var("DOCS_API_KEY"),
        }
        test_fn().await;
        std::env::remove_var("DOCS_API_KEY");
    }

    // Test: Docs accessible without key when env var not set (dev mode)
    #[tokio::test]
    async fn test_docs_accessible_without_key_in_dev_mode() {
        with_env(None, || async {
            let app = create_docs_router::<()>();

            let request = HttpRequest::builder()
                .uri("/native/docs/openapi.json")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        })
        .await;
    }

    // Test: Docs return 404 without key when env var is set (prod mode)
    #[tokio::test]
    async fn test_docs_return_404_without_key_in_prod_mode() {
        with_env(Some("secret-docs-key-prod"), || async {
            let app = create_docs_router::<()>();

            let request = HttpRequest::builder()
                .uri("/native/docs/openapi.json")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        })
        .await;
    }

    // Test: Docs return 404 with wrong key (prod mode)
    #[tokio::test]
    async fn test_docs_return_404_with_wrong_key() {
        with_env(Some("secret-docs-key-wrong"), || async {
            let app = create_docs_router::<()>();

            let request = HttpRequest::builder()
                .uri("/native/docs/openapi.json")
                .header("X-Docs-Key", "wrong-key")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        })
        .await;
    }

    // Test: Docs accessible with correct key (prod mode)
    #[tokio::test]
    async fn test_docs_accessible_with_correct_key() {
        with_env(Some("secret-docs-key-correct"), || async {
            let app = create_docs_router::<()>();

            let request = HttpRequest::builder()
                .uri("/native/docs/openapi.json")
                .header("X-Docs-Key", "secret-docs-key-correct")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        })
        .await;
    }

    // Test: OpenAPI JSON contains expected structure
    #[tokio::test]
    async fn test_openapi_json_structure() {
        with_env(None, || async {
            let app = create_docs_router::<()>();

            let request = HttpRequest::builder()
                .uri("/native/docs/openapi.json")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();

            // Verify OpenAPI structure
            assert!(spec["openapi"].as_str().unwrap().starts_with("3."));
            assert!(spec["info"]["title"]
                .as_str()
                .unwrap()
                .contains("Sentinel"));
            assert!(spec["paths"]["/native/v1/chat/completions"].is_object());
            assert!(spec["components"]["schemas"]["ChatCompletionRequest"].is_object());
            assert!(spec["components"]["securitySchemes"]["bearer_auth"].is_object());
        })
        .await;
    }

    // Test: Swagger UI HTML is served
    #[tokio::test]
    async fn test_swagger_ui_html_served() {
        with_env(None, || async {
            let app = create_docs_router::<()>();

            let request = HttpRequest::builder()
                .uri("/native/docs")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let html = String::from_utf8_lossy(&body);

            // Verify it's Swagger UI HTML
            assert!(html.contains("swagger-ui"));
            assert!(html.contains("/native/docs/openapi.json"));
        })
        .await;
    }
}
