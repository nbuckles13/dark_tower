//! One subscriber's downlink group stream: a per-destination, byte-capped,
//! non-blocking queue on the fan-out side, and the writer that drains it into
//! the transport (story 3 R-12, R-21; ADR-0036 §1 amendment).
//!
//! # Who writes
//!
//! The publisher's ingress task NEVER awaits a subscriber. It pushes into a
//! [`Downlink`] — `push_header` / `push_chunk` are synchronous and never block
//! — and a per-SUBSCRIBER-connection task ([`run_stream_egress`]) runs one
//! writer per downlink ([`run_writer`]), which awaits `open_uni` and
//! `write_all` against the downlink's own abandon signal. A subscriber that
//! withholds stream credit or `MAX_STREAMS` therefore stalls only its own
//! writer: its backlog grows until the cap, and then MH abandons that
//! destination. The publisher, the source's other subscribers and every
//! subscriber's datagram audio are untouched.
//!
//! # The backlog cap is per DESTINATION
//!
//! [`BacklogBudget`] is shared by all of one edge's live downlinks (at most two:
//! the current group and its predecessor finishing its in-flight frame), so the
//! memory a destination can hold is `MH_EGRESS_STREAM_BACKLOG_BYTES` however
//! its groups overlap. Bytes are charged when pushed and released when the
//! writer's `write_all` accepts them (quinn then owns a copy, bounded by the
//! declared connection send window). A push that would exceed the cap fails
//! with [`PushError::Backlog`] and the caller abandons the destination
//! (`subscriber_backlog`).
//!
//! # Copy-on-backlog: the charge equals the memory
//!
//! A payload chunk from the ingress side is a `Bytes` slice of quinn's receive
//! buffer, and RETAINING it pins that parent allocation (one receive batch, up
//! to ~64 GRO segments × ~1.5 KB). Charging the slice length while pinning the
//! parent would let a publisher sending one payload byte per padded packet
//! inflate one destination's real footprint by orders of magnitude past its
//! charge. So a chunk is queued ZERO-COPY only while no other uncopied chunk is
//! outstanding on this downlink; otherwise it is copied into the downlink's
//! own coalescing buffer before it is retained, and that buffer is what the
//! charge measures. Headers are always copied (they are rewritten per
//! subscriber), each into an exact allocation; the coalescing buffer is taken
//! whole by the writer and dropped on abandon, so no capacity outlives its
//! charge. The residual is `BytesMut`'s growth (at most 2x the coalesced
//! bytes), which `crate::config::BACKLOG_ALLOCATION_FACTOR` carries into the
//! boot memory check. At most ONE uncopied slice per downlink is therefore alive at
//! any instant — including the one inside a pending `write_all` — which bounds
//! the pinned-parent overhang at 2 downlinks × stream ceiling × one receive
//! batch (figures in `crate::config::check_pod_memory`). On the steady path —
//! a writer keeping up — the payload fans out with no copy at all, which is
//! what ADR-0036 §10's refcount gate asserts.
//!
//! # Finish only at a frame boundary; everything else is a RESET
//!
//! [`Downlink::request_finish`] made mid-frame is deferred to the frame's end
//! ([`Downlink::end_frame`]). A frame that never completes is abandoned by the
//! publisher-side terminal function with the upstream cause, and a writer that
//! is dropped without finishing resets through the transport seam's drop
//! contract. A half-written frame never looks complete.
//!
//! # Counting
//!
//! [`Downlink::abandon`] is the ONLY place `mh_media_group_resets_total` is
//! counted: once per downlink group abandoned for a destination, whether or
//! not its uni stream was ever opened (idempotent), whether or not the
//! transport `reset` later succeeds. A downlink lost because its subscriber's
//! connection or stream went away ([`Downlink::mark_lost`]) is not an
//! abandonment and counts nothing here: the next frame for that edge counts
//! `connection_closed` (see `crate::media::edge_runtime`). Egress group starts
//! are counted when the writer's `open_uni` returns.

