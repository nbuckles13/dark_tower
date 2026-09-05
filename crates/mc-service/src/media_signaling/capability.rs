//! Parsing and validation of a client's receive-capability declaration
//! (ADR-0036 §6).
//!
//! # Parse, don't validate
//!
//! [`ReceiveCapabilityDeclaration`] is constructible only through
//! [`ReceiveCapabilityDeclaration::parse`], and every field it holds has already
//! passed its check. A partially-accepted declaration is therefore
//! *unrepresentable* rather than merely avoided by discipline: there is no way
//! to build one holding an out-of-range slot id, a duplicate, or a zero pin.
//!
//! # Every rejection is decidable without touching the meeting actor
//!
//! This is a load-bearing security property, not an incidental one. Every check
//! below runs against the decoded message plus the caller's cached
//! [`PlannedAudioSlot`] — an owned `u16` resolved once per connection at join.
//! Nothing here reads meeting state, so a client looping malformed or
//! unservable declarations cannot drive O(N) roster clones through the shared
//! meeting actor's mailbox. Only *accepted* declarations reach that path, and
//! those are bounded per connection by the declaration budget.
//!
//! Keep it that way: if a future check needs meeting state, it belongs in the
//! composition step, not here.

use super::outcome::CapabilityOutcome;
use proto_gen::dark_tower::signaling::v1::{MediaKind, ReceiveCapability};
use std::collections::HashSet;
use std::num::NonZeroU16;

/// A subscriber-chosen receive slot identifier.
///
/// A `u16` because the value shares the frame's relay-region `stream_id` value
/// space — which is precisely what lets a receiver validate an arriving
/// `stream_id` against its own declared slots.
///
/// # Three lookalike numeric bounds, named so they are not collapsed
///
/// This tree carries three 16-or-8-bit id bounds that look interchangeable and
/// are not. Do **not** introduce a shared `MAX_16BIT_ID` constant or a shared
/// `validate_16bit_id()` helper across them — `signaling.proto` bars it
/// explicitly, and collapsing them silently loses the invariants that differ:
///
/// | Bound | Kind | Scope | Reuse |
/// |---|---|---|---|
/// | `SlotId` (here), 16-bit | **validation** bound, relay-region derived | per subscriber connection | freely reusable across declarations, and across subscribers |
/// | [`SenderId`], 16-bit | **allocation** bound, key-id derived | per meeting | never recycled while a KEK is live (R-35) |
/// | [`super::directive::StreamNumber`], 8-bit | **allocation** bound, key-id derived | per publisher | one per media stream a publisher produces |
///
/// # Observability
///
/// A slot is a stream identity (ADR-0036 §11). Never a metric label, never a
/// span attribute, never a per-frame log dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(u16);

impl SlotId {
    /// The numeric value.
    #[must_use]
    pub fn get(self) -> u16 {
        self.0
    }

    /// Wrap a slot id MC itself produced (an assignment's `slot_id`, which is a
    /// `u16` already). Infallible by type; there is deliberately no `u32`
    /// constructor other than the validating one on the client path.
    #[must_use]
    pub fn from_u16(value: u16) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for SlotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A source a subscriber asked to see in one of its slots.
///
/// Deliberately **not** a [`SenderId`]. A pin is an attacker-supplied number on
/// the client-to-MC wire; a `SenderId` is a handle MC's allocator issued and
/// never recycles. Minting one from client input would let a peer name an id
/// the allocator has not issued, and the type would then assert an invariant it
/// does not have. MC validates a pin's *shape* and never resolves a source from
/// it — source identity comes from the assignment output alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedSenderId(NonZeroU16);

impl PinnedSenderId {
    /// The numeric value, always in `1..=65535`.
    #[must_use]
    pub fn get(self) -> NonZeroU16 {
        self.0
    }
}

/// One validated slot from a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclaredSlot {
    /// The subscriber's identifier for this slot, echoed back on the assignment.
    pub slot_id: SlotId,
    /// What this slot can decode. Never `MEDIA_KIND_UNSPECIFIED` — that is a
    /// rejection, so this field cannot hold it.
    pub media_kind: MediaKind,
    /// The optional pin. Shape-validated; never used to resolve a source.
    pub pinned_sender_id: Option<PinnedSenderId>,
}

