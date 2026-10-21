//! The chunk-fed frame-header reader for cut-through stream forwarding
//! (ADR-0036 §1/§2, R-21).
//!
//! A video group travels on one QUIC stream as a sequence of frames, and a
//! media handler forwards each frame **as it arrives** rather than after the
//! whole frame is buffered. It therefore needs the header — and only the
//! header — before the payload exists. [`HeaderReader`] accumulates the header
//! from arbitrarily split chunks into a fixed, inline
//! [`MAX_HEADER_BYTES`] buffer and reports the parsed [`FrameHeader`] plus how
//! many bytes of the current chunk it used; everything after that in the chunk
//! is the frame's tail (payload, then signature), which the caller forwards
//! untouched.
//!
//! # A pure state machine
//!
//! No I/O, no clock, no allocation: the only storage is the inline header
//! buffer. Bounds on buffered time and on streams per connection, and counting
//! a give-up, are the caller's (see the obligations in [`crate::frame`]).
//!
//! # One parser
//!
//! The reader never reads a header field itself. After every step it hands its
//! buffer to [`crate::codec::parse_header_prefix`] — the crate's single layout
//! parser — and copies **exactly** the `needed` bytes that parse asks for, so:
//!
//! * it can never consume past the end of the header (the tail stays with the
//!   caller);
//! * every terminal check the parser makes from the fixed prefix —
//!   `unknown_version`, `reserved_flag_bit_set`, `key_bearing_discardable`,
//!   `payload_length_exceeds_max` — fires before a single byte beyond the
//!   prefix is requested, and nothing is ever sized from `payload_length`;
//! * the outcome cannot depend on how the input was split, because the parser
//!   sees the same bytes either way (asserted by `tests/header_prefix.rs` and
//!   the `header_reader_split` fuzz target).
//!
//! An over-maximum declared length on a stream reuses
//! `payload_length_exceeds_max`; there is no reader-specific reject token.

use crate::codec::{parse_header_prefix, DecodeError, HeaderPrefix, Region};
use crate::frame::{FrameHeader, MAX_HEADER_BYTES};
use core::fmt;

/// What one [`HeaderReader::feed`] call produced.
///
/// Carries **no bytes** — only the scalar [`FrameHeader`] and a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feed {
    /// The chunk was consumed entirely and the header is still incomplete.
    /// Not a rejection.
    NeedMore,
    /// The header is complete and valid.
    Complete {
        /// The parsed header.
        header: FrameHeader,
        /// How many bytes **of this chunk** were consumed into the header. The
        /// rest of the chunk, `chunk[consumed..]`, is the start of the frame's
        /// tail ([`FrameHeader::tail_len`] bytes in total).
        consumed: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Reading,
    Complete(FrameHeader),
    Failed(DecodeError),
}

/// Incrementally reads one frame header from arbitrary chunk splits.
///
/// # Lifecycle
///
/// * [`Self::feed`] chunks until it returns [`Feed::Complete`] (or `Err`).
/// * After `Complete`, further `feed` calls are **sticky**: they return the
///   same `Complete` with `consumed: 0` and never start a new header. Drain the
///   tail, then call [`Self::reset`] for the next frame on the same stream.
/// * After `Err`, further `feed` calls return the **same error** until
///   [`Self::reset`]. A rejected stream's framing is desynchronised; the caller
///   resets the stream rather than the reader.
///
/// `Debug` is hand-rolled and prints only the fill level and the state name:
/// the buffer holds the wrapped transmit key of a key-bearing frame.
#[derive(Clone)]
pub struct HeaderReader {
    buf: [u8; MAX_HEADER_BYTES],
    filled: usize,
    state: State,
}

