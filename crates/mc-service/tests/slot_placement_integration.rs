//! Multi-party join-order slots and shared-handler edges, end to end through the
//! real join path and the real MH→MC connectivity path (story 2 R-1..R-4, R-33;
//! ADR-0036 §6, §8, §9).
//!
//! What these tests add over the pure `media_routing::{slots, connectivity}`
//! unit tests and model check: that the meeting actor is wired to them — every
//! structural change (join, leave via each removal path, declaration, mute, and
//! a SETTLED connectivity change reported by a handler) re-renders, re-pushes
//! ONLY what changed, and re-emits every affected participant's view — and that
//! every client is offered the FULL handler set.
//!
//! Connectivity is always produced the way production produces it: a
//! `NotifyParticipantConnected` / `Disconnected` through the real
//! `McMediaCoordinationService` (`common::media_session::notify_*`). There is
//! no placement seam. Every assertion is a PROPERTY — an edge is on a handler
//! both parties are connected to, a sender's targets are the handlers owning
//! its edges — never WHICH handler MC chose.
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

use std::collections::{BTreeMap, BTreeSet};
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
use test_common::media_session::{
    audio_slots, capability_frame, connection_id_for, join_as, join_as_on, mute_frame,
    notify_connected, notify_disconnected, start_stack,
};
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

/// The LAST snapshot pushed to each handler endpoint, as `(subscriber, source)`
/// edges.
fn latest_edges(calls: &[RegisterMeetingCall]) -> BTreeMap<String, BTreeSet<(u32, u32)>> {
    let mut out: BTreeMap<String, BTreeSet<(u32, u32)>> = BTreeMap::new();
    for call in calls {
        let edges = call
            .assignment
            .egress_streams
            .iter()
            .filter_map(|p| {
                p.candidate_sources.first().map(|source| {
                    (
                        u32::from(p.subscriber.get().get()),
                        u32::from(source.get().get()),
                    )
                })
            })
            .collect();
        out.insert(call.mh_grpc_endpoint.clone(), edges);
    }
    out
}

/// A sender's targets as MH sees them: the handler endpoints whose latest
/// snapshot carries at least one edge FROM it.
fn targets_at_mh(calls: &[RegisterMeetingCall], sender: u32) -> BTreeSet<String> {
    latest_edges(calls)
        .into_iter()
        .filter(|(_, edges)| edges.iter().any(|(_, source)| *source == sender))
        .map(|(endpoint, _)| endpoint)
        .collect()
}

/// A subscriber's sources across every handler.
fn heard_by(calls: &[RegisterMeetingCall], subscriber: u32) -> BTreeSet<u32> {
    latest_edges(calls)
        .values()
        .flatten()
        .filter(|(sub, _)| *sub == subscriber)
        .map(|(_, source)| *source)
        .collect()
}

/// The handler endpoint carrying the `subscriber <- source` edge, if any.
fn edge_owner(calls: &[RegisterMeetingCall], subscriber: u32, source: u32) -> Option<String> {
    latest_edges(calls)
        .into_iter()
        .find(|(_, edges)| edges.contains(&(subscriber, source)))
        .map(|(endpoint, _)| endpoint)
}

