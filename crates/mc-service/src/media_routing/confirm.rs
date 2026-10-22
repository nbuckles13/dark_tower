//! Total classification of `RegisterMeetingResponse` into one bounded outcome
//! (ADR-0036 §8).
//!
//! Pure: no metrics, no logging, no I/O. The caller emits and decides; this
//! module only says *what the response was*.

use super::assignment::SourceStreamPlan;
use proto_gen::dark_tower::internal::v1::RegisterMeetingResponse;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::BTreeSet;
use std::num::NonZeroU64;

/// What MC declared on the push it is now confirming.
#[derive(Debug, Clone, Copy)]
pub struct PushExpectation<'a> {
    /// The handler MC dialed, as MC knows it (`MhAssignment.mh_id`).
    pub handler_id: &'a str,
    /// The generation MC sent.
    pub policy_generation: NonZeroU64,
    /// The per-stream `source_streams` MC declared (story 3 R-20), each with
    /// the transport MH must echo back in `applied_source_streams`.
    pub source_streams: &'a BTreeSet<SourceStreamPlan>,
}

/// Which way a sent stream's per-stream echo disagreed (story 3 R-20). A LOG
/// FIELD, never a metric label: all three map to `transport_mode_mismatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchoArm {
    /// A sent (sender, stream) is absent from `applied_source_streams`.
    AbsentFromEcho,
    /// It was echoed `TRANSPORT_MODE_UNSPECIFIED` — a mismatch, never a skip.
    EchoedUnspecified,
    /// It was echoed with a different, specified mode.
    ModeDiffers,
}

impl EchoArm {
    /// Bounded log-field form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AbsentFromEcho => "absent_from_echo",
            Self::EchoedUnspecified => "echoed_unspecified",
            Self::ModeDiffers => "mode_differs",
        }
    }
}

/// The first sent stream (in canonical (sender, stream) order) whose echo
/// disagrees, with the arm and its `stream_number` (log fields only), or
/// `None` if every sent stream was echoed with its declared mode.
///
/// Compared per stream, keyed by (sender_id, stream_number), order
/// irrelevant, UNCONDITIONALLY (`internal.proto`
/// `RegisterMeetingResponse.applied_source_streams`). Per stream, not per-mode
/// counts: counts read green on an audio/video swap. An echo entry MC did not
/// send is not a mismatch arm (MH's rejects already bar it).
#[must_use]
pub fn first_echo_mismatch(
    expected: &PushExpectation<'_>,
    response: &RegisterMeetingResponse,
) -> Option<(EchoArm, u8)> {
    // Index the echo ONCE: O(sent + echoed), never O(sent x echoed) — a
    // registration declares up to 2 x roster streams.
    let echo: std::collections::HashMap<(u32, u32), i32> = response
        .applied_source_streams
        .iter()
        .map(|e| ((e.sender_id, e.stream_number), e.transport_mode))
        .collect();
    for sent in expected.source_streams {
        let key = (
            u32::from(sent.sender.get().get()),
            u32::from(sent.stream_number),
        );
        let arm = match echo.get(&key) {
            None => Some(EchoArm::AbsentFromEcho),
            Some(raw) => {
                // An unrecognised enum value decodes to `Unspecified` and is a
                // mismatch, never a skip.
                let mode = TransportMode::try_from(*raw).unwrap_or(TransportMode::Unspecified);
                if mode == TransportMode::Unspecified {
                    Some(EchoArm::EchoedUnspecified)
                } else if mode != sent.transport_mode.mode() {
                    Some(EchoArm::ModeDiffers)
                } else {
                    None
                }
            }
        };
        if let Some(arm) = arm {
            return Some((arm, sent.stream_number));
        }
    }
    None
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
    /// MH's per-stream echo disagreed with a declared stream's transport:
    /// absent from the echo, echoed UNSPECIFIED, or echoed with a different
    /// mode ([`EchoArm`], logged; story 3 R-20). §8's two-ends-must-agree
    /// check.
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
/// Every sent stream's echoed transport and `handler_id` are compared
/// **unconditionally**. There is no `if echoed != UNSPECIFIED { compare }` and
/// no `if !echoed.is_empty() { compare }` anywhere in this function. Both would fail open against exactly
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
    evaluate_with_echo(expected, response).0
}

