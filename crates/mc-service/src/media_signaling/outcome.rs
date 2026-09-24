//! Bounded telemetry vocabularies for client-facing media signalling.
//!
//! One home for every label value this module can emit, so the metric
//! vocabulary is a type rather than a scattering of string literals. Each enum
//! exposes `label()` and an `ALL` constant.
//!
//! # What is compiler-enforced here, and what is NOT
//!
//! **Enforced**: `label()` is an exhaustive `match`, so adding a variant without
//! giving it a label is a compile error. That is what bounds cardinality by the
//! type rather than by review — no variant can reach a metric unlabelled.
//!
//! **NOT enforced**: `ALL` is a plain array literal. Adding a variant and
//! updating only `label()` compiles cleanly and leaves `ALL` short, and the
//! vocabulary tests iterate `ALL`, so the missing variant is silently untested
//! rather than caught. Nothing pins `ALL.len()` to the variant count
//! (`std::mem::variant_count` is unstable).
//!
//! **Also NOT enforced**: the two documentation encodings of this vocabulary —
//! the rustdoc in `observability/metrics.rs` and the operator-facing label list
//! in `docs/observability/metrics/mc-service.md`. One vocabulary, four
//! encodings, one of them checked. This was demonstrated live rather than
//! theorised: `AcceptedUnchanged` landed in the enum and `ALL` and in neither
//! document, and left both asserting a partition rule that had become false.
//!
//! A drift test deriving the catalog's list from `ALL` is filed as an extraction
//! opportunity in `docs/TODO.md` §Cross-Service Duplication; it would cover
//! `media_routing::confirm::PolicyPushOutcome::ALL` too, which carries the same
//! claim and the same gap.
//!
//! # Nothing here carries identity
//!
//! ADR-0036 §11: no meeting identifier (raw or hashed), no participant id, and
//! no stream identity — slot id, sender id, stream number, switch command id —
//! may appear as a metric label, a span attribute, or a per-frame log
//! dimension. These are dispositions, not identities, and they must stay that
//! way. If a variant ever needs a value to be useful, it is the wrong variant.

/// Disposition of one client receive-capability declaration.
///
/// Partitions **declarations**: every declaration lands in exactly one variant,
/// which is what makes the success set `{accepted, accepted_unchanged}` complete
/// and `outcome!~"accepted|accepted_unchanged"` a complete count of rejections. A rejection reported
/// on some other metric would silently break that relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityOutcome {
    /// Validated and ACTED ON: MC composed and emitted a directive and
    /// assignments.
    ///
    /// **This alone is "declarations MC did work for"**, which is why the no-op
    /// repeat below is a separate value.
    Accepted,
    /// Validated, but identical to the declaration already in force.
    ///
    /// MC does **no** work and sends **nothing** — it returns before composing.
    /// Counted rather than dropped so the partition holds: every declaration
    /// lands in exactly one bucket of exactly one metric, and a client stuck in
    /// a re-declaration loop stays visible.
    ///
    /// SECURITY: **client-inflatable at near-zero server cost.** It MUST NOT
    /// appear in the denominator of any ratio a client has an incentive to
    /// deflate — a rejection ratio computed over it is an evadable alert. Use
    /// [`Self::Accepted`] alone as that denominator.
    AcceptedUnchanged,
    /// Two slots claimed the same id. Client defect.
    DuplicateSlotId,
    /// More slots than the configured server-side cap. Client defect.
    SlotCountOverCap,
    /// A slot id did not fit the 16-bit relay-region field. Client defect.
    SlotIdOutOfRange,
    /// A pin named sender id 0, which is reserved-invalid. Client defect.
    PinnedSenderIdZero,
    /// A pin did not fit the 16-bit sender field. Client defect.
    PinnedSenderIdOutOfRange,
    /// A slot left `media_kind` unset, or named a value outside the enum.
    ///
    /// Most likely a **version-skewed client that meant audio** — the proto's
    /// zero is what a pre-ADR-0036 peer's `AUDIO = 0` decodes to — not a client
    /// that forgot a field. Read a rising rate as client-fleet skew first.
    MediaKindUnspecified,
    /// The connection exhausted its budget of accepted declarations.
    DeclarationBudgetExhausted,
}

