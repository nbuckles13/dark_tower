//! The pattern the guard exists to ENFORCE. Must be green.
//!
//! ADR-0036 §11: "allow cached-handle record and increment calls — the guard
//! denies macro forms and must not touch handle methods, or it bans the
//! pattern it exists to enforce."

use crate::observability::metrics::{MediaDropReason, MediaMetricHandles};

pub struct ConnectionForwarder {
    handles: MediaMetricHandles,
}

impl ConnectionForwarder {
    pub fn on_frame(&self, elapsed_secs: f64, depth: f64, reason: MediaDropReason) {
        self.handles.frames_forwarded.increment(1);
        self.handles.frames_total.absolute(4_096);
        self.handles.forward_latency.record(elapsed_secs);
        self.handles.egress_queue_depth.set(depth);
        self.handles.egress_queue_depth.decrement(1.0);
        self.handles.dropped(reason).increment(1);
    }
}

// Invariant: ZERO hits.
//
// All five allowed methods appear: `.increment`, `.record`, `.set`,
// `.absolute`, `.decrement` — the exhaustive `metrics` handle surface
// (`Counter::{increment, absolute}`, `Gauge::{set, increment, decrement}`,
// `Histogram::{record}`). There is no sixth method to miss.
//
// What THIS fixture shows, exactly: the five allowed handle-method spellings
// appear in realistic media-path shapes and none of them is flagged.
//
// What it does NOT show, stated so the gap cannot be read as coverage: that
// the allow is satisfied BY CONSTRUCTION rather than by a runtime line filter.
// This body contains zero macro invocations, so a hypothetical filter that
// dropped every line containing `.increment(` would pass it identically. The
// claim needs a line carrying BOTH a handle call and a real denied macro, and
// that lives at `pos_handle_call_colocated_with_macro.rs` — one physical line,
// exactly one finding, spelling `counter`.
//
// The distinction is worth keeping rather than collapsing: a line filter would
// suppress a real `counter!(...)` that happened to share a line with a handle
// call, which is a masked failure (CLAUDE.md §Fail loudly) and is §11's
// "touching handle methods" in one move. There is no allow-list in the code to
// inspect — `ALLOWED_HANDLE_METHODS` is documentation and a test, never a
// runtime subtraction — so the co-location fixture is the only way to observe
// the difference from outside.
//
// CORRECTED 2026-09-08 (@observability): this block previously ended "This
// fixture is the proof." It was not. The most load-bearing property on the
// allow side had prose in three places — here, the module doc, and
// `cached_handle_methods_are_never_flagged` — and coverage in none.
