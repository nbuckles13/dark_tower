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
// media-only `*_span!` open shape, which requires a literal `_` in the name.
// Deleting the enumerated entry on the theory that "the open shape covers it"
// is the mistake this fixture exists to red.
//
// (Stated as the PROPERTY rather than as a copy of `OPEN_SPAN_RE`'s pattern:
// the anchor is widened from time to time — it gained `\s*` before the `!` on
// 2026-09-08 — and a quoted regex here goes stale silently every time, which
// is the read-as-accurate failure this whole fixture suite is about. The
// requires-a-literal-`_` property is what the argument depends on, and it
// survives anchor changes.)
//
// WHY SPAN IS DENIED AT ALL — read before trimming this family as scope creep.
// §11's enforcement sentence names log and metric macros and `event!`, not
// spans, so this file looks like an extension. It is not: §11's retention
// bullet names "media-path logs, metric labels, or span attributes" directly,
// so on the leak axis span IS enumerated.
//
// On the cost axis, stated accurately because the trimmer who raises it is
// the reader who knows `tracing`: DISABLED, the field expressions are NOT
// evaluated — `valueset_all!` sits inside the enabled arm — but you still pay
// `level_enabled!`, `__CALLSITE.interest()` and, when interest is
// `sometimes`, `Dispatch::current()`. That interest/dispatch check is exactly
// the "registry lookup" §11 forbids per frame. ENABLED, you pay the valueset
// evaluation, the allocation and the recorded fields, per frame. And which of
// the two you get is a subscriber configuration this guard cannot see, which
// is §11's "A log level is not an acceptable gate" verbatim. Full argument in
// the `media_telemetry_deny` module doc, §"SPAN and #[instrument]".
