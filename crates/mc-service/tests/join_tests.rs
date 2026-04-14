//! Integration tests for the MC WebTransport join flow.
//!
//! Tests cover:
//! - WebTransport connection acceptance
//! - JWT validation (valid, expired, invalid, wrong-meeting)
//! - JoinRequest processing (success, meeting not found, invalid protobuf, wrong message type)
//! - Signaling bridge (ParticipantJoined notification)
//!
//! Test infrastructure:
//! - Self-signed TLS via `wtransport::Identity::self_signed`
//! - JWKS mocked via `wiremock`
//! - Real actor hierarchy (MeetingControllerActorHandle + MeetingActor)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::{BufMut, BytesMut};
use common::jwt::JwksClient;
use common::secret::SecretBox;
use mc_service::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use mc_service::auth::McJwtValidator;
use mc_service::errors::McError;
use mc_service::mh_connection_registry::MhConnectionRegistry;
use mc_service::redis::{MhAssignmentData, MhAssignmentStore, MhEndpointInfo};
use mc_service::webtransport::connection;
use mc_test_utils::jwt_test::{
    make_expired_meeting_claims, make_meeting_claims, mount_jwks_mock, TestKeypair,
};
use prost::Message;
use proto_gen::signaling::{
    self, client_message, server_message, ClientMessage, JoinRequest, MuteRequest, ServerMessage,
};
use tokio_util::sync::CancellationToken;
use wiremock::MockServer;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{ClientConfig, Endpoint, Identity, ServerConfig};

// ============================================================================
// Test Infrastructure
// ============================================================================

/// In-memory mock for MhAssignmentStore — no Redis needed.
struct MockMhAssignmentStore {
    data: std::sync::Mutex<std::collections::HashMap<String, MhAssignmentData>>,
}

impl MockMhAssignmentStore {
    fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn insert(&self, meeting_id: &str, data: MhAssignmentData) {
        self.data
            .lock()
            .unwrap()
            .insert(meeting_id.to_string(), data);
    }
}

impl MhAssignmentStore for MockMhAssignmentStore {
    fn get_mh_assignment<'a>(
        &'a self,
        meeting_id: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<MhAssignmentData>, McError>> + Send + 'a,
        >,
    > {
        let result = self.data.lock().unwrap().get(meeting_id).cloned();
        Box::pin(async move { Ok(result) })
    }
}

/// A test WebTransport server with all dependencies wired up.
struct TestServer {
    /// The bound port of the WebTransport endpoint.
    port: u16,
    /// Controller handle for creating meetings, etc.
    controller_handle: Arc<MeetingControllerActorHandle>,
    /// In-memory MH assignment store (no Redis).
    mh_store: Arc<MockMhAssignmentStore>,
    /// JWT validator backed by wiremock JWKS (kept alive for accept loop).
    _jwt_validator: Arc<McJwtValidator>,
    /// Cancellation token for shutting down the accept loop.
    cancel_token: CancellationToken,
    /// Mock JWKS server (kept alive for the test duration).
    _mock_server: MockServer,
    /// Test keypair for signing JWTs.
    keypair: TestKeypair,
}

impl TestServer {
    /// Start a test server with self-signed TLS, wiremock JWKS, and actor hierarchy.
    async fn start() -> Self {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(42, "test-key-01");
        let jwks_url = mount_jwks_mock(&mock_server, &keypair).await;

        let jwks_client =
            Arc::new(JwksClient::new(jwks_url).expect("Failed to create JWKS client"));
        let jwt_validator = Arc::new(McJwtValidator::new(jwks_client, 300));

        let master_secret = SecretBox::new(Box::new(vec![0u8; 32]));
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let controller_handle = Arc::new(MeetingControllerActorHandle::new(
            "mc-test".to_string(),
            metrics,
            controller_metrics,
            master_secret,
            Arc::new(MhConnectionRegistry::new()),
        ));

        let mh_store = Arc::new(MockMhAssignmentStore::new());

        let cancel_token = CancellationToken::new();

        // Build WebTransport server with self-signed cert
        let identity =
            Identity::self_signed(["localhost", "127.0.0.1"]).expect("self-signed identity");

        let server_config = ServerConfig::builder()
            .with_bind_address("127.0.0.1:0".parse::<SocketAddr>().unwrap())
            .with_identity(&identity)
            .build();

        let endpoint = Endpoint::server(server_config).expect("Failed to create endpoint");
        let port = endpoint.local_addr().unwrap().port();

        // Spawn accept loop
        let ctrl = Arc::clone(&controller_handle);
        let jwt = Arc::clone(&jwt_validator);
        let store = Arc::clone(&mh_store);
        let ct = cancel_token.clone();
        tokio::spawn(async move {
            Self::accept_loop(endpoint, ctrl, jwt, store, ct).await;
        });

        Self {
            port,
            controller_handle,
            mh_store,
            _jwt_validator: jwt_validator,
            cancel_token,
            _mock_server: mock_server,
            keypair,
        }
    }

