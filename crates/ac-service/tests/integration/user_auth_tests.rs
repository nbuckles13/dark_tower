//! E2E tests for user authentication flows (ADR-0020).
//!
//! Tests user registration and login endpoints with subdomain-based
//! organization extraction.
//!
//! ## Test Categories
//!
//! - **Registration**: User self-registration flow
//! - **Login**: User authentication flow
//! - **Org Extraction**: Subdomain-based organization identification
//!
//! ## Test Naming
//!
//! Tests follow the convention: `test_<feature>_<scenario>_<expected_result>`

use ac_test_utils::server_harness::TestAuthServer;
use reqwest::StatusCode;
use serde_json::json;
use sqlx::PgPool;

// ============================================================================
// Registration Tests (12 tests, incl. wire-shape golden lock)
// ============================================================================

/// Test that valid registration returns user_id and access_token.
///
/// Happy path: A new user can register with valid email, password, and display name.
/// Response includes user_id, access_token for auto-login.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_happy_path(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("acme", "Acme Corp").await?;

    // Act
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("acme"))
        .json(&json!({
            "email": "alice@example.com",
            "password": "password123",
            "displayName": "Alice"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Registration should succeed"
    );

    let body: serde_json::Value = response.json().await?;
    assert!(
        body.get("userId").is_some(),
        "Response should include userId"
    );
    assert!(
        body.get("accessToken").is_some(),
        "Response should include accessToken"
    );
    assert_eq!(body["email"].as_str(), Some("alice@example.com"));
    assert_eq!(body["displayName"].as_str(), Some("Alice"));
    assert_eq!(body["tokenType"].as_str(), Some("Bearer"));
    assert!(body["expiresIn"].as_u64().unwrap_or(0) > 0);

    Ok(())
}

/// WIRE-SHAPE GOLDEN LOCK at the HTTP-integration level (R-53 / task #23 / task #51).
///
/// Companion to the struct-level locks in `auth_handler.rs`. Exists because the
/// R-53 camelCase migration broke the register flow at the HTTP-INTEGRATION level
/// specifically — task #23's in-clone verification ran `cargo test -p ac-service
/// --lib` and never exercised this binary, so the struct-level serde tests passed
/// while these tests sent stale snake_case keys. This lock asserts the EXACT
/// real-wire round-trip (request accepted + full response key set) so a future
/// rename sweep that misses an HTTP surface fails loudly HERE, in the scope that
/// actually broke.
///
/// CONTRACT (R-11/R-53 as amended by task #51): the register response wire shape is
/// UNIFORM camelCase — task #51 removed the former per-field OAuth snake_case carve-out
/// on this user-flow endpoint. There is NO mixed scheme and NO carve-out.
///
/// IF THIS FAILS DURING A RENAME SWEEP: confirm the contract (R-11/R-53) before
/// touching the golden set — the SDK and every HTTP client depend on these exact
/// camelCase keys. (Task #51 applied a SINGLE rule: ALL AC token-response wire fields
/// are camelCase across every endpoint, including the genuine OAuth `/service/token`
/// client_credentials response — no per-field snake_case carve-out remains.)
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_wire_shape_golden_lock(pool: PgPool) -> Result<(), anyhow::Error> {
    use std::collections::BTreeSet;

    // The COMPLETE, intended set of register-response wire keys. Uniform camelCase
    // (task #51 single rule, no OAuth carve-out). Update ONLY in lockstep with a
    // deliberate R-53/R-11 contract change (and the struct-level lock).
    let golden_response_keys: BTreeSet<String> = [
        "userId",      // camelCase (R-53)
        "email",       // single-word, scheme-invariant
        "displayName", // camelCase (R-53)
        "accessToken", // camelCase (task #51 — was snake_case access_token)
        "tokenType",   // camelCase (task #51 — was snake_case token_type)
        "expiresIn",   // camelCase (task #51 — was snake_case expires_in)
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("wirelock", "Wire Lock Corp").await?;

    // Request MUST be sent with the exact camelCase wire keys; a snake_case
    // `display_name` here would 422 (this is the regression this test guards).
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("wirelock"))
        .json(&json!({
            "email": "wirelock@example.com",
            "password": "password123",
            "displayName": "Wire Lock",
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "golden camelCase request body must be accepted (R-53 wire contract). \
         A 422 here means the request derive drifted from the camelCase contract — \
         DO NOT silently re-baseline; confirm the contract and update the SDK (R-11)."
    );

    let body: serde_json::Value = response.json().await?;
    let actual_keys: BTreeSet<String> = body
        .as_object()
        .expect("register response must be a JSON object")
        .keys()
        .cloned()
        .collect();

    assert_eq!(
        actual_keys, golden_response_keys,
        "register response wire-key set drifted from the golden shape. \
         If intentional, update this golden set AND the struct-level lock in \
         auth_handler.rs AND the SDK (R-11). The shape is UNIFORM camelCase \
         (task #51 single rule — no OAuth snake_case carve-out)."
    );

    // Explicit credential-echo guards. Set-equality above already implies these,
    // but asserting them explicitly preserves the security intent if anyone
    // later loosens the equality check to a subset check.
    for forbidden in [
        "password",
        "passwordHash",
        "password_hash",
        "secret",
        "clientSecret",
        "client_secret",
    ] {
        assert!(
            !actual_keys.contains(forbidden),
            "register response must not contain credential key `{forbidden}` (no credential echo)"
        );
    }
    assert!(
        !body.to_string().contains("password123"),
        "register response must not echo the raw request password value anywhere"
    );

    // Single-rule invariant ENFORCED at the HTTP boundary (task #51): camelCase
    // present, snake_case forms ABSENT — the exact inverse of the pre-#51 carve-out.
    // A future reintroduction of the snake_case OAuth carve-out fails here.
    for camel in ["accessToken", "tokenType", "expiresIn"] {
        assert!(
            actual_keys.contains(camel),
            "user-flow token field `{camel}` must be camelCase on the wire (task #51 single rule). \
             DO NOT silently re-baseline: the per-field OAuth snake_case carve-out was removed."
        );
    }
    for snake in ["access_token", "token_type", "expires_in"] {
        assert!(
            !actual_keys.contains(snake),
            "snake_case form `{snake}` must be ABSENT on the user-flow wire (task #51 single rule). \
             DO NOT silently re-baseline: do NOT re-add the per-field #[serde(rename)] OAuth carve-out — \
             that reintroduces the mixed scheme task #51 removed."
        );
    }

    Ok(())
}

/// Test that registration token contains user claims.
///
/// The JWT access_token should contain sub, org_id, email, roles, jti claims.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_token_has_user_claims(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("claims", "Claims Corp").await?;

    // Act
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("claims"))
        .json(&json!({
            "email": "bob@example.com",
            "password": "securepass123",
            "displayName": "Bob"
        }))
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await?;
    let token = body["accessToken"]
        .as_str()
        .expect("Should have accessToken");

    // Decode JWT payload (second part)
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 parts");

    let payload_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1])?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)?;

    // Assert claims
    assert!(payload.get("sub").is_some(), "Token should have sub claim");
    assert!(
        payload.get("org_id").is_some(),
        "Token should have org_id claim"
    );
    assert!(
        payload.get("email").is_some(),
        "Token should have email claim"
    );
    assert!(
        payload.get("roles").is_some(),
        "Token should have roles claim"
    );
    assert!(payload.get("jti").is_some(), "Token should have jti claim");
    assert!(payload.get("iat").is_some(), "Token should have iat claim");
    assert!(payload.get("exp").is_some(), "Token should have exp claim");

    Ok(())
}

