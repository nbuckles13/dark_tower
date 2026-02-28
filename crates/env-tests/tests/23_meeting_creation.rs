//! P1 Tests: Meeting Creation (POST /api/v1/meetings)
//!
//! End-to-end tests validating the create-meeting endpoint on the Global Controller.
//! Tests cover authenticated creation, secure defaults, error paths, and round-trip
//! verification (create then join by code).
//!
//! # User Authentication
//!
//! The create-meeting endpoint uses `require_user_auth` middleware, which requires
//! a user JWT (UserClaims with org_id, roles). Tests obtain user JWTs by registering
//! users via AC's `POST /api/v1/auth/register` endpoint.
//!
//! # Rate Limiting
//!
//! AC limits registrations to 5 per IP per hour per org. Tests are designed to
//! stay within this limit (4 registrations total across all tests).
//!
//! # Prerequisites
//!
//! - Kind cluster with AC and GC deployed
//! - Port-forwards active: AC (8082), GC (8080)
//! - Test data seeded: `devtest` organization in database

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::{TokenRequest, UserRegistrationRequest};
use env_tests::fixtures::gc_client::{CreateMeetingRequest, GcClient};
use env_tests::fixtures::AuthClient;
use std::collections::HashSet;

/// Helper to create a cluster connection and verify both AC and GC are available.
async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster
        .check_gc_health()
        .await
        .expect("GC service must be running for meeting creation tests");
    cluster
        .check_ac_health()
        .await
        .expect("AC service must be running for user registration");
    cluster
}

/// Register a test user via AC and return the user JWT access token.
///
/// Creates a unique user with a UUID-based email to prevent collisions.
async fn register_test_user(auth_client: &AuthClient, display_name: &str) -> String {
    let request = UserRegistrationRequest::unique(display_name);

    let response = auth_client
        .register_user(&request)
        .await
        .expect("AC should register test user");

    response.access_token
}

// ============================================================================
// Test 1: Authenticated Create with Secure Defaults
// ============================================================================

/// Test: Authenticated user can create a meeting with secure defaults.
///
/// Validates:
/// 1. User JWT from AC registration works with GC's require_user_auth middleware
/// 2. POST /api/v1/meetings returns 201 Created
/// 3. Response contains expected fields (meeting_id, meeting_code, display_name, status)
/// 4. Meeting code is 12 alphanumeric characters (base62)
/// 5. Secure defaults applied: require_auth=true, allow_guests=false,
///    allow_external_participants=false, waiting_room_enabled=true,
///    enable_e2e_encryption=true, recording_enabled=false
#[tokio::test]
async fn test_authenticated_user_can_create_meeting() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Step 1: Register user and get JWT
    let user_token = register_test_user(&auth_client, "Create Meeting Test User").await;

    // Step 2: Create meeting with minimal request (secure defaults)
    let create_request = CreateMeetingRequest::new("Env Test Meeting");

    let response = gc_client
        .create_meeting(&user_token, &create_request)
        .await
        .expect("Should create meeting successfully");

    // Step 3: Verify response fields
    assert!(
        !response.meeting_id.is_nil(),
        "meeting_id should be a valid non-nil UUID"
    );

    assert_eq!(
        response.meeting_code.len(),
        12,
        "Meeting code should be exactly 12 characters"
    );
    assert!(
        response
            .meeting_code
            .chars()
            .all(|c| c.is_ascii_alphanumeric()),
        "Meeting code should be base62 (alphanumeric only), got: {}",
        response.meeting_code
    );

    assert_eq!(response.display_name, "Env Test Meeting");
    assert_eq!(response.status, "scheduled");
    assert_eq!(response.max_participants, 100, "Default max_participants");

    // Step 4: Verify secure defaults (R-7, security-critical)
    assert!(
        response.enable_e2e_encryption,
        "Secure default: enable_e2e_encryption should be true"
    );
    assert!(
        response.require_auth,
        "Secure default: require_auth should be true"
    );
    assert!(
        !response.recording_enabled,
        "Secure default: recording_enabled should be false"
    );
    assert!(
        !response.allow_guests,
        "Secure default: allow_guests should be false"
    );
    assert!(
        !response.allow_external_participants,
        "Secure default: allow_external_participants should be false"
    );
    assert!(
        response.waiting_room_enabled,
        "Secure default: waiting_room_enabled should be true"
    );

    // Step 5: Verify created_at is present and non-empty
    assert!(
        !response.created_at.is_empty(),
        "created_at should be present"
    );
}

// ============================================================================
// Test 2: Round-Trip Joinable
// ============================================================================

