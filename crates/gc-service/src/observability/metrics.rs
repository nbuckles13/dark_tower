//! Metrics definitions for Global Controller per ADR-0011.
//!
//! All metrics follow Prometheus naming conventions:
//! - `gc_` prefix for Global Controller
//! - `_total` suffix for counters
//! - `_seconds` suffix for duration histograms
//!
//! # Cardinality
//!
//! Labels are bounded to prevent cardinality explosion:
//! - `method`: 7 values max (GET, POST, PATCH, DELETE, PUT, HEAD, OPTIONS)
//! - `endpoint`: ~10 values (parameterized paths)
//! - `status`: 3 values (success, error, timeout)
//! - `operation`: bounded by code (select_mc, insert_assignment, etc.)
//! - `error_type`: bounded by error variants
//!
//! # SLO Alignment
//!
//! Histogram buckets are aligned with ADR-0010/ADR-0011 SLO targets:
//! - HTTP request p95 < 200ms
//! - MC assignment p95 < 20ms
//! - DB query p99 < 50ms

use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Initialize Prometheus metrics recorder and return the handle
/// for serving metrics via HTTP.
///
/// ADR-0011: Must be called before any metrics are recorded.
/// Configures histogram buckets aligned with SLO targets:
/// - HTTP request p95 < 200ms
/// - MC assignment p95 < 20ms
/// - DB queries p99 < 50ms
///
/// # Errors
///
/// Returns error if Prometheus recorder fails to install (e.g., already installed).
pub fn init_metrics_recorder() -> Result<PrometheusHandle, String> {
    PrometheusBuilder::new()
        // HTTP request buckets aligned with 200ms p95 SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("gc_http_request".to_string()),
            &[
                0.005, 0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.300, 0.500, 1.000, 2.000,
            ],
        )
        .map_err(|e| format!("Failed to set HTTP request buckets: {e}"))?
        // MC assignment buckets aligned with 20ms p95 SLO target (ADR-0010)
        .set_buckets_for_metric(
            Matcher::Prefix("gc_mc_assignment".to_string()),
            &[
                0.005, 0.010, 0.015, 0.020, 0.030, 0.050, 0.100, 0.250, 0.500,
            ],
        )
        .map_err(|e| format!("Failed to set MC assignment buckets: {e}"))?
        // DB query buckets aligned with 50ms p99 SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("gc_db_query".to_string()),
            &[
                0.001, 0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set DB query buckets: {e}"))?
        // gRPC MC call buckets
        .set_buckets_for_metric(
            Matcher::Prefix("gc_grpc_mc".to_string()),
            &[
                0.005, 0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.500,
            ],
        )
        .map_err(|e| format!("Failed to set gRPC MC buckets: {e}"))?
        // MH selection buckets
        .set_buckets_for_metric(
            Matcher::Prefix("gc_mh_selection".to_string()),
            &[0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250],
        )
        .map_err(|e| format!("Failed to set MH selection buckets: {e}"))?
        // AC request buckets - sub-second granularity for p95 latency detection
        .set_buckets_for_metric(
            Matcher::Prefix("gc_ac_request".to_string()),
            &[
                0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000,
            ],
        )
        .map_err(|e| format!("Failed to set AC request buckets: {e}"))?
        // Token refresh buckets
        .set_buckets_for_metric(
            Matcher::Prefix("gc_token_refresh".to_string()),
            &[0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000],
        )
        .map_err(|e| format!("Failed to set token refresh buckets: {e}"))?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus recorder: {e}"))
}

// ============================================================================
// HTTP Request Metrics
// ============================================================================

