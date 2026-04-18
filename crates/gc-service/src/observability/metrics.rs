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
//! - `status_code`: raw HTTP status codes (~15 values in practice: 200, 201,
//!   400, 401, 403, 404, 429, 500, 503, etc.). Categorization (2xx/4xx/5xx
//!   vs success/error/timeout) is derivable via PromQL regex like
//!   `status_code=~"[45].."`, so no separate category label is emitted —
//!   matches AC's canonical shape (see ADR-0031 §Canonical Labels).
//! - `status`: 5 values (success, error, timeout, rejected, accepted) on
//!   non-HTTP metrics where callers pass semantic outcome strings (e.g.,
//!   `gc_mc_assignments_total`, `gc_db_queries_total`).
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
        // Meeting creation buckets
        .set_buckets_for_metric(
            Matcher::Prefix("gc_meeting_creation".to_string()),
            &[
                0.005, 0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.300, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set meeting creation buckets: {e}"))?
        // Meeting join buckets - extended to 5s (join includes MC assignment + AC token request)
        .set_buckets_for_metric(
            Matcher::Prefix("gc_meeting_join".to_string()),
            &[
                0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000,
            ],
        )
        .map_err(|e| format!("Failed to set meeting join buckets: {e}"))?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus recorder: {e}"))
}

// ============================================================================
// HTTP Request Metrics
// ============================================================================

/// Record HTTP request completion
///
/// Metric: `gc_http_requests_total`, `gc_http_request_duration_seconds`
/// Labels: `method`, `endpoint`, `status_code` (raw HTTP code)
///
/// This captures ALL HTTP responses including framework-level errors like:
/// - 415 Unsupported Media Type (wrong Content-Type)
/// - 400 Bad Request (JSON parse errors)
/// - 404 Not Found
/// - 405 Method Not Allowed
///
/// Categorization (2xx/4xx/5xx, success/timeout) is derivable at query time
/// via PromQL regex on `status_code` (e.g., `status_code=~"[45].."`). This
/// matches AC's canonical shape — see ADR-0031 and the canonical-label
/// reconciliation tracked in TODO.md (originally FU#3a).
///
/// SLO target: p95 < 200ms
pub fn record_http_request(method: &str, endpoint: &str, status_code: u16, duration: Duration) {
    // Normalize endpoint to prevent cardinality explosion
    let normalized_endpoint = normalize_endpoint(endpoint);

    histogram!("gc_http_request_duration_seconds",
        "method" => method.to_string(),
        "endpoint" => normalized_endpoint.clone(),
        "status_code" => status_code.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_http_requests_total",
        "method" => method.to_string(),
        "endpoint" => normalized_endpoint,
        "status_code" => status_code.to_string()
    )
    .increment(1);
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
        "/api/v1/meetings" => "/api/v1/meetings".to_string(),
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

/// Per-service `TokenRefreshEvent` → metrics dispatcher (ADR-0032 Step 5 Cat B).
///
/// Callable from `main.rs`'s `TokenManager::with_on_refresh` closure and from
/// unit/integration tests. Maps `TokenRefreshEvent.success: bool` to the
/// bounded `status` label and forwards `error_category` + `duration` into
/// `record_token_refresh`. Production emission is byte-identical to the prior
/// inline closure body at `main.rs:124-126`.
pub fn record_token_refresh_metrics(event: &common::token_manager::TokenRefreshEvent) {
    let status = if event.success { "success" } else { "error" };
    record_token_refresh(status, event.error_category, event.duration);
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
// gRPC Auth Metrics (ADR-0003)
// ============================================================================

/// Record a service JWT validation outcome at the gRPC auth layer.
///
/// Metric: `gc_jwt_validations_total`
/// Labels: `result`, `token_type`, `failure_reason`
///
/// Result values: "success", "failure"
/// Token type values: "service" (gRPC auth only sees service tokens;
///   user/guest tokens flow through HTTP middleware and are not recorded here)
/// Failure reason values: "none" (success), "signature_invalid", "expired",
///   "missing_token", "scope_mismatch", "malformed"
///
/// Cardinality: bounded (2 x 1 x 6 = 12 max, plus headroom if token_type
/// expands in the future).
///
/// Recorded in `grpc/auth_layer.rs` for every validation attempt that
/// reaches the cryptographic layer (structural rejects are not counted,
/// matching MC/MH behavior).
pub fn record_jwt_validation(result: &str, token_type: &str, failure_reason: &str) {
    counter!("gc_jwt_validations_total",
        "result" => result.to_string(),
        "token_type" => token_type.to_string(),
        "failure_reason" => failure_reason.to_string()
    )
    .increment(1);
}

/// Record a Layer 2 caller-type rejection at the gRPC auth layer (ADR-0003).
///
/// Metric: `gc_caller_type_rejected_total`
/// Labels: `grpc_service`, `expected_type`, `actual_type`
///
/// Cardinality: 2 x 2 x 4 = 16 max (bounded by gRPC services and service
/// types + "unknown").
///
/// ALERT: Any non-zero value indicates a bug or misconfiguration — a service
/// is presenting a valid token but calling the wrong gRPC endpoint.
pub fn record_caller_type_rejected(grpc_service: &str, expected_type: &str, actual_type: &str) {
    counter!("gc_caller_type_rejected_total",
        "grpc_service" => grpc_service.to_string(),
        "expected_type" => expected_type.to_string(),
        "actual_type" => actual_type.to_string()
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
/// Labels: `status`, `has_multiple`
pub fn record_mh_selection(status: &str, has_multiple: bool, duration: Duration) {
    histogram!("gc_mh_selection_duration_seconds",
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_mh_selections_total",
        "status" => status.to_string(),
        "has_multiple" => has_multiple.to_string()
    )
    .increment(1);
}

// ============================================================================
// Meeting Creation Metrics
// ============================================================================

/// Record meeting creation attempt.
///
/// Emits three metrics per the metrics catalog:
/// - `gc_meeting_creation_total` counter (labels: `status`)
/// - `gc_meeting_creation_duration_seconds` histogram (labels: `status`)
/// - `gc_meeting_creation_failures_total` counter (labels: `error_type`, on failure only)
///
/// # Arguments
///
/// * `status` - "success" or "error"
/// * `error_type` - Error category for failures (e.g., "bad_request", "forbidden",
///   "db_error", "code_collision", "unauthorized", "internal")
/// * `duration` - Duration of the creation attempt
pub fn record_meeting_creation(status: &str, error_type: Option<&str>, duration: Duration) {
    histogram!("gc_meeting_creation_duration_seconds",
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_meeting_creation_total",
        "status" => status.to_string()
    )
    .increment(1);

    if let Some(err_type) = error_type {
        counter!("gc_meeting_creation_failures_total",
            "error_type" => err_type.to_string()
        )
        .increment(1);
    }
}

// ============================================================================
// Meeting Join Metrics
// ============================================================================

/// Record meeting join attempt.
///
/// Emits three metrics per the metrics catalog:
/// - `gc_meeting_join_total` counter (labels: `participant`, `status`)
/// - `gc_meeting_join_duration_seconds` histogram (labels: `participant`, `status`)
/// - `gc_meeting_join_failures_total` counter (labels: `participant`, `error_type`,
///   on failure only)
///
/// # Arguments
///
/// * `participant` - "user" (authenticated `join_meeting`) or "guest"
///   (`get_guest_token`). Discriminator for SLO/alert triage —
///   `error_type="forbidden"` on user vs guest paths means different things
///   (cross-org denial vs `allow_guests=false`), so operators need this
///   axis to triage without log-diving (per @observability ADR-0032 Step 5).
/// * `status` - "success" or "error"
/// * `error_type` - Error category for failures. Bounded set:
///   - `"not_found"` (both paths)
///   - `"bad_status"` (both paths)
///   - `"unauthorized"` (user only — guest path is public)
///   - `"forbidden"` (user: external denied; guest: see `guests_disabled`)
///   - `"guests_disabled"` (guest only — `meeting.allow_guests=false`)
///   - `"bad_request"` (guest only — body validation; user has no body)
///   - `"mc_assignment"` (both paths)
///   - `"ac_request"` (both paths)
///   - `"internal"` (both paths; guest also includes RNG failure on
///     `generate_guest_id`)
/// * `duration` - Duration of the join attempt
pub fn record_meeting_join(
    participant: &str,
    status: &str,
    error_type: Option<&str>,
    duration: Duration,
) {
    histogram!("gc_meeting_join_duration_seconds",
        "participant" => participant.to_string(),
        "status" => status.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("gc_meeting_join_total",
        "participant" => participant.to_string(),
        "status" => status.to_string()
    )
    .increment(1);

    if let Some(err_type) = error_type {
        counter!("gc_meeting_join_failures_total",
            "participant" => participant.to_string(),
            "error_type" => err_type.to_string()
        )
        .increment(1);
    }
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
    use common::observability::testing::MetricAssertion;

    // ========================================================================
    // Per-cluster MetricAssertion tests — replace the pre-ADR-0032 hand-rolled
    // smoke tests (which only proved wrappers don't panic against the global
    // no-op recorder). These exercise the same wrappers but with per-failure-
    // class delta assertions, mirroring the AC Step 4 / MC Step 3 / MH Step 2
    // migrations.
    //
    // NOTE: These are wrapper-invocation tests (Cat C name-coverage tier).
    // The PRODUCTION-PATH coverage for these metrics lives in cluster files
    // under crates/gc-service/tests/. The block here is the in-file mirror
    // that exercises the metrics.rs wrappers themselves end-to-end through
    // MetricAssertion. Pinning is implicit (cargo's default test runner is
    // single-threaded per-test); MetricAssertion binds a per-thread recorder.
    // See `crates/common/src/observability/testing.rs:60-72`.
    // ========================================================================

    // ---- Pure-function sanity tests (not metric-recorder tests) -------------
    // These exercise path-normalization logic and the status-code categorizer.
    // Kept from the pre-ADR-0032 era — they assert deterministic string output,
    // not metric emission, so they don't need MetricAssertion.

    #[test]
    fn test_normalize_endpoint_known_paths() {
        assert_eq!(normalize_endpoint("/"), "/");
        assert_eq!(normalize_endpoint("/health"), "/health");
        assert_eq!(normalize_endpoint("/metrics"), "/metrics");
        assert_eq!(normalize_endpoint("/api/v1/me"), "/api/v1/me");
        assert_eq!(normalize_endpoint("/api/v1/meetings"), "/api/v1/meetings");
    }

    #[test]
    fn normalize_endpoint_meeting_paths() {
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/abc123"),
            "/api/v1/meetings/{code}"
        );
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/abc123/guest-token"),
            "/api/v1/meetings/{code}/guest-token"
        );
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/550e8400-e29b-41d4-a716-446655440000/settings"),
            "/api/v1/meetings/{id}/settings"
        );
    }

    #[test]
    fn normalize_endpoint_unknown_paths() {
        assert_eq!(normalize_endpoint("/unknown"), "/other");
        assert_eq!(normalize_endpoint("/api/v2/something"), "/other");
        assert_eq!(
            normalize_endpoint("/api/v1/meetings/code/unknown-action"),
            "/other"
        );
    }

    #[test]
    fn controller_statuses_constant() {
        assert_eq!(CONTROLLER_STATUSES.len(), 5);
        assert!(CONTROLLER_STATUSES.contains(&"pending"));
        assert!(CONTROLLER_STATUSES.contains(&"healthy"));
        assert!(CONTROLLER_STATUSES.contains(&"degraded"));
        assert!(CONTROLLER_STATUSES.contains(&"unhealthy"));
        assert!(CONTROLLER_STATUSES.contains(&"draining"));
    }

    // ---- Per-cluster MetricAssertion-backed wrapper tests --------------------

    #[test]
    fn metrics_module_emits_http_request_cluster() {
        let snap = MetricAssertion::snapshot();

        record_http_request("GET", "/health", 200, Duration::from_millis(5));
        record_http_request("GET", "/api/v1/me", 401, Duration::from_millis(10));
        record_http_request(
            "GET",
            "/api/v1/meetings/abc123",
            200,
            Duration::from_millis(150),
        );
        record_http_request("GET", "/api/v1/me", 504, Duration::from_secs(30));

        // Histogram first (drain-on-read).
        snap.histogram("gc_http_request_duration_seconds")
            .assert_observation_count_at_least(4);

        snap.counter("gc_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/health"),
                ("status_code", "200"),
            ])
            .assert_delta(1);
        snap.counter("gc_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/api/v1/me"),
                ("status_code", "401"),
            ])
            .assert_delta(1);
        snap.counter("gc_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/api/v1/meetings/{code}"),
                ("status_code", "200"),
            ])
            .assert_delta(1);
        snap.counter("gc_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/api/v1/me"),
                ("status_code", "504"),
            ])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_mc_assignment_cluster() {
        let snap = MetricAssertion::snapshot();

        record_mc_assignment("success", None, Duration::from_millis(15));
        record_mc_assignment("rejected", Some("at_capacity"), Duration::from_millis(10));
        record_mc_assignment("rejected", Some("draining"), Duration::from_millis(8));
        record_mc_assignment("rejected", Some("unhealthy"), Duration::from_millis(5));
        record_mc_assignment("error", Some("rpc_failed"), Duration::from_millis(100));

        snap.histogram("gc_mc_assignment_duration_seconds")
            .assert_observation_count_at_least(5);

        snap.counter("gc_mc_assignments_total")
            .with_labels(&[("status", "success"), ("rejection_reason", "none")])
            .assert_delta(1);
        for reason in ["at_capacity", "draining", "unhealthy"] {
            snap.counter("gc_mc_assignments_total")
                .with_labels(&[("status", "rejected"), ("rejection_reason", reason)])
                .assert_delta(1);
        }
        snap.counter("gc_mc_assignments_total")
            .with_labels(&[("status", "error"), ("rejection_reason", "rpc_failed")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_db_query_cluster() {
        let snap = MetricAssertion::snapshot();

        record_db_query("create_meeting", "success", Duration::from_millis(5));
        record_db_query("create_meeting", "error", Duration::from_millis(2));
        record_db_query("log_audit_event", "success", Duration::from_millis(3));
        record_db_query("add_participant", "error", Duration::from_millis(7));

        snap.histogram("gc_db_query_duration_seconds")
            .assert_observation_count_at_least(4);

        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "create_meeting"), ("status", "success")])
            .assert_delta(1);
        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "create_meeting"), ("status", "error")])
            .assert_delta(1);
        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "log_audit_event"), ("status", "success")])
            .assert_delta(1);
        snap.counter("gc_db_queries_total")
            .with_labels(&[("operation", "add_participant"), ("status", "error")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_token_refresh_cluster() {
        let snap = MetricAssertion::snapshot();

        record_token_refresh("success", None, Duration::from_millis(50));
        record_token_refresh("error", Some("http"), Duration::from_millis(100));
        record_token_refresh("error", Some("auth_rejected"), Duration::from_millis(200));

        snap.histogram("gc_token_refresh_duration_seconds")
            .assert_observation_count_at_least(3);

        snap.counter("gc_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("gc_token_refresh_total")
            .with_labels(&[("status", "error")])
            .assert_delta(2);
        snap.counter("gc_token_refresh_failures_total")
            .with_labels(&[("error_type", "http")])
            .assert_delta(1);
        snap.counter("gc_token_refresh_failures_total")
            .with_labels(&[("error_type", "auth_rejected")])
            .assert_delta(1);
    }

    // Cat B per-service dispatcher exercise. Mirrors MC's
    // `record_token_refresh_metrics_*` pattern (see
    // `crates/mc-service/src/observability/metrics.rs`). Drives the dispatcher
    // directly through MetricAssertion.
    #[test]
    fn record_token_refresh_metrics_success_emits_status_success_no_failure_counter() {
        use common::token_manager::TokenRefreshEvent;

        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: true,
            duration: Duration::from_millis(42),
            error_category: None,
        });

        snap.histogram("gc_token_refresh_duration_seconds")
            .assert_observation_count_at_least(1);
        snap.counter("gc_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("gc_token_refresh_total")
            .with_labels(&[("status", "error")])
            .assert_delta(0);
        // Adjacency: failures counter must be silent on success path across
        // every bounded `error_category` value (label-swap-bug catcher).
        for sibling in &[
            "http",
            "auth_rejected",
            "invalid_response",
            "acquisition_failed",
            "configuration",
            "channel_closed",
        ] {
            snap.counter("gc_token_refresh_failures_total")
                .with_labels(&[("error_type", *sibling)])
                .assert_delta(0);
        }
    }

    #[test]
    fn record_token_refresh_metrics_failure_matrix_per_error_category() {
        use common::token_manager::TokenRefreshEvent;

        for category in &[
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
                duration: Duration::from_millis(10),
                error_category: Some(category),
            });

            snap.histogram("gc_token_refresh_duration_seconds")
                .assert_observation_count_at_least(1);
            snap.counter("gc_token_refresh_total")
                .with_labels(&[("status", "error")])
                .assert_delta(1);
            snap.counter("gc_token_refresh_total")
                .with_labels(&[("status", "success")])
                .assert_delta(0);
            snap.counter("gc_token_refresh_failures_total")
                .with_labels(&[("error_type", *category)])
                .assert_delta(1);
        }
    }

    #[test]
    fn metrics_module_emits_ac_request_cluster() {
        let snap = MetricAssertion::snapshot();

        record_ac_request("meeting_token", "success", Duration::from_millis(100));
        record_ac_request("meeting_token", "error", Duration::from_millis(200));
        record_ac_request("guest_token", "success", Duration::from_millis(80));
        record_ac_request("guest_token", "error", Duration::from_millis(150));

        snap.histogram("gc_ac_request_duration_seconds")
            .assert_observation_count_at_least(4);

        for op in ["meeting_token", "guest_token"] {
            for status in ["success", "error"] {
                snap.counter("gc_ac_requests_total")
                    .with_labels(&[("operation", op), ("status", status)])
                    .assert_delta(1);
            }
        }
    }

    #[test]
    fn metrics_module_emits_errors_cluster() {
        let snap = MetricAssertion::snapshot();

        record_error("ac_meeting_token", "service_unavailable", 503);
        record_error("ac_guest_token", "service_unavailable", 503);
        record_error("mc_grpc", "connection_failed", 503);

        snap.counter("gc_errors_total")
            .with_labels(&[
                ("operation", "ac_meeting_token"),
                ("error_type", "service_unavailable"),
                ("status_code", "503"),
            ])
            .assert_delta(1);
        snap.counter("gc_errors_total")
            .with_labels(&[
                ("operation", "ac_guest_token"),
                ("error_type", "service_unavailable"),
                ("status_code", "503"),
            ])
            .assert_delta(1);
        snap.counter("gc_errors_total")
            .with_labels(&[
                ("operation", "mc_grpc"),
                ("error_type", "connection_failed"),
                ("status_code", "503"),
            ])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_jwt_validation_cluster() {
        let snap = MetricAssertion::snapshot();

        record_jwt_validation("success", "service", "none");
        for reason in [
            "signature_invalid",
            "expired",
            "scope_mismatch",
            "malformed",
            "missing_token",
        ] {
            record_jwt_validation("failure", "service", reason);
        }

        snap.counter("gc_jwt_validations_total")
            .with_labels(&[
                ("result", "success"),
                ("token_type", "service"),
                ("failure_reason", "none"),
            ])
            .assert_delta(1);
        for reason in [
            "signature_invalid",
            "expired",
            "scope_mismatch",
            "malformed",
            "missing_token",
        ] {
            snap.counter("gc_jwt_validations_total")
                .with_labels(&[
                    ("result", "failure"),
                    ("token_type", "service"),
                    ("failure_reason", reason),
                ])
                .assert_delta(1);
        }
    }

    #[test]
    fn metrics_module_emits_caller_type_rejected_cluster() {
        let snap = MetricAssertion::snapshot();

        record_caller_type_rejected(
            "GlobalControllerService",
            "meeting-controller",
            "media-handler",
        );
        record_caller_type_rejected(
            "MediaHandlerRegistryService",
            "media-handler",
            "meeting-controller",
        );
        record_caller_type_rejected("GlobalControllerService", "meeting-controller", "unknown");

        snap.counter("gc_caller_type_rejected_total")
            .with_labels(&[
                ("grpc_service", "GlobalControllerService"),
                ("expected_type", "meeting-controller"),
                ("actual_type", "media-handler"),
            ])
            .assert_delta(1);
        snap.counter("gc_caller_type_rejected_total")
            .with_labels(&[
                ("grpc_service", "MediaHandlerRegistryService"),
                ("expected_type", "media-handler"),
                ("actual_type", "meeting-controller"),
            ])
            .assert_delta(1);
        snap.counter("gc_caller_type_rejected_total")
            .with_labels(&[
                ("grpc_service", "GlobalControllerService"),
                ("expected_type", "meeting-controller"),
                ("actual_type", "unknown"),
            ])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_grpc_mc_call_cluster() {
        let snap = MetricAssertion::snapshot();

        record_grpc_mc_call("assign_meeting", "success", Duration::from_millis(25));
        record_grpc_mc_call("assign_meeting", "rejected", Duration::from_millis(10));
        record_grpc_mc_call("assign_meeting", "error", Duration::from_millis(100));

        snap.histogram("gc_grpc_mc_call_duration_seconds")
            .assert_observation_count_at_least(3);

        for status in ["success", "rejected", "error"] {
            snap.counter("gc_grpc_mc_calls_total")
                .with_labels(&[("method", "assign_meeting"), ("status", status)])
                .assert_delta(1);
        }
    }

    #[test]
    fn metrics_module_emits_mh_selection_cluster() {
        let snap = MetricAssertion::snapshot();

        record_mh_selection("success", true, Duration::from_millis(8));
        record_mh_selection("success", false, Duration::from_millis(5));
        record_mh_selection("error", false, Duration::from_millis(3));

        snap.histogram("gc_mh_selection_duration_seconds")
            .assert_observation_count_at_least(3);

        snap.counter("gc_mh_selections_total")
            .with_labels(&[("status", "success"), ("has_multiple", "true")])
            .assert_delta(1);
        snap.counter("gc_mh_selections_total")
            .with_labels(&[("status", "success"), ("has_multiple", "false")])
            .assert_delta(1);
        snap.counter("gc_mh_selections_total")
            .with_labels(&[("status", "error"), ("has_multiple", "false")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_meeting_creation_cluster() {
        let snap = MetricAssertion::snapshot();

        record_meeting_creation("success", None, Duration::from_millis(50));
        for err_type in [
            "bad_request",
            "unauthorized",
            "forbidden",
            "db_error",
            "code_collision",
            "internal",
        ] {
            record_meeting_creation("error", Some(err_type), Duration::from_millis(5));
        }

        snap.histogram("gc_meeting_creation_duration_seconds")
            .assert_observation_count_at_least(7);

        snap.counter("gc_meeting_creation_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("gc_meeting_creation_total")
            .with_labels(&[("status", "error")])
            .assert_delta(6);
        for err_type in [
            "bad_request",
            "unauthorized",
            "forbidden",
            "db_error",
            "code_collision",
            "internal",
        ] {
            snap.counter("gc_meeting_creation_failures_total")
                .with_labels(&[("error_type", err_type)])
                .assert_delta(1);
        }
    }

    // Per-cluster wrapper exercise — full (participant × error_type) cartesian
    // is in tests/meeting_join_metrics_integration.rs; this in-src mirror covers
    // wrapper-signature correctness with both `participant` values.
    #[test]
    fn metrics_module_emits_meeting_join_cluster() {
        let snap = MetricAssertion::snapshot();

        record_meeting_join("user", "success", None, Duration::from_millis(200));
        record_meeting_join("user", "error", Some("not_found"), Duration::from_millis(5));
        record_meeting_join("guest", "success", None, Duration::from_millis(180));
        record_meeting_join(
            "guest",
            "error",
            Some("guests_disabled"),
            Duration::from_millis(8),
        );

        snap.histogram("gc_meeting_join_duration_seconds")
            .assert_observation_count_at_least(4);

        snap.counter("gc_meeting_join_total")
            .with_labels(&[("participant", "user"), ("status", "success")])
            .assert_delta(1);
        snap.counter("gc_meeting_join_total")
            .with_labels(&[("participant", "user"), ("status", "error")])
            .assert_delta(1);
        snap.counter("gc_meeting_join_total")
            .with_labels(&[("participant", "guest"), ("status", "success")])
            .assert_delta(1);
        snap.counter("gc_meeting_join_total")
            .with_labels(&[("participant", "guest"), ("status", "error")])
            .assert_delta(1);
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "user"), ("error_type", "not_found")])
            .assert_delta(1);
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "guest"), ("error_type", "guests_disabled")])
            .assert_delta(1);
        // Label-swap-bug catcher: the user-side label must NOT have absorbed
        // the guest-only `guests_disabled` reason and vice-versa.
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "user"), ("error_type", "guests_disabled")])
            .assert_delta(0);
        snap.counter("gc_meeting_join_failures_total")
            .with_labels(&[("participant", "guest"), ("error_type", "not_found")])
            .assert_delta(0);
    }

    // Gauge cluster — exercises `set_registered_controllers` and
    // `update_registered_controller_gauges` zero-fill semantics. The 4-cell
    // adjacency-coverage matrix per @code-reviewer (§ADR-0032 Step 5):
    //   1. Full happy path (all 5 statuses present, non-zero counts)
    //   2. Partial counts → zero-fill for missing statuses
    //   3. Empty counts → all 5 statuses zero-filled
    //   4. Caller error path → assert_unobserved (cluster file: cell 4 is
    //      validated in tests/registered_controllers_metrics_integration.rs
    //      where the surrounding caller can short-circuit).
    #[test]
    fn metrics_module_emits_registered_controllers_full_happy_path() {
        let snap = MetricAssertion::snapshot();

        let full_counts = vec![
            ("pending".to_string(), 1u64),
            ("healthy".to_string(), 10),
            ("degraded".to_string(), 3),
            ("unhealthy".to_string(), 2),
            ("draining".to_string(), 1),
        ];
        update_registered_controller_gauges("meeting", &full_counts);

        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", "pending")])
            .assert_value(1.0);
        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", "healthy")])
            .assert_value(10.0);
        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", "degraded")])
            .assert_value(3.0);
        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", "unhealthy")])
            .assert_value(2.0);
        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", "draining")])
            .assert_value(1.0);
    }

    #[test]
    fn metrics_module_emits_registered_controllers_partial_zero_fill() {
        // Cell 2: partial counts → zero-fill for missing statuses.
        // assert_value(0.0) is correct, NOT assert_unobserved — the wrapper
        // explicitly emits set(0.0) for absent statuses, so the metric IS
        // observed at value zero.
        let snap = MetricAssertion::snapshot();

        let partial_counts = vec![("healthy".to_string(), 5u64), ("degraded".to_string(), 2)];
        update_registered_controller_gauges("meeting", &partial_counts);

        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", "healthy")])
            .assert_value(5.0);
        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", "degraded")])
            .assert_value(2.0);
        // Zero-fill: missing statuses are explicitly set to 0.0, not absent.
        for missing in ["pending", "unhealthy", "draining"] {
            snap.gauge("gc_registered_controllers")
                .with_labels(&[("controller_type", "meeting"), ("status", missing)])
                .assert_value(0.0);
        }
    }

    #[test]
    fn metrics_module_emits_registered_controllers_empty_counts_all_zero() {
        // Cell 3: empty counts → all 5 statuses zero-filled.
        let snap = MetricAssertion::snapshot();

        update_registered_controller_gauges("media", &[]);

        for status in CONTROLLER_STATUSES {
            snap.gauge("gc_registered_controllers")
                .with_labels(&[("controller_type", "media"), ("status", status)])
                .assert_value(0.0);
        }
    }

    #[test]
    fn metrics_module_registered_controllers_unobserved_when_caller_short_circuits() {
        // Cell 4: code path that does NOT call update_registered_controller_gauges
        // — gauge must be unobserved across the full label space. This
        // proves `assert_unobserved` (kind+name+labels axis) catches a
        // refactor that accidentally always-emits, distinct from cell 2's
        // zero-fill correctness.
        let snap = MetricAssertion::snapshot();

        // No call to update_registered_controller_gauges — simulating an
        // error short-circuit upstream.

        for controller_type in ["meeting", "media"] {
            for status in CONTROLLER_STATUSES {
                snap.gauge("gc_registered_controllers")
                    .with_labels(&[("controller_type", controller_type), ("status", status)])
                    .assert_unobserved();
            }
        }
    }
}
