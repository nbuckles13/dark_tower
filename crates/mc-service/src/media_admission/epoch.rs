//! The meeting's KEK and its `sender_id` namespace, held as ONE value
//! (ADR-0036 §4; story 2 R-16).
//!
//! # Why these two live together
//!
//! A `sender_id` reused under the same KEK repeats a wrap nonce, and a repeated
//! AES-GCM nonce costs authentication-key recovery, not merely confidentiality.
//! So the namespace may be reclaimed ONLY together with a new KEK. Holding the
//! key state and the allocator as separate actor fields would make "reissue
//! before rotation" a two-line mistake; holding them here, with no way to reseat
//! the allocator except through [`AdmissionEpoch::admit`]'s reset, makes it
//! unrepresentable. The meeting actor holds an `AdmissionEpoch` and nothing
//! else of either.
//!
//! # The invariant, stated once
//!
//! **Within one KEK generation, a `sender_id` is bound to at most one identity
//! ever. An id released during generation g is reissued only under a
//! generation > g, and an epoch reset always bumps the generation.** The
//! allocator's exclusion snapshot is what gives "ever" rather than "among live
//! members"; see [`super::sender_id::SenderIdAllocator`], which records the
//! liveness-query shortcut as the prohibited implementation.
//!
//! Receivers scope per-sender replay state by `(kek_generation, sender_id)`,
//! so this invariant is what lets a reissued sender be heard at all — the
//! reciprocal statement lives in
//! `packages/sdk-core/src/media/frame/receivePath.ts`. Keep the two in step.

use super::kek::{KekGenerationFailed, KekRotationFailed, MeetingKeyState};
use super::sender_id::{Allocation, SenderId, SenderIdAllocator, SenderIdSpaceExhausted};
use ring::rand::SystemRandom;
use std::collections::{HashMap, HashSet};

/// Every `sender_id` MC has handed to a media handler as a sender binding, and
/// not yet seen released (story 2 task 9, @paired-protocol Gate-3 F-1).
///
/// # Why the roster alone is not the exclusion set
///
/// MH binds `(meeting, sender_id)` to a connection only from MC's answer to its
/// `NotifyParticipantConnected`, and releases it only when that CONNECTION closes
/// (`mh-service/src/webtransport/connection.rs`), which a roster removal does not
/// cause. A departed participant whose MH connection stays up — a buggy or
/// hostile client, or simply the SDK keeping its media transport when MC
/// signalling drops (`docs/TODO.md`) — therefore still holds its id at MH after
/// it has left MC's roster. Reissue that id at an epoch reset and MH's
/// incumbent-wins check refuses the new holder
/// (`declined_sender_binding_conflict`): the joiner has a KEK and can never send.
/// So the exclusion snapshot is the roster PLUS this ledger.
///
/// # Recorded where MC ANSWERS, not where it tracks connectivity
///
/// An entry is written whenever MC answers a binding with a sender id — even if
/// the connectivity update in the same turn is refused (e.g. a handler outside
/// the frozen set), because MH binds on the ANSWER. Reconstructing the set from
/// connectivity state at removal time would miss exactly those.
///
/// # Fails TOWARD exclusion — deliberately
///
/// MH clears its binding BEFORE it sends Disconnected, so a Disconnected seen
/// here reliably means MH has freed the id, and releasing on it is correctly
/// ordered. But that Disconnected is best-effort (fire-and-forget at MH). A lost
/// one leaves its id here, excluded from every later epoch of this meeting: one
/// namespace slot leaked, a reset pulled slightly earlier. The opposite default —
/// reclaim unless proven held — would reintroduce the refused-joiner defect on
/// every dropped notification, trading a bounded leak for a silent media loss.
/// Reconciling against MH's authoritative connection state would close the leak
/// and needs a contract change (`docs/TODO.md` §Media Path Obligations).
///
/// The OTHER direction — a `Disconnected` from a path that never unbound,
/// which would release an id MH still holds — does not arise. MH unbinds on
/// exactly one path, successful teardown, before it notifies. Its declined
/// paths also notify, but only when MC answered, and every decline holds no
/// binding when it does: the binding is the LAST step of MH's resolve, an
/// out-of-range decline returns before it, and a conflict decline mutates
/// nothing (`mh-service/src/session/mod.rs` `SenderBindings::bind`;
/// traced by @dry-reviewer at Gate 3).
///
/// A connection with no id (a pre-`connection_id` MH, the degraded rollout
/// state) cannot be matched to a release, so its id is recorded as never
/// releasable — the same fail-toward-exclusion direction.
///
/// Bounded by the MH connections MC has bound in this meeting's life, less
/// those it has seen close. MH's own connection cap bounds the live part.
#[derive(Debug, Default)]
pub struct HandedOutBindings {
    by_connection: HashMap<(String, String), SenderId>,
    unreleasable: HashSet<SenderId>,
}

