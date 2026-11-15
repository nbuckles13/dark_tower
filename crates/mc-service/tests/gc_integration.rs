//! Integration tests for MC-GC communication.
//!
//! Tests the registration, heartbeat, and assignment flows between
//! Meeting Controller and Global Controller.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mc_service::actors::ControllerMetrics;
use mc_service::config::Config;
use mc_service::errors::McError;
use mc_service::grpc::GcClient;
use mc_test_utils::test_token_receiver;

use common::secret::SecretString;
use proto_gen::dark_tower::internal::v1::global_controller_service_server::{
    GlobalControllerService, GlobalControllerServiceServer,
};
use proto_gen::dark_tower::internal::v1::{
    AcquireMediaHandlerRequest, AcquireMediaHandlerResponse, MhAssignment, ReleaseCause,
};
use proto_gen::dark_tower::internal::v1::{
    ComprehensiveHeartbeatRequest, ComprehensiveHeartbeatResponse, FastHeartbeatRequest,
    FastHeartbeatResponse, HealthStatus, NotifyMeetingEndedOutcome, NotifyMeetingEndedRequest,
    NotifyMeetingEndedResponse, RegisterMcRequest, RegisterMcResponse,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

// ============================================================================
// Mock GC Server
// ============================================================================

/// Behavior mode for MockGcServer.
#[derive(Debug, Clone, Copy)]
enum MockBehavior {
    /// Accept all requests normally.
    Accept,
    /// Reject registrations.
    Reject,
    /// Return NOT_FOUND for heartbeats (simulates MC not registered).
    NotFound,
    /// Return NOT_FOUND for first heartbeat, then accept (simulates re-registration).
    NotFoundThenAccept,
}

/// Mock GC server for testing MC registration and heartbeats.
struct MockGcServer {
    /// Behavior mode.
    behavior: MockBehavior,
    /// Fast heartbeat interval to return (ms).
    fast_heartbeat_interval_ms: u64,
    /// Comprehensive heartbeat interval to return (ms).
    comprehensive_heartbeat_interval_ms: u64,
    /// Count of received registrations.
    registration_count: AtomicU32,
    /// Count of received fast heartbeats.
    fast_heartbeat_count: AtomicU32,
    /// Count of received comprehensive heartbeats.
    comprehensive_heartbeat_count: AtomicU32,
    /// Channel to notify when registration received.
    registration_tx: Option<mpsc::Sender<RegisterMcRequest>>,
    /// Channel to notify when fast heartbeat received.
    fast_heartbeat_tx: Option<mpsc::Sender<FastHeartbeatRequest>>,
    /// Channel to notify when comprehensive heartbeat received.
    comprehensive_heartbeat_tx: Option<mpsc::Sender<ComprehensiveHeartbeatRequest>>,
    /// `NotifyMeetingEnded` requests received (shared so the test can read it
    /// after the server owns the mock).
    notify_meeting_ended_calls: Arc<AtomicU32>,
    /// When set, `NotifyMeetingEnded` answers with this status instead of
    /// acking, for the next `notify_failures_left` calls (`u32::MAX` = always).
    notify_meeting_ended_failure: Option<tonic::Code>,
    notify_failures_left: Arc<AtomicU32>,
    /// A meeting id whose notify ALWAYS fails `UNAVAILABLE` (head-of-line).
    notify_always_fails_for: Option<String>,
    /// The outcome an acked notify carries.
    notify_outcome: NotifyMeetingEndedOutcome,
    /// Every `NotifyMeetingEnded` request received, in order.
    notify_requests: Arc<std::sync::Mutex<Vec<NotifyMeetingEndedRequest>>>,
    /// `RegisterMCResponse.max_exclude_handler_ids` (shared so a test can change
    /// it between a registration and a re-registration).
    max_exclude_handler_ids: Arc<AtomicU32>,
    /// How `AcquireMediaHandler` answers.
    acquire_reply: Arc<std::sync::Mutex<AcquireReply>>,
    /// Every `AcquireMediaHandler` request received.
    acquire_requests: Arc<std::sync::Mutex<Vec<AcquireMediaHandlerRequest>>>,
}

/// How the mock answers `AcquireMediaHandler`.
#[derive(Debug, Clone)]
enum AcquireReply {
    /// OK with this response.
    Ok(AcquireMediaHandlerResponse),
    /// This status code.
    Status(tonic::Code),
    /// Never answer (a hung GC).
    Hang,
}

impl MockGcServer {
    fn new() -> Self {
        Self {
            behavior: MockBehavior::Accept,
            fast_heartbeat_interval_ms: 10_000,
            comprehensive_heartbeat_interval_ms: 30_000,
            registration_count: AtomicU32::new(0),
            fast_heartbeat_count: AtomicU32::new(0),
            comprehensive_heartbeat_count: AtomicU32::new(0),
            registration_tx: None,
            fast_heartbeat_tx: None,
            comprehensive_heartbeat_tx: None,
            notify_meeting_ended_calls: Arc::new(AtomicU32::new(0)),
            notify_meeting_ended_failure: None,
            notify_failures_left: Arc::new(AtomicU32::new(0)),
            notify_always_fails_for: None,
            notify_outcome: NotifyMeetingEndedOutcome::Ended,
            notify_requests: Arc::default(),
            // GC's shipped default (GC_MH_MAX_EXCLUDE_HANDLER_IDS).
            max_exclude_handler_ids: Arc::new(AtomicU32::new(16)),
            acquire_reply: Arc::new(std::sync::Mutex::new(AcquireReply::Status(
                tonic::Code::Unimplemented,
            ))),
            acquire_requests: Arc::default(),
        }
    }

    /// Fail the next `times` notifies with `code` (`u32::MAX` = every one).
    fn with_notify_meeting_ended_failure(mut self, code: tonic::Code, times: u32) -> Self {
        self.notify_meeting_ended_failure = Some(code);
        self.notify_failures_left.store(times, Ordering::SeqCst);
        self
    }

    fn with_notify_always_failing_for(mut self, meeting_id: &str) -> Self {
        self.notify_always_fails_for = Some(meeting_id.to_string());
        self
    }

    fn with_notify_outcome(mut self, outcome: NotifyMeetingEndedOutcome) -> Self {
        self.notify_outcome = outcome;
        self
    }

    fn new_with_behavior(behavior: MockBehavior) -> Self {
        Self {
            behavior,
            ..Self::new()
        }
    }

    fn accepting() -> Self {
        Self::new()
    }

    fn rejecting() -> Self {
        Self::new_with_behavior(MockBehavior::Reject)
    }

    fn with_registration_channel(mut self, tx: mpsc::Sender<RegisterMcRequest>) -> Self {
        self.registration_tx = Some(tx);
        self
    }

    fn with_fast_heartbeat_channel(mut self, tx: mpsc::Sender<FastHeartbeatRequest>) -> Self {
        self.fast_heartbeat_tx = Some(tx);
        self
    }

    fn with_comprehensive_heartbeat_channel(
        mut self,
        tx: mpsc::Sender<ComprehensiveHeartbeatRequest>,
    ) -> Self {
        self.comprehensive_heartbeat_tx = Some(tx);
        self
    }

    fn with_heartbeat_intervals(mut self, fast_ms: u64, comprehensive_ms: u64) -> Self {
        self.fast_heartbeat_interval_ms = fast_ms;
        self.comprehensive_heartbeat_interval_ms = comprehensive_ms;
        self
    }
}

#[tonic::async_trait]
impl GlobalControllerService for MockGcServer {
    async fn register_mc(
        &self,
        request: Request<RegisterMcRequest>,
    ) -> Result<Response<RegisterMcResponse>, Status> {
        let inner = request.into_inner();
        self.registration_count.fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = &self.registration_tx {
            let _ = tx.send(inner.clone()).await;
        }

        match self.behavior {
            MockBehavior::Accept | MockBehavior::NotFound | MockBehavior::NotFoundThenAccept => {
                Ok(Response::new(RegisterMcResponse {
                    accepted: true,
                    message: "Registration accepted".to_string(),
                    fast_heartbeat_interval_ms: self.fast_heartbeat_interval_ms,
                    comprehensive_heartbeat_interval_ms: self.comprehensive_heartbeat_interval_ms,
                    max_exclude_handler_ids: self.max_exclude_handler_ids.load(Ordering::SeqCst),
                }))
            }
            MockBehavior::Reject => Ok(Response::new(RegisterMcResponse {
                accepted: false,
                message: "Registration rejected by mock".to_string(),
                fast_heartbeat_interval_ms: 0,
                comprehensive_heartbeat_interval_ms: 0,
                max_exclude_handler_ids: 0,
            })),
        }
    }

    async fn fast_heartbeat(
        &self,
        request: Request<FastHeartbeatRequest>,
    ) -> Result<Response<FastHeartbeatResponse>, Status> {
        let inner = request.into_inner();
        let count = self.fast_heartbeat_count.fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = &self.fast_heartbeat_tx {
            let _ = tx.send(inner).await;
        }

        match self.behavior {
            MockBehavior::NotFound => {
                // Return NOT_FOUND status
                Err(Status::not_found("MC not registered with GC"))
            }
            MockBehavior::NotFoundThenAccept => {
                // First heartbeat returns NOT_FOUND, subsequent ones accept
                if count == 0 {
                    Err(Status::not_found("MC not registered with GC"))
                } else {
                    Ok(Response::new(FastHeartbeatResponse {
                        acknowledged: true,
                        timestamp: chrono::Utc::now().timestamp() as u64,
                    }))
                }
            }
            MockBehavior::Accept | MockBehavior::Reject => {
                Ok(Response::new(FastHeartbeatResponse {
                    acknowledged: true,
                    timestamp: chrono::Utc::now().timestamp() as u64,
                }))
            }
        }
    }

    async fn comprehensive_heartbeat(
        &self,
        request: Request<ComprehensiveHeartbeatRequest>,
    ) -> Result<Response<ComprehensiveHeartbeatResponse>, Status> {
        let inner = request.into_inner();
        let count = self
            .comprehensive_heartbeat_count
            .fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = &self.comprehensive_heartbeat_tx {
            let _ = tx.send(inner).await;
        }

        match self.behavior {
            MockBehavior::NotFound => {
                // Return NOT_FOUND status
                Err(Status::not_found("MC not registered with GC"))
            }
            MockBehavior::NotFoundThenAccept => {
                // First heartbeat returns NOT_FOUND, subsequent ones accept
                if count == 0 {
                    Err(Status::not_found("MC not registered with GC"))
                } else {
                    Ok(Response::new(ComprehensiveHeartbeatResponse {
                        acknowledged: true,
                        timestamp: chrono::Utc::now().timestamp() as u64,
                    }))
                }
            }
            MockBehavior::Accept | MockBehavior::Reject => {
                Ok(Response::new(ComprehensiveHeartbeatResponse {
                    acknowledged: true,
                    timestamp: chrono::Utc::now().timestamp() as u64,
                }))
            }
        }
    }

    async fn notify_meeting_ended(
        &self,
        request: Request<NotifyMeetingEndedRequest>,
    ) -> Result<Response<NotifyMeetingEndedResponse>, Status> {
        let inner = request.into_inner();
        self.notify_meeting_ended_calls
            .fetch_add(1, Ordering::SeqCst);
        self.notify_requests
            .lock()
            .expect("notify_requests poisoned")
            .push(inner.clone());
        if self.notify_always_fails_for.as_deref() == Some(inner.meeting_id.as_str()) {
            return Err(Status::unavailable(
                "mock: this meeting's notify always fails",
            ));
        }
        if let Some(code) = self.notify_meeting_ended_failure {
            let left = self.notify_failures_left.load(Ordering::SeqCst);
            if left > 0 {
                if left != u32::MAX {
                    self.notify_failures_left.store(left - 1, Ordering::SeqCst);
                }
                return Err(Status::new(code, "mock NotifyMeetingEnded failure"));
            }
        }
        Ok(Response::new(NotifyMeetingEndedResponse {
            outcome: self.notify_outcome.into(),
        }))
    }

    async fn acquire_media_handler(
        &self,
        request: Request<AcquireMediaHandlerRequest>,
    ) -> Result<Response<AcquireMediaHandlerResponse>, Status> {
        self.acquire_requests
            .lock()
            .unwrap()
            .push(request.into_inner());
        let reply = self.acquire_reply.lock().unwrap().clone();
        match reply {
            AcquireReply::Ok(response) => Ok(Response::new(response)),
            AcquireReply::Status(code) => Err(Status::new(code, "mock AcquireMediaHandler")),
            AcquireReply::Hang => std::future::pending().await,
        }
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

fn test_config(gc_url: &str) -> Config {
    // The media triple comes from the shared fixture, whose rustdoc carries the
    // matches-the-ConfigMap claim. Restating it here would make this the second
    // encoding of a value the ConfigMap owns — and would leave this function and
    // `otel_grpc_outbound_integration.rs::test_config`, which were byte-identical
    // before this diff, silently disagreeing about the source of truth.
    let media = test_common::client_media_config();
    Config {
        mc_id: "mc-test-001".to_string(),
        region: "us-east-1".to_string(),
        webtransport_bind_address: "0.0.0.0:4433".to_string(),
        grpc_bind_address: "0.0.0.0:50052".to_string(),
        health_bind_address: "0.0.0.0:8081".to_string(),
        redis_url: SecretString::from("redis://localhost:6379"),
        gc_grpc_url: gc_url.to_string(),
        max_meetings: 1000,
        max_participants: 10000,
        clock_skew_seconds: 5,
        disconnect_grace_period_seconds: 30,
        meeting_join_window_seconds: mc_service::actors::meeting_timers::MAX_JOIN_WINDOW.as_secs(),
        quic_max_idle_timeout_seconds: 10,
        binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
        ac_endpoint: "https://ac.example.com".to_string(),
        client_id: "mc-service".to_string(),
        client_secret: SecretString::from("test-client-secret"),
        ac_jwks_url: "https://ac.example.com/.well-known/jwks.json".to_string(),
        tls_cert_path: "/dev/null".to_string(),
        tls_key_path: "/dev/null".to_string(),
        grpc_advertise_address: "http://localhost:50052".to_string(),
        webtransport_advertise_address: "https://localhost:4433".to_string(),
        otel_enabled: false,
        otel_endpoint: String::new(),
        otel_sample_rate: 1.0,
        environment: "development".to_string(),
        receive_slot_caps: media.receive_slot_caps,
        media_connect_settle_ms: 1500,
        mh_heartbeat_timeout_ms: 6000,
        kek_rotation_debounce_seconds: 60,
        media_dial_token_ttl_seconds: test_common::media_dial_timing().ttl_seconds(),
        mh_replacement_min_interval_ms: 1000,
        mh_replacement_backoff_max_ms: 30000,
        planned_transfer_margin_ms: 5000,
        client_loss_disagreement: mc_service::media_routing::DisagreementThreshold::new(
            mc_service::media_routing::DisagreementFraction::new(0.5).unwrap(),
            std::num::NonZeroU32::new(3).unwrap(),
        ),
        max_receive_capability_declarations: media.max_receive_capability_declarations,
        audio_encoding: media.audio_encoding,
        video_encoding: media.video_encoding,
    }
}

async fn start_mock_gc_server(mock_gc: MockGcServer) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();

    // Convert tokio listener to tonic-compatible incoming stream
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let server = Server::builder()
        .add_service(GlobalControllerServiceServer::new(mock_gc))
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
// Registration Tests
// ============================================================================

#[tokio::test]
async fn test_gc_client_registration_success() {
    let mock_gc = MockGcServer::accepting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    assert!(!gc_client.is_registered());

    gc_client.register().await.unwrap();

    assert!(gc_client.is_registered());

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_registration_rejected() {
    let mock_gc = MockGcServer::rejecting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    let result = gc_client.register().await;
    assert!(result.is_err());
    assert!(!gc_client.is_registered());

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_registration_content() {
    let (registration_tx, mut registration_rx) = mpsc::channel(1);
    let mock_gc = MockGcServer::accepting().with_registration_channel(registration_tx);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    let request = registration_rx.recv().await.unwrap();
    assert_eq!(request.id, config.mc_id);
    assert_eq!(request.region, config.region);
    assert_eq!(request.max_meetings, config.max_meetings);
    assert_eq!(request.max_participants, config.max_participants);
    assert_eq!(request.grpc_endpoint, config.grpc_advertise_address);
    assert_eq!(
        request.webtransport_endpoint,
        config.webtransport_advertise_address
    );

    cancel_token.cancel();
}

/// Story 4 MC-C: `RegisterMCResponse.max_exclude_handler_ids` 0 FAILS MC
/// registration and is not retried (a GC predating GC-3 sends 0 on every
/// attempt). "Absent" and 0 are the same proto3 wire value, so the 0 arm IS
/// the absent arm. Positive arm: 16 registers and is stored; a
/// re-registration refreshes it (7).
#[tokio::test]
async fn registration_requires_a_positive_max_exclude_cap_and_reregistration_refreshes_it() {
    use mc_service::grpc::gc_client::HandlerAcquirer;

    // 0 (= absent): refused, once, loudly.
    let mock_gc = MockGcServer::accepting();
    let cap = Arc::clone(&mock_gc.max_exclude_handler_ids);
    cap.store(0, Ordering::SeqCst);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{addr}");
    let gc_client = GcClient::new(gc_url.clone(), test_token_receiver(), test_config(&gc_url))
        .await
        .unwrap();
    match gc_client.register().await {
        Err(McError::Config(msg)) => assert!(
            msg.contains("max_exclude_handler_ids"),
            "names the field: {msg}"
        ),
        other => panic!("expected a Config refusal, got {other:?}"),
    }
    assert!(!gc_client.is_registered());
    assert_eq!(gc_client.max_exclude_handler_ids(), None);
    assert!(gc_client.attempt_reregistration().await.is_err());
    assert!(!gc_client.is_registered());

    // 16: registered, stored; a re-registration with 7 refreshes it.
    cap.store(16, Ordering::SeqCst);
    gc_client.register().await.unwrap();
    assert!(gc_client.is_registered());
    assert_eq!(
        HandlerAcquirer::max_exclude_handler_ids(&gc_client).map(|c| c.get()),
        Some(16)
    );
    cap.store(7, Ordering::SeqCst);
    gc_client.attempt_reregistration().await.unwrap();
    assert_eq!(
        gc_client.max_exclude_handler_ids().map(|c| c.get()),
        Some(7)
    );
    cancel_token.cancel();
}

/// Story 4 MC-C: the AcquireMediaHandler request carries the deduplicated
/// exclude list, the projected trigger, the optional replaced id and this
/// MC's id; each documented status lands in its class with code and message
/// kept.
#[tokio::test]
async fn acquire_media_handler_request_and_status_classes() {
    use mc_service::grpc::gc_client::{
        AcquireCall, AcquireRejection, AcquireResult, HandlerAcquirer, MalformedGrant,
    };
    use mc_service::media_routing::AssignmentRejected;
    use mc_service::media_routing::{HandlerId, RemovalTrigger};

    let mock_gc = MockGcServer::accepting();
    let reply = Arc::clone(&mock_gc.acquire_reply);
    let requests = Arc::clone(&mock_gc.acquire_requests);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let gc_client = GcClient::new(gc_url, test_token_receiver(), config.clone())
        .await
        .unwrap();

    let assignment = MhAssignment {
        mh_id: "mh-new".to_string(),
        webtransport_endpoint: "https://mh-new:4434".to_string(),
        grpc_endpoint: "http://mh-new:50053".to_string(),
    };
    *reply.lock().unwrap() = AcquireReply::Ok(AcquireMediaHandlerResponse {
        assignment: Some(assignment.clone()),
    });
    let call = AcquireCall {
        meeting_id: "m-1".to_string(),
        exclude: [
            HandlerId::new("mh-b"),
            HandlerId::new("mh-a"),
            HandlerId::new("mh-a"),
        ]
        .into_iter()
        .collect(),
        trigger: RemovalTrigger::ApplyLag,
        replaced: Some(HandlerId::new("mh-a")),
    };
    match gc_client.acquire(&call).await {
        AcquireResult::Granted(endpoint) => {
            assert_eq!(endpoint.id.as_str(), "mh-new");
            assert_eq!(endpoint.webtransport_url, "https://mh-new:4434");
            assert_eq!(endpoint.grpc_endpoint, "http://mh-new:50053");
        }
        other => panic!("expected a grant, got {other:?}"),
    }
    let sent = requests.lock().unwrap().last().cloned().unwrap();
    assert_eq!(sent.meeting_id, "m-1");
    assert_eq!(sent.controller_id, config.mc_id);
    assert_eq!(
        sent.exclude_handler_ids,
        vec!["mh-a", "mh-b"],
        "deduplicated"
    );
    assert_eq!(sent.trigger, ReleaseCause::ApplyLag as i32);
    assert_eq!(sent.replaced_handler_id.as_deref(), Some("mh-a"));

    // Absent replaced id is OMITTED, not empty.
    let no_replaced = AcquireCall {
        replaced: None,
        ..call.clone()
    };
    let _ = gc_client.acquire(&no_replaced).await;
    assert_eq!(
        requests.lock().unwrap().last().unwrap().replaced_handler_id,
        None
    );

    // A malformed OK fails closed, one arm per missing field.
    for (broken, want) in [
        (None, MalformedGrant::NoAssignment),
        (
            Some(MhAssignment {
                mh_id: String::new(),
                ..assignment.clone()
            }),
            MalformedGrant::Assignment(AssignmentRejected::EmptyHandlerId),
        ),
        (
            Some(MhAssignment {
                webtransport_endpoint: String::new(),
                ..assignment.clone()
            }),
            MalformedGrant::Assignment(AssignmentRejected::EmptyWebtransportEndpoint),
        ),
        (
            Some(MhAssignment {
                grpc_endpoint: String::new(),
                ..assignment.clone()
            }),
            MalformedGrant::Assignment(AssignmentRejected::EmptyGrpcEndpoint),
        ),
    ] {
        *reply.lock().unwrap() =
            AcquireReply::Ok(AcquireMediaHandlerResponse { assignment: broken });
        match gc_client.acquire(&call).await {
            AcquireResult::Rejected {
                reason: AcquireRejection::Malformed(m),
                ..
            } => assert_eq!(m, want),
            other => panic!("{want:?}: expected a malformed rejection, got {other:?}"),
        }
    }

    // Status classes, code and message kept.
    for code in [
        tonic::Code::ResourceExhausted,
        tonic::Code::FailedPrecondition,
        tonic::Code::InvalidArgument,
        tonic::Code::Unavailable,
        tonic::Code::Internal,
        tonic::Code::Unimplemented,
        tonic::Code::Unauthenticated,
        tonic::Code::PermissionDenied,
    ] {
        *reply.lock().unwrap() = AcquireReply::Status(code);
        let result = gc_client.acquire(&call).await;
        match (code, result) {
            (tonic::Code::ResourceExhausted, AcquireResult::NoCapacity { message }) => {
                assert!(message.contains("mock"));
            }
            (
                tonic::Code::FailedPrecondition,
                AcquireResult::Rejected {
                    reason: AcquireRejection::NotOwner,
                    code: got,
                    message,
                },
            )
            | (
                tonic::Code::InvalidArgument,
                AcquireResult::Rejected {
                    reason: AcquireRejection::InvalidRequest,
                    code: got,
                    message,
                },
            ) => {
                assert_eq!(got, code);
                assert!(message.contains("mock"));
            }
            (_, AcquireResult::Unavailable { code: got, message }) => {
                assert_eq!(got, code, "retryable class keeps its code");
                assert!(message.contains("mock"));
            }
            (code, other) => panic!("{code:?}: unexpected class {other:?}"),
        }
    }
    cancel_token.cancel();
}

/// Story 4 MC-C (operations): a GC that accepts the call but never answers
/// resolves through the channel's RPC timeout as a RETRYABLE failure — the
/// meeting's one in-flight acquisition can never wedge. Paused time after the
/// connection is up, so the 10 s timeout costs nothing.
#[tokio::test(flavor = "current_thread")]
async fn a_hung_gc_times_out_into_a_retryable_failure() {
    use mc_service::grpc::gc_client::{AcquireCall, AcquireResult, HandlerAcquirer};
    use mc_service::media_routing::RemovalTrigger;

    let mock_gc = MockGcServer::accepting();
    let reply = Arc::clone(&mock_gc.acquire_reply);
    let requests = Arc::clone(&mock_gc.acquire_requests);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{addr}");
    let gc_client = GcClient::new(gc_url.clone(), test_token_receiver(), test_config(&gc_url))
        .await
        .unwrap();
    gc_client.register().await.unwrap(); // the connection is up
    *reply.lock().unwrap() = AcquireReply::Hang;
    tokio::time::pause();
    let started = tokio::time::Instant::now();
    let result = gc_client
        .acquire(&AcquireCall {
            meeting_id: "m-hung".to_string(),
            exclude: Default::default(),
            trigger: RemovalTrigger::HeartbeatTimeout,
            replaced: None,
        })
        .await;
    assert!(
        matches!(result, AcquireResult::Unavailable { .. }),
        "a timeout is retryable: {result:?}"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(10),
        "bounded by the RPC timeout"
    );
    assert_eq!(requests.lock().unwrap().len(), 1, "the request reached GC");
    cancel_token.cancel();
}

// ============================================================================
// Heartbeat Tests
// ============================================================================

#[tokio::test]
async fn test_gc_client_fast_heartbeat() {
    let (heartbeat_tx, mut heartbeat_rx) = mpsc::channel(1);
    let mock_gc = MockGcServer::accepting().with_fast_heartbeat_channel(heartbeat_tx);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    // Send a fast heartbeat
    gc_client
        .fast_heartbeat(
            5,
            50,
            proto_gen::dark_tower::internal::v1::HealthStatus::Healthy,
        )
        .await
        .unwrap();

    let request = heartbeat_rx.recv().await.unwrap();
    assert_eq!(request.controller_id, config.mc_id);
    let capacity = request.capacity.unwrap();
    assert_eq!(capacity.current_meetings, 5);
    assert_eq!(capacity.current_participants, 50);

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_comprehensive_heartbeat() {
    let (heartbeat_tx, mut heartbeat_rx) = mpsc::channel(1);
    let mock_gc = MockGcServer::accepting().with_comprehensive_heartbeat_channel(heartbeat_tx);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    // Send a comprehensive heartbeat
    gc_client
        .comprehensive_heartbeat(
            10,
            100,
            proto_gen::dark_tower::internal::v1::HealthStatus::Healthy,
            45.5,
            60.0,
        )
        .await
        .unwrap();

    let request = heartbeat_rx.recv().await.unwrap();
    assert_eq!(request.controller_id, config.mc_id);
    let capacity = request.capacity.unwrap();
    assert_eq!(capacity.current_meetings, 10);
    assert_eq!(capacity.current_participants, 100);
    assert!((request.cpu_usage_percent - 45.5).abs() < 0.1);
    assert!((request.memory_usage_percent - 60.0).abs() < 0.1);

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_heartbeat_skipped_when_not_registered() {
    let mock_gc = MockGcServer::accepting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    // Don't register - heartbeat should be skipped
    assert!(!gc_client.is_registered());

    // These should succeed (skipped, not failed)
    gc_client
        .fast_heartbeat(
            5,
            50,
            proto_gen::dark_tower::internal::v1::HealthStatus::Healthy,
        )
        .await
        .unwrap();

    gc_client
        .comprehensive_heartbeat(
            10,
            100,
            proto_gen::dark_tower::internal::v1::HealthStatus::Healthy,
            45.5,
            60.0,
        )
        .await
        .unwrap();

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_heartbeat_intervals_from_gc() {
    let mock_gc = MockGcServer::accepting().with_heartbeat_intervals(5000, 15000);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    gc_client.register().await.unwrap();

    // Intervals should be updated from GC response
    assert_eq!(gc_client.fast_heartbeat_interval_ms(), 5000);
    assert_eq!(gc_client.comprehensive_heartbeat_interval_ms(), 15000);

    cancel_token.cancel();
}

// ============================================================================
// ControllerMetrics Tests
// ============================================================================

#[tokio::test]
async fn test_controller_metrics_concurrent_updates() {
    let metrics = ControllerMetrics::new();

    // Spawn multiple tasks updating concurrently
    let metrics_clone1 = Arc::clone(&metrics);
    let metrics_clone2 = Arc::clone(&metrics);
    let metrics_clone3 = Arc::clone(&metrics);

    let h1 = tokio::spawn(async move {
        for _ in 0..100 {
            metrics_clone1.increment_meetings();
            metrics_clone1.increment_participants();
        }
    });

    let h2 = tokio::spawn(async move {
        for _ in 0..50 {
            metrics_clone2.increment_meetings();
            metrics_clone2.decrement_meetings();
        }
    });

    let h3 = tokio::spawn(async move {
        for _ in 0..100 {
            metrics_clone3.increment_participants();
        }
    });

    h1.await.unwrap();
    h2.await.unwrap();
    h3.await.unwrap();

    // h1 added 100 meetings, h2 added/removed 50 (net 0) = 100 meetings
    assert_eq!(metrics.meetings(), 100);
    // h1 added 100 participants, h3 added 100 = 200 participants
    assert_eq!(metrics.participants(), 200);
}

// ============================================================================
// Actor Handle Tests
// ============================================================================

#[tokio::test]
async fn test_actor_handle_creation() {
    let _controller_handle = Arc::new(test_common::make_controller("mc-test"));

    // Controller should be created without error
    // Actual operation tests are in unit tests
}

// ============================================================================
// Re-registration Tests (Iteration 4)
// ============================================================================

#[tokio::test]
async fn test_heartbeat_not_found_detection() {
    // Test that heartbeat returns NotRegistered error when GC returns NOT_FOUND

    // Create a mock GC that returns NOT_FOUND for heartbeats
    let mock_gc = MockGcServer::new_with_behavior(MockBehavior::NotFound);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    // Mark as registered (bypass registration)
    gc_client.register().await.unwrap();

    // Fast heartbeat should detect NOT_FOUND and return NotRegistered error
    let result = gc_client.fast_heartbeat(5, 10, HealthStatus::Healthy).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, McError::NotRegistered),
        "Expected NotRegistered, got: {err:?}"
    );

    // After NOT_FOUND, is_registered should be false
    assert!(!gc_client.is_registered());

    cancel_token.cancel();
}

#[tokio::test]
async fn test_comprehensive_heartbeat_not_found_detection() {
    // Test that comprehensive heartbeat also detects NOT_FOUND

    let mock_gc = MockGcServer::new_with_behavior(MockBehavior::NotFound);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    gc_client.register().await.unwrap();

    // Comprehensive heartbeat should detect NOT_FOUND
    let result = gc_client
        .comprehensive_heartbeat(5, 10, HealthStatus::Healthy, 50.0, 75.0)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, McError::NotRegistered),
        "Expected NotRegistered, got: {err:?}"
    );

    assert!(!gc_client.is_registered());

    cancel_token.cancel();
}

#[tokio::test]
async fn test_attempt_reregistration_success() {
    // Test that attempt_reregistration works after NOT_FOUND

    let mock_gc = MockGcServer::new();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    // Don't call register() initially - simulate MC that lost registration

    // attempt_reregistration should succeed
    let result = gc_client.attempt_reregistration().await;
    assert!(result.is_ok(), "Re-registration should succeed");

    // Should be marked as registered
    assert!(gc_client.is_registered());

    // Subsequent heartbeats should work
    let result = gc_client.fast_heartbeat(5, 10, HealthStatus::Healthy).await;
    assert!(result.is_ok());

    cancel_token.cancel();
}

#[tokio::test]
async fn test_attempt_reregistration_after_not_found() {
    // Test the full flow: heartbeat gets NOT_FOUND, then re-registration

    let mock_gc = MockGcServer::new_with_behavior(MockBehavior::NotFoundThenAccept);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    // Initial registration
    gc_client.register().await.unwrap();
    assert!(gc_client.is_registered());

    // First heartbeat gets NOT_FOUND
    let result = gc_client.fast_heartbeat(5, 10, HealthStatus::Healthy).await;
    assert!(matches!(result.unwrap_err(), McError::NotRegistered));
    assert!(!gc_client.is_registered());

    // Re-registration should succeed (mock switches to Accept behavior)
    let result = gc_client.attempt_reregistration().await;
    assert!(result.is_ok());
    assert!(gc_client.is_registered());

    // Subsequent heartbeats should work
    let result = gc_client.fast_heartbeat(5, 10, HealthStatus::Healthy).await;
    assert!(result.is_ok());

    cancel_token.cancel();
}

// ============================================================================
// ADR-0032 Step 3 §Cluster F: per-failure-class fidelity for
// `mc_gc_heartbeats_total` and `mc_gc_heartbeat_latency_seconds`.
//
// Each `#[tokio::test(flavor = "current_thread")]` below is pinned EXPLICITLY
// — `MetricAssertion` binds a per-thread recorder, and the `GcClient`
// heartbeat future runs on the test's task. On `current_thread` that's the
// test thread and emissions are captured. See
// `crates/common/src/observability/testing.rs:60-72`.
//
// Histogram-first ordering (drain-on-read) and `assert_delta(0)` adjacency
// across all four (status, type) combos per ADR-0032 §Pattern #3.
// ============================================================================

#[tokio::test(flavor = "current_thread")]
async fn gc_fast_heartbeat_success_records_status_success_type_fast() {
    use ::common::observability::testing::MetricAssertion;

    let mock_gc = MockGcServer::accepting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();
    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();
    gc_client.register().await.unwrap();

    let snap = MetricAssertion::snapshot();
    gc_client
        .fast_heartbeat(1, 1, HealthStatus::Healthy)
        .await
        .unwrap();

    // Histogram first.
    snap.histogram("mc_gc_heartbeat_latency_seconds")
        .with_labels(&[("heartbeat_type", "fast")])
        .assert_observation_count_at_least(1);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "success"), ("heartbeat_type", "fast")])
        .assert_delta(1);
    // Adjacency on the other 3 combos.
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "error"), ("heartbeat_type", "fast")])
        .assert_delta(0);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "success"), ("heartbeat_type", "comprehensive")])
        .assert_delta(0);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "error"), ("heartbeat_type", "comprehensive")])
        .assert_delta(0);

    cancel_token.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn gc_comprehensive_heartbeat_success_records_status_success_type_comprehensive() {
    use ::common::observability::testing::MetricAssertion;

    let mock_gc = MockGcServer::accepting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();
    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();
    gc_client.register().await.unwrap();

    let snap = MetricAssertion::snapshot();
    gc_client
        .comprehensive_heartbeat(1, 1, HealthStatus::Healthy, 10.0, 20.0)
        .await
        .unwrap();

    snap.histogram("mc_gc_heartbeat_latency_seconds")
        .with_labels(&[("heartbeat_type", "comprehensive")])
        .assert_observation_count_at_least(1);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "success"), ("heartbeat_type", "comprehensive")])
        .assert_delta(1);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "success"), ("heartbeat_type", "fast")])
        .assert_delta(0);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "error"), ("heartbeat_type", "comprehensive")])
        .assert_delta(0);

    cancel_token.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn gc_fast_heartbeat_error_records_status_error_type_fast() {
    use ::common::observability::testing::MetricAssertion;

    // Cancel the server BEFORE the heartbeat → transport failure → error path
    // at `gc_client.rs:376` fires `record_gc_heartbeat("error", "fast")`.
    let mock_gc = MockGcServer::accepting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();
    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();
    gc_client.register().await.unwrap();

    cancel_token.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let snap = MetricAssertion::snapshot();
    let result = gc_client.fast_heartbeat(1, 1, HealthStatus::Healthy).await;
    assert!(result.is_err(), "expected Err after server cancel");

    snap.histogram("mc_gc_heartbeat_latency_seconds")
        .with_labels(&[("heartbeat_type", "fast")])
        .assert_observation_count_at_least(1);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "error"), ("heartbeat_type", "fast")])
        .assert_delta(1);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "success"), ("heartbeat_type", "fast")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn gc_comprehensive_heartbeat_error_records_status_error_type_comprehensive() {
    use ::common::observability::testing::MetricAssertion;

    let mock_gc = MockGcServer::accepting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{}", addr);
    let config = test_config(&gc_url);
    let token_rx = test_token_receiver();
    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();
    gc_client.register().await.unwrap();

    cancel_token.cancel();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let snap = MetricAssertion::snapshot();
    let result = gc_client
        .comprehensive_heartbeat(1, 1, HealthStatus::Healthy, 10.0, 20.0)
        .await;
    assert!(result.is_err(), "expected Err after server cancel");

    snap.histogram("mc_gc_heartbeat_latency_seconds")
        .with_labels(&[("heartbeat_type", "comprehensive")])
        .assert_observation_count_at_least(1);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "error"), ("heartbeat_type", "comprehensive")])
        .assert_delta(1);
    snap.counter("mc_gc_heartbeats_total")
        .with_labels(&[("status", "error"), ("heartbeat_type", "fast")])
        .assert_delta(0);
}

