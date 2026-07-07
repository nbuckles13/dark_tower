//! Integration test: inbound gRPC trace continuity (R-56, MC as server).
//!
//! MH→MC inbound gRPC call (`notify_participant_connected`) carrying
//! `traceparent` metadata → MC's `otel_grpc::server_interceptor()` extracts it →
//! the handler's own `#[instrument(name = "mc.grpc.media_coordination.connected")]`
//! span MUST inherit the same trace-id.
//!
//! This is the test that catches the "silent no-op" regression:
//! `.with_interceptor(...)` alone does nothing without a span-creating layer
//! (`TraceLayer::new_for_grpc()`) ahead of it on the server builder — without
//! that layer the assertion below fails (the interceptor finds no active span,
//! and the handler span comes up as an unrelated root). Verified by temporarily
//! removing `.layer(TraceLayer::new_for_grpc())` → the positive test fails →
//! restored.
//!
//! ## Coverage scope — why MediaCoordinationService, not MeetingControllerService
//!
//! MC's inbound builder serves TWO services, each with its own
//! `.with_interceptor(server_interceptor())` — `MediaCoordinationServiceServer`
//! and `MeetingControllerServiceServer`. This component-tier test exercises
//! `MediaCoordinationService` because it constructs with only an in-memory
//! `MhConnectionRegistry`. `MeetingControllerService` (`McAssignmentService`)
//! requires an `Arc<FencedRedisClient>` whose constructor eagerly connects to a
//! live Redis — not available at the component tier (no MC component test
//! instantiates `FencedRedisClient`). Both services compose the IDENTICAL
//! `server_interceptor()` behind the SAME shared `TraceLayer`, so this proves
//! the reparent mechanism for the generated `*Server::with_interceptor` shape;
//! the `MeetingControllerService` path is exercised end-to-end in the env-test
//! tier (real cluster + Redis).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;

use mc_service::grpc::McMediaCoordinationService;
use mc_service::mh_connection_registry::MhConnectionRegistry;
use proto_gen::dark_tower::internal::v1::media_coordination_service_client::MediaCoordinationServiceClient;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationServiceServer;
use proto_gen::dark_tower::internal::v1::NotifyParticipantConnectedRequest;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Endpoint, Server as TonicServer};
use tonic::Request as TonicRequest;

use test_common::otel_capture::{
    install_test_propagator, known_trace_id_hex, known_traceparent, trace_id_hex, SpanCapture,
};

