//! Media Handler configuration.
//!
//! Configuration is loaded from environment variables. All sensitive
//! fields are redacted in Debug output.
//!
//! ## OAuth Configuration (ADR-0003)
//!
//! MH uses OAuth 2.0 client credentials for authenticating to GC via AC.
//! Required environment variables:
//! - `AC_ENDPOINT`: Authentication Controller endpoint (e.g., `http://localhost:8082`)
//! - `MH_CLIENT_ID`: OAuth client ID for MH
//! - `MH_CLIENT_SECRET`: OAuth client secret for MH

use common::secret::SecretString;
use std::collections::HashMap;
use std::env;
use std::fmt;
use thiserror::Error;

/// Default gRPC bind address for MC→MH communication.
pub const DEFAULT_GRPC_BIND_ADDRESS: &str = "0.0.0.0:50053";

/// Default health endpoint bind address.
pub const DEFAULT_HEALTH_BIND_ADDRESS: &str = "0.0.0.0:8083";

/// Default WebTransport bind address.
pub const DEFAULT_WEBTRANSPORT_BIND_ADDRESS: &str = "0.0.0.0:4434";

/// Default maximum concurrent streams.
pub const DEFAULT_MAX_STREAMS: u32 = 1000;

/// Default MH instance ID prefix.
pub const DEFAULT_MH_ID_PREFIX: &str = "mh";

/// Default `RegisterMeeting` timeout in seconds.
/// Clients connecting to unregistered meetings are provisionally accepted
/// for this window; if `RegisterMeeting` does not arrive, they are disconnected.
pub const DEFAULT_REGISTER_MEETING_TIMEOUT_SECONDS: u64 = 15;

/// Maximum allowed `RegisterMeeting` timeout in seconds (5 minutes).
/// Capped to prevent misconfiguration from effectively disabling the
/// provisional timeout security control (R-14).
pub const MAX_REGISTER_MEETING_TIMEOUT_SECONDS: u64 = 300;

/// Default maximum concurrent WebTransport connections.
pub const DEFAULT_MAX_CONNECTIONS: usize = 10_000;

/// Media Handler configuration.
///
/// Loaded from environment variables with sensible defaults.
/// Sensitive fields are redacted in Debug output.
#[derive(Clone)]
pub struct Config {
    /// gRPC server bind address for MC→MH communication (default: "0.0.0.0:50053").
    pub grpc_bind_address: String,

    /// Health endpoint bind address (default: "0.0.0.0:8083").
    pub health_bind_address: String,

    /// WebTransport server bind address (default: "0.0.0.0:4434").
    pub webtransport_bind_address: String,

    /// Deployment region identifier (e.g., "us-east-1").
    pub region: String,

    /// URL to Global Controller for registration.
    pub gc_grpc_url: String,

    /// Unique identifier for this MH instance.
    pub handler_id: String,

    /// Maximum concurrent streams this MH can handle.
    pub max_streams: u32,

    /// Authentication Controller endpoint for OAuth token acquisition.
    pub ac_endpoint: String,

    /// OAuth client ID for MH (used for client credentials flow to AC).
    pub client_id: String,

    /// OAuth client secret for MH (used for client credentials flow to AC).
    /// Protected by `SecretString` to prevent accidental logging.
    pub client_secret: SecretString,

    /// Path to TLS certificate file (PEM) for WebTransport server.
    pub tls_cert_path: String,

    /// Path to TLS private key file (PEM) for WebTransport server.
    pub tls_key_path: String,

    /// Advertised gRPC address for GC registration.
    /// This is the address GC uses to reach this MH pod (e.g., `grpc://10.244.0.5:50053`).
    /// Required environment variable: `MH_GRPC_ADVERTISE_ADDRESS`.
    pub grpc_advertise_address: String,

    /// Advertised WebTransport address for GC registration.
    /// This is the address GC uses to reach this MH pod (e.g., `https://10.244.0.5:4434`).
    /// Required environment variable: `MH_WEBTRANSPORT_ADVERTISE_ADDRESS`.
    pub webtransport_advertise_address: String,

    /// AC JWKS endpoint URL for JWT validation.
    /// Required environment variable: `AC_JWKS_URL`.
    pub ac_jwks_url: String,

