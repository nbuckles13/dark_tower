//! Integration tests for internal token endpoints.
//!
//! These tests validate the meeting token and guest token endpoints
//! used by the Global Controller to issue tokens for meeting participants.
//!
//! Coverage targets:
//! - handlers/internal_tokens.rs (51.61% -> >90%)
//! - middleware/auth.rs require_service_auth (0% -> >90%)

use ac_test_utils::server_harness::TestAuthServer;
use reqwest::StatusCode;
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// Test Helpers
// ============================================================================

/// Helper to generate a fixed test UUID for reproducibility
fn test_uuid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

/// Build a meeting token request payload
fn meeting_token_request(
    subject_user_id: Uuid,
    meeting_id: Uuid,
    meeting_org_id: Uuid,
    home_org_id: Uuid,
) -> serde_json::Value {
    serde_json::json!({
        "subject_user_id": subject_user_id.to_string(),
        "meeting_id": meeting_id.to_string(),
        "meeting_org_id": meeting_org_id.to_string(),
        "home_org_id": home_org_id.to_string(),
        "participant_type": "member",
        "role": "participant",
        "capabilities": ["video", "audio"],
        "ttl_seconds": 600
    })
}

/// Build a guest token request payload
fn guest_token_request(
    guest_id: Uuid,
    meeting_id: Uuid,
    meeting_org_id: Uuid,
) -> serde_json::Value {
    serde_json::json!({
        "guest_id": guest_id.to_string(),
        "display_name": "Test Guest",
        "meeting_id": meeting_id.to_string(),
        "meeting_org_id": meeting_org_id.to_string(),
        "waiting_room": true,
        "ttl_seconds": 300
    })
}

// ============================================================================
// require_service_auth Middleware Tests
// ============================================================================

/// Test that internal endpoint requires authentication.
///
/// Validates that require_service_auth middleware returns 401 when
/// no Authorization header is present.
#[sqlx::test(migrations = "../../migrations")]
async fn test_internal_endpoint_requires_authentication(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Act - Request without Authorization header
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Request without Authorization header should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

/// Test that internal endpoint rejects malformed Authorization header.
///
/// Validates that require_service_auth middleware returns 401 when
/// Authorization header doesn't have "Bearer " prefix.
#[sqlx::test(migrations = "../../migrations")]
async fn test_internal_endpoint_rejects_malformed_auth_header(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Act - Malformed Authorization header (missing "Bearer " prefix)
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .header("Authorization", "Basic some-credentials")
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Malformed Authorization header should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

/// Test that internal endpoint rejects invalid JWT tokens.
///
/// Validates that require_service_auth middleware returns 401 when
/// the token is not a valid JWT.
#[sqlx::test(migrations = "../../migrations")]
async fn test_internal_endpoint_rejects_invalid_jwt(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Act - Invalid JWT (not even a real JWT)
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth("not-a-valid-jwt-token")
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Invalid JWT should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

/// Test that internal endpoint rejects expired tokens.
///
/// Validates that require_service_auth middleware rejects expired JWTs.
#[sqlx::test(migrations = "../../migrations")]
async fn test_internal_endpoint_rejects_expired_token(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create an expired token (expired 1 hour ago)
    let expired_token = server
        .create_expired_token("test-service", &["internal:meeting-token"], 3600)
        .await?;

    // Act - Request with expired token
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&expired_token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expired token should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

/// Test that internal endpoint rejects tokens with tampered signature.
///
/// Validates that require_service_auth middleware verifies JWT signatures.
#[sqlx::test(migrations = "../../migrations")]
async fn test_internal_endpoint_rejects_tampered_signature(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create a valid token and tamper with the signature
    let valid_token = server
        .create_service_token("test-service", &["internal:meeting-token"])
        .await?;

    // Tamper with the signature (last part of JWT)
    let parts: Vec<&str> = valid_token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 parts");

    let tampered_signature = format!("{}X", &parts[2][..parts[2].len() - 1]);
    let tampered_token = format!("{}.{}.{}", parts[0], parts[1], tampered_signature);

    // Act - Request with tampered token
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&tampered_token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Token with invalid signature should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

// ============================================================================
// Meeting Token Handler Tests - Scope Validation
// ============================================================================

/// Test that meeting token endpoint rejects tokens without required scope.
///
/// Validates that handle_meeting_token checks for internal:meeting-token scope.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_rejects_insufficient_scope(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create service token WITHOUT internal:meeting-token scope
    let token = server
        .create_service_token("test-service", &["meeting:create", "meeting:read"])
        .await?;

    // Act - Request with valid token but wrong scope
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Token without internal:meeting-token scope should return 403"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INSUFFICIENT_SCOPE"),
        "Error code should be INSUFFICIENT_SCOPE"
    );

    // Verify error message mentions required scope
    let message = body["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("internal:meeting-token"),
        "Error message should mention required scope, got: {}",
        message
    );

    Ok(())
}

/// Test that meeting token endpoint accepts valid token with correct scope.
///
/// Validates the happy path: a service token with internal:meeting-token scope
/// can successfully issue a meeting token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_success(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create service token WITH internal:meeting-token scope
    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    // Act - Request with valid token and correct scope
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Valid request should succeed"
    );

    let body: serde_json::Value = response.json().await?;

    // Verify response structure
    assert!(body["token"].is_string(), "Response should include token");
    assert!(
        body["expires_in"].is_number(),
        "Response should include expires_in"
    );

    // Verify token is a valid JWT format (3 parts separated by dots)
    let issued_token = body["token"].as_str().unwrap();
    let parts: Vec<&str> = issued_token.split('.').collect();
    assert_eq!(parts.len(), 3, "Issued token should be a valid JWT format");

    // Verify expires_in is capped at 900 (requested was 600, should be 600)
    let expires_in = body["expires_in"].as_u64().unwrap();
    assert_eq!(expires_in, 600, "expires_in should match requested TTL");

    Ok(())
}

