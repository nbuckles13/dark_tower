//! The `event!` macro inside the media path.
//!
//! ADR-0036 §11: "Deny the event macro too, since level macros expand to it."
//! A level macro that got past the level list would still land here, so this
//! is the backstop for the whole tracing family rather than one more entry.

use tracing::Level;

pub fn on_frame(payload_length: usize) {
    event!(Level::TRACE, payload_length, "frame received");
    tracing::event!(Level::DEBUG, size = payload_length, "frame sized");
}

// Invariant: two hits, `media-telemetry-deny-macro-in-media-path`, plus one
// `media-telemetry-deny-telemetry-crate-import` for the `use tracing::Level`
// line — `tracing` is a denied crate root and `Level` being a type rather than
// a macro does not exempt it. Three hits total.
//
// The import hit is deliberate and is the point: the payload_length argument
// here is exactly ADR-0036 §11's "time-ordered sequence of sizes for a single
// stream is the voice-activity trace".
