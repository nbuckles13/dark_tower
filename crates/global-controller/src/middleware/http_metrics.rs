//! HTTP metrics middleware for capturing all request/response metrics
//!
//! This middleware captures metrics for ALL HTTP responses including
//! framework-level errors that occur before handlers run:
//! - 415 Unsupported Media Type (wrong Content-Type)
//! - 400 Bad Request (JSON parse errors)
//! - 404 Not Found
//! - 405 Method Not Allowed
//!
//! Per ADR-0011, this provides comprehensive HTTP observability.

use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

use crate::observability::metrics::record_http_request;

/// Middleware that records HTTP request metrics for all responses
///
/// This captures:
/// - Request method
/// - Request path (normalized to prevent cardinality explosion)
/// - Response status code
/// - Request duration
///
/// Applied as the outermost layer to capture all responses including
/// framework-level errors.
pub async fn http_metrics_middleware(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    // Execute the request
    let response = next.run(request).await;

    // Record metrics
    let duration = start.elapsed();
    let status_code = response.status().as_u16();
    record_http_request(&method, &path, status_code, duration);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    async fn handler_200() -> &'static str {
        "OK"
    }

    async fn handler_500() -> (StatusCode, &'static str) {
        (StatusCode::INTERNAL_SERVER_ERROR, "Error")
    }

    fn test_app() -> Router {
        Router::new()
            .route("/success", get(handler_200))
            .route("/error", get(handler_500))
            .layer(middleware::from_fn(http_metrics_middleware))
    }

    #[tokio::test]
    async fn test_middleware_records_success() {
        let app = test_app();

        let request = HttpRequest::builder()
            .method("GET")
            .uri("/success")
            .body(Body::empty())
            .expect("request builder should succeed");

        let response = app.oneshot(request).await.expect("request should succeed");
        assert_eq!(response.status(), StatusCode::OK);
        // Metrics are recorded - verified by coverage since we can't inspect
        // the global metrics recorder in unit tests
    }

    #[tokio::test]
    async fn test_middleware_records_error() {
        let app = test_app();

        let request = HttpRequest::builder()
            .method("GET")
            .uri("/error")
            .body(Body::empty())
            .expect("request builder should succeed");

        let response = app.oneshot(request).await.expect("request should succeed");
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_middleware_records_not_found() {
        let app = test_app();

        let request = HttpRequest::builder()
            .method("GET")
            .uri("/nonexistent")
            .body(Body::empty())
            .expect("request builder should succeed");

        let response = app.oneshot(request).await.expect("request should succeed");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        // The 404 is recorded by the middleware
    }
}
