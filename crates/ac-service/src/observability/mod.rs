//! Observability module for AC service
//!
//! Implements metrics and instrumentation per ADR-0011 (Observability Framework).
//!
//! # Privacy by Default
//!
//! All instrumentation uses `#[instrument(skip_all)]` and explicit safe field allow-listing.
//! Fields are categorized as:
//! - **SAFE**: Can be logged in plaintext (enums, operation types)
//! - **HASHED**: Must be SHA-256 hashed for correlation (client_id)
//! - **NEVER**: Must never appear in logs (secrets, tokens, keys)
//!
//! ## Phase 4 Implementation Note: HMAC Migration
//!
//! ADR-0011 Section 3.4 specifies HMAC-SHA256 with per-service key for correlation hashing.
//! The current implementation uses plain SHA-256, which provides:
//! - ✅ Consistent correlation across log entries
//! - ✅ One-way transformation (not reversible without brute force)
//! - ⚠️ Vulnerable to rainbow table attacks if client_ids are enumerable
//!
//! **Phase 4 Remediation**: Migrate to HMAC-SHA256 with:
//! - Per-service secret key loaded from environment (`AC_HASH_SECRET`)
//! - 30-day key rotation capability
//! - `h:` prefix to distinguish hashed values
//!
//! For development/testing (Phase 3), SHA-256 is sufficient as client_ids are
//! not exposed to external attackers.

pub mod metrics;

// Re-exports for handler-level instrumentation (Phase 3)
// These are used by the handlers module but clippy doesn't see cross-module usage
#[allow(unused_imports)]
pub use metrics::{record_jwks_request, record_key_rotation, record_token_issuance};

use sha2::{Digest, Sha256};

/// Hash a field value for correlation in logs (SHA-256, first 8 hex chars)
///
/// Used for fields like `client_id` that need correlation across log entries
/// but should not be stored in plaintext.
///
/// # Privacy
///
/// This is NOT cryptographically secure for secrets - it's a one-way hash
/// for correlation purposes only. The truncation to 8 chars provides
/// sufficient uniqueness for debugging while limiting reversibility.
///
/// # Phase 4 Migration
///
/// ADR-0011 specifies HMAC-SHA256 with per-service key. Current implementation
/// uses plain SHA-256 which is sufficient for Phase 3 (development/testing).
/// See module-level documentation for migration details.
pub fn hash_for_correlation(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    // Take first 8 hex chars (32 bits) - enough for correlation, limits reversibility
    hex::encode(&result[..4])
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

    #[test]
    fn test_hash_for_correlation_consistency() {
        let value = "test-client-id";
        let hash1 = hash_for_correlation(value);
        let hash2 = hash_for_correlation(value);
        assert_eq!(hash1, hash2, "Same input should produce same hash");
    }

    #[test]
    fn test_hash_for_correlation_uniqueness() {
        let hash1 = hash_for_correlation("client-a");
        let hash2 = hash_for_correlation("client-b");
        assert_ne!(
            hash1, hash2,
            "Different inputs should produce different hashes"
        );
    }

    #[test]
    fn test_hash_for_correlation_length() {
        let hash = hash_for_correlation("any-value");
        assert_eq!(hash.len(), 8, "Hash should be 8 hex characters");
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
        let hash1 = hash_for_correlation("");
        let hash2 = hash_for_correlation("");
        assert_eq!(hash1, hash2, "Empty string should produce consistent hash");
        assert_eq!(hash1.len(), 8, "Hash should be 8 hex characters");
    }

    #[test]
    fn test_hash_for_correlation_unicode() {
        // Edge case: Unicode characters should be handled correctly
        let hash = hash_for_correlation("日本語テスト");
        assert_eq!(hash.len(), 8, "Unicode input should produce 8 hex chars");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should only contain hex digits"
        );
    }

    #[test]
    fn test_hash_for_correlation_hex_format() {
        // Verify output is valid lowercase hex
        let hash = hash_for_correlation("test-value");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should only contain hex digits"
        );
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "Hash should be lowercase hex"
        );
    }
}
