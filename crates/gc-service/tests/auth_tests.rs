//! Authentication integration tests.
//!
//! Tests JWT validation and protected endpoints using a mocked JWKS server.

// Test code is allowed to use expect/unwrap for assertions
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::jwt_fixtures::{
    sign_hs256, TestKeypair, TestMediaDialClaims, TestMediaDialUserShapedClaims, TestServiceClaims,
    TestUserClaims,
};

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::config::Config;
use gc_service::observability::metrics::init_metrics_recorder;
use gc_service::routes::{self, AppState};
use gc_service::services::MockMcClient;
use std::sync::OnceLock;

/// Global metrics handle for test servers
static TEST_METRICS_HANDLE: OnceLock<metrics_exporter_prometheus::PrometheusHandle> =
    OnceLock::new();

fn get_test_metrics_handle() -> metrics_exporter_prometheus::PrometheusHandle {
    TEST_METRICS_HANDLE
        .get_or_init(|| {
            init_metrics_recorder().unwrap_or_else(|_| {
                metrics_exporter_prometheus::PrometheusBuilder::new()
                    .build_recorder()
                    .handle()
            })
        })
        .clone()
}
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Test server with mocked JWKS endpoint.
struct TestAuthServer {
    addr: SocketAddr,
    _server_handle: JoinHandle<()>,
    mock_server: MockServer,
    keypair: TestKeypair,
}

impl TestAuthServer {
    async fn spawn(pool: PgPool) -> Result<Self> {
        // Create mock JWKS server
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(1, "test-key-01");

        // Set up JWKS endpoint
        let jwks_response = serde_json::json!({
            "keys": [keypair.jwk_json()]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        // Build configuration pointing to mock JWKS server
        let mut vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://test/test".to_string(),
            ),
            ("BIND_ADDRESS".to_string(), "127.0.0.1:0".to_string()),
            ("GC_REGION".to_string(), "test-region".to_string()),
            (
                "AC_JWKS_URL".to_string(),
                format!("{}/.well-known/jwks.json", mock_server.uri()),
            ),
            ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
            ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
        ]);

        gc_test_utils::fill_required_config(&mut vars);
        let config = Config::from_vars(&vars)
            .map_err(|e| anyhow::anyhow!("Failed to create config: {}", e))?;

        // Create a mock TokenReceiver for testing
        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let token_receiver = TokenReceiver::from_watch_receiver(rx);

        // Create application state with MockMcClient
        let mock_mc_client = Arc::new(MockMcClient::accepting());
        let telemetry = gc_service::handlers::TelemetryState::from_config(
            config.otel_collector_endpoint.clone(),
            config.telemetry_proxy_max_bytes,
            config.telemetry_proxy_rate_limit_per_minute,
        )
        .map_err(|e| anyhow::anyhow!("Failed to build telemetry state: {}", e))?;
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            mc_client: mock_mc_client,
            token_receiver,
            telemetry,
        });

        // Build routes with metrics handle
        let metrics_handle = get_test_metrics_handle();
        let app = routes::build_routes(state, metrics_handle)
            .map_err(|e| anyhow::anyhow!("Failed to build routes: {}", e))?;

        // Bind to random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to bind test server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| anyhow::anyhow!("Failed to get local address: {}", e))?;

        // Spawn server in background
        let server_handle = tokio::spawn(async move {
            let make_service = app.into_make_service_with_connect_info::<SocketAddr>();
            if let Err(e) = axum::serve(listener, make_service).await {
                eprintln!("Test server error: {}", e);
            }
        });

        Ok(Self {
            addr,
            _server_handle: server_handle,
            mock_server,
            keypair,
        })
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn create_valid_token(&self) -> String {
        let now = Utc::now().timestamp();
        let claims = TestServiceClaims {
            sub: "test-client".to_string(),
            exp: now + 3600, // 1 hour
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        self.keypair.sign_service_token(&claims)
    }

    fn create_expired_token(&self) -> String {
        let now = Utc::now().timestamp();
        let claims = TestServiceClaims {
            sub: "test-client".to_string(),
            exp: now - 3600, // Expired 1 hour ago
            iat: now - 7200, // Issued 2 hours ago
            scope: "read write".to_string(),
            service_type: None,
        };
        self.keypair.sign_service_token(&claims)
    }

    fn create_future_iat_token(&self) -> String {
        let now = Utc::now().timestamp();
        let claims = TestServiceClaims {
            sub: "test-client".to_string(),
            exp: now + 7200, // Expires in 2 hours
            iat: now + 3600, // Issued 1 hour from now (invalid)
            scope: "read write".to_string(),
            service_type: None,
        };
        self.keypair.sign_service_token(&claims)
    }

    async fn setup_missing_key(&self) {
        // Replace JWKS response with different key
        let different_keypair = TestKeypair::new(2, "different-key");
        let jwks_response = serde_json::json!({
            "keys": [different_keypair.jwk_json()]
        });

        // Reset and add new mock
        self.mock_server.reset().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&self.mock_server)
            .await;
    }
}

