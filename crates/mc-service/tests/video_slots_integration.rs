//! Story 3 MC-A over the real WebTransport accept loop and the real meeting
//! actor: the join's media declaration (R-2, R-22), per-kind truncation (R-1),
//! video slots and their flat states (R-3, R-4), watched-only video directives
//! and their withdrawal (R-8), the video-off lever (R-26), and the reconnect
//! publish-set refusal at the actor seam.
//!
//! Every assertion reads per-entity evidence — the session's own wire messages
//! and the pushes MC recorded — plus metric DELTAS taken after setup.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use common::observability::testing::MetricAssertion;
use mc_service::media_signaling::{MediaKindLabel, PublishSet, ReceiveCapabilityDeclaration};
use proto_gen::dark_tower::signaling::v1::{
    Codec, MediaKind, ReceiveCapability, SendDirective, SlotState, StreamAssignments, TransportMode,
};

#[path = "common/mod.rs"]
mod test_common;

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::media_session::{
    capability_frame, connection_id_for, mute_frame, slot, start_stack, start_stack_with,
    try_join_shaped, JoinShape, Session,
};
use test_common::{
    build_test_stack, client_media_config, mh_handler, seed_meeting_with_handlers,
    seed_meeting_with_mh, test_identity_key, test_join_media, TestStackHandles,
};

const HANDLER: &str = "mh-test-1";

/// POSITIVE poll (bounded; a rig deadline, never a performance assertion):
/// wait until MORE than `n` pushes were recorded, and return them all.
async fn pushes_beyond(
    stack: &TestStackHandles,
    n: usize,
) -> Vec<test_common::RegisterMeetingCall> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let calls = stack.mh_reg_client.calls();
        if calls.len() > n {
            return calls;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no push beyond {n} arrived"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// An actor-level join of `user` (audio publisher, no declaration).
async fn actor_join(
    stack: &TestStackHandles,
    meeting: &mc_service::actors::MeetingActorHandle,
    user: &str,
    media: mc_service::actors::JoinMedia,
) -> (
    mc_service::actors::JoinResult,
    tokio::sync::mpsc::Receiver<bytes::Bytes>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);
    let _ = stack;
    let joined = meeting
        .connection_join(
            format!("conn-{user}"),
            format!("user-{user}"),
            format!("part-{user}"),
            String::new(),
            false,
            test_identity_key(),
            Some(tx),
            media,
        )
        .await
        .unwrap();
    (joined, rx)
}

/// Join with `shape` and report a media connection to the one handler.
async fn joined(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting: &str,
    user: &str,
    shape: JoinShape,
) -> Session {
    let s = try_join_shaped(rig, stack, meeting, user, shape)
        .await
        .unwrap_or_else(|e| panic!("{user}: expected JoinResponse, got {e}"));
    assert_eq!(
        s.connect_to(stack, HANDLER, &connection_id_for(user, HANDLER))
            .await,
        s.sender_id
    );
    s
}

fn audio(id: u32) -> proto_gen::dark_tower::signaling::v1::ReceiveSlot {
    slot(id, MediaKind::Audio)
}

fn video(id: u32) -> proto_gen::dark_tower::signaling::v1::ReceiveSlot {
    slot(id, MediaKind::VideoCamera)
}

fn av() -> JoinShape {
    JoinShape::publishing(&[MediaKind::Audio, MediaKind::VideoCamera])
}

fn receive_only() -> JoinShape {
    JoinShape::publishing(&[])
}

fn video_stream(d: &SendDirective) -> Option<&proto_gen::dark_tower::signaling::v1::SendStream> {
    d.streams.iter().find(|s| s.stream_number == 1)
}

fn states(a: &StreamAssignments) -> Vec<(u32, i32, Option<u32>)> {
    a.assignments
        .iter()
        .map(|x| (x.slot_id, x.slot_state, x.sender_id))
        .collect()
}

