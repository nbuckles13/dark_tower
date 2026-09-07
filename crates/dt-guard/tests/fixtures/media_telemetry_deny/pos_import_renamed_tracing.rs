//! An aliased telemetry import inside the media path.
//!
//! `use tracing::info as note;` + `note!(...)` walks straight through a
//! macro-NAME deny list — the invocation matcher fires on membership, and
//! `note` is not a member. This is why the family includes an import deny at
//! all, and it encodes an invariant media-handler already wrote down:
//! `crates/mh-service/src/media/mod.rs` states "no `tracing` or `log` import".

use tracing::info as note;
use tracing as t;
pub use log::error as complain;

pub fn on_frame(payload_length: usize) {
    note!(payload_length, "frame in");
    complain!("drop");
    t::warn!("slow");
}

// Invariant: three `media-telemetry-deny-telemetry-crate-import` hits (the two
// `use` lines plus the `pub use` re-export) and one
// `media-telemetry-deny-macro-in-media-path` for `t::warn!` = four.
//
// `note!(...)` and `complain!(...)` are NOT caught by the invocation matcher —
// the deny list has no such members, and being `!`-anchored buys nothing on its
// own. The import deny is the only thing standing between this file and green.
// A `pub use` in a SIBLING, reached as `crate::obs::note!(...)`, is caught by
// neither matcher; that residual is stated in the module doc and is the one
// gap only an allowlist of permitted macro names could close.
