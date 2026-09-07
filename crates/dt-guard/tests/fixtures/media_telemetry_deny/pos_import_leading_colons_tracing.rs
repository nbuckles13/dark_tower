//! The leading-`::` telemetry import inside the media path.
//!
//! In Rust 2018+ a leading `::` forces resolution from the extern prelude, so
//! `::tracing` is the MOST unambiguous spelling of the external crate —
//! strictly more so than bare `tracing`, which a local `mod tracing` could
//! shadow. An anchor that skipped a leading `::` as "not a crate root" would
//! let the one spelling that cannot resolve to anything local walk through.

use ::tracing::info as note;
use ::log::error;
use ::metrics::counter;
pub(crate) use ::tracing_subscriber::fmt as subscriber_fmt;

// Invariant: four hits, all `media-telemetry-deny-telemetry-crate-import`.
//
// The anchor is KEYWORD-based, not punctuation-based: skip an optional
// `pub`/`pub(crate)` qualifier and an optional leading `::`, then deny when the
// first identifier segment is `tracing`/`log`/`metrics`/`tracing_subscriber`;
// exclude only when that first segment is the keyword `crate`, `self` or
// `super`. The keyword list is closed and small; the punctuation list is not.
