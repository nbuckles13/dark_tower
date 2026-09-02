//! Synthetic key material for the generated vectors.
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! # Every value here is structured and low-entropy, on purpose
//!
//! Each key is a run of consecutive bytes from a distinct base (`0x10 + i`,
//! `0x20 + i`, …), so a human reading the generated JSON sees at a glance that
//! it is **not CSPRNG output**. Nothing here derives from any real environment,
//! deployment or secret.
//!
//! That legibility is the control, and it is enforced rather than hoped for: no
//! `dt-guard` module scans `.json` — every credential module is extension-scoped
//! to `.rs`/`.ts`/`.tsx`/`.svelte` — so `no-hardcoded-secrets.sh` will never
//! look at the generated file. Guard check **g13** asserts the pattern instead,
//! and additionally that each `crypto` member is classified as key material or
//! as derived-public, so a field cannot escape the pattern check by being
//! quietly reclassified.

/// Length of an AES-256 key and of an Ed25519 seed.
pub const KEY_LEN: usize = 32;

/// A run of [`KEY_LEN`] consecutive bytes starting at `base`.
///
/// Deliberately not random-looking. `base` is unique per role so two fixtures
/// are never confusable in a diff.
///
/// # Why a macro rather than a `const fn` loop
///
/// The obvious `while i < KEY_LEN { out[i] = base.wrapping_add(i as u8) }`
/// needs **two** things this workspace forbids: an index into a slice
/// (ADR-0002 `indexing_slicing`) and a truncating `usize -> u8` cast. In a
/// `const fn` neither has an alternative — `get_mut` and `try_from` are not
/// const-callable here — so keeping the loop would have meant an `#[allow]` on
/// exactly the two lints this crate is least entitled to suppress. It landed a
/// **checked** key-id packer precisely because a masking cast aliases sender
/// 65536 to 0; shipping a truncating cast beside it would be indefensible.
///
/// Enumerating the offsets removes both rather than silencing them. The
/// consecutive-run property is then visible in the expansion instead of implied
/// by a loop, and `pattern_is_a_consecutive_run` below pins that the expansion
/// stays exactly [`KEY_LEN`] long — so the macro cannot drift from the constant
/// silently.
macro_rules! pattern {
    ($base:expr) => {
        [
            $base.wrapping_add(0),
            $base.wrapping_add(1),
            $base.wrapping_add(2),
            $base.wrapping_add(3),
            $base.wrapping_add(4),
            $base.wrapping_add(5),
            $base.wrapping_add(6),
            $base.wrapping_add(7),
            $base.wrapping_add(8),
            $base.wrapping_add(9),
            $base.wrapping_add(10),
            $base.wrapping_add(11),
            $base.wrapping_add(12),
            $base.wrapping_add(13),
            $base.wrapping_add(14),
            $base.wrapping_add(15),
            $base.wrapping_add(16),
            $base.wrapping_add(17),
            $base.wrapping_add(18),
            $base.wrapping_add(19),
            $base.wrapping_add(20),
            $base.wrapping_add(21),
            $base.wrapping_add(22),
            $base.wrapping_add(23),
            $base.wrapping_add(24),
            $base.wrapping_add(25),
            $base.wrapping_add(26),
            $base.wrapping_add(27),
            $base.wrapping_add(28),
            $base.wrapping_add(29),
            $base.wrapping_add(30),
            $base.wrapping_add(31),
        ]
    };
}

/// The meeting key-encryption key. MC-generated in production; fixed here.
pub const MEETING_KEK: [u8; KEY_LEN] = pattern!(0x10u8);
/// The second KEK, used only by the wrong-KEK unwrap-reject row.
pub const OTHER_KEK: [u8; KEY_LEN] = pattern!(0x18u8);
/// Alice's transmit key — the `SFrame` **base key**, not the derived AEAD key.
pub const ALICE_TRANSMIT_KEY: [u8; KEY_LEN] = pattern!(0x20u8);
/// Bob's transmit key.
pub const BOB_TRANSMIT_KEY: [u8; KEY_LEN] = pattern!(0x30u8);
/// Alice's Ed25519 seed. Test-only private material; see g13.
pub const ALICE_IDENTITY_SEED: [u8; KEY_LEN] = pattern!(0x40u8);
/// Bob's Ed25519 seed. Test-only private material; see g13.
pub const BOB_IDENTITY_SEED: [u8; KEY_LEN] = pattern!(0x50u8);

/// The KEK generation carried in every key-bearing fixture frame.
pub const KEK_GENERATION: u16 = 1;

/// Legible ASCII plaintext, so a reader can tell fixture from ciphertext on
/// sight without decoding anything.
pub const PLAINTEXT: &[u8] = b"dark-tower frame v2 vector plaintext";

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the two properties the macro asserts about itself: the expansion is
    /// exactly `KEY_LEN` long, and it really is a consecutive run.
    ///
    /// Without this, adding a byte to `KEY_LEN` would leave the macro emitting
    /// 32 entries and the array type would stop matching — a compile error, so
    /// that half is safe — but *removing* an offset from the macro while
    /// shortening `KEY_LEN` would compile and quietly produce a non-consecutive
    /// fixture that guard check g13's pattern predicate would then reject at a
    /// confusing distance from the cause.
    #[test]
    fn pattern_is_a_consecutive_run() {
        for base in [0x10u8, 0x18, 0x20, 0x30, 0x40, 0x50] {
            let run: [u8; KEY_LEN] = pattern!(base);
            assert_eq!(run.len(), KEY_LEN);
            for (offset, byte) in run.iter().enumerate() {
                let expected = base.wrapping_add(u8::try_from(offset).unwrap_or(0));
                assert_eq!(*byte, expected, "offset {offset} from base {base:#04x}");
            }
        }
    }
}