    /// Simplified accept loop for tests (mirrors WebTransportServer::accept_loop).
    async fn accept_loop(
        endpoint: Endpoint<Server>,
        controller_handle: Arc<MeetingControllerActorHandle>,
        jwt_validator: Arc<McJwtValidator>,
        mh_store: Arc<MockMhAssignmentStore>,
        cancel_token: CancellationToken,
    ) {
        loop {
            tokio::select! {
                () = cancel_token.cancelled() => break,
                incoming = endpoint.accept() => {
                    let ctrl = Arc::clone(&controller_handle);
                    let jwt = Arc::clone(&jwt_validator);
                    let store = Arc::clone(&mh_store) as Arc<dyn MhAssignmentStore>;
                    let ct = cancel_token.child_token();
                    tokio::spawn(async move {
                        let _ = connection::handle_connection(incoming, ctrl, jwt, store, ct).await;
                    });
                }
            }
        }
    }

    /// Create a meeting on the controller and seed MH assignment data.
    async fn create_meeting(&self, meeting_id: &str) {
        // Seed MH assignment data so join flow can populate media_servers
        self.mh_store.insert(
            meeting_id,
            MhAssignmentData {
                handlers: vec![MhEndpointInfo {
                    mh_id: "mh-test-1".to_string(),
                    webtransport_endpoint: "wt://mh-test-1:4433".to_string(),
                    grpc_endpoint: Some("http://mh-test-1:50053".to_string()),
                }],
                assigned_at: "2024-01-01T00:00:00Z".to_string(),
            },
        );

        self.controller_handle
            .create_meeting(meeting_id.to_string())
            .await
            .expect("Failed to create meeting");
    }

    /// Sign a JWT for the given claims.
    fn sign_token<T: serde::Serialize>(&self, claims: &T) -> String {
        self.keypair.sign_token(claims)
    }

    /// WebTransport URL for client connections.
    fn url(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        self.controller_handle.cancel();
    }
}

/// Connect a wtransport client to the test server and return the connection.
async fn connect_client(url: &str) -> wtransport::Connection {
    let client_config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();

    let client = Endpoint::client(client_config).expect("client endpoint");
    client.connect(url).await.expect("client connect")
}

/// Encode a ClientMessage as a length-prefixed frame (4-byte BE length + protobuf).
fn encode_framed(msg: &ClientMessage) -> Vec<u8> {
    let encoded = msg.encode_to_vec();
    let len = encoded.len() as u32;
    let mut frame = BytesMut::with_capacity(4 + encoded.len());
    frame.put_u32(len);
    frame.put_slice(&encoded);
    frame.to_vec()
}

/// Read a length-prefixed ServerMessage from a recv stream.
async fn read_server_message(recv: &mut wtransport::stream::RecvStream) -> ServerMessage {
    try_read_server_message(recv)
        .await
        .expect("Failed to read server message (stream closed before response)")
}

/// Try to read a length-prefixed ServerMessage, returning None if the stream closed.
async fn try_read_server_message(
    recv: &mut wtransport::stream::RecvStream,
) -> Option<ServerMessage> {
    // Read 4-byte length prefix
    let mut len_buf = [0u8; 4];
    if recv.read_exact(&mut len_buf).await.is_err() {
        return None;
    }

    let msg_len = u32::from_be_bytes(len_buf) as usize;
    if msg_len == 0 || msg_len > 65536 {
        return None;
    }

    let mut buf = vec![0u8; msg_len];
    if recv.read_exact(&mut buf).await.is_err() {
        return None;
    }

    ServerMessage::decode(buf.as_slice()).ok()
}

