//! The demonstration that the allow is BY CONSTRUCTION and not by
//! subtraction. This is the fixture `neg_cached_handles.rs` points at.
//!
//! ADR-0036 §11 allows cached-handle calls and denies macro forms. The module
//! doc claims that allow is satisfied structurally — `.record(x)` cannot match
//! `record!(` under a `!`-anchored matcher — and that a line-level filter
//! dropping lines containing `.increment(` would be a MASKING bug, because it
//! would suppress a real violation that happened to share a line with a handle
//! call (CLAUDE.md §Fail loudly).
//!
//! That claim was asserted in three places and demonstrated in none. The
//! allow-fixture holds five handle calls and zero macro invocations, so **a
//! hypothetical line filter passes it identically** — it cannot distinguish
//! the property it was said to prove.
//!
//! One physical line carrying BOTH is the only shape that can.

use crate::observability::metrics::MediaMetricHandles;

pub struct ConnectionForwarder {
    handles: MediaMetricHandles,
}

impl ConnectionForwarder {
    pub fn on_frame(&self) {
        self.handles.frames_forwarded.increment(1); counter!("mh_media_frames_forwarded_total", 1);
    }
}

// Invariant: exactly ONE `media-telemetry-deny-macro-in-media-path` hit, with
// spelling `counter`.
//
// One, not two: the `.increment(1)` on the same physical line must NOT be
// flagged, because `.increment(` structurally cannot match `<name>!` followed
// by a delimiter. And not zero: the `counter!(` must still fire despite the
// allowed handle call sharing its line.
//
// Both halves are load-bearing and they fail in opposite directions:
//   - If a future edit added a line filter dropping lines that contain an
//     allowed handle method, this fixture reds at ZERO hits — the masking bug,
//     caught. `neg_cached_handles.rs` would not notice.
//   - If the anchor were ever relaxed to match method calls, this fixture reds
//     at TWO — §11's "it bans the pattern it exists to enforce", caught.
//
// The single-line layout is the whole point and must survive formatting. If a
// future reader splits it across two lines to satisfy a style preference, the
// fixture silently stops testing anything and becomes a duplicate of
// pos_metrics_label_macros.rs. Note these fixtures are in no cargo target, so
// `cargo fmt` never sees this file — the risk is a human, not the formatter.
