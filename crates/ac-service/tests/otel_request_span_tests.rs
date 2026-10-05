//! AC request-span tests (§11) through AC's real router under the DEPLOYED
//! log filter (read from the manifest):
//!
//! - probe/scrape paths export no request span and never warn, while an API
//!   path exports an INFO `request` span carrying `method` + normalized
//!   `endpoint`;
//! - a known `traceparent` reparents AC's request span — the receive half of
//!   GC→AC continuity (GC's `AcClient` injects on every call).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::otel::{ac_deployed_rust_log, get, send};

use axum::body::Body;
use axum::http::Request;
use common::observability::testing::otel::{
    attr, install_test_propagator, known_traceparent, SpanCapture, KNOWN_PARENT_SPAN_ID_HEX,
    KNOWN_TRACE_ID_HEX,
};
use opentelemetry_sdk::trace::SpanData;
use sqlx::PgPool;

fn single_request_span(capture: &SpanCapture) -> SpanData {
    let spans = capture.spans_named("request");
    assert_eq!(
        spans.len(),
        1,
        "expected exactly one exported `request` span, got {:?}",
        capture
            .finished_spans()
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    );
    spans.into_iter().next().unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn probe_paths_export_no_span_and_api_paths_export_info_span(pool: PgPool) {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&ac_deployed_rust_log());

    for probe in common::observability::otel_http::PROBE_ENDPOINTS {
        send(&pool, get(probe)).await;
    }
    assert!(
        capture.spans_named("request").is_empty(),
        "probe/scrape paths must not export a request span under the deployed filter"
    );
    assert_eq!(capture.extraction_warns(), 0, "probes must never warn");

    // Positive control in the same run: an API path exports an INFO span.
    send(&pool, get("/.well-known/jwks.json")).await;
    let span = single_request_span(&capture);
    assert_eq!(
        attr(&span, "endpoint").as_deref(),
        Some("/.well-known/jwks.json")
    );
    assert_eq!(attr(&span, "method").as_deref(), Some("GET"));
    assert_eq!(capture.extraction_warns(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn http_request_span_reparents_under_deployed_filter(pool: PgPool) {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&ac_deployed_rust_log());

    let request = Request::builder()
        .uri("/.well-known/jwks.json")
        .header("traceparent", known_traceparent())
        .body(Body::empty())
        .unwrap();
    send(&pool, request).await;

    let span = single_request_span(&capture);
    assert_eq!(
        format!("{:032x}", span.span_context.trace_id()),
        KNOWN_TRACE_ID_HEX,
        "AC request span must continue the inbound trace under the deployed filter"
    );
    assert_eq!(
        format!("{:016x}", span.parent_span_id),
        KNOWN_PARENT_SPAN_ID_HEX
    );
    assert_eq!(capture.extraction_warns(), 0);
}
