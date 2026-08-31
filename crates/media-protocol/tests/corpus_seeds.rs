//! The fuzz seed corpus, and the assertions that keep it honest.
//!
//! `crates/media-protocol/fuzz/corpus/` is gitignored, so "regenerate the
//! corpus" cannot mean checking in blobs — and hand-maintained blobs would
//! drift from the codec anyway. The checked-in artifact is this table: every
//! seed is built **through the v2 encoder** (so a seed cannot drift from the
//! codec) and every seed's expected outcome is asserted here.
//!
//! These assertions run under `cargo test`, which the validation pipeline
//! executes. An earlier design put the generator in `examples/`, where
//! `clippy --all-targets` compiles `main()` but `cargo test` never runs it —
//! the assertions would have been lint-clean and never executed a single
//! decode.
//!
//! Regenerate with:
//!
//! ```text
//! cargo test -p media-protocol --test corpus_seeds
//! ```

// Test code: a panic on a bad index IS the failure signal here, and these
// helpers deliberately index at named constant offsets.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

mod common;

use common::{
    build, ext_length_offset, ext_region_offset, flip_bit, overwrite, set_byte, truncated_to,
    with_trailing, Shape, ALL_SHAPES, SENTINEL_PAYLOAD,
};
use media_protocol::codec::{decode_datagram, decode_stream_frame, peek_frame_len, RejectReason};
use media_protocol::frame::{
    FLAGS_OFFSET, LEGAL_FLAG_MASK, MAX_EXT_BYTES, MAX_PAYLOAD_BYTES, PAYLOAD_LENGTH_OFFSET,
    PROTOCOL_VERSION, VERSION_OFFSET,
};
use std::fs;
use std::path::PathBuf;

/// What a seed must do when decoded as a datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Decodes cleanly.
    Accept,
    /// Rejects with this specific reason.
    Reject(RejectReason),
}

struct Seed {
    name: String,
    bytes: Vec<u8>,
    outcome: Outcome,
}

const MAXIMAL: Shape = Shape {
    key_bearing: true,
    extensions: true,
};

fn seeds() -> Vec<Seed> {
    let mut out: Vec<Seed> = Vec::new();
    let mut push = |name: String, bytes: Vec<u8>, outcome: Outcome| {
        out.push(Seed {
            name,
            bytes,
            outcome,
        });
    };

    // --- Accepting seeds: the full shape matrix, at several payload sizes ---
    for shape in ALL_SHAPES {
        for (label, payload) in [
            ("empty", [].as_slice()),
            ("small", SENTINEL_PAYLOAD.as_slice()),
            ("large", &[0x5Au8; 1024]),
        ] {
            let name = format!(
                "accept_key{}_ext{}_{label}",
                u8::from(shape.key_bearing),
                u8::from(shape.extensions)
            );
            push(name, build(shape, payload), Outcome::Accept);
        }
    }
    // Salience at both ends of the declared accepted range.
    for value in [0u8, 100] {
        push(
            format!("accept_salience_{value}"),
            common::build_with_salience(MAXIMAL, &SENTINEL_PAYLOAD, value),
            Outcome::Accept,
        );
    }

    let frame = build(MAXIMAL, &SENTINEL_PAYLOAD);

    // --- One seed per reject reason ---
    push(
        "reject_unknown_version".into(),
        set_byte(&frame, VERSION_OFFSET, PROTOCOL_VERSION + 1),
        Outcome::Reject(RejectReason::UnknownVersion),
    );
    // Each undefined flag bit individually.
    for bit in 0..8u32 {
        if LEGAL_FLAG_MASK & (1u8 << bit) == 0 {
            push(
                format!("reject_flag_bit_{bit}"),
                flip_bit(&frame, FLAGS_OFFSET, bit),
                Outcome::Reject(RejectReason::ReservedFlagBitSet),
            );
        }
    }
    push(
        "reject_payload_over_max".into(),
        overwrite(
            &frame,
            PAYLOAD_LENGTH_OFFSET,
            &u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap().to_be_bytes(),
        ),
        Outcome::Reject(RejectReason::PayloadLengthExceedsMax),
    );
    push(
        "accept_payload_at_max_declared_but_absent".into(),
        overwrite(
            &frame,
            PAYLOAD_LENGTH_OFFSET,
            &u32::try_from(MAX_PAYLOAD_BYTES).unwrap().to_be_bytes(),
        ),
        Outcome::Reject(RejectReason::PayloadLengthExceedsAvailable),
    );
    push(
        "reject_ext_too_large".into(),
        overwrite(
            &frame,
            ext_length_offset(&frame),
            &u16::try_from(MAX_EXT_BYTES + 1).unwrap().to_be_bytes(),
        ),
        Outcome::Reject(RejectReason::ExtensionsTooLarge),
    );
    push(
        "reject_ext_unknown_type".into(),
        set_byte(&frame, ext_region_offset(&frame), 0x7F),
        Outcome::Reject(RejectReason::ExtensionsMalformed),
    );
    push(
        "reject_ext_wrong_value_length".into(),
        set_byte(&frame, ext_region_offset(&frame) + 1, 2),
        Outcome::Reject(RejectReason::ExtensionsMalformed),
    );
    push(
        "reject_ext_value_out_of_range".into(),
        set_byte(&frame, ext_region_offset(&frame) + 2, 101),
        Outcome::Reject(RejectReason::ExtensionsMalformed),
    );
    for extra in [1usize, 8] {
        push(
            format!("reject_trailing_{extra}"),
            with_trailing(&frame, extra),
            Outcome::Reject(RejectReason::TrailingBytes),
        );
    }

    // --- Truncation at every byte boundary, for both conditional shapes ---
    for shape in ALL_SHAPES {
        let full = build(shape, &SENTINEL_PAYLOAD);
        for len in 0..full.len() {
            push(
                format!(
                    "truncate_key{}_ext{}_{len}",
                    u8::from(shape.key_bearing),
                    u8::from(shape.extensions)
                ),
                truncated_to(&full, len),
                // Every prefix of a valid frame is short of some region, and on
                // the datagram path a shortfall is either `truncated` or, for
                // the payload specifically, `payload_length_exceeds_available`.
                // Both are asserted below by class rather than pinned here.
                Outcome::Reject(RejectReason::Truncated),
            );
        }
    }
    out
}