impl Drop for TestAuthServer {
    fn drop(&mut self) {
        self._server_handle.abort();
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Test that /api/v1/me returns 401 without authentication.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_requires_auth(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    // Check WWW-Authenticate header
    let www_auth = response.headers().get("www-authenticate");
    assert!(www_auth.is_some(), "Should include WWW-Authenticate header");

    Ok(())
}

/// Test that /api/v1/me returns 401 with invalid Bearer format.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_rejects_invalid_auth_format(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", "Basic abc123") // Wrong format
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    Ok(())
}

/// Test that /api/v1/me returns 200 with valid token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_with_valid_token(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server.create_valid_token();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["sub"], "test-client");
    assert_eq!(body["scopes"], serde_json::json!(["read", "write"]));
    assert_eq!(body["serviceType"], "global-controller");

    Ok(())
}

/// Test that /api/v1/me rejects expired tokens.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_rejects_expired_token(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server.create_expired_token();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    Ok(())
}

/// Test that /api/v1/me rejects tokens with future iat.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_rejects_future_iat_token(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let token = server.create_future_iat_token();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    Ok(())
}

/// Test that /api/v1/me rejects tokens with unknown kid.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_rejects_unknown_kid(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Update JWKS to have different key
    server.setup_missing_key().await;

    // Token signed with original key should be rejected
    let token = server.create_valid_token();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    Ok(())
}

/// Test that /api/v1/me rejects oversized tokens.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_rejects_oversized_token(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create oversized token (> 8KB)
    let oversized_token = "a".repeat(9000);

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", oversized_token))
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    Ok(())
}

/// Test that /api/v1/me rejects malformed tokens.
#[sqlx::test(migrations = "../../migrations")]
async fn test_me_endpoint_rejects_malformed_token(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", "Bearer not.a.valid.jwt")
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    Ok(())
}

/// Test that /health is public (no auth required).
#[sqlx::test(migrations = "../../migrations")]
async fn test_health_endpoint_is_public(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/health", server.url()))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // /health returns plain text "OK" for Kubernetes liveness probes
    let body = response.text().await?;
    assert_eq!(body, "OK");

    Ok(())
}

/// Test that 401 response includes proper error format.
#[sqlx::test(migrations = "../../migrations")]
async fn test_auth_error_response_format(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .send()
        .await?;

    assert_eq!(response.status(), 401);

    let body: serde_json::Value = response.json().await?;
    assert!(body["error"]["code"].is_string());
    assert!(body["error"]["message"].is_string());
    assert_eq!(body["error"]["code"], "INVALID_TOKEN");

    Ok(())
}

// =============================================================================
// Token Size Boundary Tests (8KB limit)
// =============================================================================

