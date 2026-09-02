//! The Rust leg of the cross-language drift gate.
//!
//! Reads `proto/test-vectors/frame-v2.vectors.json` as **data** — never through
//! the generator's structs — and drives every row through `media-protocol`.
//! Reading it as data matters: consuming the generator's own types would test
//! that a struct round-trips through serde, not that the committed bytes decode.
//!
//! # Exhaustiveness is proved, not assumed
//!
//! Two mechanisms, because they catch different things. `rows_consumed ==
//! rows.len()` catches a row nothing reads. The `kind` dispatch has **no
//! catch-all**, so an unrecognised kind fails rather than falling through green
//! — the row-nothing-reads failure mode is invisible in a passing suite
//! otherwise.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "Test code: a panic is the failure report."
)]

use media_protocol::codec::{decode_datagram, ALL_REJECT_REASONS};
use media_protocol::frame::{
    AEAD_TAG_BYTES, KEY_ID_BYTES, LEGAL_FLAG_MASK, MAX_PAYLOAD_BYTES, PROTOCOL_VERSION,
};
use media_vector_gen::{crypto, schedule, CIPHER_SUITE_ID};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::signature;
use serde_json::Value;
use std::collections::BTreeSet;

const VECTORS: &str = "../../proto/test-vectors/frame-v2.vectors.json";

fn doc() -> Value {
    let text = std::fs::read_to_string(VECTORS).expect("vector file must exist");
    serde_json::from_str(&text).expect("vector file must be valid JSON")
}

fn hexf(v: &Value, k: &str) -> Vec<u8> {
    hex::decode(
        v[k].as_str()
            .unwrap_or_else(|| panic!("missing hex field `{k}`")),
    )
    .unwrap_or_else(|e| panic!("field `{k}` is not hex: {e}"))
}

#[test]
fn wire_constants_match_the_rust_codec() {
    let d = doc();
    assert_eq!(
        d["header_version"].as_u64(),
        Some(u64::from(PROTOCOL_VERSION))
    );
    assert_eq!(
        d["max_payload_bytes"].as_u64(),
        Some(MAX_PAYLOAD_BYTES as u64),
        "the cross-language SSoT for MAX_PAYLOAD_BYTES disagrees with the Rust constant"
    );
    assert_eq!(
        d["wire_constants"]["legal_flag_mask"].as_u64(),
        Some(u64::from(LEGAL_FLAG_MASK))
    );
    assert_eq!(
        d["cipher_suite_id"].as_u64(),
        Some(u64::from(CIPHER_SUITE_ID))
    );
}