// ============================================================================
// Meeting-ended notification to GC (fenced by assignment_id, so retried):
// `mc_gc_notify_meeting_ended_total{outcome}`, the pending gauges and
// `mc_gc_meeting_ended_notifications_dropped_total`.
//
// `current_thread` for the same reason as the heartbeat block above: the
// notify future runs on the test's task, so the thread-local recorder sees it.
// ============================================================================

use ::common::meeting_lifecycle::AssignmentId;
use mc_service::grpc::gc_client::{
    drain_meeting_ended, EndedMeeting, MeetingEndedQueue, NotifyAttempt,
};

fn ended(meeting_id: &str) -> EndedMeeting {
    EndedMeeting {
        meeting_id: meeting_id.to_string(),
        assignment_id: AssignmentId::new_v4(),
        enqueued_at: tokio::time::Instant::now(),
    }
}

async fn connected_client(mock_gc: MockGcServer) -> (Arc<GcClient>, CancellationToken) {
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
    let gc_url = format!("http://{addr}");
    let gc_client = GcClient::new(gc_url.clone(), test_token_receiver(), test_config(&gc_url))
        .await
        .unwrap();
    (Arc::new(gc_client), cancel_token)
}

/// The request names this MC and the incarnation: GC's fence matches on both.
#[tokio::test(flavor = "current_thread")]
async fn a_notify_carries_the_controller_id_and_the_assignment_id() {
    use ::common::observability::testing::MetricAssertion;

    let mock_gc = MockGcServer::accepting();
    let requests = Arc::clone(&mock_gc.notify_requests);
    let (gc_client, cancel_token) = connected_client(mock_gc).await;
    let e = ended("meeting-ended-ok");

    let snap = MetricAssertion::snapshot();
    assert_eq!(
        gc_client.notify_meeting_ended(&e).await,
        NotifyAttempt::Acked
    );

    let requests = requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meeting_id, "meeting-ended-ok");
    assert_eq!(requests[0].controller_id, "mc-test-001");
    assert_eq!(requests[0].region, "us-east-1");
    assert_eq!(
        AssignmentId::parse(&requests[0].assignment_id),
        Ok(e.assignment_id)
    );
    snap.counter("mc_gc_notify_meeting_ended_total")
        .with_labels(&[("outcome", "acked")])
        .assert_delta(1);
    cancel_token.cancel();
}

