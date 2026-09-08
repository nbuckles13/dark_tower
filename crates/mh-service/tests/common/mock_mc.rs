//! Mock `MediaCoordinationService` gRPC server used by mh-service tests.
//!
//! Supports two modes of usage:
//! - `MockMcServer::new(MockBehavior)` — counts invocations (used by
//!   `mc_client_integration.rs` to exercise `McClient` retry semantics)
//! - Channel capture via `with_connected_tx` / `with_disconnected_tx` — pushes
//!   received request payloads on an `mpsc::Sender` so integration tests can
//!   assert on the exact fields MH sent.
//! - `with_traceparent_tx` — pushes the inbound `traceparent` metadata value
//!   (if any) seen on each call, in call order, for R-56 outbound-injection
//!   assertions (`otel_grpc_integration.rs`).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use proto_gen::dark_tower::internal::v1::media_coordination_service_server::{
    MediaCoordinationService, MediaCoordinationServiceServer,
};
use proto_gen::dark_tower::internal::v1::{
    NotifyParticipantConnectedRequest, NotifyParticipantConnectedResponse,
    NotifyParticipantDisconnectedRequest, NotifyParticipantDisconnectedResponse,
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

/// How the mock answers `NotifyParticipantConnected`'s `sender_id`.
///
/// # The default is NOT a valid ordinal, deliberately
///
/// [`SenderReplies::default`] answers `0` for every participant, i.e. "MC has no
/// answer" — so a test that wants a media session must SAY which ordinal each
/// participant gets. The tempting alternative, defaulting to some valid id, is
/// the lazy repair when this file next fails to compile, and it would silently
/// disarm every reject arm in the suite at once: each accept-path test would
/// start a media session whether or not the binding logic worked.
#[derive(Debug, Clone, Default)]
pub struct SenderReplies {
    per_participant: HashMap<String, u32>,
}

impl SenderReplies {
    /// Answer `sender_id` for `participant_id`, and `0` for everyone else.
    ///
    /// Values are the mock's *record* of what it allocated: assertions must read
    /// them back from here rather than hard-coding the same literal twice. An
    /// expectation written from memory passes on exactly the input it was
    /// written to match (review protocol §Assertion Vacuity, mechanism 4).
    #[must_use]
    pub fn with(mut self, participant_id: &str, sender_id: u32) -> Self {
        self.per_participant
            .insert(participant_id.to_string(), sender_id);
        self
    }

    /// What this table says `participant_id` was allocated. `0` when unset.
    #[must_use]
    pub fn allocated_for(&self, participant_id: &str) -> u32 {
        self.per_participant
            .get(participant_id)
            .copied()
            .unwrap_or(0)
    }
}

/// Mock MC `MediaCoordinationService` for integration testing.
pub struct MockMcServer {
    behavior: MockBehavior,
    sender_replies: SenderReplies,
    connected_count: AtomicU32,
    disconnected_count: AtomicU32,
    connected_tx: Option<mpsc::Sender<NotifyParticipantConnectedRequest>>,
    disconnected_tx: Option<mpsc::Sender<NotifyParticipantDisconnectedRequest>>,
    traceparent_tx: Option<mpsc::Sender<Option<String>>>,
}

impl MockMcServer {
    pub fn new(behavior: MockBehavior) -> Self {
        Self {
            behavior,
            sender_replies: SenderReplies::default(),
            connected_count: AtomicU32::new(0),
            disconnected_count: AtomicU32::new(0),
            connected_tx: None,
            disconnected_tx: None,
            traceparent_tx: None,
        }
    }

    /// Answer `NotifyParticipantConnected` from `replies` instead of the
    /// answer-nobody default.
    #[must_use]
    pub fn with_sender_replies(mut self, replies: SenderReplies) -> Self {
        self.sender_replies = replies;
        self
    }

    pub fn with_connected_tx(
        mut self,
        tx: mpsc::Sender<NotifyParticipantConnectedRequest>,
    ) -> Self {
        self.connected_tx = Some(tx);
        self
    }

    pub fn with_disconnected_tx(
        mut self,
        tx: mpsc::Sender<NotifyParticipantDisconnectedRequest>,
    ) -> Self {
        self.disconnected_tx = Some(tx);
        self
    }

    /// Capture the inbound `traceparent` metadata value (if any) seen on
    /// each call, in call order — one send per RPC regardless of outcome.
    pub fn with_traceparent_tx(mut self, tx: mpsc::Sender<Option<String>>) -> Self {
        self.traceparent_tx = Some(tx);
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

/// Read the `traceparent` metadata value off an inbound request, if present.
fn extract_traceparent<T>(request: &Request<T>) -> Option<String> {
    request
        .metadata()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[tonic::async_trait]
impl MediaCoordinationService for MockMcServer {
    async fn notify_participant_connected(
        &self,
        request: Request<NotifyParticipantConnectedRequest>,
    ) -> Result<Response<NotifyParticipantConnectedResponse>, Status> {
        self.connected_count.fetch_add(1, Ordering::SeqCst);
        let traceparent = extract_traceparent(&request);
        let inner = request.into_inner();

        if let Some(tx) = &self.connected_tx {
            let _ = tx.send(inner.clone()).await;
        }
        if let Some(tx) = &self.traceparent_tx {
            let _ = tx.send(traceparent).await;
        }

        if let Some(status) = self.should_fail() {
            return Err(status);
        }

        // `acknowledged: true` alongside `sender_id: 0` is the LEGAL, EXPECTED
        // shape when MC has no answer (`internal.proto`), so the mock emits it
        // rather than coupling the two. A consumer that gates on `acknowledged`
        // must fail against this mock.
        Ok(Response::new(NotifyParticipantConnectedResponse {
            acknowledged: true,
            sender_id: self.sender_replies.allocated_for(&inner.participant_id),
        }))
    }

    async fn notify_participant_disconnected(
        &self,
        request: Request<NotifyParticipantDisconnectedRequest>,
    ) -> Result<Response<NotifyParticipantDisconnectedResponse>, Status> {
        self.disconnected_count.fetch_add(1, Ordering::SeqCst);
        let traceparent = extract_traceparent(&request);
        let inner = request.into_inner();

        if let Some(tx) = &self.disconnected_tx {
            let _ = tx.send(inner.clone()).await;
        }
        if let Some(tx) = &self.traceparent_tx {
            let _ = tx.send(traceparent).await;
        }

        if let Some(status) = self.should_fail() {
            return Err(status);
        }

        Ok(Response::new(NotifyParticipantDisconnectedResponse {
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
