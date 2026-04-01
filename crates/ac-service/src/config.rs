use base64::{engine::general_purpose, Engine as _};
// Re-export clock skew constants from common for backwards compatibility
pub use common::jwt::{
    DEFAULT_CLOCK_SKEW as DEFAULT_JWT_CLOCK_SKEW, MAX_CLOCK_SKEW as MAX_JWT_CLOCK_SKEW,
};
use common::secret::{ExposeSecret, SecretBox};
use std::collections::HashMap;
use std::env;
use std::fmt;
use thiserror::Error;
use tracing::warn;

/// Default bcrypt cost factor (12 per ADR-0003).
///
/// Cost 12 = 2^12 = 4,096 iterations, providing appropriate security
/// for 2024+ per OWASP guidelines. Approximate hash time: ~200ms.
pub const DEFAULT_BCRYPT_COST: u32 = 12;

/// Minimum allowed bcrypt cost factor (10 per OWASP 2024 guidelines).
///
/// Cost < 10 is considered insecure by modern standards.
/// Cost 10 = 2^10 = 1,024 iterations, approximate hash time: ~50ms.
pub const MIN_BCRYPT_COST: u32 = 10;

/// Maximum allowed bcrypt cost factor (14 to prevent excessive latency).
///
/// Cost > 14 causes unacceptable latency for authentication endpoints.
/// Cost 14 = 2^14 = 16,384 iterations, approximate hash time: ~800ms.
pub const MAX_BCRYPT_COST: u32 = 14;

// =============================================================================
// Rate Limit Configuration Defaults & Bounds
// =============================================================================

/// Default login rate limit window in minutes (15-minute sliding window per ADR-0003).
pub const DEFAULT_RATE_LIMIT_WINDOW_MINUTES: i64 = 15;
/// Minimum login rate limit window (1 minute).
pub const MIN_RATE_LIMIT_WINDOW_MINUTES: i64 = 1;
/// Maximum login rate limit window (60 minutes / 1 hour).
pub const MAX_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;

/// Default login rate limit max attempts before lockout (5 per ADR-0003).
pub const DEFAULT_RATE_LIMIT_MAX_ATTEMPTS: i64 = 5;
/// Minimum login rate limit max attempts (1).
pub const MIN_RATE_LIMIT_MAX_ATTEMPTS: i64 = 1;
/// Maximum login rate limit max attempts (100).
pub const MAX_RATE_LIMIT_MAX_ATTEMPTS: i64 = 100;

/// Default registration rate limit window in minutes (60 minutes / 1 hour).
pub const DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES: i64 = 60;
/// Minimum registration rate limit window (1 minute).
pub const MIN_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES: i64 = 1;
/// Maximum registration rate limit window (1440 minutes / 24 hours).
pub const MAX_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES: i64 = 1440;

/// Default registration rate limit max attempts (5 per IP per window).
pub const DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS: i64 = 5;
/// Minimum registration rate limit max attempts (1).
pub const MIN_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS: i64 = 1;
/// Maximum registration rate limit max attempts (100).
pub const MAX_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS: i64 = 100;

/// Application configuration with secure handling of sensitive fields.
///
/// Sensitive fields (`master_key`, `hash_secret`) are wrapped in `SecretBox`
/// which provides:
/// - Automatic redaction in Debug output (prevents accidental logging)
/// - Explicit `.expose_secret()` required to access values
/// - Zeroization on drop (when using zeroize feature)
///
/// The `database_url` is also redacted in Debug output as it may contain
/// credentials in the connection string.
///
/// Clone is manually implemented since SecretBox requires explicit cloning.
pub struct Config {
    pub database_url: String,
    pub bind_address: String,
    /// AES-256 master key for encrypting private keys at rest.
    /// Must be exactly 32 bytes. Use `.expose_secret()` to access.
    pub master_key: SecretBox<Vec<u8>>,
    /// HMAC-SHA256 secret for correlation ID hashing.
    /// Must be at least 32 bytes. Use `.expose_secret()` to access.
    pub hash_secret: SecretBox<Vec<u8>>,
    #[allow(dead_code)] // Will be used in Phase 4 for observability
    pub otlp_endpoint: Option<String>,
    /// JWT clock skew tolerance in seconds for `iat` validation.
    /// Per NIST SP 800-63B: Clock synchronization should be maintained within
    /// reasonable bounds (typically 5 minutes) for time-based security controls.
    pub jwt_clock_skew_seconds: i64,
    /// Bcrypt cost factor for password hashing.
    /// Per ADR-0003 and OWASP 2024 guidelines: cost 10-14 is recommended.
    /// Default: 12 (2^12 = 4,096 iterations, ~200ms hash time).
    /// Minimum: 10 (security floor per OWASP 2024).
    /// Maximum: 14 (prevents excessive latency ~800ms).
    pub bcrypt_cost: u32,
    /// Login rate limit sliding window in minutes.
    /// Default: 15 (per ADR-0003). Range: 1-60.
    pub rate_limit_window_minutes: i64,
    /// Login rate limit max failed attempts before lockout.
    /// Default: 5 (per ADR-0003). Range: 1-100.
    pub rate_limit_max_attempts: i64,
    /// Registration rate limit sliding window in minutes.
    /// Default: 60. Range: 1-1440.
    pub registration_rate_limit_window_minutes: i64,
    /// Registration rate limit max attempts per IP per window.
    /// Default: 5. Range: 1-100.
    pub registration_rate_limit_max_attempts: i64,
}