/// NOT_CURRENT is a success (it also covers "my earlier attempt committed"):
/// acked, never retried.
#[tokio::test(flavor = "current_thread")]
async fn a_not_current_answer_is_acked_not_retried() {
    use ::common::observability::testing::MetricAssertion;

    let mock_gc =
        MockGcServer::accepting().with_notify_outcome(NotifyMeetingEndedOutcome::NotCurrent);
    let (gc_client, cancel_token) = connected_client(mock_gc).await;
    let snap = MetricAssertion::snapshot();
    assert_eq!(
        gc_client.notify_meeting_ended(&ended("m-stale")).await,
        NotifyAttempt::Acked
    );
    snap.counter("mc_gc_notify_meeting_ended_total")
        .with_labels(&[("outcome", "acked")])
        .assert_delta(1);
    cancel_token.cancel();
}

/// The two defects are final: `INVALID_ARGUMENT` and an UNSPECIFIED outcome
/// are counted `rejected` and the drain makes EXACTLY ONE attempt for them.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn invalid_argument_and_unspecified_are_rejected_after_exactly_one_attempt() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;

    for mock_gc in [
        MockGcServer::accepting()
            .with_notify_meeting_ended_failure(tonic::Code::InvalidArgument, u32::MAX),
        MockGcServer::accepting().with_notify_outcome(NotifyMeetingEndedOutcome::Unspecified),
    ] {
        let calls = Arc::clone(&mock_gc.notify_meeting_ended_calls);
        let (gc_client, cancel_token) = connected_client(mock_gc).await;
        let (queue, rx) = MeetingEndedQueue::with_capacity(4);
        let notify_token = CancellationToken::new();
        let drain = tokio::spawn(drain_meeting_ended(gc_client, rx, notify_token.clone()));

        let snap = MetricAssertion::snapshot();
        queue.meeting_ended("m-defect", AssignmentId::new_v4());
        // Far past every backoff a retry would have used.
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a defect is never retried");
        snap.counter("mc_gc_notify_meeting_ended_total")
            .with_labels(&[("outcome", "rejected")])
            .assert_delta(1);
        snap.counter("mc_gc_notify_meeting_ended_total")
            .with_labels(&[("outcome", "retried")])
            .assert_delta(0);
        notify_token.cancel();
        drain.await.unwrap();
        cancel_token.cancel();
    }
}

