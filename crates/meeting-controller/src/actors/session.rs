//! Session binding token generation and validation (ADR-0023).
//!
//! Implements secure binding tokens for session recovery after connection drops:
//!
//! - **Generation**: `HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)`
//! - **Key derivation**: `HKDF-SHA256(master_secret, salt=meeting_id, info="session-binding")`
//! - **Validation**: Constant-time comparison via `ring::constant_time`
//!
//! # Security Properties
//!
//! - One-time nonces prevent replay attacks
//! - HKDF ensures meeting-specific keys
//! - 30-second TTL limits exposure window
//! - Binding tokens are defense-in-depth (also requires valid JWT)

use ring::{hkdf, hmac, rand};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Default binding token TTL (ADR-0023: 30 seconds).
const BINDING_TOKEN_TTL: Duration = Duration::from_secs(30);

/// Session binding token manager.
///
/// Handles generation and validation of binding tokens per ADR-0023 Section 1.
pub struct SessionBindingManager {
    /// Master secret for HKDF key derivation.
    master_secret: Vec<u8>,
}

impl SessionBindingManager {
    /// Create a new session binding manager with the given master secret.
    ///
    /// # Arguments
    ///
    /// * `master_secret` - Must be at least 32 bytes for security.
    ///
    /// # Panics
    ///
    /// Panics if master_secret is less than 32 bytes (security requirement).
    #[must_use]
    pub fn new(master_secret: Vec<u8>) -> Self {
        assert!(
            master_secret.len() >= 32,
            "Master secret must be at least 32 bytes"
        );
        Self { master_secret }
    }

    /// Generate a new binding token for a session.
    ///
    /// Per ADR-0023 Section 1:
    /// ```text
    /// meeting_key = HKDF-SHA256(master_secret, salt=meeting_id, info="session-binding")
    /// binding_token = HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)
    /// ```
    ///
    /// # Arguments
    ///
    /// * `meeting_id` - Meeting identifier (used as HKDF salt)
    /// * `correlation_id` - Correlation ID for reconnection
    /// * `participant_id` - Participant identifier
    ///
    /// # Returns
    ///
    /// A tuple of (binding_token_hex, nonce_hex).
    #[must_use]
    #[allow(clippy::expect_used)] // ADR-0002: CSPRNG fill is an unreachable invariant
    pub fn generate_token(
        &self,
        meeting_id: &str,
        correlation_id: &str,
        participant_id: &str,
    ) -> (String, String) {
        // Generate random nonce
        // ADR-0002: CSPRNG fill on 16 bytes is an unreachable failure condition
        // SystemRandom uses OS-level entropy sources (getrandom/urandom) which
        // only fail if the OS itself is catastrophically broken
        let rng = rand::SystemRandom::new();
        let mut nonce_bytes = [0u8; 16];
        rand::SecureRandom::fill(&rng, &mut nonce_bytes)
            .expect("CSPRNG should not fail on 16 bytes");
        let nonce = hex::encode(nonce_bytes);

        // Derive meeting-specific key via HKDF
        let meeting_key = self.derive_meeting_key(meeting_id);

        // Compute HMAC: correlation_id || participant_id || nonce
        let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &meeting_key);
        let message = format!("{}{}{}", correlation_id, participant_id, nonce);
        let tag = hmac::sign(&hmac_key, message.as_bytes());

        (hex::encode(tag.as_ref()), nonce)
    }

    /// Validate a binding token.
    ///
    /// Performs constant-time comparison to prevent timing attacks.
    ///
    /// # Arguments
    ///
    /// * `meeting_id` - Meeting identifier
    /// * `correlation_id` - Correlation ID from reconnect request
    /// * `participant_id` - Participant ID
    /// * `nonce` - Nonce from original token generation
    /// * `binding_token` - Token to validate (hex-encoded)
    ///
    /// # Returns
    ///
    /// `true` if the token is valid, `false` otherwise.
    #[must_use]
    pub fn validate_token(
        &self,
        meeting_id: &str,
        correlation_id: &str,
        participant_id: &str,
        nonce: &str,
        binding_token: &str,
    ) -> bool {
        // Derive meeting-specific key via HKDF
        let meeting_key = self.derive_meeting_key(meeting_id);

        // Create HMAC key
        let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &meeting_key);
        let message = format!("{}{}{}", correlation_id, participant_id, nonce);

        // Decode the provided token
        let provided_bytes = match hex::decode(binding_token) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };

        // Constant-time comparison using hmac::verify
        // This validates by re-computing the HMAC and comparing via constant-time eq
        hmac::verify(&hmac_key, message.as_bytes(), &provided_bytes).is_ok()
    }

    /// Derive a meeting-specific key using HKDF-SHA256.
    ///
    /// Per ADR-0023:
    /// ```text
    /// meeting_key = HKDF-SHA256(
    ///     ikm: master_secret,
    ///     salt: meeting_id,
    ///     info: b"session-binding"
    /// )
    /// ```
    ///
    /// # ADR-0002 Compliance
    ///
    /// The expect() calls here are unreachable invariants:
    /// - HKDF expand with fixed info and 32-byte output cannot fail
    /// - fill() with matching array size cannot fail
    #[allow(clippy::expect_used)]
    fn derive_meeting_key(&self, meeting_id: &str) -> [u8; 32] {
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, meeting_id.as_bytes());
        let prk = salt.extract(&self.master_secret);
        let okm = prk
            .expand(&[b"session-binding"], MeetingKeyLen)
            .expect("HKDF expand with fixed info and 32-byte output cannot fail");

        let mut key = [0u8; 32];
        okm.fill(&mut key)
            .expect("fill with matching array size cannot fail");
        key
    }
}