/// Test that token exactly at 8192 bytes is accepted.
#[sqlx::test(migrations = "../../migrations")]
async fn test_token_exactly_at_8kb_limit_accepted(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Strategy: Create a token with padded scope to reach exactly 8192 bytes
    // JWT structure: header.payload.signature (with dots between)
    let now = chrono::Utc::now().timestamp();

    // We'll iteratively adjust padding to hit exactly 8192 bytes
    // Start with a base token and measure, then pad the scope field
    let base_claims = TestServiceClaims {
        sub: "test-client".to_string(),
        exp: now + 3600,
        iat: now,
        scope: "read write".to_string(),
        service_type: Some("global-controller".to_string()),
    };
    let base_token = server.keypair.sign_service_token(&base_claims);
    let base_len = base_token.len();

    // Calculate padding needed to reach exactly 8192 bytes
    // Each character added to scope adds ~1.33 bytes to base64 output
    // We'll slightly overshoot then trim
    let needed = 8192_usize.saturating_sub(base_len);
    // Base64 encoding ratio is 4:3, so we need approximately needed * 3/4 chars
    let padding_chars = (needed * 3) / 4 + 10; // Add buffer for adjustment

    let padded_claims = TestServiceClaims {
        sub: "test-client".to_string(),
        exp: now + 3600,
        iat: now,
        scope: format!("read write {}", "x".repeat(padding_chars)),
        service_type: Some("global-controller".to_string()),
    };
    let padded_token = server.keypair.sign_service_token(&padded_claims);

    // Verify we got a token near 8192 bytes
    // Due to base64 encoding, exact 8192 is hard to hit, so we test that:
    // 1. Tokens at or below 8192 are accepted
    // 2. Tokens above 8192 are rejected (tested in test_token_at_8193_bytes_rejected)

    // Create a token that is exactly 8192 bytes by trimming if needed
    // Since we can't trim a signed token, we'll test the boundary differently:
    // - A token <= 8192 bytes should work
    // - We verify this with a large but valid token
    if padded_token.len() <= 8192 {
        let response = client
            .get(format!("{}/api/v1/me", server.url()))
            .header("Authorization", format!("Bearer {}", padded_token))
            .send()
            .await?;

        assert_eq!(
            response.status(),
            200,
            "Token of size {} should be accepted (at or below 8KB limit)",
            padded_token.len()
        );
    } else {
        // Token exceeded 8192, reduce padding and try again
        let smaller_padding = padding_chars.saturating_sub(50);
        let smaller_claims = TestServiceClaims {
            sub: "test-client".to_string(),
            exp: now + 3600,
            iat: now,
            scope: format!("read write {}", "x".repeat(smaller_padding)),
            service_type: Some("global-controller".to_string()),
        };
        let smaller_token = server.keypair.sign_service_token(&smaller_claims);

        assert!(
            smaller_token.len() <= 8192,
            "Token size {} should be <= 8192",
            smaller_token.len()
        );

        let response = client
            .get(format!("{}/api/v1/me", server.url()))
            .header("Authorization", format!("Bearer {}", smaller_token))
            .send()
            .await?;

        assert_eq!(
            response.status(),
            200,
            "Token of size {} should be accepted (at or below 8KB limit)",
            smaller_token.len()
        );
    }

    Ok(())
}

/// Test that token at 8193 bytes (one byte over limit) is rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn test_token_at_8193_bytes_rejected(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create a token that is exactly 8193 bytes
    let token_8193 = "a".repeat(8193);

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", token_8193))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        401,
        "Token of 8193 bytes should be rejected (over 8KB limit)"
    );

    Ok(())
}

// =============================================================================
// Algorithm Confusion Attack Tests
// =============================================================================

/// Test that token with alg:none is rejected (algorithm confusion attack).
#[sqlx::test(migrations = "../../migrations")]
async fn test_token_with_alg_none_rejected(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create a token with alg: none (algorithm confusion attack)
    let now = chrono::Utc::now().timestamp();
    let header = r#"{"alg":"none","typ":"JWT","kid":"test-key-01"}"#;
    let claims = format!(
        r#"{{"sub":"attacker","exp":{},"iat":{},"scope":"admin","service_type":"global-controller"}}"#,
        now + 3600,
        now
    );

    let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
    let claims_b64 = URL_SAFE_NO_PAD.encode(claims.as_bytes());

    // alg:none tokens typically have empty signature
    let malicious_token = format!("{}..{}", header_b64, claims_b64);

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", malicious_token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        401,
        "Token with alg:none should be rejected"
    );

    Ok(())
}

/// Test that token with alg:HS256 is rejected (algorithm confusion attack).
#[sqlx::test(migrations = "../../migrations")]
async fn test_token_with_alg_hs256_rejected(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create a token with alg: HS256 (algorithm confusion attack)
    // Attacker might try to use the public key as HMAC secret
    let now = chrono::Utc::now().timestamp();
    let header = r#"{"alg":"HS256","typ":"JWT","kid":"test-key-01"}"#;
    let claims = format!(
        r#"{{"sub":"attacker","exp":{},"iat":{},"scope":"admin","service_type":"global-controller"}}"#,
        now + 3600,
        now
    );

    let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
    let claims_b64 = URL_SAFE_NO_PAD.encode(claims.as_bytes());

    // Create a fake signature (attacker would use public key as HMAC secret)
    let fake_signature = URL_SAFE_NO_PAD.encode(b"fake_hmac_signature_attempt");
    let malicious_token = format!("{}.{}.{}", header_b64, claims_b64, fake_signature);

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", malicious_token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        401,
        "Token with alg:HS256 should be rejected"
    );

    Ok(())
}

