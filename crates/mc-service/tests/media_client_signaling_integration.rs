//! Integration tests for MC's client-facing media signalling (ADR-0036 §5, §6).
//!
//! Every case drives a REAL framed `ClientMessage` through the post-join
//! decode+dispatch seam over a live WebTransport connection, and asserts on the
//! **wire bytes** MC sends back plus the **metric deltas** it records. Nothing
//! here asserts on a log line, and nothing calls a compose function directly —
//! the composition functions are covered by unit tests in
//! `media_signaling::{capability,directive,assignments}`; what these tests add is
//! that the seam is wired to them at all.
//!
//! # The two required cases
//!
//! - **(a)** receive-capability in -> send directive + slot assignment out, with
//!   the expected fields.
//! - **(b)** a mute/unmute cycle leaves the directive untouched.
//!
//! (b)'s primary control is structural, not this test:
//! `media_signaling::directive` has no path to mute state and
//! `build_send_directive` takes no mute argument, so a mute cycle altering the
//! directive is a compile error. This test is the backstop against a future
//! refactor that adds the parameter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use common::observability::testing::MetricAssertion;
use mc_service::grpc::MhRegistrationClient;
use mc_service::redis::MhAssignmentStore;
use mc_test_utils::jwt_test::make_meeting_claims;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, Codec, JoinRequest, MediaConnectionUpdate,
    MediaKind, MhConnectionStatus, MuteRequest, ReceiveCapability, ReceiveSlot, SendDirective,
    ServerMessage, SlotState, StreamAssignments, TransportMode,
};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::{
    build_test_stack, client_media_config, connect, encode_framed, read_server_message,
    sample_identity_public_key, seed_meeting_with_mh, TestStackHandles,
};

/// The handler url `seed_meeting_with_mh` seeds, which is what MC must put on
/// both the send target and the slot assignment.
const SEEDED_HANDLER_URL: &str = "wt://mh-test-1:4433";

/// A url no real handler could ever have (RFC 2606 `.invalid` never resolves),
/// used to poison the client-supplied MH-status map in the redirect test.
///
/// The sentinel must be impossible to collide with a real or future fixture
/// value, which is what makes searching the encoded bytes for it sound.
const ATTACKER_URL: &str = "https://attacker.example.invalid/redirect";

/// Let the bridge loop read and dispatch a client message that produces no
/// reply.
///
/// A `MediaConnectionUpdate` is answered only by a metric, so asserting straight
/// after the write races the read — which is how a premise assertion becomes
/// vacuous, passing sometimes for the wrong reason and failing sometimes for
/// one. Same 500 ms settle as `media_connection_update_integration.rs`, which is
/// the established shape for this seam.
///
/// A settle bound, NOT a performance assertion: no test in this file asserts a
/// wall-clock threshold (R-27 — latency is observed, never gated).
async fn settle_no_reply_message() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

async fn start_stack(label: &str) -> (TestStackHandles, AcceptLoopRig) {
    start_stack_with(label, client_media_config()).await
}

async fn start_stack_with(
    label: &str,
    media_config: mc_service::media_signaling::ClientMediaConfig,
) -> (TestStackHandles, AcceptLoopRig) {
    let stack = build_test_stack(label).await;
    let rig = AcceptLoopRig::start_with_media_config(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
        Arc::clone(&stack.mh_store) as Arc<dyn MhAssignmentStore>,
        Arc::clone(&stack.mh_reg_client) as Arc<dyn MhRegistrationClient>,
        "mc-test".to_string(),
        "http://mc-test:50052".to_string(),
        32,
        media_config,
    )
    .await;
    (stack, rig)
}

fn join_frame(meeting_id: &str, join_token: &str) -> bytes::BytesMut {
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
            identity_public_key: sample_identity_public_key(),
        })),
    })
}

fn slot(slot_id: u32, kind: MediaKind) -> ReceiveSlot {
    ReceiveSlot {
        slot_id,
        media_kind: kind as i32,
        pinned_sender_id: None,
    }
}

fn pinned_slot(slot_id: u32, pin: u32) -> ReceiveSlot {
    ReceiveSlot {
        slot_id,
        media_kind: MediaKind::Audio as i32,
        pinned_sender_id: Some(pin),
    }
}

fn capability_frame(slots: Vec<ReceiveSlot>) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::ReceiveCapability(
            ReceiveCapability { slots },
        )),
    })
}

fn mute_frame(audio_muted: bool) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::MuteRequest(MuteRequest {
            audio_muted,
            video_muted: false,
        })),
    })
}

/// A mute frame that toggles the VIDEO flag only.
///
/// The slot view is derived from `audio_self_muted` alone, so this must reach
/// the meeting actor (video mute is roster state other clients render) and must
/// NOT trigger a recomposition.
fn video_mute_frame(video_muted: bool) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::MuteRequest(MuteRequest {
            audio_muted: false,
            video_muted,
        })),
    })
}

