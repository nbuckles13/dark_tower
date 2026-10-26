//! Integration tests for MC's client-facing media signalling (ADR-0036 §5, §6).
//!
//! Every case drives REAL framed `ClientMessage`s through the post-join
//! decode+dispatch seam over live WebTransport connections, and asserts on the
//! **wire bytes** MC sends back plus the **metric deltas** it records.
//!
//! # Multi-party, since story 2
//!
//! Loopback is removed (R-3): a participant never hears itself, so a single
//! participant can only ever be told "nobody to hear" and "send nothing". Every
//! case that needs a filled slot or a non-empty send target therefore joins two
//! participants on one handler. A participant's view is pushed by the meeting
//! actor whenever meeting state changes it — including on the OTHER
//! participant's declaration or mute — so the reads below go through
//! `media_session::Session`, which skips roster traffic.
//!
//! # The two required cases
//!
//! - **(a)** receive-capability in -> send directive + slot assignment out, with
//!   the expected fields — including the re-sent directive a publisher gets the
//!   moment someone starts holding it (paired-client C1).
//! - **(b)** a mute/unmute cycle leaves every directive untouched.
//!
//! (b)'s primary control is structural: `media_signaling::directive` has no
//! path to mute state and `build_send_directive` takes no mute argument.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use common::observability::testing::MetricAssertion;
use mc_service::media_signaling::MediaKindLabel;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    client_message, ClientMessage, Codec, MediaConnectionUpdate, MediaKind, MhConnectionStatus,
    ReceiveSlot, SlotState, TransportMode,
};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::media_session::{
    audio_slots, capability_frame, join_as, mute_frame, mute_kinds_frame, slot, start_stack,
    start_stack_with, Session,
};
use test_common::{
    client_media_config, encode_framed, mh_handler, seed_meeting_with_handlers,
    seed_meeting_with_mh, TestStackHandles,
};

/// The handler url `seed_meeting_with_mh` seeds, which is what MC must put on
/// `media_servers`, the send target and the slot assignment alike.
const SEEDED_HANDLER_URL: &str = "wt://mh-test-1:4433";

/// A url no real handler could ever have (RFC 2606 `.invalid` never resolves),
/// used to poison the client-supplied MH-status map in the redirect test.
const ATTACKER_URL: &str = "https://attacker.example.invalid/redirect";

/// Let the bridge loop read and dispatch a client message that produces no
/// reply. A settle bound, NOT a performance assertion. Sibling settle bounds:
/// `slot_placement_integration.rs` (`pushes_caught_up`, `actor_drained`,
/// `virtual_turns`) and `common::media_session::Session::settle` (quiet read).
async fn settle_no_reply_message() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

fn pinned_slot(slot_id: u32, pin: u32) -> ReceiveSlot {
    ReceiveSlot {
        slot_id,
        media_kind: MediaKind::Audio as i32,
        pinned_sender_id: Some(pin),
    }
}

/// One participant in a freshly seeded single-handler meeting.
async fn solo(rig: &AcceptLoopRig, stack: &TestStackHandles, meeting_id: &str) -> Session {
    seed_meeting_with_mh(stack, meeting_id).await;
    join_as(rig, stack, meeting_id, "user-a").await
}

/// Two participants, both declaring one audio slot, both settled: each holds
/// the other, and each has a non-empty send target.
async fn declared_pair(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
) -> (Session, Session) {
    seed_meeting_with_mh(stack, meeting_id).await;
    let mut a = join_as(rig, stack, meeting_id, "user-a").await;
    let mut b = join_as(rig, stack, meeting_id, "user-b").await;
    a.write(capability_frame(audio_slots(1))).await;
    b.write(capability_frame(audio_slots(1))).await;
    let _ = a.settle().await;
    let _ = b.settle().await;
    (a, b)
}

// ============================================================================
// (a) REQUIRED: receive capability in -> send directive + slot assignment out
// ============================================================================

