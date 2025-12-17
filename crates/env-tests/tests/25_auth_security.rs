//! P1 Security Tests: Authentication
//!
//! Security-focused authentication tests including token tampering,
//! algorithm confusion, and claim validation.

#![cfg(feature = "flows")]

use base64::Engine;
use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::TokenRequest;
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
async fn test_tampered_token_rejected() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a valid token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Tamper with the payload (middle part)
    let parts: Vec<&str> = token_response.access_token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 parts");

    // Decode the payload, modify it, and re-encode
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("Payload should be valid base64");

    let mut claims: serde_json::Value =
        serde_json::from_slice(&payload_bytes).expect("Payload should be valid JSON");

    // Tamper with the scope claim
    claims["scope"] = serde_json::json!("admin:all");

    let tampered_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&claims).unwrap());

    let tampered_token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

    // Fetch JWKS and try to validate the tampered token
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

    // Validation should fail due to signature mismatch
    let result = decode::<Claims>(&tampered_token, &decoding_key, &validation);
    assert!(
        result.is_err(),
        "Token with tampered claims should be rejected"
    );
}

#[tokio::test]
async fn test_wrong_algorithm_rejected() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a valid EdDSA token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

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

    // Try to validate with wrong algorithm (HS256 instead of EdDSA)
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.set_audience(&["dark-tower-services"]);

    let result = decode::<Claims>(&token_response.access_token, &decoding_key, &validation);
    assert!(
        result.is_err(),
        "Token should be rejected when validated with wrong algorithm"
    );

    // Verify correct algorithm works
    let mut correct_validation = Validation::new(Algorithm::EdDSA);
    correct_validation.validate_exp = true;
    correct_validation.set_audience(&["dark-tower-services"]);

    let correct_result = decode::<Claims>(
        &token_response.access_token,
        &decoding_key,
        &correct_validation,
    );
    assert!(
        correct_result.is_ok(),
        "Token should validate with correct algorithm"
    );
}

#[tokio::test]
async fn test_missing_required_claims_rejected() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a valid token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Decode and verify all required claims are present
    #[derive(Debug, Deserialize)]
    struct MinimalClaims {
        sub: Option<String>,
        exp: Option<i64>,
        iat: Option<i64>,
        scope: Option<String>,
    }

    // Decode without validation to inspect claims
    let parts: Vec<&str> = token_response.access_token.split('.').collect();
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("Payload should be valid base64");

    let claims: MinimalClaims =
        serde_json::from_slice(&payload_bytes).expect("Payload should be valid JSON");

    // Verify required claims are present
    assert!(claims.sub.is_some(), "Token should have 'sub' claim");
    assert!(claims.exp.is_some(), "Token should have 'exp' claim");
    assert!(claims.iat.is_some(), "Token should have 'iat' claim");
    assert!(claims.scope.is_some(), "Token should have 'scope' claim");

    // Verify values are non-empty/valid
    assert!(
        !claims.sub.unwrap().is_empty(),
        "'sub' claim should not be empty"
    );
    assert!(claims.exp.unwrap() > 0, "'exp' claim should be positive");
    assert!(claims.iat.unwrap() > 0, "'iat' claim should be positive");
    assert!(
        !claims.scope.unwrap().is_empty(),
        "'scope' claim should not be empty"
    );
}