/// Retried with NO attempt cap: GC fails more times than the old six-attempt
/// budget ever allowed, then the notify still gets through. Paused time; the
/// capped backoff is what keeps the retries spaced.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_ambiguous_failure_is_retried_past_any_old_cap_until_acked() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;

    const FAILURES: u32 = 12;
    let mock_gc = MockGcServer::accepting()
        .with_notify_meeting_ended_failure(tonic::Code::Unavailable, FAILURES);
    let calls = Arc::clone(&mock_gc.notify_meeting_ended_calls);
    let (gc_client, cancel_token) = connected_client(mock_gc).await;
    let (queue, rx) = MeetingEndedQueue::with_capacity(4);
    let notify_token = CancellationToken::new();
    let drain = tokio::spawn(drain_meeting_ended(gc_client, rx, notify_token.clone()));

    let snap = MetricAssertion::snapshot();
    queue.meeting_ended("m-flaky", AssignmentId::new_v4());
    while calls.load(Ordering::SeqCst) < FAILURES + 1 {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    tokio::time::sleep(Duration::from_secs(60)).await;

    assert_eq!(
        calls.load(Ordering::SeqCst),
        FAILURES + 1,
        "no attempt after the ack"
    );
    snap.counter("mc_gc_notify_meeting_ended_total")
        .with_labels(&[("outcome", "retried")])
        .assert_delta(u64::from(FAILURES));
    snap.counter("mc_gc_notify_meeting_ended_total")
        .with_labels(&[("outcome", "acked")])
        .assert_delta(1);
    snap.gauge("mc_gc_notify_meeting_ended_pending")
        .assert_value(0.0);
    snap.gauge("mc_gc_notify_meeting_ended_oldest_pending_age_seconds")
        .assert_value(0.0);
    notify_token.cancel();
    drain.await.unwrap();
    cancel_token.cancel();
}

