//! ADR-0036 §10 **Tier-1b** back-pressure gates.
//!
//! "Frames past the bound are reset or dropped per assigned policy rather than
//! queued unboundedly. Written against *the declared bound, whatever it is*."
//! Every assertion below is written against `config::EGRESS_QUEUE_FRAMES`, not
//! against the number it happens to hold.
//!
//! # The three failures this file exists to separate
//!
//! An overflow counter that increments plus a queue that stays within its bound
//! is satisfied by all three of these, and only one of them is correct:
//!
//! 1. **Correct**: MH sheds its oldest backlog and keeps delivering the
//!    freshest frames — current audio with gaps, not growing delay.
//! 2. **Wedged**: the queue fills, nothing drains, the overflow counter climbs,
//!    the transport is handed nothing so it never refuses, both counters look
//!    healthy and MH has silently stopped forwarding. Caught by the
//!    forward-progress gate.
//! 3. **Drop-newest**: sheds the same number of frames, increments the same
//!    counter, makes the same forward progress — and delivers *stale* audio.
//!    Separable only by frame identity. Caught by the drop-direction gate.
//!
//! A trip in any of these is a REAL finding, not a flake to loosen.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

#[path = "common/mod.rs"]
mod common;

use common::media_frame::{audio_datagram, marker_of};
use common::media_rig::LoopbackRig;

use ::common::observability::testing::MetricAssertion;
use mh_service::config::EGRESS_QUEUE_FRAMES;
use mh_service::media::forward::{forward_one, EgressQueue, IngressFrame};
use mh_service::media::ingress::run_egress;
use mh_service::media::MediaTaskContext;
use mh_service::routing::RoutingTable;
use mh_service::session::LocalSubscribers;
use mh_test_utils::media_policy::loopback_egress;
use mh_test_utils::transport_shim::LossDelayTransport;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const MEETING: &str = "meeting-backpressure";
const PUBLISHER: u32 = 7;
const SLOT: u32 = 0;

/// The rig plus the process-state the egress loop needs.
///
/// The loopback policy, the routing table, the subscriber registry and the
/// forwarder come from `common::media_rig::LoopbackRig`, shared with the other
/// two media binaries; only the egress-loop context is local, because only this
/// file runs task C.
struct Rig {
    inner: LoopbackRig,
    egress_queue: Arc<EgressQueue>,
    context: Arc<MediaTaskContext>,
}

fn rig() -> Rig {
    let inner = LoopbackRig::new(
        MEETING,
        PUBLISHER,
        vec![loopback_egress(1, PUBLISHER, SLOT)],
        1.0,
    );
    let egress_queue = inner.subscriber(PUBLISHER);
    let context = Arc::new(MediaTaskContext {
        // The egress loop reads neither of these — it drains a queue and sends
        // — so empty registries are the honest shape rather than a shortcut.
        routing: Arc::new(RoutingTable::new()),
        subscribers: Arc::new(LocalSubscribers::new()),
        handles: Arc::clone(&inner.handles),
    });

    Rig {
        inner,
        egress_queue,
        context,
    }
}

impl Rig {
    /// Forward one frame carrying `marker` as its payload identity.
    fn forward(&mut self, sequence: u32, marker: u8) {
        forward_one(
            &mut self.inner.forwarder,
            &self.inner.routing.load(),
            &self.inner.subscribers.load(),
            IngressFrame {
                payload: audio_datagram(sequence, marker),
                received_at: Instant::now(),
            },
        );
    }
}

/// How far past the declared bound each pressure test drives.
///
/// Derived from the bound, never typed: if `EGRESS_QUEUE_FRAMES` changes, the
/// overshoot changes with it and the assertions stay meaningful.
const OVERSHOOT: usize = 5;

/// The subscriber is stalled (task C is not running), so the bounded queue
/// sheds — it does not grow.
#[tokio::test]
async fn frames_past_the_declared_bound_are_dropped_and_counted_never_queued() {
    let snapshot = MetricAssertion::snapshot();
    let mut rig = rig();

    let total = EGRESS_QUEUE_FRAMES + OVERSHOOT;
    for round in 0..total {
        rig.forward(
            u32::try_from(round).unwrap(),
            u8::try_from(round % 251).unwrap(),
        );
    }

    assert_eq!(
        rig.egress_queue.depth(),
        EGRESS_QUEUE_FRAMES,
        "queue depth must never exceed the DECLARED bound (config::EGRESS_QUEUE_FRAMES)"
    );
    assert_eq!(
        rig.egress_queue.capacity(),
        EGRESS_QUEUE_FRAMES,
        "the queue must be built from the declared bound, not a literal"
    );

    snapshot
        .counter("mh_media_frames_dropped_total")
        .with_labels(&[
            ("reason", "egress_queue_overflow"),
            ("direction", "egress"),
            ("key_custody", "operator"),
        ])
        .assert_delta(u64::try_from(OVERSHOOT).unwrap());
}

