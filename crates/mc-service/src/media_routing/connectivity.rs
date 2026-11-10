//! One participant's OBSERVED media connectivity, and when MC acts on it
//! (ADR-0036 §9; story 2 R-33; Lead rulings R1, R2).
//!
//! # Source: what the handlers told MC, never what a client said
//!
//! Connectivity enters only through `NotifyParticipantConnected` /
//! `NotifyParticipantDisconnected` (`grpc/media_coordination.rs`), whose
//! participant identity is the validated meeting token's `sub` (resolved to ONE
//! roster entry by the meeting actor, or to none) and whose `handler_id` is
//! resolved against the meeting's handler set by exact match. The client-reported
//! `ParticipantActor::mh_statuses` map is a diagnostic and is never read here.
//!
//! # Keyed by connection, never by (participant, handler)
//!
//! The key of connected(P) is the CONNECTION: `(handler_id, connection_id)`,
//! with `connection_id` MH's opaque per-session id (`internal.proto`). A handler
//! is in connected(P) iff at least one live key remains for it. So:
//!
//! - a retried Connected with the same key is idempotent (never inflates);
//! - a Disconnected removes only its own key — a stale Disconnected from an old
//!   session cannot remove a handler a newer session is live on (the sticky
//!   denial a bare `(participant, handler)` set suffered);
//! - a Disconnected for a key this participant does not hold is a counted
//!   no-op (`unknown_connection`) — ROUTINE, because every connection MC
//!   declined to bind produces one.
//!
//! MC never parses `connection_id` or infers structure from it: it is an opaque
//! bounded byte string.
//!
//! **`connection_id` is REQUIRED (ADR-0039 §6).** An empty id never reaches
//! this module: the gRPC boundary refuses it first
//! (`grpc/media_coordination.rs` `refuse_missing_connection_id`; sender 0,
//! counted as `connection_id_missing`). There is no per-(participant, handler) fallback
//! key.
//!
//! # Lost Disconnecteds: the remove-only heartbeat reconcile (ADR-0039 §6)
//!
//! Each live key records WHEN MC recorded its bind. A handler's accepted
//! heartbeat retires a key of that handler absent from its `connection_ids`
//! iff the bind was recorded strictly before the PREVIOUS accepted heartbeat's
//! receipt ([`ParticipantConnectivity::retire_absent`], predicate
//! `liveness::retirable`). A heartbeat never ADDS a key: the sender-id bind is
//! the only add path.
//!
//! # The tombstone is REQUIRED (the receive-order defence)
//!
//! `internal.proto` makes a Disconnected terminal for its id — but as a SEND
//! order only: MH never SENDS a Connected for an id after SENDING its
//! Disconnected. Delivery is not ordered: a Connected attempt MH abandoned
//! (retries exhausted, then a notify-on-uncertainty Disconnected) can still be
//! applied here AFTER that Disconnected, which would leave a live key for a
//! closed session — phantom connectivity, fail-OPEN, never retired. So each
//! participant keeps the last [`RETIRED_CONNECTION_TOMBSTONES`] retired keys,
//! keyed on the PAIR (never the id alone, which would let one handler's
//! retirement suppress another handler's live connection), and a Connected
//! naming one is ignored (`retired_connection`). **Do not delete this as
//! redundant with the proto's terminal rule: the proto guarantees send order,
//! not receive order, and names this tombstone as the defence it relies on.**
//! Residual, by its trigger: more than [`RETIRED_CONNECTION_TOMBSTONES`]
//! retirements for one participant between an abandoned Connected and its
//! delayed arrival. The consequence is misrouting inside the meeting's own
//! handler set (security S1b's class), never cross-meeting.
//!
//! # The settle rule — why the window exists (Lead ruling R2)
//!
//! Edges are stable: an edge stays on its handler while both parties remain
//! connected to it (`slots.rs`), so NOTHING ever re-consolidates them. Without a
//! settle window, a participant whose second handler connection lands ~100 ms
//! after its first becomes routable on ONE handler, every edge involving it is
//! placed there, and when the second connection arrives none of those edges
//! moves — so in a meeting where everyone is connected to everything, every
//! sender ends up sending to BOTH handlers, permanently. That breaks "all
//! connected ⇒ one handler per sender" (the story's required property) and
//! doubles uplink; it also permanently perturbs R-2 join-order fill. **This is
//! not a debounce; do not delete it.** The tripwire is the paused-time
//! integration test `staggered_connects_settle_to_one_target_per_sender`
//! (`tests/slot_placement_integration.rs`), which fails without this window;
//! that test's doc points back here.
//!
//! The rule, pessimistic only (security S6):
//!
//! - **NotConnected** → the first live key starts an EPISODE: `Establishing`.
//!   While establishing the participant is NOT routable: no edges, and named
//!   unreachable to no one. Connectivity is never assumed.
//! - The episode's deadline is `start + window`, fixed ONCE from a
//!   server-observed event (the first MH connect notification). Later connects
//!   never extend it, and nothing a client sends can touch it.
//! - It ends EARLY once the participant is connected to every handler in the
//!   meeting's set — so an all-connected participant pays no added latency —
//!   and otherwise at the deadline, with whatever was observed.
//! - Losing every live key returns the participant to `NotConnected` (no
//!   edges, not unreachable); its next connect starts a new episode.
//! - **Episode floor (security S10):** a new episode cannot SETTLE sooner than
//!   `window` after the participant last settled. An early connect is still
//!   recorded; only the settle waits. The floor anchors on the last SETTLE — the
//!   last time routing actually changed — never on episodes that were cancelled
//!   before settling (those changed nothing, and anchoring on them would let a
//!   reconnect storm push the participant's return arbitrarily far out). A
//!   client cycling ALL its media transports therefore drives at most one
//!   settle-driven recompute per window, and only ever delays itself.
//! - **What the floor does NOT bound (security S-1, Gate 3).** It gates EPISODE
//!   STARTS only. A settled participant that keeps at least one live key never
//!   returns to `NotConnected`, so flapping ONE handler is not floored: each
//!   disconnect and each reconnect of that handler changes the routable set and
//!   drives a meeting-wide recompute (`reconcile(Affected::All)` + a view flush,
//!   O(N) each) — two per flap cycle, bounded only by how fast the client can
//!   re-establish a session to that handler (MH caps concurrency, not rate). It
//!   is not silent: the per-change connectivity INFO line fires on every
//!   disconnect and reconnect, and `mc_media_edge_moves_total{reason=
//!   "connectivity_change"}` counts the first cycle's moves off the flapping
//!   handler. Edge stability then keeps the edges where they landed, so the
//!   render converges: the edge-move counter and the MC→MH pushes stop
//!   (generation advance-only-on-change) while the render/compose cost does
//!   not — the INFO line is the only per-cycle signal. Deliberately not floored: flooring every settled-set change
//!   would delay legitimate repair after a real handler loss, a worse trade.
//!
//! # Bounds (security S13)
//!
//! Per participant: at most [`MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER`] live keys
//! per handler, and [`RETIRED_CONNECTION_TOMBSTONES`] tombstones. Per meeting:
//! |roster| ≤ the `SenderId` space (65 535) × |handlers| ≤ 2 (GC assignment) ×
//! 4 live keys, plus |roster| × 16 tombstones × ≤ 256-byte keys. The MC-wide
//! participant bound still holds. This replaced the deleted
//! `MhConnectionRegistry`'s literal 1000-connection-per-meeting cap; the chain
//! above is the replacement, stated rather than asserted.

