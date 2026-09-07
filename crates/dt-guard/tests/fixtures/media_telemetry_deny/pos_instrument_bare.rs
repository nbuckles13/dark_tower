//! The bare `#[instrument]` attribute inside the media path.

#[instrument]
pub fn forward_one(payload_length: usize) -> usize {
    payload_length
}

#[instrument(skip_all, name = "mh.media.forward")]
pub fn forward_two(payload_length: usize) -> usize {
    payload_length
}

// Invariant: two hits, `media-telemetry-deny-macro-in-media-path`.
//
// `skip_all` does NOT exempt the second one. `skip_all` is the argument-capture
// discipline enforced by the `instrument-skip-all` guard, and it is orthogonal
// to this deny: §11's invariant is that no span is CONSTRUCTED per frame at
// all, because the construction is the cost. A `skip_all` span is a cheap span,
// not an absent one.