/// A client's receive-capability declaration, validated.
///
/// Holding one of these is proof that every check in
/// [`ReceiveCapabilityDeclaration::parse`] passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveCapabilityDeclaration {
    slots: Vec<DeclaredSlot>,
}

impl ReceiveCapabilityDeclaration {
    /// The declared slots, in declaration order.
    ///
    /// Order is preserved rather than sorted: the assignment echoes the
    /// subscriber's own ordering back, which keeps a wire capture readable
    /// against the declaration that produced it.
    #[must_use]
    pub fn slots(&self) -> &[DeclaredSlot] {
        &self.slots
    }

    /// Does this declaration contain a slot for `kind` numbered `slot_id`?
    #[must_use]
    pub fn contains(&self, slot_id: SlotId, kind: MediaKind) -> bool {
        self.slots
            .iter()
            .any(|s| s.slot_id == slot_id && s.media_kind == kind)
    }

    /// Does this declaration ask to receive any audio at all?
    #[must_use]
    pub fn declares_audio(&self) -> bool {
        self.slots.iter().any(|s| s.media_kind == MediaKind::Audio)
    }

    /// Validate a decoded `ReceiveCapability` into a declaration MC can act on.
    ///
    /// # Check order is load-bearing, not stylistic
    ///
    /// The two O(1) comparisons — declaration budget, then slot count against
    /// the configured cap — run **before** any per-slot iteration and before the
    /// duplicate-detection set is allocated. A hostile client packing a 64 KiB
    /// frame with tens of thousands of minimal slots must cost a length compare,
    /// not a multi-thousand-entry `HashSet` build. Same discipline as the
    /// per-message fan-out bound in `webtransport::connection`.
    ///
    /// # Errors
    ///
    /// A [`CapabilityOutcome`] rejection variant. **The whole declaration is
    /// rejected on any violation** — never last-write-wins on a duplicate,
    /// never clamp-or-truncate on an out-of-range id, never a partially
    /// accepted slot list. A declaration is one statement about what a
    /// subscriber can decode; half of it is not a weaker version of it.
    pub fn parse(
        message: &ReceiveCapability,
        max_slots: usize,
        budget_remaining: bool,
        planned_audio_slot: PlannedAudioSlot,
    ) -> Result<Self, CapabilityOutcome> {
        if !budget_remaining {
            return Err(CapabilityOutcome::DeclarationBudgetExhausted);
        }

        if message.slots.len() > max_slots {
            return Err(CapabilityOutcome::SlotCountOverCap);
        }

        let mut slots = Vec::with_capacity(message.slots.len());
        let mut seen: HashSet<u16> = HashSet::with_capacity(message.slots.len());

        for slot in &message.slots {
            // Real range check, compiled into release builds. NOT a
            // `debug_assert!` — release profiles do not enable debug
            // assertions, so it would be compiled out in exactly the builds
            // facing untrusted clients. NOT a clamp: this value is forwarded
            // into the relay-region address space, and a truncated id addresses
            // a different slot rather than failing.
            let slot_id = u16::try_from(slot.slot_id)
                .map(SlotId)
                .map_err(|_| CapabilityOutcome::SlotIdOutOfRange)?;

            if !seen.insert(slot_id.get()) {
                // A duplicate means "two sources into one slot", which has no
                // fulfilment. Reject rather than let the last one win.
                return Err(CapabilityOutcome::DuplicateSlotId);
            }

            // Fail closed on the proto3 zero. `MEDIA_KIND_UNSPECIFIED` is what
            // a pre-ADR-0036 peer's `AUDIO = 0` decodes to, so the likeliest
            // producer is a version-skewed client that MEANT audio — not one
            // that forgot a field. Reading it as "a kind we happen not to have"
            // would turn a client-fleet rollback into "everyone's meetings are
            // empty" with no counter naming skew. `MediaKind::try_from` also
            // rejects any value outside the enum, which a stale or hostile peer
            // can send.
            let media_kind = MediaKind::try_from(slot.media_kind)
                .map_err(|_| CapabilityOutcome::MediaKindUnspecified)?;
            if media_kind == MediaKind::Unspecified {
                return Err(CapabilityOutcome::MediaKindUnspecified);
            }

            // The one place MC receives an attacker-supplied sender id. Zero is
            // reserved-invalid, so `Some(0)` is rejected like any other
            // out-of-range pin — and `NonZeroU16` is what enforces it, not a
            // bare `u16::try_from`, which would accept the one value the
            // contract reserves.
            let pinned_sender_id = match slot.pinned_sender_id {
                None => None,
                Some(raw) => {
                    let narrowed = u16::try_from(raw)
                        .map_err(|_| CapabilityOutcome::PinnedSenderIdOutOfRange)?;
                    Some(PinnedSenderId(
                        NonZeroU16::new(narrowed).ok_or(CapabilityOutcome::PinnedSenderIdZero)?,
                    ))
                }
            };

            slots.push(DeclaredSlot {
                slot_id,
                media_kind,
                pinned_sender_id,
            });
        }

        let declaration = Self { slots };

        // The namespace check, and the only one that is NOT a client defect.
        //
        // MH stamps the relay-region `stream_id` from the egress slot id in the
        // policy MC pushed at first-participant join — before this client could
        // declare anything. A subscriber asking for audio in a slot MC has no
        // plan for is making a request the wire contract permits and MC cannot
        // serve: its frames would arrive stamped with a slot it never opened.
        //
        // Rejecting is the honest answer. Accepting and reporting the slot as a
        // source shortage would tell the client "there is nobody to show you"
        // while MH actively forwards a source it will drop — a false wire state
        // that leaves a healthy-looking session with no audio.
        //
        // ZERO-AUDIO DECLARATIONS ARE EXEMPT. A subscriber that declares no
        // audio slot is saying "I want to send but not receive audio", which the
        // contract anticipates and about which MC makes no false claim. Only a
        // declaration that ASKS for audio and names a slot MC cannot fill is
        // rejected.
        //
        // This rejection is retired — not weakened — by the capability-triggered
        // policy re-push (see the module doc and `docs/TODO.md` §Media Path
        // Obligations). Do not relax the equality into a kind-only match to make
        // it "more flexible"; that darkens the media path silently.
        if declaration.declares_audio()
            && !declaration.contains(planned_audio_slot.slot_id(), MediaKind::Audio)
        {
            return Err(CapabilityOutcome::SlotIdNotPlanned);
        }

        Ok(declaration)
    }
}

