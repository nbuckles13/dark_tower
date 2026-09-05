//! The row inventory and the assembled document.
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! Every row here earns its place by proving something no other row proves.
//! Where a row is one of a deliberate *set* (the four AAD-span combinations,
//! the wrap-determinism pair), the `description` says which member it is and
//! why the set is not reducible.

use crate::error::GenError;
use crate::fixtures;
use crate::json::{
    ExtensionEntry, ExtensionRegistry, GatedBy, KeyIdLayout, RejectReasonEntry, Row, VectorFile,
    WireConstants,
};
use crate::kid::KeyIdParts;
use crate::rows::{build_frame, FrameSpec};
use crate::{CIPHER_SUITE_ID, CIPHER_SUITE_NAME, NON_PRODUCTION_BANNER, SCHEMA_VERSION};
use media_protocol::codec::ALL_REJECT_REASONS;
use media_protocol::extensions::{AcceptedValues, EXT_REGISTRY};
use media_protocol::frame::{
    AEAD_TAG_BYTES, EXT_LENGTH_FIELD_SIZE, KEK_GENERATION_FIELD_BYTES, KEY_ID_BYTES,
    KEY_ID_GENERATION_BITS, KEY_ID_SENDER_ID_BITS, KEY_ID_STREAM_BITS, LEGAL_FLAG_MASK,
    MAX_EXT_BYTES, MAX_PAYLOAD_BYTES, PROTOCOL_VERSION, PUBLISHER_FIXED_PREFIX_SIZE,
    RELAY_REGION_SIZE, SIGNATURE_SIZE, WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES,
    WRAPPED_TRANSMIT_KEY_SIZE,
};

/// Story-file path carrying the verbatim `wrap_key_id_mismatch` spelling.
const STORY_ANCHOR: &str = "docs/user-stories/2026-08-27-hear-yourself-through-handler.md";

