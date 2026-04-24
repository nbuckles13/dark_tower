//! Token-refresh metric emission extracted from the `main.rs`
//! `TokenManager::with_on_refresh` closure.
//!
//! # Why this module exists (ADR-0032 Category B)
//!
//! The metric emission previously lived inside a closure passed to
//! `TokenManagerConfig::with_on_refresh` in `main.rs`. Closures captured
//! inside `main.rs` are not reachable from tests — `cargo test -p mh-service`
//! builds the library, not the binary, so there is no way to assert on
//! metric emissions from that closure.
//!
//! Lifting the closure body into a stateless function here makes the
//! emission path testable without adding any production-code test affordance
//! (no hooks, no opt-in channels, no `#[cfg(test)]` carve-outs). The closure
//! in `main.rs` becomes a one-line call into this fn. Production behaviour
//! is byte-identical: same counter, same histogram, same labels, same order.
//!
//! The parallel structure (service-local `token_refresh_metrics.rs` holding a
//! tiny fn) is the template for the equivalent extraction in
//! `crates/mc-service/src/main.rs` under ADR-0032 Step 3.

use common::token_manager::TokenRefreshEvent;

use crate::observability::metrics;

/// Record metrics for a token-refresh attempt.
///
/// Maps `TokenRefreshEvent.success: bool` to the bounded `status` label
/// (`"success"` | `"error"`) and forwards the event's `error_category` +
/// `duration` to the service-level `record_token_refresh` helper.
///
/// Invoked from the `TokenManager::with_on_refresh` closure in `main.rs`.
pub fn record_token_refresh_metrics(event: &TokenRefreshEvent) {
    let status = if event.success { "success" } else { "error" };
    metrics::record_token_refresh(status, event.error_category, event.duration);
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::observability::testing::MetricAssertion;
    use std::time::Duration;

    #[test]
    fn success_event_emits_counter_histogram_no_failure() {
        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: true,
            duration: Duration::from_millis(42),
            error_category: None,
        });

        // Histogram is asserted first because `Snapshotter::snapshot` drains
        // histogram observations on read (see common::observability::testing
        // §"Delta semantics"); counter asserts are idempotent on re-read.
        snap.histogram("mh_token_refresh_duration_seconds")
            .assert_observation_count_at_least(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
    }

    // Each failure branch exercises a distinct `error_category` value. The
    // categories are defined as bounded `&'static str`s in
    // `common::token_manager::error_category`; enumerating them here makes
    // sure none of the mappings silently regress.

    #[test]
    fn error_event_http_emits_counter_histogram_and_failure_label() {
        assert_failure_emits("http");
    }

    #[test]
    fn error_event_auth_rejected_emits_failure_label() {
        assert_failure_emits("auth_rejected");
    }

    #[test]
    fn error_event_invalid_response_emits_failure_label() {
        assert_failure_emits("invalid_response");
    }

    #[test]
    fn error_event_acquisition_failed_emits_failure_label() {
        assert_failure_emits("acquisition_failed");
    }

    #[test]
    fn error_event_configuration_emits_failure_label() {
        assert_failure_emits("configuration");
    }

    #[test]
    fn error_event_channel_closed_emits_failure_label() {
        assert_failure_emits("channel_closed");
    }

    fn assert_failure_emits(category: &'static str) {
        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: false,
            duration: Duration::from_millis(10),
            error_category: Some(category),
        });

        // Histogram is asserted before counters because `Snapshotter::snapshot`
        // drains histogram observations on read (see common::observability
        // ::testing §"Delta semantics"); counter asserts are idempotent.
        snap.histogram("mh_token_refresh_duration_seconds")
            .assert_observation_count_at_least(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.counter("mh_token_refresh_failures_total")
            .with_labels(&[("error_type", category)])
            .assert_delta(1);
    }
}
