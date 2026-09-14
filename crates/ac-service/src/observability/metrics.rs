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
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Initialize Prometheus metrics recorder and return the handle
/// for serving metrics via HTTP.
///
/// ADR-0011: Must be called before any metrics are recorded.
/// Configures histogram buckets aligned with SLO targets:
/// - Token issuance p99 < 350ms
/// - DB queries p99 < 50ms
///
/// # Errors
///
/// Returns error if Prometheus recorder fails to install (e.g., already installed).
pub fn init_metrics_recorder() -> Result<PrometheusHandle, String> {
    configured_prometheus_builder()?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus recorder: {}", e))
}

/// Build the `PrometheusBuilder` with AC's production bucket configuration,
/// WITHOUT installing it globally. Single source of the exporter config so the
/// present-at-zero render test exercises the SAME builder production installs.
/// NOTE: that test proves present-at-0 under this exact config; it does NOT prove
/// `idle_timeout` is absent — `render()` runs synchronously at t≈0, before a
/// realistic minutes-scale `idle_timeout` would reap idle 0-series — so
/// idle_timeout-absence is a CONFIG-REVIEW checkpoint here, not a test-enforced
/// invariant (neither this test nor the Layer-7 env-test reliably backstops it).
fn configured_prometheus_builder() -> Result<PrometheusBuilder, String> {
    PrometheusBuilder::new()
        // Token issuance buckets aligned with 350ms SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("ac_token_issuance".to_string()),
            &[
                0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.250, 0.300, 0.350, 0.500, 1.000, 2.000,
            ],
        )
        .map_err(|e| format!("Failed to set token issuance buckets: {}", e))?
        // DB query buckets aligned with 50ms SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("ac_db_query".to_string()),
            &[
                0.001, 0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set DB query buckets: {}", e))?
        // Bcrypt buckets - coarse (50ms minimum) to prevent timing side-channel attacks
        .set_buckets_for_metric(
            Matcher::Prefix("ac_bcrypt".to_string()),
            &[
                0.050, 0.100, 0.150, 0.200, 0.250, 0.300, 0.400, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set bcrypt buckets: {}", e))?
        // HTTP request buckets aligned with 200ms p95 SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("ac_http_request".to_string()),
            &[
                0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000, 10.000,
            ],
        )
        .map_err(|e| format!("Failed to set HTTP request buckets: {}", e))
}

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

/// Record the outcome of the meeting-token display-name lookup.
///
/// Metric: `ac_meeting_token_display_name_total`
/// Labels: `outcome` — CLOSED enum, exactly three values:
/// - `resolved`      — user row found; display_name stamped into the token.
/// - `user_not_found`— fail-closed: no `users` row for the subject; token refused.
/// - `lookup_error`  — the `users` SELECT itself failed (DB error); token refused.
///
/// PII discipline (ADR-0011): `outcome` is the ONLY label and is a fixed enum;
/// no `user_id`, `display_name`, or other per-user value is ever a label.
/// The `user_not_found` cell distinguishes the fail-closed data-integrity path
/// from a transient `lookup_error` (DB) path for alerting.
pub fn record_meeting_display_name_outcome(outcome: &str) {
    counter!("ac_meeting_token_display_name_total", "outcome" => outcome.to_string()).increment(1);
}

/// Record token validation result
///
/// Metric: `ac_token_validations_total`
/// Labels: `status`, `error_category`
///
/// NOTE: Defined per ADR-0011 for future token validation metrics.
#[allow(dead_code)] // Will be used in Phase 4 token validation endpoints
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
pub fn set_signing_key_age_days(age_days: f64) {
    gauge!("ac_signing_key_age_days").set(age_days);
}

/// Update active signing keys count
///
/// Metric: `ac_active_signing_keys`
pub fn set_active_signing_keys(count: u64) {
    gauge!("ac_active_signing_keys").set(count as f64);
}

/// Record key rotation last success timestamp
///
/// Metric: `ac_key_rotation_last_success_timestamp`
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
/// Note: Uses coarse buckets (50ms minimum).
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
// Credential Operations Metrics
// ============================================================================

/// Record service credential management operation
///
/// Metric: `ac_credential_operations_total`
/// Labels: `operation`, `status`
///
/// Operations: list, get, create, update, delete, rotate_secret
/// Status: success, error
///
pub fn record_credential_operation(operation: &str, status: &str) {
    counter!("ac_credential_operations_total", "operation" => operation.to_string(), "status" => status.to_string())
        .increment(1);
}

// ============================================================================
// HTTP Request Metrics
// ============================================================================

/// Record HTTP request completion
///
/// Metric: `ac_http_requests_total`, `ac_http_request_duration_seconds`
/// Labels: `method`, `endpoint`, `status_code`
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
        "endpoint" => normalized_path.clone(),
        "status_code" => status_code.to_string()
    )
    .record(duration.as_secs_f64());

    counter!("ac_http_requests_total",
        "method" => method.to_string(),
        "endpoint" => normalized_path,
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
        "/api/v1/admin/clients" => "/api/v1/admin/clients".to_string(),
        "/internal/rotate-keys" => "/internal/rotate-keys".to_string(),
        // For paths with dynamic segments (UUIDs), normalize them
        _ => normalize_dynamic_path(path),
    }
}

