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

#[path = "common/mod.rs"]
mod test_common;
use test_common::jwt_fixtures::{TestKeypair, TestServiceClaims, TestUserClaims};

use anyhow::Result;
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
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// `TestUserClaims`, `TestServiceClaims`, `TestKeypair`, and
// `build_pkcs8_from_seed` are imported from `tests/common/jwt_fixtures.rs`
// (consolidated under ADR-0032 Step 5 — see module doc-comment).

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
        let app = routes::build_routes(state, metrics_handle)
            .map_err(|e| anyhow::anyhow!("Failed to build routes: {}", e))?;

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
            "displayName": "Team Standup"
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 201, "Expected 201 Created");

    let body: serde_json::Value = resp.json().await?;
    assert!(body["meetingId"].is_string(), "Should have meetingId");
    assert!(body["meetingCode"].is_string(), "Should have meetingCode");
    assert_eq!(body["displayName"], "Team Standup");
    assert_eq!(body["status"], "scheduled");
    assert_eq!(body["maxParticipants"], 100); // Default
    assert_eq!(body["enableE2eEncryption"], true); // Secure default
    assert_eq!(body["requireAuth"], true); // Secure default
    assert_eq!(body["recordingEnabled"], false); // Secure default
    assert_eq!(body["allowGuests"], false); // Secure default
    assert_eq!(body["allowExternalParticipants"], false); // Secure default
    assert_eq!(body["waitingRoomEnabled"], true); // Secure default
    assert!(body["createdAt"].is_string(), "Should have createdAt");

    // Meeting code format: 12 base62 chars
    let code = body["meetingCode"].as_str().unwrap();
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
        .json(&serde_json::json!({"displayName": "No Auth"}))
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
        .json(&serde_json::json!({"displayName": "Expired"}))
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
        .json(&serde_json::json!({"displayName": "Wrong Token Type"}))
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
        .json(&serde_json::json!({"displayName": "No Permission"}))
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
            "displayName": "Test",
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
        .json(&serde_json::json!({"displayName": "Over Limit"}))
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
        .json(&serde_json::json!({"displayName": "Secret Test"}))
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
            "displayName": "  Persisted Meeting  ",
            "maxParticipants": 25,
            "enableE2eEncryption": false,
            "allowGuests": true
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await?;
    let meeting_id: Uuid = body["meetingId"].as_str().unwrap().parse().unwrap();

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
        .json(&serde_json::json!({"displayName": "Audit Test"}))
        .send()
        .await?;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await?;
    let meeting_id: Uuid = body["meetingId"].as_str().unwrap().parse().unwrap();

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

    // maxParticipants = 1 should fail
    let resp = client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "displayName": "Too Few",
            "maxParticipants": 1
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
            "displayName": "Custom Settings",
            "maxParticipants": 50,
            "enableE2eEncryption": false,
            "requireAuth": false,
            "recordingEnabled": true,
            "allowGuests": true,
            "allowExternalParticipants": true,
            "waitingRoomEnabled": false
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = resp.json().await?;
    assert_eq!(body["displayName"], "Custom Settings");
    assert_eq!(body["maxParticipants"], 50);
    assert_eq!(body["enableE2eEncryption"], false);
    assert_eq!(body["requireAuth"], false);
    assert_eq!(body["recordingEnabled"], true);
    assert_eq!(body["allowGuests"], true);
    assert_eq!(body["allowExternalParticipants"], true);
    assert_eq!(body["waitingRoomEnabled"], false);

    Ok(())
}

