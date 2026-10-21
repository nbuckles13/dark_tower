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
//! # Locking
//!
//! Each cell's stream state is behind its own `std::sync::Mutex`, held only
//! for synchronous work and never across an `.await`. The datagram path never
//! takes it: its only touch is the hop's atomic `fetch_add`.
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
use crate::observability::metrics::{
    GroupResetReason, MediaDirection, MediaDropReason, MediaMetricHandles,
};
use crate::routing::{CandidateRef, EgressEdge, SenderId};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::mpsc::error::TrySendError;

/// What reconcile needs that the policy does not carry.
#[derive(Clone)]
pub struct EdgeContext {
    /// `MH_EGRESS_STREAM_BACKLOG_BYTES`: each stream edge's budget.
    pub backlog_cap: usize,
    /// Handles the downlinks count through.
    pub handles: Arc<MediaMetricHandles>,
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

/// One egress edge's runtime cell.
pub struct EdgeRuntime {
    egress_stream_id: u32,
    hop: HopSequence,
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
}

/// The most downlinks one edge keeps live: the current group and the
/// predecessor finishing its in-flight frame. The application bound on
/// concurrent unfinished egress groups is therefore this × a subscriber's
/// stream edges.
pub const MAX_LIVE_DOWNLINKS_PER_EDGE: usize = 2;

impl StreamEdge {
    fn new(target: Option<CandidateRef>, paused: bool, backlog_cap: usize) -> Self {
        Self {
            machine: EdgeMachine::new(target, paused),
            current: None,
            live: Vec::new(),
            budget: Arc::new(BacklogBudget::new(backlog_cap)),
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
            self.machine.on_abandoned();
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
                self.machine.on_abandoned();
                return None;
            }
            Err(TrySendError::Closed(_)) => {
                handles
                    .dropped(MediaDropReason::ConnectionClosed)
                    .increment(1);
                self.machine.on_abandoned();
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
        self.machine.on_abandoned();
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
    pub now: tokio::time::Instant,
    /// Header-complete instant (MEASUREMENT clock) if this frame is sampled.
    pub sampled: Option<std::time::Instant>,
}

impl EdgeRuntime {
    fn new(egress_stream_id: u32) -> Self {
        Self {
            egress_stream_id,
            hop: HopSequence::new(),
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

    /// MH-B's keyframe-request input: `Some(first seen)` while this stream
    /// edge waits for a group start from a source already seen.
    #[must_use]
    pub fn waiting_with_source_seen(&self) -> Option<tokio::time::Instant> {
        self.lock()
            .as_ref()
            .and_then(|edge| edge.machine.waiting_with_source_seen())
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
    /// group start, take the hop, push the rewritten header. Returns the
    /// downlink the frame's tail must follow, or `None` if this edge does not
    /// carry the frame (in which case no hop number was consumed, except on
    /// the counted abandon paths).
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

        let lost = stream_edge.current.as_ref().is_some_and(|d| d.is_lost());
        if lost {
            stream_edge.current = None;
            stream_edge.machine.on_abandoned();
        }
        let decision = stream_edge.machine.on_frame(
            header.source,
            header.group,
            header.independently_decodable,
            header.now,
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
            FrameDecision::StartGroup => stream_edge.open_downlink(edge, egress, handles)?,
        };
        // The number is consumed at REWRITE time (see `HopSequence`).
        let hop = self.hop.take();
        stream_edge.push_header(&downlink, edge, header, hop, handles)
    }

    /// A tail push to `downlink` was refused. Past the backlog cap the whole
    /// destination is abandoned; a gone downlink needs nothing here.
    pub fn on_push_failed(&self, downlink: &Arc<Downlink>, error: PushError) {
        if error != PushError::Backlog {
            return;
        }
        let mut guard = self.lock();
        if let Some(stream_edge) = guard.as_mut() {
            if stream_edge.live.iter().any(|d| Arc::ptr_eq(d, downlink)) {
                stream_edge.abandon_all(GroupResetReason::SubscriberBacklog);
                stream_edge.machine.on_abandoned();
                return;
            }
        }
        drop(guard);
        downlink.abandon(GroupResetReason::SubscriberBacklog);
    }

    /// The ingress group that feeds `downlink` ended ABNORMALLY: reset it with
    /// the upstream cause. If it was the edge's current downlink, the edge
    /// waits for the next group start.
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
                stream_edge.machine.on_abandoned();
            }
        }
        drop(guard);
        downlink.abandon(reason);
    }

    /// The ingress group that feeds `downlink` ended cleanly at a frame
    /// boundary: FIN it once drained.
    pub fn group_ended(&self, downlink: &Arc<Downlink>) {
        let mut guard = self.lock();
        if let Some(stream_edge) = guard.as_mut() {
            if stream_edge
                .current
                .as_ref()
                .is_some_and(|c| Arc::ptr_eq(c, downlink))
            {
                stream_edge.current = None;
            }
        }
        drop(guard);
        downlink.request_finish();
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
    }

    /// Release this cell: its edge was removed, or its meeting released.
    pub fn release(&self) {
        if let Some(stream_edge) = self.lock().as_mut() {
            stream_edge.abandon_all(GroupResetReason::EdgeRemoved);
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
        stream_edge.abandon_current(GroupResetReason::SourceEnded);
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
}

/// Reconcile the runtime cells against a newly validated policy, carrying
/// every surviving cell over by `egress_stream_id`. See the module docs.
///
/// `previous` is the installed meeting's `(edges, cells)`, `status` resolves a
/// candidate against the NEW policy's declarations.
pub fn reconcile(
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

    let mut cells = Vec::with_capacity(edges.len());
    let mut still_forwarding = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        let cell = carried
            .remove(&edge.egress_stream_id)
            .unwrap_or_else(|| Arc::new(EdgeRuntime::new(edge.egress_stream_id)));
        let is_stream = edge.transport == Some(TransportMode::StreamPerGroup);
        {
            let mut guard = cell.lock();
            if is_stream {
                let target = edge.candidates.first().copied();
                let target_status = target.map_or(SourceStatus::Undeclared, &status);
                let paused = target_status == SourceStatus::Paused;
                let stream_edge = guard
                    .get_or_insert_with(|| StreamEdge::new(target, paused, context.backlog_cap));
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
                            stream_edge.machine.on_abandoned();
                        }
                        _ => {}
                    }
                }
                if stream_edge.machine.assign(target, paused) == AssignEffect::Detach {
                    stream_edge.detach_current();
                }
                if let Some(forwarding) = stream_edge.machine.forwarding_source() {
                    if Some(forwarding) != target {
                        still_forwarding.push((forwarding, index));
                    }
                }
            } else if let Some(mut stream_edge) = guard.take() {
                // The edge stopped being a stream edge: its groups end.
                stream_edge.abandon_all(GroupResetReason::EdgeRemoved);
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
        EdgeContext {
            backlog_cap: 1 << 20,
            handles: Arc::new(resolve_media_handles()),
        }
    }

    #[test]
    fn a_re_push_carries_the_same_cell_and_its_hop() {
        let ctx = context();
        let edges = vec![edge(10, 1), edge(11, 2)];
        let first = reconcile(None, &edges, |_| SourceStatus::Forward, &ctx);
        first.cells[0].take_hop();
        first.cells[0].take_hop();
        let second = reconcile(
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
        let first = reconcile(None, &before, |_| SourceStatus::Forward, &ctx);
        {
            let mut guard = first.cells[0].lock();
            let se = guard.as_mut().unwrap();
            se.machine
                .on_frame(src(1), 1, true, tokio::time::Instant::now());
        }
        let after = vec![edge(10, 2)];
        let second = reconcile(
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
        let first = reconcile(None, &before, |_| SourceStatus::Forward, &ctx);
        {
            let mut guard = first.cells[0].lock();
            guard
                .as_mut()
                .unwrap()
                .machine
                .on_frame(src(1), 1, true, tokio::time::Instant::now());
        }
        let after = vec![edge(10, 2)];
        let second = reconcile(
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
                seen_at: None,
            }),
            "A kept, still pending on B"
        );
    }
}