fn join_failure<'a>(
    snap: &'a common::observability::testing::MetricSnapshot,
    error_type: &'static str,
) -> common::observability::testing::CounterQuery<'a> {
    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", error_type)])
}

// ============================================================================
// R-2 / R-22: join refusals, each with a positive twin differing in ONE field
// ============================================================================

#[tokio::test]
async fn join_refusals_are_counted_generically_and_each_has_an_admitted_twin() {
    let (stack, rig) = start_stack("vs-refusals").await;
    seed_meeting_with_mh(&stack, "vs-refusals").await;
    let snap = MetricAssertion::snapshot();

    let cases: Vec<(&str, JoinShape, JoinShape, &'static str)> = vec![
        (
            "v2-only",
            JoinShape {
                header_versions: vec![2],
                ..JoinShape::default()
            },
            JoinShape::default(),
            "header_version_unsupported",
        ),
        (
            "screen",
            JoinShape::publishing(&[MediaKind::VideoScreen]),
            JoinShape::publishing(&[MediaKind::VideoCamera]),
            "publish_kind_invalid",
        ),
        (
            "neither",
            receive_only(),
            receive_only().receiving(vec![audio(0)]),
            "publishes_and_receives_nothing",
        ),
        (
            "bad-decl",
            JoinShape::default().receiving(vec![slot(0, MediaKind::VideoScreen)]),
            JoinShape::default().receiving(vec![video(0)]),
            "receive_capability_invalid",
        ),
    ];
    // Admitted sessions are HELD: the last participant's leave ends the
    // meeting, and a later join would then be `meeting_not_found`.
    let mut held = Vec::new();
    for (i, (name, refused, admitted, token)) in cases.into_iter().enumerate() {
        let err = try_join_shaped(&rig, &stack, "vs-refusals", &format!("u-{i}-x"), refused)
            .await
            .err()
            .unwrap_or_else(|| panic!("{name}: must be refused"));
        // Generic: one message for every media-declaration refusal.
        assert!(err.contains("Invalid join request"), "{name}: {err}");
        assert!(err.contains("InvalidRequest"), "{name}: {err}");
        join_failure(&snap, token).assert_delta(1);
        let ok = try_join_shaped(&rig, &stack, "vs-refusals", &format!("u-{i}-ok"), admitted).await;
        assert!(
            ok.is_ok(),
            "{name}: the one-field twin must be admitted: {:?}",
            ok.err()
        );
        held.extend(ok);
    }
    // The malformed join declaration's specific cause, on the capability series.
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "media_kind_unsupported"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

/// R-2: a redeclaration that would leave a receive-only participant receiving
/// nothing is refused, counted, and the previous declaration stays in force.
#[tokio::test]
async fn a_redeclaration_to_nothing_is_refused_and_the_previous_view_is_kept() {
    let (stack, rig) = start_stack("vs-redecl").await;
    seed_meeting_with_mh(&stack, "vs-redecl").await;
    let mut p = joined(&rig, &stack, "vs-redecl", "user-p", JoinShape::default()).await;
    let mut r = joined(
        &rig,
        &stack,
        "vs-redecl",
        "user-r",
        receive_only().receiving(vec![audio(0)]),
    )
    .await;
    let held = r
        .assignments_until("R hears P", |a| {
            a.assignments.first().and_then(|x| x.sender_id) == Some(p.sender_id)
        })
        .await;
    let _ = p.settle().await;
    let snap = MetricAssertion::snapshot();

    r.write(capability_frame(vec![])).await;
    assert!(!r.expect_error().await.is_empty());
    r.expect_no_media().await;
    p.expect_no_media().await;
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "would_publish_and_receive_nothing"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    // The previous declaration is still in force: re-sending it is a no-op.
    r.write(capability_frame(vec![audio(0)])).await;
    r.expect_no_media().await;
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "accepted_unchanged"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    assert_eq!(held.assignments[0].sender_id, Some(p.sender_id));
}

/// Client contract (shim a): the SDK re-sends its join-carried declaration
/// post-join; MC answers `accepted_unchanged` and nothing moves.
#[tokio::test]
async fn an_identical_post_join_redeclaration_of_the_join_declaration_is_a_no_op() {
    let (stack, rig) = start_stack("vs-shim-a").await;
    seed_meeting_with_mh(&stack, "vs-shim-a").await;
    let decl = vec![audio(0), video(1)];
    let mut a = joined(
        &rig,
        &stack,
        "vs-shim-a",
        "user-a",
        av().receiving(decl.clone()),
    )
    .await;
    let _ = a.settle().await;
    let snap = MetricAssertion::snapshot();
    let pushes = stack.mh_reg_client.calls().len();
    a.write(capability_frame(decl)).await;
    a.expect_no_media().await;
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "accepted_unchanged"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(0);
    assert_eq!(
        stack.mh_reg_client.calls().len(),
        pushes,
        "no actor hop, no re-push"
    );
}

// ============================================================================
// R-3 / R-4: video slots and their flat states
// ============================================================================

/// A video slot holds ANY participant: an audio-only one as AVATAR_NO_VIDEO
/// (named, no url), a video publisher as ACTIVE with NO sender, and the
/// truncated excess as UNAVAILABLE with its id echoed.
#[tokio::test]
async fn video_slots_show_no_video_avatars_senderless_live_video_and_unavailable() {
    let mut config = client_media_config();
    config.receive_slot_caps = [8, 2];
    let (stack, rig) = start_stack_with("vs-states", config).await;
    seed_meeting_with_mh(&stack, "vs-states").await;
    let a_only = joined(&rig, &stack, "vs-states", "user-a", JoinShape::default()).await;
    let p = joined(&rig, &stack, "vs-states", "user-p", av()).await;
    let mut s = joined(
        &rig,
        &stack,
        "vs-states",
        "user-s",
        receive_only().receiving(vec![video(10), video(11), video(12)]),
    )
    .await;
    let view = s
        .assignments_until("both video slots filled", |a| {
            a.assignments
                .iter()
                .filter(|x| x.slot_state != SlotState::FewerSourcesThanSlots as i32)
                .count()
                == 3
        })
        .await;
    assert_eq!(
        states(&view),
        vec![
            (10, SlotState::AvatarNoVideo as i32, Some(a_only.sender_id)),
            (11, SlotState::Active as i32, None),
            (12, SlotState::Unavailable as i32, None),
        ]
    );
    assert!(
        view.assignments[0].media_handler_url.is_empty(),
        "no edge, no url"
    );
    assert!(
        !view.assignments[1].media_handler_url.is_empty(),
        "the live edge's handler"
    );
    assert!(view.assignments[2].media_handler_url.is_empty());
    let _ = p;
}

// ============================================================================
// R-8: watched-only video directive, withdrawal, restore
// ============================================================================

#[tokio::test]
async fn a_video_directive_is_watched_only_withdrawn_on_the_last_leave_and_restored() {
    let (stack, rig) = start_stack("vs-watch").await;
    seed_meeting_with_mh(&stack, "vs-watch").await;
    let mut p = joined(&rig, &stack, "vs-watch", "user-p", av()).await;

    // Unwatched: BOTH published kinds listed, both with EMPTY targets.
    let first = p.join_directive.clone().expect("P directed at join");
    assert_eq!(
        first
            .streams
            .iter()
            .map(|s| s.stream_number)
            .collect::<Vec<_>>(),
        vec![0, 1],
        "audio 0 and video 1, always listed"
    );
    assert!(first.streams.iter().all(|s| s.targets.is_empty()));
    let v = video_stream(&first).unwrap();
    assert_eq!(v.media_kind, MediaKind::VideoCamera as i32);
    let e = v.encoding.as_ref().unwrap();
    let configured = client_media_config().video_encoding;
    assert_eq!(e.codec, Codec::Vp8 as i32);
    assert_eq!(
        (e.width, e.height, e.frame_rate, e.max_bitrate_bps),
        (
            configured.width(),
            configured.height(),
            configured.frame_rate_hz(),
            configured.max_bitrate_bps()
        )
    );
    assert_eq!(
        e.min_keyframe_interval_ms,
        Some(configured.min_keyframe_interval_ms())
    );
    let a = &first.streams[0];
    assert_eq!(a.encoding.as_ref().unwrap().min_keyframe_interval_ms, None);

    // A watcher appears: the video target is STREAM_PER_GROUP.
    let s = joined(
        &rig,
        &stack,
        "vs-watch",
        "user-s",
        receive_only().receiving(vec![video(0)]),
    )
    .await;
    let watched = p
        .directive_until("P's video watched", |d| {
            video_stream(d).is_some_and(|v| !v.targets.is_empty())
        })
        .await;
    let v = video_stream(&watched).unwrap();
    assert_eq!(v.targets.len(), 1);
    assert_eq!(
        v.targets[0].transport_mode,
        TransportMode::StreamPerGroup as i32
    );
    assert!(
        watched.streams[0].targets.is_empty(),
        "nobody holds P's audio"
    );
    // The MH snapshot: one video edge, stream 1, supersede, priority 2; and
    // P's two streams declared with their transports.
    let last = stack.mh_reg_client.calls().last().cloned().unwrap();
    let video_edges: Vec<_> = last
        .assignment
        .egress_streams
        .iter()
        .filter(|e| e.stream_number == 1)
        .collect();
    assert_eq!(video_edges.len(), 1);
    assert!(video_edges[0].supersede_on_independent_frame);
    assert_eq!(video_edges[0].priority_group, 2);
    let p_streams: Vec<(u8, TransportMode)> = last
        .assignment
        .source_streams
        .iter()
        .filter(|x| u32::from(x.sender.get().get()) == p.sender_id)
        .map(|x| (x.stream_number, x.transport_mode.mode()))
        .collect();
    assert_eq!(
        p_streams,
        vec![
            (0, TransportMode::Datagram),
            (1, TransportMode::StreamPerGroup)
        ]
    );

    // The last watcher leaves: withdrawn (listed, empty) — counted ONCE.
    let snap = MetricAssertion::snapshot();
    s.close();
    let withdrawn = p
        .directive_until("P's video withdrawn", |d| {
            video_stream(d).is_some_and(|v| v.targets.is_empty())
        })
        .await;
    assert_eq!(
        withdrawn.streams.len(),
        2,
        "withdrawal is empty targets, never removal"
    );
    // A forced unrelated re-render (P toggles its own audio mute) re-sends
    // nothing to P and must not count the withdrawal again.
    p.write(mute_frame(true, false)).await;
    p.write(mute_frame(false, false)).await;
    p.expect_no_media().await;
    let withdrawals = |kind: &'static str| {
        snap.counter("mc_media_send_directive_withdrawals_total")
            .with_labels(&[
                ("media_kind", kind),
                ("reason", "unwatched"),
                ("key_custody", "operator"),
            ])
    };
    withdrawals("video").assert_delta(1);
    withdrawals("audio").assert_delta(0);

    // A new watcher restores it.
    let _s2 = joined(
        &rig,
        &stack,
        "vs-watch",
        "user-s2",
        receive_only().receiving(vec![video(0)]),
    )
    .await;
    let _ = p
        .directive_until("P's video restored", |d| {
            video_stream(d).is_some_and(|v| !v.targets.is_empty())
        })
        .await;
    withdrawals("video").assert_delta(1);
}

