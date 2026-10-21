//! Reject rows, built by mutating a valid frame.
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! # Why these are derived from a valid frame rather than authored
//!
//! Each reject row is one byte (or one field) away from a frame that decodes,
//! verifies and decrypts. That is the point: a hand-authored malformed blob can
//! fail for reasons the author did not intend, and then the row asserts a
//! rejection that would have happened anyway. Mutating a known-good frame at a
//! **named offset** — never a literal position — means the row isolates exactly
//! the condition it claims to.
//!
//! Offsets come from the decoded view's range accessors, so a layout change
//! moves them rather than silently pointing the mutation somewhere else.

use crate::error::GenError;
use crate::json::Offsets;
use crate::json::Row;
use media_protocol::codec::decode_datagram;
use media_protocol::frame::{
    FLAGS_OFFSET, FLAG_DISCARDABLE, GOP_COUNTER_OFFSET, PAYLOAD_LENGTH_OFFSET, PROTOCOL_VERSION,
    VERSION_OFFSET,
};

/// Apply a mutation to a built row, re-deriving the offsets that remain
/// meaningful and recording the reject reason.
///
/// The `derived` and `crypto` blocks are carried over unchanged from the base
/// frame: for a reject row they document **what the frame would have been**, so
/// a harness can confirm the row is a near-miss rather than noise.
/// Apply a mutation to a built row, recording the reject reason.
///
/// # The decoded and derived blocks describe the MUTATED bytes when they decode
///
/// A row whose `decoded` block described the *pre*-mutation frame would be
/// actively misleading: a harness comparing the decoded view against it would
/// fail on the tamper rows for the wrong reason — reporting a field mismatch
/// where the row's actual claim is that the **signature** fails. So when the
/// mutated frame still decodes (every `verify_reject`, `decrypt_reject` and
/// `replay_reject` row), the offsets, decoded fields and the three span
/// derivations are re-derived from the mutated bytes. Spans are a property of
/// the bytes; the failure these rows assert is a layer above.
///
/// When the frame does **not** decode (`decode_reject` rows) the base blocks are
/// carried over unchanged, where they document what the frame would have been —
/// which is what lets a reader confirm the row is a near-miss rather than noise.
/// `crypto` is always carried over: it is the sender's material, unaffected by
/// any mutation.
fn mutated(
    base: &Row,
    frame: &[u8],
    name: &'static str,
    description: &'static str,
    kind: &'static str,
    reason: &'static str,
    expected: serde_json::Value,
) -> Row {
    mutated_with(base, frame, name, description, kind, Some(reason), expected)
}