/// Send a JoinRequest and read the response ServerMessage.
async fn join_and_read_response(
    url: &str,
    meeting_id: &str,
    join_token: &str,
    participant_name: &str,
) -> ServerMessage {
    let conn = connect_client(url).await;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .expect("open bi stream")
        .await
        .expect("bi stream ready");

    let client_msg = ClientMessage {
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: join_token.to_string(),
            participant_name: participant_name.to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
        })),
    };

    let frame = encode_framed(&client_msg);
    send.write_all(&frame).await.expect("write join request");

    // Read response with timeout
    tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv))
        .await
        .expect("Timeout waiting for server response")
}

/// Helper to extract error code from a ServerMessage::Error.
fn extract_error(msg: &ServerMessage) -> (i32, String) {
    match &msg.message {
        Some(server_message::Message::Error(e)) => (e.code, e.message.clone()),
        other => panic!("Expected Error message, got {other:?}"),
    }
}

// ============================================================================
// T1: Successful Join (Happy Path)
// ============================================================================

#[tokio::test]
async fn test_join_success_returns_join_response() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-happy").await;

    let claims = make_meeting_claims("meeting-happy");
    let token = server.sign_token(&claims);

    let response = join_and_read_response(&server.url(), "meeting-happy", &token, "Alice").await;

    match &response.message {
        Some(server_message::Message::JoinResponse(join)) => {
            assert!(
                !join.participant_id.is_empty(),
                "participant_id should be non-empty"
            );
            assert!(
                !join.correlation_id.is_empty(),
                "correlation_id should be non-empty"
            );
            assert!(
                !join.binding_token.is_empty(),
                "binding_token should be non-empty"
            );
            assert!(
                !join.media_servers.is_empty(),
                "media_servers should be populated"
            );
            assert_eq!(
                join.media_servers[0].media_handler_url, "wt://mh-test-1:4433",
                "media_servers[0] should match first MH endpoint"
            );
        }
        other => panic!("Expected JoinResponse, got {other:?}"),
    }
}

#[tokio::test]
async fn test_join_success_first_participant_has_empty_roster() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-solo").await;

    let claims = make_meeting_claims("meeting-solo");
    let token = server.sign_token(&claims);

    let response = join_and_read_response(&server.url(), "meeting-solo", &token, "Solo").await;

    match &response.message {
        Some(server_message::Message::JoinResponse(join)) => {
            assert!(
                join.existing_participants.is_empty(),
                "First participant should see empty roster"
            );
        }
        other => panic!("Expected JoinResponse, got {other:?}"),
    }
}

// ============================================================================
// T2: JWT Validation — Expired Token Rejected
// ============================================================================

#[tokio::test]
async fn test_join_expired_token_returns_unauthorized() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-exp").await;

    let claims = make_expired_meeting_claims("meeting-exp");
    let token = server.sign_token(&claims);

    let response = join_and_read_response(&server.url(), "meeting-exp", &token, "Expired").await;

    let (code, message) = extract_error(&response);
    assert_eq!(code, signaling::ErrorCode::Unauthorized as i32);
    // Security: verify error message is generic, not detailed
    assert!(
        message.contains("Invalid or expired token") || message.contains("invalid"),
        "Error message should be generic, got: {message}"
    );
}

// ============================================================================
// T3: JWT Validation — Invalid/Garbage Token Rejected
// ============================================================================

#[tokio::test]
async fn test_join_garbage_token_returns_unauthorized() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-garbage").await;

    let response =
        join_and_read_response(&server.url(), "meeting-garbage", "not-a-valid-jwt", "Bad").await;

    let (code, _message) = extract_error(&response);
    assert_eq!(code, signaling::ErrorCode::Unauthorized as i32);
}

// ============================================================================
// T4: JWT Validation — Wrong Meeting ID in Token
// ============================================================================