/// Under overflow the subscriber receives the FRESH TAIL, not a stale
/// contiguous head.
///
/// This is why the queue exists at all: under a slow subscriber MH sheds
/// backlog and delivers the freshest frames, which makes the application queue
/// a **latency ceiling rather than a buffer** (ADR-0036 §1: "a realtime path
/// must prefer loss to unbounded latency"). A drop-newest implementation
/// passes the overflow count and the forward-progress gate and fails only here.
#[tokio::test]
async fn under_overflow_the_subscriber_receives_the_fresh_tail_not_a_stale_head() {
    let mut rig = rig();
    let transport = Arc::new(LossDelayTransport::new());
    let total = EGRESS_QUEUE_FRAMES + OVERSHOOT;

    // Identities 0..total while the subscriber is stalled: C is not spawned
    // yet, so nothing drains and the ring must shed the head.
    for round in 0..total {
        rig.forward(
            u32::try_from(round).unwrap(),
            u8::try_from(round % 251).unwrap(),
        );
    }

    // Release the subscriber.
    let cancel = CancellationToken::new();
    let egress = tokio::spawn(run_egress(
        Arc::clone(&transport),
        Arc::clone(&rig.egress_queue),
        Arc::clone(&rig.context),
        cancel.clone(),
    ));
    wait_until(|| transport.delivered_count() >= u64::try_from(EGRESS_QUEUE_FRAMES).unwrap()).await;
    cancel.cancel();
    let _ = egress.await;

    assert!(
        !transport.capture_truncated(),
        "the capture must be complete for an identity assertion — a truncated capture makes \
         this test pass or fail for reasons unrelated to drop direction"
    );

    let delivered: Vec<u8> = transport
        .delivered_datagrams()
        .iter()
        .map(|frame| marker_of(frame))
        .collect();
    let expected: Vec<u8> = (OVERSHOOT..total)
        .map(|round| u8::try_from(round % 251).unwrap())
        .collect();
    assert_eq!(
        delivered, expected,
        "drop-OLDEST: the subscriber must receive the freshest {EGRESS_QUEUE_FRAMES} frames. A \
         stale contiguous head here means drop-newest, which sheds the same number of frames, \
         increments the same counter, and delivers growing delay instead of current audio"
    );
}

/// Sustained pressure keeps making forward progress.
///
/// The trap: a wedged queue (fills, never drains) increments the overflow
/// counter, hands the transport nothing so it never refuses, leaves both
/// counters looking healthy — and MH has stopped forwarding. Only a *rising*
/// delivered count separates that from correct shedding.
#[tokio::test]
async fn sustained_pressure_keeps_delivering() {
    let mut rig = rig();
    let transport = Arc::new(LossDelayTransport::new());
    let cancel = CancellationToken::new();
    let egress = tokio::spawn(run_egress(
        Arc::clone(&transport),
        Arc::clone(&rig.egress_queue),
        Arc::clone(&rig.context),
        cancel.clone(),
    ));

    let mut observed = Vec::new();
    for burst in 0..4 {
        for round in 0..(EGRESS_QUEUE_FRAMES * 2) {
            rig.forward(
                u32::try_from(burst * 100 + round).unwrap(),
                u8::try_from(round % 251).unwrap(),
            );
        }
        let target = transport.delivered_count() + 1;
        wait_until(|| transport.delivered_count() >= target).await;
        observed.push(transport.delivered_count());
    }

    cancel.cancel();
    let _ = egress.await;

    for pair in observed.windows(2) {
        assert!(
            pair[1] > pair[0],
            "delivery must keep RISING under sustained pressure; a flat count with a climbing \
             overflow counter is a wedged queue wearing back-pressure's clothes: {observed:?}"
        );
    }
}

/// The application bound trips **before** the transport ceiling, so the
/// transport never refuses.
///
/// ADR-0036 §1's ordering premise, exercised rather than assumed. The shim's
/// `set_send_capacity` is the ceiling proxy — the only `WouldBlock` producer in
/// the tree — and the pressure is driven past *that* capacity, not merely past
/// `EGRESS_QUEUE_FRAMES`, so the zero-assertion is a real trap: if the
/// application queue did not shed first, the transport would be handed more
/// datagrams than its ceiling admits and `transport_send_refused` would fire.
#[tokio::test]
async fn the_application_bound_trips_before_the_transport_ceiling() {
    let snapshot = MetricAssertion::snapshot();
    let mut rig = rig();
    let transport = Arc::new(LossDelayTransport::new());
    // The ceiling: exactly the application bound. Everything the app queue
    // sheds is a datagram the transport is never offered.
    transport.set_send_capacity(u64::try_from(EGRESS_QUEUE_FRAMES).unwrap());

    // Far more pressure than the ceiling admits.
    let total = EGRESS_QUEUE_FRAMES * 8;
    for round in 0..total {
        rig.forward(
            u32::try_from(round).unwrap(),
            u8::try_from(round % 251).unwrap(),
        );
    }

    let cancel = CancellationToken::new();
    let egress = tokio::spawn(run_egress(
        Arc::clone(&transport),
        Arc::clone(&rig.egress_queue),
        Arc::clone(&rig.context),
        cancel.clone(),
    ));
    wait_until(|| transport.egress_accepted() >= u64::try_from(EGRESS_QUEUE_FRAMES).unwrap()).await;
    cancel.cancel();
    let _ = egress.await;

    assert_eq!(
        transport.refused_for_capacity(),
        0,
        "the transport must never refuse: the application queue is the observable shed, and if \
         it stops binding first the loss moves below the seam where MH cannot count it"
    );
    snapshot
        .counter("mh_media_frames_dropped_total")
        .with_labels(&[
            ("reason", "transport_send_refused"),
            ("direction", "egress"),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);
}

/// Poll `condition` until it holds, failing loudly rather than hanging.
///
/// A liveness backstop, never a timing assertion: no test here passes or fails
/// on how long something took.
async fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "the rig made no progress within 5s — this is a broken rig or a wedged pipeline, \
             not a slow machine"
        );
        tokio::task::yield_now().await;
    }
}
