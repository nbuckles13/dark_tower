//! Per-meeting `sender_id` allocation (ADR-0036 §2, §4; invariant R-35).

use media_protocol::frame::KEY_ID_SENDER_ID_BITS;
use std::collections::HashSet;
use std::num::NonZeroU16;

/// The sender component of the SFrame key id is 16 bits wide, so [`NonZeroU16`]
/// **is** the bound.
///
/// This assertion is the single source of truth link: there is deliberately no
/// local `65535`, no `1 << 16` and no width constant in this file, so there is
/// nothing that can drift from the anchor. If the key-id layout ever reallots
/// the sender field, this fails the build rather than letting MC issue ids that
/// truncate into a narrower field.
const _: () = {
    assert!(
        KEY_ID_SENDER_ID_BITS == u16::BITS as usize,
        "sender_id is carried in the key id's sender field; NonZeroU16 must be exactly that width \
         or MC will allocate ids that truncate on encode and corrupt frame attribution"
    );
};

/// Fraction of the namespace consumed at which the meeting logs a one-shot
/// warning.
///
/// A **leading indicator** of an upcoming KEK-epoch reset (story 2 R-16).
/// Exhaustion is no longer terminal: at 100% MC rotates the KEK and reissues
/// ids from a fresh namespace, and the meeting keeps admitting. So this warning
/// now precedes a *recoverable* event rather than a permanent break, and the
/// action it prompts is to investigate the driver — a flapping client or a
/// scripted join loop consuming ids at a rate no human churn reaches — not to
/// end the meeting.
///
/// The latch is per-allocator, and each epoch reset installs a fresh
/// allocator, so this re-arms every epoch: it fires before EACH reset.
///
/// Deliberately a compile-time constant rather than configuration — it selects
/// no behaviour.
const SENDER_ID_HIGH_WATERMARK_PERCENT: u32 = 90;

/// The allocator could not produce an id.
///
/// Two paths reach this, and neither is a join failure an operator sees:
///
/// - **The live namespace ran out.** The meeting actor catches this and
///   performs a KEK-epoch reset (`super::epoch::AdmissionEpoch::admit`), so the
///   admission succeeds. This is the common path, and it is recovered, not
///   surfaced.
/// - **A fresh epoch has nothing allocatable** (security D-3). The epoch reset
///   fails closed rather than admitting. The exclusion set is the roster
///   (grace included) PLUS every id a media handler may still hold
///   (`super::epoch::HandedOutBindings`), and that second part only grows under
///   lost MH notifications. So this is reachable through accumulated
///   operational churn over a long-lived meeting — NOT bounded by the
///   participant cap. See `super::epoch::AdmitFailed::NoAllocatableId` for the
///   bound and what would restore a real one.
///
/// Neither is a wrap. See [`SenderIdAllocator`] for why that matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SenderIdSpaceExhausted;

impl std::fmt::Display for SenderIdSpaceExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("sender id space exhausted")
    }
}

impl std::error::Error for SenderIdSpaceExhausted {}

/// A participant's compact per-meeting sender handle.
///
/// Valid range 1..=65535. **Zero is reserved-invalid**, which is why this wraps
/// [`NonZeroU16`] rather than `u16`: a bare `u16::try_from` would accept 0, the
/// one value the contract reserves. Two senders sharing 0 is not a recycled id
/// but N concurrently-live colliding ones — and since the key id encodes
/// `(sender, stream, generation)`, the wrap nonce derives from the key id, and
/// the key id is the associated data, two senders at 0 with the same stream and
/// generation wrap distinct transmit keys under the same KEK at the same nonce.
/// That is AES-GCM authentication-key recovery, not merely a confidentiality
/// loss (ADR-0036 §4).
///
/// This is a **per-meeting** id MC allocates. It is not derived from, and is
/// not, a stable user identity — carrying a durable user id into the media path
/// would reintroduce exactly the cross-meeting linkability that §4's
/// meeting-scoped keys exist to prevent.
///
/// Observability: never a metric label, never a span attribute, never a
/// per-frame log dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SenderId(NonZeroU16);

impl SenderId {
    /// The numeric value, always in 1..=65535.
    pub fn get(self) -> NonZeroU16 {
        self.0
    }