#[tokio::test]
async fn capability_yields_send_directive_and_slot_assignment() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-pair").await;
    seed_meeting_with_mh(&stack, "mcs-meeting-a").await;
    let mut a = join_as(&rig, &stack, "mcs-meeting-a", "user-a").await;
    let mut b = join_as(&rig, &stack, "mcs-meeting-a", "user-b").await;
    assert_eq!(a.media_servers, vec![SEEDED_HANDLER_URL.to_string()]);

    // Story 3: the JOIN carries the declaration (here none: zero slots), so
    // A was directed in its join turn — its published audio LISTED with EMPTY
    // targets (§5: send nothing; nobody holds A).
    let directive = a.join_directive.clone().expect("A is directed at join");
    assert_eq!(
        directive.header_version,
        u32::from(media_protocol::frame::PROTOCOL_VERSION),
        "header version must be DERIVED from the media-protocol anchor, never a literal 2"
    );
    assert_eq!(
        directive.streams.len(),
        1,
        "the published audio is always listed"
    );
    assert!(
        directive.streams[0].targets.is_empty(),
        "held by nobody: send nothing"
    );

    // A declares while B has not: A's slot is filled with B. A's directive is
    // unchanged (still unwatched), so only the view is re-sent.
    a.write(capability_frame(audio_slots(1))).await;
    let assignments = a.expect_assignments().await;
    assert_eq!(assignments.assignments.len(), 1);
    let slot_a = &assignments.assignments[0];
    assert_eq!(slot_a.slot_id, 0);
    assert_eq!(
        slot_a.sender_id,
        Some(b.sender_id),
        "A's slot holds B, never A itself (R-3)"
    );
    assert_ne!(slot_a.sender_id, Some(a.sender_id));
    assert_eq!(slot_a.media_kind, MediaKind::Audio as i32);
    assert_eq!(slot_a.media_handler_url, SEEDED_HANDLER_URL);
    assert_eq!(slot_a.slot_state, SlotState::Active as i32);
    assert!(slot_a.switch_command_id.is_none());

    // B declares: B now holds A — so A is RE-SENT a directive with one target
    // (paired-client C1). Without this A would never send and nobody would
    // hear it.
    b.write(capability_frame(audio_slots(1))).await;
    let a_directive = a
        .directive_until("A's directive once B holds A", |d| {
            d.streams.iter().any(|s| !s.targets.is_empty())
        })
        .await;
    assert_eq!(
        a_directive.streams.len(),
        1,
        "exactly one audio media stream"
    );
    let stream = &a_directive.streams[0];
    assert_eq!(
        stream.stream_number,
        u32::from(mc_service::media_routing::MAIN_AUDIO_STREAM_NUMBER)
    );
    assert_eq!(stream.media_kind, MediaKind::Audio as i32);
    let encoding = stream.encoding.as_ref().expect("encoding parameters");
    assert_eq!(encoding.codec, Codec::Opus as i32);
    assert_ne!(encoding.codec, Codec::Unspecified as i32);
    let configured = client_media_config().audio_encoding.to_proto();
    assert_eq!(encoding.max_bitrate_bps, configured.max_bitrate_bps);
    assert_eq!(encoding.frame_rate, configured.frame_rate);
    assert_eq!(
        stream.targets.len(),
        1,
        "one target: the one handler A shares with B"
    );
    assert_eq!(
        stream.targets[0].media_handler_url, a.media_servers[0],
        "the send target and media_servers are the same string"
    );
    assert_eq!(
        stream.targets[0].transport_mode,
        TransportMode::Datagram as i32
    );

    // B's own view: a directive with one target and A in its slot.
    let (b_directive, b_assignments) = b.settle().await;
    let b_directive = b_directive.expect("B is directed");
    assert_eq!(b_directive.streams[0].targets.len(), 1);
    let b_assignments = b_assignments.expect("B is assigned");
    assert_eq!(b_assignments.assignments[0].sender_id, Some(a.sender_id));
    assert!(b_assignments.unreachable_sender_ids.is_empty());

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(2);
    // Two join-turn directives with empty targets (A, B); then B's (when A
    // declared) and A's (when B declared) gain a target.
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[
            ("outcome", "emitted_empty_targets"),
            ("key_custody", "operator"),
        ])
        .assert_delta(2);
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(2);
    // Composed and handed over: nothing dropped at either later hop.
    snap.counter("mc_media_slot_view_emissions_total")
        .with_labels(&[("outcome", "delivery_failed"), ("key_custody", "operator")])
        .assert_delta(0);
    snap.counter("mc_participant_outbound_messages_dropped_total")
        .with_labels(&[("payload_kind", "signaling_raw")])
        .assert_delta(0);
}

/// R-3: a solo participant hears nothing — every declared slot says so
/// explicitly — and is directed to send nothing.
#[tokio::test]
async fn a_solo_participant_hears_nothing_and_is_told_to_send_nothing() {
    let (stack, rig) = start_stack("mcs-solo").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-solo").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();

    // Directed at join: the published audio listed with EMPTY targets.
    let directive = a.join_directive.clone().expect("directed at join");
    assert_eq!(directive.streams.len(), 1);
    assert!(directive.streams[0].targets.is_empty());
    a.write(capability_frame(audio_slots(2))).await;
    let assignments = a.expect_assignments().await;
    assert_eq!(assignments.assignments.len(), 2);
    for s in &assignments.assignments {
        assert_eq!(s.slot_state, SlotState::FewerSourcesThanSlots as i32);
        assert!(s.sender_id.is_none());
        assert!(s.media_handler_url.is_empty());
    }
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[
            ("slot_state", "active"),
            ("media_kind", "audio"),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);
}

// ============================================================================
// (b) REQUIRED: a mute/unmute cycle leaves every directive untouched
// ============================================================================

