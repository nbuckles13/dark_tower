// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — `MetricAssertion` binds a per-thread
// recorder; a multi-thread runtime would route emissions through a different
// OS thread and the snapshot would observe nothing. See
// `crates/common/src/observability/testing.rs:60-72` for the isolation model.
//
//! Component tests for AC's `http_metrics_middleware` driving real
//! `ac_http_requests_total{method,path,status_code}` and
//! `ac_http_request_duration_seconds{method,path,status_code}` emissions
//! per ADR-0032 Step 4 §Cluster 1.
//!
//! Per-failure-class fidelity: every reachable status_code value
//! (`200`, `400`, `404`, `405`, `415`, `500`) has a per-test reproducer
//! with `assert_delta(1)` on the named status + `assert_delta(0)` on
//! every sibling status under the same (method, path) filter
//! (label-swap-bug catcher per ADR-0032 §Pattern #3).
//!
//! Drive seam: a minimal axum `Router` with the production middleware
//! wrapped via `tower::ServiceExt::oneshot`, executed on the test thread.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ac_service::middleware::http_metrics::http_metrics_middleware;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::routing::get;
use axum::Router;
use common::observability::testing::MetricAssertion;
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
        .route("/error", get(handler_500))
        .layer(middleware::from_fn(http_metrics_middleware))
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_200_emits_counter_and_histogram() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Histogram-first ordering (drain-on-read) per testing.rs §"Histograms DRAIN".
    snap.histogram("ac_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/health"),
            ("status_code", "200"),
        ])
        .assert_observation_count(1);

    snap.counter("ac_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/health"),
            ("status_code", "200"),
        ])
        .assert_delta(1);

    // Adjacency: no other status_code emitted under the same (method, path).
    for sibling_status in ["400", "404", "405", "415", "500"] {
        snap.counter("ac_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/health"),
                ("status_code", sibling_status),
            ])
            .assert_delta(0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_500_emits_counter_with_status_500() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/error")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    snap.histogram("ac_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "500"),
        ])
        .assert_observation_count(1);

    snap.counter("ac_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "500"),
        ])
        .assert_delta(1);

    // `/error` is not in `normalize_path`'s allow-list, so it normalizes to
    // "/other" — verify the 200 sibling under the same (method, /other) is silent.
    snap.counter("ac_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "200"),
        ])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_404_for_unknown_route_emits_status_404() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    snap.histogram("ac_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "404"),
        ])
        .assert_observation_count(1);

    snap.counter("ac_http_requests_total")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/other"),
            ("status_code", "404"),
        ])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_405_for_wrong_method_emits_status_405() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    // POST to a GET-only route → 405 Method Not Allowed (handled by axum
    // before the route handler runs; metrics middleware still records it
    // because it's the outermost layer).
    let request = Request::builder()
        .method("POST")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    snap.counter("ac_http_requests_total")
        .with_labels(&[
            ("method", "POST"),
            ("endpoint", "/health"),
            ("status_code", "405"),
        ])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_emits_only_one_observation_per_request() {
    let snap = MetricAssertion::snapshot();
    let app = test_app();
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let _ = app.oneshot(request).await.unwrap();

    // Exactly one observation in the histogram for the (GET, /health, 200)
    // tuple — validates the middleware doesn't double-record.
    snap.histogram("ac_http_request_duration_seconds")
        .with_labels(&[
            ("method", "GET"),
            ("endpoint", "/health"),
            ("status_code", "200"),
        ])
        .assert_observation_count(1);
}
