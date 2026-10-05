//! Integration tests for admin endpoint authentication
//!
//! These tests validate the `require_admin_scope` middleware by exercising
//! the admin endpoints (e.g., POST /api/v1/admin/services/register) with various
//! authentication scenarios.
//!
//! Coverage target: middleware/auth.rs (currently 0%)

use ac_test_utils::server_harness::TestAuthServer;
use reqwest::StatusCode;
use sqlx::PgPool;

// ============================================================================
// Test 1: Admin endpoint requires authentication
// ============================================================================

/// Test that admin endpoint rejects requests without Authorization header
///
/// Validates that the `require_admin_scope` middleware returns 401 when
/// no Authorization header is present.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_requires_authentication(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Act - Request without Authorization header
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .json(&serde_json::json!({
            "service_type": "global-controller",
            "region": "us-west-2"
        }))
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

// ============================================================================
// Test 2: Admin endpoint rejects invalid token
// ============================================================================

/// Test that admin endpoint rejects malformed Authorization header
///
/// Validates that the middleware returns 401 when the Authorization header
/// doesn't match "Bearer <token>" format.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_rejects_invalid_token(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Act - Malformed Authorization header (missing "Bearer " prefix)
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .header("Authorization", "InvalidFormat some-random-token")
        .json(&serde_json::json!({
            "service_type": "global-controller",
        }))
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

/// Test that admin endpoint rejects completely invalid JWT
///
/// Validates that the middleware returns 401 when the token is
/// not a valid JWT at all (e.g., random string).
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_rejects_garbage_token(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Act - Completely invalid token (not even a JWT)
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth("not-a-valid-jwt-token")
        .json(&serde_json::json!({
            "service_type": "meeting-controller",
        }))
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

// ============================================================================
// Test 3: Admin endpoint rejects insufficient scope
// ============================================================================

/// Test that admin endpoint rejects token without admin:services scope
///
/// Validates that a valid service token with other scopes (but not admin:services)
/// is rejected with 403 Forbidden.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_rejects_insufficient_scope(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create service token WITHOUT admin:services scope
    let token = server
        .create_service_token("test-service", &["scope-a", "scope-b"])
        .await?;

    // Act - Request with valid token but wrong scope
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "service_type": "media-handler",
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Token without admin:services scope should return 403"
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
        message.contains("admin:services"),
        "Error message should mention required scope, got: {}",
        message
    );

    Ok(())
}

// ============================================================================
// Test 4: Admin endpoint accepts valid admin token
// ============================================================================

/// Test that admin endpoint accepts token with admin:services scope
///
/// Validates the happy path: a service token with admin:services scope
/// can successfully register a new service.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_accepts_valid_admin_token(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create service token WITH admin:services scope
    let admin_token = server
        .create_service_token("admin-service", &["admin:services"])
        .await?;

    // Act - Request with valid admin token
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({
            "service_type": "global-controller",
            "region": "us-east-1"
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Valid admin token should succeed: {:?}",
        response.text().await?
    );

    // Verify response contains expected fields
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({
            "service_type": "meeting-controller",
        }))
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await?;
    assert!(
        body["client_id"].is_string(),
        "Response should include client_id"
    );
    assert!(
        body["client_secret"].is_string(),
        "Response should include client_secret"
    );
    assert_eq!(
        body["service_type"].as_str(),
        Some("meeting-controller"),
        "Response should include service_type"
    );
    assert!(body["scopes"].is_array(), "Response should include scopes");

    Ok(())
}

// ============================================================================
// Test 5: Admin endpoint rejects user token
// ============================================================================

/// A scope-bearing token with no `service_type` claim passes the admin gate.
///
/// `require_admin_scope` checks the `admin:services` scope, not
/// `service_type`, so a `ServiceClaims` token without a service type is
/// accepted. (This is NOT a real user token — real AC user tokens carry
/// `UserClaims`, which have no `scope`; see
/// `test_admin_endpoint_rejects_real_user_token`.)
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_accepts_scope_bearing_token_without_service_type(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server
        .create_scope_token_without_service_type("user-alice", &["admin:services"])
        .await?;

    // Act
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "service_type": "global-controller",
        }))
        .send()
        .await?;

    // Assert: the gate is scope-based.
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a ServiceClaims token with admin:services passes regardless of service_type"
    );

    Ok(())
}

