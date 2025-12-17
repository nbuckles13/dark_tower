//! Observability module for AC service
//!
//! Implements metrics and instrumentation per ADR-0011 (Observability Framework).
//!
//! # Privacy by Default
//!
//! All instrumentation uses `#[instrument(skip_all)]` and explicit safe field allow-listing.
//! Fields are categorized as:
//! - **SAFE**: Can be logged in plaintext (enums, operation types)
//! - **HASHED**: Must be HMAC-SHA256 hashed for correlation (client_id)
//! - **NEVER**: Must never appear in logs (secrets, tokens, keys)
//!
//! ## HMAC-SHA256 Correlation Hashing (ADR-0011 Section 3.4)
//!
//! The correlation hash function uses HMAC-SHA256 with per-service key to prevent
//! rainbow table attacks:
//! - ✅ Consistent correlation across log entries (same input = same hash)
//! - ✅ One-way transformation (not reversible without secret key)
//! - ✅ Resistant to rainbow table attacks (requires per-service secret)
//! - ✅ `h:` prefix distinguishes HMAC hashes from legacy SHA-256 hashes
//!
//! **Configuration**:
//! - Secret key loaded from `AC_HASH_SECRET` environment variable (base64-encoded)
//! - Must be at least 32 bytes
//! - Defaults to 32 zero bytes for tests (production MUST override)

pub mod metrics;

// Re-exports for handler-level instrumentation (Phase 3)
// These are used by the handlers module but clippy doesn't see cross-module usage
#[allow(unused_imports)]
pub use metrics::{
    record_http_request, record_jwks_request, record_key_rotation, record_token_issuance,
};

use ring::hmac;

/// Hash a field value for correlation in logs (HMAC-SHA256, first 8 hex chars)
///
/// Used for fields like `client_id` that need correlation across log entries
/// but should not be stored in plaintext.
///
/// # Privacy
///
/// Uses HMAC-SHA256 with per-service secret key (ADR-0011 Section 3.4) to prevent
/// rainbow table attacks. The truncation to 8 hex chars provides sufficient
/// uniqueness for debugging while limiting reversibility.
///
/// # Implementation
///
/// - Uses `ring::hmac` for HMAC-SHA256
/// - Prefixes output with `h:` to distinguish from legacy SHA-256 hashes
/// - Truncates to 4 bytes (8 hex chars) for correlation
///
/// # Arguments
///
/// * `value` - The string to hash (e.g., client_id)
/// * `secret` - The HMAC secret key (from config.hash_secret)
pub fn hash_for_correlation(value: &str, secret: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let tag = hmac::sign(&key, value.as_bytes());
    // Take first 8 hex chars (32 bits) - enough for correlation, limits reversibility
    // Prefix with "h:" to distinguish from legacy SHA-256 hashes
    // Note: HMAC-SHA256 always produces 32 bytes, so .get(..4) always succeeds
    let tag_bytes = tag.as_ref();
    let prefix = tag_bytes.get(..4).unwrap_or(tag_bytes);
    format!("h:{}", hex::encode(prefix))
}

/// Error categories for metrics labels (bounded cardinality)
///
/// Maps internal error types to 4 categories per debate consensus.
///
/// NOTE: This enum is defined per ADR-0011 for service-layer instrumentation
/// which will be added in Phase 4 (Documentation & Testing).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Authentication failures (invalid credentials, rate limit)
    Authentication,
    /// Authorization failures (insufficient scope)
    Authorization,
    /// Cryptographic errors (invalid token, signature)
    Cryptographic,
    /// Internal errors (database, system)
    Internal,
}

impl ErrorCategory {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::Authentication => "authentication",
            ErrorCategory::Authorization => "authorization",
            ErrorCategory::Cryptographic => "cryptographic",
            ErrorCategory::Internal => "internal",
        }
    }
}

