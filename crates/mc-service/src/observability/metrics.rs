//! Metrics definitions for Meeting Controller per ADR-0011 and ADR-0023 Section 11
//!
//! All metrics follow Prometheus naming conventions:
//! - `mc_` prefix for Meeting Controller
//! - `_total` suffix for counters
//! - `_seconds` suffix for duration histograms
//!
//! # Cardinality
//!
//! Labels are bounded to prevent cardinality explosion (ADR-0011):
//! - `actor_type`: 3 values max (controller, meeting, participant)
//! - `operation`: bounded by Redis commands (~10 values)
//! - `reason`: bounded fencing reasons (2-3 values)
//!
//! Maximum 1,000 unique label combinations per metric.

use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Initialize Prometheus metrics recorder and return the handle
/// for serving metrics via HTTP.
///
/// ADR-0011: Must be called before any metrics are recorded.
/// Configures histogram buckets aligned with SLO targets:
/// - GC heartbeat p95 < 100ms
/// - Redis latency p99 < 10ms
///
/// # Errors
///
/// Returns error if Prometheus recorder fails to install (e.g., already installed).
pub fn init_metrics_recorder() -> Result<PrometheusHandle, String> {
    PrometheusBuilder::new()
        // GC heartbeat latency buckets - internal service call (p95 < 100ms)
        .set_buckets_for_metric(
            Matcher::Prefix("mc_gc_heartbeat".to_string()),
            &[
                0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set GC heartbeat buckets: {e}"))?
        // Redis latency buckets - internal service call (like DB queries)
        .set_buckets_for_metric(
            Matcher::Prefix("mc_redis".to_string()),
            &[
                0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set Redis latency buckets: {e}"))?
        // Token refresh buckets - SLO-aligned (matches GC buckets)
        .set_buckets_for_metric(
            Matcher::Prefix("mc_token_refresh".to_string()),
            &[0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000],
        )
        .map_err(|e| format!("Failed to set token refresh buckets: {e}"))?
        // Session join buckets - extended to 5s (join includes actor processing)
        .set_buckets_for_metric(
            Matcher::Prefix("mc_session_join".to_string()),
            &[
                0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000,
            ],
        )
        .map_err(|e| format!("Failed to set session join buckets: {e}"))?
        // MH RegisterMeeting RPC buckets - internal service call
        .set_buckets_for_metric(
            Matcher::Prefix("mc_register_meeting".to_string()),
            &[
                0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set register meeting buckets: {e}"))?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus metrics recorder: {e}"))
}

// ============================================================================
// Connection & Meeting Metrics (Gauges)
// ============================================================================

/// Set the number of active WebTransport connections.
///
/// Metric: `mc_connections_active`
/// Labels: none
///
/// This is updated by the actor system when connections are established/closed.
pub fn set_connections_active(count: u64) {
    // u64 to f64 conversion is safe for realistic connection counts (< 2^53)
    #[allow(clippy::cast_precision_loss)]
    gauge!("mc_connections_active").set(count as f64);
}

/// Set the number of active meetings.
///
/// Metric: `mc_meetings_active`
/// Labels: none
///
/// This is updated by the actor system when meetings are created/removed.
pub fn set_meetings_active(count: u64) {
    // u64 to f64 conversion is safe for realistic meeting counts (< 2^53)
    #[allow(clippy::cast_precision_loss)]
    gauge!("mc_meetings_active").set(count as f64);
}

// ============================================================================
// Actor Mailbox Metrics (Gauges)
// ============================================================================

/// Set the mailbox depth for an actor type.
///
/// Metric: `mc_actor_mailbox_depth`
/// Labels: `actor_type` (controller, meeting, connection)
///
/// Cardinality: 3 (bounded by ActorType enum)
///
/// Used for backpressure monitoring. High values indicate the actor is
/// falling behind in message processing.
pub fn set_actor_mailbox_depth(actor_type: &str, depth: usize) {
    // usize to f64 conversion is safe for realistic mailbox depths
    #[allow(clippy::cast_precision_loss)]
    gauge!("mc_actor_mailbox_depth", "actor_type" => actor_type.to_string()).set(depth as f64);
}

// ============================================================================
// Latency Metrics (Histograms)
// ============================================================================

/// Record Redis operation latency.
///
/// Metric: `mc_redis_latency_seconds`
/// Labels: `operation`
///
/// Cardinality: ~10 (bounded by Redis command types)
/// Operations: get, set, del, incr, hset, hget, eval, zadd, zrange, etc.
///
/// SLO target: p99 < 10ms for Redis operations
pub fn record_redis_latency(operation: &str, duration: Duration) {
    histogram!("mc_redis_latency_seconds", "operation" => operation.to_string())
        .record(duration.as_secs_f64());
}

// ============================================================================
// Fencing Metrics (Counters)
// ============================================================================

/// Record a fenced-out event (split-brain recovery).
///
/// Metric: `mc_fenced_out_total`
/// Labels: `reason`
///
/// Cardinality: 2-3 (bounded by fencing reasons)
/// Reasons: stale_generation, concurrent_write
///
/// Non-zero values indicate split-brain scenarios occurred.
/// Should be rare in normal operation - investigate if rate > 0.1/min.
pub fn record_fenced_out(reason: &str) {
    counter!("mc_fenced_out_total", "reason" => reason.to_string()).increment(1);
}

// ============================================================================
// Additional Operational Metrics
// ============================================================================

/// Record an actor panic event.
///
/// Metric: `mc_actor_panics_total`
/// Labels: `actor_type`
///
/// ALERT: Any non-zero value indicates a bug and should trigger investigation.
pub fn record_actor_panic(actor_type: &str) {
    counter!("mc_actor_panics_total", "actor_type" => actor_type.to_string()).increment(1);
}

/// Record messages dropped due to backpressure.
///
/// Metric: `mc_messages_dropped_total`
/// Labels: `actor_type`
///
/// Non-zero values indicate the system is overloaded.
pub fn record_message_dropped(actor_type: &str) {
    counter!("mc_messages_dropped_total", "actor_type" => actor_type.to_string()).increment(1);
}

/// Record GC heartbeat result.
///
/// Metric: `mc_gc_heartbeats_total`
/// Labels: `status` (success, error), `type` (fast, comprehensive)
///
/// Cardinality: 4 (2 statuses x 2 types)
pub fn record_gc_heartbeat(status: &str, heartbeat_type: &str) {
    counter!("mc_gc_heartbeats_total", "status" => status.to_string(), "type" => heartbeat_type.to_string())
        .increment(1);
}

/// Record GC heartbeat latency.
///
/// Metric: `mc_gc_heartbeat_latency_seconds`
/// Labels: `type` (fast, comprehensive)
///
/// SLO target: p99 < 100ms for heartbeats
pub fn record_gc_heartbeat_latency(heartbeat_type: &str, duration: Duration) {
    histogram!("mc_gc_heartbeat_latency_seconds", "type" => heartbeat_type.to_string())
        .record(duration.as_secs_f64());
}

// ============================================================================
// Token Manager Metrics (ADR-0010 Section 4a)
// ============================================================================

/// Record token refresh attempt.
///
/// Emits three metrics per the metrics catalog (`docs/observability/metrics/mc.md`):
/// - `mc_token_refresh_total` counter (labels: `status`)
/// - `mc_token_refresh_duration_seconds` histogram (no labels)
/// - `mc_token_refresh_failures_total` counter (labels: `error_type`, on failure only)
///
/// Called from the `TokenRefreshCallback` wired in `main.rs`.
///
/// # Arguments
///
/// * `status` - "success" or "error"
/// * `error_type` - Error category for failures (e.g., "http", "auth_rejected")
/// * `duration` - Duration of the acquire_token call (excludes backoff)
pub fn record_token_refresh(status: &str, error_type: Option<&str>, duration: Duration) {
    histogram!("mc_token_refresh_duration_seconds").record(duration.as_secs_f64());

    counter!("mc_token_refresh_total",
        "status" => status.to_string()
    )
    .increment(1);

    if let Some(err_type) = error_type {
        counter!("mc_token_refresh_failures_total",
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
/// inline closure body at `main.rs:147-155`.
pub fn record_token_refresh_metrics(event: &common::token_manager::TokenRefreshEvent) {
    let status = if event.success { "success" } else { "error" };
    record_token_refresh(status, event.error_category, event.duration);
}

// ============================================================================
// Join Flow Metrics (R-13)
// ============================================================================

/// Record a WebTransport connection acceptance or rejection.
///
/// Metric: `mc_webtransport_connections_total`
/// Labels: `status`
///
/// Status values: "accepted", "rejected", "error"
/// Cardinality: 3
///
/// Recorded in the WebTransport accept loop (`server.rs`).
pub fn record_webtransport_connection(status: &str) {
    counter!("mc_webtransport_connections_total",
        "status" => status.to_string()
    )
    .increment(1);
}

/// Record a JWT validation attempt.
///
/// Metric: `mc_jwt_validations_total`
/// Labels: `result`, `token_type`, `failure_reason`
///
/// Result values: "success", "failure"
/// Token type values: "meeting", "guest", "service"
/// Failure reason values: "none" (success), "signature_invalid", "expired",
///   "missing_token", "scope_mismatch", "malformed"
/// Cardinality: bounded (2 x 3 x 6 = 36 max, but most combos are sparse in practice)
///
/// Recorded in `connection.rs` after JWT validation,
/// `grpc/auth_interceptor.rs` for service tokens.
pub fn record_jwt_validation(result: &str, token_type: &str, failure_reason: &str) {
    counter!("mc_jwt_validations_total",
        "result" => result.to_string(),
        "token_type" => token_type.to_string(),
        "failure_reason" => failure_reason.to_string()
    )
    .increment(1);
}

/// Record a session join attempt.
///
/// Emits three metrics per the GC `record_meeting_join()` pattern:
/// - `mc_session_joins_total` counter (labels: `status`)
/// - `mc_session_join_duration_seconds` histogram (labels: `status`)
/// - `mc_session_join_failures_total` counter (labels: `error_type`, on failure only)
///
/// # Arguments
///
/// * `status` - "success" or "failure"
/// * `error_type` - Error category for failures. Bounded by `McError::error_type_label()`
///   enum variants (e.g., "jwt_validation", "internal", "meeting_not_found",
///   "mc_capacity_exceeded"). `None` for success.
/// * `duration` - Duration from session accept to JoinResponse sent (or error).
pub fn record_session_join(status: &str, error_type: Option<&str>, duration: Duration) {
    histogram!("mc_session_join_duration_seconds",
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("mc_session_joins_total",
        "status" => status.to_string()
    )
    .increment(1);

    if let Some(err_type) = error_type {
        counter!("mc_session_join_failures_total",
            "error_type" => err_type.to_string()
        )
        .increment(1);
    }
}

// ============================================================================
// MH Registration Metrics
// ============================================================================

/// Record a RegisterMeeting RPC attempt to an MH.
///
/// Emits two metrics:
/// - `mc_register_meeting_total` counter (labels: `status`)
/// - `mc_register_meeting_duration_seconds` histogram (no labels)
///
/// # Arguments
///
/// * `status` - "success" or "error"
/// * `duration` - Duration of the RegisterMeeting RPC call
pub fn record_register_meeting(status: &str, duration: Duration) {
    histogram!("mc_register_meeting_duration_seconds").record(duration.as_secs_f64());

    counter!("mc_register_meeting_total",
        "status" => status.to_string()
    )
    .increment(1);
}

// ============================================================================
// MH Coordination Metrics (R-28)
// ============================================================================

/// Record an MH participant notification received by MC.
///
/// Metric: `mc_mh_notifications_received_total`
/// Labels: `event`
///
/// Event values: "connected", "disconnected"
/// Cardinality: 2
///
/// Recorded in `media_coordination.rs` when MH notifies MC
/// of participant connection/disconnection events.
pub fn record_mh_notification(event_type: &str) {
    counter!("mc_mh_notifications_received_total",
        "event" => event_type.to_string()
    )
    .increment(1);
}

/// Record a client-reported media connection failure.
///
/// Metric: `mc_media_connection_failures_total`
/// Labels: `all_failed`
///
/// All-failed values: "true", "false"
/// Cardinality: 2
///
/// Recorded in the WebTransport bridge loop when a client sends
/// a `MediaConnectionFailed` signaling message. Per R-20, no
/// reallocation action is taken; the metric is for observability only.
pub fn record_media_connection_failed(all_failed: bool) {
    counter!("mc_media_connection_failures_total",
        "all_failed" => if all_failed { "true" } else { "false" }.to_string()
    )
    .increment(1);
}

// ============================================================================
// gRPC Auth Layer 2 Metrics (ADR-0003)
// ============================================================================

/// Record a caller service_type rejection by Layer 2 routing.
///
/// Metric: `mc_caller_type_rejected_total`
/// Labels: `grpc_service`, `expected_type`, `actual_type`
///
/// Cardinality: 2 x 3 x 4 = 24 max (bounded by gRPC services and service types + "unknown")
///
/// ALERT: Any non-zero value indicates a bug or misconfiguration — a service
/// is presenting a valid token but calling the wrong gRPC endpoint.
pub fn record_caller_type_rejected(grpc_service: &str, expected_type: &str, actual_type: &str) {
    counter!("mc_caller_type_rejected_total",
        "grpc_service" => grpc_service.to_string(),
        "expected_type" => expected_type.to_string(),
        "actual_type" => actual_type.to_string()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::observability::testing::MetricAssertion;
    use common::token_manager::TokenRefreshEvent;

    // Note: The legacy `test_*` functions below execute the metric recording
    // functions without a recorder — the metrics crate records to a global
    // no-op recorder when none is installed, which is sufficient for coverage
    // testing. They are retained because they exercise constant-folded code
    // paths the per-failure-class tests don't reach (e.g.,
    // `test_cardinality_bounds`).
    //
    // The newer per-failure-class tests in this module use
    // `MetricAssertion::snapshot()` which binds a per-thread `DebuggingRecorder`
    // for the duration of the snapshot — see `common::observability::testing`
    // module docs for the isolation model. These tests live in
    // `#[test]` (not `#[tokio::test]`) blocks: the recorders bind to the test
    // thread and all wrapper calls happen synchronously on it, so no
    // `flavor = "current_thread"` discipline is needed for the metrics-module
    // tests specifically. Tests that drive metric emission through `tokio::spawn`
    // paths live in `crates/mc-service/tests/` and DO need the explicit pinning
    // (see those file headers).
    //
    // Per ADR-0002: These tests do not panic on missing recorder.

    #[test]
    fn test_set_connections_active() {
        // Test with various connection counts
        set_connections_active(0);
        set_connections_active(1);
        set_connections_active(100);
        set_connections_active(10_000);
    }

    #[test]
    fn test_set_meetings_active() {
        // Test with various meeting counts
        set_meetings_active(0);
        set_meetings_active(1);
        set_meetings_active(50);
        set_meetings_active(1000);
    }

    #[test]
    fn test_set_actor_mailbox_depth() {
        // Test with all actor types
        set_actor_mailbox_depth("controller", 0);
        set_actor_mailbox_depth("meeting", 50);
        set_actor_mailbox_depth("connection", 100);
        set_actor_mailbox_depth("meeting", 500); // Warning threshold
        set_actor_mailbox_depth("connection", 200); // Critical threshold
    }

    #[test]
    fn test_record_redis_latency() {
        // Test with various Redis operations
        record_redis_latency("get", Duration::from_micros(500));
        record_redis_latency("set", Duration::from_micros(800));
        record_redis_latency("del", Duration::from_micros(300));
        record_redis_latency("incr", Duration::from_micros(400));
        record_redis_latency("hset", Duration::from_millis(1));
        record_redis_latency("eval", Duration::from_millis(2));
    }

    #[test]
    fn test_record_fenced_out() {
        // Test with various fencing reasons
        record_fenced_out("stale_generation");
        record_fenced_out("concurrent_write");
    }

    #[test]
    fn test_record_actor_panic() {
        // Test with all actor types
        record_actor_panic("controller");
        record_actor_panic("meeting");
        record_actor_panic("connection");
    }

    #[test]
    fn test_record_message_dropped() {
        // Test with all actor types
        record_message_dropped("controller");
        record_message_dropped("meeting");
        record_message_dropped("connection");
    }

    #[test]
    fn test_record_gc_heartbeat() {
        // Test with various statuses and types
        record_gc_heartbeat("success", "fast");
        record_gc_heartbeat("success", "comprehensive");
        record_gc_heartbeat("error", "fast");
        record_gc_heartbeat("error", "comprehensive");
    }

    #[test]
    fn test_record_gc_heartbeat_latency() {
        // Test with various heartbeat types and durations
        record_gc_heartbeat_latency("fast", Duration::from_millis(5));
        record_gc_heartbeat_latency("comprehensive", Duration::from_millis(20));
        record_gc_heartbeat_latency("fast", Duration::from_millis(100)); // Slow
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
    fn test_record_register_meeting() {
        // Success path
        record_register_meeting("success", Duration::from_millis(20));

        // Error path
        record_register_meeting("error", Duration::from_millis(100));
    }

    #[test]
    fn test_record_webtransport_connection() {
        // Test all 3 bounded status values
        record_webtransport_connection("accepted");
        record_webtransport_connection("rejected");
        record_webtransport_connection("error");
    }

    #[test]
    fn test_record_jwt_validation() {
        // Test all bounded combinations (2 results x 3 token types x representative reasons)
        record_jwt_validation("success", "meeting", "none");
        record_jwt_validation("success", "guest", "none");
        record_jwt_validation("success", "service", "none");
        record_jwt_validation("failure", "meeting", "signature_invalid");
        record_jwt_validation("failure", "guest", "expired");
        record_jwt_validation("failure", "service", "scope_mismatch");
        record_jwt_validation("failure", "service", "malformed");
        record_jwt_validation("failure", "service", "missing_token");
    }

    #[test]
    fn test_record_session_join() {
        // Success path
        record_session_join("success", None, Duration::from_millis(200));

        // Failure paths with bounded error types from McError::error_type_label()
        record_session_join("failure", Some("jwt_validation"), Duration::from_millis(5));
        record_session_join("failure", Some("internal"), Duration::from_millis(10));
        record_session_join(
            "failure",
            Some("meeting_not_found"),
            Duration::from_millis(3),
        );
        record_session_join(
            "failure",
            Some("mc_capacity_exceeded"),
            Duration::from_millis(1),
        );
        record_session_join(
            "failure",
            Some("meeting_capacity_exceeded"),
            Duration::from_millis(8),
        );
    }

    #[test]
    fn test_record_mh_notification() {
        // Test all 2 bounded event values
        record_mh_notification("connected");
        record_mh_notification("disconnected");
    }

    #[test]
    fn test_record_caller_type_rejected() {
        // Test representative label combinations (ADR-0003 Layer 2)
        record_caller_type_rejected(
            "MeetingControllerService",
            "global-controller",
            "media-handler",
        );
        record_caller_type_rejected(
            "MediaCoordinationService",
            "media-handler",
            "global-controller",
        );
        record_caller_type_rejected("MeetingControllerService", "global-controller", "unknown");
        record_caller_type_rejected("MediaCoordinationService", "media-handler", "unknown");
    }

    #[test]
    fn test_record_media_connection_failed() {
        // Test both boolean states
        record_media_connection_failed(true);
        record_media_connection_failed(false);
    }

    #[test]
    fn test_cardinality_bounds() {
        // Verify actor_type labels are bounded
        let valid_actor_types = ["controller", "meeting", "participant"];
        for actor_type in &valid_actor_types {
            set_actor_mailbox_depth(actor_type, 10);
            record_actor_panic(actor_type);
            record_message_dropped(actor_type);
        }

        // Verify heartbeat type labels are bounded
        let valid_heartbeat_types = ["fast", "comprehensive"];
        for hb_type in &valid_heartbeat_types {
            record_gc_heartbeat("success", hb_type);
            record_gc_heartbeat_latency(hb_type, Duration::from_millis(10));
        }

        // Verify fencing reason labels are bounded
        let valid_reasons = ["stale_generation", "concurrent_write"];
        for reason in &valid_reasons {
            record_fenced_out(reason);
        }

        // Verify join flow labels are bounded
        let valid_connection_statuses = ["accepted", "rejected", "error"];
        for status in &valid_connection_statuses {
            record_webtransport_connection(status);
        }

        let valid_jwt_results = ["success", "failure"];
        let valid_token_types = ["meeting", "guest", "service"];
        let valid_failure_reasons = [
            "none",
            "signature_invalid",
            "expired",
            "missing_token",
            "scope_mismatch",
            "malformed",
        ];
        for result in &valid_jwt_results {
            for token_type in &valid_token_types {
                for reason in &valid_failure_reasons {
                    record_jwt_validation(result, token_type, reason);
                }
            }
        }

        let valid_join_statuses = ["success", "failure"];
        for status in &valid_join_statuses {
            record_session_join(status, None, Duration::from_millis(100));
        }

        // Verify MH coordination labels are bounded
        let valid_mh_events = ["connected", "disconnected"];
        for event in &valid_mh_events {
            record_mh_notification(event);
        }

        // Verify media connection failure labels are bounded
        record_media_connection_failed(true);
        record_media_connection_failed(false);

        // Verify caller type rejection labels are bounded (ADR-0003)
        record_caller_type_rejected(
            "MeetingControllerService",
            "global-controller",
            "media-handler",
        );
        record_caller_type_rejected(
            "MediaCoordinationService",
            "media-handler",
            "global-controller",
        );
    }

    // ========================================================================
    // ADR-0032 Category B: pure-fn extraction matrix for `record_token_refresh_metrics`
    //
    // The `with_on_refresh` closure at `main.rs:147-155` now calls
    // `record_token_refresh_metrics(&event)` instead of inlining the
    // `success bool → status &str` mapping. This matrix covers the success path
    // plus every `error_category` variant emitted by `common::token_manager`
    // (`http`, `auth_rejected`, `invalid_response`, `acquisition_failed`,
    // `configuration`, `channel_closed`).
    //
    // Histogram-first ordering (drain-on-read) and per-failure-class
    // `assert_delta(0)` adjacency on sibling `error_type` labels per ADR-0032.
    // ========================================================================

    fn make_event(success: bool, error_category: Option<&'static str>) -> TokenRefreshEvent {
        TokenRefreshEvent {
            success,
            duration: Duration::from_millis(42),
            error_category,
        }
    }

    #[test]
    fn record_token_refresh_metrics_success_emits_success_status_no_failure_counter() {
        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&make_event(true, None));

        // Histogram first (drain-on-read).
        snap.histogram("mc_token_refresh_duration_seconds")
            .assert_observation_count_at_least(1);
        snap.counter("mc_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mc_token_refresh_total")
            .with_labels(&[("status", "error")])
            .assert_delta(0);
        // No failure counter on success path — adjacency check across all
        // bounded error_type labels.
        for sibling in &[
            "http",
            "auth_rejected",
            "invalid_response",
            "acquisition_failed",
            "configuration",
            "channel_closed",
        ] {
            snap.counter("mc_token_refresh_failures_total")
                .with_labels(&[("error_type", *sibling)])
                .assert_delta(0);
        }
    }

    #[test]
    fn record_token_refresh_metrics_failure_matrix_per_error_category() {
        // One snapshot per error_category — each must isolate cleanly.
        for category in &[
            "http",
            "auth_rejected",
            "invalid_response",
            "acquisition_failed",
            "configuration",
            "channel_closed",
        ] {
            let snap = MetricAssertion::snapshot();
            record_token_refresh_metrics(&make_event(false, Some(category)));

            // Histogram first.
            snap.histogram("mc_token_refresh_duration_seconds")
                .assert_observation_count_at_least(1);

            snap.counter("mc_token_refresh_total")
                .with_labels(&[("status", "error")])
                .assert_delta(1);
            snap.counter("mc_token_refresh_total")
                .with_labels(&[("status", "success")])
                .assert_delta(0);

            // Named error_type counter delta=1, every sibling delta=0
            // (label-swap-bug catcher per ADR-0032 §Pattern #3).
            snap.counter("mc_token_refresh_failures_total")
                .with_labels(&[("error_type", *category)])
                .assert_delta(1);
            for sibling in &[
                "http",
                "auth_rejected",
                "invalid_response",
                "acquisition_failed",
                "configuration",
                "channel_closed",
            ] {
                if sibling == category {
                    continue;
                }
                snap.counter("mc_token_refresh_failures_total")
                    .with_labels(&[("error_type", *sibling)])
                    .assert_delta(0);
            }
        }
    }

    // ========================================================================
    // Per-cluster MetricAssertion tests — replaces the pre-ADR-0032 hand-rolled
    // `DebuggingRecorder::install()` block (which only proved the snapshot was
    // non-empty). These exercise the same wrappers but with per-failure-class
    // delta assertions, mirroring the MH Step 2 migration in
    // `crates/mh-service/src/observability/metrics.rs`.
    //
    // NOTE: These are wrapper-invocation tests (Cat C name-coverage tier).
    // The PRODUCTION-PATH coverage for these metrics lives in:
    //   - tests/webtransport_accept_loop_integration.rs (accept_loop, jwt, session_join)
    //   - tests/auth_layer_integration.rs (service-token jwt, caller_type_rejected)
    //   - tests/media_coordination_integration.rs (mh_notifications)
    //   - tests/register_meeting_integration.rs (register_meeting + duration)
    //   - tests/gc_integration.rs (gc_heartbeats + latency)
    //   - tests/actor_metrics_integration.rs (mailbox_depth, panics, dropped, gauges)
    //   - tests/redis_metrics_integration.rs (redis_latency, fenced_out)
    // The block here is the in-file mirror that exercises the metrics.rs
    // wrappers themselves end-to-end through MetricAssertion.
    // ========================================================================

    #[test]
    fn metrics_module_emits_join_flow_cluster() {
        let snap = MetricAssertion::snapshot();

        // Histogram first.
        record_session_join("success", None, Duration::from_millis(200));
        record_session_join("failure", Some("jwt_validation"), Duration::from_millis(5));
        snap.histogram("mc_session_join_duration_seconds")
            .with_labels(&[("status", "success")])
            .assert_observation_count_at_least(1);

        record_webtransport_connection("accepted");
        record_webtransport_connection("rejected");
        record_webtransport_connection("error");
        snap.counter("mc_webtransport_connections_total")
            .with_labels(&[("status", "accepted")])
            .assert_delta(1);
        snap.counter("mc_webtransport_connections_total")
            .with_labels(&[("status", "rejected")])
            .assert_delta(1);
        snap.counter("mc_webtransport_connections_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);

        record_jwt_validation("success", "meeting", "none");
        record_jwt_validation("failure", "meeting", "signature_invalid");
        snap.counter("mc_jwt_validations_total")
            .with_labels(&[("token_type", "meeting"), ("result", "success")])
            .assert_delta(1);
        snap.counter("mc_jwt_validations_total")
            .with_labels(&[
                ("token_type", "meeting"),
                ("result", "failure"),
                ("failure_reason", "signature_invalid"),
            ])
            .assert_delta(1);

        snap.counter("mc_session_joins_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mc_session_joins_total")
            .with_labels(&[("status", "failure")])
            .assert_delta(1);
        snap.counter("mc_session_join_failures_total")
            .with_labels(&[("error_type", "jwt_validation")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_actor_system_cluster() {
        let snap = MetricAssertion::snapshot();

        set_meetings_active(7);
        set_connections_active(13);
        set_actor_mailbox_depth("meeting", 42);
        record_actor_panic("controller");
        record_message_dropped("participant");

        snap.gauge("mc_meetings_active").assert_value(7.0);
        snap.gauge("mc_connections_active").assert_value(13.0);
        snap.gauge("mc_actor_mailbox_depth")
            .with_labels(&[("actor_type", "meeting")])
            .assert_value(42.0);
        snap.counter("mc_actor_panics_total")
            .with_labels(&[("actor_type", "controller")])
            .assert_delta(1);
        snap.counter("mc_messages_dropped_total")
            .with_labels(&[("actor_type", "participant")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_gc_heartbeat_cluster() {
        let snap = MetricAssertion::snapshot();

        // Histogram first.
        record_gc_heartbeat_latency("fast", Duration::from_millis(10));
        record_gc_heartbeat_latency("comprehensive", Duration::from_millis(40));
        snap.histogram("mc_gc_heartbeat_latency_seconds")
            .with_labels(&[("type", "fast")])
            .assert_observation_count_at_least(1);

        record_gc_heartbeat("success", "fast");
        record_gc_heartbeat("error", "comprehensive");
        snap.counter("mc_gc_heartbeats_total")
            .with_labels(&[("status", "success"), ("type", "fast")])
            .assert_delta(1);
        snap.counter("mc_gc_heartbeats_total")
            .with_labels(&[("status", "error"), ("type", "comprehensive")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_mh_coordination_cluster() {
        let snap = MetricAssertion::snapshot();

        record_mh_notification("connected");
        record_mh_notification("disconnected");
        record_media_connection_failed(true);
        record_media_connection_failed(false);

        snap.counter("mc_mh_notifications_received_total")
            .with_labels(&[("event", "connected")])
            .assert_delta(1);
        snap.counter("mc_mh_notifications_received_total")
            .with_labels(&[("event", "disconnected")])
            .assert_delta(1);
        snap.counter("mc_media_connection_failures_total")
            .with_labels(&[("all_failed", "true")])
            .assert_delta(1);
        snap.counter("mc_media_connection_failures_total")
            .with_labels(&[("all_failed", "false")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_register_meeting_cluster() {
        let snap = MetricAssertion::snapshot();

        record_register_meeting("success", Duration::from_millis(20));
        record_register_meeting("error", Duration::from_millis(150));

        snap.histogram("mc_register_meeting_duration_seconds")
            .assert_observation_count_at_least(2);
        snap.counter("mc_register_meeting_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mc_register_meeting_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_caller_type_rejected_cluster() {
        let snap = MetricAssertion::snapshot();

        record_caller_type_rejected(
            "MeetingControllerService",
            "global-controller",
            "media-handler",
        );
        record_caller_type_rejected(
            "MediaCoordinationService",
            "media-handler",
            "global-controller",
        );

        snap.counter("mc_caller_type_rejected_total")
            .with_labels(&[
                ("grpc_service", "MeetingControllerService"),
                ("expected_type", "global-controller"),
                ("actual_type", "media-handler"),
            ])
            .assert_delta(1);
        snap.counter("mc_caller_type_rejected_total")
            .with_labels(&[
                ("grpc_service", "MediaCoordinationService"),
                ("expected_type", "media-handler"),
                ("actual_type", "global-controller"),
            ])
            .assert_delta(1);
    }

    #[test]
    fn test_all_adr0023_metrics_have_correct_names() {
        // This test verifies the metric names match ADR-0023 Section 11 requirements.
        // We can't easily verify the exact names without parsing Prometheus output,
        // but we verify the functions exist and can be called without panicking.

        // Required metrics per ADR-0023 Section 11:
        // 1. mc_connections_active (gauge)
        set_connections_active(0);

        // 2. mc_meetings_active (gauge)
        set_meetings_active(0);

        // 3. mc_actor_mailbox_depth (gauge with actor_type label)
        set_actor_mailbox_depth("controller", 0);

        // 4. mc_redis_latency_seconds (histogram with operation label)
        record_redis_latency("test_op", Duration::from_millis(1));

        // 5. mc_fenced_out_total (counter with reason label)
        record_fenced_out("test_reason");

        // If we get here without panicking, all metric functions are callable
    }
}