/// Test that new users get the default "user" role.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_assigns_default_user_role(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("roles", "Roles Corp").await?;

    // Act
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("roles"))
        .json(&json!({
            "email": "charlie@example.com",
            "password": "password123",
            "displayName": "Charlie"
        }))
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await?;
    let token = body["accessToken"]
        .as_str()
        .expect("Should have accessToken");

    // Decode JWT payload
    let parts: Vec<&str> = token.split('.').collect();
    let payload_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1])?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)?;

    // Assert roles includes "user"
    let roles = payload["roles"].as_array().expect("roles should be array");
    assert!(
        roles.iter().any(|r| r.as_str() == Some("user")),
        "New user should have 'user' role, got: {:?}",
        roles
    );

    Ok(())
}

/// Test that registration with invalid email format returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_invalid_email(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("email", "Email Corp").await?;

    // Act - invalid email format
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("email"))
        .json(&json!({
            "email": "not-an-email",
            "password": "password123",
            "displayName": "Invalid"
        }))
        .send()
        .await?;

    // Assert: 400 INVALID_REQUEST with the fixed message, and exactly one
    // subject-less failure row (the registration limiter's input).
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Invalid email should return 400"
    );
    assert_error_body(response, "INVALID_REQUEST", "Invalid email format").await?;
    assert_eq!(failure_rows(server.pool(), "invalid_email").await?, 1);

    Ok(())
}

