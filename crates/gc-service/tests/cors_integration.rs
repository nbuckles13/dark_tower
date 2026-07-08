// Every `#[tokio::test]` here is pinned to `flavor = "current_thread"` and that
// pinning is LOAD-BEARING — `MetricAssertion` binds a per-thread recorder; a
// multi-thread runtime would route the observer's `gc_cors_preflight_total`
// emission through a different OS thread and the snapshot would observe nothing.
// See `crates/common/src/observability/testing.rs` for the isolation model.
//
//! End-to-end CORS tests (R-1 / R-52) driving GC's REAL router
//! (`routes::build_routes`) so the preflight-bypasses-auth (case 3) and
//! guest-token (case 6) guarantees are genuinely exercised, not a minimal
//! stand-in stack.
//!
//! Drive seam: build the production `Router` via `build_routes` over a lazily
//! constructed `PgPool` (never connects — every case is a preflight, which
//! `CorsLayer` short-circuits before any handler/DB access, or a no-Origin
//! request that never reaches the DB) and exercise it with
//! `tower::ServiceExt::oneshot`. A live `tokio::spawn`ed server is deliberately
//! NOT used: it would defeat the per-thread `MetricAssertion` recorder.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use common::observability::testing::{MetricAssertion, MetricSnapshot};
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::config::Config;
use gc_service::handlers::TelemetryState;
use gc_service::routes::{self, AppState};
use gc_service::services::MockMcClient;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tower::ServiceExt;

const DEV_ORIGIN: &str = "http://localhost:5173";
const DENIED_ORIGIN: &str = "http://evil.test";

/// Build GC's production router with the given CORS allowlist. Uses a lazy
/// (never-connecting) pool and a mock MC client; no DB or live server.
fn build_test_router(allowed_origins: &str) -> Router {
    let vars = HashMap::from([
        (
            "DATABASE_URL".to_string(),
            "postgresql://test/test".to_string(),
        ),
        ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
        ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
        (
            "CORS_ALLOWED_ORIGINS".to_string(),
            allowed_origins.to_string(),
        ),
    ]);
    let config = Config::from_vars(&vars).expect("config should load");

    // Lazy pool: `build_routes`/`AppState` need a `PgPool` value, but no test
    // here reaches a DB query (preflight short-circuits at CorsLayer).
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://test/test")
        .expect("lazy pool");

    let (_tx, rx) = watch::channel(SecretString::from("test-token"));
    let token_receiver = TokenReceiver::from_watch_receiver(rx);

    let telemetry = TelemetryState::from_config(
        config.otel_collector_endpoint.clone(),
        config.telemetry_proxy_max_bytes,
        config.telemetry_proxy_rate_limit_per_minute,
    )
    .expect("telemetry state");

    let state = Arc::new(AppState {
        pool,
        config,
        mc_client: Arc::new(MockMcClient::accepting()),
        token_receiver,
        telemetry,
    });

    // A non-installed handle just to satisfy the `/metrics` route param — the
    // per-thread MetricAssertion recorder captures emissions independently.
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder()
        .handle();

    routes::build_routes(state, metrics_handle).expect("build_routes")
}

fn preflight(uri: &str, origin: &str, request_method: &str) -> Request<Body> {
    Request::builder()
        .method(Method::OPTIONS)
        .uri(uri)
        .header(header::ORIGIN, origin)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, request_method)
        .body(Body::empty())
        .unwrap()
}

/// Assert every cell of the 2×2 `gc_cors_preflight_total` label space is
/// unobserved — used by the non-preflight cases (5, 9) to prove the observer
/// records NOTHING for a non-genuine preflight.
fn assert_no_preflight_metric(snap: &MetricSnapshot) {
    for (origin_class, status) in [
        ("allowed", "200"),
        ("denied", "403"),
        ("allowed", "403"),
        ("denied", "200"),
    ] {
        snap.counter("gc_cors_preflight_total")
            .with_labels(&[("origin_class", origin_class), ("status", status)])
            .assert_unobserved();
    }
}

// Case 1 — allowed-origin preflight → 200 + ACAO; metric allowed/200 with the
// off-diagonal AND cross cells unobserved (hard label-swap catcher).
#[tokio::test(flavor = "current_thread")]
async fn allowed_origin_preflight_200_with_acao_and_metric() {
    let snap = MetricAssertion::snapshot();
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(preflight("/api/v1/meetings", DEV_ORIGIN, "POST"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok()),
        Some(DEV_ORIGIN)
    );

    snap.counter("gc_cors_preflight_total")
        .with_labels(&[("origin_class", "allowed"), ("status", "200")])
        .assert_delta(1);
    for (origin_class, status) in [("denied", "403"), ("allowed", "403"), ("denied", "200")] {
        snap.counter("gc_cors_preflight_total")
            .with_labels(&[("origin_class", origin_class), ("status", status)])
            .assert_delta(0);
    }
}

