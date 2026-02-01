//! Meeting integration tests for Global Controller.
//!
//! Tests the meeting join, guest token, and settings update endpoints:
//!
//! - `GET /v1/meetings/{code}` - Join meeting (authenticated)
//! - `POST /v1/meetings/{code}/guest-token` - Get guest token (public)
//! - `PATCH /v1/meetings/{id}/settings` - Update meeting settings (host only)
//!
//! # Test Setup
//!
//! Tests use:
//! - wiremock to mock AC internal endpoints for token generation
//! - sqlx test macro for database setup with migrations
//! - Ed25519 keypair for signing test tokens

// Test code is allowed to use expect/unwrap for assertions
#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use futures::future::join_all;
use global_controller::config::Config;
use global_controller::routes::{self, AppState};
use global_controller::services::MockMcClient;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ============================================================================
// Test Helpers
// ============================================================================

/// JWT Claims for test tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestClaims {
    sub: String,
    exp: i64,
    iat: i64,
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_type: Option<String>,
}

/// Test keypair for signing tokens.
struct TestKeypair {
    kid: String,
    public_key_bytes: Vec<u8>,
    private_key_pkcs8: Vec<u8>,
}

impl TestKeypair {
    fn new(seed: u8, kid: &str) -> Self {
        // Create deterministic seed
        let mut seed_bytes = [0u8; 32];
        seed_bytes[0] = seed;
        for (i, byte) in seed_bytes.iter_mut().enumerate().skip(1) {
            *byte = seed.wrapping_mul(i as u8).wrapping_add(i as u8);
        }

        let key_pair = Ed25519KeyPair::from_seed_unchecked(&seed_bytes)
            .expect("Failed to create test keypair");

        let public_key_bytes = key_pair.public_key().as_ref().to_vec();
        let private_key_pkcs8 = build_pkcs8_from_seed(&seed_bytes);

        Self {
            kid: kid.to_string(),
            public_key_bytes,
            private_key_pkcs8,
        }
    }

    fn sign_token(&self, claims: &TestClaims) -> String {
        let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.kid.clone());

        encode(&header, claims, &encoding_key).expect("Failed to sign token")
    }

    /// Create a token with HS256 algorithm (wrong algorithm attack).
    fn create_hs256_token(&self, claims: &TestClaims) -> String {
        // Create a fake HS256 token - this uses the public key as the HMAC secret
        // which is a known attack vector (algorithm confusion)
        let encoding_key = EncodingKey::from_secret(&self.public_key_bytes);
        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.kid.clone());

        encode(&header, claims, &encoding_key).expect("Failed to sign HS256 token")
    }

    /// Create a token signed with a different (unknown) key.
    fn create_token_with_wrong_key(&self, claims: &TestClaims) -> String {
        // Create a different keypair (different seed)
        let wrong_keypair = TestKeypair::new(99, &self.kid); // Same kid, different key
        wrong_keypair.sign_token(claims)
    }

    /// Create a tampered token (modify payload after signing).
    fn create_tampered_token(&self, claims: &TestClaims) -> String {
        // First, sign the token normally
        let valid_token = self.sign_token(claims);

        // Parse the token parts
        let parts: Vec<&str> = valid_token.split('.').collect();
        let header = parts.first().expect("JWT missing header");
        let signature = parts.get(2).expect("JWT missing signature");

        // Create modified claims
        let mut modified_claims = claims.clone();
        modified_claims.scope = "admin superuser".to_string(); // Escalate privileges

        // Encode the modified payload
        let modified_payload =
            URL_SAFE_NO_PAD.encode(serde_json::to_string(&modified_claims).unwrap().as_bytes());

        // Reassemble with original signature (which won't match the modified payload)
        format!("{}.{}.{}", header, modified_payload, signature)
    }

    fn jwk_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kty": "OKP",
            "kid": self.kid,
            "crv": "Ed25519",
            "x": URL_SAFE_NO_PAD.encode(&self.public_key_bytes),
            "alg": "EdDSA",
            "use": "sig"
        })
    }
}