/// The audio slot id MC's forwarding assignment routes this subscriber's audio
/// into, resolved once per connection at join.
///
/// # Why this is cached rather than recomputed
///
/// Caching is what keeps every rejection decidable without touching the meeting
/// actor. If the namespace check read live meeting state, a client looping
/// unservable declarations would drive an O(N) roster clone per message through
/// the shared actor while never being charged the declaration budget, which
/// charges only on acceptance.
///
/// # Why caching is sound in this story, and when it stops being
///
/// The value can only change if the pushed forwarding policy changes, and the
/// only trigger for that today is the first-participant join, which precedes
/// every post-join dispatch on every connection. When the capability-triggered
/// re-push lands, this cache and the [`CapabilityOutcome::SlotIdNotPlanned`]
/// rejection are retired together.
///
/// Derived from the assignment output rather than reconstructed from
/// `MAIN_AUDIO_SLOT_ID`, so MC keeps exactly one answer to "which slot do I
/// route into".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedAudioSlot {
    slot_id: SlotId,
}

impl PlannedAudioSlot {
    /// Record the slot MC's assignment routes this subscriber's audio into.
    ///
    /// Deliberately does NOT carry the subscriber id: the caller already used it
    /// to *find* the plan, so a stored copy would be state with no reader — and
    /// dead state kept alive behind an `allow(dead_code)` is exactly what stops
    /// being one field later.
    #[must_use]
    pub fn new(slot_id: SlotId) -> Self {
        Self { slot_id }
    }

