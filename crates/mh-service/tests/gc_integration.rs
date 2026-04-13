//! Integration tests for MH-GC communication.
//!
//! Tests the registration, heartbeat, and re-registration flows between
//! Media Handler and Global Controller.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use mh_service::config::Config;
use mh_service::errors::MhError;
use mh_service::grpc::GcClient;

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use proto_gen::internal::media_handler_registry_service_server::{
    MediaHandlerRegistryService, MediaHandlerRegistryServiceServer,
};
use proto_gen::internal::{
    MhLoadReportRequest, MhLoadReportResponse, RegisterMhRequest, RegisterMhResponse,
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

// ============================================================================
// Mock GC Server
// ============================================================================

/// Behavior mode for the mock GC server.
#[derive(Debug, Clone, Copy)]
enum MockBehavior {
    /// Accept all requests normally.
    Accept,
    /// Reject registrations.
    Reject,
    /// Return NOT_FOUND for load reports (simulates MH not registered).
    NotFound,
}

/// Mock GC server for testing MH registration and load reports.
struct MockGcServer {
    behavior: MockBehavior,
    load_report_interval_ms: u64,
    registration_count: AtomicU32,
    load_report_count: AtomicU32,
    registration_tx: Option<mpsc::Sender<RegisterMhRequest>>,
    load_report_tx: Option<mpsc::Sender<MhLoadReportRequest>>,
}

impl MockGcServer {
    fn new() -> Self {
        Self {
            behavior: MockBehavior::Accept,
            load_report_interval_ms: 10_000,
            registration_count: AtomicU32::new(0),
            load_report_count: AtomicU32::new(0),
            registration_tx: None,
            load_report_tx: None,
        }
    }

    fn accepting() -> Self {
        Self::new()
    }

    fn rejecting() -> Self {
        Self {
            behavior: MockBehavior::Reject,
            ..Self::new()
        }
    }

    fn not_found() -> Self {
        Self {
            behavior: MockBehavior::NotFound,
            ..Self::new()
        }
    }

    fn with_registration_channel(mut self, tx: mpsc::Sender<RegisterMhRequest>) -> Self {
        self.registration_tx = Some(tx);
        self
    }

    fn with_load_report_channel(mut self, tx: mpsc::Sender<MhLoadReportRequest>) -> Self {
        self.load_report_tx = Some(tx);
        self
    }

    fn with_load_report_interval(mut self, interval_ms: u64) -> Self {
        self.load_report_interval_ms = interval_ms;
        self
    }
}

#[tonic::async_trait]
impl MediaHandlerRegistryService for MockGcServer {
    async fn register_mh(
        &self,
        request: Request<RegisterMhRequest>,
    ) -> Result<Response<RegisterMhResponse>, Status> {
        let inner = request.into_inner();
        self.registration_count.fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = &self.registration_tx {
            let _ = tx.send(inner.clone()).await;
        }

        match self.behavior {
            MockBehavior::Accept | MockBehavior::NotFound => {
                Ok(Response::new(RegisterMhResponse {
                    accepted: true,
                    message: "Registration accepted".to_string(),
                    load_report_interval_ms: self.load_report_interval_ms,
                }))
            }
            MockBehavior::Reject => Ok(Response::new(RegisterMhResponse {
                accepted: false,
                message: "Registration rejected by mock".to_string(),
                load_report_interval_ms: 0,
            })),
        }
    }

    async fn send_load_report(
        &self,
        request: Request<MhLoadReportRequest>,
    ) -> Result<Response<MhLoadReportResponse>, Status> {
        let inner = request.into_inner();
        self.load_report_count.fetch_add(1, Ordering::SeqCst);

        if let Some(tx) = &self.load_report_tx {
            let _ = tx.send(inner).await;
        }

        match self.behavior {
            MockBehavior::NotFound => Err(Status::not_found("MH not registered with GC")),
            MockBehavior::Accept | MockBehavior::Reject => {
                Ok(Response::new(MhLoadReportResponse {
                    acknowledged: true,
                    timestamp: 1_700_000_000,
                }))
            }
        }
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

fn test_config(gc_url: &str) -> Config {
    Config {
        grpc_bind_address: "0.0.0.0:50053".to_string(),
        health_bind_address: "0.0.0.0:8083".to_string(),
        webtransport_bind_address: "0.0.0.0:4434".to_string(),
        region: "us-east-1".to_string(),
        gc_grpc_url: gc_url.to_string(),
        handler_id: "mh-test-001".to_string(),
        max_streams: 500,
        ac_endpoint: "https://ac.example.com".to_string(),
        client_id: "media-handler".to_string(),
        client_secret: SecretString::from("test-client-secret"),
        tls_cert_path: "/dev/null".to_string(),
        tls_key_path: "/dev/null".to_string(),
        grpc_advertise_address: "grpc://localhost:50053".to_string(),
        webtransport_advertise_address: "https://localhost:4434".to_string(),
        ac_jwks_url: "http://localhost:8082/.well-known/jwks.json".to_string(),
        register_meeting_timeout_seconds: 15,
        max_connections: 10_000,
    }
}

/// Create a mock `TokenReceiver` for testing.
fn mock_token_receiver() -> TokenReceiver {
    use std::sync::OnceLock;

    static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();

    let sender = TOKEN_SENDER.get_or_init(|| {
        let (tx, _rx) = watch::channel(SecretString::from("test-service-token"));
        tx
    });

    TokenReceiver::from_watch_receiver(sender.subscribe())
}

async fn start_mock_gc_server(mock_gc: MockGcServer) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancel_token = CancellationToken::new();
    let cancel_token_clone = cancel_token.clone();

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let server = Server::builder()
        .add_service(MediaHandlerRegistryServiceServer::new(mock_gc))
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

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = mock_token_receiver();

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

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = mock_token_receiver();

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

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = mock_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    let request = registration_rx.recv().await.unwrap();
    assert_eq!(request.handler_id, config.handler_id);
    assert_eq!(request.region, config.region);
    assert_eq!(request.max_streams, config.max_streams);
    assert_eq!(request.grpc_endpoint, config.grpc_advertise_address);
    assert_eq!(
        request.webtransport_endpoint,
        config.webtransport_advertise_address
    );

    cancel_token.cancel();
}

// ============================================================================
// Load Report Tests
// ============================================================================

#[tokio::test]
async fn test_gc_client_load_report_success() {
    let (load_report_tx, mut load_report_rx) = mpsc::channel(1);
    let mock_gc = MockGcServer::accepting().with_load_report_channel(load_report_tx);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = mock_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config.clone())
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    gc_client.send_load_report().await.unwrap();

    let request = load_report_rx.recv().await.unwrap();
    assert_eq!(request.handler_id, config.handler_id);
    assert_eq!(request.health, 1); // HEALTHY

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_load_report_skipped_when_not_registered() {
    let mock_gc = MockGcServer::accepting();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = mock_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    // Don't register - load report should be silently skipped
    assert!(!gc_client.is_registered());

    // Should succeed (skipped, not failed)
    gc_client.send_load_report().await.unwrap();

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_load_report_not_found_clears_registration() {
    // First register with an accepting server, then switch to NOT_FOUND
    // We use NOT_FOUND mock directly and manually set is_registered via register()
    let mock_gc = MockGcServer::not_found();
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = mock_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    // Register (mock accepts registrations even in NotFound mode)
    gc_client.register().await.unwrap();
    assert!(gc_client.is_registered());

    // Load report should get NOT_FOUND and return NotRegistered
    let result = gc_client.send_load_report().await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, MhError::NotRegistered),
        "Expected NotRegistered, got: {err:?}"
    );

    // is_registered should be cleared
    assert!(!gc_client.is_registered());

    cancel_token.cancel();
}

#[tokio::test]
async fn test_gc_client_load_report_interval_from_gc() {
    let mock_gc = MockGcServer::accepting().with_load_report_interval(5000);
    let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;

    let gc_url = format!("http://{addr}");
    let config = test_config(&gc_url);
    let token_rx = mock_token_receiver();

    let gc_client = GcClient::new(gc_url, token_rx, config).await.unwrap();

    gc_client.register().await.unwrap();

    // Interval should be updated from GC response
    assert_eq!(gc_client.load_report_interval_ms(), 5000);

    cancel_token.cancel();
}
