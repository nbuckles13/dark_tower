//! Global Controller configuration.
//!
//! Configuration is loaded from environment variables. All sensitive
//! fields are redacted in Debug output.

use crate::mh_endpoint_policy::MhEndpointAllowlist;
use axum::http::HeaderValue;
use common::jwt::MAX_CLOCK_SKEW;
use common::secret::SecretString;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::time::Duration;
use thiserror::Error;

/// Default GC instance ID prefix.
pub const DEFAULT_GC_ID_PREFIX: &str = "gc";

// =============================================================================
// MC liveness and assignment commit budget (ADR-0010 §3)
// =============================================================================

/// Upper bound on `GC_ASSIGN_COMMIT_BUDGET_MS`: half of MC's shortest join
/// window. The other half is margin for what the budget cannot police
/// (rationale at [`common::meeting_lifecycle::MIN_MEETING_JOIN_WINDOW`]).
pub const MAX_ASSIGN_COMMIT_BUDGET: Duration = Duration::from_millis(
    common::meeting_lifecycle::MIN_MEETING_JOIN_WINDOW.as_millis() as u64 / 2,
);

/// MC liveness and the assignment write budget. Both REQUIRED.
///
/// - `MC_STALENESS_THRESHOLD_SECONDS` (> the MC fast-heartbeat interval GC
///   pushes, 10 s): THE MC liveness threshold. The
///   health checker marks MCs stale with it, and every assignment-row
///   decision (join reuse, failover, cleanup backstop) asks "is this MC's
///   heartbeat younger than it?" — one value, so they cannot disagree.
/// - `GC_ASSIGN_COMMIT_BUDGET_MS` (1 ..= [`MAX_ASSIGN_COMMIT_BUDGET`]): how
///   long after GC SENDS `AssignMeetingWithMh` the assignment row may still
///   commit. Past it the row is not written and the next MC is tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McAssignmentConfig {
    mc_staleness: Duration,
    commit_budget: Duration,
}

impl McAssignmentConfig {
    /// Build from explicit values (tests, and [`Self::from_vars`]).
    ///
    /// # Errors
    ///
    /// `InvalidMcAssignmentConfig` / `InvalidMcStalenessThreshold` naming the
    /// out-of-range value.
    pub fn new(mc_staleness: Duration, commit_budget: Duration) -> Result<Self, ConfigError> {
        if mc_staleness < Duration::from_secs(1) {
            return Err(ConfigError::InvalidMcStalenessThreshold(
                "MC_STALENESS_THRESHOLD_SECONDS must be greater than 0".to_string(),
            ));
        }
        // A threshold at or below the heartbeat cadence GC pushes to MCs
        // would mark every live MC stale between beats, and liveness-keyed
        // failover would then split live meetings.
        let heartbeat =
            Duration::from_millis(crate::grpc::mc_service::DEFAULT_FAST_HEARTBEAT_INTERVAL_MS);
        if mc_staleness <= heartbeat {
            return Err(ConfigError::InvalidMcStalenessThreshold(format!(
                "MC_STALENESS_THRESHOLD_SECONDS must exceed the MC fast-heartbeat interval ({} s), got {}",
                heartbeat.as_secs(),
                mc_staleness.as_secs()
            )));
        }
        if commit_budget.is_zero() || commit_budget > MAX_ASSIGN_COMMIT_BUDGET {
            return Err(ConfigError::InvalidMcAssignmentConfig(format!(
                "GC_ASSIGN_COMMIT_BUDGET_MS must be in [1, {}], got {}",
                MAX_ASSIGN_COMMIT_BUDGET.as_millis(),
                commit_budget.as_millis()
            )));
        }
        Ok(Self {
            mc_staleness,
            commit_budget,
        })
    }

    /// Parse the two REQUIRED keys.
    ///
    /// # Errors
    ///
    /// `MissingEnvVar` for an absent key; the typed variants for a value that
    /// does not parse or is out of range.
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let staleness_raw = vars.get("MC_STALENESS_THRESHOLD_SECONDS").ok_or_else(|| {
            ConfigError::MissingEnvVar("MC_STALENESS_THRESHOLD_SECONDS".to_string())
        })?;
        let staleness_secs: u64 = parse_positive(
            "MC_STALENESS_THRESHOLD_SECONDS",
            staleness_raw,
            ConfigError::InvalidMcStalenessThreshold,
        )?;
        let budget_raw = vars
            .get("GC_ASSIGN_COMMIT_BUDGET_MS")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_ASSIGN_COMMIT_BUDGET_MS".to_string()))?;
        let budget_ms: u64 = parse_positive(
            "GC_ASSIGN_COMMIT_BUDGET_MS",
            budget_raw,
            ConfigError::InvalidMcAssignmentConfig,
        )?;
        Self::new(
            Duration::from_secs(staleness_secs),
            Duration::from_millis(budget_ms),
        )
    }

    /// The MC liveness threshold.
    #[must_use]
    pub fn mc_staleness(&self) -> Duration {
        self.mc_staleness
    }

    /// The accept->commit budget.
    #[must_use]
    pub fn commit_budget(&self) -> Duration {
        self.commit_budget
    }
}

// =============================================================================
// Assignment cleanup (ADR-0010 §3 item 3)
// =============================================================================

/// Assignment cleanup job settings. All three REQUIRED.
///
/// - `GC_CLEANUP_INTERVAL_SECONDS` (>= 1): check interval (0 would panic
///   `tokio::time::interval`).
/// - `GC_INACTIVITY_HOURS` (>= 1): age floor before the backstop may end a
///   row whose MC is no longer alive. Not an inactivity timer: GC has no
///   participant data, and the backstop's safety property is MC liveness.
/// - `GC_RETENTION_DAYS` (>= 1): retention before ended rows are hard-deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssignmentCleanupConfig {
    /// Cleanup check interval in seconds.
    pub check_interval_seconds: u64,
    /// Minimum assignment age (hours) before the backstop may end it.
    pub inactivity_hours: i32,
    /// Days to retain ended assignments before hard-deleting.
    pub retention_days: i32,
}

impl AssignmentCleanupConfig {
    /// Parse the three REQUIRED keys.
    ///
    /// # Errors
    ///
    /// `MissingEnvVar` for an absent key; `InvalidAssignmentCleanupConfig`
    /// naming the key for a value that does not parse or is below 1.
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let interval_raw = vars
            .get("GC_CLEANUP_INTERVAL_SECONDS")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_CLEANUP_INTERVAL_SECONDS".to_string()))?;
        let inactivity_raw = vars
            .get("GC_INACTIVITY_HOURS")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_INACTIVITY_HOURS".to_string()))?;
        let retention_raw = vars
            .get("GC_RETENTION_DAYS")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_RETENTION_DAYS".to_string()))?;
        Ok(Self {
            check_interval_seconds: parse_positive(
                "GC_CLEANUP_INTERVAL_SECONDS",
                interval_raw,
                ConfigError::InvalidAssignmentCleanupConfig,
            )?,
            inactivity_hours: parse_positive(
                "GC_INACTIVITY_HOURS",
                inactivity_raw,
                ConfigError::InvalidAssignmentCleanupConfig,
            )?,
            retention_days: parse_positive(
                "GC_RETENTION_DAYS",
                retention_raw,
                ConfigError::InvalidAssignmentCleanupConfig,
            )?,
        })
    }
}

// =============================================================================
// MH registry freshness and sizing (ADR-0039 §3, §10; R-23, R-24, R-26)
//
// All five keys are REQUIRED: `ConfigError::MissingEnvVar`, no Rust default.
// Their reference values live in `infra/services/gc-service/config.env`, the
// artifact an operator reads; a `DEFAULT_*` constant here would be a second
// copy of the same value. The `MissingEnvVar("...")` constructor and its key
// literal stay on ONE line at each site: `dt-guard env-config` discovers
// required keys by a single-line regex over this file.
// =============================================================================

/// The five REQUIRED MH registry keys. (Presence checks are spelled out per
/// key in `MhRegistryConfig::from_vars` for `dt-guard env-config`; this list
/// is for consumers that enumerate them, e.g. tests.)
pub const MH_REGISTRY_KEYS: [&str; 5] = [
    "GC_MH_LOAD_REPORT_INTERVAL_MS",
    "GC_MH_STALENESS_MULTIPLIER",
    "GC_MH_REGISTRY_RETENTION_SECONDS",
    "GC_MH_INITIAL_HANDLERS_PER_MEETING",
    "GC_MH_MAX_EXCLUDE_HANDLER_IDS",
];

// =============================================================================
// RegisterMH endpoint allowlists (ADR-0039 §7, arm 0 of the registration rule;
// typed parse in `crate::mh_endpoint_policy`)
//
// All five keys are REQUIRED, with no Rust default: missing = `MissingEnvVar`,
// malformed or EMPTY = `InvalidMhEndpointAllowlist` naming the key (an empty
// allowlist would reject every MH while GC reports healthy, so it fails at
// startup instead). The base `infra/services/gc-service/config.env` carries
// the four topology keys EMPTY; every deploy path renders them (Kind:
// `infra/kind/scripts/deploy.sh:render_env_overlay`, derived from the MH slot
// table and the kind podSubnet). `GC_MH_GRPC_ALLOWED_SCHEMES` is `grpc` in
// the base, which means CLEARTEXT h2c, UNAUTHENTICATED: no TLS and no peer
// authentication of the MH server, so MC's bearer crosses that channel in
// cleartext (S6). It flips by config only to a TLS scheme when S6 mTLS lands.
// =============================================================================

/// The five REQUIRED RegisterMH endpoint allowlist keys. (Presence checks are
/// spelled out per key in [`mh_endpoint_allowlist_from_vars`] for `dt-guard
/// env-config`; this list is for consumers that enumerate them, e.g. tests.)
pub const MH_ENDPOINT_ALLOWLIST_KEYS: [&str; 5] = [
    "GC_MH_WT_ALLOWED_HOSTS",
    "GC_MH_WT_ALLOWED_PORTS",
    "GC_MH_GRPC_ALLOWED_SCHEMES",
    "GC_MH_GRPC_ALLOWED_CIDRS",
    "GC_MH_GRPC_ALLOWED_PORTS",
];

/// The ONE format site for allowlist config errors: every message starts with
/// the key.
fn allowlist_err(key: &str, detail: String) -> ConfigError {
    ConfigError::InvalidMhEndpointAllowlist(format!("{key}: {detail}"))
}

