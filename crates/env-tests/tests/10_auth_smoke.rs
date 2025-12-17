//! P0 Smoke Tests: Authentication
//!
//! Basic authentication flow tests using pre-seeded credentials.

#![cfg(feature = "smoke")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::TokenRequest;
use env_tests::fixtures::AuthClient;

/// Helper to create a cluster connection for tests.
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

#[tokio::test]
async fn test_token_issuance_with_valid_credentials() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed with valid credentials");

    assert_eq!(token_response.token_type, "Bearer");
    assert!(!token_response.access_token.is_empty());
    assert!(token_response.expires_in > 0);
    assert_eq!(token_response.scope, "test:all");
}

#[tokio::test]
async fn test_token_issuance_rejected_invalid_credentials() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    let request = TokenRequest::client_credentials("test-client", "wrong-secret", "test:all");

    let result = auth_client.issue_token(request).await;

    assert!(
        result.is_err(),
        "Token issuance should fail with invalid credentials"
    );

    let error = result.unwrap_err();
    let error_string = error.to_string();
    assert!(
        error_string.contains("401") || error_string.contains("403"),
        "Should return 401 Unauthorized or 403 Forbidden, got: {}",
        error_string
    );
}

#[tokio::test]
async fn test_jwks_endpoint_returns_keys() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS endpoint should return keys");

    assert!(
        !jwks.keys.is_empty(),
        "JWKS should contain at least one key"
    );
}

#[tokio::test]
async fn test_jwks_keys_are_valid_format() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS endpoint should return keys");

    for key in &jwks.keys {
        // Validate required fields
        assert_eq!(key.kty, "OKP", "Key type should be OKP for Ed25519");
        assert!(!key.kid.is_empty(), "Key ID should not be empty");

        // Validate algorithm
        if let Some(ref alg) = key.alg {
            assert_eq!(alg, "EdDSA", "Algorithm should be EdDSA");
        }

        // Validate curve for EdDSA keys
        if let Some(ref crv) = key.crv {
            assert_eq!(crv, "Ed25519", "Curve should be Ed25519");
        }

        // Validate public key coordinates are present
        assert!(key.x.is_some(), "Public key x coordinate should be present");

        // Validate key use
        if let Some(ref key_use) = key.key_use {
            assert_eq!(key_use, "sig", "Key use should be 'sig' for signatures");
        }
    }
}