/// [`mutated`], for a row that may carry no reject reason (the relay-rewrite
/// row, which is accepted).
fn mutated_with(
    base: &Row,
    frame: &[u8],
    name: &'static str,
    description: &'static str,
    kind: &'static str,
    reason: Option<&'static str>,
    expected: serde_json::Value,
) -> Row {
    let mut offsets = base.offsets.clone();
    let mut decoded = base.decoded.clone();
    let mut derived = base.derived.clone();

    if let Ok(view) = decode_datagram(frame) {
        let payload_range = view.payload_range();
        offsets = Offsets {
            publisher_region_len: narrow(view.publisher_region().len()),
            relay_region_offset: narrow(view.relay_region_offset()),
            payload_offset: narrow(payload_range.start),
            signature_offset: narrow(view.signature_range().start),
        };
        decoded.payload_length = narrow(view.payload_length());
        decoded.stream_sequence = view.stream_sequence();
        decoded.gop_counter = view.gop_counter();
        decoded.stream_id = view.stream_id();
        decoded.hop_sequence = view.hop_sequence();

        let [pubreg, payload] = view.signed_ranges();
        let mut signed = Vec::with_capacity(pubreg.len() + payload.len());
        signed.extend_from_slice(pubreg);
        signed.extend_from_slice(payload);
        derived.aead_aad_hex = hex::encode(pubreg);
        derived.signed_input_hex = hex::encode(&signed);
        derived.naive_contiguous_signed_input_hex =
            hex::encode(prefix(frame, payload_range.end).unwrap_or_default());
        derived.signature_hex = hex::encode(view.signature());

        // **Every frame-dependent derived value, not only the spans.**
        //
        // The SFrame nonce is `salt XOR BE12(counter)` and the counter IS
        // `stream_sequence`, so a mutation inside that field changes the nonce.
        // `tamper_publisher_region` flips a bit at offset 6 — inside
        // `stream_sequence` — and an earlier version left `sframe_nonce_hex` at
        // its pre-mutation value, so the row pinned a nonce its own frame's
        // counter does not produce. A TypeScript harness computing the nonce
        // from `frame.stream_sequence`, which is the only sane thing to do,
        // would have disagreed with the pinned value.
        //
        // That matters more here than anywhere else in the file: ADR-0036 §2's
        // entire argument for `stream_sequence` immutability is that rewriting
        // it breaks decryption. A vector asserting a nonce inconsistent with its
        // own counter, in the artifact that exists so two languages derive
        // identical values from identical bytes, is the one place a silent
        // convention must not live.
        //
        // `sframe_key_hex` and `sframe_salt_hex` need no recomputation: both
        // derive from the key id, which no mutation here touches. The payload
        // ciphertext and tag likewise stay valid — these mutations are all in
        // the header, so the payload bytes are untouched.
        if let Ok(salt) = hex::decode(&derived.sframe_salt_hex) {
            let mut nonce = salt;
            let ctr = u64::from(view.stream_sequence()).to_be_bytes();
            let start = nonce.len().saturating_sub(ctr.len());
            for (i, c) in ctr.iter().enumerate() {
                if let Some(slot) = nonce.get_mut(start + i) {
                    *slot ^= *c;
                }
            }
            derived.sframe_nonce_hex = hex::encode(nonce);
        }
    }

    // If the mutated frame does NOT decode, the loop above could not re-derive
    // anything and `derived` still describes the BASE frame. Declare that per
    // row rather than leaving a consumer to infer it from `kind`: a byte string
    // sitting next to a `derived` block that does not describe it is exactly the
    // silent disagreement this file exists to prevent, and guard check g7
    // requires this marker on any row where the two diverge.
    let mut expected = expected;
    if decode_datagram(frame).is_err() {
        if let Some(obj) = expected.as_object_mut() {
            obj.insert(
                "derived_describes".to_string(),
                serde_json::Value::String("base_frame_before_mutation".to_string()),
            );
            obj.insert(
                "_derived_note".to_string(),
                serde_json::Value::String(
                    "This frame does not decode, so it has no publisher region and no                      meaningful AAD. `derived` and `decoded` describe the well-formed base                      frame this row was mutated from, NOT the bytes in `frame_hex`. Do not                      recompute them from `frame_hex` and do not compare them to it."
                        .to_string(),
                ),
            );
        }
    }

    Row {
        name: name.to_string(),
        description: description.to_string(),
        kind: kind.to_string(),
        frame_hex: hex::encode(frame),
        offsets,
        decoded,
        derived,
        crypto: base.crypto.clone(),
        reject_reason: reason.map(ToString::to_string),
        expected: Some(expected),
    }
}