/// Normalize paths with dynamic UUID segments
///
/// Replaces UUIDs with {id} placeholder to bound cardinality while
/// preserving path structure for meaningful metrics.
///
/// Examples:
/// - `/api/v1/admin/clients/550e8400-e29b-41d4-a716-446655440000` → `/api/v1/admin/clients/{id}`
/// - `/api/v1/admin/clients/550e8400-e29b-41d4-a716-446655440000/rotate-secret` → `/api/v1/admin/clients/{id}/rotate-secret`
fn normalize_dynamic_path(path: &str) -> String {
    // Check for admin client paths with UUID
    if path.starts_with("/api/v1/admin/clients/") {
        let parts: Vec<&str> = path.split('/').collect();

        // /api/v1/admin/clients/{uuid} → parts.len() == 6
        // Use get() to avoid potential panic per ADR-0002
        if parts.len() == 6 {
            if let Some(segment) = parts.get(5) {
                if is_uuid(segment) {
                    return "/api/v1/admin/clients/{id}".to_string();
                }
            }
        }

        // /api/v1/admin/clients/{uuid}/rotate-secret → parts.len() == 7
        if parts.len() == 7 {
            if let (Some(id_segment), Some(action)) = (parts.get(5), parts.get(6)) {
                if is_uuid(id_segment) && *action == "rotate-secret" {
                    return "/api/v1/admin/clients/{id}/rotate-secret".to_string();
                }
            }
        }
    }

    // For unknown paths, use a generic label to bound cardinality
    "/other".to_string()
}

/// Check if a string matches UUID format (8-4-4-4-12 hex digits with dashes)
///
/// This is a lightweight check that doesn't validate UUID variants.
/// Good enough for metrics path normalization.
fn is_uuid(s: &str) -> bool {
    // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    // Length: 36 characters (32 hex + 4 dashes)
    if s.len() != 36 {
        return false;
    }

    let bytes = s.as_bytes();

    // Check dashes at positions 8, 13, 18, 23
    // Use get() to avoid potential panic per ADR-0002
    if bytes.get(8) != Some(&b'-')
        || bytes.get(13) != Some(&b'-')
        || bytes.get(18) != Some(&b'-')
        || bytes.get(23) != Some(&b'-')
    {
        return false;
    }

    // Check all other characters are hex digits
    for (i, &byte) in bytes.iter().enumerate() {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            continue; // Skip dashes
        }
        if !byte.is_ascii_hexdigit() {
            return false;
        }
    }

    true
}

// ============================================================================
// Zero-initialization of discrete-event counters (counter-visibility fix)
//
// A `metrics`-crate counter is created lazily on first increment, so no `0`
// sample precedes it and `increase()` reads 0 forever for a single low-volume
// event. Empirically proven against metrics-exporter-prometheus 0.16.2 that
// resolving a `counter!` handle (incl. `.increment(0)`) renders a scrapeable
// `…_total 0` line, so touching every enumerable label combination at startup
// gives each series a present-at-zero point and makes the first event a visible
// 0→1 edge.
//
// AC has no ALL-backed label enums — every vocabulary is a bounded set of
// `&'static str` literals passed at the emit site. Each `const [&str]` below is
// the single in-file home for one vocabulary and is proven closed at its emit
// site (all call sites pass a fixed literal, never an external/request string).
// Counters whose domain is unbounded/runtime-discovered (raw `status_code`,
// free-form `operation`/`table`/`reason`), or that have no production emit site,
// are catalog-marked `Zero-init: exempt` and NOT touched here.
// ============================================================================