#[tokio::test]
async fn mute_unmute_cycle_leaves_the_send_directive_untouched() {
    let (stack, rig) = start_stack("mcs-mute").await;
    let (mut a, mut b) = declared_pair(&rig, &stack, "mcs-meeting-b").await;
    let snap = MetricAssertion::snapshot();

    // --- A mutes: B, who holds A, sees AVATAR_CLIENT_MUTED; nobody gets a
    // directive ---
    a.write(mute_kinds_frame(Some(true), None)).await;
    let muted = b.expect_assignments().await;
    assert_eq!(
        muted.assignments[0].slot_state,
        SlotState::AvatarClientMuted as i32,
        "the far-end-muted slot state is conveyed explicitly, never inferred from frame absence"
    );
    assert_eq!(
        muted.assignments[0].sender_id,
        Some(a.sender_id),
        "the source is present and named; it has only muted itself"
    );

    // --- A unmutes: B's slot flips back ---
    a.write(mute_kinds_frame(Some(false), None)).await;
    let unmuted = b.expect_assignments().await;
    assert_eq!(unmuted.assignments[0].slot_state, SlotState::Active as i32);

    // Nothing else on either wire: in particular no directive to anyone.
    b.expect_no_media().await;
    a.expect_no_media().await;
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[("outcome", "emitted"), ("key_custody", "operator")])
        .assert_delta(0);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[
            ("slot_state", "avatar_client_muted"),
            ("media_kind", "audio"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[
            ("slot_state", "active"),
            ("media_kind", "audio"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

// ============================================================================
// No-op repeats are bounded, and counted under their own outcome
// ============================================================================

#[tokio::test]
async fn an_identical_redeclaration_does_no_work_and_is_counted_separately() {
    let (stack, rig) = start_stack("mcs-redeclare").await;
    let (mut a, mut b) = declared_pair(&rig, &stack, "mcs-meeting-q").await;
    let snap = MetricAssertion::snapshot();
    let pushes_before = stack.mh_reg_client.calls().len();

    for _ in 0..3 {
        a.write(capability_frame(audio_slots(1))).await;
    }
    a.expect_no_media().await;
    b.expect_no_media().await;

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(0);
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "accepted_unchanged"),
            ("key_custody", "operator"),
        ])
        .assert_delta(3);
    // No actor hop, so no re-push either.
    assert_eq!(stack.mh_reg_client.calls().len(), pushes_before);
}

#[tokio::test]
async fn a_repeated_identical_mute_report_does_no_work() {
    let (stack, rig) = start_stack("mcs-mute-noop").await;
    let (mut a, mut b) = declared_pair(&rig, &stack, "mcs-meeting-r").await;
    let snap = MetricAssertion::snapshot();

    a.write(mute_kinds_frame(Some(true), None)).await;
    let muted = b.expect_assignments().await;
    assert_eq!(
        muted.assignments[0].slot_state,
        SlotState::AvatarClientMuted as i32
    );

    for _ in 0..4 {
        a.write(mute_kinds_frame(Some(true), None)).await;
    }
    b.expect_no_media().await;
    // One increment per (request, named kind): the 4 repeats name audio only.
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[
            ("outcome", "unchanged"),
            ("media_kind", "audio"),
            ("key_custody", "operator"),
        ])
        .assert_delta(4);
    for outcome in ["applied", "unchanged"] {
        snap.counter("mc_media_mute_requests_total")
            .with_labels(&[
                ("outcome", outcome),
                ("media_kind", "video"),
                ("key_custody", "operator"),
            ])
            .assert_delta(0);
    }

    a.write(mute_kinds_frame(Some(false), None)).await;
    let unmuted = b.expect_assignments().await;
    assert_eq!(unmuted.assignments[0].slot_state, SlotState::Active as i32);
}

/// Story 3: a request naming video from an AUDIO-ONLY publisher changes
/// nothing any subscriber is told: since 2026-10-25 the connection refuses the
/// WHOLE request (`rejected_total{reason="not_published"}`: no state, no
/// fan-out, no disposition, even for the audio it also names), and an
/// audio-only mute still re-emits.
#[tokio::test]
async fn a_video_only_mute_by_an_audio_only_publisher_re_emits_nothing() {
    let (stack, rig) = start_stack("mcs-mute-video").await;
    let (mut a, mut b) = declared_pair(&rig, &stack, "mcs-meeting-video-mute").await;
    let snap = MetricAssertion::snapshot();
    let pushes_before = stack.mh_reg_client.calls().len();

    a.write(mute_frame(false, true)).await;
    a.write(mute_frame(false, false)).await;
    let _ = a.expect_error().await;
    let _ = a.expect_error().await;
    b.expect_no_media().await;
    assert_eq!(
        stack.mh_reg_client.calls().len(),
        pushes_before,
        "no stream of A's changed disposition: the identical re-render is not re-pushed"
    );

    a.write(mute_kinds_frame(Some(true), None)).await;
    let muted = b.expect_assignments().await;
    assert_eq!(
        muted.assignments[0].slot_state,
        SlotState::AvatarClientMuted as i32
    );

    let mute = |outcome: &'static str, kind: &'static str| {
        snap.counter("mc_media_mute_requests_total").with_labels(&[
            ("outcome", outcome),
            ("media_kind", kind),
            ("key_custody", "operator"),
        ])
    };
    snap.counter("mc_media_mute_requests_rejected_total")
        .with_labels(&[
            ("surface", "client_mute"),
            ("reason", "not_published"),
            ("key_custody", "operator"),
        ])
        .assert_delta(2);
    mute("applied", "video").assert_delta(0);
    mute("unchanged", "video").assert_delta(0);
    mute("applied", "audio").assert_delta(1);
    mute("unchanged", "audio").assert_delta(0);
}

