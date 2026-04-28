// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — do not "simplify" it away. `MetricAssertion`
// binds a per-thread recorder at snapshot time; the accept-path counter, the
// session-join histogram, and the spawned connection handler all emit from
// inside a `tokio::spawn` task. On `current_thread` those tasks share the test
// thread so the recorder captures them; on `flavor = "multi_thread"` they land
// on worker threads and the assertions silently observe zero (and
// `assert_delta(1)` passes as 0 without error). See
// `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for the real `WebTransportServer::accept_loop` (MC-side).
//!
//! Drives the production accept-loop byte-identically to `main.rs:376-388`
//! via `tests/common/accept_loop_rig.rs`, asserting on the three accept-path
//! status labels of `mc_webtransport_connections_total` (`accepted`,
//! `rejected`, `error`) plus companion `mc_jwt_validations_total` and
//! `mc_session_join_duration_seconds` emissions.
//!
//! Per @test review of the plan (ADR-0032 Step 3 §F2/F3):
//! - Negative `assert_delta(0)` adjacency on sibling labels — catches
//!   label-swap bugs (e.g., a refactor that flips `accepted` and `rejected`)
//! - Histogram-first ordering enforced — drain-on-read invariant
//! - Tests use real injected faults (capacity = 1, expired JWT, ...) — no
//!   wrapper shortcuts.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use bytes::{BufMut, BytesMut};
use mc_service::grpc::MhRegistrationClient;
use mc_service::redis::{MhAssignmentData, MhAssignmentStore, MhEndpointInfo};
use mc_test_utils::jwt_test::{make_expired_meeting_claims, make_meeting_claims, TestKeypair};
use prost::Message;
use proto_gen::signaling::{client_message, ClientMessage, JoinRequest};
use wtransport::{ClientConfig, Endpoint};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::{build_test_stack, TestStackHandles};

// ---------------------------------------------------------------------------
// Shared bring-up — thin wrapper around `build_test_stack` (in
// `tests/common/mod.rs`) per @dry-reviewer F-DRY-1. The stack itself is
// identical to what `join_tests.rs::TestServer::start` builds; only the
// keypair label and `max_connections` axis differ.
// ---------------------------------------------------------------------------

struct RigBundle {
    rig: AcceptLoopRig,
    stack: TestStackHandles,
}

async fn start_rig(max_connections: usize) -> RigBundle {
    let stack = build_test_stack("mc-accept-loop-test").await;

    let rig = AcceptLoopRig::start_with(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
        Arc::clone(&stack.mh_store) as Arc<dyn MhAssignmentStore>,
        Arc::clone(&stack.mh_reg_client) as Arc<dyn MhRegistrationClient>,
        "mc-test".to_string(),
        "http://mc-test:50052".to_string(),
        max_connections,
    )
    .await;

    RigBundle { rig, stack }
}

async fn create_meeting_with_mh(bundle: &RigBundle, meeting_id: &str) {
    // Equivalent to `seed_meeting_with_mh(&bundle.stack, meeting_id)` — the
    // shared helper in `tests/common/mod.rs` does the same insert + actor
    // create_meeting. Kept as a local fn for the readability of the call
    // sites below.
    test_common::seed_meeting_with_mh(&bundle.stack, meeting_id).await;
}

fn build_client() -> Endpoint<wtransport::endpoint::endpoint_side::Client> {
    let cfg = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    Endpoint::client(cfg).expect("client endpoint")
}

async fn open_bi_send_join(
    rig_url: &str,
    meeting_id: &str,
    token: &str,
) -> (
    wtransport::Connection,
    wtransport::stream::SendStream,
    wtransport::stream::RecvStream,
) {
    let client = build_client();
    let conn = client.connect(rig_url).await.expect("client connect");
    let (mut send, recv) = conn
        .open_bi()
        .await
        .expect("open_bi")
        .await
        .expect("bi stream ready");

    let msg = ClientMessage {
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: token.to_string(),
            participant_name: "AcceptLoopTester".to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
        })),
    };
    let encoded = msg.encode_to_vec();
    let mut frame = BytesMut::with_capacity(4 + encoded.len());
    frame.put_u32(encoded.len() as u32);
    frame.put_slice(&encoded);
    send.write_all(&frame).await.expect("write JoinRequest");

    (conn, send, recv)
}

