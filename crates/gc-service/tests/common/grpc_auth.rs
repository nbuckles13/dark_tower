//! Real GC gRPC auth for test rigs that wire the inbound stack through
//! `gc_service::grpc::server_layers` (the same function `main.rs` uses).
//!
//! The auth layer's JWT validation creates child spans under the request span,
//! which is exactly what made trace-context extraction order-sensitive under
//! tracing-opentelemetry ≥0.32 — so continuity rigs must run with it, not
//! without it.

use super::jwt_fixtures::{TestKeypair, TestServiceClaims};
use gc_service::auth::jwks::JwksClient;
use gc_service::auth::jwt::JwtValidator;
use gc_service::grpc::auth_layer::GrpcAuthLayer;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A JWKS mock serving one keypair, and the real `GrpcAuthLayer` validating
/// against it. Keep the `MockServer` alive for the rig's lifetime.
pub async fn grpc_auth() -> (MockServer, TestKeypair, GrpcAuthLayer) {
    let mock_server = MockServer::start().await;
    let keypair = TestKeypair::new(42, "gc-grpc-rig-key-01");
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "keys": [keypair.jwk_json()] })),
        )
        .mount(&mock_server)
        .await;
    let jwks_client = Arc::new(
        JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri()))
            .expect("JWKS client"),
    );
    let layer = GrpcAuthLayer::new(Arc::new(JwtValidator::new(jwks_client, 300)));
    (mock_server, keypair, layer)
}

/// `Bearer` metadata value for a valid MC→GC service token (scope
/// `service.write.gc`, `service_type` meeting-controller).
pub fn mc_bearer(keypair: &TestKeypair) -> tonic::metadata::MetadataValue<tonic::metadata::Ascii> {
    let now = chrono::Utc::now().timestamp();
    let token = keypair.sign_service_token(&TestServiceClaims {
        sub: "mc-otel-rig".to_string(),
        exp: now + 3600,
        iat: now,
        scope: "service.write.gc".to_string(),
        service_type: Some("meeting-controller".to_string()),
    });
    format!("Bearer {token}").parse().expect("bearer parses")
}
