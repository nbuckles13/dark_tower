//! Generate the `jwt_validation` fuzz seed: one EdDSA token that VERIFIES
//! against the target's fixed key, so input 1 reaches and passes signature
//! verification.
//!
//! The token is generated at fuzz time and never committed: a really-signed
//! JWT in the repo would trip secret scanning. It is written to the gitignored
//! `crates/ac-service/fuzz/corpus/jwt_validation/valid_eddsa`, anchored on this
//! crate's `CARGO_MANIFEST_DIR` (so it lands there whatever the cwd), which is
//! the corpus dir `cargo fuzz run jwt_validation` reads by default. Takes no
//! arguments; any argument is a usage error (exit 2).
//!
//! Local workflow, from `crates/ac-service`:
//!
//!     cargo +nightly run --manifest-path fuzz/Cargo.toml --example gen_jwt_seed
//!     cargo +nightly fuzz run jwt_validation
//!
//! fuzz-nightly runs the same command (the `seed_cmd` of the jwt_validation
//! matrix entry) before fuzzing, and fails the job if it fails or if the corpus
//! dir is still empty. See also `docs/FUZZING.md` §Seed Corpus.
//!
//! Signs fixed claims with the harness's fixed key (`common/jwt_seed_key.rs`),
//! writes the file, then READS IT BACK and decodes those bytes with the
//! target's exact key and `Validation`, panicking (non-zero exit) unless that
//! is `Ok`. So these assertions execute on every nightly, not only when someone
//! regenerates by hand. Deterministic (Ed25519 signing is): the success line's
//! sha256 is the same every run. The token itself is never printed.

use jsonwebtoken::{decode, encode, Algorithm, Header};
use ring::digest::{digest, SHA256};
use std::path::Path;
use std::process::ExitCode;

#[path = "../common/jwt_seed_key.rs"]
mod jwt_seed_key;

use jwt_seed_key::{decoding_key, encoding_key, validation, Claims};

/// Relative to this crate's `CARGO_MANIFEST_DIR` (`crates/ac-service/fuzz`).
const OUT: &str = "corpus/jwt_validation/valid_eddsa";

fn main() -> ExitCode {
    if std::env::args_os().len() > 1 {
        eprintln!(
            "usage: gen_jwt_seed (no arguments)\n\
             writes $CARGO_MANIFEST_DIR/{OUT} (crates/ac-service/fuzz/{OUT})"
        );
        return ExitCode::from(2);
    }

    let claims = Claims {
        sub: "fuzz-seed".to_string(),
        // 2100-01-01T00:00:00Z; the target does not validate exp anyway.
        exp: 4_102_444_800,
    };
    let token = encode(&Header::new(Algorithm::EdDSA), &claims, &encoding_key())
        .expect("signing the seed token failed");

    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join(OUT);
    std::fs::create_dir_all(out.parent().expect("OUT has a parent"))
        .expect("creating the corpus dir failed");
    std::fs::write(&out, token.as_bytes()).expect("writing the seed file failed");

    // Verify what is on disk, not the in-memory string.
    let written = std::fs::read(&out).expect("reading the seed file back failed");
    let written = std::str::from_utf8(&written).expect("seed file is not UTF-8");
    let decoded = decode::<Claims>(written, &decoding_key(), &validation()).expect(
        "SEED SELF-CHECK FAILED: the written seed does not verify against the target's key",
    );
    assert_eq!(
        decoded.claims.sub, claims.sub,
        "seed self-check: sub mismatch"
    );
    assert_eq!(
        decoded.header.alg,
        Algorithm::EdDSA,
        "seed self-check: alg mismatch"
    );

    let sha256: String = digest(&SHA256, written.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!(
        "wrote {} ({} bytes, sha256 {sha256}); self-check: decode Ok, sub={}, alg={:?}",
        out.display(),
        written.len(),
        decoded.claims.sub,
        decoded.header.alg
    );
    ExitCode::SUCCESS
}
