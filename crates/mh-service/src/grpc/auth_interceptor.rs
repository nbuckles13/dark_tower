//! gRPC authentication for MH service.
//!
//! Provides JWKS-based cryptographic validation of MC's OAuth service tokens
//! on incoming gRPC requests. Uses an async `tower::Layer`/`tower::Service`
//! to support async JWKS cache lookups.
//!
//! # Security
//!
//! - All gRPC requests from MC require valid Bearer token
//! - Tokens are validated cryptographically via `JwtValidator<ServiceClaims>`
//! - Scope authorization enforced (`service.write.mh`)
//! - Generic error messages prevent information leakage
//! - Failed authentication returns UNAUTHENTICATED status
//!
//! # Validation Chain
//!
//! 1. Structural fast-path: format, non-empty, size limit (8KB)
//! 2. Cryptographic: `EdDSA` signature via JWKS
//! 3. Claims: exp, iat with clock skew tolerance
//! 4. Authorization: `service.write.mh` scope check

use crate::auth::CommonJwtValidator;
use crate::observability::metrics;
use axum::http;
use common::jwt::{JwksClient, ServiceClaims, MAX_JWT_SIZE_BYTES};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tonic::body::BoxBody;
use tower::{Layer, Service};

/// Required scope for MC→MH gRPC operations (ADR-0003).
const REQUIRED_SCOPE: &str = "service.write.mh";

/// Async authentication layer for MH gRPC service.
///
/// Wraps an inner service and validates Bearer tokens cryptographically
/// via JWKS before forwarding requests.
#[derive(Clone)]
pub struct MhAuthLayer {
    jwt_validator: Arc<CommonJwtValidator>,
    require_auth: bool,
}

impl MhAuthLayer {
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
        // Create a dummy validator — it won't be used since require_auth is false
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

impl<S> Layer<S> for MhAuthLayer {
    type Service = MhAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MhAuthService {
            inner,
            jwt_validator: Arc::clone(&self.jwt_validator),
            require_auth: self.require_auth,
        }
    }
}

/// Async authentication service wrapping an inner gRPC service.
#[derive(Clone)]
pub struct MhAuthService<S> {
    inner: S,
    jwt_validator: Arc<CommonJwtValidator>,
    require_auth: bool,
}

impl<S> Service<http::Request<BoxBody>> for MhAuthService<S>
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
                tracing::debug!(target: "mh.grpc.auth", "Missing authorization metadata");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            };

            let Ok(auth_str) = auth_header.to_str() else {
                tracing::debug!(target: "mh.grpc.auth", "Authorization header not valid ASCII");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            };

            // Extract Bearer token
            let Some(token) = auth_str.strip_prefix("Bearer ") else {
                tracing::debug!(target: "mh.grpc.auth", "Invalid authorization format");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            };

            // Structural fast-path checks
            if token.is_empty() {
                tracing::debug!(target: "mh.grpc.auth", "Empty token");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            }

            if token.len() > MAX_JWT_SIZE_BYTES {
                tracing::debug!(
                    target: "mh.grpc.auth",
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
                        target: "mh.grpc.auth",
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
                    target: "mh.grpc.auth",
                    scope = %claims.scope,
                    required = REQUIRED_SCOPE,
                    "Service token missing required scope"
                );
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            }

            tracing::trace!(
                target: "mh.grpc.auth",
                "Service token validated successfully"
            );

            inner.call(request).await
        })
    }
}

// Legacy re-export: MhAuthInterceptor is kept as a type alias for backward
// compatibility with existing code that references it. The actual auth is now
// performed by MhAuthLayer/MhAuthService.
/// Legacy auth interceptor (structural-only, replaced by `MhAuthLayer`).
///
/// Kept for backward compatibility. New code should use `MhAuthLayer`.
#[derive(Clone, Debug)]
pub struct MhAuthInterceptor {
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
        reason = "Legacy interceptor method; kept for backward compatibility until full MhAuthService migration"
    )]
    fn extract_token<'a>(
        &self,
        auth_value: &'a tonic::metadata::MetadataValue<tonic::metadata::Ascii>,
    ) -> Option<&'a str> {
        let auth_str = auth_value.to_str().ok()?;
        auth_str.strip_prefix("Bearer ")
    }
}

