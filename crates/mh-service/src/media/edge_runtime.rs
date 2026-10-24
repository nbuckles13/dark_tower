//! Per-edge runtime state, OUTSIDE the immutable routing snapshot (story 3
//! MH-A2; ADR-0036 §2, §7, §10).
//!
//! The routing snapshot (`crate::routing`) is immutable and replaced wholesale
//! on every apply. What an edge must REMEMBER across applies lives here, in one
//! [`EdgeRuntime`] cell per `egress_stream_id`, which the snapshot holds by
//! `Arc` and every successor snapshot carries over:
//!
//! * the downlink **hop sequence** — for BOTH transports, so it continues
//!   across group streams, a source switch and an MC slot refill (MC keeps
//!   `egress_stream_id` stable across a refill);
//! * for a stream-transport edge, its **switch state machine**
//!   (`crate::media::edge`), its **live downlinks** (at most two) and its
//!   per-destination **backlog budget**.
//!
//! An identical or unrelated re-push hands the SAME cell to the new snapshot,
//! so state and hop are untouched (ADR-0036 §10 re-assert idempotency).
//!
//! # Reconcile runs on apply and never awaits
//!
//! [`reconcile`] is called by `RoutingTable::install` (the session actor is its
//! only caller) BEFORE the new snapshot is published. It only mutates cells
//! and signals downlinks — `assign`, `abandon`, `request_finish` — all
//! synchronous; it never awaits a task or a channel:
//!
//! * a removed edge resets its downlinks, `edge_removed`;
//! * a forwarded source MC no longer declares resets, `source_ended`, and a
//!   pending switch target that disappeared abandons the switch (A kept);
//! * a forwarded source now `DROP_AT_INGRESS` stops being forwarded: its
//!   downlink finishes at its frame boundary (its frames no longer arrive);
//! * a candidate or disposition change becomes the machine's `assign`.
//!
//! # Pending and keyframe requests (story 3 MH-B; R-4, R-10, R-12)
//!
//! Every transition of a stream edge's machine happens inside one of this
//! module's `EdgeRuntime` methods, and EVERY such method ends in
//! `StreamEdge::settle`. That is what makes the two rules below hold by
//! construction rather than by each caller remembering them:
//!
//! * **Pending accounting.** An edge is pending while it WANTS a group start:
//!   its machine awaits one (`Waiting` on an assigned source, or
//!   `SwitchPending`), that source is declared `FORWARD`, and its subscriber
//!   has a downlink channel on THIS handler. Entry stamps the injected clock;
//!   each exit is counted exactly once on
//!   `mh_media_edge_pending_exits_total{outcome}` and observed once on
//!   `mh_media_edge_pending_duration_seconds`. The gauges are tick-sampled
//!   from these stamps (`crate::routing::RoutingTable::publish_pending`),
//!   never per frame.
//! * **MH-originated requests.** While an edge wants source S, `settle`
//!   asks S's publisher binding for a keyframe at MH's newest counter
//!   (`crate::media::keyframe::PublisherKeyframes::request`), which dedupes to
//!   once per (source stream, counter). An unseen source requests nothing.
//!   Because `settle` runs AFTER the machine consumed a frame, an edge whose
//!   incoming frame IS the group start has gone to `Forwarding` and requests
//!   nothing.
//!
//! An edge whose subscriber is NOT connected here neither counts as pending
//! nor requests: otherwise every group start, failing `no_local_subscriber`,
//! would re-enter `Waiting` at a newer counter and pump a request per group.
//! It enters pending when the subscriber connects
//! (`crate::routing::RoutingTable::subscriber_presence`) and leaves it
//! (`edge_removed`) when that connection goes away. Symmetrically, the wanted
//! source's PUBLISHER must be bound here: its departure ends the episode
//! (`abandoned_source_left`), its arrival
//! (`crate::routing::RoutingTable::publisher_bound`) starts one.
//!
//! # Locking
//!
//! Each cell's stream state is behind its own `std::sync::Mutex`, held only
//! for synchronous work and never across an `.await`. The datagram path never
//! takes it: its only touch is the hop's atomic `fetch_add`. While it is held
//! a per-source keyframe record's lock may be taken (edge → record), never
//! the reverse (`crate::media::keyframe`).
//!
//! # Counting
//!
//! Group resets are counted by `Downlink::abandon`, nowhere else. The frame
//! counters this module increments are `forwarded{egress}` (one per
//! destination at the rewritten-header push, the instant the hop is consumed),
//! `connection_closed` (on the frame that DISCOVERS a lost downlink, which is
//! then not pushed) and `no_local_subscriber`.