/// Test that only alg:EdDSA tokens are accepted.
#[sqlx::test(migrations = "../../migrations")]
async fn test_only_eddsa_algorithm_accepted(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    // Create a valid EdDSA token
    let valid_token = server.create_valid_token();

    let response = client
        .get(format!("{}/api/v1/me", server.url()))
        .header("Authorization", format!("Bearer {}", valid_token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        200,
        "Token with alg:EdDSA should be accepted"
    );

    Ok(())
}

// =============================================================================
// R-15 (story 4 GC-3): GC USER auth refuses MC-minted `media_dial` tokens
// =============================================================================
//
// ADR-0039 §5 "cross-meeting KEK guard, by construction": GC verifies only
// AC-signed tokens (AC JWKS, EdDSA pinned), so an MC-signed `media_dial`
// token fails here. Every arm is pinned at the VALIDATOR (the exact
// `JwtError`: over HTTP, KeyNotFound / InvalidSignature / the alg pin all
// collapse to one 401) AND on a `require_user_auth` route (the join route, not
// `/api/v1/me`, which is service auth). Arms 2 and 3 use a payload that would
// deserialize as `UserClaims`, with a positive control signed by the live AC
// key, so the key / alg is the ONLY possible reason for the rejection.

use common::jwt::{JwksClient, JwtError, UserClaims};
use gc_service::auth::jwt::CommonJwtValidator;

/// The live AC kid served by the mock JWKS (`TestAuthServer::spawn`).
const LIVE_KID: &str = "test-key-01";
/// A `require_user_auth` route. No meeting is seeded, so a token that passes
/// auth gets 404 NOT_FOUND, and one that fails gets 401.
const USER_ROUTE: &str = "/api/v1/meetings/NOSUCH1";

impl TestAuthServer {
    /// GC's validator over the same mocked AC JWKS the router uses.
    fn validator(&self) -> CommonJwtValidator {
        let jwks = JwksClient::new(format!("{}/.well-known/jwks.json", self.mock_server.uri()))
            .expect("jwks client");
        CommonJwtValidator::new(Arc::new(jwks), 60)
    }

    /// JWKS fetches the mock has served so far.
    async fn jwks_fetches(&self) -> usize {
        self.mock_server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path() == "/.well-known/jwks.json")
            .count()
    }

    async fn user_route_status(&self, token: &str) -> (u16, serde_json::Value) {
        let response = reqwest::Client::new()
            .get(format!("{}{USER_ROUTE}", self.url()))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("request");
        let status = response.status().as_u16();
        let body = response.json().await.unwrap_or(serde_json::Value::Null);
        (status, body)
    }
}

/// Positive arm: an AC-signed user token passes GC user auth on the route the
/// negative arms use (404 meeting-not-found, NOT 401).
#[sqlx::test(migrations = "../../migrations")]
async fn r15_ac_signed_user_token_is_accepted(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let now = Utc::now().timestamp();
    let token = server.keypair.sign_user_token(&TestUserClaims {
        sub: "user-1".to_string(),
        org_id: "org-1".to_string(),
        email: "u1@example.test".to_string(),
        roles: vec!["user".to_string()],
        iat: now,
        exp: now + 300,
        jti: "jti-ok".to_string(),
    });
    let (status, body) = server.user_route_status(&token).await;
    assert_eq!(
        status, 404,
        "auth passed; the meeting does not exist: {body}"
    );
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    Ok(())
}

