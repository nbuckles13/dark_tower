//! gRPC authentication for MC service.
//!
//! Provides two authentication mechanisms:
//!
//! - `McAuthInterceptor`: Synchronous structural validation for GC→MC requests
//!   (format, non-empty, size limits). Full JWKS upgrade deferred.
//! - `McAuthLayer`/`McAuthService`: Async JWKS-based cryptographic validation
//!   for MH→MC requests (R-22). Uses `tower::Layer`/`tower::Service` to support
//!   async JWKS cache lookups.
//!
//! # Security
//!
//! - All gRPC requests require valid Bearer token
//! - MH→MC tokens are validated cryptographically via `JwtValidator<ServiceClaims>`
//! - Scope authorization enforced (`service.write.mc`) (ADR-0003)
//! - Generic error messages prevent information leakage
//! - Failed authentication returns UNAUTHENTICATED status
//!
//! # Validation Chain (McAuthLayer)
//!
//! 1. Structural fast-path: format, non-empty, size limit (8KB)
//! 2. Cryptographic: `EdDSA` signature via JWKS
//! 3. Claims: exp, iat with clock skew tolerance
//! 4. Authorization: `service.write.mc` scope check

use crate::auth::CommonJwtValidator;
use crate::observability::metrics;
use axum::http;
use common::jwt::{JwksClient, ServiceClaims, MAX_JWT_SIZE_BYTES};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tonic::body::BoxBody;
use tonic::{service::Interceptor, Request, Status};
use tower::{Layer, Service};
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

/// Required scope for MC service tokens (ADR-0003).
///
/// All callers (GC, MH) must present a token with this scope to call MC gRPC.
const REQUIRED_SCOPE: &str = "service.write.mc";

/// Async authentication layer for MC gRPC service (R-22).
///
/// Wraps an inner service and validates service tokens cryptographically
/// via JWKS before forwarding requests. Applied at the server level to
/// authenticate both GC→MC and MH→MC calls.
///
/// Enforces `service.write.mc` scope authorization after JWKS validation.
#[derive(Clone)]
pub struct McAuthLayer {
    jwt_validator: Arc<CommonJwtValidator>,
    require_auth: bool,
}

impl McAuthLayer {
    /// Create a new auth layer with JWKS-based validation.
    #[must_use]
    pub fn new(jwks_client: Arc<JwksClient>, clock_skew_seconds: i64) -> Self {
        Self {
            jwt_validator: Arc::new(CommonJwtValidator::new(jwks_client, clock_skew_seconds)),
            require_auth: true,
        }
    }

    /// Create an auth layer with authentication disabled (for testing only).
    ///
    /// # Panics
    ///
    /// Panics if the dummy JWKS client cannot be created.
    #[must_use]
    #[cfg(test)]
    pub fn disabled() -> Self {
        let jwks_client = Arc::new(
            JwksClient::new("http://localhost:0/.well-known/jwks.json".to_string())
                .expect("Failed to create dummy JWKS client"),
        );
        Self {
            jwt_validator: Arc::new(CommonJwtValidator::new(jwks_client, 300)),
            require_auth: false,
        }
    }
}

impl<S> Layer<S> for McAuthLayer {
    type Service = McAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        McAuthService {
            inner,
            jwt_validator: Arc::clone(&self.jwt_validator),
            require_auth: self.require_auth,
        }
    }
}

/// Async authentication service wrapping an inner gRPC service.
///
/// Validates MH service tokens via JWKS before forwarding to the inner service.
#[derive(Clone)]
pub struct McAuthService<S> {
    inner: S,
    jwt_validator: Arc<CommonJwtValidator>,
    require_auth: bool,
}

