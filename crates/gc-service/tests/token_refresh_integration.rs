//! Integration cover for `record_token_refresh_metrics` (Cat B extraction)
//! per ADR-0032 Step 5 §Cluster 13.
//!
//! The full per-failure-class matrix (success + every `error_category`
//! variant) lives in `crates/gc-service/src/observability/metrics.rs::tests`
//! at `record_token_refresh_metrics_*`. This file exercises one
//! representative path per status to satisfy the
//! `validate-metric-coverage.sh` guard's `tests/**/*.rs` scan for the three
//! token-refresh metric names (`gc_token_refresh_total`,
//! `gc_token_refresh_duration_seconds`, `gc_token_refresh_failures_total`).
//!
//! Mirrors `crates/mc-service/tests/token_refresh_integration.rs` and
//! `crates/mh-service/tests/token_refresh_integration.rs` — the per-service
//! Cat B canonical pattern (ADR-0032 Step 5; cross-service consolidation
//! rejected).
//!
//! Each `#[test]` runs on the cargo test runner's per-test thread, so the
//! `MetricAssertion` per-thread recorder isolation applies. No tokio runtime
//! pinning is needed because these calls are synchronous.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use ::common::token_manager::TokenRefreshEvent;
use gc_service::observability::metrics::record_token_refresh_metrics;

#[test]
fn record_token_refresh_metrics_success_emits_status_success() {
    let snap = MetricAssertion::snapshot();
    record_token_refresh_metrics(&TokenRefreshEvent {
        success: true,
        duration: Duration::from_millis(50),
        error_category: None,
    });

    // Histogram first (drain-on-read).
    snap.histogram("gc_token_refresh_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("gc_token_refresh_total")
        .with_labels(&[("status", "success")])
        .assert_delta(1);
}

#[test]
fn record_token_refresh_metrics_failure_emits_status_error_and_failures_counter() {
    let snap = MetricAssertion::snapshot();
    record_token_refresh_metrics(&TokenRefreshEvent {
        success: false,
        duration: Duration::from_millis(100),
        error_category: Some("http"),
    });

    snap.histogram("gc_token_refresh_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("gc_token_refresh_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("gc_token_refresh_failures_total")
        .with_labels(&[("error_type", "http")])
        .assert_delta(1);
}