struct TestGrpcServer {
    addr: std::net::SocketAddr,
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl Drop for TestGrpcServer {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

/// Start MC's REAL inbound gRPC stack for `MediaCoordinationService`, wired
/// exactly as `main.rs` wires it (R-56): `TraceLayer::new_for_grpc()` OUTERMOST
/// (first `.layer()` — required so `server_interceptor()` has an active span to
/// attach the parent to), then `.with_interceptor(svc, server_interceptor())`.
/// `McAuthLayer` is intentionally omitted — auth is a separate, already-tested
/// boundary; this test isolates trace-continuity only. The layer stack is
/// visible + removable HERE so the non-tautology proof is demonstrable.
async fn start_test_grpc_server() -> TestGrpcServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test gRPC listener");
    let addr = listener.local_addr().expect("local addr");
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let svc = McMediaCoordinationService::new(Arc::new(MhConnectionRegistry::new()));

    let server = TonicServer::builder()
        .layer(tower_http::trace::TraceLayer::new_for_grpc())
        .add_service(MediaCoordinationServiceServer::with_interceptor(
            svc,
            common::observability::otel_grpc::server_interceptor(),
        ))
        .serve_with_incoming_shutdown(incoming, async move {
            cancel_clone.cancelled().await;
        });

    let handle = tokio::spawn(async move {
        let _ = server.await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    TestGrpcServer {
        addr,
        cancel,
        handle: Some(handle),
    }
}

async fn connect(
    addr: std::net::SocketAddr,
) -> MediaCoordinationServiceClient<tonic::transport::Channel> {
    let endpoint = Endpoint::from_shared(format!("http://{addr}"))
        .expect("valid endpoint")
        .connect()
        .await
        .expect("connect to test gRPC server");
    MediaCoordinationServiceClient::new(endpoint)
}

fn connected_request() -> NotifyParticipantConnectedRequest {
    NotifyParticipantConnectedRequest {
        meeting_id: "otel-inbound-meeting".to_string(),
        participant_id: "otel-inbound-part".to_string(),
        handler_id: "mh-otel-1".to_string(),
    }
}

#[tokio::test]
async fn inbound_call_with_traceparent_produces_matching_span() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let server = start_test_grpc_server().await;
    let mut client = connect(server.addr).await;

    let mut request = TonicRequest::new(connected_request());
    request.metadata_mut().insert(
        "traceparent",
        known_traceparent().parse().expect("traceparent parses"),
    );
    client
        .notify_participant_connected(request)
        .await
        .expect("notify_participant_connected should succeed");

    tokio::task::yield_now().await;

    let span = capture
        .find_span("mc.grpc.media_coordination.connected")
        .expect("expected an exported mc.grpc.media_coordination.connected span");
    assert_eq!(
        trace_id_hex(&span),
        known_trace_id_hex(),
        "handler span's trace_id must match the inbound traceparent — this FAILS if \
         TraceLayer::new_for_grpc() is removed (server_interceptor would have no active span \
         to attach the parent to, and the span would come up as an unrelated root)"
    );

    drop(server);
}

/// Negative companion: no inbound `traceparent` ⇒ the handler span is a
/// freshly-sampled root under the TraceLayer span, NOT the fixture trace-id.
#[tokio::test]
async fn inbound_call_without_traceparent_gets_fresh_trace_id() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let server = start_test_grpc_server().await;
    let mut client = connect(server.addr).await;

    client
        .notify_participant_connected(TonicRequest::new(connected_request()))
        .await
        .expect("notify_participant_connected should succeed");

    tokio::task::yield_now().await;

    let span = capture
        .find_span("mc.grpc.media_coordination.connected")
        .expect("expected an exported span");
    assert_ne!(
        trace_id_hex(&span),
        known_trace_id_hex(),
        "with no inbound traceparent the handler span must have a fresh trace-id, not the fixture"
    );

    drop(server);
}

/// Security (R-56): an inbound call carrying BOTH `traceparent` AND
/// `authorization: Bearer <token>` reparents the span but leaks NO auth
/// material into the span attributes (the interceptor reads only the two W3C
/// PROPAGATED_HEADERS).
#[tokio::test]
async fn inbound_call_does_not_leak_authorization_into_span() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let server = start_test_grpc_server().await;
    let mut client = connect(server.addr).await;

    let secret = "super-secret-jwt-value-abc123";
    let mut request = TonicRequest::new(connected_request());
    request.metadata_mut().insert(
        "traceparent",
        known_traceparent().parse().expect("traceparent parses"),
    );
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {secret}").parse().expect("auth parses"),
    );
    client
        .notify_participant_connected(request)
        .await
        .expect("call should succeed");

    tokio::task::yield_now().await;

    let span = capture
        .find_span("mc.grpc.media_coordination.connected")
        .expect("expected an exported span");
    // Reparent still worked...
    assert_eq!(trace_id_hex(&span), known_trace_id_hex());
    // ...and no attribute carries the token or auth header name.
    for kv in &span.attributes {
        let key = kv.key.as_str();
        let value = kv.value.as_str();
        assert!(
            !key.contains("authorization") && !key.contains("bearer"),
            "span attribute key {key:?} references auth metadata"
        );
        assert!(
            !value.contains(secret) && !value.contains("Bearer"),
            "span attribute {key:?} leaked the bearer token"
        );
    }

    drop(server);
}