impl<S> Service<http::Request<BoxBody>> for McAuthService<S>
where
    S: Service<http::Request<BoxBody>, Response = http::Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<BoxBody>) -> Self::Future {
        // Clone the inner service for use in the async block
        let mut inner = self.inner.clone();
        // Swap so self.inner is the ready clone (per tower Service contract)
        std::mem::swap(&mut self.inner, &mut inner);

        let jwt_validator = Arc::clone(&self.jwt_validator);
        let require_auth = self.require_auth;

        Box::pin(async move {
            if !require_auth {
                return inner.call(request).await;
            }

            // Extract authorization header
            let Some(auth_header) = request.headers().get("authorization") else {
                tracing::debug!(target: "mc.grpc.auth", "Missing authorization metadata");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            };

            let Ok(auth_str) = auth_header.to_str() else {
                tracing::debug!(target: "mc.grpc.auth", "Authorization header not valid ASCII");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            };

            // Extract Bearer token
            let Some(token) = auth_str.strip_prefix("Bearer ") else {
                tracing::debug!(target: "mc.grpc.auth", "Invalid authorization format");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            };

            // Structural fast-path checks
            if token.is_empty() {
                tracing::debug!(target: "mc.grpc.auth", "Empty token");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            }

            if token.len() > MAX_JWT_SIZE_BYTES {
                tracing::debug!(
                    target: "mc.grpc.auth",
                    token_size = token.len(),
                    "Token exceeds size limit"
                );
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            }

            // Cryptographic validation via JWKS
            let claims: ServiceClaims = match jwt_validator.validate(token).await {
                Ok(claims) => {
                    metrics::record_jwt_validation("success", "service");
                    claims
                }
                Err(e) => {
                    tracing::warn!(
                        target: "mc.grpc.auth",
                        error = ?e,
                        "Service token validation failed"
                    );
                    metrics::record_jwt_validation("failure", "service");
                    let response = tonic::Status::unauthenticated("Invalid token").into_http();
                    return Ok(response);
                }
            };

            // Scope authorization check (ADR-0003)
            if !claims.has_scope(REQUIRED_SCOPE) {
                tracing::warn!(
                    target: "mc.grpc.auth",
                    scope = %claims.scope,
                    required = REQUIRED_SCOPE,
                    "Service token missing required scope"
                );
                metrics::record_jwt_validation("failure", "service");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            }

            tracing::trace!(
                target: "mc.grpc.auth",
                "Service token validated successfully"
            );

            inner.call(request).await
        })
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

    // ========================================================================
    // McAuthLayer / McAuthService async tests (JWKS-based validation)
    // ========================================================================

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use chrono::Utc;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use tower::ServiceExt;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct TestKeypair {
        kid: String,
        public_key_bytes: Vec<u8>,
        private_key_pkcs8: Vec<u8>,
    }

    impl TestKeypair {
        fn new(seed: u8, kid: &str) -> Self {
            let mut seed_bytes = [0u8; 32];
            seed_bytes[0] = seed;
            for (i, byte) in seed_bytes.iter_mut().enumerate().skip(1) {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "Seed bytes are max 32 elements, fits in u8"
                )]
                {
                    *byte = seed.wrapping_mul(i as u8).wrapping_add(i as u8);
                }
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

        fn sign_token<T: serde::Serialize>(&self, claims: &T) -> String {
            let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
            let mut header = Header::new(Algorithm::EdDSA);
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

    async fn setup_auth_layer() -> (MockServer, TestKeypair, McAuthLayer) {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(42, "mc-auth-test-key-01");

        let jwks_response = serde_json::json!({
            "keys": [keypair.jwk_json()]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let jwks_url = format!("{}/.well-known/jwks.json", mock_server.uri());
        let jwks_client =
            Arc::new(JwksClient::new(jwks_url).expect("Failed to create JWKS client"));
        let layer = McAuthLayer::new(jwks_client, 300);

        (mock_server, keypair, layer)
    }

    fn make_service_claims(scope: &str) -> ServiceClaims {
        let now = Utc::now().timestamp();
        ServiceClaims::new(
            "mh-service".to_string(),
            now + 3600,
            now,
            scope.to_string(),
            Some("mh".to_string()),
        )
    }

    /// A no-op inner service that returns an empty OK response.
    #[derive(Clone)]
    struct NoopService;

    impl Service<http::Request<BoxBody>> for NoopService {
        type Response = http::Response<BoxBody>;
        type Error = Box<dyn std::error::Error + Send + Sync>;
        type Future =
            Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: http::Request<BoxBody>) -> Self::Future {
            Box::pin(async { Ok(http::Response::new(BoxBody::default())) })
        }
    }

    /// Helper to assert a response has gRPC UNAUTHENTICATED status.
    fn assert_unauthenticated(response: &http::Response<BoxBody>, context: &str) {
        let status = tonic::Status::from_header_map(response.headers());
        assert!(
            status.is_some(),
            "{context}: expected gRPC status header in response"
        );
        assert_eq!(
            status.unwrap().code(),
            tonic::Code::Unauthenticated,
            "{context}: expected UNAUTHENTICATED"
        );
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_missing_auth_header() {
        let (_mock_server, _keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let request = http::Request::builder().body(BoxBody::default()).unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "missing auth header");
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_invalid_bearer_format() {
        let (_mock_server, _keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let request = http::Request::builder()
            .header("authorization", "Basic dXNlcjpwYXNz")
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "invalid Bearer format");
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_empty_token() {
        let (_mock_server, _keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let request = http::Request::builder()
            .header("authorization", "Bearer ")
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "empty token");
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_oversized_token() {
        let (_mock_server, _keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let oversized = "a".repeat(8193);
        let request = http::Request::builder()
            .header("authorization", format!("Bearer {oversized}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "oversized token");
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_invalid_signature() {
        let (_mock_server, _keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Sign with a different key than what JWKS serves
        let wrong_keypair = TestKeypair::new(99, "wrong-key");
        let claims = make_service_claims("service.write.mc");
        let token = wrong_keypair.sign_token(&claims);

        let request = http::Request::builder()
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "invalid signature");
    }

    #[tokio::test]
    async fn test_auth_layer_accepts_valid_token() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let claims = make_service_claims("service.write.mc");
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        let status = tonic::Status::from_header_map(response.headers());

        // No gRPC error status means the request passed through to inner service
        assert!(
            status.is_none(),
            "Valid token should pass through, got: {status:?}"
        );
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_wrong_scope() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Token with wrong scope should be rejected (ADR-0003)
        let claims = make_service_claims("service.write.gc");
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "wrong scope");
    }

    #[tokio::test]
    async fn test_auth_layer_disabled_skips_validation() {
        let layer = McAuthLayer::disabled();
        let mut svc = layer.layer(NoopService);

        // No auth header at all
        let request = http::Request::builder().body(BoxBody::default()).unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        let status = tonic::Status::from_header_map(response.headers());

        assert!(
            status.is_none(),
            "Disabled layer should pass through without auth"
        );
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_expired_token() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Create expired token
        let now = Utc::now().timestamp();
        let claims = ServiceClaims::new(
            "mh-service".to_string(),
            now - 3600, // expired 1 hour ago
            now - 7200,
            "service.write.mc".to_string(),
            Some("mh".to_string()),
        );
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "expired token");
    }
}