/// [`evaluate`], also returning the per-stream echo mismatch it found (the
/// arm and stream number, for the confirm-failure LOG only), so a caller
/// never recomputes it. The echo is `None` when the comparison was not
/// reached (a higher-precedence outcome) or every stream agreed.
#[must_use]
pub fn evaluate_with_echo(
    expected: &PushExpectation<'_>,
    response: &RegisterMeetingResponse,
) -> (PolicyPushOutcome, Option<(EchoArm, u8)>) {
    if response.applied_generation == 0 {
        return (PolicyPushOutcome::NoAppliedGeneration, None);
    }
    if response.applied_generation != expected.policy_generation.get() {
        return (PolicyPushOutcome::GenerationMismatch, None);
    }

    // Per stream, unconditionally (story 3): the legacy single
    // `transport_mode` echo is no longer compared — MH echoes it UNSPECIFIED
    // once `source_streams` is declared, and MC-B deletes the field.
    let echo = first_echo_mismatch(expected, response);
    if echo.is_some() {
        return (PolicyPushOutcome::TransportModeMismatch, echo);
    }

    // Unconditional, empty string included.
    if expected.handler_id != response.handler_id {
        return (PolicyPushOutcome::HandlerIdMismatch, None);
    }

    (PolicyPushOutcome::Match, None)
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
    use crate::media_admission::SenderId;
    use crate::media_routing::assignment::{SourceDispositionPlan, TransportModeOrd};
    use proto_gen::dark_tower::internal::v1::SourceStreamEcho;
    use std::num::NonZeroU16;
    use std::sync::LazyLock;

    const EXPECTED_HANDLER: &str = "mh-0";

    /// Sender 1 publishes audio (0, DATAGRAM) and video (1, STREAM_PER_GROUP).
    static SENT: LazyLock<BTreeSet<SourceStreamPlan>> = LazyLock::new(|| {
        let sender = SenderId::from_nonzero(NonZeroU16::new(1).unwrap());
        [
            (0u8, TransportMode::Datagram),
            (1, TransportMode::StreamPerGroup),
        ]
        .into_iter()
        .map(|(stream_number, mode)| SourceStreamPlan {
            sender,
            stream_number,
            transport_mode: TransportModeOrd::new(mode),
            disposition: SourceDispositionPlan::Forward,
        })
        .collect()
    });

    fn expectation(generation: u64) -> PushExpectation<'static> {
        PushExpectation {
            handler_id: EXPECTED_HANDLER,
            policy_generation: NonZeroU64::new(generation).unwrap(),
            source_streams: &SENT,
        }
    }

    fn echo(audio: Option<TransportMode>, video: Option<TransportMode>) -> Vec<SourceStreamEcho> {
        [(0u32, audio), (1, video)]
            .into_iter()
            .filter_map(|(stream_number, mode)| {
                mode.map(|m| SourceStreamEcho {
                    sender_id: 1,
                    stream_number,
                    transport_mode: m as i32,
                })
            })
            .collect()
    }

    fn agreeing() -> Vec<SourceStreamEcho> {
        echo(
            Some(TransportMode::Datagram),
            Some(TransportMode::StreamPerGroup),
        )
    }

    fn response(
        applied_generation: u64,
        handler_id: &str,
        applied_source_streams: Vec<SourceStreamEcho>,
    ) -> RegisterMeetingResponse {
        RegisterMeetingResponse {
            accepted: true,
            applied_generation,
            handler_id: handler_id.to_string(),
            process_start_epoch_ms: 0,
            // The legacy single echo: UNSPECIFIED once `source_streams` is
            // declared, and never compared.
            transport_mode: TransportMode::Unspecified as i32,
            applied_source_streams,
        }
    }

    #[test]
    fn echoed_generation_equal_to_sent_with_agreeing_streams_is_match() {
        let outcome = evaluate(&expectation(4), &response(4, EXPECTED_HANDLER, agreeing()));
        assert_eq!(outcome, PolicyPushOutcome::Match);
        assert_eq!(outcome.disposition(), PushDisposition::Programmed);
    }

    /// The legacy single field is not compared: an UNSPECIFIED legacy echo
    /// with an agreeing per-stream echo is a match.
    #[test]
    fn the_legacy_transport_echo_is_no_longer_compared() {
        let mut r = response(4, EXPECTED_HANDLER, agreeing());
        r.transport_mode = TransportMode::StreamPerGroup as i32;
        assert_eq!(evaluate(&expectation(4), &r), PolicyPushOutcome::Match);
    }

    #[test]
    fn applied_zero_is_no_applied_generation_and_neither_neighbour() {
        let outcome = evaluate(&expectation(4), &response(0, EXPECTED_HANDLER, agreeing()));
        assert_eq!(outcome, PolicyPushOutcome::NoAppliedGeneration);
        assert_eq!(outcome.disposition(), PushDisposition::Retryable);
    }

    #[test]
    fn applied_below_sent_is_generation_mismatch() {
        let outcome = evaluate(&expectation(5), &response(3, EXPECTED_HANDLER, agreeing()));
        assert_eq!(outcome, PolicyPushOutcome::GenerationMismatch);
        assert_eq!(outcome.disposition(), PushDisposition::Retryable);
    }

    #[test]
    fn applied_above_sent_is_generation_mismatch_and_yields_a_positive_magnitude() {
        let expected = expectation(1);
        let outcome = evaluate(&expected, &response(5, EXPECTED_HANDLER, agreeing()));
        assert_eq!(outcome, PolicyPushOutcome::GenerationMismatch);
        assert_eq!(divergence_magnitude(expected.policy_generation, 5), 4);
    }

    // ---- the three echo arms, each on its own -----------------------------

    #[test]
    fn a_sent_stream_absent_from_the_echo_is_a_mismatch() {
        let r = response(
            4,
            EXPECTED_HANDLER,
            echo(Some(TransportMode::Datagram), None),
        );
        assert_eq!(
            evaluate(&expectation(4), &r),
            PolicyPushOutcome::TransportModeMismatch
        );
        assert_eq!(
            first_echo_mismatch(&expectation(4), &r),
            Some((EchoArm::AbsentFromEcho, 1))
        );
        // An EMPTY echo — what a pre-story MH sends — is a mismatch too.
        let r = response(4, EXPECTED_HANDLER, Vec::new());
        assert_eq!(
            first_echo_mismatch(&expectation(4), &r),
            Some((EchoArm::AbsentFromEcho, 0))
        );
    }

    #[test]
    fn a_stream_echoed_unspecified_is_a_mismatch_not_a_skip() {
        let r = response(
            4,
            EXPECTED_HANDLER,
            echo(
                Some(TransportMode::Unspecified),
                Some(TransportMode::StreamPerGroup),
            ),
        );
        assert_eq!(
            evaluate(&expectation(4), &r),
            PolicyPushOutcome::TransportModeMismatch
        );
        assert_eq!(
            first_echo_mismatch(&expectation(4), &r),
            Some((EchoArm::EchoedUnspecified, 0))
        );
        assert_eq!(
            PolicyPushOutcome::TransportModeMismatch.disposition(),
            PushDisposition::Terminal
        );
    }

    /// A swap (audio echoed stream-per-group, video datagram): per stream, so
    /// it reds where per-mode counts would read green.
    #[test]
    fn a_stream_echoed_with_a_different_mode_is_a_mismatch_including_a_swap() {
        let r = response(
            4,
            EXPECTED_HANDLER,
            echo(
                Some(TransportMode::StreamPerGroup),
                Some(TransportMode::Datagram),
            ),
        );
        assert_eq!(
            evaluate(&expectation(4), &r),
            PolicyPushOutcome::TransportModeMismatch
        );
        assert_eq!(
            first_echo_mismatch(&expectation(4), &r),
            Some((EchoArm::ModeDiffers, 0))
        );
    }

    #[test]
    fn echo_arm_labels_are_distinct() {
        let labels: std::collections::HashSet<&str> = [
            EchoArm::AbsentFromEcho,
            EchoArm::EchoedUnspecified,
            EchoArm::ModeDiffers,
        ]
        .iter()
        .map(|a| a.label())
        .collect();
        assert_eq!(labels.len(), 3);
    }

    #[test]
    fn wrong_handler_id_is_a_mismatch() {
        let outcome = evaluate(&expectation(4), &response(4, "mh-9", agreeing()));
        assert_eq!(outcome, PolicyPushOutcome::HandlerIdMismatch);
    }

    #[test]
    fn empty_handler_id_echo_is_a_mismatch_not_a_skip() {
        let outcome = evaluate(&expectation(4), &response(4, "", agreeing()));
        assert_eq!(outcome, PolicyPushOutcome::HandlerIdMismatch);
    }

    // ---- precedence collision cases --------------------------------------
    // no_applied_generation > generation_mismatch > transport_mode_mismatch
    // > handler_id_mismatch > match — unchanged by the per-stream echo.

    #[test]
    fn no_applied_generation_pins_precedence_above_handler_id_and_transport() {
        let outcome = evaluate(&expectation(4), &response(0, "mh-9", Vec::new()));
        assert_eq!(outcome, PolicyPushOutcome::NoAppliedGeneration);
    }

    #[test]
    fn generation_mismatch_pins_precedence_above_transport_and_handler_id() {
        let outcome = evaluate(&expectation(4), &response(3, "mh-9", Vec::new()));
        assert_eq!(outcome, PolicyPushOutcome::GenerationMismatch);
    }

    /// Every echo arm sits above `handler_id_mismatch`, with the generation
    /// matched so neither higher arm can claim it.
    #[test]
    fn every_echo_arm_pins_precedence_above_handler_id() {
        for streams in [
            Vec::new(),
            echo(
                Some(TransportMode::Unspecified),
                Some(TransportMode::StreamPerGroup),
            ),
            echo(
                Some(TransportMode::StreamPerGroup),
                Some(TransportMode::StreamPerGroup),
            ),
        ] {
            let outcome = evaluate(&expectation(4), &response(4, "mh-9", streams));
            assert_eq!(outcome, PolicyPushOutcome::TransportModeMismatch);
        }
    }

    #[test]
    fn handler_id_mismatch_pins_precedence_at_bottom_and_is_non_fatal() {
        let outcome = evaluate(&expectation(4), &response(4, "mh-9", agreeing()));
        assert_eq!(outcome, PolicyPushOutcome::HandlerIdMismatch);
        assert_eq!(outcome.disposition(), PushDisposition::Programmed);
    }

    #[test]
    fn all_covers_every_variant_with_a_distinct_label() {
        let labels: std::collections::HashSet<&str> =
            PolicyPushOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(labels.len(), PolicyPushOutcome::ALL.len());
        assert!(labels.contains("transport_mode_mismatch"));
    }

    #[test]
    fn divergence_magnitude_is_symmetric_and_zero_only_on_agreement() {
        let sent = NonZeroU64::new(5).unwrap();
        assert_eq!(divergence_magnitude(sent, 5), 0);
        assert_eq!(divergence_magnitude(sent, 3), 2);
        assert_eq!(divergence_magnitude(sent, 7), 2);
        assert_eq!(
            divergence_magnitude(NonZeroU64::MIN, u64::MAX),
            u64::MAX - 1
        );
    }
}