/// A live post-join session: the streams stay open so the bridge loop runs.
struct Session {
    _conn: wtransport::Connection,
    send: wtransport::stream::SendStream,
    recv: wtransport::stream::RecvStream,
    /// The joiner's own sender id, read off the `JoinResponse` rather than
    /// hardcoded — so an assignment naming the wrong participant fails.
    sender_id: u32,
}

impl Session {
    async fn write(&mut self, frame: bytes::BytesMut) {
        self.send.write_all(&frame).await.expect("write frame");
    }

    /// Read the next `ServerMessage`, or `None` if none arrives promptly.
    ///
    /// The timeout is how a negative ("MC sent nothing") is bounded. It is not a
    /// performance assertion: no test here asserts a wall-clock threshold.
    async fn next(&mut self) -> Option<ServerMessage> {
        tokio::time::timeout(Duration::from_secs(3), read_server_message(&mut self.recv))
            .await
            .ok()
    }

    async fn expect_directive(&mut self) -> SendDirective {
        match self.next().await.map(|m| m.message) {
            Some(Some(server_message::Message::SendDirective(d))) => d,
            other => panic!("expected SendDirective, got {other:?}"),
        }
    }

    async fn expect_assignments(&mut self) -> StreamAssignments {
        match self.next().await.map(|m| m.message) {
            Some(Some(server_message::Message::StreamAssignments(a))) => a,
            other => panic!("expected StreamAssignments, got {other:?}"),
        }
    }

    async fn expect_error(&mut self) -> String {
        match self.next().await.map(|m| m.message) {
            Some(Some(server_message::Message::Error(e))) => e.message,
            other => panic!("expected ErrorMessage, got {other:?}"),
        }
    }

    async fn expect_silence(&mut self) {
        if let Some(msg) = self.next().await {
            panic!("expected no server message, got {:?}", msg.message);
        }
    }
}

async fn join(rig: &AcceptLoopRig, stack: &TestStackHandles, meeting_id: &str) -> Session {
    seed_meeting_with_mh(stack, meeting_id).await;
    let token = stack.keypair.sign_token(&make_meeting_claims(meeting_id));

    let conn = connect(&rig.url).await;
    let (mut send, mut recv) = conn.open_bi().await.expect("open bi").await.expect("bi");

    send.write_all(&join_frame(meeting_id, &token))
        .await
        .expect("write join");

    let resp = tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv))
        .await
        .expect("join response timeout");
    let sender_id = match resp.message {
        Some(server_message::Message::JoinResponse(r)) => {
            r.sender_id.expect("MC allocates a sender id at join")
        }
        other => panic!("expected JoinResponse, got {other:?}"),
    };

    Session {
        _conn: conn,
        send,
        recv,
        sender_id,
    }
}

// ============================================================================
// (a) REQUIRED: receive capability in -> send directive + slot assignment out
// ============================================================================

#[tokio::test]
async fn capability_yields_send_directive_and_slot_assignment() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-loopback").await;
    let mut s = join(&rig, &stack, "mcs-meeting-a").await;

    s.write(capability_frame(vec![slot(0, MediaKind::Audio)]))
        .await;

    // --- the send directive (§5) ---
    let directive = s.expect_directive().await;
    assert_eq!(
        directive.header_version,
        u32::from(media_protocol::frame::PROTOCOL_VERSION),
        "header version must be DERIVED from the media-protocol anchor, never a literal 2"
    );
    assert_eq!(directive.streams.len(), 1, "exactly one audio media stream");

    let stream = &directive.streams[0];
    assert_eq!(
        stream.stream_number,
        u32::from(mc_service::media_routing::MAIN_AUDIO_STREAM_NUMBER)
    );
    assert_eq!(stream.media_kind, MediaKind::Audio as i32);

    let encoding = stream.encoding.as_ref().expect("encoding parameters");
    assert_eq!(encoding.codec, Codec::Opus as i32);
    assert_ne!(
        encoding.codec,
        Codec::Unspecified as i32,
        "CODEC_UNSPECIFIED is never valid in a send directive"
    );
    // Read from the same config the service runs on, not a literal.
    let configured = client_media_config().audio_encoding.to_proto();
    assert_eq!(encoding.max_bitrate_bps, configured.max_bitrate_bps);
    assert_eq!(encoding.frame_rate, configured.frame_rate);

    assert_eq!(stream.targets.len(), 1, "one target: the assigned handler");
    assert_eq!(stream.targets[0].media_handler_url, SEEDED_HANDLER_URL);
    assert_eq!(
        stream.targets[0].transport_mode,
        TransportMode::Datagram as i32,
        "audio rides datagrams (ADR-0036 §1)"
    );

    // --- the slot assignment (§6) ---
    let assignments = s.expect_assignments().await;
    assert_eq!(assignments.assignments.len(), 1);
    let a = &assignments.assignments[0];
    assert_eq!(a.slot_id, 0);
    assert_eq!(
        a.sender_id,
        Some(s.sender_id),
        "the participant's own audio fills its own slot, named by sender_id"
    );
    assert_eq!(a.media_kind, MediaKind::Audio as i32);
    assert_eq!(a.media_handler_url, SEEDED_HANDLER_URL);
    assert_eq!(a.slot_state, SlotState::Active as i32);
    assert!(
        a.switch_command_id.is_none(),
        "present iff SWITCH_PENDING, which MC never emits this story"
    );

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "active"), ("key_custody", "operator")])
        .assert_delta(1);

    // Both messages MC composed actually reached the client's outbound channel.
    //
    // Zero is the meaningful assertion HERE: the wire assertions above already
    // fail (by timeout) if a message never arrives, but they cannot distinguish
    // "MC never composed it" from "MC composed it and the mailbox dropped it" —
    // and only the second is silent. Asserting the drop counter is flat states
    // that the composed count and the delivered count agree.
    //
    // The drop path itself is driven deterministically in
    // `actors::participant::tests` by filling a one-slot channel; it is not
    // reachable from a healthy wire test, and constructing a wedged client here
    // would test the mailbox rather than this seam.
    snap.counter("mc_participant_outbound_messages_dropped_total")
        .with_labels(&[("payload_kind", "signaling_raw")])
        .assert_delta(0);
}

