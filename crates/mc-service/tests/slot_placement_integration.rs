//! Multi-party static join-order slot placement, end to end through the real
//! join path (story 2 R-1..R-4, R-33; ADR-0036 §6, §8, §9).
//!
//! What these tests add over the pure `media_routing::slots` unit tests and
//! model check: that the meeting actor is wired to them — every structural
//! change (join, leave via each removal path, declaration, mute) re-renders,
//! re-pushes ONLY what changed, and re-emits every affected participant's view
//! — and that placement scopes every handler url a client is given.
//!
//! "No push" assertions read the recording MH client's call list and each
//! handler's generations, never only a metric. No fixed sleep gates any
//! real-time assertion:
//! - POSITIVES poll ([`pushes_caught_up`]) until the last push to a handler
//!   carries the generation the actor last rendered for it, under a generous
//!   deadline.
//! - NEGATIVES are deterministic: after the actor's turn has drained
//!   ([`actor_drained`]), the handler's rendered generation is UNCHANGED. The
//!   actor records a generation before publishing it and the pusher never
//!   re-pushes a confirmed generation (`media_routing::pusher` unit tests),
//!   so an unchanged generation means no push can follow.
//!
//! Paused-time (actor-level) tests use [`virtual_turns`] instead; sibling settle
//! bounds live in `media_client_signaling_integration.rs::settle_no_reply_message`
//! and `common::media_session::Session::settle`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use common::observability::testing::MetricAssertion;
use mc_service::actors::{DisconnectCause, MeetingSeams};
use mc_service::grpc::{MhClient, MhRegistrationClient};
use mc_service::media_routing::{HandlerId, PolicyPushOutcome};
use mc_service::redis::MhAssignmentStore;
use mc_test_utils::mock_mh::{AppliedGenerationBehaviour, MediaHandlerStub};
use proto_gen::dark_tower::signaling::v1::{SlotState, TransportMode};

use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::media_session::{audio_slots, capability_frame, join_as, mute_frame, start_stack};
use test_common::{
    build_test_stack, client_media_config, mh_handler, seed_meeting_with_handlers,
    seed_meeting_with_mh, test_identity_key, test_join_media, RegisterMeetingCall,
    TestStackHandles,
};

/// Deadline for a POSITIVE push poll. A rig bound, never a performance
/// assertion.
const PUSH_DEADLINE: Duration = Duration::from_secs(10);