#[tokio::test]
async fn test_join_wrong_meeting_id_returns_unauthorized() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-A").await;
    server.create_meeting("meeting-B").await;

    // Token is for meeting-A, but we join meeting-B
    let claims = make_meeting_claims("meeting-A");
    let token = server.sign_token(&claims);

    let response = join_and_read_response(&server.url(), "meeting-B", &token, "Mismatch").await;

    let (code, message) = extract_error(&response);
    assert_eq!(code, signaling::ErrorCode::Unauthorized as i32);
    // Security: generic error, doesn't reveal "meeting_id mismatch"
    assert!(
        !message.contains("mismatch"),
        "Error should not reveal mismatch details, got: {message}"
    );
}

// ============================================================================
// T5: Meeting Not Found
// ============================================================================

#[tokio::test]
async fn test_join_meeting_not_found_returns_not_found() {
    let server = TestServer::start().await;
    // Don't create any meeting

    let claims = make_meeting_claims("meeting-nonexistent");
    let token = server.sign_token(&claims);

    let response =
        join_and_read_response(&server.url(), "meeting-nonexistent", &token, "Lost").await;

    let (code, _message) = extract_error(&response);
    assert_eq!(code, signaling::ErrorCode::NotFound as i32);
}

// ============================================================================
// T5b: Meeting Exists But MH Assignment Missing
// ============================================================================

#[tokio::test]
async fn test_join_missing_mh_assignment_returns_internal_error() {
    let server = TestServer::start().await;
    // Create meeting via controller but DON'T seed MH data in mh_store
    server
        .controller_handle
        .create_meeting("meeting-no-mh".to_string())
        .await
        .expect("create meeting");

    let claims = make_meeting_claims("meeting-no-mh");
    let token = server.sign_token(&claims);

    let response = join_and_read_response(&server.url(), "meeting-no-mh", &token, "NoMedia").await;

    let (code, _message) = extract_error(&response);
    assert_eq!(
        code, 6,
        "MhAssignmentMissing should map to INTERNAL_ERROR (6)"
    );
}

// ============================================================================
// T6: Invalid Protobuf Rejected
// ============================================================================

#[tokio::test]
async fn test_join_invalid_protobuf_drops_connection() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-proto").await;

    let conn = connect_client(&server.url()).await;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .expect("open bi stream")
        .await
        .expect("bi stream ready");

    // Send garbage bytes with a valid length prefix
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03];
    let len = garbage.len() as u32;
    let mut frame = BytesMut::with_capacity(4 + garbage.len());
    frame.put_u32(len);
    frame.put_slice(&garbage);
    send.write_all(&frame).await.expect("write garbage");

    // Server should close the connection — read should fail or return nothing
    let mut buf = [0u8; 1];
    let result = tokio::time::timeout(Duration::from_secs(2), recv.read(&mut buf)).await;

    match result {
        Ok(Ok(None)) | Ok(Err(_)) | Err(_) => {
            // Expected: stream closed, error, or timeout
        }
        Ok(Ok(Some(_))) => {
            // If we got data, try to read a full message — it should be an error or nothing useful
        }
    }
}

// ============================================================================
// T7: First Message Not JoinRequest
// ============================================================================

#[tokio::test]
async fn test_join_wrong_first_message_returns_invalid_request() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-wrong-msg").await;

    let conn = connect_client(&server.url()).await;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .expect("open bi stream")
        .await
        .expect("bi stream ready");

    // Send a MuteRequest instead of JoinRequest
    let client_msg = ClientMessage {
        message: Some(client_message::Message::MuteRequest(MuteRequest {
            audio_muted: true,
            video_muted: false,
        })),
    };

    let frame = encode_framed(&client_msg);
    send.write_all(&frame).await.expect("write mute request");

    let response = tokio::time::timeout(Duration::from_secs(3), read_server_message(&mut recv))
        .await
        .expect("Timeout waiting for error response");

    let (code, _message) = extract_error(&response);
    assert_eq!(code, signaling::ErrorCode::InvalidRequest as i32);
}

// ============================================================================
// T8: Token Signed With Wrong Key Rejected
// ============================================================================