fn narrow(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------------------
// Checked mutation primitives
// ---------------------------------------------------------------------------
//
// Every mutation below goes through these rather than indexing directly. Two
// reasons, and the second is the substantive one.
//
// ADR-0002 forbids `indexing_slicing` workspace-wide. But the rule earns its
// keep here specifically: this crate's whole job is computing byte spans, so a
// panic *inside* span arithmetic is the failure mode it exists to detect, not
// one it should exhibit. Every offset below is derived from a decoded header —
// `ext_length_field_range`, `signature_range`, `relay_region_offset` — which is
// exactly the attacker-influenced quantity the codec treats as a parsing trust
// boundary. Indexing them would put an unchecked read on the far side of a
// boundary the codec was built to hold.
//
// The practical payoff is that a layout change producing an out-of-range offset
// surfaces as a named error naming the offset and the frame length, instead of
// a panic with a backtrace into `core::slice::index`.

/// Overwrite one byte.
fn set_byte(frame: &mut [u8], at: usize, value: u8) -> Result<(), GenError> {
    let len = frame.len();
    *frame
        .get_mut(at)
        .ok_or_else(|| oob("set_byte", at, 1, len))? = value;
    Ok(())
}

/// XOR a mask into one byte.
fn xor_byte(frame: &mut [u8], at: usize, mask: u8) -> Result<(), GenError> {
    let len = frame.len();
    let slot = frame
        .get_mut(at)
        .ok_or_else(|| oob("xor_byte", at, 1, len))?;
    *slot ^= mask;
    Ok(())
}

/// Overwrite a range with `value`, which must be exactly the range's length.
fn set_range(frame: &mut [u8], at: usize, value: &[u8]) -> Result<(), GenError> {
    let len = frame.len();
    let dst = frame
        .get_mut(at..at + value.len())
        .ok_or_else(|| oob("set_range", at, value.len(), len))?;
    dst.copy_from_slice(value);
    Ok(())
}

/// Copy the first `end` bytes.
fn prefix(frame: &[u8], end: usize) -> Result<Vec<u8>, GenError> {
    frame
        .get(..end)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| oob("prefix", 0, end, frame.len()))
}

fn oob(op: &str, at: usize, len: usize, frame_len: usize) -> GenError {
    GenError::Encode {
        detail: format!(
            "{op}: offset {at} length {len} is out of range for a {frame_len}-byte frame. \
             The offset came from a decoded header, so suspect a layout change rather than \
             an arithmetic slip."
        ),
    }
}

/// Assert the mutated frame really is rejected, and for the claimed reason.
///
/// Without this, a row could claim `truncated` while the codec actually
/// rejected it as `unknown_version` — the row would still "pass" a harness that
/// only checks that *something* was rejected, and the taxonomy would drift from
/// the codec silently.
fn assert_rejects_as(frame: &[u8], expect: &str) -> Result<(), GenError> {
    match decode_datagram(frame) {
        Ok(_) => Err(GenError::Encode {
            detail: format!("expected reject `{expect}`, but the frame decoded successfully"),
        }),
        Err(e) => {
            let got = e.reason().as_str();
            if got == expect {
                Ok(())
            } else {
                Err(GenError::Encode {
                    detail: format!("expected reject `{expect}`, codec produced `{got}`"),
                })
            }
        }
    }
}

/// Assert a mutated frame rejects for the claimed reason, then record the row.
///
/// Shared by both halves of the decode-reject set. The assertion is the point:
/// without it a row could claim `truncated` while the codec actually rejected it
/// as `unknown_version` — still "rejected", so a harness checking only that
/// *something* failed would pass, and the taxonomy would drift from the codec
/// silently.
///
/// # Errors
///
/// [`GenError::Encode`] if the frame decodes, or rejects for a different reason.
fn push_reject(
    base: &Row,
    out: &mut Vec<Row>,
    name: &'static str,
    description: &'static str,
    reason: &'static str,
    frame: &[u8],
    note: &str,
) -> Result<(), GenError> {
    assert_rejects_as(frame, reason)?;
    out.push(mutated(
        base,
        frame,
        name,
        description,
        "decode_reject",
        reason,
        serde_json::json!({ "mutation": note }),
    ));
    Ok(())
}

