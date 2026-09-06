//! Per-connection and per-egress-stream forward state.
//!
//! Holds the hop sequence, the fan-out arena, the pre-resolved metric handles
//! and the latency sampler. No setup and no teardown live here: constructing
//! one of these is the caller's job (`crate::webtransport::connection`), and
//! ADR-0036 §11 keeps lifecycle a **sibling** of the media directory.

use crate::config::EGRESS_QUEUE_FRAMES;
use crate::media::sampler::LatencySampler;
use crate::observability::metrics::MediaMetricHandles;
use crate::routing::{MeetingKey, SenderId};
use bytes::BytesMut;
use media_protocol::frame::HOP_SEQUENCE_FIELD_BYTES;
use std::sync::Arc;

/// The relay's per-(connection, media stream) downlink send count.
///
/// # MH runtime state, not wire format
///
/// This type lands here rather than in `media-protocol` deliberately: the hop
/// counter is **MH-generated and never publisher-set**, and the wire contract is
/// already complete without it — `frame::HOP_SEQUENCE_FIELD_BYTES` gives the
/// width and `codec::rewrite_relay_region` takes the value as a `u32`
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
/// # Scope: per (connection, MEDIA stream), never per QUIC stream
///
/// Keyed by `egress_stream_id`, MC's policy-plane identity, which is stable
/// across slot renumbering. A video QUIC stream lasts one group of pictures
/// (§1), so a counter scoped to it would reset at every keyframe and detect
/// nothing.
///
/// # The number is consumed at REWRITE time, not at send time
///
/// It must be assigned before the first lossy step, because a counter
/// incremented only on success can never reveal loss — every number that exists
/// would have been delivered, which defeats the field's purpose. So a frame
/// shed by egress-queue overflow *after* its hop number was assigned leaves a
/// gap, and the subscriber reads that gap as loss, which it **is**. An
/// intentionally unforwarded frame — one whose routing lookup never reached the
/// rewrite — consumes no number, which is §2's rule that gaps must reflect
/// genuine transport loss and never selection policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HopSequence(u32);

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
        Self(0)
    }

    /// The number to write into this frame's relay region, advancing the
    /// counter.
    ///
    /// Wrapping, not saturating: a saturating counter pins at `u32::MAX` and
    /// every subsequent frame looks like a duplicate to the receiver, which is
    /// worse than a single wrap the receiver reads as one large gap.
    pub fn take(&mut self) -> u32 {
        let value = self.0;
        self.0 = self.0.wrapping_add(1);
        value
    }

    /// The next number this counter would hand out. Test-facing.
    #[must_use]
    pub const fn peek(self) -> u32 {
        self.0
    }
}

/// Per-egress-stream state: the identity MC assigned, and its hop counter.
#[derive(Debug)]
pub struct StreamForwarder {
    /// MC-allocated policy-plane identity for this egress stream.
    egress_stream_id: u32,
    hop: HopSequence,
}

impl StreamForwarder {
    /// A forwarder for one egress stream, having sent nothing.
    #[must_use]
    pub const fn new(egress_stream_id: u32) -> Self {
        Self {
            egress_stream_id,
            hop: HopSequence::new(),
        }
    }

    /// The egress stream this forwarder counts for.
    #[must_use]
    pub const fn egress_stream_id(&self) -> u32 {
        self.egress_stream_id
    }

    /// The hop number for the frame being rewritten now.
    pub fn take_hop(&mut self) -> u32 {
        self.hop.take()
    }

    /// The next hop number, without consuming it. Test-facing.
    #[must_use]
    pub const fn peek_hop(&self) -> u32 {
        self.hop.peek()
    }
}

