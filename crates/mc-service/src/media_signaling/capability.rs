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
//! below runs against the decoded message and the connection's budget. Nothing
//! here reads meeting state, so a client looping malformed declarations cannot
//! drive work through the shared meeting actor's mailbox. Only *accepted*
//! declarations reach the actor, and those are bounded per connection by the
//! declaration budget.
//!
//! Keep it that way: if a future check needs meeting state, it belongs in the
//! composition step, not here.

use super::outcome::{CapabilityOutcome, MediaKindLabel};
use crate::media_kind::{served_kind, KindRefusal};
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
    /// The wire kind, always a SERVED one (`AUDIO` or `VIDEO_CAMERA`): every
    /// other value is a rejection, so this field cannot hold it.
    pub media_kind: MediaKind,
    /// The slot's kind, as the slot table and the metric label read it.
    /// Projected once, here, at parse ([`served_kind`]), so no recorder ever
    /// performs a fallible lookup.
    pub kind_label: MediaKindLabel,
    /// The optional pin. Shape-validated; never used to resolve a source.
    pub pinned_sender_id: Option<PinnedSenderId>,
}

/// A client's receive-capability declaration, validated, AS SENT.
///
/// Holding one of these is proof that every check in
/// [`ReceiveCapabilityDeclaration::parse`] passed for EVERY slot — including
/// the slots per-kind truncation will later report `UNAVAILABLE`, so an
/// unvalidated slot id is never echoed (Gate 1 security S2). It is NOT yet
/// truncated: [`Self::truncate`] is a separate pure step, because the
/// neither-publish-nor-receive refusal judges the declaration as sent (R-2) and
/// truncation is never a refusal (R-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveCapabilityDeclaration {
    slots: Vec<DeclaredSlot>,
}

/// The ONE neither-publish-nor-receive rule (story 3 R-2): a participant that
/// publishes nothing AND whose declaration, AS SENT (before truncation),
/// declares no slot. Refused at join (`publishes_and_receives_nothing`), on a
/// post-join redeclaration and on a reconnect's redeclaration (both
/// `would_publish_and_receive_nothing`). Judged before truncation: a
/// receive-only client whose video slots a cap of 0 truncates declared
/// something, and is admitted (R-26).
#[must_use]
pub fn publishes_and_receives_nothing(
    publishes: crate::media_kind::PublishSet,
    declaration: &ReceiveCapabilityDeclaration,
) -> bool {
    publishes.is_empty() && !declaration.declares_any()
}

/// A declaration truncated to MC's per-kind caps (story 3 R-1, R-26).
///
/// Keeps the first `cap` slots OF EACH KIND, in declaration order; every
/// further slot of that kind is permanently `SLOT_STATE_UNAVAILABLE` for this
/// declaration. The excess ids are stored once, as plain ids inside the
/// declaration, and get no slot-table position, fill, edge or render work.
///
/// # Amplification, stated so it is bounded on purpose
///
/// The declaration is bounded only by the signalling frame
/// (`webtransport::connection::MAX_MESSAGE_SIZE`, 64 KiB), so a hostile client
/// can declare on the order of 10k minimal slots. That costs it ~10k
/// `UNAVAILABLE` entries, emitted ONLY to that same client, and only when its
/// own view changes; nothing about it reaches any other participant's view,
/// the slot table or MH. A view whose encoding would exceed the frame limit is
/// refused at composition, loudly and counted (`composition_failed`), never
/// written as a frame the client's reader must reject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotDemand {
    declaration: ReceiveCapabilityDeclaration,
    caps: [u8; 2],
}

impl SlotDemand {
    /// The declaration as sent.
    #[must_use]
    pub fn declaration(&self) -> &ReceiveCapabilityDeclaration {
        &self.declaration
    }