    /// Construct a `SenderId` directly. **Test builds only.**
    ///
    /// Deliberately NOT available in production: [`SenderIdAllocator`] is the
    /// only way a `SenderId` comes into existence there, which is what makes
    /// R-35 a property of the type system rather than of reviewer vigilance.
    ///
    /// What the types guarantee, stated precisely since story 2 R-16: **no id
    /// is reissued within one KEK generation.** The epoch reset is the ONLY
    /// reissue path, and it bumps the generation atomically with reseating the
    /// allocator (`super::epoch::AdmissionEpoch`). A production constructor here
    /// would let a caller mint an id the allocator already issued in this
    /// generation — the key-id collision R-35 exists to prevent — and no amount
    /// of documentation would stop it.
    ///
    /// `NonZeroU16` still excludes the reserved-invalid 0 even here.
    #[cfg(any(test, feature = "test-seams"))]
    pub fn from_nonzero(value: NonZeroU16) -> Self {
        Self(value)
    }
}

/// Outcome of a successful allocation.
#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    /// The freshly allocated id, never issued before in this KEK generation.
    pub sender_id: SenderId,
    /// True exactly once per allocator, on the allocation that first crosses
    /// the high-watermark. Edge-triggered so the caller logs once rather than on
    /// every admission past the line. Re-arms with each epoch's fresh allocator.
    pub high_watermark_crossed: bool,
}

/// Monotonic `sender_id` allocator for one KEK epoch of one meeting.
///
/// # The invariant (R-35, as refined by story 2 R-16)
///
/// **Within one KEK generation, a `sender_id` is bound to at most one identity
/// EVER** — not merely unique among concurrently-live members. That is the
/// stronger form (story 2 task 9, @paired-client), and it is the one receivers
/// depend on: they scope per-sender replay state by `(kek_generation,
/// sender_id)`, so an id handed to a second identity inside one generation
/// would land the new holder in the previous holder's replay bucket.
///
/// Within an epoch the cursor is **monotonic and never revisits**, so no id
/// released during the epoch is reissued before the next generation bump.
///
/// # Reissue happens only at an epoch reset — and never onto a bound id
///
/// Ids ARE reused across epochs: at exhaustion the meeting rotates the KEK and
/// this allocator is replaced by one seeded with an **exclusion snapshot** —
/// every id bound at that moment, live AND grace-period members. That reuse is
/// sound for exactly the reason reuse *within* a KEK is not: the wrap nonce
/// derives from the key id, so reusing an id under a **live** KEK collides two
/// senders on one AES-GCM nonce, but the reset installs a **new** KEK, so a
/// reissued id is never under the KEK its previous holder used.
///
/// **The snapshot is PERMANENT for the epoch. A liveness query in the
/// allocation path is the PROHIBITED implementation** (story 2 task 9, security
/// D-1). It is tempting to "reclaim" an excluded id once its holder leaves — do
/// not. A carried-over member that leaves mid-epoch does NOT advance the
/// generation (its rotation is debounced up to W), so reissuing its id then
/// would bind a second identity to that id under the SAME generation: the new
/// holder's transmit generation 0 would reuse the departed holder's key ids
/// under the same KEK — wrap-nonce reuse, not merely the receiver-side replay
/// blackout. This type therefore has no liveness input at all: it is handed a
/// snapshot once and never consults the roster again.
///
/// # Exhaustion is a signal, never a wrap
///
/// At the point the space would wrap, allocation fails with
/// [`SenderIdSpaceExhausted`]. There is no `wrapping_add`, no `%`, no `as`, and
/// no truncation anywhere in this type: a wrap would silently reissue a bound id
/// under a live KEK.
///
/// # What `issued()` and `remaining()` measure
///
/// **Cursor consumption, not admissions.** After an epoch reset the cursor
/// advances *past* excluded ids without issuing them, so `issued()` counts them.
/// That is the right quantity for namespace pressure — an excluded id genuinely
/// is not available — and it is what `mc_meeting_sender_ids_issued_max`
/// publishes. It is NOT comparable to a join count.
///
/// State is a cursor plus the epoch's exclusion snapshot, bounded by the roster
/// size at reset time — no per-participant map grows with churn.
#[derive(Debug)]
pub struct SenderIdAllocator {
    /// The next id to consider; `None` once the space is exhausted.
    next: Option<NonZeroU16>,
    /// Ids bound when this epoch began. Permanent for the epoch; see the type
    /// docs for why this is never re-evaluated against liveness.
    excluded: HashSet<SenderId>,
    /// Whether the one-shot high-watermark warning has already fired.
    watermark_warned: bool,
}

impl Default for SenderIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl SenderIdAllocator {
    /// A fresh allocator for a new meeting, starting at 1, excluding nothing.
    pub fn new() -> Self {
        Self {
            next: Some(NonZeroU16::MIN),
            excluded: HashSet::new(),
            watermark_warned: false,
        }
    }

