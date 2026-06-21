//! Global Controller configuration.
//!
//! Configuration is loaded from environment variables. All sensitive
//! fields are redacted in Debug output.

use common::jwt::{DEFAULT_CLOCK_SKEW, MAX_CLOCK_SKEW};
use common::secret::SecretString;
use std::collections::HashMap;
use std::env;
use std::fmt;
use thiserror::Error;

/// Default rate limit in requests per minute.
pub const DEFAULT_RATE_LIMIT_RPM: u32 = 100;

/// Default gRPC bind address.
pub const DEFAULT_GRPC_BIND_ADDRESS: &str = "0.0.0.0:50051";

/// Default MC staleness threshold in seconds.
pub const DEFAULT_MC_STALENESS_THRESHOLD_SECONDS: u64 = 30;

/// Default GC instance ID prefix.
pub const DEFAULT_GC_ID_PREFIX: &str = "gc";

/// Default in-cluster OTel collector endpoint for the telemetry proxy (R-2).
///
/// This is the **bare base** (scheme + host + port, no path, no trailing slash).
/// The telemetry forwarder appends the per-signal suffix `/v1/metrics` or
/// `/v1/traces` itself, so this value must NOT include a `/v1/...` path.
/// (Contract with R-59/INFRA-OTEL: the env var holds the bare base; GC owns the
/// suffix. Distinct from R-55's `otel_endpoint`, which is the gRPC `:4317`
/// endpoint for GC's own span export — this is the HTTP `:4318` endpoint the
/// `/api/v1/telemetry` proxy forwards to.)
pub const DEFAULT_OTEL_COLLECTOR_ENDPOINT: &str =
    "http://otel-collector.default.svc.cluster.local:4318";

/// Default maximum telemetry proxy payload size in bytes (256 KiB).
pub const DEFAULT_TELEMETRY_PROXY_MAX_BYTES: usize = 256 * 1024;