/// Build PKCS#8 v1 document from Ed25519 seed.
fn build_pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
    let mut pkcs8 = Vec::new();

    // Outer SEQUENCE tag
    pkcs8.push(0x30);
    pkcs8.push(0x2e); // Length: 46 bytes

    // Version: INTEGER 0
    pkcs8.extend_from_slice(&[0x02, 0x01, 0x00]);

    // Algorithm Identifier: SEQUENCE
    pkcs8.push(0x30);
    pkcs8.push(0x05); // Length: 5 bytes
                      // OID for Ed25519: 1.3.101.112
    pkcs8.extend_from_slice(&[0x06, 0x03, 0x2b, 0x65, 0x70]);

    // Private Key: OCTET STRING
    pkcs8.push(0x04);
    pkcs8.push(0x22); // Length: 34 bytes
                      // Inner OCTET STRING with seed
    pkcs8.push(0x04);
    pkcs8.push(0x20); // Length: 32 bytes
    pkcs8.extend_from_slice(seed);

    pkcs8
}

/// Test server with mocked JWKS and AC internal endpoints.
struct TestMeetingServer {
    addr: SocketAddr,
    _server_handle: JoinHandle<()>,
    _mock_server: MockServer,
    keypair: TestKeypair,
    pool: PgPool,
}

impl TestMeetingServer {
    async fn spawn(pool: PgPool) -> Result<Self> {
        // Create mock server for JWKS and AC internal endpoints
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

        // Set up AC internal meeting-token endpoint (default success)
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/internal/meeting-token"))
            .and(header("Authorization", "Bearer test-service-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSJ9.test-meeting-token",
                "expires_in": 900
            })))
            .mount(&mock_server)
            .await;

        // Set up AC internal guest-token endpoint (default success)
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/internal/guest-token"))
            .and(header("Authorization", "Bearer test-service-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSJ9.test-guest-token",
                "expires_in": 900
            })))
            .mount(&mock_server)
            .await;

        // Build configuration pointing to mock server
        let vars = HashMap::from([
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
            ("AC_INTERNAL_URL".to_string(), mock_server.uri()),
        ]);

        let config = Config::from_vars(&vars)
            .map_err(|e| anyhow::anyhow!("Failed to create config: {}", e))?;

        // Create application state with MockMcClient (tests production code path)
        let mock_mc_client = Arc::new(MockMcClient::accepting());
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            mc_client: mock_mc_client,
        });

        // Build routes
        let app = routes::build_routes(state);

        // Bind to random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to bind test server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| anyhow::anyhow!("Failed to get local address: {}", e))?;

        // Set the GC_SERVICE_TOKEN environment variable for the test
        std::env::set_var("GC_SERVICE_TOKEN", "test-service-token");

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
            _mock_server: mock_server,
            keypair,
            pool,
        })
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Create a valid token for a specific user ID.
    fn create_token_for_user(&self, user_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestClaims {
            sub: user_id.to_string(),
            exp: now + 3600, // 1 hour
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        self.keypair.sign_token(&claims)
    }

    /// Create an expired token.
    fn create_expired_token(&self) -> String {
        let now = Utc::now().timestamp();
        let claims = TestClaims {
            sub: Uuid::new_v4().to_string(),
            exp: now - 3600, // Expired 1 hour ago
            iat: now - 7200, // Issued 2 hours ago
            scope: "read write".to_string(),
            service_type: None,
        };
        self.keypair.sign_token(&claims)
    }

    /// Create a token with HS256 algorithm (algorithm confusion attack).
    fn create_hs256_token_for_user(&self, user_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestClaims {
            sub: user_id.to_string(),
            exp: now + 3600,
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        self.keypair.create_hs256_token(&claims)
    }

    /// Create a token signed with a different (wrong) key.
    fn create_token_with_wrong_key(&self, user_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestClaims {
            sub: user_id.to_string(),
            exp: now + 3600,
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        self.keypair.create_token_with_wrong_key(&claims)
    }

    /// Create a tampered token (payload modified after signing).
    fn create_tampered_token(&self, user_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestClaims {
            sub: user_id.to_string(),
            exp: now + 3600,
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        self.keypair.create_tampered_token(&claims)
    }
}

impl Drop for TestMeetingServer {
    fn drop(&mut self) {
        self._server_handle.abort();
    }
}

// ============================================================================
// Database Fixture Helpers
// ============================================================================

/// Create a test organization in the database.
async fn create_test_org(pool: &PgPool, subdomain: &str, display_name: &str) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO organizations (org_id, subdomain, display_name, plan_tier, is_active)
        VALUES ($1, $2, $3, 'pro', true)
        "#,
    )
    .bind(org_id)
    .bind(subdomain)
    .bind(display_name)
    .execute(pool)
    .await
    .expect("Failed to create test organization");

    org_id
}

/// Create a test user in the database.
async fn create_test_user(pool: &PgPool, org_id: Uuid, email: &str, display_name: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (user_id, org_id, email, password_hash, display_name, is_active)
        VALUES ($1, $2, $3, '$2b$12$test_hash_not_real', $4, true)
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(email)
    .bind(display_name)
    .execute(pool)
    .await
    .expect("Failed to create test user");

    user_id
}

/// Create an inactive test user in the database.
async fn create_inactive_test_user(
    pool: &PgPool,
    org_id: Uuid,
    email: &str,
    display_name: &str,
) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (user_id, org_id, email, password_hash, display_name, is_active)
        VALUES ($1, $2, $3, '$2b$12$test_hash_not_real', $4, false)
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(email)
    .bind(display_name)
    .execute(pool)
    .await
    .expect("Failed to create inactive test user");

    user_id
}

