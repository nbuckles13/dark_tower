//! The five `*_span!` constructors inside the media path.

pub fn on_frame() {
    let _a = trace_span!("mh.media.a").entered();
    let _b = debug_span!("mh.media.b").entered();
    let _c = info_span!("mh.media.c").entered();
    let _d = warn_span!("mh.media.d").entered();
    let _e = error_span!("mh.media.e").entered();
}

// Invariant: five hits, all `media-telemetry-deny-macro-in-media-path`.
//
// These five plus bare `span!` are the complete public span-macro surface of
// `tracing`, which is why the promoted SPAN group is enumerated rather than
// shape-matched: an enumerated group is the only membership a SHARED canonical
// home can honestly export to consumers like `rust_pii`, which must not
// inherit false positives on arbitrary `*_span!` spellings.
