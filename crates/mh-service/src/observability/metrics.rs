//! Metrics definitions for Media Handler per ADR-0011
//!
//! All metrics follow Prometheus naming conventions:
//! - `mh_` prefix for Media Handler
//! - `_total` suffix for counters
//! - `_seconds` suffix for duration histograms
//!
//! # Cardinality
//!
//! Labels are bounded to prevent cardinality explosion (ADR-0011). The
//! per-metric label sets, their permitted values and their cardinality are
//! catalogued in `docs/observability/metrics/mh-service.md`, which is the
//! single source of truth for them.
//!
//! They are deliberately **not** re-listed here. Four homes used to restate
//! them and three of the four were wrong: this block claimed `method` had
//! three values and omitted `register_meeting`, the only value the code has
//! ever emitted. A docstring that *points at* a binding cannot drift; one that
//! *restates* an enumeration is a copy.

use common::observability::labels::{KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Initialize Prometheus metrics recorder and return the handle
/// for serving metrics via HTTP.
///
/// ADR-0011: Must be called before any metrics are recorded.
/// Configures histogram buckets aligned with SLO targets.
///
/// # Errors
///
/// Returns error if Prometheus recorder fails to install (e.g., already installed).
pub fn init_metrics_recorder() -> Result<PrometheusHandle, String> {
    PrometheusBuilder::new()
        // GC heartbeat latency buckets - internal service call (p95 < 100ms)
        .set_buckets_for_metric(
            Matcher::Prefix("mh_gc_heartbeat".to_string()),
            &[
                0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set GC heartbeat buckets: {e}"))?
        // GC registration latency buckets - registration can be slower
        .set_buckets_for_metric(
            Matcher::Prefix("mh_gc_registration".to_string()),
            &[0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000],
        )
        .map_err(|e| format!("Failed to set GC registration buckets: {e}"))?
        // Token refresh latency buckets
        .set_buckets_for_metric(
            Matcher::Prefix("mh_token_refresh".to_string()),
            &[0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000],
        )
        .map_err(|e| format!("Failed to set token refresh buckets: {e}"))?
        // WebTransport handshake latency buckets (R-26)
        .set_buckets_for_metric(
            Matcher::Prefix("mh_webtransport_handshake".to_string()),
            &[
                0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000,
            ],
        )
        .map_err(|e| format!("Failed to set WebTransport handshake buckets: {e}"))?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus recorder: {e}"))
}

/// Record a GC registration attempt.
///
/// Metric: `mh_gc_registration_total`
/// Labels: `status` (success | error)
/// Cardinality: 2
pub fn record_gc_registration(status: &str) {
    counter!("mh_gc_registration_total", "status" => status.to_string()).increment(1);
}

/// Record GC registration RPC latency.
///
/// Metric: `mh_gc_registration_duration_seconds`
/// Labels: none
/// Buckets: [0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000]
pub fn record_gc_registration_latency(duration: Duration) {
    histogram!("mh_gc_registration_duration_seconds").record(duration.as_secs_f64());
}

/// Record a GC heartbeat (load report) attempt.
///
/// Metric: `mh_gc_heartbeats_total`
/// Labels: `status` (success | error)
/// Cardinality: 2
pub fn record_gc_heartbeat(status: &str) {
    counter!("mh_gc_heartbeats_total", "status" => status.to_string()).increment(1);
}

/// Record GC heartbeat RPC latency.
///
/// Metric: `mh_gc_heartbeat_latency_seconds`
/// Labels: none
/// Buckets: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
pub fn record_gc_heartbeat_latency(duration: Duration) {
    histogram!("mh_gc_heartbeat_latency_seconds").record(duration.as_secs_f64());
}

/// Record a token refresh attempt.
///
/// Metric: `mh_token_refresh_total`
/// Labels: `status` (success | error)
/// Cardinality: 2
///
/// On error, also increments `mh_token_refresh_failures_total` with `error_type`.
pub fn record_token_refresh(status: &str, error_type: Option<&str>, duration: Duration) {
    counter!("mh_token_refresh_total", "status" => status.to_string()).increment(1);
    histogram!("mh_token_refresh_duration_seconds").record(duration.as_secs_f64());

    if let Some(err_type) = error_type {
        counter!(
            "mh_token_refresh_failures_total",
            "error_type" => err_type.to_string()
        )
        .increment(1);
    }
}

/// Record metrics for a token-refresh attempt (ADR-0032 Category B extraction).
///
/// Callable from `main.rs`'s `TokenManager::with_on_refresh` closure and from
/// unit/integration tests. Maps `TokenRefreshEvent.success: bool` to the
/// bounded `status` label and forwards `error_category` + `duration` into
/// `record_token_refresh`. Production emission is byte-identical to the prior
/// inline closure body at `main.rs:114-122`.
pub fn record_token_refresh_metrics(event: &common::token_manager::TokenRefreshEvent) {
    let status = if event.success { "success" } else { "error" };
    record_token_refresh(status, event.error_category, event.duration);
}

/// The only method on `MediaHandlerService`, as a metric label value.
///
/// `internal.proto`'s service block is the single source of truth for this
/// value set: ADR-0036 §8 makes meeting registration the control plane, so the
/// service "gains FIELDS rather than sibling RPCs" and is expected to keep
/// exactly one method. The
/// `register`, `route_media` and `stream_telemetry` values retired with the
/// 2026-09-01 reshape can never appear again — the proto's tombstone block
/// forbids resurrecting the names.
const GRPC_METHOD_REGISTER_MEETING: &str = "register_meeting";

/// Record an incoming gRPC request from MC.
///
/// Metric: `mh_grpc_requests_total`
/// Labels: `method` (single value, bound by `GRPC_METHOD_REGISTER_MEETING`), `status` (success | error)
/// Cardinality: 2
///
/// The `method` label stays on the series — runbook queries and dashboard
/// panels select on `method="register_meeting"`, and an absent label yields an
/// empty result rather than an error, so removing it would silently blank them.
/// The *parameter* is gone, which is what makes the single value structural: a
/// second one cannot be introduced from a call site.
pub fn record_grpc_request(status: &str) {
    counter!(
        "mh_grpc_requests_total",
        "method" => GRPC_METHOD_REGISTER_MEETING,
        "status" => status.to_string()
    )
    .increment(1);
}

/// Record the outcome of an ADR-0036 §8 forwarding-policy apply.
///
/// Metric: `mh_media_policy_applies_total`
/// Labels: `outcome` (5 values, see [`PolicyApplyOutcome`]), `key_custody` (single value `operator`)
/// Cardinality: 5
///
/// Counts **registrations whose policy MH considered** — every terminal path
/// from the first read of a policy-bearing field (`egress_streams`,
/// `selection_rules`, `policy_generation`) onward, exactly once. Checks that
/// read only the caller-identity and reachability scalars (`meeting_id`,
/// `mc_id`, `mc_grpc_endpoint`) are *pre-boundary* and stay on
/// `mh_grpc_requests_total{status="error"}` alone: "MC's assignment computation
/// produced a policy MH will not apply" and "this caller's endpoint is
/// malformed" have different owners and different remedies, and folding them
/// into one series makes the sum unusable as a denominator.
///
/// The label key is `outcome`, not `status`. `status` is `label-taxonomy.md`'s
/// *coarse, fleet-wide shared* classification; this is a fine-grained,
/// metric-local taxonomy in which each value names a distinct remedy — and
/// `internal.proto` already names the MC-side counterpart of this same RPC
/// `outcome`, so both ends of the handshake carry one label key.
///
/// Emits no generation value, no `meeting_id` (raw or hashed), and no stream
/// identity. Generations belong in the log line and in this metric's *value*;
/// as labels they would be unbounded, one new series per policy change.
pub fn record_media_policy_apply(outcome: PolicyApplyOutcome) {
    counter!(
        "mh_media_policy_applies_total",
        "outcome" => outcome.as_label(),
        KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
    )
    .increment(1);
}

/// Bounded `outcome` label values for `mh_media_policy_applies_total`.
///
/// An enum rather than a free `&str` so the label set is closed at the type
/// level: a typo or a sixth value is a compile error, not a new time series
/// discovered in production.
///
/// Each value exists because its **remedy differs**, which is the test for
/// whether a bounded outcome label is doing any work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyApplyOutcome {
    /// The live forward path reflects the generation MC sent. Covers a fresh
    /// install and an idempotent re-assert of an already-installed generation.
    ///
    /// The re-assert case belongs here, not under `RejectedStale`: ADR-0036
    /// §8's cadence re-asserts every meeting every <=10 s in perfect health, so
    /// counting it as a rejection would drive that series monotonically upward
    /// in the steady state and make any alert on it dead on arrival.
    Applied,
    /// A generation strictly lower than the installed one was ignored.
    /// Reordered or retried delivery on the MC→MH path; abnormal.
    RejectedStale,
    /// `policy_generation` was 0 — MC named no generation.
    ///
    /// The **expected steady state for the whole task-11→task-13 window**, when
    /// MC has not yet begun emitting real generations. It is also the value the
    /// *rejection* will land on once task 13 turns enforcement on: two eras,
    /// one value, no rename, and the series falls to zero exactly when MC
    /// starts sending >= 1.
    NoGeneration,
    /// The policy failed structural validation. MC sent bad policy; the remedy
    /// is upstream. Distinct from `ApplyFailed` so an MC policy bug is never
    /// indistinguishable from an MH internal fault.
    RejectedInvalid,
    /// MH could not install it — mailbox full, apply timed out, actor gone, or
    /// the aggregate egress-edge bound. The prior generation stays live.
    ApplyFailed,
}

impl PolicyApplyOutcome {
    /// Every value of this enum, in catalog order.
    ///
    /// **The one hand-maintained list, and it lives here.** `as_label`'s
    /// wildcard-free `match` is the only thing a new variant forces an update
    /// to; every *enumeration* of the variants elsewhere would compile clean
    /// while staying silently short, and the enumerations that go short first
    /// are the exhaustiveness tests whose whole job is to be complete.
    /// `media-protocol`'s `reject_reasons!` macro and its `ALL_REJECT_REASONS`
    /// slice exist for exactly this failure; that macro is not exported and
    /// lives in a Guarded Shared Area, so this applies the pattern locally.
    /// Every consumer — the recorders' tests, the cardinality test and the
    /// integration label assertions — iterates this slice.
    /// The length is written out rather than inferred: it is the catalogued
    /// cardinality of the `outcome` label, so a variant added to this array
    /// without the catalog and the dashboard being revisited fails to compile
    /// here first.
    pub const ALL: [Self; 5] = [
        Self::Applied,
        Self::RejectedStale,
        Self::NoGeneration,
        Self::RejectedInvalid,
        Self::ApplyFailed,
    ];

    /// The wire label value.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::RejectedStale => "rejected_stale",
            Self::NoGeneration => "no_generation",
            Self::RejectedInvalid => "rejected_invalid",
            Self::ApplyFailed => "apply_failed",
        }
    }
}