// ============================================================================
// (b) REQUIRED: a mute/unmute cycle leaves the directive untouched
// ============================================================================

#[tokio::test]
async fn mute_unmute_cycle_leaves_the_send_directive_untouched() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-mute").await;
    let mut s = join(&rig, &stack, "mcs-meeting-b").await;

    s.write(capability_frame(vec![slot(0, MediaKind::Audio)]))
        .await;
    let directive_before = s.expect_directive().await.encode_to_vec();
    let active = s.expect_assignments().await;
    assert_eq!(active.assignments[0].slot_state, SlotState::Active as i32);

    // --- mute: slot state flips, NO directive ---
    s.write(mute_frame(true)).await;
    let muted = s.expect_assignments().await;
    assert_eq!(
        muted.assignments[0].slot_state,
        SlotState::SourceMuted as i32,
        "the far-end-muted slot state is conveyed explicitly, never inferred from frame absence"
    );
    assert_eq!(
        muted.assignments[0].sender_id,
        Some(s.sender_id),
        "the source is present and named; it has only muted itself"
    );

    // --- unmute: slot state flips back, still NO directive ---
    s.write(mute_frame(false)).await;
    let unmuted = s.expect_assignments().await;
    assert_eq!(unmuted.assignments[0].slot_state, SlotState::Active as i32);

    // Nothing further on the wire: in particular, no second directive.
    s.expect_silence().await;

    // Exactly ONE directive was composed and exactly one crossed the wire in the
    // whole cycle. MC neither withdrew nor re-issued it, which is what keeps
    // "MC has not asked you to send" and "you have muted yourself" distinct and
    // makes unmute a purely local decision with no round trip.
    //
    // NOTE what this asserts and what it does not. There is no second directive
    // to compare bytes against — that IS the property — so a
    // `directive_before == directive_after` line here would compare a value with
    // itself and assert nothing. The evidence is the pair below: the compose
    // path ran exactly once, and nothing followed the last assignments on the
    // wire. Determinism of the encoding for unchanged input is covered
    // separately by `media_signaling::directive`'s byte-identity unit test.
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(1);
    assert!(
        !directive_before.is_empty(),
        "the one directive must have been non-trivial"
    );

    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "source_muted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "active"), ("key_custody", "operator")])
        .assert_delta(2);
}

// ============================================================================
// No-op repeats are bounded, and counted under their own outcome
// ============================================================================

#[tokio::test]
async fn an_identical_redeclaration_does_no_work_and_is_counted_separately() {
    // `accepted` is the denominator for any rejection ratio, and this arm is
    // client-inflatable at near-zero server cost — so it must land under its own
    // outcome rather than inflating that denominator.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-redeclare").await;
    let mut s = join(&rig, &stack, "mcs-meeting-q").await;

    let declaration = || capability_frame(vec![slot(0, MediaKind::Audio)]);

    s.write(declaration()).await;
    let _ = s.expect_directive().await;
    let _ = s.expect_assignments().await;

    // Three identical repeats: MC must send NOTHING for any of them.
    for _ in 0..3 {
        s.write(declaration()).await;
    }
    s.expect_silence().await;

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "accepted_unchanged"),
            ("key_custody", "operator"),
        ])
        .assert_delta(3);
    // No recomposition happened, so no directive and no slot state moved.
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "active"), ("key_custody", "operator")])
        .assert_delta(1);
}

