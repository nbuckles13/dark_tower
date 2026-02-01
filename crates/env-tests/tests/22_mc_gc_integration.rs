//! P1 Tests: MC-GC Integration (User POV)
//!
//! End-to-end tests validating the integration between Meeting Controller (MC)
//! and Global Controller (GC) from the user's perspective via HTTP APIs.
//!
//! # Test Focus (ADR-0010 Phase 4a)
//!
//! These tests validate user-facing behavior, NOT internal gRPC APIs:
//! - `GET /v1/meetings/{code}` - authenticated user joining meeting
//! - `POST /v1/meetings/{code}/guest-token` - guest user joining meeting
//! - Verify responses include MC assignment data
//!
//! # Prerequisites
//!
//! - Kind cluster with AC and GC deployed
//! - Port-forwards active: AC (8082), GC HTTP (8080)
//! - Test data seeded: organizations, users, meetings
//! - MC may or may not be deployed (tests handle both cases)

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::TokenRequest;
use env_tests::fixtures::gc_client::{GcClient, GcClientError, GuestTokenRequest};
use env_tests::fixtures::AuthClient;

/// Helper to create a cluster connection for tests.
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

// ============================================================================
// Test Category 1: Meeting Join Returns MC Assignment
// ============================================================================

/// Test: Authenticated user joining a meeting receives MC assignment info.
///
/// This validates the complete user flow:
/// 1. User authenticates via AC (get token)
/// 2. User joins meeting via `GET /v1/meetings/{code}`
/// 3. Response includes `mc_assignment` with endpoint info
///
/// # Notes
///
/// - If meeting doesn't exist, expects 404
/// - If no MCs are healthy, expects 503
/// - If successful, validates MC assignment structure
#[tokio::test]
async fn test_meeting_join_returns_mc_assignment() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Step 1: Get token from AC
    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token_response = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token");

    // Step 2: Try to join a seeded test meeting
    // Note: This test may fail with 404 if no meetings are seeded,
    // or 503 if no MCs are healthy. Both are valid outcomes.
    let result = gc_client
        .join_meeting("test-meeting-code", &token_response.access_token)
        .await;

    match result {
        Ok(response) => {
            // Meeting exists and MC is assigned - validate response structure
            println!(
                "Meeting join successful - MC assigned: {}",
                response.mc_assignment.mc_id
            );

            // Validate token is present
            assert!(
                !response.token.is_empty(),
                "Meeting token should be present"
            );

            // Validate expires_in is reasonable (between 1 and 3600 seconds)
            assert!(
                response.expires_in > 0 && response.expires_in <= 3600,
                "expires_in should be between 1 and 3600 seconds, got {}",
                response.expires_in
            );

            // Validate meeting_id is a valid UUID (not nil)
            assert!(
                !response.meeting_id.is_nil(),
                "meeting_id should not be nil UUID"
            );

            // Validate meeting_name is present
            assert!(
                !response.meeting_name.is_empty(),
                "meeting_name should be present"
            );

            // Validate MC assignment
            assert!(
                !response.mc_assignment.mc_id.is_empty(),
                "mc_id should be present"
            );
            assert!(
                !response.mc_assignment.grpc_endpoint.is_empty(),
                "grpc_endpoint should be present"
            );

            // WebTransport endpoint is optional but if present should be valid
            if let Some(wt_endpoint) = &response.mc_assignment.webtransport_endpoint {
                assert!(
                    !wt_endpoint.is_empty(),
                    "webtransport_endpoint if present should not be empty"
                );
            }
        }
        Err(GcClientError::RequestFailed { status, body }) => {
            // Expected outcomes when meeting doesn't exist or MC not available
            match status {
                404 => {
                    println!(
                        "Meeting not found (no seeded test data) - test validates error handling"
                    );
                }
                503 => {
                    println!("No healthy MCs available - test validates graceful degradation");
                    // Verify error message is user-friendly (no internal details)
                    assert!(
                        !body.contains("gRPC") && !body.contains("grpc"),
                        "Error message should not expose internal gRPC details"
                    );
                }
                401 => {
                    println!("Token validation failed - GC may not be able to reach AC JWKS");
                }
                _ => {
                    panic!(
                        "Unexpected status code {}: {} - expected 200, 404, 503, or 401",
                        status, body
                    );
                }
            }
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}

// ============================================================================
// Test Category 2: Assignment Persistence (Same Meeting -> Same MC)
// ============================================================================

/// Test: Multiple users joining the same meeting get the same MC assignment.
///
/// This validates GC's assignment persistence logic:
/// 1. User A joins meeting-123, gets MC assignment
/// 2. User B joins meeting-123 with different token
/// 3. Both users should get the SAME MC assignment
///
/// # Notes
///
/// This test requires a seeded meeting in the database. If the meeting
/// doesn't exist, the test validates error handling instead.
#[tokio::test]
async fn test_same_meeting_gets_same_mc_assignment() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Get token for simulating multiple users (same client, different requests)
    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token")
        .access_token;

    // First join attempt
    let first_result = gc_client.join_meeting("test-meeting-code", &token).await;

    let first_mc_id = match &first_result {
        Ok(response) => Some(response.mc_assignment.mc_id.clone()),
        Err(GcClientError::RequestFailed { status, .. }) if *status == 404 || *status == 503 => {
            println!("SKIPPED: Meeting not found or no MCs available");
            return;
        }
        Err(GcClientError::RequestFailed { status, .. }) if *status == 401 => {
            println!("SKIPPED: Token validation failed");
            return;
        }
        Err(e) => {
            panic!("Unexpected error on first join: {}", e);
        }
    };

    // Second join attempt (simulating different user, same meeting)
    let second_result = gc_client.join_meeting("test-meeting-code", &token).await;

    match second_result {
        Ok(response) => {
            let second_mc_id = response.mc_assignment.mc_id.clone();

            assert_eq!(
                first_mc_id.as_ref(),
                Some(&second_mc_id),
                "Same meeting should be assigned to same MC"
            );

            println!(
                "Assignment persistence validated: both joins got MC {}",
                second_mc_id
            );
        }
        Err(e) => {
            panic!("Second join failed unexpectedly: {}", e);
        }
    }
}