use super::assignment::HandlerId;
use super::placement::{ConnectedHandlers, HandlerEndpoint, MeetingHandlers};
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;
use tokio::time::Instant;

/// Live connection keys one participant may hold on one handler.
///
/// A legitimate client holds one, two across a transport reconnect. On
/// overflow the NEW key is REFUSED (`connection_bound_refused`, expected-empty)
/// rather than an old one evicted: evicting would discard state MC knows is
/// live in favour of a caller's claim, and the evicted session's eventual
/// Disconnected would be a no-op — the sticky shape this module exists to
/// remove. Residual: a fifth concurrent session from one participant to one
/// handler is untracked; its Disconnected is a counted no-op. Self-scoped.
pub const MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER: usize = 4;

/// Retired `(handler, connection_id)` pairs remembered per participant, to
/// refuse a Connected delivered after its own Disconnected (see the module
/// doc for why this is required and what the residual is).
pub const RETIRED_CONNECTION_TOMBSTONES: usize = 16;

/// MH's opaque per-session connection id. Never empty: the gRPC boundary
/// (`refuse_missing_connection_id`) refuses an empty id before anything is
/// recorded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionKey(String);

impl ConnectionKey {
    /// Wrap the wire value, already length-validated. Never parsed.
    #[must_use]
    pub fn new(wire: impl Into<String>) -> Self {
        Self(wire.into())
    }