/// Create a test meeting in the database.
#[allow(clippy::too_many_arguments)]
async fn create_test_meeting(
    pool: &PgPool,
    org_id: Uuid,
    created_by_user_id: Uuid,
    meeting_code: &str,
    status: &str,
    allow_guests: bool,
    allow_external_participants: bool,
    waiting_room_enabled: bool,
) -> Uuid {
    let meeting_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO meetings (
            meeting_id, org_id, created_by_user_id, display_name, meeting_code,
            join_token_secret, status, allow_guests, allow_external_participants,
            waiting_room_enabled
        )
        VALUES ($1, $2, $3, 'Test Meeting', $4, 'test-secret', $5, $6, $7, $8)
        "#,
    )
    .bind(meeting_id)
    .bind(org_id)
    .bind(created_by_user_id)
    .bind(meeting_code)
    .bind(status)
    .bind(allow_guests)
    .bind(allow_external_participants)
    .bind(waiting_room_enabled)
    .execute(pool)
    .await
    .expect("Failed to create test meeting");

    meeting_id
}

/// Register a healthy Meeting Controller for testing.
///
/// MC assignment is required for meeting join operations. This helper
/// creates a healthy MC in the test-region that can handle assignments.
async fn register_healthy_mc_for_region(pool: &PgPool, region: &str) {
    let grpc_endpoint = format!("https://mc-test-{}.example.com:50051", region);
    sqlx::query(
        r#"
        INSERT INTO meeting_controllers (
            controller_id, region, endpoint, grpc_endpoint, webtransport_endpoint,
            max_meetings, max_participants, current_meetings, current_participants,
            health_status, last_heartbeat_at, created_at
        )
        VALUES ($1, $2, $3, $3, $4, 100, 1000, 0, 0, 'healthy', NOW(), NOW())
        ON CONFLICT (controller_id) DO UPDATE SET
            last_heartbeat_at = NOW(),
            health_status = 'healthy'
        "#,
    )
    .bind(format!("mc-test-{}", region))
    .bind(region)
    .bind(&grpc_endpoint)
    .bind(format!("https://mc-test-{}.example.com:443", region))
    .execute(pool)
    .await
    .expect("Failed to register healthy MC for testing");
}

