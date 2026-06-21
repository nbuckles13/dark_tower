//! Integration cover for the telemetry-proxy metrics (R-2 / R-51) per
//! ADR-0032 Step 5.
//!
//! - `gc_telemetry_ingest_total{status, payload_kind}`
//! - `gc_telemetry_ingest_duration_seconds{status}`
//! - `gc_telemetry_payload_bytes{payload_kind}`
//! - `gc_telemetry_rate_limited_total{reason}`
//! - `gc_telemetry_pii_attributes_dropped_total{kind}`
//!
//! # WRAPPER-CAT-C framing
//!
//! Real recording-site coverage (the handler driven end-to-end against a real
//! DB + wiremock JWKS + wiremock collector) lives in
//! `crates/gc-service/tests/telemetry_proxy_tests.rs`. This file holds the
//! per-label cartesian + hard-form adjacency assertions that
//! `MetricAssertion`'s per-thread recorder isolation makes deterministic. The
//! recording fns are synchronous, so no tokio runtime pinning is needed.
//!
//! The distinct-bucket-matcher correctness (F1 — `gc_telemetry_payload_bytes`
//! must NOT inherit the seconds-scale default buckets) is verified separately
//! in `telemetry_payload_bytes_lands_in_byte_bucket_not_overflow` against the
//! real Prometheus render.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::{
    record_telemetry_ingest, record_telemetry_pii_dropped, record_telemetry_rate_limited,
};

/// The declared `status` values (R-51). NOTE on declared-but-unemitted values:
/// - `rejected_pii` — drops-forward-as-success design; never emitted this story.
/// - `rejected_auth` — 401s are rejected by the `require_user_auth` route layer
///   BEFORE the handler/IngestGuard runs, so NO real request reaches
///   `record_telemetry_ingest("rejected_auth", ...)`. 401s are observed on
///   `gc_http_requests_total{status_code="401"}`, not here. The assertions below
///   only prove the wrapper CAN carry these labels when called directly — they
///   do NOT claim a request path emits them.
const STATUSES: &[&str] = &[
    "success",
    "rejected_auth",
    "rejected_size",
    "rejected_rate",
    "rejected_pii",
    "error",
];

/// The 6 structural drop levels.
const KINDS: &[&str] = &[
    "resource",
    "scope",
    "datapoint",
    "span",
    "span_event",
    "span_link",
];

#[test]
fn ingest_total_and_duration_emit_for_every_status() {
    let snap = MetricAssertion::snapshot();

    for status in STATUSES {
        record_telemetry_ingest(status, "metric", Duration::from_millis(10));
    }

    // Histogram first (drain-on-read): one observation per status.
    snap.histogram("gc_telemetry_ingest_duration_seconds")
        .assert_observation_count_at_least(STATUSES.len());

    for status in STATUSES {
        snap.counter("gc_telemetry_ingest_total")
            .with_labels(&[("status", status), ("payload_kind", "metric")])
            .assert_delta(1);
    }
}

#[test]
fn ingest_payload_kind_metric_and_trace_do_not_cross_contaminate() {
    let snap = MetricAssertion::snapshot();

    record_telemetry_ingest("success", "metric", Duration::from_millis(5));
    record_telemetry_ingest("success", "trace", Duration::from_millis(5));

    snap.histogram("gc_telemetry_ingest_duration_seconds")
        .assert_observation_count_at_least(2);

    snap.counter("gc_telemetry_ingest_total")
        .with_labels(&[("status", "success"), ("payload_kind", "metric")])
        .assert_delta(1);
    snap.counter("gc_telemetry_ingest_total")
        .with_labels(&[("status", "success"), ("payload_kind", "trace")])
        .assert_delta(1);
    // log is declared but must not appear unless emitted.
    snap.counter("gc_telemetry_ingest_total")
        .with_labels(&[("status", "success"), ("payload_kind", "log")])
        .assert_delta(0);
}

