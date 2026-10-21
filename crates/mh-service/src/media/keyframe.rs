//! Per-source `GoP` state and on-demand keyframes (story 3 R-4, R-9, R-10,
//! R-11, R-12, R-13; story Design §Wire freeze, Amendment 4; ADR-0036 §1, §7
//! amendments of 2026-10-21).
//!
//! # One record per (publisher binding, source stream) — THE single source
//!
//! [`PublisherKeyframes`] is owned by ONE publisher connection (its binding):
//! created when the media session starts, held by that connection's stream
//! ingress, registered in the meeting-scoped [`PublisherKeyframesRegistry`]
//! and unregistered (compare-and-remove on the connection id) at teardown. So
//! a story-2 R-16 sender-id reissue or a same-participant reconnect starts
//! with a fresh object by construction — no counter and no dedupe record is
//! ever inherited across bindings — and unbind frees everything by `Drop`.
//!
//! Inside it, one [`SourceRecord`] per source stream:
//!
//! * `newest` — the HIGHEST `gop_counter` read from any header of that stream
//!   (forwarded, server-muted, unrouted or paused frames alike, and a stream
//!   MH stopped at the open-rate limit after reading its header). The record
//!   existing IS "seen since bind"; there is no `0` sentinel (0 is a legal
//!   counter).
//!
//!   **Max, not last-read.** With cut-through, group N's stream can still be
//!   delivering frames after group N+1's header was read; last-read would
//!   regress to N, MH's request would name a counter the publisher treats as
//!   stale, and the slot would freeze silently. The counter never wraps (the
//!   stream stops at `u32::MAX`, ADR-0036 §2), so max is safe for an honest
//!   publisher. A HOSTILE publisher regressing or jumping its own counter can
//!   only make its own requests stale or unanswerable, and its own
//!   subscribers' recovery requests `stale` / `counter_ahead`: the harm is
//!   confined to its own media.
//! * `sent` — the last counter actually written (or counted failed) for it.
//! * `queued` — the newest-only outbound slot: at most one unsent request per
//!   stream; a newer counter REPLACES a queued older one.
//!
//! "Already requested c" means `sent == c` or `queued == c`. Every request for
//! a source goes through ONE method, [`PublisherKeyframes::request`] — MH's
//! own (an edge waiting for a group start) and a remapped receiver
//! `RecoveryRequest` alike — so each (source stream, counter) yields at most
//! one `KeyframeRequest` to the publisher. There is NO retry and NO timer: the
//! request rides a reliable stream, so repeating a counter achieves nothing,
//! and a later loss is a new event with a newer counter.
//!
//! # Bounded by construction
//!
//! A record is created only for a label the routing snapshot's single
//! attribution lookup resolved to a DECLARED stream of the bound sender
//! (`Route` or `ServerMuted`), never from a raw header label. So the records
//! are at most that sender's declared streams, which
//! `MH_MAX_SOURCE_STREAMS_PER_MEETING` bounds (and the 8-bit
//! [`StreamNumber`] bounds absolutely). Records for streams MC stops declaring
//! are freed on every apply ([`PublisherKeyframes::reconcile`]).
//!
//! # Lock order
//!
//! **An edge's lock (`crate::media::edge_runtime`) may be held while a
//! record's lock is taken; NEVER the reverse.** Nothing in this module takes
//! an edge lock, and the writer task holds no lock across an `.await`.
//!
//! # No telemetry macro here
//!
//! Under `media/`'s directory-scoped deny: counting goes through pre-resolved
//! handles, nothing is logged, and no counter value, identity or timing
//! leaves this module.