/// Build every structural decode-reject row from one valid base frame.
///
/// # Errors
///
/// [`GenError::Encode`] if any mutation fails to produce the intended reject
/// reason — which would mean the row is not testing what it claims.
pub fn decode_reject_rows(base_frame: &[u8], base: &Row) -> Result<Vec<Row>, GenError> {
    let view = decode_datagram(base_frame).map_err(|e| GenError::Encode {
        detail: format!("{e:?}"),
    })?;
    let ext_len_range = view.ext_length_field_range();
    let mut out = Vec::new();

    // Version byte at a named offset, not a literal position.
    let mut f = base_frame.to_vec();
    set_byte(&mut f, VERSION_OFFSET, PROTOCOL_VERSION.saturating_add(1))?;
    push_reject(
        base,
        &mut out,
        "decode_reject_unknown_version",
        "Version byte set to 4, one past the only version (3). There is no forward \
         compatibility: an unknown version is terminal even on a one-byte buffer (§2 decode \
         step 2), and the version field is the extension path.",
        "unknown_version",
        &f,
        "frame[VERSION_OFFSET] = 4",
    )?;

    // v2-labelled: every other byte a valid v3 frame. R-22: v3 is the only
    // version, with no allowlist, no floor and no dual decode.
    let mut f = base_frame.to_vec();
    set_byte(&mut f, VERSION_OFFSET, 2)?;
    push_reject(
        base,
        &mut out,
        "decode_reject_v2_labelled",
        "Version byte set to 2, every other byte a valid v3 frame. Header v3 is the ONLY version \
         (ADR-0036 §2, R-22): there is no allowlist, no floor and no dual decode, so a v2 peer \
         is refused loudly as unknown_version — never decoded under the v2 layout, which lacks \
         the 4-byte gop_counter and would misread every later field.",
        "unknown_version",
        &f,
        "frame[VERSION_OFFSET] = 2",
    )?;

    // Undefined flag bit. This is the covert-channel closure: v1 masked unknown
    // bits and continued, which made the flag byte a channel out of a
    // compromised relay that no test would fail.
    let mut f = base_frame.to_vec();
    xor_byte(&mut f, FLAGS_OFFSET, 0b1000_0000)?;
    push_reject(
        base,
        &mut out,
        "decode_reject_reserved_flag_bit",
        "Flag bit 7 set. Every bit is decoded into a field the receiver inspects or rejected if \
         set (§2) — there is no masking variant of the flag parser, because masking-and-continuing \
         is what made v1's flag byte a covert channel.",
        "reserved_flag_bit_set",
        &f,
        "frame[FLAGS_OFFSET] |= 0x80",
    )?;

    flag_byte_rejects(base_frame, base, &mut out)?;

    // Declared payload length over MAX_PAYLOAD_BYTES: the parsing trust
    // boundary. Enforced BEFORE any allocation.
    let mut f = base_frame.to_vec();
    set_range(&mut f, PAYLOAD_LENGTH_OFFSET, &0xFFFF_FFFFu32.to_be_bytes())?;
    push_reject(
        base,
        &mut out,
        "decode_reject_payload_length_exceeds_max",
        "Declared payload length 0xFFFFFFFF. The wire-format maximum is enforced BEFORE any \
         allocation — the parsing trust boundary of §2. A reader that pre-allocates from this \
         attacker-controlled field is the defect that passes fuzzing of the decode function while \
         being wrong in the reader that calls it.",
        "payload_length_exceeds_max",
        &f,
        "frame[PAYLOAD_LENGTH_OFFSET..+4] = 0xFFFFFFFF",
    )?;

    // Declared length larger than the bytes present, but under the max.
    let mut f = base_frame.to_vec();
    let bigger = base.decoded.payload_length + 1;
    set_range(&mut f, PAYLOAD_LENGTH_OFFSET, &bigger.to_be_bytes())?;
    push_reject(
        base,
        &mut out,
        "decode_reject_payload_length_exceeds_available",
        "Declared payload length one byte beyond what the datagram carries, and well under the \
         maximum — so it is the availability check firing, not the limit check. Distinct triage \
         from the max case: this one points at sender framing or upstream truncation.",
        "payload_length_exceeds_available",
        &f,
        "frame[PAYLOAD_LENGTH_OFFSET..+4] = payload_length + 1",
    )?;

    // Truncated INSIDE the fixed-size relay region.
    //
    // Deliberately not `frame[..len-1]`: cutting the last byte leaves the
    // header intact and the codec reports `payload_length_exceeds_available`,
    // because the declared length no longer fits. That is a different reason
    // with different triage, and a row that cut the tail would have been
    // labelled `truncated` while exercising the availability check — the row
    // asserting a rejection that would have happened anyway, for another
    // reason. Cutting inside the relay region ends the frame before a required
    // FIXED-SIZE region, which is what `truncated` actually means.
    let cut = usize::try_from(base.offsets.relay_region_offset).unwrap_or(0) + 1;
    let f = prefix(base_frame, cut)?;
    push_reject(
        base,
        &mut out,
        "decode_reject_truncated",
        "Frame cut one byte into the fixed-size relay region, so a required region is \
         incomplete. Deliberately NOT a tail truncation: cutting the last byte leaves the header \
         intact and produces payload_length_exceeds_available instead, a different reason with \
         different triage. Note this is decode_datagram — the prefix-tolerant stream entry point \
         reports this same condition as the non-reject Ok(None), which is why the two entry \
         points need different operator guidance.",
        "truncated",
        &f,
        "frame[..relay_region_offset + 1]",
    )?;

    extension_and_framing_rejects(base_frame, base, ext_len_range, &mut out)?;

    Ok(out)
}

