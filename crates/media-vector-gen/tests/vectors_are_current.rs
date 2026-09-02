//! The committed vector file must be exactly what this generator produces.
//!
//! Two failures, one test. A **hand-edited** vector file reds here, which is
//! what stops someone "fixing" a row to make a codec pass — the escalation rule
//! in `proto/test-vectors/README.md` exists because conforming to a wrong
//! vector makes both implementations consistently wrong with the drift guard
//! green. And a **broken generator** reds here too, before it can emit a file
//! that gates anything.
//!
//! It is also the reason determinism is a hard requirement rather than a
//! nicety: a churny generator would red this on every unrelated run and the
//! test would be deleted within two stories.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "Test code: a panic is the failure report."
)]

const COMMITTED: &str = "../../proto/test-vectors/frame-v2.vectors.json";

fn regenerate() -> String {
    let file = media_vector_gen::build_vector_file().expect("generator must succeed");
    let mut text = serde_json::to_string_pretty(&file).expect("serialize");
    text.push('\n');
    text
}

#[test]
fn committed_file_matches_a_fresh_generation() {
    let on_disk = std::fs::read_to_string(COMMITTED)
        .expect("proto/test-vectors/frame-v2.vectors.json must exist");
    let fresh = regenerate();
    if on_disk != fresh {
        let at = on_disk
            .bytes()
            .zip(fresh.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| on_disk.len().min(fresh.len()));
        panic!(
            "the committed vector file differs from a fresh generation at byte {at} \
             (committed {} bytes, fresh {} bytes).\n\
             If you edited the file by hand: don't — edit the generator and regenerate.\n\
             If you changed the generator: run\n    \
             cargo run -p media-vector-gen --bin generate-frame-vectors",
            on_disk.len(),
            fresh.len()
        );
    }
}

#[test]
fn generation_is_deterministic() {
    assert_eq!(
        regenerate(),
        regenerate(),
        "two generations in one process differ: something here is reading a clock, \
         a random source, or an unordered map, and the drift guard would red on \
         every unrelated devloop"
    );
}
