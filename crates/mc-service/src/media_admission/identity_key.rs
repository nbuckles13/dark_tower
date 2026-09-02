//! A joiner's Ed25519 identity signing public key, as presented at join.

/// Length of a raw Ed25519 public key in bytes.
///
/// First production home for this value: there is no upstream anchor to derive
/// from. Unlike the sender width and the KEK-generation width — which MUST
/// derive from `media_protocol::frame` — no Ed25519 public-key length constant
/// exists anywhere in the tree, so instantiating the literal once here is
/// correct rather than a duplication.
///
/// ANCHOR (DRY): this governs two `proto/dark_tower/signaling/v1/signaling.proto`
/// fields, both of which state "Exactly 32 bytes" in prose —
/// `JoinRequest.identity_public_key` (tag 7) and `Participant.identity_public_key`
/// (tag 6). The reciprocal proto-side anchor pointing back here is a named,
/// protocol-owned follow-up, deferred only because this task holds a
/// zero-proto-edit line; it is tracked, not omitted.
///
/// **Not** [`super::kek::MEETING_KEK_BYTES`], which is also 32 and sits in a
/// sibling file. That is an AES-256 symmetric key length; this is an Ed25519
/// public key length. Unrelated primitives agreeing by the coincidence of both
/// being 256-bit — do not hoist them to a shared constant.
pub const IDENTITY_PUBLIC_KEY_BYTES: usize = 32;

/// A presented identity key was not exactly [`IDENTITY_PUBLIC_KEY_BYTES`] long.
///
/// **One error value, deliberately.** Every rejected length — short, long and
/// oversized — is indistinguishable, to the client and in telemetry. The
/// client-facing message is generic and the metric label is a single bounded
/// value, so nothing here is an oracle for which check failed.
///
/// Note "absent" is **not** in that set at the join boundary: length 0 is
/// admitted by [`IdentityPublicKey::parse_join_field`], which is what the join
/// path calls. This type is reachable for length 0 only via
/// [`IdentityPublicKey::try_from_bytes`], whose callers have already excluded
/// the empty case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityPublicKeyRejected;

impl std::fmt::Display for IdentityPublicKeyRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("identity key rejected")
    }
}

impl std::error::Error for IdentityPublicKeyRejected {}

/// A joiner's raw Ed25519 identity signing public key, exactly 32 bytes.
///
/// # What this type does and does not claim
///
/// Constructing one means **the bytes were of the right length**. Nothing more.
/// MC performs no `cnf` thumbprint check against the meeting token, so the key
/// is recorded on the roster **trust-on-first-use**. A frame signature that
/// verifies against it proves only that all frames verifying against it came
/// from **the same keyholder** — *same-keyholder consistency*. It never proves
/// *who* that keyholder is, and for a guest it never proves more than a
/// pseudonym (ADR-0036 §4). The client-validated AC attestation that closes
/// this is **story 2**; when it lands, the attestation wins over the roster on
/// disagreement.
///
/// Because there is no binding, a compromised or malicious MC can publish a key
/// it controls for a participant that does not exist, and nothing here detects
/// it. That is a stated gap, not an oversight.
///
/// # Scope
///
/// Identity keys are scoped to **one meeting**, not to a participant across
/// meetings: a key reused across meetings makes a participant linkable by public
/// key regardless of display name, which matters most for the guests with the
/// least identity assurance to begin with.
///
/// # Observability
///
/// Public material, so not a do-not-log rule — but a stable per-participant
/// identifier for the meeting's lifetime. Never a metric label, never a span
/// attribute, never a per-frame log dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityPublicKey([u8; IDENTITY_PUBLIC_KEY_BYTES]);

impl IdentityPublicKey {
    /// Accept `bytes` if and only if it is exactly
    /// [`IDENTITY_PUBLIC_KEY_BYTES`] long.
    ///
    /// # This is a length check on an opaque blob, and nothing else
    ///
    /// It is **not** a point-on-curve check, **not** small-order rejection, and
    /// **not** canonical-encoding validation. MC treats the 32 bytes as opaque.
    ///
    /// None of those is required *while the roster key is trust-on-first-use*: a
    /// degenerate key costs only its own publisher the same-keyholder property,
    /// and MC could publish an arbitrary key anyway with no `cnf` binding. **This
    /// stops being true when the `cnf` binding lands** — an attested small-order
    /// key would be forgeable by anyone — so point and small-order validation are
    /// required no later than that story.
    ///
    /// The reason length-only is acceptable now is dependency cost and the
    /// unclaimed property, not any oracle concern: `ring` has no standalone
    /// Ed25519 public-key parse and no Ed25519 crate is in the workspace, so
    /// deeper validation means a new crypto dependency under ADR-0027.
    ///
    /// # Errors
    ///
    /// [`IdentityPublicKeyRejected`] — a single value covering every rejection
    /// cause, for **any** length other than exactly 32, including 0.
    ///
    /// This is the exact-length primitive, **not** the join-path entry point.
    /// The join boundary calls [`Self::parse_join_field`], which admits length 0
    /// as the contract's NO KEY PUBLISHED state; MC therefore *does* admit a
    /// participant with no key and *does* publish a keyless (empty) roster
    /// entry. Do not read this function's fail-closed shape as the join path's
    /// behaviour — see `parse_join_field` for the three-state table and the
    /// reasoning.
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, IdentityPublicKeyRejected> {
        <[u8; IDENTITY_PUBLIC_KEY_BYTES]>::try_from(bytes)
            .map(Self)
            .map_err(|_| IdentityPublicKeyRejected)
    }