/// Register healthy Media Handlers for testing.
///
/// MH assignment is required for meeting join operations with the new flow.
/// This helper creates two healthy MHs in the test-region (primary and backup).
async fn register_healthy_mhs_for_region(pool: &PgPool, region: &str) {
    // Register primary MH
    sqlx::query(
        r#"
        INSERT INTO media_handlers (
            handler_id, region, webtransport_endpoint, grpc_endpoint,
            max_streams, current_streams, health_status, last_heartbeat_at, registered_at
        )
        VALUES ($1, $2, $3, $4, 1000, 0, 'healthy', NOW(), NOW())
        ON CONFLICT (handler_id) DO UPDATE SET
            last_heartbeat_at = NOW(),
            health_status = 'healthy'
        "#,
    )
    .bind(format!("mh-primary-{}", region))
    .bind(region)
    .bind(format!("https://mh-primary-{}.example.com:443", region))
    .bind(format!("grpc://mh-primary-{}.example.com:50051", region))
    .execute(pool)
    .await
    .expect("Failed to register primary MH for testing");

    // Register backup MH
    sqlx::query(
        r#"
        INSERT INTO media_handlers (
            handler_id, region, webtransport_endpoint, grpc_endpoint,
            max_streams, current_streams, health_status, last_heartbeat_at, registered_at
        )
        VALUES ($1, $2, $3, $4, 1000, 0, 'healthy', NOW(), NOW())
        ON CONFLICT (handler_id) DO UPDATE SET
            last_heartbeat_at = NOW(),
            health_status = 'healthy'
        "#,
    )
    .bind(format!("mh-backup-{}", region))
    .bind(region)
    .bind(format!("https://mh-backup-{}.example.com:443", region))
    .bind(format!("grpc://mh-backup-{}.example.com:50051", region))
    .execute(pool)
    .await
    .expect("Failed to register backup MH for testing");
}

// ============================================================================
// Meeting Join Flow Tests - GET /v1/meetings/{code}
// ============================================================================

/// Test that valid authenticated user can join a meeting.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_authenticated_success(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region (required for meeting join)
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    // Create test fixtures
    let org_id = create_test_org(&server.pool, "test-org", "Test Organization").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "ABC123",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Create token for the user
    let token = server.create_token_for_user(user_id);

    let response = client
        .get(format!("{}/v1/meetings/ABC123", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 200, "Should return 200 OK");

    let body: serde_json::Value = response.json().await?;
    assert!(body["token"].is_string(), "Should return a meeting token");
    assert!(body["expires_in"].is_number(), "Should return expires_in");
    assert!(body["meeting_id"].is_string(), "Should return meeting_id");
    assert_eq!(body["meeting_name"], "Test Meeting");
    // Verify MC assignment is present
    assert!(
        body["mc_assignment"]["mc_id"].is_string(),
        "Should return mc_assignment with mc_id"
    );
    assert!(
        body["mc_assignment"]["grpc_endpoint"].is_string(),
        "Should return mc_assignment with grpc_endpoint"
    );

    Ok(())
}

/// Test that meeting not found returns 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_not_found(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Create test fixtures (no meeting with code "NOTFOUND")
    let org_id = create_test_org(&server.pool, "test-org", "Test Organization").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;

    let token = server.create_token_for_user(user_id);

    let response = client
        .get(format!("{}/v1/meetings/NOTFOUND", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 404, "Should return 404 Not Found");

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    Ok(())
}

/// Test that cancelled meeting returns 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_cancelled_returns_not_found(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "test-org", "Test Organization").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "CANCEL1",
        "cancelled", // Cancelled meeting
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(user_id);

    let response = client
        .get(format!("{}/v1/meetings/CANCEL1", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        404,
        "Cancelled meeting should return 404"
    );

    Ok(())
}

/// Test that ended meeting returns 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_ended_returns_not_found(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "test-org", "Test Organization").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "ENDED1",
        "ended", // Ended meeting
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(user_id);

    let response = client
        .get(format!("{}/v1/meetings/ENDED1", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 404, "Ended meeting should return 404");

    Ok(())
}

