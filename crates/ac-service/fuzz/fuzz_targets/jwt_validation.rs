#![no_main]

//! JWT validation fuzz target: the production verify path — EdDSA, a real
//! Ed25519 public key, jsonwebtoken's `aws_lc_rs` backend (ADR-0027).
//!
//! Seed corpus: `examples/gen_jwt_seed.rs` generates, at fuzz time, one token
//! that VERIFIES against this key into the gitignored default corpus dir, so
//! input 1 already reaches and passes signature verification. Nothing is
//! committed; how to run it locally and in fuzz-nightly is in that file's
//! header and `docs/FUZZING.md` §Seed Corpus.

use jsonwebtoken::decode;
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

#[path = "../common/jwt_seed_key.rs"]
mod jwt_seed_key;

use jwt_seed_key::{decoding_key, validation, Claims};

fuzz_target!(|data: &[u8]| {
    static KEY: OnceLock<jsonwebtoken::DecodingKey> = OnceLock::new();
    static VALIDATION: OnceLock<jsonwebtoken::Validation> = OnceLock::new();
    let key = KEY.get_or_init(decoding_key);
    let validation = VALIDATION.get_or_init(validation);

    // Try to interpret the fuzz input as a UTF-8 string (JWT format)
    if let Ok(token_str) = std::str::from_utf8(data) {
        // Try to decode - should never panic
        let _ = decode::<Claims>(token_str, key, validation);
    }

    // Also fuzz the raw JWT parser by trying different separators
    // JWT format: header.payload.signature
    if data.len() >= 3 {
        if let Ok(s) = std::str::from_utf8(data) {
            // Look for potential JWT-like structures
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() == 3 {
                // Try base64 decoding each part
                use base64::engine::general_purpose;
                use base64::Engine;

                let _ = general_purpose::URL_SAFE_NO_PAD.decode(parts[0]);
                let _ = general_purpose::URL_SAFE_NO_PAD.decode(parts[1]);
                let _ = general_purpose::URL_SAFE_NO_PAD.decode(parts[2]);
            }
        }
    }
});