// Case 2 — denied-origin preflight → 403, NO ACAO, NO ACAC, empty body; metric
// denied/403 with off-diagonal + cross cells unobserved.
#[tokio::test(flavor = "current_thread")]
async fn denied_origin_preflight_403_no_cors_headers_empty_body_and_metric() {
    let snap = MetricAssertion::snapshot();
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(preflight("/api/v1/meetings", DENIED_ORIGIN, "POST"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    assert!(!response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS));
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        body.is_empty(),
        "denied 403 must carry a generic empty body"
    );

    snap.counter("gc_cors_preflight_total")
        .with_labels(&[("origin_class", "denied"), ("status", "403")])
        .assert_delta(1);
    for (origin_class, status) in [("allowed", "200"), ("allowed", "403"), ("denied", "200")] {
        snap.counter("gc_cors_preflight_total")
            .with_labels(&[("origin_class", origin_class), ("status", status)])
            .assert_delta(0);
    }
}

// Case 3 — preflight bypasses auth: OPTIONS to the user-authenticated
// `/api/v1/meetings` route with NO Authorization header returns 200 (CorsLayer
// short-circuits before `require_user_auth`), NOT 401.
#[tokio::test(flavor = "current_thread")]
async fn preflight_bypasses_user_auth() {
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(preflight("/api/v1/meetings", DEV_ORIGIN, "POST"))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "preflight must short-circuit before auth (not 401)"
    );
}

// Case 4 — no-Origin GET passes through untouched (health check).
#[tokio::test(flavor = "current_thread")]
async fn no_origin_get_passes_through() {
    let snap = MetricAssertion::snapshot();
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_no_preflight_metric(&snap);
}

// Case 5 — no-Origin OPTIONS is NOT a genuine preflight: passes through (not
// 403) and records NO metric at all (security condition 1).
#[tokio::test(flavor = "current_thread")]
async fn no_origin_options_passes_through_no_metric() {
    let snap = MetricAssertion::snapshot();
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/meetings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::FORBIDDEN);
    assert_no_preflight_metric(&snap);
}

// Case 6 — preflight applies to the guest-token route (R-1 "including
// guest-token"): allowed origin → 200 + ACAO.
#[tokio::test(flavor = "current_thread")]
async fn guest_token_preflight_allowed_200_with_acao() {
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(preflight(
            "/api/v1/meetings/ABC123/guest-token",
            DEV_ORIGIN,
            "POST",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok()),
        Some(DEV_ORIGIN)
    );
}

// Case 7 — allowed preflight advertises the configured methods/headers + max-age.
#[tokio::test(flavor = "current_thread")]
async fn allowed_preflight_advertises_methods_headers_max_age() {
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(preflight("/api/v1/meetings", DEV_ORIGIN, "POST"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();

    let methods = headers
        .get(header::ACCESS_CONTROL_ALLOW_METHODS)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_uppercase();
    for m in ["GET", "POST", "PATCH", "OPTIONS"] {
        assert!(methods.contains(m), "ACAM must advertise {m}: {methods}");
    }

    let allow_headers = headers
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    for h in ["authorization", "content-type", "traceparent", "tracestate"] {
        assert!(
            allow_headers.contains(h),
            "ACAH must advertise {h}: {allow_headers}"
        );
    }

    assert_eq!(
        headers
            .get(header::ACCESS_CONTROL_MAX_AGE)
            .and_then(|v| v.to_str().ok()),
        Some("600")
    );
}

// Case 8 — a simple (non-preflight) allowed GET carries ACAO but records NO
// preflight metric (only OPTIONS preflights are counted).
#[tokio::test(flavor = "current_thread")]
async fn simple_allowed_get_has_acao_but_no_preflight_metric() {
    let snap = MetricAssertion::snapshot();
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .header(header::ORIGIN, DEV_ORIGIN)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok()),
        Some(DEV_ORIGIN)
    );
    assert_no_preflight_metric(&snap);
}

// Case 9 — Origin present but a non-preflight method (simple GET) from a denied
// origin: observer never touches it (not 403) and records no metric.
#[tokio::test(flavor = "current_thread")]
async fn denied_origin_simple_get_not_rewritten_no_metric() {
    let snap = MetricAssertion::snapshot();
    let app = build_test_router(DEV_ORIGIN);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .header(header::ORIGIN, DENIED_ORIGIN)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::FORBIDDEN);
    assert_no_preflight_metric(&snap);
}

// Fail-closed: with an EMPTY allowlist, an otherwise well-formed preflight from
// the dev origin is denied (403) — proves the prod-base `CORS_ALLOWED_ORIGINS=""`
// exposes no origin.
#[tokio::test(flavor = "current_thread")]
async fn empty_allowlist_denies_all_preflight() {
    let snap = MetricAssertion::snapshot();
    let app = build_test_router("");

    let response = app
        .oneshot(preflight("/api/v1/meetings", DEV_ORIGIN, "POST"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    snap.counter("gc_cors_preflight_total")
        .with_labels(&[("origin_class", "denied"), ("status", "403")])
        .assert_delta(1);
}
