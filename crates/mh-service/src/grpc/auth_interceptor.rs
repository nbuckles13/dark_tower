//! gRPC authentication for MH service.
//!
//! Provides async JWKS-based two-layer authentication for all incoming gRPC calls:
//!
//! - `MhAuthLayer`/`MhAuthService`: Tower layer applied at the gRPC server level.
//!   Validates service tokens cryptographically via JWKS, then enforces caller-type
//!   routing based on `service_type` claim (ADR-0003).
//!
//! # Security
//!
//! - All gRPC requests require valid Bearer token
//! - Tokens validated cryptographically via `JwtValidator<ServiceClaims>`
//! - Layer 1: Scope authorization enforced (`service.write.mh`) (ADR-0003)
//! - Layer 2: Caller `service_type` must match gRPC service being called
//! - Validated `ServiceClaims` injected into request extensions for downstream use
//! - Generic error messages prevent information leakage
//! - Failed authentication returns UNAUTHENTICATED status
//! - Failed authorization (valid token, wrong caller) returns `PERMISSION_DENIED` status
//!
//! # Validation Chain (`MhAuthLayer`)
//!
//! 1. Structural fast-path: format, non-empty, size limit (8KB)
//! 2. Cryptographic: `EdDSA` signature via JWKS
//! 3. Claims: exp, iat with clock skew tolerance
//! 4. Authorization: `service.write.mh` scope check
//! 5. Routing: `service_type` must match target gRPC service (fail closed)

use crate::auth::CommonJwtValidator;
use crate::observability::metrics;
use axum::http;
use common::jwt::{JwksClient, JwtError, ServiceClaims, MAX_JWT_SIZE_BYTES};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tonic::body::BoxBody;
use tower::{Layer, Service};

/// Required scope for MC→MH gRPC operations (ADR-0003).
const REQUIRED_SCOPE: &str = "service.write.mh";

/// Map a `JwtError` to a bounded `failure_reason` label for the
/// `mh_jwt_validations_total` metric.
fn classify_jwt_error(err: &JwtError) -> &'static str {
    match err {
        JwtError::TokenTooLarge | JwtError::MalformedToken | JwtError::MissingKid => "malformed",
        JwtError::InvalidSignature | JwtError::KeyNotFound | JwtError::ServiceUnavailable(_) => {
            "signature_invalid"
        }
        JwtError::IatTooFarInFuture => "expired",
    }
}