/// Test: Created meeting is persisted and findable by meeting code.
///
/// Creates a meeting, then attempts to look it up by meeting code via the
/// join endpoint. The join endpoint uses service auth and will fail at
/// user ID parsing (since the service token sub is not a UUID), but
/// crucially should NOT return 404 — proving the meeting was persisted.
#[tokio::test]
async fn test_create_meeting_round_trip_findable() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Step 1: Register user and create meeting
    let user_token = register_test_user(&auth_client, "Round-Trip Test User").await;

    let create_request = CreateMeetingRequest::new("Round-Trip Test Meeting");
    let created = gc_client
        .create_meeting(&user_token, &create_request)
        .await
        .expect("Should create meeting");

    // Step 2: Get service token for join endpoint (which uses require_auth, not require_user_auth)
    let service_token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");
    let service_token = auth_client
        .issue_token(service_token_request)
        .await
        .expect("Should issue service token");

    // Step 3: Try to join by meeting code with service token
    // The join handler will find the meeting (not 404) but fail at
    // parse_user_id since the service token sub is "test-client" (not a UUID).
    let result = gc_client
        .raw_join_meeting(&created.meeting_code, Some(&service_token.access_token))
        .await
        .expect("Network request should succeed");

    let status = result.status().as_u16();

    // The key assertion: NOT 404 means the meeting was found in the database
    assert_ne!(
        status, 404,
        "Meeting with code '{}' should exist in database (created moments ago). \
         Got 404 which means the meeting was not persisted.",
        created.meeting_code
    );

    // Expected: 401 (invalid token for user context) or 500 (parse_user_id failure)
    // Either way, the meeting was found.
    println!(
        "Round-trip verified: meeting code '{}' found (status: {})",
        created.meeting_code, status
    );
}

// ============================================================================
// Test 3: Unauthenticated Rejection
// ============================================================================

/// Test: Create meeting endpoint rejects unauthenticated requests.
///
/// POST /api/v1/meetings without Authorization header should return 401.
#[tokio::test]
async fn test_create_meeting_unauthenticated_rejected() {
    let cluster = cluster().await;

    let gc_client = GcClient::new(&cluster.gc_base_url);

    let body = r#"{"display_name":"Should Not Work"}"#;

    let result = gc_client
        .raw_create_meeting(None, body)
        .await
        .expect("Network request should succeed");

    assert_eq!(
        result.status().as_u16(),
        401,
        "Unauthenticated create meeting should return 401"
    );
}

// ============================================================================
// Test 4: Service Token Rejection
// ============================================================================

/// Test: Create meeting endpoint rejects service tokens.
///
/// POST /api/v1/meetings uses `require_user_auth` middleware which expects
/// UserClaims (with org_id, roles). A service token (with scope Claims)
/// should be rejected with 401.
#[tokio::test]
async fn test_create_meeting_rejects_service_token() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Get a valid service token (client_credentials flow)
    let token_request =
        TokenRequest::client_credentials("test-client", "test-client-secret-dev-999", "test:all");
    let token_response = auth_client
        .issue_token(token_request)
        .await
        .expect("Should issue service token");

    // Try to create meeting with service token
    let body = r#"{"display_name":"Service Token Meeting"}"#;
    let result = gc_client
        .raw_create_meeting(Some(&token_response.access_token), body)
        .await
        .expect("Network request should succeed");

    assert_eq!(
        result.status().as_u16(),
        401,
        "Service token should be rejected by require_user_auth middleware"
    );
}

// ============================================================================
// Test 5: Invalid Body Rejection
// ============================================================================

/// Test: Create meeting endpoint rejects invalid request bodies.
///
/// Validates:
/// 1. Malformed JSON returns 400
/// 2. Missing required field (display_name) returns 400
#[tokio::test]
async fn test_create_meeting_invalid_body_rejected() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Register user for authentication
    let user_token = register_test_user(&auth_client, "Invalid Body Test User").await;

    // Test 1: Malformed JSON
    let result = gc_client
        .raw_create_meeting(Some(&user_token), "not valid json{{{")
        .await
        .expect("Network request should succeed");

    assert_eq!(
        result.status().as_u16(),
        400,
        "Malformed JSON should return 400"
    );

    // Test 2: Missing display_name (empty object)
    let result = gc_client
        .raw_create_meeting(Some(&user_token), "{}")
        .await
        .expect("Network request should succeed");

    assert_eq!(
        result.status().as_u16(),
        400,
        "Missing display_name should return 400"
    );

    // Test 3: Empty display_name (whitespace only)
    let result = gc_client
        .raw_create_meeting(Some(&user_token), r#"{"display_name":"   "}"#)
        .await
        .expect("Network request should succeed");

    assert_eq!(
        result.status().as_u16(),
        400,
        "Whitespace-only display_name should return 400"
    );
}

// ============================================================================
// Test 6: Unique Codes
// ============================================================================

/// Test: Multiple meeting creations produce unique meeting codes.
///
/// Creates 3 meetings with the same user and verifies all meeting codes
/// are distinct (72 bits entropy should make collisions practically impossible).
#[tokio::test]
async fn test_create_meeting_unique_codes() {
    let cluster = cluster().await;

    let auth_client = AuthClient::new(&cluster.ac_base_url);
    let gc_client = GcClient::new(&cluster.gc_base_url);

    // Register a single user (to conserve rate limit quota)
    let user_token = register_test_user(&auth_client, "Unique Codes Test User").await;

    let mut codes = HashSet::new();
    let meeting_count = 3;

    for i in 0..meeting_count {
        let request = CreateMeetingRequest::new(format!("Unique Code Test {}", i));

        let response = gc_client
            .create_meeting(&user_token, &request)
            .await
            .unwrap_or_else(|e| panic!("Failed to create meeting {}: {:?}", i, e));

        assert!(
            codes.insert(response.meeting_code.clone()),
            "Meeting code '{}' was duplicated (meeting {})",
            response.meeting_code,
            i
        );
    }

    assert_eq!(
        codes.len(),
        meeting_count,
        "All {} meeting codes should be unique",
        meeting_count
    );
}
