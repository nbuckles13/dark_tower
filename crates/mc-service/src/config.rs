//! Meeting Controller configuration.
//!
//! Configuration is loaded from environment variables. All sensitive
//! fields are redacted in Debug output.
//!
//! ## OAuth Configuration (ADR-0010)
//!
//! MC uses OAuth 2.0 client credentials for authenticating to GC via AC.
//! Required environment variables:
//! - `AC_ENDPOINT`: Authentication Controller endpoint (e.g., `https://ac.example.com`)
//! - `MC_CLIENT_ID`: OAuth client ID for MC
//! - `MC_CLIENT_SECRET`: OAuth client secret for MC

use common::secret::SecretString;
use std::collections::HashMap;
use std::env;
use std::fmt;
use thiserror::Error;

/// Default WebTransport bind address.
pub const DEFAULT_WEBTRANSPORT_BIND_ADDRESS: &str = "0.0.0.0:4433";

/// Default gRPC bind address for GC communication.
pub const DEFAULT_GRPC_BIND_ADDRESS: &str = "0.0.0.0:50052";

/// Default health endpoint bind address.
pub const DEFAULT_HEALTH_BIND_ADDRESS: &str = "0.0.0.0:8081";

/// Default binding token TTL in seconds (ADR-0023).
pub const DEFAULT_BINDING_TOKEN_TTL_SECONDS: u64 = 30;

/// Default clock skew allowance in seconds (ADR-0023).
pub const DEFAULT_CLOCK_SKEW_SECONDS: u64 = 5;

/// Default nonce grace window in seconds (ADR-0023).
pub const DEFAULT_NONCE_GRACE_WINDOW_SECONDS: u64 = 5;

/// Default participant disconnect grace period in seconds (ADR-0023).
pub const DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS: u64 = 30;

/// Default QUIC max-idle-timeout in seconds for the WebTransport server.
///
/// This is the ONLY detector for a crash / network-loss departure (a clean tab
/// close is caught immediately by `Connection::closed()`; see
/// `webtransport::connection`). It is deliberately set to a few seconds:
/// - fast enough that an abrupt departure is detected promptly, and
/// - paired with a keep-alive interval (`< idle`) so a healthy-but-quiet
///   connection is kept alive and is NEVER spuriously idle-timed-out.
///
/// The effective timeout is `min(this, peer_idle_timeout)`, so this value
/// governs versus the browser's larger default. It also bounds the documented
/// worst-case crash roster-remove latency (`idle_timeout + grace_period +
/// grace_check_interval`).
pub const DEFAULT_QUIC_MAX_IDLE_TIMEOUT_SECONDS: u64 = 10;

/// Default MC instance ID prefix.
pub const DEFAULT_MC_ID_PREFIX: &str = "mc";

/// Default OTLP sample rate (R-55). 1.0 in dev; tune down for prod.
pub const DEFAULT_OTEL_SAMPLE_RATE: f64 = 1.0;

/// Default deployment environment (R-55, OTel resource attribute).
pub const DEFAULT_ENVIRONMENT: &str = "development";

// ============================================================================
// Bounds on the ADR-0036 client-signalling knobs
//
// These are BOUNDS, not defaults. There are deliberately no `DEFAULT_*`
// constants for the five `MC_*` media-signalling keys — they are required, and
// their documented values live in `infra/services/mc-service/configmap.yaml`.
// ============================================================================

/// Smallest legal `MC_MAX_RECEIVE_SLOTS`.
///
/// Zero would mean no client can ever receive media — a silent total outage
/// that reads as a valid configuration.
const MIN_RECEIVE_SLOTS: usize = 1;

/// Largest legal `MC_MAX_RECEIVE_SLOTS`.
///
/// ADR-0036 §6 leans on this cap for *"resource-amplification-by-request
/// becomes structurally impossible rather than rate-limited"*. A cap set to
/// 100000 quietly retires that property while still looking like a cap, so the
/// cap's own value is bounded. 64 is generous against the two media kinds §11
/// ships and a realistic grid of ≤5 video plus audio.
const MAX_RECEIVE_SLOTS: usize = 64;

/// Smallest legal `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`.
const MIN_RECEIVE_CAPABILITY_DECLARATIONS: u32 = 1;

/// Largest legal `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`.
///
/// Same argument as the slot cap: a budget whose own value is unbounded is not
/// a budget.
const MAX_RECEIVE_CAPABILITY_DECLARATIONS: u32 = 4096;