/// Test that cross-org user is denied when allow_external=false.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_cross_org_denied(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Create two organizations
    let org1_id = create_test_org(&server.pool, "org-one", "Organization One").await;
    let org2_id = create_test_org(&server.pool, "org-two", "Organization Two").await;

    // Host in org1
    let host_id = create_test_user(&server.pool, org1_id, "host@org1.com", "Host User").await;
    // User in org2 (external)
    let external_user_id =
        create_test_user(&server.pool, org2_id, "external@org2.com", "External User").await;

    // Meeting with allow_external_participants = false
    let _meeting_id = create_test_meeting(
        &server.pool,
        org1_id,
        host_id,
        "NOEXT1",
        "scheduled",
        false,
        false, // External not allowed
        true,
    )
    .await;

    // External user tries to join
    let token = server.create_token_for_user(external_user_id);

    let response = client
        .get(format!("{}/v1/meetings/NOEXT1", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        403,
        "External user should be denied when allow_external=false"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    Ok(())
}

/// Test that cross-org user can join when allow_external=true.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_cross_org_allowed(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    // Create two organizations
    let org1_id = create_test_org(&server.pool, "org-one-ext", "Organization One").await;
    let org2_id = create_test_org(&server.pool, "org-two-ext", "Organization Two").await;

    // Host in org1
    let host_id = create_test_user(&server.pool, org1_id, "host@org1.com", "Host User").await;
    // User in org2 (external)
    let external_user_id =
        create_test_user(&server.pool, org2_id, "external@org2.com", "External User").await;

    // Meeting with allow_external_participants = true
    let _meeting_id = create_test_meeting(
        &server.pool,
        org1_id,
        host_id,
        "EXT001",
        "scheduled",
        false,
        true, // External allowed
        true,
    )
    .await;

    // External user joins
    let token = server.create_token_for_user(external_user_id);

    let response = client
        .get(format!("{}/v1/meetings/EXT001", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        200,
        "External user should be allowed when allow_external=true"
    );

    Ok(())
}

/// Test that host joins own meeting successfully.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_host_success(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    let org_id = create_test_org(&server.pool, "host-org", "Host Organization").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Meeting Host").await;

    // Host's own meeting
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "HOST01",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(host_id);

    let response = client
        .get(format!("{}/v1/meetings/HOST01", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 200, "Host should join successfully");

    Ok(())
}

/// Test that non-host member joins as participant.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_non_host_member(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    let org_id = create_test_org(&server.pool, "member-org", "Member Organization").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Meeting Host").await;
    let member_id = create_test_user(&server.pool, org_id, "member@test.com", "Team Member").await;

    // Meeting created by host
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "MEMBER1",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Member joins
    let token = server.create_token_for_user(member_id);

    let response = client
        .get(format!("{}/v1/meetings/MEMBER1", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        200,
        "Same-org member should join successfully"
    );

    Ok(())
}

/// Test that missing auth returns 401.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_missing_auth(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "noauth-org", "No Auth Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "NOAUTH1",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // No Authorization header
    let response = client
        .get(format!("{}/v1/meetings/NOAUTH1", server.url()))
        .send()
        .await?;

    assert_eq!(response.status(), 401, "Missing auth should return 401");

    Ok(())
}

/// Test that invalid auth returns 401.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_invalid_auth(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "badauth-org", "Bad Auth Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "BADAUTH",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Expired token
    let token = server.create_expired_token();

    let response = client
        .get(format!("{}/v1/meetings/BADAUTH", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 401, "Expired token should return 401");

    Ok(())
}

// ============================================================================
// Guest Token Flow Tests - POST /v1/meetings/{code}/guest-token
// ============================================================================

/// Test that valid guest request returns token.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_success(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    let org_id = create_test_org(&server.pool, "guest-org", "Guest Organization").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "GUEST01",
        "scheduled",
        true, // Guests allowed
        false,
        true,
    )
    .await;

    let response = client
        .post(format!("{}/v1/meetings/GUEST01/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "John Guest",
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(response.status(), 200, "Guest token request should succeed");

    let body: serde_json::Value = response.json().await?;
    assert!(body["token"].is_string(), "Should return guest token");
    assert!(body["expires_in"].is_number(), "Should return expires_in");

    Ok(())
}

/// Test that guest token for non-existent meeting returns 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_meeting_not_found(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/v1/meetings/NOSUCH/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "John Guest",
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        404,
        "Non-existent meeting should return 404"
    );

    Ok(())
}