fn u32_of(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// The complete reason vocabulary — the whole R-25 label space, not a subset.
///
/// `layer` and `has_vector` are **independent axes** and neither may be derived
/// from the other in either direction. `layer` answers *which subsystem owns the
/// remedy*; `has_vector` answers *can a byte string determine this outcome*.
/// Two rows demonstrate the independence in the data rather than in prose:
/// `unwrap_failed` is byte-determined with a key-distribution remedy, and
/// `no_transmit_key` is codec-layer while firing on key-store membership.
fn reject_reasons() -> Vec<RejectReasonEntry> {
    // Built through a helper taking `&str` rather than inline
    // `token: "...".to_string()` literals. Those literals are reject-REASON
    // vocabulary, not credentials, but `rust_secrets`'s
    // `secret_identifier_literal_assignment` rule cannot know that: it sees an
    // identifier named `token` assigned a string literal. Restructuring removes
    // the pattern; a `guard:ignore` would only suppress it, and a suppression on
    // a file that genuinely handles key material is a bad precedent to set.
    fn entry(
        name: &str,
        layer: &str,
        drops_frame: bool,
        has_vector: bool,
        fleet: Option<&str>,
        anchor: Option<&str>,
    ) -> RejectReasonEntry {
        RejectReasonEntry {
            token: name.to_string(),
            layer: layer.to_string(),
            drops_frame,
            has_vector,
            fleet_spelling: fleet.map(ToString::to_string),
            spec_anchor: anchor.map(ToString::to_string),
        }
    }

    let mut out: Vec<RejectReasonEntry> = ALL_REJECT_REASONS
        .iter()
        .map(|r| entry(r.as_str(), "codec", true, true, None, None))
        .collect();

    // Codec-family but receiver-state-dependent, so no byte string determines
    // it. ADR-0036 §4 places it "in the decode-reject bucket alongside unknown
    // flag bits" and is explicit that it is NOT a third key-material reason —
    // "not a third reason" has the preceding paragraph's pair as its antecedent.
    out.push(entry("no_transmit_key", "codec", true, false, None, None));

    // `signature_invalid` REUSES a live fleet spelling: it is a three-service
    // JWT `failure_reason` value today (gc-service/src/grpc/auth_layer.rs,
    // mc-service/src/grpc/auth_interceptor.rs, mh-service/src/grpc/
    // auth_interceptor.rs). Same predicate, different subject — "a JWT signature
    // failed" versus "an Ed25519 FRAME signature failed" — and one vocabulary
    // for one concept beats internal symmetry.
    out.push(entry(
        "signature_invalid",
        "crypto",
        true,
        true,
        Some("failure_reason"),
        None,
    ));
    out.push(entry("decrypt_failed", "crypto", true, true, None, None));
    out.push(entry("unwrap_failed", "crypto", true, true, None, None));
    out.push(entry("replay_detected", "crypto", true, true, None, None));
    // The ONLY drops_frame:false. §4 says the receiver IGNORES a mis-bound wrap
    // rather than dropping the frame; keeping a played frame out of the drop
    // counter is what preserves R-25's `received = played + sum(drops)`.
    out.push(entry(
        "wrap_key_id_mismatch",
        "crypto",
        false,
        true,
        None,
        Some(STORY_ANCHOR),
    ));

    out.push(entry(
        "no_kek_for_generation",
        "key",
        true,
        false,
        None,
        None,
    ));
    out.push(entry("no_roster_entry", "key", true, false, None, None));

    out
}

fn wire_constants() -> WireConstants {
    WireConstants {
        legal_flag_mask: LEGAL_FLAG_MASK,
        signature_bytes: u32_of(SIGNATURE_SIZE),
        aead_tag_bytes: u32_of(AEAD_TAG_BYTES),
        key_id_bytes: u32_of(KEY_ID_BYTES),
        relay_region_bytes: u32_of(RELAY_REGION_SIZE),
        publisher_fixed_prefix_bytes: u32_of(PUBLISHER_FIXED_PREFIX_SIZE),
        wrapped_transmit_key_bytes: u32_of(WRAPPED_TRANSMIT_KEY_SIZE),
        kek_generation_field_bytes: u32_of(KEK_GENERATION_FIELD_BYTES),
        wrapped_transmit_key_material_bytes: u32_of(WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES),
        ext_length_field_bytes: u32_of(EXT_LENGTH_FIELD_SIZE),
        max_ext_bytes: u32_of(MAX_EXT_BYTES),
        key_id_layout: KeyIdLayout {
            sender_id_bits: u32_of(KEY_ID_SENDER_ID_BITS),
            stream_bits: u32_of(KEY_ID_STREAM_BITS),
            generation_bits: u32_of(KEY_ID_GENERATION_BITS),
        },
    }
}

fn extension_registry() -> ExtensionRegistry {
    ExtensionRegistry {
        comment: "The rejection semantics travel with the table, not just the entries. \
                   A decoder could pin an identical registry and still SKIP unknown types, \
                   which is the ADR-0036 §2 defect behind a perfectly matching table."
            .to_string(),
        entries: EXT_REGISTRY
            .iter()
            .map(|s| {
                let AcceptedValues::ByteRange { min, max } = s.accepted;
                ExtensionEntry {
                    ext_type: s.ext_type,
                    value_len: u32_of(s.value_len),
                    accepted_min: min,
                    accepted_max: max,
                }
            })
            .collect(),
        unknown_type_is_rejected: true,
        duplicate_type_is_rejected: true,
        ascending_type_order_required: true,
        value_outside_accepted_set_is_rejected: true,
    }
}

/// The four associated-data-span combinations.
///
/// **Three of these never occur in loopback traffic**, and they are exactly
/// where two hand-written span computations can silently diverge: the span
/// varies with the key-bearing flag (50 bytes) and with the extension region
/// (2 + n bytes), so a codec that gets the conditional arithmetic wrong is
/// still correct on whichever combination it was developed against.
fn aad_span_rows() -> Result<Vec<Row>, GenError> {
    let combos = [
        ("aad_span_no_key_no_ext", false, None, "neither conditional region present: the minimum publisher region. Never appears in loopback traffic, where every audio frame is key-bearing"),
        ("aad_span_key_no_ext", true, None, "key-bearing only. This is the loopback audio shape, and therefore the ONLY combination a single-client demo exercises"),
        ("aad_span_no_key_with_ext", false, Some(42u8), "extensions only. Never appears in loopback traffic"),
        ("aad_span_key_with_ext", true, Some(100u8), "both conditional regions present, salience at its accepted maximum. Never appears in loopback traffic"),
    ];
    let mut out = Vec::new();
    for (name, key_bearing, salience, why) in combos {
        let mut spec = FrameSpec::base(
            name,
            Box::leak(
                format!(
                    "AAD-span combination ({}, {}): {why}. The span is \
                     PUBLISHER_FIXED_PREFIX + (key_bearing ? 50 : 0) + 2 + ext_len, so it varies \
                     with both conditionals; pinning all four is what makes a wrong conditional \
                     detectable rather than merely improbable.",
                    if key_bearing { "key-bearing" } else { "no key" },
                    if salience.is_some() {
                        "extensions"
                    } else {
                        "no extensions"
                    }
                )
                .into_boxed_str(),
            ),
            "roundtrip",
        );
        spec.key_bearing = key_bearing;
        spec.salience = salience;
        out.push(build_frame(&spec)?.row);
    }
    Ok(out)
}

/// The wrap-determinism pair: one transmit key wraps to one ciphertext.
fn wrap_determinism_rows() -> Result<Vec<Row>, GenError> {
    let mut out = Vec::new();
    for (name, seq) in [
        ("wrap_determinism_seq_lo", 100u32),
        ("wrap_determinism_seq_hi", 999u32),
    ] {
        let mut spec = FrameSpec::base(
            name,
            "Wrap-determinism pair. Same key id, different stream_sequence; the 50-byte \
             wrapped-key field must be BYTE-IDENTICAL across the two. This is the only pair \
             asserting ADR-0036 §4's 'one transmit key wraps to one ciphertext, byte-identical \
             from frame to frame within a generation' — the sentence that is the whole argument \
             for a derived rather than carried wrap nonce. Catches two self-consistent \
             single-language bugs: a randomised wrap nonce (unopenable by anyone, since the \
             nonce is not transmitted) and a nonce derived from stream_sequence (opens fine in \
             loopback, destroys the key-id-uniqueness reduction that §4's nonce safety rests on).",
            "roundtrip",
        );
        spec.stream_sequence = seq;
        out.push(build_frame(&spec)?.row);
    }
    Ok(out)
}

/// The two adversarial rows: wrap-binding and insider forgery.
///
/// Grouped because they are the pair that proves the *key-binding* and
/// *authorship* controls independently, and because a reader looking for
/// "where are the attack rows" should find one place rather than two points in
/// a long enumeration.
///
/// # Errors
///
/// Propagates [`GenError`] from row construction.
fn adversarial_rows() -> Result<Vec<Row>, GenError> {
    let mut out = Vec::new();
    // wrap_binding: a correctly self-signed frame carrying a wrap for a
    // DIFFERENT key id. Verification PASSES and the frame is otherwise
    // processed; the mis-bound wrap is IGNORED, not cached (§4).
    let mut wrap_spec = FrameSpec::base(
        "wrap_for_different_key_id",
        "Wrap-binding. A correctly self-signed frame whose wrapped transmit key is bound to a \
         DIFFERENT key id. The signature verifies, the frame is NOT dropped, and the payload \
         decrypts — but the receiver must IGNORE the mis-bound wrap rather than cache it (§4: \
         'receivers accept a wrapped key only for the key id of the frame carrying it'). \
         Asserted as CACHE STATE, never as a reject reason: a harness that asserts a rejection \
         here is testing the wrong control.",
        "wrap_binding",
    );
    wrap_spec.wrap_for_key_id = Some(KeyIdParts {
        sender_id: 0x0102,
        stream: 0x03,
        // Same sender and stream, next generation: the closest possible
        // mis-binding, so the row cannot pass by a coarse comparison.
        generation: 0x04_0506_0709,
    });
    wrap_spec.reject_reason = Some("wrap_key_id_mismatch");
    wrap_spec.expected = Some(serde_json::json!({
        "outcome": "wrap_key_id_mismatch",
        "signature_valid": true,
        "frame_dropped": false,
        "wrap_cached": false,
        "decrypts": true,
        "_assert": "Observe the receiver's key cache: key id 0x0102030405060709 must be ABSENT \
                    after processing. Do not assert a dropped frame or a reject reason.",
        // Declared in the SSoT rather than left to harness convention, mirroring
        // `replay_of`, so a codec in a third language cannot get this wrong by
        // omission. `decrypts: true` is only satisfiable if the receiver already
        // holds a usable transmit key for the frame's OWN key id 0x0102030405060708
        // — this frame's wrap is bound to 0x0102030405060709 and is ignored, so it
        // cannot supply that key.
        "receiver_precondition": {
            "cached_transmit_key_for_key_id": "0102030405060708",
            "primed_by": "full_frame_compose",
            "prime_via": "unwrap_only",
            "replay_window_advanced": false,
            "_assert": "Before processing this row, prime the receiver's transmit-key cache for key \
                        id 0x0102030405060708 by unwrapping full_frame_compose's wrapped-key block \
                        under its KEK — the UNWRAP PATH ONLY. Do NOT run full_frame_compose through \
                        the replay-checked receive path: it shares (key_id, stream_sequence) = \
                        (0x0102030405060708, 7) with this row, so processing it advances the replay \
                        window to seq 7 and this row is then rejected as replay_detected. Unwrap-only \
                        priming keeps the cached key originating from a genuine valid wrap (so \
                        decrypts:true stays self-protecting) while leaving the replay window \
                        unadvanced. The harness MUST fail loudly if this precondition is not \
                        established — a wrap-binding row that silently no-ops when unprimed is \
                        indistinguishable from a pass."
        }
    }));
    out.push(build_frame(&wrap_spec)?.row);

    // Insider forgery: Bob signs a frame sealed under Alice's transmit key and
    // carrying Alice's key id. Bob genuinely holds Alice's key (every member
    // can unwrap every transmit key under a shared KEK), so it WOULD decrypt if
    // verification were skipped. ADR-0036 Assumption 4.
    let mut forge_spec = FrameSpec::base(
        "insider_forgery_other_sender_key_id",
        "Insider forgery (ADR-0036 Assumption 4). Signed by Bob's identity key but sealed under \
         ALICE's transmit key and carrying Alice's key id. Under a shared meeting KEK every \
         member can unwrap every sender's transmit key, so Bob genuinely holds it and the frame \
         WOULD decrypt to valid plaintext if verification were skipped. Both harnesses must \
         assert `would_decrypt_if_verification_skipped` as well as the rejection — otherwise the \
         row proves the signature layer load-bearing only by assertion. Verification against \
         ALICE's roster key must FAIL.",
        "verify_reject",
    );
    forge_spec.identity_seed = fixtures::BOB_IDENTITY_SEED;
    forge_spec.reject_reason = Some("signature_invalid");
    // Alice's public key must be IN the row: `crypto.identity_public_hex` on a
    // forgery row is the FORGER's key (Bob's), because that is what signed the
    // frame. A harness told only to "verify against Alice" would have no Alice
    // to verify against, and the likely repair is verifying against the row's
    // own crypto block — which would SUCCEED and silently invert the test.
    let alice_public = hex::encode(
        crate::crypto::Identity::from_seed(&fixtures::ALICE_IDENTITY_SEED)?.public_key(),
    );
    forge_spec.expected = Some(serde_json::json!({
        "verify_against_public_hex": alice_public,
        "_verify_against_note": "This is ALICE's key — the sender the frame claims. \
                                 `crypto.identity_public_hex` on this row is BOB's, the forger's, \
                                 because that is what actually signed. Verifying against the row's \
                                 own crypto block would SUCCEED and invert the test.",
        "signed_by": "bob_identity_seed",
        "would_decrypt_if_verification_skipped": true,
        "_assert": "Assert BOTH: verification against Alice's key fails, AND decryption with the \
                    carried key id succeeds to the pinned plaintext. The second is what proves \
                    the signature layer is load-bearing rather than decorative."
    }));
    out.push(build_frame(&forge_spec)?.row);

    Ok(out)
}

/// The three `_`-prefixed commentary strings emitted at the top of the file.
///
/// Extracted from [`build`] so that adding a note to the published file does not
/// push the assembly function over `too_many_lines` — documentation growing
/// should not cost a refactor, and it is the kind of pressure that otherwise
/// gets relieved by shortening the note.
fn comment_kind() -> &'static str {
    "`kind` classifies the fixture row and is never a metric label value; \
             `reject_reason` is the label taxonomy."
}