/// Record HTTP request completion
///
/// Metric: `gc_http_requests_total`, `gc_http_request_duration_seconds`
/// Labels: `method`, `endpoint`, `status`
///
/// This captures ALL HTTP responses including framework-level errors like:
/// - 415 Unsupported Media Type (wrong Content-Type)
/// - 400 Bad Request (JSON parse errors)
/// - 404 Not Found
/// - 405 Method Not Allowed
///
/// SLO target: p95 < 200ms
pub fn record_http_request(method: &str, endpoint: &str, status_code: u16, duration: Duration) {
    // Normalize endpoint to prevent cardinality explosion
    let normalized_endpoint = normalize_endpoint(endpoint);

    // Determine status category for simplified querying
    let status = categorize_status_code(status_code);

    histogram!("gc_http_request_duration_seconds",
        "method" => method.to_string(),
        "endpoint" => normalized_endpoint.clone(),
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_http_requests_total",
        "method" => method.to_string(),
        "endpoint" => normalized_endpoint,
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

/// Categorize HTTP status code into success/error/timeout
fn categorize_status_code(status_code: u16) -> &'static str {
    match status_code {
        200..=299 => "success",
        408 | 504 => "timeout",
        _ => "error",
    }
}

/// Normalize endpoint path to prevent label cardinality explosion
///
/// Replaces dynamic segments (UUIDs, meeting codes) with placeholders.
fn normalize_endpoint(path: &str) -> String {
    // Known static paths
    match path {
        "/" => "/".to_string(),
        "/health" => "/health".to_string(),
        "/metrics" => "/metrics".to_string(),
        "/api/v1/me" => "/api/v1/me".to_string(),
        _ => normalize_dynamic_endpoint(path),
    }
}

/// Normalize paths with dynamic segments
///
/// Replaces meeting codes and UUIDs with placeholders.
fn normalize_dynamic_endpoint(path: &str) -> String {
    // Meeting endpoints: /api/v1/meetings/{code}
    if path.starts_with("/api/v1/meetings/") {
        let parts: Vec<&str> = path.split('/').collect();

        // /api/v1/meetings/{code} → parts.len() == 5
        if parts.len() == 5 {
            return "/api/v1/meetings/{code}".to_string();
        }

        // /api/v1/meetings/{code}/guest-token → parts.len() == 6
        if parts.len() == 6 {
            if let Some(action) = parts.get(5) {
                if *action == "guest-token" {
                    return "/api/v1/meetings/{code}/guest-token".to_string();
                }
            }
        }

        // /api/v1/meetings/{id}/settings → parts.len() == 6
        if parts.len() == 6 {
            if let Some(action) = parts.get(5) {
                if *action == "settings" {
                    return "/api/v1/meetings/{id}/settings".to_string();
                }
            }
        }
    }

    // Unknown paths normalized to "/other" to bound cardinality
    "/other".to_string()
}

// ============================================================================
// MC Assignment Metrics
// ============================================================================

/// Record MC assignment duration and outcome
///
/// Metric: `gc_mc_assignment_duration_seconds`, `gc_mc_assignments_total`
/// Labels: `status`, `rejection_reason`
///
/// SLO target: p95 < 20ms (ADR-0010)
pub fn record_mc_assignment(status: &str, rejection_reason: Option<&str>, duration: Duration) {
    let reason = rejection_reason.unwrap_or("none");

    histogram!("gc_mc_assignment_duration_seconds",
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_mc_assignments_total",
        "status" => status.to_string(),
        "rejection_reason" => reason.to_string()
    )
    .increment(1);
}

// ============================================================================
// Database Metrics
// ============================================================================

/// Record database query execution
///
/// Metric: `gc_db_query_duration_seconds`, `gc_db_queries_total`
/// Labels: `operation`, `status`
///
/// Operations: select_mc, insert_assignment, update_heartbeat, get_healthy_assignment,
///             get_candidate_mcs, atomic_assign, end_assignment, etc.
pub fn record_db_query(operation: &str, status: &str, duration: Duration) {
    histogram!("gc_db_query_duration_seconds",
        "operation" => operation.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_db_queries_total",
        "operation" => operation.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
}

// ============================================================================
// Token Manager Metrics (ADR-0010 Section 4a)
// ============================================================================

/// Record token refresh attempt.
///
/// Emits three metrics per the metrics catalog (`docs/observability/metrics/gc.md`):
/// - `gc_token_refresh_total` counter (labels: `status`)
/// - `gc_token_refresh_duration_seconds` histogram (no labels)
/// - `gc_token_refresh_failures_total` counter (labels: `error_type`, on failure only)
///
/// Called from the `TokenRefreshCallback` wired in `main.rs`.
///
/// # Arguments
///
/// * `status` - "success" or "error"
/// * `error_type` - Error category for failures (e.g., "http", "auth_rejected")
/// * `duration` - Duration of the acquire_token call (excludes backoff)
pub fn record_token_refresh(status: &str, error_type: Option<&str>, duration: Duration) {
    histogram!("gc_token_refresh_duration_seconds").record(duration.as_secs_f64());

    counter!("gc_token_refresh_total",
        "status" => status.to_string()
    )
    .increment(1);

    if let Some(err_type) = error_type {
        counter!("gc_token_refresh_failures_total",
            "error_type" => err_type.to_string()
        )
        .increment(1);
    }
}

// ============================================================================
// AC Client Metrics
// ============================================================================

/// Record AC client request duration and outcome.
///
/// Metric: `gc_ac_request_duration_seconds`, `gc_ac_requests_total`
/// Labels: `operation`, `status`
///
/// Operations: "meeting_token", "guest_token"
/// Status: "success", "error"
pub fn record_ac_request(operation: &str, status: &str, duration: Duration) {
    histogram!("gc_ac_request_duration_seconds",
        "operation" => operation.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_ac_requests_total",
        "operation" => operation.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
}

// ============================================================================
// Error Metrics
// ============================================================================

/// Record error by category.
///
/// Metric: `gc_errors_total`
/// Labels: `operation`, `error_type`, `status_code`
///
/// The `operation` label uses a subsystem prefix to disambiguate across
/// the global error counter (e.g., `"ac_meeting_token"`, `"ac_guest_token"`,
/// `"mc_grpc"`). This differs from per-subsystem metrics like
/// `gc_ac_requests_total` where the operation is already scoped.
pub fn record_error(operation: &str, error_type: &str, status_code: u16) {
    counter!("gc_errors_total",
        "operation" => operation.to_string(),
        "error_type" => error_type.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

// ============================================================================
// gRPC Metrics
// ============================================================================

/// Record gRPC call to MC
///
/// Metric: `gc_grpc_mc_calls_total`, `gc_grpc_mc_call_duration_seconds`
/// Labels: `method`, `status`
///
/// Status values: "success", "rejected", "error"
pub fn record_grpc_mc_call(method: &str, status: &str, duration: Duration) {
    histogram!("gc_grpc_mc_call_duration_seconds",
        "method" => method.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_grpc_mc_calls_total",
        "method" => method.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
}

// ============================================================================
// MH Selection Metrics
// ============================================================================

/// Record MH selection duration and outcome
///
/// Metric: `gc_mh_selection_duration_seconds`, `gc_mh_selections_total`
/// Labels: `status`, `has_backup`
pub fn record_mh_selection(status: &str, has_backup: bool, duration: Duration) {
    histogram!("gc_mh_selection_duration_seconds",
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_mh_selections_total",
        "status" => status.to_string(),
        "has_backup" => has_backup.to_string()
    )
    .increment(1);
}

// ============================================================================
// Registered Controllers Gauge (Fleet Monitoring)
// ============================================================================

/// Set the count of registered controllers by type and status.
///
/// Metric: `gc_registered_controllers`
/// Type: Gauge
/// Labels: `controller_type`, `status`
///
/// Cardinality: controller_type (2 values: "meeting", "media") × status (5 values:
/// "pending", "healthy", "degraded", "unhealthy", "draining") = 10 combinations.
///
/// This gauge is set:
/// - On GC startup (query DB for current counts)
/// - After MC/MH registration (re-query current counts)
/// - After heartbeat updates that change status (re-query affected status counts)
/// - After health checker marks stale controllers unhealthy
///
/// # Arguments
///
/// * `controller_type` - Type of controller: "meeting" or "media"
/// * `status` - Health status: "pending", "healthy", "degraded", "unhealthy", "draining"
/// * `count` - Current count of controllers with this type and status
pub fn set_registered_controllers(controller_type: &str, status: &str, count: u64) {
    gauge!("gc_registered_controllers",
        "controller_type" => controller_type.to_string(),
        "status" => status.to_string()
    )
    .set(count as f64);
}

/// All valid health status values for controller metrics.
///
/// Used to ensure all status gauges are set (including zeros) when updating
/// the registered controllers metric.
pub const CONTROLLER_STATUSES: [&str; 5] =
    ["pending", "healthy", "degraded", "unhealthy", "draining"];

/// Update all registered controller gauges for a given controller type.
///
/// Takes a list of (status, count) pairs and sets the corresponding gauges.
/// For any status not in the list, sets the gauge to 0.
///
/// # Arguments
///
/// * `controller_type` - Type of controller: "meeting" or "media"
/// * `counts` - List of (status, count) pairs from database query
pub fn update_registered_controller_gauges(controller_type: &str, counts: &[(String, u64)]) {
    // Create a map from status to count
    let count_map: std::collections::HashMap<&str, u64> = counts
        .iter()
        .map(|(status, count)| (status.as_str(), *count))
        .collect();

    // Set gauge for each status, defaulting to 0 if not in map
    for status in CONTROLLER_STATUSES {
        let count = count_map.get(status).copied().unwrap_or(0);
        set_registered_controllers(controller_type, status, count);
    }
}

// ============================================================================
// Tests
// ============================================================================

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
    fn test_record_http_request() {
        // Test with various methods and statuses
        record_http_request("GET", "/health", 200, Duration::from_millis(5));
        record_http_request("GET", "/api/v1/me", 200, Duration::from_millis(50));
        record_http_request(
            "GET",
            "/api/v1/meetings/abc123",
            200,
            Duration::from_millis(150),
        );
        record_http_request(
            "POST",
            "/api/v1/meetings/abc123/guest-token",
            200,
            Duration::from_millis(200),
        );
        record_http_request(
            "PATCH",
            "/api/v1/meetings/uuid-here/settings",
            200,
            Duration::from_millis(100),
        );

        // Test error cases
        record_http_request("GET", "/api/v1/me", 401, Duration::from_millis(10));
        record_http_request(
            "GET",
            "/api/v1/meetings/notfound",
            404,
            Duration::from_millis(5),
        );
        record_http_request(
            "POST",
            "/api/v1/meetings/abc123/guest-token",
            429,
            Duration::from_millis(2),
        );

        // Test timeout
        record_http_request("GET", "/api/v1/me", 504, Duration::from_secs(30));
        record_http_request(
            "GET",
            "/api/v1/meetings/abc123",
            408,
            Duration::from_secs(30),
        );
    }

    #[test]
    fn test_categorize_status_code() {
        // Success codes
        assert_eq!(categorize_status_code(200), "success");
        assert_eq!(categorize_status_code(201), "success");
        assert_eq!(categorize_status_code(204), "success");
        assert_eq!(categorize_status_code(299), "success");

        // Timeout codes
        assert_eq!(categorize_status_code(408), "timeout");
        assert_eq!(categorize_status_code(504), "timeout");

        // Error codes
        assert_eq!(categorize_status_code(400), "error");
        assert_eq!(categorize_status_code(401), "error");
        assert_eq!(categorize_status_code(403), "error");
        assert_eq!(categorize_status_code(404), "error");
        assert_eq!(categorize_status_code(429), "error");
        assert_eq!(categorize_status_code(500), "error");
        assert_eq!(categorize_status_code(503), "error");
    }

    #[test]
    fn test_normalize_endpoint_known_paths() {
        assert_eq!(normalize_endpoint("/"), "/");
        assert_eq!(normalize_endpoint("/health"), "/health");
        assert_eq!(normalize_endpoint("/metrics"), "/metrics");
        assert_eq!(normalize_endpoint("/api/v1/me"), "/api/v1/me");
    }

    #[test]
    fn test_normalize_endpoint_meeting_paths() {
        // Meeting join
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/abc123"),
            "/api/v1/meetings/{code}"
        );
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/some-meeting-code"),
            "/api/v1/meetings/{code}"
        );

        // Guest token
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/abc123/guest-token"),
            "/api/v1/meetings/{code}/guest-token"
        );

        // Settings
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/550e8400-e29b-41d4-a716-446655440000/settings"),
            "/api/v1/meetings/{id}/settings"
        );
    }

    #[test]
    fn test_normalize_endpoint_unknown_paths() {
        assert_eq!(normalize_endpoint("/unknown"), "/other");
        assert_eq!(normalize_endpoint("/api/v2/something"), "/other");
        assert_eq!(normalize_endpoint("/api/v1/meetings"), "/other");
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/code/unknown-action"),
            "/other"
        );
    }

    #[test]
    fn test_record_mc_assignment() {
        record_mc_assignment("success", None, Duration::from_millis(15));
        record_mc_assignment("rejected", Some("at_capacity"), Duration::from_millis(10));
        record_mc_assignment("rejected", Some("draining"), Duration::from_millis(8));
        record_mc_assignment("rejected", Some("unhealthy"), Duration::from_millis(5));
        record_mc_assignment("error", Some("rpc_failed"), Duration::from_millis(100));
    }

    #[test]
    fn test_record_db_query() {
        record_db_query("select_mc", "success", Duration::from_millis(5));
        record_db_query(
            "get_healthy_assignment",
            "success",
            Duration::from_millis(3),
        );
        record_db_query("get_candidate_mcs", "success", Duration::from_millis(7));
        record_db_query("atomic_assign", "success", Duration::from_millis(10));
        record_db_query("end_assignment", "success", Duration::from_millis(5));
        record_db_query("update_heartbeat", "error", Duration::from_millis(50));
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
    fn test_record_ac_request() {
        // Success paths for both operations
        record_ac_request("meeting_token", "success", Duration::from_millis(100));
        record_ac_request("guest_token", "success", Duration::from_millis(80));

        // Error paths
        record_ac_request("meeting_token", "error", Duration::from_millis(200));
        record_ac_request("guest_token", "error", Duration::from_millis(150));
    }

    #[test]
    fn test_record_error() {
        record_error("join_meeting", "not_found", 404);
        record_error("join_meeting", "forbidden", 403);
        record_error("guest_token", "rate_limit", 429);
        record_error("update_settings", "unauthorized", 401);
        record_error("mc_assignment", "service_unavailable", 503);
    }

    #[test]
    fn test_record_grpc_mc_call() {
        record_grpc_mc_call("assign_meeting", "success", Duration::from_millis(25));
        record_grpc_mc_call("assign_meeting", "rejected", Duration::from_millis(10));
        record_grpc_mc_call("assign_meeting", "error", Duration::from_millis(100));
    }

    #[test]
    fn test_record_mh_selection() {
        record_mh_selection("success", true, Duration::from_millis(8));
        record_mh_selection("success", false, Duration::from_millis(5));
        record_mh_selection("error", false, Duration::from_millis(3));
    }

    #[test]
    fn test_set_registered_controllers() {
        // Test all valid controller types and statuses (cardinality: 2 * 5 = 10)
        // Meeting controllers
        set_registered_controllers("meeting", "pending", 0);
        set_registered_controllers("meeting", "healthy", 5);
        set_registered_controllers("meeting", "degraded", 1);
        set_registered_controllers("meeting", "unhealthy", 2);
        set_registered_controllers("meeting", "draining", 1);

        // Media handlers (future)
        set_registered_controllers("media", "pending", 0);
        set_registered_controllers("media", "healthy", 10);
        set_registered_controllers("media", "degraded", 0);
        set_registered_controllers("media", "unhealthy", 1);
        set_registered_controllers("media", "draining", 0);
    }

    #[test]
    fn test_update_registered_controller_gauges() {
        // Test with partial counts (some statuses missing)
        let counts = vec![("healthy".to_string(), 5), ("degraded".to_string(), 2)];

        // Should set all 5 statuses, with missing ones set to 0
        update_registered_controller_gauges("meeting", &counts);

        // Test with all statuses
        let full_counts = vec![
            ("pending".to_string(), 1),
            ("healthy".to_string(), 10),
            ("degraded".to_string(), 3),
            ("unhealthy".to_string(), 2),
            ("draining".to_string(), 1),
        ];
        update_registered_controller_gauges("meeting", &full_counts);

        // Test with empty counts
        update_registered_controller_gauges("media", &[]);
    }

    #[test]
    fn test_controller_statuses_constant() {
        // Verify we have all expected statuses
        assert_eq!(CONTROLLER_STATUSES.len(), 5);
        assert!(CONTROLLER_STATUSES.contains(&"pending"));
        assert!(CONTROLLER_STATUSES.contains(&"healthy"));
        assert!(CONTROLLER_STATUSES.contains(&"degraded"));
        assert!(CONTROLLER_STATUSES.contains(&"unhealthy"));
        assert!(CONTROLLER_STATUSES.contains(&"draining"));
    }
}