impl HandedOutBindings {
    /// MC answered `(handler, connection_id)` with `sender`.
    pub fn record(&mut self, handler: &str, connection_id: &str, sender: SenderId) {
        if connection_id.is_empty() {
            self.unreleasable.insert(sender);
        } else {
            self.by_connection
                .insert((handler.to_string(), connection_id.to_string()), sender);
        }
    }

    /// MH reported `(handler, connection_id)` closed. Returns whether it named a
    /// binding MC had handed out. A connection with no id releases nothing.
    pub fn release(&mut self, handler: &str, connection_id: &str) -> bool {
        !connection_id.is_empty()
            && self
                .by_connection
                .remove(&(handler.to_string(), connection_id.to_string()))
                .is_some()
    }

    /// Every id an MH may still hold.
    pub fn held(&self) -> impl Iterator<Item = SenderId> + '_ {
        self.by_connection
            .values()
            .copied()
            .chain(self.unreleasable.iter().copied())
    }
}

/// Why an admission failed. Both arms are fail-closed: the joiner is refused
/// and NOTHING about the meeting's key state or namespace changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmitFailed {
    /// The namespace was exhausted and the KEK rotation that must accompany a
    /// reset failed. See [`KekRotationFailed`].
    Rotation(KekRotationFailed),
    /// The namespace was exhausted and a fresh epoch had nothing allocatable
    /// once the bound ids were excluded (security D-3). Fail-closed.
    ///
    /// **Its reachability is NOT bounded by the participant cap** (security
    /// F-9). The exclusion set is the roster (grace included) PLUS
    /// [`HandedOutBindings`], and that ledger is monotonic under lost releases:
    /// a lost MH `Disconnected`, and every binding without a `connection_id`,
    /// stays excluded for the meeting's life. So the set can grow with
    /// accumulated OPERATIONAL churn — MH crashes, restarts, lost notifications —
    /// over a long-lived meeting, independent of how many participants are ever
    /// present at once. Each such event leaks at most the ids bound on one
    /// handler at that moment (roster-scale), and `connection_id` comes from MH,
    /// not the client, so this is not a cheap attack primitive; but it is a
    /// bound of a different KIND from the participant cap, and reaching it
    /// refuses admission permanently. What would restore a real bound is
    /// reconciling the ledger against MH's live bindings (`docs/TODO.md`
    /// §Media Path Obligations).
    NoAllocatableId,
}

/// A successful admission.
#[derive(Debug, Clone, Copy)]
pub struct Admitted {
    /// The joiner's id, never issued before in the current generation.
    pub allocation: Allocation,
    /// True iff this admission performed a KEK-epoch reset. The caller MUST then
    /// push the new KEK to every existing member before broadcasting the join.
    pub epoch_reset: bool,
}

/// A meeting's KEK state and its `sender_id` allocator, reseated only together.
#[derive(Debug)]
pub struct AdmissionEpoch {
    keys: MeetingKeyState,
    sender_ids: SenderIdAllocator,
    /// Test seam: where a post-reset allocator's cursor starts. Lets a test
    /// place a reset's exclusion set in the cursor's path without 65,535 joins.
    #[cfg(feature = "test-seams")]
    reset_cursor: Option<std::num::NonZeroU16>,
}

impl AdmissionEpoch {
    /// Generation-0 key state and a fresh namespace for a new meeting.
    ///
    /// # Errors
    ///
    /// [`KekGenerationFailed`] if the system CSPRNG fails. Fail closed: the
    /// meeting is not created.
    pub fn generate(rng: &SystemRandom) -> Result<Self, KekGenerationFailed> {
        Ok(Self::with_allocator(
            MeetingKeyState::generate(rng)?,
            SenderIdAllocator::new(),
        ))
    }

    fn with_allocator(keys: MeetingKeyState, sender_ids: SenderIdAllocator) -> Self {
        Self {
            keys,
            sender_ids,
            #[cfg(feature = "test-seams")]
            reset_cursor: None,
        }
    }

    /// Construct with test-only overrides. **Test builds only.**
    #[cfg(feature = "test-seams")]
    pub(crate) fn with_seams(
        rng: &SystemRandom,
        sender_ids: SenderIdAllocator,
        reset_cursor: Option<std::num::NonZeroU16>,
    ) -> Result<Self, KekGenerationFailed> {
        let mut epoch = Self::with_allocator(MeetingKeyState::generate(rng)?, sender_ids);
        epoch.reset_cursor = reset_cursor;
        Ok(epoch)
    }

    /// The current KEK and generation.
    pub fn keys(&self) -> &MeetingKeyState {
        &self.keys
    }

