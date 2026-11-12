//! Integration tests for R-60 (MC half): a real framed
//! `ClientMessage{MediaConnectionUpdate}` driven through the post-join
//! `handle_client_message` decode+dispatch seam emits
//! `mc_participant_mh_status_total{state}` once per status (the
//! `MCMediaConnectionAllFailed` alert input).
//!
//! Asserts on the metric (the cluster-observable effect) via `MetricAssertion`;
//! the deterministic per-state emission is exercised in
//! `webtransport::connection::tests`. Hostile client strings are exercised here
//! end-to-end (a >256-byte `failure_reason` must not panic the seam).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use bytes::{BufMut, BytesMut};
use common::observability::testing::MetricAssertion;
use mc_service::grpc::MhRegistrationClient;
use mc_test_utils::jwt_test::make_meeting_claims;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, ConnectionState, JoinRequest,
    MediaConnectionUpdate, MhConnectionStatus, ServerMessage,
};
use wtransport::{ClientConfig, Endpoint};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::{
    build_test_stack, sample_identity_public_key, seed_meeting_with_mh, TestStackHandles,
};

async fn start_stack(label: &str) -> (TestStackHandles, AcceptLoopRig) {
    let stack = build_test_stack(label).await;
    let rig = AcceptLoopRig::start_with(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
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

fn join_frame(meeting_id: &str, join_token: &str) -> Vec<u8> {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: join_token.to_string(),
            participant_name: "Alice".to_string(),
            capabilities: Some(
                proto_gen::dark_tower::signaling::v1::ParticipantCapabilities {
                    supported_codecs: Vec::new(),
                    // v3 is the only header version (story 3 R-22).
                    supported_header_versions: vec![u32::from(
                        media_protocol::frame::PROTOCOL_VERSION,
                    )],
                },
            ),
            correlation_id: String::new(),
            binding_token: String::new(),
            // ADR-0036 §4: raw Ed25519 identity signing public key. Empty here —
            // this test exercises the join path, not attribution, and MC performs
            // no attestation check this story.
            identity_public_key: sample_identity_public_key(),
            // Story 3 R-2: the suite publishes audio (story-2 shape).
            published_kinds: vec![proto_gen::dark_tower::signaling::v1::PublishedKind {
                media_kind: proto_gen::dark_tower::signaling::v1::MediaKind::Audio as i32,
                client_muted: false,
            }],
            receive_capability: None,
        })),
    })
}

fn status(handler_ref: u32, state: ConnectionState) -> MhConnectionStatus {
    MhConnectionStatus {
        state: state as i32,
        failure_reason: None,
        failure_code: None,
        observed_at: None,
        handler_ref,
        handler_set_version: 0,
    }
}

fn update_frame(statuses: Vec<MhConnectionStatus>) -> Vec<u8> {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::MediaConnectionUpdate(
            MediaConnectionUpdate { statuses },
        )),
    })
}

async fn read_server_message(recv: &mut wtransport::stream::RecvStream) -> ServerMessage {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await.expect("len prefix");
    let msg_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; msg_len];
    recv.read_exact(&mut buf).await.expect("body");
    ServerMessage::decode(buf.as_slice()).expect("decode ServerMessage")
}

/// Join a meeting, then send `statuses` as a `MediaConnectionUpdate` on the same
/// bidi stream. Keeps the connection alive for the caller to assert on the
/// resulting metric, returning the live connection so it isn't dropped early.
async fn join_and_send_update(
    url: &str,
    meeting_id: &str,
    join_token: &str,
    statuses: Vec<MhConnectionStatus>,
) -> wtransport::Connection {
    let conn = connect_client(url).await;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .expect("open bi")
        .await
        .expect("bi ready");

    send.write_all(&join_frame(meeting_id, join_token))
        .await
        .expect("write join");
    // Confirm the join landed (participant actor exists, bridge loop running).
    let resp = tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv))
        .await
        .expect("join response timeout");
    assert!(
        matches!(resp.message, Some(server_message::Message::JoinResponse(_))),
        "expected JoinResponse, got {:?}",
        resp.message
    );

    send.write_all(&update_frame(statuses))
        .await
        .expect("write update");
    // Let the bridge loop read the frame and the actor process it. The
    // connection MUST stay open across this settle so the bridge loop is alive.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Keep `send`/`recv` alive by moving them into the returned connection's
    // scope via forget-free drop ordering: drop the streams, return the conn.
    drop(send);
    drop(recv);
    conn
}