use crate::observability::metrics::{KeyframeTrigger, MediaMetricHandles, RecoveryOutcome};
use crate::routing::{MeetingKey, RoutingSnapshot, RoutingTable, SenderId, StreamNumber};
use crate::transport::framing::{
    encode_framed, FrameEnd, Framed, FramedReader, CONTROL_PARSE_BYTES,
};
use crate::transport::{MediaRecvStream, MediaSendStream, StreamReadError};
use arc_swap::ArcSwap;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    mh_client_message, mh_server_message, KeyframeRequest, MhClientMessage, MhServerMessage,
    RecoveryRequest,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// One source stream's `GoP` state. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceRecord {
    newest: u32,
    sent: Option<u32>,
    queued: Option<(u32, KeyframeTrigger)>,
    /// A group of this stream was refused at the open-rate limit since the
    /// last ADMITTED group stream: later refusals skip the edge walk.
    refused_since_admit: bool,
}

impl SourceRecord {
    fn already_requested(&self, counter: u32) -> bool {
        self.sent == Some(counter) || self.queued.is_some_and(|(c, _)| c == counter)
    }
}

#[derive(Debug, Default)]
struct Inner {
    records: HashMap<StreamNumber, SourceRecord>,
    /// The control stream failed: nothing more can be delivered. Every later
    /// request is marked sent and counted a write failure at once, so nothing
    /// queues forever.
    failed: bool,
}

/// What [`PublisherKeyframes::record_refused`] asks of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The first refusal since the last admitted group: walk the source's
    /// edges (repair them by request).
    RepairEdges,
    /// A later refusal: nothing to walk (any follow-up request was made).
    Coalesced,
}

/// What [`PublisherKeyframes::request`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOutcome {
    /// One request queued for the writer (newest-only: a newer counter may
    /// still replace it before it is written).
    Queued,
    /// Would have been queued, but this publisher's control stream already
    /// failed: counted undeliverable at once, never queued or re-emitted.
    Undeliverable,
    /// That (source stream, counter) was already requested: nothing sent.
    AlreadyRequested,
    /// No frame from the stream since bind: MH has no counter, nothing sent.
    /// The publisher keys every target with no group in progress.
    NotSeen,
    /// The caller's expected counter is LOWER than MH's newest.
    Stale,
    /// The caller's expected counter is HIGHER than MH's newest.
    Ahead,
}

/// One publisher connection's per-source `GoP` state and its outbound
/// newest-only request slots. See the module docs.
pub struct PublisherKeyframes {
    inner: Mutex<Inner>,
    /// Wakes the writer task when a slot is filled.
    wake: Notify,
    /// Refused-group edge walks (test-facing).
    refusal_walks: AtomicU64,
    handles: Arc<MediaMetricHandles>,
}

impl std::fmt::Debug for PublisherKeyframes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No stream, no counter.
        f.debug_struct("PublisherKeyframes").finish_non_exhaustive()
    }
}

impl PublisherKeyframes {
    /// A fresh binding's state: nothing seen.
    #[must_use]
    pub fn new(handles: Arc<MediaMetricHandles>) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            wake: Notify::new(),
            refusal_walks: AtomicU64::new(0),
            handles,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record one header's counter for an ATTRIBUTED, declared stream. A
    /// queued request older than the new newest is discarded: a group start
    /// superseded it (the counter rises on every independently-decodable
    /// frame), and the publisher would ignore it as stale anyway.
    pub fn record(&self, stream: StreamNumber, counter: u32) {
        let mut inner = self.lock();
        Self::record_locked(&mut inner, stream, counter);
    }

    fn record_locked(inner: &mut Inner, stream: StreamNumber, counter: u32) -> &mut SourceRecord {
        let record = inner.records.entry(stream).or_insert(SourceRecord {
            newest: counter,
            sent: None,
            queued: None,
            refused_since_admit: false,
        });
        record.newest = record.newest.max(counter);
        if record.queued.is_some_and(|(c, _)| c < record.newest) {
            record.queued = None;
        }
        record
    }

    /// [`Self::record`] for the FIRST header of an ADMITTED group stream: also
    /// re-arms the refused-group edge walk (see [`Self::record_refused`]).
    /// Called once per admitted stream, so the walks it re-arms are bounded
    /// by the stream-open bucket.
    pub fn record_admitted_group(&self, stream: StreamNumber, counter: u32) {
        let mut inner = self.lock();
        Self::record_locked(&mut inner, stream, counter).refused_since_admit = false;
    }

