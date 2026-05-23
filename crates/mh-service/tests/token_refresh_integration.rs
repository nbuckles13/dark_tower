//! Integration coverage for `mh_service::observability::metrics::record_token_refresh_metrics`.
//!
//! The real emission path runs inside the `TokenManager::with_on_refresh`
//! closure in `main.rs`, which the binary's integration environment exercises
//! via the TokenManager background task. Since `main.rs` is the binary entry
//! point (not reachable from `cargo test -p mh-service`), ADR-0032 Category B
//! extracted the closure body into
//! `observability::metrics::record_token_refresh_metrics`. Calling that fn
//! directly from an integration test exercises the same production code path
//! that the closure invokes at runtime, end-to-end through
//! `observability::metrics::record_token_refresh`.
//!
//! The in-module `#[cfg(test)] mod tests` in
//! `src/observability/metrics.rs` also covers the per-`error_category` matrix;
//! this file adds a `tests/` reference so `validate-metric-coverage.sh` sees
//! the metric names in component-tier scope per ADR-0032 §Enforcement.

// No `#![expect(clippy::unwrap_used, clippy::expect_used, ...)]` preamble —
// this file's body does not currently invoke either, and `#[expect]` with
// `-D unfulfilled_lint_expectations` fails the build when the lint never
// fires. If future additions introduce `.unwrap()`/`.expect()` in setup
// code, add the attribute at that point per ADR-0002.

use std::time::Duration;

use common::observability::testing::MetricAssertion;
use common::token_manager::TokenRefreshEvent;
use mh_service::observability::metrics::record_token_refresh_metrics;

#[test]
fn success_refresh_emits_token_refresh_metrics_end_to_end() {
    let snap = MetricAssertion::snapshot();
    record_token_refresh_metrics(&TokenRefreshEvent {
        success: true,
        duration: Duration::from_millis(25),
        error_category: None,
    });

    // Histogram first — `Snapshotter::snapshot()` drains histograms on read
    // per `common::observability::testing` §"Delta semantics".
    snap.histogram("mh_token_refresh_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mh_token_refresh_total")
        .with_labels(&[("status", "success")])
        .assert_delta(1);
}

#[test]
fn failed_refresh_emits_token_refresh_metrics_end_to_end() {
    let snap = MetricAssertion::snapshot();
    record_token_refresh_metrics(&TokenRefreshEvent {
        success: false,
        duration: Duration::from_millis(15),
        error_category: Some("http"),
    });

    snap.histogram("mh_token_refresh_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mh_token_refresh_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("mh_token_refresh_failures_total")
        .with_labels(&[("error_type", "http")])
        .assert_delta(1);
}

/// Matrix over the bounded `error_category` set defined by
/// `common::token_manager::error_category` (private mapper at
/// `crates/common/src/token_manager.rs:155-164`). Mirrors MC's
/// `token_refresh_integration.rs` precedent so the failure-counter is
/// component-tier-covered for every category that emits at runtime.
#[test]
fn failed_refresh_emits_failure_counter_for_every_error_category() {
    for error_category in [
        "http",
        "auth_rejected",
        "invalid_response",
        "acquisition_failed",
        "configuration",
        "channel_closed",
    ] {
        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: false,
            duration: Duration::from_millis(5),
            error_category: Some(error_category),
        });
        snap.counter("mh_token_refresh_failures_total")
            .with_labels(&[("error_type", error_category)])
            .assert_delta(1);
    }
}