// ---------------------------------------------------------------------------
// Accept-path status labels: `accepted` | `rejected` | `error`
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn accept_loop_emits_accepted_status_and_session_join_observation_on_happy_path() {
    // Valid JWT + pre-registered meeting + non-empty MH assignment → handler
    // reaches `connection.rs:391` and records:
    //   - `mc_webtransport_connections_total{status=accepted}` delta=1 (server.rs:183)
    //   - `mc_jwt_validations_total{result=success,token_type=meeting}` delta=1
    //   - `mc_session_join_duration_seconds{status=success}` at-least-one observation
    //   - sibling `status=rejected|error` for `mc_webtransport_connections_total` delta=0
    let bundle = start_rig(2).await;
    create_meeting_with_mh(&bundle, "meeting-accept-ok").await;

    let snap = MetricAssertion::snapshot();
    let claims = make_meeting_claims("meeting-accept-ok");
    let token = bundle.stack.keypair.sign_token(&claims);
    let (_conn, _send, _recv) =
        open_bi_send_join(&bundle.rig.url, "meeting-accept-ok", &token).await;

    // Bounded window for the WT stream completion to fire JoinResponse
    // synchronously on the connection task. 300ms is plenty on
    // current_thread for the spawned `handle_connection` to complete the
    // session_join_seconds histogram observation and the
    // mc_session_joins_total counter increment.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Histogram first (drain-on-read).
    snap.histogram("mc_session_join_duration_seconds")
        .with_labels(&[("status", "success")])
        .assert_observation_count_at_least(1);

    snap.counter("mc_webtransport_connections_total")
        .with_labels(&[("status", "accepted")])
        .assert_delta(1);
    // Adjacency — accept happy-path must NOT emit rejected/error.
    snap.counter("mc_webtransport_connections_total")
        .with_labels(&[("status", "rejected")])
        .assert_delta(0);
    snap.counter("mc_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);

    // Companion JWT validation: success/meeting/none.
    snap.counter("mc_jwt_validations_total")
        .with_labels(&[
            ("result", "success"),
            ("token_type", "meeting"),
            ("failure_reason", "none"),
        ])
        .assert_delta(1);

    snap.counter("mc_session_joins_total")
        .with_labels(&[("status", "success")])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn accept_loop_emits_rejected_status_when_at_capacity() {
    // `max_connections = 1`. Open conn #1 (valid JWT, pre-registered meeting,
    // non-empty MH assignment) and hold it live. Open conn #2 — `accept_loop`
    // sees `active_connections >= max_connections` at server.rs:170, emits
    // `status=rejected`, and drops `incoming_session` without accepting.
    let bundle = start_rig(1).await;
    create_meeting_with_mh(&bundle, "meeting-rejected").await;

    let snap = MetricAssertion::snapshot();
    let claims = make_meeting_claims("meeting-rejected");
    let token1 = bundle.stack.keypair.sign_token(&claims);

    // Conn #1 — must land. Hold it live for the rest of the test.
    let (_conn1, _send1, _recv1) =
        open_bi_send_join(&bundle.rig.url, "meeting-rejected", &token1).await;
    // Bounded sleep (300ms is plenty on current_thread) for the spawned
    // handle_connection to promote to the meeting and bring active_connections
    // to >= max_connections=1 before we attempt conn #2.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Conn #2 — attempt to connect; accept loop should reject at capacity.
    // The rejection emission happens on the accept task synchronously
    // (server.rs:178), no spawn. On current_thread it lands before the
    // sleep-window expires.
    let client = build_client();
    let _conn2_result =
        tokio::time::timeout(Duration::from_secs(3), client.connect(&bundle.rig.url))
            .await
            .ok();

    tokio::time::sleep(Duration::from_millis(300)).await;

    snap.counter("mc_webtransport_connections_total")
        .with_labels(&[("status", "rejected")])
        .assert_delta(1);
}

#[tokio::test(flavor = "current_thread")]
async fn accept_loop_emits_error_status_when_handler_returns_err() {
    // Valid handshake to the WT layer but expired JWT — handler reaches
    // `connection.rs:215` JWT-validation branch and returns `Err(e)`.
    // accept_loop emits `status=error` when the spawned task exits with Err
    // at server.rs:209.
    let bundle = start_rig(8).await;
    create_meeting_with_mh(&bundle, "meeting-error").await;

    let snap = MetricAssertion::snapshot();
    let claims = make_expired_meeting_claims("meeting-error");
    let token = bundle.stack.keypair.sign_token(&claims);
    let (_conn, _send, _recv) = open_bi_send_join(&bundle.rig.url, "meeting-error", &token).await;

    // Wait for the spawned handler to complete and accept_loop to emit.
    tokio::time::sleep(Duration::from_millis(500)).await;

    snap.counter("mc_webtransport_connections_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);

    // Companion JWT validation failure: meeting/signature_invalid.
    // Per code-reviewer F1: past-`exp` lands in JwtError::InvalidSignature
    // (caught at common/src/jwt.rs:1027-1030 by `validation.validate_exp = true`),
    // which connection.rs:215 maps to `failure_reason=signature_invalid`.
    snap.counter("mc_jwt_validations_total")
        .with_labels(&[
            ("result", "failure"),
            ("token_type", "meeting"),
            ("failure_reason", "signature_invalid"),
        ])
        .assert_delta(1);

    // Production semantics: accept_loop emits `accepted` synchronously at
    // server.rs:181/183 BEFORE spawning the handler task. The spawned
    // handler then errors and accept_loop emits `error` at server.rs:209
    // once the JoinHandle resolves Err. Both counters fire exactly once on
    // this path — the test asserts both to lock the (accept→error) ordering
    // invariant.
    snap.counter("mc_webtransport_connections_total")
        .with_labels(&[("status", "accepted")])
        .assert_delta(1);
}

// ---------------------------------------------------------------------------
// Per-failure-class drilldown: `mc_session_join_failures_total{error_type}`
//
// Each test injects a real production fault and asserts both the named
// `error_type` delta=1 AND adjacency `(0)` on three siblings (label-swap-bug
// catcher per @test review). Histogram-first ordering enforced.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn accept_loop_records_session_join_failure_meeting_not_found() {
    // Meeting NOT registered with controller → `controller_handle.join_connection`
    // returns `McError::MeetingNotFound`, which `error_type_label()` maps to
    // `"meeting_not_found"`.
    let bundle = start_rig(8).await;
    // Insert MH assignment but DO NOT create_meeting on the controller.
    bundle.stack.mh_store.insert(
        "meeting-not-found",
        MhAssignmentData {
            handlers: vec![MhEndpointInfo {
                mh_id: "mh-test-1".to_string(),
                webtransport_endpoint: "wt://mh-test-1:4433".to_string(),
                grpc_endpoint: "http://mh-test-1:50053".to_string(),
            }],
            assigned_at: "2026-04-25T00:00:00Z".to_string(),
        },
    );

    let snap = MetricAssertion::snapshot();
    let claims = make_meeting_claims("meeting-not-found");
    let token = bundle.stack.keypair.sign_token(&claims);
    let (_conn, _send, _recv) =
        open_bi_send_join(&bundle.rig.url, "meeting-not-found", &token).await;

    tokio::time::sleep(Duration::from_millis(400)).await;

    snap.histogram("mc_session_join_duration_seconds")
        .with_labels(&[("status", "failure")])
        .assert_observation_count_at_least(1);

    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", "meeting_not_found")])
        .assert_delta(1);
    // Adjacency on the other rig-reachable error_types.
    for sibling in &["jwt_validation", "mh_assignment_missing", "internal"] {
        snap.counter("mc_session_join_failures_total")
            .with_labels(&[("error_type", *sibling)])
            .assert_delta(0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn accept_loop_records_session_join_failure_mh_assignment_missing() {
    // Meeting created on controller but mh_store is empty for that meeting →
    // `build_join_response` fails with `McError::MhAssignmentMissing`.
    let bundle = start_rig(8).await;
    bundle
        .rig
        .controller_handle
        .create_meeting("meeting-no-mh".to_string())
        .await
        .expect("create_meeting");

    let snap = MetricAssertion::snapshot();
    let claims = make_meeting_claims("meeting-no-mh");
    let token = bundle.stack.keypair.sign_token(&claims);
    let (_conn, _send, _recv) = open_bi_send_join(&bundle.rig.url, "meeting-no-mh", &token).await;

    tokio::time::sleep(Duration::from_millis(400)).await;

    snap.histogram("mc_session_join_duration_seconds")
        .with_labels(&[("status", "failure")])
        .assert_observation_count_at_least(1);

    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", "mh_assignment_missing")])
        .assert_delta(1);
    for sibling in &["jwt_validation", "meeting_not_found", "internal"] {
        snap.counter("mc_session_join_failures_total")
            .with_labels(&[("error_type", *sibling)])
            .assert_delta(0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn accept_loop_records_session_join_failure_jwt_validation() {
    // Wrong-signing-key JWT → `validate_meeting_token` returns
    // `Err(JwtValidation)` at `connection.rs:215`, which `error_type_label()`
    // maps to `"jwt_validation"`. Distinct from the `mc_jwt_validations_total`
    // counter which we already cover in the accepted/error happy-path tests.
    let bundle = start_rig(8).await;
    create_meeting_with_mh(&bundle, "meeting-jwt-fail").await;

    let snap = MetricAssertion::snapshot();
    let wrong_keypair = TestKeypair::new(99, "wrong-key");
    let claims = make_meeting_claims("meeting-jwt-fail");
    let token = wrong_keypair.sign_token(&claims);
    let (_conn, _send, _recv) =
        open_bi_send_join(&bundle.rig.url, "meeting-jwt-fail", &token).await;

    tokio::time::sleep(Duration::from_millis(400)).await;

    snap.histogram("mc_session_join_duration_seconds")
        .with_labels(&[("status", "failure")])
        .assert_observation_count_at_least(1);

    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", "jwt_validation")])
        .assert_delta(1);
    for sibling in &["meeting_not_found", "mh_assignment_missing", "internal"] {
        snap.counter("mc_session_join_failures_total")
            .with_labels(&[("error_type", *sibling)])
            .assert_delta(0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn accept_loop_records_session_join_failure_internal_for_wrong_first_message() {
    // First message is a MuteRequest, not a JoinRequest → `connection.rs:160`
    // returns `McError::Internal("Expected JoinRequest as first message")`.
    use proto_gen::signaling::MuteRequest;

    let bundle = start_rig(8).await;
    create_meeting_with_mh(&bundle, "meeting-wrong-first").await;

    let snap = MetricAssertion::snapshot();

    let client = build_client();
    let conn = client
        .connect(&bundle.rig.url)
        .await
        .expect("client connect");
    let (mut send, _recv) = conn
        .open_bi()
        .await
        .expect("open_bi")
        .await
        .expect("bi stream ready");

    let mute_msg = ClientMessage {
        message: Some(client_message::Message::MuteRequest(MuteRequest {
            audio_muted: true,
            video_muted: false,
        })),
    };
    let encoded = mute_msg.encode_to_vec();
    let mut frame = BytesMut::with_capacity(4 + encoded.len());
    frame.put_u32(encoded.len() as u32);
    frame.put_slice(&encoded);
    send.write_all(&frame).await.expect("write MuteRequest");

    tokio::time::sleep(Duration::from_millis(400)).await;

    snap.histogram("mc_session_join_duration_seconds")
        .with_labels(&[("status", "failure")])
        .assert_observation_count_at_least(1);

    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", "internal")])
        .assert_delta(1);
    for sibling in &[
        "jwt_validation",
        "meeting_not_found",
        "mh_assignment_missing",
    ] {
        snap.counter("mc_session_join_failures_total")
            .with_labels(&[("error_type", *sibling)])
            .assert_delta(0);
    }
}