impl CapabilityOutcome {
    /// Every variant, for exhaustive metric-vocabulary tests.
    pub const ALL: [Self; 9] = [
        Self::Accepted,
        Self::AcceptedUnchanged,
        Self::DuplicateSlotId,
        Self::SlotCountOverCap,
        Self::SlotIdOutOfRange,
        Self::PinnedSenderIdZero,
        Self::PinnedSenderIdOutOfRange,
        Self::MediaKindUnspecified,
        Self::DeclarationBudgetExhausted,
    ];

    /// Bounded metric-label form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AcceptedUnchanged => "accepted_unchanged",
            Self::DuplicateSlotId => "duplicate_slot_id",
            Self::SlotCountOverCap => "slot_count_over_cap",
            Self::SlotIdOutOfRange => "slot_id_out_of_range",
            Self::PinnedSenderIdZero => "pinned_sender_id_zero",
            Self::PinnedSenderIdOutOfRange => "pinned_sender_id_out_of_range",
            Self::MediaKindUnspecified => "media_kind_unspecified",
            Self::DeclarationBudgetExhausted => "declaration_budget_exhausted",
        }
    }

    /// Bounded, static message returned to the client on rejection.
    ///
    /// `&'static str` by design: no client-supplied value is ever echoed back,
    /// so this cannot become a reflection vector or a log-injection carrier.
    /// The text is specific enough to debug against and says nothing about
    /// other participants or meeting state.
    #[must_use]
    pub fn client_message(self) -> &'static str {
        match self {
            Self::Accepted | Self::AcceptedUnchanged => "receive capability accepted",
            Self::DuplicateSlotId => "receive capability rejected: duplicate slot id",
            Self::SlotCountOverCap => "receive capability rejected: too many slots",
            Self::SlotIdOutOfRange => "receive capability rejected: slot id out of range",
            Self::PinnedSenderIdZero | Self::PinnedSenderIdOutOfRange => {
                "receive capability rejected: invalid pinned sender id"
            }
            Self::MediaKindUnspecified => "receive capability rejected: slot media kind unset",
            Self::DeclarationBudgetExhausted => {
                "receive capability rejected: too many declarations on this connection"
            }
        }
    }

    /// Was the declaration accepted, in either form?
    ///
    /// The success set is `{accepted, accepted_unchanged}`. Note this is NOT the
    /// ratio denominator — see [`Self::AcceptedUnchanged`].
    #[must_use]
    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted | Self::AcceptedUnchanged)
    }
}

/// Disposition of one attempt to compose and emit a send directive.
///
/// Sequential stages — read meeting state, compute the assignment, resolve
/// handler urls, build streams — and the first failing stage returns its own
/// variant. The variants are therefore **disjoint by construction**: an
/// assignment failure can never also be reported as an unresolved handler url,
/// because url resolution is never reached.
///
/// # Recorded by the meeting actor, on every composition
///
/// Since story 2 the meeting actor composes every participant's view (after a
/// join, leave, capability declaration or mute), so every composition failure
/// records here unconditionally — including those triggered by a peer's action
/// rather than this participant's own. *Every* reason a client was not told to
/// send is a value on `mc_media_send_directives_total`, so "clients are not
/// being directed, and why" is one query. The one pre-actor failure — the
/// connection could not obtain the meeting handle at join — also records
/// [`Self::MeetingStateUnavailable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectiveOutcome {
    /// Emitted with at least one target.
    Emitted,
    /// Emitted with an empty target set.
    ///
    /// A **specified success**, not a failure (ADR-0036 §5: "A target set may be
    /// empty. That means send nothing."). It becomes routine rather than
    /// anomalous once §7 turns off publishers nobody watches, so any alerting
    /// predicate must treat it as success alongside [`Self::Emitted`].
    EmittedEmptyTargets,
    /// A plan named a stream number MC has no policy entry for. **MC defect.**
    UnknownStreamNumber,
    /// A plan carried no transport mode. **MC defect** — never defaulted to
    /// datagram; the enum's zero is a rejection, not a convenience.
    TransportModeUnspecified,
    /// A plan's handler had no client-facing url in the meeting's handler set.
    ///
    /// **MC defect since story 2**: every url MC hands out — `media_servers`,
    /// `StreamAssignment.media_handler_url`, `SendTarget.media_handler_url` —
    /// comes from one frozen `MeetingHandlers` value, so a miss cannot happen
    /// by construction. Fails closed: the participant's view is not emitted at
    /// all, never an ACTIVE slot or a send target with an empty url.
    HandlerUrlUnresolved,
    /// Meeting state could not be read. Environmental.
    MeetingStateUnavailable,
    /// The forwarding assignment could not be computed. Environmental.
    AssignmentFailed,
}

