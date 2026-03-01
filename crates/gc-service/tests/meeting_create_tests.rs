//! Integration tests for POST /api/v1/meetings endpoint.
//!
//! Tests the meeting creation flow including:
//! - User JWT authentication (via require_user_auth middleware)
//! - Role enforcement
//! - Input validation
//! - Org meeting limit enforcement
//! - Response format (excludes join_token_secret)
//! - Audit log creation
//!
//! # Test Setup
//!
//! Tests use:
//! - wiremock to mock AC JWKS endpoint
//! - sqlx test macro for database setup with migrations
//! - Ed25519 keypair for signing test user tokens

#![allow(clippy::unwrap_used, clippy::expect_used)]

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
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ============================================================================
// Test Claims Types
// ============================================================================

/// User JWT Claims for test tokens (matches common::jwt::UserClaims).
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

/// Service JWT Claims (for testing wrong token type).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestServiceClaims {
    sub: String,
    exp: i64,
    iat: i64,
    scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_type: Option<String>,
}

// ============================================================================
// Test Keypair
// ============================================================================

struct TestKeypair {
    kid: String,
    public_key_bytes: Vec<u8>,
    private_key_pkcs8: Vec<u8>,
}

impl TestKeypair {
    fn new(seed: u8, kid: &str) -> Self {
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

    fn sign_user_token(&self, claims: &TestUserClaims) -> String {
        let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.kid.clone());

        encode(&header, claims, &encoding_key).expect("Failed to sign user token")
    }

    fn sign_service_token(&self, claims: &TestServiceClaims) -> String {
        let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.kid.clone());

        encode(&header, claims, &encoding_key).expect("Failed to sign service token")
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

fn build_pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
    let mut pkcs8 = Vec::new();
    pkcs8.push(0x30);
    pkcs8.push(0x2e);
    pkcs8.extend_from_slice(&[0x02, 0x01, 0x00]);
    pkcs8.push(0x30);
    pkcs8.push(0x05);
    pkcs8.extend_from_slice(&[0x06, 0x03, 0x2b, 0x65, 0x70]);
    pkcs8.push(0x04);
    pkcs8.push(0x22);
    pkcs8.push(0x04);
    pkcs8.push(0x20);
    pkcs8.extend_from_slice(seed);
    pkcs8
}

// ============================================================================
// Test Server
// ============================================================================

struct TestCreateMeetingServer {
    addr: SocketAddr,
    _server_handle: JoinHandle<()>,
    _mock_server: MockServer,
    keypair: TestKeypair,
}

impl TestCreateMeetingServer {
    async fn spawn(pool: PgPool) -> Result<Self> {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(42, "create-meeting-key-01");

        // JWKS endpoint
        let jwks_response = serde_json::json!({
            "keys": [keypair.jwk_json()]
        });
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
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
            config,
            mc_client: mock_mc_client,
            token_receiver,
        });

        let metrics_handle = get_test_metrics_handle();
        let app = routes::build_routes(state, metrics_handle);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

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
        })
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn create_user_token(&self, user_id: Uuid, org_id: Uuid, roles: Vec<String>) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: user_id.to_string(),
            org_id: org_id.to_string(),
            email: "test@example.com".to_string(),
            roles,
            iat: now,
            exp: now + 3600,
            jti: Uuid::new_v4().to_string(),
        };
        self.keypair.sign_user_token(&claims)
    }

    fn create_expired_user_token(&self, user_id: Uuid, org_id: Uuid) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: user_id.to_string(),
            org_id: org_id.to_string(),
            email: "test@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: now - 7200,
            exp: now - 3600, // Expired
            jti: Uuid::new_v4().to_string(),
        };
        self.keypair.sign_user_token(&claims)
    }

    fn create_service_token(&self) -> String {
        let now = Utc::now().timestamp();
        let claims = TestServiceClaims {
            sub: "gc-service".to_string(),
            exp: now + 3600,
            iat: now,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };
        self.keypair.sign_service_token(&claims)
    }
}

impl Drop for TestCreateMeetingServer {
    fn drop(&mut self) {
        self._server_handle.abort();
    }
}

// ============================================================================
// Database Fixtures
// ============================================================================

