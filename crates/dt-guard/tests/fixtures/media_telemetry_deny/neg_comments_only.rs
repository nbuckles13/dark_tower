//! Denied forms appearing ONLY in comments. Must be green.
//!
//! The two lines below are copied VERBATIM from the real media directory. This
//! is the fixture that decides whether Layer 3 is red for the whole team on the
//! guard's first run, and it is easy to miss because both halves of ADR-0036's
//! coverage frame are about positives.

/// formatting would happen in a *sibling* — a `debug!(?frame)` in
//! `#[instrument]`. Observation goes through handles resolved once at setup
// A trailing case: println!("x") mentioned after code on the same line.
/* A block comment containing counter!("m").increment(1) and event!(Level::INFO, "x"). */

pub fn on_frame(payload_length: usize) -> usize {
    let doc = "see `#[tracing::instrument]` and `info_span!(\"x\")`";
    let _ = doc; // and a trailing comment with eprintln!("y")
    payload_length
}

// Invariant: ZERO hits.
//
// The two verbatim lines break for DIFFERENT reasons and a fixture covering
// only one reads as coverage for both:
//
//   * `crates/mh-service/src/media/forward.rs:97` — `debug!(?frame)` carries a
//     real `!(`, so it matches a CORRECTLY-shaped invocation anchor exactly.
//     This is the proof that the invocation anchor ALONE is insufficient and
//     comment stripping is mandatory, not a nicety. (The argument is about the
//     anchor's existence, not its exact spelling, which has since widened.)
//   * `crates/mh-service/src/media/mod.rs:16` — `#[instrument]` in prose. The
//     attribute matcher has no paren to anchor on at all, so it breaks even for
//     someone who reasoned "the backticks and the missing paren will save us."
//
// All four exclusion classes appear: line (`//`), doc (`///`, `//!`), block
// (`/* */`), and string literal. `instrument_skip_all::is_comment_line` is a
// line-PREFIX check and covers neither the trailing-comment nor the block case.
