//! Outcome of resolving a participant's `sender_id` for a Media Handler.
//!
//! MH asks MC "who is this participant?" over `NotifyParticipantConnected`, and
//! MC answers with the ordinal it allocated at join — or with `0`, meaning **"I
//! do not know this participant"**. This type is the bounded vocabulary for
//! which of those happened, and why.
//!
//! # Why `0` is an answer and not a failure
//!
//! `0` is reserved-invalid for a `sender_id` (`SenderId` wraps `NonZeroU16`),
//! so it can carry a second meaning on this response without colliding with any
//! real ordinal. MH rejects on it and closes the connection. That is the whole
//! fail-closed design: **MC never invents, defaults to, or reuses an id.** The
//! type system carries most of that guarantee — `SenderId` has no production
//! constructor outside the allocator — but the honest `0` is what makes the
//! *unresolvable* case expressible at all.

/// Why MC's `NotifyParticipantConnected` reply carried the `sender_id` it did.
///
/// Bounded `outcome` metric-label vocabulary for
/// `mc_media_sender_binding_responses_total`.
///
/// # Why the four unresolved arms are separate
///
/// Every variant except [`Self::Resolved`] answers `0` on the wire, so there
/// are **four** unresolved arms, not two. They triage differently, and
/// collapsing any of them would mix a should-investigate condition with
/// expected background — the failure mode
/// `docs/observability/metrics/mh-service.md`'s drop-reason rule names as "a
/// counter that mixes expected shedding with an invariant violation can never
/// be alerted on".
///
/// [`Self::MeetingUnknown`] means MH notified MC about a meeting MC does not
/// hold: a routing/registration fault whose remedy lives in the MC<->MH
/// registration path. [`Self::ParticipantUnknown`] is an expected low-rate race
/// between MH's connect notification and MC's join completing. [`Self::RegistryFull`]
/// is the per-meeting connection cap declining to bind a connection MC is not
/// tracking — a capacity remedy, not a lifecycle one. [`Self::UserAmbiguous`]
/// is one user with two roster entries for one `sub`; it **never self-clears,
/// and reconnecting CAUSES it**, so the `participant_unknown` remedy is actively
/// harmful here.
///
/// # The information MH structurally cannot have
///
/// MH observes only `sender_id == 0` and **cannot** reconstruct which of the
/// four unresolved arms produced it. Its own
/// `mh_media_session_starts_total{outcome="declined_no_sender_binding"}` is the
/// *union* of the four. That is why this counter is not redundant with MH's, and
/// why the catalog entry says so in words: the MC series carries strictly more
/// information than any function of the MH series.
///
/// # Observability
///
/// The `sender_id` **value** is never a label, span attribute or log field
/// (ADR-0036 §11). Only this outcome is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SenderBindingOutcome {
    /// MC held the meeting, found the participant on its roster, and answered
    /// with that participant's allocated ordinal in `1..=65535`.
    Resolved,
    /// MC does not hold this meeting at all. Answered `0`.
    ///
    /// A routing or registration fault, not a race: MH is notifying MC about a
    /// meeting MC has no actor for.
    MeetingUnknown,
    /// MC holds the meeting but the participant is not on its roster. Answered
    /// `0`.
    ///
    /// Expected at a low rate — MH's connect notification can arrive before
    /// MC's join completes. Sustained non-zero is a lifecycle bug.
    ParticipantUnknown,
    /// MC refused to register the connection (per-meeting registry cap), so it
    /// declines to hand out a binding for it. Answered `0`.
    ///
    /// **Not folded into [`Self::ParticipantUnknown`]**, because the participant
    /// may be perfectly well known: the refusal is about capacity, and its
    /// remedy is capacity, not a lifecycle investigation. Answering a real
    /// ordinal here would let MH forward media for a connection MC is not
    /// tracking and will never send `NotifyParticipantDisconnected` for — the
    /// two sides disagreeing about whether the connection exists, invisibly.
    RegistryFull,
    /// **More than one** participant on the meeting's roster carries the token
    /// `sub` MH asked about, so the question does not identify a single sender.
    /// Answered `0`.
    ///
    /// **Not folded into [`Self::ParticipantUnknown`]**: that one is a benign
    /// race that clears itself, this one does not clear and is not a race. MC
    /// mints a fresh `participant_id` per join and does not bar the same user
    /// joining twice, so a user on two devices lands here and **stays** here.
    /// Its remedy is a per-connection identifier on the wire — a contract
    /// change — not waiting.
    ///
    /// Answering with either candidate would bind MH's connection to an ordinal
    /// that may belong to the user's *other* participant, and MH would stamp
    /// that participant's `sender_id` onto this connection's frames: a
    /// cross-participant misattribution inside one user's own identity. Right
    /// half the time, undetectable when wrong.
    UserAmbiguous,
}