/// Bound an already-fetched REQUIRED env value as a `usize`, on both sides.
///
/// Fail-loud on every arm — non-numeric or out of range stops the process at
/// load. A media-path knob that silently reverts to a library or code default is
/// the failure mode ADR-0036 §11's "derive rather than guard" rule and R-32 both
/// exist to prevent.
///
/// # The presence check is deliberately NOT in here
///
/// The caller constructs `ConfigError::MissingEnvVar` with a spelled-out string
/// literal inline and passes the value in. That is not an oversight:
/// `dt-guard env-config` discovers required variables by regex-matching that
/// constructor and its literal in this file, so moving the presence check behind
/// a helper would blind its per-workload check on every key routed through it
/// while it kept printing STATUS=OK. Same shape, and the same reason, as
/// `mh-service`'s five ADR-0036 §1 transport keys.
///
/// Two ways to break the discovery, both silent, both found by running the
/// guard's own regex over this file rather than by reading it:
/// - **wrapping** the constructor and its literal onto separate lines, since the
///   pattern does not span lines; and
/// - **writing an example of the pattern in a comment**, which registers a
///   phantom required variable that no ConfigMap declares.
///
/// Neither is caught by `cargo check`, and the first is what an automatic
/// reformat does.
/// # Generic over the integer type, so a knob is bounded in its OWN domain
///
/// `T` is inferred from the bound constants at the call site, so a `u32`-valued
/// knob passes `u32` bounds and gets a `u32` back. This is what removes the
/// casts in each direction: routing a `u32` knob through a `usize`-only helper
/// forced `MIN as usize` / `MAX as usize` going in and a `u32::try_from(..)`
/// with an unreachable "does not fit u32" arm coming out — four lossless casts
/// and three error messages an operator could never see. The generic keeps that
/// property with one body and one pair of messages, where two monomorphic
/// siblings were character-for-character identical modulo the type.
fn bounded<T>(name: &str, raw: &str, min: T, max: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display,
    T::Err: std::fmt::Display,
{
    let parsed = raw.trim().parse::<T>().map_err(|e| {
        ConfigError::InvalidValue(format!(
            "{name} must be a non-negative integer, got '{raw}': {e}"
        ))
    })?;
    if parsed < min || parsed > max {
        return Err(ConfigError::InvalidValue(format!(
            "{name} must be in {min}..={max}, got {parsed}"
        )));
    }
    Ok(parsed)
}

/// Meeting Controller configuration.
///
/// Loaded from environment variables with sensible defaults.
/// Sensitive fields are redacted in Debug output.
#[derive(Clone)]
#[allow(dead_code)] // Fields used in Phase 6b+
pub struct Config {
    /// Redis connection URL (for session state).
    /// Protected by `SecretString` to prevent accidental logging.
    pub redis_url: SecretString,

    /// WebTransport server bind address (default: "0.0.0.0:4433").
    pub webtransport_bind_address: String,

    /// gRPC server bind address for GC communication (default: "0.0.0.0:50052").
    pub grpc_bind_address: String,

    /// Health endpoint bind address (default: "0.0.0.0:8081").
    pub health_bind_address: String,

    /// Deployment region identifier (e.g., "us-east-1").
    pub region: String,

    /// URL to Global Controller for registration.
    pub gc_grpc_url: String,

    /// Unique identifier for this MC instance.
    pub mc_id: String,

    /// Maximum concurrent meetings this MC can handle.
    pub max_meetings: u32,

    /// Maximum total participants across all meetings.
    pub max_participants: u32,

    /// Binding token TTL in seconds (default: 30, per ADR-0023).
    pub binding_token_ttl_seconds: u64,

    /// Clock skew allowance in seconds (default: 5, per ADR-0023).
    pub clock_skew_seconds: u64,

    /// Nonce grace window in seconds (default: 5, per ADR-0023).
    pub nonce_grace_window_seconds: u64,

    /// Participant disconnect grace period in seconds (default: 30, per ADR-0023).
    pub disconnect_grace_period_seconds: u64,

    /// QUIC max-idle-timeout in seconds for the WebTransport server (default: 10).
    ///
    /// Bounds detection of a crash/network-loss departure. Parsed FAIL-LOUD
    /// (non-numeric or `0` is rejected at load) — an invalid value must not
    /// silently fall back to a library default, which would re-introduce the
    /// unbounded-detection bug. Env var: `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS`.
    pub quic_max_idle_timeout_seconds: u64,

    /// Master secret for binding token HMAC (base64-encoded).
    /// Rotates on each deployment for defense-in-depth.
    /// Protected by `SecretString` to prevent accidental logging.
    pub binding_token_secret: SecretString,

    /// Authentication Controller endpoint for OAuth token acquisition.
    /// Must use HTTPS in production (enforced by TokenManager::new_secure).
    pub ac_endpoint: String,

    /// OAuth client ID for MC (used for client credentials flow to AC).
    pub client_id: String,

    /// OAuth client secret for MC (used for client credentials flow to AC).
    /// Protected by `SecretString` to prevent accidental logging.
    pub client_secret: SecretString,

    /// URL to Auth Controller's JWKS endpoint for meeting token validation.
    /// Required environment variable: `AC_JWKS_URL`.
    pub ac_jwks_url: String,

    /// Path to TLS certificate file (PEM) for WebTransport server.
    /// Required environment variable: `MC_TLS_CERT_PATH`.
    pub tls_cert_path: String,

    /// Path to TLS private key file (PEM) for WebTransport server.
    /// Required environment variable: `MC_TLS_KEY_PATH`.
    pub tls_key_path: String,

    /// Advertised gRPC address for GC registration.
    /// This is the address GC uses to reach this MC pod (e.g., `http://10.244.0.5:50052`).
    /// Required environment variable: `MC_GRPC_ADVERTISE_ADDRESS`.
    pub grpc_advertise_address: String,

    /// Advertised WebTransport address for GC registration.
    /// This is the address GC uses to reach this MC pod (e.g., `https://10.244.0.5:4433`).
    /// Required environment variable: `MC_WEBTRANSPORT_ADVERTISE_ADDRESS`.
    pub webtransport_advertise_address: String,

    /// Whether OpenTelemetry SDK export is enabled (R-55).
    /// Explicit boolean gating (env `OTEL_ENABLED`, default `false`) — NOT
    /// presence-of-endpoint. When `true`, `otel_endpoint` must be non-empty.
    pub otel_enabled: bool,

    /// OTLP-gRPC collector endpoint (R-55, env `OTLP_ENDPOINT`, default `""`).
    /// Consumed by `init_otel` only when `otel_enabled` is `true`.
    pub otel_endpoint: String,

    /// Trace sample rate in `[0.0, 1.0]` (R-55, env `OTEL_SAMPLE_RATE`, default
    /// `1.0`). Parsed here; range validation is `init_otel`'s responsibility.
    pub otel_sample_rate: f64,

    /// Deployment environment resource attribute (R-55, env
    /// `DEPLOYMENT_ENVIRONMENT`, default `"development"`).
    pub environment: String,

    // ========================================================================
    // Client-facing media signalling (ADR-0036 §5, §6)
    //
    // All five are REQUIRED — `ConfigError::MissingEnvVar`, no Rust default.
    // A Rust-defaulted key wired into `mc-0` and forgotten on `mc-1` passes
    // `dt-guard env-config` green (its `orphan_configmap_key` check fires only
    // when NO workload references a key; per-workload enforcement exists only
    // for keys it discovers from the spelled-out constructor literal), and the two
    // MC instances would then direct DIFFERENT encoder parameters to their
    // respective clients with nothing failing anywhere. Making them required IS
    // the drift guard. Same shape and reasoning as mh-service's five ADR-0036 §1
    // transport keys.
    //
    // The `MissingEnvVar("...")` literals below are deliberately written out at
    // each site rather than extracted into a helper: the guard discovers
    // required vars by matching that literal, so a helper would blind it on all
    // five while it kept printing STATUS=OK.
    //
    // Documented defaults live in `infra/services/mc-service/configmap.yaml`,
    // which is the artifact an operator actually reads — deliberately NOT in
    // `DEFAULT_*` constants here, including in test fixtures, since a constant
    // surviving only in a fixture reads as retired while remaining a live second
    // encoding of the value.
    // ========================================================================
    /// Maximum receive slots one client may declare (`MC_MAX_RECEIVE_SLOTS`).
    ///
    /// ADR-0036 §6 caps slot count server-side by configuration; MC rejects a
    /// whole declaration exceeding it. The cap is what makes
    /// resource-amplification-by-request structurally impossible rather than
    /// rate-limited, so the value itself is validated (`1..=64`) — a cap whose
    /// own value is unbounded is not a cap.
    pub max_receive_slots: usize,

    /// Maximum ACCEPTED receive-capability declarations per connection
    /// (`MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`).
    ///
    /// Bounds client-driven O(N) roster reads on the shared meeting actor.
    /// Charged only on acceptance; every rejection is decided without touching
    /// the actor, so a client looping rejects cannot bypass it. Validated
    /// `1..=4096` for the same reason the slot cap is.
    pub max_receive_capability_declarations: u32,

    /// The audio encoding MC directs clients to produce (`MC_AUDIO_CODEC`,
    /// `MC_AUDIO_MAX_BITRATE_BPS`, `MC_AUDIO_FRAME_RATE_HZ`).
    ///
    /// A parsed, validated value rather than three loose scalars:
    /// `CODEC_UNSPECIFIED` is rejected at load, which is what makes it
    /// structurally unrepresentable at the send-directive emit site.
    ///
    /// **No observable effect until the client honours the send directive**
    /// (story task 19). An operator tuning these before then changes nothing.
    pub audio_encoding: crate::media_signaling::AudioEncoding,
}

/// Custom Debug implementation that redacts sensitive fields.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("redis_url", &"[REDACTED]")
            .field("webtransport_bind_address", &self.webtransport_bind_address)
            .field("grpc_bind_address", &self.grpc_bind_address)
            .field("health_bind_address", &self.health_bind_address)
            .field("region", &self.region)
            .field("gc_grpc_url", &self.gc_grpc_url)
            .field("mc_id", &self.mc_id)
            .field("max_meetings", &self.max_meetings)
            .field("max_participants", &self.max_participants)
            .field("binding_token_ttl_seconds", &self.binding_token_ttl_seconds)
            .field("clock_skew_seconds", &self.clock_skew_seconds)
            .field(
                "nonce_grace_window_seconds",
                &self.nonce_grace_window_seconds,
            )
            .field(
                "disconnect_grace_period_seconds",
                &self.disconnect_grace_period_seconds,
            )
            .field(
                "quic_max_idle_timeout_seconds",
                &self.quic_max_idle_timeout_seconds,
            )
            .field("binding_token_secret", &"[REDACTED]")
            .field("ac_endpoint", &self.ac_endpoint)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("ac_jwks_url", &self.ac_jwks_url)
            .field("tls_cert_path", &self.tls_cert_path)
            .field("tls_key_path", &self.tls_key_path)
            .field("grpc_advertise_address", &self.grpc_advertise_address)
            .field(
                "webtransport_advertise_address",
                &self.webtransport_advertise_address,
            )
            .field("otel_enabled", &self.otel_enabled)
            .field("otel_endpoint", &self.otel_endpoint)
            .field("otel_sample_rate", &self.otel_sample_rate)
            .field("environment", &self.environment)
            .field("max_receive_slots", &self.max_receive_slots)
            .field(
                "max_receive_capability_declarations",
                &self.max_receive_capability_declarations,
            )
            .field("audio_encoding", &self.audio_encoding)
            .finish()
    }
}

