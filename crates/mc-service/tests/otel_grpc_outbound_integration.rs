//! Integration test: outbound gRPC trace injection (R-56, MC as client).
//!
//! MC's outbound clients (`GcClient` → GC, `MhClient` → MH) are each built with
//! `otel_grpc::client_interceptor()`. When a call is made from within an active
//! span, the outbound request MUST carry a `traceparent` metadata entry whose
//! trace-id matches that span's trace-id. When there is NO active recording
//! span, no `traceparent` is injected (the `BoundedTraceContextPropagator`
//! skips an invalid context).
//!
//! Both clients are exercised (paired-test must-resolve: "both outbound
//! clients"). The mock servers capture inbound metadata via a server-side
//! interceptor; the assertion is on what MC actually put on the wire.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use mc_service::config::Config;
use mc_service::grpc::{GcClient, MhClient};
use proto_gen::dark_tower::internal::v1::global_controller_service_server::{
    GlobalControllerService, GlobalControllerServiceServer,
};
use proto_gen::dark_tower::internal::v1::media_handler_service_server::{
    MediaHandlerService, MediaHandlerServiceServer,
};
use proto_gen::dark_tower::internal::v1::{
    ComprehensiveHeartbeatRequest, ComprehensiveHeartbeatResponse, FastHeartbeatRequest,
    FastHeartbeatResponse, NotifyMeetingEndedRequest, NotifyMeetingEndedResponse,
    RegisterMcRequest, RegisterMcResponse, RegisterMeetingRequest, RegisterMeetingResponse,
    RegisterRequest, RegisterResponse, RouteMediaRequest, RouteMediaResponse,
    StreamTelemetryRequest, StreamTelemetryResponse,
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use test_common::otel_capture::{install_test_propagator, known_traceparent, SpanCapture};

// ---------------------------------------------------------------------------
// Metadata-capturing server interceptor + mocks
// ---------------------------------------------------------------------------

type Captured = Arc<Mutex<Vec<Option<String>>>>;

// `tonic::Status` is a large `Err` variant, but the `Interceptor` closure
// signature is fixed by tonic — boxing it isn't possible here.
#[allow(clippy::result_large_err)]
fn capture_interceptor(store: Captured) -> impl tonic::service::Interceptor + Clone {
    move |req: Request<()>| -> Result<Request<()>, Status> {
        let tp = req
            .metadata()
            .get("traceparent")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        store.lock().expect("capture mutex poisoned").push(tp);
        Ok(req)
    }
}

#[derive(Default)]
struct MockGc;

#[tonic::async_trait]
impl GlobalControllerService for MockGc {
    async fn register_mc(
        &self,
        _request: Request<RegisterMcRequest>,
    ) -> Result<Response<RegisterMcResponse>, Status> {
        Ok(Response::new(RegisterMcResponse {
            accepted: true,
            message: "ok".to_string(),
            fast_heartbeat_interval_ms: 10_000,
            comprehensive_heartbeat_interval_ms: 30_000,
        }))
    }
    async fn fast_heartbeat(
        &self,
        _request: Request<FastHeartbeatRequest>,
    ) -> Result<Response<FastHeartbeatResponse>, Status> {
        Err(Status::unimplemented("not used"))
    }
    async fn comprehensive_heartbeat(
        &self,
        _request: Request<ComprehensiveHeartbeatRequest>,
    ) -> Result<Response<ComprehensiveHeartbeatResponse>, Status> {
        Err(Status::unimplemented("not used"))
    }
    async fn notify_meeting_ended(
        &self,
        _request: Request<NotifyMeetingEndedRequest>,
    ) -> Result<Response<NotifyMeetingEndedResponse>, Status> {
        Err(Status::unimplemented("not used"))
    }
}

#[derive(Default)]
struct MockMh;

#[tonic::async_trait]
impl MediaHandlerService for MockMh {
    async fn register(
        &self,
        _request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        Err(Status::unimplemented("not used"))
    }
    async fn register_meeting(
        &self,
        _request: Request<RegisterMeetingRequest>,
    ) -> Result<Response<RegisterMeetingResponse>, Status> {
        Ok(Response::new(RegisterMeetingResponse { accepted: true }))
    }
    async fn route_media(
        &self,
        _request: Request<RouteMediaRequest>,
    ) -> Result<Response<RouteMediaResponse>, Status> {
        Err(Status::unimplemented("not used"))
    }
    async fn stream_telemetry(
        &self,
        _request: Request<tonic::Streaming<StreamTelemetryRequest>>,
    ) -> Result<Response<StreamTelemetryResponse>, Status> {
        Err(Status::unimplemented("not used"))
    }
}

async fn start_gc_server(store: Captured) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let server = Server::builder()
        .add_service(GlobalControllerServiceServer::with_interceptor(
            MockGc,
            capture_interceptor(store),
        ))
        .serve_with_incoming_shutdown(incoming, async move { cancel_clone.cancelled().await });
    tokio::spawn(async move {
        let _ = server.await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, cancel)
}

async fn start_mh_server(store: Captured) -> (SocketAddr, CancellationToken) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let server = Server::builder()
        .add_service(MediaHandlerServiceServer::with_interceptor(
            MockMh,
            capture_interceptor(store),
        ))
        .serve_with_incoming_shutdown(incoming, async move { cancel_clone.cancelled().await });
    tokio::spawn(async move {
        let _ = server.await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, cancel)
}

fn mock_token_receiver() -> TokenReceiver {
    use std::sync::OnceLock;
    static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();
    let sender = TOKEN_SENDER.get_or_init(|| {
        let (tx, _rx) = watch::channel(SecretString::from("test-service-token"));
        tx
    });
    TokenReceiver::from_test_channel(sender.subscribe())
}

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
    }
}