/// The two flag-byte rejects beyond bit 7: the lowest undefined bit, and the
/// v3 key-bearing-with-discardable combination (§2 decode step 5).
///
/// Split out of [`decode_reject_rows`] rather than suppressing
/// `too_many_lines`; both mutate only the flag byte.
///
/// # Errors
///
/// [`GenError::Encode`] if either mutation fails to produce its intended reject
/// reason.
fn flag_byte_rejects(base_frame: &[u8], base: &Row, out: &mut Vec<Row>) -> Result<(), GenError> {
    // The LOWEST undefined bit, so both ends of the 0xF8 undefined mask are
    // pinned: a decoder whose mask were 0x0F would pass the bit-7 row.
    let mut f = base_frame.to_vec();
    xor_byte(&mut f, FLAGS_OFFSET, 0b0000_1000)?;
    push_reject(
        base,
        out,
        "decode_reject_reserved_flag_bit_3",
        "Flag bit 3 set — the lowest undefined bit, so the 0xF8 undefined mask is pinned at both \
         ends (bit 7 above, bit 3 here). A decoder whose legal mask were 0x0F instead of 0x07 \
         passes the bit-7 row and fails this one.",
        "reserved_flag_bit_set",
        &f,
        "frame[FLAGS_OFFSET] |= 0x08",
    )?;

    // Key-bearing AND discardable (v3, §2 decode step 5). The base is
    // key-bearing; set discardable. No encoder can produce this frame — every
    // conforming encoder refuses it — so it exists only as a mutation.
    let mut f = base_frame.to_vec();
    xor_byte(&mut f, FLAGS_OFFSET, FLAG_DISCARDABLE)?;
    push_reject(
        base,
        out,
        "decode_reject_key_bearing_discardable",
        "A key-bearing frame with the discardable flag set (flags 0x07). New in header v3 (§2 \
         decode step 5): a frame carrying a new transmit key must never be droppable, or a \
         receiver that dropped it could not open what follows (§4). Decidable from the flag \
         byte alone, after the reserved-bit check and before any length check.",
        "key_bearing_discardable",
        &f,
        "frame[FLAGS_OFFSET] |= 0x02 (base is key-bearing)",
    )?;

    Ok(())
}

/// The two rejects that are not single-field header mutations.
///
/// Split out of [`decode_reject_rows`] rather than suppressing
/// `too_many_lines`: these two differ in kind from their neighbours. The header
/// mutations above each corrupt **one fixed-offset scalar**; these corrupt a
/// **conditional region's declared length** and the **frame boundary itself**,
/// which is why they need the decoded `ext_length_field_range` rather than a
/// constant offset.
///
/// # Errors
///
/// [`GenError::Encode`] if either mutation fails to produce its intended reject
/// reason — which would mean the row is not testing what it claims.
fn extension_and_framing_rejects(
    base_frame: &[u8],
    base: &Row,
    ext_len_range: core::ops::Range<usize>,
    out: &mut Vec<Row>,
) -> Result<(), GenError> {
    // Declared extension region larger than the registry could ever produce.
    let mut f = base_frame.to_vec();
    set_range(&mut f, ext_len_range.start, &0xFFFFu16.to_be_bytes())?;
    push_reject(
        base,
        out,
        "decode_reject_extensions_too_large",
        "Declared extension region 0xFFFF, larger than the registry-derived maximum. The bound is \
         DERIVED from the registry, not chosen: a grammatically valid region is a subset of the \
         registry with each type at most once. Detected before the TLV walk, so no per-byte work \
         is done on hostile input.",
        "extensions_too_large",
        &f,
        "frame[ext_length_field_range] = 0xFFFF",
    )?;

    // Trailing bytes after a well-formed frame. QUIC preserves datagram
    // boundaries, so this is never a transport artifact.
    let mut f = base_frame.to_vec();
    f.push(0x00);
    push_reject(
        base,
        out,
        "decode_reject_trailing_bytes",
        "One byte appended after a well-formed frame's signature. QUIC preserves datagram \
         boundaries, so this is never a transport artifact — some endpoint wrote it, and the \
         signature covers neither. If the frame otherwise verifies, the appender sits downstream \
         of the publisher and it is a security escalation.",
        "trailing_bytes",
        &f,
        "frame || 0x00",
    )?;

    Ok(())
}