impl From<&crate::errors::AcError> for ErrorCategory {
    fn from(err: &crate::errors::AcError) -> Self {
        use crate::errors::AcError;
        match err {
            AcError::InvalidCredentials
            | AcError::RateLimitExceeded
            | AcError::TooManyRequests { .. } => ErrorCategory::Authentication,
            AcError::InsufficientScope { .. } => ErrorCategory::Authorization,
            AcError::InvalidToken(_) | AcError::Crypto(_) => ErrorCategory::Cryptographic,
            AcError::Database(_) | AcError::Internal => ErrorCategory::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test secret: 32 zero bytes (consistent with config.rs default)
    const TEST_SECRET: &[u8] = &[0u8; 32];

    #[test]
    fn test_hash_for_correlation_consistency() {
        let value = "test-client-id";
        let hash1 = hash_for_correlation(value, TEST_SECRET);
        let hash2 = hash_for_correlation(value, TEST_SECRET);
        assert_eq!(hash1, hash2, "Same input should produce same hash");
    }

    #[test]
    fn test_hash_for_correlation_uniqueness() {
        let hash1 = hash_for_correlation("client-a", TEST_SECRET);
        let hash2 = hash_for_correlation("client-b", TEST_SECRET);
        assert_ne!(
            hash1, hash2,
            "Different inputs should produce different hashes"
        );
    }

    #[test]
    fn test_hash_for_correlation_length() {
        let hash = hash_for_correlation("any-value", TEST_SECRET);
        // Length is 10: "h:" prefix (2) + 8 hex chars (8)
        assert_eq!(hash.len(), 10, "Hash should be 'h:' + 8 hex characters");
        assert!(hash.starts_with("h:"), "Hash should start with 'h:' prefix");
    }

    #[test]
    fn test_error_category_mapping() {
        use crate::errors::AcError;

        assert_eq!(
            ErrorCategory::from(&AcError::InvalidCredentials),
            ErrorCategory::Authentication
        );
        assert_eq!(
            ErrorCategory::from(&AcError::RateLimitExceeded),
            ErrorCategory::Authentication
        );
        assert_eq!(
            ErrorCategory::from(&AcError::TooManyRequests {
                retry_after_seconds: 60,
                message: "test".into()
            }),
            ErrorCategory::Authentication
        );
        assert_eq!(
            ErrorCategory::from(&AcError::InsufficientScope {
                required: "test".into(),
                provided: vec![]
            }),
            ErrorCategory::Authorization
        );
        assert_eq!(
            ErrorCategory::from(&AcError::InvalidToken("test".into())),
            ErrorCategory::Cryptographic
        );
        assert_eq!(
            ErrorCategory::from(&AcError::Internal),
            ErrorCategory::Internal
        );
    }

    #[test]
    fn test_error_category_crypto_variant() {
        use crate::errors::AcError;

        // Test AcError::Crypto variant maps to Cryptographic category
        assert_eq!(
            ErrorCategory::from(&AcError::Crypto("test crypto error".into())),
            ErrorCategory::Cryptographic
        );
    }

    #[test]
    fn test_error_category_database_variant() {
        use crate::errors::AcError;

        // Test AcError::Database variant maps to Internal category
        assert_eq!(
            ErrorCategory::from(&AcError::Database("connection failed".into())),
            ErrorCategory::Internal
        );
    }

    #[test]
    fn test_error_category_as_str() {
        // Test all ErrorCategory variants return correct strings
        assert_eq!(ErrorCategory::Authentication.as_str(), "authentication");
        assert_eq!(ErrorCategory::Authorization.as_str(), "authorization");
        assert_eq!(ErrorCategory::Cryptographic.as_str(), "cryptographic");
        assert_eq!(ErrorCategory::Internal.as_str(), "internal");
    }

    #[test]
    fn test_hash_for_correlation_empty_input() {
        // Edge case: empty string should produce consistent hash
        let hash1 = hash_for_correlation("", TEST_SECRET);
        let hash2 = hash_for_correlation("", TEST_SECRET);
        assert_eq!(hash1, hash2, "Empty string should produce consistent hash");
        assert_eq!(hash1.len(), 10, "Hash should be 'h:' + 8 hex characters");
    }

    #[test]
    fn test_hash_for_correlation_unicode() {
        // Edge case: Unicode characters should be handled correctly
        let hash = hash_for_correlation("日本語テスト", TEST_SECRET);
        assert_eq!(
            hash.len(),
            10,
            "Unicode input should produce 'h:' + 8 hex chars"
        );
        assert!(hash.starts_with("h:"), "Hash should start with 'h:' prefix");
        // Check the hex part after "h:"
        let hex_part = &hash[2..];
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should only contain hex digits after 'h:' prefix"
        );
    }

    #[test]
    fn test_hash_for_correlation_hex_format() {
        // Verify output is valid lowercase hex with h: prefix
        let hash = hash_for_correlation("test-value", TEST_SECRET);
        assert!(hash.starts_with("h:"), "Hash should start with 'h:' prefix");
        let hex_part = &hash[2..];
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should only contain hex digits after 'h:' prefix"
        );
        assert!(
            hex_part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "Hash should be lowercase hex"
        );
    }

    #[test]
    fn test_hash_for_correlation_hmac_consistency() {
        // Verify HMAC produces consistent output for same secret
        let value = "test-client-id";
        let hash1 = hash_for_correlation(value, TEST_SECRET);
        let hash2 = hash_for_correlation(value, TEST_SECRET);
        assert_eq!(hash1, hash2, "HMAC should produce consistent hashes");
    }

    #[test]
    fn test_hash_for_correlation_different_secrets() {
        // Verify different secrets produce different hashes
        let value = "test-client-id";
        let secret1 = [0u8; 32];
        let secret2 = [1u8; 32];
        let hash1 = hash_for_correlation(value, &secret1);
        let hash2 = hash_for_correlation(value, &secret2);
        assert_ne!(
            hash1, hash2,
            "Different secrets should produce different hashes"
        );
    }
}