// ============================================================================
// R-26: the video-off lever
// ============================================================================

/// With `MC_MAX_VIDEO_RECEIVE_SLOTS=0`: a video-only receive-only client is
/// ADMITTED (judged as sent), every video slot is UNAVAILABLE, no video target
/// exists anywhere, and audio is identical to a run where nobody declares
/// video. An integration test, not an env-test, because it needs MC restarted.
#[tokio::test]
async fn the_video_cap_zero_lever_turns_video_off_and_leaves_audio_identical() {
    async fn run(
        label: &str,
        video_cap: u8,
        declare_video: bool,
    ) -> (
        Vec<(u32, i32, Option<u32>)>,
        SendDirective,
        Option<Vec<i32>>,
    ) {
        let mut config = client_media_config();
        config.receive_slot_caps = [8, video_cap];
        let (stack, rig) = start_stack_with(label, config).await;
        seed_meeting_with_mh(&stack, label).await;
        let mut p = joined(&rig, &stack, label, "user-p", av()).await;
        let mut slots = vec![audio(0)];
        if declare_video {
            slots.push(video(1));
            slots.push(video(2));
        }
        let mut s = joined(&rig, &stack, label, "user-s", av().receiving(slots)).await;
        let view = s
            .assignments_until("S hears P", |a| {
                a.assignments.first().and_then(|x| x.sender_id).is_some()
            })
            .await;
        let directive = p
            .directive_until("P's audio watched", |d| !d.streams[0].targets.is_empty())
            .await;
        let audio_only: Vec<(u32, i32, Option<u32>)> = states(&view)
            .into_iter()
            .filter(|(id, _, _)| *id == 0)
            .collect();
        let video_states = declare_video.then(|| {
            view.assignments
                .iter()
                .filter(|x| x.media_kind == MediaKind::VideoCamera as i32)
                .map(|x| x.slot_state)
                .collect()
        });
        // A video-only, receive-only client is admitted even at cap 0.
        let r = try_join_shaped(
            &rig,
            &stack,
            label,
            "user-r",
            receive_only().receiving(vec![video(5)]),
        )
        .await;
        assert!(
            r.is_ok(),
            "{label}: judged as sent, never a truncation refusal: {:?}",
            r.err()
        );
        drop((p, s));
        (audio_only, directive, video_states)
    }
    let (audio_off, directive_off, video_off) = run("vs-lever-off", 0, true).await;
    let (audio_plain, directive_plain, _) = run("vs-lever-plain", 4, false).await;
    assert_eq!(
        video_off,
        Some(vec![
            SlotState::Unavailable as i32,
            SlotState::Unavailable as i32
        ])
    );
    assert!(
        video_stream(&directive_off).is_some_and(|v| v.targets.is_empty()),
        "no camera is directed to send"
    );
    assert_eq!(audio_off, audio_plain, "audio slots identical");
    assert_eq!(
        directive_off.streams[0], directive_plain.streams[0],
        "the audio directive is identical"
    );
}

