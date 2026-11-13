//! Register before token, at the first join (ADR-0039 §3, L4-2).
//!
//! A handler becomes client-visible only once its `RegisterMeeting` is acked.
//! A meeting's initial handlers (GC's hand-off) are programmed by the push
//! workers the FIRST join's reconcile spawns, so the joins that arrive before
//! those handlers' verdicts would otherwise be answered with an empty set, and
//! the client would dial nothing until the next set push.
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

/// The SDK's join deadline (`DEFAULT_JOIN_TIMEOUT_MS`,
/// `packages/sdk-core/src/signaling/SignalingClient.ts`): how long a client
/// waits for its JoinResponse.
///
/// A mirror of the client's value, pinned EQUAL to it by the drift test in
/// this module (the SDK source is read in exactly that one place). It bounds
/// the park above and feeds the meeting join window's floor and derived
/// default (`actors::meeting_timers`), so a client change that moved it
/// without this constant would silently undercount both.
pub const SDK_JOIN_DEADLINE: Duration = Duration::from_secs(15);

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

    /// Read an integer literal token (`15_000`, `500`) loudly.
    fn parse_int_token(name: &str, token: &str) -> u64 {
        let digits = token.trim().replace('_', "");
        let value: u64 = digits.parse().unwrap_or_else(|e| {
            panic!("EXTRACTION FAILED: `{name}` literal {token:?} is not an integer: {e}")
        });
        // Self-check: the whole token was the number (no expression, no unit
        // suffix silently dropped).
        assert_eq!(
            value.to_string(),
            digits,
            "EXTRACTION FAILED: `{name}` {token:?} is not a plain integer literal"
        );
        value
    }

    /// THE extraction of a TS integer constant: the `[export ]const NAME =
    /// <literal>;` line of `source`. Every failure is an `EXTRACTION FAILED`
    /// panic — never confusable with the value checks that follow.
    fn extract_ts_int_const(source: &str, file: &str, name: &str) -> u64 {
        let declaration = format!("const {name} =");
        let line = source
            .lines()
            .map(str::trim_start)
            .find(|l| {
                l.strip_prefix("export ")
                    .unwrap_or(l)
                    .starts_with(&declaration)
            })
            .unwrap_or_else(|| {
                panic!(
                    "EXTRACTION FAILED: no `{declaration} ...` line in {file} (renamed or \
                     moved?) — update this drift test, never skip it"
                )
            });
        // The literal token: everything between `=` and `;`, e.g. `15_000`.
        let token = line
            .split_once('=')
            .and_then(|(_, rhs)| rhs.split_once(';'))
            .map(|(literal, _)| literal.trim())
            .unwrap_or_else(|| panic!("EXTRACTION FAILED: no `= <literal>;` in {line:?}"));
        parse_int_token(name, token)
    }

    /// The SDK join deadline, read from the SDK source itself (the single
    /// source of truth for the client's value) and pinned EQUAL to
    /// [`SDK_JOIN_DEADLINE`]; then the park's margin below it.
    ///
    /// Distinct failures, so a vacuous pass is impossible: the EXTRACTION
    /// fails loudly if the constant is missing, renamed or unparseable; the
    /// MIRROR check fails if the SDK moved its deadline; the MARGIN check
    /// fails if the park deadline crept too close.
    #[test]
    fn the_park_deadline_stays_below_the_sdk_join_deadline() {
        /// Headroom between a released JoinResponse and the client giving up:
        /// the reply still has to cross the network and be processed, and the
        /// SDK's clock started before MC's (TLS + session setup + auth).
        const MARGIN: Duration = Duration::from_secs(5);
        const SDK_SOURCE: &str =
            include_str!("../../../../packages/sdk-core/src/signaling/SignalingClient.ts");
        const NAME: &str = "DEFAULT_JOIN_TIMEOUT_MS";

        let sdk_ms = extract_ts_int_const(SDK_SOURCE, "SignalingClient.ts", NAME);
        assert_eq!(
            SDK_JOIN_DEADLINE,
            Duration::from_millis(sdk_ms),
            "MIRROR DRIFT: SDK_JOIN_DEADLINE no longer equals the SDK's {NAME} ({sdk_ms} ms); \
             update it — the join window's floor and derived default sum it"
        );
        assert!(
            FIRST_JOIN_PARK_DEADLINE + MARGIN < SDK_JOIN_DEADLINE,
            "MARGIN VIOLATED: park deadline {FIRST_JOIN_PARK_DEADLINE:?} + margin {MARGIN:?} \
             must be below the SDK join deadline {SDK_JOIN_DEADLINE:?}"
        );
    }

    /// The SDK's mirror of `END_MEETING_BACKOFF` (`MC_END_MEETING_BACKOFF_MS`,
    /// `packages/sdk-core/src/config/clientConfig.ts`) is pinned EQUAL to the
    /// MC constant. The join-retry span formula lives ONLY in TS
    /// (`session/joinRetry.ts`), whose test asserts the derived span exceeds
    /// that mirror — so the formula is never restated here, and this pin keeps
    /// the mirror honest.
    #[test]
    fn the_sdk_end_meeting_backoff_mirror_equals_mcs() {
        const SDK_CONFIG: &str =
            include_str!("../../../../packages/sdk-core/src/config/clientConfig.ts");
        const NAME: &str = "MC_END_MEETING_BACKOFF_MS";
        let sdk_ms = extract_ts_int_const(SDK_CONFIG, "clientConfig.ts", NAME);
        assert_eq!(
            crate::media_routing::teardown::END_MEETING_BACKOFF,
            Duration::from_millis(sdk_ms),
            "MIRROR DRIFT: the SDK's {NAME} ({sdk_ms} ms) no longer equals MC's \
             END_MEETING_BACKOFF; the join-retry floor is sized against it"
        );
    }

    /// Positive control: the extractor reads an `export const` fixture.
    #[test]
    fn the_ts_extractor_reads_an_export_const() {
        let fixture = "// x\nexport const MC_END_MEETING_BACKOFF_MS = 1_000;\n";
        assert_eq!(
            extract_ts_int_const(fixture, "fixture", "MC_END_MEETING_BACKOFF_MS"),
            1000
        );
    }

    /// And it fails as EXTRACTION — never as a value verdict — when the
    /// constant is missing or is not a plain literal.
    #[test]
    #[should_panic(expected = "EXTRACTION FAILED")]
    fn the_ts_extractor_fails_loudly_on_a_missing_constant() {
        let _ = extract_ts_int_const(
            "export const OTHER = 1;",
            "fixture",
            "MC_END_MEETING_BACKOFF_MS",
        );
    }

    #[test]
    #[should_panic(expected = "EXTRACTION FAILED")]
    fn the_ts_extractor_fails_loudly_on_an_expression() {
        let _ = extract_ts_int_const(
            "export const MC_END_MEETING_BACKOFF_MS = 2 * 500;",
            "fixture",
            "MC_END_MEETING_BACKOFF_MS",
        );
    }
}
