//! Register before token, at the first join (ADR-0039 §3, L4-2).
//!
//! A handler becomes client-visible only once its `RegisterMeeting` is acked.
//! A meeting's initial handlers (GC's hand-off) are programmed by the push
//! workers the FIRST join's reconcile spawns, so the joins that arrive before
//! those handlers' verdicts would otherwise be answered with an empty set — and
//! a pre-C1 SDK, which reads only the legacy `media_servers`, would then have
//! no media for its session.
//!
//! So the meeting actor PARKS the reply of every join admitted while the
//! initial verdict is pending, and keeps serving its mailbox: admission
//! (sender id, roster, participant actor, broadcasts, slots) completes in the
//! join's own turn exactly as for any join; only the `JoinResponse` waits.
//! Joins after the verdict reply at once.
//!
//! # Release
//!
//! Every parked reply is released, together, by the FIRST of:
//! - every initial handler has a verdict — acked, or its push worker's first
//!   snapshot settled without an ack (retries exhausted, a terminal refusal,
//!   or the worker ended: `pusher::RegistrationReporter`'s `Drop`); or
//! - [`FIRST_JOIN_PARK_DEADLINE`] after the first join parked.
//!
//! A released reply carries the client snapshot AT RELEASE (Active only,
//! possibly empty). A handler that acks later is an ordinary membership
//! change: the set `ServerMessage` reaches every participant.
//!
//! The actor answers every parked reply `Draining` when the meeting ends, so
//! no reply sender outlives the actor.
//!
//! # Not an MH await
//!
//! Nothing here blocks: the park is state, the deadline is a `select!` arm
//! armed only while a reply is parked, and verdicts arrive as mailbox-like
//! reports. The actor never awaits MH inside a turn.

use super::messages::JoinResult;
use crate::errors::McError;
use crate::media_routing::HandlerId;
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::Instant;

/// How long a parked first join waits for its meeting's initial handlers.
///
/// A Rust constant, not config, because it is COUPLED to the client: it must
/// stay clearly below the SDK's join deadline (`DEFAULT_JOIN_TIMEOUT_MS`,
/// `packages/sdk-core/src/signaling/SignalingClient.ts`), or a dead initial
/// handler would turn into a client join timeout and retry instead of a join
/// that succeeds with an empty set. The drift test in this module reads the
/// SDK constant and fails if the margin is violated. The pusher's own worst
/// case (three attempts against a hanging MH) is several times longer, which
/// is why the park needs its own bound.
pub const FIRST_JOIN_PARK_DEADLINE: Duration = Duration::from_secs(8);

/// Why parked replies were released (a log field, never a label).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseReason {
    /// Every initial handler has a verdict.
    Verdicts,
    /// The park deadline fired first.
    Deadline,
}

impl ReleaseReason {
    /// Log-field form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Verdicts => "verdicts",
            Self::Deadline => "deadline",
        }
    }
}

/// One parked reply: the channel the connection awaits, and the join's
/// admission-time result. At release the actor refreshes the meeting-wide
/// fields (KEK, its generation, the handler snapshot) from its current state.
#[derive(Debug)]
pub(super) struct ParkedJoin {
    pub respond_to: oneshot::Sender<Result<JoinResult, McError>>,
    pub result: JoinResult,
}

/// The first-join park.
#[derive(Debug)]
pub(super) struct JoinPark {
    /// Initial handlers still without a verdict; `None` once the initial
    /// verdict is settled (for the meeting's life).
    awaiting: Option<BTreeSet<HandlerId>>,
    parked: Vec<ParkedJoin>,
    deadline: Option<Instant>,
}

impl JoinPark {
    /// A park awaiting `initial` (GC's hand-off). An empty hand-off has
    /// nothing to wait for.
    pub(super) fn new(initial: impl IntoIterator<Item = HandlerId>) -> Self {
        let awaiting: BTreeSet<HandlerId> = initial.into_iter().collect();
        Self {
            awaiting: (!awaiting.is_empty()).then_some(awaiting),
            parked: Vec::new(),
            deadline: None,
        }
    }

    /// Must a join admitted now park its reply?
    pub(super) fn is_pending(&self) -> bool {
        self.awaiting.is_some()
    }

    /// Park one reply. The first park arms the deadline.
    pub(super) fn park(&mut self, parked: ParkedJoin, now: Instant) {
        self.deadline
            .get_or_insert_with(|| now + FIRST_JOIN_PARK_DEADLINE);
        self.parked.push(parked);
    }

    /// The deadline, only while a reply is parked (the `select!` arm is armed
    /// only then, so it never fires on a stale instant).
    pub(super) fn deadline(&self) -> Option<Instant> {
        if self.parked.is_empty() {
            None
        } else {
            self.deadline
        }
    }