/// A REAL AC user token (`UserClaims`, roles incl. "admin", same signing key)
/// is rejected with 401 on an admin route.
///
/// The rejection is shape-based today: `ServiceClaims.scope` is required and
/// `UserClaims` has none, so verification fails to deserialize. This test pins
/// that guard — adding a `scope` field to `UserClaims` must fail here loudly.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_rejects_real_user_token(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestAuthServer::spawn(pool).await?;
    let org_id = server.create_test_org("realuser", "Real User Corp").await?;
    let token = server
        .create_real_user_token(org_id, &["user", "admin"])
        .await?;

    let response = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/admin/clients/{}",
            server.url(),
            uuid::Uuid::new_v4()
        ))
        .bearer_auth(&token)
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}

// ============================================================================
// Test 6: Admin endpoint rejects expired token
// ============================================================================

/// Test that admin endpoint rejects expired tokens
///
/// Validates that expired tokens are rejected with 401 Unauthorized during
/// JWT verification in the middleware.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_rejects_expired_token(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create an expired token (expired 1 hour ago) with admin:services scope
    let expired_token = server
        .create_expired_token("admin-service", &["admin:services"], 3600)
        .await?;

    // Act - Request with expired token
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&expired_token)
        .json(&serde_json::json!({
            "service_type": "global-controller",
        }))
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

// ============================================================================
// Test 7: Admin endpoint validates token signature
// ============================================================================

/// Test that admin endpoint rejects tokens with invalid signatures
///
/// Validates that tokens signed with a different key are rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_rejects_wrong_signature(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create a valid token and tamper with it by changing the signature
    let valid_token = server
        .create_service_token("admin-service", &["admin:services"])
        .await?;

    // Tamper with the signature (last part of JWT)
    let parts: Vec<&str> = valid_token.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT should have 3 parts");

    // Change last character of signature to invalidate it
    let tampered_signature = format!("{}X", &parts[2][..parts[2].len() - 1]);
    let tampered_token = format!("{}.{}.{}", parts[0], parts[1], tampered_signature);

    // Act - Request with tampered token
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&tampered_token)
        .json(&serde_json::json!({
            "service_type": "meeting-controller",
        }))
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
// Test 8: Multiple scopes including admin:services
// ============================================================================

/// Test that admin endpoint accepts token with multiple scopes including admin:services
///
/// Validates that the middleware correctly parses space-separated scopes and
/// accepts tokens that have admin:services among other scopes.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_accepts_multiple_scopes(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create token with multiple scopes including admin:services
    let multi_scope_token = server
        .create_service_token("multi-admin", &["admin:services", "scope-a", "scope-b"])
        .await?;

    // Act - Request with multi-scope token
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&multi_scope_token)
        .json(&serde_json::json!({
            "service_type": "media-handler",
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Token with multiple scopes including admin:services should succeed"
    );

    Ok(())
}

// ============================================================================
// Test 9: Case-sensitive scope matching
// ============================================================================

/// Test that scope matching is case-sensitive
///
/// Validates that "Admin:Services" or "ADMIN:SERVICES" does NOT match
/// the required "admin:services" scope.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_scope_case_sensitive(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create token with wrong-case scope
    let wrong_case_token = server
        .create_service_token("test-service", &["Admin:Services"]) // Wrong case
        .await?;

    // Act - Request with wrong-case scope
    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&wrong_case_token)
        .json(&serde_json::json!({
            "service_type": "global-controller",
        }))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Wrong-case scope should return 403"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INSUFFICIENT_SCOPE"),
        "Error code should be INSUFFICIENT_SCOPE"
    );

    Ok(())
}

// ============================================================================
// Test 10: Token with similar but different scope
// ============================================================================

