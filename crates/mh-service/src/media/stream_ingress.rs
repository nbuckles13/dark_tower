//! Stream-per-group INGRESS with cut-through egress (story 3 R-10, R-12, R-16,
//! R-17, R-20, R-21; ADR-0036 §1 amendment, §2, §7).
//!
//! ```text
//! accept loop (one per publisher connection):
//!     accept_uni -> TokenBucket (BEFORE any read) -> spawn group task
//! group task (one per publisher uni stream = one group):
//!     read_chunk -> HeaderReader -> attribute (task 8's lookup)
//!       -> per edge: edge machine -> hop -> rewritten header -> downlink
//!     tail chunks -> each frame's downlinks, refcounted, never awaited
//! ```
//!
//! # Cut-through, holding only a header and the chunk in flight
//!
//! The frame header is read through `media_protocol::reader::HeaderReader`
//! (the codec's own chunk-fed state machine; nothing here parses a field), so
//! `payload_length` is checked against `MAX_PAYLOAD_BYTES` from the 14-byte
//! prefix before anything is accepted, and nothing is ever sized from it. Per
//! stream MH holds the reader's inline header buffer and the chunk in flight;
//! the payload and signature stream through as `Bytes` sub-slices of what the
//! transport delivered. A chunk can end one frame, hold a whole next frame and
//! start a third: after each tail the reader is reset and fed the SAME chunk.
//!
//! # Attribution, per frame, never cached
//!
//! Every header runs `RoutingSnapshot::attribute` with the bound sender, the
//! uplink relay `stream_id` and the arrival transport (`STREAM_PER_GROUP`) —
//! task 8's single lookup; no second rule lives here. Outcomes:
//!
//! * `Route` — `forwarded{ingress}` +1 at header completion (once per frame,
//!   never per chunk or edge), the paused overlay if PAUSED, and fan-out.
//! * `ServerMuted` (`DROP_AT_INGRESS`) — counted per frame, the tail discarded,
//!   and the stream KEPT reading (still through the reader, re-attributed per
//!   frame, under the stall timer), so a lifted mute resumes on this stream at
//!   the next group start.
//! * `NoPolicy` — counted per frame, discarded, kept reading.
//! * `Unmatched` / `TransportMismatch` — counted ONCE, the stream STOPPED.
//! * A label differing from the stream's first frame — the stream is one group
//!   of one source — counted `unmatched_stream`, stopped.
//! * A codec reject (sticky) — counted by its codec token, stopped.
//!
//! # Every exit goes through ONE terminal function
//!
//! [`GroupState::terminate`] maps how the publisher stream ended to the
//! downstream action and the counters; the group's `Drop` routes an abort
//! (connection teardown, shutdown) through it too, so no `?`, cancel or panic
//! can skip it:
//!
//! | End | Ingress counter | Downstream streams this group fed |
//! |---|---|---|
//! | FIN at a frame boundary | — | FINISH |
//! | FIN / RESET / connection loss mid-header | `partial_frame_discard` | RESET `publisher_reset` (`source_ended` on loss) |
//! | FIN / RESET mid-tail | — (already forwarded) | RESET `publisher_reset` |
//! | stall past the bound | `partial_frame_discard` iff mid-header | RESET `publisher_stall`, stream stopped |
//! | refusal (codec, unmatched, mismatch, label change) | its token | RESET `publisher_reset` (none on a first frame) |
//! | MH cancel / abort | `partial_frame_discard` iff mid-header | RESET `source_ended` |
//!
//! # The stall timer is an INACTIVITY bound
//!
//! Armed only while a frame is incomplete — mid-HEADER (`is_mid_header`) or
//! mid-tail — and re-armed on every chunk of progress. That satisfies
//! `media_protocol::frame`'s caller obligation to bound "time-to-completion" in
//! its inactivity form, matching the task's "stalled mid-frame": a publisher
//! trickling a byte per bound keeps its frame open, but the damage is confined
//! to its own media and its resources are bounded (quinn's declared concurrent
//! uni streams × the 64 KiB stream window, two downlinks per edge, and the
//! copy-on-backlog charge in `crate::media::downlink`).
//!
//! # No telemetry macro here
//!
//! This file is under `media/`'s directory-scoped deny. Counting goes through
//! pre-resolved handles; nothing is logged.