// ============================================================================
// Reconnect publish-set refusal (actor seam; not production-reachable until
// reconnect is wired to the accept path)
// ============================================================================

#[tokio::test]
async fn a_reconnect_presenting_a_different_publish_set_is_refused_and_never_adopted() {
    let stack = build_test_stack("vs-reconnect").await;
    seed_meeting_with_handlers(&stack, "vs-reconnect", vec![mh_handler(HANDLER)]).await;
    let meeting = stack
        .controller_handle
        .get_meeting_handle("vs-reconnect".to_string())
        .await
        .unwrap();
    let (joined, _rx_a) = actor_join(
        &stack,
        &meeting,
        "a",
        test_join_media(&stack, &[mh_handler(HANDLER)]),
    )
    .await;
    meeting
        .connection_disconnected(
            "conn-a".to_string(),
            "part-a".to_string(),
            mc_service::actors::DisconnectCause::ConnectionLost,
        )
        .await
        .unwrap();

    let snap = MetricAssertion::snapshot();
    let pushes = stack.mh_reg_client.calls().len();
    let refused = meeting
        .connection_reconnect(
            "conn-a2".to_string(),
            joined.correlation_id.clone(),
            joined.binding_token.clone(),
            PublishSet::of(&[MediaKindLabel::Audio, MediaKindLabel::Video]),
            None,
        )
        .await;
    assert!(
        matches!(
            refused,
            Err(mc_service::errors::McError::ReconnectPublishSetMismatch)
        ),
        "got {refused:?}"
    );
    join_failure(&snap, "reconnect_publish_set_mismatch").assert_delta(1);

    // The entry is untouched: the SAME binding still reconnects with the
    // ORIGINAL set (the refusal rotated nothing) ...
    meeting
        .connection_reconnect(
            "conn-a3".to_string(),
            joined.correlation_id.clone(),
            joined.binding_token.clone(),
            PublishSet::of(&[MediaKindLabel::Audio]),
            None,
        )
        .await
        .expect("the original set reconnects: the refusal rotated nothing");
    // ... and a push FORCED after the refusal (B's join changes the snapshot)
    // declares A's audio only. No push after the refusal ever carries A's
    // stream 1.
    let (_b, _rx_b) = actor_join(
        &stack,
        &meeting,
        "b",
        test_join_media(&stack, &[mh_handler(HANDLER)]),
    )
    .await;
    let calls = pushes_beyond(&stack, pushes).await;
    let a_streams = |c: &test_common::RegisterMeetingCall| -> Vec<u8> {
        c.assignment
            .source_streams
            .iter()
            .filter(|s| s.sender == joined.sender_id)
            .map(|s| s.stream_number)
            .collect()
    };
    assert_eq!(a_streams(calls.last().unwrap()), vec![0]);
    assert!(
        calls[pushes..].iter().all(|c| !a_streams(c).contains(&1)),
        "the publish set was never adopted from the refused reconnect"
    );
}