    /// The KEPT slot ids of `kind`, in declaration order: the subscriber's slot
    /// demand for that kind, never longer than its cap. Ordinal *i* is the
    /// *i*-th kept slot of the kind, which is what keeps a slot's ordinal (and
    /// so its `egress_stream_id`) stable across a same-count redeclaration.
    #[must_use]
    pub fn kept_slot_ids(&self, kind: MediaKindLabel) -> Vec<u16> {
        let cap = usize::from(kind.of(self.caps));
        self.declaration
            .slots
            .iter()
            .filter(|s| s.kind_label == kind)
            .take(cap)
            .map(|s| s.slot_id.get())
            .collect()
    }

    /// Every declared slot in declaration order, with its ordinal among the
    /// KEPT slots of its kind, or `None` when truncation made it UNAVAILABLE.
    #[must_use]
    pub fn entries(&self) -> Vec<(DeclaredSlot, Option<usize>)> {
        let mut seen = [0usize; 2];
        self.declaration
            .slots
            .iter()
            .map(|slot| {
                let n = slot.kind_label.of_mut(&mut seen);
                let ordinal = *n;
                *n = n.saturating_add(1);
                let cap = usize::from(slot.kind_label.of(self.caps));
                (*slot, (ordinal < cap).then_some(ordinal))
            })
            .collect()
    }
}

impl ReceiveCapabilityDeclaration {
    /// A declaration of no slots: what an absent `JoinRequest.receive_capability`
    /// means (zero slots of each kind; not an error by itself).
    #[must_use]
    pub fn empty() -> Self {
        Self { slots: Vec::new() }
    }

    /// The declared slots, in declaration order.
    ///
    /// Order is preserved rather than sorted: the assignment echoes the
    /// subscriber's own ordering back, which keeps a wire capture readable
    /// against the declaration that produced it.
    #[must_use]
    pub fn slots(&self) -> &[DeclaredSlot] {
        &self.slots
    }

    /// Does this declaration, AS SENT, ask to receive anything at all? The
    /// input to the neither-publish-nor-receive refusal (R-2), which is judged
    /// before truncation: a receive-only client whose video slots a video cap
    /// of 0 truncates entirely still declared something, and is admitted.
    #[must_use]
    pub fn declares_any(&self) -> bool {
        !self.slots.is_empty()
    }

    /// Truncate to MC's per-kind caps, laid out by [`MediaKindLabel::index`].
    #[must_use]
    pub fn truncate(self, caps: [u8; 2]) -> SlotDemand {
        SlotDemand {
            declaration: self,
            caps,
        }
    }

