//! Server mute at MH ingress (story 2 R-9; ADR-0036 §7) and the
//! multi-handler forwarding proof S10c (R-4, R-33; ADR-0036 §9).
//!
//! Every test drives `media::forward::forward_one` against REAL routing
//! snapshots and REAL subscriber registries. The fixtures mirror what MC
//! actually produces (story 2 task 20's edge model; @paired-meeting-controller
//! at Gate 1):
//!
//! - **A mute does not remove edges.** A muted sender keeps its slots and its
//!   egress streams, so in production a muted `sender_id` is always a live
//!   candidate with subscribers. Every mute fixture here keeps that shape: a
//!   fixture that removed the muted sender's edges would be testing
//!   `no_subscriber`, and the "drop" would be vacuous.
//! - **A mute is SOURCE-only.** The muted participant still hears everyone,
//!   so an edge where the muted sender is the SUBSCRIBER keeps forwarding.
//!   Keying the check on the wrong side is the obvious bug; it is pinned.
//! - Ids follow MC's packing: `egress_stream_id = (subscriber << 8) |
//!   ordinal`, one candidate at stream 0, priority group 1, datagram, and the
//!   subscriber's CLIENT-DECLARED slot id. Sender ids and slot ids are drawn
//!   from disjoint ranges so a mixed-up lookup cannot pass by coincidence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

#[path = "common/mod.rs"]
mod common;

use common::media_frame::{audio_datagram, marker_of};
use common::media_rig::sender;

use ::common::observability::testing::MetricAssertion;
use media_protocol::codec::decode_datagram;
use mh_service::config::{EGRESS_QUEUE_FRAMES, NOMINAL_AUDIO_FRAME_BYTES};
use mh_service::media::forward::{forward_one, EgressQueue, ForwardOutcome, IngressFrame};
use mh_service::media::forwarder::ConnectionForwarder;
use mh_service::media::queue::SharedQueue;
use mh_service::observability::metrics::{resolve_media_handles, MediaDropReason};
use mh_service::routing::{MeetingKey, MeetingPolicy, RoutingTable};
use mh_service::session::{ApplyOutcome, LocalSubscribers, SessionManagerHandle};
use mh_test_utils::admission::{fixture_policy_limits, with_ceiling};
use mh_test_utils::media_policy::{egress, register_request, register_request_muted};
use mh_test_utils::session::apply_registered;
use proto_gen::dark_tower::internal::v1::EgressStream;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

const MEETING: &str = "meeting-server-mute";

// Sender ids (per-meeting ordinals) and client-declared slot ids, from
// deliberately disjoint ranges.
const A: u32 = 101;
const B: u32 = 202;
const C: u32 = 303;
const A_SLOT: u32 = 7;
const B_SLOT_0: u32 = 513;
const B_SLOT_1: u32 = 3;
const C_SLOT: u32 = 260;

/// One MC-shaped egress stream: `source` into `subscriber`'s slot at
/// `ordinal`, with MC's id packing and priority group.
fn edge(subscriber: u32, ordinal: u32, slot: u32, source: u32) -> EgressStream {
    let mut stream = egress((subscriber << 8) | ordinal, subscriber, slot, source);
    stream.priority_group = 1;
    stream
}

fn install(table: &RoutingTable, generation: u64, streams: Vec<EgressStream>, muted: &[u32]) {
    let request = register_request_muted(MEETING, generation, streams, muted);
    table.install(&MeetingPolicy::from_request(&request, &fixture_policy_limits()).unwrap());
}

fn forwarder_for(publisher: u32) -> ConnectionForwarder {
    ConnectionForwarder::new(
        MeetingKey::new(MEETING),
        sender(publisher),
        Arc::new(resolve_media_handles()),
        1.0,
        NOMINAL_AUDIO_FRAME_BYTES,
    )
}

fn connect(registry: &LocalSubscribers, subscriber: u32, handler: &str) -> Arc<EgressQueue> {
    let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
    registry.register(
        MeetingKey::new(MEETING),
        sender(subscriber),
        &format!("{handler}-conn-{subscriber}"),
        &queue,
    );
    queue
}

fn send(
    forwarder: &mut ConnectionForwarder,
    table: &RoutingTable,
    registry: &LocalSubscribers,
    seq: u32,
    marker: u8,
) -> ForwardOutcome {
    forward_one(
        forwarder,
        &table.load(),
        &registry.load(),
        IngressFrame {
            payload: audio_datagram(seq, marker),
            received_at: Instant::now(),
        },
    )
}