/// Disposition of one client `MuteRequest` (ADR-0036 §5 client mute).
///
/// Partitions **mute reports**: every post-join `MuteRequest` on a connection
/// with a media-signalling context lands in exactly one variant.
///
/// # Why this is a separate vocabulary from [`CapabilityOutcome`]
///
/// Not squeamishness about mixing: `CapabilityOutcome`'s documented partition
/// is over *declarations*, and it is the denominator machinery for the
/// rejection ratio. Folding mute dispositions in would make that partition
/// claim false and silently change what every existing query over it counts.
///
/// # The limiter criterion this vocabulary records
///
/// [`Self::RateLimited`] comes from a **rate limit**, not a budget, and the
/// distinction is the reusable part:
///
/// - a **cumulative budget** bounds total work and PERMANENTLY denies once
///   spent — right for a rare action whose omission costs nothing (declaring a
///   receive capability);
/// - a **rate limit** bounds work per unit time and never permanently denies —
///   required for a repeatable steady-state user action (mute is one toggle per
///   utterance, forever).
///
/// Budgeting mute would freeze `audio_self_muted` on the roster for the rest of
/// the session, so every other participant renders a live speaker as muted.
/// That is a worse failure than the amplification it would prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteOutcome {
    /// The audio flag moved on a declared connection and was reported to the
    /// meeting actor, which re-emits the changed slot view to the subscribers
    /// holding this source. A failed re-emit is counted by the actor itself
    /// (`mc_media_send_directives_total` and
    /// `mc_media_slot_view_emissions_total{outcome="composition_failed"}`).
    Applied,
    /// Reported to the meeting actor, with nothing to re-convey.
    ///
    /// Either this connection has not declared a receive capability yet, or
    /// only `video_muted` changed — and the slot view reads only audio mute, so
    /// no subscriber's view changes.
    ///
    /// A healthy, routine value: it is what a camera button produces.
    AppliedNoRecompose,
    /// Identical to the report already in force. No actor hop, no recomposition.
    Unchanged,
    /// The connection's mute-work rate limit was exhausted.
    ///
    /// The report was NOT applied. See the type doc for why this is a rate
    /// limit rather than a budget, and `webtransport::connection`'s suppression
    /// comment for why dropping the report is safe.
    RateLimited,
    /// The meeting actor could not be reached. Environmental; the connection is
    /// already terminal by the time this can happen.
    ActorUnavailable,
}

impl MuteOutcome {
    /// Every variant, for exhaustive metric-vocabulary tests.
    pub const ALL: [Self; 5] = [
        Self::Applied,
        Self::AppliedNoRecompose,
        Self::Unchanged,
        Self::RateLimited,
        Self::ActorUnavailable,
    ];