/// The salience-out-of-range row.
///
/// # Errors
///
/// [`GenError::Encode`] if the codec does not reject it as `extensions_malformed`.
pub fn salience_out_of_range_row(base_frame: &[u8], base: &Row) -> Result<Row, GenError> {
    let view = decode_datagram(base_frame).map_err(|e| GenError::Encode {
        detail: format!("{e:?}"),
    })?;
    let ext = view.extensions_range();
    let mut frame = base_frame.to_vec();
    // Last byte of the single-entry TLV region is the salience value.
    let value_at = ext.end.saturating_sub(1);
    set_byte(&mut frame, value_at, 101)?;
    assert_rejects_as(&frame, "extensions_malformed")?;
    Ok(mutated(
        base,
        frame.as_slice(),
        "ext_salience_out_of_range",
        "Salience 101, one past the registry's declared accepted maximum of 100. The accepted \
         SET is enforced, not merely the length and type — §7 treats publisher-supplied selection \
         signals as untrusted, and enforcing the declared set is what drives the surplus of \
         representable-over-declared values to zero. Without this row the accepted range is a \
         trust boundary with no cross-language vector at all.",
        "decode_reject",
        "extensions_malformed",
        serde_json::json!({ "mutation": "salience value byte = 101 (accepted max is 100)" }),
    ))
}

/// The tamper pair: mutated publisher region, mutated signature.
///
/// # Errors
///
/// [`GenError::Encode`] if either mutation still decodes cleanly at the codec
/// layer — they must, because both are *verification* failures rather than
/// parse failures, and a parse rejection would mean the row never reaches the
/// control it is testing.
pub fn tamper_rows(base_frame: &[u8], base: &Row) -> Result<Vec<Row>, GenError> {
    let view = decode_datagram(base_frame).map_err(|e| GenError::Encode {
        detail: format!("{e:?}"),
    })?;
    let mut out = Vec::new();

    // Flip a bit in the stream sequence: inside the publisher region, so it is
    // BOTH signed and associated data. It must still DECODE — otherwise the row
    // tests the parser rather than the signature.
    let seq = media_protocol::frame::STREAM_SEQUENCE_OFFSET;
    let mut frame = base_frame.to_vec();
    xor_byte(&mut frame, seq, 0x01)?;
    if decode_datagram(&frame).is_err() {
        return Err(GenError::Encode {
            detail: "tampered publisher region must still DECODE, or the row tests the parser \
                     instead of the signature layer"
                .to_string(),
        });
    }
    out.push(mutated(
        base,
        frame.as_slice(),
        "tamper_publisher_region",
        "One bit flipped in stream_sequence, inside the publisher region. The frame still \
         DECODES — this is a verification failure, not a parse failure. It fails twice over, \
         which is the point: stream_sequence is covered by the signature AND is the AEAD nonce \
         input AND sits in the associated data, so tampering cannot silently reorder or replay.",
        "verify_reject",
        "signature_invalid",
        serde_json::json!({
            "mutation": "frame[STREAM_SEQUENCE_OFFSET] ^= 0x01",
            "decodes": true,
            "signature_valid": false,
            "_assert": "Must still decode. A parse rejection here means the row never reaches \
                        the signature check it exists to exercise."
        }),
    ));

    // Flip a bit in the signature itself.
    let sig = view.signature_range();
    // Flip a bit in the v3 GoP counter: inside the fixed prefix, so it is
    // signed AND associated data. Every u32 is a legal gop_counter, so the
    // mutated frame still DECODES; only verification can catch it.
    let mut frame = base_frame.to_vec();
    xor_byte(&mut frame, GOP_COUNTER_OFFSET, 0x01)?;
    if decode_datagram(&frame).is_err() {
        return Err(GenError::Encode {
            detail: "tampered gop_counter must still DECODE: every u32 is a legal value"
                .to_string(),
        });
    }
    out.push(mutated(
        base,
        frame.as_slice(),
        "tamper_gop_counter",
        "One bit flipped in the v3 gop_counter (offset 10). Every u32 is a legal value, so the \
         frame still DECODES and no parse check can catch this — only the signature can. The \
         field is in the publisher region, so it is signed AND associated data: a relay that \
         rewrote it (to fake a group start for MH's switching, say) fails verification at \
         every receiver.",
        "verify_reject",
        "signature_invalid",
        serde_json::json!({
            "mutation": "frame[GOP_COUNTER_OFFSET] ^= 0x01",
            "decodes": true,
            "signature_valid": false,
            "would_decrypt_if_verification_skipped": false,
            "_assert": "Must still decode, and verification must fail. Decryption would ALSO \
                        fail (gop_counter is in the AAD), but the signature is checked first."
        }),
    ));

    out.push(mutated(
        base,
        frame_sig(base_frame, sig.start)?.as_slice(),
        "tamper_signature",
        "One bit flipped in the trailing signature. The publisher region and payload are \
         untouched, so this frame DECRYPTS correctly and only verification fails — the mirror \
         image of the publisher-region tamper, and together they show the two controls are \
         independent rather than one control counted twice.",
        "verify_reject",
        "signature_invalid",
        serde_json::json!({
            "mutation": "frame[signature_offset] ^= 0x01",
            "decodes": true,
            "signature_valid": false,
            "would_decrypt_if_verification_skipped": true
        }),
    ));
    Ok(out)
}

