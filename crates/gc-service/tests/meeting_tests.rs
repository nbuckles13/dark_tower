//! Meeting integration tests for Global Controller.
//!
//! Tests the meeting join, guest token, and settings update endpoints:
//!
//! - `GET /api/v1/meetings/{code}` - Join meeting (authenticated)
//! - `POST /api/v1/meetings/{code}/guest-token` - Get guest token (public)
//! - `PATCH /api/v1/meetings/{id}/settings` - Update meeting settings (host only)
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
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use futures::future::join_all;
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
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ============================================================================
// Test Helpers
// ============================================================================

/// JWT Claims for service test tokens (used by /api/v1/me).
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestClaims {
    sub: String,
    exp: i64,
    iat: i64,
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_type: Option<String>,
}

/// JWT Claims for user test tokens (used by join/settings/create endpoints).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestUserClaims {
    sub: String,
    org_id: String,
    email: String,
    roles: Vec<String>,
    iat: i64,
    exp: i64,
    jti: String,
}

/// Test keypair for signing tokens.
struct TestKeypair {
    kid: String,
    public_key_bytes: Vec<u8>,
    private_key_pkcs8: Vec<u8>,
}

#[allow(dead_code)]
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

    fn sign_user_token(&self, claims: &TestUserClaims) -> String {
        let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.kid.clone());

        encode(&header, claims, &encoding_key).expect("Failed to sign user token")
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
    mock_server: MockServer,
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
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSJ9.test-meeting-token",
                "expires_in": 900
            })))
            .mount(&mock_server)
            .await;

        // Set up AC internal guest-token endpoint (default success)
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/internal/guest-token"))
            .and(header("Authorization", "Bearer test-token"))
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
            ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
            ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
        ]);

        let config = Config::from_vars(&vars)
            .map_err(|e| anyhow::anyhow!("Failed to create config: {}", e))?;

        // Create a mock TokenReceiver for testing
        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let token_receiver = TokenReceiver::from_watch_receiver(rx);

        // Create application state with MockMcClient (tests production code path)
        let mock_mc_client = Arc::new(MockMcClient::accepting());
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            mc_client: mock_mc_client,
            token_receiver,
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
            pool,
        })
    }

    /// Spawn a server variant where the AC meeting-token endpoint returns 500.
    ///
    /// JWKS still works (so user auth passes), but the subsequent AC call
    /// to get a meeting token fails, exercising the AC-down error path.
    async fn spawn_with_ac_failure(pool: PgPool) -> Result<Self> {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(1, "test-key-01");

        // JWKS endpoint (still works — auth must pass)
        let jwks_response = serde_json::json!({
            "keys": [keypair.jwk_json()]
        });
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        // AC meeting-token returns 500 (simulates AC down)
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/internal/meeting-token"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "internal server error"
            })))
            .mount(&mock_server)
            .await;

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
            ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
            ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
        ]);

        let config = Config::from_vars(&vars)
            .map_err(|e| anyhow::anyhow!("Failed to create config: {}", e))?;

        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let token_receiver = TokenReceiver::from_watch_receiver(rx);

        let mock_mc_client = Arc::new(MockMcClient::accepting());
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config: config.clone(),
            mc_client: mock_mc_client,
            token_receiver,
        });

        let metrics_handle = get_test_metrics_handle();
        let app = routes::build_routes(state, metrics_handle)
            .map_err(|e| anyhow::anyhow!("Failed to build routes: {}", e))?;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to bind test server: {}", e))?;

        let addr = listener
            .local_addr()
            .map_err(|e| anyhow::anyhow!("Failed to get local address: {}", e))?;

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
            pool,
        })
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Create a valid user token for a specific user ID and org ID.
    fn create_token_for_user(&self, user_id: Uuid, org_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: user_id.to_string(),
            org_id: org_id.to_string(),
            email: format!("{}@test.com", user_id),
            roles: vec!["user".to_string()],
            iat: now,
            exp: now + 3600, // 1 hour
            jti: Uuid::new_v4().to_string(),
        };
        self.keypair.sign_user_token(&claims)
    }

    /// Create an expired user token.
    fn create_expired_token(&self) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: Uuid::new_v4().to_string(),
            org_id: Uuid::new_v4().to_string(),
            email: "expired@test.com".to_string(),
            roles: vec!["user".to_string()],
            iat: now - 7200, // Issued 2 hours ago
            exp: now - 3600, // Expired 1 hour ago
            jti: Uuid::new_v4().to_string(),
        };
        self.keypair.sign_user_token(&claims)
    }

    /// Create a user token with HS256 algorithm (algorithm confusion attack).
    fn create_hs256_token_for_user(&self, user_id: Uuid, org_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: user_id.to_string(),
            org_id: org_id.to_string(),
            email: format!("{}@test.com", user_id),
            roles: vec!["user".to_string()],
            iat: now,
            exp: now + 3600,
            jti: Uuid::new_v4().to_string(),
        };
        // Use public key as HMAC secret (algorithm confusion attack)
        let encoding_key = EncodingKey::from_secret(&self.keypair.public_key_bytes);
        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.keypair.kid.clone());
        encode(&header, &claims, &encoding_key).expect("Failed to sign HS256 token")
    }

    /// Create a user token signed with a different (wrong) key.
    fn create_token_with_wrong_key(&self, user_id: Uuid, org_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: user_id.to_string(),
            org_id: org_id.to_string(),
            email: format!("{}@test.com", user_id),
            roles: vec!["user".to_string()],
            iat: now,
            exp: now + 3600,
            jti: Uuid::new_v4().to_string(),
        };
        // Sign with a different keypair (same kid, different private key)
        let wrong_keypair = TestKeypair::new(99, &self.keypair.kid);
        wrong_keypair.sign_user_token(&claims)
    }

    /// Create a service token (wrong claims shape for user auth).
    ///
    /// This token has valid EdDSA signature but contains service claims
    /// (scope, service_type) instead of user claims (org_id, roles, email, jti).
    /// The user auth middleware should reject it with 401.
    fn create_service_token(&self) -> String {
        let now = Utc::now().timestamp();
        let claims = TestClaims {
            sub: "gc-service".to_string(),
            exp: now + 3600,
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        self.keypair.sign_token(&claims)
    }

    /// Create a tampered user token (payload modified after signing).
    fn create_tampered_token(&self, user_id: Uuid, org_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: user_id.to_string(),
            org_id: org_id.to_string(),
            email: format!("{}@test.com", user_id),
            roles: vec!["user".to_string()],
            iat: now,
            exp: now + 3600,
            jti: Uuid::new_v4().to_string(),
        };
        // Sign the valid token first
        let valid_token = self.keypair.sign_user_token(&claims);
        let parts: Vec<&str> = valid_token.split('.').collect();
        let header = parts.first().expect("JWT missing header");
        let signature = parts.get(2).expect("JWT missing signature");

        // Modify claims to escalate privileges
        let mut modified_claims = claims;
        modified_claims.roles = vec!["admin".to_string(), "superuser".to_string()];
        let modified_payload =
            URL_SAFE_NO_PAD.encode(serde_json::to_string(&modified_claims).unwrap().as_bytes());

        // Reassemble with original signature (won't match)
        format!("{}.{}.{}", header, modified_payload, signature)
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
/// This helper creates two healthy MH peers in the test-region.
async fn register_healthy_mhs_for_region(pool: &PgPool, region: &str) {
    // Register first MH peer
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
    .bind(format!("mh-1-{}", region))
    .bind(region)
    .bind(format!("https://mh-1-{}.example.com:443", region))
    .bind(format!("grpc://mh-1-{}.example.com:50051", region))
    .execute(pool)
    .await
    .expect("Failed to register MH peer 1 for testing");

    // Register second MH peer
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
    .bind(format!("mh-2-{}", region))
    .bind(region)
    .bind(format!("https://mh-2-{}.example.com:443", region))
    .bind(format!("grpc://mh-2-{}.example.com:50051", region))
    .execute(pool)
    .await
    .expect("Failed to register MH peer 2 for testing");
}

// ============================================================================
// Meeting Join Flow Tests - GET /api/v1/meetings/{code}
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
    let token = server.create_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/ABC123", server.url()))
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

    let token = server.create_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/NOTFOUND", server.url()))
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

    let token = server.create_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/CANCEL1", server.url()))
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

    let token = server.create_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/ENDED1", server.url()))
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

    // External user tries to join (token carries org2_id)
    let token = server.create_token_for_user(external_user_id, org2_id);

    let response = client
        .get(format!("{}/api/v1/meetings/NOEXT1", server.url()))
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

    // External user joins (token carries org2_id)
    let token = server.create_token_for_user(external_user_id, org2_id);

    let response = client
        .get(format!("{}/api/v1/meetings/EXT001", server.url()))
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

    let token = server.create_token_for_user(host_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/HOST01", server.url()))
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
    let token = server.create_token_for_user(member_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/MEMBER1", server.url()))
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
        .get(format!("{}/api/v1/meetings/NOAUTH1", server.url()))
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
        .get(format!("{}/api/v1/meetings/BADAUTH", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 401, "Expired token should return 401");

    Ok(())
}

// ============================================================================
// Guest Token Flow Tests - POST /api/v1/meetings/{code}/guest-token
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
        .post(format!(
            "{}/api/v1/meetings/GUEST01/guest-token",
            server.url()
        ))
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
        .post(format!(
            "{}/api/v1/meetings/NOSUCH/guest-token",
            server.url()
        ))
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
        .post(format!(
            "{}/api/v1/meetings/NOGUEST/guest-token",
            server.url()
        ))
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
        .post(format!(
            "{}/api/v1/meetings/EMPTY01/guest-token",
            server.url()
        ))
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
        .post(format!(
            "{}/api/v1/meetings/WS0001/guest-token",
            server.url()
        ))
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
        .post(format!(
            "{}/api/v1/meetings/SHORT1/guest-token",
            server.url()
        ))
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
        .post(format!(
            "{}/api/v1/meetings/CAPT01/guest-token",
            server.url()
        ))
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
        .post(format!(
            "{}/api/v1/meetings/GCAN01/guest-token",
            server.url()
        ))
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
// Update Meeting Settings Tests - PATCH /api/v1/meetings/{id}/settings
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

    let token = server.create_token_for_user(host_id, org_id);

    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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

    let token = server.create_token_for_user(host_id, org_id);

    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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

    let token = server.create_token_for_user(host_id, org_id);

    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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
    let token = server.create_token_for_user(other_user_id, org_id);

    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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

    let token = server.create_token_for_user(user_id, org_id);
    let non_existent_id = Uuid::new_v4();

    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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

    let token = server.create_token_for_user(host_id, org_id);

    // Empty update body (no fields set)
    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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

    let token = server.create_token_for_user(host_id, org_id);

    // Only update allow_guests
    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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

    let token = server.create_token_for_user(host_id, org_id);

    // Update all three settings
    let response = client
        .patch(format!(
            "{}/api/v1/meetings/{}/settings",
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
            "{}/api/v1/meetings/{}/settings",
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
    let token = server.create_hs256_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/JWTALG", server.url()))
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
    let token = server.create_token_with_wrong_key(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/JWTKEY", server.url()))
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
    let token = server.create_tampered_token(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/JWTAMP", server.url()))
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
        .post(format!(
            "{}/api/v1/meetings/MAXNAM/guest-token",
            server.url()
        ))
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
    let url = format!("{}/api/v1/meetings/CONCUR/guest-token", server.url());

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
// Token-based Auth Tests (UserClaims)
// ============================================================================

/// Test that user with valid token but non-existent user_id can still join
/// (org_id comes from token, not DB lookup).
///
/// After migrating from service Claims to UserClaims, the join handler no
/// longer performs a get_user_org_id DB lookup — org_id is extracted directly
/// from the JWT. A non-existent user_id in the token is still a valid UUID
/// and the join proceeds based on the token's org_id claim.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_non_existent_user_succeeds_with_valid_token(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

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

    // Token with valid org_id but non-existent user_id — should succeed
    // because org_id comes from the token, not a DB lookup
    let non_existent_user_id = Uuid::new_v4();
    let token = server.create_token_for_user(non_existent_user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/NOUSER", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        200,
        "Non-existent user with valid org_id token should join (no DB user lookup)"
    );

    Ok(())
}

// ============================================================================
// R-18 Join Integration Tests (Task 14)
// ============================================================================

/// Test that a properly signed service token (wrong claims shape) is rejected
/// by the user auth middleware on the join endpoint.
///
/// The token has a valid EdDSA signature but contains service claims
/// (scope, service_type) instead of user claims (org_id, roles, email, jti).
/// The require_user_auth middleware should reject it with 401.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_service_token_rejected(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    let org_id = create_test_org(&server.pool, "svctoken-org", "Service Token Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "SVCTOK",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Service token: valid signature, but wrong claims shape for user auth
    let token = server.create_service_token();

    let response = client
        .get(format!("{}/api/v1/meetings/SVCTOK", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        401,
        "Service token (wrong claims shape) should return 401 on user-auth endpoint"
    );

    Ok(())
}

/// Test that join fails with 503 when AC is unavailable (cannot issue meeting token).
///
/// Auth passes (JWKS works), MC assignment succeeds, but the AC meeting-token
/// endpoint returns 500 — GC should map this to 503 Service Unavailable.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_ac_unavailable(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn_with_ac_failure(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs (so assignment succeeds before AC call)
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    let org_id = create_test_org(&server.pool, "acdown-org", "AC Down Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "ACDOWN",
        "active",
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/ACDOWN", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        503,
        "AC unavailable should return 503 Service Unavailable"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");

    Ok(())
}

/// Test that join fails with 503 when no Meeting Controllers are available.
///
/// Auth passes, meeting is found, but MC assignment fails because no healthy
/// MCs are registered in the database for the test region.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_no_mc_available(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Deliberately do NOT register any MCs or MHs
    let org_id = create_test_org(&server.pool, "nomc-org", "No MC Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "NOMCAV",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/NOMCAV", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        503,
        "No available MCs should return 503 Service Unavailable"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");

    Ok(())
}

/// Test that joining an active meeting succeeds with full response validation.
///
/// Verifies the complete success path: user joins an active (not just scheduled)
/// meeting and receives meeting token, MC assignment with webtransport_endpoint.
#[sqlx::test(migrations = "../../migrations")]
async fn test_join_meeting_active_status_success(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    let org_id = create_test_org(&server.pool, "active-org", "Active Meeting Org").await;
    let user_id = create_test_user(&server.pool, org_id, "user@test.com", "Test User").await;
    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        user_id,
        "ACTIV1",
        "active", // Active status (not scheduled)
        false,
        false,
        true,
    )
    .await;

    let token = server.create_token_for_user(user_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/ACTIV1", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 200, "Active meeting should be joinable");

    let body: serde_json::Value = response.json().await?;
    assert!(body["token"].is_string(), "Should return a meeting token");
    assert!(body["expires_in"].is_number(), "Should return expires_in");
    assert!(body["meeting_id"].is_string(), "Should return meeting_id");
    assert_eq!(body["meeting_name"], "Test Meeting");
    // Verify MC assignment fields including webtransport_endpoint
    assert!(
        body["mc_assignment"]["mc_id"].is_string(),
        "Should return mc_assignment with mc_id"
    );
    assert!(
        body["mc_assignment"]["grpc_endpoint"].is_string(),
        "Should return mc_assignment with grpc_endpoint"
    );
    assert!(
        body["mc_assignment"]["webtransport_endpoint"].is_string(),
        "Should return mc_assignment with webtransport_endpoint"
    );

    Ok(())
}

// ============================================================================
// Regression Tests — home_org_id same-org invariant (Bug Fix)
// ============================================================================

/// Regression test: same-org member join must send home_org_id equal to user_org_id.
///
/// Before the fix, GC sent `home_org_id: null` for same-org users (the field was
/// `Option<Uuid>` with `skip_serializing_if`), causing AC deserialization failure.
/// Now `home_org_id` is always `Uuid` and set to the user's org_id.
#[sqlx::test(migrations = "../../migrations")]
async fn test_same_org_join_sends_home_org_id_equal_to_user_org_id(pool: PgPool) -> Result<()> {
    let server = TestMeetingServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Register healthy MC and MHs for the test region
    register_healthy_mc_for_region(&server.pool, "test-region").await;
    register_healthy_mhs_for_region(&server.pool, "test-region").await;

    // Create org and user — both in the SAME org as the meeting
    let org_id = create_test_org(&server.pool, "same-org-test", "Same Org Test").await;
    let host_id = create_test_user(&server.pool, org_id, "host@sameorg.com", "Host User").await;
    let member_id = create_test_user(
        &server.pool,
        org_id,
        "member@sameorg.com",
        "Same Org Member",
    )
    .await;

    let _meeting_id = create_test_meeting(
        &server.pool,
        org_id,
        host_id,
        "HOMEORG1",
        "scheduled",
        false,
        false,
        true,
    )
    .await;

    // Same-org member joins
    let token = server.create_token_for_user(member_id, org_id);

    let response = client
        .get(format!("{}/api/v1/meetings/HOMEORG1", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(response.status(), 200, "Same-org join should succeed");

    // Inspect the request that GC sent to AC's meeting-token endpoint
    let received = server
        .mock_server
        .received_requests()
        .await
        .expect("Request recording should be enabled");

    let token_requests: Vec<_> = received
        .iter()
        .filter(|r| r.url.path() == "/api/v1/auth/internal/meeting-token")
        .collect();

    assert_eq!(
        token_requests.len(),
        1,
        "Should have sent exactly one meeting-token request to AC"
    );

    let body: serde_json::Value =
        serde_json::from_slice(&token_requests[0].body).expect("Request body should be valid JSON");

    // The core invariant: home_org_id must be present and equal to the user's org_id
    let home_org_id = body["home_org_id"]
        .as_str()
        .expect("home_org_id must be present in request body (not null/missing)");

    assert_eq!(
        home_org_id,
        org_id.to_string(),
        "home_org_id must equal user's org_id for same-org joins"
    );

    // Also verify meeting_org_id matches (same-org scenario)
    let meeting_org_id = body["meeting_org_id"]
        .as_str()
        .expect("meeting_org_id must be present");

    assert_eq!(
        home_org_id, meeting_org_id,
        "For same-org joins, home_org_id must equal meeting_org_id"
    );

    Ok(())
}