    /// The planned slot id.
    #[must_use]
    pub fn slot_id(self) -> SlotId {
        self.slot_id
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use proto_gen::dark_tower::signaling::v1::ReceiveSlot;

    const CAP: usize = 8;

    fn planned(slot: u16) -> PlannedAudioSlot {
        PlannedAudioSlot::new(SlotId::from_u16(slot))
    }

    fn slot(id: u32, kind: MediaKind, pin: Option<u32>) -> ReceiveSlot {
        ReceiveSlot {
            slot_id: id,
            media_kind: kind as i32,
            pinned_sender_id: pin,
        }
    }

    fn parse(slots: Vec<ReceiveSlot>) -> Result<ReceiveCapabilityDeclaration, CapabilityOutcome> {
        ReceiveCapabilityDeclaration::parse(&ReceiveCapability { slots }, CAP, true, planned(0))
    }

    #[test]
    fn accepts_the_loopback_declaration() {
        let declaration = parse(vec![slot(0, MediaKind::Audio, None)]).unwrap();
        assert_eq!(declaration.slots().len(), 1);
        assert_eq!(declaration.slots()[0].slot_id.get(), 0);
        assert_eq!(declaration.slots()[0].media_kind, MediaKind::Audio);
        assert!(declaration.slots()[0].pinned_sender_id.is_none());
    }

    #[test]
    fn preserves_declaration_order() {
        // The assignment echoes the subscriber's own ordering back, which is
        // what keeps a wire capture readable against the declaration that
        // produced it. Sorting here would silently break that.
        let declaration = parse(vec![
            slot(3, MediaKind::Audio, None),
            slot(0, MediaKind::Audio, None),
            slot(1, MediaKind::VideoCamera, None),
        ])
        .unwrap();
        let ids: Vec<u16> = declaration
            .slots()
            .iter()
            .map(|s| s.slot_id.get())
            .collect();
        assert_eq!(ids, vec![3, 0, 1]);
    }

    #[test]
    fn rejects_duplicate_slot_id_never_last_write_wins() {
        assert_eq!(
            parse(vec![
                slot(0, MediaKind::Audio, None),
                slot(0, MediaKind::VideoCamera, None),
            ]),
            Err(CapabilityOutcome::DuplicateSlotId)
        );
    }

    #[test]
    fn rejects_slot_id_above_u16_never_truncates() {
        // 65536 truncates to 0 under `as`, which would silently address the
        // planned loopback slot. It must be an error.
        assert_eq!(
            parse(vec![slot(65_536, MediaKind::Audio, None)]),
            Err(CapabilityOutcome::SlotIdOutOfRange)
        );
        assert_eq!(
            parse(vec![slot(u32::MAX, MediaKind::Audio, None)]),
            Err(CapabilityOutcome::SlotIdOutOfRange)
        );
    }

    #[test]
    fn accepts_slot_id_at_the_u16_boundary() {
        // 65535 is in range; the bound is inclusive. Rejected only because it
        // is not the planned slot, which is a different check — asserted here
        // so an over-tight range check cannot hide behind the namespace one.
        let outcome = parse(vec![slot(65_535, MediaKind::Audio, None)]);
        assert_eq!(outcome, Err(CapabilityOutcome::SlotIdNotPlanned));
    }

    #[test]
    fn rejects_slot_count_over_cap() {
        let slots = (0..=u32::try_from(CAP).unwrap())
            .map(|i| slot(i, MediaKind::Audio, None))
            .collect();
        assert_eq!(parse(slots), Err(CapabilityOutcome::SlotCountOverCap));
    }

    #[test]
    fn slot_cap_is_configuration_not_a_constant() {
        let three = vec![
            slot(0, MediaKind::Audio, None),
            slot(1, MediaKind::Audio, None),
            slot(2, MediaKind::Audio, None),
        ];
        assert!(ReceiveCapabilityDeclaration::parse(
            &ReceiveCapability {
                slots: three.clone()
            },
            3,
            true,
            planned(0)
        )
        .is_ok());
        assert_eq!(
            ReceiveCapabilityDeclaration::parse(
                &ReceiveCapability { slots: three },
                2,
                true,
                planned(0)
            ),
            Err(CapabilityOutcome::SlotCountOverCap)
        );
    }

    #[test]
    fn rejects_zero_pin_and_out_of_range_pin_distinctly() {
        assert_eq!(
            parse(vec![slot(0, MediaKind::Audio, Some(0))]),
            Err(CapabilityOutcome::PinnedSenderIdZero)
        );
        assert_eq!(
            parse(vec![slot(0, MediaKind::Audio, Some(65_536))]),
            Err(CapabilityOutcome::PinnedSenderIdOutOfRange)
        );
    }

    #[test]
    fn accepts_a_well_formed_pin_without_resolving_a_source_from_it() {
        let declaration = parse(vec![slot(0, MediaKind::Audio, Some(7))]).unwrap();
        let pin = declaration.slots()[0].pinned_sender_id.unwrap();
        assert_eq!(pin.get().get(), 7);
        // A pin is a `PinnedSenderId`, deliberately NOT a `SenderId`: minting an
        // allocator handle from client input would assert an invariant the value
        // does not have. Source identity comes from the assignment alone.
    }

    #[test]
    fn rejects_unspecified_media_kind_fail_closed() {
        assert_eq!(
            parse(vec![slot(0, MediaKind::Unspecified, None)]),
            Err(CapabilityOutcome::MediaKindUnspecified)
        );
    }

    #[test]
    fn rejects_media_kind_outside_the_enum() {
        // A stale or hostile peer can put any i32 on the wire.
        let bogus = ReceiveSlot {
            slot_id: 0,
            media_kind: 4242,
            pinned_sender_id: None,
        };
        assert_eq!(
            parse(vec![bogus]),
            Err(CapabilityOutcome::MediaKindUnspecified)
        );
    }

    #[test]
    fn rejects_audio_in_a_slot_mc_does_not_plan() {
        assert_eq!(
            parse(vec![slot(7, MediaKind::Audio, None)]),
            Err(CapabilityOutcome::SlotIdNotPlanned)
        );
        assert_eq!(
            parse(vec![
                slot(1, MediaKind::Audio, None),
                slot(2, MediaKind::Audio, None)
            ]),
            Err(CapabilityOutcome::SlotIdNotPlanned)
        );
    }

    #[test]
    fn accepts_extra_audio_slots_alongside_the_planned_one() {
        // The predicate is "a planned slot is absent", NOT "an unplanned slot is
        // present". Rejecting this would swallow the genuine source-shortage
        // case, which is what `FEWER_SOURCES_THAN_SLOTS` exists to convey.
        let declaration = parse(vec![
            slot(0, MediaKind::Audio, None),
            slot(7, MediaKind::Audio, None),
        ])
        .unwrap();
        assert_eq!(declaration.slots().len(), 2);
    }

    #[test]
    fn zero_audio_declarations_are_exempt_from_the_namespace_check() {
        // "I want to send but not receive audio" is a declaration the contract
        // anticipates, and MC makes no false claim by accepting it — unlike the
        // asks-for-audio-in-an-unservable-slot case.
        assert!(parse(vec![]).is_ok());
        assert!(parse(vec![slot(1, MediaKind::VideoCamera, None)]).is_ok());
        assert!(parse(vec![slot(9, MediaKind::VideoScreen, None)]).is_ok());
    }

    #[test]
    fn rejects_when_the_declaration_budget_is_exhausted() {
        assert_eq!(
            ReceiveCapabilityDeclaration::parse(
                &ReceiveCapability {
                    slots: vec![slot(0, MediaKind::Audio, None)]
                },
                CAP,
                false,
                planned(0)
            ),
            Err(CapabilityOutcome::DeclarationBudgetExhausted)
        );
    }

    #[test]
    fn a_rejected_declaration_leaves_no_partially_accepted_state() {
        // Parse-don't-validate: the only way to obtain a declaration is through
        // `parse`, so a rejection yields NO value at all rather than a
        // half-populated one. This test states the property; the type system
        // enforces it.
        let outcome = parse(vec![
            slot(0, MediaKind::Audio, None),
            slot(0, MediaKind::Audio, None),
        ]);
        assert!(outcome.is_err());
    }

    #[test]
    fn every_outcome_has_a_distinct_bounded_label() {
        let labels: std::collections::HashSet<&str> =
            CapabilityOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(labels.len(), CapabilityOutcome::ALL.len());
        for outcome in CapabilityOutcome::ALL {
            assert!(!outcome.label().is_empty());
            assert!(!outcome.client_message().is_empty());
        }
    }
}
