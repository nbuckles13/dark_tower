//! Integration tests for R-58 (MH side): `MhClientMessage` envelope
//! `trace_parent`/`trace_state` extraction in
//! `webtransport/connection.rs::handle_connection`.
//!
//! Drives the real `WebTransportServer::bind() → accept_loop()` (via
//! `AcceptLoopRig`, byte-identical to `main.rs`), sends a real
//! `MhClientMessage` envelope over the wire, and asserts on the EXPORTED
//! `mh.webtransport.connection` span — not on the extraction helper in
//! isolation (there is none to isolate: it's inline in `handle_connection`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use mh_service::auth::MhJwtValidator;
use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;
use opentelemetry::trace::SpanId;

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::jwks_rig::JwksRig;
use test_common::otel_capture::{install_test_propagator, trace_id_hex, SpanCapture};
use test_common::test_token_receiver;
use test_common::tokens::mint_meeting_token;
use test_common::wt_client::{connect_and_open_bi, write_mh_connect_with_trace};

const KNOWN_TRACE_ID_U128: u128 = 0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736;
const KNOWN_SPAN_ID_U64: u64 = 0x00f0_67aa_0ba9_02b7;

fn known_traceparent_header() -> String {
    format!("00-{KNOWN_TRACE_ID_U128:032x}-{KNOWN_SPAN_ID_U64:016x}-01")
}

struct WtSuite {
    jwks: JwksRig,
    session_manager: SessionManagerHandle,
    wt: AcceptLoopRig,
}

impl WtSuite {
    async fn start() -> Self {
        let jwks = JwksRig::start(44, "mh-otel-wt-integ-01").await;
        let session_manager = SessionManagerHandle::new();
        let jwt_validator = Arc::new(MhJwtValidator::new(jwks.jwks_client(), 300));
        let mc_client = Arc::new(McClient::new(test_token_receiver()));

        let wt = AcceptLoopRig::start_with(
            jwt_validator,
            session_manager.clone(),
            mc_client,
            "mh-otel-wt-test-001".to_string(),
            32,
            Duration::from_secs(30),
        )
        .await;

        Self {
            jwks,
            session_manager,
            wt,
        }
    }
}

/// Bounded-deadline poll on `active_connection_count`, mirroring
/// `webtransport_integration.rs::wait_for_active_count`.
async fn wait_for_active_count(
    session_manager: &SessionManagerHandle,
    expected: usize,
    deadline: Duration,
) -> bool {
    let stop = Instant::now() + deadline;
    while Instant::now() < stop {
        if session_manager.active_connection_count().await == expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn test_wt_valid_traceparent_reparents_connection_span() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let suite = WtSuite::start().await;
    suite
        .session_manager
        .register_meeting(
            "otel-wt-meeting".to_string(),
            mh_service::session::MeetingRegistration {
                mc_id: "otel-wt-mc".to_string(),
                mc_grpc_endpoint: "http://localhost:1".to_string(),
                registered_at: Instant::now(),
            },
        )
        .await;

    let token = mint_meeting_token(&suite.jwks.keypair, "otel-wt-meeting", "otel-wt-user");
    let (conn, mut send, recv) = connect_and_open_bi(&suite.wt.url).await;
    write_mh_connect_with_trace(&mut send, &token, &known_traceparent_header(), "")
        .await
        .expect("failed to write MhClientMessage frame");

    assert!(
        wait_for_active_count(&suite.session_manager, 1, Duration::from_secs(3)).await,
        "accept path did not register an active connection within 3s",
    );

    // Close the connection so `handle_connection`'s monitor loop (Step 6)
    // observes the disconnect, returns, and its `#[instrument]` span ends —
    // spans only export on close (SimpleSpanProcessor).
    drop(recv);
    drop(send);
    drop(conn);

    let deadline = Instant::now() + Duration::from_secs(3);
    let span = loop {
        if let Some(span) = capture.find_span("mh.webtransport.connection") {
            break span;
        }
        assert!(
            Instant::now() < deadline,
            "mh.webtransport.connection span was never exported within 3s"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };

    assert_eq!(
        trace_id_hex(&span),
        format!("{KNOWN_TRACE_ID_U128:032x}"),
        "connection span should be re-parented to the envelope's injected trace id"
    );
}

/// Proto3-default empty `trace_parent`/`trace_state` (today's default, and
/// what every other WebTransport test's `write_mh_connect` sends) must be a
/// no-op: the connection span stays a fresh root, exactly as it behaved
/// before R-58. Unlike the gRPC inbound path (which always nests under
/// `SpanLayer`'s ambient span), nothing wraps `handle_connection` in an
/// outer span, so "no-op" here means a genuinely parentless span.
#[tokio::test]
async fn test_wt_empty_trace_fields_stays_parentless_root_span() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let suite = WtSuite::start().await;
    suite
        .session_manager
        .register_meeting(
            "otel-wt-meeting-noop".to_string(),
            mh_service::session::MeetingRegistration {
                mc_id: "otel-wt-mc-noop".to_string(),
                mc_grpc_endpoint: "http://localhost:1".to_string(),
                registered_at: Instant::now(),
            },
        )
        .await;

    let token = mint_meeting_token(
        &suite.jwks.keypair,
        "otel-wt-meeting-noop",
        "otel-wt-user-2",
    );
    let (conn, mut send, recv) = connect_and_open_bi(&suite.wt.url).await;
    // Empty trace_parent/trace_state (proto3 default) — same helper every
    // other WT test uses, so this is also an implicit regression guard that
    // R-58 didn't change existing no-trace-header behavior.
    write_mh_connect_with_trace(&mut send, &token, "", "")
        .await
        .expect("failed to write MhClientMessage frame");

    assert!(
        wait_for_active_count(&suite.session_manager, 1, Duration::from_secs(3)).await,
        "accept path did not register an active connection within 3s",
    );

    drop(recv);
    drop(send);
    drop(conn);

    let deadline = Instant::now() + Duration::from_secs(3);
    let span = loop {
        if let Some(span) = capture.find_span("mh.webtransport.connection") {
            break span;
        }
        assert!(
            Instant::now() < deadline,
            "mh.webtransport.connection span was never exported within 3s"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };

    assert_eq!(
        span.parent_span_id,
        SpanId::INVALID,
        "empty trace fields must be a no-op — connection span must have no parent, got {:?}",
        span.parent_span_id
    );
}