/// Test that registration with password less than 8 characters returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_password_too_short(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("short", "Short Corp").await?;

    // Act - password too short (< 8 chars)
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("short"))
        .json(&json!({
            "email": "short@example.com",
            "password": "1234567",  // 7 chars, need 8
            "displayName": "Short Pass"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Short password should return 400"
    );
    assert_error_body(
        response,
        "INVALID_REQUEST",
        "Password must be at least 8 characters",
    )
    .await?;
    assert_eq!(failure_rows(server.pool(), "weak_password").await?, 1);

    Ok(())
}

/// Test that registration with empty display_name returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_empty_display_name(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("empty", "Empty Corp").await?;

    // Act - empty display name
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("empty"))
        .json(&json!({
            "email": "empty@example.com",
            "password": "password123",
            "displayName": ""
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Empty display name should return 400"
    );
    assert_error_body(response, "INVALID_REQUEST", "Display name cannot be empty").await?;
    assert_eq!(failure_rows(server.pool(), "empty_display_name").await?, 1);

    Ok(())
}

/// Test that registering with duplicate email in same org returns 409.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_duplicate_email(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("dup", "Dup Corp").await?;

    // First registration - should succeed
    let response1 = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("dup"))
        .json(&json!({
            "email": "duplicate@example.com",
            "password": "password123",
            "displayName": "First User"
        }))
        .send()
        .await?;

    assert_eq!(
        response1.status(),
        StatusCode::OK,
        "First registration should succeed"
    );

    // Act - second registration with same email
    let response2 = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("dup"))
        .json(&json!({
            "email": "duplicate@example.com",
            "password": "differentpass",
            "displayName": "Second User"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response2.status(),
        StatusCode::CONFLICT,
        "Duplicate email should return 409"
    );
    assert_error_body(
        response2,
        "CONFLICT",
        "An account with this email already exists",
    )
    .await?;
    assert_eq!(failure_rows(server.pool(), "email_exists").await?, 1);

    Ok(())
}

/// Test that same email can be used in different organizations.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_same_email_different_orgs(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org1 = server.create_test_org("org1", "Org 1").await?;
    let _org2 = server.create_test_org("org2", "Org 2").await?;

    // Act - register same email in org1
    let response1 = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("org1"))
        .json(&json!({
            "email": "shared@example.com",
            "password": "password123",
            "displayName": "Org 1 User"
        }))
        .send()
        .await?;

    // Assert first registration succeeds
    assert_eq!(
        response1.status(),
        StatusCode::OK,
        "First org registration should succeed"
    );

    // Act - register same email in org2
    let response2 = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("org2"))
        .json(&json!({
            "email": "shared@example.com",
            "password": "password456",
            "displayName": "Org 2 User"
        }))
        .send()
        .await?;

    // Assert second registration also succeeds
    assert_eq!(
        response2.status(),
        StatusCode::OK,
        "Same email in different org should succeed"
    );

    // Verify they have different user_ids
    // Note: response1 body was not captured before .text() was called on status check
    // So we just verify body2 has valid data
    let body2: serde_json::Value = response2.json().await?;
    assert!(
        body2.get("userId").is_some(),
        "Second registration should have userId"
    );

    Ok(())
}