    /// The wire value (for exact-match membership in a heartbeat's list).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a handler notification did not become connectivity. A bounded
/// vocabulary for `mc_mh_notifications_unapplied_total{reason}`.
///
/// ANCHOR (DRY): the three tokens shared with `SenderBindingOutcome::label`
/// (`media_admission/binding_response.rs`) — `meeting_unknown`,
/// `participant_unknown`, `user_ambiguous` — are spelled identically on
/// purpose (dashboards and the metrics catalog read them as one vocabulary);
/// pinned by `shared_tokens_match_the_sender_binding_vocabulary`. The enums
/// stay separate (different domains and arm sets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unapplied {
    /// No meeting actor for the notification's meeting id.
    MeetingUnknown,
    /// No roster entry carries the token `sub`. Usually a connect racing the
    /// join; the connect is then FORGOTTEN for connectivity too (MH declines
    /// the session on `sender_id == 0` and the client's reconnect brings a
    /// fresh notification).
    ParticipantUnknown,
    /// Two roster entries share the token `sub`: connectivity is applied to
    /// NEITHER (security S2). Not resolved by `connection_id`: see
    /// `SenderLookup::Ambiguous`.
    UserAmbiguous,
    /// The `handler_id` is not byte-identical to a member of the meeting's
    /// handler set (never in it, or removed). The binding gate (ADR-0039 §6):
    /// a Connected is answered `sender_id` 0 and NOTHING is recorded; a
    /// Disconnected is a no-op that touches no other handler's state.
    HandlerNotInSet,
    /// An empty `connection_id` (REQUIRED, ADR-0039 §6): refused on Connected
    /// (sender 0, nothing recorded) and a no-op on Disconnected.
    ConnectionIdMissing,
    /// A Disconnected for a key this participant does not hold. Routine.
    UnknownConnection,
    /// A Connected beyond [`MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER`].
    /// Expected-empty.
    ConnectionBoundRefused,
    /// A Connected for a key already retired by its Disconnected.
    RetiredConnection,
}

impl Unapplied {
    /// Every value, for zero-initialisation.
    pub const ALL: [Self; 8] = [
        Self::MeetingUnknown,
        Self::ParticipantUnknown,
        Self::UserAmbiguous,
        Self::HandlerNotInSet,
        Self::ConnectionIdMissing,
        Self::UnknownConnection,
        Self::ConnectionBoundRefused,
        Self::RetiredConnection,
    ];

    /// The metric label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::MeetingUnknown => "meeting_unknown",
            Self::ParticipantUnknown => "participant_unknown",
            Self::UserAmbiguous => "user_ambiguous",
            Self::HandlerNotInSet => "handler_not_in_set",
            Self::ConnectionIdMissing => "connection_id_missing",
            Self::UnknownConnection => "unknown_connection",
            Self::ConnectionBoundRefused => "connection_bound_refused",
            Self::RetiredConnection => "retired_connection",
        }
    }
}