#[test]
fn rate_limited_only_per_user_wired() {
    let snap = MetricAssertion::snapshot();

    record_telemetry_rate_limited("per_user");

    snap.counter("gc_telemetry_rate_limited_total")
        .with_labels(&[("reason", "per_user")])
        .assert_delta(1);
    // per_org / global reserved/unwired.
    snap.counter("gc_telemetry_rate_limited_total")
        .with_labels(&[("reason", "per_org")])
        .assert_delta(0);
    snap.counter("gc_telemetry_rate_limited_total")
        .with_labels(&[("reason", "global")])
        .assert_delta(0);
}

#[test]
fn pii_dropped_per_kind_adjacency() {
    let snap = MetricAssertion::snapshot();

    // Drive a distinct count at each kind; assert each kind moved by exactly its
    // own count and NO sibling absorbed it (label-swap catcher).
    for (i, kind) in KINDS.iter().enumerate() {
        record_telemetry_pii_dropped(kind, (i + 1) as u64);
    }

    for (i, kind) in KINDS.iter().enumerate() {
        snap.counter("gc_telemetry_pii_attributes_dropped_total")
            .with_labels(&[("kind", *kind)])
            .assert_delta((i + 1) as u64);
    }
}

#[test]
fn pii_dropped_zero_is_unobserved() {
    let snap = MetricAssertion::snapshot();

    // A clean payload (no drops) must NOT emit the counter for any kind.
    record_telemetry_pii_dropped("resource", 0);
    record_telemetry_pii_dropped("span", 0);

    for kind in KINDS {
        snap.counter("gc_telemetry_pii_attributes_dropped_total")
            .with_labels(&[("kind", *kind)])
            .assert_unobserved();
    }
}

/// Hard-form: on a pure reject path (e.g. 415/429) the handler records the
/// ingest outcome but the filter and forwarder NEVER run — so the PII-dropped
/// counter and the payload-bytes histogram must be unobserved. This proves an
/// always-emit refactor regression would be caught.
#[test]
fn reject_path_leaves_filter_and_payload_unobserved() {
    let snap = MetricAssertion::snapshot();

    // Simulate a reject path: only the ingest outcome is recorded.
    record_telemetry_ingest("rejected_rate", "metric", Duration::from_micros(20));

    snap.histogram("gc_telemetry_ingest_duration_seconds")
        .assert_observation_count_at_least(1);

    // Filter never ran.
    for kind in KINDS {
        snap.counter("gc_telemetry_pii_attributes_dropped_total")
            .with_labels(&[("kind", *kind)])
            .assert_unobserved();
    }
    // Payload-bytes never recorded (only recorded for accepted payloads).
    snap.histogram("gc_telemetry_payload_bytes")
        .with_labels(&[("payload_kind", "metric")])
        .assert_unobserved();
}

/// F1 regression guard: `gc_telemetry_payload_bytes` must use the BYTE bucket
/// matcher, not the seconds-scale default. A 50 KiB observation must land in a
/// real byte bucket (`le="65536"`) and NOT collapse into `+Inf` overflow.
///
/// This drives the REAL Prometheus recorder via `init_metrics_recorder` and
/// inspects the rendered exposition. It is gated on being able to install the
/// recorder (only one process-wide install succeeds); when another test already
/// installed a recorder we skip rather than assert against the wrong buckets.
#[test]
fn telemetry_payload_bytes_lands_in_byte_bucket_not_overflow() {
    use metrics::histogram;

    let handle = match gc_service::observability::metrics::init_metrics_recorder() {
        Ok(h) => h,
        // Another test in this binary already installed the global recorder;
        // the bucket config is identical, but we can only render from the handle
        // we installed. Skip rather than assert against a foreign recorder.
        Err(_) => return,
    };

    histogram!("gc_telemetry_payload_bytes", "payload_kind" => "metric").record((50 * 1024) as f64);

    let rendered = handle.render();

    // The 65536 bucket must have a non-zero cumulative count (the 50 KiB sample
    // is ≤ 65536), proving the byte buckets are in effect.
    let has_65536_bucket = rendered.lines().any(|l| {
        l.contains("gc_telemetry_payload_bytes_bucket")
            && l.contains("le=\"65536\"")
            && l.trim_end().ends_with(" 1")
    });
    assert!(
        has_65536_bucket,
        "expected 50 KiB observation in the le=65536 byte bucket; \
         payload_bytes is using the wrong (seconds-scale) buckets.\n{rendered}"
    );
}
