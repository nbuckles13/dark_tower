//! The fuzz seed corpus, and the assertions that keep it honest.
//!
//! `crates/media-protocol/fuzz/corpus/` is gitignored, so "regenerate the
//! corpus" cannot mean checking in blobs — and hand-maintained blobs would
//! drift from the codec anyway. The checked-in artifact is this table: every
//! seed is built **through the v3 encoder** (so a seed cannot drift from the
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

use media_protocol::codec::{
    decode_datagram, decode_stream_frame, parse_header_prefix, RejectReason,
};
use media_protocol::reader::HeaderReader;
use std::fs;
use std::path::PathBuf;

use common::seeds::{seeds, Outcome};

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
        let _ = parse_header_prefix(&seed.bytes);
        let _ = HeaderReader::new().feed(&seed.bytes);
        let mut writable = seed.bytes.clone();
        let _ = media_protocol::codec::rewrite_relay_region(&mut writable, 1, 2);
        let mut writable = seed.bytes.clone();
        let _ = media_protocol::codec::rewrite_relay_region_header(&mut writable, 1, 2);
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

/// Write the seed corpus for every fuzz target.
///
/// Runs on every `cargo test`, writing into the gitignored corpus directories.
/// Deterministic: the same seeds with the same names every time, so this is
/// idempotent rather than accumulating.
#[test]
fn regenerate_fuzz_corpus() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fuzz")
        .join("corpus");
    for target in [
        "codec_decode",
        "codec_roundtrip",
        "header_prefix",
        "header_reader_split",
    ] {
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