#[tokio::test]
async fn mute_work_is_rate_limited_and_the_limit_is_not_permanent() {
    // A client alternating `audio_muted` defeats both equality short-circuits,
    // so each message would otherwise drive a meeting-actor hop, a re-emit
    // to every holder and (story 3 R-15 fix) a roster fan-out owed to every
    // other participant. The per-connection token bucket bounds it per sender;
    // the per-meeting fan-out bucket bounds the fan-out across senders.
    //
    // The expected counter values are DERIVED from the wire: each applied
    // toggle changes B's view exactly once, so B's received count fixes the
    // `applied` delta; the rest are asserted as a partition SUM (a suppressed
    // report does not update `last_reported_mute`, so the split between
    // `rate_limited` and `unchanged` depends on refill timing).
    const TOGGLES: u64 = 40;

    let (stack, rig) = start_stack("mcs-mute-rate").await;
    let (mut a, mut b) = declared_pair(&rig, &stack, "mcs-meeting-mute-rate").await;
    let snap = MetricAssertion::snapshot();

    for i in 0..TOGGLES {
        a.write(mute_kinds_frame(Some(i % 2 == 0), None)).await;
    }

    let mut applied = 0u64;
    while b.next_media().await.is_some() {
        applied += 1;
    }
    assert!(
        applied < TOGGLES,
        "the bound must engage: all {applied} were served"
    );
    assert!(
        applied > 0,
        "the burst must be spendable before the limiter engages"
    );

    let remaining = TOGGLES - applied;
    let mute = |outcome: &'static str, kind: &'static str| {
        snap.counter("mc_media_mute_requests_total").with_labels(&[
            ("outcome", outcome),
            ("media_kind", kind),
            ("key_custody", "operator"),
        ])
    };
    mute("applied", "audio").assert_delta(applied);
    let rate_limited = mute("rate_limited", "audio").delta();
    let unchanged = mute("unchanged", "audio").delta();
    // Request-level outcomes count once per named kind; the toggles name
    // audio only, so no video cell moves.
    mute("rate_limited", "video").assert_delta(0);
    mute("unchanged", "video").assert_delta(0);
    mute("applied", "video").assert_delta(0);
    assert_eq!(
        rate_limited + unchanged,
        remaining,
        "every suppressed toggle must land in exactly one bucket"
    );
    assert!(
        rate_limited >= 1,
        "at least one genuine transition was refused"
    );

    // NOT PERMANENT: the drain waited out a read timeout, so the bucket has
    // refilled; one of these two is necessarily a transition.
    a.write(mute_kinds_frame(Some(true), None)).await;
    a.write(mute_kinds_frame(Some(false), None)).await;
    let recovered = b.expect_assignments().await;
    assert_eq!(recovered.assignments.len(), 1);
}

// ============================================================================
// Disposition matrix: what MC accepts, and what it conveys
// ============================================================================

#[tokio::test]
async fn an_extra_slot_reports_a_source_shortage_with_sender_id_absent() {
    let (stack, rig) = start_stack("mcs-extra-slot").await;
    seed_meeting_with_mh(&stack, "mcs-meeting-c").await;
    let mut a = join_as(&rig, &stack, "mcs-meeting-c", "user-a").await;
    let b = join_as(&rig, &stack, "mcs-meeting-c", "user-b").await;

    a.write(capability_frame(audio_slots(2))).await;
    // Unwatched before and after: only the view is re-sent.
    let assignments = a.expect_assignments().await;
    assert_eq!(assignments.assignments.len(), 2);
    assert_eq!(assignments.assignments[0].sender_id, Some(b.sender_id));
    assert_eq!(
        assignments.assignments[0].slot_state,
        SlotState::Active as i32
    );
    let extra = &assignments.assignments[1];
    assert_eq!(extra.slot_state, SlotState::FewerSourcesThanSlots as i32);
    assert!(
        extra.sender_id.is_none(),
        "absent MUST NOT be coerced to 0 — a shared zero is N colliding live ids"
    );
    assert_ne!(extra.sender_id, Some(0));
    assert!(extra.media_handler_url.is_empty());
}