/// How an establishing participant settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOutcome {
    /// Connected to every handler of the meeting before the deadline.
    Complete,
    /// The window elapsed with a partial set — the population signal for
    /// "participants that did not reach every handler".
    WindowElapsed,
}

impl SettleOutcome {
    /// Every value, for zero-initialisation.
    pub const ALL: [Self; 2] = [Self::Complete, Self::WindowElapsed];

    /// The metric label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::WindowElapsed => "window_elapsed",
        }
    }
}

/// Where a participant is in its connectivity episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// No live key. Not routable; not unreachable.
    NotConnected,
    /// At least one live key, not yet acted on. Not routable; not unreachable.
    Establishing {
        /// The episode may not settle before this (the S10 floor).
        not_before: Instant,
        /// Settle with whatever is observed by this.
        deadline: Instant,
    },
    /// Routing follows the live set.
    Settled,
}

impl Phase {
    /// The label logged on the connectivity INFO line.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotConnected => "not_connected",
            Self::Establishing { .. } => "establishing",
            Self::Settled => "settled",
        }
    }
}

/// One participant's observed connectivity.
#[derive(Debug, Clone)]
pub struct ParticipantConnectivity {
    /// Live keys per handler, each with the instant MC recorded its bind (the
    /// reconcile's input; the same instant the binding ledger records).
    live: BTreeMap<HandlerId, BTreeMap<ConnectionKey, Instant>>,
    retired: VecDeque<(HandlerId, ConnectionKey)>,
    phase: Phase,
    last_settled_at: Option<Instant>,
}

impl Default for ParticipantConnectivity {
    fn default() -> Self {
        Self {
            live: BTreeMap::new(),
            retired: VecDeque::new(),
            phase: Phase::NotConnected,
            last_settled_at: None,
        }
    }
}

impl ParticipantConnectivity {
    /// Record a Connected. `Ok(())` includes the idempotent duplicate.
    ///
    /// # Errors
    ///
    /// [`Unapplied::RetiredConnection`] or [`Unapplied::ConnectionBoundRefused`];
    /// nothing is mutated on error.
    pub fn connect(
        &mut self,
        endpoint: &HandlerEndpoint,
        key: ConnectionKey,
        now: Instant,
        window: Duration,
    ) -> Result<(), Unapplied> {
        if self
            .retired
            .iter()
            .any(|(h, k)| h == &endpoint.id && k == &key)
        {
            return Err(Unapplied::RetiredConnection);
        }
        let keys = self.live.entry(endpoint.id.clone()).or_default();
        if keys.contains_key(&key) {
            return Ok(());
        }
        if keys.len() >= MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER {
            return Err(Unapplied::ConnectionBoundRefused);
        }
        keys.insert(key, now);
        if self.phase == Phase::NotConnected {
            let not_before = self
                .last_settled_at
                .map_or(now, |last| last + window)
                .max(now);
            // The deadline counts from the first connect of the episode (a
            // server-observed event), never from the floor, and is never
            // extended afterwards.
            self.phase = Phase::Establishing {
                not_before,
                deadline: (now + window).max(not_before),
            };
        }
        Ok(())
    }

    /// Record a Disconnected.
    ///
    /// # Errors
    ///
    /// [`Unapplied::UnknownConnection`] if this participant holds no such key.
    pub fn disconnect(
        &mut self,
        handler: &HandlerId,
        key: &ConnectionKey,
    ) -> Result<(), Unapplied> {
        let Some(keys) = self.live.get_mut(handler) else {
            return Err(Unapplied::UnknownConnection);
        };
        if keys.remove(key).is_none() {
            return Err(Unapplied::UnknownConnection);
        }
        if keys.is_empty() {
            self.live.remove(handler);
        }
        if self.retired.len() >= RETIRED_CONNECTION_TOMBSTONES {
            self.retired.pop_front();
        }
        self.retired.push_back((handler.clone(), key.clone()));
        if self.live.is_empty() {
            self.phase = Phase::NotConnected;
        }
        Ok(())
    }

