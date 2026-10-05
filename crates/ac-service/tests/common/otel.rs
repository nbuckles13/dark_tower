//! Shared helpers for AC's OTel request-span tests (§11): AC's production
//! router over the standard test `AppState`, plus AC's deployed `RUST_LOG`.
//! Span capture and W3C fixtures come from
//! `common::observability::testing::otel` (one home for GC/AC/MC/MH).

#![allow(
    dead_code,
    clippy::unwrap_used,
    reason = "shared test module: each test binary uses a subset; unwrap is the test's fail-fast"
)]

use super::test_state::make_app_state;
use ac_service::routes;
use axum::body::Body;
use axum::http::Request;
use sqlx::PgPool;
use tower::ServiceExt;

/// AC's deployed `RUST_LOG`, read from the AC manifest.
#[must_use]
pub fn ac_deployed_rust_log() -> String {
    common::observability::testing::otel::deployed_rust_log(
        "infra/services/ac-service/statefulset.yaml",
    )
}

/// AC's production router (`build_routes`) over the standard test `AppState`.
pub fn router(pool: PgPool) -> axum::Router {
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder()
        .handle();
    routes::build_routes(make_app_state(pool), metrics_handle)
}

/// Send one request through the router, dropping the response so the
/// `TraceLayer` request span closes and exports.
pub async fn send(pool: &PgPool, request: Request<Body>) {
    drop(router(pool.clone()).oneshot(request).await.unwrap());
}

/// A bodiless `GET`.
pub fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}