/// Default per-user telemetry proxy rate limit (requests per minute).
pub const DEFAULT_TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE: u32 = 60;

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

    /// gRPC server bind address (default: "0.0.0.0:50051").
    pub grpc_bind_address: String,

    /// MC staleness threshold in seconds (default: 30).
    /// Controllers that haven't sent a heartbeat within this time are marked unhealthy.
    pub mc_staleness_threshold_seconds: u64,

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
    /// See [`DEFAULT_OTEL_COLLECTOR_ENDPOINT`].
    pub otel_collector_endpoint: String,

    /// Maximum accepted telemetry proxy payload size in bytes (default 256 KiB).
    /// Enforced on real body bytes (oversize → 413).
    pub telemetry_proxy_max_bytes: usize,

    /// Per-user (JWT `sub`) telemetry proxy rate limit, requests per minute
    /// (default 60). Exceeding it → 429.
    pub telemetry_proxy_rate_limit_per_minute: u32,
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
            .field(
                "mc_staleness_threshold_seconds",
                &self.mc_staleness_threshold_seconds,
            )
            .field("gc_id", &self.gc_id)
            .field("gc_client_id", &self.gc_client_id)
            .field("gc_client_secret", &"[REDACTED]")
            .field("otel_collector_endpoint", &self.otel_collector_endpoint)
            .field("telemetry_proxy_max_bytes", &self.telemetry_proxy_max_bytes)
            .field(
                "telemetry_proxy_rate_limit_per_minute",
                &self.telemetry_proxy_rate_limit_per_minute,
            )
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

        let bind_address = vars
            .get("BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| "0.0.0.0:8080".to_string());

        let region = vars
            .get("GC_REGION")
            .cloned()
            .unwrap_or_else(|| "us-east-1".to_string());

        let ac_jwks_url = vars
            .get("AC_JWKS_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:8082/.well-known/jwks.json".to_string());

        let ac_internal_url = vars
            .get("AC_INTERNAL_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:8082".to_string());

        // Parse JWT clock skew tolerance with validation
        let jwt_clock_skew_seconds = if let Some(value_str) = vars.get("JWT_CLOCK_SKEW_SECONDS") {
            let value: i64 = value_str.parse().map_err(|e| {
                ConfigError::InvalidJwtClockSkew(format!(
                    "JWT_CLOCK_SKEW_SECONDS must be a valid integer, got '{}': {}",
                    value_str, e
                ))
            })?;

            if value <= 0 {
                return Err(ConfigError::InvalidJwtClockSkew(format!(
                    "JWT_CLOCK_SKEW_SECONDS must be positive, got {}",
                    value
                )));
            }

            if value > MAX_CLOCK_SKEW.as_secs() as i64 {
                return Err(ConfigError::InvalidJwtClockSkew(format!(
                    "JWT_CLOCK_SKEW_SECONDS must not exceed {} seconds, got {}",
                    MAX_CLOCK_SKEW.as_secs(),
                    value
                )));
            }

            value
        } else {
            DEFAULT_CLOCK_SKEW.as_secs() as i64
        };

        // Parse rate limit with validation
        let rate_limit_rpm = if let Some(value_str) = vars.get("RATE_LIMIT_RPM") {
            let value: u32 = value_str.parse().map_err(|e| {
                ConfigError::InvalidRateLimit(format!(
                    "RATE_LIMIT_RPM must be a valid positive integer, got '{}': {}",
                    value_str, e
                ))
            })?;

            if value == 0 {
                return Err(ConfigError::InvalidRateLimit(
                    "RATE_LIMIT_RPM must be greater than 0".to_string(),
                ));
            }

            value
        } else {
            DEFAULT_RATE_LIMIT_RPM
        };

        // Parse gRPC bind address
        let grpc_bind_address = vars
            .get("GC_GRPC_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_GRPC_BIND_ADDRESS.to_string());

        // Parse MC staleness threshold with validation
        let mc_staleness_threshold_seconds =
            if let Some(value_str) = vars.get("MC_STALENESS_THRESHOLD_SECONDS") {
                let value: u64 = value_str.parse().map_err(|e| {
                    ConfigError::InvalidMcStalenessThreshold(format!(
                    "MC_STALENESS_THRESHOLD_SECONDS must be a valid positive integer, got '{}': {}",
                    value_str, e
                ))
                })?;

                if value == 0 {
                    return Err(ConfigError::InvalidMcStalenessThreshold(
                        "MC_STALENESS_THRESHOLD_SECONDS must be greater than 0".to_string(),
                    ));
                }

                value
            } else {
                DEFAULT_MC_STALENESS_THRESHOLD_SECONDS
            };

        // Generate GC instance ID
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

        // Telemetry proxy: in-cluster OTel collector endpoint (R-2).
        // Bare base — forwarder appends the per-signal suffix. Empty string is
        // permitted and disables the proxy (route returns 503).
        let otel_collector_endpoint = vars
            .get("OTEL_COLLECTOR_ENDPOINT")
            .cloned()
            .unwrap_or_else(|| DEFAULT_OTEL_COLLECTOR_ENDPOINT.to_string());

        // Telemetry proxy max payload size with validation (must be > 0).
        let telemetry_proxy_max_bytes =
            if let Some(value_str) = vars.get("TELEMETRY_PROXY_MAX_BYTES") {
                let value: usize = value_str.parse().map_err(|e| {
                    ConfigError::InvalidTelemetryProxyMaxBytes(format!(
                        "TELEMETRY_PROXY_MAX_BYTES must be a valid positive integer, got '{}': {}",
                        value_str, e
                    ))
                })?;

                if value == 0 {
                    return Err(ConfigError::InvalidTelemetryProxyMaxBytes(
                        "TELEMETRY_PROXY_MAX_BYTES must be greater than 0".to_string(),
                    ));
                }

                value
            } else {
                DEFAULT_TELEMETRY_PROXY_MAX_BYTES
            };

        // Telemetry proxy per-user rate limit with validation (must be > 0:
        // governor's Quota is built from a NonZeroU32, and a zero quota would
        // deny all telemetry).
        let telemetry_proxy_rate_limit_per_minute = if let Some(value_str) =
            vars.get("TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE")
        {
            let value: u32 = value_str.parse().map_err(|e| {
                    ConfigError::InvalidTelemetryProxyRateLimit(format!(
                        "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE must be a valid positive integer, got '{}': {}",
                        value_str, e
                    ))
                })?;

            if value == 0 {
                return Err(ConfigError::InvalidTelemetryProxyRateLimit(
                    "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE must be greater than 0".to_string(),
                ));
            }

            value
        } else {
            DEFAULT_TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE
        };

        Ok(Config {
            database_url,
            bind_address,
            region,
            ac_jwks_url,
            ac_internal_url,
            jwt_clock_skew_seconds,
            rate_limit_rpm,
            grpc_bind_address,
            mc_staleness_threshold_seconds,
            gc_id,
            gc_client_id,
            gc_client_secret: SecretString::from(gc_client_secret),
            otel_collector_endpoint,
            telemetry_proxy_max_bytes,
            telemetry_proxy_rate_limit_per_minute,
        })
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
        ])
    }

    #[test]
    fn test_from_vars_success_with_defaults() {
        let vars = base_vars();

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.database_url, "postgresql://localhost/gc_test");
        assert_eq!(config.bind_address, "0.0.0.0:8080");
        assert_eq!(config.region, "us-east-1");
        assert_eq!(
            config.ac_jwks_url,
            "http://localhost:8082/.well-known/jwks.json"
        );
        assert_eq!(config.ac_internal_url, "http://localhost:8082");
        assert_eq!(
            config.jwt_clock_skew_seconds,
            DEFAULT_CLOCK_SKEW.as_secs() as i64
        );
        assert_eq!(config.rate_limit_rpm, DEFAULT_RATE_LIMIT_RPM);
        assert_eq!(config.grpc_bind_address, DEFAULT_GRPC_BIND_ADDRESS);
        assert_eq!(
            config.mc_staleness_threshold_seconds,
            DEFAULT_MC_STALENESS_THRESHOLD_SECONDS
        );
        // GC ID should be auto-generated
        assert!(config.gc_id.starts_with("gc-"));
        // OAuth client credentials should be loaded
        assert_eq!(config.gc_client_id, "test-gc-client");
        assert_eq!(config.gc_client_secret.expose_secret(), "test-gc-secret");
        // Telemetry proxy defaults
        assert_eq!(
            config.otel_collector_endpoint,
            DEFAULT_OTEL_COLLECTOR_ENDPOINT
        );
        assert_eq!(
            config.telemetry_proxy_max_bytes,
            DEFAULT_TELEMETRY_PROXY_MAX_BYTES
        );
        assert_eq!(config.telemetry_proxy_max_bytes, 262144);
        assert_eq!(
            config.telemetry_proxy_rate_limit_per_minute,
            DEFAULT_TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE
        );
        assert_eq!(config.telemetry_proxy_rate_limit_per_minute, 60);
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
        assert_eq!(config.mc_staleness_threshold_seconds, 60);
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
}
