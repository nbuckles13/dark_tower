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
#[allow(dead_code)]
pub fn record_error(operation: &str, error_category: &str, status_code: u16) {
    counter!("ac_errors_total",
        "operation" => operation.to_string(),
        "error_category" => error_category.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_token_issuance_compiles() {
        // Just verify the function compiles with correct types
        // Actual metric recording requires a recorder to be installed
        let _ = || {
            record_token_issuance("client_credentials", "success", Duration::from_millis(250));
        };
    }

    #[test]
    fn test_record_db_query_compiles() {
        let _ = || {
            record_db_query(
                "select",
                "service_credentials",
                "success",
                Duration::from_millis(5),
            );
        };
    }

    #[test]
    fn test_record_rate_limit_compiles() {
        let _ = || {
            record_rate_limit_decision("allowed");
            record_rate_limit_decision("rejected");
        };
    }
}
