//! The per-meeting key-encryption key (ADR-0036 §4).

use common::secret::{ExposeSecret, SecretBox};
use media_protocol::frame::KEK_GENERATION_FIELD_BYTES;
use ring::rand::{SecureRandom, SystemRandom};

/// Length of the meeting KEK in bytes. AES-256, so 32.
///
/// First production home for this value — no upstream anchor exists. Kept
/// honest by [`tests::meeting_kek_length_matches_aes_256_gcm`], which asserts
/// equality with `ring::aead::AES_256_GCM.key_len()`; `key_len()` is not `const`
/// in ring 0.17, so a test is the drift guard rather than a `const` assertion
/// (CLAUDE.md: "derive one from the other, or add a guard that fails validation
/// on drift").
///
/// **Not** `media_protocol::frame::WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES`, which
/// is also 32. That is a transmit key's length; this is the KEK's. Unrelated
/// values that agree today, same recorded boundary as `AEAD_TAG_BYTES` versus
/// AC's at-rest storage-envelope tag length — do not share a constant.
///
/// **Not** [`super::identity_key::IDENTITY_PUBLIC_KEY_BYTES`] either, which is
/// also 32 and sits in a sibling file in this module — the more reachable
/// accidental collapse of the two, because a reader sees two adjacent `= 32`
/// and hoists them to `mod.rs`. That one is an Ed25519 *public key* length and
/// this is an AES-256 *symmetric key* length: unrelated primitives agreeing by
/// the coincidence of both being 256-bit.
pub const MEETING_KEK_BYTES: usize = 32;

/// The KEK generation counter is a `u16`, tied to the frame header's field.
const _: () = {
    assert!(
        KEK_GENERATION_FIELD_BYTES * 8 == u16::BITS as usize,
        "kek_generation is carried in a fixed-width frame header field; if that field's width \
         changes, the Rust type carrying it must change with it or MC will silently truncate a \
         generation into the header"
    );
};

/// KEK generation could not be produced because the system CSPRNG failed.
///
/// Deliberately carries no detail and no source error: this value reaches a
/// join-failure path, and ADR-0002 forbids a fallback. There is no
/// partial-entropy, zeroed or default KEK — meeting-actor creation fails
/// instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KekGenerationFailed;

impl std::fmt::Display for KekGenerationFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("secure random generation failed")
    }
}

impl std::error::Error for KekGenerationFailed {}

/// A meeting's key-encryption key: AES-256, random, in memory only.
///
/// # Custody
///
/// Held by MC's per-meeting actor and by every client MC admits (via the join
/// response). **Never** by MH, never in any `dark_tower.internal.v1` message,
/// never on disk, never in a log, metric label, span attribute, or error/panic
/// payload. ADR-0036 §4: "MH stays keyless, and that is a guard, not a
/// convention."
///
/// # Why `SecretBox` rather than a hand-written redacting `Debug`
///
/// The derived `Debug` on this type renders
/// `MeetingKek(SecretBox<[u8; 32]>([REDACTED]))` because
/// [`SecretBox`]'s own `Debug` redacts. Redaction and zeroize-on-drop are
/// therefore properties of the type rather than of a hand-written impl a later
/// author could get wrong — structurally prevented, not reviewer-enforced.
/// Do not replace this with a manual `Debug`; it can only be weaker.
///
/// # Lifetime, and what its loss costs
///
/// Never derived, never persisted, and it **dies with the meeting actor**. A
/// database, a backup or a log yields nothing, and no master secret exists whose
/// loss reaches backward across meetings — which is why it is generated rather
/// than computed. The operational consequence: an MC restart or an ADR-0023
/// ownership move destroys it, so **every participant needs a fresh join to
/// obtain the new one**. This story ships no KEK-push message and no re-attach
/// delivery path; both land with KEK rotation in a later story.
#[derive(Debug)]
pub struct MeetingKek(SecretBox<[u8; MEETING_KEK_BYTES]>);

impl MeetingKek {
    /// Generate a fresh KEK from the system CSPRNG.
    ///
    /// # Errors
    ///
    /// [`KekGenerationFailed`] if `ring`'s [`SystemRandom`] cannot fill the
    /// buffer. **Fail closed**: the caller must abandon meeting creation. There
    /// is deliberately no fallback RNG and no default key — a predictable KEK
    /// would defeat every confidentiality property ADR-0036 §4 claims, and would
    /// do so silently.
    pub fn generate(rng: &SystemRandom) -> Result<Self, KekGenerationFailed> {
        let mut bytes = [0u8; MEETING_KEK_BYTES];
        rng.fill(&mut bytes).map_err(|_| KekGenerationFailed)?;
        Ok(Self(SecretBox::new(Box::new(bytes))))
    }

    /// Borrow the raw key bytes.
    ///
    /// The only route to the material, and every call site is a custody
    /// decision. Legitimate callers: the join-response builder, which copies it
    /// onto `JoinResponse.meeting_kek` for a participant MC has just admitted.
    /// Nothing else. Never pass the result to a log, a metric, a span, an error
    /// payload, or any MC→MH message.
    pub fn expose(&self) -> &[u8; MEETING_KEK_BYTES] {
        self.0.expose_secret()
    }
}

