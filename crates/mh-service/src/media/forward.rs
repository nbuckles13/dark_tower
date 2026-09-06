//! `forward_one` — the ADR-0036 §10 Tier-1a pure function.
//!
//! One frame in, N egress queue pushes out. No QUIC, no crypto, no clock beyond
//! a monotonic read, no `.await`, no allocation on the steady-state path, and
//! no metric macro: every observation goes through a handle resolved before the
//! loop started.
//!
//! # What this function may read
//!
//! `payload_length` and nothing else (ADR-0036 §7 keeps MH type-blind and §4
//! keeps it keyless). Nothing here calls `wrapped_transmit_key()`,
//! `expose_wrapped_key()` or `expose_wrap_tag()`, and nothing inspects the
//! payload's first 8 bytes — the `SFrame` key id. `rewrite_relay_region` reads
//! the key-bearing *flag* internally to derive its offset and never hands MH
//! the key material; reading a flag is not parsing the field, so the
//! credential-leak scope trigger in `docs/TODO.md` §Media Path Obligations does
//! not fire. Recorded so the next reader can check the claim rather than trust
//! it.
//!
//! # Decode THEN rewrite, and why that is two parses on purpose
//!
//! MH calls `decode_datagram` on ingress **and** `rewrite_relay_region` for the
//! rewrite — the two fallible codec entry points a relay uses. Both invoke the
//! same `parse_layout`, so this is two parses and not a second *parser*; the
//! codec's "two parsers diverge on exactly the malformed ones an attacker
//! constructs" invariant is intact. The cost is ~100 ns and zero allocation,
//! and it buys three things:
//!
//! 1. `decode_datagram` is the only entry point that rejects `trailing_bytes`.
//!    QUIC preserves datagram boundaries, so a tail is never a transport
//!    artifact: some endpoint wrote bytes no signature covers. Without this
//!    call MH would relay them.
//! 2. It makes every codec reject reason reachable from MH's audio ingress,
//!    which is what the shared `reason` vocabulary assumes.
//! 3. It resolves the entry-point ambiguity **structurally**. Every codec token
//!    MH emits comes from `decode_datagram`, i.e. is a sender-side condition; a
//!    failure from `rewrite_relay_region` on an already-decoded frame is not a
//!    sender fault at all, it is an MH bug, and it carries its own
//!    `relay_rewrite_failed` token which should read zero forever. No
//!    `entry_point` label is added.
//!
//! You cannot have both "the rewrite derives its own offset" and "only one
//! parse": the single-parse alternative is decode-then-pass-the-offset, which
//! is the cached-offset hazard with a shorter cache lifetime. Two parses is the
//! price of the derived-offset guarantee, and that guarantee is not optional —
//! see [`rewrite`] below.

use crate::media::forwarder::{ConnectionForwarder, ForwardParts};
use crate::media::queue::{depth_as_gauge, SharedQueue};
use crate::observability::metrics::{MediaDirection, MediaDropReason, MediaLatencyPhase};
use crate::observability::per_frame_trace;
use crate::routing::{EgressEdge, RoutingSnapshot};
use crate::session::LocalSubscriberSnapshot;
use bytes::{Bytes, BytesMut};
use core::fmt;
use media_protocol::codec::{decode_datagram, rewrite_relay_region};
use std::time::Instant;

/// A datagram accepted off the transport, waiting to be forwarded.
///
/// `received_at` is on `std::time::Instant` — the MEASUREMENT clock, real
/// elapsed time, never paused. Deadlines use `tokio::time` instead; see
/// [`crate::media::caps`] for the split and why the two must not cross.
///
/// `Debug` is hand-rolled and redacts `payload`; see the impl below.
#[derive(Clone)]
pub struct IngressFrame {
    /// The opaque signed blob: publisher region, payload and signature.
    pub payload: Bytes,
    /// When the transport handed it to us. MEASUREMENT clock.
    pub received_at: Instant,
}

/// A rewritten datagram waiting for one subscriber's transport.
///
/// `Debug` is hand-rolled and redacts `payload`; see the impl below.
#[derive(Clone)]
pub struct EgressFrame {
    /// The rewritten blob. Only the 6-byte relay region differs from ingress.
    pub payload: Bytes,
    /// Ingress arrival. MEASUREMENT clock.
    pub received_at: Instant,
    /// When this frame was pushed onto the egress queue. MEASUREMENT clock.
    pub queued_at: Instant,
    /// Whether this frame's latency is observed. Decided ONCE per ingress
    /// frame so all four phases of one frame are observed together — sampling
    /// each phase independently would make the decomposition incomparable with
    /// the total.
    pub sampled: bool,
}