    /// Record the counter of a group MH REFUSED at the stream-open limit
    /// (story 3 amendment 4), and say whether the caller must walk the
    /// source's edges to repair them
    /// (`crate::routing::RoutingTable::source_group_refused`).
    ///
    /// **Coalesced (security SEC-R1).** Only the FIRST refusal since the last
    /// admitted group walks: no edge can newly start forwarding this source
    /// without an admitted group start, which re-arms the walk, and every
    /// other change to an edge's wants runs its own settle. Every later
    /// refusal costs O(1) — one header parse and this one lock — and only
    /// follows up the chain: if MH requested the PREVIOUS newest counter
    /// (someone still wants a group), it requests the new one, so an honest
    /// publisher whose answer is refused again (c1 refused -> request c1 ->
    /// c2 refused) is still asked for c2.
    pub fn record_refused(&self, stream: StreamNumber, counter: u32) -> Refusal {
        let follow_up = {
            let mut inner = self.lock();
            let previous = inner
                .records
                .get(&stream)
                .map(|r| (r.newest, r.already_requested(r.newest)));
            let record = Self::record_locked(&mut inner, stream, counter);
            if !record.refused_since_admit {
                record.refused_since_admit = true;
                drop(inner);
                self.refusal_walks.fetch_add(1, Ordering::Relaxed);
                return Refusal::RepairEdges;
            }
            let newest = record.newest;
            previous.is_some_and(|(prev, requested)| requested && newest > prev)
        };
        if follow_up {
            let _ = self.request(stream, KeyframeTrigger::Initial, None);
        }
        Refusal::Coalesced
    }

    /// Refused-group edge walks so far. Test-facing (SEC-R1's bound).
    #[must_use]
    pub fn refusal_walks(&self) -> u64 {
        self.refusal_walks.load(Ordering::Relaxed)
    }

    /// MH's newest counter for `stream`, or `None` if never seen since bind.
    #[must_use]
    pub fn newest(&self, stream: StreamNumber) -> Option<u32> {
        self.lock().records.get(&stream).map(|r| r.newest)
    }

    /// THE one dedupe method: request a keyframe for `stream` at MH's newest
    /// counter, at most once per (stream, counter).
    ///
    /// `expect` is a receiver's counter (item 3's equality rule, judged under
    /// the same lock as the dedupe so the two cannot race); `None` for MH's
    /// own request. Non-blocking: fills the newest-only slot and wakes the
    /// writer; never awaits a write.
    pub fn request(
        &self,
        stream: StreamNumber,
        trigger: KeyframeTrigger,
        expect: Option<u32>,
    ) -> RequestOutcome {
        let mut inner = self.lock();
        let failed = inner.failed;
        let Some(record) = inner.records.get_mut(&stream) else {
            return RequestOutcome::NotSeen;
        };
        let counter = record.newest;
        match expect {
            Some(expected) if expected < counter => return RequestOutcome::Stale,
            Some(expected) if expected > counter => return RequestOutcome::Ahead,
            _ => {}
        }
        if record.already_requested(counter) {
            return RequestOutcome::AlreadyRequested;
        }
        if failed {
            // Undeliverable: counted once, never queued, never re-emitted.
            record.sent = Some(counter);
            drop(inner);
            self.handles.keyframe_request_write_failures().increment(1);
            return RequestOutcome::Undeliverable;
        }
        record.queued = Some((counter, trigger));
        drop(inner);
        self.wake.notify_one();
        RequestOutcome::Queued
    }

    /// Apply a new policy to this binding's records: free every record whose
    /// stream is no longer `declared` (unassign), and discard the queued,
    /// unsent request (`sent` untouched, so the same counter can be requested
    /// again if wanted again — it was never sent)
    /// request of every stream no stream edge wants any more (`wanted`).
    pub fn reconcile(
        &self,
        declared: impl Fn(StreamNumber) -> bool,
        wanted: impl Fn(StreamNumber) -> bool,
    ) {
        let mut inner = self.lock();
        inner.records.retain(|stream, _| declared(*stream));
        for (stream, record) in &mut inner.records {
            if !wanted(*stream) {
                record.queued = None;
            }
        }
    }

