//! The bare `span!` macro inside the media path.

use tracing::Level;

pub fn on_frame() {
    let _guard = span!(Level::TRACE, "mh.media.forward").entered();
    let _other = tracing::span!(Level::DEBUG, "mh.media.egress").entered();
}

// Invariant: two `media-telemetry-deny-macro-in-media-path` hits plus one
// `media-telemetry-deny-telemetry-crate-import` = three.
//
// Bare `span!` needs its own enumerated entry and is NOT reachable by the
// media-only open shape `\b\w+_span!\s*\(`, which requires a literal `_`.
// Deleting the enumerated entry on the theory that "the open shape covers it"
// is the mistake this fixture exists to red.