/// HKDF output key length for meeting keys.
struct MeetingKeyLen;

impl hkdf::KeyType for MeetingKeyLen {
    fn len(&self) -> usize {
        32
    }
}

/// Stored binding information for a participant session.
#[derive(Debug)]
pub struct StoredBinding {
    /// Correlation ID for this binding.
    pub correlation_id: String,
    /// Participant ID.
    pub participant_id: String,
    /// User ID from JWT (for validation).
    pub user_id: String,
    /// Nonce used for this binding.
    pub nonce: String,
    /// Binding token (hex-encoded).
    pub binding_token: String,
    /// When this binding was created.
    pub created_at: Instant,
}

impl StoredBinding {
    /// Create a new stored binding.
    #[must_use]
    pub fn new(
        correlation_id: String,
        participant_id: String,
        user_id: String,
        nonce: String,
        binding_token: String,
    ) -> Self {
        Self {
            correlation_id,
            participant_id,
            user_id,
            nonce,
            binding_token,
            created_at: Instant::now(),
        }
    }

    /// Check if the binding has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > BINDING_TOKEN_TTL
    }

    /// Generate a new correlation ID.
    #[must_use]
    pub fn generate_correlation_id() -> String {
        Uuid::new_v4().to_string()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_manager() -> SessionBindingManager {
        SessionBindingManager::new(vec![0u8; 32])
    }

    #[test]
    fn test_generate_token_returns_valid_hex() {
        let manager = test_manager();
        let (token, nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        // Token should be valid hex (HMAC-SHA256 = 32 bytes = 64 hex chars)
        assert_eq!(token.len(), 64);
        assert!(hex::decode(&token).is_ok());

        // Nonce should be valid hex (16 bytes = 32 hex chars)
        assert_eq!(nonce.len(), 32);
        assert!(hex::decode(&nonce).is_ok());
    }

    #[test]
    fn test_validate_token_success() {
        let manager = test_manager();
        let (token, nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        let valid = manager.validate_token("meeting-1", "corr-1", "part-1", &nonce, &token);
        assert!(valid);
    }

    #[test]
    fn test_validate_token_wrong_correlation_id() {
        let manager = test_manager();
        let (token, nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        let valid = manager.validate_token("meeting-1", "corr-2", "part-1", &nonce, &token);
        assert!(!valid);
    }

    #[test]
    fn test_validate_token_wrong_participant_id() {
        let manager = test_manager();
        let (token, nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        let valid = manager.validate_token("meeting-1", "corr-1", "part-2", &nonce, &token);
        assert!(!valid);
    }

    #[test]
    fn test_validate_token_wrong_nonce() {
        let manager = test_manager();
        let (token, _nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        let valid =
            manager.validate_token("meeting-1", "corr-1", "part-1", "wrong-nonce-value", &token);
        assert!(!valid);
    }

    #[test]
    fn test_validate_token_wrong_meeting_id() {
        let manager = test_manager();
        let (token, nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        // Different meeting ID = different derived key = different expected HMAC
        let valid = manager.validate_token("meeting-2", "corr-1", "part-1", &nonce, &token);
        assert!(!valid);
    }

    #[test]
    fn test_validate_token_invalid_hex() {
        let manager = test_manager();
        let (_, nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        let valid =
            manager.validate_token("meeting-1", "corr-1", "part-1", &nonce, "not-valid-hex");
        assert!(!valid);
    }

    #[test]
    fn test_validate_token_wrong_length() {
        let manager = test_manager();
        let (_, nonce) = manager.generate_token("meeting-1", "corr-1", "part-1");

        // Valid hex but wrong length (too short)
        let valid = manager.validate_token("meeting-1", "corr-1", "part-1", &nonce, "abcd1234");
        assert!(!valid);
    }

    #[test]
    fn test_different_secrets_produce_different_tokens() {
        let manager1 = SessionBindingManager::new(vec![1u8; 32]);
        let manager2 = SessionBindingManager::new(vec![2u8; 32]);

        // Use same meeting/correlation/participant but we can't use same nonce
        // since it's randomly generated. Instead, validate that tokens are not interchangeable.
        let (token1, nonce1) = manager1.generate_token("meeting-1", "corr-1", "part-1");

        // Token from manager1 should not validate with manager2
        let valid = manager2.validate_token("meeting-1", "corr-1", "part-1", &nonce1, &token1);
        assert!(!valid);
    }

    #[test]
    fn test_stored_binding_expiration() {
        let binding = StoredBinding::new(
            "corr-1".to_string(),
            "part-1".to_string(),
            "user-1".to_string(),
            "nonce".to_string(),
            "token".to_string(),
        );

        // Fresh binding should not be expired
        assert!(!binding.is_expired());
    }

    #[test]
    fn test_generate_correlation_id() {
        let id1 = StoredBinding::generate_correlation_id();
        let id2 = StoredBinding::generate_correlation_id();

        // Should be valid UUIDs
        assert_eq!(id1.len(), 36);
        assert_eq!(id2.len(), 36);

        // Should be unique
        assert_ne!(id1, id2);
    }

    #[test]
    #[should_panic(expected = "Master secret must be at least 32 bytes")]
    fn test_manager_requires_32_byte_secret() {
        let _ = SessionBindingManager::new(vec![0u8; 16]);
    }
}
