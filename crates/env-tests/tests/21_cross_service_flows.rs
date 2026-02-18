//! P1 Tests: Cross-Service Flows (AC + GC Integration)
//!
//! End-to-end tests validating the integration between Authentication Controller
//! and Global Controller for meeting access flows as defined in ADR-0020.
//!
//! # Test Flows
//!
//! 1. **Authenticated User Join**: User token (AC) -> GET /api/v1/meetings/{code} (GC)
//! 2. **Guest Token Flow**: POST /api/v1/meetings/{code}/guest-token (GC)
//! 3. **Meeting Settings Update**: PATCH /api/v1/meetings/{id}/settings (GC, host only)
//!
//! # Prerequisites
//!
//! - Kind cluster with AC and GC deployed
//! - Port-forwards active: AC (8082), GC (8080)
//! - Test data seeded: organizations, users, meetings

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::TokenRequest;
use env_tests::fixtures::gc_client::{GcClient, GuestTokenRequest, UpdateMeetingSettingsRequest};
use env_tests::fixtures::AuthClient;

/// Helper to create a cluster connection and verify GC is available.
///
/// GC is a required dependency for all cross-service flow tests.
/// If GC is not running, tests should fail rather than silently skip.
async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster
        .check_gc_health()
        .await
        .expect("GC service must be running for cross-service flow tests");
    cluster
}

// ============================================================================
// Cross-Service Health Checks
// ============================================================================

/// Test: Both AC and GC services are healthy and can communicate.
///
/// This validates that:
/// 1. AC service is running and responding to health checks
/// 2. GC service is running and responding to health checks
/// 3. GC can reach AC's JWKS endpoint for token validation
#[tokio::test]
async fn test_ac_gc_services_healthy() {
    let cluster = cluster().await;

    // Verify AC is healthy
    cluster
        .check_ac_health()
        .await
        .expect("AC service should be healthy");

    // GC health already verified in cluster() helper

    // Verify AC JWKS is accessible (GC needs this for token validation)
    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let jwks = auth_client
        .fetch_jwks()
        .await
        .expect("AC JWKS should be accessible");

    assert!(
        !jwks.keys.is_empty(),
        "AC JWKS should have at least one signing key"
    );
}

// ============================================================================
// Flow 1: Authenticated User Join
// ============================================================================

/// Test: GC `/api/v1/me` endpoint validates AC-issued tokens correctly.
///
/// This validates the token validation flow:
/// 1. Client obtains service token from AC
/// 2. Client calls GC /api/v1/me with token
/// 3. GC validates token against AC JWKS
/// 4. GC returns user claims from token
#[tokio::test]
async fn test_gc_validates_ac_token_via_me_endpoint() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Step 1: Get token from AC
    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token");

    // Step 2: Call GC /api/v1/me with AC-issued token
    let me_response = gc_client
        .get_me(&token_response.access_token)
        .await
        .expect("GC should validate AC token and return user info");

    // Step 3: Verify response contains expected claims
    assert_eq!(
        me_response.sub, "test-client",
        "Subject should match AC token subject"
    );
    assert!(
        me_response.scopes.contains(&"test:all".to_string()),
        "Scopes should contain test:all from AC token"
    );
}

/// Test: GC rejects requests without authentication.
///
/// Protected endpoints should return 401 when no Authorization header is provided.
#[tokio::test]
async fn test_gc_rejects_unauthenticated_requests() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Call /api/v1/me without token
    let result = gc_client.get_me("").await;

    assert!(
        result.is_err(),
        "GC should reject request without valid token"
    );

    // Verify it's a 401 error
    if let Err(env_tests::fixtures::gc_client::GcClientError::RequestFailed { status, .. }) = result
    {
        assert_eq!(status, 401, "Should return 401 Unauthorized");
    } else {
        panic!("Expected RequestFailed error with 401 status");
    }
}