    /// Free every record (the meeting was released).
    pub fn clear(&self) {
        self.lock().records.clear();
    }

    /// Records held right now. Test-facing (the bound and the free hooks).
    #[must_use]
    pub fn record_count(&self) -> usize {
        self.lock().records.len()
    }

    /// Whether `stream` has a queued, unsent request. Test-facing.
    #[must_use]
    pub fn has_queued(&self, stream: StreamNumber) -> bool {
        self.lock()
            .records
            .get(&stream)
            .is_some_and(|r| r.queued.is_some())
    }

    /// Take one queued request for the writer, lock released on return.
    fn take_next(&self) -> Option<(StreamNumber, u32, KeyframeTrigger)> {
        let mut inner = self.lock();
        inner.records.iter_mut().find_map(|(stream, record)| {
            record.queued.take().map(|(counter, trigger)| {
                // Marked sent BEFORE the write: a write is never repeated, and
                // a concurrent request for the same counter is a duplicate.
                record.sent = Some(counter);
                (*stream, counter, trigger)
            })
        })
    }

    fn mark_failed(&self) -> usize {
        let mut inner = self.lock();
        inner.failed = true;
        // Whatever is still queued can never be delivered: counted now.
        let mut undelivered = 0;
        for record in inner.records.values_mut() {
            if let Some((counter, _)) = record.queued.take() {
                record.sent = Some(counter);
                undelivered += 1;
            }
        }
        undelivered
    }
}

/// The ONE upstream message shape: MH's own fields only — the source's uplink
/// `stream_number` and MH's newest counter. Nothing a receiver sent is ever
/// copied in; trace tags stay unset.
#[must_use]
pub fn keyframe_request_message(stream: StreamNumber, gop_counter: u32) -> MhServerMessage {
    MhServerMessage {
        message: Some(mh_server_message::Message::KeyframeRequest(
            KeyframeRequest {
                stream_number: u32::from(stream.get()),
                gop_counter,
            },
        )),
        ..MhServerMessage::default()
    }
}

/// One publisher connection's writer: drains the newest-only slots onto the
/// send half of the publisher's control stream, one length-framed
/// `MhServerMessage` per request.
///
/// The forward path never waits on this task: a publisher that never reads
/// its control stream blocks only this task, forever, holding at most one
/// slot per declared stream. A failed write is counted
/// (`keyframe_request_write_failures`), never re-emitted, and ends the writer;
/// every later request is counted undeliverable at once.
pub async fn run_keyframe_writer<S: MediaSendStream>(
    keyframes: Arc<PublisherKeyframes>,
    mut sink: S,
    cancel: CancellationToken,
) {
    loop {
        // Register interest BEFORE checking, so a request between the check
        // and the await is not a lost wakeup.
        let woken = keyframes.wake.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();
        while let Some((stream, counter, trigger)) = keyframes.take_next() {
            let message = keyframe_request_message(stream, counter).encode_to_vec();
            let written = match encode_framed(&message) {
                Some(framed) => tokio::select! {
                    () = cancel.cancelled() => return,
                    result = sink.write_all(&framed) => result.is_ok(),
                },
                None => false,
            };
            if written {
                keyframes
                    .handles
                    .keyframe_requests_sent(trigger)
                    .increment(1);
            } else {
                let undelivered = keyframes.mark_failed();
                keyframes
                    .handles
                    .keyframe_request_write_failures()
                    .increment(1 + undelivered as u64);
                return;
            }
        }
        tokio::select! {
            () = cancel.cancelled() => return,
            () = woken => {}
        }
    }
}

/// Every publisher binding on this handler, meeting-scoped.
///
/// Same shape and reasoning as `crate::session::LocalSubscribers`: `ArcSwap`
/// read lock-free, written with `rcu` because per-connection tasks race, and
/// unregistered through the SAME compare-and-remove body
/// (`crate::session::remove_if_held`) so a superseded connection's teardown
/// cannot free the live successor's state.
#[derive(Debug, Default)]
pub struct PublisherKeyframesRegistry {
    bindings: ArcSwap<Bindings>,
}

