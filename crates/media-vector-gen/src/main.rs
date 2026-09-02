//! Generates `proto/test-vectors/frame-v2.vectors.json`.
//!
//! **NON-PRODUCTION.** See the crate docs in `lib.rs` for why deliberately
//! dead-looking Rust crypto exists here and must not be deleted.
//!
//! Deterministic by construction: no `rand`, no clock, fixed Ed25519 seeds, and
//! serde field-declaration order for stable key ordering. Regenerating with
//! nothing changed leaves `git diff` empty.

use media_vector_gen::{build_vector_file, VECTOR_FILE_PATH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = build_vector_file()?;
    let mut text = serde_json::to_string_pretty(&file)?;
    text.push('\n');
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        // Relative to the workspace root, which is where `cargo run` puts us.
        VECTOR_FILE_PATH.to_string()
    });
    std::fs::write(&path, text)?;
    println!("wrote {} ({} rows)", path, file.vectors.len());
    Ok(())
}
