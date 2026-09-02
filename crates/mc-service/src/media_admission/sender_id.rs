//! Per-meeting `sender_id` allocation (ADR-0036 §2, §4; invariant R-35).

use media_protocol::frame::KEY_ID_SENDER_ID_BITS;
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
/// forensic warning.
///
/// **Not** an action threshold: the remedy at 90% and at 100% is identical
/// (end and restart the meeting), so acting early only disrupts a meeting that
/// still works. It exists so that after an exhaustion incident an operator can
/// reconstruct whether consumption was sudden or gradual. Deliberately a
/// compile-time constant rather than configuration — it selects no behaviour.
const SENDER_ID_HIGH_WATERMARK_PERCENT: u32 = 90;

/// The 16-bit `sender_id` space is exhausted for this meeting.
///
/// Terminal for the meeting: this story ships no KEK-epoch reset, so there is
/// no in-place operator remedy. The meeting must end and restart.
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
    /// the R-35 never-recycled invariant a property of the type system rather
    /// than of reviewer vigilance. A production constructor would let a caller
    /// mint an id the allocator has already issued — the exact key-id collision
    /// R-35 exists to prevent — and no amount of documentation would stop it.
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
    /// The freshly allocated, never-before-issued id.
    pub sender_id: SenderId,
    /// True exactly once, on the allocation that first crosses the
    /// high-watermark. Edge-triggered so the caller logs once rather than on
    /// every admission past the line.
    pub high_watermark_crossed: bool,
}

/// Monotonic, non-recycling `sender_id` allocator for one meeting.
///
/// # The invariant (R-35)
///
/// A key id is never reused under one KEK. So ids are allocated **monotonically
/// and never recycled**: a departed participant's id is never reissued to a
/// later joiner. Because ids are never recycled the namespace is consumed by
/// **cumulative lifetime admissions**, not by concurrent participants — the
/// per-instance participant cap does not bound it, and a long-lived, high-churn
/// meeting is the exposure.
///
/// **Reconnect continuity is not recycling and does not involve this type.** A
/// reconnecting participant keeps the id it already holds because the
/// participant is still on the roster; the allocator is not consulted. The
/// continuity key is the ADR-0023 `correlation_id` plus its HMAC
/// `binding_token`, never a client-supplied participant id. A reconnect *after*
/// the participant was evicted is a fresh join and correctly receives a **new**
/// id — do not "fix" that into recycling.
///
/// # Exhaustion is a reject, never a wrap
///
/// At the point the space would wrap, allocation **fails and the admission is
/// refused**. A warn-and-continue that still wrapped would silently reissue a
/// live `sender_id` and produce exactly the key-id collision R-35 exists to
/// prevent. There is no `wrapping_add`, no `%`, no `as`, and no truncation
/// anywhere in this type. The KEK-epoch reset that would reclaim the namespace
/// is deferred with all KEK rotation.
///
/// State is a single cursor — no per-participant map and no retained entries for
/// departed participants, so a meeting's allocator memory is O(1) regardless of
/// churn.
#[derive(Debug)]
pub struct SenderIdAllocator {
    /// The next id to issue; `None` once the space is exhausted.
    next: Option<NonZeroU16>,
    /// Whether the one-shot high-watermark warning has already fired.
    watermark_warned: bool,
}

impl Default for SenderIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl SenderIdAllocator {
    /// A fresh allocator for a new meeting, starting at 1.
    pub fn new() -> Self {
        Self {
            next: Some(NonZeroU16::MIN),
            watermark_warned: false,
        }
    }

    /// Allocate the next id.
    ///
    /// # Errors
    ///
    /// [`SenderIdSpaceExhausted`] once all 65535 ids have been issued. The
    /// caller MUST refuse the admission; it must not retry, wrap, or substitute.
    pub fn allocate(&mut self) -> Result<Allocation, SenderIdSpaceExhausted> {
        let id = self.next.ok_or(SenderIdSpaceExhausted)?;

        // `checked_add` is the wall: at 65535 this yields `None`, the allocator
        // latches exhausted, and every later call rejects. Nothing wraps.
        self.next = id.get().checked_add(1).and_then(NonZeroU16::new);

        let crossed = !self.watermark_warned && self.is_past_high_watermark();
        if crossed {
            self.watermark_warned = true;
        }

        Ok(Allocation {
            sender_id: SenderId(id),
            high_watermark_crossed: crossed,
        })
    }

    /// How many ids remain unissued.
    pub fn remaining(&self) -> u32 {
        match self.next {
            // `u16::MAX` is the last issuable id, so the count still available
            // includes `next` itself.
            Some(next) => u32::from(u16::MAX) - u32::from(next.get()) + 1,
            None => 0,
        }
    }

    /// How many ids have been issued over the meeting's life.
    pub fn issued(&self) -> u32 {
        u32::from(u16::MAX) - self.remaining()
    }

    fn is_past_high_watermark(&self) -> bool {
        let total = u32::from(u16::MAX);
        self.issued() * 100 >= total * SENDER_ID_HIGH_WATERMARK_PERCENT
    }

    /// Resume from an arbitrary cursor. **Test seam.**
    ///
    /// Exists only so the exhaustion reject can be exercised end-to-end without
    /// performing 65535 joins. It is compiled **only** under the non-default
    /// `test-seams` feature, and a release build with that feature on fails to
    /// compile (see `lib.rs`) — a control that has to be noticed fails silently
    /// for anyone building outside the pipeline.
    ///
    /// This is the exhaustion guard's bypass: it lets a caller seed the cursor
    /// arbitrarily. It must never be reachable in production.
    #[cfg(feature = "test-seams")]
    pub fn resuming_from(next: Option<NonZeroU16>) -> Self {
        Self {
            next,
            watermark_warned: false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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

    /// The R-35 property stated as a test: no id is ever issued twice, whatever
    /// happens to the participants that held them.
    #[test]
    fn no_id_is_ever_issued_twice() {
        let mut alloc = SenderIdAllocator::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(
                seen.insert(alloc.allocate().unwrap().sender_id),
                "an id was recycled; this is the collision R-35 exists to prevent"
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

    /// The wall: the last id is issuable, and the next call rejects rather than
    /// wrapping back onto a live id.
    #[cfg(feature = "test-seams")]
    #[test]
    fn exhaustion_rejects_and_never_wraps() {
        let mut alloc = SenderIdAllocator::resuming_from(NonZeroU16::new(u16::MAX));
        let last = alloc.allocate().unwrap().sender_id;
        assert_eq!(last.get().get(), u16::MAX);
        assert_eq!(alloc.remaining(), 0);
        assert_eq!(alloc.allocate().unwrap_err(), SenderIdSpaceExhausted);
        // Latched: it stays refused rather than recovering on a later attempt.
        assert_eq!(alloc.allocate().unwrap_err(), SenderIdSpaceExhausted);
    }

    #[cfg(feature = "test-seams")]
    #[test]
    fn high_watermark_fires_exactly_once() {
        // 90% of 65535 is 58981.5, so the crossing is at the 58982nd issue.
        let mut alloc = SenderIdAllocator::resuming_from(NonZeroU16::new(58_980));
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
}
