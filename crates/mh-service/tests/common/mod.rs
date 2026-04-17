//! Shared test rigs and helpers for mh-service integration tests.
//!
//! This module centralizes the fixtures that the integration tests share:
//!
//! - [`TestKeypair`] / [`mount_jwks_mock`] — Ed25519 keypair + wiremock JWKS
//! - [`mock_mc`] — MC gRPC mock (MediaCoordinationService) with channel capture
//! - [`grpc_rig`] — Real `MhAuthLayer` + `MhMediaService` on `127.0.0.1:0`
//! - [`wt_rig`] — Real `WebTransportServer` with self-signed TLS
//! - [`tokens`] — MC service-token and meeting-token minting helpers
//! - [`wt_client`] — Thin WebTransport client wrapper
//!
//! # Integration value
//!
//! These tests wire components as `main.rs` does: real `MhAuthLayer` +
//! `MhMediaService` behind tonic, real `WebTransportServer` with production
//! TLS code path (self-signed cert flows through the same `Identity::load_*`
//! API at runtime). Unit-level matrices live in `auth_interceptor.rs::tests`,
//! `auth/mod.rs::tests`, `session/mod.rs::tests`, and `grpc/mh_service.rs::tests`.
//!
//! # JWKS caching
//!
//! `common::jwt::JwksClient` caches by URL in-process. Each test obtains a
//! fresh `wiremock::MockServer` (unique ephemeral port → unique URL) so there
//! is no cross-test cache interference.

// Test-helper modules are consumed selectively by each integration test
// binary, so some items are unused from any individual binary's perspective.
#![allow(dead_code, clippy::too_many_arguments)]

pub mod grpc_rig;
pub mod jwks_rig;
pub mod mock_mc;
pub mod tokens;
pub mod wt_client;
pub mod wt_rig;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Deterministic Ed25519 keypair for signing JWTs and publishing JWKs.
///
/// Inlined (rather than pulling `mc-test-utils` as a dev-dependency) to avoid
/// transitively dragging `mc-service` into the mh-service test build.
///
/// TODO: consolidate the four workspace copies (this file, `mc-test-utils`,
/// `src/grpc/auth_interceptor.rs::tests`, `src/auth/mod.rs::tests`) behind a
/// `common::jwt` `test-utils` feature, or a new `crates/common-test-utils`
/// crate that holds only the Ed25519 + JWKS primitives (no service deps).
pub struct TestKeypair {
    pub kid: String,
    pub public_key_bytes: Vec<u8>,
    pub private_key_pkcs8: Vec<u8>,
}

impl TestKeypair {
    pub fn new(seed: u8, kid: &str) -> Self {
        let mut seed_bytes = [0u8; 32];
        seed_bytes[0] = seed;
        for (i, byte) in seed_bytes.iter_mut().enumerate().skip(1) {
            #[allow(clippy::cast_possible_truncation)]
            {
                *byte = seed.wrapping_mul(i as u8).wrapping_add(i as u8);
            }
        }

        let key_pair = Ed25519KeyPair::from_seed_unchecked(&seed_bytes)
            .expect("failed to derive Ed25519 keypair from deterministic test seed");

        let public_key_bytes = key_pair.public_key().as_ref().to_vec();
        let private_key_pkcs8 = build_pkcs8_from_seed(&seed_bytes);

        Self {
            kid: kid.to_string(),
            public_key_bytes,
            private_key_pkcs8,
        }
    }

    pub fn sign_token<T: Serialize>(&self, claims: &T) -> String {
        let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.kid.clone());
        encode(&header, claims, &encoding_key).expect("failed to sign EdDSA test token")
    }

    pub fn jwk_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kty": "OKP",
            "kid": self.kid,
            "crv": "Ed25519",
            "x": URL_SAFE_NO_PAD.encode(&self.public_key_bytes),
            "alg": "EdDSA",
            "use": "sig",
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

/// Mount `/.well-known/jwks.json` on the mock server with the given keypair's JWK.
pub async fn mount_jwks_mock(mock_server: &MockServer, keypair: &TestKeypair) -> String {
    let jwks_response = serde_json::json!({
        "keys": [keypair.jwk_json()],
    });

    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
        .mount(mock_server)
        .await;

    format!("{}/.well-known/jwks.json", mock_server.uri())
}