#[derive(Debug, Error)]
#[allow(dead_code)] // InvalidValue used in Phase 6b+
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),

    #[error("Invalid OTel configuration: {0}")]
    InvalidOtelConfig(String),
}

impl Config {
    /// Load configuration from environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(&env::vars().collect())
    }

    /// Load configuration from a `HashMap` (for testing).
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let redis_url = SecretString::from(
            vars.get("REDIS_URL")
                .ok_or_else(|| ConfigError::MissingEnvVar("REDIS_URL".to_string()))?
                .clone(),
        );

        let binding_token_secret = SecretString::from(
            vars.get("MC_BINDING_TOKEN_SECRET")
                .ok_or_else(|| ConfigError::MissingEnvVar("MC_BINDING_TOKEN_SECRET".to_string()))?
                .clone(),
        );

        let ac_endpoint = vars
            .get("AC_ENDPOINT")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_ENDPOINT".to_string()))?
            .clone();

        let client_id = vars
            .get("MC_CLIENT_ID")
            .ok_or_else(|| ConfigError::MissingEnvVar("MC_CLIENT_ID".to_string()))?
            .clone();

        let client_secret = SecretString::from(
            vars.get("MC_CLIENT_SECRET")
                .ok_or_else(|| ConfigError::MissingEnvVar("MC_CLIENT_SECRET".to_string()))?
                .clone(),
        );

        let ac_jwks_url = vars
            .get("AC_JWKS_URL")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_JWKS_URL".to_string()))?
            .clone();

        // Basic validation: JWKS URL must use http:// or https://
        if !ac_jwks_url.starts_with("http://") && !ac_jwks_url.starts_with("https://") {
            return Err(ConfigError::InvalidValue(
                "AC_JWKS_URL must start with http:// or https://".to_string(),
            ));
        }

        let tls_cert_path = vars
            .get("MC_TLS_CERT_PATH")
            .ok_or_else(|| ConfigError::MissingEnvVar("MC_TLS_CERT_PATH".to_string()))?
            .clone();

        let tls_key_path = vars
            .get("MC_TLS_KEY_PATH")
            .ok_or_else(|| ConfigError::MissingEnvVar("MC_TLS_KEY_PATH".to_string()))?
            .clone();

        // Validate TLS cert and key files exist at startup (fail-fast)
        if !std::path::Path::new(&tls_cert_path).exists() {
            return Err(ConfigError::InvalidValue(format!(
                "MC_TLS_CERT_PATH file does not exist: {tls_cert_path}"
            )));
        }
        if !std::path::Path::new(&tls_key_path).exists() {
            return Err(ConfigError::InvalidValue(format!(
                "MC_TLS_KEY_PATH file does not exist: {tls_key_path}"
            )));
        }

        let grpc_advertise_address = vars
            .get("MC_GRPC_ADVERTISE_ADDRESS")
            .ok_or_else(|| ConfigError::MissingEnvVar("MC_GRPC_ADVERTISE_ADDRESS".to_string()))?
            .clone();

        let webtransport_advertise_address = vars
            .get("MC_WEBTRANSPORT_ADVERTISE_ADDRESS")
            .ok_or_else(|| {
                ConfigError::MissingEnvVar("MC_WEBTRANSPORT_ADVERTISE_ADDRESS".to_string())
            })?
            .clone();

        let webtransport_bind_address = vars
            .get("MC_WEBTRANSPORT_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_WEBTRANSPORT_BIND_ADDRESS.to_string());

        let grpc_bind_address = vars
            .get("MC_GRPC_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_GRPC_BIND_ADDRESS.to_string());

        let health_bind_address = vars
            .get("MC_HEALTH_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_HEALTH_BIND_ADDRESS.to_string());

        let region = vars
            .get("MC_REGION")
            .cloned()
            .unwrap_or_else(|| "us-east-1".to_string());

        let gc_grpc_url = vars
            .get("GC_GRPC_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:50051".to_string());

        // Parse capacity limits
        let max_meetings = vars
            .get("MC_MAX_MEETINGS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);

        let max_participants = vars
            .get("MC_MAX_PARTICIPANTS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(10000);

        // Parse ADR-0023 session binding parameters
        let binding_token_ttl_seconds = vars
            .get("MC_BINDING_TOKEN_TTL_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_BINDING_TOKEN_TTL_SECONDS);

        let clock_skew_seconds = vars
            .get("MC_CLOCK_SKEW_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_CLOCK_SKEW_SECONDS);

        let nonce_grace_window_seconds = vars
            .get("MC_NONCE_GRACE_WINDOW_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_NONCE_GRACE_WINDOW_SECONDS);

        let disconnect_grace_period_seconds = vars
            .get("MC_DISCONNECT_GRACE_PERIOD_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS);

        // QUIC max-idle-timeout: FAIL-LOUD parse (mirrors the OTEL_SAMPLE_RATE
        // pattern, NOT the lenient `.parse().ok().unwrap_or(DEFAULT)` used by the
        // other `_SECONDS` vars). A non-numeric value, or `0` (which maps to an
        // infinite idle timeout — the wtransport docs warn this can hang
        // futures, and it re-introduces the unbounded crash-detection bug),
        // must crash the process at load rather than silently reverting to a
        // library default.
        let quic_max_idle_timeout_seconds = if let Some(value_str) =
            vars.get("MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS")
        {
            let parsed = value_str.trim().parse::<u64>().map_err(|e| {
                ConfigError::InvalidValue(format!(
                    "MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS must be a positive integer, got '{value_str}': {e}"
                ))
            })?;
            if parsed == 0 {
                return Err(ConfigError::InvalidValue(
                    "MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS must be greater than 0 (0 = infinite idle timeout, which leaves crash/network-loss departures undetected)".to_string(),
                ));
            }
            parsed
        } else {
            DEFAULT_QUIC_MAX_IDLE_TIMEOUT_SECONDS
        };

        // Generate MC instance ID
        let mc_id = vars.get("MC_ID").cloned().unwrap_or_else(|| {
            let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
            let uuid_suffix = uuid::Uuid::new_v4().to_string();
            let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
            format!("{DEFAULT_MC_ID_PREFIX}-{hostname}-{short_suffix}")
        });

        // OpenTelemetry configuration (R-55). Explicit boolean gating via
        // OTEL_ENABLED — NOT presence-of-endpoint. Mirrors AC/GC/MH exactly:
        // case-insensitive + trimmed, and an unrecognized value fails fast at
        // load (never silently coerces to false — that would leave an operator's
        // `OTEL_ENABLED=TRUE`/`1`/`yes` on MC silently OFF while ON elsewhere).
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

        let otel_endpoint = vars.get("OTLP_ENDPOINT").cloned().unwrap_or_default();

        // Sample rate: PARSE-ONLY. The `[0.0, 1.0]` bound has a SINGLE owner —
        // `init_otel` — which re-validates at startup on the enabled path. We do
        // NOT duplicate the range check here (mirrors AC/GC/MH). Non-numeric
        // still fails fast at load rather than silently sampling at the default.
        let otel_sample_rate = if let Some(value_str) = vars.get("OTEL_SAMPLE_RATE") {
            value_str.parse::<f64>().map_err(|e| {
                ConfigError::InvalidOtelConfig(format!(
                    "OTEL_SAMPLE_RATE must be a number, got '{}': {}",
                    value_str, e
                ))
            })?
        } else {
            DEFAULT_OTEL_SAMPLE_RATE
        };

        let environment = vars
            .get("DEPLOYMENT_ENVIRONMENT")
            .cloned()
            .unwrap_or_else(|| DEFAULT_ENVIRONMENT.to_string());

        // ADR-0036 §6: the server-side receive-slot cap. Required, fail-loud,
        // and bounded on BOTH sides — rejecting only 0 would stop the
        // silent-total-outage direction while leaving the amplification bound
        // an operator could retire with a typo.
        let max_receive_slots = bounded(
            "MC_MAX_RECEIVE_SLOTS",
            vars.get("MC_MAX_RECEIVE_SLOTS")
                .ok_or_else(|| ConfigError::MissingEnvVar("MC_MAX_RECEIVE_SLOTS".to_string()))?,
            MIN_RECEIVE_SLOTS,
            MAX_RECEIVE_SLOTS,
        )?;

        // Per-connection budget on ACCEPTED declarations (client-driven
        // meeting-actor work). Bounded on both sides by the same argument.
        let max_receive_capability_declarations = bounded(
            "MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS",
            // DO NOT LET `cargo fmt` TIDY THE NEXT LINE, AND DO NOT LENGTHEN
            // THIS KEY'S NAME. The `MissingEnvVar` constructor and its string
            // literal must stay on ONE line: `dt-guard env-config` discovers
            // required variables with a regex that does not span lines, so a
            // rustfmt-shaped wrap between `MissingEnvVar(` and the string
            // silently removes this key from the discovered required set — and
            // the guard then prints STATUS=OK while the key is unenforced
            // per-workload on every deployment. Verified by running that regex
            // over this file: it missed this key before the hand-reflow.
            //
            // THE MARGIN IS EXACTLY ZERO. That line is **exactly 100
            // characters**, which is rustfmt's default `max_width`, and there is
            // no `rustfmt.toml` anywhere in the tree to change it. One more
            // level of nesting, or one more character in the key name, and the
            // formatter reflows it and the failure is silent. This comment is a
            // warning, NOT a control — the formatter does not read it and acts
            // on width, not intent. The real fix is guard-side and tracked in
            // `docs/TODO.md` §Infrastructure Validation in Devloops.
            vars.get("MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS")
                .ok_or_else(|| {
                    ConfigError::MissingEnvVar("MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS".to_string())
                })?,
            MIN_RECEIVE_CAPABILITY_DECLARATIONS,
            MAX_RECEIVE_CAPABILITY_DECLARATIONS,
        )?;

        // The audio encoding MC directs. Parsed and validated here so that
        // `CODEC_UNSPECIFIED` and out-of-band values cannot reach the emit site
        // at all — an unrecognised codec is an error, NEVER a fall-back to Opus,
        // which would leave an operator believing they configured something they
        // did not.
        let audio_codec = crate::media_signaling::AudioEncoding::parse_codec(
            vars.get("MC_AUDIO_CODEC")
                .ok_or_else(|| ConfigError::MissingEnvVar("MC_AUDIO_CODEC".to_string()))?,
        )
        .map_err(|e| ConfigError::InvalidValue(format!("MC_AUDIO_CODEC is invalid: {e}")))?;

        let audio_max_bitrate_bps = bounded(
            "MC_AUDIO_MAX_BITRATE_BPS",
            vars.get("MC_AUDIO_MAX_BITRATE_BPS").ok_or_else(|| {
                ConfigError::MissingEnvVar("MC_AUDIO_MAX_BITRATE_BPS".to_string())
            })?,
            crate::media_signaling::AUDIO_BITRATE_MIN_BPS,
            crate::media_signaling::AUDIO_BITRATE_MAX_BPS,
        )?;

        let audio_frame_rate_hz = bounded(
            "MC_AUDIO_FRAME_RATE_HZ",
            vars.get("MC_AUDIO_FRAME_RATE_HZ")
                .ok_or_else(|| ConfigError::MissingEnvVar("MC_AUDIO_FRAME_RATE_HZ".to_string()))?,
            crate::media_signaling::AUDIO_FRAME_RATE_MIN_HZ,
            crate::media_signaling::AUDIO_FRAME_RATE_MAX_HZ,
        )?;

        // Codec, bitrate and frame rate become ONE validated value here. After
        // this line an unspecified codec is unrepresentable, which is what makes
        // the send-directive emit site structurally unable to violate
        // `signaling.proto`'s "zero is not a silent Opus" rule.
        let audio_encoding = crate::media_signaling::AudioEncoding::new(
            audio_codec,
            audio_max_bitrate_bps,
            audio_frame_rate_hz,
        )
        .map_err(|e| {
            // Name all THREE inputs, not just `MC_AUDIO_CODEC`. Today
            // `CodecNotAudio` is the only reachable variant here — the bitrate
            // and frame-rate bands are already enforced by `bounded` above,
            // so `BitrateOutOfBand`/`FrameRateOutOfBand` cannot arrive — but
            // hardcoding one key name would silently mislead the moment those
            // bounds diverge. An operator reading this in a CrashLoop gets the
            // effective value of every key that feeds the failure.
            ConfigError::InvalidValue(format!(
                "audio encoding configuration is invalid: {e} \
                 (MC_AUDIO_CODEC={audio_codec:?}, \
                  MC_AUDIO_MAX_BITRATE_BPS={audio_max_bitrate_bps}, \
                  MC_AUDIO_FRAME_RATE_HZ={audio_frame_rate_hz})"
            ))
        })?;

        // Fail-fast: enabling OTel without an endpoint is a config bug — surface
        // it at startup rather than silently no-op'ing exports.
        if otel_enabled && otel_endpoint.trim().is_empty() {
            return Err(ConfigError::InvalidOtelConfig(
                "OTEL_ENABLED=true requires a non-empty OTLP_ENDPOINT".to_string(),
            ));
        }

        Ok(Config {
            redis_url,
            webtransport_bind_address,
            grpc_bind_address,
            health_bind_address,
            region,
            gc_grpc_url,
            mc_id,
            max_meetings,
            max_participants,
            binding_token_ttl_seconds,
            clock_skew_seconds,
            nonce_grace_window_seconds,
            disconnect_grace_period_seconds,
            quic_max_idle_timeout_seconds,
            binding_token_secret,
            ac_endpoint,
            client_id,
            client_secret,
            ac_jwks_url,
            tls_cert_path,
            tls_key_path,
            grpc_advertise_address,
            webtransport_advertise_address,
            otel_enabled,
            otel_endpoint,
            otel_sample_rate,
            environment,
            max_receive_slots,
            max_receive_capability_declarations,
            audio_encoding,
        })
    }

    /// The client-facing media-signalling configuration, as one validated
    /// value.
    ///
    /// Everything here was validated at load, so this is a projection rather
    /// than a second parse.
    #[must_use]
    pub fn client_media_config(&self) -> crate::media_signaling::ClientMediaConfig {
        crate::media_signaling::ClientMediaConfig {
            max_receive_slots: self.max_receive_slots,
            max_receive_capability_declarations: self.max_receive_capability_declarations,
            audio_encoding: self.audio_encoding,
        }
    }

    /// Build an [`OtelConfig`] iff OTel is enabled (R-55).
    ///
    /// Returns `Some` only when `otel_enabled` is `true`; `main.rs` gates the
    /// `init_otel` call on this so a disabled deployment never touches the SDK.
    /// Sample-rate range validation is deferred to `init_otel`.
    #[must_use]
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use common::secret::ExposeSecret;

    fn base_vars() -> HashMap<String, String> {
        HashMap::from([
            (
                "REDIS_URL".to_string(),
                "redis://localhost:6379".to_string(),
            ),
            (
                "MC_BINDING_TOKEN_SECRET".to_string(),
                "dGVzdC1zZWNyZXQtMTIzNDU2Nzg5MA==".to_string(),
            ),
            (
                "AC_ENDPOINT".to_string(),
                "https://ac.example.com".to_string(),
            ),
            ("MC_CLIENT_ID".to_string(), "mc-service".to_string()),
            (
                "MC_CLIENT_SECRET".to_string(),
                "test-client-secret".to_string(),
            ),
            (
                "AC_JWKS_URL".to_string(),
                "https://ac.example.com/.well-known/jwks.json".to_string(),
            ),
            ("MC_TLS_CERT_PATH".to_string(), "/dev/null".to_string()),
            ("MC_TLS_KEY_PATH".to_string(), "/dev/null".to_string()),
            (
                "MC_GRPC_ADVERTISE_ADDRESS".to_string(),
                "http://localhost:50052".to_string(),
            ),
            (
                "MC_WEBTRANSPORT_ADVERTISE_ADDRESS".to_string(),
                "https://localhost:4433".to_string(),
            ),
            // ADR-0036 §5/§6 client-signalling keys. Values are spelled out as
            // LITERALS, never referenced from a `DEFAULT_*` constant: a constant
            // surviving only in a fixture reads as retired to a grep of the load
            // path while remaining a live second encoding of the value. Same
            // discipline as `mh-service`, which deleted
            // `DEFAULT_MAX_CONNECTIONS`, its `unwrap_or`, AND the fixture
            // assertion against it.
            ("MC_MAX_RECEIVE_SLOTS".to_string(), "8".to_string()),
            (
                "MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS".to_string(),
                "64".to_string(),
            ),
            ("MC_AUDIO_CODEC".to_string(), "opus".to_string()),
            ("MC_AUDIO_MAX_BITRATE_BPS".to_string(), "48000".to_string()),
            ("MC_AUDIO_FRAME_RATE_HZ".to_string(), "50".to_string()),
        ])
    }

    /// Every one of the five ADR-0036 client-signalling keys is REQUIRED.
    ///
    /// This is the sole runtime backstop for their presence: `dt-guard
    /// env-config` discovers them by a single-line regex over this file that a
    /// reformat can silently defeat (see `bounded`'s rustdoc and
    /// `docs/TODO.md` §Infrastructure Validation in Devloops), so if the guard
    /// stops seeing a key, this test is what still fails.
    #[test]
    fn test_from_vars_missing_each_media_signalling_key_fails() {
        for key in [
            "MC_MAX_RECEIVE_SLOTS",
            "MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS",
            "MC_AUDIO_CODEC",
            "MC_AUDIO_MAX_BITRATE_BPS",
            "MC_AUDIO_FRAME_RATE_HZ",
        ] {
            let mut vars = base_vars();
            vars.remove(key);
            let err = Config::from_vars(&vars)
                .expect_err(&format!("{key} must be required, not defaulted"));
            match err {
                ConfigError::MissingEnvVar(name) => assert_eq!(name, key),
                other => panic!("{key}: expected MissingEnvVar, got {other:?}"),
            }
        }
    }

    /// Each bounded knob is rejected on BOTH sides.
    ///
    /// Rejecting only the low end would leave the value an operator could
    /// retire with a typo — a cap whose own value is unbounded is not a cap.
    #[test]
    fn test_from_vars_media_signalling_bounds_reject_both_ends() {
        // (key, below-range, above-range)
        for (key, low, high) in [
            ("MC_MAX_RECEIVE_SLOTS", "0", "65"),
            ("MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS", "0", "4097"),
            ("MC_AUDIO_MAX_BITRATE_BPS", "31999", "48001"),
            ("MC_AUDIO_FRAME_RATE_HZ", "24", "51"),
        ] {
            for value in [low, high] {
                let mut vars = base_vars();
                vars.insert(key.to_string(), value.to_string());
                let err =
                    Config::from_vars(&vars).expect_err(&format!("{key}={value} must be rejected"));
                assert!(
                    matches!(err, ConfigError::InvalidValue(_)),
                    "{key}={value}: expected InvalidValue, got {err:?}"
                );
            }
        }
    }

    /// Non-numeric values fail loud rather than silently reverting.
    #[test]
    fn test_from_vars_media_signalling_non_numeric_fails_loud() {
        for key in [
            "MC_MAX_RECEIVE_SLOTS",
            "MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS",
            "MC_AUDIO_MAX_BITRATE_BPS",
            "MC_AUDIO_FRAME_RATE_HZ",
        ] {
            let mut vars = base_vars();
            vars.insert(key.to_string(), "not-a-number".to_string());
            assert!(
                matches!(Config::from_vars(&vars), Err(ConfigError::InvalidValue(_))),
                "{key} must fail loud on a non-numeric value"
            );
        }
    }

    /// An unrecognised codec is an ERROR at load, never a fall-back to Opus.
    ///
    /// A silent coercion would leave an operator believing they had configured
    /// something they had not — and `CODEC_UNSPECIFIED` reaching a send
    /// directive is what `AudioEncoding`'s private fields make unrepresentable
    /// downstream, so this load-time check is where that guarantee starts.
    #[test]
    fn test_from_vars_audio_codec_rejects_unknown_unspecified_and_video() {
        // Two DIFFERENT failure paths, asserted separately because they produce
        // different messages and the message is the point.
        //
        // Asserting only `InvalidValue(_)` would be satisfied by any text at
        // all, including a message that names no key — so a refactor dropping
        // the key name would pass unchanged. An operator reading this in a
        // CrashLoop needs to know WHICH variable to edit, and this is the
        // file's own convention (see the
        // `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` assertions below).

        // Path 1: rejected in `parse_codec` — not a codec name at all.
        for value in ["oppus", "", "unspecified"] {
            let mut vars = base_vars();
            vars.insert("MC_AUDIO_CODEC".to_string(), value.to_string());
            let err = Config::from_vars(&vars)
                .expect_err("an unrecognised codec must be rejected, never coerced to Opus");
            match err {
                ConfigError::InvalidValue(msg) => assert!(
                    msg.contains("MC_AUDIO_CODEC"),
                    "MC_AUDIO_CODEC='{value}': message must name the key, got: {msg}"
                ),
                other => panic!("MC_AUDIO_CODEC='{value}': expected InvalidValue, got {other:?}"),
            }
        }

        // Path 2: a REAL codec that is not an audio codec. `parse_codec`
        // accepts it and `AudioEncoding::new` rejects it, so this takes the
        // message that names all three `MC_AUDIO_*` inputs.
        for value in ["vp9", "h264", "av1"] {
            let mut vars = base_vars();
            vars.insert("MC_AUDIO_CODEC".to_string(), value.to_string());
            let err = Config::from_vars(&vars)
                .expect_err("a video codec must be rejected for an audio-only knob");
            match err {
                ConfigError::InvalidValue(msg) => {
                    assert!(
                        msg.contains("not an audio codec"),
                        "MC_AUDIO_CODEC='{value}': expected the audio-kind reason, got: {msg}"
                    );
                    // All three, not just the one that happened to be wrong:
                    // the message must stay useful if the bounds ever diverge
                    // and another variant becomes reachable here.
                    for key in [
                        "MC_AUDIO_CODEC",
                        "MC_AUDIO_MAX_BITRATE_BPS",
                        "MC_AUDIO_FRAME_RATE_HZ",
                    ] {
                        assert!(
                            msg.contains(key),
                            "MC_AUDIO_CODEC='{value}': message must name {key}, got: {msg}"
                        );
                    }
                }
                other => panic!("MC_AUDIO_CODEC='{value}': expected InvalidValue, got {other:?}"),
            }
        }
    }

    /// The codec parse is trimmed and case-folded, so ordinary ConfigMap
    /// whitespace and capitalisation are accepted rather than crashing a pod.
    #[test]
    fn test_from_vars_audio_codec_accepts_trimmed_and_case_folded() {
        for value in ["opus", "  opus  ", "OPUS", "Opus"] {
            let mut vars = base_vars();
            vars.insert("MC_AUDIO_CODEC".to_string(), value.to_string());
            let config = Config::from_vars(&vars)
                .unwrap_or_else(|e| panic!("MC_AUDIO_CODEC='{value}' should load: {e:?}"));
            assert_eq!(
                config.audio_encoding.codec(),
                proto_gen::dark_tower::signaling::v1::Codec::Opus
            );
        }
    }

    /// The effective values reach `Config`, since they are what MC directs every
    /// client's encoder with and what the startup line reports.
    #[test]
    fn test_from_vars_media_signalling_values_are_effective() {
        let mut vars = base_vars();
        vars.insert("MC_MAX_RECEIVE_SLOTS".to_string(), "3".to_string());
        vars.insert(
            "MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS".to_string(),
            "7".to_string(),
        );
        vars.insert("MC_AUDIO_MAX_BITRATE_BPS".to_string(), "32000".to_string());
        vars.insert("MC_AUDIO_FRAME_RATE_HZ".to_string(), "25".to_string());

        let config = Config::from_vars(&vars).expect("in-range values load");
        assert_eq!(config.max_receive_slots, 3);
        assert_eq!(config.max_receive_capability_declarations, 7);
        assert_eq!(config.audio_encoding.max_bitrate_bps(), 32_000);
        assert_eq!(config.audio_encoding.frame_rate_hz(), 25);

        let projected = config.client_media_config();
        assert_eq!(projected.max_receive_slots, 3);
        assert_eq!(projected.max_receive_capability_declarations, 7);
        assert_eq!(projected.audio_encoding, config.audio_encoding);
    }

    #[test]
    fn test_from_vars_success_with_defaults() {
        let vars = base_vars();

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.redis_url.expose_secret(), "redis://localhost:6379");
        assert_eq!(
            config.webtransport_bind_address,
            DEFAULT_WEBTRANSPORT_BIND_ADDRESS
        );
        assert_eq!(config.grpc_bind_address, DEFAULT_GRPC_BIND_ADDRESS);
        assert_eq!(config.health_bind_address, DEFAULT_HEALTH_BIND_ADDRESS);
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.gc_grpc_url, "http://localhost:50051");
        assert_eq!(config.max_meetings, 1000);
        assert_eq!(config.max_participants, 10000);
        assert_eq!(
            config.binding_token_ttl_seconds,
            DEFAULT_BINDING_TOKEN_TTL_SECONDS
        );
        assert_eq!(config.clock_skew_seconds, DEFAULT_CLOCK_SKEW_SECONDS);
        assert_eq!(
            config.nonce_grace_window_seconds,
            DEFAULT_NONCE_GRACE_WINDOW_SECONDS
        );
        assert_eq!(
            config.disconnect_grace_period_seconds,
            DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS
        );
        assert_eq!(
            config.quic_max_idle_timeout_seconds,
            DEFAULT_QUIC_MAX_IDLE_TIMEOUT_SECONDS
        );
        // MC ID should be auto-generated
        assert!(config.mc_id.starts_with("mc-"));
        assert_eq!(
            config.ac_jwks_url,
            "https://ac.example.com/.well-known/jwks.json"
        );
        assert_eq!(config.grpc_advertise_address, "http://localhost:50052");
        assert_eq!(
            config.webtransport_advertise_address,
            "https://localhost:4433"
        );
    }

    #[test]
    fn test_from_vars_success_with_custom_values() {
        let mut vars = base_vars();
        vars.insert(
            "MC_WEBTRANSPORT_BIND_ADDRESS".to_string(),
            "127.0.0.1:4434".to_string(),
        );
        vars.insert(
            "MC_GRPC_BIND_ADDRESS".to_string(),
            "127.0.0.1:50053".to_string(),
        );
        vars.insert(
            "MC_HEALTH_BIND_ADDRESS".to_string(),
            "127.0.0.1:8082".to_string(),
        );
        vars.insert("MC_REGION".to_string(), "eu-west-1".to_string());
        vars.insert("GC_GRPC_URL".to_string(), "http://gc:50051".to_string());
        vars.insert("MC_MAX_MEETINGS".to_string(), "500".to_string());
        vars.insert("MC_MAX_PARTICIPANTS".to_string(), "5000".to_string());
        vars.insert("MC_BINDING_TOKEN_TTL_SECONDS".to_string(), "60".to_string());
        vars.insert("MC_CLOCK_SKEW_SECONDS".to_string(), "10".to_string());
        vars.insert(
            "MC_DISCONNECT_GRACE_PERIOD_SECONDS".to_string(),
            "45".to_string(),
        );
        vars.insert(
            "MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS".to_string(),
            "15".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.webtransport_bind_address, "127.0.0.1:4434");
        assert_eq!(config.grpc_bind_address, "127.0.0.1:50053");
        assert_eq!(config.health_bind_address, "127.0.0.1:8082");
        assert_eq!(config.region, "eu-west-1");
        assert_eq!(config.gc_grpc_url, "http://gc:50051");
        assert_eq!(config.max_meetings, 500);
        assert_eq!(config.max_participants, 5000);
        assert_eq!(config.binding_token_ttl_seconds, 60);
        assert_eq!(config.clock_skew_seconds, 10);
        assert_eq!(config.disconnect_grace_period_seconds, 45);
        assert_eq!(config.quic_max_idle_timeout_seconds, 15);
    }

    // =========================================================================
    // QUIC max-idle-timeout config — FAIL-LOUD parse (task #64)
    // =========================================================================

    #[test]
    fn test_quic_max_idle_timeout_default() {
        let config = Config::from_vars(&base_vars()).expect("Config should load");
        assert_eq!(
            config.quic_max_idle_timeout_seconds,
            DEFAULT_QUIC_MAX_IDLE_TIMEOUT_SECONDS
        );
    }

    #[test]
    fn test_quic_max_idle_timeout_custom_value() {
        let mut vars = base_vars();
        vars.insert(
            "MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS".to_string(),
            "7".to_string(),
        );
        let config = Config::from_vars(&vars).expect("Config should load");
        assert_eq!(config.quic_max_idle_timeout_seconds, 7);
    }

    #[test]
    fn test_quic_max_idle_timeout_non_numeric_fails_loud() {
        // A non-numeric value fails fast at load rather than silently reverting
        // to a library default (which would re-introduce unbounded detection).
        let mut vars = base_vars();
        vars.insert(
            "MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS".to_string(),
            "not-a-number".to_string(),
        );
        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidValue(msg))
                if msg.contains("MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS") && msg.contains("not-a-number")),
            "expected fail-loud error naming the var and echoing the bad value"
        );
    }

    #[test]
    fn test_quic_max_idle_timeout_zero_rejected() {
        // 0 = infinite idle timeout → crash/network-loss departures would never
        // be detected. Must be rejected, not accepted.
        let mut vars = base_vars();
        vars.insert(
            "MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS".to_string(),
            "0".to_string(),
        );
        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidValue(msg))
                if msg.contains("MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS") && msg.contains("greater than 0")),
            "expected zero to be rejected fail-loud"
        );
    }

    #[test]
    fn test_mc_id_custom_value() {
        let mut vars = base_vars();
        vars.insert("MC_ID".to_string(), "mc-custom-001".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.mc_id, "mc-custom-001");
    }

    #[test]
    fn test_from_vars_missing_redis_url() {
        let mut vars = base_vars();
        vars.remove("REDIS_URL");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "REDIS_URL"));
    }

    #[test]
    fn test_from_vars_missing_binding_token_secret() {
        let mut vars = base_vars();
        vars.remove("MC_BINDING_TOKEN_SECRET");

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MC_BINDING_TOKEN_SECRET")
        );
    }

    #[test]
    fn test_from_vars_missing_ac_endpoint() {
        let mut vars = base_vars();
        vars.remove("AC_ENDPOINT");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "AC_ENDPOINT"));
    }

    #[test]
    fn test_from_vars_missing_client_id() {
        let mut vars = base_vars();
        vars.remove("MC_CLIENT_ID");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MC_CLIENT_ID"));
    }

    #[test]
    fn test_from_vars_missing_client_secret() {
        let mut vars = base_vars();
        vars.remove("MC_CLIENT_SECRET");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MC_CLIENT_SECRET"));
    }

    #[test]
    fn test_oauth_config_loaded_correctly() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.ac_endpoint, "https://ac.example.com");
        assert_eq!(config.client_id, "mc-service");
        assert_eq!(config.client_secret.expose_secret(), "test-client-secret");
    }

    #[test]
    fn test_debug_redacts_sensitive_fields() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        let debug_output = format!("{config:?}");

        // Sensitive fields should be redacted
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("redis://"));
        assert!(!debug_output.contains("dGVzdC1zZWNyZXQ"));
        assert!(!debug_output.contains("test-client-secret"));
        // ac_endpoint and client_id are not sensitive
        assert!(debug_output.contains("https://ac.example.com"));
        assert!(debug_output.contains("mc-service"));
        // ac_jwks_url is not sensitive
        assert!(debug_output.contains(".well-known/jwks.json"));
    }

    #[test]
    fn test_from_vars_missing_ac_jwks_url() {
        let mut vars = base_vars();
        vars.remove("AC_JWKS_URL");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "AC_JWKS_URL"));
    }

    #[test]
    fn test_from_vars_invalid_ac_jwks_url_scheme() {
        let mut vars = base_vars();
        vars.insert("AC_JWKS_URL".to_string(), "ftp://bad-scheme".to_string());

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::InvalidValue(msg)) if msg.contains("http")));
    }

    #[test]
    fn test_ac_jwks_url_http_allowed() {
        let mut vars = base_vars();
        vars.insert(
            "AC_JWKS_URL".to_string(),
            "http://localhost:8082/.well-known/jwks.json".to_string(),
        );

        let config = Config::from_vars(&vars).expect("http should be allowed for local dev");
        assert_eq!(
            config.ac_jwks_url,
            "http://localhost:8082/.well-known/jwks.json"
        );
    }

    #[test]
    fn test_from_vars_missing_tls_cert_path() {
        let mut vars = base_vars();
        vars.remove("MC_TLS_CERT_PATH");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MC_TLS_CERT_PATH"));
    }

    #[test]
    fn test_from_vars_missing_tls_key_path() {
        let mut vars = base_vars();
        vars.remove("MC_TLS_KEY_PATH");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MC_TLS_KEY_PATH"));
    }

    #[test]
    fn test_from_vars_tls_cert_path_nonexistent() {
        let mut vars = base_vars();
        vars.insert(
            "MC_TLS_CERT_PATH".to_string(),
            "/nonexistent/cert.pem".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidValue(msg)) if msg.contains("does not exist"))
        );
    }

    #[test]
    fn test_from_vars_tls_key_path_nonexistent() {
        let mut vars = base_vars();
        vars.insert(
            "MC_TLS_KEY_PATH".to_string(),
            "/nonexistent/key.pem".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidValue(msg)) if msg.contains("does not exist"))
        );
    }

    #[test]
    fn test_tls_config_loaded_correctly() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.tls_cert_path, "/dev/null");
        assert_eq!(config.tls_key_path, "/dev/null");
    }

    #[test]
    fn test_from_vars_missing_grpc_advertise_address() {
        let mut vars = base_vars();
        vars.remove("MC_GRPC_ADVERTISE_ADDRESS");

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MC_GRPC_ADVERTISE_ADDRESS")
        );
    }

    #[test]
    fn test_from_vars_missing_webtransport_advertise_address() {
        let mut vars = base_vars();
        vars.remove("MC_WEBTRANSPORT_ADVERTISE_ADDRESS");

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MC_WEBTRANSPORT_ADVERTISE_ADDRESS")
        );
    }

    // =========================================================================
    // OpenTelemetry config (R-55) — mirrors AC #25 / GC #26 / MH #27
    // =========================================================================

    #[test]
    fn test_otel_defaults_disabled() {
        let config = Config::from_vars(&base_vars()).expect("Config should load");
        assert!(!config.otel_enabled);
        assert_eq!(config.otel_endpoint, "");
        assert!((config.otel_sample_rate - DEFAULT_OTEL_SAMPLE_RATE).abs() < f64::EPSILON);
        assert_eq!(config.environment, DEFAULT_ENVIRONMENT);
    }

    #[test]
    fn test_otel_enabled_true_with_endpoint() {
        let mut vars = base_vars();
        vars.insert("OTEL_ENABLED".to_string(), "true".to_string());
        vars.insert(
            "OTLP_ENDPOINT".to_string(),
            "http://otel-collector.dark-tower:4317".to_string(),
        );
        let config = Config::from_vars(&vars).expect("Config should load");
        assert!(config.otel_enabled);
        assert_eq!(
            config.otel_endpoint,
            "http://otel-collector.dark-tower:4317"
        );
    }

    #[test]
    fn test_otel_enabled_case_insensitive() {
        // Mirrors AC/GC/MH: parsing is case-insensitive + trimmed, so an
        // operator's `OTEL_ENABLED=TRUE` enables OTel just like elsewhere.
        let mut vars = base_vars();
        vars.insert("OTEL_ENABLED".to_string(), "  TRUE ".to_string());
        vars.insert(
            "OTLP_ENDPOINT".to_string(),
            "http://collector:4317".to_string(),
        );
        let config = Config::from_vars(&vars).expect("Config should load");
        assert!(config.otel_enabled);
    }

    #[test]
    fn test_otel_enabled_invalid_value_rejected() {
        // Mirrors AC/GC/MH: an unrecognized value fails fast at load (carrying
        // the offending value) rather than silently coercing to false — that
        // silent coercion would leave MC's tracing OFF for a value that enables
        // it on the sibling services.
        for bad in ["1", "yes", "on", "enabled"] {
            let mut vars = base_vars();
            vars.insert("OTEL_ENABLED".to_string(), bad.to_string());
            let result = Config::from_vars(&vars);
            assert!(
                matches!(&result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains(bad)),
                "OTEL_ENABLED={bad:?} should be rejected with the offending value, got {result:?}"
            );
        }
    }

    #[test]
    fn test_otel_sample_rate_parsed() {
        let mut vars = base_vars();
        vars.insert("OTEL_SAMPLE_RATE".to_string(), "0.25".to_string());
        let config = Config::from_vars(&vars).expect("Config should load");
        assert!((config.otel_sample_rate - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_otel_sample_rate_non_numeric_rejected() {
        // Mirrors AC/GC/MH: a non-numeric value fails fast at load rather than
        // silently sampling at the default (a prod typo → 100% collector flood).
        // Range validation ([0.0, 1.0]) stays init_otel's job — parse-only here.
        let mut vars = base_vars();
        vars.insert("OTEL_SAMPLE_RATE".to_string(), "not-a-number".to_string());
        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("OTEL_SAMPLE_RATE"))
        );
    }

    #[test]
    fn test_environment_parsed() {
        let mut vars = base_vars();
        vars.insert(
            "DEPLOYMENT_ENVIRONMENT".to_string(),
            "production".to_string(),
        );
        let config = Config::from_vars(&vars).expect("Config should load");
        assert_eq!(config.environment, "production");
    }

    #[test]
    fn test_otel_config_none_when_disabled() {
        let config = Config::from_vars(&base_vars()).expect("Config should load");
        assert!(config.otel_config().is_none());
    }

    #[test]
    fn test_otel_config_some_when_enabled() {
        let mut vars = base_vars();
        vars.insert("OTEL_ENABLED".to_string(), "true".to_string());
        vars.insert(
            "OTLP_ENDPOINT".to_string(),
            "http://collector:4317".to_string(),
        );
        vars.insert("OTEL_SAMPLE_RATE".to_string(), "0.5".to_string());
        let config = Config::from_vars(&vars).expect("Config should load");
        let otel = config.otel_config().expect("otel_config should be Some");
        assert_eq!(otel.endpoint, "http://collector:4317");
        assert!((otel.sample_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_otel_enabled_empty_endpoint_fails_fast() {
        let mut vars = base_vars();
        vars.insert("OTEL_ENABLED".to_string(), "true".to_string());
        // OTLP_ENDPOINT deliberately absent → default "".
        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("OTLP_ENDPOINT"))
        );
    }

    #[test]
    fn test_otel_enabled_whitespace_endpoint_fails_fast() {
        let mut vars = base_vars();
        vars.insert("OTEL_ENABLED".to_string(), "true".to_string());
        vars.insert("OTLP_ENDPOINT".to_string(), "   ".to_string());
        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::InvalidOtelConfig(_))));
    }

    #[test]
    fn test_otel_disabled_empty_endpoint_ok() {
        // Disabled + empty endpoint is the safe base default — must NOT fail.
        let config = Config::from_vars(&base_vars()).expect("disabled+empty must load");
        assert!(!config.otel_enabled);
        assert!(config.otel_config().is_none());
    }

    #[test]
    fn test_debug_includes_otel_non_secret_fields() {
        let mut vars = base_vars();
        vars.insert("OTEL_ENABLED".to_string(), "true".to_string());
        vars.insert(
            "OTLP_ENDPOINT".to_string(),
            "http://otel-collector.dark-tower:4317".to_string(),
        );
        let config = Config::from_vars(&vars).expect("Config should load");
        let debug_output = format!("{config:?}");
        // OTel fields are non-secret and appear in Debug (mirrors AC/GC/MH).
        assert!(debug_output.contains("otel_enabled"));
        assert!(debug_output.contains("otel-collector.dark-tower:4317"));
        assert!(debug_output.contains("environment"));
    }
}