use crate::config::StreamPathLimits;
use crate::media::caps::TokenBucket;
use crate::media::downlink::Downlink;
use crate::media::edge::GroupToken;
use crate::media::edge_runtime::{EdgeRuntime, HeaderIn};
use crate::media::ingress::LoopExit;
use crate::media::sampler::LatencySampler;
use crate::observability::metrics::{
    GroupResetReason, MediaDirection, MediaDropReason, MediaLatencyPhase, MediaMetricHandles,
};
use crate::routing::{Attribution, CandidateRef, MeetingKey, RoutingTable, SenderId};
use crate::session::LocalSubscribers;
use crate::transport::{MediaRecvStream, MediaTransport, StreamReadError, INGRESS_REFUSED};
use bytes::{Buf, Bytes};
use media_protocol::reader::{Feed, HeaderReader};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// The most bytes one `read_chunk` returns: one stream receive window. A
/// ceiling on what the transport hands over, never a buffer MH allocates.
pub const READ_CHUNK_MAX: usize = crate::config::STREAM_RECEIVE_WINDOW_BYTES as usize;

/// Process-unique ingress group tokens.
static NEXT_GROUP: AtomicU64 = AtomicU64::new(1);

/// What one publisher connection's stream ingress needs.
pub struct StreamIngressContext {
    /// The meeting the publisher is bound to.
    pub meeting: MeetingKey,
    /// The publisher, bound at spawn from the JWT-gated control plane.
    pub sender: SenderId,
    /// Re-read per frame.
    pub routing: Arc<RoutingTable>,
    /// Subscribers' downlink channels, re-read per frame.
    pub subscribers: Arc<LocalSubscribers>,
    /// Pre-resolved handles.
    pub handles: Arc<MediaMetricHandles>,
    /// Rate and stall bounds.
    pub limits: StreamPathLimits,
    /// The latency sample ratio (the same field the gauge publishes).
    pub sample_ratio: f64,
}

/// The accept loop: admit each publisher uni stream against the per-connection
/// creation-rate cap BEFORE reading a byte, then hand it to its own group task
/// (a loop-owned `JoinSet`, so one stalled stream never blocks the next
/// accept, and every group task is aborted — through its `Drop` guard — when
/// this loop returns).
pub async fn run_stream_ingress<T: MediaTransport>(
    transport: Arc<T>,
    context: Arc<StreamIngressContext>,
    cancel: CancellationToken,
) -> LoopExit {
    let rate = context.limits.opens_per_connection_per_sec;
    let mut bucket = TokenBucket::new(rate, rate, tokio::time::Instant::now());
    let mut groups = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            () = cancel.cancelled() => return LoopExit::Cancelled,
            accepted = transport.accept_uni() => match accepted {
                Ok(mut stream) => {
                    if bucket.admit(tokio::time::Instant::now()) {
                        groups.spawn(run_group(stream, Arc::clone(&context)));
                    } else {
                        // One per refused STREAM: a lower bound on frames lost.
                        stream.stop(INGRESS_REFUSED);
                        context
                            .handles
                            .dropped(MediaDropReason::StreamRateLimited)
                            .increment(1);
                    }
                }
                Err(_) => return LoopExit::ConnectionClosed,
            },
            Some(_) = groups.join_next(), if !groups.is_empty() => {}
        }
    }
}

/// How a publisher group stream ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Fin,
    Reset,
    Stall,
    ConnectionLost,
    Refused,
    Cancelled,
}

/// What `on_chunk` asks the read loop to do.
enum Step {
    Continue,
    Refuse,
}

/// The process-wide live-groups gauge, moved by RAII so every exit — panic and
/// abort included — decrements it.
struct GroupGauge(Arc<MediaMetricHandles>);

impl GroupGauge {
    fn new(handles: &Arc<MediaMetricHandles>) -> Self {
        handles.stream_ingress_groups().increment(1.0);
        Self(Arc::clone(handles))
    }
}

impl Drop for GroupGauge {
    fn drop(&mut self) {
        self.0.stream_ingress_groups().decrement(1.0);
    }
}

/// One edge's downlink a frame (or the group) is feeding.
type Lease = (Arc<EdgeRuntime>, Arc<Downlink>);

/// One publisher group stream's state.
struct GroupState {
    context: Arc<StreamIngressContext>,
    token: GroupToken,
    reader: HeaderReader,
    /// Tail bytes (payload + signature) of the frame in flight still to come.
    remaining: usize,
    /// Downlinks the frame in flight is feeding.
    frame: Vec<Lease>,
    /// Discard the frame in flight's tail (muted / no policy / unforwarded).
    discard: bool,
    /// Downlinks this group opened, for FIN (finish) and abnormal ends (reset).
    opened: Vec<Lease>,
    /// The first frame's uplink label: a group is one source.
    label: Option<u16>,
    /// Whether this stream has been counted as an attributed ingress group.
    started: bool,
    last_progress: tokio::time::Instant,
    sampler: LatencySampler,
    terminated: bool,
    _gauge: GroupGauge,
}