use crate::media::downlink::{quic_priority, BacklogBudget, Downlink, DownlinkSender, PushError};
use crate::media::edge::{AssignEffect, EdgeMachine, FrameDecision, GroupToken, SourceEndedEffect};
use crate::media::forwarder::HopSequence;
use crate::media::keyframe::{PublisherKeyframes, PublisherKeyframesRegistry};
use crate::observability::metrics::{
    GroupResetReason, KeyframeTrigger, MediaDirection, MediaDropReason, MediaMetricHandles,
    PendingExit,
};
use crate::routing::{CandidateRef, EgressEdge, MeetingKey, SenderId};
use crate::session::LocalSubscribers;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::mpsc::error::TrySendError;
use tokio::time::Instant;

/// What reconcile needs that the policy does not carry.
#[derive(Clone)]
pub struct EdgeContext {
    /// `MH_EGRESS_STREAM_BACKLOG_BYTES`: each stream edge's budget.
    pub backlog_cap: usize,
    /// Handles the downlinks count through.
    pub handles: Arc<MediaMetricHandles>,
    /// The subscriber registry: whether an edge's subscriber is connected to
    /// THIS handler (pending and requests require it).
    pub subscribers: Arc<LocalSubscribers>,
    /// Every publisher binding's per-source keyframe state.
    pub keyframes: Arc<PublisherKeyframesRegistry>,
}

impl EdgeContext {
    /// A context with a fresh subscriber registry and keyframe registry. The
    /// session handle and the media tasks read both FROM here, so there is one
    /// registry of each per routing table.
    #[must_use]
    pub fn new(backlog_cap: usize, handles: Arc<MediaMetricHandles>) -> Self {
        Self {
            backlog_cap,
            handles,
            subscribers: Arc::new(LocalSubscribers::new()),
            keyframes: Arc::new(PublisherKeyframesRegistry::new()),
        }
    }
}

impl std::fmt::Debug for EdgeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EdgeContext")
            .field("backlog_cap", &self.backlog_cap)
            .finish_non_exhaustive()
    }
}

/// How the routing layer describes a candidate's declaration to reconcile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    /// Not declared (any more).
    Undeclared,
    /// Declared `FORWARD`.
    Forward,
    /// Declared `PAUSED`.
    Paused,
    /// Declared `DROP_AT_INGRESS`, or its sender is wholly server-muted.
    Muted,
}

/// What every cell of one meeting shares: where to request keyframes and how
/// to count.
struct Shared {
    meeting: MeetingKey,
    keyframes: Arc<PublisherKeyframesRegistry>,
    handles: Arc<MediaMetricHandles>,
}

/// One egress edge's runtime cell.
pub struct EdgeRuntime {
    egress_stream_id: u32,
    hop: HopSequence,
    shared: Shared,
    stream: Mutex<Option<StreamEdge>>,
}

impl std::fmt::Debug for EdgeRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Identity-free: no source, no state, no counter.
        f.debug_struct("EdgeRuntime").finish_non_exhaustive()
    }
}

/// A stream-transport edge's switch and downlink state.
struct StreamEdge {
    machine: EdgeMachine,
    /// The downlink the current ingress group continues.
    current: Option<Arc<Downlink>>,
    /// Every downlink not yet done, oldest first; includes `current`.
    live: Vec<Arc<Downlink>>,
    budget: Arc<BacklogBudget>,
    /// The assigned target is declared `FORWARD`.
    target_forwardable: bool,
    /// The subscriber has a downlink channel on this handler.
    subscriber_present: bool,
    /// Pending since, and on which source.
    pending: Option<(Instant, CandidateRef)>,
    /// MH refused the forwarded source's NEXT group while this edge's
    /// current downlink was still live: when that downlink's group ends, wait
    /// (and request). See [`EdgeRuntime::on_source_group_refused`].
    repair_at_group_end: bool,
}

/// The most downlinks one edge keeps live: the current group and the
/// predecessor finishing its in-flight frame. The application bound on
/// concurrent unfinished egress groups is therefore this × a subscriber's
/// stream edges.
pub const MAX_LIVE_DOWNLINKS_PER_EDGE: usize = 2;

impl StreamEdge {
    fn new(
        target: Option<CandidateRef>,
        paused: bool,
        backlog_cap: usize,
        target_forwardable: bool,
        subscriber_present: bool,
    ) -> Self {
        Self {
            machine: EdgeMachine::new(target, paused),
            current: None,
            live: Vec::new(),
            budget: Arc::new(BacklogBudget::new(backlog_cap)),
            target_forwardable,
            subscriber_present,
            pending: None,
            repair_at_group_end: false,
        }
    }

    fn prune(&mut self) {
        self.live.retain(|d| !d.is_done());
    }

