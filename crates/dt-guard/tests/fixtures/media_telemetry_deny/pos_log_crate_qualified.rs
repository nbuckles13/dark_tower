//! The `log` crate's own macro surface inside the media path.

use log::error;

pub fn on_drop(reason: &str) {
    log!(log::Level::Warn, "generic log macro: {reason}");
    error!("drop: {reason}");
    ::log::info!("leading-colons spelling");
}

// Invariant: three `media-telemetry-deny-macro-in-media-path` hits plus one
// `media-telemetry-deny-telemetry-crate-import` for `use log::error;` = four.
//
// `log!` is the generic form and is why LOG_CRATE is a group rather than an
// alias of LEVEL: the five level names overlap, but `log!` has no tracing
// counterpart and would be missed by a LEVEL-only list.
