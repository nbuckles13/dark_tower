//! Total classification of `RegisterMeetingResponse` into one bounded outcome
//! (ADR-0036 §8).
//!
//! Pure: no metrics, no logging, no I/O. The caller emits and decides; this
//! module only says *what the response was*.

use proto_gen::dark_tower::internal::v1::RegisterMeetingResponse;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::num::NonZeroU64;

/// What MC declared on the push it is now confirming.
#[derive(Debug, Clone, Copy)]
pub struct PushExpectation<'a> {
    /// The handler MC dialed, as MC knows it (`MhAssignment.mh_id`).
    pub handler_id: &'a str,
    /// The generation MC sent.
    pub policy_generation: NonZeroU64,
    /// The transport mode MC declared on every egress stream.
    pub transport_mode: TransportMode,
}

/// What a handler's reply to a policy push was.
///
/// The five values are `internal.proto`'s canonical `outcome` vocabulary, which
/// claims sole definition of the set ("defined here and deliberately not
/// restated elsewhere"). These arms restate it, so:
///
/// ANCHOR (DRY): `internal.v1.RegisterMeetingResponse.applied_generation` in
/// `proto/dark_tower/internal/v1/internal.proto` defines this vocabulary. This
/// is an unguarded anchor comment, joining the existing class in
/// `signaling.proto` — nothing fails if the proto's list moves and this does
/// not.
///
/// Not duplication of `mh-service`'s `PolicyApplyOutcome`: that is the
/// **apply-side** answer to "what did MH do with the policy" and its five
/// values are different ones. This is the **confirm-side** answer to "what did
/// the response prove". Shared idiom, different value sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyPushOutcome {
    /// MH's live forward path reflects the generation MC sent, the transport
    /// mode agrees, and the handler identified itself as the one MC dialed.
    Match,
    /// `applied_generation` was 0: MH's forward path reflects no generation at
    /// all for this meeting. Distinct from [`Self::GenerationMismatch`] because
    /// "never programmed" and "programmed at the wrong generation" have
    /// different causes; folding 0 into the mismatch arm would hide the case
    /// where MH installed nothing whatsoever.
    NoAppliedGeneration,
    /// MH applied a generation, but not the one MC sent. The transient apply
    /// failure — an identical re-send is an idempotent MH no-op, so it is worth
    /// retrying.
    GenerationMismatch,
    /// MH echoed a transport mode other than the one MC declared. §8's
    /// two-ends-must-agree check.
    TransportModeMismatch,
    /// The handler identified itself as someone other than the one MC dialed.
    ///
    /// **Non-fatal and lowest-precedence for the interim** — see [`evaluate`]
    /// for why, and for the single condition that reverts both.
    HandlerIdMismatch,
}

impl PolicyPushOutcome {
    /// Every value, in catalog order.
    ///
    /// The one hand-maintained list. Every consumer — the metric recorder's
    /// tests, the cardinality assertion, the integration label checks —
    /// iterates this rather than re-enumerating, because an enumeration
    /// written out elsewhere compiles clean while staying silently short. The
    /// length is written out rather than inferred: it is the catalogued
    /// cardinality of the `outcome` label, so a sixth variant fails to compile
    /// here first, before the catalog and dashboard go stale.
    pub const ALL: [Self; 5] = [
        Self::Match,
        Self::NoAppliedGeneration,
        Self::GenerationMismatch,
        Self::TransportModeMismatch,
        Self::HandlerIdMismatch,
    ];