    /// Cursor consumption in the current epoch, including ids skipped by the
    /// exclusion snapshot. NOT an admission count. This is what
    /// `mc_meeting_sender_ids_issued_max` publishes.
    pub fn sender_ids_issued(&self) -> u32 {
        self.sender_ids.issued()
    }

    /// Ids the cursor has not yet passed in the current epoch.
    pub fn sender_ids_remaining(&self) -> u32 {
        self.sender_ids.remaining()
    }

    /// Leave-triggered rotation: new KEK, generation + 1, namespace UNCHANGED.
    ///
    /// The allocator is deliberately kept. Its cursor is monotonic, so no id is
    /// reissued across this boundary at all — the invariant holds trivially.
    ///
    /// # Errors
    ///
    /// As [`MeetingKeyState::rotate`]; nothing changes on failure.
    pub fn rotate(&mut self, rng: &SystemRandom) -> Result<(), KekRotationFailed> {
        self.keys.rotate(rng)
    }

    /// Admit a joiner, performing an immediate KEK-epoch reset if the namespace
    /// is exhausted.
    ///
    /// `bound` is called ONLY on the reset path, and must return every id bound
    /// right now — live AND grace-period members (security D-1), PLUS every id a
    /// media handler may still hold ([`HandedOutBindings`], Gate-3 F-1). It is
    /// taken once and becomes the new epoch's permanent exclusion snapshot.
    ///
    /// # Atomicity (security D-2)
    ///
    /// On the reset path the fallible steps run FIRST — build the new
    /// allocator, allocate the joiner's id from it, rotate the KEK — and only
    /// then is the allocator swapped in. A failure at any step leaves the key,
    /// the generation and the namespace exactly as they were. There is no
    /// interleaving in which a reseated allocator exists under the old
    /// generation.
    ///
    /// The reset is IMMEDIATE and exempt from the leave debounce: debouncing a
    /// fail-closed admission condition would stall joins for up to W.
    ///
    /// # Errors
    ///
    /// [`AdmitFailed`]; the joiner must be refused.
    pub fn admit(
        &mut self,
        rng: &SystemRandom,
        bound: impl FnOnce() -> HashSet<SenderId>,
    ) -> Result<Admitted, AdmitFailed> {
        match self.sender_ids.allocate() {
            Ok(allocation) => Ok(Admitted {
                allocation,
                epoch_reset: false,
            }),
            Err(SenderIdSpaceExhausted) => {
                let mut fresh = self.fresh_allocator(bound());
                let allocation = fresh
                    .allocate()
                    .map_err(|SenderIdSpaceExhausted| AdmitFailed::NoAllocatableId)?;
                self.keys.rotate(rng).map_err(AdmitFailed::Rotation)?;
                // Both fallible steps succeeded; the swap cannot fail.
                self.sender_ids = fresh;
                Ok(Admitted {
                    allocation,
                    epoch_reset: true,
                })
            }
        }
    }

    #[cfg(not(feature = "test-seams"))]
    fn fresh_allocator(&self, bound: HashSet<SenderId>) -> SenderIdAllocator {
        SenderIdAllocator::for_epoch(bound)
    }

