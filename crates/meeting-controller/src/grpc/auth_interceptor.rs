//! gRPC authentication interceptor for MC service.
//!
//! Validates incoming requests from Global Controller have proper authorization.
//! This provides defense-in-depth beyond transport-level security.
//!
//! # Security
//!
//! - All gRPC requests from GC require valid Bearer token
//! - Generic error messages prevent information leakage
//! - Failed authentication returns UNAUTHENTICATED status
//!
//! # Note
//!
//! Token validation is currently structural (format, non-empty, size limits).
//! Full cryptographic validation will be added when MC JWKS integration is
//! implemented in Phase 6h.

use common::jwt::MAX_JWT_SIZE_BYTES;
use tonic::{service::Interceptor, Request, Status};
use tracing::instrument;

/// gRPC authentication interceptor for MC service.
///
/// Validates that incoming requests have proper authorization headers.
/// This is a synchronous interceptor that performs basic validation.
#[derive(Clone, Debug)]
pub struct McAuthInterceptor {
    /// Whether to require authorization (can be disabled for testing).
    require_auth: bool,
}

impl Default for McAuthInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl McAuthInterceptor {
    /// Create a new auth interceptor with authorization required.
    #[must_use]
    pub fn new() -> Self {
        Self { require_auth: true }
    }

    /// Create an auth interceptor with authorization disabled (for testing only).
    #[must_use]
    #[cfg(test)]
    pub fn disabled() -> Self {
        Self {
            require_auth: false,
        }
    }

    /// Extract Bearer token from authorization metadata.
    fn extract_token<'a>(
        &self,
        auth_value: &'a tonic::metadata::MetadataValue<tonic::metadata::Ascii>,
    ) -> Option<&'a str> {
        let auth_str = auth_value.to_str().ok()?;
        auth_str.strip_prefix("Bearer ")
    }
}

impl Interceptor for McAuthInterceptor {
    /// Intercept the request and validate authorization.
    ///
    /// Validates:
    /// - Authorization header is present
    /// - Bearer token format is correct
    /// - Token is not empty
    /// - Token size is within limits (8KB)
    #[instrument(skip_all, name = "mc.grpc.auth_interceptor")]
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        // Skip auth if disabled (testing only)
        if !self.require_auth {
            return Ok(request);
        }

        // Extract authorization metadata
        let auth_header = request.metadata().get("authorization").ok_or_else(|| {
            tracing::debug!(target: "mc.grpc.auth", "Missing authorization metadata");
            Status::unauthenticated("Missing authorization header")
        })?;

        // Extract Bearer token
        let token = self.extract_token(auth_header).ok_or_else(|| {
            tracing::debug!(target: "mc.grpc.auth", "Invalid authorization format");
            Status::unauthenticated("Invalid authorization format")
        })?;

        // Basic token format validation
        if token.is_empty() {
            tracing::debug!(target: "mc.grpc.auth", "Empty token");
            return Err(Status::unauthenticated("Empty token"));
        }

        // Check token size limit (8KB max per security requirements)
        if token.len() > MAX_JWT_SIZE_BYTES {
            tracing::debug!(
                target: "mc.grpc.auth",
                token_size = token.len(),
                "Token exceeds size limit"
            );
            return Err(Status::unauthenticated("Invalid token"));
        }

        // Token format validated - request can proceed
        // Full cryptographic validation deferred to Phase 6h (JWKS integration)
        tracing::trace!(
            target: "mc.grpc.auth",
            token_len = token.len(),
            "Authorization validated"
        );

        Ok(request)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn create_request_with_auth(auth_value: &str) -> Request<()> {
        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert("authorization", auth_value.parse().unwrap());
        request
    }

    fn create_request_without_auth() -> Request<()> {
        Request::new(())
    }

    #[test]
    fn test_interceptor_default_requires_auth() {
        let interceptor = McAuthInterceptor::default();
        assert!(interceptor.require_auth);
    }

    #[test]
    fn test_interceptor_missing_authorization_header() {
        let mut interceptor = McAuthInterceptor::new();
        let request = create_request_without_auth();

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Missing authorization header"));
    }

    #[test]
    fn test_interceptor_invalid_auth_format_basic() {
        let mut interceptor = McAuthInterceptor::new();
        let request = create_request_with_auth("Basic dXNlcjpwYXNz");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Invalid authorization format"));
    }

    #[test]
    fn test_interceptor_invalid_auth_format_no_bearer() {
        let mut interceptor = McAuthInterceptor::new();
        let request = create_request_with_auth("Token abc123");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Invalid authorization format"));
    }

    #[test]
    fn test_interceptor_empty_token() {
        let mut interceptor = McAuthInterceptor::new();
        let request = create_request_with_auth("Bearer ");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Empty token"));
    }

    #[test]
    fn test_interceptor_oversized_token() {
        let mut interceptor = McAuthInterceptor::new();
        let oversized_token = "a".repeat(8193);
        let request = create_request_with_auth(&format!("Bearer {}", oversized_token));

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        // Generic error message per security requirements
        assert!(status.message().contains("Invalid token"));
    }

    #[test]
    fn test_interceptor_token_at_8192_bytes_accepted() {
        let mut interceptor = McAuthInterceptor::new();
        // Token exactly at the limit should be accepted
        let token_at_limit = "a".repeat(8192);
        let request = create_request_with_auth(&format!("Bearer {}", token_at_limit));

        let result = interceptor.call(request);

        assert!(result.is_ok());
    }

    #[test]
    fn test_interceptor_valid_token() {
        let mut interceptor = McAuthInterceptor::new();
        let request = create_request_with_auth("Bearer valid.jwt.token");

        let result = interceptor.call(request);

        assert!(result.is_ok());
    }

    #[test]
    fn test_interceptor_bearer_case_sensitive() {
        let mut interceptor = McAuthInterceptor::new();
        // "bearer" lowercase should not work
        let request = create_request_with_auth("bearer token123");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_interceptor_disabled_skips_validation() {
        let mut interceptor = McAuthInterceptor::disabled();
        let request = create_request_without_auth();

        let result = interceptor.call(request);

        // Should pass even without auth when disabled
        assert!(result.is_ok());
    }

    #[test]
    fn test_max_token_size_constant() {
        // Verify constant matches security requirements
        assert_eq!(MAX_JWT_SIZE_BYTES, 8192);
    }

    #[test]
    fn test_extract_token_helper() {
        let interceptor = McAuthInterceptor::new();

        // Valid Bearer token
        let meta: tonic::metadata::MetadataValue<tonic::metadata::Ascii> =
            "Bearer abc123".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), Some("abc123"));

        // No Bearer prefix
        let meta: tonic::metadata::MetadataValue<tonic::metadata::Ascii> =
            "Token xyz".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), None);

        // Just "Bearer" without token
        let meta: tonic::metadata::MetadataValue<tonic::metadata::Ascii> =
            "Bearer".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), None);
    }

    #[test]
    fn test_interceptor_debug_impl() {
        let interceptor = McAuthInterceptor::new();
        let debug_str = format!("{:?}", interceptor);
        assert!(debug_str.contains("McAuthInterceptor"));
        assert!(debug_str.contains("require_auth: true"));
    }
}