/// Test that registration with invalid subdomain format returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_invalid_subdomain(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    // Don't create org - testing subdomain validation before DB lookup

    // Act - uppercase subdomain (invalid per extract_subdomain)
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header(
            "Host",
            format!("INVALID.localhost:{}", server.addr().port()),
        )
        .json(&json!({
            "email": "test@example.com",
            "password": "password123",
            "displayName": "Test"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Invalid subdomain format should return 401 (InvalidToken)"
    );

    Ok(())
}

/// Test that registration with unknown subdomain returns 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_unknown_org(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    // Don't create the "unknown" org

    // Act
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header("unknown"))
        .json(&json!({
            "email": "test@example.com",
            "password": "password123",
            "displayName": "Test"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Unknown subdomain should return 404"
    );

    Ok(())
}

/// Test that exactly the configured number of registrations per IP succeed
/// and the next is rate limited (429).
///
/// The limiter counts `user_registered` + `user_registration_failed` rows per
/// IP — one per attempt — so the threshold is the configured maximum itself.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_rate_limit(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server
        .create_test_org("ratelimit", "Rate Limit Corp")
        .await?;
    let max = usize::try_from(server.config().registration_rate_limit_max_attempts)?;

    let mut success_count = 0;
    let mut hit_rate_limit = false;

    for i in 0..=max {
        let response = server
            .client()
            .post(format!("{}/api/v1/auth/register", server.url()))
            .header("Host", server.host_header("ratelimit"))
            .json(&json!({
                "email": format!("user{}@example.com", i),
                "password": "password123",
                "displayName": format!("User {}", i)
            }))
            .send()
            .await?;

        if response.status() == StatusCode::OK {
            success_count += 1;
        } else if response.status() == StatusCode::TOO_MANY_REQUESTS {
            hit_rate_limit = true;
            break;
        }
    }

    // Assert: exactly MAX succeed, attempt MAX+1 is refused.
    assert_eq!(
        success_count, max,
        "exactly the configured maximum of registrations should succeed"
    );
    assert!(hit_rate_limit, "attempt {} should be rate limited", max + 1);

    Ok(())
}

// ============================================================================
// Login Tests (7 tests)
// ============================================================================

/// Test that valid login returns access_token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_happy_path(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server.create_test_org("login", "Login Corp").await?;
    let _user_id = server
        .create_test_user(org_id, "loginuser@example.com", "password123", "Login User")
        .await?;

    // Act
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("login"))
        .json(&json!({
            "email": "loginuser@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(response.status(), StatusCode::OK, "Login should succeed");

    let body: serde_json::Value = response.json().await?;
    assert!(
        body.get("accessToken").is_some(),
        "Response should include accessToken"
    );
    assert_eq!(body["tokenType"].as_str(), Some("Bearer"));
    assert!(body["expiresIn"].as_u64().unwrap_or(0) > 0);

    Ok(())
}

/// Test that login token contains correct user claims.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_token_has_user_claims(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server
        .create_test_org("loginclaims", "Login Claims Corp")
        .await?;
    let _user_id = server
        .create_test_user(org_id, "claims@example.com", "password123", "Claims User")
        .await?;

    // Act
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("loginclaims"))
        .json(&json!({
            "email": "claims@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await?;
    let token = body["accessToken"]
        .as_str()
        .expect("Should have accessToken");

    // Decode JWT payload
    let parts: Vec<&str> = token.split('.').collect();
    let payload_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1])?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)?;

    // Assert claims
    assert!(payload.get("sub").is_some(), "Token should have sub claim");
    assert!(
        payload.get("org_id").is_some(),
        "Token should have org_id claim"
    );
    assert_eq!(payload["email"].as_str(), Some("claims@example.com"));
    assert!(
        payload.get("roles").is_some(),
        "Token should have roles claim"
    );
    assert!(payload.get("jti").is_some(), "Token should have jti claim");

    Ok(())
}