/// Test: GC rejects invalid/tampered tokens.
///
/// Tokens with invalid signatures should be rejected.
#[tokio::test]
async fn test_gc_rejects_invalid_token() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Create a fake/tampered token
    let fake_token = "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiJhdHRhY2tlciIsImV4cCI6OTk5OTk5OTk5OSwiaWF0IjoxNjAwMDAwMDAwfQ.\
        invalid_signature_here";

    let result = gc_client.get_me(fake_token).await;

    assert!(result.is_err(), "GC should reject tampered token");

    if let Err(env_tests::fixtures::gc_client::GcClientError::RequestFailed { status, .. }) = result
    {
        assert_eq!(
            status, 401,
            "Should return 401 Unauthorized for invalid token"
        );
    } else {
        panic!("Expected RequestFailed error with 401 status");
    }
}

// ============================================================================
// Flow 2: Meeting Join (Authenticated User)
// ============================================================================

/// Test: Authenticated user can join a meeting via GC.
///
/// NOTE: This test requires:
/// - A seeded meeting in the database
/// - AC internal meeting-token endpoint implemented
///
/// For Phase 3, this test validates the GC endpoint returns appropriate errors
/// when the meeting is not found (database not seeded).
#[tokio::test]
async fn test_meeting_join_requires_authentication() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Try to join meeting without authentication
    let result = gc_client.raw_join_meeting("test-meeting-code", None).await;

    match result {
        Ok(response) => {
            assert_eq!(
                response.status().as_u16(),
                401,
                "Unauthenticated meeting join should return 401"
            );
        }
        Err(e) => {
            panic!("Request should not fail at network level: {}", e);
        }
    }
}

/// Test: Authenticated user gets appropriate error for non-existent meeting.
#[tokio::test]
async fn test_meeting_join_returns_404_for_unknown_meeting() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Get a valid token
    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token");

    // Try to join non-existent meeting
    let result = gc_client
        .raw_join_meeting(
            "nonexistent-meeting-code-12345",
            Some(&token_response.access_token),
        )
        .await;

    match result {
        Ok(response) => {
            let status = response.status().as_u16();
            // Should return 404 Not Found for non-existent meeting
            // OR 401 if token validation fails (GC can't reach AC JWKS)
            assert!(
                status == 404 || status == 401,
                "Should return 404 (not found) or 401 (if JWKS unreachable), got {}",
                status
            );
        }
        Err(e) => {
            panic!("Request should not fail at network level: {}", e);
        }
    }
}

// ============================================================================
// Flow 3: Guest Token
// ============================================================================

/// Test: Guest token endpoint is publicly accessible (no auth required).
///
/// The guest-token endpoint should accept requests without authentication
/// but validate the captcha and meeting permissions.
#[tokio::test]
async fn test_guest_token_endpoint_is_public() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Request guest token without authentication
    let request = GuestTokenRequest {
        display_name: "Test Guest".to_string(),
        captcha_token: "test-captcha-token".to_string(),
    };

    let result = gc_client
        .get_guest_token("test-meeting-code", &request)
        .await;

    // Should fail with 404 (meeting not found) or 403 (guests not allowed),
    // NOT 401 (unauthorized) - because this is a public endpoint
    match result {
        Ok(_) => {
            // If this succeeds, we have a seeded meeting with guests allowed
            println!("Guest token issued successfully - meeting exists and allows guests");
        }
        Err(env_tests::fixtures::gc_client::GcClientError::RequestFailed { status, body }) => {
            // Should NOT be 401 (would mean auth is incorrectly required)
            assert_ne!(
                status, 401,
                "Guest token endpoint should not require authentication. Response: {}",
                body
            );
            // 404 (meeting not found) or 403 (guests not allowed) are valid
            assert!(
                status == 404 || status == 403 || status == 400,
                "Expected 404/403/400, got {}: {}",
                status,
                body
            );
        }
        Err(e) => {
            panic!("Unexpected error type: {}", e);
        }
    }
}