use crate::observability::metrics::{
    GroupResetReason, MediaDirection, MediaLatencyPhase, MediaMetricHandles,
};
use crate::transport::{MediaSendStream, MediaTransport, GROUP_ABANDONED};
use bytes::{Bytes, BytesMut};
use media_protocol::codec::{rewrite_relay_region_header, DecodeError};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;

/// Map MC's `priority_group` (lower number = higher priority; audio 1, video
/// 2) onto quinn's send-stream priority (HIGHER value = sent first).
#[must_use]
pub fn quic_priority(priority_group: u32) -> i32 {
    i32::try_from(priority_group).map_or(i32::MIN, |group| -group)
}

/// One destination's byte budget, shared by its live downlinks.
#[derive(Debug)]
pub struct BacklogBudget {
    cap: usize,
    used: AtomicUsize,
}

impl BacklogBudget {
    /// A budget of `cap` bytes.
    #[must_use]
    pub const fn new(cap: usize) -> Self {
        Self {
            cap,
            used: AtomicUsize::new(0),
        }
    }

    /// Charge `n` bytes, or refuse without charging if that exceeds the cap.
    fn try_charge(&self, n: usize) -> bool {
        self.used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(n).filter(|total| *total <= self.cap)
            })
            .is_ok()
    }

    fn release(&self, n: usize) {
        let _ = self
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                Some(used.saturating_sub(n))
            });
    }

    /// Bytes currently charged.
    #[must_use]
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }

    /// The cap.
    #[must_use]
    pub const fn cap(&self) -> usize {
        self.cap
    }
}

/// Why a push was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushError {
    /// The destination's backlog cap would be exceeded. Abandon the
    /// destination (`subscriber_backlog`).
    Backlog,
    /// The downlink is already finished, abandoned or lost.
    Gone,
    /// The header failed the codec's relay rewrite: an MH bug on an already
    /// parsed header (`relay_rewrite_failed`).
    Rewrite(DecodeError),
}

/// Latency stamps carried by a sampled header (MEASUREMENT clock).
#[derive(Debug, Clone, Copy)]
struct Stamps {
    header_at: Instant,
    pushed_at: Instant,
}

/// One unit the writer hands to `write_all`.
#[derive(Clone)]
pub struct Item {
    bytes: Bytes,
    /// An uncopied slice of a receive buffer (see "Copy-on-backlog").
    shared: bool,
    stamps: Option<Stamps>,
}

/// Hand-rolled: the bytes are a payload slice or a header holding a wrapped
/// transmit key, and must never render (same reasoning as
/// `crate::media::forward::IngressFrame`).
impl std::fmt::Debug for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Item")
            .field("bytes", &format_args!("<redacted {} B>", self.bytes.len()))
            .field("shared", &self.shared)
            .finish_non_exhaustive()
    }
}

impl Item {
    /// The bytes to write.
    #[must_use]
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Whether this item is an uncopied receive-buffer slice.
    #[must_use]
    pub const fn is_shared(&self) -> bool {
        self.shared
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Accepting pushes.
    Open,
    /// Finish requested at a frame boundary; the writer finishes once the
    /// queue drains.
    Finishing,
    /// The writer finished the stream.
    Done,
    /// MH abandoned this destination's group.
    Abandoned,
    /// The subscriber's connection or stream went away under the writer.
    Lost,
}

#[derive(Debug)]
struct Inner {
    phase: Phase,
    queue: VecDeque<Item>,
    /// Coalescing tail, after every queued item: copies land here.
    pending: BytesMut,
    in_frame: bool,
    finish_at_frame_end: bool,
    /// An uncopied slice is queued or being written.
    shared_outstanding: bool,
    /// Bytes charged to the budget and not yet released.
    charged: usize,
}

/// What the writer does next.
#[derive(Debug)]
pub enum WriterStep {
    /// Write this item.
    Write(Item),
    /// FIN at a frame boundary.
    Finish,
    /// RESET (abandoned).
    Reset,
    /// The downlink was lost; nothing to do.
    Stop,
}

/// One subscriber's group stream, shared by the fan-out side (push) and the
/// writer (drain).
pub struct Downlink {
    inner: Mutex<Inner>,
    notify: Notify,
    budget: Arc<BacklogBudget>,
    priority: i32,
    handles: Arc<MediaMetricHandles>,
}

impl std::fmt::Debug for Downlink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Downlink")
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}