/// `(meeting, sender)` -> the holding connection and its state.
type Bindings = HashMap<(MeetingKey, SenderId), Bound>;

/// One registered binding.
#[derive(Debug, Clone)]
struct Bound {
    connection_id: String,
    keyframes: Arc<PublisherKeyframes>,
}

impl crate::session::HeldByConnection for Bound {
    fn holder(&self) -> &str {
        &self.connection_id
    }
}

impl PublisherKeyframesRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `keyframes` as `sender`'s binding in `meeting` (setup).
    pub fn register(
        &self,
        meeting: MeetingKey,
        sender: SenderId,
        connection_id: &str,
        keyframes: &Arc<PublisherKeyframes>,
    ) {
        let key = (meeting, sender);
        let entry = Bound {
            connection_id: connection_id.to_string(),
            keyframes: Arc::clone(keyframes),
        };
        self.bindings.rcu(|current| {
            let mut bindings = current.as_ref().clone();
            bindings.insert(key.clone(), entry.clone());
            bindings
        });
    }

    /// Remove `sender`'s binding in `meeting`, only if `connection_id` still
    /// holds it (teardown).
    pub fn unregister(&self, meeting: &MeetingKey, sender: SenderId, connection_id: &str) {
        let key = (meeting.clone(), sender);
        self.bindings.rcu(|current| {
            let mut bindings = current.as_ref().clone();
            crate::session::remove_if_held(&mut bindings, &key, connection_id);
            bindings
        });
    }

    /// The binding currently holding `sender` in `meeting` — resolved per
    /// request, never cached across a reconnect.
    #[must_use]
    pub fn lookup(
        &self,
        meeting: &MeetingKey,
        sender: SenderId,
    ) -> Option<Arc<PublisherKeyframes>> {
        self.bindings
            .load()
            .get(&(meeting.clone(), sender))
            .map(|bound| Arc::clone(&bound.keyframes))
    }

    /// Every binding in `meeting` (apply-time reconcile; never per frame).
    #[must_use]
    pub fn in_meeting(&self, meeting: &MeetingKey) -> Vec<(SenderId, Arc<PublisherKeyframes>)> {
        self.bindings
            .load()
            .iter()
            .filter(|((m, _), _)| m == meeting)
            .map(|((_, sender), bound)| (*sender, Arc::clone(&bound.keyframes)))
            .collect()
    }
}

/// Judge one receiver `RecoveryRequest` (story 3 R-13) and, if it is to be
/// forwarded, queue MH's OWN `KeyframeRequest` for the source.
///
/// A fixed-size message and one lookup; nothing else:
/// 1. `slot_id` resolves ONLY within the requesting connection's own edges
///    (`own` is the connection's bound sender), to a stream-per-group edge
///    with a FORWARD assigned source — else `unmapped_slot`;
/// 2. `sender_id` must be that slot's CURRENT ASSIGNED source (B during a
///    pending A→B switch) — else `source_mismatch` (0 never matches). It is a
///    staleness guard only: never used to choose a source, never forwarded;
/// 3. the counter against MH's newest for that source, under the dedupe lock:
///    no counter yet → `counter_ahead`; lower → `stale`; higher →
///    `counter_ahead`; equal and already requested → `duplicate`; equal →
///    `forwarded`. No clamping.
#[must_use]
pub fn classify_recovery(
    snapshot: &RoutingSnapshot,
    registry: &PublisherKeyframesRegistry,
    meeting: &MeetingKey,
    own: SenderId,
    request: &RecoveryRequest,
) -> RecoveryOutcome {
    let Some(source) = snapshot.slot_source(meeting, own, request.slot_id) else {
        return RecoveryOutcome::UnmappedSlot;
    };
    if SenderId::from_wire(request.sender_id).ok() != Some(source.sender) {
        return RecoveryOutcome::SourceMismatch;
    }
    let Some(keyframes) = registry.lookup(meeting, source.sender) else {
        return RecoveryOutcome::CounterAhead;
    };
    match keyframes.request(
        source.stream,
        KeyframeTrigger::ReceiverRecovery,
        Some(request.gop_counter),
    ) {
        // `Undeliverable` is judged forwardable too: its non-delivery is
        // already counted on `mh_media_keyframe_request_write_failures_total`
        // (the label set is fixed: `forwarded` means accepted for forwarding,
        // not delivered).
        RequestOutcome::Queued | RequestOutcome::Undeliverable => RecoveryOutcome::Forwarded,
        RequestOutcome::AlreadyRequested => RecoveryOutcome::Duplicate,
        RequestOutcome::Stale => RecoveryOutcome::Stale,
        RequestOutcome::Ahead | RequestOutcome::NotSeen => RecoveryOutcome::CounterAhead,
    }
}