    fn abandon_all(&mut self, reason: GroupResetReason) {
        for downlink in self.live.drain(..) {
            downlink.abandon(reason);
        }
        self.current = None;
    }

    fn abandon_current(&mut self, reason: GroupResetReason) {
        if let Some(current) = self.current.take() {
            current.abandon(reason);
            self.live.retain(|d| !Arc::ptr_eq(d, &current));
        }
    }

    fn detach_current(&mut self) {
        if let Some(current) = self.current.take() {
            current.request_finish();
        }
    }

    /// The source this edge wants a group start from, if it is pending: the
    /// machine awaits one, the source is declared FORWARD, the subscriber is
    /// connected here, and the source's publisher is bound here.
    ///
    /// Returns the publisher binding's state with the target, so `settle`
    /// requests without a second registry lookup.
    fn wanted(&self, shared: &Shared) -> Option<(CandidateRef, Arc<PublisherKeyframes>)> {
        let target = self
            .machine
            .awaiting_group_start()
            .filter(|_| self.target_forwardable && self.subscriber_present)?;
        let keyframes = shared.keyframes.lookup(&shared.meeting, target.sender)?;
        Some((target, keyframes))
    }

    /// End a pending episode (if any) with `exit`: one count, one observation.
    fn end_pending(&mut self, shared: &Shared, now: Instant, exit: PendingExit) {
        if let Some((since, _)) = self.pending.take() {
            shared
                .handles
                .edge_pending_duration()
                .record(now.saturating_duration_since(since).as_secs_f64());
            shared.handles.edge_pending_exits(exit).increment(1);
        }
    }

    /// Run after EVERY transition (see the module docs). `exit` is the
    /// outcome if this transition ended the pending episode with nothing new
    /// wanted; a change of wanted source is always `superseded`.
    fn settle(&mut self, shared: &Shared, now: Instant, exit: PendingExit) {
        let wanted = self.wanted(shared);
        let wanted_target = wanted.as_ref().map(|(target, _)| *target);
        if let Some((_, pending_on)) = self.pending {
            if wanted_target != Some(pending_on) {
                let exit = if wanted.is_some() {
                    PendingExit::Superseded
                } else {
                    exit
                };
                self.end_pending(shared, now, exit);
            }
        }
        let Some((target, keyframes)) = wanted else {
            return;
        };
        if self.pending.is_none() {
            self.pending = Some((now, target));
        }
        // Once per (source stream, counter), deduped inside; an unseen source
        // requests nothing.
        let _ = keyframes.request(target.stream, KeyframeTrigger::Initial, None);
    }

    /// Wait for the next group start (an abandonment, or a refused group).
    fn abandoned(&mut self) {
        self.repair_at_group_end = false;
        self.machine.on_abandoned();
    }
}

impl StreamEdge {
    /// A group start: hand a fresh downlink to the subscriber's writer task
    /// and make it current; the predecessor finishes at its frame boundary.
    /// `None` (counted) when the destination cannot take one.
    fn open_downlink(
        &mut self,
        edge: &EgressEdge,
        egress: Option<&DownlinkSender>,
        handles: &Arc<MediaMetricHandles>,
    ) -> Option<Arc<Downlink>> {
        let Some(egress) = egress else {
            handles
                .dropped(MediaDropReason::NoLocalSubscriber)
                .increment(1);
            self.abandoned();
            return None;
        };
        let downlink = Arc::new(Downlink::new(
            Arc::clone(&self.budget),
            quic_priority(edge.priority_group),
            Arc::clone(handles),
        ));
        match egress.try_send(Arc::clone(&downlink)) {
            Ok(()) => {}
            // The subscriber's writer task is not draining: this destination
            // cannot keep up. Counted per downlink group abandoned: the new
            // one (never opened) plus every live one.
            Err(TrySendError::Full(_)) => {
                downlink.abandon(GroupResetReason::SubscriberBacklog);
                self.abandon_all(GroupResetReason::SubscriberBacklog);
                self.abandoned();
                return None;
            }
            Err(TrySendError::Closed(_)) => {
                handles
                    .dropped(MediaDropReason::ConnectionClosed)
                    .increment(1);
                self.abandoned();
                return None;
            }
        }
        self.detach_current();
        self.live.push(Arc::clone(&downlink));
        self.current = Some(Arc::clone(&downlink));
        // At most two live: an older predecessor still not done means this
        // destination is a group behind.
        while self.live.len() > MAX_LIVE_DOWNLINKS_PER_EDGE {
            let oldest = self.live.remove(0);
            oldest.abandon(GroupResetReason::SubscriberBacklog);
        }
        Some(downlink)
    }