/// One publisher group stream, read until it ends.
async fn run_group<R: MediaRecvStream>(mut stream: R, context: Arc<StreamIngressContext>) {
    let stall = context.limits.mid_frame_stall;
    let mut group = GroupState::new(context);
    loop {
        let deadline = group.stall_deadline(stall);
        let read = tokio::select! {
            read = stream.read_chunk(READ_CHUNK_MAX) => read,
            () = sleep_until_or_forever(deadline) => {
                group.terminate(End::Stall);
                stream.stop(INGRESS_REFUSED);
                return;
            }
        };
        match read {
            Ok(Some(chunk)) => {
                if let Step::Refuse = group.on_chunk(chunk) {
                    group.terminate(End::Refused);
                    stream.stop(INGRESS_REFUSED);
                    return;
                }
            }
            Ok(None) => return group.terminate(End::Fin),
            Err(StreamReadError::Reset(_)) => return group.terminate(End::Reset),
            Err(StreamReadError::ConnectionClosed(_) | StreamReadError::Failed(_)) => {
                return group.terminate(End::ConnectionLost)
            }
        }
    }
}

async fn sleep_until_or_forever(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

impl GroupState {
    fn new(context: Arc<StreamIngressContext>) -> Self {
        let gauge = GroupGauge::new(&context.handles);
        let sampler = LatencySampler::new(context.sample_ratio);
        Self {
            token: NEXT_GROUP.fetch_add(1, Ordering::Relaxed),
            reader: HeaderReader::new(),
            remaining: 0,
            frame: Vec::new(),
            discard: false,
            opened: Vec::new(),
            label: None,
            started: false,
            last_progress: tokio::time::Instant::now(),
            sampler,
            terminated: false,
            _gauge: gauge,
            context,
        }
    }

    const fn mid_frame(&self) -> bool {
        self.reader.is_mid_header() || self.remaining > 0
    }

    fn stall_deadline(&self, stall: std::time::Duration) -> Option<tokio::time::Instant> {
        self.mid_frame().then(|| self.last_progress + stall)
    }

    /// Feed one transport chunk: header bytes into the reader, tail bytes to
    /// the frame's downlinks, as many frames as the chunk holds.
    fn on_chunk(&mut self, mut chunk: Bytes) -> Step {
        self.last_progress = tokio::time::Instant::now();
        while !chunk.is_empty() {
            if self.remaining > 0 {
                let take = self.remaining.min(chunk.len());
                let part = chunk.split_to(take);
                self.forward_tail(&part);
                self.remaining -= take;
                if self.remaining == 0 {
                    self.end_frame();
                }
                continue;
            }
            match self.reader.feed(&chunk) {
                Err(error) => {
                    // `RejectReason::as_str()` is the only thing that leaves:
                    // `DecodeError`'s `Display` carries per-frame sizes.
                    if let Some(counter) = self.context.handles.codec_dropped(error.reason()) {
                        counter.increment(1);
                    }
                    return Step::Refuse;
                }
                Ok(Feed::NeedMore) => return Step::Continue,
                Ok(Feed::Complete { header, consumed }) => {
                    chunk.advance(consumed.min(chunk.len()));
                    if let Step::Refuse = self.on_header(header) {
                        return Step::Refuse;
                    }
                    self.remaining = header.tail_len();
                    self.reader.reset();
                }
            }
        }
        Step::Continue
    }

    /// One complete header: attribute it and fan the rewritten header out.
    fn on_header(&mut self, header: media_protocol::frame::FrameHeader) -> Step {
        let context = Arc::clone(&self.context);
        let handles = &context.handles;
        let label = header.stream_id();
        if *self.label.get_or_insert(label) != label {
            handles
                .dropped(MediaDropReason::UnmatchedStream)
                .increment(1);
            return Step::Refuse;
        }
        let snapshot = context.routing.load();
        self.discard = true;
        let attribution = snapshot.attribute(
            &context.meeting,
            context.sender,
            label,
            TransportMode::StreamPerGroup,
        );
        let Attribution::Route {
            stream,
            paused,
            edges,
        } = attribution
        else {
            // One mapping for both paths (`crate::media::attribution_drop_reason`).
            // `no_policy` and `server_muted` keep reading (counted per frame);
            // `unmatched_stream` and `transport_mismatch` stop the stream.
            let Some(reason) = crate::media::attribution_drop_reason(&attribution) else {
                return Step::Continue;
            };
            handles.dropped(reason).increment(1);
            return match reason {
                MediaDropReason::NoPolicy | MediaDropReason::ServerMuted => Step::Continue,
                _ => Step::Refuse,
            };
        };
        // The complete header's bytes, read BEFORE anything is counted: a
        // complete header always has them, and if that ever broke it is an MH
        // bug — counted on its own token, the stream refused — never a frame
        // counted forwarded and then silently discarded.
        let Some(bytes) = self.reader.header_bytes() else {
            handles
                .dropped(MediaDropReason::RelayRewriteFailed)
                .increment(1);
            return Step::Refuse;
        };
        handles.forwarded(MediaDirection::Ingress).increment(1);
        if paused {
            handles.paused_source_frames().increment(1);
        }
        if !self.started {
            self.started = true;
            handles.group_starts(MediaDirection::Ingress).increment(1);
        }

        let header_at = self.sampler.should_sample().then(std::time::Instant::now);
        let incoming = HeaderIn {
            bytes,
            independently_decodable: header.flags().independently_decodable,
            source: CandidateRef {
                sender: context.sender,
                stream,
            },
            group: self.token,
            now: tokio::time::Instant::now(),
            sampled: header_at,
        };
        let subscribers = context.subscribers.load();
        let mut any_edge = false;
        for edge in edges {
            any_edge = true;
            let egress = subscribers.stream_egress(&context.meeting, edge.edge.subscriber.sender);
            if let Some(downlink) = edge
                .runtime
                .deliver_header(edge.edge, &incoming, egress, handles)
            {
                if !self.opened.iter().any(|(_, d)| Arc::ptr_eq(d, &downlink)) {
                    self.opened
                        .push((Arc::clone(edge.runtime), Arc::clone(&downlink)));
                }
                self.frame.push((Arc::clone(edge.runtime), downlink));
            }
        }
        if !any_edge {
            // Attributed, ingested, delivered nowhere: nobody watches this
            // stream. Frames GATED by an edge waiting for a group start are
            // deliberately uncounted (see the catalog).
            handles.dropped(MediaDropReason::NoSubscriber).increment(1);
        }
        if let Some(header_at) = header_at {
            if !self.frame.is_empty() {
                handles.latency(MediaLatencyPhase::Processing).record(
                    std::time::Instant::now()
                        .saturating_duration_since(header_at)
                        .as_secs_f64(),
                );
            }
        }
        self.discard = false;
        Step::Continue
    }

    /// Hand one tail slice to every downlink the frame feeds. Never awaits.
    fn forward_tail(&mut self, part: &Bytes) {
        if self.discard {
            return;
        }
        self.frame
            .retain(|(runtime, downlink)| match downlink.push_chunk(part) {
                Ok(()) => true,
                Err(error) => {
                    runtime.on_push_failed(downlink, error);
                    false
                }
            });
    }

    fn end_frame(&mut self) {
        for (_, downlink) in self.frame.drain(..) {
            downlink.end_frame();
        }
        self.discard = false;
        self.opened.retain(|(_, d)| !d.is_done());
    }

    /// The ONE terminal function (see the module table).
    fn terminate(&mut self, end: End) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        let handles = Arc::clone(&self.context.handles);
        let mid_header = self.reader.is_mid_header();
        let clean = end == End::Fin && !self.mid_frame();
        if mid_header
            && matches!(
                end,
                End::Fin | End::Reset | End::Stall | End::ConnectionLost | End::Cancelled
            )
        {
            handles
                .dropped(MediaDropReason::PartialFrameDiscard)
                .increment(1);
        }
        if clean {
            for (runtime, downlink) in self.opened.drain(..) {
                runtime.group_ended(&downlink);
            }
            return;
        }
        let reason = match end {
            End::Stall => GroupResetReason::PublisherStall,
            End::ConnectionLost | End::Cancelled => GroupResetReason::SourceEnded,
            End::Fin | End::Reset | End::Refused => GroupResetReason::PublisherReset,
        };
        for (runtime, downlink) in self.frame.drain(..).chain(self.opened.drain(..)) {
            runtime.abandon_downlink(&downlink, reason);
        }
    }
}

impl Drop for GroupState {
    fn drop(&mut self) {
        // An aborted group task (connection teardown, shutdown) never reached
        // a terminal step: route it through the same function.
        self.terminate(End::Cancelled);
    }
}