/// Judge one post-connect control-stream message from a subscriber
/// connection (story 3 R-13): a fixed-size decode, then
/// [`classify_recovery`]. After connect the stream carries ONLY
/// `recovery_request`: anything else — undecodable, an empty oneof, or a
/// second `connect_request` (which NEVER re-authenticates, rebinds or
/// notifies) — is `malformed`. Envelope trace tags are ignored: no context is
/// extracted and no span is made.
#[must_use]
pub fn judge_control_message(
    body: &[u8],
    snapshot: &RoutingSnapshot,
    registry: &PublisherKeyframesRegistry,
    meeting: &MeetingKey,
    own: SenderId,
) -> RecoveryOutcome {
    match MhClientMessage::decode(body) {
        Ok(MhClientMessage {
            message: Some(mh_client_message::Message::RecoveryRequest(request)),
            ..
        }) => classify_recovery(snapshot, registry, meeting, own, &request),
        Ok(_) | Err(_) => RecoveryOutcome::Malformed,
    }
}

/// What one [`serve_control_message`] call left the subscriber's control
/// stream in.
#[derive(Debug)]
pub enum ControlStep {
    /// One message (or oversize body) judged and counted; keep reading.
    Continue,
    /// A framing break: counted `malformed` once; stop reading the control
    /// stream (the connection is kept).
    FramingBroken,
    /// The client finished the stream.
    Finished,
    /// The stream or connection failed (the cause is kept).
    Failed(StreamReadError),
}