/// Test that login updates last_login_at timestamp.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_updates_last_login(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server
        .create_test_org("lastlogin", "Last Login Corp")
        .await?;
    let user_id = server
        .create_test_user(
            org_id,
            "lastlogin@example.com",
            "password123",
            "Last Login User",
        )
        .await?;

    // Check initial last_login_at is NULL
    let initial: Option<(Option<chrono::DateTime<chrono::Utc>>,)> =
        sqlx::query_as("SELECT last_login_at FROM users WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(server.pool())
            .await?;

    assert!(
        initial.is_some() && initial.as_ref().unwrap().0.is_none(),
        "Initial last_login_at should be NULL"
    );

    // Act - login
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("lastlogin"))
        .json(&json!({
            "email": "lastlogin@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    // Assert - last_login_at should now be set
    let updated: Option<(Option<chrono::DateTime<chrono::Utc>>,)> =
        sqlx::query_as("SELECT last_login_at FROM users WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(server.pool())
            .await?;

    assert!(
        updated.is_some() && updated.as_ref().unwrap().0.is_some(),
        "last_login_at should be set after login"
    );

    Ok(())
}

/// Test that login with wrong password returns 401.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_wrong_password(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server
        .create_test_org("wrongpass", "Wrong Pass Corp")
        .await?;
    let _user_id = server
        .create_test_user(
            org_id,
            "wrongpass@example.com",
            "correctpassword",
            "Wrong Pass User",
        )
        .await?;

    // Act - wrong password
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("wrongpass"))
        .json(&json!({
            "email": "wrongpass@example.com",
            "password": "wrongpassword"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Wrong password should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_CREDENTIALS"),
        "Error code should be INVALID_CREDENTIALS"
    );

    Ok(())
}

/// Test that login with nonexistent email returns 401 (same error as wrong password).
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_nonexistent_user(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let _org_id = server.create_test_org("nouser", "No User Corp").await?;
    // Don't create the user

    // Act - login with nonexistent email
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("nouser"))
        .json(&json!({
            "email": "nonexistent@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    // Assert - should return same error as wrong password (prevent enumeration)
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Nonexistent user should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_CREDENTIALS"),
        "Error code should be INVALID_CREDENTIALS (same as wrong password)"
    );

    Ok(())
}

/// Test that login with inactive user returns 401.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_inactive_user(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server.create_test_org("inactive", "Inactive Corp").await?;
    let _user_id = server
        .create_inactive_test_user(
            org_id,
            "inactive@example.com",
            "password123",
            "Inactive User",
        )
        .await?;

    // Act
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("inactive"))
        .json(&json!({
            "email": "inactive@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Inactive user should return 401"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_CREDENTIALS"),
        "Error code should be INVALID_CREDENTIALS"
    );

    Ok(())
}

/// Test that login rate limiting kicks in after failed attempts.
#[sqlx::test(migrations = "../../migrations")]
async fn test_login_rate_limit_lockout(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server.create_test_org("lockout", "Lockout Corp").await?;
    let _user_id = server
        .create_test_user(
            org_id,
            "lockout@example.com",
            "correctpassword",
            "Lockout User",
        )
        .await?;

    // Make 5 failed login attempts
    for i in 0..5 {
        let response = server
            .client()
            .post(format!("{}/api/v1/auth/user/token", server.url()))
            .header("Host", server.host_header("lockout"))
            .json(&json!({
                "email": "lockout@example.com",
                "password": format!("wrongpassword{}", i)
            }))
            .send()
            .await?;

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Failed attempt {} should return 401",
            i + 1
        );
    }

    // 6th attempt should hit rate limit
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("lockout"))
        .json(&json!({
            "email": "lockout@example.com",
            "password": "wrongpassword6"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "6th failed attempt should return 429"
    );

    // Verify even correct password is blocked
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("lockout"))
        .json(&json!({
            "email": "lockout@example.com",
            "password": "correctpassword"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Correct password should also be blocked after lockout"
    );

    Ok(())
}

// ============================================================================
// Org Extraction Tests (4 tests)
// ============================================================================

/// Test that valid subdomain extracts org_id correctly.
#[sqlx::test(migrations = "../../migrations")]
async fn test_org_extraction_valid_subdomain(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server.create_test_org("validorg", "Valid Org").await?;
    let _user_id = server
        .create_test_user(
            org_id,
            "orgtest@example.com",
            "password123",
            "Org Test User",
        )
        .await?;

    // Act - login should work because org was extracted successfully
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("validorg"))
        .json(&json!({
            "email": "orgtest@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Valid subdomain should allow login"
    );

    // Verify org_id in token matches
    let body: serde_json::Value = response.json().await?;
    let token = body["accessToken"]
        .as_str()
        .expect("Should have accessToken");
    let parts: Vec<&str> = token.split('.').collect();
    let payload_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1])?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)?;

    assert_eq!(
        payload["org_id"].as_str(),
        Some(org_id.to_string().as_str()),
        "Token org_id should match"
    );

    Ok(())
}

/// Test that subdomain with port works (e.g., "acme.localhost:3000").
#[sqlx::test(migrations = "../../migrations")]
async fn test_org_extraction_with_port(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server.create_test_org("porttest", "Port Test").await?;
    let _user_id = server
        .create_test_user(
            org_id,
            "porttest@example.com",
            "password123",
            "Port Test User",
        )
        .await?;

    // Act - Host header includes port (which is normal for test server)
    // server.host_header already includes port
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", server.host_header("porttest"))
        .json(&json!({
            "email": "porttest@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Subdomain with port should work"
    );

    Ok(())
}

/// Test that IP address in Host header is rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn test_org_extraction_ip_rejected(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;

    // Act - Use IP address as Host header
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header("Host", format!("192.168.1.1:{}", server.addr().port()))
        .json(&json!({
            "email": "test@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    // Assert - Should fail because IP addresses don't have subdomains
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "IP address should be rejected"
    );

    Ok(())
}

/// Test that uppercase subdomain is rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn test_org_extraction_uppercase_rejected(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    // Create org with lowercase
    let _org_id = server
        .create_test_org("uppercase", "Uppercase Corp")
        .await?;

    // Act - Use uppercase subdomain (should be rejected by extract_subdomain)
    let response = server
        .client()
        .post(format!("{}/api/v1/auth/user/token", server.url()))
        .header(
            "Host",
            format!("UPPERCASE.localhost:{}", server.addr().port()),
        )
        .json(&json!({
            "email": "test@example.com",
            "password": "password123"
        }))
        .send()
        .await?;

    // Assert - Uppercase subdomain should be rejected
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Uppercase subdomain should be rejected"
    );

    Ok(())
}

// ============================================================================
// Registration error bodies + failed-attempt throttle (ADR-0020; devloop
// 2026-10-04 §10)
// ============================================================================

/// Assert an AC error body's `code` and exact `message`.
async fn assert_error_body(
    response: reqwest::Response,
    code: &str,
    message: &str,
) -> Result<(), anyhow::Error> {
    let body: serde_json::Value = response.json().await?;
    let error = body.get("error");
    assert_eq!(
        error.and_then(|e| e.get("code")).and_then(|v| v.as_str()),
        Some(code),
        "body: {body}"
    );
    assert_eq!(
        error
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str()),
        Some(message),
        "body: {body}"
    );
    Ok(())
}

/// Number of `user_registration_failed` rows with `failure_reason`.
async fn failure_rows(pool: &PgPool, reason: &str) -> Result<i64, anyhow::Error> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM auth_events \
         WHERE event_type = 'user_registration_failed' AND failure_reason = $1",
    )
    .bind(reason)
    .fetch_one(pool)
    .await?)
}