/// Test that guest token for meeting with allow_guests=false returns 403.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_guests_not_allowed(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "noguest-org", "No Guest Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "NOGUEST",
        "scheduled",
        false, // Guests NOT allowed
        false,
        true,
    )
    .await;

    let response = client
        .post(format!("{}/v1/meetings/NOGUEST/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "John Guest",
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        403,
        "Guests not allowed should return 403"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    Ok(())
}

/// Test that empty display_name returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_empty_display_name(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "empty-org", "Empty Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "EMPTY01",
        "scheduled",
        true,
        false,
        true,
    )
    .await;

    // Empty display_name
    let response = client
        .post(format!("{}/v1/meetings/EMPTY01/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "",
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        400,
        "Empty display name should return 400"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "BAD_REQUEST");

    Ok(())
}

/// Test that whitespace-only display_name returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_whitespace_display_name(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "ws-org", "Whitespace Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "WS0001",
        "scheduled",
        true,
        false,
        true,
    )
    .await;

    // Whitespace-only display_name
    let response = client
        .post(format!("{}/v1/meetings/WS0001/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "   ",
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        400,
        "Whitespace-only display name should return 400"
    );

    Ok(())
}

/// Test that display_name too short returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_short_display_name(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "short-org", "Short Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "SHORT1",
        "scheduled",
        true,
        false,
        true,
    )
    .await;

    // Single character display_name (min is 2)
    let response = client
        .post(format!("{}/v1/meetings/SHORT1/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "J",
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        400,
        "Too short display name should return 400"
    );

    Ok(())
}

/// Test that empty captcha_token returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_empty_captcha(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "captcha-org", "Captcha Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "CAPT01",
        "scheduled",
        true,
        false,
        true,
    )
    .await;

    // Empty captcha_token
    let response = client
        .post(format!("{}/v1/meetings/CAPT01/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "John Guest",
            "captcha_token": ""
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        400,
        "Empty captcha token should return 400"
    );

    Ok(())
}

/// Test that guest token for cancelled meeting returns 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_cancelled_meeting(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "cancel-org", "Cancel Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "GCAN01",
        "cancelled", // Cancelled
        true,
        false,
        true,
    )
    .await;

    let response = client
        .post(format!("{}/v1/meetings/GCAN01/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": "John Guest",
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        404,
        "Guest token for cancelled meeting should return 404"
    );

    Ok(())
}

// ============================================================================
// Update Meeting Settings Tests - PATCH /v1/meetings/{id}/settings
// ============================================================================

/// Test that host can update allow_guests setting.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_allow_guests(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org1", "Update Org 1").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD001",
        "scheduled",
        false, // Initially false
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(host_id);

    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "allow_guests": true
        }))
        .send()
        .await?;

    assert_eq!(response.status(), 200, "Host should update settings");

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["allow_guests"], true, "allow_guests should be updated");

    Ok(())
}

/// Test that host can update allow_external_participants setting.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_allow_external(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org2", "Update Org 2").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD002",
        "scheduled",
        false,
        false, // Initially false
        true,
    )
    .await;

    let token = server.create_token_for_user(host_id);

    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "allow_external_participants": true
        }))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["allow_external_participants"], true,
        "allow_external_participants should be updated"
    );

    Ok(())
}

/// Test that host can update waiting_room_enabled setting.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_waiting_room(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org3", "Update Org 3").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD003",
        "scheduled",
        false,
        false,
        true, // Initially true
    )
    .await;

    let token = server.create_token_for_user(host_id);

    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "waiting_room_enabled": false
        }))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["waiting_room_enabled"], false,
        "waiting_room_enabled should be updated"
    );

    Ok(())
}

/// Test that non-host user gets 403.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_non_host_forbidden(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org4", "Update Org 4").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let other_user_id =
        create_test_user(&server.pool, org_id, "other@test.com", "Other User").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD004",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Other user (not host) tries to update
    let token = server.create_token_for_user(other_user_id);

    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "allow_guests": true
        }))
        .send()
        .await?;

    assert_eq!(response.status(), 403, "Non-host should get 403 Forbidden");

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "FORBIDDEN");

    Ok(())
}

/// Test that updating non-existent meeting returns 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_meeting_not_found(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org5", "Update Org 5").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "User").await;

    let token = server.create_token_for_user(user_id);
    let non_existent_id = Uuid::new_v4();

    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            non_existent_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "allow_guests": true
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        404,
        "Non-existent meeting should return 404"
    );

    Ok(())
}