/// Record an error for the global error counter.
///
/// Metric: `mh_errors_total`
/// Labels: `operation`, `error_type`, `status_code`
/// Cardinality: bounded by `MhError` variants x operations
pub fn record_error(operation: &str, error_type: &str, status_code: u16) {
    counter!(
        "mh_errors_total",
        "operation" => operation.to_string(),
        "error_type" => error_type.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

/// Record a WebTransport connection event (R-26).
///
/// Metric: `mh_webtransport_connections_total`
/// Labels: `status` (accepted | rejected | error)
/// Cardinality: 3
pub fn record_webtransport_connection(status: &str) {
    counter!("mh_webtransport_connections_total", "status" => status.to_string()).increment(1);
}

/// Record WebTransport handshake duration (R-26).
///
/// Metric: `mh_webtransport_handshake_duration_seconds`
/// Labels: none
/// Buckets: [0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000]
pub fn record_webtransport_handshake_duration(duration: Duration) {
    histogram!("mh_webtransport_handshake_duration_seconds").record(duration.as_secs_f64());
}

/// Set the active WebTransport connections gauge (R-26).
///
/// Metric: `mh_active_connections`
/// Labels: none
pub fn set_active_connections(count: f64) {
    gauge!("mh_active_connections").set(count);
}

/// Record a `RegisterMeeting` provisional-accept timeout (R-26).
///
/// Metric: `mh_register_meeting_timeouts_total`
/// Labels: none
/// Cardinality: 1
///
/// Fires from the timeout arm of
/// `webtransport::connection::await_meeting_registration` when a
/// WebTransport client was accepted before MC finished assigning the
/// meeting to this MH, and `MC::RegisterMeeting` did not arrive within
/// the configured `register_meeting_timeout`. The connection is then
/// disconnected.
///
/// Does NOT fire on shutdown-driven cancellation or successful
/// registration (separate arms). This invariant is enforced by behavioral
/// tests co-located with the helper.
///
/// A non-zero rate signals MC → MH `RegisterMeeting` latency issues or a
/// client-side sequencing bug (client connected before MC finished assignment).
pub fn record_register_meeting_timeout() {
    counter!("mh_register_meeting_timeouts_total").increment(1);
}

/// Record an MC notification delivery attempt (R-16/R-17).
///
/// Metric: `mh_mc_notifications_total`
/// Labels: `event_type` (connected | disconnected), `status` (success | error)
/// Cardinality: 4 (2 event types x 2 statuses)
pub fn record_mc_notification(event_type: &str, status: &str) {
    counter!(
        "mh_mc_notifications_total",
        "event_type" => event_type.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
}

/// Record a JWT validation attempt (R-27).
///
/// Metric: `mh_jwt_validations_total`
/// Labels: `result`, `token_type`, `failure_reason`
///
/// Result values: "success", "failure"
/// Token type values: "meeting", "service"
/// Failure reason values: `none` (success), `signature_invalid`, `expired`,
///   `scope_mismatch`, `malformed`, `validation_failed`
/// Cardinality: bounded (2 x 2 x 6 = 24 max, but most combos are sparse in practice)
pub fn record_jwt_validation(result: &str, token_type: &str, failure_reason: &str) {
    counter!("mh_jwt_validations_total",
        "result" => result.to_string(),
        "token_type" => token_type.to_string(),
        "failure_reason" => failure_reason.to_string()
    )
    .increment(1);
}

// ============================================================================
// gRPC Auth Layer 2 Metrics (ADR-0003)
// ============================================================================

/// Record a caller `service_type` rejection by Layer 2 routing.
///
/// Metric: `mh_caller_type_rejected_total`
/// Labels: `grpc_service`, `expected_type`, `actual_type`
///
/// Cardinality: 1 x 1 x 3 = 3 max (1 gRPC service, 1 expected type, ~3 actual types + "unknown")
///
/// ALERT: Any non-zero value indicates a bug or misconfiguration — a service
/// is presenting a valid token but calling the wrong gRPC endpoint.
pub fn record_caller_type_rejected(grpc_service: &str, expected_type: &str, actual_type: &str) {
    counter!("mh_caller_type_rejected_total",
        "grpc_service" => grpc_service.to_string(),
        "expected_type" => expected_type.to_string(),
        "actual_type" => actual_type.to_string()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests execute the metric recording functions to ensure code coverage.
    // The metrics crate will record to a global no-op recorder if none is installed,
    // which is sufficient for coverage testing.
    //
    // Per ADR-0002: These tests do not panic on missing recorder.

    #[test]
    fn test_record_gc_registration() {
        record_gc_registration("success");
        record_gc_registration("error");
    }

    #[test]
    fn test_record_gc_registration_latency() {
        record_gc_registration_latency(Duration::from_millis(50));
        record_gc_registration_latency(Duration::from_millis(500));
        record_gc_registration_latency(Duration::from_secs(2));
    }

    #[test]
    fn test_record_gc_heartbeat() {
        record_gc_heartbeat("success");
        record_gc_heartbeat("error");
    }

    #[test]
    fn test_record_gc_heartbeat_latency() {
        record_gc_heartbeat_latency(Duration::from_millis(5));
        record_gc_heartbeat_latency(Duration::from_millis(50));
        record_gc_heartbeat_latency(Duration::from_millis(500));
    }

    #[test]
    fn test_record_token_refresh() {
        // Success path
        record_token_refresh("success", None, Duration::from_millis(50));

        // Error paths with different error types
        record_token_refresh("error", Some("http"), Duration::from_millis(100));
        record_token_refresh("error", Some("auth_rejected"), Duration::from_millis(200));
        record_token_refresh("error", Some("invalid_response"), Duration::from_millis(30));
        record_token_refresh(
            "error",
            Some("acquisition_failed"),
            Duration::from_millis(10),
        );
    }

    #[test]
    fn test_record_grpc_request() {
        // Both combinations: 1 method x 2 statuses. `MediaHandlerService` has
        // exactly one RPC by design (ADR-0036 §8), and the method label value
        // is now a const inside the recorder rather than a parameter.
        record_grpc_request("success");
        record_grpc_request("error");
    }

    #[test]
    fn test_record_media_policy_apply_covers_every_outcome() {
        for outcome in PolicyApplyOutcome::ALL {
            record_media_policy_apply(outcome);
        }
    }

    #[test]
    fn test_policy_apply_outcome_labels_are_distinct_and_stable() {
        let labels: Vec<&str> = PolicyApplyOutcome::ALL
            .iter()
            .map(|o| o.as_label())
            .collect();
        // The expected side is spelled out on purpose: it is the wire
        // contract, and a test that derived it from `as_label` would assert
        // nothing. What must NOT be hand-listed is the variant set, which is
        // why the left side iterates `ALL`.
        assert_eq!(
            labels,
            [
                "applied",
                "rejected_stale",
                "no_generation",
                "rejected_invalid",
                "apply_failed"
            ],
            "outcome label values are wire-visible; a rename silently breaks every dashboard and alert selecting on them"
        );
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(
            unique.len(),
            labels.len(),
            "outcome labels must be distinct"
        );
    }

    #[test]
    fn test_record_error() {
        record_error("gc_registration", "grpc", 503);
        record_error("gc_heartbeat", "grpc", 503);
        record_error("token_refresh", "http", 500);
        record_error("grpc_service", "internal", 500);
    }

    #[test]
    fn test_record_webtransport_connection() {
        record_webtransport_connection("accepted");
        record_webtransport_connection("rejected");
        record_webtransport_connection("error");
    }

    #[test]
    fn test_record_webtransport_handshake_duration() {
        record_webtransport_handshake_duration(Duration::from_millis(50));
        record_webtransport_handshake_duration(Duration::from_millis(200));
        record_webtransport_handshake_duration(Duration::from_secs(1));
    }

    #[test]
    fn test_set_active_connections() {
        set_active_connections(0.0);
        set_active_connections(42.0);
        set_active_connections(0.0);
    }

    #[test]
    fn test_record_jwt_validation() {
        record_jwt_validation("success", "meeting", "none");
        record_jwt_validation("failure", "meeting", "validation_failed");
        record_jwt_validation("success", "service", "none");
        record_jwt_validation("failure", "service", "signature_invalid");
        record_jwt_validation("failure", "service", "expired");
        record_jwt_validation("failure", "service", "malformed");
        record_jwt_validation("failure", "service", "scope_mismatch");
    }

    #[test]
    fn test_record_caller_type_rejected() {
        // Test representative label combinations (ADR-0003 Layer 2)
        record_caller_type_rejected(
            "MediaHandlerService",
            "meeting-controller",
            "global-controller",
        );
        record_caller_type_rejected("MediaHandlerService", "meeting-controller", "unknown");
    }

    #[test]
    fn test_record_mc_notification() {
        // All 4 combinations: 2 events x 2 statuses
        record_mc_notification("connected", "success");
        record_mc_notification("connected", "error");
        record_mc_notification("disconnected", "success");
        record_mc_notification("disconnected", "error");
    }

    #[test]
    fn test_cardinality_bounds() {
        // Verify status labels are bounded to 2 values
        let valid_statuses = ["success", "error"];
        for status in &valid_statuses {
            record_gc_registration(status);
            record_gc_heartbeat(status);
        }

        // The `method` label is single-valued by construction — the value is a
        // const inside the recorder, so cardinality is bounded by the 2
        // statuses alone.
        for status in &valid_statuses {
            record_grpc_request(status);
        }

        // The `outcome` label is bounded by the PolicyApplyOutcome enum.
        for outcome in PolicyApplyOutcome::ALL {
            record_media_policy_apply(outcome);
        }

        // Verify error_type labels are bounded by MhError variants
        let valid_error_types = [
            "grpc",
            "not_registered",
            "config",
            "internal",
            "token_acquisition",
            "token_timeout",
        ];
        for error_type in &valid_error_types {
            record_error("test_op", error_type, 500);
        }
    }

    #[test]
    fn test_prometheus_metrics_endpoint_integration() {
        // NOT an ADR-0032 coverage assertion — this test pre-dates ADR-0032
        // and lives in `src/` (outside the guard's `tests/**/*.rs` scan), so
        // the `record_*` calls here are NOT the "test-side reference to a
        // production emission path" the guard is designed to catch. This is
        // an ADR-0011 plumbing test: it exercises every `record_*` wrapper
        // to verify the `counter!`/`histogram!` macro + recorder + exporter
        // wiring captures emissions (the guard's coverage-fidelity
        // requirement is separately met by the per-caller component tests in
        // `crates/mh-service/tests/*.rs` — `gc_integration.rs`,
        // `webtransport_integration.rs`, `webtransport_accept_loop_integration.rs`,
        // etc.). Future readers: do NOT propagate "test-body `record_*`" to
        // `crates/mh-service/tests/`; that DOES dodge the guard.
        //
        // Migrated from hand-rolled `DebuggingRecorder::new() + recorder.install()`
        // to `common::observability::testing::MetricAssertion` purely as DRY
        // cleanup (removes one of two hand-rolled-install sites tracked in
        // `docs/TODO.md`). `MetricAssertion::snapshot()` binds a per-thread
        // `DebuggingRecorder` for this test, dropping the global-install
        // isolation pain the original inline comment called out.
        use common::observability::testing::MetricAssertion;

        let snap = MetricAssertion::snapshot();

        // Record the same set of MH metrics the legacy test exercised so the
        // "every recorded metric actually lands in the recorder" intent is
        // preserved. Histograms are asserted before counters because
        // `Snapshotter::snapshot` drains histogram observations on read
        // (see common::observability::testing §"Delta semantics").
        record_gc_registration("success");
        record_gc_registration("error");
        record_gc_registration_latency(Duration::from_millis(100));
        record_gc_heartbeat("success");
        record_gc_heartbeat("error");
        record_gc_heartbeat_latency(Duration::from_millis(10));
        record_token_refresh("success", None, Duration::from_millis(50));
        record_token_refresh("error", Some("http"), Duration::from_millis(100));
        record_grpc_request("success");
        record_grpc_request("error");
        record_media_policy_apply(PolicyApplyOutcome::Applied);
        record_error("gc_heartbeat", "grpc", 503);

        // Single histogram assertion — `Snapshotter::snapshot()` drains
        // every histogram across all names on read, so asserting multiple
        // histogram names after each other would see zero on the 2nd+ call.
        // A single representative observation proves the recorder captured
        // histogram emissions; the per-histogram-name coverage comes from
        // the individual `record_*_*_seconds` unit tests earlier in this
        // module + the gc_integration.rs assertions on gc-specific names.
        snap.histogram("mh_gc_registration_duration_seconds")
            .assert_observation_count_at_least(1);

        // Counters — one representative per name+label tuple emitted above.
        snap.counter("mh_gc_registration_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mh_gc_registration_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.counter("mh_gc_heartbeats_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mh_gc_heartbeats_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
    }

    // =======================================================================
    // record_token_refresh_metrics (ADR-0032 Category B)
    // =======================================================================
    //
    // These tests cover the `TokenRefreshEvent -> metrics` mapping lifted out
    // of `main.rs` per ADR-0032 Step 2. Each test takes its own per-thread
    // `MetricAssertion::snapshot()`; histograms are asserted first because
    // `Snapshotter::snapshot()` drains histogram observations on read.

    #[test]
    fn record_token_refresh_metrics_success_event_emits_counter_and_histogram() {
        use common::observability::testing::MetricAssertion;
        use common::token_manager::TokenRefreshEvent;

        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: true,
            duration: Duration::from_millis(42),
            error_category: None,
        });

        snap.histogram("mh_token_refresh_duration_seconds")
            .assert_observation_count_at_least(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
    }

    // Each failure branch exercises a distinct `error_category` value. The
    // categories are bounded `&'static str`s produced by
    // `common::token_manager::error_category`; enumerating them here guards
    // the mapping against silent regression.

    #[test]
    fn record_token_refresh_metrics_error_http() {
        assert_token_refresh_failure_emits("http");
    }

    #[test]
    fn record_token_refresh_metrics_error_auth_rejected() {
        assert_token_refresh_failure_emits("auth_rejected");
    }

    #[test]
    fn record_token_refresh_metrics_error_invalid_response() {
        assert_token_refresh_failure_emits("invalid_response");
    }

    #[test]
    fn record_token_refresh_metrics_error_acquisition_failed() {
        assert_token_refresh_failure_emits("acquisition_failed");
    }

    #[test]
    fn record_token_refresh_metrics_error_configuration() {
        assert_token_refresh_failure_emits("configuration");
    }

    #[test]
    fn record_token_refresh_metrics_error_channel_closed() {
        assert_token_refresh_failure_emits("channel_closed");
    }

    fn assert_token_refresh_failure_emits(category: &'static str) {
        use common::observability::testing::MetricAssertion;
        use common::token_manager::TokenRefreshEvent;

        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: false,
            duration: Duration::from_millis(10),
            error_category: Some(category),
        });

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
