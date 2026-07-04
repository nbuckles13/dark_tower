//! Integration tests for R-56 gRPC trace-context propagation (MH side).
//!
//! Exercises the REAL `GcClient`/`McClient` → mock-server round trips and the
//! REAL `MhAuthLayer` + `MhMediaService` gRPC stack (via `GrpcRig`), not the
//! `common::observability::otel_grpc` interceptors in isolation — those are
//! already covered by that module's own unit tests (task #24). This file
//! proves MH actually WIRED them at every construction site, and that the
//! wiring produces a real, exportable trace: outbound calls inject a
//! `traceparent` matching the caller's ambient span, inbound calls re-parent
//! the handler's span to the caller's injected context.
//!
//! `GcClient`'s two outbound-injection tests
//! (`test_gc_client_register_injects_traceparent_matching_ambient_span` /
//! `..._send_load_report_...`) live in `gc_integration.rs` instead of here —
//! `MockGcServer` is private to that file, and duplicating it here would be
//! pure copy-paste. This file covers `McClient` (outbound, shared
//! `test_common::mock_mc`) and the inbound `MediaHandlerServiceServer` path.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;

use opentelemetry::trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState};
use opentelemetry::Context;
use proto_gen::dark_tower::internal::v1::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::dark_tower::internal::v1::RegisterMeetingRequest;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use test_common::grpc_rig::GrpcRig;
use test_common::jwks_rig::JwksRig;
use test_common::mock_mc::{start_mock_mc_server, MockBehavior, MockMcServer};
use test_common::otel_capture::{install_test_propagator, trace_id_hex, SpanCapture};
use test_common::test_token_receiver;
use test_common::tokens::mint_valid_mc_token;

// ============================================================================
// Shared fixtures
// ============================================================================

const KNOWN_TRACE_ID_U128: u128 = 0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736;
const KNOWN_SPAN_ID_U64: u64 = 0x00f0_67aa_0ba9_02b7;

/// A remote `Context` with a known, fixed trace/span id — used as the
/// "caller's" ambient trace context for injection assertions.
fn known_remote_context() -> Context {
    Context::new().with_remote_span_context(SpanContext::new(
        TraceId::from(KNOWN_TRACE_ID_U128),
        SpanId::from(KNOWN_SPAN_ID_U64),
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    ))
}

/// A raw W3C `traceparent` header value for [`KNOWN_TRACE_ID_U128`] /
/// [`KNOWN_SPAN_ID_U64`] (version 00, sampled).
fn known_traceparent_header() -> String {
    format!("00-{KNOWN_TRACE_ID_U128:032x}-{KNOWN_SPAN_ID_U64:016x}-01")
}

// ============================================================================
// Outbound injection — McClient
// ============================================================================

#[tokio::test]
async fn test_mc_client_notify_connected_injects_traceparent_matching_ambient_span() {
    install_test_propagator();
    // Needed for `set_parent`/`Span::current().context()` to do anything —
    // without an installed `OpenTelemetryLayer`, both are no-ops.
    let _capture = SpanCapture::install();

    let (traceparent_tx, mut traceparent_rx) = tokio::sync::mpsc::channel(1);
    let mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::Accept).with_traceparent_tx(traceparent_tx),
    )
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let span = tracing::info_span!("test_notify_connected_root");
    span.set_parent(known_remote_context());
    async {
        client
            .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
            .await
            .unwrap();
    }
    .instrument(span)
    .await;

    let tp = traceparent_rx
        .recv()
        .await
        .expect("mock MC should have received one call")
        .expect("NotifyParticipantConnected RPC should carry a traceparent header");
    let trace_id_hex = tp.get(3..35).unwrap_or_default();
    assert_eq!(
        trace_id_hex,
        format!("{KNOWN_TRACE_ID_U128:032x}"),
        "injected trace_id should match the ambient span's trace id"
    );
}