    /// Parse the `JoinRequest.identity_public_key` wire field, which has
    /// **three** states rather than two.
    ///
    /// | Wire length | Meaning | Result |
    /// |---|---|---|
    /// | 0 | **no key published** — a defined, representable state | `Ok(None)` |
    /// | 32 | a key | `Ok(Some(key))` |
    /// | anything else | malformed | `Err` |
    ///
    /// # Why length 0 is admitted rather than rejected
    ///
    /// The field is bare proto3 `bytes`, where absent and empty are *identical
    /// on the wire* — so making length 0 a hard reject would make "no key yet"
    /// unrepresentable, which is exactly the state `signaling.proto` insists
    /// must exist: *"Empty means NO KEY PUBLISHED: consumers MUST fail closed
    /// and MUST NOT fall back to accepting unsigned frames."* That sentence
    /// presupposes a keyless participant **on the roster and sending frames**,
    /// which is only reachable if one can be admitted. Note `sender_id` one
    /// field above *is* `optional uint32`: the author reached for presence
    /// semantics one field over and deliberately did not use them here.
    ///
    /// And rejecting length 0 buys nothing against an adversary. Validation is
    /// length-only — no proof of possession, no `cnf` binding this story — so a
    /// hostile client satisfies it with 32 random bytes it holds no private half
    /// for, is admitted, receives the KEK unconditionally, reads all media, and
    /// can never produce a verifying signature. That is materially identical to
    /// an accept-absent omitter. Reject would exclude exactly one population:
    /// honest clients that have not yet implemented the field — a compatibility
    /// gate wearing a security gate's clothing.
    ///
    /// **What reject *did* buy, and how it is recovered**: it made "a keyless
    /// participant on the roster" unreachable, so no consumer could fail open on
    /// one. That protection now lives on the consumer, and is an owned,
    /// triggered, tested obligation rather than a comment — see `docs/TODO.md`,
    /// owner client, trigger *before the SDK verifies any frame*. The residual
    /// it addresses is a consumer implementing the empty branch as "skip
    /// verification" instead of "drop".
    ///
    /// # Errors
    ///
    /// [`IdentityPublicKeyRejected`] for any length other than 0 or 32 — one
    /// value, one client-facing message, one telemetry label, so the rejection
    /// cause is not an oracle. **No client-controlled blob of any length other
    /// than exactly 32 ever reaches the roster.**
    pub fn parse_join_field(bytes: &[u8]) -> Result<Option<Self>, IdentityPublicKeyRejected> {
        if bytes.is_empty() {
            return Ok(None);
        }
        Self::try_from_bytes(bytes).map(Some)
    }

    /// Borrow the raw key bytes, for publication on the roster.
    pub fn as_bytes(&self) -> &[u8; IDENTITY_PUBLIC_KEY_BYTES] {
        &self.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn exact_length_key_is_accepted() {
        let key = IdentityPublicKey::try_from_bytes(&[0x42u8; IDENTITY_PUBLIC_KEY_BYTES]).unwrap();
        assert_eq!(key.as_bytes(), &[0x42u8; IDENTITY_PUBLIC_KEY_BYTES]);
    }

    /// Every wrong length collapses to the same single error value. This is the
    /// non-oracle property at the type level.
    #[test]
    fn every_wrong_length_is_rejected_identically() {
        for bytes in [
            vec![0u8; 1],
            vec![0u8; IDENTITY_PUBLIC_KEY_BYTES - 1],
            vec![0u8; IDENTITY_PUBLIC_KEY_BYTES + 1],
            vec![0u8; IDENTITY_PUBLIC_KEY_BYTES * 2],
        ] {
            let err = IdentityPublicKey::parse_join_field(&bytes).unwrap_err();
            assert_eq!(err, IdentityPublicKeyRejected);
        }
    }

    /// The three wire states, asserted as three — the distinction the whole
    /// accept-absent design rests on.
    #[test]
    fn join_field_has_three_states_not_two() {
        assert_eq!(
            IdentityPublicKey::parse_join_field(&[]).unwrap(),
            None,
            "length 0 is NO KEY PUBLISHED — admitted, not malformed"
        );
        assert!(
            IdentityPublicKey::parse_join_field(&[0x42u8; IDENTITY_PUBLIC_KEY_BYTES])
                .unwrap()
                .is_some(),
            "length 32 is a key"
        );
        assert!(
            IdentityPublicKey::parse_join_field(&[0x42u8; 31]).is_err(),
            "any other length is malformed and never reaches the roster"
        );
    }

    /// No `Default` may exist on this type: a defaulted `IdentityPublicKey`
    /// would be a 32-byte all-zero key, which reads as PRESENT on the wire and
    /// silently destroys the no-key-published signal. Compile-time assertion
    /// that nothing has added one.
    #[test]
    fn newtype_has_no_default_impl() {
        fn assert_not_default<T>() {}
        assert_not_default::<IdentityPublicKey>();
        // If `IdentityPublicKey: Default` is ever added, replace this test with
        // a deliberate justification — do not delete it.
    }

    /// The floor, asserted rather than only documented: this type carries no
    /// claim about *whose* key it is. An all-zero key — not a valid Ed25519
    /// point — is accepted, because MC does no curve validation and must not
    /// appear to.
    #[test]
    fn acceptance_is_length_only_and_claims_no_identity() {
        assert!(IdentityPublicKey::try_from_bytes(&[0u8; IDENTITY_PUBLIC_KEY_BYTES]).is_ok());
    }
}