    /// Bounded `outcome` metric-label value.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::NoAppliedGeneration => "no_applied_generation",
            Self::GenerationMismatch => "generation_mismatch",
            Self::TransportModeMismatch => "transport_mode_mismatch",
            Self::HandlerIdMismatch => "handler_id_mismatch",
        }
    }

    /// How the caller must treat this outcome.
    #[must_use]
    pub fn disposition(self) -> PushDisposition {
        match self {
            // The meeting is programmed.
            Self::Match => PushDisposition::Programmed,

            // INTERIM: NON-FATAL. See `evaluate`'s precedence comment — this and
            // the demotion to lowest precedence are ONE decision with ONE cause
            // and they revert together, tracked in `2026-09-02-mh-stable-handler-id`.
            //
            // This is NOT a swallowed error, and the distinction is the whole
            // of it: the failure is fully observable — an `error!` line, a
            // `mc_media_policy_pushes_total{outcome="handler_id_mismatch"}`
            // increment, and its own named bounded outcome. ONLY the abort is
            // suppressed. Masking would be not reporting it, which this does
            // not do.
            //
            // Reaching this arm at all already proves the meeting IS programmed:
            // under the demoted order, `handler_id_mismatch` is returned only
            // when `applied_generation == sent` and the transport mode agreed.
            // So a correct handler that merely restarted is never turned into a
            // kicked meeting.
            Self::HandlerIdMismatch => PushDisposition::Programmed,

            // A genuine two-ends version skew. Backoff cannot make a handler
            // running a different transport mode agree, so failing three times
            // slowly is strictly worse than failing once loudly.
            Self::TransportModeMismatch => PushDisposition::Terminal,

            // Transient apply failures: mailbox full, apply timed out, actor
            // gone. An identical re-send is idempotent MH-side, so retrying is
            // both safe and the likely fix.
            Self::NoAppliedGeneration | Self::GenerationMismatch => PushDisposition::Retryable,
        }
    }
}

/// What the caller does with a [`PolicyPushOutcome`].
///
/// Separated from the outcome itself because the observable vocabulary
/// (`internal.proto`'s five values) and the control decision are different
/// things that changed independently once already: `handler_id_mismatch` moved
/// from terminal to non-fatal without the enum changing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushDisposition {
    /// The meeting is programmed. The call returns `Ok`.
    Programmed,
    /// Failed; another attempt may succeed. Consumes a retry.
    Retryable,
    /// Failed; no attempt will succeed. Fail immediately.
    Terminal,
}

/// Classify a handler's reply against what MC declared.
///
/// # Precedence (INTERIM)
///
/// `no_applied_generation > generation_mismatch > transport_mode_mismatch >
/// handler_id_mismatch > match`
///
/// **(a) This ordering is interim and reverts only when `handler_id` is stable
/// per deployment.** Restore handler-first, and make it fatal, at that point —
/// not before. The two changes are one decision: `handler_id_mismatch` being
/// non-fatal (see [`PolicyPushOutcome::disposition`]) and its being ranked last
/// have one cause and revert together. A later reader restoring one must
/// restore the other.
///
/// **(b) Why.** `handler_id` is a **per-incarnation token**, not a handler
/// identity: `mh-service`'s config derives it from `HOSTNAME` plus a fresh
/// `Uuid::new_v4()` sampled at process start whenever `MH_HANDLER_ID` is unset,
/// and it is unset in all three MH manifests. MC compares against the id frozen
/// in its Redis assignment snapshot, which has no TTL, so **every ordinary MH
/// pod restart makes the mismatch expected by construction**. At the top of the
/// chain that benign, expected outcome would suppress a real
/// `generation_mismatch` underneath it for the whole post-restart window — a
/// false positive masking the exact control this task exists to build, and
/// masking it silently, which is worse than the loud outage it replaced. Ranked
/// last, the outcome instead means "policy applied correctly, identity differs"
/// — precisely the benign restart case — and it can never hide a generation
/// fault. A genuine misroute still surfaces: a wrong handler holds no state for
/// this meeting, so it reports `no_applied_generation` or
/// `generation_mismatch`, which outrank this and are retried.
///
/// **(c) Tracked in `2026-09-02-mh-stable-handler-id`.** Its definition of done
/// spans two halves — a stable `MH_HANDLER_ID` (MH config + three manifests)
/// AND GC's assignment-refresh path, because `assign_meeting_with_mh` returns
/// early on an existing assignment, so a stable id alone still leaves MC's
/// snapshot stale against a genuinely reassigned handler, producing a check
/// that looks trustworthy and is not.
///
/// # Two fail-open shapes, both inoculated
///
/// `transport_mode` and `handler_id` are compared **unconditionally**. There is
/// no `if echoed != UNSPECIFIED { compare }` and no `if !echoed.is_empty() {
/// compare }` anywhere in this function. Both would fail open against exactly
/// the peer population the checks exist to police — a pre-reshape MH decodes
/// both fields to their proto3 defaults — and both survive review as
/// defensiveness while their skip-branch stays unreachable in any test written
/// against a same-version peer, i.e. invisible to the suite permanently.
/// `internal.proto` warns about this for the enum; it is the same hazard for
/// the string.
///
/// `accepted == true` is **not** success and is deliberately not read here:
/// MH's handler returns before the apply completes, so `accepted` means
/// "received and parsed". Only `applied_generation == sent` is success.
#[must_use]
pub fn evaluate(
    expected: &PushExpectation<'_>,
    response: &RegisterMeetingResponse,
) -> PolicyPushOutcome {
    if response.applied_generation == 0 {
        return PolicyPushOutcome::NoAppliedGeneration;
    }
    if response.applied_generation != expected.policy_generation.get() {
        return PolicyPushOutcome::GenerationMismatch;
    }

    // An unrecognised enum value decodes to `Unspecified`, which then compares
    // unequal to the declared mode — a mismatch, which is correct. It must not
    // become a skip.
    let echoed_mode =
        TransportMode::try_from(response.transport_mode).unwrap_or(TransportMode::Unspecified);
    if expected.transport_mode != echoed_mode {
        return PolicyPushOutcome::TransportModeMismatch;
    }

    // Unconditional, empty string included.
    if expected.handler_id != response.handler_id {
        return PolicyPushOutcome::HandlerIdMismatch;
    }

    PolicyPushOutcome::Match
}