#[tokio::test]
async fn test_media_connection_update_all_connected_records_metric() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-all-connected").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-a").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-a"));

    let _conn = join_and_send_update(
        &rig.url,
        "mcu-meeting-a",
        &token,
        vec![
            status(1, ConnectionState::Connected),
            status(2, ConnectionState::Connected),
        ],
    )
    .await;

    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(2);
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "failed")])
        .assert_delta(0);
}

#[tokio::test]
async fn test_media_connection_update_partial_records_per_state() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-partial").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-b").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-b"));

    let _conn = join_and_send_update(
        &rig.url,
        "mcu-meeting-b",
        &token,
        vec![
            status(1, ConnectionState::Connected),
            status(2, ConnectionState::Failed),
        ],
    )
    .await;

    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(1);
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "failed")])
        .assert_delta(1);
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "disconnected")])
        .assert_delta(0);
}

#[tokio::test]
async fn test_media_connection_update_all_failed_records_metric() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-all-failed").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-c").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-c"));

    let _conn = join_and_send_update(
        &rig.url,
        "mcu-meeting-c",
        &token,
        vec![
            status(1, ConnectionState::Failed),
            status(2, ConnectionState::Failed),
        ],
    )
    .await;

    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "failed")])
        .assert_delta(2);
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(0);
}

/// Security (gotcha #6): hostile >256-byte multi-byte `failure_reason` /
/// `failure_code` strings are dropped unread at the seam without panicking,
/// and the status is still counted (metric moves).
#[tokio::test]
async fn test_media_connection_update_oversized_strings_dropped_no_panic() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-truncate").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-d").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-d"));

    // ~3KB multi-byte string (each '€' is 3 bytes) as both client strings.
    let hostile = "€".repeat(1000);
    let mut hostile_status = status(1, ConnectionState::Failed);
    hostile_status.failure_reason = Some(hostile.clone());
    hostile_status.failure_code = Some(hostile.clone());

    let _conn = join_and_send_update(&rig.url, "mcu-meeting-d", &token, vec![hostile_status]).await;

    // The seam did not panic and the status was counted.
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "failed")])
        .assert_delta(1);
}

/// Every status in a within-bound update is counted, however many distinct
/// refs it names: there is no per-participant store to cap (each counts once,
/// and nothing is dropped).
#[tokio::test]
async fn test_media_connection_update_many_refs_all_counted() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-many").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-e").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-e"));

    let statuses: Vec<MhConnectionStatus> = (1..=17)
        .map(|r| status(r, ConnectionState::Connected))
        .collect();

    let _conn = join_and_send_update(&rig.url, "mcu-meeting-e", &token, statuses).await;

    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(17);
    snap.counter("mc_participant_mh_status_dropped_total")
        .with_labels(&[("reason", "over_limit")])
        .assert_delta(0);
}

/// Security (over_limit): a single `MediaConnectionUpdate` carrying more than
/// `MAX_MH_STATUSES_PER_UPDATE = 64` statuses is bounded at the seam BEFORE any
/// per-entry work — the excess is refused and
/// `mc_participant_mh_status_dropped_total{reason="over_limit"}` bumps once;
/// the 64 processed are each counted. Guards against input amplification (a
/// 64 KiB frame can pack tens of thousands of minimal statuses).
#[tokio::test]
async fn test_media_connection_update_over_message_limit_drops_and_counts() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-overlimit").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-f").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-f"));

    // 70 statuses in ONE message (> the 64 per-message bound).
    let statuses: Vec<MhConnectionStatus> = (1..=70)
        .map(|r| status(r, ConnectionState::Connected))
        .collect();

    let _conn = join_and_send_update(&rig.url, "mcu-meeting-f", &token, statuses).await;

    // The batch exceeded the per-message bound → one over_limit drop.
    snap.counter("mc_participant_mh_status_dropped_total")
        .with_labels(&[("reason", "over_limit")])
        .assert_delta(1);
    // Exactly the bounded prefix is counted.
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(64);
}
