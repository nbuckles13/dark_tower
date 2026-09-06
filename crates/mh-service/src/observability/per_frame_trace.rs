//! Development-only per-frame media tracing (ADR-0036 §11).
//!
//! # A SIBLING of the hot path, and it has to be
//!
//! This module holds the only `tracing` macro on the media path. It cannot live
//! under `crate::media`: §11 requires that directory to hold the hot path and
//! nothing else, so the directory boundary and the hot-path boundary are the
//! same boundary and a directory-scoped deny scopes exactly. `media/` calls
//! [`record`] — a plain function — and this module decides whether that call
//! expands to anything at all.
//!
//! # What it may emit, and what it must never emit
//!
//! **Per-frame SIZE and a bounded outcome token. Nothing else.** Never payload
//! bytes, never the `SFrame` key id (`payload()`'s first 8 bytes), never
//! wrapped key material, and never a participant, stream or meeting identity.
//!
//! Per-frame size **is** the sensitive item under §11 — with discontinuous
//! transmission, the time-ordered sequence of per-frame sizes for one stream is
//! the voice-activity trace — and this facility legitimately emits it. That is
//! the entire reason it is gated at compile time rather than by a log level:
//! §11 rules a level out in terms, because "the incident motivating a level
//! change is the same incident producing the sensitive trace", and enabling
//! debug logging on a pod is one routine action away from a fleet-wide
//! voice-activity trace entering the shipping pipeline. A cargo feature that is
//! a `compile_error!` in a release build has nothing to notice and nothing to
//! toggle at runtime.
//!
//! The `compile_error!` gate itself, and the reasoning behind its predicate,
//! are at `crate`'s root — see `crates/mh-service/src/lib.rs`.

/// Record one frame's size and outcome, if the facility is compiled in.
///
/// `outcome` must be a bounded `&'static str` — a drop token or a forward
/// marker — never a formatted value.
#[cfg(feature = "per-frame-trace")]
pub fn record(frame_bytes: usize, outcome: &'static str) {
    tracing::trace!(
        target: "mh.media.per_frame",
        frame_bytes,
        outcome,
        "media frame"
    );
}

/// The compiled-out form: an inlined no-op, so the call sites under
/// `crate::media` cost nothing and need no `#[cfg]` of their own.
///
/// Keeping the `#[cfg]` here rather than at each call site is deliberate: a
/// hot-path file carrying `#[cfg(feature = "per-frame-trace")]` blocks would
/// put the *decision* inside the directory that is supposed to contain only
/// forwarding, and the next author would reasonably add a `tracing` import
/// beside it.
#[cfg(not(feature = "per-frame-trace"))]
#[inline(always)]
pub fn record(_frame_bytes: usize, _outcome: &'static str) {}

/// The bounded outcome tokens this facility emits for a frame that was
/// forwarded. Drop outcomes reuse the `reason` vocabulary
/// (`MediaDropReason::as_str` / `RejectReason::as_str`) rather than inventing a
/// third spelling.
pub const FORWARDED: &str = "forwarded";