/// The base frame with one bit of its signature flipped.
fn frame_sig(base_frame: &[u8], at: usize) -> Result<Vec<u8>, GenError> {
    let mut frame = base_frame.to_vec();
    xor_byte(&mut frame, at, 0x01)?;
    if decode_datagram(&frame).is_err() {
        return Err(GenError::Encode {
            detail: "tampered signature must still DECODE".to_string(),
        });
    }
    Ok(frame)
}

/// The relay-rewrite row: the base (uplink) frame with its relay region
/// rewritten to a downlink value, which still verifies and decrypts.
///
/// The rewrite goes through `media_protocol::codec::rewrite_relay_region`, the
/// relay's own entry point. What the row pins is not the rewrite mechanics but
/// the **span claim**: the relay region is outside both the signature and the
/// AEAD associated data, so a changed `stream_id`/`hop_sequence` leaves the
/// signed input and the AAD byte-identical to the base row's.
///
/// # Errors
///
/// [`GenError::Encode`] if the rewrite fails or the spans moved.
pub fn relay_rewrite_row(base_frame: &[u8], base: &Row) -> Result<Row, GenError> {
    let mut frame = base_frame.to_vec();
    // A downlink slot id that is neither audio's nor video's send id, so it
    // cannot be mistaken for the uplink value.
    let slot_id: u16 = 0x0007;
    let hop: u32 = 0x0000_002A;
    media_protocol::codec::rewrite_relay_region(&mut frame, slot_id, hop).map_err(|e| {
        GenError::Encode {
            detail: format!("relay rewrite failed: {e:?}"),
        }
    })?;
    let row = mutated_with(
        base,
        &frame,
        "relay_rewrite_still_verifies",
        "relay_rewrite_base_uplink with its relay region rewritten as MH does on the DOWNLINK: \
         stream_id from the publisher's send id (0, audio) to a subscriber slot id (7), and a \
         new hop_sequence. The signature, the AEAD associated data and the signed input are \
         byte-identical to the base row's, so the frame still verifies and decrypts: the relay \
         region is outside both (§2, §3, §4). This is what lets MH rewrite per subscriber — and \
         why the uplink stream_id is a publisher-set, unauthenticated attribution label.",
        "relay_rewrite",
        None,
        serde_json::json!({
            "rewritten_from": base.name,
            "mutation": "relay stream_id := 0x0007, hop_sequence := 0x0000002A",
            "decodes": true,
            "signature_valid": true,
            "decrypts": true,
            "_assert": "Verify against the base row's identity key and decrypt with its transmit \
                        key; assert derived.signed_input_hex and derived.aead_aad_hex EQUAL the \
                        base row's while the relay bytes differ."
        }),
    );
    if row.derived.signed_input_hex != base.derived.signed_input_hex
        || row.derived.aead_aad_hex != base.derived.aead_aad_hex
    {
        return Err(GenError::SpanDisagreement {
            span: "relay rewrite moved the signed input or the AAD",
            spec_derived: base.derived.signed_input_hex.len(),
            codec_derived: row.derived.signed_input_hex.len(),
        });
    }
    Ok(row)
}

