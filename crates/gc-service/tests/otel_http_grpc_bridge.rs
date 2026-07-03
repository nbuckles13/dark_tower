//! Integration test: HTTP→gRPC trace-context bridge (R-56 surfaces (a) + (c)).
//!
//! An inbound HTTP request carrying `traceparent` → GC's axum extraction
//! middleware attaches it as the request span's parent → a handler-level
//! call to the REAL `McClient::assign_meeting` (which wires
//! `otel_grpc::client_interceptor()`, surface (c)) MUST carry the SAME
//! trace-id in its outbound gRPC metadata.
//!
//! Uses a real minimal Tonic server (mirrors
//! `crates/mh-service/tests/common/mock_mc.rs`'s `MockMcHandle`/
//! `with_connected_tx` pattern) rather than `MockMcClient` — `MockMcClient`
//! implements `McClientTrait` *post*-interceptor with plain-arg methods (no
//! `tonic::Request`/`MetadataMap` anywhere in that boundary), so it
//! structurally cannot exercise `otel_grpc::client_interceptor()`, which only
//! runs inside the real `McClient::assign_meeting`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::otel_support::{known_traceparent, setup_otel_test_environment};

use axum::{
    body::Body,
    extract::Request as AxumRequest,
    http::{Request as HttpRequest, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::post,
    Router,
};
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::services::McClient;
use proto_gen::dark_tower::internal::v1::meeting_controller_service_server::{
    MeetingControllerService, MeetingControllerServiceServer,
};
use proto_gen::dark_tower::internal::v1::{
    AssignMeetingWithMhRequest, AssignMeetingWithMhResponse,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::metadata::MetadataMap;
use tonic::transport::Server as TonicServer;
use tonic::{Request as TonicRequest, Response as TonicResponse, Status};
use tower::ServiceExt;

/// Minimal mock MC gRPC service that captures the received request's
/// metadata (including whatever `client_interceptor()` injected) onto a
/// channel, then always accepts.
struct MockMc {
    metadata_tx: mpsc::Sender<MetadataMap>,
}

#[tonic::async_trait]
impl MeetingControllerService for MockMc {
    async fn assign_meeting_with_mh(
        &self,
        request: TonicRequest<AssignMeetingWithMhRequest>,
    ) -> Result<TonicResponse<AssignMeetingWithMhResponse>, Status> {
        let _ = self.metadata_tx.send(request.metadata().clone()).await;
        Ok(TonicResponse::new(AssignMeetingWithMhResponse {
            accepted: true,
            rejection_reason: 0,
        }))
    }
}

/// RAII handle mirroring `mh-service`'s `MockMcHandle`: dropping cancels +
/// aborts the spawned server task so a panicking assertion doesn't leak the
/// port.
struct MockMcHandle {
    addr: SocketAddr,
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl Drop for MockMcHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

async fn start_mock_mc(metadata_tx: mpsc::Sender<MetadataMap>) -> MockMcHandle {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mock MC listener");
    let addr = listener.local_addr().expect("failed to read local addr");
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let server = TonicServer::builder()
        .add_service(MeetingControllerServiceServer::new(MockMc { metadata_tx }))
        .serve_with_incoming_shutdown(incoming, async move {
            cancel_clone.cancelled().await;
        });

    let handle = tokio::spawn(async move {
        let _ = server.await;
    });

    // Tonic needs a moment to spin up the HTTP/2 server before accepting.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    MockMcHandle {
        addr,
        cancel,
        handle: Some(handle),
    }
}

/// Extraction middleware wrapper mirroring `gc_service::middleware::otel`'s
/// production middleware (re-declared here rather than imported, since it's
/// a `pub(crate)`-adjacent thin wrapper over the public `otel_http` helper —
/// same call, same placement rule).
async fn extract_trace_context_middleware(request: AxumRequest, next: Next) -> Response {
    common::observability::otel_http::extract_trace_context(request.headers());
    next.run(request).await
}

#[tokio::test]
async fn inbound_http_traceparent_bridges_to_outbound_mc_grpc_call() {
    let _otel_guard = setup_otel_test_environment();

    let (metadata_tx, mut metadata_rx) = mpsc::channel(1);
    let mock_mc = start_mock_mc(metadata_tx).await;

    let (_tx, rx) = watch::channel(SecretString::from("test-service-token"));
    let token_receiver = TokenReceiver::from_watch_receiver(rx);
    let mc_client = Arc::new(McClient::new(token_receiver));
    let mc_endpoint = format!("http://{}", mock_mc.addr);

    // Handler closure captures the real mock endpoint (the top-level `handler`
    // fn above documents intent; this closure is what's actually routed, so
    // the endpoint threading is explicit and doesn't rely on a magic constant).
    let mc_client_for_handler = Arc::clone(&mc_client);
    let app = Router::new()
        .route(
            "/",
            post(move || {
                let mc_client = Arc::clone(&mc_client_for_handler);
                let mc_endpoint = mc_endpoint.clone();
                async move {
                    let result = mc_client
                        .assign_meeting(&mc_endpoint, "meeting-bridge-test", &[], "gc-bridge-test")
                        .await;
                    match result {
                        Ok(_) => StatusCode::OK,
                        Err(_) => StatusCode::BAD_GATEWAY,
                    }
                }
            }),
        )
        // Same placement rule as production `routes/mod.rs`: the extraction
        // middleware must be more inner than `TraceLayer` so it runs after
        // the request span exists.
        .layer(middleware::from_fn(extract_trace_context_middleware))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let traceparent = known_traceparent();
    let response = app
        .oneshot(
            HttpRequest::builder()
                .method("POST")
                .uri("/")
                .header("traceparent", traceparent.clone())
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let received_metadata =
        tokio::time::timeout(std::time::Duration::from_secs(2), metadata_rx.recv())
            .await
            .expect("mock MC should receive a call within timeout")
            .expect("channel should not be closed");

    let forwarded_traceparent = received_metadata
        .get("traceparent")
        .expect("outbound gRPC call must carry a traceparent metadata entry")
        .to_str()
        .expect("valid metadata string");

    let inbound_trace_id = &traceparent[3..35];
    let forwarded_trace_id = forwarded_traceparent
        .get(3..35)
        .expect("forwarded traceparent has a trace-id segment");
    assert_eq!(
        forwarded_trace_id, inbound_trace_id,
        "HTTP->gRPC bridge must preserve the inbound trace-id on the outbound MC call"
    );

    drop(mock_mc);
}
