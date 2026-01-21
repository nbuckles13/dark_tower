//! gRPC authentication interceptor for JWT validation.
//!
//! Extracts Bearer token from the `authorization` metadata and validates
//! it using the existing JwtValidator infrastructure.
//!
//! # Security
//!
//! - All gRPC requests require valid JWT authentication
//! - Tokens are validated using JWKS from Auth Controller
//! - Generic error messages prevent information leakage
//! - Failed authentication returns UNAUTHENTICATED status

use crate::auth::JwtValidator;
use std::sync::Arc;
use tonic::{metadata::MetadataValue, service::Interceptor, Request, Status};
use tracing::instrument;

/// Maximum token size in bytes (8KB) per security requirements.
const MAX_TOKEN_SIZE: usize = 8192;

/// gRPC authentication interceptor.
///
/// Validates JWT tokens from the `authorization` metadata header.
/// This is a synchronous interceptor - for async validation, we extract
/// the token here and the service layer can perform additional validation
/// if needed.
///
/// Note: This interceptor is provided as an alternative to the async layer.
/// It stores the token for later validation by the service layer.
#[derive(Clone)]
#[allow(dead_code)] // Alternative API to async_auth layer
pub struct GrpcAuthInterceptor {
    jwt_validator: Arc<JwtValidator>,
}

#[allow(dead_code)] // Alternative API to async layer
impl GrpcAuthInterceptor {
    /// Create a new gRPC auth interceptor.
    pub fn new(jwt_validator: Arc<JwtValidator>) -> Self {
        Self { jwt_validator }
    }

    /// Extract Bearer token from authorization metadata.
    fn extract_token<'a>(
        &self,
        auth_value: &'a MetadataValue<tonic::metadata::Ascii>,
    ) -> Option<&'a str> {
        let auth_str = auth_value.to_str().ok()?;
        auth_str.strip_prefix("Bearer ")
    }
}

impl Interceptor for GrpcAuthInterceptor {
    /// Intercept the request and validate the JWT token.
    ///
    /// Note: tonic Interceptor is synchronous, so we cannot perform async
    /// JWT validation here. Instead, we extract and do basic validation,
    /// storing the token for later async validation if needed.
    ///
    /// For full async validation, consider using a Tower layer instead.
    #[instrument(skip_all, name = "gc.grpc.auth_interceptor")]
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        // Extract authorization metadata
        let auth_header = request.metadata().get("authorization").ok_or_else(|| {
            tracing::debug!(target: "gc.grpc.auth", "Missing authorization metadata");
            Status::unauthenticated("Missing authorization header")
        })?;

        // Extract Bearer token
        let token = self.extract_token(auth_header).ok_or_else(|| {
            tracing::debug!(target: "gc.grpc.auth", "Invalid authorization format");
            Status::unauthenticated("Invalid authorization format")
        })?;

        // Basic token format validation (not empty, reasonable length)
        if token.is_empty() {
            return Err(Status::unauthenticated("Empty token"));
        }

        // Check token size limit (8KB max per security requirements)
        if token.len() > MAX_TOKEN_SIZE {
            tracing::debug!(
                target: "gc.grpc.auth",
                token_size = token.len(),
                "Token exceeds size limit"
            );
            return Err(Status::unauthenticated("Invalid token"));
        }

        // Copy token before releasing borrow
        let token_string = token.to_string();

        // Store the token in request extensions for async validation by the service
        // The service can then call jwt_validator.validate() asynchronously
        let mut request = request;
        request
            .extensions_mut()
            .insert(PendingTokenValidation(token_string));

        Ok(request)
    }
}

/// Marker type for tokens pending async validation.
///
/// The interceptor extracts the token synchronously, and the service
/// layer performs async JWT validation using this stored token.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Used with GrpcAuthInterceptor
pub struct PendingTokenValidation(pub String);

/// Async gRPC authentication service layer.
///
/// This module provides an async-capable authentication layer for gRPC.
/// Use this instead of the interceptor when you need full async JWT validation.
pub mod async_auth {
    use super::*;
    use crate::auth::Claims;
    use axum::http;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tonic::body::BoxBody;
    use tower::{Layer, Service};

    /// Tower layer for async gRPC authentication.
    #[derive(Clone)]
    pub struct GrpcAuthLayer {
        jwt_validator: Arc<JwtValidator>,
    }

    impl GrpcAuthLayer {
        /// Create a new async auth layer.
        pub fn new(jwt_validator: Arc<JwtValidator>) -> Self {
            Self { jwt_validator }
        }
    }

    impl<S> Layer<S> for GrpcAuthLayer {
        type Service = GrpcAuthService<S>;

        fn layer(&self, inner: S) -> Self::Service {
            GrpcAuthService {
                inner,
                jwt_validator: self.jwt_validator.clone(),
            }
        }
    }

    /// Tower service for async gRPC authentication.
    #[derive(Clone)]
    pub struct GrpcAuthService<S> {
        inner: S,
        jwt_validator: Arc<JwtValidator>,
    }

    impl<S, ReqBody> Service<http::Request<ReqBody>> for GrpcAuthService<S>
    where
        S: Service<http::Request<ReqBody>, Response = http::Response<BoxBody>>
            + Clone
            + Send
            + 'static,
        S::Future: Send + 'static,
        ReqBody: Send + 'static,
    {
        type Response = S::Response;
        type Error = S::Error;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
            let mut inner = self.inner.clone();
            let jwt_validator = self.jwt_validator.clone();

            Box::pin(async move {
                // Extract authorization header
                let auth_header = match req.headers().get("authorization") {
                    Some(h) => h,
                    None => {
                        tracing::debug!(target: "gc.grpc.auth", "Missing authorization header");
                        return Ok(unauthenticated_response());
                    }
                };

                // Parse header value
                let auth_str = match auth_header.to_str() {
                    Ok(s) => s,
                    Err(_) => {
                        tracing::debug!(target: "gc.grpc.auth", "Invalid authorization header encoding");
                        return Ok(unauthenticated_response());
                    }
                };

                // Extract Bearer token
                let token = match auth_str.strip_prefix("Bearer ") {
                    Some(t) => t,
                    None => {
                        tracing::debug!(target: "gc.grpc.auth", "Invalid authorization format");
                        return Ok(unauthenticated_response());
                    }
                };

                // Validate JWT asynchronously
                let claims = match jwt_validator.validate(token).await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::debug!(target: "gc.grpc.auth", error = %e, "JWT validation failed");
                        return Ok(unauthenticated_response());
                    }
                };

                // Store validated claims in request extensions
                let (mut parts, body) = req.into_parts();
                parts.extensions.insert(ValidatedClaims(claims));
                let req = http::Request::from_parts(parts, body);

                // Continue to inner service
                inner.call(req).await
            })
        }
    }

    /// Wrapper for validated claims in request extensions.
    #[derive(Clone, Debug)]
    #[allow(dead_code)] // Will be used by service handlers
    pub struct ValidatedClaims(pub Claims);

    /// Create an unauthenticated gRPC response.
    fn unauthenticated_response() -> http::Response<BoxBody> {
        let status = Status::unauthenticated("Authentication required");
        status.into_http()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_token_validation_debug() {
        let pending = PendingTokenValidation("test-token".to_string());
        let debug_str = format!("{:?}", pending);
        // Token value is visible in debug - this is intentional for testing only
        assert!(debug_str.contains("PendingTokenValidation"));
    }

    #[test]
    fn test_max_token_size() {
        // Verify constant matches security requirements
        assert_eq!(super::MAX_TOKEN_SIZE, 8192);
    }
}
