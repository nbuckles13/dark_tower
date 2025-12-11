use base64::{engine::general_purpose, Engine as _};
use std::collections::HashMap;
use std::env;
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_address: String,
    pub master_key: Vec<u8>,
    pub hash_secret: Vec<u8>,
    #[allow(dead_code)] // Will be used in Phase 4 for observability
    pub otlp_endpoint: Option<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Invalid master key format: {0}")]
    InvalidMasterKey(String),

    #[error("Invalid hash secret format: {0}")]
    InvalidHashSecret(String),

    #[error("Base64 decode error: {0}")]
    Base64Error(#[from] base64::DecodeError),
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(&env::vars().collect())
    }

    /// Load configuration from a HashMap (for testing)
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let database_url = vars
            .get("DATABASE_URL")
            .ok_or_else(|| ConfigError::MissingEnvVar("DATABASE_URL".to_string()))?
            .clone();

        let bind_address = vars
            .get("BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| "0.0.0.0:8082".to_string());

        let master_key_base64 = vars
            .get("AC_MASTER_KEY")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_MASTER_KEY".to_string()))?;

        let master_key = general_purpose::STANDARD
            .decode(master_key_base64)
            .map_err(ConfigError::Base64Error)?;

        if master_key.len() != 32 {
            return Err(ConfigError::InvalidMasterKey(format!(
                "Expected 32 bytes, got {}",
                master_key.len()
            )));
        }

        // ADR-0011: Load hash secret for HMAC-SHA256 correlation hashing
        // Default to 32 zero bytes for tests to avoid requiring env var in test environment
        let hash_secret = if let Some(hash_secret_base64) = vars.get("AC_HASH_SECRET") {
            let secret = general_purpose::STANDARD
                .decode(hash_secret_base64)
                .map_err(ConfigError::Base64Error)?;

            if secret.len() < 32 {
                return Err(ConfigError::InvalidHashSecret(format!(
                    "Expected at least 32 bytes, got {}",
                    secret.len()
                )));
            }

            secret
        } else {
            // Default for tests: 32 zero bytes
            // Production MUST set AC_HASH_SECRET via environment
            vec![0u8; 32]
        };

        let otlp_endpoint = vars.get("OTLP_ENDPOINT").cloned();

        // ADR-0012: Validate TLS configuration for PostgreSQL
        // Production deployments should use sslmode=verify-full
        // Allow non-TLS for local development but warn
        Self::validate_tls_config(&database_url);

        Ok(Config {
            database_url,
            bind_address,
            master_key,
            hash_secret,
            otlp_endpoint,
        })
    }

    /// Validates TLS configuration in DATABASE_URL
    /// Warns if sslmode is not set to verify-full (ADR-0012 requirement)
    fn validate_tls_config(database_url: &str) {
        // Skip validation in test mode to avoid tracing initialization issues
        if cfg!(test) {
            return;
        }

        let has_sslmode = database_url.contains("sslmode=");
        let has_verify_full = database_url.contains("sslmode=verify-full");

        if !has_sslmode {
            warn!(
                "DATABASE_URL does not specify sslmode. ADR-0012 requires sslmode=verify-full for production. \
                 This is acceptable for local development, but production deployments MUST use TLS."
            );
        } else if !has_verify_full {
            warn!(
                "DATABASE_URL uses sslmode other than verify-full. ADR-0012 requires sslmode=verify-full \
                 for production deployments to ensure proper certificate validation."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_master_key_base64() -> String {
        general_purpose::STANDARD.encode([0u8; 32])
    }

    #[test]
    fn test_from_vars_success() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("BIND_ADDRESS".to_string(), "127.0.0.1:9000".to_string()),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            (
                "OTLP_ENDPOINT".to_string(),
                "http://localhost:4317".to_string(),
            ),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.database_url, "postgresql://localhost/test");
        assert_eq!(config.bind_address, "127.0.0.1:9000");
        assert_eq!(config.master_key.len(), 32);
        assert_eq!(
            config.otlp_endpoint,
            Some("http://localhost:4317".to_string())
        );
    }

    #[test]
    fn test_from_vars_missing_database_url() {
        let vars = HashMap::from([("AC_MASTER_KEY".to_string(), test_master_key_base64())]);

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "DATABASE_URL"));
    }

    #[test]
    fn test_from_vars_missing_master_key() {
        let vars = HashMap::from([(
            "DATABASE_URL".to_string(),
            "postgresql://localhost/test".to_string(),
        )]);

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "AC_MASTER_KEY"));
    }

    #[test]
    fn test_from_vars_invalid_base64() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            (
                "AC_MASTER_KEY".to_string(),
                "not-valid-base64!@#$".to_string(),
            ),
        ]);

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::Base64Error(_))));
    }

    #[test]
    fn test_from_vars_master_key_too_short() {
        let short_key = general_purpose::STANDARD.encode([0u8; 16]);
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), short_key),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidMasterKey(msg)) if msg.contains("Expected 32 bytes, got 16"))
        );
    }

    #[test]
    fn test_from_vars_master_key_too_long() {
        let long_key = general_purpose::STANDARD.encode([0u8; 64]);
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), long_key),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidMasterKey(msg)) if msg.contains("Expected 32 bytes, got 64"))
        );
    }

    #[test]
    fn test_from_vars_default_bind_address() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.bind_address, "0.0.0.0:8082");
    }

    #[test]
    fn test_from_vars_custom_bind_address() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BIND_ADDRESS".to_string(), "192.168.1.100:3000".to_string()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.bind_address, "192.168.1.100:3000");
    }

    #[test]
    fn test_from_vars_optional_otlp_endpoint() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.otlp_endpoint, None);
    }

    #[test]
    fn test_from_vars_hash_secret_default() {
        // ADR-0011: hash_secret defaults to 32 zero bytes for tests
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.hash_secret.len(), 32);
        assert_eq!(config.hash_secret, vec![0u8; 32]);
    }

    #[test]
    fn test_from_vars_hash_secret_custom() {
        // ADR-0011: hash_secret can be explicitly set via AC_HASH_SECRET
        let hash_secret_base64 = general_purpose::STANDARD.encode([1u8; 32]);
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_HASH_SECRET".to_string(), hash_secret_base64),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.hash_secret.len(), 32);
        assert_eq!(config.hash_secret, vec![1u8; 32]);
    }

    #[test]
    fn test_from_vars_hash_secret_too_short() {
        // ADR-0011: hash_secret must be at least 32 bytes
        let short_secret = general_purpose::STANDARD.encode([0u8; 16]);
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_HASH_SECRET".to_string(), short_secret),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidHashSecret(msg)) if msg.contains("Expected at least 32 bytes, got 16"))
        );
    }

    #[test]
    fn test_from_vars_hash_secret_allows_longer() {
        // ADR-0011: hash_secret can be longer than 32 bytes (HMAC accepts variable-length keys)
        let long_secret = general_purpose::STANDARD.encode([2u8; 64]);
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_HASH_SECRET".to_string(), long_secret),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.hash_secret.len(), 64);
        assert_eq!(config.hash_secret, vec![2u8; 64]);
    }

    #[test]
    fn test_from_vars_hash_secret_invalid_base64() {
        // ADR-0011: hash_secret must be valid base64
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            (
                "AC_HASH_SECRET".to_string(),
                "not-valid-base64!@#$".to_string(),
            ),
        ]);

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::Base64Error(_))));
    }
}
