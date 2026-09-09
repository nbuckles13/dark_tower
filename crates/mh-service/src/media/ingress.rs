//! The three per-connection loops.
//!
//! ```text
//! A ingress:  recv_datagram() -> size cap -> ingress ring
//! B forward:  ingress ring    -> routing read -> relay rewrite -> egress rings
//! C egress:   this connection's egress ring -> send_datagram()
//! ```
//!
//! # Why C is its own task
//!
//! Because otherwise **the egress ring can never fill**. With B draining its
//! own pushes, a refusing sink backs up the *ingress* ring instead and
//! `egress_queue_overflow` would have no firing path at all — the counter would
//! exist and be untestable, which ADR-0036's coverage rule treats as worse than
//! absent. Splitting C also yields §11's latency decomposition for free:
//! `receive_buffer` is recv→ingress-pop, `processing` is
//! ingress-pop→egress-push, `transmit_buffer` is egress-push→send-returns, and
//! `total` is recv→send-returns.
//!
//! # Production reality, stated rather than implied
//!
//! `DatagramSendError::WouldBlock` has **no production producer** (see
//! `crate::transport::DatagramSendError`). In production C drains immediately
//! and the application queue sheds only under genuine scheduling starvation;
//! the counter's deterministic firing path is the test shim. The genuine loss
//! below us is quinn's silent eviction, which this code structurally cannot
//! see — nothing here, in the catalog, on a dashboard or in a runbook may
//! describe an MH egress signal as observing transport back-pressure.
//!
//! # These loops RETURN; the caller emits
//!
//! Each loop returns a bounded exit enum. `crate::webtransport::connection` —
//! a sibling, not a child — logs it. No `tracing`, `log`, `metrics`,
//! `println!`, `event!`, span or `#[instrument]` macro appears anywhere under
//! `media/`, and `DecodeError`'s `Display` never crosses out of this directory:
//! it carries per-frame sizes, and a sibling formatting it into a log line
//! would rebuild the voice-activity trace by a second route that a
//! directory-scoped deny cannot see.

use crate::media::forward::{forward_one, EgressFrame, EgressQueue, IngressFrame};
use crate::media::forwarder::ConnectionForwarder;
use crate::media::queue::{depth_as_gauge, SharedQueue};
use crate::media::{caps, MediaTaskContext};
use crate::observability::metrics::{MediaDirection, MediaDropReason, MediaLatencyPhase};
use crate::transport::{DatagramSendError, MediaTransport, TransportError};
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// Why the ingress loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopExit {
    /// The connection is gone. The ordinary end of a media session.
    ConnectionClosed,
    /// The server is shutting down (ADR-0036 §11: MH sheds media sessions on
    /// restart, and that is the decision, not merely the current behaviour).
    Cancelled,
}

/// How many datagrams [`run_ingress`] took off the transport, for one
/// connection.
///
/// # This type exists to make a privacy hazard a COMPILE ERROR
///
/// At one 20 ms Opus frame per datagram, a per-connection count of received
/// media frames **is talk duration for that participant**. It is the same class
/// of value this module's docs bar `DecodeError`'s `Display` from carrying out
/// of `media/`, and for the same reason: the sibling that formats it into a log
/// line rebuilds the voice-activity trace by a route the directory-scoped macro
/// deny structurally cannot see — the walker looks for macros *under* `media/`,
/// not for a sibling logging a value that came *out* of it.
///
/// The hazard is concrete rather than theoretical. Widening this loop's return
/// type breaks the homogeneous `[("ingress", …), ("forward", …), ("egress", …)]`
/// teardown array in `crate::webtransport::connection`, forcing a hand-written
/// ingress arm — and that module logs `connection_id` beside `participant_id`
/// eighty lines later, so the two are joinable.
///
/// So: **no `Display`, no `Debug`, no derive of either, a private field, and no
/// accessor that returns the count.** The only method is
/// [`Self::unread_since`], which consumes the value and yields the *difference*
/// against a received total. A sibling therefore cannot obtain the count at
/// all — only the residual, which is near-zero in health and is not a
/// talk-duration proxy. Same remedy as [`crate::media::forward::IngressFrame`]'s
/// hand-rolled `Debug`, taken one step further because here nothing legitimate
/// needs to read the number.
///
/// # Deliberately NOT a metric, and no counter formula reconstructs it
///
/// This is task-local and is never published as a series, and the reason is
/// **not** that it duplicates existing counters — an earlier draft of this
/// paragraph claimed it equalled
/// `forwarded{ingress} + dropped{ingress_queue_overflow} + dropped{oversize_datagram}`,
/// then equalled that plus `dropped{no_policy}`, and **both were wrong**.
/// `frames_read` is not reconstructible from the drop vocabulary at all: a
/// frame counted here can terminate in a codec reject (which returns before the
/// `forwarded{ingress}` increment), in `no_policy` (whose branch deliberately
/// does not increment it), or in the ring residue left when the forward loop's
/// `select!` cancels without draining — counted here, forwarded never, dropped
/// never. **No formula is stated in its place**, deliberately: enumerating an
/// accounting identity that interacts with `forward_one`'s disposition set
/// would rot the first time that set changes, which is the same trap the
/// deleted ordinals in `crate::observability::metrics` were.
///
/// The real justification stands on its own and always did: what makes this
/// value worth having is that it is **per-connection and exact at teardown**,
/// which no aggregate counter can give across a scrape boundary. That is why it
/// exists, and equally why it must not be published — a per-connection media
/// frame count is the talk-duration hazard the type above exists to contain.
pub struct FramesRead(u64);