/// Hand-rolled so `payload` cannot render.
///
/// **A `#[derive(Debug)]` here would be the voice-activity trace by the one
/// route the directory-scoped macro deny structurally cannot see.** These types
/// are `pub` in a `pub mod`, `bytes::Bytes` prints its contents, and the
/// formatting would happen in a *sibling* — a `debug!(?frame)` in
/// `webtransport/connection.rs`, a teardown path, or any `#[derive(Debug)]`
/// container that transitively holds one — so the walker in
/// `tests/media_metrics_integration.rs` would report clean while the `SFrame`
/// ciphertext and the exact per-frame size shipped.
///
/// Same defect class and same remedy as `media_protocol::codec::MediaFrameParts`
/// and `media_protocol::frame::WrappedTransmitKey`, whose impls carry the
/// reasoning in full — cited rather than restated, so the argument keeps one
/// home. The redaction discloses the length in the rendered string, which is
/// deliberate and matches that precedent: a hand-rolled impl is a site a
/// reviewer reads, whereas a derive is invisible.
impl fmt::Debug for IngressFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IngressFrame")
            .field(
                "payload",
                &format_args!("<redacted {} B>", self.payload.len()),
            )
            .field("received_at", &self.received_at)
            .finish()
    }
}

/// Hand-rolled for the same reason as [`IngressFrame`]'s.
impl fmt::Debug for EgressFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EgressFrame")
            .field(
                "payload",
                &format_args!("<redacted {} B>", self.payload.len()),
            )
            .field("received_at", &self.received_at)
            .field("queued_at", &self.queued_at)
            .field("sampled", &self.sampled)
            .finish()
    }
}

/// One subscriber connection's bounded egress queue.
pub type EgressQueue = SharedQueue<EgressFrame>;

/// What happened to one ingress frame. Returned for tests and for the calling
/// loop; the counters have already been incremented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardOutcome {
    /// Egress queues this frame was pushed onto.
    pub delivered: usize,
    /// Edges that resolved but could not be delivered to.
    pub dropped: usize,
    /// The single reason a frame that reached no edge at all was dropped.
    pub rejected: Option<MediaDropReason>,
}

impl ForwardOutcome {
    const fn rejected(reason: MediaDropReason) -> Self {
        Self {
            delivered: 0,
            dropped: 0,
            rejected: Some(reason),
        }
    }
}