    #[cfg(feature = "test-seams")]
    fn fresh_allocator(&self, bound: HashSet<SenderId>) -> SenderIdAllocator {
        match self.reset_cursor {
            Some(cursor) => SenderIdAllocator::resuming_from(Some(cursor), bound),
            None => SenderIdAllocator::for_epoch(bound),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::num::NonZeroU16;

    fn id(v: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(v).unwrap())
    }

    /// An epoch whose allocator is already exhausted, at generation 0.
    fn exhausted_epoch(rng: &SystemRandom) -> AdmissionEpoch {
        let mut alloc = SenderIdAllocator::new();
        while alloc.allocate().is_ok() {}
        AdmissionEpoch::with_allocator(MeetingKeyState::generate(rng).unwrap(), alloc)
    }

    #[test]
    fn ordinary_admission_does_not_touch_the_key() {
        let rng = SystemRandom::new();
        let mut epoch = AdmissionEpoch::generate(&rng).unwrap();
        let admitted = epoch.admit(&rng, HashSet::new).unwrap();
        assert!(!admitted.epoch_reset);
        assert_eq!(epoch.keys().generation(), 0);
    }

    /// The whole 16-bit space, in memory: every id is issued once, and the
    /// admission after the last one resets the epoch rather than failing.
    #[test]
    fn full_space_then_reset_admits_and_bumps_the_generation() {
        let rng = SystemRandom::new();
        let mut epoch = AdmissionEpoch::generate(&rng).unwrap();
        let mut seen = HashSet::new();
        for _ in 0..u16::MAX {
            let a = epoch.admit(&rng, HashSet::new).unwrap();
            assert!(!a.epoch_reset);
            assert!(seen.insert(a.allocation.sender_id));
        }
        let before = *epoch.keys().kek().expose();
        let bound: HashSet<_> = [id(1), id(7)].into_iter().collect();
        let reset = epoch.admit(&rng, || bound).unwrap();
        assert!(reset.epoch_reset, "exhaustion resets rather than refusing");
        assert_eq!(
            epoch.keys().generation(),
            1,
            "a reset always bumps the generation"
        );
        assert_ne!(
            *epoch.keys().kek().expose(),
            before,
            "and always installs a new KEK"
        );
        assert_eq!(
            reset.allocation.sender_id,
            id(2),
            "the fresh epoch skips the bound ids"
        );
    }

    /// The bound set is consulted only on the reset path — never on the
    /// ordinary one, so the common admission does not pay a roster walk.
    #[test]
    fn bound_is_not_computed_on_the_ordinary_path() {
        let rng = SystemRandom::new();
        let mut epoch = AdmissionEpoch::generate(&rng).unwrap();
        epoch
            .admit(&rng, || panic!("bound() must not run without a reset"))
            .unwrap();
    }

    /// D-2 + D-3 together: a reset with nothing allocatable fails closed and
    /// changes NOTHING — the discriminating case for atomicity, because a
    /// non-atomic implementation that rotated first would pass every happy-path
    /// test and leave the generation bumped with no allocator swapped.
    #[test]
    fn reset_with_nothing_allocatable_fails_closed_and_changes_nothing() {
        let rng = SystemRandom::new();
        let mut epoch = exhausted_epoch(&rng);
        let before = *epoch.keys().kek().expose();
        let everything: HashSet<_> = (1..=u16::MAX).map(id).collect();
        let err = epoch.admit(&rng, || everything).unwrap_err();
        assert_eq!(err, AdmitFailed::NoAllocatableId);
        assert_eq!(
            epoch.keys().generation(),
            0,
            "no generation bump on failure"
        );
        assert_eq!(
            *epoch.keys().kek().expose(),
            before,
            "no key change on failure"
        );
        assert_eq!(
            epoch.sender_ids_remaining(),
            0,
            "the old allocator is still installed"
        );
    }

    /// A failed KEK rotation on the reset path also changes nothing.
    #[test]
    fn reset_with_exhausted_generation_fails_closed_and_changes_nothing() {
        let rng = SystemRandom::new();
        let mut alloc = SenderIdAllocator::new();
        while alloc.allocate().is_ok() {}
        let mut epoch =
            AdmissionEpoch::with_allocator(MeetingKeyState::at_generation(&rng, u16::MAX), alloc);
        let err = epoch.admit(&rng, HashSet::new).unwrap_err();
        assert_eq!(
            err,
            AdmitFailed::Rotation(KekRotationFailed::GenerationExhausted)
        );
        assert_eq!(
            epoch.sender_ids_remaining(),
            0,
            "the fresh allocator was built but never swapped in"
        );
    }

    #[test]
    fn a_handed_out_binding_is_held_until_its_connection_is_reported_closed() {
        let mut ledger = HandedOutBindings::default();
        ledger.record("mh-1", "conn-a", id(7));
        assert_eq!(ledger.held().collect::<Vec<_>>(), vec![id(7)]);
        assert!(
            !ledger.release("mh-2", "conn-a"),
            "a different handler's close names nothing"
        );
        assert!(ledger.release("mh-1", "conn-a"));
        assert_eq!(ledger.held().count(), 0);
        assert!(
            !ledger.release("mh-1", "conn-a"),
            "a duplicate close releases nothing"
        );
    }

    /// A connection with no id cannot be matched to a release, so it fails
    /// TOWARD exclusion: never released.
    #[test]
    fn a_binding_without_a_connection_id_is_never_released() {
        let mut ledger = HandedOutBindings::default();
        ledger.record("mh-1", "", id(9));
        assert!(!ledger.release("mh-1", ""));
        assert_eq!(ledger.held().collect::<Vec<_>>(), vec![id(9)]);
    }

    /// Leave rotation keeps the namespace: the cursor continues, so no id is
    /// reissued across the boundary.
    #[test]
    fn leave_rotation_keeps_the_allocator() {
        let rng = SystemRandom::new();
        let mut epoch = AdmissionEpoch::generate(&rng).unwrap();
        let first = epoch
            .admit(&rng, HashSet::new)
            .unwrap()
            .allocation
            .sender_id;
        epoch.rotate(&rng).unwrap();
        let second = epoch
            .admit(&rng, HashSet::new)
            .unwrap()
            .allocation
            .sender_id;
        assert_eq!(epoch.keys().generation(), 1);
        assert!(second.get() > first.get(), "the cursor did not restart");
    }
}