/// Test that meeting token endpoint caps TTL to maximum (15 minutes).
///
/// Validates TTL capping defense in depth - even if client requests
/// longer TTL, it's capped to MAX_TOKEN_TTL_SECONDS (900).
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_ttl_capping(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    // Request with TTL above maximum (1 hour)
    let mut payload = meeting_token_request(test_uuid(1), test_uuid(2), test_uuid(3), test_uuid(4));
    payload["ttl_seconds"] = serde_json::json!(3600); // 1 hour

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await?;

    // Verify TTL was capped to 900
    let expires_in = body["expires_in"].as_u64().unwrap();
    assert_eq!(
        expires_in, 900,
        "TTL should be capped to 900 (15 minutes), got: {}",
        expires_in
    );

    Ok(())
}

/// Test meeting token with multiple scopes including required one.
///
/// Validates that scope validation works when the required scope is
/// among multiple scopes in the token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_multiple_scopes(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create token with multiple scopes including internal:meeting-token
    let token = server
        .create_service_token(
            "gc-service-multi",
            &["internal:meeting-token", "meeting:create", "meeting:read"],
        )
        .await?;

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Token with multiple scopes including required one should succeed"
    );

    Ok(())
}

/// Test meeting token with host role.
///
/// Validates that different roles are properly included in the issued token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_host_role(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    let mut payload = meeting_token_request(test_uuid(1), test_uuid(2), test_uuid(3), test_uuid(4));
    payload["role"] = serde_json::json!("host");
    payload["participant_type"] = serde_json::json!("member");
    payload["capabilities"] = serde_json::json!(["video", "audio", "screen_share"]);

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await?;
    assert!(body["token"].is_string(), "Response should include token");

    Ok(())
}

/// Test meeting token with external participant type.
///
/// Validates external participant type is properly handled.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_external_participant(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    let mut payload = meeting_token_request(test_uuid(1), test_uuid(2), test_uuid(3), test_uuid(4));
    payload["participant_type"] = serde_json::json!("external");

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK);

    Ok(())
}

// ============================================================================
// Guest Token Handler Tests
// ============================================================================

/// Test that guest token endpoint rejects tokens without required scope.
///
/// Validates that handle_guest_token checks for internal:meeting-token scope.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_rejects_insufficient_scope(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create service token WITHOUT internal:meeting-token scope
    let token = server
        .create_service_token("test-service", &["meeting:create"])
        .await?;

    // Act
    let response = client
        .post(format!("{}/api/v1/auth/internal/guest-token", server.url()))
        .bearer_auth(&token)
        .json(&guest_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Token without internal:meeting-token scope should return 403"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INSUFFICIENT_SCOPE"),
        "Error code should be INSUFFICIENT_SCOPE"
    );

    Ok(())
}

/// Test that guest token endpoint accepts valid token with correct scope.
///
/// Validates the happy path for guest token issuance.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_success(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    // Act
    let response = client
        .post(format!("{}/api/v1/auth/internal/guest-token", server.url()))
        .bearer_auth(&token)
        .json(&guest_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Valid request should succeed"
    );

    let body: serde_json::Value = response.json().await?;

    // Verify response structure
    assert!(body["token"].is_string(), "Response should include token");
    assert!(
        body["expires_in"].is_number(),
        "Response should include expires_in"
    );

    // Verify token is a valid JWT format
    let issued_token = body["token"].as_str().unwrap();
    let parts: Vec<&str> = issued_token.split('.').collect();
    assert_eq!(parts.len(), 3, "Issued token should be a valid JWT format");

    // Verify expires_in matches requested TTL (300)
    let expires_in = body["expires_in"].as_u64().unwrap();
    assert_eq!(expires_in, 300, "expires_in should match requested TTL");

    Ok(())
}

