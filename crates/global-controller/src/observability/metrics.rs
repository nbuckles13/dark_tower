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

use metrics::{counter, histogram};
use std::time::Duration;

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
///
/// NOTE: Defined per ADR-0011 for MC assignment metrics. Will be wired in as
/// instrumentation is expanded across services.
#[allow(dead_code)]
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
///
/// NOTE: Defined per ADR-0011 for database metrics. Will be wired in as
/// instrumentation is expanded across repositories.
#[allow(dead_code)]
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

/// Record token refresh attempt
///
/// Metric: `gc_token_refresh_total`, `gc_token_refresh_duration_seconds`
/// Labels: `status`
///
/// NOTE: Defined per ADR-0010 Section 4a for TokenManager metrics.
/// Will be wired into common/token_manager.rs.
#[allow(dead_code)]
pub fn record_token_refresh(status: &str, duration: Duration) {
    histogram!("gc_token_refresh_duration_seconds").record(duration.as_secs_f64());

    counter!("gc_token_refresh_total",
        "status" => status.to_string()
    )
    .increment(1);
}

/// Record token refresh failure by error type
///
/// Metric: `gc_token_refresh_failures_total`
/// Labels: `error_type`
///
/// NOTE: Defined per ADR-0010 Section 4a for TokenManager metrics.
/// Will be wired into common/token_manager.rs.
#[allow(dead_code)]
pub fn record_token_refresh_failure(error_type: &str) {
    counter!("gc_token_refresh_failures_total",
        "error_type" => error_type.to_string()
    )
    .increment(1);
}

// ============================================================================
// Error Metrics
// ============================================================================

/// Record error by category
///
/// Metric: `gc_errors_total`
/// Labels: `operation`, `error_type`, `status_code`
///
/// NOTE: Defined per ADR-0011 for error tracking. Will be wired into
/// error handlers as instrumentation is expanded.
#[allow(dead_code)]
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
/// NOTE: Defined per ADR-0011 for gRPC metrics. Will be wired into
/// services/mc_client.rs as instrumentation is expanded.
#[allow(dead_code)]
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
///
/// NOTE: Defined per ADR-0011 for MH selection metrics. Will be wired into
/// services/mh_selection.rs as instrumentation is expanded.
#[allow(dead_code)]
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
        record_token_refresh("success", Duration::from_millis(100));
        record_token_refresh("error", Duration::from_millis(500));
    }

    #[test]
    fn test_record_token_refresh_failure() {
        record_token_refresh_failure("http_error");
        record_token_refresh_failure("auth_rejected");
        record_token_refresh_failure("invalid_response");
        record_token_refresh_failure("timeout");
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
}
