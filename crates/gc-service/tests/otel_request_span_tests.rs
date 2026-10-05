//! GC request-span tests (§11): span shape, probe levels, and inbound
//! reparenting under the DEPLOYED log filter, through GC's real router and
//! its real inbound gRPC stack.
//!
//! - Capability-route spans record `method` + normalized `endpoint` only:
//!   the meeting code (a guest-access capability) appears nowhere in the
//!   exported span or in the JSON log lines emitted inside the request.
//! - Under the manifest's `RUST_LOG`, probe/scrape paths export no request
//!   span and never warn, while API paths export an INFO `request` span.
//! - A known `traceparent` reparents the HTTP and gRPC request spans under
//!   that same filter (the DEBUG-default span would be disabled there, making
//!   reparenting a silent no-op).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::grpc_auth::{grpc_auth, mc_bearer};

use common::observability::testing::otel::{
    attr, deployed_rust_log, install_test_propagator, known_traceparent, SpanCapture,
    KNOWN_PARENT_SPAN_ID_HEX, KNOWN_TRACE_ID_HEX,
};

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::config::Config;
use gc_service::grpc::McService;
use gc_service::handlers::TelemetryState;
use gc_service::routes::{self, AppState};
use gc_service::services::MockMcClient;
use opentelemetry_sdk::trace::SpanData;
use proto_gen::dark_tower::internal::v1::global_controller_service_client::GlobalControllerServiceClient;
use proto_gen::dark_tower::internal::v1::global_controller_service_server::GlobalControllerServiceServer;
use proto_gen::dark_tower::internal::v1::RegisterMcRequest;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Endpoint, Server as TonicServer};
use tower::ServiceExt;

/// GC's deployed `RUST_LOG`.
fn gc_deployed_rust_log() -> String {
    deployed_rust_log("infra/services/gc-service/deployment.yaml")
}

/// A meeting code distinctive enough that any occurrence in a span or log
/// line can only have come from the request path.
const MEETING_CODE: &str = "zq7capx9k2";

fn app_state(pool: PgPool) -> Arc<AppState> {
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
    let telemetry = TelemetryState::from_config(
        config.otel_collector_endpoint.clone(),
        config.telemetry_proxy_max_bytes,
        config.telemetry_proxy_rate_limit_per_minute,
    )
    .expect("telemetry state");
    Arc::new(AppState {
        pool,
        config,
        mc_client: Arc::new(MockMcClient::accepting()),
        token_receiver: TokenReceiver::from_watch_receiver(rx),
        telemetry,
    })
}

/// GC's production router over a lazy (never-connecting) pool. Every request
/// in this file is rejected or answered before any DB access.
fn router() -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://test/test")
        .expect("lazy pool");
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder()
        .handle();
    routes::build_routes(app_state(pool), metrics_handle).expect("build_routes")
}