    /// Push the rewritten header and map the outcome; every failure leaves
    /// the machine, the live set and the wire consistent.
    fn push_header(
        &mut self,
        downlink: &Arc<Downlink>,
        edge: &EgressEdge,
        header: &HeaderIn<'_>,
        hop: u32,
        handles: &Arc<MediaMetricHandles>,
    ) -> Option<Arc<Downlink>> {
        match downlink.push_header(
            header.bytes,
            edge.subscriber.slot.get(),
            hop,
            header.sampled,
        ) {
            Ok(()) => {
                handles.forwarded(MediaDirection::Egress).increment(1);
                return Some(Arc::clone(downlink));
            }
            Err(PushError::Backlog) => {
                self.abandon_all(GroupResetReason::SubscriberBacklog);
            }
            Err(PushError::Gone) => {
                if downlink.is_lost() {
                    handles
                        .dropped(MediaDropReason::ConnectionClosed)
                        .increment(1);
                }
                self.current = None;
            }
            Err(PushError::Rewrite(_)) => {
                // An already-parsed header the codec refused to rewrite: an MH
                // bug, never a sender fault, counted on its own token (should
                // read zero forever). The downlink must not carry a group
                // whose frame is missing: it is withdrawn (lost: the drop
                // contract resets any opened stream, uncounted as a group
                // abandonment — the bug is what is counted).
                handles
                    .dropped(MediaDropReason::RelayRewriteFailed)
                    .increment(1);
                downlink.mark_lost();
                self.live.retain(|d| !Arc::ptr_eq(d, downlink));
                if self
                    .current
                    .as_ref()
                    .is_some_and(|c| Arc::ptr_eq(c, downlink))
                {
                    self.current = None;
                }
            }
        }
        self.abandoned();
        None
    }
}

/// One frame header arriving for an edge.
#[derive(Debug, Clone, Copy)]
pub struct HeaderIn<'a> {
    /// The header bytes (publisher + relay region), as read.
    pub bytes: &'a [u8],
    /// The wire flag.
    pub independently_decodable: bool,
    /// The attributed source stream.
    pub source: CandidateRef,
    /// The ingress group it arrived on.
    pub group: GroupToken,
    /// Injected clock.
    pub now: Instant,
    /// Header-complete instant (MEASUREMENT clock) if this frame is sampled.
    pub sampled: Option<std::time::Instant>,
}

