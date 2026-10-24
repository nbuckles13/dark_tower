//! P1 Tests: Authentication Flows
//!
//! Comprehensive authentication flow tests including token validation and
//! cross-replica consistency.
//!
//! Expiry is NOT tested here: an env-test cannot mint an expired token (no
//! signing key) and AC's lifetime is an hour. The issued `exp`/`iat` and the
//! lifetime are checked in `25_auth_security.rs`
//! (`test_iat_claim_is_current`, `test_token_lifetime_is_reasonable`); that
//! services REJECT an expired token is tested where one can be minted:
//! `gc-service/tests/auth_tests.rs::test_me_endpoint_rejects_expired_token`
//! and `mc-service/tests/join_tests.rs::test_join_expired_token_returns_unauthorized`.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "flows")]

use base64::Engine;
use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::{TokenRequest, DEV_TEST_CLIENT_ID, DEV_TEST_CLIENT_SECRET};
use env_tests::fixtures::AuthClient;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims structure for validation.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
    iat: i64,
    scope: String,
    aud: Option<Vec<String>>,
}

/// Helper to create a cluster connection for tests.
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

#[tokio::test]
async fn test_token_validates_against_jwks() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a token
    let request =
        TokenRequest::client_credentials(DEV_TEST_CLIENT_ID, DEV_TEST_CLIENT_SECRET, "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Fetch JWKS
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS fetch should succeed");

    // Decode token header to get kid
    let header =
        decode_header(&token_response.access_token).expect("Token header should be decodable");

    let kid = header.kid.expect("Token should have kid in header");

    // Find matching key in JWKS
    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == kid)
        .expect("JWKS should contain key with matching kid");

    // Extract public key coordinates
    let x = jwk.x.as_ref().expect("JWK should have x coordinate");

    // Decode base64url-encoded public key
    let public_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(x)
        .expect("Public key should be valid base64");

    // Create decoding key for Ed25519
    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    // Validate token
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;
    validation.validate_nbf = false;
    validation.set_audience(&["dark-tower-services"]);

    let token_data = decode::<Claims>(&token_response.access_token, &decoding_key, &validation)
        .expect("Token should validate against JWKS public key");

    assert_eq!(token_data.claims.sub, DEV_TEST_CLIENT_ID);
    assert_eq!(token_data.claims.scope, "test:all");
}

#[tokio::test]
async fn test_token_has_expected_claims() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a token
    let request =
        TokenRequest::client_credentials(DEV_TEST_CLIENT_ID, DEV_TEST_CLIENT_SECRET, "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Decode token without validation to inspect claims
    let header =
        decode_header(&token_response.access_token).expect("Token header should be decodable");

    // Verify header fields
    assert_eq!(header.alg, Algorithm::EdDSA);
    assert!(header.kid.is_some(), "Token should have kid in header");

    // Note: Full claim validation is done in test_token_validates_against_jwks
    // This test focuses on structural requirements
}

#[tokio::test]
async fn test_cross_replica_token_consistency() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue multiple tokens through the load-balanced service
    // With 2 replicas, some requests will hit different pods
    let mut tokens = Vec::new();

    for _ in 0..10 {
        let request = TokenRequest::client_credentials(
            DEV_TEST_CLIENT_ID,
            DEV_TEST_CLIENT_SECRET,
            "test:all",
        );

        let token_response = auth_client
            .issue_token(request)
            .await
            .expect("Token issuance should succeed");

        tokens.push(token_response.access_token);
    }

    // Fetch JWKS (should be consistent across replicas)
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS fetch should succeed");

    // Validate all tokens against the JWKS
    // If cross-replica key sync is broken, some tokens will fail validation
    for token in &tokens {
        let header = decode_header(token).expect("Token header should be decodable");

        let kid = header.kid.expect("Token should have kid in header");

        let jwk = jwks
            .keys
            .iter()
            .find(|k| k.kid == kid)
            .expect("JWKS should contain key for all issued tokens");

        let x = jwk.x.as_ref().expect("JWK should have x coordinate");
        let public_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(x)
            .expect("Public key should be valid base64");

        let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_exp = true;
        validation.set_audience(&["dark-tower-services"]);

        decode::<Claims>(token, &decoding_key, &validation)
            .expect("All tokens should validate against JWKS");
    }
}

#[tokio::test]
async fn test_invalid_signature_rejected() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a valid token
    let request =
        TokenRequest::client_credentials(DEV_TEST_CLIENT_ID, DEV_TEST_CLIENT_SECRET, "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Tamper with the token signature (last part after the second '.')
    let mut parts: Vec<&str> = token_response.access_token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 parts");

    // Corrupt the signature
    parts[2] = "invalid_signature_data_that_wont_verify";
    let tampered_token = parts.join(".");

    // Fetch JWKS
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS fetch should succeed");

    let header =
        decode_header(&token_response.access_token).expect("Token header should be decodable");

    let kid = header.kid.expect("Token should have kid in header");

    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == kid)
        .expect("JWKS should contain key with matching kid");

    let x = jwk.x.as_ref().expect("JWK should have x coordinate");
    let public_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(x)
        .expect("Public key should be valid base64");

    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;
    validation.set_audience(&["dark-tower-services"]);

    // Validation should fail
    let result = decode::<Claims>(&tampered_token, &decoding_key, &validation);
    assert!(
        result.is_err(),
        "Token with invalid signature should be rejected"
    );
}