/// Wrong transmit key and wrong KEK: two AES-GCM failures, opposite teams.
///
/// # Errors
///
/// [`GenError::Encode`] if the base frame does not decode.
pub fn crypto_reject_rows(base_frame: &[u8], base: &Row) -> Result<Vec<Row>, GenError> {
    let _ = decode_datagram(base_frame).map_err(|e| GenError::Encode {
        detail: format!("{e:?}"),
    })?;
    let mut out = Vec::new();

    let mut row = mutated(
        base,
        base_frame,
        "decrypt_reject_wrong_transmit_key",
        "The frame is untouched and valid; the RECEIVER holds the wrong transmit key. Decryption \
         of the SFrame payload fails. Distinct from unwrap_reject_wrong_kek and routed to a \
         different team: this points at the key schedule or the sender, that one at key \
         distribution.",
        "decrypt_reject",
        "decrypt_failed",
        serde_json::json!({
            "use_transmit_key_hex": hex::encode(crate::fixtures::BOB_TRANSMIT_KEY),
            "decodes": true,
            "signature_valid": true,
            "_assert": "Decrypt with the WRONG transmit key above, not the row's crypto block. \
                        The GCM tag check must fail."
        }),
    );
    row.crypto = base.crypto.clone();
    out.push(row);

    let row = mutated(
        base,
        base_frame,
        "unwrap_reject_wrong_kek",
        "The frame is untouched and valid; the RECEIVER holds the wrong meeting KEK. The KEK \
         unwrap fails before decryption is even attempted. Two AES-GCM decrypts on one receive \
         path with OPPOSITE remedies is exactly why decrypt_failed was split: one \
         decrypt_reject kind carrying two reason values is the kind-vs-reason separation doing \
         its job.",
        "decrypt_reject",
        "unwrap_failed",
        serde_json::json!({
            "use_kek_hex": hex::encode(crate::fixtures::OTHER_KEK),
            "decodes": true,
            "signature_valid": true,
            "_assert": "Unwrap with the WRONG KEK above. The wrap tag check must fail, and \
                        decryption must never be attempted."
        }),
    );
    out.push(row);
    Ok(out)
}

/// The replay row: byte-identical to a frame already accepted.
#[must_use]
pub fn replay_row(base_frame: &[u8], base: &Row) -> Row {
    mutated(
        base,
        base_frame,
        "replay_same_stream_sequence",
        "BYTE-IDENTICAL to full_frame_compose. It carries a valid signature and a valid \
         authentication tag because it WAS legitimately produced, so no cryptographic check can \
         reject it — only the receiver's sliding window per (sender, stream, generation) can. \
         The row exists because that window is a receiver obligation the frame format cannot \
         enforce, and an implementation that omits it passes every other row here.",
        "replay_reject",
        "replay_detected",
        serde_json::json!({
            "replay_of": "full_frame_compose",
            "decodes": true,
            "signature_valid": true,
            "decrypts": true,
            "_assert": "Feed full_frame_compose FIRST, then this row. Accepting it means the \
                        replay window is missing. Asserting rejection without the first feed \
                        would pass against a receiver that rejects everything."
        }),
    )
}
