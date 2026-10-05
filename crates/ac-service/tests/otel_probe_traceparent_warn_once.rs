//! A `traceparent` sent to an AC probe path (whose request span is DEBUG and
//! so disabled under the deployed filter) is context AC dropped: it warns,
//! once per process.
//!
//! Its own test binary with a single test: the warn-once latch behind
//! `common::observability::otel::set_remote_parent` is process-global, so
//! sharing a binary with another traceparent-bearing request would make the
//! count order-dependent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::otel::{ac_deployed_rust_log, send};

use axum::body::Body;
use axum::http::Request;
use common::observability::testing::otel::{
    install_test_propagator, known_traceparent, SpanCapture,
};
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn traceparent_on_probe_path_warns_exactly_once(pool: PgPool) {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&ac_deployed_rust_log());

    for _ in 0..2 {
        let request = Request::builder()
            .uri("/health")
            .header("traceparent", known_traceparent())
            .body(Body::empty())
            .unwrap();
        send(&pool, request).await;
    }

    assert_eq!(
        capture.extraction_warns(),
        1,
        "two traceparent-bearing probe requests must warn exactly once"
    );
}