    /// The remove-only heartbeat reconcile for ONE handler (ADR-0039 §6).
    ///
    /// Walks THIS participant's own live keys on `handler` — never looks
    /// anything up by an incoming id — and retires each key that
    /// `liveness::retirable` says to: absent from `present` and recorded
    /// STRICTLY before `before` (the previous accepted heartbeat's receipt; a
    /// bind recorded at exactly that instant is KEPT). Retirement goes through
    /// [`Self::disconnect`], so the key is tombstoned exactly as a delivered
    /// Disconnected would. Clock skew errs toward keeping: see
    /// `liveness::retirable`. Never adds a key.
    pub fn retire_absent(
        &mut self,
        handler: &HandlerId,
        present: &std::collections::HashSet<&str>,
        before: Instant,
    ) -> Vec<ConnectionKey> {
        let doomed: Vec<ConnectionKey> = self
            .live
            .get(handler)
            .into_iter()
            .flatten()
            .filter(|(key, recorded_at)| {
                super::liveness::retirable(**recorded_at, before, present.contains(key.as_str()))
            })
            .map(|(key, _)| key.clone())
            .collect();
        // Report only keys actually retired: a key that was not (an invariant
        // breach — it was just read from `live`) is neither claimed nor
        // counted as a correction.
        doomed
            .into_iter()
            .filter(|key| self.disconnect(handler, key).is_ok())
            .collect()
    }

    /// The live keys on `handler`, with their bind instants (test and
    /// diagnostics view).
    pub fn keys_on(&self, handler: &HandlerId) -> impl Iterator<Item = (&ConnectionKey, &Instant)> {
        self.live.get(handler).into_iter().flatten()
    }

    /// Settle if due. Returns how, the one time it happens.
    pub fn poll(&mut self, now: Instant, handler_count: usize) -> Option<SettleOutcome> {
        let Phase::Establishing {
            not_before,
            deadline,
        } = self.phase
        else {
            return None;
        };
        if now < not_before {
            return None;
        }
        let outcome = if self.live.len() >= handler_count {
            SettleOutcome::Complete
        } else if now >= deadline {
            SettleOutcome::WindowElapsed
        } else {
            return None;
        };
        self.phase = Phase::Settled;
        self.last_settled_at = Some(now);
        Some(outcome)
    }

    /// When [`Self::poll`] could next change something, if ever.
    #[must_use]
    pub fn next_wake(&self, handler_count: usize) -> Option<Instant> {
        match self.phase {
            Phase::Establishing {
                not_before,
                deadline,
            } => Some(if self.live.len() >= handler_count {
                not_before
            } else {
                deadline
            }),
            _ => None,
        }
    }

    /// The current phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The handlers with at least one live key, in id order (observed, whether
    /// or not yet acted on).
    pub fn observed(&self) -> impl Iterator<Item = &HandlerId> {
        self.live.keys()
    }