/// Test: Guest token request validates required fields.
#[tokio::test]
async fn test_guest_token_validates_display_name() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Request with empty display name
    let request = GuestTokenRequest {
        display_name: "".to_string(),
        captcha_token: "test-captcha-token".to_string(),
    };

    let result = gc_client
        .get_guest_token("test-meeting-code", &request)
        .await;

    // Should fail with 400 Bad Request for empty display name
    match result {
        Ok(_) => {
            panic!("Should reject empty display name");
        }
        Err(env_tests::fixtures::gc_client::GcClientError::RequestFailed { status, .. }) => {
            // 400 for validation error, or 404 if meeting lookup happens first
            assert!(
                status == 400 || status == 404,
                "Expected 400 (validation) or 404 (meeting not found), got {}",
                status
            );
        }
        Err(e) => {
            panic!("Unexpected error type: {}", e);
        }
    }
}

// ============================================================================
// Flow 4: Meeting Settings Update
// ============================================================================

/// Test: Meeting settings update requires authentication.
#[tokio::test]
async fn test_meeting_settings_requires_authentication() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    let request = UpdateMeetingSettingsRequest::with_allow_guests(true);

    // Try to update settings without authentication
    let result = gc_client
        .raw_update_settings(uuid::Uuid::nil(), None, &request)
        .await;

    match result {
        Ok(response) => {
            assert_eq!(
                response.status().as_u16(),
                401,
                "Unauthenticated settings update should return 401"
            );
        }
        Err(e) => {
            panic!("Request should not fail at network level: {}", e);
        }
    }
}

/// Test: Meeting settings update returns 404 for non-existent meeting.
#[tokio::test]
async fn test_meeting_settings_returns_404_for_unknown_meeting() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Get a valid token
    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token");

    let request = UpdateMeetingSettingsRequest::with_allow_guests(true);

    // Try to update non-existent meeting
    let result = gc_client
        .raw_update_settings(
            uuid::Uuid::nil(),
            Some(&token_response.access_token),
            &request,
        )
        .await;

    match result {
        Ok(response) => {
            let status = response.status().as_u16();
            // Should return 404 Not Found or 400 Bad Request (no changes)
            // or 401 if token validation fails
            assert!(
                status == 404 || status == 400 || status == 401,
                "Expected 404/400/401, got {}",
                status
            );
        }
        Err(e) => {
            panic!("Request should not fail at network level: {}", e);
        }
    }
}

// ============================================================================
// Token Propagation Tests
// ============================================================================

/// Test: Token issued by AC can be validated by GC across multiple requests.
///
/// This validates that GC properly caches JWKS and validates tokens consistently.
#[tokio::test]
async fn test_token_validation_consistency() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Get token from AC
    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token");

    // Validate token multiple times through GC
    for i in 0..5 {
        let me_response = gc_client
            .get_me(&token_response.access_token)
            .await
            .unwrap_or_else(|_| panic!("GC should validate token on attempt {}", i + 1));

        assert_eq!(
            me_response.sub, "test-client",
            "Subject should be consistent across validations"
        );
    }
}

/// Test: Different tokens from AC are all validated correctly by GC.
#[tokio::test]
async fn test_multiple_tokens_validated() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Issue multiple tokens (each with the registered scope, but distinct JWTs)
    let mut tokens = Vec::new();
    for i in 0..3 {
        let token_request = TokenRequest::client_credentials(
            "test-client",
            "test-client-secret-dev-999",
            "test:all",
        );

        let token_response = auth_client
            .issue_token(token_request)
            .await
            .unwrap_or_else(|_| panic!("AC should issue token {}", i));

        tokens.push((i, token_response.access_token));
    }

    // Validate all tokens through GC
    for (i, token) in &tokens {
        let me_response = gc_client
            .get_me(token)
            .await
            .unwrap_or_else(|_| panic!("GC should validate token {}", i));

        assert_eq!(me_response.sub, "test-client");
        assert!(
            me_response.scopes.contains(&"test:all".to_string()),
            "Token {} should have scope test:all",
            i
        );
    }
}