/// Parse the five REQUIRED RegisterMH endpoint allowlist keys.
///
/// # Errors
///
/// `MissingEnvVar` naming an absent key; `InvalidMhEndpointAllowlist`
/// (message starts with the key, then the offending entry) for an empty or
/// malformed one.
pub fn mh_endpoint_allowlist_from_vars(
    vars: &HashMap<String, String>,
) -> Result<MhEndpointAllowlist, ConfigError> {
    use crate::mh_endpoint_policy as p;
    // One-line `MissingEnvVar(` + literal per key for `dt-guard env-config`.
    let wt_hosts = vars
        .get("GC_MH_WT_ALLOWED_HOSTS")
        .ok_or_else(|| ConfigError::MissingEnvVar("GC_MH_WT_ALLOWED_HOSTS".to_string()))?;
    let wt_ports = vars
        .get("GC_MH_WT_ALLOWED_PORTS")
        .ok_or_else(|| ConfigError::MissingEnvVar("GC_MH_WT_ALLOWED_PORTS".to_string()))?;
    let grpc_schemes = vars
        .get("GC_MH_GRPC_ALLOWED_SCHEMES")
        .ok_or_else(|| ConfigError::MissingEnvVar("GC_MH_GRPC_ALLOWED_SCHEMES".to_string()))?;
    let grpc_cidrs = vars
        .get("GC_MH_GRPC_ALLOWED_CIDRS")
        .ok_or_else(|| ConfigError::MissingEnvVar("GC_MH_GRPC_ALLOWED_CIDRS".to_string()))?;
    let grpc_ports = vars
        .get("GC_MH_GRPC_ALLOWED_PORTS")
        .ok_or_else(|| ConfigError::MissingEnvVar("GC_MH_GRPC_ALLOWED_PORTS".to_string()))?;
    Ok(MhEndpointAllowlist::new(
        p::parse_wt_hosts(wt_hosts).map_err(|d| allowlist_err("GC_MH_WT_ALLOWED_HOSTS", d))?,
        p::parse_port_set(wt_ports).map_err(|d| allowlist_err("GC_MH_WT_ALLOWED_PORTS", d))?,
        p::parse_scheme_set(grpc_schemes)
            .map_err(|d| allowlist_err("GC_MH_GRPC_ALLOWED_SCHEMES", d))?,
        p::parse_cidr_set(grpc_cidrs).map_err(|d| allowlist_err("GC_MH_GRPC_ALLOWED_CIDRS", d))?,
        p::parse_port_set(grpc_ports).map_err(|d| allowlist_err("GC_MH_GRPC_ALLOWED_PORTS", d))?,
    ))
}

/// Bounds of `GC_MH_LOAD_REPORT_INTERVAL_MS`, milliseconds.
const MIN_MH_LOAD_REPORT_INTERVAL_MS: u64 = 1_000;
const MAX_MH_LOAD_REPORT_INTERVAL_MS: u64 = 60_000;
/// Bounds of `GC_MH_STALENESS_MULTIPLIER` (k).
const MIN_MH_STALENESS_MULTIPLIER: u64 = 2;
const MAX_MH_STALENESS_MULTIPLIER: u64 = 10;
/// Upper bound of `GC_MH_REGISTRY_RETENTION_SECONDS` (30 days). The lower
/// bound is derived: retention must exceed the staleness bound. The cap exists
/// so an absurd value fails HERE, at startup, rather than overflowing the
/// millisecond bind or PostgreSQL's interval range on every health-check tick.
const MAX_MH_REGISTRY_RETENTION_SECONDS: u64 = 2_592_000;
/// Bounds of `GC_MH_INITIAL_HANDLERS_PER_MEETING`.
const MIN_MH_INITIAL_HANDLERS_PER_MEETING: u64 = 1;
pub(crate) const MAX_MH_INITIAL_HANDLERS_PER_MEETING: u64 = 4;
/// Bounds of `GC_MH_MAX_EXCLUDE_HANDLER_IDS` (the cap on
/// `AcquireMediaHandlerRequest.exclude_handler_ids`, pushed to MC in
/// `RegisterMCResponse.max_exclude_handler_ids`). It must also exceed
/// `GC_MH_INITIAL_HANDLERS_PER_MEETING` (cross-checked in `MhRegistryConfig::new`).
const MIN_MH_MAX_EXCLUDE_HANDLER_IDS: u64 = 4;
const MAX_MH_MAX_EXCLUDE_HANDLER_IDS: u64 = 64;

/// GC's media-handler registry timing and sizing (ADR-0039 §3, §10).
///
/// `staleness_bound` = k × `load_report_interval`, computed once here. It is
/// the ONLY MH staleness value in GC: selection, the stale sweep and the
/// `gc_mh_available` recompute all read it, and so does the published
/// `gc_mh_staleness_threshold_seconds` gauge. `load_report_interval` is the
/// value pushed to every MH in `RegisterMHResponse`. `max_exclude_handler_ids`
/// is BOTH the value pushed to every MC in `RegisterMCResponse` and the bound
/// `AcquireMediaHandler` enforces (one source for push and enforcement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MhRegistryConfig {
    load_report_interval_ms: u64,
    staleness_multiplier: u64,
    staleness_bound: Duration,
    retention: Duration,
    initial_handlers_per_meeting: usize,
    max_exclude_handler_ids: u32,
}

impl MhRegistryConfig {
    /// Validate and build. Every failure names its key.
    ///
    /// # Errors
    ///
    /// `ConfigError::InvalidMhRegistryConfig` when a value is out of range,
    /// retention does not exceed the derived staleness bound, or the exclude
    /// cap does not exceed the initial handler count.
    pub fn new(
        load_report_interval_ms: u64,
        staleness_multiplier: u64,
        retention_seconds: u64,
        initial_handlers_per_meeting: u64,
        max_exclude_handler_ids: u64,
    ) -> Result<Self, ConfigError> {
        in_range(
            "GC_MH_LOAD_REPORT_INTERVAL_MS",
            load_report_interval_ms,
            MIN_MH_LOAD_REPORT_INTERVAL_MS,
            MAX_MH_LOAD_REPORT_INTERVAL_MS,
        )?;
        in_range(
            "GC_MH_STALENESS_MULTIPLIER",
            staleness_multiplier,
            MIN_MH_STALENESS_MULTIPLIER,
            MAX_MH_STALENESS_MULTIPLIER,
        )?;
        in_range(
            "GC_MH_INITIAL_HANDLERS_PER_MEETING",
            initial_handlers_per_meeting,
            MIN_MH_INITIAL_HANDLERS_PER_MEETING,
            MAX_MH_INITIAL_HANDLERS_PER_MEETING,
        )?;
        in_range(
            "GC_MH_MAX_EXCLUDE_HANDLER_IDS",
            max_exclude_handler_ids,
            MIN_MH_MAX_EXCLUDE_HANDLER_IDS,
            MAX_MH_MAX_EXCLUDE_HANDLER_IDS,
        )?;
        // A meeting's exclude list holds at least its current set, so a cap
        // at or below the initial count would leave MC no replacement room.
        if max_exclude_handler_ids <= initial_handlers_per_meeting {
            return Err(ConfigError::InvalidMhRegistryConfig(format!(
                "GC_MH_MAX_EXCLUDE_HANDLER_IDS ({}) must exceed \
                 GC_MH_INITIAL_HANDLERS_PER_MEETING ({})",
                max_exclude_handler_ids, initial_handlers_per_meeting
            )));
        }
        let max_exclude_handler_ids = u32::try_from(max_exclude_handler_ids).map_err(|_| {
            ConfigError::InvalidMhRegistryConfig(
                "GC_MH_MAX_EXCLUDE_HANDLER_IDS does not fit u32".to_string(),
            )
        })?;
        if retention_seconds > MAX_MH_REGISTRY_RETENTION_SECONDS {
            return Err(ConfigError::InvalidMhRegistryConfig(format!(
                "GC_MH_REGISTRY_RETENTION_SECONDS must be at most {}, got {}",
                MAX_MH_REGISTRY_RETENTION_SECONDS, retention_seconds
            )));
        }

        let staleness_bound_ms = load_report_interval_ms
            .checked_mul(staleness_multiplier)
            .ok_or_else(|| {
                ConfigError::InvalidMhRegistryConfig(
                    "GC_MH_STALENESS_MULTIPLIER x GC_MH_LOAD_REPORT_INTERVAL_MS overflows"
                        .to_string(),
                )
            })?;
        let retention_ms = retention_seconds.checked_mul(1_000).ok_or_else(|| {
            ConfigError::InvalidMhRegistryConfig(
                "GC_MH_REGISTRY_RETENTION_SECONDS overflows in milliseconds".to_string(),
            )
        })?;
        if retention_ms <= staleness_bound_ms {
            return Err(ConfigError::InvalidMhRegistryConfig(format!(
                "GC_MH_REGISTRY_RETENTION_SECONDS ({} ms) must exceed the MH staleness bound \
                 GC_MH_STALENESS_MULTIPLIER x GC_MH_LOAD_REPORT_INTERVAL_MS ({} ms)",
                retention_ms, staleness_bound_ms
            )));
        }
        let initial_handlers_per_meeting =
            usize::try_from(initial_handlers_per_meeting).map_err(|_| {
                ConfigError::InvalidMhRegistryConfig(
                    "GC_MH_INITIAL_HANDLERS_PER_MEETING does not fit usize".to_string(),
                )
            })?;

        Ok(Self {
            load_report_interval_ms,
            staleness_multiplier,
            staleness_bound: Duration::from_millis(staleness_bound_ms),
            retention: Duration::from_millis(retention_ms),
            initial_handlers_per_meeting,
            max_exclude_handler_ids,
        })
    }

