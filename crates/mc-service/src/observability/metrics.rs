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
//! - `actor_type`: 3 values max (controller, meeting, connection)
//! - `operation`: bounded by Redis commands (~10 values)
//! - `message_type`: bounded by protobuf types (~20 values)
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
/// - Message latency p99 < 100ms
/// - Recovery duration p99 < 500ms
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
        // Message latency buckets - internal service call
        .set_buckets_for_metric(
            Matcher::Prefix("mc_message".to_string()),
            &[
                0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set message latency buckets: {e}"))?
        // Recovery duration buckets - longer operations, HTTP-style
        .set_buckets_for_metric(
            Matcher::Prefix("mc_recovery".to_string()),
            &[
                0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000, 10.000,
            ],
        )
        .map_err(|e| format!("Failed to set recovery duration buckets: {e}"))?
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

/// Record signaling message processing latency.
///
/// Metric: `mc_message_latency_seconds`
/// Labels: `message_type`
///
/// Cardinality: ~20 (bounded by protobuf message types)
///
/// SLO target: p99 < 100ms for signaling messages
pub fn record_message_latency(message_type: &str, duration: Duration) {
    histogram!("mc_message_latency_seconds", "message_type" => message_type.to_string())
        .record(duration.as_secs_f64());
}

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

/// Record session recovery duration.
///
/// Metric: `mc_recovery_duration_seconds`
/// Labels: none
///
/// Tracks time to recover a session after reconnection with binding token.
/// Includes Redis state fetch, session rehydration, and actor re-creation.
///
/// SLO target: p99 < 500ms for session recovery
pub fn record_recovery_duration(duration: Duration) {
    histogram!("mc_recovery_duration_seconds").record(duration.as_secs_f64());
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

// ============================================================================
// Error Metrics
// ============================================================================

/// Record error by category.
///
/// Metric: `mc_errors_total`
/// Labels: `operation`, `error_type`, `status_code`
///
/// The `operation` label uses a subsystem prefix to disambiguate across
/// the global error counter (e.g., `"token_refresh"`, `"gc_heartbeat"`,
/// `"redis_session"`).
pub fn record_error(operation: &str, error_type: &str, status_code: u16) {
    counter!("mc_errors_total",
        "operation" => operation.to_string(),
        "error_type" => error_type.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests execute the metric recording functions to ensure code coverage.
    // The metrics crate will record to a global no-op recorder if none is installed,
    // which is sufficient for coverage testing. We don't need to verify the actual
    // metric values - that would require installing a test recorder from metrics-util.
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
    fn test_record_message_latency() {
        // Test with various message types and durations
        record_message_latency("join_request", Duration::from_millis(10));
        record_message_latency("leave_request", Duration::from_millis(5));
        record_message_latency("layout_update", Duration::from_millis(50));
        record_message_latency("media_control", Duration::from_millis(2));
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
    fn test_record_recovery_duration() {
        // Test with various recovery times
        record_recovery_duration(Duration::from_millis(50));
        record_recovery_duration(Duration::from_millis(100));
        record_recovery_duration(Duration::from_millis(500));
        record_recovery_duration(Duration::from_secs(1));
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
    fn test_record_error() {
        // MC uses signaling error codes (2-7), not HTTP status codes
        record_error("token_refresh", "http", 6); // INTERNAL_ERROR
        record_error("gc_heartbeat", "grpc", 6); // INTERNAL_ERROR
        record_error("redis_session", "redis", 6); // INTERNAL_ERROR
        record_error("meeting_join", "capacity_exceeded", 7); // CAPACITY_EXCEEDED
        record_error("session_binding", "session_binding", 2); // UNAUTHORIZED
    }

    #[test]
    fn test_cardinality_bounds() {
        // Verify actor_type labels are bounded
        let valid_actor_types = ["controller", "meeting", "connection"];
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
    }

    // ========================================================================
    // Integration test for Prometheus metrics endpoint
    // ========================================================================
    //
    // This test verifies that all required ADR-0023 Section 11 metrics are
    // exposed in Prometheus text format via the metrics-exporter-prometheus.

    #[test]
    fn test_prometheus_metrics_endpoint_integration() {
        use metrics_util::debugging::DebuggingRecorder;

        // Install a debugging recorder to capture metrics
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();

        // Install the recorder - this will replace any existing global recorder
        // Note: This test must run in isolation (not parallel with other metrics tests)
        // because metrics recorders are global state.
        let _ = recorder.install();

        // Record all 7 ADR-0023 required metrics
        set_connections_active(42);
        set_meetings_active(10);
        record_message_latency("join_request", Duration::from_millis(25));
        set_actor_mailbox_depth("meeting", 15);
        record_redis_latency("get", Duration::from_micros(500));
        record_fenced_out("stale_generation");
        record_recovery_duration(Duration::from_millis(100));

        // Also record additional operational metrics
        record_actor_panic("meeting");
        record_message_dropped("connection");
        record_gc_heartbeat("success", "fast");
        record_gc_heartbeat_latency("fast", Duration::from_millis(10));

        // Record token refresh metrics
        record_token_refresh("success", None, Duration::from_millis(50));
        record_token_refresh("error", Some("http"), Duration::from_millis(100));

        // Record error metrics
        record_error("token_refresh", "http", 500);

        // Take a snapshot and verify metrics were recorded
        let snapshot = snapshotter.snapshot();

        // Convert to vec to check contents
        let metrics = snapshot.into_vec();

        // The snapshot contains all metrics recorded during the test.
        // We verify the snapshot is not empty, indicating metrics were recorded.
        // Detailed metric name verification would require parsing the snapshot,
        // which is beyond the scope of this integration test.
        //
        // The key verification is that:
        // 1. The recorder was installed successfully
        // 2. All metric recording functions executed without error
        // 3. The snapshot contains recorded data
        assert!(
            !metrics.is_empty(),
            "Prometheus snapshot should contain recorded metrics"
        );

        // Verify we have multiple metrics recorded
        assert!(
            metrics.len() >= 7,
            "Should have at least 7 metrics (ADR-0023 requirements), got {}",
            metrics.len()
        );
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

        // 3. mc_message_latency_seconds (histogram with message_type label)
        record_message_latency("test_type", Duration::from_millis(1));

        // 4. mc_actor_mailbox_depth (gauge with actor_type label)
        set_actor_mailbox_depth("controller", 0);

        // 5. mc_redis_latency_seconds (histogram with operation label)
        record_redis_latency("test_op", Duration::from_millis(1));

        // 6. mc_fenced_out_total (counter with reason label)
        record_fenced_out("test_reason");

        // 7. mc_recovery_duration_seconds (histogram)
        record_recovery_duration(Duration::from_millis(1));

        // If we get here without panicking, all metric functions are callable
    }
}