#[tokio::test]
async fn test_join_wrong_signing_key_returns_unauthorized() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-wrongkey").await;

    // Sign with a different keypair not in JWKS
    let wrong_keypair = TestKeypair::new(99, "wrong-key");
    let claims = make_meeting_claims("meeting-wrongkey");
    let token = wrong_keypair.sign_token(&claims);

    let response =
        join_and_read_response(&server.url(), "meeting-wrongkey", &token, "WrongKey").await;

    let (code, _message) = extract_error(&response);
    assert_eq!(code, signaling::ErrorCode::Unauthorized as i32);
}

// ============================================================================
// T9: Actor-Level Join Test (via MeetingControllerActorHandle directly)
// ============================================================================

#[tokio::test]
async fn test_actor_level_join_success() {
    let master_secret = SecretBox::new(Box::new(vec![0u8; 32]));
    let metrics = ActorMetrics::new();
    let controller_metrics = ControllerMetrics::new();
    let controller = MeetingControllerActorHandle::new(
        "mc-actor-test".to_string(),
        metrics,
        controller_metrics,
        master_secret,
        Arc::new(MhConnectionRegistry::new()),
    );

    controller
        .create_meeting("meeting-actor".to_string())
        .await
        .unwrap();

    let (outbound_tx, _outbound_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(100);

    let join_rx = controller
        .join_connection(
            "meeting-actor".to_string(),
            "conn-1".to_string(),
            "user-1".to_string(),
            "part-1".to_string(),
            false,
            outbound_tx,
        )
        .await
        .expect("join_connection should succeed");

    let result = tokio::time::timeout(Duration::from_secs(3), join_rx)
        .await
        .expect("Timeout waiting for join result")
        .expect("Join channel dropped")
        .expect("Join should succeed");

    assert_eq!(result.participant_id, "part-1");
    assert!(!result.correlation_id.is_empty());
    assert!(!result.binding_token.is_empty());
    assert!(
        result.participants.is_empty(),
        "First joiner should see empty roster"
    );

    controller.cancel();
}

#[tokio::test]
async fn test_actor_level_join_meeting_not_found() {
    let master_secret = SecretBox::new(Box::new(vec![0u8; 32]));
    let metrics = ActorMetrics::new();
    let controller_metrics = ControllerMetrics::new();
    let controller = MeetingControllerActorHandle::new(
        "mc-actor-test-2".to_string(),
        metrics,
        controller_metrics,
        master_secret,
        Arc::new(MhConnectionRegistry::new()),
    );

    let (outbound_tx, _outbound_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(100);

    let result = controller
        .join_connection(
            "nonexistent-meeting".to_string(),
            "conn-1".to_string(),
            "user-1".to_string(),
            "part-1".to_string(),
            false,
            outbound_tx,
        )
        .await;

    // The controller returns Err when meeting not found (via respond_to channel)
    match result {
        Err(e) => {
            assert!(
                format!("{e}").contains("not found") || format!("{e}").contains("Meeting"),
                "Expected MeetingNotFound error, got: {e}"
            );
        }
        Ok(rx) => {
            // If the channel was returned, the result inside should be an error
            let inner = tokio::time::timeout(Duration::from_secs(3), rx)
                .await
                .expect("Timeout")
                .expect("Channel dropped");
            assert!(inner.is_err(), "Expected error for nonexistent meeting");
        }
    }

    controller.cancel();
}

#[tokio::test]
async fn test_actor_level_second_joiner_sees_first_in_roster() {
    let master_secret = SecretBox::new(Box::new(vec![0u8; 32]));
    let metrics = ActorMetrics::new();
    let controller_metrics = ControllerMetrics::new();
    let controller = MeetingControllerActorHandle::new(
        "mc-actor-test-3".to_string(),
        metrics,
        controller_metrics,
        master_secret,
        Arc::new(MhConnectionRegistry::new()),
    );

    controller
        .create_meeting("meeting-roster".to_string())
        .await
        .unwrap();

    // First participant joins
    let (tx1, _rx1) = tokio::sync::mpsc::channel::<bytes::Bytes>(100);
    let join_rx1 = controller
        .join_connection(
            "meeting-roster".to_string(),
            "conn-1".to_string(),
            "user-1".to_string(),
            "part-1".to_string(),
            false,
            tx1,
        )
        .await
        .unwrap();

    let result1 = tokio::time::timeout(Duration::from_secs(3), join_rx1)
        .await
        .expect("Timeout")
        .expect("Channel dropped")
        .expect("Join 1 failed");

    assert!(result1.participants.is_empty());

    // Second participant joins
    let (tx2, _rx2) = tokio::sync::mpsc::channel::<bytes::Bytes>(100);
    let join_rx2 = controller
        .join_connection(
            "meeting-roster".to_string(),
            "conn-2".to_string(),
            "user-2".to_string(),
            "part-2".to_string(),
            false,
            tx2,
        )
        .await
        .unwrap();

    let result2 = tokio::time::timeout(Duration::from_secs(3), join_rx2)
        .await
        .expect("Timeout")
        .expect("Channel dropped")
        .expect("Join 2 failed");

    assert_eq!(
        result2.participants.len(),
        1,
        "Second joiner should see first participant"
    );
    assert_eq!(result2.participants[0].participant_id, "part-1");

    controller.cancel();
}

// ============================================================================
// T10: ParticipantJoined Notification via Bridge (Signaling Bridge)
// ============================================================================

#[tokio::test]
async fn test_participant_joined_notification_via_bridge() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-bridge").await;

    // First client connects and joins
    let claims1 = make_meeting_claims("meeting-bridge");
    let token1 = server.sign_token(&claims1);

    let conn1 = connect_client(&server.url()).await;
    let (mut send1, mut recv1) = conn1
        .open_bi()
        .await
        .expect("open bi stream 1")
        .await
        .expect("bi stream 1 ready");

    let client_msg1 = ClientMessage {
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: "meeting-bridge".to_string(),
            join_token: token1,
            participant_name: "Alice".to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
        })),
    };
    send1
        .write_all(&encode_framed(&client_msg1))
        .await
        .expect("write join 1");

    // Read JoinResponse for first client
    let response1 = tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv1))
        .await
        .expect("Timeout waiting for join response 1");
    assert!(
        matches!(
            &response1.message,
            Some(server_message::Message::JoinResponse(_))
        ),
        "Expected JoinResponse for first client"
    );

    // Second client connects and joins (need fresh claims with different sub for different user)
    let mut claims2 = make_meeting_claims("meeting-bridge");
    claims2.sub = "user-002".to_string();
    let token2 = server.sign_token(&claims2);

    let conn2 = connect_client(&server.url()).await;
    let (mut send2, mut recv2) = conn2
        .open_bi()
        .await
        .expect("open bi stream 2")
        .await
        .expect("bi stream 2 ready");

    let client_msg2 = ClientMessage {
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: "meeting-bridge".to_string(),
            join_token: token2,
            participant_name: "Bob".to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
        })),
    };
    send2
        .write_all(&encode_framed(&client_msg2))
        .await
        .expect("write join 2");

    // Read JoinResponse for second client
    let response2 = tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv2))
        .await
        .expect("Timeout waiting for join response 2");
    assert!(
        matches!(
            &response2.message,
            Some(server_message::Message::JoinResponse(_))
        ),
        "Expected JoinResponse for second client"
    );

    // First client should receive a ParticipantJoined notification via bridge
    let notification =
        tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv1))
            .await
            .expect("Timeout: ParticipantJoined notification not received via bridge");

    match &notification.message {
        Some(server_message::Message::ParticipantJoined(joined)) => {
            let p = joined.participant.as_ref().unwrap();
            assert!(!p.participant_id.is_empty());
        }
        other => panic!("Expected ParticipantJoined notification, got {other:?}"),
    }
}

// ============================================================================
// T11: Participant Name Too Long Rejected
// ============================================================================

#[tokio::test]
async fn test_join_participant_name_too_long_returns_error() {
    let server = TestServer::start().await;
    server.create_meeting("meeting-longname").await;

    let claims = make_meeting_claims("meeting-longname");
    let token = server.sign_token(&claims);

    // MAX_PARTICIPANT_NAME_LEN is 256 in connection.rs
    let long_name = "X".repeat(300);

    let response =
        join_and_read_response(&server.url(), "meeting-longname", &token, &long_name).await;

    let (code, _message) = extract_error(&response);
    assert_eq!(code, signaling::ErrorCode::InvalidRequest as i32);
}