#[test]
fn every_seed_matches_its_expected_outcome() {
    for seed in seeds() {
        match seed.outcome {
            Outcome::Accept => {
                let view = decode_datagram(&seed.bytes)
                    .unwrap_or_else(|err| panic!("seed {} must decode, got {err}", seed.name));
                assert_eq!(view.encoded_len(), seed.bytes.len(), "seed {}", seed.name);
            }
            Outcome::Reject(RejectReason::Truncated) => {
                // Truncation seeds: assert the *class*, since a prefix can fall
                // short in the payload region as well as a fixed one.
                let err = decode_datagram(&seed.bytes)
                    .err()
                    .unwrap_or_else(|| panic!("seed {} must reject", seed.name));
                assert!(
                    matches!(
                        err.reason(),
                        RejectReason::Truncated | RejectReason::PayloadLengthExceedsAvailable
                    ),
                    "seed {} rejected as {} rather than a shortfall",
                    seed.name,
                    err.reason()
                );
            }
            Outcome::Reject(expected) => {
                let err = decode_datagram(&seed.bytes)
                    .err()
                    .unwrap_or_else(|| panic!("seed {} must reject", seed.name));
                assert_eq!(err.reason(), expected, "seed {}", seed.name);
            }
        }
    }
}

#[test]
fn no_seed_panics_on_any_entry_point() {
    // The property the fuzzer looks for, asserted over the seed set: every
    // entry point returns rather than panicking, for every seed.
    for seed in seeds() {
        let _ = decode_datagram(&seed.bytes);
        let _ = decode_stream_frame(&seed.bytes);
        let _ = peek_frame_len(&seed.bytes);
        let mut writable = seed.bytes.clone();
        let _ = media_protocol::codec::rewrite_relay_region(&mut writable, 1, 2);
    }
}

#[test]
fn seed_names_are_unique() {
    let mut names: Vec<String> = seeds().into_iter().map(|s| s.name).collect();
    let before = names.len();
    names.sort();
    names.dedup();
    assert_eq!(
        before,
        names.len(),
        "duplicate seed name would overwrite a corpus file"
    );
}

/// Write the seed corpus for both fuzz targets.
///
/// Runs on every `cargo test`, writing into the gitignored corpus directories.
/// Deterministic: the same seeds with the same names every time, so this is
/// idempotent rather than accumulating.
#[test]
fn regenerate_fuzz_corpus() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz")
        .join("corpus");
    for target in ["codec_decode", "codec_roundtrip"] {
        let dir = root.join(target);
        fs::create_dir_all(&dir)
            .unwrap_or_else(|err| panic!("cannot create corpus dir {}: {err}", dir.display()));
        for seed in seeds() {
            let path = dir.join(&seed.name);
            fs::write(&path, &seed.bytes)
                .unwrap_or_else(|err| panic!("cannot write {}: {err}", path.display()));
        }
    }
}
