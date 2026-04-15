//! Integration tests for MH→MC notification delivery.
//!
//! Tests the McClient retry logic and auth-error short-circuit using
//! a mock `MediaCoordinationService` gRPC server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use mh_service::errors::MhError;
use mh_service::grpc::McClient;
use proto_gen::internal::media_coordination_service_server::{
    MediaCoordinationService, MediaCoordinationServiceServer,
};
use proto_gen::internal::{
    ParticipantMediaConnected, ParticipantMediaConnectedResponse, ParticipantMediaDisconnected,
    ParticipantMediaDisconnectedResponse,
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

// ============================================================================
// Mock MC Server
// ============================================================================

/// Behavior mode for the mock MC server.
#[derive(Debug, Clone, Copy)]
enum MockBehavior {
    /// Accept all notifications.
    Accept,
    /// Return UNAVAILABLE (retryable) for the first N calls, then accept.
    FailThenAccept { fail_count: u32 },
    /// Return UNAUTHENTICATED (non-retryable).
    Unauthenticated,
    /// Return PERMISSION_DENIED (non-retryable).
    PermissionDenied,
}

/// Mock MC MediaCoordinationService for testing notification delivery.
struct MockMcServer {
    behavior: MockBehavior,
    connected_count: AtomicU32,
    disconnected_count: AtomicU32,
}

impl MockMcServer {
    fn new(behavior: MockBehavior) -> Self {
        Self {
            behavior,
            connected_count: AtomicU32::new(0),
            disconnected_count: AtomicU32::new(0),
        }
    }

    fn total_calls(&self) -> u32 {
        self.connected_count.load(Ordering::SeqCst) + self.disconnected_count.load(Ordering::SeqCst)
    }

    fn should_fail(&self) -> Option<Status> {
        match self.behavior {
            MockBehavior::Accept => None,
            MockBehavior::FailThenAccept { fail_count } => {
                let total = self.total_calls();
                if total <= fail_count {
                    Some(Status::unavailable("MC temporarily unavailable"))
                } else {
                    None
                }
            }
            MockBehavior::Unauthenticated => Some(Status::unauthenticated("Invalid service token")),
            MockBehavior::PermissionDenied => {
                Some(Status::permission_denied("Service not authorized"))
            }
        }
    }
}

#[tonic::async_trait]
impl MediaCoordinationService for MockMcServer {
    async fn notify_participant_connected(
        &self,
        _request: Request<ParticipantMediaConnected>,
    ) -> Result<Response<ParticipantMediaConnectedResponse>, Status> {
        self.connected_count.fetch_add(1, Ordering::SeqCst);

        if let Some(status) = self.should_fail() {
            return Err(status);
        }

        Ok(Response::new(ParticipantMediaConnectedResponse {
            acknowledged: true,
        }))
    }

    async fn notify_participant_disconnected(
        &self,
        _request: Request<ParticipantMediaDisconnected>,
    ) -> Result<Response<ParticipantMediaDisconnectedResponse>, Status> {
        self.disconnected_count.fetch_add(1, Ordering::SeqCst);

        if let Some(status) = self.should_fail() {
            return Err(status);
        }

        Ok(Response::new(ParticipantMediaDisconnectedResponse {
            acknowledged: true,
        }))
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a mock `TokenReceiver` for testing.
fn mock_token_receiver() -> TokenReceiver {
    let (tx, rx) = watch::channel(SecretString::from("test-service-token"));
    // Keep sender alive by leaking it (test only)
    std::mem::forget(tx);
    TokenReceiver::from_test_channel(rx)
}

async fn start_mock_mc_server(mock_mc: MockMcServer) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let server = Server::builder()
        .add_service(MediaCoordinationServiceServer::new(mock_mc))
        .serve_with_incoming_shutdown(incoming, async move {
            cancel_token_clone.cancelled().await;
        });

    tokio::spawn(async move {
        let _ = server.await;
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    (addr, cancel_token)
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_notify_connected_success() {
    let mock_mc = MockMcServer::new(MockBehavior::Accept);
    let (addr, cancel_token) = start_mock_mc_server(mock_mc).await;

    let mc_url = format!("http://{addr}");
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    assert!(result.is_ok());
    cancel_token.cancel();
}

#[tokio::test]
async fn test_notify_disconnected_success() {
    let mock_mc = MockMcServer::new(MockBehavior::Accept);
    let (addr, cancel_token) = start_mock_mc_server(mock_mc).await;

    let mc_url = format!("http://{addr}");
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_disconnected(&mc_url, "meeting-1", "user-1", "mh-1", 1)
        .await;

    assert!(result.is_ok());
    cancel_token.cancel();
}

#[tokio::test]
async fn test_retry_succeeds_after_transient_failure() {
    // Fail the first call, succeed on retry
    let mock_mc = MockMcServer::new(MockBehavior::FailThenAccept { fail_count: 1 });
    let (addr, cancel_token) = start_mock_mc_server(mock_mc).await;

    let mc_url = format!("http://{addr}");
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    // Should succeed on second attempt
    assert!(result.is_ok());
    cancel_token.cancel();
}

#[tokio::test]
async fn test_retry_exhaustion_after_max_attempts() {
    // Fail all 3 attempts (fail_count >= MAX_RETRY_ATTEMPTS)
    let mock_mc = MockMcServer::new(MockBehavior::FailThenAccept { fail_count: 10 });
    let (addr, cancel_token) = start_mock_mc_server(mock_mc).await;

    let mc_url = format!("http://{addr}");
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    assert!(
        matches!(&result, Err(MhError::Grpc(msg)) if msg.contains("MC notification RPC failed")),
        "Expected Grpc error after exhausting retries, got: {result:?}"
    );
    cancel_token.cancel();
}

#[tokio::test]
async fn test_unauthenticated_error_skips_retry() {
    let mock_mc = MockMcServer::new(MockBehavior::Unauthenticated);
    let (addr, cancel_token) = start_mock_mc_server(mock_mc).await;

    let mc_url = format!("http://{addr}");
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;

    // Should fail immediately with JwtValidation (mapped from UNAUTHENTICATED)
    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "Expected JwtValidation error for auth failure, got: {result:?}"
    );
    cancel_token.cancel();
}

#[tokio::test]
async fn test_permission_denied_skips_retry() {
    let mock_mc = MockMcServer::new(MockBehavior::PermissionDenied);
    let (addr, cancel_token) = start_mock_mc_server(mock_mc).await;

    let mc_url = format!("http://{addr}");
    let client = McClient::new(mock_token_receiver());

    let result = client
        .notify_participant_disconnected(&mc_url, "meeting-1", "user-1", "mh-1", 1)
        .await;

    // Should fail immediately with JwtValidation (mapped from PERMISSION_DENIED)
    assert!(
        matches!(&result, Err(MhError::JwtValidation(_))),
        "Expected JwtValidation error for permission denied, got: {result:?}"
    );
    cancel_token.cancel();
}
