//! ADR-0036 §10 **Tier-1a** gates for the audio datagram forward path.
//!
//! Pure function, no QUIC, no crypto, no clock beyond a monotonic read. Every
//! test here drives `media::forward::forward_one` directly against a real
//! routing snapshot and a real subscriber registry.
//!
//! # What each gate is looking for
//!
//! Not "does it work" — every one of these is written against a *specific*
//! wrong implementation that would otherwise pass the rest of the suite:
//!
//! - frames-in-equals-frames-out catches a relay that rewrites more than the
//!   six relay bytes, which fails Ed25519 verification at every receiver and
//!   is invisible to MH (MH is keyless and counts nothing for it);
//! - the hop-sequence gates catch a counter scoped per connection or per QUIC
//!   stream instead of per media stream;
//! - the allocation gate catches a fan-out that allocates per frame, WITH a
//!   proof-of-trap so the gate itself cannot go green-but-blind;
//! - `no_policy` / `no_subscriber` catch the story's signature failure mode:
//!   every signal green and no audio.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

#[path = "common/mod.rs"]
mod common;

use common::media_frame::{
    audio_datagram, audio_datagram_shaped, marker_of, relay_region_offset, FrameShape,
};
use common::media_rig::{sender, LoopbackRig};

use bytes::Bytes;
use media_protocol::codec::decode_datagram;
use mh_service::config::{PolicyLimits, EGRESS_QUEUE_FRAMES, NOMINAL_AUDIO_FRAME_BYTES};
use mh_service::media::forward::{forward_one, EgressQueue, IngressFrame};
use mh_service::media::forwarder::ConnectionForwarder;
use mh_service::media::queue::SharedQueue;
use mh_service::observability::metrics::resolve_media_handles;
use mh_service::routing::{MeetingKey, MeetingPolicy, RoutingTable};
use mh_service::session::LocalSubscribers;
use mh_test_utils::media_policy::{egress, loopback_egress, register_request};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;

const MEETING: &str = "meeting-forward";
/// The slot MC assigns a participant's own audio in the loopback shape.
const MAIN_AUDIO_SLOT: u32 = 0;

/// A counting allocator, armed only around the steady-state measurement.
///
/// A global allocator is the only way to observe "this code path allocated"
/// without instrumenting the code path itself — instrumenting it would make
/// the gate assert its own instrumentation rather than the behaviour.
///
/// # The arming state is THREAD-LOCAL, and that is not a detail
///
/// A global `AtomicBool`/`AtomicUsize` pair reads every allocation the whole
/// **process** makes while armed, and `cargo test` runs this binary's tests on
/// several threads at once. The measurement then passes when run alone and
/// fails under `cargo test --workspace` — a flake whose cause looks like the
/// code under test and is not. Thread-local `Cell`s scope the count to the
/// measuring thread. `const`-initialised so the TLS access inside `alloc`
/// cannot itself allocate and recurse.
struct CountingAllocator;