/// Total `auth_events` rows (for exact-delta assertions).
async fn total_events(pool: &PgPool) -> Result<i64, anyhow::Error> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM auth_events")
        .fetch_one(pool)
        .await?)
}

/// POST /api/v1/auth/register for `subdomain`.
async fn register(
    server: &TestAuthServer,
    subdomain: &str,
    email: &str,
    password: &str,
    display_name: &str,
) -> Result<reqwest::Response, anyhow::Error> {
    Ok(server
        .client()
        .post(format!("{}/api/v1/auth/register", server.url()))
        .header("Host", server.host_header(subdomain))
        .json(&json!({
            "email": email,
            "password": password,
            "displayName": display_name
        }))
        .send()
        .await?)
}

/// Insert a registration-limiter row for `ip` at `age` in the past (a
/// subject-less `user_registration_failed`, exactly what the limiter counts).
async fn insert_limiter_row(
    pool: &PgPool,
    ip: &str,
    age: chrono::Duration,
) -> Result<(), anyhow::Error> {
    sqlx::query(
        "INSERT INTO auth_events (event_type, success, failure_reason, ip_address, created_at) \
         VALUES ('user_registration_failed', false, 'invalid_email', $1::inet, $2)",
    )
    .bind(ip)
    .bind(chrono::Utc::now() - age)
    .execute(pool)
    .await?;
    Ok(())
}

/// (event_type, success, failure_reason, user_id, credential_id, ip, metadata,
/// whole row as JSON text).
type FailureRow = (
    String,
    bool,
    Option<String>,
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Option<String>,
    Option<serde_json::Value>,
    String,
);