    /// The set routing should use: the live handlers once settled, else `None`.
    /// Built only from ACTIVE members of the meeting's set (ADR-0039 §3: a
    /// handler whose registration is not yet acked is never client-visible,
    /// so no edge — and therefore no `SendTarget` or `StreamAssignment` — may
    /// name it).
    #[must_use]
    pub fn routable(&self, handlers: &MeetingHandlers) -> Option<ConnectedHandlers> {
        if self.phase != Phase::Settled || self.live.is_empty() {
            return None;
        }
        let mut set = ConnectedHandlers::new();
        for id in self.live.keys() {
            if let Some(endpoint) = handlers.active_endpoint(id) {
                set.insert(endpoint);
            }
        }
        Some(set).filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The ANCHOR (DRY) on [`Unapplied`]: the shared tokens cannot drift.
    #[test]
    fn shared_tokens_match_the_sender_binding_vocabulary() {
        use crate::media_admission::binding_response::SenderBindingOutcome;
        for (unapplied, binding) in [
            (
                Unapplied::MeetingUnknown,
                SenderBindingOutcome::MeetingUnknown,
            ),
            (
                Unapplied::ParticipantUnknown,
                SenderBindingOutcome::ParticipantUnknown,
            ),
            (
                Unapplied::UserAmbiguous,
                SenderBindingOutcome::UserAmbiguous,
            ),
        ] {
            assert_eq!(unapplied.label(), binding.label());
        }
    }

    const W: Duration = Duration::from_millis(1500);

    fn handlers() -> MeetingHandlers {
        MeetingHandlers::active_test_ids(&["mh-0", "mh-1"])
    }

    fn ep(set: &MeetingHandlers, id: &str) -> HandlerEndpoint {
        set.resolve(id).unwrap().clone()
    }

    fn key(s: &str) -> ConnectionKey {
        ConnectionKey::new(s)
    }

    fn ids(c: &ParticipantConnectivity, set: &MeetingHandlers) -> Option<Vec<String>> {
        c.routable(set)
            .map(|r| r.iter().map(ToString::to_string).collect())
    }

    /// Register before token: a connection observed on a handler whose
    /// registration is not yet acked is never routable; once it is Active the
    /// same observation is.
    #[test]
    fn a_registering_handler_is_never_routable() {
        let mut set =
            MeetingHandlers::from_handoff([MeetingHandlers::test_endpoint("mh-0")]).unwrap();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("a"), t0, W).unwrap();
        assert_eq!(c.poll(t0, 1), Some(SettleOutcome::Complete));
        assert_eq!(ids(&c, &set), None, "Registering: not routable");
        let r = set.handler_ref_of(&HandlerId::new("mh-0")).unwrap();
        set.mark_active(&HandlerId::new("mh-0"), r).unwrap();
        assert_eq!(ids(&c, &set), Some(vec!["mh-0".into()]), "Active: routable");
    }