#[tokio::test]
async fn a_repeated_identical_mute_report_does_no_work() {
    // `MuteRequest` reaches an O(N) roster read and an assignment computation
    // from a ~4-byte message, and the declaration budget does not cover it. A
    // no-op repeat must short-circuit before the actor hop; a REAL transition
    // must still pass straight through, because R-2 requires unmute to be
    // instantaneous.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-mute-noop").await;
    let mut s = join(&rig, &stack, "mcs-meeting-r").await;

    s.write(capability_frame(vec![slot(0, MediaKind::Audio)]))
        .await;
    let _ = s.expect_directive().await;
    let _ = s.expect_assignments().await;

    // First mute is a real transition: one recomposition.
    s.write(mute_frame(true)).await;
    let muted = s.expect_assignments().await;
    assert_eq!(
        muted.assignments[0].slot_state,
        SlotState::SourceMuted as i32
    );

    // Four identical repeats: no recomposition, nothing on the wire — and
    // counted under their own outcome, because the vocabulary partitions mute
    // reports and a message counted nowhere breaks every ratio over it.
    for _ in 0..4 {
        s.write(mute_frame(true)).await;
    }
    s.expect_silence().await;
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "unchanged"), ("key_custody", "operator")])
        .assert_delta(4);

    // A genuine change still passes through immediately — the short-circuit
    // must not have swallowed the transition.
    s.write(mute_frame(false)).await;
    let unmuted = s.expect_assignments().await;
    assert_eq!(unmuted.assignments[0].slot_state, SlotState::Active as i32);

    // Two real transitions => two recompositions, and the directive never moved.
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "source_muted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "active"), ("key_custody", "operator")])
        .assert_delta(2);
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(1);
}

#[tokio::test]
async fn a_video_only_mute_is_recorded_but_never_recomposed() {
    // The cache-key half of the mute fix. `last_reported_mute` is the
    // `(audio, video)` pair, so a video-only toggle clears the dedupe and the
    // report legitimately reaches the meeting actor — but `SourceMuteView` and
    // `build_stream_assignments` read `audio_self_muted` ALONE, so recomposing
    // would spend an O(N) roster read, an assignment computation and an
    // outbound message to produce a `StreamAssignments` byte-identical to the
    // last one, and would move `mc_media_slot_states_total` with provably zero
    // information delivered.
    //
    // Not an attack: this is what an ordinary camera button does.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-mute-video").await;
    let mut s = join(&rig, &stack, "mcs-meeting-video-mute").await;

    s.write(capability_frame(vec![slot(0, MediaKind::Audio)]))
        .await;
    let _ = s.expect_directive().await;
    let _ = s.expect_assignments().await;

    // Camera off, then on. Both are real changes to the reported pair, so both
    // must reach the actor — and neither may recompose.
    s.write(video_mute_frame(true)).await;
    s.write(video_mute_frame(false)).await;
    s.expect_silence().await;

    // Then a real AUDIO change, which must still recompose.
    s.write(mute_frame(true)).await;
    let muted = s.expect_assignments().await;
    assert_eq!(
        muted.assignments[0].slot_state,
        SlotState::SourceMuted as i32
    );

    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[
            ("outcome", "applied_no_recompose"),
            ("key_custody", "operator"),
        ])
        .assert_delta(2);
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "applied"), ("key_custody", "operator")])
        .assert_delta(1);

    // ONE `active` — from the declaration — and one `source_muted`. Before the
    // fix the two video toggles added two more `active`, inflating a fleet
    // distribution while sending a byte-identical message.
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "active"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[("slot_state", "source_muted"), ("key_custody", "operator")])
        .assert_delta(1);
}