/// Regression guard for the "OTel disabled" default (MH's `OTEL_ENABLED`
/// default is `false`): with no `OpenTelemetryLayer` installed on this
/// thread — deliberately NOT calling `SpanCapture::install()` here, and
/// deliberately not depending on whether some other test in this binary
/// already called `install_test_propagator()` (a process-global, so a
/// per-test assertion on ITS state would race across parallel tests) — the
/// interceptor's `Span::current().context()` has no valid span to inject
/// from, so the outbound call carries NO `traceparent` header and still
/// dispatches successfully. Matches `client_interceptor`'s documented
/// "no span ⇒ no injection" contract (`otel_grpc.rs`).
#[tokio::test]
async fn test_mc_client_notify_connected_no_otel_layer_sends_no_traceparent() {
    let (traceparent_tx, mut traceparent_rx) = tokio::sync::mpsc::channel(1);
    let mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::Accept).with_traceparent_tx(traceparent_tx),
    )
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let result = client
        .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
        .await;
    assert!(result.is_ok(), "call must still succeed with OTel absent");

    let tp = traceparent_rx
        .recv()
        .await
        .expect("mock MC should have received one call");
    assert!(
        tp.is_none(),
        "no OpenTelemetryLayer installed ⇒ no traceparent should be injected, got {tp:?}"
    );
}

/// Directive 6 (test reviewer, Gate 1): under `FailThenAccept`, every retry
/// attempt must inject the IDENTICAL trace_id+span_id — the outer
/// `#[instrument]` span on `notify_participant_connected` stays entered
/// across the whole `send_with_retry` backoff loop (it's one ambient span
/// for the entire call, not re-created per attempt).
#[tokio::test]
async fn test_mc_client_retry_injects_same_traceparent_on_every_attempt() {
    install_test_propagator();
    let _capture = SpanCapture::install();

    const FAIL_COUNT: u32 = 2;
    let (traceparent_tx, mut traceparent_rx) = tokio::sync::mpsc::channel(8);
    let mc = start_mock_mc_server(
        MockMcServer::new(MockBehavior::FailThenAccept {
            fail_count: FAIL_COUNT,
        })
        .with_traceparent_tx(traceparent_tx),
    )
    .await;

    let mc_url = format!("http://{}", mc.addr);
    let client = McClient::new(test_token_receiver());

    let span = tracing::info_span!("test_retry_root");
    span.set_parent(known_remote_context());
    async {
        client
            .notify_participant_connected(&mc_url, "meeting-1", "user-1", "mh-1")
            .await
            .unwrap();
    }
    .instrument(span)
    .await;

    // FAIL_COUNT failing attempts + 1 final success = FAIL_COUNT + 1 calls.
    let expected_attempts = usize::try_from(FAIL_COUNT).unwrap_or(0) + 1;
    let mut traceparents = Vec::with_capacity(expected_attempts);
    for attempt in 0..expected_attempts {
        let tp = traceparent_rx
            .recv()
            .await
            .unwrap_or_else(|| panic!("expected a call on attempt {attempt}"))
            .unwrap_or_else(|| panic!("attempt {attempt} should carry a traceparent header"));
        traceparents.push(tp);
    }

    assert_eq!(traceparents.len(), expected_attempts);

    // The trace_id is inherited from `test_retry_root` (the known ambient
    // span) on every attempt...
    let expected_trace_id_prefix = format!("00-{KNOWN_TRACE_ID_U128:032x}-");
    for (attempt, tp) in traceparents.iter().enumerate() {
        assert!(
            tp.starts_with(&expected_trace_id_prefix),
            "attempt {attempt} traceparent {tp:?} does not carry the ambient span's trace_id"
        );
    }

    // ...but the injected SPAN id is `notify_participant_connected`'s OWN
    // `#[instrument]` span (a child of `test_retry_root`, not
    // `test_retry_root` itself) — that span is created ONCE and stays
    // entered across the whole `send_with_retry` backoff loop, so its span
    // id must be IDENTICAL on every retry attempt (directive 6's actual
    // load-bearing claim: retries don't fragment into separate spans).
    let first = &traceparents[0];
    for (attempt, tp) in traceparents.iter().enumerate() {
        assert_eq!(
            tp, first,
            "attempt {attempt} injected a different traceparent than attempt 0 \
             — the ambient span must stay the same across retries"
        );
    }
}

// ============================================================================
// Inbound extraction — MediaHandlerServiceServer
// ============================================================================

async fn connect_client(rig: &GrpcRig) -> MediaHandlerServiceClient<Channel> {
    let channel = Endpoint::from_shared(rig.url())
        .expect("endpoint url parses")
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .connect()
        .await
        .expect("connect to mh-service test gRPC server");
    MediaHandlerServiceClient::new(channel)
}