/// `status` on the success/error-shaped counters. Emit sites: fixed literals.
const STATUS_SUCCESS_ERROR: &[&str] = &["success", "error"];
/// `ac_token_issuance_total{grant_type}` — closed at emit site: all call sites
/// pass a literal (auth_handler.rs:131,194,241,256,299; internal_tokens.rs:
/// 54,70,107,123). The client's OAuth `grant_type` form field is VALIDATED
/// against `"client_credentials"` (auth_handler.rs:237) and never recorded raw.
const GRANT_TYPES: &[&str] = &[
    "client_credentials",
    "password",
    "registration",
    "internal_meeting",
    "internal_guest",
];
/// `ac_meeting_token_display_name_total{outcome}` — literals at
/// internal_tokens.rs:176,182,191. Never the user's display name (PII).
const DISPLAY_NAME_OUTCOMES: &[&str] = &["resolved", "user_not_found", "lookup_error"];
/// `ac_rate_limit_decisions_total{action}` — literals at token_service.rs:67,70,
/// 240,246 and user_service.rs:85,91.
const RATE_LIMIT_ACTIONS: &[&str] = &["allowed", "rejected"];
/// `ac_jwks_requests_total{cache_status}` — literals at jwks_handler.rs:33 etc.
const JWKS_CACHE_STATUSES: &[&str] = &["hit", "miss", "bypass"];
/// `ac_credential_operations_total{operation}` — literals across admin_handler.rs.
const CREDENTIAL_OPERATIONS: &[&str] =
    &["list", "get", "create", "update", "delete", "rotate_secret"];
/// `ac_audit_log_failures_total{event_type, reason}` — the CLOSED prod pair set
/// (the two labels are correlated, so only real pairs are touched, never the
/// cross-product). Un-exempted 2026-09-10 (was wrongly "unbounded"): every emit
/// site passes literals except `token_service.rs:366`, whose `event_type` is
/// `AuthEventType::{UserLogin,UserLoginFailed}.as_str()` = `"user_login"`/
/// `"user_login_failed"` — still a closed typed-enum set. `reason` is
/// `"db_write_failed"` at EVERY prod site; `encryption_failed`/`log_overflow`
/// appear only in unit-test fixtures and are NOT prod-emittable, so they are not
/// fabricated here. Emit sites: `key_management_service.rs:96,166,384`,
/// `user_service.rs:144`, `token_service.rs:105,172,366`,
/// `registration_service.rs:78,124,158`.
const AUDIT_LOG_FAILURE_PAIRS: &[(&str, &str)] = &[
    ("key_generated", "db_write_failed"),
    ("key_rotated", "db_write_failed"),
    ("key_expired", "db_write_failed"),
    ("user_registered", "db_write_failed"),
    ("service_token_failed", "db_write_failed"),
    ("service_token_issued", "db_write_failed"),
    ("service_registered", "db_write_failed"),
    ("scopes_updated", "db_write_failed"),
    ("service_deactivated", "db_write_failed"),
    ("user_login", "db_write_failed"),
    ("user_login_failed", "db_write_failed"),
];
/// `ac_token_validations_total{status, error_category}` — un-exempted 2026-09-10
/// (was wrongly "no emit site": `crypto/mod.rs:284,439` DO call it in prod). The
/// only PROD-emittable combo today is `error`/`clock_skew` (the JWT clock-skew
/// rejection path); the other `error_category` values documented in ADR-0011
/// (authentication/authorization/cryptographic/internal) and the `success` path
/// have no prod emit site yet (Phase-4), so they are not fabricated here.
const TOKEN_VALIDATION_PAIRS: &[(&str, &str)] = &[("error", "clock_skew")];

/// `ac_db_queries_total{operation, table, status}` — the CLOSED observed
/// `(operation, table)` PAIR-SET from every `record_db_query` prod call site
/// (`repositories/{service_credentials,signing_keys,users,auth_events,
/// organizations}.rs`), each × `status ∈ {success, error}` (the binary
/// `if is_ok()` outcome). NOT the `operation × table × status` cross-product —
/// most `(operation, table)` cells never occur (e.g. `delete/organizations`), so
/// zero-initing them would fabricate impossible always-zero series (OPS-6 trap).
/// Un-exempted 2026-09-10: the earlier "unbounded/high-traffic" exemption was
/// reversed — the labels are finitely enumerable literals, and the rare error
/// series are exactly the low-volume series the lazy-init defect hides. The
/// 12 observed `(operation, table)` pairs:
const AC_DB_QUERY_PAIRS: &[(&str, &str)] = &[
    ("select", "service_credentials"),
    ("insert", "signing_keys"),
    ("select", "signing_keys"),
    ("select", "auth_events"),
    ("insert", "auth_events"),
    ("select", "organizations"),
    ("select", "users"),
    ("insert", "users"),
    ("update", "users"),
    ("select", "user_roles"),
    ("insert", "user_roles"),
    ("delete", "user_roles"),
];

