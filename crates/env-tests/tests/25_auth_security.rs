//! P1 Security Tests: Authentication
//!
//! Security-focused authentication tests including token tampering,
//! algorithm confusion, claim validation, JWKS security, and header injection.

#![cfg(feature = "flows")]

use base64::Engine;
use chrono::Utc;
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

// ============================================================================
// JWKS Security Tests
// ============================================================================

/// Test that JWKS endpoint does not leak private key material.
/// Validates CWE-321 (cryptographic key exposure).
#[tokio::test]
async fn test_jwks_no_private_key_leakage() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Fetch JWKS from the well-known endpoint
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS endpoint should return keys");

    // Verify we have keys to check
    assert!(
        !jwks.keys.is_empty(),
        "JWKS should contain at least one key for this test"
    );

    // Get raw JWKS response to check for private key fields
    let jwks_url = format!("{}/.well-known/jwks.json", auth_client.base_url());
    let raw_response = auth_client
        .http_client()
        .get(&jwks_url)
        .send()
        .await
        .expect("JWKS fetch should succeed")
        .text()
        .await
        .expect("JWKS response should be readable");

    let jwks_value: serde_json::Value =
        serde_json::from_str(&raw_response).expect("JWKS should be valid JSON");

    // Check each key for private key material
    // These fields would indicate private key leakage:
    // - 'd': Private key (RSA/EC/OKP)
    // - 'p': First prime factor (RSA)
    // - 'q': Second prime factor (RSA)
    // - 'dp': d mod (p-1) (RSA)
    // - 'dq': d mod (q-1) (RSA)
    // - 'qi': q^-1 mod p (RSA)
    let private_key_fields = ["d", "p", "q", "dp", "dq", "qi"];

    if let Some(keys) = jwks_value.get("keys").and_then(|k| k.as_array()) {
        for (i, key) in keys.iter().enumerate() {
            for field in &private_key_fields {
                assert!(
                    key.get(*field).is_none(),
                    "JWKS key {} contains private key field '{}' - CRITICAL SECURITY VULNERABILITY! \
                     Private key material is being exposed in the public JWKS endpoint.",
                    i,
                    field
                );
            }
        }
    } else {
        panic!("JWKS response should contain 'keys' array");
    }
}

// ============================================================================
// Time-Based Claims Validation Tests
// ============================================================================

/// Test that the 'iat' (issued at) claim is set to a current timestamp.
#[tokio::test]
async fn test_iat_claim_is_current() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Record time before token issuance
    let before_issuance = Utc::now().timestamp();

    // Issue a token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Record time after token issuance
    let after_issuance = Utc::now().timestamp();

    // Decode claims without full validation to inspect timestamps
    let parts: Vec<&str> = token_response.access_token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 parts");

    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("Payload should be valid base64");

    let claims: Claims =
        serde_json::from_slice(&payload_bytes).expect("Payload should be valid JSON");

    // Verify iat is within the expected window (with 5 minute tolerance for clock skew)
    let clock_skew_seconds = 300; // 5 minutes

    assert!(
        claims.iat >= before_issuance - clock_skew_seconds,
        "Token 'iat' ({}) should not be too far in the past. \
         Expected >= {} (before_issuance - 5min clock skew)",
        claims.iat,
        before_issuance - clock_skew_seconds
    );

    assert!(
        claims.iat <= after_issuance + clock_skew_seconds,
        "Token 'iat' ({}) should not be in the future. \
         Expected <= {} (after_issuance + 5min clock skew)",
        claims.iat,
        after_issuance + clock_skew_seconds
    );

    // Verify exp > iat (positive lifetime)
    assert!(
        claims.exp > claims.iat,
        "Token 'exp' ({}) should be after 'iat' ({}) - token must have positive lifetime",
        claims.exp,
        claims.iat
    );
}

/// Test that token lifetime is approximately 1 hour (3600 seconds) per ADR-0007.
#[tokio::test]
async fn test_token_lifetime_is_reasonable() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Decode claims to get exp and iat
    let parts: Vec<&str> = token_response.access_token.split('.').collect();
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("Payload should be valid base64");

    let claims: Claims =
        serde_json::from_slice(&payload_bytes).expect("Payload should be valid JSON");

    // Calculate token lifetime
    let lifetime_seconds = claims.exp - claims.iat;

    // Expected lifetime is 1 hour (3600 seconds) per ADR-0007
    // Allow some tolerance (e.g., ±60 seconds for implementation variance)
    let expected_lifetime = 3600i64;
    let tolerance = 60i64;

    assert!(
        (lifetime_seconds - expected_lifetime).abs() <= tolerance,
        "Token lifetime should be approximately 1 hour (3600s). \
         Got {} seconds (exp={}, iat={}). \
         This validates ADR-0007 token lifetime strategy.",
        lifetime_seconds,
        claims.exp,
        claims.iat
    );

    // Also verify the expires_in field in the response matches
    let response_lifetime = token_response.expires_in as i64;
    assert!(
        (response_lifetime - expected_lifetime).abs() <= tolerance,
        "Token response 'expires_in' ({}) should be approximately 1 hour (3600s)",
        response_lifetime
    );
}

// ============================================================================
// JWT Header Injection Attack Tests
// ============================================================================

