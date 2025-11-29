//! Custom test assertions for expressive tests
//!
//! Provides trait-based assertions for token validation.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;

/// JWT header structure
#[derive(Debug, Deserialize)]
struct JwtHeader {
    pub alg: String,
    pub typ: String,
    #[serde(default)]
    pub kid: Option<String>,
}

/// JWT claims structure
#[derive(Debug, Deserialize)]
struct JwtClaims {
    pub sub: String,
    pub exp: i64,
    #[expect(dead_code)] // Used for JWT structure validation but not accessed
    pub iat: i64,
    pub scope: String,
}

/// Custom assertions for token responses
///
/// # Example
/// ```rust,ignore
/// token
///     .assert_valid_jwt()
///     .assert_has_scope("meeting:create")
///     .assert_signed_by("test-key-2025-01");
/// ```
pub trait TokenAssertions {
    /// Assert that the token is a valid JWT format
    fn assert_valid_jwt(&self) -> &Self;

    /// Assert that the token contains the specified scope
    fn assert_has_scope(&self, scope: &str) -> &Self;

    /// Assert that the token was signed by the specified key
    fn assert_signed_by(&self, key_id: &str) -> &Self;

    /// Assert that the token expires within the specified seconds
    fn assert_expires_in(&self, seconds: u64) -> &Self;

    /// Assert that the token is for the specified subject
    fn assert_for_subject(&self, subject: &str) -> &Self;
}

impl TokenAssertions for String {
    fn assert_valid_jwt(&self) -> &Self {
        let parts: Vec<_> = self.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "JWT must have 3 parts (header.payload.signature), got {}",
            parts.len()
        );

        // Decode and validate header
        let header_result = URL_SAFE_NO_PAD.decode(parts[0]);
        assert!(
            header_result.is_ok(),
            "Failed to base64 decode JWT header: {:?}",
            header_result.err()
        );

        let header: Result<JwtHeader, _> = serde_json::from_slice(&header_result.unwrap());
        assert!(
            header.is_ok(),
            "Failed to parse JWT header JSON: {:?}",
            header.err()
        );

        let header = header.unwrap();
        assert_eq!(header.alg, "EdDSA", "Expected EdDSA algorithm");
        assert_eq!(header.typ, "JWT", "Expected JWT type");

        // Decode and validate payload
        let payload_result = URL_SAFE_NO_PAD.decode(parts[1]);
        assert!(
            payload_result.is_ok(),
            "Failed to base64 decode JWT payload: {:?}",
            payload_result.err()
        );

        let claims: Result<JwtClaims, _> = serde_json::from_slice(&payload_result.unwrap());
        assert!(
            claims.is_ok(),
            "Failed to parse JWT claims JSON: {:?}",
            claims.err()
        );

        self
    }

    fn assert_has_scope(&self, scope: &str) -> &Self {
        let parts: Vec<_> = self.split('.').collect();
        let payload = URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Invalid JWT payload");
        let claims: JwtClaims =
            serde_json::from_slice(&payload).expect("Failed to parse JWT claims");

        let scopes: Vec<_> = claims.scope.split_whitespace().collect();
        assert!(
            scopes.contains(&scope),
            "Token does not contain scope '{}'. Available scopes: {}",
            scope,
            claims.scope
        );

        self
    }

    fn assert_signed_by(&self, key_id: &str) -> &Self {
        let parts: Vec<_> = self.split('.').collect();
        let header = URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Invalid JWT header");
        let jwt_header: JwtHeader =
            serde_json::from_slice(&header).expect("Failed to parse JWT header");

        assert_eq!(
            jwt_header.kid.as_deref(),
            Some(key_id),
            "Expected key_id '{}', got {:?}",
            key_id,
            jwt_header.kid
        );

        self
    }

    fn assert_expires_in(&self, seconds: u64) -> &Self {
        let parts: Vec<_> = self.split('.').collect();
        let payload = URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Invalid JWT payload");
        let claims: JwtClaims =
            serde_json::from_slice(&payload).expect("Failed to parse JWT claims");

        let now = chrono::Utc::now().timestamp();
        let expires_in = claims.exp - now;

        // Allow 5-second tolerance for clock skew
        assert!(
            (expires_in - seconds as i64).abs() <= 5,
            "Expected token to expire in {} seconds, but expires in {} seconds",
            seconds,
            expires_in
        );

        self
    }

    fn assert_for_subject(&self, subject: &str) -> &Self {
        let parts: Vec<_> = self.split('.').collect();
        let payload = URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Invalid JWT payload");
        let claims: JwtClaims =
            serde_json::from_slice(&payload).expect("Failed to parse JWT claims");

        assert_eq!(
            claims.sub, subject,
            "Expected subject '{}', got '{}'",
            subject, claims.sub
        );

        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_valid_jwt_with_valid_token() {
        // This is a test JWT with valid structure (header.payload.signature)
        // Note: Signature validation is done separately in crypto tests
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key-1"}"#;
        let payload = r#"{"sub":"test-client","exp":9999999999,"iat":1234567890,"scope":"meeting:create"}"#;

        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let signature_b64 = "fake_signature_for_testing";

        let token = format!("{}.{}.{}", header_b64, payload_b64, signature_b64);

        // Should not panic
        token.assert_valid_jwt();
    }

    #[test]
    #[should_panic(expected = "JWT must have 3 parts")]
    fn test_assert_valid_jwt_with_invalid_structure() {
        let token = "invalid.token".to_string();
        token.assert_valid_jwt();
    }

    #[test]
    fn test_assert_has_scope() {
        let header = r#"{"alg":"EdDSA","typ":"JWT"}"#;
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"meeting:create meeting:read"}"#;

        let token = format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(header.as_bytes()),
            URL_SAFE_NO_PAD.encode(payload.as_bytes())
        );

        token
            .assert_has_scope("meeting:create")
            .assert_has_scope("meeting:read");
    }

    #[test]
    #[should_panic(expected = "does not contain scope")]
    fn test_assert_has_scope_missing() {
        let header = r#"{"alg":"EdDSA","typ":"JWT"}"#;
        let payload =
            r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"meeting:create"}"#;

        let token = format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(header.as_bytes()),
            URL_SAFE_NO_PAD.encode(payload.as_bytes())
        );

        token.assert_has_scope("admin");
    }

    #[test]
    fn test_assert_signed_by() {
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key-2025"}"#;
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"test"}"#;

        let token = format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(header.as_bytes()),
            URL_SAFE_NO_PAD.encode(payload.as_bytes())
        );

        token.assert_signed_by("test-key-2025");
    }

    #[test]
    fn test_assert_for_subject() {
        let header = r#"{"alg":"EdDSA","typ":"JWT"}"#;
        let payload =
            r#"{"sub":"test-client-123","exp":9999999999,"iat":1234567890,"scope":"test"}"#;

        let token = format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(header.as_bytes()),
            URL_SAFE_NO_PAD.encode(payload.as_bytes())
        );

        token.assert_for_subject("test-client-123");
    }
}