/// Test that similar scopes don't match (prefix/suffix attacks)
///
/// Validates that scopes like "admin:services:extra" or "pre:admin:services"
/// do NOT match the required "admin:services" scope.
#[sqlx::test(migrations = "../../migrations")]
async fn test_admin_endpoint_rejects_scope_prefix_suffix(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Test with suffix
    let suffix_token = server
        .create_service_token("test-service", &["admin:services:read"]) // Has suffix
        .await?;

    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&suffix_token)
        .json(&serde_json::json!({
            "service_type": "global-controller",
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Scope with suffix should not match"
    );

    // Test with prefix
    let prefix_token = server
        .create_service_token("test-service-2", &["super:admin:services"]) // Has prefix
        .await?;

    let response = client
        .post(format!("{}/api/v1/admin/services/register", server.url()))
        .bearer_auth(&prefix_token)
        .json(&serde_json::json!({
            "service_type": "meeting-controller",
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Scope with prefix should not match"
    );

    Ok(())
}

// ============================================================================
// Admin client routes through the REAL router (`/api/v1/admin/clients/{id}`)
//
// These four endpoints went live with axum 0.8: under 0.7 the `{id}` segment
// was a literal, so every existing-UUID request below would have been a 404.
// The existing-UUID success cases are the positive control for that.
// ============================================================================

mod admin_clients {
    use super::*;
    use serde_json::{json, Value};

    struct Fixture {
        server: TestAuthServer,
        admin: String,
        non_admin: String,
    }

    async fn fixture(pool: PgPool) -> Result<Fixture, anyhow::Error> {
        let server = TestAuthServer::spawn(pool).await?;
        let admin = server
            .create_service_token("admin-operator", &["admin:services"])
            .await?;
        let non_admin = server
            .create_service_token("plain-service", &["service.write.mc"])
            .await?;
        Ok(Fixture {
            server,
            admin,
            non_admin,
        })
    }

    /// A client created through the API (so a `service_registered` audit row
    /// references it, as in production).
    struct Client {
        id: String,
        client_id: String,
        secret: String,
    }

    async fn create_client(f: &Fixture, service_type: &str) -> Result<Client, anyhow::Error> {
        let resp = f
            .server
            .client()
            .post(format!("{}/api/v1/admin/clients", f.server.url()))
            .bearer_auth(&f.admin)
            .json(&json!({ "service_type": service_type }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = resp.json().await?;
        let field = |name: &str| {
            body.get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Ok(Client {
            id: field("id"),
            client_id: field("client_id"),
            secret: field("client_secret"),
        })
    }

    #[derive(Clone, Copy)]
    enum Route {
        Get,
        Put,
        Delete,
        Rotate,
    }

    const ROUTES: [Route; 4] = [Route::Get, Route::Put, Route::Delete, Route::Rotate];

    fn request(f: &Fixture, route: Route, id: &str) -> reqwest::RequestBuilder {
        let base = format!("{}/api/v1/admin/clients/{}", f.server.url(), id);
        let c = f.server.client();
        match route {
            Route::Get => c.get(base),
            Route::Put => c.put(base).json(&json!({ "scopes": [] })),
            Route::Delete => c.delete(base),
            Route::Rotate => c.post(format!("{base}/rotate-secret")),
        }
    }

    async fn total_events(pool: &PgPool) -> Result<i64, anyhow::Error> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM auth_events")
            .fetch_one(pool)
            .await?)
    }

    async fn service_token(
        f: &Fixture,
        client_id: &str,
        secret: &str,
        scope: Option<&str>,
    ) -> Result<reqwest::Response, anyhow::Error> {
        let body = match scope {
            Some(scope) => json!({
                "grant_type": "client_credentials",
                "client_id": client_id,
                "client_secret": secret,
                "scope": scope,
            }),
            None => json!({
                "grant_type": "client_credentials",
                "client_id": client_id,
                "client_secret": secret,
            }),
        };
        Ok(f.server
            .client()
            .post(format!("{}/api/v1/auth/service/token", f.server.url()))
            .json(&body)
            .send()
            .await?)
    }

    async fn get_detail(f: &Fixture, id: &str) -> Result<Value, anyhow::Error> {
        let resp = request(f, Route::Get, id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(resp.json().await?)
    }

    /// No secret material in detail responses: no `client_secret` and no key
    /// containing "hash".
    fn assert_no_secret_keys(body: &Value) {
        assert!(body.is_object(), "expected an object body: {body}");
        let keys: Vec<&String> = body
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        assert!(!keys.iter().any(|k| *k == "client_secret"), "body: {body}");
        assert!(
            keys.iter().all(|k| !k.contains("hash")),
            "body has a hash key: {body}"
        );
    }

    // ---- auth / routing matrix, every route -------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn no_token_is_401_with_www_authenticate(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        let before = total_events(f.server.pool()).await?;
        for route in ROUTES {
            for id in [c.id.as_str(), "not-a-uuid"] {
                let resp = request(&f, route, id).send().await?;
                assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
                assert!(resp.headers().contains_key("www-authenticate"));
            }
        }
        assert_eq!(total_events(f.server.pool()).await?, before, "no audit row");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn token_without_admin_scope_is_403(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        let before = total_events(f.server.pool()).await?;
        for route in ROUTES {
            let resp = request(&f, route, &c.id)
                .bearer_auth(&f.non_admin)
                .send()
                .await?;
            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
            let body: Value = resp.json().await?;
            assert_eq!(body["error"]["code"], "INSUFFICIENT_SCOPE");
        }
        assert_eq!(total_events(f.server.pool()).await?, before, "no audit row");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn unknown_uuid_reaches_handler_404(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let before = total_events(f.server.pool()).await?;
        for route in ROUTES {
            let resp = request(&f, route, &uuid::Uuid::new_v4().to_string())
                .bearer_auth(&f.admin)
                .send()
                .await?;
            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
            // Handler-shaped JSON, not the router's empty fallback 404.
            let body: Value = resp.json().await?;
            assert_eq!(body["error"]["code"], "NOT_FOUND");
        }
        assert_eq!(total_events(f.server.pool()).await?, before, "no audit row");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn malformed_id_is_400(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let before = total_events(f.server.pool()).await?;
        for route in ROUTES {
            let resp = request(&f, route, "not-a-uuid")
                .bearer_auth(&f.admin)
                .send()
                .await?;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        }
        assert_eq!(total_events(f.server.pool()).await?, before, "no audit row");
        Ok(())
    }

    // ---- success cases (would have been 404 under axum 0.7) ---------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn get_returns_seeded_client_without_secret(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        let body = get_detail(&f, &c.id).await?;
        assert_eq!(body["client_id"], c.client_id.as_str());
        assert_eq!(body["is_active"], true);
        assert_no_secret_keys(&body);
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn put_narrows_scopes_and_writes_one_audit_row(
        pool: PgPool,
    ) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;

        let before = total_events(f.server.pool()).await?;
        let resp = f
            .server
            .client()
            .put(format!("{}/api/v1/admin/clients/{}", f.server.url(), c.id))
            .bearer_auth(&f.admin)
            .json(&json!({ "scopes": ["service.write.mc"] }))
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = resp.json().await?;
        assert_no_secret_keys(&body);
        assert_eq!(total_events(f.server.pool()).await? - before, 1);

        let detail = get_detail(&f, &c.id).await?;
        assert_eq!(detail["scopes"], json!(["service.write.mc"]));

        let (event_type, success, failure_reason, metadata): (String, bool, Option<String>, Value) =
            sqlx::query_as(
                "SELECT event_type, success, failure_reason, metadata FROM auth_events \
                 WHERE credential_id = $1::uuid AND event_type = 'service_scopes_updated'",
            )
            .bind(&c.id)
            .fetch_one(f.server.pool())
            .await?;
        assert_eq!(event_type, "service_scopes_updated");
        assert!(success);
        assert!(failure_reason.is_none());
        assert_eq!(
            metadata,
            json!({
                "actor_sub": "admin-operator",
                "old_scopes": ["service.write.mc", "internal:meeting-token"],
                "new_scopes": ["service.write.mc"],
            })
        );
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_revokes_idempotently(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;

        let before = total_events(f.server.pool()).await?;
        let resp = request(&f, Route::Delete, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.json::<Value>().await?, json!({ "deleted": true }));
        assert_eq!(total_events(f.server.pool()).await? - before, 1);
        let (metadata,): (Value,) = sqlx::query_as(
            "SELECT metadata FROM auth_events \
             WHERE credential_id = $1::uuid AND event_type = 'service_deactivated' AND success",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        assert_eq!(metadata, json!({ "actor_sub": "admin-operator" }));

        // Soft delete: still readable, inactive.
        let detail = get_detail(&f, &c.id).await?;
        assert_eq!(detail["is_active"], false);

        // Repeat DELETE: 200, no new audit row.
        let before = total_events(f.server.pool()).await?;
        let resp = request(&f, Route::Delete, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(total_events(f.server.pool()).await?, before);

        // A narrowing PUT on the deactivated client succeeds, persists, writes
        // its audit row — and cannot reactivate.
        let before = total_events(f.server.pool()).await?;
        let resp = request(&f, Route::Put, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(total_events(f.server.pool()).await? - before, 1);
        let detail = get_detail(&f, &c.id).await?;
        assert_eq!(detail["is_active"], false);
        assert_eq!(detail["scopes"], json!([]));

        // Its credentials no longer get a token — and the refusal is the same
        // body a wrong secret gets (no "inactive" oracle).
        let revoked = service_token(&f, &c.client_id, &c.secret, None).await?;
        assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
        let revoked_body: Value = revoked.json().await?;
        let wrong = service_token(&f, &c.client_id, "wrong-secret", None).await?;
        assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(revoked_body, wrong.json::<Value>().await?);
        Ok(())
    }

    /// A deactivated client stays visible to list (inactive), and the PUT scope
    /// policy still applies to it: widening is refused with
    /// `scope_not_permitted` and nothing changes.
    #[sqlx::test(migrations = "../../migrations")]
    async fn deactivated_client_is_listed_and_still_cannot_widen(
        pool: PgPool,
    ) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        let resp = request(&f, Route::Delete, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let list: Value = f
            .server
            .client()
            .get(format!("{}/api/v1/admin/clients", f.server.url()))
            .bearer_auth(&f.admin)
            .send()
            .await?
            .json()
            .await?;
        let entry = list
            .as_array()
            .and_then(|items| items.iter().find(|i| i["id"] == c.id.as_str()))
            .cloned();
        assert!(
            entry.is_some(),
            "deactivated client missing from list: {list}"
        );
        assert_eq!(entry.unwrap_or_default()["is_active"], false);

        let scopes_before = get_detail(&f, &c.id).await?["scopes"].clone();
        let before = total_events(f.server.pool()).await?;
        let resp = put_scopes(&f, &c.id, json!(["admin:services"])).await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body: Value = resp.json().await?;
        assert_eq!(
            body["error"]["message"],
            "Scope not permitted for this client's service type"
        );
        assert_eq!(total_events(f.server.pool()).await? - before, 1);
        let (reason,): (Option<String>,) = sqlx::query_as(
            "SELECT failure_reason FROM auth_events \
             WHERE credential_id = $1::uuid AND event_type = 'service_scopes_updated' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        assert_eq!(reason.as_deref(), Some("scope_not_permitted"));
        let detail = get_detail(&f, &c.id).await?;
        assert_eq!(detail["scopes"], scopes_before);
        assert_eq!(detail["is_active"], false);
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn rotate_issues_working_secret_and_kills_old(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;

        let resp = request(&f, Route::Rotate, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = resp.json().await?;
        let new_secret = body["client_secret"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert_eq!(body["client_id"], c.client_id.as_str());
        assert!(!new_secret.is_empty() && new_secret != c.secret);

        // Exactly one audit row, secret-free.
        let (metadata, row_text): (Value, String) = sqlx::query_as(
            "SELECT metadata, row_to_json(auth_events)::text FROM auth_events \
             WHERE credential_id = $1::uuid AND event_type = 'service_secret_rotated'",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        assert_eq!(
            metadata,
            json!({ "actor_sub": "admin-operator", "client_id": c.client_id })
        );
        let (hash,): (String,) = sqlx::query_as(
            "SELECT client_secret_hash FROM service_credentials WHERE credential_id = $1::uuid",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        assert!(
            !row_text.contains(&new_secret),
            "audit row carries the secret"
        );
        assert!(!row_text.contains(&hash), "audit row carries the hash");

        assert_eq!(
            service_token(&f, &c.client_id, &new_secret, None)
                .await?
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            service_token(&f, &c.client_id, &c.secret, None)
                .await?
                .status(),
            StatusCode::UNAUTHORIZED
        );
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn rotate_delta_is_one_row(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        let before = total_events(f.server.pool()).await?;
        let resp = request(&f, Route::Rotate, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(total_events(f.server.pool()).await? - before, 1);
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn rotate_on_deactivated_is_409_and_mints_nothing(
        pool: PgPool,
    ) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        let resp = request(&f, Route::Delete, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::OK);

        let hash_before: (String,) = sqlx::query_as(
            "SELECT client_secret_hash FROM service_credentials WHERE credential_id = $1::uuid",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        let before = total_events(f.server.pool()).await?;

        let resp = request(&f, Route::Rotate, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body: Value = resp.json().await?;
        assert_eq!(body["error"]["code"], "CONFLICT");
        assert_eq!(body["error"]["message"], "client is deactivated");
        assert!(body.get("client_secret").is_none());

        let hash_after: (String,) = sqlx::query_as(
            "SELECT client_secret_hash FROM service_credentials WHERE credential_id = $1::uuid",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        assert_eq!(hash_before, hash_after, "stored hash must be unchanged");

        assert_eq!(total_events(f.server.pool()).await? - before, 1);
        let (success, reason, metadata): (bool, Option<String>, Value) = sqlx::query_as(
            "SELECT success, failure_reason, metadata FROM auth_events \
             WHERE credential_id = $1::uuid AND event_type = 'service_secret_rotated'",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        assert!(!success);
        assert_eq!(reason.as_deref(), Some("client_deactivated"));
        assert_eq!(metadata, json!({ "actor_sub": "admin-operator" }));
        Ok(())
    }

    // ---- A1: PUT may only narrow (no privilege escalation) ----------------

    async fn put_scopes(
        f: &Fixture,
        id: &str,
        scopes: Value,
    ) -> Result<reqwest::Response, anyhow::Error> {
        Ok(f.server
            .client()
            .put(format!("{}/api/v1/admin/clients/{}", f.server.url(), id))
            .bearer_auth(&f.admin)
            .json(&json!({ "scopes": scopes }))
            .send()
            .await?)
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn put_cannot_escalate_scopes(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let gc = create_client(&f, "global-controller").await?;
        let mh = create_client(&f, "media-handler").await?;

        for (client, scope) in [
            (&gc, "admin:services"),
            (&gc, "admin.force-rotate-keys.ac"),
            (&gc, "service.rotate-keys.ac"),
            (&gc, "service.write.mh"),       // another type's scope
            (&mh, "internal:meeting-token"), // GC-only scope on an MH client
        ] {
            let scopes_before = get_detail(&f, &client.id).await?["scopes"].clone();
            let before = total_events(f.server.pool()).await?;

            let resp = put_scopes(&f, &client.id, json!([scope])).await?;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{scope}");
            let body: Value = resp.json().await?;
            assert_eq!(body["error"]["code"], "INVALID_REQUEST");
            assert_eq!(
                body["error"]["message"],
                "Scope not permitted for this client's service type"
            );

            // Stored scopes unchanged; one refused audit row with the request.
            assert_eq!(get_detail(&f, &client.id).await?["scopes"], scopes_before);
            assert_eq!(total_events(f.server.pool()).await? - before, 1);
            let (success, reason, metadata): (bool, Option<String>, Value) = sqlx::query_as(
                "SELECT success, failure_reason, metadata FROM auth_events \
                 WHERE credential_id = $1::uuid AND event_type = 'service_scopes_updated' \
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(&client.id)
            .fetch_one(f.server.pool())
            .await?;
            assert!(!success);
            assert_eq!(reason.as_deref(), Some("scope_not_permitted"));
            assert_eq!(
                metadata,
                json!({ "actor_sub": "admin-operator", "requested_scopes": [scope] })
            );
        }

        // End to end: the refused scope is still not grantable.
        let resp = service_token(&f, &gc.client_id, &gc.secret, Some("admin:services")).await?;
        assert_ne!(resp.status(), StatusCode::OK);
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn put_scope_count_boundary(pool: PgPool) -> Result<(), anyhow::Error> {
        use ac_service::services::registration_service::MAX_SCOPES_PER_CLIENT;

        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;

        // MAX permitted scopes (duplicates of a permitted scope) → 200.
        let at_max = vec!["service.write.mc"; MAX_SCOPES_PER_CLIENT];
        let resp = put_scopes(&f, &c.id, json!(at_max)).await?;
        assert_eq!(resp.status(), StatusCode::OK);

        // MAX + 1 → 400.
        let over = vec!["service.write.mc"; MAX_SCOPES_PER_CLIENT + 1];
        let resp = put_scopes(&f, &c.id, json!(over)).await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body: Value = resp.json().await?;
        assert_eq!(body["error"]["message"], "Too many scopes");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn put_invalid_format_records_only_the_reason(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        let raw = "evil scope <script>";

        let resp = put_scopes(&f, &c.id, json!([raw])).await?;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body: Value = resp.json().await?;
        assert_eq!(
            body["error"]["message"],
            "Scope contains invalid characters"
        );

        let (reason, metadata, row_text): (Option<String>, Value, String) = sqlx::query_as(
            "SELECT failure_reason, metadata, row_to_json(auth_events)::text FROM auth_events \
             WHERE credential_id = $1::uuid AND event_type = 'service_scopes_updated'",
        )
        .bind(&c.id)
        .fetch_one(f.server.pool())
        .await?;
        assert_eq!(reason.as_deref(), Some("invalid_scope_format"));
        assert_eq!(metadata, json!({ "actor_sub": "admin-operator" }));
        assert!(
            !row_text.contains(raw),
            "raw rejected string must not be recorded"
        );
        Ok(())
    }

    // ---- error bodies and response headers --------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_and_register_invalid_service_type_fixed_message(
        pool: PgPool,
    ) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        for path in ["/api/v1/admin/clients", "/api/v1/admin/services/register"] {
            let resp = f
                .server
                .client()
                .post(format!("{}{}", f.server.url(), path))
                .bearer_auth(&f.admin)
                .json(&json!({ "service_type": "echo-me-back" }))
                .send()
                .await?;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{path}");
            let body: Value = resp.json().await?;
            assert_eq!(body["error"]["code"], "INVALID_REQUEST");
            assert_eq!(
                body["error"]["message"],
                "Invalid service_type. Must be one of: global-controller, meeting-controller, \
                 media-handler"
            );
        }
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn put_empty_and_too_long_scope_fixed_messages(
        pool: PgPool,
    ) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;
        for (scopes, message) in [
            (json!([""]), "Scope cannot be empty"),
            (
                json!(["a".repeat(101)]),
                "Scope exceeds maximum length of 100 characters",
            ),
        ] {
            let resp = put_scopes(&f, &c.id, scopes).await?;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
            let body: Value = resp.json().await?;
            assert_eq!(body["error"]["message"], message);
        }
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn responses_carry_no_store_except_jwks(pool: PgPool) -> Result<(), anyhow::Error> {
        let f = fixture(pool).await?;
        let c = create_client(&f, "global-controller").await?;

        let cache_control = |r: &reqwest::Response| {
            r.headers()
                .get("cache-control")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };

        let token = service_token(&f, &c.client_id, &c.secret, None).await?;
        assert_eq!(token.status(), StatusCode::OK);
        assert_eq!(cache_control(&token).as_deref(), Some("no-store"));

        let created = f
            .server
            .client()
            .post(format!("{}/api/v1/admin/clients", f.server.url()))
            .bearer_auth(&f.admin)
            .json(&json!({ "service_type": "media-handler" }))
            .send()
            .await?;
        assert_eq!(cache_control(&created).as_deref(), Some("no-store"));

        let rotated = request(&f, Route::Rotate, &c.id)
            .bearer_auth(&f.admin)
            .send()
            .await?;
        assert_eq!(cache_control(&rotated).as_deref(), Some("no-store"));

        // User token endpoint (failure response is enough to see the header).
        f.server.create_test_org("nostore", "No Store").await?;
        let user_token = f
            .server
            .client()
            .post(format!("{}/api/v1/auth/user/token", f.server.url()))
            .header("Host", f.server.host_header("nostore"))
            .json(&json!({ "email": "nobody@example.com", "password": "password123" }))
            .send()
            .await?;
        assert_eq!(cache_control(&user_token).as_deref(), Some("no-store"));

        let jwks = f
            .server
            .client()
            .get(format!("{}/.well-known/jwks.json", f.server.url()))
            .send()
            .await?;
        assert_eq!(cache_control(&jwks).as_deref(), Some("max-age=3600"));
        Ok(())
    }
}
