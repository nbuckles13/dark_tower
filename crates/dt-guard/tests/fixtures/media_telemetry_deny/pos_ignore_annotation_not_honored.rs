//! A denied macro carrying a `guard:ignore` annotation, inside the media path.
//!
//! This guard honours NO suppression annotation. ADR-0036's move throughout is
//! "prefer structural impossibility over a control that has to notice", and an
//! annotation escape hatch is a control that has to notice. In practice a
//! suppression on a control like this gets applied by whoever is inconvenienced,
//! at the moment of inconvenience, with no review.

pub fn on_frame(payload_length: usize) {
    // guard:ignore(hot-path telemetry needed for the incident, will remove) — this
    // justification is well-formed, cites a reason, and is STILL not honoured.
    counter!("mh_media_frames_forwarded_total").increment(1);

    // guard:ignore(temporary)
    println!("len={payload_length}");
}

// Invariant: two hits, `media-telemetry-deny-macro-in-media-path`. The
// annotations change nothing.
//
// This fixture is the MECHANISM for "no bypass exists", replacing a sentence of
// prose in the module doc. Without it, "we do not honour ignores" is an
// assertion about code nobody wrote — the same class of claim as an alert whose
// selector matches no pod.