/// Equality, not subset. Subset stays green when a variant is **deleted** from
/// the `reject_reasons!` macro; only equality makes an omission unrepresentable
/// across the language boundary, which is what the macro already achieves
/// within Rust.
///
/// Scoped to `has_vector == true` because `no_transmit_key` is codec-family but
/// receiver-state-dependent, so it is deliberately not a Rust codec variant.
#[test]
fn codec_reject_tokens_equal_all_reject_reasons() {
    let d = doc();
    let from_file: BTreeSet<String> = d["reject_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["layer"] == "codec" && e["has_vector"] == true)
        .map(|e| e["token"].as_str().unwrap().to_string())
        .collect();
    let from_codec: BTreeSet<String> = ALL_REJECT_REASONS
        .iter()
        .map(|r| r.as_str().to_string())
        .collect();
    assert_eq!(from_file, from_codec);
}

#[test]
fn has_vector_true_tokens_have_rows_and_false_tokens_have_none() {
    let d = doc();
    let used: BTreeSet<String> = d["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["reject_reason"].as_str().map(ToString::to_string))
        .collect();
    for e in d["reject_reasons"].as_array().unwrap() {
        let token = e["token"].as_str().unwrap();
        let has_vector = e["has_vector"].as_bool().unwrap();
        if has_vector {
            assert!(
                used.contains(token),
                "`{token}` claims has_vector but no row asserts it"
            );
        } else {
            assert!(
                !used.contains(token),
                "`{token}` claims has_vector: false but a row asserts it; either the row \
                 fabricates receiver state or the classification is wrong"
            );
        }
    }
}

#[test]
fn crypto_block_is_exactly_partitioned() {
    let d = doc();
    let secret: BTreeSet<&str> = d["key_material_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let public: BTreeSet<&str> = d["derived_public_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        secret.is_disjoint(&public),
        "a crypto field classified as both secret and public"
    );
    let members: BTreeSet<&str> = d["vectors"][0]["crypto"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let classified: BTreeSet<&str> = secret.union(&public).copied().collect();
    assert_eq!(
        members, classified,
        "every `crypto` member must be classified exactly once: an unclassified field \
         would escape the synthetic-pattern check by silence"
    );
}

#[test]
fn every_row_is_consumed_and_every_kind_is_handled() {
    let d = doc();
    let rows = d["vectors"].as_array().unwrap();
    assert!(!rows.is_empty(), "zero rows: the gate would assert nothing");
    let mut consumed = 0usize;

    for row in rows {
        let name = row["name"].as_str().unwrap();
        let frame = hexf(row, "frame_hex");
        let kind = row["kind"].as_str().unwrap();

        // NO catch-all arm. An unrecognised kind fails here rather than
        // falling through green.
        match kind {
            "roundtrip" | "decode_ok" | "full_frame" | "wrap_binding" | "verify_reject"
            | "decrypt_reject" | "replay_reject" => check_decodes(row, &frame, name),
            "decode_reject" => check_rejects(row, &frame, name),
            other => panic!(
                "row `{name}` has unrecognised kind `{other}`; add an arm rather than a \
                 catch-all, or the row is silently unread"
            ),
        }
        consumed += 1;
    }
    assert_eq!(
        consumed,
        rows.len(),
        "not every row was consumed: a row nothing reads is a row that proves nothing"
    );
}

fn check_rejects(row: &Value, frame: &[u8], name: &str) {
    let expect = row["reject_reason"]
        .as_str()
        .unwrap_or_else(|| panic!("decode_reject row `{name}` must declare a reject_reason"));
    match decode_datagram(frame) {
        Ok(_) => panic!("row `{name}` expected reject `{expect}` but decoded successfully"),
        Err(e) => assert_eq!(
            e.reason().as_str(),
            expect,
            "row `{name}` expected `{expect}`, codec produced `{}`",
            e.reason().as_str()
        ),
    }
}

fn check_decodes(row: &Value, frame: &[u8], name: &str) {
    let view = decode_datagram(frame)
        .unwrap_or_else(|e| panic!("row `{name}` should decode but was rejected: {e:?}"));

    // Offsets and decoded fields.
    let off = &row["offsets"];
    assert_eq!(
        view.publisher_region().len() as u64,
        off["publisher_region_len"].as_u64().unwrap(),
        "row `{name}`: publisher_region_len"
    );
    assert_eq!(
        view.relay_region_offset() as u64,
        off["relay_region_offset"].as_u64().unwrap(),
        "row `{name}`: relay_region_offset"
    );
    assert_eq!(
        view.payload_range().start as u64,
        off["payload_offset"].as_u64().unwrap(),
        "row `{name}`: payload_offset"
    );
    assert_eq!(
        view.signature_range().start as u64,
        off["signature_offset"].as_u64().unwrap(),
        "row `{name}`: signature_offset"
    );

    let dec = &row["decoded"];
    assert_eq!(
        u64::from(view.stream_sequence()),
        dec["stream_sequence"].as_u64().unwrap()
    );
    assert_eq!(
        u64::from(view.stream_id()),
        dec["stream_id"].as_u64().unwrap()
    );
    assert_eq!(
        u64::from(view.hop_sequence()),
        dec["hop_sequence"].as_u64().unwrap()
    );
    assert_eq!(
        view.payload_length() as u64,
        dec["payload_length"].as_u64().unwrap()
    );
    assert_eq!(
        view.flags().key_bearing,
        dec["flags"]["key_bearing"].as_bool().unwrap()
    );

    // THE THREE PROTECTED COMPUTATIONS, checked against the pinned spans.
    let aad = hexf(&row["derived"], "aead_aad_hex");
    assert_eq!(
        view.publisher_region(),
        aad.as_slice(),
        "row `{name}`: the AEAD associated-data span disagrees with the pinned value"
    );
    let signed = hexf(&row["derived"], "signed_input_hex");
    let [p, pay] = view.signed_ranges();
    let mut ours = Vec::new();
    ours.extend_from_slice(p);
    ours.extend_from_slice(pay);
    assert_eq!(
        ours, signed,
        "row `{name}`: the signed range disagrees with the pinned value"
    );
    assert!(
        signed.starts_with(&aad) && aad.len() < signed.len(),
        "row `{name}`: the AAD must be a STRICT prefix-subset of the signed input"
    );
    let naive = hexf(&row["derived"], "naive_contiguous_signed_input_hex");
    assert_ne!(
        naive, signed,
        "row `{name}`: the pinned near-miss coincides with the signed input, making it vacuous"
    );

    // Detached-tag split: key_id || tag || ciphertext.
    let payload = view.payload();
    assert_eq!(
        hex::encode(&payload[..KEY_ID_BYTES]),
        dec["key_id"].as_str().unwrap(),
        "row `{name}`: key id on the wire disagrees with the pinned value"
    );
    assert_eq!(
        hex::encode(&payload[KEY_ID_BYTES..KEY_ID_BYTES + AEAD_TAG_BYTES]),
        row["derived"]["payload_tag_hex"].as_str().unwrap(),
        "row `{name}`: detached tag is not at the pinned offset"
    );

    // The key id must equal a re-pack of the DECOMPOSED fields. This is the one
    // place the two are compared; everywhere else the wire bytes are used, so a
    // re-pack bug shows up as a crypto mismatch rather than cancelling out.
    let kd = &dec["key_id_decomposed"];
    let repacked = (kd["sender_id"].as_u64().unwrap() << 48)
        | (kd["stream"].as_u64().unwrap() << 40)
        | u64::from_be_bytes({
            let g = hex::decode(kd["generation"].as_str().unwrap()).unwrap();
            let mut b = [0u8; 8];
            b.copy_from_slice(&g);
            b
        });
    assert_eq!(
        hex::encode(repacked.to_be_bytes()),
        dec["key_id"].as_str().unwrap(),
        "row `{name}`: the decomposed fields do not re-pack to the wire key id"
    );

    // The nonce must track THIS frame's counter, on every row that decodes —
    // not only the crypto-verified ones. @security finding K: a tamper row that
    // mutates a byte inside `stream_sequence` changes the nonce, and pinning the
    // pre-mutation value made the row assert a nonce its own counter does not
    // produce. Checked here rather than only in the guard because the XOR is
    // awkward in jq and trivial in Rust, and because this is the field where a
    // silent convention is least acceptable: §2's whole argument for
    // `stream_sequence` immutability is that rewriting it breaks decryption.
    let salt = hexf(&row["derived"], "sframe_salt_hex");
    let mut expect_nonce = salt.clone();
    let ctr = u64::from(view.stream_sequence()).to_be_bytes();
    let at = expect_nonce.len() - ctr.len();
    for (i, c) in ctr.iter().enumerate() {
        expect_nonce[at + i] ^= *c;
    }
    assert_eq!(
        hex::encode(&expect_nonce),
        row["derived"]["sframe_nonce_hex"].as_str().unwrap(),
        "row `{name}`: pinned sframe_nonce_hex is not salt XOR BE12(stream_sequence) for the \
         stream_sequence in THIS row's frame"
    );

    // Only well-formed rows carry a reproducible crypto path.
    if row["kind"] == "roundtrip" || row["kind"] == "full_frame" || row["kind"] == "wrap_binding" {
        verify_and_open(row, &view, name);
    }
}

fn verify_and_open(row: &Value, view: &media_protocol::frame::MediaFrameView<'_>, name: &str) {
    // Send-side conformance: re-sign from the PINNED SEED and compare bytes.
    // Without this the vectors gate only verification — half the contract, and
    // the untested half is where a construction-order bug lives.
    let seed = hexf(&row["crypto"], "identity_private_seed_hex");
    let id = crypto::Identity::from_seed(&seed).expect("seed");
    assert_eq!(
        hex::encode(id.public_key()),
        row["crypto"]["identity_public_hex"].as_str().unwrap(),
        "row `{name}`: the pinned public key does not derive from the pinned seed"
    );
    let signed = hexf(&row["derived"], "signed_input_hex");
    assert_eq!(
        hex::encode(id.sign(&signed)),
        row["derived"]["signature_hex"].as_str().unwrap(),
        "row `{name}`: re-signing the pinned signed input does not reproduce the pinned \
         signature (Ed25519 is deterministic, so this cannot be nondeterminism)"
    );

    // Receive side, from the wire only.
    let public = signature::UnparsedPublicKey::new(
        &signature::ED25519,
        hexf(&row["crypto"], "identity_public_hex"),
    );
    public
        .verify(&signed, view.signature())
        .unwrap_or_else(|_| panic!("row `{name}`: signature must verify"));

    let payload = view.payload();
    let kid: [u8; KEY_ID_BYTES] = payload[..KEY_ID_BYTES].try_into().unwrap();
    let base_key = hexf(&row["crypto"], "transmit_key_hex");
    let derived = schedule::derive(&base_key, &kid, CIPHER_SUITE_ID).expect("derive");
    assert_eq!(
        hex::encode(derived.key),
        row["derived"]["sframe_key_hex"].as_str().unwrap()
    );
    assert_eq!(
        hex::encode(schedule::nonce_from(
            &derived.salt,
            u64::from(view.stream_sequence())
        )),
        row["derived"]["sframe_nonce_hex"].as_str().unwrap(),
        "row `{name}`: the pinned nonce must derive from the key id ON THE WIRE"
    );

    let mut buf = payload[KEY_ID_BYTES + AEAD_TAG_BYTES..].to_vec();
    buf.extend_from_slice(&payload[KEY_ID_BYTES..KEY_ID_BYTES + AEAD_TAG_BYTES]);
    let aead = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &derived.key).unwrap());
    let pt = aead
        .open_in_place(
            Nonce::assume_unique_for_key(schedule::nonce_from(
                &derived.salt,
                u64::from(view.stream_sequence()),
            )),
            Aad::from(view.publisher_region()),
            &mut buf,
        )
        .unwrap_or_else(|_| {
            panic!(
                "row `{name}`: decryption failed with AAD = the publisher region from the wire. \
                 This is the construction-order failure: payload_length must be written before \
                 the AAD is sliced."
            )
        });
    assert_eq!(
        hex::encode(pt),
        row["crypto"]["plaintext_hex"].as_str().unwrap()
    );
}
