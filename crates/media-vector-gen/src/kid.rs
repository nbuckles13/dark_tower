//! The `SFrame` key identifier: `sender_id(16) | stream(8) | generation(40)`.
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! # Why 16 bits of sender and 40 of generation
//!
//! Chosen over a 32-bit sender so that **generation is effectively
//! inexhaustible under one KEK**. A sender rotates its transmit key on every
//! video group and every `T` for audio (ADR-0036 §4), so generation is the fast
//! counter; at 2^40 it cannot be driven to wrap within any meeting lifetime.
//! That is why no generation-ceiling guard and no ceiling vector exist: the
//! ceiling is unreachable rather than unchecked.
//!
//! # Why every conversion is checked
//!
//! A masking pack aliases `sender_id` 65536 to 0. That is not a cosmetic
//! truncation: two senders would share a key id, hence a (key, nonce) pair,
//! and under AES-GCM a repeated pair leaks the authentication subkey and
//! permits forgery. A bare shift is worse still — it overflows into the
//! adjacent field silently. Both are refused: every component uses a checked
//! conversion sized to its field and returns `Err`.

use crate::error::GenError;
use media_protocol::frame::{
    KEY_ID_BYTES, KEY_ID_GENERATION_BITS, KEY_ID_SENDER_ID_BITS, KEY_ID_STREAM_BITS,
};

/// One past the largest representable `sender_id`.
pub const SENDER_ID_EXCLUSIVE_BOUND: u64 = 1 << KEY_ID_SENDER_ID_BITS;
/// One past the largest representable `stream`.
pub const STREAM_EXCLUSIVE_BOUND: u64 = 1 << KEY_ID_STREAM_BITS;
/// One past the largest representable `generation`.
pub const GENERATION_EXCLUSIVE_BOUND: u64 = 1 << KEY_ID_GENERATION_BITS;

/// The decomposed key id, before packing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyIdParts {
    /// Meeting-scoped sender identifier, `1..=65535`.
    pub sender_id: u64,
    /// Sender-scoped stream number.
    ///
    /// **Not** the relay `stream_id`: that is subscriber-scoped, two bytes wide,
    /// rewritten by a media handler and authenticated by nobody. This one is
    /// sender-scoped, one byte, and cryptographically bound via the key
    /// derivation.
    pub stream: u64,
    /// Monotonic per sender across its membership; never reset.
    pub generation: u64,
}

impl KeyIdParts {
    /// Pack into the 8-byte big-endian key id.
    ///
    /// # Errors
    ///
    /// [`GenError::SenderIdZero`] if `sender_id` is zero, and
    /// [`GenError::KeyIdFieldOverflow`] if any component exceeds its field
    /// width. Never masks, never shifts past a boundary.
    pub fn pack(self) -> Result<[u8; KEY_ID_BYTES], GenError> {
        if self.sender_id == 0 {
            return Err(GenError::SenderIdZero);
        }
        // Checked conversion sized to the field. `u16::try_from` is the
        // canonical form the ADR names; the other two have no narrower integer
        // type to convert into, so they are explicit bound checks against the
        // same named constants.
        let sender_id =
            u16::try_from(self.sender_id).map_err(|_| GenError::KeyIdFieldOverflow {
                field: "sender_id",
                value: self.sender_id,
                bits: KEY_ID_SENDER_ID_BITS,
                exclusive_bound: SENDER_ID_EXCLUSIVE_BOUND,
            })?;
        if self.stream >= STREAM_EXCLUSIVE_BOUND {
            return Err(GenError::KeyIdFieldOverflow {
                field: "stream",
                value: self.stream,
                bits: KEY_ID_STREAM_BITS,
                exclusive_bound: STREAM_EXCLUSIVE_BOUND,
            });
        }
        if self.generation >= GENERATION_EXCLUSIVE_BOUND {
            return Err(GenError::KeyIdFieldOverflow {
                field: "generation",
                value: self.generation,
                bits: KEY_ID_GENERATION_BITS,
                exclusive_bound: GENERATION_EXCLUSIVE_BOUND,
            });
        }
        let packed = (u64::from(sender_id) << (KEY_ID_STREAM_BITS + KEY_ID_GENERATION_BITS))
            | (self.stream << KEY_ID_GENERATION_BITS)
            | self.generation;
        Ok(packed.to_be_bytes())
    }
}