impl FramesRead {
    /// Datagrams the QUIC connection received that this loop never read.
    ///
    /// Consumes `self`: the count is spent here and cannot be observed
    /// elsewhere. `saturating_sub` because the two counts are sampled from
    /// different sources and a transient skew must not wrap into a vast
    /// bogus reading.
    #[must_use]
    pub const fn unread_since(self, received: u64) -> u64 {
        received.saturating_sub(self.0)
    }
}

/// A queue of datagrams accepted off the transport, awaiting forwarding.
pub type IngressQueue = SharedQueue<IngressFrame>;

/// Loop A: receive datagrams, enforce the pre-allocation caps, enqueue.
///
/// The size check happens on the value the transport already owns, before
/// anything is allocated on our behalf and before any parse — ADR-0036 §11's
/// "maximum frame payload before any allocation".
pub async fn run_ingress<T: MediaTransport>(
    transport: Arc<T>,
    queue: Arc<IngressQueue>,
    context: Arc<MediaTaskContext>,
    cancel: CancellationToken,
) -> (LoopExit, FramesRead) {
    // Loop-local, non-atomic, returned rather than shared — see `FramesRead`
    // for why it is neither a metric nor loggable. One `u64 += 1` per datagram
    // is not a §11 concern: no macro form, no allocation, no registry lookup,
    // no clock read.
    let mut frames_read = 0_u64;
    loop {
        let payload = tokio::select! {
            () = cancel.cancelled() => return (LoopExit::Cancelled, FramesRead(frames_read)),
            received = transport.recv_datagram() => match received {
                Ok(payload) => payload,
                Err(TransportError::ConnectionClosed | TransportError::StreamClosed) => {
                    return (LoopExit::ConnectionClosed, FramesRead(frames_read))
                }
            },
        };

        // Counted the instant it leaves the transport, BEFORE the size cap, so
        // the tally is "what this loop took off the wire" and not "what it
        // liked". A cap rejection is still a datagram quinn delivered and MH
        // read, and must not reappear as unread.
        frames_read = frames_read.saturating_add(1);

        // MEASUREMENT clock: real elapsed time, never paused.
        let received_at = Instant::now();

        if let Err(reason) = caps::check_datagram_size(payload.len()) {
            context.handles.dropped(reason).increment(1);
            continue;
        }

        let (evicted, _depth) = queue.push(IngressFrame {
            payload,
            received_at,
        });
        if evicted.is_some() {
            context
                .handles
                .dropped(MediaDropReason::IngressQueueOverflow)
                .increment(1);
        }
    }
}

/// Loop B: forward. Pops the ingress ring, reads the routing snapshot, rewrites
/// the relay region and pushes onto each subscriber's egress ring.
pub async fn run_forward(
    queue: Arc<IngressQueue>,
    mut forwarder: ConnectionForwarder,
    context: Arc<MediaTaskContext>,
    cancel: CancellationToken,
) -> LoopExit {
    loop {
        let frame = tokio::select! {
            () = cancel.cancelled() => return LoopExit::Cancelled,
            frame = queue.pop() => frame,
        };

        // Re-read per frame, cache nothing: ADR-0036 §7's server mute and
        // participant removal are enforced through this lookup, so a cached
        // edge list that outlives a policy change is a mute bypass.
        let routes = context.routing.load();
        let subscribers = context.subscribers.load();
        let _ = forward_one(&mut forwarder, &routes, &subscribers, frame);
    }
}

/// Loop C: drain this connection's egress ring into the transport.
pub async fn run_egress<T: MediaTransport>(
    transport: Arc<T>,
    queue: Arc<EgressQueue>,
    context: Arc<MediaTaskContext>,
    cancel: CancellationToken,
) -> LoopExit {
    loop {
        let frame = tokio::select! {
            () = cancel.cancelled() => return LoopExit::Cancelled,
            frame = queue.pop() => frame,
        };

        // Republish depth AFTER the pop. Publishing only on push leaves the
        // gauge at the last push-time depth once traffic subsides — it reports
        // a deep queue across an idle period, which is the opposite failure
        // from the one the catalog entry warns about and the same lie.
        context
            .handles
            .egress_queue_depth()
            .set(depth_as_gauge(queue.depth()));

        let EgressFrame {
            payload,
            received_at,
            queued_at,
            sampled,
        } = frame;

        match transport.send_datagram(payload) {
            Ok(()) => {
                context
                    .handles
                    .forwarded(MediaDirection::Egress)
                    .increment(1);
                if sampled {
                    // MEASUREMENT clock on both reads.
                    let sent_at = Instant::now();
                    context
                        .handles
                        .latency(MediaLatencyPhase::TransmitBuffer)
                        .record(sent_at.saturating_duration_since(queued_at).as_secs_f64());
                    context
                        .handles
                        .latency(MediaLatencyPhase::Total)
                        .record(sent_at.saturating_duration_since(received_at).as_secs_f64());
                }
            }
            // Routine: a subscriber hangs up mid-flight in every meeting, many
            // times. Its own token precisely so it does not make
            // `transport_send_refused` an unalertable mixture of "we have a
            // bug" and "someone left".
            Err(DatagramSendError::ConnectionClosed) => {
                context
                    .handles
                    .dropped(MediaDropReason::ConnectionClosed)
                    .increment(1);
                return LoopExit::ConnectionClosed;
            }
            // `TooLarge` means MH built an oversize datagram;
            // `DatagramsUnsupported` means a connection-setup invariant broke;
            // `WouldBlock` has no production producer at all. All three should
            // read zero forever, which is what makes this counter alertable.
            Err(
                DatagramSendError::TooLarge
                | DatagramSendError::DatagramsUnsupported
                | DatagramSendError::WouldBlock(_),
            ) => {
                context
                    .handles
                    .dropped(MediaDropReason::TransportSendRefused)
                    .increment(1);
            }
        }
    }
}