/// Head-of-line: a notification GC keeps failing does NOT delay a later one,
/// and the oldest-pending age tracks the stuck entry while the other acks.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_stuck_notify_does_not_delay_another_and_the_age_tracks_the_oldest() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;

    let mock_gc = MockGcServer::accepting().with_notify_always_failing_for("m-stuck");
    let requests = Arc::clone(&mock_gc.notify_requests);
    let (gc_client, cancel_token) = connected_client(mock_gc).await;
    let (queue, rx) = MeetingEndedQueue::with_capacity(4);
    let notify_token = CancellationToken::new();
    let drain = tokio::spawn(drain_meeting_ended(gc_client, rx, notify_token.clone()));

    let snap = MetricAssertion::snapshot();
    queue.meeting_ended("m-stuck", AssignmentId::new_v4());
    // Let the stuck entry fail into its backoff ladder.
    tokio::time::sleep(Duration::from_secs(100)).await;
    queue.meeting_ended("m-fine", AssignmentId::new_v4());
    let queued_fine = tokio::time::Instant::now();
    while !requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.meeting_id == "m-fine")
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        queued_fine.elapsed() < Duration::from_secs(1),
        "the healthy notify must not wait behind the stuck one's backoff; waited {:?}",
        queued_fine.elapsed()
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    snap.gauge("mc_gc_notify_meeting_ended_pending")
        .assert_value(1.0);
    snap.gauge("mc_gc_notify_meeting_ended_oldest_pending_age_seconds")
        .assert_value_in_range(100.0..=103.0);

    notify_token.cancel();
    drain.await.unwrap();
    cancel_token.cancel();
}