/// Test guest token TTL capping.
///
/// Validates that guest token TTL is also capped to MAX_TOKEN_TTL_SECONDS.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_ttl_capping(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    // Request with TTL above maximum
    let mut payload = guest_token_request(test_uuid(1), test_uuid(2), test_uuid(3));
    payload["ttl_seconds"] = serde_json::json!(7200); // 2 hours

    // Act
    let response = client
        .post(format!("{}/api/v1/auth/internal/guest-token", server.url()))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await?;

    // Verify TTL was capped to 900
    let expires_in = body["expires_in"].as_u64().unwrap();
    assert_eq!(
        expires_in, 900,
        "TTL should be capped to 900 (15 minutes), got: {}",
        expires_in
    );

    Ok(())
}

/// Test guest token with waiting_room = false.
///
/// Validates that waiting_room flag is properly handled.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_no_waiting_room(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    let mut payload = guest_token_request(test_uuid(1), test_uuid(2), test_uuid(3));
    payload["waiting_room"] = serde_json::json!(false);

    // Act
    let response = client
        .post(format!("{}/api/v1/auth/internal/guest-token", server.url()))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK);

    Ok(())
}

/// Test guest token requires authentication (require_service_auth middleware).
///
/// Validates the middleware is applied to the guest token endpoint.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_requires_authentication(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Act - Request without Authorization header
    let response = client
        .post(format!("{}/api/v1/auth/internal/guest-token", server.url()))
        .json(&guest_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Request without Authorization header should return 401"
    );

    Ok(())
}

// ============================================================================
// Scope Validation Edge Cases
// ============================================================================

/// Test that similar scopes don't match (scope validation is exact).
///
/// Validates that "internal:meeting" or "internal:meeting-token-extra"
/// do NOT satisfy the "internal:meeting-token" requirement.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_rejects_similar_scope(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Test with prefix scope (missing "-token" suffix)
    let prefix_token = server
        .create_service_token("test-service-prefix", &["internal:meeting"])
        .await?;

    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&prefix_token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Prefix scope should not match"
    );

    // Test with suffix scope (extra "-extra")
    let suffix_token = server
        .create_service_token("test-service-suffix", &["internal:meeting-token-extra"])
        .await?;

    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&suffix_token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Suffix scope should not match"
    );

    Ok(())
}

/// Test that scope matching is case-sensitive.
///
/// Validates that "Internal:Meeting-Token" does NOT match "internal:meeting-token".
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_scope_case_sensitive(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create token with wrong-case scope
    let wrong_case_token = server
        .create_service_token("test-service-case", &["Internal:Meeting-Token"])
        .await?;

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&wrong_case_token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Wrong-case scope should return 403"
    );

    Ok(())
}

/// Test empty scope token is rejected.
///
/// Validates that a token with no scopes is properly rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_rejects_empty_scope(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create token with empty scopes
    let empty_scope_token = server
        .create_service_token("test-service-empty", &[])
        .await?;

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&empty_scope_token)
        .json(&meeting_token_request(
            test_uuid(1),
            test_uuid(2),
            test_uuid(3),
            test_uuid(4),
        ))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Empty scope token should return 403"
    );

    Ok(())
}

// ============================================================================
// Request Validation Tests
// ============================================================================

/// Test meeting token with minimum required fields.
///
/// Validates that request succeeds with only required fields.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_minimal_request(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    // Request with only required fields (use defaults for optionals)
    let payload = serde_json::json!({
        "subject_user_id": test_uuid(1).to_string(),
        "meeting_id": test_uuid(2).to_string(),
        "meeting_org_id": test_uuid(3).to_string(),
        "home_org_id": test_uuid(4).to_string()
    });

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Minimal request should succeed"
    );

    let body: serde_json::Value = response.json().await?;
    // Default TTL should be 900
    let expires_in = body["expires_in"].as_u64().unwrap();
    assert_eq!(expires_in, 900, "Default TTL should be 900");

    Ok(())
}

/// Test guest token with minimum required fields.
///
/// Validates that guest request succeeds with only required fields.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_minimal_request(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    // Request with only required fields
    let payload = serde_json::json!({
        "guest_id": test_uuid(1).to_string(),
        "display_name": "Guest User",
        "meeting_id": test_uuid(2).to_string(),
        "meeting_org_id": test_uuid(3).to_string()
    });

    // Act
    let response = client
        .post(format!("{}/api/v1/auth/internal/guest-token", server.url()))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Minimal request should succeed"
    );

    let body: serde_json::Value = response.json().await?;
    // Default TTL should be 900 (default)
    let expires_in = body["expires_in"].as_u64().unwrap();
    assert_eq!(expires_in, 900, "Default TTL should be 900");

    Ok(())
}

