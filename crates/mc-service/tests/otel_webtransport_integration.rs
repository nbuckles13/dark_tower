//! Integration tests for R-57 (MC side): `ClientMessage` `trace_parent`/
//! `trace_state` extraction at the `webtransport/connection.rs` JoinRequest
//! dispatch site.
//!
//! Drives the real `WebTransportServer::bind() → accept_loop()` (via
//! `AcceptLoopRig`, byte-identical to `main.rs`), sends a real `JoinRequest`
//! carrying a W3C `traceparent`, and asserts on the EXPORTED
//! `mc.webtransport.connection` span — not on the extraction helper in
//! isolation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{BufMut, BytesMut};
use mc_service::grpc::MhRegistrationClient;
use mc_service::redis::MhAssignmentStore;
use mc_test_utils::jwt_test::make_meeting_claims;
use opentelemetry::trace::SpanId;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{client_message, ClientMessage, JoinRequest};
use wtransport::{ClientConfig, Endpoint};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::otel_capture::{
    install_test_propagator, known_trace_id_hex, known_traceparent, trace_id_hex, SpanCapture,
};
use test_common::{build_test_stack, seed_meeting_with_mh, TestStackHandles};

/// The W3C `trace_parent` value MC extracts is the `ClientMessage.trace_parent`
/// field, which the SDK populates with the same `00-<trace>-<span>-<flags>`
/// header form — reuse the shared `known_traceparent()`.
fn known_client_trace_parent() -> String {
    known_traceparent()
}

async fn start_stack(label: &str) -> (TestStackHandles, AcceptLoopRig) {
    let stack = build_test_stack(label).await;
    let rig = AcceptLoopRig::start_with(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
        Arc::clone(&stack.mh_store) as Arc<dyn MhAssignmentStore>,
        Arc::clone(&stack.mh_reg_client) as Arc<dyn MhRegistrationClient>,
        "mc-test".to_string(),
        "http://mc-test:50052".to_string(),
        32,
    )
    .await;
    (stack, rig)
}

async fn connect_client(url: &str) -> wtransport::Connection {
    let client_config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    let client = Endpoint::client(client_config).expect("client endpoint");
    client.connect(url).await.expect("client connect")
}

fn encode_framed(msg: &ClientMessage) -> Vec<u8> {
    let encoded = msg.encode_to_vec();
    let len = encoded.len() as u32;
    let mut frame = BytesMut::with_capacity(4 + encoded.len());
    frame.put_u32(len);
    frame.put_slice(&encoded);
    frame.to_vec()
}

/// Send a JoinRequest carrying the given trace fields, then close the
/// connection so `handle_connection`'s span ends and exports.
async fn join_with_trace(
    url: &str,
    meeting_id: &str,
    join_token: &str,
    trace_parent: &str,
    trace_state: &str,
) {
    let conn = connect_client(url).await;
    let (mut send, _recv) = conn
        .open_bi()
        .await
        .expect("open bi")
        .await
        .expect("bi ready");

    let client_msg = ClientMessage {
        trace_parent: trace_parent.to_string(),
        trace_state: trace_state.to_string(),
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: join_token.to_string(),
            participant_name: "Alice".to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
        })),
    };
    send.write_all(&encode_framed(&client_msg))
        .await
        .expect("write join");
    // Give the server a moment to process the join before we drop the conn.
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(send);
    drop(conn);
}

/// Poll the capture until the connection span exports (it exports on close).
async fn await_connection_span(
    capture: &SpanCapture,
    deadline: Duration,
) -> opentelemetry_sdk::export::trace::SpanData {
    let stop = Instant::now() + deadline;
    loop {
        if let Some(span) = capture.find_span("mc.webtransport.connection") {
            return span;
        }
        assert!(
            Instant::now() < stop,
            "mc.webtransport.connection span was never exported"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn test_wt_valid_traceparent_reparents_connection_span() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let (stack, rig) = start_stack("otel-wt-valid").await;
    seed_meeting_with_mh(&stack, "otel-wt-meeting").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("otel-wt-meeting"));

    join_with_trace(
        &rig.url,
        "otel-wt-meeting",
        &token,
        &known_client_trace_parent(),
        "",
    )
    .await;

    let span = await_connection_span(&capture, Duration::from_secs(5)).await;
    assert_eq!(
        trace_id_hex(&span),
        known_trace_id_hex(),
        "connection span must be re-parented to the JoinRequest's injected trace id"
    );
}

/// Empty proto3-default `trace_parent`/`trace_state` must be a no-op: the
/// connection span stays a fresh parentless root (regression guard that R-57
/// didn't change existing no-trace behavior).
#[tokio::test]
async fn test_wt_empty_trace_fields_stays_parentless_root_span() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let (stack, rig) = start_stack("otel-wt-empty").await;
    seed_meeting_with_mh(&stack, "otel-wt-meeting-noop").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("otel-wt-meeting-noop"));

    join_with_trace(&rig.url, "otel-wt-meeting-noop", &token, "", "").await;

    let span = await_connection_span(&capture, Duration::from_secs(5)).await;
    assert_eq!(
        span.parent_span_id,
        SpanId::INVALID,
        "empty trace fields must be a no-op — connection span must have no parent, got {:?}",
        span.parent_span_id
    );
}

/// Security (R-57): a JoinRequest carrying a real JWT in `join_token` +
/// a valid `traceparent` reparents the span but leaks NO token material into
/// the span's attributes. The connection span only ever carries
/// `connection_id`.
#[tokio::test]
async fn test_wt_no_auth_leak_in_connection_span_attributes() {
    install_test_propagator();
    let capture = SpanCapture::install();

    let (stack, rig) = start_stack("otel-wt-noleak").await;
    seed_meeting_with_mh(&stack, "otel-wt-meeting-leak").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("otel-wt-meeting-leak"));

    join_with_trace(
        &rig.url,
        "otel-wt-meeting-leak",
        &token,
        &known_client_trace_parent(),
        "",
    )
    .await;

    let span = await_connection_span(&capture, Duration::from_secs(5)).await;
    // Reparent still worked...
    assert_eq!(trace_id_hex(&span), known_trace_id_hex());
    // ...and NO attribute key or value contains the token or "join_token".
    for kv in &span.attributes {
        let key = kv.key.as_str();
        let value = kv.value.as_str();
        assert!(
            !key.contains("join_token") && !key.contains("token"),
            "span attribute key {key:?} unexpectedly references a token"
        );
        assert!(
            !value.contains(token.as_str()),
            "span attribute {key:?} unexpectedly contains the JWT value"
        );
    }
}