    /// Parse the five REQUIRED keys from `vars` and validate (the one parse
    /// path: `Config::from_vars` and gc-test-utils' reference config both
    /// call this).
    ///
    /// # Errors
    ///
    /// `MissingEnvVar` naming an absent key; `InvalidMhRegistryConfig`
    /// naming a malformed or out-of-range one.
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        // MH registry freshness (ADR-0039 §10). REQUIRED; one-line
        // `MissingEnvVar(` + literal per key for `dt-guard env-config`.
        let mh_load_report_interval_ms = parse_u64(
            "GC_MH_LOAD_REPORT_INTERVAL_MS",
            vars.get("GC_MH_LOAD_REPORT_INTERVAL_MS").ok_or_else(|| {
                ConfigError::MissingEnvVar("GC_MH_LOAD_REPORT_INTERVAL_MS".to_string())
            })?,
        )?;
        let mh_staleness_multiplier = parse_u64(
            "GC_MH_STALENESS_MULTIPLIER",
            vars.get("GC_MH_STALENESS_MULTIPLIER").ok_or_else(|| {
                ConfigError::MissingEnvVar("GC_MH_STALENESS_MULTIPLIER".to_string())
            })?,
        )?;
        let mh_registry_retention_seconds = parse_u64(
            "GC_MH_REGISTRY_RETENTION_SECONDS",
            vars.get("GC_MH_REGISTRY_RETENTION_SECONDS")
                .ok_or_else(|| {
                    ConfigError::MissingEnvVar("GC_MH_REGISTRY_RETENTION_SECONDS".to_string())
                })?,
        )?;
        let mh_initial_handlers_per_meeting = parse_u64(
            "GC_MH_INITIAL_HANDLERS_PER_MEETING",
            vars.get("GC_MH_INITIAL_HANDLERS_PER_MEETING")
                .ok_or_else(|| {
                    ConfigError::MissingEnvVar("GC_MH_INITIAL_HANDLERS_PER_MEETING".to_string())
                })?,
        )?;
        let mh_max_exclude_handler_ids = parse_u64(
            "GC_MH_MAX_EXCLUDE_HANDLER_IDS",
            vars.get("GC_MH_MAX_EXCLUDE_HANDLER_IDS").ok_or_else(|| {
                ConfigError::MissingEnvVar("GC_MH_MAX_EXCLUDE_HANDLER_IDS".to_string())
            })?,
        )?;
        Self::new(
            mh_load_report_interval_ms,
            mh_staleness_multiplier,
            mh_registry_retention_seconds,
            mh_initial_handlers_per_meeting,
            mh_max_exclude_handler_ids,
        )
    }

    /// Load-report interval pushed to MH at registration.
    pub fn load_report_interval(&self) -> Duration {
        Duration::from_millis(self.load_report_interval_ms)
    }

    /// The same interval in milliseconds, the unit of
    /// `RegisterMHResponse.load_report_interval_ms`.
    pub fn load_report_interval_ms(&self) -> u64 {
        self.load_report_interval_ms
    }

    /// The staleness multiplier k.
    pub fn staleness_multiplier(&self) -> u64 {
        self.staleness_multiplier
    }

    /// The one MH staleness bound (k × interval).
    pub fn staleness_bound(&self) -> Duration {
        self.staleness_bound
    }

    /// Registry retention: rows with no load report for this long are deleted.
    pub fn retention(&self) -> Duration {
        self.retention
    }

    /// Number of MHs selected for a new meeting (upper bound; fewer when fewer
    /// candidates exist).
    pub fn initial_handlers_per_meeting(&self) -> usize {
        self.initial_handlers_per_meeting
    }

    /// Cap on `AcquireMediaHandlerRequest.exclude_handler_ids` length: pushed
    /// to MC in `RegisterMCResponse.max_exclude_handler_ids` and enforced by
    /// `AcquireMediaHandler` (the same value at both sites).
    pub fn max_exclude_handler_ids(&self) -> u32 {
        self.max_exclude_handler_ids
    }
}

/// Range-check an already-parsed value, naming the key on failure.
fn in_range(key: &str, value: u64, min: u64, max: u64) -> Result<(), ConfigError> {
    if value < min || value > max {
        return Err(ConfigError::InvalidMhRegistryConfig(format!(
            "{} must be in [{}, {}], got {}",
            key, min, max, value
        )));
    }
    Ok(())
}

/// Parse an already-fetched REQUIRED value as `u64`, naming the key.
///
/// The presence check is deliberately at the call site (see the section note).
fn parse_u64(key: &str, raw: &str) -> Result<u64, ConfigError> {
    raw.trim().parse::<u64>().map_err(|e| {
        ConfigError::InvalidMhRegistryConfig(format!(
            "{} must be a non-negative integer, got '{}': {}",
            key, raw, e
        ))
    })
}

/// Global Controller configuration.
///
/// Loaded from environment variables with sensible defaults.
/// Database URL is redacted in Debug output to prevent credential leakage.
#[derive(Clone)]
pub struct Config {
    /// PostgreSQL connection URL.
    pub database_url: String,

    /// Server bind address (default: "0.0.0.0:8080").
    pub bind_address: String,

    /// Deployment region identifier (e.g., "us-east-1").
    pub region: String,

    /// URL to Auth Controller JWKS endpoint for token validation.
    pub ac_jwks_url: String,

    /// URL to Auth Controller internal API for token generation.
    pub ac_internal_url: String,

    /// JWT clock skew tolerance in seconds for token validation.
    pub jwt_clock_skew_seconds: i64,

    /// Rate limit in requests per minute per client.
    pub rate_limit_rpm: u32,

    /// gRPC server bind address.
    pub grpc_bind_address: String,

    /// MC liveness threshold and the accept->commit budget for assignment.
    pub mc_assignment: McAssignmentConfig,

    /// Assignment cleanup job (backstop + retention).
    pub assignment_cleanup: AssignmentCleanupConfig,

    /// Graceful-shutdown drain after SIGTERM, before exit (`GC_DRAIN_SECONDS`,
    /// [0, [`MAX_DRAIN`]]; 0 skips the drain).
    pub drain: Duration,

    /// Unique identifier for this GC instance.
    /// Used for assignment tracking and debugging.
    pub gc_id: String,

    /// OAuth client ID for GC to authenticate with AC.
    pub gc_client_id: String,

    /// OAuth client secret for GC to authenticate with AC (SecretString prevents logging).
    pub gc_client_secret: SecretString,

    /// In-cluster OTel collector endpoint for the telemetry proxy (R-2).
    ///
    /// **Bare base** (scheme + host + port, no path, no trailing slash). The
    /// forwarder appends `/v1/metrics` or `/v1/traces` per signal. An empty
    /// string disables the proxy (the route returns 503 rather than forwarding).
    pub otel_collector_endpoint: String,

    /// Maximum accepted telemetry proxy payload size in bytes.
    /// Enforced on real body bytes (oversize → 413).
    pub telemetry_proxy_max_bytes: usize,

    /// Per-user (JWT `sub`) telemetry proxy rate limit, requests per minute
    /// Exceeding it → 429.
    pub telemetry_proxy_rate_limit_per_minute: u32,

    /// Whether to initialize the OpenTelemetry SDK (R-55). Default `false`.
    /// When `true`, `main` calls `init_otel` (eager collector probe, fail-hard
    /// at init) and composes the tracing-opentelemetry layer; when `false`, no
    /// OTel layer is added and no collector probe occurs. Enablement is this
    /// explicit boolean, NOT presence of `otel_endpoint`.
    pub otel_enabled: bool,
    /// OTLP-gRPC collector endpoint (env `OTLP_ENDPOINT`). Only consumed when
    /// `otel_enabled` is `true`; must be a parseable `http(s)://host:port` URL.
    /// Distinct from `otel_collector_endpoint` (the `:4318`
    /// OTLP-HTTP endpoint the `/api/v1/telemetry` proxy forwards to) — this is
    /// the `:4317` OTLP-gRPC endpoint for GC's own span export.
    pub otel_endpoint: String,
    /// Head-sampling ratio in `[0.0, 1.0]` (env `OTEL_SAMPLE_RATE`). `init_otel`
    /// is the authoritative validator and re-checks this bound at startup.
    pub otel_sample_rate: f64,
    /// Deployment environment for the `deployment.environment` OTel resource
    /// attribute (env `DEPLOYMENT_ENVIRONMENT`, ADR-0011).
    pub environment: String,

    /// CORS allowlist for the browser SDK / Vite demo (R-1/R-3). Loaded from
    /// `CORS_ALLOWED_ORIGINS` (comma-separated) and parsed ONCE here into
    /// header values. Explicit origins only — the router's `CorsLayer` NEVER
    /// uses `*`. Unset or EMPTY is fail-closed: no origin is allowed, so no
    /// `Access-Control-Allow-Origin` is ever emitted. An entry that is not a
    /// valid header value fails startup (never silently dropped). Not secret —
    /// shown in Debug output.
    pub cors_allowed_origins: Vec<HeaderValue>,

    /// MH registry timing and sizing (ADR-0039 §10). Not secret.
    pub mh_registry: MhRegistryConfig,

    /// RegisterMH endpoint allowlists (ADR-0039 §7, arm 0). Not secret.
    pub mh_endpoint_allowlist: MhEndpointAllowlist,
}

/// Custom Debug implementation that redacts sensitive fields.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"[REDACTED]")
            .field("bind_address", &self.bind_address)
            .field("region", &self.region)
            .field("ac_jwks_url", &self.ac_jwks_url)
            .field("ac_internal_url", &self.ac_internal_url)
            .field("jwt_clock_skew_seconds", &self.jwt_clock_skew_seconds)
            .field("rate_limit_rpm", &self.rate_limit_rpm)
            .field("grpc_bind_address", &self.grpc_bind_address)
            .field("mc_assignment", &self.mc_assignment)
            .field("assignment_cleanup", &self.assignment_cleanup)
            .field("drain", &self.drain)
            .field("gc_id", &self.gc_id)
            .field("gc_client_id", &self.gc_client_id)
            .field("gc_client_secret", &"[REDACTED]")
            .field("otel_collector_endpoint", &self.otel_collector_endpoint)
            .field("telemetry_proxy_max_bytes", &self.telemetry_proxy_max_bytes)
            .field(
                "telemetry_proxy_rate_limit_per_minute",
                &self.telemetry_proxy_rate_limit_per_minute,
            )
            .field("otel_enabled", &self.otel_enabled)
            .field("otel_endpoint", &self.otel_endpoint)
            .field("otel_sample_rate", &self.otel_sample_rate)
            .field("environment", &self.environment)
            .field("cors_allowed_origins", &self.cors_allowed_origins)
            .field("mh_registry", &self.mh_registry)
            .field("mh_endpoint_allowlist", &self.mh_endpoint_allowlist)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Invalid JWT clock skew configuration: {0}")]
    InvalidJwtClockSkew(String),

    #[error("Invalid rate limit configuration: {0}")]
    InvalidRateLimit(String),

    #[error("Invalid MC staleness threshold configuration: {0}")]
    InvalidMcStalenessThreshold(String),

    #[error("Invalid telemetry proxy max bytes configuration: {0}")]
    InvalidTelemetryProxyMaxBytes(String),

    #[error("Invalid telemetry proxy rate limit configuration: {0}")]
    InvalidTelemetryProxyRateLimit(String),

    #[error("Invalid OTel configuration: {0}")]
    InvalidOtelConfig(String),

    #[error("Invalid MH registry configuration: {0}")]
    InvalidMhRegistryConfig(String),

    #[error("Invalid MH endpoint allowlist configuration: {0}")]
    InvalidMhEndpointAllowlist(String),

    #[error("Invalid assignment cleanup configuration: {0}")]
    InvalidAssignmentCleanupConfig(String),

    #[error("Invalid MC assignment configuration: {0}")]
    InvalidMcAssignmentConfig(String),

    #[error("Invalid CORS configuration: {0}")]
    InvalidCorsConfig(String),

    #[error("Invalid drain configuration: {0}")]
    InvalidDrainConfig(String),
}