/// Read, judge and count ONE post-connect control-stream message from a
/// subscriber connection (story 3 R-13) — the per-message step of the
/// connection's lifecycle loop, so the deterministic shim reaches the same
/// code. Exactly one `mh_media_recovery_requests_received_total{outcome}`
/// per message or framing break; an end of stream is not a message and is
/// not counted. No allocation: the body is parsed from the caller's fixed
/// buffer.
pub async fn serve_control_message<R: MediaRecvStream>(
    control: &mut FramedReader<R>,
    buf: &mut [u8; CONTROL_PARSE_BYTES],
    routing: &RoutingTable,
    meeting: &MeetingKey,
    own: SenderId,
    handles: &MediaMetricHandles,
) -> ControlStep {
    let (outcome, step) = match control.next_into(buf).await {
        Ok(Framed::Message(len)) => (
            judge_control_message(
                buf.get(..len).unwrap_or_default(),
                &routing.load(),
                routing.keyframes(),
                meeting,
                own,
            ),
            ControlStep::Continue,
        ),
        Ok(Framed::Oversize) => (RecoveryOutcome::Malformed, ControlStep::Continue),
        Err(FrameEnd::FramingBreak) => (RecoveryOutcome::Malformed, ControlStep::FramingBroken),
        Err(FrameEnd::Finished) => return ControlStep::Finished,
        Err(FrameEnd::Failed(error)) => return ControlStep::Failed(error),
    };
    handles.recovery_requests(outcome).increment(1);
    step
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::metrics::resolve_media_handles;

    fn stream(n: u32) -> StreamNumber {
        StreamNumber::from_wire(n).unwrap()
    }

    fn fresh() -> PublisherKeyframes {
        PublisherKeyframes::new(Arc::new(resolve_media_handles()))
    }

    #[test]
    fn an_unseen_stream_never_requests() {
        let k = fresh();
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::NotSeen
        );
        k.record(stream(1), 4);
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::Queued,
            "positive control: once seen, it requests"
        );
    }

    #[test]
    fn one_request_per_counter_shared_by_both_triggers() {
        let k = fresh();
        k.record(stream(1), 7);
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::Queued
        );
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::AlreadyRequested
        );
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::ReceiverRecovery, Some(7)),
            RequestOutcome::AlreadyRequested,
            "a receiver after MH's own request is a duplicate"
        );
        assert_eq!(
            k.take_next(),
            Some((stream(1), 7, KeyframeTrigger::Initial))
        );
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::ReceiverRecovery, Some(7)),
            RequestOutcome::AlreadyRequested,
            "after the write too"
        );
        k.record(stream(1), 8);
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::ReceiverRecovery, Some(8)),
            RequestOutcome::Queued,
            "a newer counter is a new event"
        );
    }

    #[test]
    fn the_equality_rule_has_no_clamping() {
        let k = fresh();
        k.record(stream(1), 10);
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::ReceiverRecovery, Some(9)),
            RequestOutcome::Stale
        );
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::ReceiverRecovery, Some(11)),
            RequestOutcome::Ahead
        );
        assert!(!k.has_queued(stream(1)), "neither arm queues");
    }

    #[test]
    fn newest_is_the_maximum_not_the_last_read() {
        let k = fresh();
        k.record(stream(1), 5);
        k.record(stream(1), 6);
        k.record(stream(1), 5);
        assert_eq!(k.newest(stream(1)), Some(6));
    }

    #[test]
    fn a_newer_counter_replaces_a_queued_older_one_and_a_group_start_discards_it() {
        let k = fresh();
        k.record(stream(1), 3);
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::Queued
        );
        assert!(k.has_queued(stream(1)), "positive control: queued");
        k.record(stream(1), 4);
        assert!(!k.has_queued(stream(1)), "older than newest: discarded");
        k.request(stream(1), KeyframeTrigger::Initial, None);
        assert_eq!(
            k.take_next(),
            Some((stream(1), 4, KeyframeTrigger::Initial))
        );
        // Equal counter (no group start): the queued request stays.
        k.record(stream(1), 5);
        k.request(stream(1), KeyframeTrigger::Initial, None);
        k.record(stream(1), 5);
        assert!(k.has_queued(stream(1)));
    }

    #[test]
    fn reconcile_frees_undeclared_and_discards_unwanted() {
        let k = fresh();
        k.record(stream(0), 1);
        k.record(stream(1), 1);
        k.request(stream(1), KeyframeTrigger::Initial, None);
        k.reconcile(|s| s == stream(1), |_| false);
        assert_eq!(k.record_count(), 1, "stream 0 unassigned: freed");
        assert!(!k.has_queued(stream(1)), "unwanted: discarded");
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::Queued,
            "never sent, so it may be requested again"
        );
    }

    #[test]
    fn after_a_failed_write_requests_are_counted_not_queued() {
        let k = fresh();
        k.record(stream(1), 1);
        k.mark_failed();
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::Undeliverable
        );
        assert!(!k.has_queued(stream(1)));
        assert_eq!(
            k.request(stream(1), KeyframeTrigger::Initial, None),
            RequestOutcome::AlreadyRequested,
            "never re-emitted"
        );
    }

    #[test]
    fn the_upstream_message_carries_only_mh_fields() {
        let message = keyframe_request_message(stream(1), 42);
        assert_eq!(message.trace_parent, "");
        assert_eq!(message.trace_state, "");
        assert_eq!(
            message.message,
            Some(mh_server_message::Message::KeyframeRequest(
                KeyframeRequest {
                    stream_number: 1,
                    gop_counter: 42,
                }
            ))
        );
    }
}