#[tokio::test]
async fn mute_work_is_rate_limited_and_the_limit_is_not_permanent() {
    // The DEMONSTRATION of the mute-work bound, not an assertion that it
    // exists. A client alternating `audio_muted` defeats both equality
    // short-circuits — every message is a genuine transition — so each one
    // would otherwise drive a `GetState` roster snapshot on the SHARED meeting
    // actor's mailbox plus a full assignment computation and an outbound
    // message. That amplifier is what this diff created by making
    // `update_self_mute` client-reachable for the first time, and the token
    // bucket is what bounds it.
    //
    // THE BOUND IS STILL REQUIRED, and this comment says so explicitly because
    // it is what the test proves. An earlier revision said each message would
    // drive the meeting actor's O(N) `broadcast_update`; that fan-out was
    // removed by S-2 (`MuteChanged` has no consumer, so it delivered zero bytes
    // to zero clients). Someone reading the old wording, checking
    // `handle_self_mute`, finding no fan-out, and concluding this test is
    // obsolete would delete a control that is still load-bearing for the
    // per-composition cost.
    //
    // # Why the expected counter values are DERIVED rather than hardcoded
    //
    // How many toggles the burst absorbs depends on wall-clock time inside the
    // bridge loop, so a hardcoded split would be a timing assertion wearing a
    // counter's clothes — green locally, flaky on a loaded CI box. Instead the
    // wire is the ground truth: each APPLIED toggle emits exactly one
    // `StreamAssignments`, so the observed reply count fixes both expected
    // deltas exactly. That also makes the assertion stronger than a hardcoded
    // one, because it pins the PARTITION — every toggle lands in exactly one
    // bucket, and a message counted nowhere would break every ratio built on
    // this metric.
    //
    // The limiter's arithmetic (burst size, refill rate, remainder carry, idle
    // capping) is pinned deterministically by the `ClientWorkLimiter` unit tests
    // in `webtransport::connection`; what this test adds is that the limiter is
    // wired into the real dispatch seam at all.
    const TOGGLES: u64 = 40;

    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-mute-rate").await;
    let mut s = join(&rig, &stack, "mcs-meeting-mute-rate").await;

    s.write(capability_frame(vec![slot(0, MediaKind::Audio)]))
        .await;
    let _ = s.expect_directive().await;
    let _ = s.expect_assignments().await;

    // Alternate far past the burst, as fast as the connection allows.
    for i in 0..TOGGLES {
        s.write(mute_frame(i % 2 == 0)).await;
    }

    // Drain everything MC chose to emit. One reply per applied toggle.
    let mut applied = 0u64;
    while s.next().await.is_some() {
        applied += 1;
    }

    // FIRES: the bound engaged rather than being decorative.
    assert!(
        applied < TOGGLES,
        "{TOGGLES} rapid alternating toggles must exhaust the mute-work bucket, \
         but all {applied} were served"
    );
    // APPLIES: and it engaged only after honouring the burst, so a human
    // flurry — which is what the burst is sized for — is never clamped.
    assert!(
        applied > 0,
        "the burst must be spendable before the limiter engages"
    );

    // THE PARTITION: every toggle lands in exactly one bucket, and a message
    // counted nowhere would break every ratio built on this metric.
    //
    // `applied` is exact and wire-derived — each applied toggle emits exactly
    // one `StreamAssignments`, so the observed reply count fixes it.
    //
    // The remaining two are asserted as a SUM, not as an exact split, and the
    // reason is a real property of the code rather than a concession. A
    // suppressed report deliberately does NOT update `last_reported_mute`, so
    // the reported state freezes at the last APPLIED value and the alternating
    // sequence then hits that frozen value on every second message — correctly
    // `unchanged` rather than `rate_limited`. Within ONE suppressed run that
    // gives ceil(L/2) and floor(L/2). But the bucket refills on a 250 ms timer
    // while these 40 frames are streamed rather than pre-queued, so a loaded
    // box can serve a toggle mid-stream and split the suppressed messages into
    // SEVERAL runs. Across K runs the difference between the two counters is
    // the number of odd-length runs, which an exact split can only predict for
    // K=1. Asserting `div_ceil(2)` / `/ 2` would therefore be a timing
    // assertion wearing a counter's clothes — green locally, red on CI under
    // ADR-0028 zero-retry — while the SUM is exact for every K.
    let remaining = TOGGLES - applied;
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "applied"), ("key_custody", "operator")])
        .assert_delta(applied);
    let rate_limited = snap
        .counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "rate_limited"), ("key_custody", "operator")])
        .delta();
    let unchanged = snap
        .counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "unchanged"), ("key_custody", "operator")])
        .delta();
    assert_eq!(
        rate_limited + unchanged,
        remaining,
        "every suppressed toggle must land in exactly one bucket: {applied} applied + \
         {rate_limited} rate_limited + {unchanged} unchanged must account for all {TOGGLES} \
         toggles, or some ratio built on this metric is counting nothing"
    );
    // The suppression was real, not all no-ops: at least one GENUINE transition
    // was refused. Without this the sum above would pass on a build where the
    // limiter never engaged and every surplus message happened to be a repeat.
    assert!(
        rate_limited >= 1,
        "the bound must have refused at least one genuine transition, but all {remaining} \
         suppressed messages were counted as no-ops"
    );

    // NOT PERMANENT — the property that makes this a rate limit and not a
    // budget, and the reason a cumulative budget was rejected: a budget would
    // leave this client unable to mute for the rest of the session, so every
    // other participant would render a live speaker as muted.
    //
    // The drain above already waited out a read timeout, so the bucket has
    // refilled. The last ACCEPTED state is unknown (it depends where the
    // limiter cut in), so both states are sent — one of them is necessarily a
    // transition, and a suppressed report deliberately does not update
    // `last_reported_mute`, so neither is swallowed by the no-op check.
    s.write(mute_frame(true)).await;
    s.write(mute_frame(false)).await;
    let recovered = s.expect_assignments().await;
    assert_eq!(recovered.assignments.len(), 1);
}

// ============================================================================
// Disposition matrix: what MC accepts, and what it conveys
// ============================================================================

#[tokio::test]
async fn an_extra_slot_reports_a_source_shortage_with_sender_id_absent() {
    let (stack, rig) = start_stack("mcs-extra-slot").await;
    let mut s = join(&rig, &stack, "mcs-meeting-c").await;

    s.write(capability_frame(vec![
        slot(0, MediaKind::Audio),
        slot(1, MediaKind::Audio),
    ]))
    .await;

    let _ = s.expect_directive().await;
    let assignments = s.expect_assignments().await;
    assert_eq!(assignments.assignments.len(), 2);
    assert_eq!(
        assignments.assignments[0].slot_state,
        SlotState::Active as i32
    );

    let extra = &assignments.assignments[1];
    assert_eq!(
        extra.slot_state,
        SlotState::FewerSourcesThanSlots as i32,
        "a genuine source shortage, distinct from the rejected unservable-numbering case"
    );
    assert!(
        extra.sender_id.is_none(),
        "absent MUST NOT be coerced to 0 — a shared zero is N colliding live ids, not a recycled one"
    );
    assert_ne!(extra.sender_id, Some(0));
    assert!(extra.media_handler_url.is_empty());
}