    /// The allocator for a new KEK epoch: starts at 1 and permanently excludes
    /// `bound`, the ids bound at the moment of the reset.
    ///
    /// Only the epoch reset constructs this, and only alongside a new KEK — see
    /// `super::epoch::AdmissionEpoch`, which is what makes "reissue before
    /// rotation" unrepresentable. `pub(crate)` so nothing outside the crate can
    /// build a reissuing allocator without the key rotation.
    pub(crate) fn for_epoch(bound: HashSet<SenderId>) -> Self {
        Self {
            next: Some(NonZeroU16::MIN),
            excluded: bound,
            watermark_warned: false,
        }
    }

    /// Allocate the next id.
    ///
    /// # Errors
    ///
    /// [`SenderIdSpaceExhausted`] once no unexcluded id remains. The caller MUST
    /// NOT retry, wrap, or substitute here — the only recovery is a KEK-epoch
    /// reset, which replaces this allocator entirely.
    pub fn allocate(&mut self) -> Result<Allocation, SenderIdSpaceExhausted> {
        let id = loop {
            let candidate = self.next.ok_or(SenderIdSpaceExhausted)?;
            // `checked_add` is the wall: at 65535 this yields `None`, the
            // allocator latches exhausted, and every later call reports it.
            // Nothing wraps.
            self.next = candidate.get().checked_add(1).and_then(NonZeroU16::new);
            if !self.excluded.contains(&SenderId(candidate)) {
                break candidate;
            }
        };

        let crossed = !self.watermark_warned && self.is_past_high_watermark();
        if crossed {
            self.watermark_warned = true;
        }

        Ok(Allocation {
            sender_id: SenderId(id),
            high_watermark_crossed: crossed,
        })
    }

    /// How many ids the cursor has not yet passed. Cursor consumption, not
    /// admissions — see the type docs.
    pub fn remaining(&self) -> u32 {
        match self.next {
            // `u16::MAX` is the last issuable id, so the count still ahead
            // includes `next` itself.
            Some(next) => u32::from(u16::MAX) - u32::from(next.get()) + 1,
            None => 0,
        }
    }

    /// How far the cursor has advanced in this epoch, INCLUDING ids it skipped
    /// because they were excluded at the reset. Not a count of admissions.
    pub fn issued(&self) -> u32 {
        u32::from(u16::MAX) - self.remaining()
    }

    fn is_past_high_watermark(&self) -> bool {
        let total = u32::from(u16::MAX);
        self.issued() * 100 >= total * SENDER_ID_HIGH_WATERMARK_PERCENT
    }

    /// Resume from an arbitrary cursor with an arbitrary exclusion set.
    /// **Test seam.**
    ///
    /// Exists so the exhaustion path and the D-3 fail-closed path (a fresh
    /// epoch with nothing allocatable) can be exercised without performing
    /// 65535 joins. Compiled **only** under the non-default `test-seams` feature,
    /// and a release build with that feature on fails to compile (see `lib.rs`).
    ///
    /// This is a bypass of TWO guards: it seeds the cursor arbitrarily, and it
    /// sets the exclusion snapshot arbitrarily. It must never be reachable in
    /// production.
    #[cfg(feature = "test-seams")]
    pub fn resuming_from(next: Option<NonZeroU16>, excluded: HashSet<SenderId>) -> Self {
        Self {
            next,
            excluded,
            watermark_warned: false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn id(v: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(v).unwrap())
    }

    #[test]
    fn allocation_starts_at_one_and_is_monotonic() {
        let mut alloc = SenderIdAllocator::new();
        let first = alloc.allocate().unwrap().sender_id;
        assert_eq!(first.get().get(), 1, "zero is reserved-invalid");
        let mut previous = first;
        for _ in 0..100 {
            let next = alloc.allocate().unwrap().sender_id;
            assert!(next.get() > previous.get(), "ids must increase");
            previous = next;
        }
    }

    /// R-35 within one epoch: no id is ever issued twice, whatever happens to
    /// the participants that held them.
    #[test]
    fn no_id_is_ever_issued_twice_within_an_epoch() {
        let mut alloc = SenderIdAllocator::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(
                seen.insert(alloc.allocate().unwrap().sender_id),
                "an id was recycled within one epoch; this is the collision R-35 exists to prevent"
            );
        }
    }

    #[test]
    fn remaining_and_issued_track_allocation() {
        let mut alloc = SenderIdAllocator::new();
        assert_eq!(alloc.remaining(), 65535);
        assert_eq!(alloc.issued(), 0);
        alloc.allocate().unwrap();
        assert_eq!(alloc.remaining(), 65534);
        assert_eq!(alloc.issued(), 1);
    }