    /// Bounded metric-label form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::AppliedNoRecompose => "applied_no_recompose",
            Self::Unchanged => "unchanged",
            Self::RateLimited => "rate_limited",
            Self::ActorUnavailable => "actor_unavailable",
        }
    }

    /// Was the reported state recorded on the meeting actor?
    ///
    /// The success set is `{applied, applied_no_recompose}`. `unchanged` is NOT
    /// in it: nothing was recorded because nothing needed to be.
    #[must_use]
    pub fn is_applied(self) -> bool {
        matches!(self, Self::Applied | Self::AppliedNoRecompose)
    }
}

/// Disposition of one dirty participant in one slot-view flush turn
/// (ADR-0036 §5/§6; story 2's server-driven re-emit).
///
/// The meeting actor re-emits a participant's `SendDirective` and
/// `StreamAssignments` whenever meeting state changes what they would say, and
/// bounds that work per actor turn. Each dirty participant it takes up — or
/// defers — in a turn lands in exactly one variant.
///
/// # This does NOT partition emissions
///
/// A participant deferred on three turns and emitted on the fourth increments
/// four times for one delivered view. `sum()` is flush-turn dispositions, not
/// views delivered.
///
/// # Failure predicate, stated positively on purpose
///
/// Success set `{sent, deferred}` — `deferred` is the routine shape of the
/// bound under load, never a page. The failure predicate is
/// `outcome=~"delivery_failed|composition_failed"`, written positively (the
/// `MuteOutcome` precedent) so that a variant added later defaults to NOT being
/// a failure and has to be classified deliberately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotViewEmission {
    /// The participant's changed view was handed to its connection.
    ///
    /// Spelled `sent`, not `emitted`: the vocabularies here never share a
    /// spelling (`mc_media_send_directives_total` already owns `emitted`), so a
    /// token grepped out of a dashboard names exactly one metric.
    Sent,
    /// The per-turn bound was reached; the participant stays dirty and is
    /// flushed on a later turn with whatever the state is then. Routine.
    Deferred,
    /// The meeting-actor → participant-handle send found the participant
    /// actor gone (mailbox closed). **Environmental.** Disjoint from
    /// `mc_participant_outbound_messages_dropped_total{payload_kind="signaling_raw"}`,
    /// which counts the LATER hop (the participant actor's `try_send` onto a
    /// full stream channel).
    DeliveryFailed,
    /// The view could not be composed. **MC defect** — any non-zero value is a
    /// bug. The stage is recorded on `mc_media_send_directives_total`, which is
    /// the alerting and triage home; this value exists so the disposition set
    /// stays complete. Do not alert on both.
    CompositionFailed,
}

impl SlotViewEmission {
    /// Every variant, for exhaustive metric-vocabulary tests.
    pub const ALL: [Self; 4] = [
        Self::Sent,
        Self::Deferred,
        Self::DeliveryFailed,
        Self::CompositionFailed,
    ];

    /// Bounded metric-label form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Deferred => "deferred",
            Self::DeliveryFailed => "delivery_failed",
            Self::CompositionFailed => "composition_failed",
        }
    }

    /// Is this one of the two failure values?
    #[must_use]
    pub fn is_failure(self) -> bool {
        matches!(self, Self::DeliveryFailed | Self::CompositionFailed)
    }
}

impl DirectiveOutcome {
    /// Every variant, for exhaustive metric-vocabulary tests.
    pub const ALL: [Self; 7] = [
        Self::Emitted,
        Self::EmittedEmptyTargets,
        Self::UnknownStreamNumber,
        Self::TransportModeUnspecified,
        Self::HandlerUrlUnresolved,
        Self::MeetingStateUnavailable,
        Self::AssignmentFailed,
    ];