impl EdgeRuntime {
    fn new(egress_stream_id: u32, meeting: &MeetingKey, context: &EdgeContext) -> Self {
        Self {
            egress_stream_id,
            hop: HopSequence::new(),
            shared: Shared {
                meeting: meeting.clone(),
                keyframes: Arc::clone(&context.keyframes),
                handles: Arc::clone(&context.handles),
            },
            stream: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<StreamEdge>> {
        self.stream.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The MC-allocated identity this cell belongs to.
    #[must_use]
    pub const fn egress_stream_id(&self) -> u32 {
        self.egress_stream_id
    }

    /// The datagram path's hop number for the frame being rewritten now.
    pub fn take_hop(&self) -> u32 {
        self.hop.take()
    }

    /// The next hop number, without consuming it. Test-facing.
    #[must_use]
    pub fn peek_hop(&self) -> u32 {
        self.hop.peek()
    }

    /// The source this stream edge's machine awaits a group start from (see
    /// `EdgeMachine::awaiting_group_start`). Test-facing.
    #[must_use]
    pub fn awaiting_group_start(&self) -> Option<CandidateRef> {
        self.lock()
            .as_ref()
            .and_then(|edge| edge.machine.awaiting_group_start())
    }

    /// When this edge's current pending episode began, if it is pending. The
    /// tick sampler's input; never read per frame.
    #[must_use]
    pub fn pending_since(&self) -> Option<Instant> {
        self.lock()
            .as_ref()
            .and_then(|edge| edge.pending.map(|(since, _)| since))
    }

    /// Bytes this edge's destination backlog holds right now
    /// (`BacklogBudget::used`, one Acquire load); 0 for a cell with no stream
    /// edge. The tick sampler's input for `mh_media_egress_stream_backlog_bytes`;
    /// never read per frame or per chunk.
    #[must_use]
    pub fn backlog_used(&self) -> usize {
        self.lock().as_ref().map_or(0, |edge| edge.budget.used())
    }

    /// The edge machine's state. Test-facing.
    #[must_use]
    pub fn machine_state(&self) -> Option<crate::media::edge::EdgeState> {
        self.lock().as_ref().map(|edge| edge.machine.state())
    }

    /// Live downlinks right now. Test-facing.
    #[must_use]
    pub fn live_downlinks(&self) -> usize {
        self.lock().as_mut().map_or(0, |edge| {
            edge.prune();
            edge.live.len()
        })
    }

    /// The edge's current downlink, if any. Test-facing.
    #[must_use]
    pub fn current_downlink(&self) -> Option<Arc<Downlink>> {
        self.lock().as_ref().and_then(|edge| edge.current.clone())
    }

    /// Deliver one frame header to this edge: decide, open a downlink on a
    /// group start, take the hop, push the rewritten header — then settle
    /// (pending, and MH's keyframe request if the edge still waits). Returns
    /// the downlink the frame's tail must follow, or `None` if this edge does
    /// not carry the frame (in which case no hop number was consumed, except
    /// on the counted abandon paths).
    pub fn deliver_header(
        &self,
        edge: &EgressEdge,
        header: &HeaderIn<'_>,
        egress: Option<&DownlinkSender>,
        handles: &Arc<MediaMetricHandles>,
    ) -> Option<Arc<Downlink>> {
        let mut guard = self.lock();
        let stream_edge = guard.as_mut()?;
        stream_edge.prune();
        let present = egress.is_some();
        if stream_edge.subscriber_present != present {
            stream_edge.subscriber_present = present;
            stream_edge.settle(&self.shared, header.now, PendingExit::EdgeRemoved);
        }
        let carried = self.deliver_locked(stream_edge, edge, header, egress, handles);
        stream_edge.settle(&self.shared, header.now, PendingExit::GroupStart);
        carried
    }

    fn deliver_locked(
        &self,
        stream_edge: &mut StreamEdge,
        edge: &EgressEdge,
        header: &HeaderIn<'_>,
        egress: Option<&DownlinkSender>,
        handles: &Arc<MediaMetricHandles>,
    ) -> Option<Arc<Downlink>> {
        let lost = stream_edge.current.as_ref().is_some_and(|d| d.is_lost());
        if lost {
            stream_edge.current = None;
            stream_edge.abandoned();
        }
        let decision = stream_edge.machine.on_frame(
            header.source,
            header.group,
            header.independently_decodable,
        );
        let downlink = match decision {
            FrameDecision::Skip => {
                if lost {
                    // The frame that discovers a lost downlink and is not
                    // forwarded: one frame, one outcome.
                    handles
                        .dropped(MediaDropReason::ConnectionClosed)
                        .increment(1);
                }
                return None;
            }
            FrameDecision::Continue => stream_edge.current.clone()?,
            FrameDecision::StartGroup => {
                stream_edge.repair_at_group_end = false;
                stream_edge.open_downlink(edge, egress, handles)?
            }
        };
        // The number is consumed at REWRITE time (see `HopSequence`).
        let hop = self.hop.take();
        stream_edge.push_header(&downlink, edge, header, hop, handles)
    }

    /// A tail push to `downlink` was refused. Past the backlog cap the whole
    /// destination is abandoned (and the edge waits, requesting); a gone
    /// downlink needs nothing here.
    pub fn on_push_failed(&self, downlink: &Arc<Downlink>, error: PushError) {
        if error != PushError::Backlog {
            return;
        }
        let mut guard = self.lock();
        if let Some(stream_edge) = guard.as_mut() {
            if stream_edge.live.iter().any(|d| Arc::ptr_eq(d, downlink)) {
                stream_edge.abandon_all(GroupResetReason::SubscriberBacklog);
                stream_edge.abandoned();
                stream_edge.settle(&self.shared, Instant::now(), PendingExit::EdgeRemoved);
                return;
            }
        }
        drop(guard);
        downlink.abandon(GroupResetReason::SubscriberBacklog);
    }

    /// The ingress group that feeds `downlink` ended ABNORMALLY: reset it with
    /// the upstream cause. If it was the edge's current downlink, the edge
    /// waits for the next group start (and requests it).
    pub fn abandon_downlink(&self, downlink: &Arc<Downlink>, reason: GroupResetReason) {
        let mut guard = self.lock();
        if let Some(stream_edge) = guard.as_mut() {
            let was_current = stream_edge
                .current
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, downlink));
            stream_edge.live.retain(|d| !Arc::ptr_eq(d, downlink));
            if was_current {
                stream_edge.current = None;
                stream_edge.abandoned();
                stream_edge.settle(&self.shared, Instant::now(), PendingExit::EdgeRemoved);
            }
        }
        drop(guard);
        downlink.abandon(reason);
    }

    /// The ingress group that feeds `downlink` ended cleanly at a frame
    /// boundary: FIN it once drained. If MH refused the source's next group
    /// meanwhile, the edge now waits and requests at the refused counter.
    pub fn group_ended(&self, downlink: &Arc<Downlink>) {
        let mut guard = self.lock();
        if let Some(stream_edge) = guard.as_mut() {
            if stream_edge
                .current
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, downlink))
            {
                stream_edge.current = None;
                if stream_edge.repair_at_group_end {
                    stream_edge.abandoned();
                    stream_edge.settle(&self.shared, Instant::now(), PendingExit::EdgeRemoved);
                }
            }
        }
        drop(guard);
        downlink.request_finish();
    }

    /// MH refused (at the stream-open limit, after reading its header) a new
    /// group of `source` (story 3 amendment 4: whoever decides an abandonment
    /// repairs it — the publisher does not re-key for MH's refusal).
    ///
    /// An edge FORWARDING `source` would otherwise sit on its old group
    /// forever, so it is treated as abandoned by MH:
    /// * its old group already ended (`current` is `None`): wait now;
    /// * its old group's downlink is still live (mid-frame or between frames):
    ///   wait when that group ends ([`Self::group_ended`]; an abnormal end
    ///   already waits via [`Self::abandon_downlink`]). The old frames are
    ///   delivered intact; no downlink, counter or hop is touched here.
    ///
    /// Then `settle` requests at MH's newest counter — the refused group's —
    /// which the publisher honours (`>= current`).
    ///
    /// **Loop bound.** Each repair yields at most one publisher group per
    /// max(directed minimum keyframe interval, compiled 100 ms floor), and
    /// every cycle carries a strictly newer counter, deduped to one request
    /// per counter: at most 10 opens/s per stream at the floor (2/s at the
    /// 500 ms default) against the per-connection open bucket (default 10/s,
    /// range 1..=100). An honest publisher's refuse → request → key cycle
    /// therefore converges.
    ///
    /// **Hostile bound (security SEC-R1).** This walk (one edge lock and one
    /// settle per edge of the source) runs only for the FIRST refusal since
    /// the source's last ADMITTED group stream
    /// (`crate::media::keyframe::PublisherKeyframes::record_refused`), so
    /// walks are bounded by admitted streams + 1, which the open bucket
    /// bounds. Any further over-limit stream costs a header parse and one
    /// record lock, and at most one request per new counter written to the
    /// hostile publisher's OWN control stream.
    pub fn on_source_group_refused(&self, source: CandidateRef) {
        let mut guard = self.lock();
        let Some(stream_edge) = guard.as_mut() else {
            return;
        };
        if let crate::media::edge::EdgeState::Forwarding { source: s, .. } =
            stream_edge.machine.state()
        {
            if s == source {
                if stream_edge.current.is_some() {
                    stream_edge.repair_at_group_end = true;
                } else {
                    stream_edge.abandoned();
                }
            }
        }
        stream_edge.settle(&self.shared, Instant::now(), PendingExit::EdgeRemoved);
    }

    /// Every source of `sender` ended (its publisher connection closed): reset
    /// what it fed (`source_ended`), and abandon a pending switch to it.
    pub fn on_sender_ended(&self, sender: SenderId) {
        let mut guard = self.lock();
        let Some(stream_edge) = guard.as_mut() else {
            return;
        };
        let referenced = referenced_sources(&stream_edge.machine);
        for source in referenced.into_iter().flatten() {
            if source.sender == sender {
                apply_source_ended(stream_edge, source);
            }
        }
        stream_edge.settle(
            &self.shared,
            Instant::now(),
            PendingExit::AbandonedSourceLeft,
        );
    }

    /// A publisher binding appeared in this edge's meeting: re-judge whether
    /// the edge is pending (and request, if its source has been seen).
    pub fn resettle(&self) {
        let mut guard = self.lock();
        if let Some(stream_edge) = guard.as_mut() {
            stream_edge.settle(&self.shared, Instant::now(), PendingExit::EdgeRemoved);
        }
    }

    /// This edge's subscriber connected to (`true`) or left (`false`) this
    /// handler.
    pub fn set_subscriber_present(&self, present: bool) {
        let mut guard = self.lock();
        if let Some(stream_edge) = guard.as_mut() {
            stream_edge.subscriber_present = present;
            stream_edge.settle(&self.shared, Instant::now(), PendingExit::EdgeRemoved);
        }
    }

    /// Release this cell: its edge was removed, or its meeting released. The
    /// stream state is taken, so a frame still holding an older snapshot finds
    /// nothing to forward to and cannot re-enter pending.
    pub fn release(&self) {
        if let Some(mut stream_edge) = self.lock().take() {
            stream_edge.abandon_all(GroupResetReason::EdgeRemoved);
            stream_edge.end_pending(&self.shared, Instant::now(), PendingExit::EdgeRemoved);
        }
    }
}