/// Parse `CORS_ALLOWED_ORIGINS`: comma-separated, trimmed, empty segments
/// dropped; unset or empty = the fail-closed empty allowlist (a mode, not a
/// substituted value). An entry that does not parse as a header value is a
/// startup error naming it: dropping it would let a typo silently narrow
/// the allowlist.
fn cors_allowed_origins_from_vars(
    vars: &HashMap<String, String>,
) -> Result<Vec<HeaderValue>, ConfigError> {
    let Some(raw) = vars.get("CORS_ALLOWED_ORIGINS") else {
        return Ok(Vec::new());
    };
    raw.split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(|o| {
            o.parse::<HeaderValue>().map_err(|e| {
                ConfigError::InvalidCorsConfig(format!(
                    "CORS_ALLOWED_ORIGINS entry {o:?} is not a valid header value: {e}"
                ))
            })
        })
        .collect()
}

/// Upper bound on `GC_DRAIN_SECONDS`. The drain runs after SIGTERM and must
/// end before Kubernetes' SIGKILL at the Deployment's
/// `terminationGracePeriodSeconds` (35), leaving room to exit;
/// `gc-test-utils` asserts the shipped manifest keeps that margin.
pub const MAX_DRAIN: Duration = Duration::from_secs(30);

/// Parse the REQUIRED `GC_DRAIN_SECONDS` (0 = skip the drain). A value that
/// does not parse or exceeds [`MAX_DRAIN`] fails startup naming the key,
/// never silently becoming a default.
fn drain_from_vars(vars: &HashMap<String, String>) -> Result<Duration, ConfigError> {
    let raw = vars
        .get("GC_DRAIN_SECONDS")
        .ok_or_else(|| ConfigError::MissingEnvVar("GC_DRAIN_SECONDS".to_string()))?;
    let secs: u64 = raw.trim().parse().map_err(|e| {
        ConfigError::InvalidDrainConfig(format!(
            "GC_DRAIN_SECONDS must be an integer in [0, {}], got '{raw}': {e}",
            MAX_DRAIN.as_secs()
        ))
    })?;
    if secs > MAX_DRAIN.as_secs() {
        return Err(ConfigError::InvalidDrainConfig(format!(
            "GC_DRAIN_SECONDS must be at most {}, got {secs}",
            MAX_DRAIN.as_secs()
        )));
    }
    Ok(Duration::from_secs(secs))
}

/// Parse a REQUIRED integer that must be `> 0`; errors name the key.
fn parse_positive<T>(key: &str, raw: &str, err: fn(String) -> ConfigError) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + Default,
    T::Err: std::fmt::Display,
{
    let value: T = raw.trim().parse().map_err(|e| {
        err(format!(
            "{key} must be a valid positive integer, got '{raw}': {e}"
        ))
    })?;
    if value <= T::default() {
        return Err(err(format!("{key} must be greater than 0")));
    }
    Ok(value)
}