/// A full queue drops the notification on its own counter, never silently and
/// never blocking the caller (the controller loop).
#[tokio::test(flavor = "current_thread")]
async fn a_full_meeting_ended_queue_counts_the_drop_and_keeps_what_it_holds() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;

    let (queue, mut rx) = MeetingEndedQueue::with_capacity(1);
    let first = AssignmentId::new_v4();

    let snap = MetricAssertion::snapshot();
    queue.meeting_ended("first", first);
    snap.counter("mc_gc_meeting_ended_notifications_dropped_total")
        .assert_delta(0);

    let snap = MetricAssertion::snapshot();
    queue.meeting_ended("second", AssignmentId::new_v4());
    snap.counter("mc_gc_meeting_ended_notifications_dropped_total")
        .assert_delta(1);

    let got = rx.recv().await.expect("first is queued");
    assert_eq!(
        (got.meeting_id.as_str(), got.assignment_id),
        ("first", first)
    );
    assert!(rx.try_recv().is_err(), "the dropped id must not be queued");
}

/// Shutdown never loses a notification silently: one produced AFTER the queue
/// is closed is counted as dropped.
#[tokio::test(flavor = "current_thread")]
async fn a_notification_produced_after_the_shutdown_close_is_counted_as_dropped() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;

    let (gc_client, cancel_token) = connected_client(MockGcServer::accepting()).await;
    let (queue, rx) = MeetingEndedQueue::with_capacity(4);
    let cancel = CancellationToken::new();
    cancel.cancel();
    drain_meeting_ended(gc_client, rx, cancel).await;

    let snap = MetricAssertion::snapshot();
    queue.meeting_ended("after-close", AssignmentId::new_v4());
    snap.counter("mc_gc_meeting_ended_notifications_dropped_total")
        .assert_delta(1);
    cancel_token.cancel();
}