/// POSITIVE: wait until the last push to `handler_id` carries the generation
/// the actor last rendered for it, and return every generation pushed there.
///
/// Drains the actor's turn FIRST, so "rendered" is the render for everything
/// the caller has already done. Without that, a render still in the mailbox
/// would leave this returning on an earlier match, and a caller asserting over
/// the whole returned list (e.g. strictly-increasing) would read a partial one.
async fn pushes_caught_up(
    stack: &TestStackHandles,
    rig: &AcceptLoopRig,
    meeting_id: &str,
    handler_id: &str,
) -> Vec<u64> {
    actor_drained(stack, meeting_id).await;
    let handler = HandlerId::new(handler_id);
    let endpoint = format!("http://{handler_id}:50053");
    let deadline = tokio::time::Instant::now() + PUSH_DEADLINE;
    loop {
        let rendered = rig
            .policy_generations
            .current(meeting_id, &handler)
            .await
            .map(std::num::NonZeroU64::get);
        let pushed = generations_for(&stack.mh_reg_client.calls(), &endpoint);
        if rendered.is_some() && pushed.last().copied() == rendered {
            return pushed;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{handler_id}: rendered generation {rendered:?} never pushed within \
             {PUSH_DEADLINE:?}; pushed so far {pushed:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Barrier: every message already in the meeting actor's mailbox (and the
/// reconcile it drove) has been processed.
async fn actor_drained(stack: &TestStackHandles, meeting_id: &str) {
    stack
        .controller_handle
        .get_meeting_handle(meeting_id.to_string())
        .await
        .expect("meeting exists")
        .get_state()
        .await
        .expect("actor answers");
}

/// The generation the actor last rendered for `handler_id`.
async fn rendered(rig: &AcceptLoopRig, meeting_id: &str, handler_id: &str) -> Option<u64> {
    rig.policy_generations
        .current(meeting_id, &HandlerId::new(handler_id))
        .await
        .map(std::num::NonZeroU64::get)
}

/// PAUSED-TIME tests only: let the actor and pusher tasks run. Virtual time
/// auto-advances only once every task is idle, so this is a scheduling
/// barrier, not a wall-clock wait.
async fn virtual_turns() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// Every generation pushed to one handler, in order.
fn generations_for(calls: &[RegisterMeetingCall], grpc_endpoint: &str) -> Vec<u64> {
    calls
        .iter()
        .filter(|c| c.mh_grpc_endpoint == grpc_endpoint)
        .map(|c| c.policy_generation)
        .collect()
}

fn senders_in(a: &proto_gen::dark_tower::signaling::v1::StreamAssignments) -> Vec<Option<u32>> {
    a.assignments.iter().map(|s| s.sender_id).collect()
}

// ============================================================================
// R-4: join fills, a middle leave refills only the freed slot, and each change
// re-pushes with a generation that advanced
// ============================================================================

#[tokio::test]
async fn a_join_fills_and_a_middle_leave_refills_only_the_freed_slot() {
    let (stack, rig) = start_stack("slot-r4").await;
    seed_meeting_with_mh(&stack, "slot-r4").await;
    let mut a = join_as(&rig, &stack, "slot-r4", "user-a").await;
    a.write(capability_frame(audio_slots(2))).await;
    let _ = a.settle().await;

    let b = join_as(&rig, &stack, "slot-r4", "user-b").await;
    let first = a
        .assignments_until("B fills A's first slot", |x| {
            senders_in(x)[0] == Some(b.sender_id)
        })
        .await;
    assert_eq!(senders_in(&first), vec![Some(b.sender_id), None]);

    let c = join_as(&rig, &stack, "slot-r4", "user-c").await;
    a.assignments_until("C fills A's second slot", |x| {
        senders_in(x) == vec![Some(b.sender_id), Some(c.sender_id)]
    })
    .await;

    // The N+2th sender: no empty slot, so D is unassigned — and NOT unreachable.
    let d = join_as(&rig, &stack, "slot-r4", "user-d").await;
    a.expect_no_media().await;
    let before = pushes_caught_up(&stack, &rig, "slot-r4", "mh-test-1").await;

    // B — the MIDDLE of A's join order — leaves. Only B's slot changes, refilled
    // by the earliest unassigned sender (D); C's slot is untouched.
    b.close();
    let after_leave = a
        .assignments_until("B's slot refilled", |x| {
            senders_in(x)[0] == Some(d.sender_id)
        })
        .await;
    assert_eq!(
        senders_in(&after_leave),
        vec![Some(d.sender_id), Some(c.sender_id)],
        "only the leaver's slot changes; C keeps its slot and its sender_id"
    );
    assert!(after_leave.unreachable_sender_ids.is_empty());

    let after = pushes_caught_up(&stack, &rig, "slot-r4", "mh-test-1").await;
    assert!(after.len() > before.len(), "the leave re-pushed");
    assert!(
        after.last() > before.last(),
        "a changed snapshot carries a higher generation: before {before:?}, after {after:?}"
    );
    // Every push carries a non-decreasing generation, and consecutive pushes
    // differ — an unchanged snapshot is never re-sent.
    assert!(
        after.windows(2).all(|w| w[0] < w[1]),
        "generations strictly increase: {after:?}"
    );
}

/// The unfilled-slot-only change is not on the MH wire: a solo participant
/// declaring slots changes no snapshot, so nothing is pushed.
#[tokio::test]
async fn an_unfilled_slot_only_change_is_not_pushed() {
    let (stack, rig) = start_stack("slot-unfilled").await;
    seed_meeting_with_mh(&stack, "slot-unfilled").await;
    let mut a = join_as(&rig, &stack, "slot-unfilled", "user-a").await;
    assert_eq!(
        pushes_caught_up(&stack, &rig, "slot-unfilled", "mh-test-1").await,
        vec![1],
        "the first join registers the handler once"
    );
    let before = stack.mh_reg_client.calls();
    assert_eq!(before[0].egress_stream_count, 0, "an empty snapshot");

    a.write(capability_frame(audio_slots(3))).await;
    let _ = a.settle().await;
    actor_drained(&stack, "slot-unfilled").await;
    assert_eq!(
        rendered(&rig, "slot-unfilled", "mh-test-1").await,
        Some(1),
        "declaring slots nobody can fill changes no forwarding edge: no new render"
    );
    assert_eq!(stack.mh_reg_client.calls().len(), 1);
}

// ============================================================================
// R-33: a cross-handler join or leave re-emits the unreachable set with NO push
// and NO generation bump on EITHER handler
// ============================================================================

#[tokio::test]
async fn a_cross_handler_join_and_leave_re_emit_unreachable_without_any_push() {
    let (stack, rig) = start_stack("slot-cross").await;
    seed_meeting_with_handlers(
        &stack,
        "slot-cross",
        vec![mh_handler("mh-1"), mh_handler("mh-0")],
    )
    .await;
    let mut a = join_as(&rig, &stack, "slot-cross", "user-a").await; // rank 0 -> mh-0
    assert_eq!(a.media_servers, vec!["wt://mh-0:4433".to_string()]);
    a.write(capability_frame(audio_slots(1))).await;
    let _ = a.settle().await;
    // The first join registers BOTH handlers (the mh-1 one empty), so a
    // participant later placed on mh-1 can be promoted by MH.
    assert_eq!(
        pushes_caught_up(&stack, &rig, "slot-cross", "mh-0").await,
        vec![1]
    );
    assert_eq!(
        pushes_caught_up(&stack, &rig, "slot-cross", "mh-1").await,
        vec![1]
    );

    let snap = MetricAssertion::snapshot();
    let b = join_as(&rig, &stack, "slot-cross", "user-b").await; // rank 1 -> mh-1
    assert_eq!(b.media_servers, vec!["wt://mh-1:4433".to_string()]);
    let with_b = a
        .assignments_until("B named unreachable", |x| {
            !x.unreachable_sender_ids.is_empty()
        })
        .await;
    assert_eq!(with_b.unreachable_sender_ids, vec![b.sender_id]);
    assert_eq!(
        with_b.assignments[0].slot_state,
        SlotState::FewerSourcesThanSlots as i32,
        "a cross-handler peer consumes no slot"
    );

    snap.counter("mc_media_unreachable_senders_total")
        .with_labels(&[("key_custody", "operator")])
        .assert_delta(1);

    b.close();
    let without_b = a
        .assignments_until("B no longer unreachable", |x| {
            x.unreachable_sender_ids.is_empty()
        })
        .await;
    assert!(without_b.unreachable_sender_ids.is_empty());

    actor_drained(&stack, "slot-cross").await;
    for handler in ["mh-0", "mh-1"] {
        assert_eq!(
            rendered(&rig, "slot-cross", handler).await,
            Some(1),
            "{handler}: a cross-handler join and leave render nothing new"
        );
    }
    let calls_after = stack.mh_reg_client.calls();
    assert_eq!(
        generations_for(&calls_after, "http://mh-0:50053"),
        vec![1],
        "no forwarding edge changed on mh-0: no push, no bump"
    );
    assert_eq!(
        generations_for(&calls_after, "http://mh-1:50053"),
        vec![1],
        "the empty handler stays at its first-join generation and is never re-pushed"
    );
}

// ============================================================================
// Placement pins (test-seams) and S10b in-process
// ============================================================================

async fn seamed_meeting(
    stack: &TestStackHandles,
    meeting_id: &str,
    handlers: Vec<mc_service::redis::MhEndpointInfo>,
    seams: MeetingSeams,
) {
    stack
        .controller_handle
        .create_meeting_with_seams(meeting_id.to_string(), seams)
        .await
        .expect("create meeting with seams");
    stack.mh_store.insert(
        meeting_id,
        mc_service::redis::MhAssignmentData {
            handlers,
            assigned_at: "2026-09-24T00:00:00Z".to_string(),
        },
    );
}

/// S10b in-process: A, C and E on mh-0; B and D on mh-1. Each handler's
/// snapshot carries only its own participants' edges, and each subscriber's
/// unreachable set is EXACTLY the other handler's participants.
#[tokio::test]
async fn a_split_meeting_programs_each_handler_with_only_its_own_edges() {
    let (stack, rig) = start_stack("slot-split").await;
    seed_meeting_with_handlers(
        &stack,
        "slot-split",
        vec![mh_handler("mh-0"), mh_handler("mh-1")],
    )
    .await;
    let mut sessions = Vec::new();
    for user in ["user-a", "user-b", "user-c", "user-d", "user-e"] {
        let mut s = join_as(&rig, &stack, "slot-split", user).await;
        s.write(capability_frame(audio_slots(4))).await;
        sessions.push(s);
    }
    let on_mh0: Vec<u32> = [0, 2, 4].iter().map(|i| sessions[*i].sender_id).collect();
    let on_mh1: Vec<u32> = [1, 3].iter().map(|i| sessions[*i].sender_id).collect();

    for (i, s) in sessions.iter_mut().enumerate() {
        let (_, view) = s.settle().await;
        let view = view.expect("every declared participant has a view");
        let (mine, other) = if i % 2 == 0 {
            (&on_mh0, &on_mh1)
        } else {
            (&on_mh1, &on_mh0)
        };
        let mut unreachable = view.unreachable_sender_ids.clone();
        unreachable.sort_unstable();
        let mut expected = other.clone();
        expected.sort_unstable();
        assert_eq!(
            unreachable, expected,
            "participant {i}: unreachable == the other handler, exactly"
        );
        for slot in view.assignments.iter().filter_map(|a| a.sender_id) {
            assert!(
                mine.contains(&slot),
                "participant {i}: a slot holds only a co-handler sender"
            );
            assert_ne!(slot, s.sender_id, "never itself");
        }
    }

    pushes_caught_up(&stack, &rig, "slot-split", "mh-0").await;
    pushes_caught_up(&stack, &rig, "slot-split", "mh-1").await;
    let calls = stack.mh_reg_client.calls();
    for (endpoint, members) in [
        ("http://mh-0:50053", &on_mh0),
        ("http://mh-1:50053", &on_mh1),
    ] {
        let last = calls
            .iter()
            .rev()
            .find(|c| c.mh_grpc_endpoint == endpoint)
            .expect("every handler is programmed");
        for plan in &last.assignment.egress_streams {
            let subscriber = u32::from(plan.subscriber.get().get());
            let source = u32::from(plan.candidate_sources[0].get().get());
            assert!(
                members.contains(&subscriber) && members.contains(&source),
                "{endpoint} carries a foreign edge"
            );
        }
        // Full mesh within the handler: every member holds every other member.
        assert_eq!(
            last.assignment.egress_streams.len(),
            members.len() * (members.len() - 1)
        );
    }
}

#[tokio::test]
async fn a_placement_pin_co_locates_two_participants_and_a_pin_outside_the_set_fails_the_join() {
    let (stack, rig) = start_stack("slot-pin").await;
    let mut pins = BTreeMap::new();
    pins.insert("user-b".to_string(), HandlerId::new("mh-0"));
    seamed_meeting(
        &stack,
        "slot-pin",
        vec![mh_handler("mh-0"), mh_handler("mh-1")],
        MeetingSeams {
            placement_pins: pins,
            ..Default::default()
        },
    )
    .await;
    let mut a = join_as(&rig, &stack, "slot-pin", "user-a").await;
    let b = join_as(&rig, &stack, "slot-pin", "user-b").await;
    assert_eq!(
        b.media_servers, a.media_servers,
        "pinned onto A's handler, not round-robin"
    );
    a.write(capability_frame(audio_slots(1))).await;
    let (_, view) = a.settle().await;
    assert_eq!(view.unwrap().assignments[0].sender_id, Some(b.sender_id));

    // A pin naming a handler outside the meeting's set fails LOUDLY — no
    // fallback to round-robin or to handler[0].
    let mut bad = BTreeMap::new();
    bad.insert("user-x".to_string(), HandlerId::new("mh-9"));
    seamed_meeting(
        &stack,
        "slot-pin-bad",
        vec![mh_handler("mh-0")],
        MeetingSeams {
            placement_pins: bad,
            ..Default::default()
        },
    )
    .await;
    let rejected =
        test_common::media_session::try_join_as(&rig, &stack, "slot-pin-bad", "user-x").await;
    assert!(rejected.is_err(), "an out-of-set pin must fail the join");
}

// ============================================================================
// The per-meeting flush bound: deferred, never dropped, and per meeting
// ============================================================================

#[tokio::test]
async fn the_flush_bound_defers_but_every_subscriber_converges() {
    let (stack, rig) = start_stack("slot-bound").await;
    seamed_meeting(
        &stack,
        "slot-bound",
        vec![mh_handler("mh-test-1")],
        MeetingSeams {
            slot_view_flush_batch: Some(1),
            ..Default::default()
        },
    )
    .await;
    // A second, unbounded meeting on the same MC: its budget is its own.
    seed_meeting_with_mh(&stack, "slot-unbound").await;

    let mut bounded = Vec::new();
    for user in ["user-a", "user-b", "user-c", "user-d"] {
        let mut s = join_as(&rig, &stack, "slot-bound", user).await;
        // Four slots: three peers fill three, leaving one empty for the joiner.
        s.write(capability_frame(audio_slots(4))).await;
        bounded.push(s);
    }
    let mut other_a = join_as(&rig, &stack, "slot-unbound", "user-a").await;
    let other_b = join_as(&rig, &stack, "slot-unbound", "user-b").await;
    other_a.write(capability_frame(audio_slots(1))).await;

    let snap = MetricAssertion::snapshot();
    // One more join dirties every declared participant at once; with a bound
    // of 1 per turn, all but one are deferred to later turns.
    let e = join_as(&rig, &stack, "slot-bound", "user-e").await;
    for s in &mut bounded {
        let (_, view) = s.settle().await;
        let view = view.expect("converged view");
        assert!(
            view.assignments
                .iter()
                .any(|a| a.sender_id == Some(e.sender_id)),
            "every subscriber converges to a view holding the joiner — deferral never drops"
        );
    }
    let (_, other_view) = other_a.settle().await;
    assert_eq!(
        other_view.unwrap().assignments[0].sender_id,
        Some(other_b.sender_id)
    );

    assert!(
        snap.counter("mc_media_slot_view_emissions_total")
            .with_labels(&[("outcome", "deferred"), ("key_custody", "operator")])
            .delta()
            >= 1,
        "the bound engaged and is visible"
    );
    snap.counter("mc_media_slot_view_emissions_total")
        .with_labels(&[("outcome", "delivery_failed"), ("key_custody", "operator")])
        .assert_delta(0);
}

/// Rapid DISTINCT redeclarations — each a real structural change — never drive
/// more than one push per change per handler, and never starve another
/// participant's flush. (The worker's coalescing under a slow handler is proven
/// deterministically in `media_routing::pusher`'s unit tests.)
#[tokio::test]
async fn rapid_distinct_redeclarations_are_bounded_and_do_not_starve_peers() {
    const FLIPS: u32 = 20;
    let (stack, rig) = start_stack("slot-flip").await;
    seed_meeting_with_mh(&stack, "slot-flip").await;
    let mut a = join_as(&rig, &stack, "slot-flip", "user-a").await;
    let mut b = join_as(&rig, &stack, "slot-flip", "user-b").await;
    let mut c = join_as(&rig, &stack, "slot-flip", "user-c").await;
    b.write(capability_frame(audio_slots(1))).await;
    let _ = b.settle().await;
    pushes_caught_up(&stack, &rig, "slot-flip", "mh-test-1").await;
    let before = stack.mh_reg_client.calls().len();

    for i in 0..FLIPS {
        a.write(capability_frame(audio_slots(1 + i % 2))).await;
        if i == FLIPS / 2 {
            // A peer declares in the middle of A's churn.
            c.write(capability_frame(audio_slots(1))).await;
        }
    }
    let (_, a_view) = a.settle().await;
    let a_view = a_view.expect("A converged");
    assert_eq!(
        a_view.assignments.len(),
        2,
        "the last declaration (2 slots) is in force"
    );
    assert_eq!(
        senders_in(&a_view),
        vec![Some(b.sender_id), Some(c.sender_id)]
    );

    pushes_caught_up(&stack, &rig, "slot-flip", "mh-test-1").await;
    let calls = stack.mh_reg_client.calls();
    let pushes = calls.len() - before;
    assert!(
        pushes <= FLIPS as usize,
        "{pushes} pushes for {FLIPS} declarations: never more than one per change"
    );
    // Positive control: convergence reached MH. The LAST push reflects the
    // final declaration — A holds B and C (two streams into A).
    let a_streams = calls
        .last()
        .unwrap()
        .assignment
        .egress_streams
        .iter()
        .filter(|p| u32::from(p.subscriber.get().get()) == a.sender_id)
        .count();
    assert_eq!(
        a_streams, 2,
        "MH's last snapshot holds A's final two-slot view"
    );

    // Peers are not starved: C, declaring mid-churn, is served its view (its
    // one slot holds A, the earliest co-handler sender) ...
    let (_, c_view) = c.settle().await;
    assert_eq!(
        senders_in(&c_view.expect("C's view arrived despite A's churn")),
        vec![Some(a.sender_id)]
    );
    // ... and a mute by B still reaches A, who holds B.
    b.write(mute_frame(true, false)).await;
    a.assignments_until("B shown muted in A's view", |x| {
        x.assignments.iter().any(|s| {
            s.sender_id == Some(b.sender_id) && s.slot_state == SlotState::SourceMuted as i32
        })
    })
    .await;
}

// ============================================================================
// Every removal path drives the same re-push: grace expiry and reconnect
// ============================================================================

/// Actor-level, on paused time: a participant lost without a clean close is
/// removed only at grace expiry, and THAT removal re-pushes and refills.
#[tokio::test(start_paused = true)]
async fn grace_expiry_drives_the_same_re_push_as_a_clean_leave() {
    let stack = build_test_stack("slot-grace").await;
    let handlers = vec![mh_handler("mh-test-1")];
    seed_meeting_with_handlers(&stack, "slot-grace", handlers.clone()).await;
    let meeting = stack
        .controller_handle
        .get_meeting_handle("slot-grace".to_string())
        .await
        .unwrap();

    let mut rx = Vec::new();
    for p in ["a", "b", "c"] {
        let (tx, r) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);
        rx.push(r);
        meeting
            .connection_join(
                format!("conn-{p}"),
                format!("user-{p}"),
                format!("part-{p}"),
                String::new(),
                false,
                test_identity_key(),
                Some(tx),
                test_join_media(&stack, &handlers),
            )
            .await
            .unwrap();
    }
    for p in ["a", "b", "c"] {
        let decl = mc_service::media_signaling::ReceiveCapabilityDeclaration::parse(
            &proto_gen::dark_tower::signaling::v1::ReceiveCapability {
                slots: audio_slots(1),
            },
            8,
            true,
        )
        .unwrap();
        meeting
            .register_receive_capability(format!("part-{p}"), decl)
            .await
            .unwrap();
    }
    virtual_turns().await;
    let before = stack.mh_reg_client.calls();

    meeting
        .connection_disconnected(
            "conn-b".to_string(),
            "part-b".to_string(),
            DisconnectCause::ConnectionLost,
        )
        .await
        .unwrap();
    virtual_turns().await;
    assert_eq!(
        stack.mh_reg_client.calls().len(),
        before.len(),
        "a participant in grace is still on the roster: nothing structural changed"
    );

    // Past the 30 s grace period (paused time auto-advances).
    tokio::time::sleep(Duration::from_secs(40)).await;
    let after = stack.mh_reg_client.calls();
    assert!(after.len() > before.len(), "grace expiry re-pushed");
    let last = after.last().unwrap();
    assert_eq!(
        last.egress_stream_count, 2,
        "A and C now hold each other; B's edges are gone"
    );
}

/// T8: a reconnect within grace keeps rank and slots — peers joining during
/// the grace window do not take the reconnecting participant's place — and the
/// new connection is re-sent its full current view, with nothing re-pushed.
///
/// **Stated gap**: this tier proves EXACTLY ONE view was sent to the
/// reconnector (the `sent` delta) and nothing re-pushed, but NOT that the view
/// was the full CURRENT one: `connection_reconnect` takes no outbound channel
/// here, so the re-sent bytes are not observable at this tier. The content
/// half rests on `flush_one` composing from the live table (the same path
/// every other view in this file asserts content through).
#[tokio::test(start_paused = true)]
async fn a_reconnect_in_grace_keeps_slots_and_re_sends_only_the_reconnectors_view() {
    let stack = build_test_stack("slot-reconnect").await;
    let handlers = vec![mh_handler("mh-test-1")];
    seed_meeting_with_handlers(&stack, "slot-reconnect", handlers.clone()).await;
    let meeting = stack
        .controller_handle
        .get_meeting_handle("slot-reconnect".to_string())
        .await
        .unwrap();
    let decl = || {
        mc_service::media_signaling::ReceiveCapabilityDeclaration::parse(
            &proto_gen::dark_tower::signaling::v1::ReceiveCapability {
                slots: audio_slots(1),
            },
            8,
            true,
        )
        .unwrap()
    };

    let mut joins = Vec::new();
    let mut rx = Vec::new();
    for p in ["a", "b"] {
        let (tx, r) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);
        rx.push(r);
        joins.push(
            meeting
                .connection_join(
                    format!("conn-{p}"),
                    format!("user-{p}"),
                    format!("part-{p}"),
                    String::new(),
                    false,
                    test_identity_key(),
                    Some(tx),
                    test_join_media(&stack, &handlers),
                )
                .await
                .unwrap(),
        );
        meeting
            .register_receive_capability(format!("part-{p}"), decl())
            .await
            .unwrap();
    }
    virtual_turns().await;

    // B drops without a clean close: grace, not removal.
    meeting
        .connection_disconnected(
            "conn-b".to_string(),
            "part-b".to_string(),
            DisconnectCause::ConnectionLost,
        )
        .await
        .unwrap();
    // C joins during B's grace. A's one slot still holds B, so C is unassigned
    // for A: B never left the roster.
    let (tx_c, _rx_c) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);
    meeting
        .connection_join(
            "conn-c".to_string(),
            "user-c".to_string(),
            "part-c".to_string(),
            String::new(),
            false,
            test_identity_key(),
            Some(tx_c),
            test_join_media(&stack, &handlers),
        )
        .await
        .unwrap();
    virtual_turns().await;
    let pushes_before = stack.mh_reg_client.calls().len();
    let a_holds = stack
        .mh_reg_client
        .calls()
        .last()
        .unwrap()
        .assignment
        .egress_streams
        .iter()
        .find(|p| p.subscriber.get().get() == joins[0].sender_id.get().get())
        .map(|p| p.candidate_sources[0])
        .expect("A holds someone");
    assert_eq!(
        a_holds, joins[1].sender_id,
        "A's slot kept B through B's grace"
    );

    let snap = MetricAssertion::snapshot();
    meeting
        .connection_reconnect(
            "conn-b2".to_string(),
            joins[1].correlation_id.clone(),
            joins[1].binding_token.clone(),
        )
        .await
        .unwrap();
    virtual_turns().await;

    snap.counter("mc_media_slot_view_emissions_total")
        .with_labels(&[("outcome", "sent"), ("key_custody", "operator")])
        .assert_delta(1);
    assert_eq!(
        stack.mh_reg_client.calls().len(),
        pushes_before,
        "a reconnect is not a structural change: nothing is re-pushed"
    );
}

/// S-C: the handler set frozen at the first join is the meeting's authority. A
/// later join carrying a DIFFERENT set (Redis changed, or was tampered with)
/// is counted and logged, and cannot widen the set or move anyone: the joiner
/// is placed on the frozen set.
#[tokio::test]
async fn a_later_join_cannot_widen_the_frozen_handler_set() {
    let stack = build_test_stack("slot-frozen").await;
    let frozen = vec![mh_handler("mh-0")];
    seed_meeting_with_handlers(&stack, "slot-frozen", frozen.clone()).await;
    let meeting = stack
        .controller_handle
        .get_meeting_handle("slot-frozen".to_string())
        .await
        .unwrap();

    let mut results = Vec::new();
    for (p, handlers) in [
        ("a", frozen.clone()),
        ("b", vec![mh_handler("mh-evil"), mh_handler("mh-0")]),
    ] {
        let snap = MetricAssertion::snapshot();
        let (tx, _rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);
        let joined = meeting
            .connection_join(
                format!("conn-{p}"),
                format!("user-{p}"),
                format!("part-{p}"),
                String::new(),
                false,
                test_identity_key(),
                Some(tx),
                test_join_media(&stack, &handlers),
            )
            .await
            .unwrap();
        let divergences = snap
            .counter("mc_media_handler_set_divergence_total")
            .with_labels(&[("key_custody", "operator")])
            .delta();
        results.push((joined.media_handler.id, divergences));
    }
    assert_eq!(
        results[0],
        (HandlerId::new("mh-0"), 0),
        "the first join freezes the set"
    );
    assert_eq!(
        results[1],
        (HandlerId::new("mh-0"), 1),
        "a divergent later join is counted, and rank 1 round-robins over the FROZEN \
         one-handler set, never onto the injected handler"
    );
}

// ============================================================================
// Re-push confirm: a mismatch on a RE-push fails loudly, like the first
// ============================================================================

#[tokio::test]
async fn an_applied_echo_mismatch_on_a_re_push_is_loud() {
    // A real MhClient against a stub that applies generation 1 and stalls on
    // every later one: the SECOND push is the one that diverges.
    let stub = MediaHandlerStub::builder()
        .accept(true)
        .applied_generation(AppliedGenerationBehaviour::EchoUpTo(1))
        .handler_id("mh-stub")
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;
    let stack = build_test_stack("slot-repush").await;
    let rig = AcceptLoopRig::start_with_media_config(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
        Arc::clone(&stack.mh_store) as Arc<dyn MhAssignmentStore>,
        Arc::new(MhClient::new(mc_test_utils::test_token_receiver()))
            as Arc<dyn MhRegistrationClient>,
        "mc-test".to_string(),
        "http://mc-test:50052".to_string(),
        32,
        client_media_config(),
    )
    .await;
    seed_meeting_with_handlers(
        &stack,
        "slot-repush",
        vec![mc_service::redis::MhEndpointInfo {
            mh_id: "mh-stub".to_string(),
            webtransport_endpoint: "wt://mh-stub:4433".to_string(),
            grpc_endpoint: stub.endpoint(),
        }],
    )
    .await;
    let snap = MetricAssertion::snapshot();

    let mut a = join_as(&rig, &stack, "slot-repush", "user-a").await;
    let _b = join_as(&rig, &stack, "slot-repush", "user-b").await;
    // Generation 1 (empty) is applied. A's declaration fills a slot -> gen 2,
    // which the stub never applies.
    a.write(capability_frame(audio_slots(1))).await;
    let _ = a.settle().await;
    // POSITIVE poll: the worker's 1 s + 2 s backoff retries gen 2 and the
    // mismatch is counted. Generous deadline; returns as soon as both hold.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mismatches = || {
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[
                ("outcome", PolicyPushOutcome::GenerationMismatch.label()),
                ("key_custody", "operator"),
            ])
            .delta()
    };
    while !(stub
        .received_generations()
        .iter()
        .filter(|g| **g == 2)
        .count()
        >= 2
        && mismatches() >= 1)
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "gen 2 was not retried and counted within 30 s: {:?}",
            stub.received_generations()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let received = stub.received_generations();
    assert_eq!(received.first(), Some(&1));
    assert!(
        received.iter().filter(|g| **g == 2).count() >= 2,
        "gen 2 retried: {received:?}"
    );
    assert!(
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[
                ("outcome", PolicyPushOutcome::GenerationMismatch.label()),
                ("key_custody", "operator"),
            ])
            .delta()
            >= 1,
        "a divergent RE-push is counted, not silent"
    );
}

// ============================================================================
// The configured cap is the published cap
// ============================================================================

#[tokio::test]
async fn the_published_slot_cap_is_the_enforced_cap() {
    let snap = MetricAssertion::snapshot();
    let (_stack, _rig) = start_stack("slot-cap-gauge").await;
    #[expect(clippy::cast_precision_loss, reason = "cap is bounded to 1..=64")]
    let expected = client_media_config().max_receive_slots as f64;
    snap.gauge("mc_media_receive_slot_cap")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(expected);
}
