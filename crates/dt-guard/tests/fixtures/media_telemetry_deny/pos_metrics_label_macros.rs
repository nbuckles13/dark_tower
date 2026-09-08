//! Label-bearing `metrics` macros inside the media path.
//!
//! These are the Cat B forms from `metric_macros::MacroKind` — `counter!`,
//! `gauge!`, `histogram!`. They must red REGARDLESS of whether the family list
//! is ever hand-edited, because the family is derived from
//! `metric_macros::MacroKind::ALL`.

use crate::media::forwarder::ConnectionForwarder;

pub fn forward_one(forwarder: &ConnectionForwarder, payload_length: usize) {
    counter!("mh_media_frames_forwarded_total").increment(1);
    gauge!("mh_media_egress_queue_depth").set(forwarder.depth() as f64);
    histogram!("mh_media_forward_latency_seconds").record(0.001_f64);
    metrics::counter!("mh_media_frames_dropped_total").increment(1);
    let _ = payload_length;
}

// Invariant: FOUR hits, all `media-telemetry-deny-macro-in-media-path`. Three
// bare forms — one per label-bearing `MacroKind` variant — plus one
// `metrics::`-qualified form, which must NOT evade: `DENIED_MACRO_RE` opens
// with a bare `\b` and no anchored qualifier group, and `\b` matches between
// `:` and `c`. (Named rather than re-spelled — the anchor's delimiter and
// whitespace classes were widened on 2026-09-08 and a quoted copy here would
// now be stale. The `\b` is the part this fixture depends on.)
//
// Three bare forms is complete, not short: `counter!`, `gauge!` and
// `histogram!` are the entire Cat B set. A fourth would duplicate a variant
// already covered and would be padding.
//
// If a future `MacroKind` variant lands and this count is unchanged, that is
// correct — this file pins the CURRENT label-bearing variants. The derivation
// from `MacroKind::ALL` is pinned by a unit test, not here.
//
// CORRECTED 2026-09-07: this block previously claimed five hits and "four bare
// forms" against a body holding three. Two numbers, one body, both wrong, in a
// fixture written by the reviewer enforcing the two-encodings rule on everyone
// else. Caught by the walker disagreeing and the disagreement being routed
// back as a question instead of the expectation being quietly adjusted — which
// is the only reason the arithmetic was ever checked against the body at all.