impl tonic::service::Interceptor for MhAuthInterceptor {
    #[tracing::instrument(skip_all, name = "mh.grpc.auth_interceptor")]
    fn call(&mut self, request: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        if !self.require_auth {
            return Ok(request);
        }

        let auth_header = request.metadata().get("authorization").ok_or_else(|| {
            tracing::debug!(target: "mh.grpc.auth", "Missing authorization metadata");
            tonic::Status::unauthenticated("Missing authorization header")
        })?;

        let token = self.extract_token(auth_header).ok_or_else(|| {
            tracing::debug!(target: "mh.grpc.auth", "Invalid authorization format");
            tonic::Status::unauthenticated("Invalid authorization format")
        })?;

        if token.is_empty() {
            tracing::debug!(target: "mh.grpc.auth", "Empty token");
            return Err(tonic::Status::unauthenticated("Empty token"));
        }

        if token.len() > MAX_JWT_SIZE_BYTES {
            tracing::debug!(
                target: "mh.grpc.auth",
                token_size = token.len(),
                "Token exceeds size limit"
            );
            return Err(tonic::Status::unauthenticated("Invalid token"));
        }

        tracing::trace!(
            target: "mh.grpc.auth",
            token_len = token.len(),
            "Authorization validated (structural)"
        );

        Ok(request)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tonic::service::Interceptor;

    fn create_request_with_auth(auth_value: &str) -> tonic::Request<()> {
        let mut request = tonic::Request::new(());
        request
            .metadata_mut()
            .insert("authorization", auth_value.parse().unwrap());
        request
    }

    fn create_request_without_auth() -> tonic::Request<()> {
        tonic::Request::new(())
    }

    // Legacy MhAuthInterceptor tests (structural validation)

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
    }

    #[test]
    fn test_interceptor_invalid_auth_format_basic() {
        let mut interceptor = MhAuthInterceptor::new();
        let request = create_request_with_auth("Basic dXNlcjpwYXNz");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_interceptor_empty_token() {
        let mut interceptor = MhAuthInterceptor::new();
        let request = create_request_with_auth("Bearer ");

        let result = interceptor.call(request);

        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
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

    #[test]
    fn test_required_scope_constant() {
        assert_eq!(REQUIRED_SCOPE, "service.write.mh");
    }

    // MhAuthService async tests (full JWKS-based validation)

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

    async fn setup_auth_layer() -> (MockServer, TestKeypair, MhAuthLayer) {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(42, "auth-test-key-01");

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
        let layer = MhAuthLayer::new(jwks_client, 300);

        (mock_server, keypair, layer)
    }

    fn make_service_claims(scope: &str) -> common::jwt::ServiceClaims {
        let now = Utc::now().timestamp();
        common::jwt::ServiceClaims::new(
            "mc-service".to_string(),
            now + 3600,
            now,
            scope.to_string(),
            Some("mc".to_string()),
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
    async fn test_auth_service_rejects_missing_auth_header() {
        let (_mock_server, _keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let request = http::Request::builder().body(BoxBody::default()).unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "missing auth header");
    }

    #[tokio::test]
    async fn test_auth_service_rejects_invalid_bearer_format() {
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
    async fn test_auth_service_rejects_empty_token() {
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
    async fn test_auth_service_rejects_oversized_token() {
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
    async fn test_auth_service_rejects_invalid_signature() {
        let (_mock_server, _keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Sign with a different key than what JWKS serves
        let wrong_keypair = TestKeypair::new(99, "wrong-key");
        let claims = make_service_claims("service.write.mh");
        let token = wrong_keypair.sign_token(&claims);

        let request = http::Request::builder()
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "invalid signature");
    }

    #[tokio::test]
    async fn test_auth_service_rejects_missing_scope() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Token with wrong scope (not service.write.mh)
        let claims = make_service_claims("service.read.mh");
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        let status = tonic::Status::from_header_map(response.headers());

        assert!(status.is_some(), "Expected gRPC status header in response");
        assert_eq!(
            status.unwrap().code(),
            tonic::Code::Unauthenticated,
            "Token without service.write.mh scope should be rejected"
        );
    }

    #[tokio::test]
    async fn test_auth_service_accepts_valid_scope() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Token with correct scope
        let claims = make_service_claims("service.write.mh");
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
            "Valid token with correct scope should pass through, got: {status:?}"
        );
    }
}