fn comment_fixture_keys() -> &'static str {
    "Every key value is a synthetic, structured, low-entropy fixture (0x10+i, 0x20+i, ...), \
             chosen so a human reader sees at a glance it is not CSPRNG output. Nothing in this \
             file derives from any real environment or secret. `key_material_fields` and \
             `derived_public_fields` must PARTITION the `crypto` block exactly; guard check g13 \
             enforces that, because no dt-guard module scans .json and authorship discipline is \
             not a control."
}

fn comment_derived_on_reject_rows() -> &'static str {
    "On a row whose `frame_hex` does NOT decode, the `derived` and `decoded` blocks \
             describe the well-formed BASE frame the row was mutated from, not the bytes in \
             `frame_hex` — a frame that fails to decode has no publisher region and therefore no \
             meaningful AAD or signed range. Such rows carry \
             `expected.derived_describes == \"base_frame_before_mutation\"`. \
             DO NOT recompute those spans from `frame_hex` and do not compare them to it: on \
             those rows they differ by exactly the mutated byte, which is the point of the row. \
             Guard check g7 asserts byte equality between `aead_aad_hex` and \
             `frame_hex[0..publisher_region_len]` on every row that does NOT carry the marker, so \
             the exemption is declared per row rather than inferred from `kind`."
}

