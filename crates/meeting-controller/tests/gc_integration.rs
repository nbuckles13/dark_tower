//! Integration tests for MC-GC communication.
//!
//! Tests the registration, heartbeat, and assignment flows between
//! Meeting Controller and Global Controller.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use meeting_controller::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use meeting_controller::config::Config;
use meeting_controller::grpc::GcClient;

use common::secret::{SecretBox, SecretString};
use proto_gen::internal::global_controller_service_server::{
    GlobalControllerService, GlobalControllerServiceServer,
};
use proto_gen::internal::{
    ComprehensiveHeartbeatRequest, FastHeartbeatRequest, HeartbeatResponse,
    NotifyMeetingEndedRequest, NotifyMeetingEndedResponse, RegisterMcRequest, RegisterMcResponse,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

// ============================================================================
// Mock GC Server
// ============================================================================

/// Mock GC server for testing MC registration and heartbeats.
struct MockGcServer {
    /// Whether to accept registrations.
    accept_registration: bool,
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
}

impl MockGcServer {
    fn accepting() -> Self {
        Self {
            accept_registration: true,
            fast_heartbeat_interval_ms: 10_000,
            comprehensive_heartbeat_interval_ms: 30_000,
            registration_count: AtomicU32::new(0),
            fast_heartbeat_count: AtomicU32::new(0),
            comprehensive_heartbeat_count: AtomicU32::new(0),
            registration_tx: None,
            fast_heartbeat_tx: None,
            comprehensive_heartbeat_tx: None,
        }
    }

    fn rejecting() -> Self {
        Self {
            accept_registration: false,
            ..Self::accepting()
        }
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

        if self.accept_registration {
            Ok(Response::new(RegisterMcResponse {
                accepted: true,
                message: "Registration accepted".to_string(),
                fast_heartbeat_interval_ms: self.fast_heartbeat_interval_ms,
                comprehensive_heartbeat_interval_ms: self.comprehensive_heartbeat_interval_ms,
            }))
        } else {
            Ok(Response::new(RegisterMcResponse {
                accepted: false,
                message: "Registration rejected by mock".to_string(),
                fast_heartbeat_interval_ms: 0,
                comprehensive_heartbeat_interval_ms: 0,
            }))
        }
    }

    async fn fast_heartbeat(
        &self,
        request: Request<FastHeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let inner = request.into_inner();
        self.fast_heartbeat_count.fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = &self.fast_heartbeat_tx {
            let _ = tx.send(inner).await;
        }

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            timestamp: chrono::Utc::now().timestamp() as u64,
        }))
    }

    async fn comprehensive_heartbeat(
        &self,
        request: Request<ComprehensiveHeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let inner = request.into_inner();
        self.comprehensive_heartbeat_count
            .fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = &self.comprehensive_heartbeat_tx {
            let _ = tx.send(inner).await;
        }

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            timestamp: chrono::Utc::now().timestamp() as u64,
        }))
    }

    async fn notify_meeting_ended(
        &self,
        _request: Request<NotifyMeetingEndedRequest>,
    ) -> Result<Response<NotifyMeetingEndedResponse>, Status> {
        Ok(Response::new(NotifyMeetingEndedResponse {
            acknowledged: true,
        }))
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

fn test_config(gc_url: &str) -> Config {
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
        binding_token_ttl_seconds: 30,
        clock_skew_seconds: 5,
        nonce_grace_window_seconds: 5,
        disconnect_grace_period_seconds: 30,
        binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
        service_token: SecretString::from("test-service-token"),
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

    let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config)
        .await
        .unwrap();

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

    let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config)
        .await
        .unwrap();

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

    let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    let request = registration_rx.recv().await.unwrap();
    assert_eq!(request.id, config.mc_id);
    assert_eq!(request.region, config.region);
    assert_eq!(request.max_meetings, config.max_meetings);
    assert_eq!(request.max_participants, config.max_participants);

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

    let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    // Send a fast heartbeat
    gc_client
        .fast_heartbeat(5, 50, proto_gen::internal::HealthStatus::Healthy)
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

    let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    // Send a comprehensive heartbeat
    gc_client
        .comprehensive_heartbeat(
            10,
            100,
            proto_gen::internal::HealthStatus::Healthy,
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

    let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config)
        .await
        .unwrap();

    // Don't register - heartbeat should be skipped
    assert!(!gc_client.is_registered());

    // These should succeed (skipped, not failed)
    gc_client
        .fast_heartbeat(5, 50, proto_gen::internal::HealthStatus::Healthy)
        .await
        .unwrap();

    gc_client
        .comprehensive_heartbeat(
            10,
            100,
            proto_gen::internal::HealthStatus::Healthy,
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

    let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config)
        .await
        .unwrap();

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
    let actor_metrics = ActorMetrics::new();
    let master_secret = SecretBox::new(Box::new(vec![0u8; 32]));
    let _controller_handle = Arc::new(MeetingControllerActorHandle::new(
        "mc-test".to_string(),
        Arc::clone(&actor_metrics),
        master_secret,
    ));

    // Controller should be created without error
    // Actual operation tests are in unit tests
}
