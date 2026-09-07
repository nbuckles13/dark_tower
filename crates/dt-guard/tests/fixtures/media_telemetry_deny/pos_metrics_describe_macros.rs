//! Describe-family `metrics` macros inside the media path.
//!
//! `describe_*!` macros are Cat A — literal name + description, no runtime
//! labels — so they carry no PII risk of their own. They are denied anyway,
//! because ADR-0036 §11's invariant is "no metric macro is reachable from the
//! forward function": the cost being avoided is the registry lookup and the
//! allocation, not the label content. A describe call in the hot path is the
//! same per-frame cost as a counter call.

pub fn describe_media_metrics() {
    describe_counter!("mh_media_frames_forwarded_total", "Frames forwarded");
    describe_gauge!("mh_media_egress_queue_depth", "Egress queue depth");
    describe_histogram!("mh_media_forward_latency_seconds", "Forward latency");
    metrics::describe_counter!("mh_media_frames_dropped_total", "Frames dropped");
}

// Invariant: four hits, all `media-telemetry-deny-macro-in-media-path`.
// Longest-alternative ordering matters: `describe_counter!` must be reported
// as `describe_counter`, NOT as `counter` with a `describe_` prefix. The
// derivation inherits `MacroKind::ALL`'s describe-first ordering.
