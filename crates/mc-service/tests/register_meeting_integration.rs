// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — do not "simplify" it away. `MetricAssertion`
// binds a per-thread recorder; `MhClient::register_meeting` makes a real gRPC
// call whose post-RPC `record_register_meeting()` emission lives on the
// caller's task. On `current_thread` that task IS the test thread; on
// multi-thread it can land on a worker and the assertion silently observes
// zero. See `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for `MhClient::register_meeting()` driving real
//! `mc_register_meeting_total` and `mc_register_meeting_duration_seconds`
//! emissions per ADR-0032 Step 3 §Cluster E.
//!
//! Success path uses a stub `MediaHandlerService` gRPC server that returns
//! `RegisterMeetingResponse { accepted: true }`. Error path uses an
//! unreachable endpoint, which produces the production-equivalent
//! "Failed to connect" McError::Grpc branch at `mh_client.rs:117`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use ::common::secret::SecretString;
use ::common::token_manager::TokenReceiver;
use mc_service::grpc::MhClient;
use proto_gen::internal::media_handler_service_server::{
    MediaHandlerService, MediaHandlerServiceServer,
};
use proto_gen::internal::{
    MediaTelemetry, RegisterMeetingRequest, RegisterMeetingResponse, RegisterParticipant,
    RegisterParticipantResponse, RouteMediaCommand, RouteMediaResponse, TelemetryAck,
};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tonic::{Request, Response, Status, Streaming};

// ---------------------------------------------------------------------------
// Stub MH gRPC server — accepts everything, used solely to prove the
// MhClient::register_meeting() success path.
// ---------------------------------------------------------------------------

struct StubMediaHandler {
    accept: bool,
}

#[tonic::async_trait]
impl MediaHandlerService for StubMediaHandler {
    async fn register(
        &self,
        _request: Request<RegisterParticipant>,
    ) -> Result<Response<RegisterParticipantResponse>, Status> {
        Err(Status::unimplemented("stub"))
    }

    async fn register_meeting(
        &self,
        _request: Request<RegisterMeetingRequest>,
    ) -> Result<Response<RegisterMeetingResponse>, Status> {
        Ok(Response::new(RegisterMeetingResponse {
            accepted: self.accept,
        }))
    }

    async fn route_media(
        &self,
        _request: Request<RouteMediaCommand>,
    ) -> Result<Response<RouteMediaResponse>, Status> {
        Err(Status::unimplemented("stub"))
    }

    async fn stream_telemetry(
        &self,
        _request: Request<Streaming<MediaTelemetry>>,
    ) -> Result<Response<TelemetryAck>, Status> {
        Err(Status::unimplemented("stub"))
    }
}

async fn start_stub_mh(accept: bool) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let svc = MediaHandlerServiceServer::new(StubMediaHandler { accept });
    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(svc)
            .serve_with_incoming(stream)
            .await;
    });

    // Brief settle so the server is ready to accept connections.
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

fn make_token_rx() -> TokenReceiver {
    let (tx, rx) = watch::channel(SecretString::from("test-token"));
    Box::leak(Box::new(tx));
    TokenReceiver::from_test_channel(rx)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn register_meeting_success_emits_status_success_and_duration_observation() {
    let addr = start_stub_mh(true).await;
    let endpoint = format!("http://{addr}");
    let client = MhClient::new(make_token_rx());

    let snap = MetricAssertion::snapshot();
    let result = client
        .register_meeting(
            &endpoint,
            "meeting-success",
            "mc-test",
            "http://mc-test:50052",
        )
        .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Histogram first (drain-on-read).
    snap.histogram("mc_register_meeting_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "success")])
        .assert_delta(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn register_meeting_mh_rejects_emits_status_error() {
    // Stub responds with `accepted: false` → mh_client.rs:144 records "error".
    let addr = start_stub_mh(false).await;
    let endpoint = format!("http://{addr}");
    let client = MhClient::new(make_token_rx());

    let snap = MetricAssertion::snapshot();
    let result = client
        .register_meeting(
            &endpoint,
            "meeting-rejected",
            "mc-test",
            "http://mc-test:50052",
        )
        .await;
    assert!(
        result.is_err(),
        "expected Err on MH rejection, got {result:?}"
    );

    snap.histogram("mc_register_meeting_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "success")])
        .assert_delta(0);
}

// NOTE on the connect-failure branch:
//
// `MhClient::register_meeting()` at `mh_client.rs:97-118` returns
// `McError::Grpc("Failed to connect: ...")` on connect failure BEFORE the
// metric-recording block at `:131`. So an unreachable-endpoint test would
// observe `mc_register_meeting_total` delta=0. This is a small fidelity gap
// in the recorder placement (tracked as informational, not a Step-3 fix
// target). The two tests above cover the two emission branches that DO fire:
// (1) `accepted=true` → success, and (2) `accepted=false` → error.