#[tokio::test]
async fn mc_echoes_the_subscribers_own_slot_numbering() {
    // The coincidence trap: `{0}` alone cannot distinguish "MC echoes the
    // declared id" from "MC and the client both happened to say 0". `{0, 7}`
    // can — no hardcoded-0 implementation can emit an assignment carrying 7.
    let (stack, rig) = start_stack("mcs-echo").await;
    let mut s = join(&rig, &stack, "mcs-meeting-d").await;

    s.write(capability_frame(vec![
        slot(0, MediaKind::Audio),
        slot(7, MediaKind::Audio),
    ]))
    .await;

    let _ = s.expect_directive().await;
    let assignments = s.expect_assignments().await;
    let ids: Vec<u32> = assignments.assignments.iter().map(|a| a.slot_id).collect();
    assert_eq!(ids, vec![0, 7]);
    assert_eq!(
        assignments.assignments[1].slot_state,
        SlotState::FewerSourcesThanSlots as i32
    );
}

#[tokio::test]
async fn a_valid_but_unsatisfiable_video_slot_is_not_a_rejection() {
    // "Your cap is too low" and "there is nobody to show you" are different
    // conditions with different owners, and they must not share a counter.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-video-slot").await;
    let mut s = join(&rig, &stack, "mcs-meeting-e").await;

    s.write(capability_frame(vec![slot(1, MediaKind::VideoCamera)]))
        .await;

    let _ = s.expect_directive().await;
    let assignments = s.expect_assignments().await;
    assert_eq!(assignments.assignments.len(), 1);
    assert_eq!(
        assignments.assignments[0].slot_state,
        SlotState::FewerSourcesThanSlots as i32
    );
    assert_eq!(
        assignments.assignments[0].media_kind,
        MediaKind::VideoCamera as i32
    );

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(1);
    // MC's planned audio egress matched nothing: MH forwards media this
    // subscriber will never accept.
    snap.counter("mc_media_unmatched_plan_slots_total")
        .with_labels(&[("key_custody", "operator")])
        .assert_delta(1);
}

#[tokio::test]
async fn a_zero_slot_declaration_still_directs_the_client_to_send() {
    // §5 and §6 are orthogonal: a client that wants to send without receiving
    // must still be told what to produce. An implementation that gated the
    // directive on having receive slots would pass an assignments-only
    // assertion, so the directive is asserted POSITIVELY here.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-zero-slot").await;
    let mut s = join(&rig, &stack, "mcs-meeting-f").await;

    s.write(capability_frame(vec![])).await;

    let directive = s.expect_directive().await;
    assert_eq!(directive.streams.len(), 1);
    assert_eq!(
        directive.header_version,
        u32::from(media_protocol::frame::PROTOCOL_VERSION)
    );
    assert_eq!(directive.streams[0].targets.len(), 1);
    assert_eq!(
        directive.streams[0].targets[0].media_handler_url,
        SEEDED_HANDLER_URL
    );
    assert_eq!(
        directive.streams[0].targets[0].transport_mode,
        TransportMode::Datagram as i32
    );

    let assignments = s.expect_assignments().await;
    assert!(
        assignments.assignments.is_empty(),
        "no slot was declared, so there is no slot id to echo — ZERO_REQUESTED cannot be \
         carried without fabricating one"
    );

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_unmatched_plan_slots_total")
        .with_labels(&[("key_custody", "operator")])
        .assert_delta(1);
}

// ============================================================================
// Rejection paths — whole-declaration rejection, one test each
// ============================================================================

/// Assert a declaration is rejected whole: an error back, and NO directive and
/// NO assignments on the wire.
async fn assert_rejected(
    session: &mut Session,
    expected_token: &str,
    snap: &common::observability::testing::MetricSnapshot,
) {
    let message = session.expect_error().await;
    assert!(!message.is_empty());
    session.expect_silence().await;

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", expected_token), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(0);
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(0);
}

#[tokio::test]
async fn rejects_duplicate_slot_ids() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-dup").await;
    let mut s = join(&rig, &stack, "mcs-meeting-g").await;
    s.write(capability_frame(vec![
        slot(0, MediaKind::Audio),
        slot(0, MediaKind::Audio),
    ]))
    .await;
    assert_rejected(&mut s, "duplicate_slot_id", &snap).await;
}

#[tokio::test]
async fn rejects_a_slot_count_over_the_configured_cap() {
    let snap = MetricAssertion::snapshot();
    // A cap of 2, set through CONFIGURATION rather than a constant, so this
    // exercises the knob and not an agreement between test and code.
    let mut config = client_media_config();
    config.max_receive_slots = 2;
    let (stack, rig) = start_stack_with("mcs-cap", config).await;
    let mut s = join(&rig, &stack, "mcs-meeting-h").await;

    s.write(capability_frame(vec![
        slot(0, MediaKind::Audio),
        slot(1, MediaKind::Audio),
        slot(2, MediaKind::Audio),
    ]))
    .await;
    assert_rejected(&mut s, "slot_count_over_cap", &snap).await;
}

