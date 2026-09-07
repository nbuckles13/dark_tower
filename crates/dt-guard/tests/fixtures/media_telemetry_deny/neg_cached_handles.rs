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
// The allow-list is satisfied BY CONSTRUCTION, not by a runtime filter: under
// the mandatory `!\s*\(` anchor, `.record(x)` structurally cannot match
// `record!(`. A line-level filter that dropped lines containing `.record(`
// would suppress a real `counter!(...)` co-located with a handle call — a
// masked failure, and the ADR's "touching handle methods" in one move. This
// fixture is the proof; there is no allow-list in the code to inspect.