impl Downlink {
    /// A new, empty downlink charging `budget`, sent at quinn `priority`.
    #[must_use]
    pub fn new(
        budget: Arc<BacklogBudget>,
        priority: i32,
        handles: Arc<MediaMetricHandles>,
    ) -> Self {
        Self {
            inner: Mutex::new(Inner {
                phase: Phase::Open,
                queue: VecDeque::new(),
                pending: BytesMut::new(),
                in_frame: false,
                finish_at_frame_end: false,
                shared_outstanding: false,
                charged: 0,
            }),
            notify: Notify::new(),
            budget,
            priority,
            handles,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The quinn priority the writer sets.
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    /// Copy `header` in, rewrite its relay region to (`slot`, `hop`) through
    /// the codec, and queue it as the start of a frame. `sampled` carries the
    /// header-complete instant of a frame whose latency is observed.
    ///
    /// # Errors
    ///
    /// [`PushError::Backlog`] past the destination's cap, [`PushError::Gone`]
    /// on a closed downlink, [`PushError::Rewrite`] if the codec refuses.
    pub fn push_header(
        &self,
        header: &[u8],
        slot: u16,
        hop: u32,
        sampled: Option<Instant>,
    ) -> Result<(), PushError> {
        let mut inner = self.lock();
        if inner.phase != Phase::Open {
            return Err(PushError::Gone);
        }
        if !self.budget.try_charge(header.len()) {
            return Err(PushError::Backlog);
        }
        // An EXACT allocation per header (never a view of a growth buffer
        // whose spare capacity later copies would land in), so the charge is
        // the memory. Rewritten through the codec before it is queued.
        let mut copy = BytesMut::with_capacity(header.len());
        copy.extend_from_slice(header);
        if let Err(error) = rewrite_relay_region_header(&mut copy, slot, hop) {
            self.budget.release(header.len());
            return Err(PushError::Rewrite(error));
        }
        inner.charged = inner.charged.saturating_add(header.len());
        // Order: anything coalesced before this header goes first, as its own
        // item (the buffer is taken whole, keeping no spare capacity).
        if !inner.pending.is_empty() {
            let earlier = std::mem::take(&mut inner.pending).freeze();
            inner.queue.push_back(Item {
                bytes: earlier,
                shared: false,
                stamps: None,
            });
        }
        let bytes = copy.freeze();
        inner.queue.push_back(Item {
            bytes,
            shared: false,
            stamps: sampled.map(|header_at| Stamps {
                header_at,
                pushed_at: Instant::now(),
            }),
        });
        inner.in_frame = true;
        drop(inner);
        self.notify.notify_waiters();
        Ok(())
    }

    /// Queue one payload/signature chunk of the frame in flight.
    ///
    /// # Errors
    ///
    /// [`PushError::Backlog`] past the destination's cap, [`PushError::Gone`]
    /// on a closed downlink.
    pub fn push_chunk(&self, chunk: &Bytes) -> Result<(), PushError> {
        if chunk.is_empty() {
            return Ok(());
        }
        let mut inner = self.lock();
        if inner.phase != Phase::Open {
            return Err(PushError::Gone);
        }
        if !self.budget.try_charge(chunk.len()) {
            return Err(PushError::Backlog);
        }
        inner.charged = inner.charged.saturating_add(chunk.len());
        if inner.pending.is_empty() && !inner.shared_outstanding {
            inner.shared_outstanding = true;
            inner.queue.push_back(Item {
                bytes: chunk.clone(),
                shared: true,
                stamps: None,
            });
        } else {
            inner.pending.extend_from_slice(chunk);
        }
        drop(inner);
        self.notify.notify_waiters();
        Ok(())
    }

    /// The frame in flight ended cleanly.
    pub fn end_frame(&self) {
        let mut inner = self.lock();
        inner.in_frame = false;
        if inner.finish_at_frame_end && inner.phase == Phase::Open {
            inner.phase = Phase::Finishing;
        }
        drop(inner);
        self.notify.notify_waiters();
    }

    /// End this group cleanly: FIN once the frame in flight (if any) is
    /// complete and the backlog drains. Mid-frame, deferred to the frame end.
    pub fn request_finish(&self) {
        let mut inner = self.lock();
        if inner.phase != Phase::Open {
            return;
        }
        if inner.in_frame {
            inner.finish_at_frame_end = true;
        } else {
            inner.phase = Phase::Finishing;
        }
        drop(inner);
        self.notify.notify_waiters();
    }

    /// Abandon this destination's group: RESET, never FIN. Counts
    /// `mh_media_group_resets_total{reason}` exactly once; returns whether this
    /// call was the one that abandoned it.
    pub fn abandon(&self, reason: GroupResetReason) -> bool {
        let mut inner = self.lock();
        if !matches!(inner.phase, Phase::Open | Phase::Finishing) {
            return false;
        }
        inner.phase = Phase::Abandoned;
        self.clear(&mut inner);
        drop(inner);
        self.handles.group_resets(reason).increment(1);
        self.notify.notify_waiters();
        true
    }

    /// The subscriber's connection or stream went away. Not an abandonment:
    /// counts nothing (see the module docs).
    pub fn mark_lost(&self) {
        let mut inner = self.lock();
        if !matches!(inner.phase, Phase::Open | Phase::Finishing) {
            return;
        }
        inner.phase = Phase::Lost;
        self.clear(&mut inner);
        drop(inner);
        self.notify.notify_waiters();
    }

    fn clear(&self, inner: &mut Inner) {
        inner.queue.clear();
        // Drop the buffer, not just its bytes: no capacity outlives the
        // charge released below.
        inner.pending = BytesMut::new();
        inner.shared_outstanding = false;
        self.budget.release(inner.charged);
        inner.charged = 0;
    }

    /// Lost to the subscriber going away.
    #[must_use]
    pub fn is_lost(&self) -> bool {
        self.lock().phase == Phase::Lost
    }

    /// Finished, abandoned or lost: holds no budget and accepts nothing.
    #[must_use]
    pub fn is_done(&self) -> bool {
        matches!(
            self.lock().phase,
            Phase::Done | Phase::Abandoned | Phase::Lost
        )
    }

    /// Bytes queued and not yet accepted by the transport.
    #[must_use]
    pub fn queued_bytes(&self) -> usize {
        self.lock().charged
    }

    /// The items queued for the writer right now — test-facing, for the
    /// zero-copy fan-out gate (asserted upstream of the transport seam).
    #[must_use]
    pub fn queued_items(&self) -> Vec<Item> {
        self.lock().queue.iter().cloned().collect()
    }

    /// Writer side: the next step, waiting until there is one.
    pub async fn next_step(&self) -> WriterStep {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut inner = self.lock();
                match inner.phase {
                    Phase::Abandoned => return WriterStep::Reset,
                    Phase::Lost | Phase::Done => return WriterStep::Stop,
                    Phase::Open | Phase::Finishing => {}
                }
                if let Some(item) = inner.queue.pop_front() {
                    return WriterStep::Write(item);
                }
                if !inner.pending.is_empty() {
                    // Take the whole coalescing buffer: its spare capacity
                    // goes with these bytes and is freed when they are, never
                    // retained for later copies.
                    let bytes = std::mem::take(&mut inner.pending).freeze();
                    return WriterStep::Write(Item {
                        bytes,
                        shared: false,
                        stamps: None,
                    });
                }
                if inner.phase == Phase::Finishing {
                    return WriterStep::Finish;
                }
            }
            notified.await;
        }
    }