impl HeaderReader {
    /// A reader positioned at the start of a frame.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [0; MAX_HEADER_BYTES],
            filled: 0,
            state: State::Reading,
        }
    }

    /// Feed the next chunk of the stream.
    ///
    /// Consumes at most the remainder of the header from `chunk`. Never
    /// allocates.
    ///
    /// # Errors
    ///
    /// Returns the terminal [`DecodeError`] the single parser reports; it is
    /// sticky until [`Self::reset`].
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Feed, DecodeError> {
        match self.state {
            State::Failed(error) => return Err(error),
            State::Complete(header) => {
                return Ok(Feed::Complete {
                    header,
                    consumed: 0,
                })
            }
            State::Reading => {}
        }
        let mut consumed = 0usize;
        loop {
            let buffered = self.buf.get(..self.filled).unwrap_or(&[]);
            match parse_header_prefix(buffered) {
                Err(error) => {
                    self.state = State::Failed(error);
                    return Err(error);
                }
                Ok(HeaderPrefix::Complete(header)) => {
                    self.state = State::Complete(header);
                    return Ok(Feed::Complete { header, consumed });
                }
                Ok(HeaderPrefix::Incomplete { needed }) => {
                    let rest = chunk.get(consumed..).unwrap_or(&[]);
                    if rest.is_empty() {
                        return Ok(Feed::NeedMore);
                    }
                    let take = needed.min(rest.len());
                    // The parser decides within MAX_HEADER_BYTES, so this
                    // range is always in bounds; a miss would be a parser
                    // defect, and it fails closed (as `truncated`) rather than
                    // panicking. No input reaches this branch — see
                    // `ProducibleBy::CompleteHeaderRequired`.
                    let end = self.filled.saturating_add(take);
                    let (Some(dst), Some(src)) =
                        (self.buf.get_mut(self.filled..end), rest.get(..take))
                    else {
                        let error = DecodeError::Truncated {
                            region: Region::RelayRegion,
                            needed: MAX_HEADER_BYTES,
                            available: self.filled,
                        };
                        self.state = State::Failed(error);
                        return Err(error);
                    };
                    dst.copy_from_slice(src);
                    self.filled = end;
                    consumed = consumed.saturating_add(take);
                }
            }
        }
    }

    /// Whether a header is partly buffered: at least one byte read and the
    /// header neither complete nor rejected. A stream that ends while this is
    /// true ended mid-frame (MH counts that as a partial-frame discard); one
    /// that ends while it is false ended cleanly at a frame boundary.
    #[must_use]
    pub const fn is_mid_header(&self) -> bool {
        matches!(self.state, State::Reading) && self.filled > 0
    }

    /// Header bytes buffered so far.
    #[must_use]
    pub const fn buffered_len(&self) -> usize {
        self.filled
    }

    /// The parsed header, once complete.
    #[must_use]
    pub const fn header(&self) -> Option<FrameHeader> {
        match self.state {
            State::Complete(header) => Some(header),
            State::Reading | State::Failed(_) => None,
        }
    }

    /// The complete header's bytes — publisher region and relay region,
    /// exactly [`FrameHeader::header_len`] of them — once complete.
    ///
    /// For forwarding: copy them into a per-subscriber buffer and call
    /// [`crate::codec::rewrite_relay_region_header`] on the copy. These bytes
    /// include the wrapped transmit key of a key-bearing frame; relay them
    /// opaquely and never format them.
    #[must_use]
    pub fn header_bytes(&self) -> Option<&[u8]> {
        match self.state {
            State::Complete(header) => self.buf.get(..header.header_len()),
            State::Reading | State::Failed(_) => None,
        }
    }

    /// Return to the start-of-frame state, clearing the buffer (including any
    /// wrapped-key bytes it held) and any sticky outcome.
    pub fn reset(&mut self) {
        self.buf = [0; MAX_HEADER_BYTES];
        self.filled = 0;
        self.state = State::Reading;
    }
}

impl Default for HeaderReader {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for HeaderReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = match self.state {
            State::Reading => "reading",
            State::Complete(_) => "complete",
            State::Failed(error) => error.reason().as_str(),
        };
        f.debug_struct("HeaderReader")
            .field("filled", &self.filled)
            .field("state", &state)
            .finish_non_exhaustive()
    }
}