#[tokio::test]
async fn rejects_a_slot_id_outside_the_16_bit_relay_field() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-slot-range").await;
    let mut s = join(&rig, &stack, "mcs-meeting-i").await;
    // 65536 truncates to 0 under `as`, which would silently address the planned
    // loopback slot. It must reject, never clamp.
    s.write(capability_frame(vec![slot(65_536, MediaKind::Audio)]))
        .await;
    assert_rejected(&mut s, "slot_id_out_of_range", &snap).await;
}

#[tokio::test]
async fn rejects_a_zero_pinned_sender_id() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-pin-zero").await;
    let mut s = join(&rig, &stack, "mcs-meeting-j").await;
    s.write(capability_frame(vec![pinned_slot(0, 0)])).await;
    assert_rejected(&mut s, "pinned_sender_id_zero", &snap).await;
}

#[tokio::test]
async fn rejects_an_out_of_range_pinned_sender_id() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-pin-range").await;
    let mut s = join(&rig, &stack, "mcs-meeting-k").await;
    s.write(capability_frame(vec![pinned_slot(0, 65_536)]))
        .await;
    assert_rejected(&mut s, "pinned_sender_id_out_of_range", &snap).await;
}

#[tokio::test]
async fn rejects_an_unset_media_kind() {
    // Fail closed on the proto3 zero. The likeliest producer is a
    // version-skewed client whose `AUDIO = 0` decodes as unspecified — reading
    // it as "a kind we happen not to have" would turn a client-fleet rollback
    // into "everyone's meetings are empty" with no counter naming skew.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-kind-unset").await;
    let mut s = join(&rig, &stack, "mcs-meeting-l").await;
    s.write(capability_frame(vec![slot(0, MediaKind::Unspecified)]))
        .await;
    assert_rejected(&mut s, "media_kind_unspecified", &snap).await;
}

#[tokio::test]
async fn rejects_audio_in_a_slot_mc_has_no_plan_for() {
    // NOT a client defect: the declaration is well-formed and the wire contract
    // permits it. MH stamps the relay-region stream id from the policy pushed at
    // join, before this client could declare, so MC cannot address slot 7.
    // Accepting and reporting a source shortage would tell the client "there is
    // nobody to show you" while MH forwards a source it will drop.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-unplanned").await;
    let mut s = join(&rig, &stack, "mcs-meeting-m").await;
    s.write(capability_frame(vec![slot(7, MediaKind::Audio)]))
        .await;
    assert_rejected(&mut s, "slot_id_not_planned", &snap).await;
}

/// The rejection-REPLY bound engages, and suppressing a reply never suppresses
/// the count.
///
/// # Why this test exists rather than trusting the limiter's unit tests
///
/// The `ClientWorkLimiter` unit tests pin the bucket ARITHMETIC, and that
/// arithmetic is genuinely shared with the mute path. What they cannot show is
/// that the rejection responder is wired to a bucket at all: delete
/// `try_spend` from `reject_capability`, or construct the field with a burst of
/// `u32::MAX`, or move the spend to AFTER the send, and every other test in this
/// file still passes. A control that is alive but undemonstrated is the shape
/// this devloop hit three times, and this one was added during review, which is
/// where it is least likely to be revisited.
///
/// # The load-bearing assertion is the third one
///
/// The token is spent AFTER `record_receive_capability` and after the WARN has
/// had its one-shot chance, precisely so that a flooding client stays fully
/// observable and only the ~10x egress amplification is rationed. That ordering
/// is the whole design, and until now it was an untested claim. The counter must
/// move by the FULL send count even though far fewer replies came back.
#[tokio::test]
async fn capability_rejection_replies_are_rate_limited_but_never_uncounted() {
    // Well past the burst of 8.
    const REJECTIONS: u64 = 40;

    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-reject-rate").await;
    let mut s = join(&rig, &stack, "mcs-meeting-reject-rate").await;

    // Two DISTINCT unplanned slot ids, alternating. MC plans slot 0, so both are
    // rejected with `slot_id_not_planned` — and alternating means the
    // identical-redeclaration short-circuit never fires, so every one of these
    // is a real rejection reaching the responder rather than a cheap no-op.
    for i in 0..REJECTIONS {
        let slot_id = if i % 2 == 0 { 7 } else { 9 };
        s.write(capability_frame(vec![slot(slot_id, MediaKind::Audio)]))
            .await;
    }

    // Drain every reply MC chose to send.
    let mut errors_received = 0u64;
    while s.next().await.is_some() {
        errors_received += 1;
    }

    // FIRES: the bound engaged rather than being decorative.
    assert!(
        errors_received < REJECTIONS,
        "{REJECTIONS} rapid rejected declarations must exhaust the reply bucket, \
         but all {errors_received} were answered"
    );
    // APPLIES: and only after honouring the burst, so a legitimate SDK
    // correcting its declaration a few times always gets told what is wrong.
    assert!(
        errors_received > 0,
        "the burst must be spendable: a client converging on a valid declaration \
         must receive its errors"
    );

    // THE POINT. Suppressing a reply must not suppress the count — otherwise the
    // rejection metric under-reports exactly when a client is misbehaving most,
    // which is when an operator needs it. Exact and timing-independent: the
    // counter is incremented before the token is spent, on every message.
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "slot_id_not_planned"),
            ("key_custody", "operator"),
        ])
        .assert_delta(REJECTIONS);

    // NOT PERMANENT — the property that makes this a rate limit and not a
    // budget. A budget would leave a client that once flooded unable to learn
    // why its declaration is refused for the rest of the session. The drain
    // above already waited out a read timeout, so the bucket has refilled.
    s.write(capability_frame(vec![slot(7, MediaKind::Audio)]))
        .await;
    let recovered = s.expect_error().await;
    assert!(
        !recovered.is_empty(),
        "after the bucket refills the client must be answered again"
    );
}

