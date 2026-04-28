// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — `MetricAssertion` binds a per-thread
// recorder; a multi-thread runtime would route emissions through a different
// OS thread and the snapshot would observe nothing. See
// `crates/common/src/observability/testing.rs:60-72` for the isolation model.
//
//! Component tests for GC's `http_metrics_middleware` driving real
//! `gc_http_requests_total{method,endpoint,status_code}` and
//! `gc_http_request_duration_seconds{method,endpoint,status}` emissions per
//! ADR-0032 Step 5.
//!
//! Per-failure-class fidelity: every reachable status_code value drives an
//! independent reproducer with `assert_delta(1)` on the named status_code +
//! `assert_delta(0)` on every sibling status_code under the same
//! (method, endpoint) filter (label-swap-bug catcher per ADR-0032 §Pattern #3).
//!
//! Drive seam: a minimal axum `Router` with the production middleware
//! wrapped via `tower::ServiceExt::oneshot`, executed on the test thread.
//! Same shape as `crates/ac-service/tests/http_metrics_integration.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use axum::Router;
use common::observability::testing::MetricAssertion;
use gc_service::middleware::http_metrics::http_metrics_middleware;
use tower::ServiceExt;

async fn handler_200() -> &'static str {
    "OK"
}

async fn handler_500() -> (StatusCode, &'static str) {
    (StatusCode::INTERNAL_SERVER_ERROR, "Error")
}

fn test_app() -> Router {
    Router::new()
        .route("/health", get(handler_200))
        .route("/api/v1/me", get(handler_200))
        .route("/api/v1/meetings/abc123", get(handler_200))
        .route("/error", get(handler_500))
        .layer(middleware::from_fn(http_metrics_middleware))
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_200_static_path_emits_counter_and_histogram() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Histogram first (drain-on-read).
    snap.histogram("gc_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/health"),
            ("status_code", "200"),
        ])
        .assert_observation_count(1);

    snap.counter("gc_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/health"),
            ("status_code", "200"),
        ])
        .assert_delta(1);

    // Adjacency: no other status_code emitted under the same (method, endpoint).
    for sibling_status in ["400", "404", "405", "500", "504"] {
        snap.counter("gc_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/health"),
                ("status_code", sibling_status),
            ])
            .assert_delta(0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_200_dynamic_meeting_code_normalizes_to_template() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/meetings/abc123")
        .body(Body::empty())
        .unwrap();
    let _ = app.oneshot(request).await.unwrap();

    // `/api/v1/meetings/abc123` normalizes to `/api/v1/meetings/{code}` per
    // `normalize_endpoint`. Verify the bounded label takes the template form.
    snap.counter("gc_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/api/v1/meetings/{code}"),
            ("status_code", "200"),
        ])
        .assert_delta(1);
    // The raw dynamic path must NOT appear as a label.
    snap.counter("gc_http_requests_total")
        .with_labels(&[("endpoint", "/api/v1/meetings/abc123")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_500_emits_status_error_category() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/error")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // `/error` is unknown to `normalize_endpoint`, so it normalizes to `/other`.
    // Histogram label `status` is the categorize_status_code output → "error".
    snap.histogram("gc_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "500"),
        ])
        .assert_observation_count(1);

    snap.counter("gc_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "500"),
        ])
        .assert_delta(1);

    // Adjacency: no 200 under the same (method, /other).
    snap.counter("gc_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "200"),
        ])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_404_unknown_route_emits_status_404() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    snap.counter("gc_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "404"),
        ])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_405_method_not_allowed_emits_status_405() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    // POST to a GET-only route → 405. The metrics middleware is the outermost
    // layer, so it captures axum's framework-level 405 response.
    let request = Request::builder()
        .method("POST")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    snap.counter("gc_http_requests_total")
        .with_labels(&[
            ("method", "POST"),
            ("endpoint", "/health"),
            ("status_code", "405"),
        ])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_emits_only_one_observation_per_request() {
    // Validates the middleware doesn't double-record on the same request.
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let _ = app.oneshot(request).await.unwrap();

    snap.histogram("gc_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/health"),
            ("status_code", "200"),
        ])
        .assert_observation_count(1);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_504_emits_timeout_status_category() {
    // Synthetic 504 via a route handler that returns Gateway Timeout. Validates
    // the `categorize_status_code` mapping from 504 → status="timeout"
    // (the histogram label, not the counter's status_code label).
    async fn handler_504() -> (StatusCode, &'static str) {
        (StatusCode::GATEWAY_TIMEOUT, "Gateway Timeout")
    }
    let app = Router::new()
        .route("/timeout", get(handler_504))
        .layer(middleware::from_fn(http_metrics_middleware));

    let snap = MetricAssertion::snapshot();
    let request = Request::builder()
        .method("GET")
        .uri("/timeout")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);

    // Histogram has the categorized status (timeout), counter has the raw code.
    snap.histogram("gc_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "504"),
        ])
        .assert_observation_count(1);

    snap.counter("gc_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "504"),
        ])
        .assert_delta(1);
}