impl SenderBindingOutcome {
    /// Every value, in catalog order.
    ///
    /// The one hand-maintained list. Every consumer — the recorder's tests, the
    /// cardinality assertion, the integration label checks — iterates this
    /// rather than re-enumerating, because an enumeration written out elsewhere
    /// compiles clean while staying silently short. **`ALL` itself is one such
    /// enumeration**: adding a variant does not break this array, so its
    /// completeness is a convention, not a compile guarantee. The compile-force
    /// lives in [`Self::label`], whose wildcard-free match rejects a variant
    /// that was given no spelling — which is the prompt to add the variant here
    /// too. The written-out length is a secondary check that `ALL` and `label`
    /// agree, not a proof that `ALL` names every variant.
    pub const ALL: [Self; 5] = [
        Self::Resolved,
        Self::MeetingUnknown,
        Self::ParticipantUnknown,
        Self::RegistryFull,
        Self::UserAmbiguous,
    ];

    /// Bounded `outcome` metric-label value.
    ///
    /// Wildcard-free on purpose: a new variant must be given a spelling here
    /// rather than silently inheriting a neighbour's.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::MeetingUnknown => "meeting_unknown",
            Self::ParticipantUnknown => "participant_unknown",
            Self::RegistryFull => "registry_full",
            Self::UserAmbiguous => "user_ambiguous",
        }
    }

    /// Whether this outcome carries a real ordinal on the wire.
    ///
    /// Exactly one variant does. Expressed as a method rather than left to each
    /// call site's `matches!`, so "which outcomes answer 0" has one home.
    #[must_use]
    pub fn is_resolved(self) -> bool {
        matches!(self, Self::Resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_labels_are_distinct_and_no_variant_lands_silently() {
        let labels: HashSet<&str> = SenderBindingOutcome::ALL
            .iter()
            .map(|o| o.label())
            .collect();
        assert_eq!(
            labels.len(),
            SenderBindingOutcome::ALL.len(),
            "two outcomes share a label spelling; the metric would silently merge two distinct \
             remedies into one series"
        );

        // A tripwire, not a proof of completeness (Rust has no variant
        // reflection without a derive). The `match` is wildcard-free, so a
        // variant added to the enum cannot compile until it is named here —
        // which forces whoever adds it to this test, where `ALL` and its
        // consumers are in view, rather than letting a new outcome ship as a
        // series nobody enumerates. `ALL`'s own completeness stays a convention;
        // this makes forgetting it a compile error rather than a silent gap.
        for variant in SenderBindingOutcome::ALL {
            match variant {
                SenderBindingOutcome::Resolved
                | SenderBindingOutcome::MeetingUnknown
                | SenderBindingOutcome::ParticipantUnknown
                | SenderBindingOutcome::RegistryFull
                | SenderBindingOutcome::UserAmbiguous => {}
            }
        }
    }

    #[test]
    fn exactly_one_outcome_is_resolved() {
        let resolved = SenderBindingOutcome::ALL
            .iter()
            .filter(|o| o.is_resolved())
            .count();
        assert_eq!(
            resolved, 1,
            "exactly one outcome may carry a real ordinal; any other resolved variant would be a \
             path on which MC answers with an id it did not allocate for this participant"
        );
    }
}
