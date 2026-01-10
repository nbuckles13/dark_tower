use base64::{engine::general_purpose, Engine as _};
use std::collections::HashMap;
use std::env;
use thiserror::Error;
use tracing::warn;

/// Default JWT clock skew tolerance in seconds (5 minutes per NIST SP 800-63B).
pub const DEFAULT_JWT_CLOCK_SKEW_SECONDS: i64 = 300;

/// Maximum allowed JWT clock skew tolerance in seconds (10 minutes).
/// This prevents misconfiguration that could weaken security.
pub const MAX_JWT_CLOCK_SKEW_SECONDS: i64 = 600;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_address: String,
    pub master_key: Vec<u8>,
    pub hash_secret: Vec<u8>,
    #[allow(dead_code)] // Will be used in Phase 4 for observability
    pub otlp_endpoint: Option<String>,
    /// JWT clock skew tolerance in seconds for `iat` validation.
    /// Per NIST SP 800-63B: Clock synchronization should be maintained within
    /// reasonable bounds (typically 5 minutes) for time-based security controls.
    pub jwt_clock_skew_seconds: i64,
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

    #[error("Invalid JWT clock skew configuration: {0}")]
    InvalidJwtClockSkew(String),
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

        // Parse JWT clock skew tolerance with validation
        // Default: 300 seconds (5 minutes) per NIST SP 800-63B
        // Max: 600 seconds (10 minutes) to prevent security misconfiguration
        let jwt_clock_skew_seconds = if let Some(value_str) = vars.get("JWT_CLOCK_SKEW_SECONDS") {
            let value: i64 = value_str.parse().map_err(|_| {
                ConfigError::InvalidJwtClockSkew(format!(
                    "JWT_CLOCK_SKEW_SECONDS must be a valid integer, got '{}'",
                    value_str
                ))
            })?;

            if value <= 0 {
                return Err(ConfigError::InvalidJwtClockSkew(format!(
                    "JWT_CLOCK_SKEW_SECONDS must be positive, got {}",
                    value
                )));
            }

            if value > MAX_JWT_CLOCK_SKEW_SECONDS {
                return Err(ConfigError::InvalidJwtClockSkew(format!(
                    "JWT_CLOCK_SKEW_SECONDS must not exceed {} seconds (10 minutes), got {}",
                    MAX_JWT_CLOCK_SKEW_SECONDS, value
                )));
            }

            // Warn if clock skew is below recommended minimum (60 seconds)
            // Very low values may cause operational issues with minor clock drift
            if value < 60 {
                warn!(
                    jwt_clock_skew_seconds = value,
                    "JWT_CLOCK_SKEW_SECONDS is below recommended minimum of 60 seconds. \
                     This may cause token rejections due to minor clock drift between servers."
                );
            }

            value
        } else {
            DEFAULT_JWT_CLOCK_SKEW_SECONDS
        };

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
            jwt_clock_skew_seconds,
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

    // ============================================================================
    // JWT Clock Skew Configuration Tests
    // ============================================================================

    #[test]
    fn test_jwt_clock_skew_default_value() {
        // When JWT_CLOCK_SKEW_SECONDS is not set, default to 300 seconds (5 minutes)
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.jwt_clock_skew_seconds, DEFAULT_JWT_CLOCK_SKEW_SECONDS,
            "Default JWT clock skew should be 300 seconds (5 minutes)"
        );
        assert_eq!(config.jwt_clock_skew_seconds, 300);
    }

    #[test]
    fn test_jwt_clock_skew_custom_value() {
        // When JWT_CLOCK_SKEW_SECONDS is set, use the provided value
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "60".to_string()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.jwt_clock_skew_seconds, 60,
            "JWT clock skew should be 60 seconds when configured"
        );
    }

    #[test]
    fn test_jwt_clock_skew_max_allowed_value() {
        // 600 seconds (10 minutes) is the maximum allowed value
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "600".to_string()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.jwt_clock_skew_seconds, 600,
            "JWT clock skew of 600 seconds should be allowed"
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_zero() {
        // Zero is not a valid clock skew value
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "0".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be positive")),
            "Zero clock skew should be rejected"
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_negative() {
        // Negative values are not valid
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "-100".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be positive")),
            "Negative clock skew should be rejected"
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_too_large() {
        // Values greater than 600 seconds (10 minutes) are not allowed
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "601".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must not exceed 600")),
            "Clock skew > 600 seconds should be rejected"
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_very_large() {
        // Very large values are rejected
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "3600".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must not exceed 600")),
            "Clock skew of 3600 seconds should be rejected"
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_non_numeric() {
        // Non-numeric values should be rejected
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            (
                "JWT_CLOCK_SKEW_SECONDS".to_string(),
                "five-minutes".to_string(),
            ),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be a valid integer")),
            "Non-numeric clock skew should be rejected"
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_float() {
        // Floating point values should be rejected (must be integer)
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "300.5".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be a valid integer")),
            "Floating point clock skew should be rejected"
        );
    }

    #[test]
    fn test_jwt_clock_skew_rejects_empty_string() {
        // Empty string should be rejected
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidJwtClockSkew(msg)) if msg.contains("must be a valid integer")),
            "Empty string clock skew should be rejected"
        );
    }

    #[test]
    fn test_jwt_clock_skew_accepts_minimum() {
        // The minimum valid value is 1 second
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("JWT_CLOCK_SKEW_SECONDS".to_string(), "1".to_string()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.jwt_clock_skew_seconds, 1,
            "JWT clock skew of 1 second should be allowed"
        );
    }
}