    /// Validate a decoded `ReceiveCapability` into a declaration MC can act on.
    ///
    /// # The work bound is the frame, not a slot count
    ///
    /// Story 3 R-1 makes an over-cap declaration a TRUNCATION, never a
    /// rejection, so there is no count check here any more (the story-2
    /// `slot_count_over_cap` refusal is retired; Lead ruling, MC-A Gate 1). The
    /// work is bounded by the signalling frame the message arrived in
    /// (`webtransport::connection::MAX_MESSAGE_SIZE`, 64 KiB): one pass and
    /// one duplicate set over an already size-bounded message, O(bytes). The
    /// budget check still runs first, O(1).
    ///
    /// # Errors
    ///
    /// A [`CapabilityOutcome`] rejection variant. **The whole declaration is
    /// rejected on any violation, on ANY slot, kept or later truncated** —
    /// never last-write-wins on a duplicate, never clamp-or-truncate on an
    /// out-of-range id, never a partially accepted slot list.
    pub fn parse(
        message: &ReceiveCapability,
        budget_remaining: bool,
    ) -> Result<Self, CapabilityOutcome> {
        if !budget_remaining {
            return Err(CapabilityOutcome::DeclarationBudgetExhausted);
        }

        let mut slots = Vec::with_capacity(message.slots.len());
        // ONE id namespace across both kinds: a slot id is the relay-region
        // `stream_id` MH stamps on the downlink, whatever the slot's kind.
        let mut seen: HashSet<u16> = HashSet::with_capacity(message.slots.len());

        for slot in &message.slots {
            // Real range check, compiled into release builds. NOT a clamp: this
            // value is forwarded into the relay-region address space, and a
            // truncated id addresses a different slot rather than failing.
            let slot_id = u16::try_from(slot.slot_id)
                .map(SlotId)
                .map_err(|_| CapabilityOutcome::SlotIdOutOfRange)?;

            if !seen.insert(slot_id.get()) {
                // "Two sources into one slot" has no fulfilment.
                return Err(CapabilityOutcome::DuplicateSlotId);
            }

            // ADMISSION, closed (`media_kind::served_kind`): screen share is a
            // kind this story does not serve and is REJECTED, never ignored
            // (story 3 R-2); UNSPECIFIED (most likely a version-skewed client
            // that meant audio) and any value outside the enum fail closed.
            let kind_label = served_kind(slot.media_kind).map_err(|refusal| match refusal {
                KindRefusal::Unsupported => CapabilityOutcome::MediaKindUnsupported,
                KindRefusal::Unspecified => CapabilityOutcome::MediaKindUnspecified,
            })?;

            // The one place MC receives an attacker-supplied sender id. Zero is
            // reserved-invalid, enforced by `NonZeroU16`.
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
                media_kind: kind_label.to_proto(),
                kind_label,
                pinned_sender_id,
            });
        }

        Ok(Self { slots })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use proto_gen::dark_tower::signaling::v1::ReceiveSlot;

    fn slot(id: u32, kind: MediaKind, pin: Option<u32>) -> ReceiveSlot {
        ReceiveSlot {
            slot_id: id,
            media_kind: kind as i32,
            pinned_sender_id: pin,
        }
    }

    fn parse(slots: Vec<ReceiveSlot>) -> Result<ReceiveCapabilityDeclaration, CapabilityOutcome> {
        ReceiveCapabilityDeclaration::parse(&ReceiveCapability { slots }, true)
    }

    const A: MediaKindLabel = MediaKindLabel::Audio;
    const V: MediaKindLabel = MediaKindLabel::Video;

    #[test]
    fn accepts_a_single_audio_slot_declaration() {
        let declaration = parse(vec![slot(0, MediaKind::Audio, None)]).unwrap();
        assert_eq!(declaration.slots().len(), 1);
        assert_eq!(declaration.slots()[0].slot_id.get(), 0);
        assert_eq!(declaration.slots()[0].media_kind, MediaKind::Audio);
        assert_eq!(declaration.slots()[0].kind_label, A);
        assert!(declaration.slots()[0].pinned_sender_id.is_none());
    }

    #[test]
    fn preserves_declaration_order() {
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

    /// Story 3 R-2: screen share is a kind this story does not serve. It is
    /// REJECTED with its own token, never accepted-and-ignored.
    #[test]
    fn rejects_a_screen_slot_as_unsupported() {
        assert_eq!(
            parse(vec![
                slot(0, MediaKind::Audio, None),
                slot(1, MediaKind::VideoScreen, None),
            ]),
            Err(CapabilityOutcome::MediaKindUnsupported)
        );
    }

    #[test]
    fn rejects_duplicate_slot_id_across_kinds_never_last_write_wins() {
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
        let d = parse(vec![slot(65_535, MediaKind::Audio, None)]).unwrap();
        assert_eq!(d.truncate([8, 4]).kept_slot_ids(A), vec![65_535]);
    }

    /// S2: a slot past the cap is validated like any other — an invalid id
    /// in the would-be-truncated tail still rejects the whole declaration, so
    /// an unvalidated id is never echoed as UNAVAILABLE.
    #[test]
    fn slots_that_truncation_would_drop_are_still_validated() {
        let mut slots: Vec<ReceiveSlot> = (0..3).map(|i| slot(i, MediaKind::Audio, None)).collect();
        slots.push(slot(70_000, MediaKind::Audio, None));
        assert_eq!(parse(slots), Err(CapabilityOutcome::SlotIdOutOfRange));
        let mut slots: Vec<ReceiveSlot> = (0..3).map(|i| slot(i, MediaKind::Audio, None)).collect();
        slots.push(slot(9, MediaKind::Audio, Some(0)));
        assert_eq!(parse(slots), Err(CapabilityOutcome::PinnedSenderIdZero));
    }

    /// R-1: over-cap is never a rejection. Each kind keeps its first `cap`
    /// slots IN DECLARATION ORDER, interleaving notwithstanding; the rest are
    /// UNAVAILABLE.
    #[test]
    fn truncation_keeps_the_first_cap_of_each_kind_in_declaration_order() {
        let d = parse(vec![
            slot(10, MediaKind::Audio, None),
            slot(20, MediaKind::VideoCamera, None),
            slot(11, MediaKind::Audio, None),
            slot(21, MediaKind::VideoCamera, None),
            slot(12, MediaKind::Audio, None),
            slot(22, MediaKind::VideoCamera, None),
        ])
        .unwrap();
        let demand = d.truncate([2, 1]);
        assert_eq!(demand.kept_slot_ids(A), vec![10, 11]);
        assert_eq!(demand.kept_slot_ids(V), vec![20]);
        let entries: Vec<(u16, Option<usize>)> = demand
            .entries()
            .iter()
            .map(|(s, o)| (s.slot_id.get(), *o))
            .collect();
        assert_eq!(
            entries,
            vec![
                (10, Some(0)),
                (20, Some(0)),
                (11, Some(1)),
                (21, None),
                (12, None),
                (22, None)
            ]
        );
    }

    /// R-26: a video cap of 0 truncates every video slot, and audio is
    /// untouched.
    #[test]
    fn a_video_cap_of_zero_keeps_no_video_slot_and_every_audio_slot() {
        let d = parse(vec![
            slot(0, MediaKind::Audio, None),
            slot(1, MediaKind::VideoCamera, None),
        ])
        .unwrap();
        let demand = d.truncate([8, 0]);
        assert_eq!(demand.kept_slot_ids(A), vec![0]);
        assert!(demand.kept_slot_ids(V).is_empty());
        assert!(demand.declaration().declares_any(), "judged as sent");
    }

    /// The bound is the frame: a declaration as large as the 64 KiB signalling
    /// frame admits is parsed in one pass and truncated, never refused for
    /// its size (Lead ruling, MC-A Gate 1).
    #[test]
    fn a_frame_sized_declaration_is_accepted_and_truncated() {
        use prost::Message;
        let mut slots = Vec::new();
        let mut id = 0u32;
        loop {
            let next = slot(id, MediaKind::VideoCamera, None);
            slots.push(next);
            if (ReceiveCapability {
                slots: slots.clone(),
            })
            .encoded_len()
                > crate::webtransport::connection::MAX_MESSAGE_SIZE - 64
            {
                slots.pop();
                break;
            }
            id += 1;
        }
        assert!(
            slots.len() > 5_000,
            "positive control: {} slots",
            slots.len()
        );
        let demand = parse(slots.clone()).unwrap().truncate([8, 4]);
        assert_eq!(demand.kept_slot_ids(V).len(), 4);
        assert_eq!(
            demand.entries().iter().filter(|(_, o)| o.is_none()).count(),
            slots.len() - 4
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
    fn zero_slot_declarations_are_accepted_and_declare_nothing() {
        let d = parse(vec![]).unwrap();
        assert!(!d.declares_any());
        assert_eq!(d, ReceiveCapabilityDeclaration::empty());
        let demand = d.truncate([8, 4]);
        assert!(demand.kept_slot_ids(A).is_empty() && demand.kept_slot_ids(V).is_empty());
    }

    #[test]
    fn rejects_when_the_declaration_budget_is_exhausted() {
        assert_eq!(
            ReceiveCapabilityDeclaration::parse(
                &ReceiveCapability {
                    slots: vec![slot(0, MediaKind::Audio, None)]
                },
                false,
            ),
            Err(CapabilityOutcome::DeclarationBudgetExhausted)
        );
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
