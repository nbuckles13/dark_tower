//! The fully-qualified `#[tracing::instrument]` spelling inside the media path.
//!
//! This is the realistic spelling, not the exotic one: it is live in-tree at
//! `crates/mh-service/src/webtransport/connection.rs:130` and
//! `crates/mh-service/src/session/mod.rs:1009` — the immediate SIBLINGS of the
//! denied directory, i.e. exactly the code a refactor would move inward.

#[tracing::instrument(skip_all, name = "mh.media.forward")]
pub fn forward_one(payload_length: usize) -> usize {
    payload_length
}

#[ tracing :: instrument ]
pub fn forward_two(payload_length: usize) -> usize {
    payload_length
}

// Invariant: two hits, `media-telemetry-deny-macro-in-media-path`.
//
// The attribute matcher must be `#\[\s*(?:tracing\s*::\s*)?instrument\b`. The
// three pre-existing `#\[instrument` regexes in the crate
// (`instrument_skip_all.rs`, `rust_pii.rs`, `rust_log_secrets.rs`) are all the
// NARROW form and would miss both of these. The second function pins the
// whitespace tolerance — rustfmt will not produce it, a human might.