async fn send(request: Request<Body>) -> StatusCode {
    let response = router().oneshot(request).await.unwrap();
    let status = response.status();
    // Drop the response (and its body) so TraceLayer's request span closes
    // and exports before the caller inspects the capture.
    drop(response);
    status
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

/// The span's name, every attribute and every event, flattened to one string.
fn span_text(span: &SpanData) -> String {
    let mut out = span.name.to_string();
    for kv in &span.attributes {
        out.push_str(&format!(" {}={}", kv.key, kv.value));
    }
    for event in span.events.iter() {
        out.push_str(&format!(" event:{}", event.name));
        for kv in &event.attributes {
            out.push_str(&format!(" {}={}", kv.key, kv.value));
        }
    }
    out
}

fn single_request_span(capture: &SpanCapture) -> SpanData {
    let spans = capture.spans_named("request");
    assert_eq!(
        spans.len(),
        1,
        "expected exactly one exported `request` span, got {:?}",
        capture
            .finished_spans()
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    );
    spans.into_iter().next().unwrap()
}

/// Shared assertion for both capability routes.
fn assert_code_absent(capture: &SpanCapture, expected_endpoint: &str) {
    let span = single_request_span(capture);
    assert_eq!(
        attr(&span, "endpoint").as_deref(),
        Some(expected_endpoint),
        "request span must carry the normalized endpoint (positive control)"
    );
    let text = span_text(&span);
    assert!(
        !text.contains(MEETING_CODE),
        "meeting code leaked into the exported request span: {text}"
    );

    let json = capture.json_output();
    let in_request: Vec<&str> = json
        .lines()
        .filter(|l| l.contains("\"name\":\"request\""))
        .collect();
    assert!(
        !in_request.is_empty(),
        "expected at least one JSON log line emitted inside the request span, got:\n{json}"
    );
    for line in &in_request {
        assert!(
            line.contains(expected_endpoint),
            "in-request log line must carry the normalized endpoint: {line}"
        );
    }
    assert!(
        !json.contains(MEETING_CODE),
        "meeting code leaked into a JSON log line:\n{json}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn join_route_span_and_logs_carry_endpoint_not_meeting_code() {
    let capture = SpanCapture::install_with_filter("debug");
    let status = send(get(&format!("/api/v1/meetings/{MEETING_CODE}"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_code_absent(&capture, "/api/v1/meetings/{code}");
}

#[tokio::test(flavor = "current_thread")]
async fn guest_token_route_span_and_logs_carry_endpoint_not_meeting_code() {
    let capture = SpanCapture::install_with_filter("debug");
    // No body / content type: the JSON extractor rejects before any DB access.
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/meetings/{MEETING_CODE}/guest-token"))
        .body(Body::empty())
        .unwrap();
    let status = send(request).await;
    assert!(
        status.is_client_error(),
        "expected a 4xx rejection, got {status}"
    );
    assert_code_absent(&capture, "/api/v1/meetings/{code}/guest-token");
}

#[tokio::test(flavor = "current_thread")]
async fn probe_paths_export_no_span_and_api_paths_export_info_span() {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&gc_deployed_rust_log());

    for probe in common::observability::otel_http::PROBE_ENDPOINTS {
        send(get(probe)).await;
    }
    assert!(
        capture.spans_named("request").is_empty(),
        "probe/scrape paths must not export a request span under the deployed filter"
    );
    assert_eq!(capture.extraction_warns(), 0, "probes must never warn");

    // Positive control in the same run: an API path exports an INFO span.
    send(get("/api/v1/meetings")).await;
    let span = single_request_span(&capture);
    assert_eq!(attr(&span, "endpoint").as_deref(), Some("/api/v1/meetings"));
    assert_eq!(attr(&span, "method").as_deref(), Some("GET"));
    assert_eq!(capture.extraction_warns(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn http_request_span_reparents_under_deployed_filter() {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&gc_deployed_rust_log());

    let request = Request::builder()
        .uri(format!("/api/v1/meetings/{MEETING_CODE}"))
        .header("traceparent", known_traceparent())
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap();
    send(request).await;

    let span = single_request_span(&capture);
    assert_eq!(
        format!("{:032x}", span.span_context.trace_id()),
        KNOWN_TRACE_ID_HEX,
        "GC HTTP request span must continue the inbound trace under the deployed filter"
    );
    assert_eq!(
        format!("{:016x}", span.parent_span_id),
        KNOWN_PARENT_SPAN_ID_HEX
    );
    assert_eq!(capture.extraction_warns(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn grpc_request_span_reparents_under_deployed_filter(pool: PgPool) {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&gc_deployed_rust_log());

    // GC's inbound gRPC stack through the SAME `server_layers` main.rs uses,
    // with the real auth layer (whose JWT-validation child spans are what make
    // the extraction order load-bearing).
    let (_jwks, keypair, auth) = grpc_auth().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let server = TonicServer::builder()
        .layer(gc_service::grpc::server_layers(auth))
        .add_service(GlobalControllerServiceServer::new(McService::new(
            app_state(pool),
        )))
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            async move { cancel_clone.cancelled().await },
        );
    let handle = tokio::spawn(async move {
        let _ = server.await;
    });

    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = GlobalControllerServiceClient::new(channel);
    let mut request = tonic::Request::new(RegisterMcRequest {
        id: "test-mc-deployed-filter".to_string(),
        region: "us-east-1".to_string(),
        grpc_endpoint: "http://mc-otel:50051".to_string(),
        webtransport_endpoint: "https://mc-otel:443".to_string(),
        max_meetings: 100,
        max_participants: 1000,
    });
    request
        .metadata_mut()
        .insert("traceparent", known_traceparent().parse().unwrap());
    request
        .metadata_mut()
        .insert("authorization", mc_bearer(&keypair));
    client.register_mc(request).await.expect("register_mc");
    tokio::task::yield_now().await;

    let spans = capture.spans_named("request");
    let span = spans
        .iter()
        .find(|s| attr(s, "uri").is_some_and(|u| u.contains("GlobalControllerService/RegisterMC")))
        .unwrap_or_else(|| panic!("no gRPC request span exported: {spans:?}"));
    assert_eq!(
        format!("{:032x}", span.span_context.trace_id()),
        KNOWN_TRACE_ID_HEX,
        "GC gRPC request span must continue the inbound trace under the deployed filter"
    );
    assert_eq!(
        format!("{:016x}", span.parent_span_id),
        KNOWN_PARENT_SPAN_ID_HEX
    );
    assert_eq!(capture.extraction_warns(), 0);

    cancel.cancel();
    handle.abort();
}