/// T3: each failed attempt writes exactly one row (total delta 1), subject-
/// less, with the IP, a fixed reason and NULL metadata — and none of the
/// submitted strings anywhere in the row.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_failure_row_contents_carry_no_pii(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("pii", "Pii Corp").await?;

    let before = total_events(server.pool()).await?;
    let response = register(
        &server,
        "pii",
        "pii-probe@example.com",
        "short",
        "Pii Probe Name",
    )
    .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(total_events(server.pool()).await? - before, 1);

    let row: FailureRow = sqlx::query_as(
        "SELECT event_type, success, failure_reason, user_id, credential_id, \
             host(ip_address), metadata, row_to_json(auth_events)::text \
             FROM auth_events WHERE event_type = 'user_registration_failed'",
    )
    .fetch_one(server.pool())
    .await?;
    assert_eq!(row.0, "user_registration_failed");
    assert!(!row.1);
    assert_eq!(row.2.as_deref(), Some("weak_password"));
    assert!(row.3.is_none() && row.4.is_none());
    assert_eq!(row.5.as_deref(), Some("127.0.0.1"));
    assert!(row.6.is_none(), "metadata must be NULL");
    for submitted in [
        "pii-probe@example.com",
        "pii-probe",
        "short",
        "Pii Probe Name",
    ] {
        assert!(
            !row.7.contains(submitted),
            "row must not contain submitted value {submitted:?}: {}",
            row.7
        );
    }
    Ok(())
}

/// T1: N failed attempts exhaust the budget; attempt N+1 is 429.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_failed_attempts_exhaust_budget(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("failbudget", "Fail Corp").await?;
    let max = server.config().registration_rate_limit_max_attempts;

    for i in 0..max {
        let r = register(
            &server,
            "failbudget",
            &format!("bad{i}"),
            "password123",
            "X",
        )
        .await?;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "attempt {i}");
    }
    let r = register(&server, "failbudget", "ok@example.com", "password123", "Ok").await?;
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    Ok(())
}

/// T1: failed and successful attempts share one budget, each counted once:
/// N-1 mixed attempts leave room for one more; N mixed attempts do not.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_mixed_attempts_share_budget(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("mixed", "Mixed Corp").await?;
    let max = server.config().registration_rate_limit_max_attempts;
    assert!(max >= 3, "test needs a budget of at least 3");

    // 2 successes, then failures up to N-1 attempts total.
    for i in 0..2 {
        let r = register(
            &server,
            "mixed",
            &format!("ok{i}@example.com"),
            "password123",
            "Ok",
        )
        .await?;
        assert_eq!(r.status(), StatusCode::OK);
    }
    for i in 2..(max - 1) {
        let r = register(&server, "mixed", &format!("bad{i}"), "password123", "X").await?;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }
    // Attempt N is still allowed (a failure here, to keep the count exact).
    let r = register(&server, "mixed", "badlast", "password123", "X").await?;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST, "attempt N is allowed");
    // Attempt N+1 is refused.
    let r = register(
        &server,
        "mixed",
        "ok-final@example.com",
        "password123",
        "Ok",
    )
    .await?;
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    Ok(())
}

/// T2 + limiter-before-validation: at the limit an INVALID request gets 429
/// (not 400), and refused requests write no row — the count stays flat.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_rate_limited_requests_write_no_row(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("flat", "Flat Corp").await?;
    let max = server.config().registration_rate_limit_max_attempts;
    for _ in 0..max {
        insert_limiter_row(server.pool(), "127.0.0.1", chrono::Duration::zero()).await?;
    }

    let before = total_events(server.pool()).await?;
    for _ in 0..3 {
        // Weak password: would be a 400 if validation ran first.
        let r = register(&server, "flat", "x@example.com", "short", "X").await?;
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    }
    assert_eq!(
        total_events(server.pool()).await?,
        before,
        "429s must write no row"
    );
    Ok(())
}

/// T6: attempts from another IP do not count against this one.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_budget_is_per_ip(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("perip", "PerIp Corp").await?;
    let max = server.config().registration_rate_limit_max_attempts;
    for _ in 0..max {
        insert_limiter_row(server.pool(), "192.0.2.77", chrono::Duration::zero()).await?;
    }

    let r = register(&server, "perip", "a@example.com", "password123", "A").await?;
    assert_eq!(
        r.status(),
        StatusCode::OK,
        "IP-B's attempts must not block IP-A"
    );
    Ok(())
}