/// Test that empty update (no changes) returns 400.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_empty_update(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org6", "Update Org 6").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD006",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(host_id);

    // Empty update body (no fields set)
    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({}))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        400,
        "Empty update should return 400 Bad Request"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "BAD_REQUEST");

    Ok(())
}

/// Test that partial updates work correctly.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_partial_update(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org7", "Update Org 7").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD007",
        "scheduled",
        false, // allow_guests = false
        false, // allow_external = false
        true,  // waiting_room = true
    )
    .await;

    let token = server.create_token_for_user(host_id);

    // Only update allow_guests
    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "allow_guests": true
        }))
        .send()
        .await?;

    assert_eq!(response.status(), 200, "Partial update should succeed");

    let body: serde_json::Value = response.json().await?;
    // Updated field
    assert_eq!(body["allow_guests"], true);
    // Unchanged fields
    assert_eq!(
        body["allow_external_participants"], false,
        "Unchanged field should remain false"
    );
    assert_eq!(
        body["waiting_room_enabled"], true,
        "Unchanged field should remain true"
    );

    Ok(())
}

/// Test that multiple settings can be updated at once.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_multiple_fields(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org8", "Update Org 8").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD008",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(host_id);

    // Update all three settings
    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "allow_guests": true,
            "allow_external_participants": true,
            "waiting_room_enabled": false
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        200,
        "Multiple field update should succeed"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["allow_guests"], true);
    assert_eq!(body["allow_external_participants"], true);
    assert_eq!(body["waiting_room_enabled"], false);

    Ok(())
}

/// Test that update settings requires authentication.
#[sqlx::test(migrations = "../../migrations")]
async fn test_update_settings_requires_auth(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "upd-org9", "Update Org 9").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "UPD009",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // No authorization header
    let response = client
        .patch(format!(
            "{}/v1/meetings/{}/settings",
            server.url(),
            meeting_id
        ))
        .json(&serde_json::json!({
            "allow_guests": true
        }))
        .send()
        .await?;

    assert_eq!(response.status(), 401, "Missing auth should return 401");

    Ok(())
}

// ============================================================================
// Security Reviewer Findings - JWT Manipulation Tests (MAJOR)
// ============================================================================

