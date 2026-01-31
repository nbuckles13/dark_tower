//! Meeting Controller configuration.
//!
//! Configuration is loaded from environment variables. All sensitive
//! fields are redacted in Debug output.

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

/// Default MC instance ID prefix.
pub const DEFAULT_MC_ID_PREFIX: &str = "mc";

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

    /// Master secret for binding token HMAC (base64-encoded).
    /// Rotates on each deployment for defense-in-depth.
    /// Protected by `SecretString` to prevent accidental logging.
    pub binding_token_secret: SecretString,
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
            .field("binding_token_secret", &"[REDACTED]")
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

        // Generate MC instance ID
        let mc_id = vars.get("MC_ID").cloned().unwrap_or_else(|| {
            let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
            let uuid_suffix = uuid::Uuid::new_v4().to_string();
            let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
            format!("{DEFAULT_MC_ID_PREFIX}-{hostname}-{short_suffix}")
        });

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
            binding_token_secret,
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
                "REDIS_URL".to_string(),
                "redis://localhost:6379".to_string(),
            ),
            (
                "MC_BINDING_TOKEN_SECRET".to_string(),
                "dGVzdC1zZWNyZXQtMTIzNDU2Nzg5MA==".to_string(),
            ),
        ])
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
        // MC ID should be auto-generated
        assert!(config.mc_id.starts_with("mc-"));
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
    fn test_debug_redacts_sensitive_fields() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        let debug_output = format!("{config:?}");

        // Sensitive fields should be redacted
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("redis://"));
        assert!(!debug_output.contains("dGVzdC1zZWNyZXQ"));
    }
}