#[tokio::test]
async fn mc_echoes_the_subscribers_own_slot_numbering() {
    // `{0, 7}` rather than `{0}`: no hardcoded-0 implementation can emit 7, and
    // since story 2 slot 7 is as servable as slot 0 (declared slots are the
    // assignment's input).
    let (stack, rig) = start_stack("mcs-echo").await;
    seed_meeting_with_mh(&stack, "mcs-meeting-d").await;
    let mut a = join_as(&rig, &stack, "mcs-meeting-d", "user-a").await;
    let b = join_as(&rig, &stack, "mcs-meeting-d", "user-b").await;
    let c = join_as(&rig, &stack, "mcs-meeting-d", "user-c").await;

    a.write(capability_frame(vec![
        slot(7, MediaKind::Audio),
        slot(0, MediaKind::Audio),
    ]))
    .await;
    // Unwatched before and after: only the view is re-sent.
    let assignments = a.expect_assignments().await;
    let ids: Vec<u32> = assignments.assignments.iter().map(|x| x.slot_id).collect();
    assert_eq!(ids, vec![7, 0], "declaration order is echoed");
    let senders: Vec<Option<u32>> = assignments
        .assignments
        .iter()
        .map(|x| x.sender_id)
        .collect();
    assert_eq!(
        senders,
        vec![Some(b.sender_id), Some(c.sender_id)],
        "join order fills declaration order"
    );
}

#[tokio::test]
async fn a_valid_but_unsatisfiable_video_slot_is_not_a_rejection() {
    let (stack, rig) = start_stack("mcs-video-slot").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-e").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();

    a.write(capability_frame(vec![slot(1, MediaKind::VideoCamera)]))
        .await;
    // Unwatched before and after: only the view is re-sent.
    let assignments = a.expect_assignments().await;
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
}

#[tokio::test]
async fn a_zero_slot_declaration_still_directs_the_client_to_send() {
    // §5 and §6 are orthogonal: a client that wants to send without receiving
    // must still be told what to produce, once somebody holds it. Story 3: the
    // join itself carried A's zero-slot declaration.
    let (stack, rig) = start_stack("mcs-zero-slot").await;
    seed_meeting_with_mh(&stack, "mcs-meeting-f").await;
    let mut a = join_as(&rig, &stack, "mcs-meeting-f", "user-a").await;
    let assignments = a.join_assignments.clone().expect("A's join-turn view");
    assert!(
        assignments.assignments.is_empty(),
        "no slot was declared, so there is no slot id to echo — ZERO_REQUESTED cannot be \
         carried without fabricating one"
    );
    let mut b = join_as(&rig, &stack, "mcs-meeting-f", "user-b").await;
    b.write(capability_frame(audio_slots(1))).await;
    let _ = b.settle().await;

    let directive = a
        .directive_until("A directed once B holds it", |d| {
            d.streams.first().is_some_and(|s| !s.targets.is_empty())
        })
        .await;
    assert_eq!(directive.streams.len(), 1);
    assert_eq!(directive.streams[0].targets.len(), 1);
    assert_eq!(
        directive.streams[0].targets[0].media_handler_url,
        SEEDED_HANDLER_URL
    );
    // An identical (empty) redeclaration is a no-op.
    a.write(capability_frame(vec![])).await;
    a.expect_no_media().await;
}

// ============================================================================
// Rejection paths — whole-declaration rejection, one test each
// ============================================================================