thread_local! {
    static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static ALLOC_ARMED: Cell<bool> = const { Cell::new(false) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `try_with`: during thread teardown the TLS slot is gone, and a
        // panic from an allocator is not survivable.
        let _ = ALLOC_ARMED.try_with(|armed| {
            if armed.get() {
                let _ = ALLOC_COUNT.try_with(|count| count.set(count.get().saturating_add(1)));
            }
        });
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// A rig for `publisher`, sampling every frame so no assertion is gated on a
/// random draw. Shared with the other two media binaries.
fn rig(
    streams: Vec<proto_gen::dark_tower::internal::v1::EgressStream>,
    publisher: u32,
) -> LoopbackRig {
    LoopbackRig::new(MEETING, publisher, streams, 1.0)
}

fn ingress(payload: Bytes) -> IngressFrame {
    IngressFrame {
        payload,
        received_at: Instant::now(),
    }
}

// ---------------------------------------------------------------------------
// Frames in equals frames out
// ---------------------------------------------------------------------------

/// Everything except the six relay bytes must survive byte-identically.
///
/// The publisher region and the payload are covered by the Ed25519 signature
/// and by the AEAD associated data, so a single byte changed anywhere else
/// fails verification at **every** receiver — and MH, being keyless, counts
/// nothing at all for it. There is no MH-side signal for this defect by
/// construction, which is why this gate asserts byte identity rather than
/// "the frame still decodes".
#[tokio::test]
async fn only_the_relay_region_changes_and_every_other_byte_survives() {
    // Parameterised over all three header shapes, because the relay-region
    // offset MOVES with two of them and the assertion below is the only
    // detector for a cached offset. See `assert_the_three_shapes_have_distinct_offsets`.
    for shape in FrameShape::ALL {
        let mut rig_ = rig(vec![loopback_egress(1, 7, MAIN_AUDIO_SLOT)], 7);
        let queue = rig_.subscriber(7);

        let original = audio_datagram_shaped(42, 0xA5, shape);
        let outcome = forward_one(
            &mut rig_.forwarder,
            &rig_.routing.load(),
            &rig_.subscribers.load(),
            ingress(original.clone()),
        );
        assert_eq!(
            outcome.delivered, 1,
            "{shape:?}: the loopback edge must deliver"
        );

        let forwarded = queue.try_pop().expect("a frame must be queued").payload;
        assert_eq!(
            forwarded.len(),
            original.len(),
            "{shape:?}: the rewrite must not change the frame length"
        );

        let sent = decode_datagram(&original).unwrap();
        let received = decode_datagram(&forwarded).unwrap();

        assert_eq!(
            received.publisher_region(),
            sent.publisher_region(),
            "{shape:?}: the publisher region is signed: a relay that touches it silences the \
             sender at every receiver, and MH cannot observe that. A HARDCODED relay offset fails \
             here on every shape but the minimal one, which is the whole reason this test is \
             parameterised."
        );
        assert_eq!(
            received.payload(),
            sent.payload(),
            "{shape:?}: payload must be opaque"
        );
        assert_eq!(
            received.signature(),
            sent.signature(),
            "{shape:?}: signature must be untouched"
        );

        // And the relay region IS rewritten, to the values policy dictates.
        assert_eq!(
            u32::from(received.stream_id()),
            MAIN_AUDIO_SLOT,
            "{shape:?}: stream id must become the SUBSCRIBER's slot"
        );
        assert_eq!(
            received.hop_sequence(),
            0,
            "{shape:?}: hop sequence must be MH's own count, not the publisher's uplink value"
        );
        assert_ne!(
            received.hop_sequence(),
            sent.hop_sequence(),
            "{shape:?}: the publisher's uplink hop value must not survive into the downlink"
        );
    }
}

/// The three fixture shapes must land the relay region at three DIFFERENT
/// offsets, or the parameterisation above proves nothing.
///
/// This is the anti-vacuous half of the derived-offset gate. If a fixture edit
/// ever collapses the shapes back together — dropping the wrapped key, emptying
/// the extension region — the byte-identity test keeps passing while silently
/// covering one shape again, and the only detector for a cached offset
/// disappears without a failure. The offsets are read through
/// `MediaFrameView::relay_region_offset()`, whose own doc says "This is **not**
/// a constant… Never precompute it"; computing them here would assert the
/// fixture's arithmetic rather than the codec's.
#[test]
fn the_three_fixture_shapes_land_the_relay_region_at_distinct_offsets() {
    let offsets: Vec<usize> = FrameShape::ALL
        .into_iter()
        .map(|shape| relay_region_offset(&audio_datagram_shaped(1, 0x01, shape)))
        .collect();

    assert_eq!(offsets.len(), FrameShape::ALL.len());
    for (i, first) in offsets.iter().enumerate() {
        for (j, second) in offsets.iter().enumerate() {
            if i != j {
                assert_ne!(
                    first,
                    second,
                    "fixture shapes {:?} and {:?} share relay-region offset {first} — the \
                     derived-offset gate is now vacuous, because a hardcoded constant would \
                     satisfy both",
                    FrameShape::ALL[i],
                    FrameShape::ALL[j]
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Hop sequence
// ---------------------------------------------------------------------------

/// Per media stream, monotonic, never reset — and independent per egress
/// stream.
#[tokio::test]
async fn the_hop_sequence_advances_per_media_stream_not_per_connection() {
    // Two subscribers, both fed by publisher 7: two egress streams, two
    // independent counters. A per-CONNECTION counter would interleave them and
    // hand out 0,1,2,3 across the pair.
    let mut rig_ = rig(
        vec![
            egress(1, 7, MAIN_AUDIO_SLOT, 7),
            egress(2, 8, MAIN_AUDIO_SLOT, 7),
        ],
        7,
    );
    let to_seven = rig_.subscriber(7);
    let to_eight = rig_.subscriber(8);

    for sequence in 0..3 {
        forward_one(
            &mut rig_.forwarder,
            &rig_.routing.load(),
            &rig_.subscribers.load(),
            ingress(audio_datagram(sequence, 0x11)),
        );
    }

    for queue in [&to_seven, &to_eight] {
        for expected in 0..3_u32 {
            let frame = queue
                .try_pop()
                .expect("three frames per subscriber")
                .payload;
            let hop = decode_datagram(&frame).unwrap().hop_sequence();
            assert_eq!(
                hop, expected,
                "each egress stream counts from zero independently"
            );
        }
    }
}

/// A frame nobody could receive consumes no hop number.
///
/// ADR-0036 §2: the hop sequence counts **what the transmitter actually sent**,
/// so gaps mean genuine transport loss and never selection policy. If an
/// unforwardable frame burned a number, every receiver would report loss that
/// never happened and the field would measure MH's policy instead.
#[tokio::test]
async fn an_edge_with_no_local_subscriber_consumes_no_hop_number() {
    // Two edges: one deliverable, one naming a subscriber that is not
    // connected here.
    let mut rig_ = rig(
        vec![
            egress(1, 7, MAIN_AUDIO_SLOT, 7),
            egress(2, 9, MAIN_AUDIO_SLOT, 7),
        ],
        7,
    );
    let to_seven = rig_.subscriber(7);

    forward_one(
        &mut rig_.forwarder,
        &rig_.routing.load(),
        &rig_.subscribers.load(),
        ingress(audio_datagram(1, 0x22)),
    );

    // Subscriber 9 arrives afterwards and starts receiving.
    let to_nine = rig_.subscriber(9);
    forward_one(
        &mut rig_.forwarder,
        &rig_.routing.load(),
        &rig_.subscribers.load(),
        ingress(audio_datagram(2, 0x23)),
    );

    let _ = to_seven.try_pop();
    let second_to_seven = to_seven.try_pop().expect("two frames to subscriber 7");
    assert_eq!(
        decode_datagram(&second_to_seven.payload)
            .unwrap()
            .hop_sequence(),
        1
    );

    let first_to_nine = to_nine.try_pop().expect("one frame to subscriber 9");
    assert_eq!(
        decode_datagram(&first_to_nine.payload)
            .unwrap()
            .hop_sequence(),
        0,
        "the frame that could not be delivered must NOT have burned hop 0 — a gap here would \
         read to the receiver as transport loss that never happened"
    );
}

// ---------------------------------------------------------------------------
// Zero-copy fan-out and allocation
// ---------------------------------------------------------------------------

/// The ingress buffer is shared across the fan-out, and the sharing ENDS when
/// the fan-out does.
///
/// Asserted strictly **upstream of `send_datagram`**: the real transport copies
/// below the seam (wtransport prepends an HTTP/3 session-id varint into a fresh
/// buffer), so a refcount assertion taken *through* the seam would pass against
/// the test double while certifying a property production does not have.
///
/// N >= 2 deliberately: at N = 1 there is no fan-out to observe, and the arena
/// path — the one every edge but the last takes — would not be exercised at
/// all.
#[tokio::test]
async fn fan_out_shares_one_ingress_buffer_and_releases_it_when_the_fan_out_ends() {
    let mut rig_ = rig(
        vec![
            egress(1, 7, MAIN_AUDIO_SLOT, 7),
            egress(2, 8, MAIN_AUDIO_SLOT, 7),
            egress(3, 9, MAIN_AUDIO_SLOT, 7),
        ],
        7,
    );
    let queues: Vec<Arc<EgressQueue>> = [7, 8, 9]
        .into_iter()
        .map(|id| rig_.subscriber(id))
        .collect();

    let original = audio_datagram(1, 0x33);
    let retained = original.clone();
    let outcome = forward_one(
        &mut rig_.forwarder,
        &rig_.routing.load(),
        &rig_.subscribers.load(),
        ingress(original),
    );
    assert_eq!(outcome.delivered, 3, "all three edges must deliver");

    // The three egress payloads must be independently owned buffers: each
    // carries a DIFFERENT relay region for the same source frame, which is
    // impossible if they alias.
    let frames: Vec<Bytes> = queues
        .iter()
        .map(|queue| queue.try_pop().expect("one frame per subscriber").payload)
        .collect();
    for frame in &frames {
        assert_eq!(
            marker_of(frame),
            0x33,
            "every copy carries the same payload"
        );
    }

    // The retained handle is the ONLY live reference once the fan-out is done
    // and the egress frames are dropped — i.e. the fan-out did not leak a
    // reference into any queue.
    drop(frames);
    assert!(
        retained.try_into_mut().is_ok(),
        "the fan-out must release the ingress buffer when it ends; a lingering share means an \
         egress payload is aliasing the ingress buffer and a later rewrite would corrupt it"
    );
}

/// Steady-state forwarding allocates nothing per frame — **and the gate can
/// detect an allocation**.
///
/// Both halves are required. Without the proof-of-trap a gate that measures the
/// wrong thing reads green forever, which is worse than no gate: it certifies
/// the property it silently stopped checking.
///
/// Measured **after warming**, because `BytesMut::reserve` reclaims in place
/// only once the previous chunk is uniquely owned. A cold measurement either
/// fails spuriously or cannot distinguish "amortized zero" from "allocates
/// until the ring saturates".
#[tokio::test]
async fn steady_state_forwarding_allocates_nothing_and_the_gate_can_see_an_allocation() {
    let mut rig_ = rig(
        vec![
            egress(1, 7, MAIN_AUDIO_SLOT, 7),
            egress(2, 8, MAIN_AUDIO_SLOT, 7),
        ],
        7,
    );
    let queues = [rig_.subscriber(7), rig_.subscriber(8)];
    let routes = rig_.routing.load();
    let subscribers = rig_.subscribers.load();

    // Warm: fill and drain the rings so every buffer the steady state needs
    // already exists.
    for round in 0..(EGRESS_QUEUE_FRAMES * 4) {
        let marker = u8::try_from(round % 251).unwrap_or(0);
        forward_one(
            &mut rig_.forwarder,
            &routes,
            &subscribers,
            ingress(audio_datagram(u32::try_from(round).unwrap_or(0), marker)),
        );
        for queue in &queues {
            let _ = queue.try_pop();
        }
    }

    // The frame is built AND first-cloned OUTSIDE the armed window. Both
    // belong to the publisher, not to the forward path: `Bytes::from(Vec)`
    // starts on the promotable vtable and the FIRST clone allocates a shared
    // control block, so cloning inside the window would attribute one
    // publisher-side allocation per frame to MH and make this gate unpassable
    // for reasons that have nothing to do with the code under test.
    let frame = audio_datagram(9_999, 0x44);
    let measured = measure_allocations(move || {
        forward_one(&mut rig_.forwarder, &routes, &subscribers, ingress(frame));
    });
    for queue in &queues {
        let _ = queue.try_pop();
    }
    assert_eq!(
        measured, 0,
        "the steady-state forward path must not allocate per frame (ADR-0036 §11)"
    );

    // PROOF OF TRAP: the same measurement, around a deliberately allocating
    // body. If this reads zero the gate above is measuring nothing.
    let trap = measure_allocations(|| {
        let deliberate: Vec<u8> = Vec::with_capacity(4096);
        std::hint::black_box(&deliberate);
    });
    assert!(
        trap > 0,
        "the allocation gate is blind: it reported zero allocations for a body that definitely \
         allocates, so its green result above proves nothing"
    );
}

/// Run `body` with the counting allocator armed.
fn measure_allocations(body: impl FnOnce()) -> usize {
    ALLOC_COUNT.with(|count| count.set(0));
    ALLOC_ARMED.with(|armed| armed.set(true));
    body();
    ALLOC_ARMED.with(|armed| armed.set(false));
    ALLOC_COUNT.with(Cell::get)
}

// ---------------------------------------------------------------------------
// Fail-closed: the story's signature failure mode
// ---------------------------------------------------------------------------

/// No policy installed: fail closed, and say which of the three causes it was.
///
/// "Every signal green and no audio" is the bug this whole story exists to
/// prevent, so the three ways a frame reaches nobody are separately counted:
/// `no_policy` is the control plane's problem, `no_subscriber` is MC's
/// assignment, `no_local_subscriber` is this handler's connection state.
#[tokio::test]
async fn a_frame_for_a_meeting_with_no_policy_is_dropped_as_no_policy() {
    // DELIBERATELY NOT the shared rig: `LoopbackRig` always installs a policy,
    // and an EMPTY installed policy is a different state from no policy at all
    // — it yields `no_subscriber`, not `no_policy`. Those are the two halves of
    // this file's central distinction (whose problem is it: the control plane's,
    // or MC's assignment?), so the fixture has to be able to express both.
    let handles = Arc::new(resolve_media_handles());
    let table = RoutingTable::new();
    let registry = LocalSubscribers::new();
    let mut forwarder = ConnectionForwarder::new(
        MeetingKey::new(MEETING),
        sender(7),
        handles,
        1.0,
        NOMINAL_AUDIO_FRAME_BYTES,
    );

    let outcome = forward_one(
        &mut forwarder,
        &table.load(),
        &registry.load(),
        ingress(audio_datagram(1, 0x55)),
    );

    assert_eq!(outcome.delivered, 0);
    assert_eq!(
        outcome.rejected.map(|reason| reason.as_str()),
        Some("no_policy"),
        "there is NO echo-to-sender fallback: loopback is an installed policy edge or it does \
         not happen"
    );
}

/// A policy exists but this publisher feeds no edge.
#[tokio::test]
async fn a_publisher_with_no_edges_is_dropped_as_no_subscriber() {
    // Policy for a DIFFERENT publisher in the same meeting.
    let mut rig_ = rig(vec![loopback_egress(1, 8, MAIN_AUDIO_SLOT)], 7);
    let _queue = rig_.subscriber(8);

    let outcome = forward_one(
        &mut rig_.forwarder,
        &rig_.routing.load(),
        &rig_.subscribers.load(),
        ingress(audio_datagram(1, 0x66)),
    );

    assert_eq!(
        outcome.rejected.map(|reason| reason.as_str()),
        Some("no_subscriber")
    );
}

/// An edge naming a subscriber who is not connected here.
#[tokio::test]
async fn an_edge_naming_an_absent_subscriber_is_dropped_as_no_local_subscriber() {
    let mut rig_ = rig(vec![egress(1, 9, MAIN_AUDIO_SLOT, 7)], 7);

    let outcome = forward_one(
        &mut rig_.forwarder,
        &rig_.routing.load(),
        &rig_.subscribers.load(),
        ingress(audio_datagram(1, 0x77)),
    );

    assert_eq!(outcome.delivered, 0);
    assert_eq!(outcome.dropped, 1);
    assert_eq!(
        outcome.rejected, None,
        "the edge RESOLVED — this is a delivery failure, not a routing one, and conflating them \
         sends a responder to the wrong team"
    );
}

// ---------------------------------------------------------------------------
// Cross-meeting
// ---------------------------------------------------------------------------

/// A `sender_id` valid in one meeting resolves to nothing in another.
///
/// Both arms, because the negative arm alone also passes for an implementation
/// that resolves nothing anywhere. Loopback is one meeting with one
/// participant, so only a deliberately multi-meeting fixture can tell a
/// correct implementation from a cross-tenant-leaking one.
#[tokio::test]
async fn a_sender_valid_in_one_meeting_forwards_nothing_in_another() {
    // DELIBERATELY NOT `common::media_rig::LoopbackRig`: that rig is
    // single-meeting by construction, and the whole subject here is one live
    // routing table and one live subscriber registry holding TWO meetings at
    // once. A per-meeting rig would give each arm its own table, and two
    // separate tables cannot demonstrate non-resolution across them — the test
    // would pass for an implementation that leaks.
    let handles = Arc::new(resolve_media_handles());
    let table = RoutingTable::new();
    for (meeting, publisher) in [("meeting-a", 5_u32), ("meeting-b", 6_u32)] {
        let request = register_request(meeting, 1, vec![loopback_egress(1, publisher, 0)]);
        let policy = MeetingPolicy::from_request(&request, &PolicyLimits::default()).unwrap();
        table.install(&policy);
    }
    let registry = LocalSubscribers::new();
    let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
    registry.register(
        MeetingKey::new("meeting-a"),
        sender(5),
        "conn-fwd-a",
        &queue,
    );

    // Positive arm: publisher 5 forwards inside meeting A.
    let mut in_a = ConnectionForwarder::new(
        MeetingKey::new("meeting-a"),
        sender(5),
        Arc::clone(&handles),
        1.0,
        NOMINAL_AUDIO_FRAME_BYTES,
    );
    let delivered = forward_one(
        &mut in_a,
        &table.load(),
        &registry.load(),
        ingress(audio_datagram(1, 0x88)),
    );
    assert_eq!(delivered.delivered, 1, "sender 5 must forward in meeting A");

    // Negative arm: the same ordinal in meeting B reaches meeting A's
    // subscriber through nothing.
    let mut in_b = ConnectionForwarder::new(
        MeetingKey::new("meeting-b"),
        sender(5),
        Arc::clone(&handles),
        1.0,
        NOMINAL_AUDIO_FRAME_BYTES,
    );
    let crossed = forward_one(
        &mut in_b,
        &table.load(),
        &registry.load(),
        ingress(audio_datagram(2, 0x99)),
    );
    assert_eq!(
        crossed.delivered, 0,
        "sender 5 is meeting A's ordinal; forwarding it in meeting B is cross-tenant leakage"
    );
    assert_eq!(
        queue.depth(),
        1,
        "meeting A's subscriber received only its own meeting's frame"
    );
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

/// A well-formed frame with appended bytes is rejected, not relayed.
///
/// QUIC preserves datagram boundaries, so a tail is never a transport
/// artifact: some endpoint wrote bytes that no signature covers. Only
/// `decode_datagram` rejects this — the rewrite entry point tolerates trailing
/// bytes because it also serves the stream path — which is the concrete reason
/// MH decodes as well as rewrites.
#[tokio::test]
async fn a_datagram_with_trailing_bytes_is_rejected_rather_than_relayed() {
    let mut rig_ = rig(vec![loopback_egress(1, 7, MAIN_AUDIO_SLOT)], 7);
    let queue = rig_.subscriber(7);

    let mut tampered = audio_datagram(1, 0xAA).to_vec();
    tampered.extend_from_slice(b"appended-by-someone-downstream");

    let outcome = forward_one(
        &mut rig_.forwarder,
        &rig_.routing.load(),
        &rig_.subscribers.load(),
        ingress(Bytes::from(tampered)),
    );

    assert_eq!(outcome.delivered, 0);
    assert_eq!(
        outcome.rejected, None,
        "a codec reject is counted under the shared codec vocabulary, not an MH-local reason"
    );
    assert_eq!(
        queue.depth(),
        0,
        "nothing may be relayed from a rejected frame"
    );
}

/// Frame-age deadlines run on the DEADLINE clock, so `start_paused` drives
/// them; latency measurement runs on the MEASUREMENT clock and is unaffected.
///
/// The split is the assertion: a measurement taken on `tokio::time` reads ~0
/// under pause (silently wrong) and a deadline taken on `std::time` never
/// responds to `advance` (hangs).
#[tokio::test(start_paused = true)]
async fn frame_age_uses_the_deadline_clock_while_measurement_uses_real_time() {
    let mut rig_ = rig(vec![loopback_egress(1, 7, MAIN_AUDIO_SLOT)], 7);
    let queue = rig_.subscriber(7);

    let before = Instant::now();
    // Advancing tokio's clock by an hour must not move the measurement clock.
    tokio::time::advance(std::time::Duration::from_secs(3600)).await;
    let elapsed_real = before.elapsed();
    assert!(
        elapsed_real < std::time::Duration::from_secs(1),
        "latency MEASUREMENT must be on std::time — real elapsed, never paused"
    );

    forward_one(
        &mut rig_.forwarder,
        &rig_.routing.load(),
        &rig_.subscribers.load(),
        ingress(audio_datagram(1, 0xBB)),
    );
    assert_eq!(
        queue.depth(),
        1,
        "forwarding is unaffected by a paused runtime"
    );
}
