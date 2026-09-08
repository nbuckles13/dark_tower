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
//
// WHY `#[instrument]` IS DENIED — it is the worst of the family per frame,
// precisely because it is invisible at the call site. It wraps every call in
// span construction plus enter/exit and, absent `skip`/`skip_all`, records
// EVERY function argument as a field. On a forward function those arguments
// are the connection identity, the stream and the frame — §11's per-frame,
// per-participant value set entering span attributes by default rather than
// by mistake. Not scope creep: §11's own retention bullet names span
// attributes. Full argument in the `media_telemetry_deny` module doc,
// §"SPAN and #[instrument]".