/// The magnitude by which the live policy differs from the one MC pushed.
///
/// **An unsigned magnitude, not a difference**, and the word matters: it is what
/// stops the sign being reintroduced.
///
/// `sent - applied` on `u64` underflows on a *reachable, documented* input. MH
/// ignores a generation lower than the one it holds and echoes the installed
/// one, so an MC restart (registry lost, re-derives from 1) against a handler
/// holding 5 yields `applied > sent`; the `policy_generation: u64::MAX` ratchet
/// wedge is the same input adversarially. In a debug build that panics
/// (ADR-0002); in release it wraps to ~1.8e19, a garbage spike that blows out
/// any shared panel axis.
///
/// Two other fixes were considered and are rejected **here**, so neither is
/// reintroduced later as a "fix":
///
/// * `saturating_sub` maps `applied > sent` to **0** — a healthy-looking zero on
///   the one response shape that proves MC and MH disagree about which policy is
///   live. Same class of silent regression as feeding the gauge from the sent
///   value, by a different route.
/// * A signed value is meaningful, but any stat threshold written `> 0` renders
///   the negative arm green, and `> 0` is what anyone will reach for.
///
/// So: unsigned magnitude. It cannot underflow, it cannot panic, and every
/// non-zero value means "the live policy is not the one MC pushed", regardless
/// of direction. Direction is not lost — the `error!` line carries both numbers,
/// so a responder reads *which way* off the log and *how far* off the gauge.
#[must_use]
pub fn divergence_magnitude(sent: NonZeroU64, applied: u64) -> u64 {
    sent.get().abs_diff(applied)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const EXPECTED_HANDLER: &str = "mh-0";

    fn expectation(generation: u64) -> PushExpectation<'static> {
        PushExpectation {
            handler_id: EXPECTED_HANDLER,
            policy_generation: NonZeroU64::new(generation).unwrap(),
            transport_mode: TransportMode::Datagram,
        }
    }

    fn response(
        applied_generation: u64,
        handler_id: &str,
        transport_mode: TransportMode,
    ) -> RegisterMeetingResponse {
        RegisterMeetingResponse {
            accepted: true,
            applied_generation,
            handler_id: handler_id.to_string(),
            process_start_epoch_ms: 0,
            transport_mode: transport_mode as i32,
        }
    }

    // ---- the five single-condition cases ---------------------------------
    // These pin no ordering: each passes under any arm order.

    #[test]
    fn echoed_generation_equal_to_sent_with_agreeing_fields_is_match() {
        let outcome = evaluate(
            &expectation(4),
            &response(4, EXPECTED_HANDLER, TransportMode::Datagram),
        );
        assert_eq!(outcome, PolicyPushOutcome::Match);
        assert_eq!(outcome.disposition(), PushDisposition::Programmed);
    }

    /// `applied == 0` is its OWN outcome — asserted to be neither `match` nor
    /// `generation_mismatch`, because folding it into either is what hides
    /// "MH installed nothing at all".
    #[test]
    fn applied_zero_is_no_applied_generation_and_neither_neighbour() {
        let outcome = evaluate(
            &expectation(4),
            &response(0, EXPECTED_HANDLER, TransportMode::Datagram),
        );
        assert_eq!(outcome, PolicyPushOutcome::NoAppliedGeneration);
        assert_ne!(outcome, PolicyPushOutcome::Match);
        assert_ne!(outcome, PolicyPushOutcome::GenerationMismatch);
        assert_eq!(outcome.disposition(), PushDisposition::Retryable);
    }

    #[test]
    fn applied_below_sent_is_generation_mismatch() {
        let outcome = evaluate(
            &expectation(5),
            &response(3, EXPECTED_HANDLER, TransportMode::Datagram),
        );
        assert_eq!(outcome, PolicyPushOutcome::GenerationMismatch);
        assert_eq!(outcome.disposition(), PushDisposition::Retryable);
    }

    /// The arm that underflows a naive `sent - applied`, and the one a suite
    /// written only against the too-low case never reaches. MH holds a HIGHER
    /// generation than MC sent — an MC restart, or the ratchet wedge.
    #[test]
    fn applied_above_sent_is_generation_mismatch_and_yields_a_positive_magnitude() {
        let expected = expectation(1);
        let outcome = evaluate(
            &expected,
            &response(5, EXPECTED_HANDLER, TransportMode::Datagram),
        );
        assert_eq!(outcome, PolicyPushOutcome::GenerationMismatch);
        assert_eq!(
            divergence_magnitude(expected.policy_generation, 5),
            4,
            "applied > sent must report magnitude 4, never 0 and never a wrap"
        );
    }

    /// The UNSPECIFIED echo specifically — the pre-reshape peer the
    /// `if echoed != UNSPECIFIED { compare }` fail-open would wave through.
    #[test]
    fn unspecified_transport_echo_is_a_mismatch_not_a_skip() {
        let outcome = evaluate(
            &expectation(4),
            &response(4, EXPECTED_HANDLER, TransportMode::Unspecified),
        );
        assert_eq!(outcome, PolicyPushOutcome::TransportModeMismatch);
        assert_eq!(outcome.disposition(), PushDisposition::Terminal);
    }

    /// A wrong-but-specified mode, so the check is not merely an
    /// is-it-unspecified test wearing a mismatch's name.
    #[test]
    fn wrong_but_specified_transport_echo_is_a_mismatch() {
        let outcome = evaluate(
            &expectation(4),
            &response(4, EXPECTED_HANDLER, TransportMode::StreamPerGroup),
        );
        assert_eq!(outcome, PolicyPushOutcome::TransportModeMismatch);
    }

    #[test]
    fn wrong_handler_id_is_a_mismatch() {
        let outcome = evaluate(
            &expectation(4),
            &response(4, "mh-9", TransportMode::Datagram),
        );
        assert_eq!(outcome, PolicyPushOutcome::HandlerIdMismatch);
    }

    /// S-2: proto3 decodes an absent string to `""`, so an empty echo is the
    /// `handler_id` twin of the UNSPECIFIED transport echo. The comparison is
    /// unconditional, so it is caught rather than waved through.
    #[test]
    fn empty_handler_id_echo_is_a_mismatch_not_a_skip() {
        let outcome = evaluate(&expectation(4), &response(4, "", TransportMode::Datagram));
        assert_eq!(outcome, PolicyPushOutcome::HandlerIdMismatch);
    }

    // ---- precedence collision cases --------------------------------------
    // Each pins ONE adjacent boundary of the interim chain:
    // no_applied_generation > generation_mismatch > transport_mode_mismatch
    // > handler_id_mismatch > match

    /// (i) Pins `no_applied_generation` above everything, INCLUDING
    /// `handler_id`. Under the pre-OPS-18 handler-first order this same input
    /// returned `handler_id_mismatch`; this is the test whose expectation the
    /// reorder inverted.
    #[test]
    fn no_applied_generation_pins_precedence_above_handler_id_and_transport() {
        let outcome = evaluate(
            &expectation(4),
            &response(0, "mh-9", TransportMode::Unspecified),
        );
        assert_eq!(outcome, PolicyPushOutcome::NoAppliedGeneration);
    }

    /// (ii) Pins `generation_mismatch` above transport and handler id.
    #[test]
    fn generation_mismatch_pins_precedence_above_transport_and_handler_id() {
        let outcome = evaluate(
            &expectation(4),
            &response(3, "mh-9", TransportMode::Unspecified),
        );
        assert_eq!(outcome, PolicyPushOutcome::GenerationMismatch);
        assert_ne!(outcome, PolicyPushOutcome::TransportModeMismatch);
        assert_ne!(outcome, PolicyPushOutcome::HandlerIdMismatch);
    }

    /// (iii) Pins `transport_mode_mismatch` above `handler_id_mismatch`, with
    /// the generation matched so neither higher arm can claim it.
    #[test]
    fn transport_mode_mismatch_pins_precedence_above_handler_id() {
        let outcome = evaluate(
            &expectation(4),
            &response(4, "mh-9", TransportMode::StreamPerGroup),
        );
        assert_eq!(outcome, PolicyPushOutcome::TransportModeMismatch);
        assert_ne!(outcome, PolicyPushOutcome::HandlerIdMismatch);
    }

    /// (iv) Pins `handler_id_mismatch` at the BOTTOM and NON-FATAL — the single
    /// test that most directly encodes the OPS-18 decision. Formerly this same
    /// input pinned handler_id at the TOP and asserted a fatal `Err`.
    #[test]
    fn handler_id_mismatch_pins_precedence_at_bottom_and_is_non_fatal() {
        let outcome = evaluate(
            &expectation(4),
            &response(4, "mh-9", TransportMode::Datagram),
        );
        assert_eq!(outcome, PolicyPushOutcome::HandlerIdMismatch);
        assert_eq!(
            outcome.disposition(),
            PushDisposition::Programmed,
            "non-fatal for the interim: reaching this arm proves applied == sent and the \
             transport mode agreed, so the meeting IS programmed and a restarted handler \
             must not be turned into a kicked meeting"
        );
    }

    // ---- vocabulary and magnitude ----------------------------------------

    #[test]
    fn all_covers_every_variant_with_a_distinct_label() {
        let labels: std::collections::HashSet<&str> =
            PolicyPushOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(labels.len(), PolicyPushOutcome::ALL.len());
        assert!(labels.contains("match"));
        assert!(labels.contains("no_applied_generation"));
        assert!(labels.contains("generation_mismatch"));
        assert!(labels.contains("transport_mode_mismatch"));
        assert!(labels.contains("handler_id_mismatch"));
    }

    #[test]
    fn divergence_magnitude_is_symmetric_and_zero_only_on_agreement() {
        let sent = NonZeroU64::new(5).unwrap();
        assert_eq!(divergence_magnitude(sent, 5), 0);
        assert_eq!(divergence_magnitude(sent, 3), 2);
        assert_eq!(divergence_magnitude(sent, 7), 2);
        // The adversarial input: the ratchet wedge. Never 0, never a wrap.
        assert_eq!(
            divergence_magnitude(NonZeroU64::MIN, u64::MAX),
            u64::MAX - 1
        );
    }
}