    /// A fresh epoch never issues an id bound at the reset.
    #[test]
    fn epoch_allocator_skips_every_excluded_id() {
        let bound: HashSet<SenderId> = [id(1), id(2), id(4)].into_iter().collect();
        let mut alloc = SenderIdAllocator::for_epoch(bound);
        assert_eq!(alloc.allocate().unwrap().sender_id, id(3));
        assert_eq!(alloc.allocate().unwrap().sender_id, id(5));
    }

    /// The strengthened invariant, stated as the discriminating case: the
    /// exclusion snapshot is PERMANENT for the epoch.
    ///
    /// A liveness-re-checked implementation would pass
    /// `epoch_allocator_skips_every_excluded_id` identically — it only diverges
    /// once an excluded holder LEAVES mid-epoch. This allocator has no liveness
    /// input by construction (it is handed a snapshot once), so there is no way
    /// to tell it a holder left; the test pins that the cursor still skips the
    /// id when it gets there. If someone adds a "release" method to reclaim an
    /// excluded id, this is where it has to be defeated.
    #[test]
    fn exclusion_is_permanent_for_the_epoch_even_after_the_holder_leaves() {
        // id 2 is a carried-over member at the reset. Imagine it leaves right
        // after: nothing reaches the allocator, and the cursor must still skip.
        let bound: HashSet<SenderId> = [id(2)].into_iter().collect();
        let mut alloc = SenderIdAllocator::for_epoch(bound);
        let issued: Vec<_> = (0..3)
            .map(|_| alloc.allocate().unwrap().sender_id)
            .collect();
        assert_eq!(
            issued,
            vec![id(1), id(3), id(4)],
            "an id excluded at the reset must never be reissued in that epoch, even once its \
             holder has left — reissuing it would bind a second identity to it under the SAME \
             KEK generation"
        );
    }

    /// `issued()` is cursor consumption: skipped ids count.
    #[test]
    fn issued_counts_cursor_consumption_including_skipped_ids() {
        let bound: HashSet<SenderId> = [id(1), id(2)].into_iter().collect();
        let mut alloc = SenderIdAllocator::for_epoch(bound);
        assert_eq!(alloc.allocate().unwrap().sender_id, id(3));
        assert_eq!(
            alloc.issued(),
            3,
            "one admission, but the cursor passed three ids; the gauge measures namespace \
             pressure, so the two skipped ids are correctly counted as consumed"
        );
    }

    /// The wall: the last id is issuable, and the next call reports exhaustion
    /// rather than wrapping back onto a bound id.
    #[cfg(feature = "test-seams")]
    #[test]
    fn exhaustion_is_reported_and_never_wraps() {
        let mut alloc = SenderIdAllocator::resuming_from(NonZeroU16::new(u16::MAX), HashSet::new());
        let last = alloc.allocate().unwrap().sender_id;
        assert_eq!(last.get().get(), u16::MAX);
        assert_eq!(alloc.remaining(), 0);
        assert_eq!(alloc.allocate().unwrap_err(), SenderIdSpaceExhausted);
        // Latched: it stays exhausted rather than recovering on a later call.
        assert_eq!(alloc.allocate().unwrap_err(), SenderIdSpaceExhausted);
    }

    /// D-3 at the allocator: an epoch whose exclusion set covers the rest of the
    /// space has nothing allocatable and fails closed rather than wrapping.
    #[cfg(feature = "test-seams")]
    #[test]
    fn excluded_tail_is_exhausted_not_wrapped() {
        let excluded: HashSet<SenderId> = (65533..=u16::MAX).map(id).collect();
        let mut alloc = SenderIdAllocator::resuming_from(NonZeroU16::new(65533), excluded);
        assert_eq!(alloc.allocate().unwrap_err(), SenderIdSpaceExhausted);
    }

    #[cfg(feature = "test-seams")]
    #[test]
    fn high_watermark_fires_exactly_once() {
        // 90% of 65535 is 58981.5, so the crossing is at the 58982nd issue.
        let mut alloc = SenderIdAllocator::resuming_from(NonZeroU16::new(58_980), HashSet::new());
        assert!(!alloc.allocate().unwrap().high_watermark_crossed);
        assert!(!alloc.allocate().unwrap().high_watermark_crossed);
        assert!(
            alloc.allocate().unwrap().high_watermark_crossed,
            "the crossing allocation reports it"
        );
        for _ in 0..50 {
            assert!(
                !alloc.allocate().unwrap().high_watermark_crossed,
                "edge-triggered: it must not re-fire on every later admission"
            );
        }
    }

    /// The watermark re-arms per allocator — i.e. per epoch — which is what
    /// makes it a leading indicator before EACH reset.
    #[test]
    fn a_fresh_epoch_allocator_re_arms_the_watermark() {
        assert!(!SenderIdAllocator::for_epoch(HashSet::new()).watermark_warned);
    }
}