/// The shutdown flush ends within its budget even while GC keeps failing:
/// what never got through is counted as dropped, once each.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn shutdown_ends_the_retries_within_the_flush_budget_and_counts_the_remainder() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;
    use mc_service::media_routing::teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET;

    let mock_gc = MockGcServer::accepting()
        .with_notify_meeting_ended_failure(tonic::Code::Unavailable, u32::MAX);
    let (gc_client, cancel_token) = connected_client(mock_gc).await;
    let (queue, rx) = MeetingEndedQueue::with_capacity(4);
    queue.meeting_ended("never-1", AssignmentId::new_v4());
    queue.meeting_ended("never-2", AssignmentId::new_v4());

    let cancel = CancellationToken::new();
    cancel.cancel();
    let snap = MetricAssertion::snapshot();
    let started = tokio::time::Instant::now();
    drain_meeting_ended(gc_client, rx, cancel).await;

    assert!(
        started.elapsed() <= SHUTDOWN_NOTIFY_FLUSH_BUDGET,
        "the flush must end inside its budget; took {:?}",
        started.elapsed()
    );
    snap.counter("mc_gc_meeting_ended_notifications_dropped_total")
        .assert_delta(2);
    snap.counter("mc_gc_notify_meeting_ended_total")
        .with_labels(&[("outcome", "acked")])
        .assert_delta(0);
    cancel_token.cancel();
}