/// The sources a machine currently refers to: forwarded and pending target.
fn referenced_sources(machine: &EdgeMachine) -> [Option<CandidateRef>; 2] {
    use crate::media::edge::EdgeState;
    match machine.state() {
        EdgeState::Forwarding { source, .. } | EdgeState::Paused { source } => [Some(source), None],
        EdgeState::SwitchPending {
            current, target, ..
        } => [Some(current), Some(target)],
        EdgeState::Waiting { target, .. } => [target, None],
    }
}

fn apply_source_ended(stream_edge: &mut StreamEdge, source: CandidateRef) {
    if stream_edge.machine.on_source_ended(source) == SourceEndedEffect::ResetCurrent {
        stream_edge.repair_at_group_end = false;
        stream_edge.abandon_current(GroupResetReason::SourceEnded);
    }
}

/// The pending exit a reconcile transition ends an episode with, when the
/// edge no longer wants anything: judged against the source it WAS pending on.
fn reconcile_exit(
    pending_on: Option<CandidateRef>,
    target: Option<CandidateRef>,
    status: impl Fn(CandidateRef) -> SourceStatus,
) -> PendingExit {
    match pending_on {
        Some(old) if Some(old) != target => PendingExit::Superseded,
        Some(old) => match status(old) {
            SourceStatus::Undeclared => PendingExit::AbandonedSourceLeft,
            SourceStatus::Paused | SourceStatus::Muted => PendingExit::PausedForwardNothing,
            // Still forwardable: the subscriber's connection left.
            SourceStatus::Forward => PendingExit::EdgeRemoved,
        },
        None => PendingExit::EdgeRemoved,
    }
}