    /// Bounded metric-label form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Emitted => "emitted",
            Self::EmittedEmptyTargets => "emitted_empty_targets",
            Self::UnknownStreamNumber => "unknown_stream_number",
            Self::TransportModeUnspecified => "transport_mode_unspecified",
            Self::HandlerUrlUnresolved => "handler_url_unresolved",
            Self::MeetingStateUnavailable => "meeting_state_unavailable",
            Self::AssignmentFailed => "assignment_failed",
        }
    }

    /// Did a directive reach the client?
    ///
    /// The success set is `{emitted, emitted_empty_targets}`. Expressed here so
    /// the classification has one home in code as well as in the catalog.
    #[must_use]
    pub fn is_success(self) -> bool {
        matches!(self, Self::Emitted | Self::EmittedEmptyTargets)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// `DirectiveOutcome` had no vocabulary test while both its siblings did —
    /// and it is the vocabulary most exposed to the gap this module's own doc
    /// describes, since `ALL` is a plain array literal that a new variant does
    /// not force anyone to update.
    #[test]
    fn every_directive_outcome_has_a_distinct_bounded_label() {
        let labels: HashSet<&str> = DirectiveOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(labels.len(), DirectiveOutcome::ALL.len());
        for outcome in DirectiveOutcome::ALL {
            assert!(!outcome.label().is_empty());
        }
    }

    /// The success set is `{emitted, emitted_empty_targets}` and nothing else.
    ///
    /// Pinned because ADR-0036 §5 makes an empty target set a SPECIFIED success,
    /// so an alerting predicate of `outcome!="emitted"` would page on a legal
    /// state — and this classification is the code-side home of that rule.
    #[test]
    fn only_the_two_emitted_directive_outcomes_are_successes() {
        for outcome in DirectiveOutcome::ALL {
            let expected = matches!(
                outcome,
                DirectiveOutcome::Emitted | DirectiveOutcome::EmittedEmptyTargets
            );
            assert_eq!(
                outcome.is_success(),
                expected,
                "{} classified wrongly",
                outcome.label()
            );
        }
    }

    #[test]
    fn every_mute_outcome_has_a_distinct_bounded_label() {
        let labels: HashSet<&str> = MuteOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(labels.len(), MuteOutcome::ALL.len());
        for outcome in MuteOutcome::ALL {
            assert!(!outcome.label().is_empty());
        }
    }

    /// `unchanged` is deliberately NOT an applied state: nothing reached the
    /// meeting actor, because nothing needed to.
    #[test]
    fn only_the_two_applied_mute_outcomes_report_to_the_actor() {
        for outcome in MuteOutcome::ALL {
            let expected = matches!(
                outcome,
                MuteOutcome::Applied | MuteOutcome::AppliedNoRecompose
            );
            assert_eq!(
                outcome.is_applied(),
                expected,
                "{} classified wrongly",
                outcome.label()
            );
        }
    }

    #[test]
    fn every_slot_view_emission_has_a_distinct_label_and_only_two_are_failures() {
        let labels: HashSet<&str> = SlotViewEmission::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(labels.len(), SlotViewEmission::ALL.len());
        for outcome in SlotViewEmission::ALL {
            let expected = matches!(
                outcome,
                SlotViewEmission::DeliveryFailed | SlotViewEmission::CompositionFailed
            );
            assert_eq!(
                outcome.is_failure(),
                expected,
                "{} classified wrongly",
                outcome.label()
            );
        }
    }

    /// No two vocabularies may share a label spelling.
    ///
    /// They land on different metrics, so a collision is not a cardinality bug —
    /// it is a TRIAGE bug: an operator who greps a token out of a dashboard and
    /// finds it documented under two metrics with different remedies has to
    /// guess which they are looking at.
    #[test]
    fn the_four_vocabularies_do_not_share_spellings() {
        let mut seen: HashSet<&str> = HashSet::new();
        for label in CapabilityOutcome::ALL
            .iter()
            .map(|o| o.label())
            .chain(DirectiveOutcome::ALL.iter().map(|o| o.label()))
            .chain(MuteOutcome::ALL.iter().map(|o| o.label()))
            .chain(SlotViewEmission::ALL.iter().map(|o| o.label()))
        {
            assert!(
                seen.insert(label),
                "label '{label}' is used by two vocabularies"
            );
        }
    }
}