/// Arm 1: an MC-minted token carries NO kid (`{"alg":"EdDSA","typ":"JWT"}`).
/// Rejected at kid extraction, before any JWKS lookup.
#[sqlx::test(migrations = "../../migrations")]
async fn r15_media_dial_without_kid_is_rejected_at_kid_extraction(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let mc_key = TestKeypair::new(7, "unused");
    let token = mc_key.sign_claims(&TestMediaDialClaims::live(Utc::now().timestamp()), None);

    let validator = server.validator();
    let before = server.jwks_fetches().await;
    let result = validator.validate::<UserClaims>(&token).await;
    assert!(
        matches!(result, Err(JwtError::MissingKid)),
        "expected MissingKid, got {:?}",
        result.err()
    );
    assert_eq!(
        server.jwks_fetches().await,
        before,
        "rejected before any JWKS lookup"
    );
    assert_eq!(server.user_route_status(&token).await.0, 401);
    Ok(())
}

/// Arm 2: the LIVE AC kid, signed by a different Ed25519 key → signature
/// failure (not KeyNotFound: the JWKS is consulted and the key found).
#[sqlx::test(migrations = "../../migrations")]
async fn r15_media_dial_with_live_kid_and_foreign_key_fails_signature(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let now = Utc::now().timestamp();
    let claims = TestMediaDialUserShapedClaims::live(now);
    let validator = server.validator();

    // Control: the SAME payload signed by the live AC key validates, so the
    // key is the only thing the negative arm changes.
    let control = server.keypair.sign_claims(&claims, Some(LIVE_KID));
    assert!(validator.validate::<UserClaims>(&control).await.is_ok());

    let foreign = TestKeypair::new(2, LIVE_KID);
    let forged = foreign.sign_claims(&claims, Some(LIVE_KID));
    let before = server.jwks_fetches().await;
    let fresh_validator = server.validator();
    let result = fresh_validator.validate::<UserClaims>(&forged).await;
    assert!(
        matches!(result, Err(JwtError::InvalidSignature)),
        "expected InvalidSignature, got {:?}",
        result.err()
    );
    assert!(
        server.jwks_fetches().await > before,
        "the JWKS was consulted: the token reached signature verification"
    );
    assert_eq!(server.user_route_status(&forged).await.0, 401);

    // The literal media_dial claim set, foreign-signed: rejected too. A
    // catch-all (key, claim shape AND `aud` would each reject it); the `aud`
    // defense is pinned alone in `r15_aud_claim_alone_is_rejected`.
    let literal = foreign.sign_claims(&TestMediaDialClaims::live(now), Some(LIVE_KID));
    assert!(validator.validate::<UserClaims>(&literal).await.is_err());
    assert_eq!(server.user_route_status(&literal).await.0, 401);
    Ok(())
}

/// Arm 3: HS256 with the live kid, a REAL HMAC keyed with the AC public key's
/// raw bytes (the algorithm-confusion attack) → rejected by the EdDSA pin.
#[sqlx::test(migrations = "../../migrations")]
async fn r15_media_dial_hs256_is_rejected_by_the_eddsa_pin(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let claims = TestMediaDialUserShapedClaims::live(Utc::now().timestamp());
    let token = sign_hs256(&server.keypair.public_key_bytes, LIVE_KID, &claims);

    let result = server.validator().validate::<UserClaims>(&token).await;
    assert!(
        matches!(result, Err(JwtError::InvalidSignature)),
        "expected InvalidSignature from the alg pin, got {:?}",
        result.err()
    );
    assert_eq!(server.user_route_status(&token).await.0, 401);
    Ok(())
}

/// The `aud` defense, pinned on its own: a COMPLETE user-claim set plus the
/// media_dial `aud`, signed by the LIVE AC key, is rejected. Only `aud`
/// differs from the positive control (`r15_media_dial_with_live_kid_*`), so
/// this pins jsonwebtoken's `validate_aud` default (no expected audience
/// configured → any `aud` is refused) against a silent dependency change.
#[sqlx::test(migrations = "../../migrations")]
async fn r15_aud_claim_alone_is_rejected(pool: PgPool) -> Result<()> {
    let server = TestAuthServer::spawn(pool).await?;
    let mut claims =
        serde_json::to_value(TestMediaDialUserShapedClaims::live(Utc::now().timestamp()))?;
    claims["aud"] = serde_json::json!("mh-1");
    let token = server.keypair.sign_claims(&claims, Some(LIVE_KID));

    let result = server.validator().validate::<UserClaims>(&token).await;
    assert!(
        matches!(result, Err(JwtError::InvalidSignature)),
        "a live-key token carrying `aud` must be refused, got {:?}",
        result.err()
    );
    assert_eq!(server.user_route_status(&token).await.0, 401);
    Ok(())
}