    /// Writer side: resolves once the downlink is abandoned or lost.
    pub async fn terminated(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if matches!(self.lock().phase, Phase::Abandoned | Phase::Lost) {
                return;
            }
            notified.await;
        }
    }

    /// Writer side: `item` was accepted by the transport.
    fn wrote(&self, item: &Item) {
        let mut inner = self.lock();
        if !matches!(inner.phase, Phase::Open | Phase::Finishing) {
            return;
        }
        inner.charged = inner.charged.saturating_sub(item.bytes.len());
        self.budget.release(item.bytes.len());
        if item.shared {
            inner.shared_outstanding = false;
        }
    }

    /// Writer side: the FIN went out.
    fn finished(&self) {
        let mut inner = self.lock();
        if inner.phase == Phase::Finishing {
            inner.phase = Phase::Done;
        }
    }
}

/// Marks the downlink lost if the writer exits (or is aborted) without
/// reaching a terminal step itself — the subscriber's connection is going away.
struct WriterGuard(Arc<Downlink>);

impl Drop for WriterGuard {
    fn drop(&mut self) {
        self.0.mark_lost();
    }
}

/// Drain one downlink into one new uni stream on the subscriber's transport.
///
/// `open_uni` and every `write_all` race the downlink's abandon signal, so a
/// subscriber withholding credit never holds an abandoned stream open. Dropped
/// mid-way (connection teardown aborting the writer set), the stream is reset
/// by the seam's drop contract — deliberately uncounted, the destination being
/// gone.
pub async fn run_writer<T: MediaTransport>(transport: Arc<T>, downlink: Arc<Downlink>) {
    let guard = WriterGuard(Arc::clone(&downlink));
    let mut stream = tokio::select! {
        () = downlink.terminated() => return,
        opened = transport.open_uni() => match opened {
            Ok(stream) => stream,
            Err(_) => return, // guard marks it lost
        },
    };
    stream.set_priority(downlink.priority());
    downlink
        .handles
        .group_starts(MediaDirection::Egress)
        .increment(1);
    loop {
        match downlink.next_step().await {
            WriterStep::Write(item) => {
                let written = tokio::select! {
                    () = downlink.terminated() => None,
                    result = stream.write_all(item.bytes()) => Some(result),
                };
                match written {
                    // Abandoned mid-write: reset now. Already-closed means
                    // there is nothing left to abandon.
                    None => {
                        let _ = stream.reset(GROUP_ABANDONED);
                        return;
                    }
                    Some(Ok(())) => {
                        downlink.wrote(&item);
                        if let Some(stamps) = item.stamps {
                            // MEASUREMENT clock, header items only: payload
                            // chunk timing is never observed (R-30).
                            let written_at = Instant::now();
                            downlink
                                .handles
                                .latency(MediaLatencyPhase::TransmitBuffer)
                                .record(
                                    written_at
                                        .saturating_duration_since(stamps.pushed_at)
                                        .as_secs_f64(),
                                );
                            downlink.handles.latency(MediaLatencyPhase::Total).record(
                                written_at
                                    .saturating_duration_since(stamps.header_at)
                                    .as_secs_f64(),
                            );
                        }
                    }
                    // The subscriber stopped the stream or its connection went:
                    // lost, not abandoned (guard).
                    Some(Err(_)) => return,
                }
            }
            WriterStep::Finish => {
                if stream.finish().await.is_ok() {
                    downlink.finished();
                }
                drop(guard);
                return;
            }
            WriterStep::Reset => {
                let _ = stream.reset(GROUP_ABANDONED);
                return;
            }
            WriterStep::Stop => return,
        }
    }
}