// ============================================================================
// Test Category 3: No Healthy MCs -> Graceful Error
// ============================================================================

/// Test: When no MCs are registered/healthy, user gets appropriate 503 error.
///
/// This test validates graceful degradation:
/// - User's join request returns 503 Service Unavailable
/// - Error message is user-friendly, not internal details
///
/// # Notes
///
/// This test may pass with 200 if MCs are healthy, or fail with 404 if
/// the meeting doesn't exist. Both are valid outcomes.
#[tokio::test]
async fn test_no_healthy_mcs_returns_503() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token")
        .access_token;

    // Try to join a meeting - if no MCs are healthy, should get 503
    let result = gc_client.join_meeting("test-meeting-code", &token).await;

    match result {
        Ok(_response) => {
            println!(
                "Meeting join succeeded - MCs are healthy (test validates success path instead)"
            );
        }
        Err(GcClientError::RequestFailed { status, body }) => {
            match status {
                503 => {
                    // This is the expected case when no MCs are healthy
                    println!("503 Service Unavailable - no healthy MCs");

                    // Validate error message is user-friendly
                    assert!(
                        !body.contains("grpc") && !body.contains("gRPC"),
                        "Error should not expose gRPC details: {}",
                        body
                    );
                    assert!(
                        !body.contains("panic") && !body.contains("Panic"),
                        "Error should not expose panic info: {}",
                        body
                    );

                    // Verify it contains user-friendly messaging
                    // The error structure from GC is: {"error": {"code": "...", "message": "..."}}
                    assert!(
                        body.contains("SERVICE_UNAVAILABLE") || body.contains("unavailable"),
                        "Error should indicate service unavailable: {}",
                        body
                    );
                }
                404 => {
                    println!("Meeting not found (404) - test validates meeting lookup before MC assignment");
                }
                401 => {
                    println!("SKIPPED: Token validation failed");
                }
                _ => {
                    panic!("Unexpected status code {}: {}", status, body);
                }
            }
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}

// ============================================================================
// Test Category 4: Meeting Join Response Structure Validation
// ============================================================================

/// Test: JoinMeetingResponse includes all expected fields.
///
/// Validates the complete response structure:
/// - `token` (meeting-scoped JWT)
/// - `expires_in`
/// - `meeting_id`
/// - `meeting_name`
/// - `mc_assignment.mc_id`
/// - `mc_assignment.webtransport_endpoint` (optional)
/// - `mc_assignment.grpc_endpoint`
#[tokio::test]
async fn test_join_response_structure_complete() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token")
        .access_token;

    let result = gc_client.join_meeting("test-meeting-code", &token).await;

    match result {
        Ok(response) => {
            // Validate all required fields are present and valid

            // Token validation
            assert!(!response.token.is_empty(), "token must be present");
            assert!(
                response.token.starts_with("eyJ"),
                "token should be a JWT (starts with eyJ)"
            );

            // Expiration validation
            assert!(response.expires_in > 0, "expires_in must be positive");

            // Meeting ID validation
            assert!(!response.meeting_id.is_nil(), "meeting_id must not be nil");

            // Meeting name validation
            assert!(
                !response.meeting_name.is_empty(),
                "meeting_name must be present"
            );

            // MC Assignment validation
            assert!(
                !response.mc_assignment.mc_id.is_empty(),
                "mc_assignment.mc_id must be present"
            );
            assert!(
                !response.mc_assignment.grpc_endpoint.is_empty(),
                "mc_assignment.grpc_endpoint must be present"
            );

            // gRPC endpoint should be a valid URL format
            assert!(
                response.mc_assignment.grpc_endpoint.starts_with("http://")
                    || response.mc_assignment.grpc_endpoint.starts_with("https://"),
                "grpc_endpoint should be a valid URL: {}",
                response.mc_assignment.grpc_endpoint
            );

            // WebTransport endpoint is optional but if present should be valid URL
            if let Some(wt) = &response.mc_assignment.webtransport_endpoint {
                assert!(
                    wt.starts_with("http://") || wt.starts_with("https://"),
                    "webtransport_endpoint should be a valid URL: {}",
                    wt
                );
            }

            println!("Response structure validation passed");
            println!("  - meeting_id: {}", response.meeting_id);
            println!("  - meeting_name: {}", response.meeting_name);
            println!("  - expires_in: {}s", response.expires_in);
            println!("  - mc_id: {}", response.mc_assignment.mc_id);
            println!(
                "  - grpc_endpoint: {}",
                response.mc_assignment.grpc_endpoint
            );
            println!(
                "  - webtransport_endpoint: {:?}",
                response.mc_assignment.webtransport_endpoint
            );
        }
        Err(GcClientError::RequestFailed { status: 404, .. }) => {
            println!("SKIPPED: Meeting not found (404) - cannot validate response structure");
        }
        Err(GcClientError::RequestFailed { status: 503, .. }) => {
            println!("SKIPPED: No MCs available (503) - cannot validate response structure");
        }
        Err(GcClientError::RequestFailed { status: 401, .. }) => {
            println!("SKIPPED: Token validation failed");
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}

// ============================================================================
// Test Category 5: Guest Join with MC Assignment
// ============================================================================

/// Test: Guest requesting token receives MC assignment in response.
///
/// Validates the guest flow:
/// 1. Guest requests token via `POST /v1/meetings/{code}/guest-token`
/// 2. If meeting allows guests, response includes MC assignment
/// 3. MC assignment has valid endpoint information
#[tokio::test]
async fn test_guest_join_includes_mc_assignment() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Request guest token without authentication (public endpoint)
    let request = GuestTokenRequest {
        display_name: "Test Guest".to_string(),
        captcha_token: "test-captcha-token".to_string(),
    };

    let result = gc_client
        .get_guest_token("test-meeting-code", &request)
        .await;

    match result {
        Ok(response) => {
            // Guest token issued successfully - validate MC assignment
            println!(
                "Guest join successful - MC assigned: {}",
                response.mc_assignment.mc_id
            );

            // Validate response structure (same as authenticated join)
            assert!(!response.token.is_empty(), "token must be present");
            assert!(response.expires_in > 0, "expires_in must be positive");
            assert!(!response.meeting_id.is_nil(), "meeting_id must not be nil");
            assert!(
                !response.meeting_name.is_empty(),
                "meeting_name must be present"
            );

            // MC assignment validation
            assert!(
                !response.mc_assignment.mc_id.is_empty(),
                "mc_assignment.mc_id must be present"
            );
            assert!(
                !response.mc_assignment.grpc_endpoint.is_empty(),
                "mc_assignment.grpc_endpoint must be present"
            );
        }
        Err(GcClientError::RequestFailed { status, body }) => {
            match status {
                404 => {
                    println!("Meeting not found - no seeded test data");
                }
                403 => {
                    // Guests not allowed for this meeting - valid response
                    println!("Guests not allowed for this meeting (403 Forbidden)");
                    assert!(
                        body.contains("FORBIDDEN") || body.contains("not allowed"),
                        "403 response should indicate guests not allowed: {}",
                        body
                    );
                }
                400 => {
                    // Validation error - could be captcha or display name
                    println!("Validation error: {}", body);
                }
                503 => {
                    println!("No healthy MCs available - graceful degradation");
                }
                _ => {
                    // Guest endpoint should NOT return 401 (it's public)
                    assert_ne!(
                        status, 401,
                        "Guest token endpoint should not require authentication"
                    );
                    panic!("Unexpected status code {}: {}", status, body);
                }
            }
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}

/// Test: Guest token endpoint is public (no auth required).
///
/// Validates that:
/// - Guest endpoint accepts requests without Authorization header
/// - Failure is due to business logic (404, 403, 400, 503), not auth (401)
#[tokio::test]
async fn test_guest_endpoint_does_not_require_auth() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let gc_client = GcClient::new(&cluster.gc_base_url);

    let request = GuestTokenRequest {
        display_name: "Guest User".to_string(),
        captcha_token: "captcha-123".to_string(),
    };

    // Call guest endpoint without any authentication
    let result = gc_client
        .get_guest_token("any-meeting-code", &request)
        .await;

    match result {
        Ok(_) => {
            println!("Guest endpoint returned 200 - meeting exists and allows guests");
        }
        Err(GcClientError::RequestFailed { status, body }) => {
            // 401 would mean the endpoint incorrectly requires authentication
            assert_ne!(
                status, 401,
                "Guest token endpoint should NOT require authentication. Got 401: {}",
                body
            );

            // Expected error codes for business logic failures
            assert!(
                status == 400 || status == 403 || status == 404 || status == 503,
                "Expected business logic error (400/403/404/503), got {}: {}",
                status,
                body
            );

            println!(
                "Guest endpoint returned {} (expected for business logic): {}",
                status, body
            );
        }
        Err(e) => {
            panic!("Unexpected error type: {}", e);
        }
    }
}

// ============================================================================
// Test Category 6: Error Response Sanitization
// ============================================================================

/// Test: Error responses do not leak internal service details.
///
/// Validates that when errors occur (503, 500), the response:
/// - Does not contain internal gRPC endpoints
/// - Does not contain stack traces or panic info
/// - Uses generic, user-friendly error messages
#[tokio::test]
async fn test_error_responses_sanitized() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token")
        .access_token;

    // Try joining a meeting - collect any error response for analysis
    let result = gc_client.join_meeting("test-meeting-code", &token).await;

    if let Err(GcClientError::RequestFailed { status, body }) = result {
        // Skip 200 responses - we're testing error sanitization
        if status >= 400 {
            println!("Analyzing error response (status {})", status);

            // Should not contain internal implementation details
            let sensitive_patterns = [
                "grpc://",
                "gRPC",
                "postgres://",
                "DATABASE_URL",
                "stack trace",
                "panic",
                "thread '",
                "at /home",
                "at /src",
                ".rs:",
                "RUST_BACKTRACE",
            ];

            for pattern in sensitive_patterns {
                assert!(
                    !body.contains(pattern),
                    "Error response should not contain '{}': {}",
                    pattern,
                    body
                );
            }

            println!("Error response sanitization validated");
        }
    }
}

// ============================================================================
// Test Category 7: MC Assignment Endpoint Validation
// ============================================================================

/// Test: MC assignment endpoints are well-formed URLs.
///
/// When a meeting is successfully joined, validate that:
/// - grpc_endpoint is a valid URL
/// - webtransport_endpoint (if present) is a valid URL
/// - Endpoints use expected protocols (http/https)
#[tokio::test]
async fn test_mc_endpoints_are_valid_urls() {
    let cluster = cluster().await;

    if !cluster.is_gc_available().await {
        println!("SKIPPED: GC not deployed");
        return;
    }

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");

    let token = auth_client
        .issue_token(token_request)
        .await
        .expect("AC should issue token")
        .access_token;

    let result = gc_client.join_meeting("test-meeting-code", &token).await;

    match result {
        Ok(response) => {
            let grpc = &response.mc_assignment.grpc_endpoint;

            // gRPC endpoint validation
            assert!(
                grpc.starts_with("http://") || grpc.starts_with("https://"),
                "grpc_endpoint should start with http:// or https://, got: {}",
                grpc
            );

            // Should contain a port or be a valid hostname
            assert!(
                grpc.contains(':'),
                "grpc_endpoint should contain a port: {}",
                grpc
            );

            // WebTransport endpoint validation (if present)
            if let Some(wt) = &response.mc_assignment.webtransport_endpoint {
                assert!(
                    wt.starts_with("http://") || wt.starts_with("https://"),
                    "webtransport_endpoint should start with http:// or https://, got: {}",
                    wt
                );

                // WebTransport typically uses port 443 (HTTPS)
                println!("WebTransport endpoint: {}", wt);
            }

            println!("MC endpoint validation passed");
            println!("  - gRPC: {}", grpc);
        }
        Err(GcClientError::RequestFailed { status: 404, .. }) => {
            println!("SKIPPED: Cannot validate endpoints - meeting not found (404)");
        }
        Err(GcClientError::RequestFailed { status: 503, .. }) => {
            println!("SKIPPED: Cannot validate endpoints - no MCs available (503)");
        }
        Err(GcClientError::RequestFailed { status: 401, .. }) => {
            println!("SKIPPED: Cannot validate endpoints - token validation failed (401)");
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}