/// The result of [`reconcile`].
#[derive(Debug)]
pub struct Reconciled {
    /// One cell per policy edge, aligned with `policy.edges`.
    pub cells: Vec<Arc<EdgeRuntime>>,
    /// `(source, edge index)` pairs the delivery index must ALSO carry: a
    /// stream edge still forwarding a source MC no longer assigns to it (a
    /// pending switch keeps A until B's group start).
    pub still_forwarding: Vec<(CandidateRef, usize)>,
    /// Sources some stream edge assigns with a `FORWARD` declaration: a
    /// queued keyframe request for any OTHER source is discarded (edge
    /// removed, PAUSED, `DROP_AT_INGRESS`).
    pub wanted_sources: HashSet<CandidateRef>,
}

/// Reconcile the runtime cells against a newly validated policy, carrying
/// every surviving cell over by `egress_stream_id`. See the module docs.
///
/// `previous` is the installed meeting's `(edges, cells)`, `status` resolves a
/// candidate against the NEW policy's declarations.
pub fn reconcile(
    meeting: &MeetingKey,
    previous: Option<(&[EgressEdge], &[Arc<EdgeRuntime>])>,
    edges: &[EgressEdge],
    status: impl Fn(CandidateRef) -> SourceStatus,
    context: &EdgeContext,
) -> Reconciled {
    let mut carried: HashMap<u32, Arc<EdgeRuntime>> = previous
        .map(|(_, cells)| {
            cells
                .iter()
                .map(|cell| (cell.egress_stream_id, Arc::clone(cell)))
                .collect()
        })
        .unwrap_or_default();
    let subscribers = context.subscribers.load();
    let now = Instant::now();

    let mut cells = Vec::with_capacity(edges.len());
    let mut still_forwarding = Vec::new();
    let mut wanted_sources = HashSet::new();
    for (index, edge) in edges.iter().enumerate() {
        let cell = carried
            .remove(&edge.egress_stream_id)
            .unwrap_or_else(|| Arc::new(EdgeRuntime::new(edge.egress_stream_id, meeting, context)));
        let is_stream = edge.transport == Some(TransportMode::StreamPerGroup);
        {
            let mut guard = cell.lock();
            if is_stream {
                let target = edge.candidates.first().copied();
                let target_status = target.map_or(SourceStatus::Undeclared, &status);
                let paused = target_status == SourceStatus::Paused;
                let forwardable = target_status == SourceStatus::Forward;
                let present = subscribers
                    .stream_egress(meeting, edge.subscriber.sender)
                    .is_some();
                let stream_edge = guard.get_or_insert_with(|| {
                    StreamEdge::new(target, paused, context.backlog_cap, forwardable, present)
                });
                let pending_on = stream_edge.pending.map(|(_, source)| source);
                // Sources the machine refers to that the new policy dropped or
                // muted.
                for source in referenced_sources(&stream_edge.machine)
                    .into_iter()
                    .flatten()
                {
                    match status(source) {
                        SourceStatus::Undeclared => apply_source_ended(stream_edge, source),
                        SourceStatus::Muted
                            if stream_edge.machine.forwarding_source() == Some(source) =>
                        {
                            // Its frames no longer arrive: finish the group at
                            // its frame boundary and wait.
                            stream_edge.detach_current();
                            stream_edge.abandoned();
                        }
                        _ => {}
                    }
                }
                if stream_edge.machine.assign(target, paused) == AssignEffect::Detach {
                    stream_edge.detach_current();
                }
                stream_edge.target_forwardable = forwardable;
                stream_edge.subscriber_present = present;
                let exit = reconcile_exit(pending_on, target, &status);
                stream_edge.settle(&cell.shared, now, exit);
                if let Some(forwarding) = stream_edge.machine.forwarding_source() {
                    if Some(forwarding) != target {
                        still_forwarding.push((forwarding, index));
                    }
                }
                if let (Some(target), true) = (target, forwardable) {
                    wanted_sources.insert(target);
                }
            } else if let Some(mut stream_edge) = guard.take() {
                // The edge stopped being a stream edge: its groups end.
                stream_edge.abandon_all(GroupResetReason::EdgeRemoved);
                stream_edge.end_pending(&cell.shared, now, PendingExit::EdgeRemoved);
            }
        }
        cells.push(cell);
    }
    for (_, removed) in carried {
        removed.release();
    }
    Reconciled {
        cells,
        still_forwarding,
        wanted_sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::metrics::resolve_media_handles;
    use crate::routing::{SlotId, StreamNumber, SubscriberRef};

    fn src(sender: u32) -> CandidateRef {
        CandidateRef {
            sender: SenderId::from_wire(sender).unwrap(),
            stream: StreamNumber::from_wire(1).unwrap(),
        }
    }

    fn edge(id: u32, candidate: u32) -> EgressEdge {
        EgressEdge {
            egress_stream_id: id,
            subscriber: SubscriberRef {
                sender: SenderId::from_wire(9).unwrap(),
                slot: SlotId::from_wire(4).unwrap(),
            },
            candidates: vec![src(candidate)],
            priority_group: 2,
            supersede_on_independent_frame: true,
            transport: Some(TransportMode::StreamPerGroup),
        }
    }

    fn context() -> EdgeContext {
        EdgeContext::new(1 << 20, Arc::new(resolve_media_handles()))
    }

    fn meeting() -> MeetingKey {
        MeetingKey::new("m")
    }

    #[test]
    fn a_re_push_carries_the_same_cell_and_its_hop() {
        let ctx = context();
        let edges = vec![edge(10, 1), edge(11, 2)];
        let first = reconcile(&meeting(), None, &edges, |_| SourceStatus::Forward, &ctx);
        first.cells[0].take_hop();
        first.cells[0].take_hop();
        let second = reconcile(
            &meeting(),
            Some((&edges, &first.cells)),
            &edges,
            |_| SourceStatus::Forward,
            &ctx,
        );
        assert!(Arc::ptr_eq(&first.cells[0], &second.cells[0]));
        assert_eq!(second.cells[0].peek_hop(), 2, "hop continues");
        assert_eq!(second.cells[1].peek_hop(), 0, "each edge counts on its own");
    }

    #[test]
    fn a_switch_keeps_the_old_source_in_the_delivery_index() {
        let ctx = context();
        let before = vec![edge(10, 1)];
        let first = reconcile(&meeting(), None, &before, |_| SourceStatus::Forward, &ctx);
        {
            let mut guard = first.cells[0].lock();
            let se = guard.as_mut().unwrap();
            se.machine.on_frame(src(1), 1, true);
        }
        let after = vec![edge(10, 2)];
        let second = reconcile(
            &meeting(),
            Some((&before, &first.cells)),
            &after,
            |_| SourceStatus::Forward,
            &ctx,
        );
        assert_eq!(second.still_forwarding, vec![(src(1), 0)]);
    }

    #[test]
    fn a_vanished_pending_target_keeps_the_current_source() {
        let ctx = context();
        let before = vec![edge(10, 1)];
        let first = reconcile(&meeting(), None, &before, |_| SourceStatus::Forward, &ctx);
        {
            let mut guard = first.cells[0].lock();
            guard.as_mut().unwrap().machine.on_frame(src(1), 1, true);
        }
        let after = vec![edge(10, 2)];
        let second = reconcile(
            &meeting(),
            Some((&before, &first.cells)),
            &after,
            |_| SourceStatus::Forward,
            &ctx,
        );
        second.cells[0].on_sender_ended(SenderId::from_wire(2).unwrap());
        assert_eq!(
            second.cells[0].machine_state(),
            Some(crate::media::edge::EdgeState::SwitchPending {
                current: src(1),
                group: 1,
                target: src(2),
            }),
            "A kept, still pending on B"
        );
    }
}