fn endpoint_of(handler_id: &str) -> String {
    format!("http://{handler_id}:50053")
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
// R-33: one general rule — hear iff a connected handler is shared
// ============================================================================

/// THE CANONICAL CASE. A is connected to both handlers, B only to mh-0, C only
/// to mh-1. A hears B and C; B hears only A; C hears only A; B and C are each
/// unreachable to the other; A sends to both handlers. Every client was offered
/// BOTH handlers — the split is produced only by which ones they reached.
#[tokio::test]
async fn canonical_partial_connectivity_routes_only_through_shared_handlers() {
    let snap = MetricAssertion::snapshot();
    let (stack, rig) = start_stack("slot-canon").await;
    // Redis enumerates mh-1 first: order must not matter anywhere.
    seed_meeting_with_handlers(
        &stack,
        "slot-canon",
        vec![mh_handler("mh-1"), mh_handler("mh-0")],
    )
    .await;
    let mut a = join_as_on(&rig, &stack, "slot-canon", "user-a", &["mh-0", "mh-1"]).await;
    let mut b = join_as_on(&rig, &stack, "slot-canon", "user-b", &["mh-0"]).await;
    let mut c = join_as_on(&rig, &stack, "slot-canon", "user-c", &["mh-1"]).await;
    for s in [&a, &b, &c] {
        let mut offered = s.media_servers.clone();
        offered.sort();
        assert_eq!(
            offered,
            vec!["wt://mh-0:4433".to_string(), "wt://mh-1:4433".to_string()],
            "every participant is offered the FULL registered set"
        );
    }
    for s in [&mut a, &mut b, &mut c] {
        s.write(capability_frame(audio_slots(2))).await;
    }

    // B and C settle at the window's end (they never reach every handler);
    // poll the views rather than sleeping.
    // Directive before assignments within one flush, so read A's converged
    // directive first, then the assignments that follow it.
    let a_directive = a
        .directive_until("A targets both handlers", |d| {
            d.streams.first().is_some_and(|s| s.targets.len() == 2)
        })
        .await;
    let a_view = a
        .assignments_until("A hears B and C", |x| {
            let mut heard: Vec<u32> = x.assignments.iter().filter_map(|s| s.sender_id).collect();
            heard.sort_unstable();
            let mut want = vec![b.sender_id, c.sender_id];
            want.sort_unstable();
            heard == want
        })
        .await;
    assert!(a_view.unreachable_sender_ids.is_empty());
    let b_view = b
        .assignments_until("B hears only A; C unreachable", |x| {
            senders_in(x) == vec![Some(a.sender_id), None]
                && x.unreachable_sender_ids == vec![c.sender_id]
        })
        .await;
    let c_view = c
        .assignments_until("C hears only A; B unreachable", |x| {
            senders_in(x) == vec![Some(a.sender_id), None]
                && x.unreachable_sender_ids == vec![b.sender_id]
        })
        .await;

    // Each slot is read on the handler that owns that edge — one the pair
    // shares, which for B's and C's single handler is forced.
    assert_eq!(b_view.assignments[0].media_handler_url, "wt://mh-0:4433");
    assert_eq!(c_view.assignments[0].media_handler_url, "wt://mh-1:4433");
    for slot in &a_view.assignments {
        let from = slot.sender_id.unwrap();
        let expected = if from == b.sender_id {
            "wt://mh-0:4433"
        } else {
            "wt://mh-1:4433"
        };
        assert_eq!(
            slot.media_handler_url, expected,
            "A reads each slot where its edge is"
        );
    }

    // A sends to both handlers.
    let mut a_targets: Vec<String> = a_directive.streams[0]
        .targets
        .iter()
        .map(|t| t.media_handler_url.clone())
        .collect();
    a_targets.sort();
    assert_eq!(a_targets, vec!["wt://mh-0:4433", "wt://mh-1:4433"]);

    // Telemetry: B and C were named unreachable to each other on delivered
    // views; while B and C were still settling, their peers' views counted
    // them as not yet connected; A's two-target directive counted two targets.
    assert!(
        snap.counter("mc_media_unreachable_senders_total")
            .with_labels(&[("key_custody", "operator")])
            .delta()
            >= 2
    );
    assert!(
        snap.counter("mc_media_not_yet_connected_senders_total")
            .with_labels(&[("key_custody", "operator")])
            .delta()
            >= 1,
        "B and C were counted while establishing"
    );
    assert!(
        snap.counter("mc_media_send_targets_total")
            .with_labels(&[("key_custody", "operator")])
            .delta()
            >= 2
    );

    // MH side: each handler holds exactly the edges it owns.
    pushes_caught_up(&stack, &rig, "slot-canon", "mh-0").await;
    pushes_caught_up(&stack, &rig, "slot-canon", "mh-1").await;
    let calls = stack.mh_reg_client.calls();
    assert_eq!(
        latest_edges(&calls)[&endpoint_of("mh-0")],
        BTreeSet::from([(a.sender_id, b.sender_id), (b.sender_id, a.sender_id)])
    );
    assert_eq!(
        latest_edges(&calls)[&endpoint_of("mh-1")],
        BTreeSet::from([(a.sender_id, c.sender_id), (c.sender_id, a.sender_id)])
    );
}

/// All-connected: everyone hears everyone, and each sender has exactly ONE
/// target — co-location, which is what the general rule produces, not a
/// separate path. Asserted without naming the handler.
#[tokio::test]
async fn all_connected_everyone_hears_everyone_with_one_target_per_sender() {
    let (stack, rig) = start_stack("slot-allconn").await;
    seed_meeting_with_handlers(
        &stack,
        "slot-allconn",
        vec![mh_handler("mh-0"), mh_handler("mh-1")],
    )
    .await;
    let mut sessions = Vec::new();
    for user in ["user-a", "user-b", "user-c", "user-d"] {
        let mut s = join_as(&rig, &stack, "slot-allconn", user).await;
        assert_eq!(s.media_servers.len(), 2, "the full set is offered");
        s.write(capability_frame(audio_slots(3))).await;
        sessions.push(s);
    }
    let ids: Vec<u32> = sessions.iter().map(|s| s.sender_id).collect();
    for s in &mut sessions {
        let (directive, view) = s.settle().await;
        let view = view.expect("view");
        let heard: BTreeSet<u32> = view
            .assignments
            .iter()
            .filter_map(|x| x.sender_id)
            .collect();
        let others: BTreeSet<u32> = ids.iter().copied().filter(|x| *x != s.sender_id).collect();
        assert_eq!(heard, others, "everyone hears everyone");
        assert!(view.unreachable_sender_ids.is_empty());
        assert_eq!(
            directive.expect("directive").streams[0].targets.len(),
            1,
            "one send target per sender"
        );
    }
    pushes_caught_up(&stack, &rig, "slot-allconn", "mh-0").await;
    pushes_caught_up(&stack, &rig, "slot-allconn", "mh-1").await;
    let calls = stack.mh_reg_client.calls();
    for id in &ids {
        assert_eq!(
            targets_at_mh(&calls, *id).len(),
            1,
            "MH agrees: one handler per sender"
        );
    }
}

/// A connectivity change is a structural change with the SAME machinery as a
/// join or leave: generations advance only when a snapshot changed.
///
/// - A disconnect from the handler NOT carrying the pair's edges changes no
///   snapshot: no render advance, no push (negative, deterministic).
/// - A disconnect from the handler that DOES carry them moves the edges to the
///   other shared handler: both handlers' snapshots change and both advance.
/// - A duplicate Connected (MH retry) changes nothing.
#[tokio::test]
async fn a_connectivity_change_re_pushes_with_generation_advance_only_on_change() {
    let (stack, rig) = start_stack("slot-conn").await;
    seed_meeting_with_handlers(
        &stack,
        "slot-conn",
        vec![mh_handler("mh-0"), mh_handler("mh-1")],
    )
    .await;
    let mut a = join_as(&rig, &stack, "slot-conn", "user-a").await;
    let mut b = join_as(&rig, &stack, "slot-conn", "user-b").await;
    a.write(capability_frame(audio_slots(1))).await;
    b.write(capability_frame(audio_slots(1))).await;
    let _ = a.settle().await;
    let _ = b.settle().await;
    pushes_caught_up(&stack, &rig, "slot-conn", "mh-0").await;
    pushes_caught_up(&stack, &rig, "slot-conn", "mh-1").await;
    let calls = stack.mh_reg_client.calls();
    let owner = edge_owner(&calls, a.sender_id, b.sender_id).expect("A holds B");
    let (carrying, idle) = if owner == endpoint_of("mh-0") {
        ("mh-0", "mh-1")
    } else {
        ("mh-1", "mh-0")
    };
    let gen = |h: &'static str| rendered(&rig, "slot-conn", h);
    let before = (gen(carrying).await, gen(idle).await);

    // Duplicate Connected for a live key: idempotent.
    b.connect_to(&stack, carrying, &connection_id_for("user-b", carrying))
        .await;
    actor_drained(&stack, "slot-conn").await;
    assert_eq!(
        (gen(carrying).await, gen(idle).await),
        before,
        "a retry changes nothing"
    );

    // B leaves the idle handler: no edge was there.
    b.disconnect_from(&stack, idle, &connection_id_for("user-b", idle))
        .await;
    actor_drained(&stack, "slot-conn").await;
    assert_eq!(
        (gen(carrying).await, gen(idle).await),
        before,
        "no snapshot changed, so no generation advanced"
    );

    // B leaves the carrying handler: the pair still shares nothing else? B is
    // on no handler now -> not connected, no edges, not unreachable.
    b.disconnect_from(&stack, carrying, &connection_id_for("user-b", carrying))
        .await;
    let view = a
        .assignments_until("B dropped from A's slot, not reported unreachable", |x| {
            senders_in(x) == vec![None]
        })
        .await;
    assert!(
        view.unreachable_sender_ids.is_empty(),
        "a participant with no connection is not-yet-connected, never unreachable"
    );
    let after = pushes_caught_up(&stack, &rig, "slot-conn", carrying).await;
    assert!(
        Some(*after.last().unwrap()) > before.0,
        "the carrying handler's snapshot changed and advanced"
    );
}

/// An edge MOVES (keeping its slot and its sender) when its handler leaves one
/// party's set while another shared handler remains — and moves only then.
#[tokio::test]
async fn an_edge_moves_to_the_remaining_shared_handler_and_keeps_its_slot() {
    let (stack, rig) = start_stack("slot-move").await;
    seed_meeting_with_handlers(
        &stack,
        "slot-move",
        vec![mh_handler("mh-0"), mh_handler("mh-1")],
    )
    .await;
    let mut a = join_as(&rig, &stack, "slot-move", "user-a").await;
    let mut b = join_as(&rig, &stack, "slot-move", "user-b").await;
    a.write(capability_frame(audio_slots(1))).await;
    b.write(capability_frame(audio_slots(1))).await;
    let (_, first) = a.settle().await;
    let first = first.unwrap();
    let _ = b.settle().await;
    let url = first.assignments[0].media_handler_url.clone();
    let carrying = if url == "wt://mh-0:4433" {
        "mh-0"
    } else {
        "mh-1"
    };

    let snap = MetricAssertion::snapshot();
    b.disconnect_from(&stack, carrying, &connection_id_for("user-b", carrying))
        .await;
    let moved = a
        .assignments_until("A's slot now read on the other handler", |x| {
            x.assignments[0].media_handler_url != url
        })
        .await;
    assert_eq!(
        senders_in(&moved),
        vec![Some(b.sender_id)],
        "same sender, same slot"
    );
    assert_eq!(moved.assignments[0].slot_id, first.assignments[0].slot_id);
    snap.counter("mc_media_edge_moves_total")
        .with_labels(&[
            ("reason", "connectivity_change"),
            ("key_custody", "operator"),
        ])
        .assert_delta(2); // A<-B and B<-A
    snap.counter("mc_media_edge_moves_total")
        .with_labels(&[("reason", "unexpected"), ("key_custody", "operator")])
        .assert_delta(0);
}

/// R1: a stale Disconnected from a superseded session does not remove a handler
/// a newer session of the same participant is live on.
#[tokio::test]
async fn a_stale_disconnect_does_not_remove_a_handler_a_newer_session_is_live_on() {
    let (stack, rig) = start_stack("slot-stale").await;
    seed_meeting_with_mh(&stack, "slot-stale").await;
    let mut a = join_as(&rig, &stack, "slot-stale", "user-a").await;
    let b = join_as(&rig, &stack, "slot-stale", "user-b").await;
    a.write(capability_frame(audio_slots(1))).await;
    a.assignments_until("A hears B", |x| senders_in(x) == vec![Some(b.sender_id)])
        .await;
    let before = rendered(&rig, "slot-stale", "mh-test-1").await;

    // B's transport reconnects: session 2 opens BEFORE session 1's close is
    // reported (idle-timeout ordering).
    b.connect_to(&stack, "mh-test-1", "user-b@mh-test-1#2")
        .await;
    b.disconnect_from(
        &stack,
        "mh-test-1",
        &connection_id_for("user-b", "mh-test-1"),
    )
    .await;
    actor_drained(&stack, "slot-stale").await;
    assert_eq!(
        rendered(&rig, "slot-stale", "mh-test-1").await,
        before,
        "B is still live on the handler through session 2: nothing changed"
    );
    a.expect_no_media().await;
}

// ============================================================================
// The per-meeting flush bound: deferred, never dropped, and per meeting
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
        // Every participant reaches the one handler, as MH would report.
        notify_connected(&stack, "slot-grace", &format!("user-{p}"), "mh-test-1", p).await;
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
        notify_connected(
            &stack,
            "slot-reconnect",
            &format!("user-{p}"),
            "mh-test-1",
            p,
        )
        .await;
        meeting
            .register_receive_capability(format!("part-{p}"), decl())
            .await
            .unwrap();
    }
    virtual_turns().await;

    // B drops without a clean close: grace, not removal. (Its MEDIA connection
    // is a separate transport and stays up, so B stays routable.)
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
    notify_connected(&stack, "slot-reconnect", "user-c", "mh-test-1", "c").await;
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
/// is counted and logged, and cannot widen the set: the joiner is offered the
/// FROZEN set, never the injected handler.
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
        let offered: Vec<HandlerId> = joined.media_handlers.ids().cloned().collect();
        results.push((offered, divergences));
    }
    assert_eq!(
        results[0],
        (vec![HandlerId::new("mh-0")], 0),
        "the first join freezes the set"
    );
    assert_eq!(
        results[1],
        (vec![HandlerId::new("mh-0")], 1),
        "a divergent later join is counted and offered the FROZEN one-handler set, never \
         the injected handler"
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

// ============================================================================
// The connect settle window (`media_routing/connectivity.rs`), on paused time
// ============================================================================

/// An actor-level meeting on `handlers` with `users` joined (not connected).
async fn actor_meeting(
    label: &str,
    handlers: &[&str],
    users: &[&str],
) -> (
    TestStackHandles,
    mc_service::actors::MeetingActorHandle,
    Vec<u32>,
    Vec<tokio::sync::mpsc::Receiver<bytes::Bytes>>,
) {
    let stack = build_test_stack(label).await;
    let endpoints: Vec<_> = handlers.iter().map(|h| mh_handler(h)).collect();
    seed_meeting_with_handlers(&stack, label, endpoints.clone()).await;
    let meeting = stack
        .controller_handle
        .get_meeting_handle(label.to_string())
        .await
        .unwrap();
    let mut senders = Vec::new();
    // Held by the caller so each participant's outbound channel stays open.
    let mut outbound = Vec::new();
    for user in users {
        let (tx, rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(1024);
        outbound.push(rx);
        let joined = meeting
            .connection_join(
                format!("conn-{user}"),
                (*user).to_string(),
                format!("part-{user}"),
                String::new(),
                false,
                test_identity_key(),
                Some(tx),
                test_join_media(&stack, &endpoints),
            )
            .await
            .unwrap();
        senders.push(u32::from(joined.sender_id.get().get()));
        let decl = mc_service::media_signaling::ReceiveCapabilityDeclaration::parse(
            &proto_gen::dark_tower::signaling::v1::ReceiveCapability {
                slots: audio_slots(3),
            },
            8,
            true,
        )
        .unwrap();
        meeting
            .register_receive_capability(format!("part-{user}"), decl)
            .await
            .unwrap();
    }
    (stack, meeting, senders, outbound)
}

fn window() -> Duration {
    client_media_config().connect_settle_window
}

/// THE TRIPWIRE for the settle window (named in `media_routing/connectivity.rs`).
///
/// Everyone ends up connected to both handlers, but the connects arrive
/// interleaved so that, WITHOUT the window, A would become routable on both
/// handlers while B was on mh-1 only and C on mh-0 only — placing A->B on mh-1
/// and A->C on mh-0 — and since a still-valid edge never moves, A would send to
/// both handlers forever. With the window every participant settles on its
/// FULL set, so each sender ends with exactly one target.
#[tokio::test(start_paused = true)]
async fn staggered_connects_settle_to_one_target_per_sender() {
    let label = "slot-stagger";
    let (stack, _meeting, s, _rx) =
        actor_meeting(label, &["mh-0", "mh-1"], &["user-a", "user-b", "user-c"]).await;
    for (user, handler) in [
        ("user-a", "mh-0"),
        ("user-b", "mh-1"),
        ("user-a", "mh-1"),
        ("user-c", "mh-0"),
        ("user-b", "mh-0"),
        ("user-c", "mh-1"),
    ] {
        notify_connected(
            &stack,
            label,
            user,
            handler,
            &connection_id_for(user, handler),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(window() * 2).await;
    virtual_turns().await;

    let calls = stack.mh_reg_client.calls();
    for (i, sender) in s.iter().enumerate() {
        let heard = heard_by(&calls, *sender);
        let others: BTreeSet<u32> = s.iter().copied().filter(|x| x != sender).collect();
        assert_eq!(heard, others, "participant {i}: everyone hears everyone");
        assert_eq!(
            targets_at_mh(&calls, *sender).len(),
            1,
            "participant {i}: all-connected yields ONE target per sender, whatever the connect order"
        );
    }
}

/// A participant that never reaches every handler settles at the window's end
/// with what was observed — and is neither routed nor named unreachable before.
#[tokio::test(start_paused = true)]
async fn a_partial_participant_settles_at_the_window_and_is_counted() {
    let label = "slot-partial";
    let (stack, _meeting, s, _rx) =
        actor_meeting(label, &["mh-0", "mh-1"], &["user-a", "user-b"]).await;
    let snap = MetricAssertion::snapshot();
    for handler in ["mh-0", "mh-1"] {
        notify_connected(
            &stack,
            label,
            "user-a",
            handler,
            &connection_id_for("user-a", handler),
        )
        .await;
    }
    notify_connected(
        &stack,
        label,
        "user-b",
        "mh-1",
        &connection_id_for("user-b", "mh-1"),
    )
    .await;
    virtual_turns().await;
    assert!(
        heard_by(&stack.mh_reg_client.calls(), s[0]).is_empty(),
        "B is still establishing: no edge before the window elapses"
    );

    tokio::time::sleep(window()).await;
    virtual_turns().await;
    let calls = stack.mh_reg_client.calls();
    assert_eq!(heard_by(&calls, s[0]), BTreeSet::from([s[1]]));
    assert_eq!(
        edge_owner(&calls, s[0], s[1]),
        Some(endpoint_of("mh-1")),
        "the only handler the pair shares"
    );
    snap.counter("mc_media_connect_settles_total")
        .with_labels(&[("outcome", "complete"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_connect_settles_total")
        .with_labels(&[("outcome", "window_elapsed"), ("key_custody", "operator")])
        .assert_delta(1);
}

/// The deadline is set ONCE, by the first MH-observed connect: repeated
/// connects (new sessions, retries) do not re-arm it.
#[tokio::test(start_paused = true)]
async fn repeated_connects_do_not_re_arm_the_settle_window() {
    let label = "slot-rearm";
    let (stack, _meeting, s, _rx) =
        actor_meeting(label, &["mh-0", "mh-1"], &["user-a", "user-b"]).await;
    for handler in ["mh-0", "mh-1"] {
        notify_connected(
            &stack,
            label,
            "user-a",
            handler,
            &connection_id_for("user-a", handler),
        )
        .await;
    }
    notify_connected(&stack, label, "user-b", "mh-0", "b-1").await;
    for n in 2..6 {
        tokio::time::sleep(window() / 5).await;
        notify_connected(&stack, label, "user-b", "mh-0", &format!("b-{n}")).await;
        notify_connected(&stack, label, "user-b", "mh-0", "b-1").await; // a retry
    }
    // 4/5 of the window has passed since B's first connect; one more fifth
    // (plus a turn) reaches the ORIGINAL deadline.
    tokio::time::sleep(window() / 5 + Duration::from_millis(10)).await;
    virtual_turns().await;
    assert_eq!(
        heard_by(&stack.mh_reg_client.calls(), s[0]),
        BTreeSet::from([s[1]]),
        "B settled at its FIRST connect's deadline, not a re-armed one"
    );
}

/// S10: a client cycling its media transports drives at most one settle per
/// window, and only ever delays itself — and a storm of cycles cancelled before
/// settling does not push its return out further than one window.
#[tokio::test(start_paused = true)]
async fn a_reconnect_storm_is_floored_to_one_episode_per_window() {
    let label = "slot-storm";
    let (stack, meeting, s, _rx) = actor_meeting(label, &["mh-0"], &["user-a", "user-b"]).await;
    let rig_gens = || async {
        let _ = meeting.get_state().await;
        stack.mh_reg_client.calls().len()
    };
    notify_connected(&stack, label, "user-a", "mh-0", "a").await;
    notify_connected(&stack, label, "user-b", "mh-0", "b-0").await;
    virtual_turns().await;
    let settled = rig_gens().await;

    // B drops and reconnects its only transport ten times, immediately.
    for n in 1..=10 {
        notify_disconnected(&stack, label, "user-b", "mh-0", &format!("b-{}", n - 1)).await;
        notify_connected(&stack, label, "user-b", "mh-0", &format!("b-{n}")).await;
    }
    virtual_turns().await;
    let during = rig_gens().await - settled;
    assert!(
        during <= 1,
        "ten drop/reconnect cycles inside one window drove {during} pushes; the floor allows one"
    );
    tokio::time::sleep(window() * 2).await;
    virtual_turns().await;
    assert_eq!(
        heard_by(&stack.mh_reg_client.calls(), s[0]),
        BTreeSet::from([s[1]]),
        "B is routable again once the floor has passed"
    );
}

/// Characterization of R3's no-repair property: a participant whose connected
/// set empties stays not-connected — no edges, unreachable to no one — and
/// NOTHING re-adds it without a new connect. There is no repair event to wait
/// for: MH sends Connected once per session.
#[tokio::test(start_paused = true)]
async fn losing_every_handler_stays_not_connected_with_no_spontaneous_regain() {
    let label = "slot-lost";
    let (stack, _meeting, s, _rx) = actor_meeting(label, &["mh-0"], &["user-a", "user-b"]).await;
    notify_connected(&stack, label, "user-a", "mh-0", "a").await;
    notify_connected(&stack, label, "user-b", "mh-0", "b").await;
    virtual_turns().await;
    assert_eq!(
        heard_by(&stack.mh_reg_client.calls(), s[0]),
        BTreeSet::from([s[1]])
    );

    notify_disconnected(&stack, label, "user-b", "mh-0", "b").await;
    virtual_turns().await;
    let pushes = stack.mh_reg_client.calls().len();
    assert!(heard_by(&stack.mh_reg_client.calls(), s[0]).is_empty());

    tokio::time::sleep(window() * 10).await;
    virtual_turns().await;
    assert_eq!(
        stack.mh_reg_client.calls().len(),
        pushes,
        "nothing re-asserts connectivity on its own: no generation re-advance"
    );
    assert!(
        heard_by(&stack.mh_reg_client.calls(), s[0]).is_empty(),
        "never spontaneously regained"
    );
}

/// Formation burst: every participant connects to BOTH handlers, i.e. 2N
/// notifications — yet each participant changes routing only once (at settle),
/// so the carrying handler sees at most one push per settled participant, not
/// one per notification.
#[tokio::test(start_paused = true)]
async fn a_formation_burst_coalesces_to_at_most_one_push_per_settle() {
    const N: usize = 6;
    let label = "slot-burst";
    let users: Vec<String> = (0..N).map(|i| format!("user-{i}")).collect();
    let user_refs: Vec<&str> = users.iter().map(String::as_str).collect();
    let (stack, _meeting, _s, _rx) = actor_meeting(label, &["mh-0", "mh-1"], &user_refs).await;
    let before = stack.mh_reg_client.calls().len();
    for user in &users {
        for handler in ["mh-0", "mh-1"] {
            notify_connected(
                &stack,
                label,
                user,
                handler,
                &connection_id_for(user, handler),
            )
            .await;
        }
    }
    virtual_turns().await;
    let calls = stack.mh_reg_client.calls();
    for handler in ["mh-0", "mh-1"] {
        let pushes = calls[before..]
            .iter()
            .filter(|c| c.mh_grpc_endpoint == endpoint_of(handler))
            .count();
        assert!(
            pushes <= N,
            "{handler}: {pushes} pushes for {} notifications — must coalesce to <= one per settle",
            2 * N
        );
    }
}

#[tokio::test]
async fn the_published_settle_window_is_the_enforced_window() {
    let snap = MetricAssertion::snapshot();
    let (_stack, _rig) = start_stack("slot-settle-gauge").await;
    snap.gauge("mc_media_connect_settle_window_seconds")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(window().as_secs_f64());
}