// ============================================================================
// JWT Claims Verification Tests
// ============================================================================

/// Test that meeting token contains proper claims.
///
/// Decodes the issued meeting token and verifies claims structure.
#[sqlx::test(migrations = "../../migrations")]
async fn test_meeting_token_claims_structure(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    let subject_user_id = test_uuid(100);
    let meeting_id = test_uuid(200);
    let meeting_org_id = test_uuid(300);
    let home_org_id = test_uuid(400);

    let mut payload =
        meeting_token_request(subject_user_id, meeting_id, meeting_org_id, home_org_id);
    payload["role"] = serde_json::json!("host");
    payload["capabilities"] = serde_json::json!(["video", "audio", "screen_share"]);

    // Act
    let response = client
        .post(format!(
            "{}/api/v1/auth/internal/meeting-token",
            server.url()
        ))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await?;
    let issued_token = body["token"].as_str().unwrap();

    // Decode the JWT payload (base64url decode the middle part)
    let parts: Vec<&str> = issued_token.split('.').collect();
    let payload_b64 = parts[1];

    // Base64url decode
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let payload_json = URL_SAFE_NO_PAD.decode(payload_b64)?;
    let claims: serde_json::Value = serde_json::from_slice(&payload_json)?;

    // Verify claims
    assert_eq!(
        claims["sub"].as_str(),
        Some(subject_user_id.to_string().as_str()),
        "sub should be subject_user_id"
    );
    assert_eq!(
        claims["token_type"].as_str(),
        Some("meeting"),
        "token_type should be 'meeting'"
    );
    assert_eq!(
        claims["meeting_id"].as_str(),
        Some(meeting_id.to_string().as_str()),
        "meeting_id should match"
    );
    assert_eq!(
        claims["role"].as_str(),
        Some("host"),
        "role should be 'host'"
    );
    assert!(claims["jti"].is_string(), "jti should be present");
    assert!(claims["iat"].is_number(), "iat should be present");
    assert!(claims["exp"].is_number(), "exp should be present");

    // Verify capabilities array
    let caps = claims["capabilities"].as_array().unwrap();
    assert_eq!(caps.len(), 3, "Should have 3 capabilities");

    Ok(())
}

/// Test that guest token contains proper claims.
///
/// Decodes the issued guest token and verifies claims structure.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_claims_structure(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_service_token("gc-service", &["internal:meeting-token"])
        .await?;

    let guest_id = test_uuid(500);
    let meeting_id = test_uuid(600);
    let meeting_org_id = test_uuid(700);

    let mut payload = guest_token_request(guest_id, meeting_id, meeting_org_id);
    payload["display_name"] = serde_json::json!("Alice Guest");
    payload["waiting_room"] = serde_json::json!(false);

    // Act
    let response = client
        .post(format!("{}/api/v1/auth/internal/guest-token", server.url()))
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await?;
    let issued_token = body["token"].as_str().unwrap();

    // Decode the JWT payload
    let parts: Vec<&str> = issued_token.split('.').collect();
    let payload_b64 = parts[1];

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let payload_json = URL_SAFE_NO_PAD.decode(payload_b64)?;
    let claims: serde_json::Value = serde_json::from_slice(&payload_json)?;

    // Verify claims
    assert_eq!(
        claims["sub"].as_str(),
        Some(guest_id.to_string().as_str()),
        "sub should be guest_id"
    );
    assert_eq!(
        claims["token_type"].as_str(),
        Some("guest"),
        "token_type should be 'guest'"
    );
    assert_eq!(
        claims["display_name"].as_str(),
        Some("Alice Guest"),
        "display_name should match"
    );
    assert_eq!(
        claims["waiting_room"].as_bool(),
        Some(false),
        "waiting_room should be false"
    );
    assert_eq!(
        claims["participant_type"].as_str(),
        Some("guest"),
        "participant_type should be 'guest'"
    );
    assert_eq!(
        claims["role"].as_str(),
        Some("guest"),
        "role should be 'guest'"
    );
    assert!(claims["jti"].is_string(), "jti should be present");
    assert!(claims["iat"].is_number(), "iat should be present");
    assert!(claims["exp"].is_number(), "exp should be present");

    // Verify default capabilities
    let caps = claims["capabilities"].as_array().unwrap();
    assert!(
        caps.len() >= 2,
        "Should have at least video and audio capabilities"
    );

    Ok(())
}
