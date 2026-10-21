//! Per-publisher-connection forward state, and the hop-sequence counter type.
//!
//! Holds the fan-out arena, the pre-resolved metric handles and the latency
//! sampler. The hop COUNTERS live per egress edge in
//! `crate::media::edge_runtime::EdgeRuntime` (story 3 MH-A2), not here: the
//! hop is per (SUBSCRIBER connection, media stream), and a publisher-owned
//! counter restarts whenever the edge's source changes. No setup and no
//! teardown live here: constructing one of these is the caller's job
//! (`crate::webtransport::connection`), and ADR-0036 §11 keeps lifecycle a
//! **sibling** of the media directory.

use crate::config::EGRESS_QUEUE_FRAMES;
use crate::media::sampler::LatencySampler;
use crate::observability::metrics::MediaMetricHandles;
use crate::routing::{MeetingKey, SenderId};
use bytes::BytesMut;
use media_protocol::frame::HOP_SEQUENCE_FIELD_BYTES;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// The relay's per-(subscriber connection, media stream) downlink send count.
///
/// # MH runtime state, not wire format
///
/// This type lands here rather than in `media-protocol` deliberately: the hop
/// counter is **MH-generated and never publisher-set**, and the wire contract is
/// already complete without it — `frame::HOP_SEQUENCE_FIELD_BYTES` gives the
/// width and the codec's rewrite entry points take the value as a `u32`
/// parameter. There is no cross-language reason to widen the codec's surface
/// for a counter only a relay owns.
///
/// # There is deliberately no reset API
///
/// Monotonic advance only, wrapping on overflow. This is emphatically **not** a
/// generic counter to be shared with `stream_sequence`: that field is the AEAD
/// nonce input, and resetting it is authentication-subkey recovery plus forgery
/// (ADR-0036 §2). The asymmetry — hop resettable in principle, stream sequence
/// never — is exactly the kind of thing a shared `SequenceNumber` type erases,
/// so the two are separate types with separate homes and this one has no reset
/// at all.
///
/// # Scope: per (subscriber connection, MEDIA stream), never per QUIC stream
///
/// One lives in each egress edge's runtime cell, keyed by `egress_stream_id`
/// (MC's policy-plane identity, stable across slot renumbering AND across a
/// slot refill — `mc-service`'s `assignment::egress_stream_id`). It therefore
/// continues across group streams (a video QUIC stream lasts one group, ADR §1,
/// so a counter scoped to it would reset at every keyframe and detect nothing)
/// and across a source switch or refill, for both transports. Before story 3
/// MH-A2 it lived in the PUBLISHER's forwarder, so an audio slot refilled A→C
/// restarted from C's counter.
///
/// # The number is consumed at REWRITE time, not at send time
///
/// It must be assigned before the first lossy step, because a counter
/// incremented only on success can never reveal loss — every number that exists
/// would have been delivered, which defeats the field's purpose. So a frame
/// shed by egress-queue overflow, or a stream frame whose downlink is reset
/// mid-payload, *after* its hop number was assigned leaves a gap, and the
/// subscriber reads that gap as loss, which it **is**. An intentionally
/// unforwarded frame — one gated, server-muted, unattributed or with no local
/// subscriber — consumes no number, which is §2's rule that gaps must reflect
/// genuine transport loss and never selection policy.
///
/// # Unique, not ordered, across concurrent takers
///
/// Atomic with `Relaxed` ordering: the only property needed is that every
/// number is handed out once. During a datagram slot refill or a video switch,
/// two publishers' tasks can briefly take numbers for one edge concurrently, so
/// numbers are unique and monotonic per edge but sends may interleave out of
/// numbering order across the two sources (and on two live video streams).
/// ADR §2 asks the receiver to detect GAPS; reordering across a switch is not
/// loss.
#[derive(Debug, Default)]
pub struct HopSequence(AtomicU32);

/// The width is derived from the wire format, never restated. If the field ever
/// widens, this fails to compile rather than silently truncating on the wire.
const _: () = assert!(
    HOP_SEQUENCE_FIELD_BYTES == core::mem::size_of::<u32>(),
    "HopSequence must be exactly the width of the frame's hop-sequence field"
);

impl HopSequence {
    /// A counter that has sent nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    /// The number to write into this frame's relay region, advancing the
    /// counter.
    ///
    /// Wrapping, not saturating (`fetch_add` wraps): a saturating counter pins
    /// at `u32::MAX` and every subsequent frame looks like a duplicate to the
    /// receiver, which is worse than a single wrap the receiver reads as one
    /// large gap.
    pub fn take(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }

    /// The next number this counter would hand out. Test-facing.
    #[must_use]
    pub fn peek(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    const fn starting_at(value: u32) -> Self {
        Self(AtomicU32::new(value))
    }
}

/// Everything one publishing connection's forward loop owns.
///
/// # Sender identity is bound at spawn, from the JWT-gated accept path
///
/// [`Self::sender`] is fixed when this value is constructed, **after** the JWT
/// gate in `crate::webtransport::connection` has returned. Nothing
/// datagram-carried influences which slots a frame can address: MH never reads
/// a sender id off the wire, and the frame header deliberately carries none. A
/// publisher-asserted sender id would be a cross-participant injection
/// primitive — a patched client claims another participant's ordinal and its
/// frames are forwarded onto that participant's edges.
pub struct ConnectionForwarder {
    meeting: MeetingKey,
    sender: SenderId,
    /// Fan-out scratch space for edges 2..N.
    ///
    /// Edge 1 takes the zero-copy path (the ingress buffer is rewritten in
    /// place); every further edge needs its own 6-byte relay span in an
    /// independently-owned buffer, so a copy is structurally unavoidable for a
    /// contiguous datagram API. Those copies come from here.
    ///
    /// Allocated once at setup and refilled: because the bounded egress ring
    /// holds at most `EGRESS_QUEUE_FRAMES` frozen chunks, the arena is uniquely
    /// owned whenever it is next reserved and `BytesMut::reserve` reclaims in
    /// place. That is an **amortized** zero-allocation property, not a
    /// steady-state-from-cold one, which is why the no-per-frame-allocation
    /// gate measures after warming the ring.
    arena: BytesMut,
    handles: Arc<MediaMetricHandles>,
    sampler: LatencySampler,
}

impl ConnectionForwarder {
    /// Bind a forwarder to one meeting-scoped publisher.
    ///
    /// `arena_frame_bytes` is the nominal egress frame size used to size the
    /// scratch arena; it is a capacity hint, not a bound — an oversized frame
    /// still forwards, it just makes the arena reserve once.
    #[must_use]
    pub fn new(
        meeting: MeetingKey,
        sender: SenderId,
        handles: Arc<MediaMetricHandles>,
        sample_ratio: f64,
        arena_frame_bytes: usize,
    ) -> Self {
        Self {
            meeting,
            sender,
            arena: BytesMut::with_capacity(EGRESS_QUEUE_FRAMES * arena_frame_bytes),
            handles,
            sampler: LatencySampler::new(sample_ratio),
        }
    }

    /// The meeting this connection publishes into.
    #[must_use]
    pub const fn meeting(&self) -> &MeetingKey {
        &self.meeting
    }

    /// The publisher this connection is bound to.
    #[must_use]
    pub const fn sender(&self) -> SenderId {
        self.sender
    }

    /// The pre-resolved metric handles.
    #[must_use]
    pub fn handles(&self) -> &MediaMetricHandles {
        &self.handles
    }

    /// The ratio the sampler draws against — the value the gauge publishes.
    #[must_use]
    pub const fn sample_ratio(&self) -> f64 {
        self.sampler.ratio()
    }

    /// Whether this frame's latency is observed.
    pub fn should_sample(&mut self) -> bool {
        self.sampler.should_sample()
    }

    /// The forward path's mutable working set, split so the fan-out closure can
    /// borrow the arena disjointly from the handles.
    pub(crate) fn parts(&mut self) -> ForwardParts<'_> {
        ForwardParts {
            meeting: &self.meeting,
            arena: &mut self.arena,
            handles: &self.handles,
        }
    }
}

/// The disjoint borrows [`crate::media::forward::forward_one`] needs.
pub(crate) struct ForwardParts<'a> {
    pub(crate) meeting: &'a MeetingKey,
    pub(crate) arena: &'a mut BytesMut,
    pub(crate) handles: &'a MediaMetricHandles,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_sequence_advances_by_one_and_wraps_rather_than_pinning() {
        let hop = HopSequence::new();
        assert_eq!(hop.take(), 0);
        assert_eq!(hop.take(), 1);
        assert_eq!(hop.peek(), 2);

        let near_max = HopSequence::starting_at(u32::MAX);
        assert_eq!(near_max.take(), u32::MAX);
        assert_eq!(
            near_max.peek(),
            0,
            "wrapping, not saturating: a pinned counter makes every later frame look like a \
             duplicate"
        );
    }
}