/// Pop one delivered frame and return `(marker, relay slot, hop)`.
fn pop(queue: &EgressQueue) -> Option<(u8, u16, u32)> {
    queue.try_pop().map(|frame| {
        let view = decode_datagram(&frame.payload).expect("relayed frame decodes");
        (
            marker_of(&frame.payload),
            view.stream_id(),
            view.hop_sequence(),
        )
    })
}

const MUTED_REASON: ForwardOutcome = ForwardOutcome {
    delivered: 0,
    dropped: 0,
    rejected: Some(MediaDropReason::ServerMuted),
};

// ---------------------------------------------------------------------------
// Enforcement shape
// ---------------------------------------------------------------------------

/// The muted sender is still a live candidate with a connected subscriber,
/// and the drop fires anyway: counted under `server_muted` (ingress), the
/// ingress FORWARD counter unchanged (the accounting identity), nothing
/// delivered. Meanwhile the muted participant still HEARS its peer.
#[tokio::test]
async fn a_muted_sender_with_live_edges_is_dropped_and_still_hears_its_peer() {
    let snapshot = MetricAssertion::snapshot();

    let table = RoutingTable::new();
    let registry = LocalSubscribers::new();
    // A <-> B, both directions live; A muted.
    install(
        &table,
        1,
        vec![edge(B, 0, B_SLOT_0, A), edge(A, 0, A_SLOT, B)],
        &[A],
    );
    let a_queue = connect(&registry, A, "h");
    let b_queue = connect(&registry, B, "h");
    let mut from_a = forwarder_for(A);
    let mut from_b = forwarder_for(B);

    assert_eq!(send(&mut from_a, &table, &registry, 1, 0xA1), MUTED_REASON);
    assert_eq!(
        b_queue.depth(),
        0,
        "a muted publisher reaches no subscriber"
    );

    snapshot
        .counter("mh_media_frames_dropped_total")
        .with_labels(&[
            ("reason", "server_muted"),
            ("direction", "ingress"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    snapshot
        .counter("mh_media_frames_forwarded_total")
        .with_labels(&[("direction", "ingress"), ("key_custody", "operator")])
        .assert_delta(0);

    // SOURCE-only: B -> A still flows. A check keyed on the SUBSCRIBER side
    // would drop this frame.
    let heard = send(&mut from_b, &table, &registry, 1, 0xB1);
    assert_eq!(heard.delivered, 1, "the muted participant still hears B");
    let (marker, slot, _) = pop(&a_queue).expect("A received B's frame");
    assert_eq!((marker, u32::from(slot)), (0xB1, A_SLOT));
}

/// A mute is checked BEFORE decode: a malformed frame from a muted sender is
/// a `server_muted` drop, not a codec reject — the muted party's frame bytes
/// are never parsed, so it cannot steer which counter moves.
#[tokio::test]
async fn a_muted_senders_malformed_frame_is_server_muted_not_a_codec_reject() {
    let table = RoutingTable::new();
    let registry = LocalSubscribers::new();
    install(&table, 1, vec![edge(B, 0, B_SLOT_0, A)], &[A]);
    let _b = connect(&registry, B, "h");
    let mut from_a = forwarder_for(A);
    let outcome = forward_one(
        &mut from_a,
        &table.load(),
        &registry.load(),
        IngressFrame {
            payload: bytes::Bytes::from_static(b"not a frame"),
            received_at: Instant::now(),
        },
    );
    assert_eq!(outcome, MUTED_REASON);
}

// ---------------------------------------------------------------------------
// Re-assert survival, through the REAL session actor apply path
// ---------------------------------------------------------------------------

/// The mute rides the full snapshot (`internal.proto` field 7):
///
/// 1. the same generation re-sent is a no-op — the SAME snapshot stays live
///    and the mute holds;
/// 2. the same generation re-sent WITHOUT the mute is also a no-op — an
///    unmute that does not advance the generation never takes effect (the MC
///    contract violation the divergence WARN names);
/// 3. a new generation still carrying the mute keeps it;
/// 4. a new generation without it restores forwarding on the very next frame
///    — and the first forwarded frame carries hop 0, proving no muted frame
///    consumed a hop number (a consumed hop would read as loss downstream).
#[tokio::test]
async fn a_mute_survives_reassert_and_an_unmute_takes_effect_in_one_generation() {
    let sm = SessionManagerHandle::new(with_ceiling(64));
    let limits = fixture_policy_limits();
    let apply = |generation: u64, muted: &'static [u32]| {
        let request = register_request_muted(
            MEETING,
            generation,
            vec![edge(B, 0, B_SLOT_0, A), edge(A, 0, A_SLOT, B)],
            muted,
        );
        let policy = MeetingPolicy::from_request(&request, &limits).unwrap();
        let sm = sm.clone();
        // Register, then apply: the production order (see
        // `mh_test_utils::session`).
        async move { apply_registered(&sm, policy).await }
    };

    let routing = sm.routing_table();
    let registry = Arc::clone(sm.subscribers());
    let b_queue = connect(&registry, B, "h");
    let mut from_a = forwarder_for(A);
    let meeting = MeetingKey::new(MEETING);

    assert_eq!(apply(1, &[A]).await, ApplyOutcome::Applied);
    assert_eq!(
        send(&mut from_a, &routing, &registry, 1, 0xA1),
        MUTED_REASON
    );

    // (1) identical re-assert: no swap, mute holds.
    let before = routing.load();
    assert_eq!(apply(1, &[A]).await, ApplyOutcome::Applied);
    assert!(
        Arc::ptr_eq(&before, &routing.load()),
        "an equal-generation re-assert is a true data-plane no-op"
    );
    assert_eq!(
        send(&mut from_a, &routing, &registry, 2, 0xA2),
        MUTED_REASON
    );

    // (2) unmute at an UNCHANGED generation: honoured generation, no swap.
    assert_eq!(apply(1, &[]).await, ApplyOutcome::Applied);
    assert!(routing.load().is_server_muted(&meeting, sender(A)));
    assert_eq!(
        send(&mut from_a, &routing, &registry, 3, 0xA3),
        MUTED_REASON
    );

    // (3) new generation, mute still carried: survives.
    assert_eq!(apply(2, &[A]).await, ApplyOutcome::Applied);
    assert_eq!(
        send(&mut from_a, &routing, &registry, 4, 0xA4),
        MUTED_REASON
    );
    assert_eq!(b_queue.depth(), 0);

    // (4) new generation without the mute: forwarding resumes immediately.
    assert_eq!(apply(3, &[]).await, ApplyOutcome::Applied);
    let resumed = send(&mut from_a, &routing, &registry, 5, 0xA5);
    assert_eq!(
        resumed.delivered, 1,
        "unmute restores forwarding in one generation"
    );
    let (marker, slot, hop) = pop(&b_queue).expect("B hears A again");
    assert_eq!((marker, u32::from(slot)), (0xA5, B_SLOT_0));
    assert_eq!(
        hop, 0,
        "four muted frames consumed no hop number: the first forwarded frame is hop 0"
    );
}

// ---------------------------------------------------------------------------
// S10c — no cross-handler forwarding, by construction (ADR-0036 §9)
// ---------------------------------------------------------------------------

/// One handler: its own routing table and subscriber registry. Two of these
/// share NOTHING — which is the property: MH is multi-handler-oblivious, with
/// no peer-handler concept, so "no cross-handler forwarding" is structural
/// rather than a runtime guard.
struct Handler {
    name: &'static str,
    table: RoutingTable,
    registry: LocalSubscribers,
}

impl Handler {
    fn new(name: &'static str, streams: Vec<EgressStream>, muted: &[u32]) -> Self {
        let table = RoutingTable::new();
        install(&table, 1, streams, muted);
        Self {
            name,
            table,
            registry: LocalSubscribers::new(),
        }
    }

    fn edge_ids(&self) -> BTreeSet<u32> {
        let snapshot = self.table.load();
        snapshot
            .routes_for(&MeetingKey::new(MEETING))
            .expect("policy installed")
            .edges()
            .iter()
            .map(|e| e.egress_stream_id)
            .collect()
    }

    fn connect(&self, subscriber: u32) -> Arc<EgressQueue> {
        connect(&self.registry, subscriber, self.name)
    }

    fn send(&self, publisher: u32, seq: u32, marker: u8) -> ForwardOutcome {
        let mut forwarder = forwarder_for(publisher);
        send(&mut forwarder, &self.table, &self.registry, seq, marker)
    }
}

fn ids(streams: &[EgressStream]) -> BTreeSet<u32> {
    streams.iter().map(|s| s.egress_stream_id).collect()
}

const NO_SUBSCRIBER: ForwardOutcome = ForwardOutcome {
    delivered: 0,
    dropped: 0,
    rejected: Some(MediaDropReason::NoSubscriber),
};

/// MC's canonical partial-connectivity shape: A connected {H1}, B {H1, H2},
/// C {H2}. H1 = {A->B, B->A}, H2 = {B->C, C->B}; A and C share no handler and
/// have no edge. B's two slots are split across the handlers (ordinal 0 on
/// H1, ordinal 1 on H2). The edge sets are DISJOINT by egress stream id, NOT
/// by participant: B appears in both snapshots.
#[tokio::test]
async fn s10c_each_handler_holds_only_its_own_edges_and_forwards_nothing_across() {
    let h1_streams = vec![edge(B, 0, B_SLOT_0, A), edge(A, 0, A_SLOT, B)];
    let h2_streams = vec![edge(C, 0, C_SLOT, B), edge(B, 1, B_SLOT_1, C)];
    let (h1_ids, h2_ids) = (ids(&h1_streams), ids(&h2_streams));
    assert!(
        h1_ids.is_disjoint(&h2_ids),
        "fixture: MC's ids are meeting-unique"
    );

    let h1 = Handler::new("h1", h1_streams, &[]);
    let h2 = Handler::new("h2", h2_streams, &[]);
    assert_eq!(
        h1.edge_ids(),
        h1_ids,
        "H1's snapshot holds exactly H1's edges"
    );
    assert_eq!(
        h2.edge_ids(),
        h2_ids,
        "H2's snapshot holds exactly H2's edges"
    );

    let a_on_h1 = h1.connect(A);
    let b_on_h1 = h1.connect(B);
    let b_on_h2 = h2.connect(B);
    let c_on_h2 = h2.connect(C);

    // A sender present only in the OTHER handler's set forwards to no one.
    assert_eq!(h1.send(C, 1, 0xC1), NO_SUBSCRIBER, "C's frame on H1");
    assert_eq!(h2.send(A, 1, 0xA1), NO_SUBSCRIBER, "A's frame on H2");

    // B on H1 reaches A only, on A's declared slot — never C (not on H1).
    assert_eq!(h1.send(B, 1, 0xB1).delivered, 1);
    assert_eq!(
        pop(&a_on_h1).map(|(m, s, _)| (m, u32::from(s))),
        Some((0xB1, A_SLOT))
    );
    // B on H2 reaches C only.
    assert_eq!(h2.send(B, 2, 0xB2).delivered, 1);
    assert_eq!(
        pop(&c_on_h2).map(|(m, s, _)| (m, u32::from(s))),
        Some((0xB2, C_SLOT))
    );
    // B's two slots: A's frame lands on ordinal 0's slot via H1, C's on
    // ordinal 1's slot via H2 — each on its own transport.
    assert_eq!(h1.send(A, 3, 0xA3).delivered, 1);
    assert_eq!(h2.send(C, 3, 0xC3).delivered, 1);
    assert_eq!(
        pop(&b_on_h1).map(|(m, s, _)| (m, u32::from(s))),
        Some((0xA3, B_SLOT_0))
    );
    assert_eq!(
        pop(&b_on_h2).map(|(m, s, _)| (m, u32::from(s))),
        Some((0xC3, B_SLOT_1))
    );

    // Nothing else arrived anywhere.
    for (who, queue) in [
        ("A@H1", &a_on_h1),
        ("B@H1", &b_on_h1),
        ("B@H2", &b_on_h2),
        ("C@H2", &c_on_h2),
    ] {
        assert!(
            pop(queue).is_none(),
            "{who} received a frame it has no edge for"
        );
    }
}

/// The split-pair variant: the two directions of ONE pair land on different
/// handlers (B->A on H1, A->B on H2), because MC co-locates by the SENDER's
/// out-edges. Each direction is independent of the other.
#[tokio::test]
async fn s10c_a_pair_split_across_handlers_forwards_each_direction_only_where_it_lives() {
    let h1 = Handler::new("h1", vec![edge(A, 0, A_SLOT, B)], &[]);
    let h2 = Handler::new("h2", vec![edge(B, 0, B_SLOT_0, A)], &[]);
    let a_on_h1 = h1.connect(A);
    let _b_on_h1 = h1.connect(B);
    let _a_on_h2 = h2.connect(A);
    let b_on_h2 = h2.connect(B);

    assert_eq!(
        h1.send(A, 1, 0xA1),
        NO_SUBSCRIBER,
        "A->B does not live on H1"
    );
    assert_eq!(
        h2.send(B, 1, 0xB1),
        NO_SUBSCRIBER,
        "B->A does not live on H2"
    );
    assert_eq!(h1.send(B, 2, 0xB2).delivered, 1);
    assert_eq!(h2.send(A, 2, 0xA2).delivered, 1);
    assert_eq!(pop(&a_on_h1).map(|(m, _, _)| m), Some(0xB2));
    assert_eq!(pop(&b_on_h2).map(|(m, _, _)| m), Some(0xA2));
}

/// With B muted, B is in BOTH handlers' sets (the per-handler filter is edge
/// OWNERSHIP: B sources an edge on each). Each handler drops B from its OWN
/// snapshot, independently, while the edges INTO B keep forwarding on both.
#[tokio::test]
async fn s10c_a_sender_muted_on_both_handlers_is_dropped_by_each_independently() {
    let h1 = Handler::new(
        "h1",
        vec![edge(B, 0, B_SLOT_0, A), edge(A, 0, A_SLOT, B)],
        &[B],
    );
    let h2 = Handler::new(
        "h2",
        vec![edge(C, 0, C_SLOT, B), edge(B, 1, B_SLOT_1, C)],
        &[B],
    );
    let a_on_h1 = h1.connect(A);
    let b_on_h1 = h1.connect(B);
    let b_on_h2 = h2.connect(B);
    let c_on_h2 = h2.connect(C);

    assert_eq!(h1.send(B, 1, 0xB1), MUTED_REASON);
    assert_eq!(h2.send(B, 1, 0xB1), MUTED_REASON);
    assert!(pop(&a_on_h1).is_none() && pop(&c_on_h2).is_none());

    // Source-only, on each handler: B still hears A and C.
    assert_eq!(h1.send(A, 2, 0xA2).delivered, 1);
    assert_eq!(h2.send(C, 2, 0xC2).delivered, 1);
    assert_eq!(pop(&b_on_h1).map(|(m, _, _)| m), Some(0xA2));
    assert_eq!(pop(&b_on_h2).map(|(m, _, _)| m), Some(0xC2));
}

/// Mute is meeting-scoped on the forward path too (@security S-2): sender A
/// muted in one meeting forwards normally in another meeting on the SAME
/// handler, where the same ordinal is a different participant.
#[tokio::test]
async fn a_sender_muted_in_one_meeting_forwards_normally_in_another_on_the_same_handler() {
    let table = RoutingTable::new();
    let limits = fixture_policy_limits();
    table.install(
        &MeetingPolicy::from_request(
            &register_request_muted("meeting-a", 1, vec![edge(B, 0, B_SLOT_0, A)], &[A]),
            &limits,
        )
        .unwrap(),
    );
    table.install(
        &MeetingPolicy::from_request(
            &register_request("meeting-b", 1, vec![edge(B, 0, B_SLOT_0, A)]),
            &limits,
        )
        .unwrap(),
    );
    let registry = LocalSubscribers::new();
    let mut queues = Vec::new();
    for meeting in ["meeting-a", "meeting-b"] {
        let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
        registry.register(
            MeetingKey::new(meeting),
            sender(B),
            &format!("{meeting}-b"),
            &queue,
        );
        queues.push(queue);
    }
    let handles = Arc::new(resolve_media_handles());
    let forwarder = |meeting: &str| {
        ConnectionForwarder::new(
            MeetingKey::new(meeting),
            sender(A),
            Arc::clone(&handles),
            1.0,
            NOMINAL_AUDIO_FRAME_BYTES,
        )
    };
    let mut in_a = forwarder("meeting-a");
    let mut in_b = forwarder("meeting-b");

    assert_eq!(send(&mut in_a, &table, &registry, 1, 0xA1), MUTED_REASON);
    assert_eq!(
        send(&mut in_b, &table, &registry, 1, 0xA2).delivered,
        1,
        "the same ordinal in meeting B is not muted"
    );
    assert_eq!(queues[0].depth(), 0);
    assert_eq!(pop(&queues[1]).map(|(m, _, _)| m), Some(0xA2));
}
