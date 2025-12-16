//! Metrics definitions for AC service per ADR-0011
//!
//! All metrics follow Prometheus naming conventions:
//! - `ac_` prefix for Auth Controller
//! - `_total` suffix for counters
//! - `_seconds` suffix for duration histograms
//!
//! # Cardinality
//!
//! Labels are bounded to prevent cardinality explosion:
//! - `grant_type`: 4 values max (client_credentials, authorization_code, etc.)
//! - `status`: 2 values (success, error)
//! - `error_category`: 4 values (authentication, authorization, cryptographic, internal)
//! - `operation`: bounded by code (select, insert, update, delete)
//! - `table`: bounded by schema (~5 tables)

use metrics::{counter, gauge, histogram};
use std::time::Duration;

// ============================================================================
// Token Metrics
// ============================================================================

/// Record token issuance duration and outcome
///
/// Metric: `ac_token_issuance_duration_seconds`
/// Labels: `grant_type`, `status`
///
/// SLO target: p99 < 350ms
pub fn record_token_issuance(grant_type: &str, status: &str, duration: Duration) {
    histogram!("ac_token_issuance_duration_seconds", "grant_type" => grant_type.to_string(), "status" => status.to_string())
        .record(duration.as_secs_f64());

    counter!("ac_token_issuance_total", "grant_type" => grant_type.to_string(), "status" => status.to_string())
        .increment(1);
}

/// Record token validation result
///
/// Metric: `ac_token_validations_total`
/// Labels: `status`, `error_category`
///
/// NOTE: Defined per ADR-0011 for future token validation metrics.
#[allow(dead_code)]
pub fn record_token_validation(status: &str, error_category: Option<&str>) {
    let category = error_category.unwrap_or("none");
    counter!("ac_token_validations_total", "status" => status.to_string(), "error_category" => category.to_string())
        .increment(1);
}

// ============================================================================
// Key Management Metrics
// ============================================================================

/// Record key rotation event
///
/// Metric: `ac_key_rotation_total`
/// Labels: `status`
pub fn record_key_rotation(status: &str) {
    counter!("ac_key_rotation_total", "status" => status.to_string()).increment(1);
}

/// Update signing key age gauge
///
/// Metric: `ac_signing_key_age_days`
#[allow(dead_code)]
pub fn set_signing_key_age_days(age_days: f64) {
    gauge!("ac_signing_key_age_days").set(age_days);
}

/// Update active signing keys count
///
/// Metric: `ac_active_signing_keys`
#[allow(dead_code)]
pub fn set_active_signing_keys(count: u64) {
    gauge!("ac_active_signing_keys").set(count as f64);
}

/// Record key rotation last success timestamp
///
/// Metric: `ac_key_rotation_last_success_timestamp`
#[allow(dead_code)]
pub fn set_key_rotation_last_success(timestamp_secs: f64) {
    gauge!("ac_key_rotation_last_success_timestamp").set(timestamp_secs);
}

// ============================================================================
// Rate Limiting Metrics
// ============================================================================

/// Record rate limit decision
///
/// Metric: `ac_rate_limit_decisions_total`
/// Labels: `action` (allowed, rejected)
#[allow(dead_code)]
pub fn record_rate_limit_decision(action: &str) {
    counter!("ac_rate_limit_decisions_total", "action" => action.to_string()).increment(1);
}

// ============================================================================
// Database Metrics
// ============================================================================

/// Record database query execution
///
/// Metric: `ac_db_query_duration_seconds`, `ac_db_queries_total`
/// Labels: `operation`, `table`, `status`
#[allow(dead_code)]
pub fn record_db_query(operation: &str, table: &str, status: &str, duration: Duration) {
    histogram!("ac_db_query_duration_seconds", "operation" => operation.to_string(), "table" => table.to_string())
        .record(duration.as_secs_f64());

    counter!("ac_db_queries_total", "operation" => operation.to_string(), "table" => table.to_string(), "status" => status.to_string())
        .increment(1);
}

// ============================================================================
// Crypto Metrics
// ============================================================================

/// Record bcrypt operation duration
///
/// Metric: `ac_bcrypt_duration_seconds`
/// Labels: `operation` (hash, verify)
///
/// Note: Uses coarse buckets (50ms minimum) per Security specialist
/// to prevent timing side-channel attacks.
#[allow(dead_code)]
pub fn record_bcrypt_duration(operation: &str, duration: Duration) {
    histogram!("ac_bcrypt_duration_seconds", "operation" => operation.to_string())
        .record(duration.as_secs_f64());
}