/// Clone implementation that explicitly clones SecretBox fields.
impl Clone for Config {
    fn clone(&self) -> Self {
        Self {
            database_url: self.database_url.clone(),
            bind_address: self.bind_address.clone(),
            master_key: SecretBox::new(Box::new(self.master_key.expose_secret().clone())),
            hash_secret: SecretBox::new(Box::new(self.hash_secret.expose_secret().clone())),
            otlp_endpoint: self.otlp_endpoint.clone(),
            jwt_clock_skew_seconds: self.jwt_clock_skew_seconds,
            bcrypt_cost: self.bcrypt_cost,
            rate_limit_window_minutes: self.rate_limit_window_minutes,
            rate_limit_max_attempts: self.rate_limit_max_attempts,
            registration_rate_limit_window_minutes: self.registration_rate_limit_window_minutes,
            registration_rate_limit_max_attempts: self.registration_rate_limit_max_attempts,
        }
    }
}

/// Custom Debug implementation that redacts sensitive fields.
///
/// Redacted fields:
/// - `master_key`: Cryptographic key material
/// - `hash_secret`: HMAC secret key
/// - `database_url`: May contain credentials in connection string
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"[REDACTED]")
            .field("bind_address", &self.bind_address)
            .field("master_key", &"[REDACTED]")
            .field("hash_secret", &"[REDACTED]")
            .field("otlp_endpoint", &self.otlp_endpoint)
            .field("jwt_clock_skew_seconds", &self.jwt_clock_skew_seconds)
            .field("bcrypt_cost", &self.bcrypt_cost)
            .field("rate_limit_window_minutes", &self.rate_limit_window_minutes)
            .field("rate_limit_max_attempts", &self.rate_limit_max_attempts)
            .field(
                "registration_rate_limit_window_minutes",
                &self.registration_rate_limit_window_minutes,
            )
            .field(
                "registration_rate_limit_max_attempts",
                &self.registration_rate_limit_max_attempts,
            )
            .finish()
    }
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

    #[error("Invalid bcrypt cost configuration: {0}")]
    InvalidBcryptCost(String),

    #[error("Invalid rate limit configuration: {0}")]
    InvalidRateLimitConfig(String),
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

            if value > MAX_JWT_CLOCK_SKEW.as_secs() as i64 {
                return Err(ConfigError::InvalidJwtClockSkew(format!(
                    "JWT_CLOCK_SKEW_SECONDS must not exceed {} seconds (10 minutes), got {}",
                    MAX_JWT_CLOCK_SKEW.as_secs(),
                    value
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
            DEFAULT_JWT_CLOCK_SKEW.as_secs() as i64
        };

        // Parse bcrypt cost factor with validation
        // Default: 12 (ADR-0003)
        // Min: 10 (OWASP 2024 security floor)
        // Max: 14 (prevent excessive latency)
        let bcrypt_cost = if let Some(value_str) = vars.get("BCRYPT_COST") {
            let value: u32 = value_str.parse().map_err(|e| {
                ConfigError::InvalidBcryptCost(format!(
                    "BCRYPT_COST must be a valid positive integer, got '{}': {}",
                    value_str, e
                ))
            })?;

            if value < MIN_BCRYPT_COST {
                return Err(ConfigError::InvalidBcryptCost(format!(
                    "BCRYPT_COST must be at least {} (OWASP 2024 security floor), got {}",
                    MIN_BCRYPT_COST, value
                )));
            }

            if value > MAX_BCRYPT_COST {
                return Err(ConfigError::InvalidBcryptCost(format!(
                    "BCRYPT_COST must not exceed {} (prevents excessive latency), got {}",
                    MAX_BCRYPT_COST, value
                )));
            }

            // Warn if cost is below recommended default of 12
            // Cost 10-11 is secure but provides less protection against brute force
            if value < DEFAULT_BCRYPT_COST {
                warn!(
                    bcrypt_cost = value,
                    default = DEFAULT_BCRYPT_COST,
                    "BCRYPT_COST is below recommended default of {}. \
                     This reduces protection against brute force attacks.",
                    DEFAULT_BCRYPT_COST
                );
            }

            value
        } else {
            DEFAULT_BCRYPT_COST
        };

        // Parse rate limit configuration
        let rate_limit_window_minutes = Self::parse_rate_limit_i64(
            vars,
            "AC_RATE_LIMIT_WINDOW_MINUTES",
            DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
            MIN_RATE_LIMIT_WINDOW_MINUTES,
            MAX_RATE_LIMIT_WINDOW_MINUTES,
        )?;

        let rate_limit_max_attempts = Self::parse_rate_limit_i64(
            vars,
            "AC_RATE_LIMIT_MAX_ATTEMPTS",
            DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
            MIN_RATE_LIMIT_MAX_ATTEMPTS,
            MAX_RATE_LIMIT_MAX_ATTEMPTS,
        )?;

        let registration_rate_limit_window_minutes = Self::parse_rate_limit_i64(
            vars,
            "AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES",
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            MIN_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
            MAX_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
        )?;

        let registration_rate_limit_max_attempts = Self::parse_rate_limit_i64(
            vars,
            "AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS",
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            MIN_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
            MAX_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
        )?;

        // Warn on non-default rate limit values
        if rate_limit_window_minutes != DEFAULT_RATE_LIMIT_WINDOW_MINUTES {
            warn!(
                rate_limit_window_minutes,
                default = DEFAULT_RATE_LIMIT_WINDOW_MINUTES,
                "AC_RATE_LIMIT_WINDOW_MINUTES differs from default of {} minutes. \
                 Shorter windows increase false-positive lockouts; longer windows delay recovery.",
                DEFAULT_RATE_LIMIT_WINDOW_MINUTES
            );
        }
        if rate_limit_max_attempts != DEFAULT_RATE_LIMIT_MAX_ATTEMPTS {
            warn!(
                rate_limit_max_attempts,
                default = DEFAULT_RATE_LIMIT_MAX_ATTEMPTS,
                "AC_RATE_LIMIT_MAX_ATTEMPTS differs from default of {}. \
                 Lower values may lock out legitimate users; higher values reduce brute force protection.",
                DEFAULT_RATE_LIMIT_MAX_ATTEMPTS
            );
        }
        if registration_rate_limit_window_minutes != DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES
        {
            warn!(
                registration_rate_limit_window_minutes,
                default = DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES,
                "AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES differs from default of {} minutes.",
                DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES
            );
        }
        if registration_rate_limit_max_attempts != DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS {
            warn!(
                registration_rate_limit_max_attempts,
                default = DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS,
                "AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS differs from default of {}. \
                 Higher values reduce registration abuse protection.",
                DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS
            );
        }

        // ADR-0012: Validate TLS configuration for PostgreSQL
        // Production deployments should use sslmode=verify-full
        // Allow non-TLS for local development but warn
        Self::validate_tls_config(&database_url);

        Ok(Config {
            database_url,
            bind_address,
            master_key: SecretBox::new(Box::new(master_key)),
            hash_secret: SecretBox::new(Box::new(hash_secret)),
            otlp_endpoint,
            jwt_clock_skew_seconds,
            bcrypt_cost,
            rate_limit_window_minutes,
            rate_limit_max_attempts,
            registration_rate_limit_window_minutes,
            registration_rate_limit_max_attempts,
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

    /// Parse an i64 rate limit config value with min/max bounds validation.
    fn parse_rate_limit_i64(
        vars: &HashMap<String, String>,
        env_var: &str,
        default: i64,
        min: i64,
        max: i64,
    ) -> Result<i64, ConfigError> {
        let value = if let Some(value_str) = vars.get(env_var) {
            let v: i64 = value_str.parse().map_err(|e| {
                ConfigError::InvalidRateLimitConfig(format!(
                    "{} must be a valid integer, got '{}': {}",
                    env_var, value_str, e
                ))
            })?;

            if v < min {
                return Err(ConfigError::InvalidRateLimitConfig(format!(
                    "{} must be at least {}, got {}",
                    env_var, min, v
                )));
            }

            if v > max {
                return Err(ConfigError::InvalidRateLimitConfig(format!(
                    "{} must not exceed {}, got {}",
                    env_var, max, v
                )));
            }

            v
        } else {
            default
        };

        Ok(value)
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::assertions_on_constants)]
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
        assert_eq!(config.master_key.expose_secret().len(), 32);
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
        assert_eq!(config.hash_secret.expose_secret().len(), 32);
        assert_eq!(config.hash_secret.expose_secret(), &vec![0u8; 32]);
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
        assert_eq!(config.hash_secret.expose_secret().len(), 32);
        assert_eq!(config.hash_secret.expose_secret(), &vec![1u8; 32]);
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
        assert_eq!(config.hash_secret.expose_secret().len(), 64);
        assert_eq!(config.hash_secret.expose_secret(), &vec![2u8; 64]);
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
            config.jwt_clock_skew_seconds,
            DEFAULT_JWT_CLOCK_SKEW.as_secs() as i64,
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

    // ============================================================================
    // Bcrypt Cost Configuration Tests
    // ============================================================================

    #[test]
    fn test_bcrypt_cost_default_value() {
        // When BCRYPT_COST is not set, default to 12 (ADR-0003)
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.bcrypt_cost, DEFAULT_BCRYPT_COST,
            "Default bcrypt cost should be 12 (ADR-0003)"
        );
        assert_eq!(config.bcrypt_cost, 12);
    }

    #[test]
    fn test_bcrypt_cost_custom_value() {
        // When BCRYPT_COST is set, use the provided value
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "13".to_string()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.bcrypt_cost, 13,
            "Bcrypt cost should be 13 when configured"
        );
    }

    #[test]
    fn test_bcrypt_cost_min_allowed_value() {
        // 10 is the minimum allowed value (OWASP 2024 security floor)
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "10".to_string()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.bcrypt_cost, 10,
            "Bcrypt cost of 10 should be allowed (OWASP minimum)"
        );
    }

    #[test]
    fn test_bcrypt_cost_max_allowed_value() {
        // 14 is the maximum allowed value (prevents excessive latency)
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "14".to_string()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.bcrypt_cost, 14,
            "Bcrypt cost of 14 should be allowed (maximum before excessive latency)"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_too_low() {
        // Values less than 10 are insecure per OWASP 2024
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "9".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must be at least 10")),
            "Bcrypt cost < 10 should be rejected (insecure per OWASP 2024)"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_zero() {
        // Zero is not a valid bcrypt cost
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "0".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must be at least 10")),
            "Bcrypt cost of 0 should be rejected"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_too_high() {
        // Values greater than 14 cause excessive latency
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "15".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must not exceed 14")),
            "Bcrypt cost > 14 should be rejected (excessive latency)"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_very_high() {
        // Very high values are rejected
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "31".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must not exceed 14")),
            "Bcrypt cost of 31 should be rejected"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_negative() {
        // Negative values should be rejected (u32 parse fails)
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "-5".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must be a valid positive integer")),
            "Negative bcrypt cost should be rejected"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_non_numeric() {
        // Non-numeric values should be rejected
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "twelve".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must be a valid positive integer")),
            "Non-numeric bcrypt cost should be rejected"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_float() {
        // Floating point values should be rejected (must be integer)
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "12.5".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must be a valid positive integer")),
            "Floating point bcrypt cost should be rejected"
        );
    }

    #[test]
    fn test_bcrypt_cost_rejects_empty_string() {
        // Empty string should be rejected
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("BCRYPT_COST".to_string(), "".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidBcryptCost(msg)) if msg.contains("must be a valid positive integer")),
            "Empty string bcrypt cost should be rejected"
        );
    }

    #[test]
    fn test_bcrypt_cost_accepts_all_valid_range() {
        // Test all valid cost values: 10, 11, 12, 13, 14
        for cost in MIN_BCRYPT_COST..=MAX_BCRYPT_COST {
            let vars = HashMap::from([
                (
                    "DATABASE_URL".to_string(),
                    "postgresql://localhost/test".to_string(),
                ),
                ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
                ("BCRYPT_COST".to_string(), cost.to_string()),
            ]);

            let config = Config::from_vars(&vars)
                .unwrap_or_else(|_| panic!("Config should accept bcrypt cost {}", cost));
            assert_eq!(
                config.bcrypt_cost, cost,
                "Bcrypt cost {} should be accepted",
                cost
            );
        }
    }

    #[test]
    fn test_bcrypt_cost_constants_are_valid() {
        // Verify our constants have sensible values
        assert_eq!(DEFAULT_BCRYPT_COST, 12, "Default should be 12 per ADR-0003");
        assert_eq!(MIN_BCRYPT_COST, 10, "Minimum should be 10 per OWASP 2024");
        assert_eq!(
            MAX_BCRYPT_COST, 14,
            "Maximum should be 14 to prevent excessive latency"
        );

        // Verify ordering: MIN <= DEFAULT <= MAX
        assert!(
            MIN_BCRYPT_COST <= DEFAULT_BCRYPT_COST,
            "Default must be >= minimum"
        );
        assert!(
            DEFAULT_BCRYPT_COST <= MAX_BCRYPT_COST,
            "Default must be <= maximum"
        );
    }

    // ============================================================================
    // Rate Limit Configuration Tests
    // ============================================================================

    #[test]
    fn test_rate_limit_defaults() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.rate_limit_window_minutes, 15);
        assert_eq!(config.rate_limit_max_attempts, 5);
        assert_eq!(config.registration_rate_limit_window_minutes, 60);
        assert_eq!(config.registration_rate_limit_max_attempts, 5);
    }

    #[test]
    fn test_rate_limit_custom_values() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_RATE_LIMIT_WINDOW_MINUTES".to_string(), "1".to_string()),
            ("AC_RATE_LIMIT_MAX_ATTEMPTS".to_string(), "100".to_string()),
            (
                "AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES".to_string(),
                "1".to_string(),
            ),
            (
                "AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS".to_string(),
                "100".to_string(),
            ),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.rate_limit_window_minutes, 1);
        assert_eq!(config.rate_limit_max_attempts, 100);
        assert_eq!(config.registration_rate_limit_window_minutes, 1);
        assert_eq!(config.registration_rate_limit_max_attempts, 100);
    }

    #[test]
    fn test_rate_limit_rejects_zero() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_RATE_LIMIT_WINDOW_MINUTES".to_string(), "0".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimitConfig(msg)) if msg.contains("must be at least 1")),
            "Zero rate limit window should be rejected"
        );
    }

    #[test]
    fn test_rate_limit_rejects_exceeds_max() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_RATE_LIMIT_WINDOW_MINUTES".to_string(), "61".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimitConfig(msg)) if msg.contains("must not exceed 60")),
            "Rate limit window > 60 should be rejected"
        );
    }

    #[test]
    fn test_rate_limit_rejects_non_numeric() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_RATE_LIMIT_MAX_ATTEMPTS".to_string(), "five".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimitConfig(msg)) if msg.contains("must be a valid integer")),
            "Non-numeric rate limit should be rejected"
        );
    }

    #[test]
    fn test_rate_limit_rejects_negative() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_RATE_LIMIT_WINDOW_MINUTES".to_string(), "-5".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimitConfig(msg)) if msg.contains("must be at least 1")),
            "Negative rate limit window should be rejected"
        );
    }

    #[test]
    fn test_rate_limit_max_attempts_rejects_exceeds_max() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            ("AC_RATE_LIMIT_MAX_ATTEMPTS".to_string(), "101".to_string()),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimitConfig(msg)) if msg.contains("must not exceed 100")),
            "Max attempts > 100 should be rejected"
        );
    }

    #[test]
    fn test_registration_rate_limit_window_max() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            (
                "AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES".to_string(),
                "1440".to_string(),
            ),
        ]);

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.registration_rate_limit_window_minutes, 1440);
    }

    #[test]
    fn test_registration_rate_limit_window_rejects_exceeds_max() {
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), test_master_key_base64()),
            (
                "AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES".to_string(),
                "1441".to_string(),
            ),
        ]);

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidRateLimitConfig(msg)) if msg.contains("must not exceed 1440")),
            "Registration window > 1440 should be rejected"
        );
    }

    #[test]
    fn test_rate_limit_constants_are_valid() {
        // Verify ordering: MIN <= DEFAULT <= MAX for all rate limit constants
        assert!(MIN_RATE_LIMIT_WINDOW_MINUTES <= DEFAULT_RATE_LIMIT_WINDOW_MINUTES);
        assert!(DEFAULT_RATE_LIMIT_WINDOW_MINUTES <= MAX_RATE_LIMIT_WINDOW_MINUTES);
        assert!(MIN_RATE_LIMIT_MAX_ATTEMPTS <= DEFAULT_RATE_LIMIT_MAX_ATTEMPTS);
        assert!(DEFAULT_RATE_LIMIT_MAX_ATTEMPTS <= MAX_RATE_LIMIT_MAX_ATTEMPTS);
        assert!(
            MIN_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES
                <= DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES
        );
        assert!(
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES
                <= MAX_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES
        );
        assert!(
            MIN_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS
                <= DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS
        );
        assert!(
            DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS
                <= MAX_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS
        );
    }
}