    /// `RegisterMeeting` arrival timeout in seconds (default: 15).
    /// Clients connecting to unregistered meetings are provisionally accepted
    /// for this duration; if `RegisterMeeting` does not arrive, they are disconnected.
    pub register_meeting_timeout_seconds: u64,

    /// Maximum concurrent WebTransport connections (default: 10000).
    pub max_connections: usize,
}

/// Custom Debug implementation that redacts sensitive fields.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("grpc_bind_address", &self.grpc_bind_address)
            .field("health_bind_address", &self.health_bind_address)
            .field("webtransport_bind_address", &self.webtransport_bind_address)
            .field("region", &self.region)
            .field("gc_grpc_url", &self.gc_grpc_url)
            .field("handler_id", &self.handler_id)
            .field("max_streams", &self.max_streams)
            .field("ac_endpoint", &self.ac_endpoint)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("tls_cert_path", &self.tls_cert_path)
            .field("tls_key_path", &self.tls_key_path)
            .field("grpc_advertise_address", &self.grpc_advertise_address)
            .field(
                "webtransport_advertise_address",
                &self.webtransport_advertise_address,
            )
            .field("ac_jwks_url", &self.ac_jwks_url)
            .field(
                "register_meeting_timeout_seconds",
                &self.register_meeting_timeout_seconds,
            )
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingEnvVar` if a required variable is missing.
    /// Returns `ConfigError::InvalidValue` if a value is invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(&env::vars().collect())
    }

    /// Load configuration from a `HashMap` (for testing).
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingEnvVar` if a required variable is missing.
    /// Returns `ConfigError::InvalidValue` if a value is invalid.
    #[expect(
        clippy::too_many_lines,
        reason = "Sequential env var parsing; splitting would obscure config loading flow"
    )]
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let ac_endpoint = vars
            .get("AC_ENDPOINT")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_ENDPOINT".to_string()))?
            .clone();

        let client_id = vars
            .get("MH_CLIENT_ID")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_CLIENT_ID".to_string()))?
            .clone();

        let client_secret = SecretString::from(
            vars.get("MH_CLIENT_SECRET")
                .ok_or_else(|| ConfigError::MissingEnvVar("MH_CLIENT_SECRET".to_string()))?
                .clone(),
        );

        let tls_cert_path = vars
            .get("MH_TLS_CERT_PATH")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_TLS_CERT_PATH".to_string()))?
            .clone();

        let tls_key_path = vars
            .get("MH_TLS_KEY_PATH")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_TLS_KEY_PATH".to_string()))?
            .clone();

        // Validate TLS cert and key files exist at startup (fail-fast)
        if !std::path::Path::new(&tls_cert_path).exists() {
            return Err(ConfigError::InvalidValue(format!(
                "MH_TLS_CERT_PATH file does not exist: {tls_cert_path}"
            )));
        }
        if !std::path::Path::new(&tls_key_path).exists() {
            return Err(ConfigError::InvalidValue(format!(
                "MH_TLS_KEY_PATH file does not exist: {tls_key_path}"
            )));
        }

        let grpc_advertise_address = vars
            .get("MH_GRPC_ADVERTISE_ADDRESS")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_GRPC_ADVERTISE_ADDRESS".to_string()))?
            .clone();

        let webtransport_advertise_address = vars
            .get("MH_WEBTRANSPORT_ADVERTISE_ADDRESS")
            .ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_WEBTRANSPORT_ADVERTISE_ADDRESS".to_string())
            })?
            .clone();

        let grpc_bind_address = vars
            .get("MH_GRPC_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_GRPC_BIND_ADDRESS.to_string());

        let health_bind_address = vars
            .get("MH_HEALTH_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_HEALTH_BIND_ADDRESS.to_string());

        let webtransport_bind_address = vars
            .get("MH_WEBTRANSPORT_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_WEBTRANSPORT_BIND_ADDRESS.to_string());

        let region = vars
            .get("MH_REGION")
            .cloned()
            .unwrap_or_else(|| "us-east-1".to_string());

        let gc_grpc_url = vars
            .get("GC_GRPC_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:50051".to_string());

        let max_streams = vars
            .get("MH_MAX_STREAMS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_STREAMS);

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

        let register_meeting_timeout_seconds = vars
            .get("MH_REGISTER_MEETING_TIMEOUT_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_REGISTER_MEETING_TIMEOUT_SECONDS)
            .min(MAX_REGISTER_MEETING_TIMEOUT_SECONDS);

        let max_connections = vars
            .get("MH_MAX_CONNECTIONS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_CONNECTIONS);

        // Generate MH instance ID
        let handler_id = vars.get("MH_HANDLER_ID").cloned().unwrap_or_else(|| {
            let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
            let uuid_suffix = uuid::Uuid::new_v4().to_string();
            let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
            format!("{DEFAULT_MH_ID_PREFIX}-{hostname}-{short_suffix}")
        });

        Ok(Config {
            grpc_bind_address,
            health_bind_address,
            webtransport_bind_address,
            region,
            gc_grpc_url,
            handler_id,
            max_streams,
            ac_endpoint,
            client_id,
            client_secret,
            tls_cert_path,
            tls_key_path,
            grpc_advertise_address,
            webtransport_advertise_address,
            ac_jwks_url,
            register_meeting_timeout_seconds,
            max_connections,
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
                "AC_ENDPOINT".to_string(),
                "http://localhost:8082".to_string(),
            ),
            ("MH_CLIENT_ID".to_string(), "media-handler".to_string()),
            (
                "MH_CLIENT_SECRET".to_string(),
                "media-handler-secret-dev-003".to_string(),
            ),
            ("MH_TLS_CERT_PATH".to_string(), "/dev/null".to_string()),
            ("MH_TLS_KEY_PATH".to_string(), "/dev/null".to_string()),
            (
                "MH_GRPC_ADVERTISE_ADDRESS".to_string(),
                "grpc://localhost:50053".to_string(),
            ),
            (
                "MH_WEBTRANSPORT_ADVERTISE_ADDRESS".to_string(),
                "https://localhost:4434".to_string(),
            ),
            (
                "AC_JWKS_URL".to_string(),
                "http://localhost:8082/.well-known/jwks.json".to_string(),
            ),
        ])
    }

    #[test]
    fn test_from_vars_success_with_defaults() {
        let vars = base_vars();

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.grpc_bind_address, DEFAULT_GRPC_BIND_ADDRESS);
        assert_eq!(config.health_bind_address, DEFAULT_HEALTH_BIND_ADDRESS);
        assert_eq!(
            config.webtransport_bind_address,
            DEFAULT_WEBTRANSPORT_BIND_ADDRESS
        );
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.gc_grpc_url, "http://localhost:50051");
        assert_eq!(config.max_streams, DEFAULT_MAX_STREAMS);
        assert!(config.handler_id.starts_with("mh-"));
        assert_eq!(config.ac_endpoint, "http://localhost:8082");
        assert_eq!(config.client_id, "media-handler");
        assert_eq!(
            config.client_secret.expose_secret(),
            "media-handler-secret-dev-003"
        );
        assert_eq!(config.grpc_advertise_address, "grpc://localhost:50053");
        assert_eq!(
            config.webtransport_advertise_address,
            "https://localhost:4434"
        );
        assert_eq!(
            config.ac_jwks_url,
            "http://localhost:8082/.well-known/jwks.json"
        );
        assert_eq!(
            config.register_meeting_timeout_seconds,
            DEFAULT_REGISTER_MEETING_TIMEOUT_SECONDS
        );
        assert_eq!(config.max_connections, DEFAULT_MAX_CONNECTIONS);
    }

    #[test]
    fn test_from_vars_success_with_custom_values() {
        let mut vars = base_vars();
        vars.insert(
            "MH_GRPC_BIND_ADDRESS".to_string(),
            "127.0.0.1:50054".to_string(),
        );
        vars.insert(
            "MH_HEALTH_BIND_ADDRESS".to_string(),
            "127.0.0.1:8084".to_string(),
        );
        vars.insert(
            "MH_WEBTRANSPORT_BIND_ADDRESS".to_string(),
            "127.0.0.1:4435".to_string(),
        );
        vars.insert("MH_REGION".to_string(), "eu-west-1".to_string());
        vars.insert("GC_GRPC_URL".to_string(), "http://gc:50051".to_string());
        vars.insert("MH_MAX_STREAMS".to_string(), "500".to_string());
        vars.insert("MH_HANDLER_ID".to_string(), "mh-custom-001".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.grpc_bind_address, "127.0.0.1:50054");
        assert_eq!(config.health_bind_address, "127.0.0.1:8084");
        assert_eq!(config.webtransport_bind_address, "127.0.0.1:4435");
        assert_eq!(config.region, "eu-west-1");
        assert_eq!(config.gc_grpc_url, "http://gc:50051");
        assert_eq!(config.max_streams, 500);
        assert_eq!(config.handler_id, "mh-custom-001");
    }

    #[test]
    fn test_handler_id_custom_value() {
        let mut vars = base_vars();
        vars.insert("MH_HANDLER_ID".to_string(), "mh-us-east-1-001".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.handler_id, "mh-us-east-1-001");
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
        vars.remove("MH_CLIENT_ID");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_CLIENT_ID"));
    }

    #[test]
    fn test_from_vars_missing_client_secret() {
        let mut vars = base_vars();
        vars.remove("MH_CLIENT_SECRET");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_CLIENT_SECRET"));
    }

    #[test]
    fn test_from_vars_missing_tls_cert_path() {
        let mut vars = base_vars();
        vars.remove("MH_TLS_CERT_PATH");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_TLS_CERT_PATH"));
    }

    #[test]
    fn test_from_vars_missing_tls_key_path() {
        let mut vars = base_vars();
        vars.remove("MH_TLS_KEY_PATH");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_TLS_KEY_PATH"));
    }

    #[test]
    fn test_from_vars_tls_cert_path_nonexistent() {
        let mut vars = base_vars();
        vars.insert(
            "MH_TLS_CERT_PATH".to_string(),
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
            "MH_TLS_KEY_PATH".to_string(),
            "/nonexistent/key.pem".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidValue(msg)) if msg.contains("does not exist"))
        );
    }

    #[test]
    fn test_debug_redacts_sensitive_fields() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        let debug_output = format!("{config:?}");

        // Sensitive fields should be redacted
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("media-handler-secret-dev-003"));
        // Non-sensitive fields should be visible
        assert!(debug_output.contains("media-handler"));
        assert!(debug_output.contains("http://localhost:8082"));
    }

    #[test]
    fn test_oauth_config_loaded_correctly() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.ac_endpoint, "http://localhost:8082");
        assert_eq!(config.client_id, "media-handler");
        assert_eq!(
            config.client_secret.expose_secret(),
            "media-handler-secret-dev-003"
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
        vars.remove("MH_GRPC_ADVERTISE_ADDRESS");

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_GRPC_ADVERTISE_ADDRESS")
        );
    }

    #[test]
    fn test_from_vars_missing_webtransport_advertise_address() {
        let mut vars = base_vars();
        vars.remove("MH_WEBTRANSPORT_ADVERTISE_ADDRESS");

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_WEBTRANSPORT_ADVERTISE_ADDRESS")
        );
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
        vars.insert(
            "AC_JWKS_URL".to_string(),
            "ftp://localhost:8082/.well-known/jwks.json".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(&result, Err(ConfigError::InvalidValue(msg)) if msg.contains("http://") && msg.contains("https://")),
            "Expected InvalidValue error for non-http scheme, got {result:?}"
        );
    }

    #[test]
    fn test_register_meeting_timeout_custom_value() {
        let mut vars = base_vars();
        vars.insert(
            "MH_REGISTER_MEETING_TIMEOUT_SECONDS".to_string(),
            "30".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.register_meeting_timeout_seconds, 30);
    }

    #[test]
    fn test_register_meeting_timeout_clamped_to_max() {
        let mut vars = base_vars();
        vars.insert(
            "MH_REGISTER_MEETING_TIMEOUT_SECONDS".to_string(),
            "999999".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.register_meeting_timeout_seconds,
            MAX_REGISTER_MEETING_TIMEOUT_SECONDS,
        );
    }

    #[test]
    fn test_max_connections_custom_value() {
        let mut vars = base_vars();
        vars.insert("MH_MAX_CONNECTIONS".to_string(), "5000".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.max_connections, 5000);
    }
}