// ============================================================================
// JWKS Metrics
// ============================================================================

/// Record JWKS cache operation
///
/// Metric: `ac_jwks_requests_total`
/// Labels: `cache_status` (hit, miss, bypass)
pub fn record_jwks_request(cache_status: &str) {
    counter!("ac_jwks_requests_total", "cache_status" => cache_status.to_string()).increment(1);
}

// ============================================================================
// Audit Metrics
// ============================================================================

/// Record audit log failure (compliance-critical)
///
/// Metric: `ac_audit_log_failures_total`
/// Labels: `event_type`, `reason`
///
/// ALERT: Any non-zero value should trigger oncall page
#[allow(dead_code)]
pub fn record_audit_log_failure(event_type: &str, reason: &str) {
    counter!("ac_audit_log_failures_total", "event_type" => event_type.to_string(), "reason" => reason.to_string())
        .increment(1);
}

// ============================================================================
// Error Metrics
// ============================================================================

/// Record error by category
///
/// Metric: `ac_errors_total`
/// Labels: `operation`, `error_category`, `status_code`
pub fn record_error(operation: &str, error_category: &str, status_code: u16) {
    counter!("ac_errors_total",
        "operation" => operation.to_string(),
        "error_category" => error_category.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

// ============================================================================
// HTTP Request Metrics
// ============================================================================

/// Record HTTP request completion
///
/// Metric: `ac_http_requests_total`, `ac_http_request_duration_seconds`
/// Labels: `method`, `path`, `status_code`
///
/// This captures ALL HTTP responses including framework-level errors like:
/// - 415 Unsupported Media Type (wrong Content-Type)
/// - 400 Bad Request (JSON parse errors)
/// - 404 Not Found
/// - 405 Method Not Allowed
pub fn record_http_request(method: &str, path: &str, status_code: u16, duration: Duration) {
    // Normalize path to prevent cardinality explosion
    // Replace UUIDs and numeric IDs with placeholders
    let normalized_path = normalize_path(path);

    histogram!("ac_http_request_duration_seconds",
        "method" => method.to_string(),
        "path" => normalized_path.clone(),
        "status_code" => status_code.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("ac_http_requests_total",
        "method" => method.to_string(),
        "path" => normalized_path,
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

/// Normalize path to prevent label cardinality explosion
///
/// Replaces dynamic segments (UUIDs, numeric IDs) with placeholders.
fn normalize_path(path: &str) -> String {
    // Simple normalization: keep known paths, replace others with pattern
    // This prevents unbounded cardinality from dynamic path segments
    match path {
        "/" => "/".to_string(),
        "/health" => "/health".to_string(),
        "/ready" => "/ready".to_string(),
        "/metrics" => "/metrics".to_string(),
        "/.well-known/jwks.json" => "/.well-known/jwks.json".to_string(),
        "/api/v1/auth/service/token" => "/api/v1/auth/service/token".to_string(),
        "/api/v1/auth/user/token" => "/api/v1/auth/user/token".to_string(),
        "/api/v1/admin/services/register" => "/api/v1/admin/services/register".to_string(),
        "/internal/rotate-keys" => "/internal/rotate-keys".to_string(),
        // For unknown paths, use a generic label to bound cardinality
        _ => "/other".to_string(),
    }
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
    fn test_record_token_issuance() {
        // Test with various grant types and statuses
        record_token_issuance("client_credentials", "success", Duration::from_millis(250));
        record_token_issuance("client_credentials", "error", Duration::from_millis(100));
        record_token_issuance("authorization_code", "success", Duration::from_millis(300));
        record_token_issuance("refresh_token", "error", Duration::from_millis(50));
    }

    #[test]
    fn test_record_token_validation() {
        // Test with and without error category
        record_token_validation("success", None);
        record_token_validation("error", Some("authentication"));
        record_token_validation("error", Some("authorization"));
        record_token_validation("error", Some("cryptographic"));
        record_token_validation("error", Some("internal"));
    }

    #[test]
    fn test_record_key_rotation() {
        // Test with success and error statuses
        record_key_rotation("success");
        record_key_rotation("error");
    }

    #[test]
    fn test_set_signing_key_age_days() {
        // Test with various age values
        set_signing_key_age_days(0.0);
        set_signing_key_age_days(15.5);
        set_signing_key_age_days(30.0);
        set_signing_key_age_days(90.0);
    }

    #[test]
    fn test_set_active_signing_keys() {
        // Test with various key counts
        set_active_signing_keys(0);
        set_active_signing_keys(1);
        set_active_signing_keys(2);
        set_active_signing_keys(5);
    }

    #[test]
    fn test_set_key_rotation_last_success() {
        // Test with various timestamps (Unix epoch seconds)
        set_key_rotation_last_success(0.0);
        set_key_rotation_last_success(1234567890.0);
        set_key_rotation_last_success(1700000000.0);
    }

    #[test]
    fn test_record_rate_limit_decision() {
        // Test with allowed and rejected actions
        record_rate_limit_decision("allowed");
        record_rate_limit_decision("rejected");
    }

    #[test]
    fn test_record_db_query() {
        // Test with various operations, tables, and statuses
        record_db_query(
            "select",
            "service_credentials",
            "success",
            Duration::from_millis(5),
        );
        record_db_query(
            "insert",
            "service_credentials",
            "success",
            Duration::from_millis(10),
        );
        record_db_query(
            "update",
            "signing_keys",
            "success",
            Duration::from_millis(7),
        );
        record_db_query("delete", "signing_keys", "error", Duration::from_millis(3));
        record_db_query("select", "jwks_cache", "success", Duration::from_millis(2));
    }

    #[test]
    fn test_record_bcrypt_duration() {
        // Test with hash and verify operations
        record_bcrypt_duration("hash", Duration::from_millis(150));
        record_bcrypt_duration("verify", Duration::from_millis(120));
        record_bcrypt_duration("hash", Duration::from_millis(200));
    }

    #[test]
    fn test_record_jwks_request() {
        // Test with various cache statuses
        record_jwks_request("hit");
        record_jwks_request("miss");
        record_jwks_request("bypass");
    }

    #[test]
    fn test_record_audit_log_failure() {
        // Test with various event types and reasons
        record_audit_log_failure("token_issued", "db_write_failed");
        record_audit_log_failure("key_rotation", "encryption_failed");
        record_audit_log_failure("authentication", "log_overflow");
    }

    #[test]
    fn test_record_error() {
        // Test with various operations, categories, and status codes
        record_error("token_issuance", "authentication", 401);
        record_error("token_issuance", "authorization", 403);
        record_error("key_rotation", "cryptographic", 500);
        record_error("db_query", "internal", 500);
        record_error("rate_limit", "authorization", 429);
    }

    #[test]
    fn test_record_http_request() {
        // Test successful requests
        record_http_request(
            "GET",
            "/.well-known/jwks.json",
            200,
            Duration::from_millis(50),
        );
        record_http_request(
            "POST",
            "/api/v1/auth/service/token",
            200,
            Duration::from_millis(250),
        );

        // Test client errors (including framework-level errors)
        record_http_request(
            "POST",
            "/api/v1/auth/service/token",
            400,
            Duration::from_millis(5),
        );
        record_http_request(
            "POST",
            "/api/v1/auth/service/token",
            415,
            Duration::from_millis(2),
        );
        record_http_request("GET", "/not-found", 404, Duration::from_millis(1));
        record_http_request(
            "DELETE",
            "/api/v1/auth/service/token",
            405,
            Duration::from_millis(1),
        );

        // Test server errors
        record_http_request(
            "POST",
            "/api/v1/auth/service/token",
            500,
            Duration::from_millis(100),
        );
    }

    #[test]
    fn test_normalize_path_known_paths() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("/health"), "/health");
        assert_eq!(normalize_path("/ready"), "/ready");
        assert_eq!(normalize_path("/metrics"), "/metrics");
        assert_eq!(
            normalize_path("/.well-known/jwks.json"),
            "/.well-known/jwks.json"
        );
        assert_eq!(
            normalize_path("/api/v1/auth/service/token"),
            "/api/v1/auth/service/token"
        );
        assert_eq!(
            normalize_path("/api/v1/auth/user/token"),
            "/api/v1/auth/user/token"
        );
        assert_eq!(
            normalize_path("/api/v1/admin/services/register"),
            "/api/v1/admin/services/register"
        );
        assert_eq!(
            normalize_path("/internal/rotate-keys"),
            "/internal/rotate-keys"
        );
    }

    #[test]
    fn test_normalize_path_unknown_paths() {
        // Unknown paths should be normalized to "/other" to bound cardinality
        assert_eq!(normalize_path("/unknown"), "/other");
        assert_eq!(normalize_path("/api/v2/something"), "/other");
        assert_eq!(normalize_path("/users/123"), "/other");
        assert_eq!(normalize_path("/api/v1/auth/service/token/extra"), "/other");
    }
}