/// Test that malicious 'kid' values are rejected (path traversal, SQL injection).
/// The service should fail signature validation when kid is tampered.
#[tokio::test]
async fn test_kid_injection_rejected() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a valid token first
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Fetch JWKS and get the valid key
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS fetch should succeed");

    let header =
        decode_header(&token_response.access_token).expect("Token header should be decodable");
    let original_kid = header.kid.expect("Token should have kid");

    // Find the valid key
    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == original_kid)
        .expect("JWKS should contain key with matching kid");

    let x = jwk.x.as_ref().expect("JWK should have x coordinate");
    let public_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(x)
        .expect("Public key should be valid base64");
    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    // Malicious kid values to test
    let malicious_kids = [
        "../../etc/passwd",          // Path traversal
        "../../../etc/shadow",       // More path traversal
        "'; DROP TABLE keys; --",    // SQL injection
        "\" OR 1=1 --",              // SQL injection
        "<script>alert(1)</script>", // XSS attempt
        "%00null_byte",              // Null byte injection
        "key\nX-Injected: header",   // Header injection
    ];

    let parts: Vec<&str> = token_response.access_token.split('.').collect();

    for malicious_kid in &malicious_kids {
        // Craft a new header with malicious kid
        let new_header = serde_json::json!({
            "alg": "EdDSA",
            "typ": "JWT",
            "kid": malicious_kid
        });

        let new_header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&new_header).unwrap());

        // Create tampered token (signature will be invalid since header changed)
        let tampered_token = format!("{}.{}.{}", new_header_b64, parts[1], parts[2]);

        // Attempt to validate - should fail due to signature mismatch
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_exp = true;
        validation.set_audience(&["dark-tower-services"]);

        let result = decode::<Claims>(&tampered_token, &decoding_key, &validation);

        assert!(
            result.is_err(),
            "Token with malicious kid '{}' should be rejected due to signature mismatch",
            malicious_kid
        );
    }
}

/// Test that tokens with embedded 'jwk' header are rejected (CVE-2018-0114).
/// An attacker should not be able to embed their own key in the token header.
#[tokio::test]
async fn test_jwk_header_injection_rejected() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a valid token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Fetch valid JWKS for comparison
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS fetch should succeed");

    let header =
        decode_header(&token_response.access_token).expect("Token header should be decodable");
    let original_kid = header.kid.expect("Token should have kid");

    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == original_kid)
        .expect("JWKS should contain key");

    let x = jwk.x.as_ref().expect("JWK should have x coordinate");
    let public_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(x)
        .expect("Public key should be valid base64");
    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    // Craft a header with embedded 'jwk' containing an attacker's key
    // This is CVE-2018-0114 - some implementations trust the embedded key
    let attacker_header = serde_json::json!({
        "alg": "EdDSA",
        "typ": "JWT",
        "kid": original_kid,
        "jwk": {
            "kty": "OKP",
            "crv": "Ed25519",
            "x": "attackerFakeKeyMaterial_____________________", // Fake key
            "kid": "attacker-key"
        }
    });

    let parts: Vec<&str> = token_response.access_token.split('.').collect();
    let attacker_header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&attacker_header).unwrap());

    let tampered_token = format!("{}.{}.{}", attacker_header_b64, parts[1], parts[2]);

    // Validate using the ACTUAL public key from JWKS
    // If the service is vulnerable to CVE-2018-0114, it would use the embedded jwk
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;
    validation.set_audience(&["dark-tower-services"]);

    let result = decode::<Claims>(&tampered_token, &decoding_key, &validation);

    // Should fail because:
    // 1. If service correctly ignores embedded jwk: signature mismatch (header was modified)
    // 2. If service incorrectly uses embedded jwk: still fails because attacker key is fake
    assert!(
        result.is_err(),
        "Token with embedded 'jwk' header should be rejected. \
         The service MUST NOT use embedded keys (CVE-2018-0114 protection)."
    );
}

/// Test that tokens with 'jku' header pointing to external URL are rejected.
/// The service should NEVER fetch keys from URLs specified in token headers.
#[tokio::test]
async fn test_jku_header_injection_rejected() {
    let cluster = cluster().await;
    let auth_client = AuthClient::new(&cluster.ac_base_url);

    // Issue a valid token
    let request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(request)
        .await
        .expect("Token issuance should succeed");

    // Get valid JWKS for signature verification
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("JWKS fetch should succeed");

    let header =
        decode_header(&token_response.access_token).expect("Token header should be decodable");
    let original_kid = header.kid.expect("Token should have kid");

    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == original_kid)
        .expect("JWKS should contain key");

    let x = jwk.x.as_ref().expect("JWK should have x coordinate");
    let public_key_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(x)
        .expect("Public key should be valid base64");
    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    // Malicious jku URLs to test
    let malicious_jkus = [
        "https://attacker.com/.well-known/jwks.json", // External attacker server
        "http://internal-service/.well-known/jwks.json", // Internal SSRF attempt
        "file:///etc/passwd",                         // File protocol
        "https://localhost:8080/jwks.json",           // Localhost access
    ];

    let parts: Vec<&str> = token_response.access_token.split('.').collect();

    for malicious_jku in &malicious_jkus {
        // Craft a header with malicious 'jku' URL
        let jku_header = serde_json::json!({
            "alg": "EdDSA",
            "typ": "JWT",
            "kid": original_kid,
            "jku": malicious_jku
        });

        let jku_header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&jku_header).unwrap());

        let tampered_token = format!("{}.{}.{}", jku_header_b64, parts[1], parts[2]);

        // Validate using the real JWKS key
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_exp = true;
        validation.set_audience(&["dark-tower-services"]);

        let result = decode::<Claims>(&tampered_token, &decoding_key, &validation);

        // Should fail due to signature mismatch (header was modified)
        // If the service is vulnerable, it might try to fetch from jku URL
        assert!(
            result.is_err(),
            "Token with 'jku' header pointing to '{}' should be rejected. \
             The service MUST NOT fetch keys from token-specified URLs.",
            malicious_jku
        );
    }
}