async fn create_test_org(pool: &PgPool, subdomain: &str) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO organizations (org_id, subdomain, display_name, plan_tier, is_active)
        VALUES ($1, $2, $3, 'pro', true)
        "#,
    )
    .bind(org_id)
    .bind(subdomain)
    .bind(format!("Test Org {}", subdomain))
    .execute(pool)
    .await
    .expect("Failed to create test organization");
    org_id
}

async fn create_test_org_with_limit(pool: &PgPool, subdomain: &str, max_meetings: i32) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO organizations (org_id, subdomain, display_name, plan_tier, max_concurrent_meetings, is_active)
        VALUES ($1, $2, $3, 'pro', $4, true)
        "#,
    )
    .bind(org_id)
    .bind(subdomain)
    .bind(format!("Test Org {}", subdomain))
    .bind(max_meetings)
    .execute(pool)
    .await
    .expect("Failed to create test organization");
    org_id
}

async fn create_test_user(pool: &PgPool, org_id: Uuid, email: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO users (user_id, org_id, email, password_hash, display_name, is_active)
        VALUES ($1, $2, $3, '$2b$12$test_hash_not_real', 'Test User', true)
        "#,
    )
    .bind(user_id)
    .bind(org_id)
    .bind(email)
    .execute(pool)
    .await
    .expect("Failed to create test user");
    user_id
}

