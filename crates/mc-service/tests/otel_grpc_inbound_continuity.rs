//! Integration test: inbound gRPC trace continuity (R-56, MC as server).
//!
//! MH→MC inbound gRPC call (`notify_participant_connected`) carrying
//! `traceparent` metadata → MC's `otel_grpc::server_interceptor()` extracts it →
//! the handler's own `#[instrument(name = "mc.grpc.media_coordination.connected")]`
//! span MUST inherit the same trace-id.
//!
//! The server is built through `mc_service::grpc::server_layers` (the same
//! function `main.rs` uses) with the REAL `McAuthLayer`, so the request span →
//! extraction → auth ordering is exercised as shipped. Moving extraction outside
//! the span layer turns these tests red; moving it after auth does not compile
//! (auth is bound to tonic's `Body`).
//!
//! ## Coverage scope — why MediaCoordinationService, not MeetingControllerService
//!
//! MC's inbound builder serves TWO services behind the one shared layer stack —
//! `MediaCoordinationServiceServer` and `MeetingControllerServiceServer`. This component-tier test exercises
//! `MediaCoordinationService` because it constructs with only an in-memory
//! meeting-controller handle. `MeetingControllerService` (`McAssignmentService`)
//! requires an `Arc<FencedRedisClient>` whose constructor eagerly connects to a
//! live Redis — not available at the component tier (no MC component test
//! instantiates `FencedRedisClient`). Both services sit behind the SAME
//! `server_layers` stack, so this proves the reparent mechanism for both;
//! the `MeetingControllerService` path is exercised end-to-end in the env-test
//! tier (real cluster + Redis).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;

use mc_service::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use mc_service::grpc::McMediaCoordinationService;
use mc_service::media_routing::PolicyGenerations;
use proto_gen::dark_tower::internal::v1::media_coordination_service_client::MediaCoordinationServiceClient;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationServiceServer;
use proto_gen::dark_tower::internal::v1::NotifyParticipantConnectedRequest;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Endpoint, Server as TonicServer};
use tonic::Request as TonicRequest;

use ::common::jwt::{JwksClient, ServiceClaims};
use mc_service::grpc::McAuthLayer;
use mc_test_utils::jwt_test::{mount_jwks_mock, TestKeypair};

use test_common::otel_capture::{
    deployed_rust_log, install_test_propagator, known_trace_id_hex, known_traceparent,
    trace_id_hex, SpanCapture, KNOWN_SPAN_ID_U64,
};

struct TestGrpcServer {
    addr: std::net::SocketAddr,
    keypair: TestKeypair,
    _jwks: wiremock::MockServer,
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

impl TestGrpcServer {
    /// Attach a valid MH→MC service token (scope `service.write.mc`,
    /// `service_type` media-handler) so the request passes the real auth layer.
    fn authed<T>(&self, mut request: TonicRequest<T>) -> TonicRequest<T> {
        let now = chrono::Utc::now().timestamp();
        let token = self.keypair.sign_token(&ServiceClaims::new(
            "mh-otel-rig".to_string(),
            now + 3600,
            now,
            "service.write.mc".to_string(),
            Some("media-handler".to_string()),
        ));
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {token}").parse().expect("bearer parses"),
        );
        request
    }
}

/// Start MC's REAL inbound gRPC stack for `MediaCoordinationService` through
/// `mc_service::grpc::server_layers` — the same function `main.rs` uses — with
/// the real `McAuthLayer`, whose JWT-validation child spans are what make the
/// request span → extraction → auth ordering load-bearing.
async fn start_test_grpc_server() -> TestGrpcServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test gRPC listener");
    let addr = listener.local_addr().expect("local addr");
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    // Empty controller: this test isolates TRACE CONTINUITY, not sender
    // resolution. The handler resolves `0`/`meeting_unknown` throughout, which
    // is the correct answer for a controller holding no meetings and does not
    // affect the span assertions.
    let controller = Arc::new(MeetingControllerActorHandle::new(
        "mc-otel-test".to_string(),
        ActorMetrics::new(),
        ControllerMetrics::new(),
        common::secret::SecretBox::new(Box::new(vec![0u8; 32])),
        Arc::new(PolicyGenerations::new()),
        mc_test_utils::kek::kek_lifecycle(),
    ));
    let svc = McMediaCoordinationService::new(controller);

    let jwks = wiremock::MockServer::start().await;
    let keypair = TestKeypair::new(42, "mc-otel-rig-key-01");
    let jwks_url = mount_jwks_mock(&jwks, &keypair).await;
    let auth = McAuthLayer::new(
        Arc::new(JwksClient::new(jwks_url).expect("JwksClient")),
        300,
    );

    let server = TonicServer::builder()
        .layer(mc_service::grpc::server_layers(auth))
        .add_service(MediaCoordinationServiceServer::new(svc))
        .serve_with_incoming_shutdown(incoming, async move {
            cancel_clone.cancelled().await;
        });

    let handle = tokio::spawn(async move {
        let _ = server.await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    TestGrpcServer {
        addr,
        keypair,
        _jwks: jwks,
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
        connection_id: "conn-otel".to_string(),
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

    let mut request = server.authed(TonicRequest::new(connected_request()));
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
         the extraction layer is removed from, or moved after auth in, `server_layers` (no \
         span to attach to, or auth's child spans already started it)"
    );
    // `configured_layer` sets `with_target(false)`: tracing-opentelemetry
    // 0.32+ would otherwise add a `target` attribute to every span.
    assert!(
        !span.attributes.iter().any(|kv| kv.key.as_str() == "target"),
        "exported span must not carry a `target` attribute: {:?}",
        span.attributes
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
        .notify_participant_connected(server.authed(TonicRequest::new(connected_request())))
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

    // A REAL token (the request must pass auth to reach the handler span).
    let mut request = server.authed(TonicRequest::new(connected_request()));
    let auth_value = request
        .metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .expect("authed request carries authorization")
        .to_string();
    let secret = auth_value
        .strip_prefix("Bearer ")
        .expect("bearer prefix")
        .to_string();
    request.metadata_mut().insert(
        "traceparent",
        known_traceparent().parse().expect("traceparent parses"),
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
            !value.contains(secret.as_str()) && !value.contains("Bearer"),
            "span attribute {key:?} leaked the bearer token"
        );
    }

    drop(server);
}

/// Same call under MC's DEPLOYED log filter (§11 (c)): the `TraceLayer`
/// request span must exist at INFO and carry the inbound parent. With the
/// DEBUG-default span this filter disables it, and reparenting is a no-op.
#[tokio::test]
async fn request_span_reparents_under_deployed_filter() {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&deployed_rust_log());

    let server = start_test_grpc_server().await;
    let mut client = connect(server.addr).await;

    let mut request = server.authed(TonicRequest::new(connected_request()));
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
        .find_span("request")
        .expect("expected an exported gRPC `request` span under the deployed filter");
    assert_eq!(trace_id_hex(&span), known_trace_id_hex());
    assert_eq!(
        format!("{:016x}", span.parent_span_id),
        format!("{KNOWN_SPAN_ID_U64:016x}")
    );

    drop(server);
}