/// Everything one publishing connection's forward loop owns.
///
/// # Sender identity is bound at spawn, from the JWT-gated accept path
///
/// [`Self::sender`] is fixed when this value is constructed, **after** the JWT
/// gate in `crate::webtransport::connection` has returned. Nothing
/// datagram-carried influences which slots a frame can address: MH never reads
/// a sender id off the wire, and frame header v2 deliberately carries none. A
/// publisher-asserted sender id would be a cross-participant injection
/// primitive — a patched client claims another participant's ordinal and its
/// frames are forwarded onto that participant's edges.
pub struct ConnectionForwarder {
    meeting: MeetingKey,
    sender: SenderId,
    /// One entry per egress stream this sender feeds.
    ///
    /// A `Vec` with a linear scan rather than a `HashMap`: a sender feeds a
    /// handful of edges, the scan is over `u32`s in one cache line's worth of
    /// memory, and it allocates only when a new egress stream is first seen
    /// (amortized, never per frame).
    streams: Vec<StreamForwarder>,
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
            streams: Vec::new(),
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
    /// borrow the arena and the stream counters disjointly from the handles.
    pub(crate) fn parts(&mut self) -> ForwardParts<'_> {
        ForwardParts {
            meeting: &self.meeting,
            sender: self.sender,
            streams: &mut self.streams,
            arena: &mut self.arena,
            handles: &self.handles,
        }
    }
}

/// The disjoint borrows [`crate::media::forward::forward_one`] needs.
pub(crate) struct ForwardParts<'a> {
    pub(crate) meeting: &'a MeetingKey,
    pub(crate) sender: SenderId,
    pub(crate) streams: &'a mut Vec<StreamForwarder>,
    pub(crate) arena: &'a mut BytesMut,
    pub(crate) handles: &'a MediaMetricHandles,
}

impl ForwardParts<'_> {
    /// The hop number to write for `egress_stream_id`, creating the counter on
    /// first sight and advancing it.
    ///
    /// Returns the number rather than a `&mut StreamForwarder` so there is no
    /// find-then-insert double borrow to route around, and so the only way to
    /// obtain a hop number is to consume one — a caller cannot peek at the next
    /// value and then decide not to send, which is how a counter silently
    /// stops meaning "what the transmitter sent".
    pub(crate) fn take_hop_for(&mut self, egress_stream_id: u32) -> u32 {
        if let Some(stream) = self
            .streams
            .iter_mut()
            .find(|stream| stream.egress_stream_id() == egress_stream_id)
        {
            return stream.take_hop();
        }
        let mut fresh = StreamForwarder::new(egress_stream_id);
        let hop = fresh.take_hop();
        self.streams.push(fresh);
        hop
    }

    /// The next hop number for `egress_stream_id` without consuming it.
    /// Test-facing.
    #[cfg(test)]
    pub(crate) fn peek_hop_for(&self, egress_stream_id: u32) -> Option<u32> {
        self.streams
            .iter()
            .find(|stream| stream.egress_stream_id() == egress_stream_id)
            .map(StreamForwarder::peek_hop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_sequence_advances_by_one_and_wraps_rather_than_pinning() {
        let mut hop = HopSequence::new();
        assert_eq!(hop.take(), 0);
        assert_eq!(hop.take(), 1);
        assert_eq!(hop.peek(), 2);

        let mut near_max = HopSequence(u32::MAX);
        assert_eq!(near_max.take(), u32::MAX);
        assert_eq!(
            near_max.peek(),
            0,
            "wrapping, not saturating: a pinned counter makes every later frame look like a \
             duplicate"
        );
    }

    #[test]
    fn each_egress_stream_gets_its_own_counter() {
        let mut forwarder = ConnectionForwarder::new(
            MeetingKey::new("m"),
            SenderId::from_wire(1).unwrap(),
            Arc::new(crate::observability::metrics::resolve_media_handles()),
            1.0,
            256,
        );
        let mut parts = forwarder.parts();
        assert_eq!(parts.take_hop_for(10), 0);
        assert_eq!(parts.take_hop_for(10), 1);
        assert_eq!(
            parts.take_hop_for(20),
            0,
            "a second egress stream counts independently — per MEDIA stream, not per connection"
        );
        assert_eq!(parts.take_hop_for(10), 2);
        assert_eq!(parts.peek_hop_for(10), Some(3));
        assert_eq!(parts.peek_hop_for(999), None);
    }
}