/// Assemble the document.
///
/// # Errors
///
/// Propagates [`GenError`] from any row.
pub fn build() -> Result<VectorFile, GenError> {
    let mut vectors = Vec::new();
    vectors.extend(aad_span_rows()?);

    // All-max key id: 0xFFFF / 0xFF / 2^40-1. Adjacent to the packer's
    // out-of-range unit assertions, so the boundary is pinned from both sides:
    // the largest accepted value round-trips, and one more is an error rather
    // than a silently aliased key id.
    let mut max_spec = FrameSpec::base(
        "key_id_all_max_roundtrip",
        "Every key-id field at its maximum: sender_id 0xFFFF, stream 0xFF, generation 2^40-1. \
         Proves the packed key id survives encode -> decode -> nonce derivation unchanged at \
         the top of every field. Paired with tests/kid_packer.rs, which asserts one more in \
         each field is an ERROR rather than a masked, colliding key id.",
        "roundtrip",
    );
    max_spec.key = KeyIdParts {
        sender_id: 0xFFFF,
        stream: 0xFF,
        generation: (1u64 << KEY_ID_GENERATION_BITS) - 1,
    };
    vectors.push(build_frame(&max_spec)?.row);

    vectors.extend(wrap_determinism_rows()?);

    vectors.extend(adversarial_rows()?);

    // Structural decode rejects, mutated from a known-good base at NAMED
    // offsets. The base carries extensions so the extension mutations have a
    // region to corrupt.
    let mut ext_base_spec = FrameSpec::base("decode_reject_base", "base", "roundtrip");
    ext_base_spec.salience = Some(50);
    let ext_base = build_frame(&ext_base_spec)?;
    vectors.extend(crate::mutate::decode_reject_rows(
        &ext_base.frame,
        &ext_base.row,
    )?);

    // Salience outside the registry's declared accepted set. §7 treats
    // publisher-supplied selection signals as untrusted, and the accepted range
    // is a trust boundary that had no cross-language vector before this row.
    vectors.push(crate::mutate::salience_out_of_range_row(
        &ext_base.frame,
        &ext_base.row,
    )?);

    // Full composed row, and the base for the tamper pair.
    let full_spec = FrameSpec::base(
        "full_frame_compose",
        "The composed receive path, asserted end to end: decode -> verify -> unwrap -> decrypt \
         -> plaintext. Every step uses only the wire bytes and the pinned crypto block, exactly \
         as a foreign implementation would. This is the row that fails if payload_length was \
         written AFTER the AAD was sliced — a construction-order defect that no single-language \
         round-trip can catch, because an encoder verifies against the AAD buffer it remembers.",
        "full_frame",
    );
    let full = build_frame(&full_spec)?;
    let mut full_row = full.row.clone();
    full_row.expected = Some(serde_json::json!({
        "decodes": true,
        "signature_valid": true,
        "unwraps": true,
        "decrypts": true,
        "plaintext_matches_crypto_block": true
    }));
    vectors.push(full_row);

    // Tamper pair. Both rejected, but by DIFFERENT layers reading different
    // spans: the publisher-region mutation is caught because those bytes are
    // signed AND are the associated data, the signature mutation because the
    // signature itself is checked. A codec that computed the signed range
    // wrongly could still pass one of these.
    vectors.extend(crate::mutate::tamper_rows(&full.frame, &full.row)?);

    // Crypto-layer rejects: wrong transmit key, and wrong KEK. Two AES-GCM
    // failures on one receive path that route to OPPOSITE teams.
    vectors.extend(crate::mutate::crypto_reject_rows(&full.frame, &full.row)?);

    // Replay: byte-identical to a frame already accepted. It carries a VALID
    // signature and a VALID tag — it was legitimately produced — so only the
    // receiver's sliding window per (sender, stream, generation) can reject it.
    vectors.push(crate::mutate::replay_row(&full.frame, &full.row));

    Ok(VectorFile {
        non_production: NON_PRODUCTION_BANNER.to_string(),
        comment_kind_is_not_a_metric_label: comment_kind().to_string(),
        comment_fixture_keys: comment_fixture_keys().to_string(),
        comment_derived_on_reject_rows: comment_derived_on_reject_rows().to_string(),
        schema_version: SCHEMA_VERSION,
        header_version: PROTOCOL_VERSION,
        cipher_suite_id: CIPHER_SUITE_ID,
        cipher_suite_name: CIPHER_SUITE_NAME.to_string(),
        max_payload_bytes: MAX_PAYLOAD_BYTES as u64,
        gated_by: GatedBy {
            rust: true,
            // Story task 15 landed the TypeScript codec and its conformance gate
            // (`packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts`):
            // every row is now checked from a second, independently-authored
            // implementation. The AAD span, the signed range and the detached-tag
            // split — which no external anchor gates — are cross-validated, so the
            // file may claim the green it now has. Guard g14 hard-fails on any drift
            // between this flag and reality, in either direction.
            typescript: true,
        },
        // Discharges ADR-0036 Assumption 4: the insider-forgery regression now runs
        // in the second implementation and rejects the frame rather than attributing
        // it, so the header is frozen in code. See the Assumption 4 annotation in
        // `docs/decisions/adr-0036-media-flow.md`.
        cross_language_property_established: true,
        wire_constants: wire_constants(),
        extension_registry: extension_registry(),
        reject_reasons: reject_reasons(),
        key_material_fields: vec![
            "kek_hex".to_string(),
            "transmit_key_hex".to_string(),
            "identity_private_seed_hex".to_string(),
        ],
        derived_public_fields: vec![
            "identity_public_hex".to_string(),
            "plaintext_hex".to_string(),
        ],
        vectors,
    })
}