/// A declaration is rejected whole: an error back, NO media on the wire, and
/// the meeting actor never learns of it.
async fn assert_rejected(
    session: &mut Session,
    expected_token: &str,
    snap: &common::observability::testing::MetricSnapshot,
) {
    let message = session.expect_error().await;
    assert!(!message.is_empty());
    session.expect_no_media().await;

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", expected_token), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(0);
    snap.counter("mc_media_send_directives_total")
        .with_labels(&[
            ("outcome", "emitted_empty_targets"),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);
}

#[tokio::test]
async fn rejects_duplicate_slot_ids() {
    let (stack, rig) = start_stack("mcs-dup").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-g").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();
    a.write(capability_frame(vec![
        slot(0, MediaKind::Audio),
        slot(0, MediaKind::Audio),
    ]))
    .await;
    assert_rejected(&mut a, "duplicate_slot_id", &snap).await;
}

/// Story 3 R-1: over the cap is TRUNCATED, never rejected: the first `cap`
/// audio slots fill as usual and the excess slot reads UNAVAILABLE with its
/// declared id echoed, no sender and an empty url. The cap comes from
/// CONFIGURATION — the same value `WebTransportServer::new` publishes as
/// `mc_media_receive_slot_cap` and the JoinResponse advertises per kind (the
/// single total cap, tag 12, was deleted by MC-B).
#[tokio::test]
async fn an_over_cap_declaration_is_truncated_to_unavailable_and_counted_accepted() {
    const MEETING: &str = "mcs-meeting-h";
    let cap = client_media_config().receive_slot_caps;
    let audio_cap = u32::from(MediaKindLabel::Audio.of(cap));

    let (stack, rig) = start_stack("mcs-cap").await;
    let mut a = solo(&rig, &stack, MEETING).await;
    let mut b = join_as(&rig, &stack, MEETING, "user-b").await;
    let _ = a.settle().await;
    let _ = b.settle().await;

    assert_eq!(
        (
            a.slot_cap(MediaKind::Audio),
            a.slot_cap(MediaKind::VideoCamera),
            a.slot_cap(MediaKind::VideoScreen),
        ),
        (audio_cap, u32::from(MediaKindLabel::Video.of(cap)), 0),
        "advertised == configured, per kind; screen serves no slots"
    );
    assert_eq!(a.receive_slot_caps.len(), 3, "one entry per known kind");

    let snap = MetricAssertion::snapshot();
    a.write(capability_frame(audio_slots(audio_cap + 1))).await;
    let assignments = a
        .assignments_until("the truncated view", |x| {
            x.assignments.len() == usize::try_from(audio_cap + 1).unwrap()
        })
        .await;
    assert_eq!(
        assignments.assignments.len(),
        usize::try_from(audio_cap + 1).unwrap()
    );
    assert_eq!(assignments.assignments[0].sender_id, Some(b.sender_id));
    let excess = assignments.assignments.last().unwrap();
    assert_eq!(excess.slot_id, audio_cap, "the declared id is echoed");
    assert_eq!(excess.slot_state, SlotState::Unavailable as i32);
    assert!(excess.sender_id.is_none());
    assert!(excess.media_handler_url.is_empty());
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_slot_states_total")
        .with_labels(&[
            ("slot_state", "unavailable"),
            ("media_kind", "audio"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

/// A lowered configured cap truncates at the LOWERED value: the wire follows
/// configuration, not a literal.
#[tokio::test]
async fn a_lowered_configured_cap_truncates_at_the_configured_value() {
    let mut config = client_media_config();
    config.receive_slot_caps = [2, 4];
    let (stack, rig) = start_stack_with("mcs-cap-low", config).await;
    let mut a = solo(&rig, &stack, "mcs-meeting-h2").await;
    let _ = a.settle().await;
    assert_eq!(a.slot_cap(MediaKind::Audio), 2);
    a.write(capability_frame(audio_slots(3))).await;
    let assignments = a
        .assignments_until("the truncated view", |x| x.assignments.len() == 3)
        .await;
    let states: Vec<i32> = assignments
        .assignments
        .iter()
        .map(|x| x.slot_state)
        .collect();
    assert_eq!(
        states,
        vec![
            SlotState::FewerSourcesThanSlots as i32,
            SlotState::FewerSourcesThanSlots as i32,
            SlotState::Unavailable as i32
        ]
    );
}

#[tokio::test]
async fn rejects_a_slot_id_outside_the_16_bit_relay_field() {
    let (stack, rig) = start_stack("mcs-slot-range").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-i").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();
    // 65536 truncates to 0 under `as`, which would silently address a
    // different slot. It must reject, never clamp.
    a.write(capability_frame(vec![slot(65_536, MediaKind::Audio)]))
        .await;
    assert_rejected(&mut a, "slot_id_out_of_range", &snap).await;
}

#[tokio::test]
async fn rejects_a_zero_pinned_sender_id() {
    let (stack, rig) = start_stack("mcs-pin-zero").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-j").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();
    a.write(capability_frame(vec![pinned_slot(0, 0)])).await;
    assert_rejected(&mut a, "pinned_sender_id_zero", &snap).await;
}

#[tokio::test]
async fn rejects_an_out_of_range_pinned_sender_id() {
    let (stack, rig) = start_stack("mcs-pin-range").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-k").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();
    a.write(capability_frame(vec![pinned_slot(0, 65_536)]))
        .await;
    assert_rejected(&mut a, "pinned_sender_id_out_of_range", &snap).await;
}

#[tokio::test]
async fn rejects_an_unset_media_kind() {
    let (stack, rig) = start_stack("mcs-kind-unset").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-l").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();
    a.write(capability_frame(vec![slot(0, MediaKind::Unspecified)]))
        .await;
    assert_rejected(&mut a, "media_kind_unspecified", &snap).await;
}

/// The rejection-REPLY bound engages, and suppressing a reply never suppresses
/// the count.
#[tokio::test]
async fn capability_rejection_replies_are_rate_limited_but_never_uncounted() {
    const REJECTIONS: u64 = 40;

    let (stack, rig) = start_stack("mcs-reject-rate").await;
    let mut a = solo(&rig, &stack, "mcs-meeting-reject-rate").await;
    // After the join: its join-turn view is not this test's subject.
    let snap = MetricAssertion::snapshot();

    // Two DISTINCT out-of-range slot ids, alternating, so the
    // identical-redeclaration short-circuit never fires.
    for i in 0..REJECTIONS {
        let slot_id = if i % 2 == 0 { 65_536 } else { 65_537 };
        a.write(capability_frame(vec![slot(slot_id, MediaKind::Audio)]))
            .await;
    }

    let mut errors_received = 0u64;
    while a.next_media().await.is_some() {
        errors_received += 1;
    }
    assert!(errors_received < REJECTIONS, "the reply bound must engage");
    assert!(errors_received > 0, "the burst must be spendable");

    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "slot_id_out_of_range"),
            ("key_custody", "operator"),
        ])
        .assert_delta(REJECTIONS);

    a.write(capability_frame(vec![slot(65_536, MediaKind::Audio)]))
        .await;
    assert!(
        !a.expect_error().await.is_empty(),
        "the bound is not permanent"
    );
}

#[tokio::test]
async fn rejects_declarations_past_the_per_connection_budget() {
    let snap = MetricAssertion::snapshot();
    let mut config = client_media_config();
    config.max_receive_capability_declarations = 1;
    let (stack, rig) = start_stack_with("mcs-budget", config).await;
    let mut a = solo(&rig, &stack, "mcs-meeting-n").await;

    a.write(capability_frame(audio_slots(1))).await;
    let _ = a.expect_assignments().await;

    a.write(capability_frame(audio_slots(2))).await;
    assert!(!a.expect_error().await.is_empty());

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
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("mcs-redirect").await;
    seed_meeting_with_mh(&stack, "mcs-meeting-o").await;
    let mut a = join_as(&rig, &stack, "mcs-meeting-o", "user-a").await;
    let mut b = join_as(&rig, &stack, "mcs-meeting-o", "user-b").await;

    a.write(encode_framed(&ClientMessage {
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
    // PREMISE: the poison was actually processed.
    settle_no_reply_message().await;
    snap.counter("mc_participant_mh_status_total")
        .with_labels(&[("state", "connected")])
        .assert_delta(1);

    a.write(capability_frame(audio_slots(1))).await;
    b.write(capability_frame(audio_slots(1))).await;
    // Full-replace semantics: the LAST directive and assignments are A's view.
    let (directive, assignments) = a.settle().await;
    let directive = directive.expect("A's directive");
    let assignments = assignments.expect("A's slot view");

    assert_eq!(
        directive.streams[0].targets[0].media_handler_url,
        SEEDED_HANDLER_URL
    );
    assert_eq!(
        assignments.assignments[0].media_handler_url,
        SEEDED_HANDLER_URL
    );
    let needle = ATTACKER_URL.as_bytes();
    for bytes in [directive.encode_to_vec(), assignments.encode_to_vec()] {
        assert!(
            !bytes.windows(needle.len()).any(|w| w == needle),
            "attacker url reached a media message"
        );
    }
}

/// S4 pin (a) + S3, in the per-EDGE shape: in a two-handler meeting where the
/// pair's edge sits on one handler, a client reporting (a) an attacker url as
/// connected and (b) the edge's REAL handler as failed changes nothing — the
/// client-reported map is never a url source and never narrows connectivity.
/// Connectivity is only what the handlers report.
#[tokio::test]
async fn client_reported_status_neither_redirects_nor_narrows_a_per_edge_url() {
    let (stack, rig) = start_stack("mcs-redirect-edge").await;
    seed_meeting_with_handlers(
        &stack,
        "mcs-meeting-edge",
        vec![mh_handler("mh-0"), mh_handler("mh-1")],
    )
    .await;
    let mut a = join_as(&rig, &stack, "mcs-meeting-edge", "user-a").await;
    let mut b = test_common::media_session::join_as_on(
        &rig,
        &stack,
        "mcs-meeting-edge",
        "user-b",
        &["mh-1"],
    )
    .await;
    a.write(capability_frame(audio_slots(1))).await;
    b.write(capability_frame(audio_slots(1))).await;
    let before = a
        .assignments_until("A holds B on mh-1", |x| {
            x.assignments.first().and_then(|a| a.sender_id) == Some(b.sender_id)
        })
        .await;
    assert_eq!(before.assignments[0].media_handler_url, "wt://mh-1:4433");

    let status = |url: &str, state: proto_gen::dark_tower::signaling::v1::ConnectionState| {
        MhConnectionStatus {
            mh_url: url.to_string(),
            state: state as i32,
            failure_reason: None,
            failure_code: None,
            observed_at: None,
        }
    };
    for s in [&mut a, &mut b] {
        s.write(encode_framed(&ClientMessage {
            trace_parent: String::new(),
            trace_state: String::new(),
            message: Some(client_message::Message::MediaConnectionUpdate(
                MediaConnectionUpdate {
                    statuses: vec![
                        status(
                            ATTACKER_URL,
                            proto_gen::dark_tower::signaling::v1::ConnectionState::Connected,
                        ),
                        status(
                            "wt://mh-1:4433",
                            proto_gen::dark_tower::signaling::v1::ConnectionState::Failed,
                        ),
                    ],
                },
            )),
        }))
        .await;
    }
    settle_no_reply_message().await;
    // Nothing moved: no new view at all for A, so the edge is still on mh-1.
    a.expect_no_media().await;
    b.write(capability_frame(audio_slots(2))).await; // force B a fresh view
    let (_, b_view) = b.settle().await;
    let b_view = b_view.expect("B's view");
    let needle = ATTACKER_URL.as_bytes();
    assert!(!b_view
        .encode_to_vec()
        .windows(needle.len())
        .any(|w| w == needle));
    assert_eq!(b_view.assignments[0].media_handler_url, "wt://mh-1:4433");
    assert_eq!(b_view.assignments[0].sender_id, Some(a.sender_id));
}

// ============================================================================
// A client that never declares
// ============================================================================

#[tokio::test]
async fn a_client_that_joins_without_a_declaration_is_still_directed_and_stays_healthy() {
    let (stack, rig) = start_stack("mcs-silent").await;
    seed_meeting_with_mh(&stack, "mcs-meeting-p").await;
    let mut a = join_as(&rig, &stack, "mcs-meeting-p", "user-a").await;
    let mut b = join_as(&rig, &stack, "mcs-meeting-p", "user-b").await;
    let snap = MetricAssertion::snapshot();

    // Story 3: the join IS the declaration (absent = zero slots), so A — which
    // never sent a ReceiveCapability — is directed once B's declaration makes
    // B hold A.
    b.write(capability_frame(audio_slots(1))).await;
    let _ = b.settle().await;
    let directive = a
        .directive_until("A directed once B holds A", |d| {
            d.streams.first().is_some_and(|s| !s.targets.is_empty())
        })
        .await;
    assert_eq!(directive.streams.len(), 1);

    a.write(encode_framed(&ClientMessage {
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

// ============================================================================
// Every handler url a client sees comes from the ONE frozen handler set
// (ADR-0036 §9, R-33)
// ============================================================================
//
// Urls below are INLINE WRITTEN LITERALS, deliberately not derived from
// `mh_handler()`: a test written against the same symbol as the code passes
// whatever that symbol becomes. If `mh_handler`'s url format ever changes,
// these go RED rather than silently tracking it.

/// Whatever order Redis enumerates the handlers in, every client is offered
/// the FULL registered set, and every send target and slot url it is given is
/// one of those urls verbatim — the url of the handler that owns that edge.
/// Properties only: nothing here asserts WHICH handler carries an edge except
/// where the pair shares exactly one.
#[tokio::test]
async fn every_client_is_offered_the_full_set_and_steered_only_to_owning_handlers() {
    let (stack, rig) = start_stack("mcs-steer-order").await;
    let registered = ["wt://mh-0:4433".to_string(), "wt://mh-1:4433".to_string()];

    for (index, order) in [["mh-0", "mh-1"], ["mh-1", "mh-0"]].into_iter().enumerate() {
        let meeting = format!("mcs-meeting-order-{index}");
        seed_meeting_with_handlers(
            &stack,
            &meeting,
            order.iter().map(|h| mh_handler(h)).collect(),
        )
        .await;
        // A reaches both handlers; B only mh-0; C both.
        let mut a = join_as(&rig, &stack, &meeting, "user-a").await;
        let b = test_common::media_session::join_as_on(&rig, &stack, &meeting, "user-b", &["mh-0"])
            .await;
        let mut c = join_as(&rig, &stack, &meeting, "user-c").await;

        for s in [&a, &b, &c] {
            let mut offered = s.media_servers.clone();
            offered.sort();
            assert_eq!(
                offered, registered,
                "order {order:?}: the full set, every time"
            );
        }

        a.write(capability_frame(audio_slots(2))).await;
        c.write(capability_frame(audio_slots(1))).await;
        let assignments = a
            .assignments_until("A holds B and C", |x| {
                x.assignments
                    .iter()
                    .filter(|s| s.sender_id.is_some())
                    .count()
                    == 2
            })
            .await;
        for slot in &assignments.assignments {
            assert!(
                registered.contains(&slot.media_handler_url),
                "order {order:?}: a slot url is a registered url verbatim"
            );
            if slot.sender_id == Some(b.sender_id) {
                assert_eq!(
                    slot.media_handler_url, "wt://mh-0:4433",
                    "order {order:?}: the only handler A and B share"
                );
            }
        }
        assert!(assignments.unreachable_sender_ids.is_empty());
        let (directive, _) = c.settle().await;
        for target in &directive.expect("C's directive").streams[0].targets {
            assert!(
                registered.contains(&target.media_handler_url),
                "order {order:?}: a send target is a registered url verbatim"
            );
        }
    }
}