async fn create_test_meeting_directly(pool: &PgPool, org_id: Uuid, user_id: Uuid, code: &str) {
    sqlx::query(
        r#"
        INSERT INTO meetings (org_id, created_by_user_id, display_name, meeting_code, join_token_secret, status)
        VALUES ($1, $2, 'Pre-existing Meeting', $3, 'secret-hex', 'scheduled')
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(code)
    .execute(pool)
    .await
    .expect("Failed to create test meeting");
}

// ============================================================================
// Integration Tests
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_happy_path(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "happy-org").await;
    let user_id = create_test_user(&pool, org_id, "user@happy.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "display_name": "Team Standup"
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 201, "Expected 201 Created");

    let body: serde_json::Value = resp.json().await?;
    assert!(body["meeting_id"].is_string(), "Should have meeting_id");
    assert!(body["meeting_code"].is_string(), "Should have meeting_code");
    assert_eq!(body["display_name"], "Team Standup");
    assert_eq!(body["status"], "scheduled");
    assert_eq!(body["max_participants"], 100); // Default
    assert_eq!(body["enable_e2e_encryption"], true); // Secure default
    assert_eq!(body["require_auth"], true); // Secure default
    assert_eq!(body["recording_enabled"], false); // Secure default
    assert_eq!(body["allow_guests"], false); // Secure default
    assert_eq!(body["allow_external_participants"], false); // Secure default
    assert_eq!(body["waiting_room_enabled"], true); // Secure default
    assert!(body["created_at"].is_string(), "Should have created_at");

    // Meeting code format: 12 base62 chars
    let code = body["meeting_code"].as_str().unwrap();
    assert_eq!(code.len(), 12);
    for ch in code.chars() {
        assert!(ch.is_ascii_alphanumeric());
    }

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_missing_auth_token(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .json(&serde_json::json!({"display_name": "No Auth"}))
        .send()
        .await?;

    assert_eq!(resp.status(), 401);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_expired_token(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "expired-org").await;
    let user_id = create_test_user(&pool, org_id, "user@expired.com").await;
    let token = server.create_expired_user_token(user_id, org_id);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"display_name": "Expired"}))
        .send()
        .await?;

    assert_eq!(resp.status(), 401);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_service_token_rejected(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let token = server.create_service_token();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"display_name": "Wrong Token Type"}))
        .send()
        .await?;

    // Service token has 'scope' but no 'org_id'/'roles'/'email'/'jti'
    // -> decode::<UserClaims>() fails -> 401
    assert_eq!(resp.status(), 401);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_insufficient_role(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "role-org").await;
    let user_id = create_test_user(&pool, org_id, "user@role.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["viewer".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"display_name": "No Permission"}))
        .send()
        .await?;

    assert_eq!(resp.status(), 403);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_missing_display_name(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "badreq-org").await;
    let user_id = create_test_user(&pool, org_id, "user@badreq.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({}))
        .send()
        .await?;

    assert_eq!(resp.status(), 400);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_unknown_field_rejected(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "unknown-org").await;
    let user_id = create_test_user(&pool, org_id, "user@unknown.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "display_name": "Test",
            "unknown_field": "should_be_rejected"
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 400);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_org_limit_exceeded(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    // Org with limit of 2 concurrent meetings
    let org_id = create_test_org_with_limit(&pool, "limit-org", 2).await;
    let user_id = create_test_user(&pool, org_id, "user@limit.com").await;

    // Create 2 existing meetings (at the limit)
    create_test_meeting_directly(&pool, org_id, user_id, "EXIST001AAAA").await;
    create_test_meeting_directly(&pool, org_id, user_id, "EXIST002BBBB").await;

    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"display_name": "Over Limit"}))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        403,
        "Should return 403 when org limit exceeded"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_response_excludes_join_token_secret(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "nosecret-org").await;
    let user_id = create_test_user(&pool, org_id, "user@nosecret.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"display_name": "Secret Test"}))
        .send()
        .await?;

    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = resp.json().await?;
    assert!(
        body.get("join_token_secret").is_none(),
        "Response must NOT contain join_token_secret"
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_db_persistence(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "persist-org").await;
    let user_id = create_test_user(&pool, org_id, "user@persist.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["admin".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "display_name": "  Persisted Meeting  ",
            "max_participants": 25,
            "enable_e2e_encryption": false,
            "allow_guests": true
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await?;
    let meeting_id: Uuid = body["meeting_id"].as_str().unwrap().parse().unwrap();

    // Verify DB row
    let row = sqlx::query_as::<_, (String, String, i32, bool, bool, String, Uuid, Uuid)>(
        r#"
        SELECT display_name, meeting_code, max_participants, enable_e2e_encryption,
               allow_guests, join_token_secret, org_id, created_by_user_id
        FROM meetings WHERE meeting_id = $1
        "#,
    )
    .bind(meeting_id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(row.0, "Persisted Meeting", "display_name should be trimmed");
    assert_eq!(row.1.len(), 12, "meeting_code should be 12 chars");
    assert_eq!(row.2, 25, "max_participants should be 25");
    assert!(!row.3, "enable_e2e_encryption should be false");
    assert!(row.4, "allow_guests should be true");
    assert_eq!(row.5.len(), 64, "join_token_secret should be 64 hex chars");
    assert_eq!(row.6, org_id, "org_id should match");
    assert_eq!(row.7, user_id, "created_by_user_id should match");

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_audit_log_created(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "audit-org").await;
    let user_id = create_test_user(&pool, org_id, "user@audit.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({"display_name": "Audit Test"}))
        .send()
        .await?;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await?;
    let meeting_id: Uuid = body["meeting_id"].as_str().unwrap().parse().unwrap();

    // Verify audit log entry
    let audit_count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*) FROM audit_logs
        WHERE org_id = $1 AND user_id = $2 AND resource_id = $3 AND action = 'meeting_created'
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(meeting_id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(audit_count.0, 1, "Should have exactly one audit log entry");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_max_participants_too_low(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "minpart-org").await;
    let user_id = create_test_user(&pool, org_id, "user@minpart.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    let client = reqwest::Client::new();

    // max_participants = 1 should fail
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "display_name": "Too Few",
            "max_participants": 1
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 400);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_with_custom_settings(pool: PgPool) -> Result<()> {
    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "custom-org").await;
    let user_id = create_test_user(&pool, org_id, "user@custom.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["org_admin".to_string()]);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "display_name": "Custom Settings",
            "max_participants": 50,
            "enable_e2e_encryption": false,
            "require_auth": false,
            "recording_enabled": true,
            "allow_guests": true,
            "allow_external_participants": true,
            "waiting_room_enabled": false
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["display_name"], "Custom Settings");
    assert_eq!(body["max_participants"], 50);
    assert_eq!(body["enable_e2e_encryption"], false);
    assert_eq!(body["require_auth"], false);
    assert_eq!(body["recording_enabled"], true);
    assert_eq!(body["allow_guests"], true);
    assert_eq!(body["allow_external_participants"], true);
    assert_eq!(body["waiting_room_enabled"], false);

    Ok(())
}