/// Rebuild the fixed `traceparent`'s remote `Context` via the global
/// propagator, so a span parented to it produces the known trace-id.
fn known_remote_context() -> opentelemetry::Context {
    let mut carrier = HashMap::new();
    carrier.insert("traceparent".to_string(), known_traceparent());
    opentelemetry::global::get_text_map_propagator(|p| p.extract(&carrier))
}

fn expected_trace_id() -> String {
    known_traceparent()[3..35].to_string()
}

fn captured_traceparents(store: &Captured) -> Vec<Option<String>> {
    store.lock().expect("capture mutex poisoned").clone()
}

// ---------------------------------------------------------------------------
// GcClient
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_client_register_injects_active_span_traceparent() {
    install_test_propagator();
    let _capture = SpanCapture::install();

    let store: Captured = Arc::new(Mutex::new(Vec::new()));
    let (addr, cancel) = start_gc_server(Arc::clone(&store)).await;
    let gc_url = format!("http://{addr}");
    let gc_client = GcClient::new(gc_url.clone(), mock_token_receiver(), test_config(&gc_url))
        .await
        .unwrap();

    let span = tracing::info_span!("mc.outbound.gc");
    span.set_parent(known_remote_context());
    gc_client.register().instrument(span).await.unwrap();

    let captured = captured_traceparents(&store);
    assert_eq!(captured.len(), 1, "expected exactly one register_mc call");
    let tp = captured[0]
        .as_ref()
        .expect("GC register_mc must carry a traceparent");
    assert!(tp.starts_with("00-"), "malformed traceparent: {tp:?}");
    assert_eq!(
        &tp[3..35],
        expected_trace_id(),
        "outbound GC traceparent trace-id must match the active span"
    );

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// MhClient
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mh_client_register_meeting_injects_active_span_traceparent() {
    install_test_propagator();
    let _capture = SpanCapture::install();

    let store: Captured = Arc::new(Mutex::new(Vec::new()));
    let (addr, cancel) = start_mh_server(Arc::clone(&store)).await;
    let mh_url = format!("http://{addr}");
    let mh_client = MhClient::new(mock_token_receiver());

    let span = tracing::info_span!("mc.outbound.mh");
    span.set_parent(known_remote_context());
    mh_client
        .register_meeting(&mh_url, "meeting-otel", "mc-test-001", "http://mc:50052")
        .instrument(span)
        .await
        .unwrap();

    let captured = captured_traceparents(&store);
    assert_eq!(
        captured.len(),
        1,
        "expected exactly one register_meeting call"
    );
    let tp = captured[0]
        .as_ref()
        .expect("MH register_meeting must carry a traceparent");
    assert_eq!(
        &tp[3..35],
        expected_trace_id(),
        "outbound MH traceparent trace-id must match the active span"
    );

    cancel.cancel();
}

/// Negative (both clients share the interceptor): with NO active recording
/// span, `client_interceptor` injects nothing — the outbound request carries no
/// `traceparent`. Guards against a hard-coded/garbage header being emitted when
/// there's no trace to propagate.
#[tokio::test]
async fn outbound_without_active_span_injects_no_traceparent() {
    install_test_propagator();
    // Deliberately NO SpanCapture / no instrumented span.

    let store: Captured = Arc::new(Mutex::new(Vec::new()));
    let (addr, cancel) = start_gc_server(Arc::clone(&store)).await;
    let gc_url = format!("http://{addr}");
    let gc_client = GcClient::new(gc_url.clone(), mock_token_receiver(), test_config(&gc_url))
        .await
        .unwrap();

    gc_client.register().await.unwrap();

    let captured = captured_traceparents(&store);
    assert_eq!(captured.len(), 1);
    assert!(
        captured[0].is_none(),
        "no active span ⇒ no traceparent should be injected, got {:?}",
        captured[0]
    );

    cancel.cancel();
}