impl Config {
    /// Load configuration from environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(&env::vars().collect())
    }

    /// Load configuration from a HashMap (for testing).
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let database_url = vars
            .get("DATABASE_URL")
            .ok_or_else(|| ConfigError::MissingEnvVar("DATABASE_URL".to_string()))?
            .clone();

        // Every behaviour/topology key below is REQUIRED: missing =
        // `MissingEnvVar`, no Rust default, value in config.env, hard ref in
        // deployment.yaml. A default would silently substitute a value the
        // operator never chose (CLAUDE.md "fail loudly"). One-line
        // `MissingEnvVar(` + literal per key for `dt-guard env-config`.
        let bind_address = vars
            .get("BIND_ADDRESS")
            .ok_or_else(|| ConfigError::MissingEnvVar("BIND_ADDRESS".to_string()))?
            .clone();

        let region = vars
            .get("GC_REGION")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_REGION".to_string()))?
            .clone();

        let ac_jwks_url = vars
            .get("AC_JWKS_URL")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_JWKS_URL".to_string()))?
            .clone();

        let ac_internal_url = vars
            .get("AC_INTERNAL_URL")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_INTERNAL_URL".to_string()))?
            .clone();

        // JWT clock skew tolerance: (0, MAX_CLOCK_SKEW].
        let jwt_clock_skew_raw = vars
            .get("JWT_CLOCK_SKEW_SECONDS")
            .ok_or_else(|| ConfigError::MissingEnvVar("JWT_CLOCK_SKEW_SECONDS".to_string()))?;
        let jwt_clock_skew_seconds: i64 = jwt_clock_skew_raw.parse().map_err(|e| {
            ConfigError::InvalidJwtClockSkew(format!(
                "JWT_CLOCK_SKEW_SECONDS must be a valid integer, got '{jwt_clock_skew_raw}': {e}"
            ))
        })?;
        if jwt_clock_skew_seconds <= 0 {
            return Err(ConfigError::InvalidJwtClockSkew(format!(
                "JWT_CLOCK_SKEW_SECONDS must be positive, got {jwt_clock_skew_seconds}"
            )));
        }
        if jwt_clock_skew_seconds > MAX_CLOCK_SKEW.as_secs() as i64 {
            return Err(ConfigError::InvalidJwtClockSkew(format!(
                "JWT_CLOCK_SKEW_SECONDS must not exceed {} seconds, got {jwt_clock_skew_seconds}",
                MAX_CLOCK_SKEW.as_secs()
            )));
        }

        let rate_limit_raw = vars
            .get("RATE_LIMIT_RPM")
            .ok_or_else(|| ConfigError::MissingEnvVar("RATE_LIMIT_RPM".to_string()))?;
        let rate_limit_rpm: u32 = parse_positive(
            "RATE_LIMIT_RPM",
            rate_limit_raw,
            ConfigError::InvalidRateLimit,
        )?;

        let grpc_bind_address = vars
            .get("GC_GRPC_BIND_ADDRESS")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_GRPC_BIND_ADDRESS".to_string()))?
            .clone();

        let mc_assignment = McAssignmentConfig::from_vars(vars)?;
        let assignment_cleanup = AssignmentCleanupConfig::from_vars(vars)?;
        let drain = drain_from_vars(vars)?;

        // Generate GC instance ID (an identity derivation, not a substituted
        // value: unset means "derive a unique one").
        let gc_id = vars.get("GC_ID").cloned().unwrap_or_else(|| {
            // Generate a unique ID based on hostname and UUID suffix
            let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
            // Use first 8 chars of UUID for uniqueness
            let uuid_suffix = uuid::Uuid::new_v4().to_string();
            let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
            format!("{}-{}-{}", DEFAULT_GC_ID_PREFIX, hostname, short_suffix)
        });

        // Parse OAuth client credentials (required for TokenManager)
        let gc_client_id = vars
            .get("GC_CLIENT_ID")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_CLIENT_ID".to_string()))?
            .clone();

        let gc_client_secret = vars
            .get("GC_CLIENT_SECRET")
            .ok_or_else(|| ConfigError::MissingEnvVar("GC_CLIENT_SECRET".to_string()))?
            .clone();

        // Telemetry proxy: in-cluster OTel collector endpoint (R-2). Bare
        // base — the forwarder appends the per-signal suffix. REQUIRED; an
        // explicitly EMPTY value is the stated "proxy disabled" mode (the
        // route returns 503).
        let otel_collector_endpoint = vars
            .get("OTEL_COLLECTOR_ENDPOINT")
            .ok_or_else(|| ConfigError::MissingEnvVar("OTEL_COLLECTOR_ENDPOINT".to_string()))?
            .clone();

        let telemetry_max_bytes_raw = vars
            .get("TELEMETRY_PROXY_MAX_BYTES")
            .ok_or_else(|| ConfigError::MissingEnvVar("TELEMETRY_PROXY_MAX_BYTES".to_string()))?;
        let telemetry_proxy_max_bytes: usize = parse_positive(
            "TELEMETRY_PROXY_MAX_BYTES",
            telemetry_max_bytes_raw,
            ConfigError::InvalidTelemetryProxyMaxBytes,
        )?;

        // Must be > 0: governor's Quota is built from a NonZeroU32, and a
        // zero quota would deny all telemetry.
        let telemetry_rate_raw = vars
            .get("TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE")
            .ok_or_else(|| {
                ConfigError::MissingEnvVar("TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE".to_string())
            })?;
        let telemetry_proxy_rate_limit_per_minute: u32 = parse_positive(
            "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE",
            telemetry_rate_raw,
            ConfigError::InvalidTelemetryProxyRateLimit,
        )?;

        // ADR-0011 / R-55: OpenTelemetry SDK configuration (GC's own span
        // export — distinct from `otel_collector_endpoint` above, which is
        // the telemetry-proxy's OTLP-HTTP forward target). Enablement is an
        // explicit boolean (OTEL_ENABLED), NOT presence of the endpoint — so
        // a populated OTLP_ENDPOINT with OTEL_ENABLED unset/false stays OFF
        // (init_otel is never called and no layer is composed). OTEL_ENABLED
        // and OTLP_ENDPOINT are MODES (unset = export off), not substituted
        // values, so they stay optional (2026-11-15 Lead ruling).
        let otel_enabled = if let Some(value_str) = vars.get("OTEL_ENABLED") {
            match value_str.trim().to_ascii_lowercase().as_str() {
                "true" => true,
                "false" => false,
                _ => {
                    return Err(ConfigError::InvalidOtelConfig(format!(
                        "OTEL_ENABLED must be 'true' or 'false', got '{}'",
                        value_str
                    )))
                }
            }
        } else {
            false
        };

        // Endpoint reuses the `OTLP_ENDPOINT` key (mirrors AC / task #28 ruling).
        let otel_endpoint = vars.get("OTLP_ENDPOINT").cloned().unwrap_or_default();

        // Sample rate: REQUIRED, PARSE-ONLY. The `[0.0, 1.0]` bound has a
        // SINGLE owner — `init_otel` (common/observability/otel.rs) — which
        // re-validates it at startup on the enabled path. Non-numeric fails
        // fast at load.
        let otel_sample_rate_raw = vars
            .get("OTEL_SAMPLE_RATE")
            .ok_or_else(|| ConfigError::MissingEnvVar("OTEL_SAMPLE_RATE".to_string()))?;
        let otel_sample_rate = otel_sample_rate_raw.parse::<f64>().map_err(|e| {
            ConfigError::InvalidOtelConfig(format!(
                "OTEL_SAMPLE_RATE must be a number, got '{otel_sample_rate_raw}': {e}"
            ))
        })?;

        let environment = vars
            .get("DEPLOYMENT_ENVIRONMENT")
            .ok_or_else(|| ConfigError::MissingEnvVar("DEPLOYMENT_ENVIRONMENT".to_string()))?
            .clone();

        // CORS allowlist (R-1/R-3): comma-separated origins, trimmed, empty
        // segments dropped. Unset or empty = fail-closed (never allows any
        // origin, never `*`) — a mode, not a substituted value. An entry that
        // is not a valid header value FAILS STARTUP: dropping it would let an
        // operator's typo silently narrow the allowlist.
        let cors_allowed_origins = cors_allowed_origins_from_vars(vars)?;

        let mh_registry = MhRegistryConfig::from_vars(vars)?;
        let mh_endpoint_allowlist = mh_endpoint_allowlist_from_vars(vars)?;

        // When OTel is enabled the endpoint is required: fail fast with a clear
        // message rather than letting init_otel reject an empty/unparseable URL.
        if otel_enabled && otel_endpoint.trim().is_empty() {
            return Err(ConfigError::InvalidOtelConfig(
                "OTEL_ENABLED=true requires a non-empty OTLP_ENDPOINT".to_string(),
            ));
        }

        Ok(Config {
            database_url,
            bind_address,
            region,
            ac_jwks_url,
            ac_internal_url,
            jwt_clock_skew_seconds,
            rate_limit_rpm,
            grpc_bind_address,
            mc_assignment,
            assignment_cleanup,
            drain,
            gc_id,
            gc_client_id,
            gc_client_secret: SecretString::from(gc_client_secret),
            otel_collector_endpoint,
            telemetry_proxy_max_bytes,
            telemetry_proxy_rate_limit_per_minute,
            otel_enabled,
            otel_endpoint,
            otel_sample_rate,
            environment,
            cors_allowed_origins,
            mh_registry,
            mh_endpoint_allowlist,
        })
    }

    /// Build the [`common::observability::otel::OtelConfig`] for `init_otel`,
    /// or `None` when OTel is disabled. Encodes the R-55 gating: `main` calls
    /// `init_otel` (and composes the layer) iff this returns `Some` — so the
    /// default-off path performs no collector probe and adds no OTel layer.
    pub fn otel_config(&self) -> Option<common::observability::otel::OtelConfig> {
        if self.otel_enabled {
            Some(common::observability::otel::OtelConfig {
                endpoint: self.otel_endpoint.clone(),
                sample_rate: self.otel_sample_rate,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use common::secret::ExposeSecret;

    fn base_vars() -> HashMap<String, String> {
        HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/gc_test".to_string(),
            ),
            ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
            ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
            // Test-local values for every REQUIRED behaviour/topology key
            // (the shipped values are config.env's).
            ("BIND_ADDRESS".to_string(), "0.0.0.0:8080".to_string()),
            (
                "GC_GRPC_BIND_ADDRESS".to_string(),
                "0.0.0.0:50051".to_string(),
            ),
            ("GC_REGION".to_string(), "us-east-1".to_string()),
            (
                "AC_JWKS_URL".to_string(),
                "http://localhost:8082/.well-known/jwks.json".to_string(),
            ),
            (
                "AC_INTERNAL_URL".to_string(),
                "http://localhost:8082".to_string(),
            ),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "300".to_string()),
            ("RATE_LIMIT_RPM".to_string(), "100".to_string()),
            (
                "MC_STALENESS_THRESHOLD_SECONDS".to_string(),
                "30".to_string(),
            ),
            ("GC_ASSIGN_COMMIT_BUDGET_MS".to_string(), "2000".to_string()),
            (
                "GC_CLEANUP_INTERVAL_SECONDS".to_string(),
                "3600".to_string(),
            ),
            ("GC_INACTIVITY_HOURS".to_string(), "1".to_string()),
            ("GC_RETENTION_DAYS".to_string(), "7".to_string()),
            ("GC_DRAIN_SECONDS".to_string(), "30".to_string()),
            (
                "OTEL_COLLECTOR_ENDPOINT".to_string(),
                "http://otel-collector.test:4318".to_string(),
            ),
            (
                "TELEMETRY_PROXY_MAX_BYTES".to_string(),
                "262144".to_string(),
            ),
            (
                "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE".to_string(),
                "60".to_string(),
            ),
            ("OTEL_SAMPLE_RATE".to_string(), "1.0".to_string()),
            ("DEPLOYMENT_ENVIRONMENT".to_string(), "test".to_string()),
            // Test-local MH registry values (validation is what these tests
            // exercise; the shipped values are config.env's).
            (
                "GC_MH_LOAD_REPORT_INTERVAL_MS".to_string(),
                "2000".to_string(),
            ),
            ("GC_MH_STALENESS_MULTIPLIER".to_string(), "4".to_string()),
            (
                "GC_MH_REGISTRY_RETENTION_SECONDS".to_string(),
                "600".to_string(),
            ),
            (
                "GC_MH_INITIAL_HANDLERS_PER_MEETING".to_string(),
                "3".to_string(),
            ),
            (
                "GC_MH_MAX_EXCLUDE_HANDLER_IDS".to_string(),
                "20".to_string(),
            ),
            // Test-local RegisterMH allowlist values (these tests exercise the
            // parse; arm behaviour is tested in mh_endpoint_policy.rs).
            (
                "GC_MH_WT_ALLOWED_HOSTS".to_string(),
                "127.0.0.1,.mh.test".to_string(),
            ),
            (
                "GC_MH_WT_ALLOWED_PORTS".to_string(),
                "4440,4441".to_string(),
            ),
            ("GC_MH_GRPC_ALLOWED_SCHEMES".to_string(), "grpc".to_string()),
            (
                "GC_MH_GRPC_ALLOWED_CIDRS".to_string(),
                "10.244.0.0/16".to_string(),
            ),
            ("GC_MH_GRPC_ALLOWED_PORTS".to_string(), "50053".to_string()),
        ])
    }

    #[test]
    fn test_mh_endpoint_allowlist_each_key_missing_fails_naming_key() {
        assert!(Config::from_vars(&base_vars()).is_ok(), "positive control");
        for key in MH_ENDPOINT_ALLOWLIST_KEYS {
            let mut vars = base_vars();
            vars.remove(key);
            let err = Config::from_vars(&vars).unwrap_err();
            assert!(
                matches!(&err, ConfigError::MissingEnvVar(k) if k == key),
                "{key}: expected MissingEnvVar naming the key, got {err:?}"
            );
        }
    }

    /// Reverse direction of the missing-key test: a map holding ONLY the
    /// listed keys parses, so every key the reader requires is in
    /// `MH_ENDPOINT_ALLOWLIST_KEYS` (which env-tests mirror and drift-check).
    #[test]
    fn test_mh_endpoint_allowlist_keys_list_is_complete() {
        let all = base_vars();
        let only_listed: HashMap<String, String> = MH_ENDPOINT_ALLOWLIST_KEYS
            .iter()
            .map(|k| ((*k).to_string(), all[*k].clone()))
            .collect();
        assert!(mh_endpoint_allowlist_from_vars(&only_listed).is_ok());
    }

    /// Malformed or empty values fail, naming the key first; each key's
    /// valid value is in `base_vars` (the positive arm).
    #[test]
    fn test_mh_endpoint_allowlist_malformed_table() {
        let cases: &[(&str, &str)] = &[
            ("GC_MH_WT_ALLOWED_HOSTS", ""),
            ("GC_MH_WT_ALLOWED_HOSTS", "127.0.0.1,"),
            ("GC_MH_WT_ALLOWED_HOSTS", "*"),
            ("GC_MH_WT_ALLOWED_HOSTS", "."),
            ("GC_MH_WT_ALLOWED_HOSTS", ".1"),
            ("GC_MH_WT_ALLOWED_HOSTS", "https://mh.test"),
            ("GC_MH_WT_ALLOWED_PORTS", ""),
            ("GC_MH_WT_ALLOWED_PORTS", "0"),
            ("GC_MH_WT_ALLOWED_PORTS", "70000"),
            ("GC_MH_GRPC_ALLOWED_SCHEMES", ""),
            ("GC_MH_GRPC_ALLOWED_SCHEMES", "*"),
            ("GC_MH_GRPC_ALLOWED_SCHEMES", "grpc,"),
            ("GC_MH_GRPC_ALLOWED_CIDRS", ""),
            ("GC_MH_GRPC_ALLOWED_CIDRS", "0.0.0.0/0"),
            ("GC_MH_GRPC_ALLOWED_CIDRS", "::/0"),
            ("GC_MH_GRPC_ALLOWED_CIDRS", "10.244.1.0/16"),
            ("GC_MH_GRPC_ALLOWED_CIDRS", "10.244.0.0"),
            ("GC_MH_GRPC_ALLOWED_PORTS", ""),
            ("GC_MH_GRPC_ALLOWED_PORTS", "-1"),
        ];
        for (key, value) in cases {
            let mut vars = base_vars();
            vars.insert((*key).to_string(), (*value).to_string());
            let err = Config::from_vars(&vars).unwrap_err();
            assert!(
                matches!(&err, ConfigError::InvalidMhEndpointAllowlist(msg)
                    if msg.starts_with(&format!("{key}: "))),
                "{key}={value:?}: expected InvalidMhEndpointAllowlist starting with the key, got {err:?}"
            );
        }
    }

    #[test]
    fn test_mh_registry_values_and_derived_bound() {
        let config = Config::from_vars(&base_vars()).unwrap();
        let mh = config.mh_registry;
        assert_eq!(mh.load_report_interval(), Duration::from_millis(2000));
        assert_eq!(mh.staleness_multiplier(), 4);
        assert_eq!(mh.staleness_bound(), Duration::from_millis(8000));
        assert_eq!(mh.retention(), Duration::from_secs(600));
        assert_eq!(mh.initial_handlers_per_meeting(), 3);
        assert_eq!(mh.max_exclude_handler_ids(), 20);
    }

    #[test]
    fn test_mh_registry_each_key_missing_fails_naming_key() {
        for key in MH_REGISTRY_KEYS {
            let mut vars = base_vars();
            vars.remove(key);
            let err = Config::from_vars(&vars).expect_err("missing key must fail");
            assert!(
                matches!(&err, ConfigError::MissingEnvVar(k) if k == key),
                "{key} missing: expected MissingEnvVar naming it, got {err:?}"
            );
        }
    }

    /// (key, value, accepted). Retention is held at its max so the interval
    /// and multiplier bounds are exercised independently of retention > bound.
    #[test]
    fn test_mh_registry_range_table() {
        let cases: &[(&str, &str, bool)] = &[
            ("GC_MH_LOAD_REPORT_INTERVAL_MS", "999", false),
            ("GC_MH_LOAD_REPORT_INTERVAL_MS", "1000", true),
            ("GC_MH_LOAD_REPORT_INTERVAL_MS", "60000", true),
            ("GC_MH_LOAD_REPORT_INTERVAL_MS", "60001", false),
            ("GC_MH_LOAD_REPORT_INTERVAL_MS", "ten", false),
            ("GC_MH_LOAD_REPORT_INTERVAL_MS", "-1", false),
            ("GC_MH_STALENESS_MULTIPLIER", "1", false),
            ("GC_MH_STALENESS_MULTIPLIER", "2", true),
            ("GC_MH_STALENESS_MULTIPLIER", "10", true),
            ("GC_MH_STALENESS_MULTIPLIER", "11", false),
            ("GC_MH_STALENESS_MULTIPLIER", "x", false),
            ("GC_MH_INITIAL_HANDLERS_PER_MEETING", "0", false),
            ("GC_MH_INITIAL_HANDLERS_PER_MEETING", "1", true),
            ("GC_MH_INITIAL_HANDLERS_PER_MEETING", "4", true),
            ("GC_MH_INITIAL_HANDLERS_PER_MEETING", "5", false),
            ("GC_MH_INITIAL_HANDLERS_PER_MEETING", "", false),
            ("GC_MH_MAX_EXCLUDE_HANDLER_IDS", "3", false),
            ("GC_MH_MAX_EXCLUDE_HANDLER_IDS", "4", true),
            ("GC_MH_MAX_EXCLUDE_HANDLER_IDS", "64", true),
            ("GC_MH_MAX_EXCLUDE_HANDLER_IDS", "65", false),
            ("GC_MH_MAX_EXCLUDE_HANDLER_IDS", "x", false),
            ("GC_MH_MAX_EXCLUDE_HANDLER_IDS", "", false),
            ("GC_MH_REGISTRY_RETENTION_SECONDS", "2592000", true),
            ("GC_MH_REGISTRY_RETENTION_SECONDS", "2592001", false),
            (
                "GC_MH_REGISTRY_RETENTION_SECONDS",
                "18446744073709551615",
                false,
            ),
            ("GC_MH_REGISTRY_RETENTION_SECONDS", "1h", false),
        ];
        for (key, value, accepted) in cases {
            let mut vars = base_vars();
            vars.insert(
                "GC_MH_REGISTRY_RETENTION_SECONDS".to_string(),
                "2592000".to_string(),
            );
            vars.insert((*key).to_string(), (*value).to_string());
            let result = Config::from_vars(&vars);
            if *accepted {
                assert!(
                    result.is_ok(),
                    "{key}={value} should be accepted: {result:?}"
                );
            } else {
                let err = result.expect_err("out-of-range value must fail");
                assert!(
                    matches!(&err, ConfigError::InvalidMhRegistryConfig(msg) if msg.contains(key)),
                    "{key}={value}: expected InvalidMhRegistryConfig naming the key, got {err:?}"
                );
            }
        }
    }

    /// The exclude cap must strictly exceed the initial handler count; the
    /// error names both keys. initial=4 keeps both values inside [4, 64].
    #[test]
    fn test_mh_max_exclude_must_exceed_initial_handlers() {
        for (initial, cap, accepted) in [("4", "4", false), ("4", "5", true), ("3", "4", true)] {
            let mut vars = base_vars();
            vars.insert(
                "GC_MH_INITIAL_HANDLERS_PER_MEETING".to_string(),
                initial.to_string(),
            );
            vars.insert("GC_MH_MAX_EXCLUDE_HANDLER_IDS".to_string(), cap.to_string());
            let result = Config::from_vars(&vars);
            if accepted {
                assert!(result.is_ok(), "initial={initial} cap={cap}: {result:?}");
            } else {
                let err = result.expect_err("cap <= initial must fail");
                assert!(
                    matches!(&err, ConfigError::InvalidMhRegistryConfig(msg)
                        if msg.contains("GC_MH_MAX_EXCLUDE_HANDLER_IDS")
                            && msg.contains("GC_MH_INITIAL_HANDLERS_PER_MEETING")),
                    "initial={initial} cap={cap}: expected an error naming both keys, got {err:?}"
                );
            }
        }
    }

    /// Retention must strictly exceed k × interval, compared in milliseconds.
    #[test]
    fn test_mh_registry_retention_must_exceed_bound() {
        let mut vars = base_vars();
        vars.insert(
            "GC_MH_LOAD_REPORT_INTERVAL_MS".to_string(),
            "1000".to_string(),
        );
        vars.insert("GC_MH_STALENESS_MULTIPLIER".to_string(), "2".to_string());
        // Bound = 2000 ms. Retention 2 s == bound → refused.
        vars.insert(
            "GC_MH_REGISTRY_RETENTION_SECONDS".to_string(),
            "2".to_string(),
        );
        let err = Config::from_vars(&vars).expect_err("retention == bound must fail");
        assert!(
            matches!(&err, ConfigError::InvalidMhRegistryConfig(msg)
                if msg.contains("GC_MH_REGISTRY_RETENTION_SECONDS")),
            "retention == bound: got {err:?}"
        );
        vars.insert(
            "GC_MH_REGISTRY_RETENTION_SECONDS".to_string(),
            "1".to_string(),
        );
        let err = Config::from_vars(&vars).expect_err("retention < bound must fail");
        assert!(
            matches!(&err, ConfigError::InvalidMhRegistryConfig(msg)
                if msg.contains("GC_MH_REGISTRY_RETENTION_SECONDS")),
            "retention < bound: got {err:?}"
        );
        // 3 s > 2000 ms → accepted (boundary positive arm).
        vars.insert(
            "GC_MH_REGISTRY_RETENTION_SECONDS".to_string(),
            "3".to_string(),
        );
        let config = Config::from_vars(&vars).expect("retention > bound accepted");
        assert_eq!(
            config.mh_registry.staleness_bound(),
            Duration::from_millis(2000)
        );
        assert_eq!(config.mh_registry.retention(), Duration::from_secs(3));
    }

    /// Every behaviour/topology key that may not be defaulted. Missing any
    /// one fails startup with `MissingEnvVar` naming it.
    const REQUIRED_KEYS: &[&str] = &[
        "DATABASE_URL",
        "GC_CLIENT_ID",
        "GC_CLIENT_SECRET",
        "BIND_ADDRESS",
        "GC_GRPC_BIND_ADDRESS",
        "GC_REGION",
        "AC_JWKS_URL",
        "AC_INTERNAL_URL",
        "JWT_CLOCK_SKEW_SECONDS",
        "RATE_LIMIT_RPM",
        "MC_STALENESS_THRESHOLD_SECONDS",
        "GC_ASSIGN_COMMIT_BUDGET_MS",
        "GC_CLEANUP_INTERVAL_SECONDS",
        "GC_INACTIVITY_HOURS",
        "GC_RETENTION_DAYS",
        "GC_DRAIN_SECONDS",
        "OTEL_COLLECTOR_ENDPOINT",
        "TELEMETRY_PROXY_MAX_BYTES",
        "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE",
        "OTEL_SAMPLE_RATE",
        "DEPLOYMENT_ENVIRONMENT",
    ];

    #[test]
    fn test_each_required_key_missing_fails_naming_key() {
        // Positive control: the full set loads.
        Config::from_vars(&base_vars()).expect("base vars must load");
        for key in REQUIRED_KEYS {
            let mut vars = base_vars();
            assert!(vars.remove(*key).is_some(), "{key} must be in base_vars");
            let err = Config::from_vars(&vars).expect_err("missing required key must fail");
            assert!(
                matches!(&err, ConfigError::MissingEnvVar(k) if k == key),
                "{key} missing: expected MissingEnvVar naming it, got {err:?}"
            );
        }
    }

    #[test]
    fn test_from_vars_loads_given_values() {
        let config = Config::from_vars(&base_vars()).expect("Config should load successfully");

        assert_eq!(config.database_url, "postgresql://localhost/gc_test");
        assert_eq!(config.bind_address, "0.0.0.0:8080");
        assert_eq!(config.region, "us-east-1");
        assert_eq!(
            config.ac_jwks_url,
            "http://localhost:8082/.well-known/jwks.json"
        );
        assert_eq!(config.ac_internal_url, "http://localhost:8082");
        assert_eq!(config.jwt_clock_skew_seconds, 300);
        assert_eq!(config.rate_limit_rpm, 100);
        assert_eq!(config.grpc_bind_address, "0.0.0.0:50051");
        assert_eq!(config.mc_assignment.mc_staleness(), Duration::from_secs(30));
        assert_eq!(
            config.mc_assignment.commit_budget(),
            Duration::from_millis(2000)
        );
        assert_eq!(
            config.assignment_cleanup,
            AssignmentCleanupConfig {
                check_interval_seconds: 3600,
                inactivity_hours: 1,
                retention_days: 7,
            }
        );
        // GC ID should be auto-generated
        assert!(config.gc_id.starts_with("gc-"));
        // OAuth client credentials should be loaded
        assert_eq!(config.gc_client_id, "test-gc-client");
        assert_eq!(config.gc_client_secret.expose_secret(), "test-gc-secret");
        assert_eq!(
            config.otel_collector_endpoint,
            "http://otel-collector.test:4318"
        );
        assert_eq!(config.telemetry_proxy_max_bytes, 262144);
        assert_eq!(config.telemetry_proxy_rate_limit_per_minute, 60);
        assert_eq!(config.environment, "test");
    }

    /// The commit budget must leave half of MC's shortest join window as
    /// margin: (0, MIN_MEETING_JOIN_WINDOW / 2].
    #[test]
    fn test_assign_commit_budget_bounds() {
        let max_ms = MAX_ASSIGN_COMMIT_BUDGET.as_millis();
        assert_eq!(
            MAX_ASSIGN_COMMIT_BUDGET * 2,
            common::meeting_lifecycle::MIN_MEETING_JOIN_WINDOW
        );
        for (value, accepted) in [
            ("1".to_string(), true),
            (max_ms.to_string(), true),
            ((max_ms + 1).to_string(), false),
            ("0".to_string(), false),
            ("soon".to_string(), false),
        ] {
            let mut vars = base_vars();
            vars.insert("GC_ASSIGN_COMMIT_BUDGET_MS".to_string(), value.clone());
            let result = Config::from_vars(&vars);
            if accepted {
                assert!(result.is_ok(), "{value} must be accepted: {result:?}");
            } else {
                assert!(
                    matches!(&result, Err(ConfigError::InvalidMcAssignmentConfig(msg))
                        if msg.contains("GC_ASSIGN_COMMIT_BUDGET_MS")),
                    "{value} must be refused naming the key: {result:?}"
                );
            }
        }
    }

    #[test]
    fn test_drain_bounds() {
        for (value, want) in [
            ("0", Some(Duration::ZERO)),
            ("30", Some(MAX_DRAIN)),
            ("31", None),
            ("-1", None),
            ("", None),
            ("soon", None),
        ] {
            let mut vars = base_vars();
            vars.insert("GC_DRAIN_SECONDS".to_string(), value.to_string());
            let result = Config::from_vars(&vars);
            match want {
                Some(d) => assert_eq!(
                    result.as_ref().map(|c| c.drain).ok(),
                    Some(d),
                    "{value:?}: {result:?}"
                ),
                None => assert!(
                    matches!(&result, Err(ConfigError::InvalidDrainConfig(msg))
                        if msg.contains("GC_DRAIN_SECONDS")),
                    "{value:?} must be refused naming the key: {result:?}"
                ),
            }
        }
    }

    #[test]
    fn test_assignment_cleanup_rejects_below_one_and_unparseable() {
        for key in [
            "GC_CLEANUP_INTERVAL_SECONDS",
            "GC_INACTIVITY_HOURS",
            "GC_RETENTION_DAYS",
        ] {
            for bad in ["0", "-1", "", "x"] {
                let mut vars = base_vars();
                vars.insert(key.to_string(), bad.to_string());
                let err = Config::from_vars(&vars).expect_err("invalid value must be refused");
                assert!(
                    matches!(&err, ConfigError::InvalidAssignmentCleanupConfig(msg) if msg.contains(key)),
                    "{key}={bad:?}: expected InvalidAssignmentCleanupConfig naming it, got {err:?}"
                );
            }
        }
    }

    #[test]
    fn test_telemetry_proxy_custom_values() {
        let mut vars = base_vars();
        vars.insert(
            "OTEL_COLLECTOR_ENDPOINT".to_string(),
            "http://collector.internal:4318".to_string(),
        );
        vars.insert(
            "TELEMETRY_PROXY_MAX_BYTES".to_string(),
            "524288".to_string(),
        );
        vars.insert(
            "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE".to_string(),
            "120".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.otel_collector_endpoint,
            "http://collector.internal:4318"
        );
        assert_eq!(config.telemetry_proxy_max_bytes, 524288);
        assert_eq!(config.telemetry_proxy_rate_limit_per_minute, 120);
    }

    #[test]
    fn test_telemetry_proxy_empty_endpoint_allowed() {
        // Empty endpoint disables the proxy (route returns 503); it must NOT
        // make the key required or fail config load.
        let mut vars = base_vars();
        vars.insert("OTEL_COLLECTOR_ENDPOINT".to_string(), String::new());

        let config = Config::from_vars(&vars).expect("Empty endpoint should be allowed");
        assert_eq!(config.otel_collector_endpoint, "");
    }

    #[test]
    fn test_telemetry_proxy_max_bytes_rejects_zero() {
        let mut vars = base_vars();
        vars.insert("TELEMETRY_PROXY_MAX_BYTES".to_string(), "0".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidTelemetryProxyMaxBytes(msg)) if msg.contains("must be greater than 0"))
        );
    }

    #[test]
    fn test_telemetry_proxy_max_bytes_rejects_non_numeric() {
        let mut vars = base_vars();
        vars.insert("TELEMETRY_PROXY_MAX_BYTES".to_string(), "lots".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidTelemetryProxyMaxBytes(msg)) if msg.contains("must be a valid positive integer"))
        );
    }

    #[test]
    fn test_telemetry_proxy_rate_limit_rejects_zero() {
        // A zero quota would break governor's NonZeroU32 and deny all telemetry.
        let mut vars = base_vars();
        vars.insert(
            "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE".to_string(),
            "0".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidTelemetryProxyRateLimit(msg)) if msg.contains("must be greater than 0"))
        );
    }

    #[test]
    fn test_telemetry_proxy_rate_limit_rejects_non_numeric() {
        let mut vars = base_vars();
        vars.insert(
            "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE".to_string(),
            "sixty".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidTelemetryProxyRateLimit(msg)) if msg.contains("must be a valid positive integer"))
        );
    }

    #[test]
    fn test_from_vars_success_with_custom_values() {
        let mut vars = base_vars();
        vars.insert("BIND_ADDRESS".to_string(), "127.0.0.1:9000".to_string());
        vars.insert("GC_REGION".to_string(), "eu-west-1".to_string());
        vars.insert(
            "AC_JWKS_URL".to_string(),
            "https://auth.example.com/.well-known/jwks.json".to_string(),
        );
        vars.insert(
            "AC_INTERNAL_URL".to_string(),
            "https://auth.internal.example.com".to_string(),
        );
        vars.insert("JWT_CLOCK_SKEW_SECONDS".to_string(), "120".to_string());
        vars.insert("RATE_LIMIT_RPM".to_string(), "500".to_string());
        vars.insert(
            "GC_GRPC_BIND_ADDRESS".to_string(),
            "127.0.0.1:50052".to_string(),
        );
        vars.insert(
            "MC_STALENESS_THRESHOLD_SECONDS".to_string(),
            "60".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.bind_address, "127.0.0.1:9000");
        assert_eq!(config.region, "eu-west-1");
        assert_eq!(
            config.ac_jwks_url,
            "https://auth.example.com/.well-known/jwks.json"
        );
        assert_eq!(config.ac_internal_url, "https://auth.internal.example.com");
        assert_eq!(config.jwt_clock_skew_seconds, 120);
        assert_eq!(config.rate_limit_rpm, 500);
        assert_eq!(config.grpc_bind_address, "127.0.0.1:50052");
        assert_eq!(config.mc_assignment.mc_staleness(), Duration::from_secs(60));
    }

    #[test]
    fn test_gc_id_custom_value() {
        let mut vars = base_vars();
        vars.insert("GC_ID".to_string(), "gc-custom-001".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.gc_id, "gc-custom-001");
    }

    #[test]
    fn test_from_vars_missing_database_url() {
        let vars = HashMap::new();

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "DATABASE_URL"));
    }

    #[test]
    fn test_from_vars_missing_gc_client_id() {
        let mut vars = base_vars();
        vars.remove("GC_CLIENT_ID");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "GC_CLIENT_ID"));
    }

    #[test]
    fn test_from_vars_missing_gc_client_secret() {
        let mut vars = base_vars();
        vars.remove("GC_CLIENT_SECRET");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "GC_CLIENT_SECRET"));
    }

    #[test]
    fn test_jwt_clock_skew_rejects_zero() {
        let mut vars = base_vars();
        vars.insert("JWT_CLOCK_SKEW_SECONDS".to_string(), "0".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be positive"))
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_negative() {
        let mut vars = base_vars();
        vars.insert("JWT_CLOCK_SKEW_SECONDS".to_string(), "-100".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be positive"))
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_too_large() {
        let mut vars = base_vars();
        vars.insert("JWT_CLOCK_SKEW_SECONDS".to_string(), "601".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must not exceed 600"))
        );
    }

    #[test]
    fn test_jwt_clock_skew_accepts_max() {
        let mut vars = base_vars();
        vars.insert("JWT_CLOCK_SKEW_SECONDS".to_string(), "600".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.jwt_clock_skew_seconds, 600);
    }

    #[test]
    fn test_jwt_clock_skew_rejects_non_numeric() {
        let mut vars = base_vars();
        vars.insert(
            "JWT_CLOCK_SKEW_SECONDS".to_string(),
            "five-minutes".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be a valid integer"))
        );
    }

    #[test]
    fn test_rate_limit_rejects_zero() {
        let mut vars = base_vars();
        vars.insert("RATE_LIMIT_RPM".to_string(), "0".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimit(msg)) if msg.contains("must be greater than 0"))
        );
    }

    #[test]
    fn test_rate_limit_rejects_negative() {
        let mut vars = base_vars();
        vars.insert("RATE_LIMIT_RPM".to_string(), "-10".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimit(msg)) if msg.contains("must be a valid positive integer"))
        );
    }

    #[test]
    fn test_rate_limit_rejects_non_numeric() {
        let mut vars = base_vars();
        vars.insert("RATE_LIMIT_RPM".to_string(), "hundred".to_string());

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimit(msg)) if msg.contains("must be a valid positive integer"))
        );
    }

    #[test]
    fn test_mc_staleness_threshold_rejects_zero() {
        let mut vars = base_vars();
        vars.insert(
            "MC_STALENESS_THRESHOLD_SECONDS".to_string(),
            "0".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidMcStalenessThreshold(msg)) if msg.contains("must be greater than 0"))
        );
    }

    #[test]
    fn test_mc_staleness_threshold_rejects_non_numeric() {
        let mut vars = base_vars();
        vars.insert(
            "MC_STALENESS_THRESHOLD_SECONDS".to_string(),
            "thirty".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidMcStalenessThreshold(msg)) if msg.contains("must be a valid positive integer"))
        );
    }

    #[test]
    fn test_debug_redacts_database_url() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        let debug_output = format!("{:?}", config);

        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("postgresql://"));
        assert!(!debug_output.contains("gc_test"));
    }

    #[test]
    fn test_debug_redacts_gc_client_secret() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        let debug_output = format!("{:?}", config);

        // gc_client_id should be visible, gc_client_secret should be redacted
        assert!(debug_output.contains("test-gc-client"));
        assert!(!debug_output.contains("test-gc-secret"));
        // Should have two [REDACTED] entries (database_url and gc_client_secret)
        let redacted_count = debug_output.matches("[REDACTED]").count();
        assert_eq!(redacted_count, 2);
    }

    // ============================================================================
    // OpenTelemetry Configuration Tests (R-55) — mirrors AC's task #25 block.
    // ============================================================================

    /// Helper: the minimal valid var set (base_vars) plus any extra OTel vars
    /// supplied by the caller.
    fn otel_vars(extra: &[(&str, &str)]) -> HashMap<String, String> {
        let mut vars = base_vars();
        for (k, v) in extra {
            vars.insert((*k).to_string(), (*v).to_string());
        }
        vars
    }

    #[test]
    fn test_otel_mode_keys_unset_mean_off() {
        // OTEL_ENABLED / OTLP_ENDPOINT are MODES: unset = export off.
        let config = Config::from_vars(&otel_vars(&[])).expect("Config should load");
        assert!(!config.otel_enabled, "OTel off when OTEL_ENABLED is unset");
        assert_eq!(config.otel_endpoint, "");
        assert_eq!(config.otel_sample_rate, 1.0);
        assert_eq!(config.environment, "test");
        // Disabled → no OtelConfig produced.
        assert!(config.otel_config().is_none());
    }

    #[test]
    fn test_otel_enabled_true_false() {
        let enabled = Config::from_vars(&otel_vars(&[
            ("OTEL_ENABLED", "true"),
            ("OTLP_ENDPOINT", "http://collector:4317"),
        ]))
        .expect("Config should load");
        assert!(enabled.otel_enabled);

        let disabled = Config::from_vars(&otel_vars(&[("OTEL_ENABLED", "false")]))
            .expect("Config should load");
        assert!(!disabled.otel_enabled);
    }

    #[test]
    fn test_otel_enabled_case_insensitive() {
        let config = Config::from_vars(&otel_vars(&[
            ("OTEL_ENABLED", "TRUE"),
            ("OTLP_ENDPOINT", "http://collector:4317"),
        ]))
        .expect("Config should load");
        assert!(config.otel_enabled);
    }

    #[test]
    fn test_otel_enabled_invalid_value_rejected() {
        // Non-bool value is an explicit ConfigError, NOT a silent false.
        let result = Config::from_vars(&otel_vars(&[("OTEL_ENABLED", "yes")]));
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("OTEL_ENABLED") && msg.contains("yes")),
            "invalid OTEL_ENABLED should be a ConfigError carrying the offending value"
        );
    }

    #[test]
    fn test_otel_endpoint_maps_from_otlp_endpoint() {
        let config = Config::from_vars(&otel_vars(&[(
            "OTLP_ENDPOINT",
            "http://otel-collector.dark-tower:4317",
        )]))
        .expect("Config should load");
        assert_eq!(
            config.otel_endpoint,
            "http://otel-collector.dark-tower:4317"
        );
    }

    #[test]
    fn test_otel_sample_rate_valid_values_stored() {
        // Parse-only: valid numeric values are parsed and stored verbatim.
        for v in ["0.0", "0.5", "1.0"] {
            let config = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", v)]))
                .expect("valid sample rate should parse");
            assert_eq!(
                config.otel_sample_rate,
                v.parse::<f64>().expect("test literal")
            );
        }
    }

    #[test]
    fn test_otel_sample_rate_out_of_range_accepted_at_config_load() {
        // Parse-only contract: config load does NOT range-check; the [0.0, 1.0]
        // bound has a single owner, init_otel, which re-validates at startup on
        // the enabled path. Out-of-range values PARSE here and are stored
        // inert; they hard-fail only at init_otel when otel_enabled=true. This
        // test locks the intentional deferral (a re-added config-load range
        // check would break it and signal a duplicated bound).
        let low = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", "-0.1")]))
            .expect("-0.1 parses at config load (range deferred to init_otel)");
        assert_eq!(low.otel_sample_rate, -0.1);

        let high = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", "1.1")]))
            .expect("1.1 parses at config load (range deferred to init_otel)");
        assert_eq!(high.otel_sample_rate, 1.1);
    }

    #[test]
    fn test_otel_sample_rate_non_numeric_rejected() {
        let result = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", "half")]));
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("OTEL_SAMPLE_RATE") && msg.contains("half")),
            "non-numeric sample rate should be a ConfigError carrying the offending value"
        );
    }

    #[test]
    fn test_deployment_environment_custom() {
        let config = Config::from_vars(&otel_vars(&[("DEPLOYMENT_ENVIRONMENT", "staging")]))
            .expect("Config should load");
        assert_eq!(config.environment, "staging");
    }

    #[test]
    fn test_otel_enabled_requires_endpoint() {
        // Enabled with no endpoint → fail fast.
        let result = Config::from_vars(&otel_vars(&[("OTEL_ENABLED", "true")]));
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("requires a non-empty OTLP_ENDPOINT")),
            "enabled + empty endpoint should be rejected"
        );
    }

    #[test]
    fn test_otel_config_some_when_enabled() {
        let config = Config::from_vars(&otel_vars(&[
            ("OTEL_ENABLED", "true"),
            ("OTLP_ENDPOINT", "http://collector:4317"),
            ("OTEL_SAMPLE_RATE", "0.5"),
        ]))
        .expect("Config should load");
        let otel = config.otel_config().expect("enabled → Some(OtelConfig)");
        assert_eq!(otel.endpoint, "http://collector:4317");
        assert_eq!(otel.sample_rate, 0.5);
    }

    #[test]
    fn test_otel_config_gating_precedence_endpoint_set_but_disabled() {
        // Regression guard: endpoint populated but OTEL_ENABLED unset →
        // otel_config() is None (NOT presence-gated).
        let config = Config::from_vars(&otel_vars(&[("OTLP_ENDPOINT", "http://collector:4317")]))
            .expect("Config should load");
        assert_eq!(config.otel_endpoint, "http://collector:4317");
        assert!(!config.otel_enabled);
        assert!(
            config.otel_config().is_none(),
            "endpoint present but disabled must NOT enable OTel (no presence-gating)"
        );
    }

    // ============================================================================
    // CORS Configuration Tests (R-1 / R-3)
    // ============================================================================

    #[test]
    fn test_cors_allowed_origins_unset_is_fail_closed() {
        // No CORS_ALLOWED_ORIGINS → empty allowlist (fail-closed mode).
        let mut vars = base_vars();
        vars.remove("CORS_ALLOWED_ORIGINS");
        let config = Config::from_vars(&vars).expect("Config should load");
        assert!(
            config.cors_allowed_origins.is_empty(),
            "unset CORS allowlist must be empty (fail-closed)"
        );
    }

    #[test]
    fn test_cors_allowed_origins_invalid_entry_fails_startup() {
        // A typo must not silently narrow the allowlist: the whole load fails,
        // naming the entry.
        let mut vars = base_vars();
        // DEL (0x7f) is not a valid header-value byte.
        vars.insert(
            "CORS_ALLOWED_ORIGINS".to_string(),
            "http://localhost:5173,http://bad\u{7f}origin".to_string(),
        );
        let err = Config::from_vars(&vars).expect_err("invalid origin must fail startup");
        assert!(
            matches!(&err, ConfigError::InvalidCorsConfig(msg) if msg.contains("CORS_ALLOWED_ORIGINS")),
            "expected InvalidCorsConfig naming the key, got {err:?}"
        );
    }

    #[test]
    fn test_cors_allowed_origins_single() {
        let mut vars = base_vars();
        vars.insert(
            "CORS_ALLOWED_ORIGINS".to_string(),
            "http://localhost:5173".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load");
        assert_eq!(config.cors_allowed_origins, vec!["http://localhost:5173"]);
    }

    #[test]
    fn test_cors_allowed_origins_multi_comma_split() {
        let mut vars = base_vars();
        vars.insert(
            "CORS_ALLOWED_ORIGINS".to_string(),
            "http://localhost:5173,https://app.example.com".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load");
        assert_eq!(
            config.cors_allowed_origins,
            vec!["http://localhost:5173", "https://app.example.com"]
        );
    }

    #[test]
    fn test_cors_allowed_origins_trims_whitespace() {
        let mut vars = base_vars();
        vars.insert(
            "CORS_ALLOWED_ORIGINS".to_string(),
            "  http://localhost:5173 , https://app.example.com  ".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load");
        assert_eq!(
            config.cors_allowed_origins,
            vec!["http://localhost:5173", "https://app.example.com"]
        );
    }

    #[test]
    fn test_cors_allowed_origins_drops_empty_segments() {
        // Leading/trailing/adjacent commas and an all-whitespace segment must
        // be dropped, not turned into empty-string origins.
        let mut vars = base_vars();
        vars.insert(
            "CORS_ALLOWED_ORIGINS".to_string(),
            ",http://localhost:5173,, ,https://app.example.com,".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load");
        assert_eq!(
            config.cors_allowed_origins,
            vec!["http://localhost:5173", "https://app.example.com"]
        );
    }

    #[test]
    fn test_cors_allowed_origins_empty_string_is_fail_closed() {
        // CORS_ALLOWED_ORIGINS="" (the prod base ConfigMap value) → empty vec.
        let mut vars = base_vars();
        vars.insert("CORS_ALLOWED_ORIGINS".to_string(), String::new());

        let config = Config::from_vars(&vars).expect("Config should load");
        assert!(
            config.cors_allowed_origins.is_empty(),
            "empty CORS_ALLOWED_ORIGINS must be fail-closed (no origins)"
        );
    }
}