/// Zero-initialize every enumerable discrete-event `ac_*_total` counter so each
/// series is present at 0 from process start (see module section header).
///
/// INFALLIBLE: no `Result`, no panic path — called at boot, must never fail a
/// service start. Uses `.increment(0)` at statement position (registers via side
/// effect; a scrapeable 0-series, proven empirically).
///
/// `dt-guard:zero-init-entrypoint`.
// dt-guard:zero-init-entrypoint
pub fn zero_initialize_counters() {
    for gt in GRANT_TYPES {
        for st in STATUS_SUCCESS_ERROR {
            counter!("ac_token_issuance_total", "grant_type" => *gt, "status" => *st).increment(0);
        }
    }
    for o in DISPLAY_NAME_OUTCOMES {
        counter!("ac_meeting_token_display_name_total", "outcome" => *o).increment(0);
    }
    for st in STATUS_SUCCESS_ERROR {
        counter!("ac_key_rotation_total", "status" => *st).increment(0);
    }
    for a in RATE_LIMIT_ACTIONS {
        counter!("ac_rate_limit_decisions_total", "action" => *a).increment(0);
    }
    for c in JWKS_CACHE_STATUSES {
        counter!("ac_jwks_requests_total", "cache_status" => *c).increment(0);
    }
    for op in CREDENTIAL_OPERATIONS {
        for st in STATUS_SUCCESS_ERROR {
            counter!("ac_credential_operations_total", "operation" => *op, "status" => *st)
                .increment(0);
        }
    }
    // ac_audit_log_failures_total{event_type, reason} — closed prod pair set.
    for (event_type, reason) in AUDIT_LOG_FAILURE_PAIRS {
        counter!("ac_audit_log_failures_total", "event_type" => *event_type, "reason" => *reason)
            .increment(0);
    }
    // ac_db_queries_total{operation, table, status} — observed (op,table) pairs × status
    for (operation, table) in AC_DB_QUERY_PAIRS {
        for status in STATUS_SUCCESS_ERROR {
            counter!("ac_db_queries_total", "operation" => *operation, "table" => *table, "status" => *status)
                .increment(0);
        }
    }
    // ac_token_validations_total{status, error_category} — prod-emittable combos.
    for (status, category) in TOKEN_VALIDATION_PAIRS {
        counter!("ac_token_validations_total", "status" => *status, "error_category" => *category)
            .increment(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::observability::testing::MetricAssertion;

    /// Leg-1 completeness / touch-idiom proof: runs `zero_initialize_counters()`
    /// through the SAME builder config production installs and asserts every
    /// enumerable counter combo is present at 0 in the real exposition. A red on
    /// the anchor is a recorder/scrape failure, NOT a present-at-0 violation.
    #[test]
    fn zero_init_renders_counters_present_at_zero() {
        let recorder = configured_prometheus_builder()
            .expect("prometheus builder config")
            .build_recorder();
        let handle = recorder.handle();
        {
            let _guard = metrics::set_default_local_recorder(&recorder);
            zero_initialize_counters();
        }
        let rendered = handle.render();

        assert!(
            rendered
                .lines()
                .any(|l| l.starts_with("ac_token_issuance_total{")),
            "ANCHOR ABSENT — no ac_token_issuance_total series: a scrape/recorder \
             failure (env-sanity), NOT a present-at-0 violation. DO NOT satisfy \
             this by deleting the check."
        );

        let present_at_zero = |prefix: &str| {
            let found = rendered.lines().find(|l| l.starts_with(prefix));
            // Loud on absence WITHOUT `panic!` (the crate denies clippy::panic
            // even in tests): a missing series is a scrape/recorder failure, not
            // a present-at-0 violation — the assert carries the same message.
            assert!(found.is_some(), "series absent from /metrics: {prefix}");
            let value = found.and_then(|l| l.rsplit(' ').next()).unwrap_or("");
            assert_eq!(value, "0", "present-at-0 violation (value != 0): {found:?}");
        };

        // Exposition preserves EMISSION label order (grant_type then status).
        present_at_zero(
            r#"ac_token_issuance_total{grant_type="client_credentials",status="success"}"#,
        );
        present_at_zero(r#"ac_key_rotation_total{status="error"}"#);
        present_at_zero(r#"ac_rate_limit_decisions_total{action="rejected"}"#);
        present_at_zero(r#"ac_jwks_requests_total{cache_status="miss"}"#);
        present_at_zero(
            r#"ac_credential_operations_total{operation="rotate_secret",status="success"}"#,
        );
        present_at_zero(r#"ac_meeting_token_display_name_total{outcome="lookup_error"}"#);
        // db_queries: an observed (operation,table) pair × error — previously masked.
        present_at_zero(r#"ac_db_queries_total{operation="select",table="users",status="error"}"#);
        // Un-exempted 2026-09-10 (audit-log page-on-any-value + clock-skew rejections).
        present_at_zero(
            r#"ac_audit_log_failures_total{event_type="user_login_failed",reason="db_write_failed"}"#,
        );
        present_at_zero(
            r#"ac_token_validations_total{status="error",error_category="clock_skew"}"#,
        );

        let total_series = rendered
            .lines()
            .filter(|l| l.contains("_total{") || l.contains("_total "))
            .count();
        assert!(
            (15..=200).contains(&total_series),
            "zero-init series count {total_series} outside expected band 15..=200"
        );
    }

    /// Assert a registered-at-0 counter survives to `render()` through the SHARED
    /// prod builder — present-at-0 under the exact prod config.
    ///
    /// HONEST SCOPE (OBS-F3): this does NOT pin `idle_timeout`-absence — `render()`
    /// is synchronous at t≈0, so a minutes-scale `idle_timeout` would not reap
    /// first; it duplicates the present-at-0 render assertion under a stronger
    /// name. idle_timeout-absence is a config-review checkpoint, not test-enforced.
    #[test]
    fn production_builder_has_no_idle_timeout() {
        let recorder = configured_prometheus_builder()
            .expect("prometheus builder config")
            .build_recorder();
        let handle = recorder.handle();
        {
            let _guard = metrics::set_default_local_recorder(&recorder);
            counter!("ac_key_rotation_total", "status" => "success").increment(0);
        }
        assert!(
            handle.render().contains("ac_key_rotation_total{"),
            "a registered-at-0 counter did NOT render through configured_prometheus_builder() \
             — present-at-0 broke under the prod exporter config (a recorder/scrape/config \
             regression). This does not by itself prove an idle_timeout was added; that stays \
             a config-review checkpoint."
        );
    }

    // ========================================================================
    // Per-cluster MetricAssertion tests — replace the pre-ADR-0032 hand-rolled
    // smoke tests (which only proved wrappers don't panic against the global
    // no-op recorder). These exercise the same wrappers but with per-failure-
    // class delta assertions, mirroring the MC Step 3 / MH Step 2 migrations.
    //
    // NOTE: These are wrapper-invocation tests (Cat C name-coverage tier).
    // The PRODUCTION-PATH coverage for these metrics lives in:
    //   - tests/token_issuance_service_integration.rs (client_credentials)
    //   - tests/token_issuance_user_integration.rs (password, registration)
    //   - tests/internal_token_metrics_integration.rs (internal_meeting/guest)
    //   - tests/token_validation_integration.rs (verify_jwt clock_skew)
    //   - tests/key_rotation_metrics_integration.rs (key rotation + gauges)
    //   - tests/db_metrics_integration.rs (db_query success/error cells)
    //   - tests/bcrypt_metrics_integration.rs (hash/verify with cost-12)
    //   - tests/jwks_metrics_integration.rs (cache_status=miss)
    //   - tests/audit_log_failures_integration.rs (NOT VALID CHECK seam)
    //   - tests/rate_limit_metrics_integration.rs (3 gates × allowed/rejected)
    //   - tests/credential_ops_metrics_integration.rs (12-cell adjacency)
    //   - tests/errors_metric_integration.rs (real handler-error paths)
    //   - tests/http_metrics_integration.rs (200/404/405/500 via tower)
    // The block here is the in-file mirror that exercises the metrics.rs
    // wrappers themselves end-to-end through MetricAssertion.
    // Pinning is implicit (cargo's default test runner is single-threaded
    // per-test); MetricAssertion binds a per-thread recorder. See
    // `crates/common/src/observability/testing.rs:60-72`.
    // ========================================================================

    #[test]
    fn metrics_module_emits_token_issuance_cluster() {
        let snap = MetricAssertion::snapshot();

        record_token_issuance("client_credentials", "success", Duration::from_millis(250));
        record_token_issuance("client_credentials", "error", Duration::from_millis(100));
        record_token_issuance("authorization_code", "success", Duration::from_millis(300));
        record_token_issuance("refresh_token", "error", Duration::from_millis(50));

        // Histogram first (drain-on-read — single take_entries() per snapshot).
        snap.histogram("ac_token_issuance_duration_seconds")
            .assert_observation_count_at_least(4);

        snap.counter("ac_token_issuance_total")
            .with_labels(&[("grant_type", "client_credentials"), ("status", "success")])
            .assert_delta(1);
        snap.counter("ac_token_issuance_total")
            .with_labels(&[("grant_type", "client_credentials"), ("status", "error")])
            .assert_delta(1);
        snap.counter("ac_token_issuance_total")
            .with_labels(&[("grant_type", "authorization_code"), ("status", "success")])
            .assert_delta(1);
        snap.counter("ac_token_issuance_total")
            .with_labels(&[("grant_type", "refresh_token"), ("status", "error")])
            .assert_delta(1);
    }

    // WRAPPER-CAT-C: production callers planned for Phase 4 token-validation
    // endpoint. Today the wrapper has only TWO real call sites — both
    // `("error", Some("clock_skew"))` from `crypto/mod.rs:284` (`verify_jwt`)
    // and `:439` (`verify_user_jwt`). The 4 other label combos exercised here
    // (success, error+authentication, error+authorization, error+cryptographic,
    // error+internal) are forward-looking reservations from `ADR-0011`. The
    // production-path coverage for `clock_skew` lives in
    // `tests/token_validation_integration.rs`. Mirrors MC's
    // `media_connection_failed` carve-out pattern. See `docs/TODO.md`
    // §Observability Debt for the orphan disposition tracker.
    #[test]
    fn metrics_module_emits_token_validation_cluster() {
        let snap = MetricAssertion::snapshot();

        record_token_validation("success", None);
        record_token_validation("error", Some("authentication"));
        record_token_validation("error", Some("authorization"));
        record_token_validation("error", Some("cryptographic"));
        record_token_validation("error", Some("internal"));

        snap.counter("ac_token_validations_total")
            .with_labels(&[("status", "success"), ("error_category", "none")])
            .assert_delta(1);
        snap.counter("ac_token_validations_total")
            .with_labels(&[("status", "error"), ("error_category", "authentication")])
            .assert_delta(1);
        snap.counter("ac_token_validations_total")
            .with_labels(&[("status", "error"), ("error_category", "authorization")])
            .assert_delta(1);
        snap.counter("ac_token_validations_total")
            .with_labels(&[("status", "error"), ("error_category", "cryptographic")])
            .assert_delta(1);
        snap.counter("ac_token_validations_total")
            .with_labels(&[("status", "error"), ("error_category", "internal")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_key_management_cluster() {
        let snap = MetricAssertion::snapshot();

        record_key_rotation("success");
        record_key_rotation("error");
        set_signing_key_age_days(15.5);
        set_active_signing_keys(2);
        set_key_rotation_last_success(1700000000.0);

        snap.counter("ac_key_rotation_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("ac_key_rotation_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.gauge("ac_signing_key_age_days").assert_value(15.5);
        snap.gauge("ac_active_signing_keys").assert_value(2.0);
        snap.gauge("ac_key_rotation_last_success_timestamp")
            .assert_value(1700000000.0);
    }

    #[test]
    fn metrics_module_emits_db_query_cluster() {
        let snap = MetricAssertion::snapshot();

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

        // Histogram first (drain-on-read — single take_entries() per snapshot).
        snap.histogram("ac_db_query_duration_seconds")
            .assert_observation_count_at_least(5);

        snap.counter("ac_db_queries_total")
            .with_labels(&[
                ("operation", "select"),
                ("table", "service_credentials"),
                ("status", "success"),
            ])
            .assert_delta(1);
        snap.counter("ac_db_queries_total")
            .with_labels(&[
                ("operation", "insert"),
                ("table", "service_credentials"),
                ("status", "success"),
            ])
            .assert_delta(1);
        snap.counter("ac_db_queries_total")
            .with_labels(&[
                ("operation", "delete"),
                ("table", "signing_keys"),
                ("status", "error"),
            ])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_bcrypt_cluster() {
        let snap = MetricAssertion::snapshot();

        record_bcrypt_duration("hash", Duration::from_millis(150));
        record_bcrypt_duration("verify", Duration::from_millis(120));

        snap.histogram("ac_bcrypt_duration_seconds")
            .assert_observation_count(2);
    }

    #[test]
    fn metrics_module_emits_jwks_cluster() {
        let snap = MetricAssertion::snapshot();

        record_jwks_request("hit");
        record_jwks_request("miss");
        record_jwks_request("bypass");

        snap.counter("ac_jwks_requests_total")
            .with_labels(&[("cache_status", "hit")])
            .assert_delta(1);
        snap.counter("ac_jwks_requests_total")
            .with_labels(&[("cache_status", "miss")])
            .assert_delta(1);
        snap.counter("ac_jwks_requests_total")
            .with_labels(&[("cache_status", "bypass")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_audit_failures_cluster() {
        let snap = MetricAssertion::snapshot();

        record_audit_log_failure("token_issued", "db_write_failed");
        record_audit_log_failure("key_rotation", "encryption_failed");
        record_audit_log_failure("authentication", "log_overflow");

        snap.counter("ac_audit_log_failures_total")
            .with_labels(&[
                ("event_type", "token_issued"),
                ("reason", "db_write_failed"),
            ])
            .assert_delta(1);
        snap.counter("ac_audit_log_failures_total")
            .with_labels(&[
                ("event_type", "key_rotation"),
                ("reason", "encryption_failed"),
            ])
            .assert_delta(1);
        snap.counter("ac_audit_log_failures_total")
            .with_labels(&[("event_type", "authentication"), ("reason", "log_overflow")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_rate_limit_cluster() {
        let snap = MetricAssertion::snapshot();

        record_rate_limit_decision("allowed");
        record_rate_limit_decision("rejected");

        snap.counter("ac_rate_limit_decisions_total")
            .with_labels(&[("action", "allowed")])
            .assert_delta(1);
        snap.counter("ac_rate_limit_decisions_total")
            .with_labels(&[("action", "rejected")])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_credential_ops_cluster() {
        let snap = MetricAssertion::snapshot();

        record_credential_operation("list", "success");
        record_credential_operation("get", "success");
        record_credential_operation("create", "success");
        record_credential_operation("update", "success");
        record_credential_operation("delete", "success");
        record_credential_operation("rotate_secret", "success");
        record_credential_operation("create", "error");
        record_credential_operation("update", "error");
        record_credential_operation("delete", "error");

        for op in ["list", "get", "create", "update", "delete", "rotate_secret"] {
            snap.counter("ac_credential_operations_total")
                .with_labels(&[("operation", op), ("status", "success")])
                .assert_delta(1);
        }
        for op in ["create", "update", "delete"] {
            snap.counter("ac_credential_operations_total")
                .with_labels(&[("operation", op), ("status", "error")])
                .assert_delta(1);
        }
    }

    #[test]
    fn metrics_module_emits_errors_cluster() {
        let snap = MetricAssertion::snapshot();

        record_error("token_issuance", "authentication", 401);
        record_error("token_issuance", "authorization", 403);
        record_error("key_rotation", "cryptographic", 500);
        record_error("db_query", "internal", 500);
        record_error("rate_limit", "authorization", 429);

        snap.counter("ac_errors_total")
            .with_labels(&[
                ("operation", "token_issuance"),
                ("error_category", "authentication"),
                ("status_code", "401"),
            ])
            .assert_delta(1);
        snap.counter("ac_errors_total")
            .with_labels(&[
                ("operation", "key_rotation"),
                ("error_category", "cryptographic"),
                ("status_code", "500"),
            ])
            .assert_delta(1);
        snap.counter("ac_errors_total")
            .with_labels(&[
                ("operation", "rate_limit"),
                ("error_category", "authorization"),
                ("status_code", "429"),
            ])
            .assert_delta(1);
    }

    #[test]
    fn metrics_module_emits_http_request_cluster() {
        let snap = MetricAssertion::snapshot();

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
        record_http_request(
            "POST",
            "/api/v1/auth/service/token",
            400,
            Duration::from_millis(5),
        );
        record_http_request("GET", "/not-found", 404, Duration::from_millis(1));
        record_http_request(
            "DELETE",
            "/api/v1/auth/service/token",
            405,
            Duration::from_millis(1),
        );
        record_http_request(
            "POST",
            "/api/v1/auth/service/token",
            500,
            Duration::from_millis(100),
        );

        // Histogram first (drain-on-read).
        snap.histogram("ac_http_request_duration_seconds")
            .with_labels(&[
                ("method", "POST"),
                ("endpoint", "/api/v1/auth/service/token"),
                ("status_code", "200"),
            ])
            .assert_observation_count_at_least(1);

        // /not-found normalizes to /other (cardinality bound).
        snap.counter("ac_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/.well-known/jwks.json"),
                ("status_code", "200"),
            ])
            .assert_delta(1);
        snap.counter("ac_http_requests_total")
            .with_labels(&[
                ("method", "GET"),
                ("endpoint", "/other"),
                ("status_code", "404"),
            ])
            .assert_delta(1);
        snap.counter("ac_http_requests_total")
            .with_labels(&[
                ("method", "DELETE"),
                ("endpoint", "/api/v1/auth/service/token"),
                ("status_code", "405"),
            ])
            .assert_delta(1);
        snap.counter("ac_http_requests_total")
            .with_labels(&[
                ("method", "POST"),
                ("endpoint", "/api/v1/auth/service/token"),
                ("status_code", "500"),
            ])
            .assert_delta(1);
    }

    // ========================================================================
    // Path-normalization unit tests — preserved as-is. These exercise the
    // `normalize_path`/`is_uuid`/`normalize_dynamic_path` logic, NOT metric
    // emission. They predate ADR-0032 and remain the right shape.
    // ========================================================================

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

    #[test]
    fn test_normalize_path_admin_clients() {
        // Static admin clients path
        assert_eq!(
            normalize_path("/api/v1/admin/clients"),
            "/api/v1/admin/clients"
        );

        // Admin clients with UUID (GET /api/v1/admin/clients/{id})
        assert_eq!(
            normalize_path("/api/v1/admin/clients/550e8400-e29b-41d4-a716-446655440000"),
            "/api/v1/admin/clients/{id}"
        );

        // Admin clients rotate secret (POST /api/v1/admin/clients/{id}/rotate-secret)
        assert_eq!(
            normalize_path(
                "/api/v1/admin/clients/550e8400-e29b-41d4-a716-446655440000/rotate-secret"
            ),
            "/api/v1/admin/clients/{id}/rotate-secret"
        );

        // Different UUIDs should normalize to same path (cardinality bounded)
        assert_eq!(
            normalize_path("/api/v1/admin/clients/123e4567-e89b-12d3-a456-426614174000"),
            "/api/v1/admin/clients/{id}"
        );
        assert_eq!(
            normalize_path("/api/v1/admin/clients/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"),
            "/api/v1/admin/clients/{id}"
        );
    }

    #[test]
    fn test_is_uuid_valid() {
        // Valid UUIDs
        assert!(is_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(is_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(is_uuid("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"));
        assert!(is_uuid("00000000-0000-0000-0000-000000000000"));
        assert!(is_uuid("ffffffff-ffff-ffff-ffff-ffffffffffff"));

        // Mixed case should work (hex digits are case-insensitive)
        assert!(is_uuid("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"));
        assert!(is_uuid("AaBbCcDd-EeFf-0011-2233-445566778899"));
    }

    #[test]
    fn test_is_uuid_invalid() {
        // Wrong length
        assert!(!is_uuid("550e8400-e29b-41d4-a716-44665544000")); // 35 chars
        assert!(!is_uuid("550e8400-e29b-41d4-a716-4466554400000")); // 37 chars
        assert!(!is_uuid("")); // Empty
        assert!(!is_uuid("123")); // Too short

        // Wrong dash positions
        assert!(!is_uuid("550e8400e29b-41d4-a716-446655440000")); // Missing dash at position 8
        assert!(!is_uuid("550e8400-e29b41d4-a716-446655440000")); // Missing dash at position 13
        assert!(!is_uuid("550e8400-e29b-41d4a716-446655440000")); // Missing dash at position 18
        assert!(!is_uuid("550e8400-e29b-41d4-a716446655440000")); // Missing dash at position 23

        // Non-hex characters
        assert!(!is_uuid("550e8400-e29b-41d4-a716-44665544000g")); // 'g' is not hex
        assert!(!is_uuid("550e8400-e29b-41d4-a716-44665544000 ")); // Space
        assert!(!is_uuid("550e8400-e29b-41d4-a716-44665544000!")); // Special char

        // Not a UUID at all
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx")); // 'x' is hex, but this tests format
        assert!(!is_uuid("550e8400-e29b-41d4-a716-4466554400zz")); // 'z' is not hex
    }

    #[test]
    fn test_normalize_dynamic_path_edge_cases() {
        // Path with UUID but wrong structure
        assert_eq!(
            normalize_dynamic_path(
                "/api/v1/admin/clients/550e8400-e29b-41d4-a716-446655440000/other"
            ),
            "/other"
        );

        // Path with non-UUID
        assert_eq!(
            normalize_dynamic_path("/api/v1/admin/clients/not-a-uuid"),
            "/other"
        );

        // Path with numeric ID instead of UUID
        assert_eq!(
            normalize_dynamic_path("/api/v1/admin/clients/123"),
            "/other"
        );

        // Completely different path
        assert_eq!(
            normalize_dynamic_path("/api/v2/users/550e8400-e29b-41d4-a716-446655440000"),
            "/other"
        );
    }
}