// ============================================================================
// A frame-sized declaration: admitted, truncated, and a too-large view is
// refused loudly rather than written as an unreadable frame
// ============================================================================

#[tokio::test]
async fn a_frame_sized_declaration_is_truncated_and_an_oversized_view_fails_closed() {
    let (stack, rig) = start_stack("vs-big").await;
    seed_meeting_with_mh(&stack, "vs-big").await;
    let mut a = joined(&rig, &stack, "vs-big", "user-a", JoinShape::default()).await;
    let snap = MetricAssertion::snapshot();

    // As many minimal video slots as fit the 64 KiB signalling frame.
    let mut slots = Vec::new();
    let mut id = 0u32;
    while prost::Message::encoded_len(&ReceiveCapability {
        slots: slots.clone(),
    }) < 60 * 1024
    {
        slots.push(video(id));
        id += 1;
    }
    let n = slots.len();
    assert!(n > 5_000, "positive control: {n} slots");
    // The big declaration, then a small one: per-connection FIFO is the fence.
    a.write(capability_frame(slots)).await;
    a.write(capability_frame(vec![audio(0)])).await;
    // The FIRST media message is the small view: no frame was written for the
    // big one (its ~N-entry view exceeds the frame the client can read).
    let view = a.expect_assignments().await;
    assert_eq!(view.assignments.len(), 1);
    // Both accepted (never a cap refusal); the big view refused at
    // composition, counted.
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[("outcome", "accepted"), ("key_custody", "operator")])
        .assert_delta(2);
    snap.counter("mc_media_slot_view_emissions_total")
        .with_labels(&[
            ("outcome", "composition_failed"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

// ============================================================================
// Config echoes, published from the same values the wire carries
// ============================================================================

/// Both per-kind caps, and the directed minimum keyframe interval in SECONDS,
/// are published at boot from the very `ClientMediaConfig` the JoinResponse
/// and the video directive read.
#[tokio::test]
async fn the_cap_and_keyframe_interval_gauges_echo_the_directed_configuration() {
    let snap = MetricAssertion::snapshot();
    let mut config = client_media_config();
    config.receive_slot_caps = [6, 0];
    let (stack, rig) = start_stack_with("vs-gauges", config).await;
    seed_meeting_with_mh(&stack, "vs-gauges").await;
    let p = joined(&rig, &stack, "vs-gauges", "user-p", av()).await;
    snap.gauge("mc_media_receive_slot_cap")
        .with_labels(&[("media_kind", "audio"), ("key_custody", "operator")])
        .assert_value(6.0);
    snap.gauge("mc_media_receive_slot_cap")
        .with_labels(&[("media_kind", "video"), ("key_custody", "operator")])
        .assert_value(0.0);
    let directed = video_stream(p.join_directive.as_ref().unwrap())
        .and_then(|v| v.encoding.as_ref())
        .and_then(|e| e.min_keyframe_interval_ms)
        .unwrap();
    snap.gauge("mc_media_video_keyframe_min_interval_seconds")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(f64::from(directed) / 1000.0);
    assert_eq!(p.max_kind_receive_slots, (Some(6), Some(0)));
}

/// R-2 at the reconnect seam: a reconnect's declaration is a REDECLARATION, so
/// one that would leave a receive-only participant receiving nothing is
/// refused and COUNTED exactly as on the post-join path, and the previous
/// declaration (and its occupant) stays in force.
#[tokio::test]
async fn a_reconnect_redeclaring_to_nothing_is_counted_and_keeps_the_previous_declaration() {
    use test_common::media_session::notify_connected;
    const M: &str = "vs-reconnect-neither";
    let stack = build_test_stack(M).await;
    seed_meeting_with_handlers(&stack, M, vec![mh_handler(HANDLER)]).await;
    let meeting = stack
        .controller_handle
        .get_meeting_handle(M.to_string())
        .await
        .unwrap();
    let (p, _rx_p) = actor_join(
        &stack,
        &meeting,
        "p",
        test_join_media(&stack, &[mh_handler(HANDLER)]),
    )
    .await;
    let mut media = test_join_media(&stack, &[mh_handler(HANDLER)]);
    media.publishes = PublishSet::NONE;
    media.demand = ReceiveCapabilityDeclaration::parse(
        &ReceiveCapability {
            slots: vec![audio(0)],
        },
        true,
    )
    .unwrap()
    .truncate(client_media_config().receive_slot_caps);
    let (r, _rx_r) = actor_join(&stack, &meeting, "r", media).await;
    for user in ["p", "r"] {
        notify_connected(&stack, M, &format!("user-{user}"), HANDLER, user).await;
    }
    // R holding P, as a predicate over a push.
    let r_holds_p = |c: &test_common::RegisterMeetingCall| {
        c.assignment
            .egress_streams
            .iter()
            .any(|e| e.subscriber == r.sender_id && e.candidate_sources == vec![p.sender_id])
    };
    // POSITIVE poll: wait for the push in which R holds P.
    let mut seen = 0;
    loop {
        let calls = pushes_beyond(&stack, seen).await;
        seen = calls.len();
        if calls.last().is_some_and(r_holds_p) {
            break;
        }
    }
    meeting
        .connection_disconnected(
            "conn-r".to_string(),
            "part-r".to_string(),
            mc_service::actors::DisconnectCause::ConnectionLost,
        )
        .await
        .unwrap();

    let snap = MetricAssertion::snapshot();
    let empty =
        ReceiveCapabilityDeclaration::empty().truncate(client_media_config().receive_slot_caps);
    meeting
        .connection_reconnect(
            "conn-r2".to_string(),
            r.correlation_id.clone(),
            r.binding_token.clone(),
            PublishSet::NONE,
            Some(empty),
        )
        .await
        .expect("the reconnect itself is admitted; only its declaration is refused");
    snap.counter("mc_media_receive_capability_declarations_total")
        .with_labels(&[
            ("outcome", "would_publish_and_receive_nothing"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    // A push FORCED after the refusal (a third participant joins) still has R
    // holding P: the previous declaration, and its occupant, were kept.
    let before = stack.mh_reg_client.calls().len();
    let (_q, _rx_q) = actor_join(
        &stack,
        &meeting,
        "q",
        test_join_media(&stack, &[mh_handler(HANDLER)]),
    )
    .await;
    let calls = pushes_beyond(&stack, before).await;
    assert!(
        r_holds_p(calls.last().unwrap()),
        "the previous declaration is kept"
    );
}

/// The positive half of "a mute change of EITHER kind re-emits to every
/// holder": A (AV) client-mutes its VIDEO. B, who holds A in a video slot,
/// sees AVATAR_CLIENT_MUTED naming A with the edge's url; the next push
/// declares A's stream 1 PAUSED and stream 0 FORWARD; C, who holds A in an
/// audio slot only, is told nothing (fenced by a later audio mute that C does
/// see); unmuting returns B's slot to a senderless ACTIVE.
#[tokio::test]
async fn a_video_client_mute_re_emits_to_video_holders_and_pauses_only_stream_one() {
    let (stack, rig) = start_stack("vs-video-mute").await;
    seed_meeting_with_mh(&stack, "vs-video-mute").await;
    let mut a = joined(&rig, &stack, "vs-video-mute", "user-a", av()).await;
    let mut b = joined(
        &rig,
        &stack,
        "vs-video-mute",
        "user-b",
        receive_only().receiving(vec![video(10)]),
    )
    .await;
    let mut c = joined(
        &rig,
        &stack,
        "vs-video-mute",
        "user-c",
        receive_only().receiving(vec![audio(20)]),
    )
    .await;
    let live = b
        .assignments_until("B shows A live", |v| {
            v.assignments
                .first()
                .is_some_and(|x| x.slot_state == SlotState::Active as i32)
        })
        .await;
    let url = live.assignments[0].media_handler_url.clone();
    assert!(!url.is_empty());
    let _ = c
        .assignments_until("C hears A", |v| {
            v.assignments.first().and_then(|x| x.sender_id) == Some(a.sender_id)
        })
        .await;
    let _ = a.settle().await;
    let pushes = stack.mh_reg_client.calls().len();

    a.write(mute_frame(false, true)).await;
    let muted = b
        .assignments_until("B shows A video-muted", |v| {
            v.assignments
                .first()
                .is_some_and(|x| x.slot_state == SlotState::AvatarClientMuted as i32)
        })
        .await;
    assert_eq!(muted.assignments[0].sender_id, Some(a.sender_id));
    assert_eq!(
        muted.assignments[0].media_handler_url, url,
        "the edge's url is kept"
    );
    use mc_service::media_routing::SourceDispositionPlan::{Forward, Paused};
    let disposition = |c: &test_common::RegisterMeetingCall, stream: u8| {
        c.assignment
            .source_streams
            .iter()
            .find(|s| u32::from(s.sender.get().get()) == a.sender_id && s.stream_number == stream)
            .map(|s| s.disposition)
    };
    // POSITIVE poll for the push that carries the mute.
    let mut seen = pushes;
    let paused = loop {
        let calls = pushes_beyond(&stack, seen).await;
        seen = calls.len();
        let last = calls.last().unwrap().clone();
        if disposition(&last, 1) == Some(Paused) {
            break last;
        }
    };
    assert_eq!(disposition(&paused, 0), Some(Forward), "audio untouched");

    // Fence C: A's AUDIO mute DOES reach C; the first view C reads must be it,
    // which proves the video mute sent C nothing.
    a.write(mute_frame(true, true)).await;
    let first = c.expect_assignments().await;
    assert_eq!(
        first.assignments[0].slot_state,
        SlotState::AvatarClientMuted as i32,
        "the video mute re-emitted nothing to an audio-only holder"
    );

    a.write(mute_frame(true, false)).await;
    let unmuted = b
        .assignments_until("B shows A live again", |v| {
            v.assignments
                .first()
                .is_some_and(|x| x.slot_state == SlotState::Active as i32)
        })
        .await;
    assert_eq!(
        unmuted.assignments[0].sender_id, None,
        "live video names no sender"
    );
}