/// Test that token with wrong algorithm (HS256 instead of EdDSA) returns 401.
///
/// This tests defense against algorithm confusion attacks where an attacker
/// tries to use the public key as an HMAC secret.
#[sqlx::test(migrations = "../../migrations")]
async fn test_jwt_wrong_algorithm_returns_401(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "jwt-alg-org", "JWT Algorithm Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "JWTALG",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Create token with HS256 algorithm (should be rejected)
    let token = server.create_hs256_token_for_user(user_id);

    let response = client
        .get(format!("{}/v1/meetings/JWTALG", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        401,
        "Token with wrong algorithm (HS256) should return 401"
    );

    Ok(())
}

/// Test that token signed with unknown/different key returns 401.
///
/// This tests defense against key substitution attacks where an attacker
/// creates a valid-looking token but signs it with their own key.
#[sqlx::test(migrations = "../../migrations")]
async fn test_jwt_wrong_key_returns_401(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "jwt-key-org", "JWT Key Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "JWTKEY",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Create token signed with a different key (same kid, wrong private key)
    let token = server.create_token_with_wrong_key(user_id);

    let response = client
        .get(format!("{}/v1/meetings/JWTKEY", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        401,
        "Token signed with unknown key should return 401"
    );

    Ok(())
}

/// Test that tampered token (payload modified after signing) returns 401.
///
/// This tests defense against token manipulation attacks where an attacker
/// modifies the payload (e.g., to escalate privileges) while keeping the
/// original signature.
#[sqlx::test(migrations = "../../migrations")]
async fn test_jwt_tampered_payload_returns_401(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "jwt-tamper-org", "JWT Tamper Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "JWTAMP",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Create tampered token (payload modified after signing)
    let token = server.create_tampered_token(user_id);

    let response = client
        .get(format!("{}/v1/meetings/JWTAMP", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        401,
        "Token with tampered payload should return 401"
    );

    Ok(())
}

/// Test that guest display_name exceeding 100 characters returns 400.
///
/// This tests boundary validation for the maximum display name length.
#[sqlx::test(migrations = "../../migrations")]
async fn test_guest_token_max_display_name_boundary(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "max-name-org", "Max Name Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "MAXNAM",
        "scheduled",
        true, // Guests allowed
        false,
        true,
    )
    .await;

    // Create display_name with 101 characters (exceeds max of 100)
    let long_name = "a".repeat(101);

    let response = client
        .post(format!("{}/v1/meetings/MAXNAM/guest-token", server.url()))
        .json(&serde_json::json!({
            "display_name": long_name,
            "captcha_token": "valid-captcha-token"
        }))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        400,
        "Display name > 100 chars should return 400"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "BAD_REQUEST");

    Ok(())
}

/// Test that concurrent guest token requests all succeed and don't cause issues.
///
/// This tests the CSPRNG-based guest ID generation under concurrent load
/// to ensure thread safety and no race conditions.
///
/// Note: Token uniqueness is verified through the generate_guest_id() unit tests.
/// This integration test verifies the endpoint handles concurrent load correctly.
#[sqlx::test(migrations = "../../migrations")]
async fn test_concurrent_guest_requests_succeed(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    let org_id = create_test_org(&server.pool, "conc-org", "Concurrent Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "CONCUR",
        "scheduled",
        true, // Guests allowed
        false,
        true,
    )
    .await;

    let num_requests = 20;
    let url = format!("{}/v1/meetings/CONCUR/guest-token", server.url());

    // Spawn concurrent guest token requests
    let mut handles = Vec::new();
    for i in 0..num_requests {
        let client = client.clone();
        let url = url.clone();
        let display_name = format!("Guest {}", i);

        handles.push(tokio::spawn(async move {
            client
                .post(&url)
                .json(&serde_json::json!({
                    "display_name": display_name,
                    "captcha_token": "valid-captcha-token"
                }))
                .send()
                .await
        }));
    }

    // Wait for all requests to complete
    let responses = join_all(handles).await;

    // Count successful responses
    let mut success_count = 0;

    for result in responses {
        let response = result
            .expect("Task should not panic")
            .expect("Request should succeed");

        if response.status() == 200 {
            success_count += 1;
            let body: serde_json::Value = response.json().await?;
            // Verify response structure is valid
            assert!(body["token"].is_string(), "Response should contain token");
            assert!(
                body["expires_in"].is_number(),
                "Response should contain expires_in"
            );
        }
    }

    // All requests should succeed under concurrent load
    assert_eq!(
        success_count, num_requests,
        "All {} concurrent requests should succeed (got {} successes)",
        num_requests, success_count
    );

    Ok(())
}

// ============================================================================
// Test Reviewer Findings - User Lookup Edge Cases (MINOR)
// ============================================================================

/// Test that JWT with valid user_id that doesn't exist in database returns 404.
///
/// This tests the user lookup path when the user_id from the token is valid
/// but the user doesn't exist in the database.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_user_not_found(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "nouser-org", "No User Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "NOUSER",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Create token with a user_id that doesn't exist in the database
    let non_existent_user_id = Uuid::new_v4();
    let token = server.create_token_for_user(non_existent_user_id);

    let response = client
        .get(format!("{}/v1/meetings/NOUSER", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        404,
        "User not found should return 404 (from get_user_org_id)"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    Ok(())
}

/// Test that inactive user attempting to join returns 404.
///
/// This tests that the is_active = true check in get_user_org_id correctly
/// excludes deactivated users.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_inactive_user_denied(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "inactive-org", "Inactive User Org").await;
    let host_id = create_test_user(&server.pool, org_id, "host@test.com", "Host").await;

    // Create an inactive user
    let inactive_user_id =
        create_inactive_test_user(&server.pool, org_id, "inactive@test.com", "Inactive User").await;

    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "INACTV",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Create valid token for the inactive user
    let token = server.create_token_for_user(inactive_user_id);

    let response = client
        .get(format!("{}/v1/meetings/INACTV", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        404,
        "Inactive user should return 404 (get_user_org_id checks is_active = true)"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "NOT_FOUND");

    Ok(())
}