/// The per-subscriber-connection channel new downlinks arrive on.
///
/// BOUNDED at 2 × the derived egress stream ceiling (`start_media_session`):
/// a downlink exists only while it is charged to one of this subscriber's
/// edges, each edge holds at most two live downlinks, and the subscriber's
/// edges are a subset of the admitted ceiling. A full channel therefore means
/// the writer task is not draining; the sender abandons that destination
/// (`subscriber_backlog`). Sends are `try_send`: nothing on the fan-out side
/// ever awaits it.
pub type DownlinkSender = mpsc::Sender<Arc<Downlink>>;

/// The per-subscriber-connection stream egress task: one writer per new
/// downlink, all owned by a `JoinSet` that is dropped (aborting every writer,
/// whose streams then RESET by the seam's drop contract) when the connection
/// ends.
pub async fn run_stream_egress<T: MediaTransport>(
    transport: Arc<T>,
    mut downlinks: mpsc::Receiver<Arc<Downlink>>,
    cancel: CancellationToken,
) {
    let mut writers = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            next = downlinks.recv() => match next {
                Some(downlink) => {
                    writers.spawn(run_writer(Arc::clone(&transport), downlink));
                }
                None => break,
            },
            Some(_) = writers.join_next(), if !writers.is_empty() => {}
        }
    }
    // Downlinks handed over but never started belong to a subscriber that is
    // gone: lost, never left open to fill up and read as a backlog abandon.
    downlinks.close();
    while let Ok(downlink) = downlinks.try_recv() {
        downlink.mark_lost();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::metrics::resolve_media_handles;

    fn downlink(cap: usize) -> Arc<Downlink> {
        Arc::new(Downlink::new(
            Arc::new(BacklogBudget::new(cap)),
            quic_priority(2),
            Arc::new(resolve_media_handles()),
        ))
    }

    #[test]
    fn priority_group_one_outranks_two() {
        assert!(quic_priority(1) > quic_priority(2));
        assert_eq!(quic_priority(u32::MAX), i32::MIN);
    }

    #[test]
    fn the_budget_refuses_without_charging() {
        let budget = BacklogBudget::new(10);
        assert!(budget.try_charge(10));
        assert!(!budget.try_charge(1));
        assert_eq!(budget.used(), 10);
        budget.release(4);
        assert!(budget.try_charge(4));
    }

    #[test]
    fn a_chunk_is_zero_copy_only_while_no_other_uncopied_chunk_is_outstanding() {
        let d = downlink(1 << 20);
        let a = Bytes::from(vec![1_u8; 32]);
        let b = Bytes::from(vec![2_u8; 32]);
        d.push_chunk(&a).unwrap();
        d.push_chunk(&b).unwrap();
        let items = d.queued_items();
        assert_eq!(
            items.len(),
            1,
            "the second chunk was coalesced into the owned tail"
        );
        assert!(items[0].is_shared());
        assert_eq!(
            items[0].bytes().as_ptr(),
            a.as_ptr(),
            "first chunk is the same allocation"
        );
    }

    #[test]
    fn finish_requested_mid_frame_is_deferred_to_the_frame_end() {
        let d = downlink(1 << 20);
        d.push_chunk(&Bytes::from_static(b"x")).unwrap();
        {
            let mut inner = d.lock();
            inner.in_frame = true;
        }
        d.request_finish();
        assert_eq!(d.lock().phase, Phase::Open, "not yet: mid-frame");
        d.end_frame();
        assert_eq!(d.lock().phase, Phase::Finishing);
    }

    #[test]
    fn abandon_is_idempotent_and_releases_the_budget() {
        let budget = Arc::new(BacklogBudget::new(1 << 20));
        let d = Downlink::new(Arc::clone(&budget), 0, Arc::new(resolve_media_handles()));
        d.push_chunk(&Bytes::from(vec![0_u8; 100])).unwrap();
        assert_eq!(budget.used(), 100);
        assert!(d.abandon(GroupResetReason::SubscriberBacklog));
        assert!(!d.abandon(GroupResetReason::EdgeRemoved), "counted once");
        assert_eq!(budget.used(), 0);
        assert_eq!(
            d.push_chunk(&Bytes::from_static(b"y")),
            Err(PushError::Gone)
        );
    }

    #[test]
    fn past_the_cap_a_push_is_refused_as_backlog() {
        let d = downlink(10);
        d.push_chunk(&Bytes::from(vec![0_u8; 10])).unwrap();
        assert_eq!(
            d.push_chunk(&Bytes::from_static(b"z")),
            Err(PushError::Backlog)
        );
    }
}