    #[test]
    fn full_connectivity_settles_at_the_last_connect_with_no_wait() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("a"), t0, W).unwrap();
        assert_eq!(c.poll(t0, 2), None, "partial: still establishing");
        assert_eq!(ids(&c, &set), None, "not routable while establishing");
        c.connect(&ep(&set, "mh-1"), key("b"), t0, W).unwrap();
        assert_eq!(c.poll(t0, 2), Some(SettleOutcome::Complete));
        assert_eq!(ids(&c, &set), Some(vec!["mh-0".into(), "mh-1".into()]));
    }

    #[test]
    fn partial_connectivity_settles_at_the_deadline_with_what_was_observed() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-1"), key("a"), t0, W).unwrap();
        assert_eq!(c.next_wake(2), Some(t0 + W));
        assert_eq!(c.poll(t0 + W - Duration::from_millis(1), 2), None);
        assert_eq!(c.poll(t0 + W, 2), Some(SettleOutcome::WindowElapsed));
        assert_eq!(ids(&c, &set), Some(vec!["mh-1".into()]));
    }

    /// The deadline is set ONCE: later connects in the episode never extend it.
    #[test]
    fn later_connects_do_not_re_arm_the_window() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("a"), t0, W).unwrap();
        c.connect(&ep(&set, "mh-0"), key("a2"), t0 + W / 2, W)
            .unwrap();
        c.connect(&ep(&set, "mh-0"), key("a"), t0 + W / 2, W)
            .unwrap();
        assert_eq!(c.next_wake(2), Some(t0 + W));
    }

    #[test]
    fn a_stale_disconnect_does_not_remove_a_live_newer_session() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("c1"), t0, W).unwrap();
        c.connect(&ep(&set, "mh-0"), key("c2"), t0, W).unwrap();
        c.disconnect(&HandlerId::new("mh-0"), &key("c1")).unwrap();
        assert_eq!(c.poll(t0 + W, 2), Some(SettleOutcome::WindowElapsed));
        assert_eq!(ids(&c, &set), Some(vec!["mh-0".into()]), "c2 keeps mh-0");
    }

    #[test]
    fn a_retried_connect_is_idempotent() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("c1"), t0, W).unwrap();
        c.connect(&ep(&set, "mh-0"), key("c1"), t0, W).unwrap();
        c.disconnect(&HandlerId::new("mh-0"), &key("c1")).unwrap();
        assert_eq!(
            c.phase(),
            Phase::NotConnected,
            "one disconnect clears one connect"
        );
    }

    #[test]
    fn a_disconnect_for_an_unheld_key_is_unknown_connection() {
        let mut c = ParticipantConnectivity::default();
        assert_eq!(
            c.disconnect(&HandlerId::new("mh-0"), &key("nope")),
            Err(Unapplied::UnknownConnection)
        );
    }

    /// P3: a Connected delivered after its own Disconnected is refused —
    /// keyed on the PAIR, so the same id on another handler is unaffected.
    #[test]
    fn a_connect_for_a_retired_key_is_refused_per_handler() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("x"), t0, W).unwrap();
        c.disconnect(&HandlerId::new("mh-0"), &key("x")).unwrap();
        assert_eq!(
            c.connect(&ep(&set, "mh-0"), key("x"), t0, W),
            Err(Unapplied::RetiredConnection)
        );
        assert!(c.connect(&ep(&set, "mh-1"), key("x"), t0, W).is_ok());
    }

    #[test]
    fn tombstones_are_bounded_fifo() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        for i in 0..=RETIRED_CONNECTION_TOMBSTONES {
            let k = key(&format!("k{i}"));
            c.connect(&ep(&set, "mh-0"), k.clone(), t0, W).unwrap();
            c.disconnect(&HandlerId::new("mh-0"), &k).unwrap();
        }
        assert_eq!(c.retired.len(), RETIRED_CONNECTION_TOMBSTONES);
        assert!(
            c.connect(&ep(&set, "mh-0"), key("k0"), t0, W).is_ok(),
            "the evicted oldest is the named residual"
        );
    }

    #[test]
    fn the_live_key_bound_refuses_new_rather_than_evicting() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        for i in 0..MAX_LIVE_MEDIA_CONNECTIONS_PER_HANDLER {
            c.connect(&ep(&set, "mh-0"), key(&format!("k{i}")), t0, W)
                .unwrap();
        }
        assert_eq!(
            c.connect(&ep(&set, "mh-0"), key("extra"), t0, W),
            Err(Unapplied::ConnectionBoundRefused)
        );
        assert!(
            c.disconnect(&HandlerId::new("mh-0"), &key("k0")).is_ok(),
            "k0 still tracked"
        );
    }

    /// Replaces the deleted legacy-mode test: the reconcile retires only the
    /// absent, strictly-earlier keys of ITS handler, tombstones them like a
    /// Disconnected, and never adds.
    #[test]
    fn retire_absent_is_remove_only_per_handler_and_tombstones() {
        let set = handlers();
        let t0 = Instant::now();
        let prev = t0 + Duration::from_millis(100);
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("early"), t0, W).unwrap();
        c.connect(&ep(&set, "mh-0"), key("listed"), t0, W).unwrap();
        c.connect(&ep(&set, "mh-0"), key("at-prev"), prev, W)
            .unwrap();
        c.connect(&ep(&set, "mh-1"), key("other"), t0, W).unwrap();
        let present: std::collections::HashSet<&str> = ["listed", "unknown"].into();
        let retired = c.retire_absent(&HandlerId::new("mh-0"), &present, prev);
        assert_eq!(retired, vec![key("early")]);
        let left: Vec<_> = c
            .keys_on(&HandlerId::new("mh-0"))
            .map(|(k, _)| k.as_str().to_string())
            .collect();
        assert_eq!(
            left,
            vec!["at-prev", "listed"],
            "equal instant kept; nothing added"
        );
        assert_eq!(
            c.keys_on(&HandlerId::new("mh-1")).count(),
            1,
            "other handler untouched"
        );
        assert_eq!(
            c.connect(&ep(&set, "mh-0"), key("early"), prev, W),
            Err(Unapplied::RetiredConnection),
            "a reconciled key is tombstoned like a Disconnected"
        );
    }

    #[test]
    fn losing_every_key_returns_to_not_connected_and_stays_there() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("a"), t0, W).unwrap();
        c.connect(&ep(&set, "mh-1"), key("b"), t0, W).unwrap();
        c.poll(t0, 2);
        c.disconnect(&HandlerId::new("mh-0"), &key("a")).unwrap();
        assert_eq!(c.phase(), Phase::Settled, "one handler left: still settled");
        c.disconnect(&HandlerId::new("mh-1"), &key("b")).unwrap();
        assert_eq!(c.phase(), Phase::NotConnected);
        assert_eq!(c.poll(t0 + W * 10, 2), None, "no spontaneous re-gain");
        assert_eq!(ids(&c, &set), None);
    }

    /// S10: a new episode cannot settle sooner than one window after the last
    /// settle — and a storm of cancelled episodes does not push that out.
    #[test]
    fn a_reconnect_cycle_is_floored_to_one_episode_per_window() {
        let set = handlers();
        let t0 = Instant::now();
        let mut c = ParticipantConnectivity::default();
        c.connect(&ep(&set, "mh-0"), key("a"), t0, W).unwrap();
        c.connect(&ep(&set, "mh-1"), key("b"), t0, W).unwrap();
        assert_eq!(c.poll(t0, 2), Some(SettleOutcome::Complete));
        c.disconnect(&HandlerId::new("mh-0"), &key("a")).unwrap();
        c.disconnect(&HandlerId::new("mh-1"), &key("b")).unwrap();

        let t1 = t0 + Duration::from_millis(10);
        c.connect(&ep(&set, "mh-0"), key("a2"), t1, W).unwrap();
        c.connect(&ep(&set, "mh-1"), key("b2"), t1, W).unwrap();
        assert_eq!(c.poll(t1, 2), None, "full set, but inside the floor");
        assert_eq!(c.next_wake(2), Some(t0 + W));
        assert_eq!(c.poll(t0 + W, 2), Some(SettleOutcome::Complete));

        // A storm of cycles cancelled before settling does not move the floor.
        let t2 = t0 + W + Duration::from_millis(1);
        for i in 0..10 {
            c.disconnect(&HandlerId::new("mh-0"), &key(&format!("a{}", i + 2)))
                .ok();
            c.disconnect(&HandlerId::new("mh-1"), &key(&format!("b{}", i + 2)))
                .ok();
            c.connect(&ep(&set, "mh-0"), key(&format!("a{}", i + 3)), t2, W)
                .unwrap();
            c.connect(&ep(&set, "mh-1"), key(&format!("b{}", i + 3)), t2, W)
                .unwrap();
        }
        assert_eq!(
            c.next_wake(2),
            Some(t0 + W + W),
            "one window after the LAST SETTLE"
        );
    }

    #[test]
    fn vocabularies_are_complete_and_distinct() {
        let labels: BTreeSet<&str> = Unapplied::ALL.iter().map(|u| u.label()).collect();
        assert_eq!(labels.len(), Unapplied::ALL.len());
        let settle: BTreeSet<&str> = SettleOutcome::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(settle.len(), SettleOutcome::ALL.len());
    }
}
