//! JWT validation for Global Controller.
//!
//! Thin wrapper around `common::jwt::JwtValidator` that provides GC-specific
//! `validate()` and `validate_user()` methods with automatic `JwtError` -> `GcError`
//! error mapping.

use crate::auth::claims::Claims;
use crate::errors::GcError;
use common::jwt::{JwksClient, UserClaims};
use std::sync::Arc;
use tracing::instrument;

/// Re-export the common JwtValidator for direct generic usage.
pub use common::jwt::JwtValidator as CommonJwtValidator;

/// JWT validator using JWKS from Auth Controller.
///
/// Wraps `common::jwt::JwtValidator` and provides GC-specific typed methods
/// that return `GcError`.
pub struct JwtValidator {
    inner: CommonJwtValidator,
}

impl JwtValidator {
    /// Create a new JWT validator.
    ///
    /// # Arguments
    ///
    /// * `jwks_client` - Client for fetching public keys
    /// * `clock_skew_seconds` - Clock skew tolerance for iat validation
    pub fn new(jwks_client: Arc<JwksClient>, clock_skew_seconds: i64) -> Self {
        Self {
            inner: CommonJwtValidator::new(jwks_client, clock_skew_seconds),
        }
    }

    /// Validate a service JWT and return the claims.
    ///
    /// # Arguments
    ///
    /// * `token` - The JWT string to validate
    ///
    /// # Errors
    ///
    /// Returns `GcError::InvalidToken` for all validation failures with a generic
    /// message to prevent information leakage.
    #[instrument(skip_all)]
    pub async fn validate(&self, token: &str) -> Result<Claims, GcError> {
        Ok(self.inner.validate::<Claims>(token).await?)
    }

    /// Validate a service JWT and return the raw `JwtError` on failure.
    ///
    /// Used by the gRPC auth layer (ADR-0003) to classify failures for the
    /// `gc_jwt_validations_total{failure_reason}` metric. HTTP call sites
    /// should keep using `validate()` for the `GcError` mapping.
    #[instrument(skip_all)]
    pub async fn validate_raw(&self, token: &str) -> Result<Claims, common::jwt::JwtError> {
        self.inner.validate::<Claims>(token).await
    }

    /// Validate a user JWT and return the user claims.
    ///
    /// Same security checks as `validate()` but deserializes into `UserClaims`
    /// (org_id, roles, email, jti) instead of service `Claims` (scope, service_type).
    ///
    /// # Arguments
    ///
    /// * `token` - The JWT string to validate
    ///
    /// # Errors
    ///
    /// Returns `GcError::InvalidToken` for all validation failures.
    #[instrument(skip_all)]
    pub async fn validate_user(&self, token: &str) -> Result<UserClaims, GcError> {
        Ok(self.inner.validate::<UserClaims>(token).await?)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use common::jwt::JwtError;

    #[test]
    fn test_jwt_error_to_gc_error_invalid_token() {
        let jwt_err = JwtError::TokenTooLarge;
        let gc_err: GcError = jwt_err.into();
        assert!(
            matches!(&gc_err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken, got {:?}",
            gc_err
        );
    }

    #[test]
    fn test_jwt_error_to_gc_error_key_not_found() {
        let jwt_err = JwtError::KeyNotFound;
        let gc_err: GcError = jwt_err.into();
        assert!(
            matches!(&gc_err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken, got {:?}",
            gc_err
        );
    }

    #[test]
    fn test_jwt_error_to_gc_error_invalid_signature() {
        let jwt_err = JwtError::InvalidSignature;
        let gc_err: GcError = jwt_err.into();
        assert!(
            matches!(&gc_err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken, got {:?}",
            gc_err
        );
    }

    #[test]
    fn test_jwt_error_to_gc_error_service_unavailable() {
        let jwt_err = JwtError::ServiceUnavailable("auth down".to_string());
        let gc_err: GcError = jwt_err.into();
        assert!(
            matches!(&gc_err, GcError::ServiceUnavailable(msg) if msg == "auth down"),
            "Expected ServiceUnavailable, got {:?}",
            gc_err
        );
    }

    #[test]
    fn test_jwt_error_to_gc_error_malformed_token() {
        let jwt_err = JwtError::MalformedToken;
        let gc_err: GcError = jwt_err.into();
        assert!(
            matches!(&gc_err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken, got {:?}",
            gc_err
        );
    }

    #[test]
    fn test_jwt_error_to_gc_error_missing_kid() {
        let jwt_err = JwtError::MissingKid;
        let gc_err: GcError = jwt_err.into();
        assert!(
            matches!(&gc_err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken, got {:?}",
            gc_err
        );
    }

    #[test]
    fn test_jwt_error_to_gc_error_iat_too_far_in_future() {
        let jwt_err = JwtError::IatTooFarInFuture;
        let gc_err: GcError = jwt_err.into();
        assert!(
            matches!(&gc_err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken, got {:?}",
            gc_err
        );
    }

    #[test]
    fn test_jwt_validator_creation() {
        let jwks_client = Arc::new(
            JwksClient::new("http://localhost:8082/.well-known/jwks.json".to_string())
                .expect("Failed to create JWKS client"),
        );
        let _validator = JwtValidator::new(jwks_client, 300);
    }
}
