//! gRPC authentication interceptor for MH service.
//!
//! Validates incoming requests from Meeting Controller have proper authorization.
//! This provides defense-in-depth beyond transport-level security.
//!
//! # Security
//!
//! - All gRPC requests from MC require valid Bearer token
//! - Generic error messages prevent information leakage
//! - Failed authentication returns UNAUTHENTICATED status
//!
//! # Note
//!
//! Token validation is currently structural (format, non-empty, size limits).
//! Full cryptographic JWKS validation will be added in a later phase.

use common::jwt::MAX_JWT_SIZE_BYTES;
use tonic::{service::Interceptor, Request, Status};
use tracing::instrument;

/// gRPC authentication interceptor for MH service.
///
/// Validates that incoming requests have proper authorization headers.
/// This is a synchronous interceptor that performs basic validation.
#[derive(Clone, Debug)]
pub struct MhAuthInterceptor {
    /// Whether to require authorization (can be disabled for testing).
    require_auth: bool,
}

impl Default for MhAuthInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl MhAuthInterceptor {
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
    #[expect(
        clippy::unused_self,
        reason = "Method uses &self for API consistency, will use self fields for JWKS validation in future phase"
    )]
    fn extract_token<'a>(
        &self,
        auth_value: &'a tonic::metadata::MetadataValue<tonic::metadata::Ascii>,
    ) -> Option<&'a str> {
        let auth_str = auth_value.to_str().ok()?;
        auth_str.strip_prefix("Bearer ")
    }
}

impl Interceptor for MhAuthInterceptor {
    /// Intercept the request and validate authorization.
    ///
    /// Validates:
    /// - Authorization header is present
    /// - Bearer token format is correct
    /// - Token is not empty
    /// - Token size is within limits (8KB)
    #[instrument(skip_all, name = "mh.grpc.auth_interceptor")]
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        if !self.require_auth {
            return Ok(request);
        }

        let auth_header = request.metadata().get("authorization").ok_or_else(|| {
            tracing::debug!(target: "mh.grpc.auth", "Missing authorization metadata");
            Status::unauthenticated("Missing authorization header")
        })?;

        let token = self.extract_token(auth_header).ok_or_else(|| {
            tracing::debug!(target: "mh.grpc.auth", "Invalid authorization format");
            Status::unauthenticated("Invalid authorization format")
        })?;

        if token.is_empty() {
            tracing::debug!(target: "mh.grpc.auth", "Empty token");
            return Err(Status::unauthenticated("Empty token"));
        }

        if token.len() > MAX_JWT_SIZE_BYTES {
            tracing::debug!(
                target: "mh.grpc.auth",
                token_size = token.len(),
                "Token exceeds size limit"
            );
            return Err(Status::unauthenticated("Invalid token"));
        }

        tracing::trace!(
            target: "mh.grpc.auth",
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
        let interceptor = MhAuthInterceptor::default();
        assert!(interceptor.require_auth);
    }

    #[test]
    fn test_interceptor_missing_authorization_header() {
        let mut interceptor = MhAuthInterceptor::new();
        let request = create_request_without_auth();

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Missing authorization header"));
    }

    #[test]
    fn test_interceptor_invalid_auth_format_basic() {
        let mut interceptor = MhAuthInterceptor::new();
        let request = create_request_with_auth("Basic dXNlcjpwYXNz");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Invalid authorization format"));
    }

    #[test]
    fn test_interceptor_empty_token() {
        let mut interceptor = MhAuthInterceptor::new();
        let request = create_request_with_auth("Bearer ");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Empty token"));
    }

    #[test]
    fn test_interceptor_oversized_token() {
        let mut interceptor = MhAuthInterceptor::new();
        let oversized_token = "a".repeat(8193);
        let request = create_request_with_auth(&format!("Bearer {oversized_token}"));

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Invalid token"));
    }

    #[test]
    fn test_interceptor_token_at_8192_bytes_accepted() {
        let mut interceptor = MhAuthInterceptor::new();
        let token_at_limit = "a".repeat(8192);
        let request = create_request_with_auth(&format!("Bearer {token_at_limit}"));

        let result = interceptor.call(request);

        assert!(result.is_ok());
    }

    #[test]
    fn test_interceptor_valid_token() {
        let mut interceptor = MhAuthInterceptor::new();
        let request = create_request_with_auth("Bearer valid.jwt.token");

        let result = interceptor.call(request);

        assert!(result.is_ok());
    }

    #[test]
    fn test_interceptor_bearer_case_sensitive() {
        let mut interceptor = MhAuthInterceptor::new();
        let request = create_request_with_auth("bearer token123");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_interceptor_disabled_skips_validation() {
        let mut interceptor = MhAuthInterceptor::disabled();
        let request = create_request_without_auth();

        let result = interceptor.call(request);

        assert!(result.is_ok());
    }

    #[test]
    fn test_max_token_size_constant() {
        assert_eq!(MAX_JWT_SIZE_BYTES, 8192);
    }

    #[test]
    fn test_extract_token_helper() {
        let interceptor = MhAuthInterceptor::new();

        let meta: tonic::metadata::MetadataValue<tonic::metadata::Ascii> =
            "Bearer abc123".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), Some("abc123"));

        let meta: tonic::metadata::MetadataValue<tonic::metadata::Ascii> =
            "Token xyz".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), None);

        let meta: tonic::metadata::MetadataValue<tonic::metadata::Ascii> =
            "Bearer".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), None);
    }
}
