//! Integration tests for R-60 (MC half): a real framed
//! `ClientMessage{MediaConnectionUpdate}` driven through the post-join
//! `handle_client_message` decode+dispatch seam records per-MH state on the
//! participant actor and emits `mc_participant_mh_status_total{state}`.
//!
//! Asserts on the metric (the cluster-observable effect) via `MetricAssertion`;
//! the per-state map contents + cap + mutual-exclusivity are exercised
//! deterministically in `actors::participant::tests`. Truncation is exercised
//! here end-to-end (a hostile >256-byte `mh_url` must not panic the seam).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use bytes::{BufMut, BytesMut};
use common::observability::testing::MetricAssertion;
use mc_service::grpc::MhRegistrationClient;
use mc_service::redis::MhAssignmentStore;
use mc_test_utils::jwt_test::make_meeting_claims;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, ConnectionState, JoinRequest,
    MediaConnectionUpdate, MhConnectionStatus, ServerMessage,
};
use wtransport::{ClientConfig, Endpoint};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::{build_test_stack, seed_meeting_with_mh, TestStackHandles};

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

fn join_frame(meeting_id: &str, join_token: &str) -> Vec<u8> {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: join_token.to_string(),
            participant_name: "Alice".to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
        })),
    })
}

fn status(url: &str, state: ConnectionState) -> MhConnectionStatus {
    MhConnectionStatus {
        mh_url: url.to_string(),
        state: state as i32,
        failure_reason: None,
        failure_code: None,
        observed_at: None,
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
            status("https://mh-1", ConnectionState::Connected),
            status("https://mh-2", ConnectionState::Connected),
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
            status("https://mh-1", ConnectionState::Connected),
            status("https://mh-2", ConnectionState::Failed),
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
            status("https://mh-1", ConnectionState::Failed),
            status("https://mh-2", ConnectionState::Failed),
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

/// Security (gotcha #6): a hostile >256-byte multi-byte `mh_url` (and
/// `failure_reason`) must be truncated at the seam without panicking, and the
/// status is still recorded (metric moves).
#[tokio::test]
async fn test_media_connection_update_oversized_strings_truncated_no_panic() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-truncate").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-d").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-d"));

    // ~3KB multi-byte string (each '€' is 3 bytes) as mh_url + failure_reason.
    let hostile = "€".repeat(1000);
    let mut hostile_status = status(&hostile, ConnectionState::Failed);
    hostile_status.failure_reason = Some(hostile.clone());

    let _conn = join_and_send_update(&rig.url, "mcu-meeting-d", &token, vec![hostile_status]).await;

    // The seam did not panic and the status was recorded.
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "failed")])
        .assert_delta(1);
}

/// Security (cap): a single `MediaConnectionUpdate` reporting more distinct MH
/// URLs than the per-participant cap (`MAX_MH_STATUSES_PER_PARTICIPANT = 16`)
/// records the first 16 and drops the rest, bumping
/// `mc_participant_mh_status_dropped_total{reason="cap"}` — end-to-end through
/// the real WT seam (the deterministic unit-level cap semantics are in
/// `actors::participant::tests`).
#[tokio::test]
async fn test_media_connection_update_over_cap_drops_and_counts() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-cap").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-e").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-e"));

    // 17 distinct MH URLs in one update → 16 recorded, 1 refused by the cap.
    let statuses: Vec<MhConnectionStatus> = (0..17)
        .map(|i| status(&format!("https://mh-cap-{i}"), ConnectionState::Connected))
        .collect();

    let _conn = join_and_send_update(&rig.url, "mcu-meeting-e", &token, statuses).await;

    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(16);
    snap.counter("mc_participant_mh_status_dropped_total")
        .with_labels(&[("reason", "cap")])
        .assert_delta(1);
}

/// Security (over_limit): a single `MediaConnectionUpdate` carrying more than
/// `MAX_MH_STATUSES_PER_UPDATE = 64` statuses is bounded at the seam BEFORE any
/// per-entry work — the excess is refused and
/// `mc_participant_mh_status_dropped_total{reason="over_limit"}` bumps once.
/// Of the 64 processed, only the map cap (16) land as `connected`; the other
/// 48 hit the per-entry `cap` drop. Guards against input amplification (a
/// 64 KiB frame can pack tens of thousands of minimal statuses).
#[tokio::test]
async fn test_media_connection_update_over_message_limit_drops_and_counts() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcu-overlimit").await;
    seed_meeting_with_mh(&stack, "mcu-meeting-f").await;
    let token = stack
        .keypair
        .sign_token(&make_meeting_claims("mcu-meeting-f"));

    // 70 distinct MH URLs in ONE message (> the 64 per-message bound).
    let statuses: Vec<MhConnectionStatus> = (0..70)
        .map(|i| status(&format!("https://mh-ovl-{i}"), ConnectionState::Connected))
        .collect();

    let _conn = join_and_send_update(&rig.url, "mcu-meeting-f", &token, statuses).await;

    // The batch exceeded the per-message bound → one over_limit drop.
    snap.counter("mc_participant_mh_status_dropped_total")
        .with_labels(&[("reason", "over_limit")])
        .assert_delta(1);
    // Only the map cap lands; the 64 processed minus 16 landed = 48 cap drops.
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(16);
    snap.counter("mc_participant_mh_status_dropped_total")
        .with_labels(&[("reason", "cap")])
        .assert_delta(48);
}
