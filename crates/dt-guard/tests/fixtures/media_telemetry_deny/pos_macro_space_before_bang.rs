//! Whitespace between the macro name and its `!`, and the non-paren
//! delimiters. Both are legal Rust; both were one-keystroke evasions.
//!
//! Rust tokenises a macro call as `path ! delim`, so whitespace between the
//! name and the `!` is permitted and `info !("x")` compiles and logs. Until
//! 2026-09-08 the anchor here required the `!` to follow the name
//! IMMEDIATELY, so every line in the first block below walked straight
//! through a deny that reported clean. It was found by auditing the guard
//! against the grammar rather than against its own tests — which is why the
//! catalog now pins it, and not only the unit tests.
//!
//! The brace and bracket forms were already denied in code; they were pinned
//! by unit tests and by **no fixture at all**. That distinction matters: the
//! fixture catalog is the artifact a later reader treats as the coverage
//! inventory, so a class living only in unit tests reads as uncovered by
//! whoever audits this next.

pub fn on_frame(sender: u64, payload_length: usize) {
    // Whitespace before `!`. One space, and one that stands in for what
    // `blank_non_code` hands the matcher for `tracing::info /*x*/ !(...)`.
    info !("frame forwarded");
    tracing::warn  !(sender, "slow subscriber");
    counter !("mh_media_frames_forwarded_total");
    custom_span !("mh.media.forward");

    // Non-paren delimiters, already denied in code, now pinned as data.
    println!{"len={}", payload_length}
    info!["frame in", sender];
}

// Invariant: SIX `media-telemetry-deny-macro-in-media-path` hits.
//
// Four from the whitespace block — `info`, `tracing::warn`, `counter`,
// `custom_span` — and two from the delimiter block, `println` and the second
// `info`. The `tracing::` and bare spellings both appear because the `\b`
// with no anchored qualifier group is what makes one pattern cover both; a
// widening that accidentally required a qualifier would still pass a
// bare-only fixture.
//
// `custom_span !(` is here for a specific reason and must not be dropped as
// redundant with the three spellings above it. It exercises OPEN_SPAN_RE,
// which is a SECOND anchor with the identical shape. Widening DENIED_MACRO_RE
// alone would leave `custom_span !(` passing while `custom_span!(` is denied
// — an asymmetry worse than the uniform gap was, because the module doc would
// then read as if the open-span shape were covered. This line is what reds if
// someone fixes one anchor and not the other.
//
// The negatives that bound the widening live in the unit test
// `media_telemetry_deny::tests::space_widening_does_not_over_match` (five
// cases) — NOT in `neg_bare_identifiers.rs`, which carries bare-identifier
// negatives of a different class. The two worth knowing: `my_info !(x)` must
// not fire (the `\b` still holds), and `if error != (x)` must not fire (the
// character after `!` must be an open delimiter, so `!=` cannot match).
//
// THE WIDENING DOES CROSS A LINE, DELIBERATELY — do not "fix" this.
// `\s*` never crosses a TOKEN, which is the bound that matters. But `\s`
// includes `\n`, and `check_file` runs the matcher over the whole
// `blank_file` output rather than line by line, so
//
//     info
//         !("x");
//
// is one finding. That is correct: it is a real macro call that compiles and
// logs, and in a fail-loud deny the direction is more denial. It is pinned by
// `space_widening_spans_lines_deliberately` so a later reader cannot quietly
// re-narrow it. Note the ASYMMETRY with the sibling anchors: `telemetry_macros`'
// consumers (`rust_pii`, `rust_log_secrets`) iterate `content.lines()`, so
// there `\s*` genuinely cannot cross a line. Same character class, different
// reach, because the consumers differ. (@security S2.)