/// Async authentication layer for MH gRPC service (R-22).
///
/// Wraps an inner service and validates service tokens cryptographically
/// via JWKS before forwarding requests. Applied at the server level to
/// authenticate MC→MH calls.
///
/// Enforces `service.write.mh` scope authorization after JWKS validation.
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
///
/// Validates service tokens via JWKS and enforces caller-type routing
/// before forwarding to the inner service.
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

    #[allow(clippy::too_many_lines)]
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
                    metrics::record_jwt_validation("success", "service", "none");
                    claims
                }
                Err(e) => {
                    let reason = classify_jwt_error(&e);
                    tracing::warn!(
                        target: "mh.grpc.auth",
                        error = ?e,
                        failure_reason = %reason,
                        "Service token validation failed"
                    );
                    metrics::record_jwt_validation("failure", "service", reason);
                    let response = tonic::Status::unauthenticated("Invalid token").into_http();
                    return Ok(response);
                }
            };

            // Layer 1: Scope authorization check (ADR-0003)
            if !claims.has_scope(REQUIRED_SCOPE) {
                tracing::warn!(
                    target: "mh.grpc.auth",
                    scope = %claims.scope,
                    required = REQUIRED_SCOPE,
                    "Service token missing required scope"
                );
                metrics::record_jwt_validation("failure", "service", "scope_mismatch");
                let response = tonic::Status::unauthenticated("Invalid token").into_http();
                return Ok(response);
            }

            // Layer 2: service_type routing (ADR-0003)
            // Match the gRPC service path to the expected caller service_type.
            let grpc_path = request.uri().path();
            let expected_type =
                if grpc_path.starts_with("/dark_tower.internal.MediaHandlerService/") {
                    "meeting-controller"
                } else {
                    // Unknown gRPC service path — fail closed
                    tracing::warn!(
                        target: "mh.grpc.auth",
                        path = %grpc_path,
                        "Unknown gRPC service path, rejecting"
                    );
                    let response = tonic::Status::permission_denied("Access denied").into_http();
                    return Ok(response);
                };

            let actual_type = claims.service_type.as_deref().unwrap_or("unknown");
            if actual_type != expected_type {
                tracing::warn!(
                    target: "mh.grpc.auth",
                    grpc_service = %grpc_path,
                    expected = %expected_type,
                    actual = %actual_type,
                    "Caller service_type does not match target gRPC service"
                );
                metrics::record_caller_type_rejected(
                    "MediaHandlerService",
                    expected_type,
                    actual_type,
                );
                let response = tonic::Status::permission_denied("Access denied").into_http();
                return Ok(response);
            }

            tracing::trace!(
                target: "mh.grpc.auth",
                service_type = %actual_type,
                "Service token validated successfully"
            );

            // Inject validated claims into request extensions for downstream handlers
            let mut request = request;
            request.extensions_mut().insert(claims);

            inner.call(request).await
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ========================================================================
    // MhAuthLayer / MhAuthService async tests (JWKS-based validation)
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

    async fn setup_auth_layer() -> (MockServer, TestKeypair, MhAuthLayer) {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(42, "mh-auth-test-key-01");

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

    /// MC→MH gRPC path for `MediaHandlerService`
    const MC_GRPC_PATH: &str = "/dark_tower.internal.MediaHandlerService/RegisterMeeting";

    fn make_service_claims(scope: &str, service_type: Option<&str>) -> ServiceClaims {
        let now = Utc::now().timestamp();
        ServiceClaims::new(
            "test-service".to_string(),
            now + 3600,
            now,
            scope.to_string(),
            service_type.map(String::from),
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

    /// Helper to assert a response has gRPC `PERMISSION_DENIED` status.
    fn assert_permission_denied(response: &http::Response<BoxBody>, context: &str) {
        let status = tonic::Status::from_header_map(response.headers());
        assert!(
            status.is_some(),
            "{context}: expected gRPC status header in response"
        );
        assert_eq!(
            status.unwrap().code(),
            tonic::Code::PermissionDenied,
            "{context}: expected PERMISSION_DENIED"
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
        let claims = make_service_claims("service.write.mh", Some("meeting-controller"));
        let token = wrong_keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri(MC_GRPC_PATH)
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "invalid signature");
    }

    #[tokio::test]
    async fn test_auth_layer_accepts_valid_mc_token() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let claims = make_service_claims("service.write.mh", Some("meeting-controller"));
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri(MC_GRPC_PATH)
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        let status = tonic::Status::from_header_map(response.headers());

        assert!(
            status.is_none(),
            "Valid MC token should pass through, got: {status:?}"
        );
    }

    #[tokio::test]
    async fn test_auth_layer_rejects_wrong_scope() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Token with wrong scope should be rejected (ADR-0003)
        let claims = make_service_claims("service.write.gc", Some("meeting-controller"));
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri(MC_GRPC_PATH)
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "wrong scope");
    }

    #[tokio::test]
    async fn test_auth_layer_disabled_skips_validation() {
        let layer = MhAuthLayer::disabled();
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
            "mc-service".to_string(),
            now - 3600, // expired 1 hour ago
            now - 7200,
            "service.write.mh".to_string(),
            Some("meeting-controller".to_string()),
        );
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri(MC_GRPC_PATH)
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_unauthenticated(&response, "expired token");
    }

    // ========================================================================
    // Layer 2: service_type routing tests (ADR-0003)
    // ========================================================================

    #[tokio::test]
    async fn test_layer2_rejects_gc_calling_media_handler_service() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // GC token trying to call MediaHandlerService (should be MC only)
        let claims = make_service_claims("service.write.mh", Some("global-controller"));
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri(MC_GRPC_PATH)
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_permission_denied(&response, "GC calling MediaHandlerService");
    }

    #[tokio::test]
    async fn test_layer2_rejects_no_service_type_fail_closed() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        // Token with no service_type should be rejected (fail closed)
        let claims = make_service_claims("service.write.mh", None);
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri(MC_GRPC_PATH)
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_permission_denied(&response, "no service_type (fail closed)");
    }

    #[tokio::test]
    async fn test_layer2_rejects_unknown_grpc_path() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;
        let mut svc = layer.layer(NoopService);

        let claims = make_service_claims("service.write.mh", Some("meeting-controller"));
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri("/dark_tower.internal.UnknownService/SomeMethod")
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        assert_permission_denied(&response, "unknown gRPC service path");
    }

    #[tokio::test]
    async fn test_claims_injected_into_request_extensions() {
        let (_mock_server, keypair, layer) = setup_auth_layer().await;

        // Use a custom inner service that checks for claims in extensions
        #[allow(clippy::items_after_statements)]
        #[derive(Clone)]
        struct ClaimsCheckService;

        impl Service<http::Request<BoxBody>> for ClaimsCheckService {
            type Response = http::Response<BoxBody>;
            type Error = Box<dyn std::error::Error + Send + Sync>;
            type Future =
                Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

            fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, request: http::Request<BoxBody>) -> Self::Future {
                let has_claims = request.extensions().get::<ServiceClaims>().is_some();
                Box::pin(async move {
                    if has_claims {
                        Ok(http::Response::new(BoxBody::default()))
                    } else {
                        // Return an error response to indicate claims were missing
                        let response =
                            tonic::Status::internal("Claims not found in extensions").into_http();
                        Ok(response)
                    }
                })
            }
        }

        let mut svc = layer.layer(ClaimsCheckService);

        let claims = make_service_claims("service.write.mh", Some("meeting-controller"));
        let token = keypair.sign_token(&claims);

        let request = http::Request::builder()
            .uri(MC_GRPC_PATH)
            .header("authorization", format!("Bearer {token}"))
            .body(BoxBody::default())
            .unwrap();

        let response = svc.ready().await.unwrap().call(request).await.unwrap();
        let status = tonic::Status::from_header_map(response.headers());

        // No error status means claims were found in extensions
        assert!(
            status.is_none(),
            "Claims should be injected into request extensions, got: {status:?}"
        );
    }

    #[test]
    fn test_required_scope_constant() {
        assert_eq!(REQUIRED_SCOPE, "service.write.mh");
    }
}
