//! Ingress denial-of-service caps, enforced **before** any per-frame work.
//!
//! ADR-0036 §11 names three: "maximum frame payload before any allocation;
//! per-connection concurrent-stream and creation-rate limits enforced before
//! per-frame work". Their status on the datagram path, stated because two of
//! the three are not this module's to enforce and a reader will otherwise
//! assume they are:
//!
//! | Cap | Where |
//! |---|---|
//! | Maximum frame payload before any allocation | **Here**, [`check_datagram_size`], wired in `crate::media::ingress::run_ingress` before anything is allocated on our behalf |
//! | Per-connection **concurrent** stream limit | A declared QUIC transport parameter, `MH_MAX_CONCURRENT_UNI_STREAMS` (`crate::config`) — enforced by quinn, not by MH code |
//! | Per-connection stream **creation-rate** limit | **NOT ENFORCED, AND NOT ENFORCEABLE TODAY.** See below. |
//!
//! # The creation-rate cap has no enforcement point in a datagram-only path
//!
//! MH opens no unidirectional-stream accept loop — `crate::config` and
//! `crate::webtransport::server` both record that, and audio is one frame per
//! datagram (ADR-0036 §1). There is therefore **no stream-creation event to
//! rate-limit**, and a limiter here would be a mechanism with no read and no
//! write on either side.
//!
//! A fixed-window limiter was built and then **removed at review**: it shipped
//! as unreachable enforcement machinery while the metrics catalog described its
//! drop token as a live detector, which is a control that reads as present and
//! cannot fire. Only the vocabulary is kept — see
//! `MediaDropReason::StreamRateLimited`, marked unreachable in the same style
//! as `PartialFrameDiscard` — so the author of the uni-stream accept path
//! inherits the token rather than inventing a second spelling for it, and
//! inherits no bound they did not choose.
//!
//! The one check that remains is a pure predicate returning a bounded
//! [`MediaDropReason`]; nothing here emits, and nothing here allocates.

use crate::observability::metrics::MediaDropReason;
use media_protocol::frame::MAX_FRAME_BYTES;

/// Reject a datagram whose **byte length** exceeds the wire-format frame
/// maximum, before any parse and before anything is allocated on our behalf.
///
/// The bound is `media_protocol::frame::MAX_FRAME_BYTES` — derived in the codec
/// from the header, payload and signature sizes, never recomputed here. A whole
/// datagram carries one complete frame (ADR-0036 §1, one frame per datagram),
/// so the whole-frame maximum is the right bound; `MAX_PAYLOAD_BYTES` would
/// reject legal frames by the size of their own header.
///
/// This is deliberately **not** the codec's `payload_length_exceeds_max`. That
/// token means "the declared `payload_length` *field* exceeded the max during
/// header validation" and is identical to the client's condition, so MH must
/// use the shared spelling there. This one is a pre-parse whole-datagram cap
/// and gets MH's own token. Same constant, two checks, two tokens.
///
/// # Errors
///
/// [`MediaDropReason::OversizeDatagram`] when `len` exceeds the bound.
pub const fn check_datagram_size(len: usize) -> Result<(), MediaDropReason> {
    if len > MAX_FRAME_BYTES {
        return Err(MediaDropReason::OversizeDatagram);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datagram_at_the_bound_is_admitted_and_one_byte_over_is_not() {
        // Written against the derived constant, never a literal: the bound
        // moves with the header, and a test carrying its own number would keep
        // passing against the old wire format.
        assert!(check_datagram_size(MAX_FRAME_BYTES).is_ok());
        assert_eq!(
            check_datagram_size(MAX_FRAME_BYTES + 1),
            Err(MediaDropReason::OversizeDatagram)
        );
        assert!(check_datagram_size(0).is_ok());
    }
}