/// OPS-26: a notification produced AFTER shutdown begins — the ordinary case,
/// since a teardown completing inside the shutdown release window calls
/// `meeting_ended` at that moment — is SENT, not dropped.
#[tokio::test(flavor = "current_thread")]
async fn a_notification_queued_during_the_release_window_is_flushed_not_dropped() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;

    let mock_gc = MockGcServer::accepting();
    let calls = Arc::clone(&mock_gc.notify_meeting_ended_calls);
    let (gc_client, cancel_token) = connected_client(mock_gc).await;
    let (queue, rx) = MeetingEndedQueue::with_capacity(4);

    let notify_token = CancellationToken::new();
    let drain = tokio::spawn(drain_meeting_ended(
        Arc::clone(&gc_client),
        rx,
        notify_token.clone(),
    ));
    queue.meeting_ended("settled-during-window", AssignmentId::new_v4());

    let snap = MetricAssertion::snapshot();
    notify_token.cancel();
    drain.await.unwrap();

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the notification produced during the release window must reach GC"
    );
    snap.counter("mc_gc_notify_meeting_ended_total")
        .with_labels(&[("outcome", "acked")])
        .assert_delta(1);
    snap.counter("mc_gc_meeting_ended_notifications_dropped_total")
        .assert_delta(0);
    cancel_token.cancel();
}

/// The oldest-pending age stays FRESH while a stuck entry waits out its
/// (30 s) backoff: the drain republishes every `NOTIFY_PENDING_PUBLISH_INTERVAL`,
/// not only on events — so `MCNotifyMeetingEndedStuck` reads the true age.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_oldest_pending_age_tracks_a_stuck_entry_between_attempts() {
    use ::common::observability::testing::MetricAssertion;
    use mc_service::actors::controller::MeetingEndedNotifier;
    use mc_service::grpc::gc_client::NOTIFY_PENDING_PUBLISH_INTERVAL;

    let mock_gc = MockGcServer::accepting().with_notify_always_failing_for("m-stuck");
    let (gc_client, cancel_token) = connected_client(mock_gc).await;
    let (queue, rx) = MeetingEndedQueue::with_capacity(4);
    let notify_token = CancellationToken::new();
    let drain = tokio::spawn(drain_meeting_ended(gc_client, rx, notify_token.clone()));

    let queued = tokio::time::Instant::now();
    queue.meeting_ended("m-stuck", AssignmentId::new_v4());
    // Into the capped (30 s) part of the backoff ladder.
    tokio::time::sleep(Duration::from_secs(100)).await;
    let tolerance = NOTIFY_PENDING_PUBLISH_INTERVAL.as_secs_f64() + 1.0;
    for _ in 0..9 {
        // Snapshot BEFORE the step: only sets inside the step are observed, so
        // each window proves a fresh publish (not a value left from earlier).
        let snap = MetricAssertion::snapshot();
        tokio::time::sleep(Duration::from_secs(5)).await;
        let age = queued.elapsed().as_secs_f64();
        snap.gauge("mc_gc_notify_meeting_ended_oldest_pending_age_seconds")
            .assert_value_in_range((age - tolerance)..=age);
    }
    notify_token.cancel();
    drain.await.unwrap();
    cancel_token.cancel();
}
