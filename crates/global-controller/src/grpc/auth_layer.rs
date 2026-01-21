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

    // =========================================================================
    // GrpcAuthInterceptor tests
    // =========================================================================

    fn create_interceptor() -> GrpcAuthInterceptor {
        // Create a minimal JwtValidator for the interceptor
        // The interceptor doesn't actually validate JWT - it just extracts and stores
        let jwks_client = Arc::new(crate::auth::JwksClient::new(
            "http://localhost:8082/.well-known/jwks.json".to_string(),
        ));
        let jwt_validator = Arc::new(JwtValidator::new(jwks_client, 300));
        GrpcAuthInterceptor::new(jwt_validator)
    }

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
    fn test_interceptor_missing_authorization_header() {
        let mut interceptor = create_interceptor();
        let request = create_request_without_auth();

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Missing authorization header"));
    }

    #[test]
    fn test_interceptor_invalid_auth_format_basic() {
        let mut interceptor = create_interceptor();
        let request = create_request_with_auth("Basic dXNlcjpwYXNz");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Invalid authorization format"));
    }

    #[test]
    fn test_interceptor_invalid_auth_format_no_bearer() {
        let mut interceptor = create_interceptor();
        let request = create_request_with_auth("Token abc123");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Invalid authorization format"));
    }

    #[test]
    fn test_interceptor_empty_token() {
        let mut interceptor = create_interceptor();
        let request = create_request_with_auth("Bearer ");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(status.message().contains("Empty token"));
    }

    #[test]
    fn test_interceptor_oversized_token() {
        let mut interceptor = create_interceptor();
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
        let mut interceptor = create_interceptor();
        // Token exactly at the limit should be accepted
        let token_at_limit = "a".repeat(8192);
        let request = create_request_with_auth(&format!("Bearer {}", token_at_limit));

        let result = interceptor.call(request);

        // Token passes size check but is stored for later validation
        assert!(result.is_ok());
        let request = result.unwrap();
        let pending = request
            .extensions()
            .get::<PendingTokenValidation>()
            .expect("Should have PendingTokenValidation");
        assert_eq!(pending.0.len(), 8192);
    }

    #[test]
    fn test_interceptor_valid_token_stored() {
        let mut interceptor = create_interceptor();
        let request = create_request_with_auth("Bearer valid.jwt.token");

        let result = interceptor.call(request);

        assert!(result.is_ok());
        let request = result.unwrap();
        let pending = request
            .extensions()
            .get::<PendingTokenValidation>()
            .expect("Should have PendingTokenValidation");
        assert_eq!(pending.0, "valid.jwt.token");
    }

    #[test]
    fn test_interceptor_bearer_case_sensitive() {
        let mut interceptor = create_interceptor();
        // "bearer" lowercase should not work
        let request = create_request_with_auth("bearer token123");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_interceptor_bearer_with_extra_spaces() {
        let mut interceptor = create_interceptor();
        // Extra spaces after Bearer should result in token with leading space
        let request = create_request_with_auth("Bearer  token-with-leading-space");

        let result = interceptor.call(request);

        assert!(result.is_ok());
        let request = result.unwrap();
        let pending = request
            .extensions()
            .get::<PendingTokenValidation>()
            .expect("Should have PendingTokenValidation");
        // Token includes the extra space
        assert_eq!(pending.0, " token-with-leading-space");
    }

    #[test]
    fn test_extract_token_helper() {
        let interceptor = create_interceptor();

        // Valid Bearer token
        let meta: MetadataValue<tonic::metadata::Ascii> = "Bearer abc123".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), Some("abc123"));

        // No Bearer prefix
        let meta: MetadataValue<tonic::metadata::Ascii> = "Token xyz".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), None);

        // Just "Bearer" without token
        let meta: MetadataValue<tonic::metadata::Ascii> = "Bearer".parse().unwrap();
        assert_eq!(interceptor.extract_token(&meta), None);
    }

    // =========================================================================
    // async_auth module tests
    // =========================================================================

    #[test]
    fn test_validated_claims_debug() {
        use crate::auth::Claims;
        use async_auth::ValidatedClaims;

        let claims = Claims {
            sub: "test-subject".to_string(),
            exp: 1234567890,
            iat: 1234567800,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        let validated = ValidatedClaims(claims);
        let debug_str = format!("{:?}", validated);
        assert!(debug_str.contains("ValidatedClaims"));
        // sub should be redacted in Claims debug
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn test_grpc_auth_layer_creation() {
        let jwks_client = Arc::new(crate::auth::JwksClient::new(
            "http://localhost:8082/.well-known/jwks.json".to_string(),
        ));
        let jwt_validator = Arc::new(JwtValidator::new(jwks_client, 300));

        let _layer = async_auth::GrpcAuthLayer::new(jwt_validator);
        // Layer creation should succeed
    }
}

// =========================================================================
// GrpcAuthService async integration tests (requires tokio runtime)
// =========================================================================
#[cfg(test)]
mod async_tests {
    use super::async_auth::*;
    use super::*;
    use axum::http::{self, HeaderValue, Request, Response};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header as JwtHeader};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde::{Deserialize, Serialize};
    use std::convert::Infallible;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tonic::body::BoxBody;
    use tower::{Layer, Service};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Mock inner service that returns OK with a marker header indicating it was reached.
    #[derive(Clone)]
    struct MockInnerService;

    /// Header to indicate the inner service was reached.
    const INNER_SERVICE_REACHED: &str = "x-inner-service-reached";

    impl<ReqBody> Service<Request<ReqBody>> for MockInnerService
    where
        ReqBody: Send + 'static,
    {
        type Response = Response<BoxBody>;
        type Error = Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<ReqBody>) -> Self::Future {
            Box::pin(async move {
                let body = BoxBody::default();
                Ok(Response::builder()
                    .status(200)
                    .header(INNER_SERVICE_REACHED, "true")
                    .body(body)
                    .expect("Failed to build response"))
            })
        }
    }

    /// Test keypair for signing tokens.
    struct TestKeypair {
        kid: String,
        public_key_bytes: Vec<u8>,
        private_key_pkcs8: Vec<u8>,
    }

    /// JWT Claims for test tokens.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        exp: i64,
        iat: i64,
        scope: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        service_type: Option<String>,
    }

    impl TestKeypair {
        fn new(seed: u8, kid: &str) -> Self {
            let mut seed_bytes = [0u8; 32];
            seed_bytes[0] = seed;
            for (i, byte) in seed_bytes.iter_mut().enumerate().skip(1) {
                *byte = seed.wrapping_mul(i as u8).wrapping_add(i as u8);
            }

            let key_pair = Ed25519KeyPair::from_seed_unchecked(&seed_bytes)
                .expect("Failed to create test keypair");

            let public_key_bytes = key_pair.public_key().as_ref().to_vec();
            let private_key_pkcs8 = build_pkcs8_from_seed(&seed_bytes);

            Self {
                kid: kid.to_string(),
                public_key_bytes,
                private_key_pkcs8,
            }
        }

        fn sign_token(&self, claims: &TestClaims) -> String {
            let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
            let mut header = JwtHeader::new(Algorithm::EdDSA);
            header.typ = Some("JWT".to_string());
            header.kid = Some(self.kid.clone());

            encode(&header, claims, &encoding_key).expect("Failed to sign token")
        }

        fn jwk_json(&self) -> serde_json::Value {
            serde_json::json!({
                "kty": "OKP",
                "kid": self.kid,
                "crv": "Ed25519",
                "x": URL_SAFE_NO_PAD.encode(&self.public_key_bytes),
                "alg": "EdDSA",
                "use": "sig"
            })
        }
    }

    /// Build PKCS#8 v1 document from Ed25519 seed.
    fn build_pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
        let mut pkcs8 = Vec::new();
        pkcs8.push(0x30);
        pkcs8.push(0x2e);
        pkcs8.extend_from_slice(&[0x02, 0x01, 0x00]);
        pkcs8.push(0x30);
        pkcs8.push(0x05);
        pkcs8.extend_from_slice(&[0x06, 0x03, 0x2b, 0x65, 0x70]);
        pkcs8.push(0x04);
        pkcs8.push(0x22);
        pkcs8.push(0x04);
        pkcs8.push(0x20);
        pkcs8.extend_from_slice(seed);
        pkcs8
    }

    async fn setup_mock_jwks(keypair: &TestKeypair) -> MockServer {
        let mock_server = MockServer::start().await;
        let jwks_response = serde_json::json!({
            "keys": [keypair.jwk_json()]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        mock_server
    }

    fn create_auth_service(jwks_url: String) -> GrpcAuthService<MockInnerService> {
        let jwks_client = Arc::new(crate::auth::JwksClient::new(jwks_url));
        let jwt_validator = Arc::new(JwtValidator::new(jwks_client, 300));
        let layer = GrpcAuthLayer::new(jwt_validator);
        layer.layer(MockInnerService)
    }

    fn create_http_request_with_auth(auth_value: &str) -> Request<()> {
        Request::builder()
            .header("authorization", auth_value)
            .body(())
            .expect("Failed to build request")
    }

    fn create_http_request_without_auth() -> Request<()> {
        Request::builder()
            .body(())
            .expect("Failed to build request")
    }

    /// Helper to check if inner service was reached (valid auth)
    fn inner_service_reached(response: &Response<BoxBody>) -> bool {
        response.headers().get(INNER_SERVICE_REACHED).is_some()
    }

    #[tokio::test]
    async fn test_async_auth_missing_authorization_header() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let mock_server = setup_mock_jwks(&keypair).await;

        let mut service =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        let request = create_http_request_without_auth();
        let response = service
            .call(request)
            .await
            .expect("Service should not error");

        // Inner service should NOT be reached - request should be rejected
        assert!(
            !inner_service_reached(&response),
            "Inner service should not be reached for unauthenticated request"
        );
    }

    #[tokio::test]
    async fn test_async_auth_invalid_header_encoding() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let mock_server = setup_mock_jwks(&keypair).await;

        let mut service =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        // Create request with non-ASCII header value (using raw bytes)
        let mut request = Request::builder()
            .body(())
            .expect("Failed to build request");
        // Insert header with invalid UTF-8 sequence
        request.headers_mut().insert(
            "authorization",
            HeaderValue::from_bytes(b"Bearer \xff\xfe invalid").unwrap(),
        );

        let response = service
            .call(request)
            .await
            .expect("Service should not error");

        // Inner service should NOT be reached
        assert!(
            !inner_service_reached(&response),
            "Inner service should not be reached for invalid header encoding"
        );
    }

    #[tokio::test]
    async fn test_async_auth_invalid_bearer_format() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let mock_server = setup_mock_jwks(&keypair).await;

        let mut service =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        // Non-Bearer format
        let request = create_http_request_with_auth("Basic dXNlcjpwYXNz");
        let response = service
            .call(request)
            .await
            .expect("Service should not error");

        // Inner service should NOT be reached
        assert!(
            !inner_service_reached(&response),
            "Inner service should not be reached for non-Bearer format"
        );
    }

    #[tokio::test]
    async fn test_async_auth_invalid_jwt_token() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let mock_server = setup_mock_jwks(&keypair).await;

        let mut service =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        // Malformed JWT
        let request = create_http_request_with_auth("Bearer not.a.valid.jwt");
        let response = service
            .call(request)
            .await
            .expect("Service should not error");

        // Inner service should NOT be reached
        assert!(
            !inner_service_reached(&response),
            "Inner service should not be reached for malformed JWT"
        );
    }

    #[tokio::test]
    async fn test_async_auth_expired_jwt_token() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let mock_server = setup_mock_jwks(&keypair).await;

        let mut service =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        // Create expired token
        let now = chrono::Utc::now().timestamp();
        let claims = TestClaims {
            sub: "test-client".to_string(),
            exp: now - 3600, // Expired 1 hour ago
            iat: now - 7200,
            scope: "read".to_string(),
            service_type: None,
        };
        let expired_token = keypair.sign_token(&claims);

        let request = create_http_request_with_auth(&format!("Bearer {}", expired_token));
        let response = service
            .call(request)
            .await
            .expect("Service should not error");

        // Inner service should NOT be reached
        assert!(
            !inner_service_reached(&response),
            "Inner service should not be reached for expired JWT"
        );
    }

    #[tokio::test]
    async fn test_async_auth_valid_jwt_token() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let mock_server = setup_mock_jwks(&keypair).await;

        let mut service =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        // Create valid token
        let now = chrono::Utc::now().timestamp();
        let claims = TestClaims {
            sub: "test-client".to_string(),
            exp: now + 3600, // Expires in 1 hour
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        let valid_token = keypair.sign_token(&claims);

        let request = create_http_request_with_auth(&format!("Bearer {}", valid_token));
        let response = service
            .call(request)
            .await
            .expect("Service should not error");

        // Inner service SHOULD be reached with valid token
        assert!(
            inner_service_reached(&response),
            "Inner service should be reached for valid JWT"
        );
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_async_auth_unknown_kid() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let different_keypair = TestKeypair::new(2, "different-key");
        let mock_server = setup_mock_jwks(&different_keypair).await; // JWKS has different key

        let mut service =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        // Token signed with keypair but JWKS only has different_keypair
        let now = chrono::Utc::now().timestamp();
        let claims = TestClaims {
            sub: "test-client".to_string(),
            exp: now + 3600,
            iat: now,
            scope: "read".to_string(),
            service_type: None,
        };
        let token = keypair.sign_token(&claims);

        let request = create_http_request_with_auth(&format!("Bearer {}", token));
        let response = service
            .call(request)
            .await
            .expect("Service should not error");

        // Inner service should NOT be reached - unknown kid
        assert!(
            !inner_service_reached(&response),
            "Inner service should not be reached for unknown kid"
        );
    }

    #[tokio::test]
    async fn test_async_auth_poll_ready_delegates() {
        let keypair = TestKeypair::new(1, "test-key-01");
        let mock_server = setup_mock_jwks(&keypair).await;

        let mut service: GrpcAuthService<MockInnerService> =
            create_auth_service(format!("{}/.well-known/jwks.json", mock_server.uri()));

        // poll_ready should succeed (delegates to inner service)
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let result = Service::<Request<()>>::poll_ready(&mut service, &mut cx);
        assert!(matches!(result, Poll::Ready(Ok(()))));
    }
}