/// Forward one datagram to every egress edge its publisher feeds.
///
/// # Fail-closed, with a distinct token per cause
///
/// There is **no** "no edges, so echo it back to the sender" fallback. Loopback
/// is an installed policy edge from `RegisterMeeting` or it does not happen.
/// The three ways a frame reaches nobody are separately counted because they
/// have three different owners: `no_policy` is the control plane's, and its
/// remedy is upstream of this handler; `no_subscriber` is MC's assignment; and
/// `no_local_subscriber` is this handler's own connection state.
///
/// # The routing table is re-read per frame and nothing is cached
///
/// `RoutingTable::load()` is a plain atomic returning an `Arc` with no guard
/// lifetime, so this costs one `HashMap` probe on an `Arc<str>` key per frame —
/// no allocation, no lock, no await held. Caching the edge list would be a
/// server-mute bypass with a staleness window: ADR-0036 §7 enforces server mute
/// and participant removal *through this lookup*, so an edge list that outlives
/// a policy change keeps forwarding a muted publisher.
///
/// The per-frame `meetings.get(meeting)` is **not** a hash-collision denial-of-
/// service vector: the `MeetingKey` is bound at spawn from the JWT-gated accept
/// path and nothing datagram-carried influences it, so an attacker cannot
/// choose the key being hashed.
pub fn forward_one(
    forwarder: &mut ConnectionForwarder,
    snapshot: &RoutingSnapshot,
    subscribers: &LocalSubscriberSnapshot,
    frame: IngressFrame,
) -> ForwardOutcome {
    // MEASUREMENT clock (`std::time::Instant`): real elapsed time. A read here
    // on `tokio::time::Instant` would report ~0 under `start_paused`, silently.
    let popped_at = Instant::now();
    let sampled = forwarder.should_sample();
    if sampled {
        observe(
            forwarder,
            MediaLatencyPhase::ReceiveBuffer,
            popped_at.saturating_duration_since(frame.received_at),
        );
    }

    // Decode first: this is the only entry point that rejects `trailing_bytes`,
    // and the only place a codec token may originate in MH. The borrow of
    // `payload` ends with this block so the buffer can be rewritten below.
    if let Err(error) = decode_datagram(&frame.payload) {
        // `RejectReason::as_str()` and nothing else crosses out of `media/`.
        // `DecodeError`'s `Display` carries per-frame scalars — declared and
        // available payload sizes — and the time-ordered sequence of per-frame
        // sizes for one stream is the voice-activity trace (ADR-0036 §11).
        if let Some(counter) = forwarder.handles().codec_dropped(error.reason()) {
            counter.increment(1);
        }
        // Size and a bounded token, never `DecodeError`'s `Display` — that
        // carries the declared and available per-frame sizes together, and a
        // sibling formatting it would rebuild the voice-activity trace by a
        // route the directory-scoped deny cannot see. Compiled out unless the
        // `per-frame-trace` feature is on, which is a `compile_error!` in a
        // release build.
        per_frame_trace::record(frame.payload.len(), error.reason().as_str());
        return ForwardOutcome {
            delivered: 0,
            dropped: 0,
            rejected: None,
        };
    }

    let payload = frame.payload;
    let outcome = {
        let mut parts = forwarder.parts();
        let mut delivered = 0_usize;
        let mut dropped = 0_usize;

        // Single-pass fan-out. Each edge is held back one iteration so the LAST
        // edge can consume the ingress buffer itself (zero copy) while every
        // earlier edge takes an arena copy — without a second traversal and
        // without knowing N in advance. For the loopback N=1 shape the single
        // edge IS the last edge, so the whole path is zero-copy.
        let mut pending: Option<PendingEdge> = None;
        let visited = snapshot.for_each_source(parts.meeting, parts.sender, |edge| {
            if let Some(previous) = pending.take() {
                match deliver(
                    &mut parts,
                    subscribers,
                    previous,
                    FrameSource::Borrowed(&payload),
                    popped_at,
                    sampled,
                ) {
                    Delivery::Delivered => delivered += 1,
                    Delivery::Dropped => dropped += 1,
                }
            }
            pending = Some(PendingEdge::from(edge));
        });

        if visited == 0 {
            // Only reached on the drop path, so the extra probe costs the happy
            // path nothing. The distinction is worth a probe: "MC never
            // programmed this handler" and "MC programmed it and this publisher
            // feeds nothing" are different incidents with different owners.
            let reason = if snapshot.routes_for(parts.meeting).is_none() {
                MediaDropReason::NoPolicy
            } else {
                // Ingested successfully, delivered nowhere: the ingress
                // attempts sum stays whole (`forwarded{ingress} +
                // dropped{ingress reasons}` = every datagram received) while
                // the drop is attributed to the egress-side cause it has.
                parts
                    .handles
                    .forwarded(MediaDirection::Ingress)
                    .increment(1);
                MediaDropReason::NoSubscriber
            };
            parts.handles.dropped(reason).increment(1);
            per_frame_trace::record(payload.len(), reason.as_str());
            return ForwardOutcome::rejected(reason);
        }

        parts
            .handles
            .forwarded(MediaDirection::Ingress)
            .increment(1);
        per_frame_trace::record(payload.len(), per_frame_trace::FORWARDED);

        if let Some(last) = pending.take() {
            match deliver(
                &mut parts,
                subscribers,
                last,
                FrameSource::Owned(payload),
                popped_at,
                sampled,
            ) {
                Delivery::Delivered => delivered += 1,
                Delivery::Dropped => dropped += 1,
            }
        }

        ForwardOutcome {
            delivered,
            dropped,
            rejected: None,
        }
    };

    if sampled {
        observe(
            forwarder,
            MediaLatencyPhase::Processing,
            Instant::now().saturating_duration_since(popped_at),
        );
    }

    outcome
}

/// The fields of an [`EgressEdge`] the fan-out needs, copied out so the closure
/// does not hold a borrow of the snapshot across the delivery.
#[derive(Debug, Clone, Copy)]
struct PendingEdge {
    egress_stream_id: u32,
    subscriber: crate::routing::SubscriberRef,
}

impl From<&EgressEdge> for PendingEdge {
    fn from(edge: &EgressEdge) -> Self {
        Self {
            egress_stream_id: edge.egress_stream_id,
            subscriber: edge.subscriber,
        }
    }
}

/// Where this edge's bytes come from.
///
/// `Owned` is used for exactly one edge per frame — the last — and is what
/// makes the loopback path allocation-free and copy-free end to end.
enum FrameSource<'a> {
    Borrowed(&'a Bytes),
    Owned(Bytes),
}

enum Delivery {
    Delivered,
    Dropped,
}

