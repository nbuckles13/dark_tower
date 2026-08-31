//! Dark Tower media frame wire format, version 2 (ADR-0036 §2 and Appendix).
//!
//! A binary codec for the frames carried between clients and media handlers.
//! The header splits into a **publisher region**, authenticated end-to-end, and
//! a **relay region** that a media handler rewrites per subscriber and that
//! nobody authenticates.
//!
//! * [`frame`] — field sizes, offsets, flags, and the zero-copy decoded view.
//! * [`extensions`] — the publisher-set TLV region and its registry.
//! * [`codec`] — the parser, the four entry points, and the reject vocabulary.
//!
//! # What this crate does not do
//!
//! It holds no keys and performs no cryptography: it neither signs, verifies,
//! encrypts, nor decrypts. The payload and the wrapped transmit key are opaque
//! byte spans, and the signed and associated-data ranges are handed out as
//! slices of the received buffer for a caller to feed to a verifier.
//!
//! It also emits no telemetry. There is no `tracing` dependency, so a log or
//! metric macro here is a compile error rather than a convention; errors return
//! and the caller counts them by [`codec::RejectReason`].

#![warn(clippy::pedantic)]
// Invariant 3 of ADR-0036 §2, enforced rather than commented: any field packed
// into a narrower wire width must use a checked conversion sized to the field,
// never a masking cast and never a bare shift. A truncating `as` cast on this
// path is a build failure.
#![deny(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
#![forbid(unsafe_code)]

pub mod codec;
pub mod extensions;
pub mod frame;
