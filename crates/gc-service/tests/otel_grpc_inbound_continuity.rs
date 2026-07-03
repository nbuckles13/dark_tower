//! Integration test: inbound gRPC trace continuity (R-56 surface (d)).
//!
//! MC→GC inbound gRPC call (`register_mc`) carrying `traceparent` metadata →
//! GC's `otel_grpc::server_interceptor()` extracts it → the handler's own
//! `#[instrument(name = "gc.grpc.register_mc")]` span MUST inherit the same
//! trace-id.
//!
//! This is the test that catches the surface-(d) "silent no-op" regression
//! found during Gate-1 review: `.with_interceptor(...)` alone does nothing
//! without a span-creating layer (`TraceLayer::new_for_grpc()`) ahead of it
//! on the server builder — without that layer, this test fails (the
//! interceptor finds no active span to attach the parent to, and
//! `register_mc`'s span comes up as an unrelated root). Distinct boundary
//! from `otel_http_grpc_bridge.rs` (that one is GC-as-client / MC-as-server;
//! this one is GC-as-server / MC-as-client) — same "spin up a real server,
//! don't mock the mechanism under test" principle, opposite direction.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::otel_support::{known_traceparent, setup_otel_test_environment_with_exporter};

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::config::Config;
use gc_service::grpc::McService;
use gc_service::handlers::TelemetryState;
use gc_service::routes::AppState;
use gc_service::services::MockMcClient;
use proto_gen::dark_tower::internal::v1::global_controller_service_client::GlobalControllerServiceClient;
use proto_gen::dark_tower::internal::v1::global_controller_service_server::GlobalControllerServiceServer;
use proto_gen::dark_tower::internal::v1::RegisterMcRequest;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Endpoint, Server as TonicServer};
use tonic::Request as TonicRequest;

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

/// Minimal `AppState` for `McService` — `register_mc` only reads
/// `state.pool`, so the rest of the fields are inert placeholders.
async fn minimal_app_state(pool: PgPool) -> Arc<AppState> {
    let vars = HashMap::from([
        (
            "DATABASE_URL".to_string(),
            "postgresql://test/test".to_string(),
        ),
        ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
        ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
    ]);
    let config = Config::from_vars(&vars).expect("config should load");
    let (_tx, rx) = watch::channel(SecretString::from("test-token"));
    let token_receiver = TokenReceiver::from_watch_receiver(rx);
    let telemetry = TelemetryState::from_config(String::new(), 256 * 1024, 60)
        .expect("telemetry state should build with disabled proxy");

    Arc::new(AppState {
        pool,
        config,
        mc_client: Arc::new(MockMcClient::accepting()),
        token_receiver,
        telemetry,
    })
}

/// Start GC's REAL inbound gRPC stack for `GlobalControllerService`, wired
/// exactly as `main.rs` wires it for R-56 surface (d): `TraceLayer::new_for_grpc()`
/// OUTERMOST (first `.layer()` — required so `otel_grpc::server_interceptor()`
/// has an active span to attach the parent to; see `main.rs`'s comment for
/// why this ordering is the opposite of axum's), then
/// `.with_interceptor(mc_service, otel_grpc::server_interceptor())`.
/// `GrpcAuthLayer` is intentionally omitted — auth is a separate, already-
/// tested boundary; this test isolates trace-continuity only.
async fn start_test_grpc_server(state: Arc<AppState>) -> TestGrpcServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test gRPC listener");
    let addr = listener.local_addr().expect("failed to read local addr");
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

    let mc_service = McService::new(state);

    let server = TonicServer::builder()
        .layer(tower_http::trace::TraceLayer::new_for_grpc())
        .add_service(GlobalControllerServiceServer::with_interceptor(
            mc_service,
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

#[sqlx::test(migrations = "../../migrations")]
async fn inbound_register_mc_call_with_traceparent_produces_matching_span(pool: PgPool) {
    let (_otel_guard, exporter) = setup_otel_test_environment_with_exporter();

    let state = minimal_app_state(pool).await;
    let server = start_test_grpc_server(state).await;

    let endpoint = Endpoint::from_shared(format!("http://{}", server.addr))
        .expect("valid endpoint")
        .connect()
        .await
        .expect("should connect to test gRPC server");
    let mut client = GlobalControllerServiceClient::new(endpoint);

    let traceparent = known_traceparent();
    let mut request = TonicRequest::new(RegisterMcRequest {
        id: "test-mc-otel-001".to_string(),
        region: "us-east-1".to_string(),
        grpc_endpoint: "http://mc-otel:50051".to_string(),
        webtransport_endpoint: "https://mc-otel:443".to_string(),
        max_meetings: 100,
        max_participants: 1000,
    });
    request.metadata_mut().insert(
        "traceparent",
        traceparent
            .parse()
            .expect("known traceparent should parse as MetadataValue"),
    );

    let response = client
        .register_mc(request)
        .await
        .expect("register_mc should succeed");
    assert!(response.into_inner().accepted);

    // Give the simple (synchronous-on-end) exporter a moment to receive the
    // span — `register_mc`'s `#[instrument]` span ends when the async fn
    // returns, which has already happened by the time `.await` above
    // resolved, but the exporter write itself is a separate step; a short
    // yield is enough on a current-thread runtime with no other work pending.
    tokio::task::yield_now().await;

    let finished_spans = exporter
        .get_finished_spans()
        .expect("exporter should return finished spans");
    let found = finished_spans
        .iter()
        .find(|s| s.name == "gc.grpc.register_mc");
    assert!(
        found.is_some(),
        "expected a finished span named gc.grpc.register_mc, got: {:?}",
        finished_spans
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    );
    let register_mc_span = found.expect("checked above");

    let expected_trace_id = &traceparent[3..35];
    let actual_trace_id = format!("{:032x}", register_mc_span.span_context.trace_id());
    assert_eq!(
        actual_trace_id, expected_trace_id,
        "gc.grpc.register_mc span's trace_id must match the inbound traceparent's trace-id — \
         this is the assertion that fails if TraceLayer::new_for_grpc() is removed from the \
         grpc_server builder (server_interceptor() would then have no active span to attach \
         the parent to, and this span would come up as an unrelated root)"
    );

    drop(server);
}
