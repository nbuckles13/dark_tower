//! P1 Tests: MC-GC Integration (User POV)
//!
//! End-to-end tests validating the integration between Meeting Controller (MC)
//! and Global Controller (GC) from the user's perspective via HTTP APIs.
//!
//! # Test Focus (ADR-0010 Phase 4a)
//!
//! These tests validate user-facing behavior, NOT internal gRPC APIs:
//! - Guest token endpoint accessibility and error handling
//! - Error response sanitization (no internal details leaked)
//!
//! # Why authenticated join tests are absent
//!
//! `GET /api/v1/meetings/{code}` (authenticated join) requires:
//! 1. A user JWT with UUID `sub` (service tokens use string client_id, not UUID)
//! 2. A seeded meeting in the database (no create-meeting endpoint exists yet)
//! 3. The user must exist in the `users` table (for org_id lookup)
//! 4. A healthy MC registered in `meeting_controllers` (for assignment)
//! 5. GC's service token for AC internal meeting-token endpoint
//!
//! Until env-test infrastructure can create user tokens and seed test data,
//! authenticated join flow tests live in `crates/gc-service/tests/meeting_tests.rs`
//! (integration tests with sqlx::test and test harness).
//!
//! TODO: Re-add authenticated join env-tests when:
//!   - GC has a create-meeting endpoint, OR test data seeding is automated
//!   - env-tests can obtain user tokens (not just service credentials)
//!   - See: .claude/TODO.md for tracking
//!
//! # Prerequisites
//!
//! - Kind cluster with AC and GC deployed
//! - Port-forwards active: AC (8082), GC HTTP (8080)

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_client::{GcClient, GcClientError, GuestTokenRequest};

/// Helper to create a cluster connection and verify GC is available.
///
/// GC is a required dependency for all MC-GC integration tests.
/// If GC is not running, tests should fail rather than silently skip.
async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster
        .check_gc_health()
        .await
        .expect("GC service must be running for MC-GC integration tests");
    cluster
}

// ============================================================================
// Test Category 1: Guest Token Endpoint
// ============================================================================

/// Test: Guest token endpoint returns 404 for non-existent meeting.
///
/// Validates that the guest-token endpoint:
/// 1. Is reachable without authentication (public endpoint)
/// 2. Returns 404 when the meeting code doesn't exist in the database
/// 3. Does not return 401 (which would mean auth is incorrectly required)
#[tokio::test]
async fn test_guest_token_returns_404_for_unknown_meeting() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    let request = GuestTokenRequest {
        display_name: "Test Guest".to_string(),
        captcha_token: "test-captcha-token".to_string(),
    };

    // Use a meeting code that definitely doesn't exist
    let result = gc_client
        .get_guest_token("nonexistent-meeting-xyz-999", &request)
        .await;

    match result {
        Ok(_) => {
            panic!(
                "Guest token should NOT succeed for non-existent meeting code. \
                 If this passes, a meeting with code 'nonexistent-meeting-xyz-999' \
                 unexpectedly exists in the database."
            );
        }
        Err(GcClientError::RequestFailed { status, body }) => {
            // Guest endpoint should NOT return 401 (it's public)
            assert_ne!(
                status, 401,
                "Guest token endpoint should not require authentication. Got 401: {}",
                body
            );

            // Should return 404 for non-existent meeting
            assert_eq!(
                status, 404,
                "Expected 404 Not Found for non-existent meeting, got {}: {}",
                status, body
            );
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
// Test Category 2: Error Response Sanitization
// ============================================================================

/// Test: Error responses do not leak internal service details.
///
/// Uses the guest-token endpoint (public, no auth needed) with a
/// guaranteed-nonexistent meeting code to trigger a 404 error, then
/// validates the error body does not contain internal details.
#[tokio::test]
async fn test_error_responses_sanitized() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Use a guaranteed-nonexistent meeting code to always trigger the error path
    let request = GuestTokenRequest {
        display_name: "Sanitization Test Guest".to_string(),
        captcha_token: "test-captcha-token".to_string(),
    };

    let result = gc_client
        .get_guest_token("nonexistent-00000000", &request)
        .await;

    match result {
        Ok(_) => {
            panic!(
                "Guest token should NOT succeed for nonexistent meeting code. \
                 If this passes, a meeting with code 'nonexistent-00000000' \
                 unexpectedly exists in the database."
            );
        }
        Err(GcClientError::RequestFailed { status, body }) => {
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

            println!(
                "Error response sanitization validated for status {}",
                status
            );
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}
