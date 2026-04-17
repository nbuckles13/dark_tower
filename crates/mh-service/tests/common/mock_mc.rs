//! Mock `MediaCoordinationService` gRPC server used by mh-service tests.
//!
//! Supports two modes of usage:
//! - `MockMcServer::new(MockBehavior)` — counts invocations (used by
//!   `mc_client_integration.rs` to exercise `McClient` retry semantics)
//! - Channel capture via `with_connected_tx` / `with_disconnected_tx` — pushes
//!   received request payloads on an `mpsc::Sender` so integration tests can
//!   assert on the exact fields MH sent.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use proto_gen::internal::media_coordination_service_server::{
    MediaCoordinationService, MediaCoordinationServiceServer,
};
use proto_gen::internal::{
    ParticipantMediaConnected, ParticipantMediaConnectedResponse, ParticipantMediaDisconnected,
    ParticipantMediaDisconnectedResponse,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

/// Behavior mode for the mock MC server.
#[derive(Debug, Clone, Copy)]
pub enum MockBehavior {
    /// Accept all notifications.
    Accept,
    /// Return UNAVAILABLE (retryable) for the first N calls, then accept.
    FailThenAccept { fail_count: u32 },
    /// Return UNAUTHENTICATED (non-retryable).
    Unauthenticated,
    /// Return `PERMISSION_DENIED` (non-retryable).
    PermissionDenied,
}

/// Mock MC `MediaCoordinationService` for integration testing.
pub struct MockMcServer {
    behavior: MockBehavior,
    connected_count: AtomicU32,
    disconnected_count: AtomicU32,
    connected_tx: Option<mpsc::Sender<ParticipantMediaConnected>>,
    disconnected_tx: Option<mpsc::Sender<ParticipantMediaDisconnected>>,
}

impl MockMcServer {
    pub fn new(behavior: MockBehavior) -> Self {
        Self {
            behavior,
            connected_count: AtomicU32::new(0),
            disconnected_count: AtomicU32::new(0),
            connected_tx: None,
            disconnected_tx: None,
        }
    }

    pub fn with_connected_tx(mut self, tx: mpsc::Sender<ParticipantMediaConnected>) -> Self {
        self.connected_tx = Some(tx);
        self
    }

    pub fn with_disconnected_tx(mut self, tx: mpsc::Sender<ParticipantMediaDisconnected>) -> Self {
        self.disconnected_tx = Some(tx);
        self
    }

    pub fn total_calls(&self) -> u32 {
        self.connected_count.load(Ordering::SeqCst) + self.disconnected_count.load(Ordering::SeqCst)
    }

    fn should_fail(&self) -> Option<Status> {
        match self.behavior {
            MockBehavior::Accept => None,
            MockBehavior::FailThenAccept { fail_count } => {
                if self.total_calls() <= fail_count {
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
        request: Request<ParticipantMediaConnected>,
    ) -> Result<Response<ParticipantMediaConnectedResponse>, Status> {
        self.connected_count.fetch_add(1, Ordering::SeqCst);
        let inner = request.into_inner();

        if let Some(tx) = &self.connected_tx {
            let _ = tx.send(inner.clone()).await;
        }

        if let Some(status) = self.should_fail() {
            return Err(status);
        }

        Ok(Response::new(ParticipantMediaConnectedResponse {
            acknowledged: true,
        }))
    }

    async fn notify_participant_disconnected(
        &self,
        request: Request<ParticipantMediaDisconnected>,
    ) -> Result<Response<ParticipantMediaDisconnectedResponse>, Status> {
        self.disconnected_count.fetch_add(1, Ordering::SeqCst);
        let inner = request.into_inner();

        if let Some(tx) = &self.disconnected_tx {
            let _ = tx.send(inner.clone()).await;
        }

        if let Some(status) = self.should_fail() {
            return Err(status);
        }

        Ok(Response::new(ParticipantMediaDisconnectedResponse {
            acknowledged: true,
        }))
    }
}

/// RAII handle to a running mock MC gRPC server.
///
/// Holding the handle keeps the server alive; dropping it (or panicking out of
/// the test scope) cancels and aborts the spawned task so the port is released
/// even if an assertion fires early.
pub struct MockMcHandle {
    pub addr: SocketAddr,
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl MockMcHandle {
    /// Trigger graceful shutdown explicitly (optional — `Drop` also does this).
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for MockMcHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

/// Bind a mock MC gRPC server on `127.0.0.1:0` and spawn its `serve` loop.
///
/// The returned [`MockMcHandle`] must outlive the test; dropping it cancels
/// and aborts the server task automatically.
pub async fn start_mock_mc_server(mock_mc: MockMcServer) -> MockMcHandle {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mock MC TcpListener");
    let addr = listener
        .local_addr()
        .expect("failed to read local addr for mock MC");

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let server = Server::builder()
        .add_service(MediaCoordinationServiceServer::new(mock_mc))
        .serve_with_incoming_shutdown(incoming, async move {
            cancel_clone.cancelled().await;
        });

    let handle = tokio::spawn(async move {
        let _ = server.await;
    });

    // Tonic needs a moment to spin up the HTTP/2 server before accepting.
    tokio::time::sleep(Duration::from_millis(50)).await;

    MockMcHandle {
        addr,
        cancel,
        handle: Some(handle),
    }
}