/// T6 window edge: rows just outside the window are not counted, rows just
/// inside are.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_window_edge(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("window", "Window Corp").await?;
    let max = server.config().registration_rate_limit_max_attempts;
    let window = chrono::Duration::minutes(server.config().registration_rate_limit_window_minutes);
    let margin = chrono::Duration::seconds(30);

    // Outside the window: a full budget's worth does not block.
    for _ in 0..max {
        insert_limiter_row(server.pool(), "127.0.0.1", window + margin).await?;
    }
    let r = register(&server, "window", "a@example.com", "password123", "A").await?;
    assert_eq!(
        r.status(),
        StatusCode::OK,
        "rows outside the window must not count"
    );

    // Just inside: top the in-window count up to the budget → refused.
    // (The successful registration above already spent one.)
    for _ in 0..(max - 1) {
        insert_limiter_row(server.pool(), "127.0.0.1", window - margin).await?;
    }
    let r = register(&server, "window", "b@example.com", "password123", "B").await?;
    assert_eq!(
        r.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "rows just inside the window must count"
    );
    Ok(())
}

/// T7: successful and failed logins do not consume the registration budget.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_logins_do_not_consume_budget(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server.create_test_org("logins", "Logins Corp").await?;
    server
        .create_test_user(org_id, "login@example.com", "password123", "Login User")
        .await?;
    let max = server.config().registration_rate_limit_max_attempts;

    for i in 0..max {
        let password = if i % 2 == 0 {
            "password123"
        } else {
            "wrong-password"
        };
        let response = server
            .client()
            .post(format!("{}/api/v1/auth/user/token", server.url()))
            .header("Host", server.host_header("logins"))
            .json(&json!({ "email": "login@example.com", "password": password }))
            .send()
            .await?;
        // Each login must actually reach authentication (and write its
        // user_login / user_login_failed row) — otherwise this test would pass
        // without exercising the decoupling.
        let expected = if i % 2 == 0 {
            StatusCode::OK
        } else {
            StatusCode::UNAUTHORIZED
        };
        assert_eq!(response.status(), expected, "login {i}");
    }
    let login_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM auth_events WHERE event_type IN ('user_login', 'user_login_failed')",
    )
    .fetch_one(server.pool())
    .await?;
    assert_eq!(login_rows, max, "every login wrote its audit row");

    for i in 0..max {
        let r = register(
            &server,
            "logins",
            &format!("new{i}@example.com"),
            "password123",
            "N",
        )
        .await?;
        assert_eq!(r.status(), StatusCode::OK, "registration {i}");
    }
    Ok(())
}

/// The limiter fails closed: when its count query cannot read `auth_events`,
/// registration is refused with 500 BEFORE any user row is inserted (a
/// `unwrap_or(0)` regression would let it through unthrottled).
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_fails_closed_when_limiter_cannot_count(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("closed", "Closed Corp").await?;
    // Break only the auth_events read path; `users` is untouched.
    sqlx::query("ALTER TABLE auth_events RENAME TO auth_events_unavailable")
        .execute(server.pool())
        .await?;

    let r = register(&server, "closed", "closed@example.com", "password123", "C").await?;
    assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = r.json().await?;
    assert_eq!(body["error"]["code"], "DATABASE_ERROR");

    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind("closed@example.com")
        .fetch_one(server.pool())
        .await?;
    assert_eq!(
        users, 0,
        "no user may be created when the limiter cannot count"
    );
    Ok(())
}

/// Registration records `user_registered` (plus the auto-login `user_login`)
/// — no longer two `user_login` rows.
#[sqlx::test(migrations = "../../migrations")]
async fn test_register_writes_user_registered_event(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    server.create_test_org("events", "Events Corp").await?;

    let r = register(&server, "events", "ev@example.com", "password123", "Ev").await?;
    assert_eq!(r.status(), StatusCode::OK);

    let counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT event_type, COUNT(*) FROM auth_events \
         WHERE event_type IN ('user_registered', 'user_login') GROUP BY event_type \
         ORDER BY event_type",
    )
    .fetch_all(server.pool())
    .await?;
    assert_eq!(
        counts,
        vec![
            ("user_login".to_string(), 1),
            ("user_registered".to_string(), 1)
        ]
    );
    Ok(())
}