    /// Record one initial handler's verdict. Returns `true` if this settles
    /// the initial verdict (the caller then releases every parked reply).
    /// A verdict for a non-initial handler, a repeat, or one after settlement
    /// changes nothing.
    pub(super) fn record_verdict(&mut self, handler: &HandlerId) -> bool {
        let Some(awaiting) = &mut self.awaiting else {
            return false;
        };
        awaiting.remove(handler);
        if awaiting.is_empty() {
            self.awaiting = None;
            true
        } else {
            false
        }
    }

    /// Settle the initial verdict by deadline (the remaining handlers are left
    /// Registering, for the replacement path).
    pub(super) fn expire(&mut self) {
        self.awaiting = None;
    }

    /// Take every parked reply (on release or on exit).
    pub(super) fn take_parked(&mut self) -> Vec<ParkedJoin> {
        self.deadline = None;
        std::mem::take(&mut self.parked)
    }

    /// The parked participants (they get the set in their JoinResponse, so a
    /// membership-change broadcast skips them).
    pub(super) fn parked_participants(&self) -> impl Iterator<Item = &str> {
        self.parked.iter().map(|p| p.result.participant_id.as_str())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn hid(id: &str) -> HandlerId {
        HandlerId::new(id)
    }

    #[test]
    fn the_initial_verdict_settles_only_when_every_initial_handler_has_one() {
        let mut park = JoinPark::new([hid("mh-0"), hid("mh-1")]);
        assert!(park.is_pending());
        assert!(!park.record_verdict(&hid("mh-9")), "not an initial handler");
        assert!(!park.record_verdict(&hid("mh-0")));
        assert!(
            !park.record_verdict(&hid("mh-0")),
            "a repeat changes nothing"
        );
        assert!(park.is_pending());
        assert!(
            park.record_verdict(&hid("mh-1")),
            "the last verdict settles"
        );
        assert!(!park.is_pending());
        assert!(!park.record_verdict(&hid("mh-1")), "settled for life");
    }

    #[test]
    fn an_empty_handoff_has_nothing_to_wait_for() {
        assert!(!JoinPark::new(Vec::new()).is_pending());
    }

    #[test]
    fn expiry_settles_the_verdict() {
        let mut park = JoinPark::new([hid("mh-0")]);
        park.expire();
        assert!(!park.is_pending());
    }

    /// The SDK join deadline this park must stay below, read from the SDK
    /// source itself (the single source of truth for the client's value).
    ///
    /// Two distinct failures, so a vacuous pass is impossible: the EXTRACTION
    /// fails loudly if the constant is missing, renamed or unparseable; the
    /// MARGIN check fails if the park deadline crept too close.
    #[test]
    fn the_park_deadline_stays_below_the_sdk_join_deadline() {
        /// Headroom between a released JoinResponse and the client giving up:
        /// the reply still has to cross the network and be processed, and the
        /// SDK's clock started before MC's (TLS + session setup + auth).
        const MARGIN: Duration = Duration::from_secs(5);
        const SDK_SOURCE: &str =
            include_str!("../../../../packages/sdk-core/src/signaling/SignalingClient.ts");
        const NAME: &str = "DEFAULT_JOIN_TIMEOUT_MS";

        let declaration = format!("const {NAME} =");
        let line = SDK_SOURCE
            .lines()
            .find(|l| l.trim_start().starts_with(&declaration))
            .unwrap_or_else(|| {
                panic!(
                    "EXTRACTION FAILED: no `{declaration} ...` line in SignalingClient.ts \
                     (renamed or moved?) — update this drift test, never skip it"
                )
            });
        // The literal token: everything between `=` and `;`, e.g. `15_000`.
        let token = line
            .split_once('=')
            .and_then(|(_, rhs)| rhs.split_once(';'))
            .map(|(literal, _)| literal.trim())
            .unwrap_or_else(|| panic!("EXTRACTION FAILED: no `= <literal>;` in {line:?}"));
        let digits = token.replace('_', "");
        let sdk_ms: u64 = digits.parse().unwrap_or_else(|e| {
            panic!("EXTRACTION FAILED: `{NAME}` literal {token:?} is not an integer: {e}")
        });
        // Self-check: the whole token was the number (no expression, no unit
        // suffix silently dropped).
        assert_eq!(
            sdk_ms.to_string(),
            digits,
            "EXTRACTION FAILED: {token:?} is not a plain integer literal"
        );

        let sdk = Duration::from_millis(sdk_ms);
        assert!(
            FIRST_JOIN_PARK_DEADLINE + MARGIN < sdk,
            "MARGIN VIOLATED: park deadline {FIRST_JOIN_PARK_DEADLINE:?} + margin {MARGIN:?} \
             must be below the SDK join deadline {sdk:?}"
        );
    }
}
