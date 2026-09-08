//! A locally-defined `*_span!` wrapper inside the media path.
//!
//! This is the evasion the media-only `*_span!` open shape (`OPEN_SPAN_RE`)
//! exists to close — it matches any name ending in a literal `_span`, which is
//! the property that matters here and which survives the anchor widenings the
//! spelling does not. It is the span-side twin of `pos_import_renamed_tracing.rs`: a
//! sibling module defines a wrapper whose name is outside the enumerated six,
//! and the enumerated list alone never sees it.

pub fn on_frame() {
    let _guard = custom_span!("mh.media.forward").entered();
    let _other = frame_span!("mh.media.frame").entered();
}

// Invariant: two hits, `media-telemetry-deny-macro-in-media-path`.
//
// This rule is media-only and deliberately NOT promoted to the shared home:
// as a DENY inside one small directory an open shape is a free widening, but
// as shared vocabulary it would hand `rust_pii` false positives on every
// `*_span!` in the tree. Same membership, different blast radius — which is
// why group membership and consumer selection are separate decisions.