/// WIRE-SHAPE GOLDEN LOCK at the HTTP-integration level (R-53 / task #23).
///
/// Companion to the struct-level locks in `models/mod.rs`. Exists because the
/// R-53 camelCase migration broke the meeting flow at the HTTP-INTEGRATION level
/// specifically — task #23's in-clone verification never ran these DB-gated
/// `#[sqlx::test]` tests, so the struct-level serde tests passed while these
/// tests sent stale snake_case keys (the false-CLEAR mechanism the GC mirror of
/// task #46 fixes). This lock asserts the EXACT real-wire round-trip (camelCase
/// request accepted with 201 + full response key set) so a future rename sweep
/// that misses an HTTP surface fails loudly HERE, in the scope that actually
/// broke. `create` is GC's analogue of AC's `register` lock (`265e56e`): the one
/// round-trip that must EXCLUDE a credential (`join_token_secret`).
///
/// GC has NO OAuth/RFC-6749 endpoints, so the whole key set is camelCase (no
/// mixed scheme — contrast AC's register lock which keeps OAuth fields snake).
///
/// IF THIS FAILS DURING A RENAME SWEEP: the SDK + env-tests fixtures + every HTTP
/// client depend on these exact camelCase keys (R-53). DO NOT silently
/// re-baseline — confirm the wire contract and update the golden set AND the
/// struct-level lock AND the SDK/env-tests fixtures in lockstep. The
/// all-camelCase, no-mixed-scheme rule is owned by task #51.
#[sqlx::test(migrations = "../../migrations")]
async fn test_create_meeting_wire_shape_golden_lock(pool: PgPool) -> Result<()> {
    use std::collections::BTreeSet;

    // The COMPLETE, intended set of create-meeting-response wire keys — all
    // camelCase (R-53; GC has no OAuth carve-out). Update ONLY in lockstep with a
    // deliberate R-53 contract change (and the struct-level lock + task #51).
    let golden_response_keys: BTreeSet<String> = [
        "meetingId",                 // camelCase (R-53)
        "meetingCode",               // camelCase (R-53)
        "displayName",               // camelCase (R-53)
        "status",                    // scheme-invariant single word
        "maxParticipants",           // camelCase (R-53)
        "enableE2eEncryption",       // camelCase (R-53)
        "requireAuth",               // camelCase (R-53)
        "recordingEnabled",          // camelCase (R-53)
        "allowGuests",               // camelCase (R-53)
        "allowExternalParticipants", // camelCase (R-53)
        "waitingRoomEnabled",        // camelCase (R-53)
        "createdAt",                 // camelCase (R-53)
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let server = TestCreateMeetingServer::spawn(pool.clone()).await?;
    let org_id = create_test_org(&pool, "wirelock-org").await;
    let user_id = create_test_user(&pool, org_id, "user@wirelock.com").await;
    let token = server.create_user_token(user_id, org_id, vec!["user".to_string()]);

    // Request MUST be sent with the exact camelCase wire keys; a snake_case
    // `display_name` here would 400 (deny_unknown_fields) — this is the
    // regression this test guards.
    let resp = client_post_camel(&server, &token).await?;

    // Assert the request was ACCEPTED (201) BEFORE reading the body. A 400 from
    // deny_unknown_fields must FAIL the test, not silently pass — this is the
    // load-bearing anti-mask check.
    assert_eq!(
        resp.status(),
        201,
        "golden camelCase request body must be accepted with 201 Created (R-53 wire contract). \
         A 400 here means the request derive drifted from the camelCase contract — \
         DO NOT silently re-baseline; confirm the contract and update the SDK (R-53, task #51)."
    );

    let body: serde_json::Value = resp.json().await?;
    let actual_keys: BTreeSet<String> = body
        .as_object()
        .expect("create-meeting response must be a JSON object")
        .keys()
        .cloned()
        .collect();

    assert_eq!(
        actual_keys, golden_response_keys,
        "create-meeting response wire-key set drifted from the R-53 camelCase golden shape. \
         If intentional, update this golden set AND the struct-level lock in models/mod.rs \
         AND the SDK/env-tests fixtures (R-53). GC is all-camelCase, no mixed scheme."
    );

    // No serialized key may contain `_` — catches a PARTIAL rename leaving one
    // field snake_case (set-equality above already implies this, but asserting it
    // explicitly preserves the all-camelCase intent if anyone loosens the check).
    for key in &actual_keys {
        assert!(
            !key.contains('_'),
            "create-meeting response key `{key}` contains `_` — a snake_case field survived. \
             GC wire shape is ALL camelCase (R-53)."
        );
    }

    // Credential-non-leak guard at HTTP scope: the join secret must not appear in
    // EITHER scheme (mirrors the model-layer check at models/mod.rs).
    for forbidden in ["join_token_secret", "joinTokenSecret"] {
        assert!(
            !actual_keys.contains(forbidden),
            "create-meeting response must not contain credential key `{forbidden}` (no secret echo)"
        );
    }

    Ok(())
}

/// Helper for the golden-lock test: POST a meeting with the exact camelCase wire
/// body. Kept inline-simple (single call site) to mirror AC's 265e56e structure
/// without accreting a shared body builder.
async fn client_post_camel(
    server: &TestCreateMeetingServer,
    token: &str,
) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    Ok(client
        .post(format!("{}/api/v1/meetings", server.url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({ "displayName": "Wire Lock" }))
        .send()
        .await?)
}