/// Rewrite for one edge and push onto that subscriber's egress queue.
fn deliver(
    parts: &mut ForwardParts<'_>,
    subscribers: &LocalSubscriberSnapshot,
    edge: PendingEdge,
    source: FrameSource<'_>,
    popped_at: Instant,
    sampled: bool,
) -> Delivery {
    let Some(queue) = subscribers.egress_queue(parts.meeting, edge.subscriber.sender) else {
        // The subscriber this edge names has no connection on this handler. No
        // hop number is consumed: the routing lookup never reached the rewrite,
        // so this is an intentionally unforwarded frame and it must not leave a
        // gap the subscriber would read as transport loss (ADR-0036 §2).
        parts
            .handles
            .dropped(MediaDropReason::NoLocalSubscriber)
            .increment(1);
        return Delivery::Dropped;
    };

    let hop = parts.take_hop_for(edge.egress_stream_id);
    let slot = edge.subscriber.slot.get();

    let mut buffer = match source {
        // Zero copy: the ingress buffer is uniquely owned, so it becomes a
        // mutable buffer in place with no allocation and no memcpy.
        FrameSource::Owned(bytes) => match bytes.try_into_mut() {
            Ok(mutable) => mutable,
            // The refcount was not 1 — the arena path is still correct, only
            // slower. Falling back rather than asserting: whether
            // `wtransport::Datagram::payload()` yields a uniquely-owned `Bytes`
            // is a property of the vendor, and correctness must not depend on
            // it.
            Err(shared) => copy_into_arena(parts.arena, &shared),
        },
        FrameSource::Borrowed(bytes) => copy_into_arena(parts.arena, bytes),
    };

    if rewrite(&mut buffer, slot, hop).is_err() {
        // A rewrite failure on an ALREADY-DECODED frame is an MH bug, never a
        // sender fault. Its own token, which should read zero forever.
        parts
            .handles
            .dropped(MediaDropReason::RelayRewriteFailed)
            .increment(1);
        return Delivery::Dropped;
    }

    let queued_at = Instant::now();
    let (evicted, depth) = queue.push(EgressFrame {
        payload: buffer.freeze(),
        received_at: popped_at,
        queued_at,
        sampled,
    });
    parts
        .handles
        .egress_queue_depth()
        .set(depth_as_gauge(depth));

    if evicted.is_some() {
        // Expected load shedding under a slow subscriber, not a fault. The shed
        // frame HAS consumed a hop number, which is correct: the subscriber
        // sees a gap, and a gap here is genuine loss.
        //
        // This counter observes MH's own queue and nothing else. quinn evicts
        // silently beneath it and that eviction is structurally invisible here;
        // nothing may describe this counter as observing transport
        // back-pressure.
        parts
            .handles
            .dropped(MediaDropReason::EgressQueueOverflow)
            .increment(1);
    }

    Delivery::Delivered
}

/// Copy `source` into the per-connection arena and split off an
/// independently-owned chunk.
///
/// The arena is empty on entry (every chunk previously put into it was split
/// off), so the copied bytes sit at the front and `split_to` yields exactly
/// them.
fn copy_into_arena(arena: &mut BytesMut, source: &Bytes) -> BytesMut {
    arena.reserve(source.len());
    arena.extend_from_slice(source);
    arena.split_to(source.len())
}

/// The relay-region rewrite, called through `media_protocol` on every frame.
///
/// # The offset is DERIVED PER FRAME and must never become a constant
///
/// The publisher region ends at `ext_start + ext_len`, which moves with the
/// key-bearing flag (a 50-byte wrapped transmit key) and with the TLV extension
/// length. **Both movers are live on audio**: ADR-0036 §4's in-band key
/// carriage sets the key-bearing flag on audio frames, and §7's salience TLV —
/// the audio selector input — *is* the extension region. A hardcoded offset
/// therefore lets one participant's extension byte make the relay overwrite
/// three bytes of another participant's **signed** publisher region, so every
/// receiver's Ed25519 verification fails and the victim goes silent. One forged
/// byte, remote, unauthenticated by MH.
///
/// # Why this may never be "optimised" into a cached offset
///
/// Normally a structural guarantee and a detector are redundant with each
/// other. Here the structural guarantee is the **only** control. If a
/// relay-side offset bug corrupted the signed publisher region, every receiver
/// would count `signature_invalid` and **MH would count nothing at all** —
/// correctly so, because the layer bar forbids MH from emitting crypto-layer
/// reasons on the ground that MH cannot observe them. A relay-side corruption
/// defect therefore has no MH-side signal by construction, and the absence of a
/// drop signal is not evidence that forwarding is healthy.
///
/// That is the argument which has to defeat a future *"we already parse twice,
/// let us cache the offset"* performance suggestion, and whoever makes that
/// suggestion will not have read the thread it came from.
fn rewrite(
    buffer: &mut BytesMut,
    slot: u16,
    hop: u32,
) -> Result<(), media_protocol::codec::DecodeError> {
    rewrite_relay_region(buffer, slot, hop)
}

/// Observe one phase through a pre-resolved handle.
fn observe(
    forwarder: &ConnectionForwarder,
    phase: MediaLatencyPhase,
    elapsed: std::time::Duration,
) {
    forwarder
        .handles()
        .latency(phase)
        .record(elapsed.as_secs_f64());
}