#[tokio::test]
async fn rejects_declarations_past_the_per_connection_budget() {
    let snap = MetricAssertion::snapshot();
    let mut config = client_media_config();
    config.max_receive_capability_declarations = 1;
    let (stack, rig) = start_stack_with("mcs-budget", config).await;
    let mut s = join(&rig, &stack, "mcs-meeting-n").await;

    // First: accepted, charges the budget.
    s.write(capability_frame(vec![slot(0, MediaKind::Audio)]))
        .await;
    let _ = s.expect_directive().await;
    let _ = s.expect_assignments().await;

    // Second, DIFFERENT declaration: budget exhausted. A different one, because
    // an identical re-declaration short-circuits before the budget check.
    s.write(capability_frame(vec![
        slot(0, MediaKind::Audio),
        slot(1, MediaKind::Audio),
    ]))
    .await;
    let message = s.expect_error().await;
    assert!(!message.is_empty());

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "declaration_budget_exhausted"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

// ============================================================================
// Security: the two url-keyed maps must not cross
// ============================================================================

#[tokio::test]
async fn a_client_supplied_handler_url_never_reaches_a_send_target() {
    // MC holds two similar-looking url-keyed maps and only one is
    // server-derived. A client-controlled url on a `SendTarget` is a redirect
    // primitive — MC telling a client where to send its media.
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-redirect").await;
    let mut s = join(&rig, &stack, "mcs-meeting-o").await;

    // Plant the poison: a MediaConnectionUpdate naming an attacker url.
    s.write(encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::MediaConnectionUpdate(
            MediaConnectionUpdate {
                statuses: vec![MhConnectionStatus {
                    mh_url: ATTACKER_URL.to_string(),
                    state: proto_gen::dark_tower::signaling::v1::ConnectionState::Connected as i32,
                    failure_reason: None,
                    failure_code: None,
                    observed_at: None,
                }],
            },
        )),
    }))
    .await;

    // PREMISE: confirm the update was actually processed. Without this the test
    // passes vacuously if the message was dropped for an unrelated reason —
    // proving nothing while reading green.
    settle_no_reply_message().await;
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(1);

    s.write(capability_frame(vec![slot(0, MediaKind::Audio)]))
        .await;
    let directive = s.expect_directive().await;
    let assignments = s.expect_assignments().await;

    // POSITIVE: the expected server-derived value, which also catches an empty
    // or defaulted url.
    assert_eq!(
        directive.streams[0].targets[0].media_handler_url,
        SEEDED_HANDLER_URL
    );
    assert_eq!(
        assignments.assignments[0].media_handler_url,
        SEEDED_HANDLER_URL
    );

    // NEGATIVE, field-agnostic: the sentinel appears nowhere in either encoded
    // message. A per-field positive assertion stops covering new surface the
    // moment a later story adds per-handler fields or a multi-target set where
    // only `targets[0]` is checked.
    let directive_bytes = directive.encode_to_vec();
    let assignment_bytes = assignments.encode_to_vec();
    let needle = ATTACKER_URL.as_bytes();
    assert!(
        !directive_bytes.windows(needle.len()).any(|w| w == needle),
        "attacker url reached the send directive"
    );
    assert!(
        !assignment_bytes.windows(needle.len()).any(|w| w == needle),
        "attacker url reached the stream assignments"
    );
}

// ============================================================================
// A client that never declares
// ============================================================================

#[tokio::test]
async fn a_client_that_never_declares_is_never_directed_and_stays_healthy() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-silent").await;
    let mut s = join(&rig, &stack, "mcs-meeting-p").await;

    // No capability. Nothing must arrive, and the session must stay usable.
    s.expect_silence().await;

    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(0);

    // The bounded negative: a message whose effect IS observable still lands,
    // which proves the session is alive rather than merely quiet.
    s.write(encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::MediaConnectionUpdate(
            MediaConnectionUpdate {
                statuses: vec![MhConnectionStatus {
                    mh_url: SEEDED_HANDLER_URL.to_string(),
                    state: proto_gen::dark_tower::signaling::v1::ConnectionState::Connected as i32,
                    failure_reason: None,
                    failure_code: None,
                    observed_at: None,
                }],
            },
        )),
    }))
    .await;

    settle_no_reply_message().await;
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(1);
}