/// A meeting's current KEK together with its generation counter.
///
/// Generation starts at **0** and never advances in this story — KEK rotation,
/// and with it the KEK-epoch reset that would reclaim the `sender_id`
/// namespace, is deferred. `signaling.proto` states that 0 is a legal first
/// generation and that the not-provisioned signal is `meeting_kek` not being
/// exactly 32 bytes; **do not** gate on `kek_generation != 0`, which would give
/// one field two meanings.
///
/// The 65535 ceiling implied by `u16` here is **not** `SenderId`'s 65535. Same
/// magnitude, different concepts: this is a rotation counter, that is an
/// allocation bound with a never-recycled-while-a-KEK-is-live invariant. Do not
/// introduce a shared constant or a shared validation helper — collapsing them
/// silently loses the allocation invariant.
#[derive(Debug)]
pub struct MeetingKeyState {
    kek: std::sync::Arc<MeetingKek>,
    generation: u16,
}

impl MeetingKeyState {
    /// Generate the initial key state for a new meeting.
    ///
    /// # Errors
    ///
    /// [`KekGenerationFailed`] — see [`MeetingKek::generate`].
    pub fn generate(rng: &SystemRandom) -> Result<Self, KekGenerationFailed> {
        Ok(Self {
            kek: std::sync::Arc::new(MeetingKek::generate(rng)?),
            generation: 0,
        })
    }

    /// The current KEK.
    ///
    /// An `Arc` so the join path clones a handle rather than the key bytes:
    /// there is exactly one *long-lived* copy of the material, held here for the
    /// meeting's life and zeroized when the last handle drops.
    ///
    /// **Stated residual — this is not "exactly one copy in memory".** The one
    /// legitimate egress (`build_join_response`) calls `expose().to_vec()`, so
    /// each join also creates a transient plain `Vec<u8>` on the `JoinResponse`
    /// and a second copy inside prost's encode buffer. **Neither is zeroized on
    /// drop**; `SecretBox`'s zeroize property covers this handle and nothing
    /// downstream of `expose()`. Fixing that means a `Zeroizing` wrapper on a
    /// prost-generated field, which is a `proto-gen` change this story does not
    /// make. The custody claim that holds is: one long-lived copy, zeroized, and
    /// transient egress copies that live as long as one join response.
    pub fn kek(&self) -> &std::sync::Arc<MeetingKek> {
        &self.kek
    }

    /// The current KEK's generation.
    pub fn generation(&self) -> u16 {
        self.generation
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Drift guard for [`MEETING_KEK_BYTES`]. `ring`'s `key_len()` is not
    /// `const`, so this cannot be a `const` assertion.
    #[test]
    fn meeting_kek_length_matches_aes_256_gcm() {
        assert_eq!(
            MEETING_KEK_BYTES,
            ring::aead::AES_256_GCM.key_len(),
            "the meeting KEK is an AES-256 key; if these disagree, MC wraps under a key of the \
             wrong length"
        );
    }

    #[test]
    fn generated_kek_is_full_length_and_not_all_zero() {
        let rng = SystemRandom::new();
        let state = MeetingKeyState::generate(&rng).unwrap();
        let bytes = state.kek().expose();
        assert_eq!(bytes.len(), MEETING_KEK_BYTES);
        assert!(
            bytes.iter().any(|&b| b != 0),
            "an all-zero KEK is never a usable key; this asserts the RNG actually wrote"
        );
    }

    /// The KEK is per-meeting and random, not derived from anything shared.
    #[test]
    fn two_meetings_get_different_keks() {
        let rng = SystemRandom::new();
        let a = MeetingKeyState::generate(&rng).unwrap();
        let b = MeetingKeyState::generate(&rng).unwrap();
        assert_ne!(a.kek().expose(), b.kek().expose());
    }

    #[test]
    fn first_generation_is_zero() {
        let rng = SystemRandom::new();
        assert_eq!(MeetingKeyState::generate(&rng).unwrap().generation(), 0);
    }

    /// The redaction is the control, so it is asserted rather than assumed.
    ///
    /// Asserts against the two renderings a non-redacting `Debug` would actually
    /// produce, rather than per-byte. Per-byte fails in both directions: it was
    /// vacuous as originally written (the `contains("REDACTED")` disjunct is
    /// already asserted true one line above, so the loop could never fail), and
    /// simply dropping that disjunct would make it *flaky* — a lone hex pair is
    /// two characters, and `32`/`ec`/`ee` all occur in the struct's own type and
    /// field names, so `1 - (253/256)^32` ≈ **31%** of random keys would trip a
    /// false failure. The
    /// decimal check is the one that matters: a raw `[u8; 32]` renders as
    /// `[238, 23, ...]`, not as hex, so a hex-only assertion would miss exactly
    /// the regression this test exists to catch (someone replacing `SecretBox`
    /// with a bare array).
    #[test]
    fn debug_redacts_key_material() {
        let rng = SystemRandom::new();
        let state = MeetingKeyState::generate(&rng).unwrap();
        let rendered = format!("{state:?}");
        assert!(rendered.contains("REDACTED"), "got: {rendered}");

        let as_decimal_array = format!("{:?}", state.kek().expose());
        assert!(
            !rendered.contains(&as_decimal_array),
            "Debug rendered the key as a byte array"
        );

        // Any 4-byte contiguous run, in hex. Eight characters is far past
        // coincidence with the surrounding type and field names, so this still
        // catches a partial render that the whole-key check above would miss.
        for window in state.kek().expose().windows(4) {
            let hex: String = window.iter().map(|b| format!("{b:02x}")).collect();
            assert!(
                !rendered.contains(&hex),
                "Debug rendered key bytes: {hex} in {rendered}"
            );
        }
    }
}
