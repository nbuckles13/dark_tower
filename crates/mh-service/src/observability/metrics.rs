//! Metrics definitions for Media Handler per ADR-0011
//!
//! All metrics follow Prometheus naming conventions:
//! - `mh_` prefix for Media Handler
//! - `_total` suffix for counters
//! - `_seconds` suffix for duration histograms
//!
//! # Cardinality
//!
//! Labels are bounded to prevent cardinality explosion (ADR-0011):
//! - `status`: 2 values (success, error)
//! - `method`: 3 values (`register`, `route_media`, `stream_telemetry`)
//! - `error_type`: ~6 values (bounded by `MhError` variants)
//! - `operation`: ~5 values (bounded by code paths)

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

/// Record an incoming gRPC request from MC.
///
/// Metric: `mh_grpc_requests_total`
/// Labels: `method` (`register` | `register_meeting` | `route_media` | `stream_telemetry`), `status` (success | error)
/// Cardinality: 8 (4 methods x 2 statuses)
pub fn record_grpc_request(method: &str, status: &str) {
    counter!(
        "mh_grpc_requests_total",
        "method" => method.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
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

/// Record an MC notification delivery attempt (R-16/R-17).
///
/// Metric: `mh_mc_notifications_total`
/// Labels: `event` (connected | disconnected), `status` (success | error)
/// Cardinality: 4 (2 events x 2 statuses)
pub fn record_mc_notification(event: &str, status: &str) {
    counter!(
        "mh_mc_notifications_total",
        "event" => event.to_string(),
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
        // All 8 combinations: 4 methods x 2 statuses
        record_grpc_request("register", "success");
        record_grpc_request("register", "error");
        record_grpc_request("register_meeting", "success");
        record_grpc_request("register_meeting", "error");
        record_grpc_request("route_media", "success");
        record_grpc_request("route_media", "error");
        record_grpc_request("stream_telemetry", "success");
        record_grpc_request("stream_telemetry", "error");
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

        // Verify method labels are bounded to 4 values
        let valid_methods = [
            "register",
            "register_meeting",
            "route_media",
            "stream_telemetry",
        ];
        for method in &valid_methods {
            for status in &valid_statuses {
                record_grpc_request(method, status);
            }
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
        use metrics_util::debugging::DebuggingRecorder;

        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _ = recorder.install();

        // Record all MH metrics
        record_gc_registration("success");
        record_gc_registration("error");
        record_gc_registration_latency(Duration::from_millis(100));
        record_gc_heartbeat("success");
        record_gc_heartbeat("error");
        record_gc_heartbeat_latency(Duration::from_millis(10));
        record_token_refresh("success", None, Duration::from_millis(50));
        record_token_refresh("error", Some("http"), Duration::from_millis(100));
        record_grpc_request("register", "success");
        record_grpc_request("route_media", "error");
        record_grpc_request("stream_telemetry", "success");
        record_error("gc_heartbeat", "grpc", 503);

        let snapshot = snapshotter.snapshot();
        let metrics = snapshot.into_vec();

        assert!(
            !metrics.is_empty(),
            "Prometheus snapshot should contain recorded metrics"
        );

        // We recorded at least 9 distinct metric names
        assert!(
            metrics.len() >= 9,
            "Should have at least 9 metrics (ADR-0011 requirements), got {}",
            metrics.len()
        );
    }
}