#[tokio::test]
async fn test_inbound_register_meeting_reparents_handler_span_to_injected_trace() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let jwks = JwksRig::start(42, "mh-otel-integ-01").await;
    let session_manager = SessionManagerHandle::new();
    let rig = GrpcRig::start(jwks.jwks_client(), session_manager.clone()).await;
    let mut client = connect_client(&rig).await;

    let token = mint_valid_mc_token(&jwks.keypair);
    let mut request = Request::new(RegisterMeetingRequest {
        meeting_id: "otel-inbound-meeting".to_string(),
        mc_id: "otel-inbound-mc".to_string(),
        mc_grpc_endpoint: "http://mc:50052".to_string(),
    });
    let auth_value: MetadataValue<_> = format!("Bearer {token}")
        .parse()
        .expect("authorization header parses");
    request.metadata_mut().insert("authorization", auth_value);
    let tp_value: MetadataValue<_> = known_traceparent_header()
        .parse()
        .expect("traceparent header parses");
    request.metadata_mut().insert("traceparent", tp_value);

    let response = client
        .register_meeting(request)
        .await
        .expect("RegisterMeeting should succeed");
    assert!(response.into_inner().accepted);

    let span = capture
        .find_span("register_meeting")
        .expect("register_meeting handler span should have been exported");
    assert_eq!(
        trace_id_hex(&span),
        format!("{KNOWN_TRACE_ID_U128:032x}"),
        "handler span should be re-parented to the injected trace id"
    );

    // Security (user-story line 411): the extraction path must never copy
    // `authorization` (or any bearer material) into span attributes.
    for kv in &span.attributes {
        let key = kv.key.as_str();
        let value = format!("{:?}", kv.value);
        assert!(
            !key.contains("authorization") && !key.contains("auth.token"),
            "span attribute key {key:?} suggests an auth leak"
        );
        assert!(
            !value.contains("Bearer") && !value.contains(&token),
            "span attribute value on {key:?} contains auth token material"
        );
    }
}

/// No inbound `traceparent`/`tracestate` metadata ⇒ the extraction step is a
/// no-op (matches `BoundedTraceContextPropagator::extract`'s documented
/// "input unchanged when no headers present" behavior): the handler span
/// picks up a freshly-sampled trace_id, NOT the fixed
/// [`KNOWN_TRACE_ID_U128`] the other inbound test injects. (The handler
/// span's immediate PARENT is still `SpanLayer`'s own `mh.grpc.request` span
/// either way — that nesting is an MH-internal ambient-span implementation
/// detail, not an inbound trace; see `grpc::span_layer` module docs. The
/// trace_id is the right axis to assert "no foreign trace was adopted" on.)
#[tokio::test]
async fn test_inbound_register_meeting_no_traceparent_gets_fresh_trace_id() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let jwks = JwksRig::start(43, "mh-otel-integ-02").await;
    let session_manager = SessionManagerHandle::new();
    let rig = GrpcRig::start(jwks.jwks_client(), session_manager.clone()).await;
    let mut client = connect_client(&rig).await;

    let token = mint_valid_mc_token(&jwks.keypair);
    let mut request = Request::new(RegisterMeetingRequest {
        meeting_id: "otel-inbound-meeting-noop".to_string(),
        mc_id: "otel-inbound-mc-noop".to_string(),
        mc_grpc_endpoint: "http://mc:50052".to_string(),
    });
    let auth_value: MetadataValue<_> = format!("Bearer {token}")
        .parse()
        .expect("authorization header parses");
    request.metadata_mut().insert("authorization", auth_value);
    // No traceparent/tracestate metadata set — today's default when a
    // caller doesn't propagate a trace.

    let response = client
        .register_meeting(request)
        .await
        .expect("RegisterMeeting should succeed");
    assert!(response.into_inner().accepted);

    let span = capture
        .find_span("register_meeting")
        .expect("register_meeting handler span should have been exported");
    assert_ne!(
        trace_id_hex(&span),
        format!("{KNOWN_TRACE_ID_U128:032x}"),
        "no inbound traceparent must NOT adopt the fixed trace id used by the positive-path test \
         — it should be a freshly sampled trace"
    );
}
