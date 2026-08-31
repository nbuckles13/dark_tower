//! Encoding and decoding of version-2 media frames.
//!
//! # One parser, four entry points
//!
//! Everything here is built on a single private layout parser. There is
//! deliberately no second parse of these bytes: two parsers agree on every
//! well-formed input and diverge on exactly the malformed ones an attacker
//! constructs, and for the relay-rewrite path that divergence would mean a
//! media handler writing six bytes at an offset the receiver does not read
//! them from — a frame malformed only from the receiver's point of view, and
//! invisible to every counter the handler owns.
//!
//! | Entry point | Shape | Use |
//! |---|---|---|
//! | [`decode_datagram`] | exact | one frame per datagram (§1); a trailing byte is a rejection |
//! | [`decode_stream_frame`] | prefix | frames share a group's stream (§1); `Ok(None)` means "need more bytes" |
//! | [`peek_frame_len`] | prefix | reader-side pre-allocation bound |
//! | [`rewrite_relay_region`] | in place | the relay's per-subscriber rewrite |
//!
//! # Incomplete is not a rejection
//!
//! [`decode_stream_frame`] returns `Ok(None)` when the buffer is a strict
//! prefix of a possibly-valid frame. That outcome carries **no reject reason**
//! and must not be counted on a drop counter: on a stream it is the ordinary
//! consequence of a frame spanning a read boundary. `Err` means *no extension
//! of this buffer can decode*.
//!
//! The rule is decidable rather than heuristic:
//!
//! * **Any shortfall** of available bytes against a required region length —
//!   fixed or declared — is **incomplete** on the stream path.
//! * **Any violation of a limit or of the grammar** is **terminal** on both
//!   paths: unknown version, an undefined flag bit, a payload length over
//!   [`MAX_PAYLOAD_BYTES`], an extension length over
//!   [`crate::frame::MAX_EXT_BYTES`], or a malformed TLV region that is fully
//!   present.
//!
//! Consequently `truncated`, `payload_length_exceeds_available` and
//! `trailing_bytes` **can never occur on the stream path**. Each
//! [`RejectReason`] records this as [`RejectReason::producible_by`].
//!
//! **`Ok(None)` obligates the caller.** See the obligation stated in
//! [`crate::frame`]: bound both buffered bytes per stream and
//! time-to-completion, and count the give-up. A peer that opens a stream and
//! stalls mid-frame is otherwise silent by construction.
//!
//! # Consumer recovery, by reason class
//!
//! * On a datagram, every rejection means **drop this frame** and continue.
//! * On a stream, a grammar or limit violation means **reset this stream**: the
//!   framing is desynchronized and cannot be resynced, because the next frame's
//!   start is only knowable from a header this one invalidated.
//! * **Never tear down the connection.** One malformed frame from one publisher
//!   is not a connection-level fault.
//!
//! # No telemetry originates here
//!
//! This crate has no `tracing` dependency, so a log or metric macro in it is a
//! compile error rather than a rule to remember. Errors return; the caller
//! counts. Only [`RejectReason::as_str`] is safe as a media-path log field or
//! metric label — it is a bounded, closed set of `&'static str`. `Display` on
//! these errors carries scalars for local diagnosis and must not become a
//! per-frame dimension: per ADR-0036 §11 the time-ordered sequence of per-frame
//! sizes for one stream is the voice-activity trace.

use crate::extensions::{encode_into, Extension, ExtensionError, Extensions};
use crate::frame::{
    FrameFlags, Layout, MediaFrameView, WrappedTransmitKey, AEAD_TAG_BYTES, EXT_LENGTH_FIELD_SIZE,
    FLAGS_OFFSET, KEK_GENERATION_FIELD_BYTES, MAX_EXT_BYTES, MAX_PAYLOAD_BYTES,
    PAYLOAD_LENGTH_OFFSET, PROTOCOL_VERSION, PUBLISHER_FIXED_PREFIX_SIZE, RELAY_REGION_SIZE,
    SIGNATURE_SIZE, STREAM_ID_FIELD_BYTES, STREAM_SEQUENCE_OFFSET, VERSION_OFFSET,
    WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES, WRAPPED_TRANSMIT_KEY_SIZE,
};
use bytes::{BufMut, Bytes, BytesMut};
use core::fmt;

// ---------------------------------------------------------------------------
// Reject reasons
// ---------------------------------------------------------------------------

/// Which **entry points** can produce a reject reason.
///
/// **This is an entry-point classification, not a media classification.** An
/// earlier version of this type said "audio is carried in datagrams and video
/// in streams, so this is also which media a reason can be observed on". That
/// was false and failed in the mis-triage direction: [`rewrite_relay_region`]
/// is a fourth fallible entry point, and a media handler calls it on
/// **stream-carried video** while re-framing. It converts a shortfall through
/// the same path [`decode_datagram`] uses, so it emits `truncated` and
/// `payload_length_exceeds_available` on the video path — which the old
/// `DatagramOnly` name told an operator was impossible.
///
/// Use [`ProducibleBy::reachable_on_stream_carried_frames`] rather than
/// inferring media from a variant name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProducibleBy {
    /// Every entry point: [`decode_datagram`], [`decode_stream_frame`],
    /// [`peek_frame_len`] and [`rewrite_relay_region`]. These are the terminal
    /// limit and grammar violations, which no additional bytes can rescue.
    AllEntryPoints,
    /// The two entry points that require a **complete** frame:
    /// [`decode_datagram`] and [`rewrite_relay_region`]. The prefix-tolerant
    /// entry points report the same condition as the non-reject `Ok(None)`.
    CompleteFrameRequired,
    /// [`decode_datagram`] alone.
    DecodeDatagramOnly,
}

impl ProducibleBy {
    /// Stable text for documentation and catalogs: the entry points, named.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllEntryPoints => {
                "decode_datagram, decode_stream_frame, peek_frame_len, rewrite_relay_region"
            }
            Self::CompleteFrameRequired => "decode_datagram, rewrite_relay_region",
            Self::DecodeDatagramOnly => "decode_datagram",
        }
    }

    /// Whether this reason can be observed while handling **stream-carried**
    /// (video) frames.
    ///
    /// True unless the reason is reachable only from [`decode_datagram`],
    /// because [`rewrite_relay_region`] runs on video frames during re-framing.
    /// This is the accessor an alert selector or a runbook should consult; do
    /// not infer media from an entry-point name.
    #[must_use]
    pub const fn reachable_on_stream_carried_frames(self) -> bool {
        match self {
            Self::AllEntryPoints | Self::CompleteFrameRequired => true,
            Self::DecodeDatagramOnly => false,
        }
    }
}

/// Define the reject vocabulary once: the enum, its wire tokens, and the
/// exhaustive list, all from a single source.
///
/// This exists so that **omitting a variant from
/// [`ALL_REJECT_REASONS`] is unrepresentable** rather than merely tested for.
/// A hand-maintained list compiles clean when a new variant is added and only
/// the wildcard-free `match` arms force an update — so the *list* silently
/// stays short, and every consumer that enumerates it (the cross-language
/// vectors, the guard fixtures, the metric catalog) agrees with the others and
/// disagrees with the codec, while a drift guard reports clean.
macro_rules! reject_reasons {
    ($( $(#[$meta:meta])* $variant:ident => $token:literal ),+ $(,)?) => {
        /// The cross-language reject-reason vocabulary for this codec.
        ///
        /// These are the **structural / parse** subset of a shared `reason`
        /// label space. Signature, decrypt, replay, `no_kek_for_generation` and
        /// `no_roster_entry` belong to the cryptography and key layers and are
        /// added by those layers. **No downstream layer may reuse one of these
        /// tokens with a different meaning.**
        ///
        /// ANCHOR (DRY): the cross-language single source of truth is
        /// `proto/test-vectors/frame-v2.vectors.json` (story task 8); both
        /// codecs assert against it rather than each hardcoding the strings.
        ///
        /// Vector precedence: vectors beat prose, but a vector contradicting a
        /// recorded ruling is a defect to escalate rather than conform to — see
        /// the note at [`crate::extensions::EXT_REGISTRY`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum RejectReason {
            $( $(#[$meta])* $variant ),+
        }

        /// Every reject reason, for consumers that must enumerate the closed
        /// set.
        ///
        /// Deliberately `&[RejectReason]` rather than `&[&str]`: a string list
        /// would give each wire token two spelling homes. Generated alongside
        /// the enum, so it cannot omit a variant. Consumers call
        /// [`RejectReason::as_str`].
        pub const ALL_REJECT_REASONS: &[RejectReason] = &[ $( RejectReason::$variant ),+ ];

        impl RejectReason {
            /// The wire/metric token.
            ///
            /// Paired with the variant at its single definition site, so a
            /// variant rename cannot silently rename a wire token and a new
            /// variant cannot exist without a token.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $token ),+ }
            }
        }
    };
}

reject_reasons! {
    /// The version byte is not [`PROTOCOL_VERSION`].
    UnknownVersion => "unknown_version",
    /// The flag byte sets a bit this version does not define.
    ReservedFlagBitSet => "reserved_flag_bit_set",
    /// The declared payload length exceeds [`MAX_PAYLOAD_BYTES`].
    PayloadLengthExceedsMax => "payload_length_exceeds_max",
    /// The declared payload length exceeds the bytes actually present.
    PayloadLengthExceedsAvailable => "payload_length_exceeds_available",
    /// The frame ends before a required fixed-size region.
    Truncated => "truncated",
    /// The declared extension length exceeds the registry-derived maximum.
    ExtensionsTooLarge => "extensions_too_large",
    /// The extension region is present in full but violates the grammar.
    ExtensionsMalformed => "extensions_malformed",
    /// Bytes follow the signature in a datagram.
    TrailingBytes => "trailing_bytes",
}

impl RejectReason {
    /// Which entry points can produce this reason.
    #[must_use]
    pub const fn producible_by(self) -> ProducibleBy {
        match self {
            Self::UnknownVersion
            | Self::ReservedFlagBitSet
            | Self::PayloadLengthExceedsMax
            | Self::ExtensionsTooLarge
            | Self::ExtensionsMalformed => ProducibleBy::AllEntryPoints,
            // Not `DatagramOnly`: `rewrite_relay_region` maps a shortfall
            // through the same path, and a media handler calls it on
            // stream-carried video.
            Self::PayloadLengthExceedsAvailable | Self::Truncated => {
                ProducibleBy::CompleteFrameRequired
            }
            Self::TrailingBytes => ProducibleBy::DecodeDatagramOnly,
        }
    }

    /// What an operator should investigate. One sentence per reason, written
    /// for a triage ladder rather than for a parser author.
    #[must_use]
    pub const fn operator_meaning(self) -> &'static str {
        match self {
            Self::UnknownVersion => {
                "A sender is emitting a header version this build does not implement. \
                 The version is meeting-wide and MC-directed (§2), so suspect a sender \
                 build mismatch or a misdirected frame rather than a negotiation failure."
            }
            Self::ReservedFlagBitSet => {
                "A sender set a flag bit this version does not define. A sender build \
                 emitting new flags without a version bump, or corrupt input."
            }
            Self::PayloadLengthExceedsMax => {
                "A frame declared a payload larger than the wire format permits. This is \
                 a fixed wire-format constant, not a capacity limit: investigate the \
                 sender, do not raise it."
            }
            Self::PayloadLengthExceedsAvailable => {
                "Two distinct causes, opposite teams. From `decode_datagram`: a datagram \
                 declared more payload than it carried — suspect a sender framing defect or \
                 truncation upstream of the receiver. From `rewrite_relay_region`: the relay \
                 was handed a buffer it had not finished assembling — a caller-side \
                 re-framing defect on the handler's own side, with no sender involvement. \
                 Fork on which entry point emitted it before triaging."
            }
            Self::Truncated => {
                "Two distinct causes, opposite teams. From `decode_datagram`: a datagram \
                 ended before a required fixed-size region — suspect truncation on the send \
                 path or a sender emitting short frames. From `rewrite_relay_region`: the \
                 relay was handed an incomplete frame to rewrite — a caller-side re-framing \
                 defect in the handler's stream reader, not a sender fault. Fork on which \
                 entry point emitted it before triaging."
            }
            Self::ExtensionsTooLarge => {
                "The declared extension region is larger than the registry could ever \
                 produce. Structurally impossible for a well-formed sender, therefore \
                 hostile or corrupt; detected before the TLV walk, so no per-byte work \
                 is done on it. Investigate the sender's build and registry version; \
                 never raise the bound, which is derived rather than chosen."
            }
            Self::ExtensionsMalformed => {
                "The extension region is within the size the registry permits but \
                 violates the grammar: TLV walk overruns, unknown type, duplicate type, \
                 wrong value length for the type, a value outside the type's declared \
                 accepted set, non-ascending type order, or a zero-length entry. \
                 Usually a sender encoder defect or a sender built against a different \
                 registry revision. Investigate that sender's encoder and the registry \
                 revision it was built against."
            }
            Self::TrailingBytes => {
                "Bytes follow a well-formed frame's signature in a datagram. QUIC \
                 preserves datagram boundaries, so this is never a transport artifact: \
                 some endpoint wrote them, and the signature covers neither. If the \
                 frame otherwise verifies, the appender sits downstream of the \
                 publisher and this is a security escalation; if it does not, suspect a \
                 sender build emitting new trailing fields without a version bump."
            }
        }
    }
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The region a truncation occurred in, for local diagnosis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// The version byte.
    Version,
    /// The always-present publisher prefix.
    PublisherPrefix,
    /// The wrapped-transmit-key field.
    WrappedTransmitKey,
    /// The extension-length field.
    ExtensionLength,
    /// The TLV extension region.
    Extensions,
    /// The relay region.
    RelayRegion,
    /// The payload.
    Payload,
    /// The trailing signature.
    Signature,
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Version => "version",
            Self::PublisherPrefix => "publisher prefix",
            Self::WrappedTransmitKey => "wrapped transmit key",
            Self::ExtensionLength => "extension length",
            Self::Extensions => "extensions",
            Self::RelayRegion => "relay region",
            Self::Payload => "payload",
            Self::Signature => "signature",
        };
        f.write_str(text)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A terminal decode rejection.
///
/// `Display` carries bounded scalars only — never payload bytes, key bytes,
/// the raw extension buffer, or the key id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The version byte is not the version this codec implements.
    #[error("unsupported frame version {version}; this codec implements version {expected}")]
    UnknownVersion {
        /// The version byte observed.
        version: u8,
        /// The version this codec implements.
        expected: u8,
    },
    /// The flag byte sets undefined bits.
    #[error("flag byte 0x{flags:02x} sets undefined bits 0x{undefined:02x}")]
    ReservedFlagBitSet {
        /// The whole flag byte.
        flags: u8,
        /// Just the undefined bits.
        undefined: u8,
    },
    /// The declared payload length exceeds the wire-format maximum.
    #[error("declared payload length {declared} exceeds the wire-format maximum {max}")]
    PayloadLengthExceedsMax {
        /// Length the frame declared.
        declared: usize,
        /// The wire-format maximum.
        max: usize,
    },
    /// The declared payload length exceeds what the buffer contains.
    #[error("declared payload length {declared} exceeds the {available} payload bytes present")]
    PayloadLengthExceedsAvailable {
        /// Length the frame declared.
        declared: usize,
        /// Payload bytes actually present once the signature is reserved.
        available: usize,
    },
    /// The frame ends before a required region.
    #[error("frame truncated in the {region}: {needed} bytes required, {available} present")]
    Truncated {
        /// Where the shortfall occurred.
        region: Region,
        /// Bytes the region requires.
        needed: usize,
        /// Bytes actually present.
        available: usize,
    },
    /// The declared extension length exceeds the registry-derived maximum.
    #[error("declared extension length {declared} exceeds the registry-derived maximum {max}")]
    ExtensionsTooLarge {
        /// Length the frame declared.
        declared: usize,
        /// The registry-derived maximum.
        max: usize,
    },
    /// The extension region violates the grammar.
    #[error("malformed extensions: {0}")]
    ExtensionsMalformed(#[from] ExtensionError),
    /// Bytes follow the signature in a datagram.
    #[error("{extra} byte(s) follow the frame's signature in a datagram")]
    TrailingBytes {
        /// How many bytes trail the frame.
        extra: usize,
    },
}

impl DecodeError {
    /// The bounded reject reason for this error. Use this, never a copied
    /// literal, as a metric label or a test assertion.
    #[must_use]
    pub const fn reason(&self) -> RejectReason {
        match self {
            Self::UnknownVersion { .. } => RejectReason::UnknownVersion,
            Self::ReservedFlagBitSet { .. } => RejectReason::ReservedFlagBitSet,
            Self::PayloadLengthExceedsMax { .. } => RejectReason::PayloadLengthExceedsMax,
            Self::PayloadLengthExceedsAvailable { .. } => {
                RejectReason::PayloadLengthExceedsAvailable
            }
            Self::Truncated { .. } => RejectReason::Truncated,
            Self::ExtensionsTooLarge { .. } => RejectReason::ExtensionsTooLarge,
            Self::ExtensionsMalformed(_) => RejectReason::ExtensionsMalformed,
            Self::TrailingBytes { .. } => RejectReason::TrailingBytes,
        }
    }
}

/// An encode rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    /// The key-bearing flag disagrees with the presence of the field it
    /// announces. The flag *is* the presence signal, so the two cannot differ.
    #[error("key-bearing flag is {flag_set} but a wrapped transmit key is {field_present}")]
    KeyBearingFlagMismatch {
        /// Whether the flag was set.
        flag_set: bool,
        /// Whether the field was supplied.
        field_present: bool,
    },
    /// The payload exceeds the wire-format maximum.
    #[error("payload of {len} bytes exceeds the wire-format maximum {max}")]
    PayloadTooLarge {
        /// The payload length offered.
        len: usize,
        /// The wire-format maximum.
        max: usize,
    },
    /// The extensions violate the grammar.
    #[error("invalid extensions: {0}")]
    Extensions(#[from] ExtensionError),
}

// ---------------------------------------------------------------------------
// The single layout parser
// ---------------------------------------------------------------------------

/// A shortfall: the buffer is a strict prefix of a possibly-valid frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shortfall {
    region: Region,
    needed: usize,
    available: usize,
}

impl Shortfall {
    /// Map a shortfall onto the datagram path's terminal error.
    ///
    /// A payload shortfall gets its own reason; every other region reports
    /// `truncated`.
    const fn into_datagram_error(self, declared_payload: usize) -> DecodeError {
        match self.region {
            Region::Payload => DecodeError::PayloadLengthExceedsAvailable {
                declared: declared_payload,
                available: self.available,
            },
            _ => DecodeError::Truncated {
                region: self.region,
                needed: self.needed,
                available: self.available,
            },
        }
    }
}

enum HeaderOutcome {
    Complete(Layout),
    Incomplete(Shortfall),
}

fn read_u16_be(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let bytes: [u8; 2] = data.get(offset..end)?.try_into().ok()?;
    Some(u16::from_be_bytes(bytes))
}

fn read_u32_be(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes: [u8; 4] = data.get(offset..end)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

/// Require `needed` bytes to be available from `offset`, or report a shortfall.
fn require(data: &[u8], offset: usize, needed: usize, region: Region) -> Result<usize, Shortfall> {
    let end = offset.checked_add(needed).ok_or(Shortfall {
        region,
        needed,
        available: data.len().saturating_sub(offset),
    })?;
    if data.len() < end {
        return Err(Shortfall {
            region,
            needed,
            available: data.len().saturating_sub(offset),
        });
    }
    Ok(end)
}

/// Parse the header through the relay region. **This is the crate's only
/// parser**, and all four public entry points are built on it.
///
/// # Keep it one function
///
/// An earlier revision split the leading fixed prefix into a private helper to
/// satisfy `clippy::too_many_lines`. That helper had one call site and a
/// rustdoc rule forbidding a second — but a documented rule is strictly weaker
/// than a structural guarantee, and the single-parser property is what stops a
/// media handler writing the relay region at an offset the receiver does not
/// read it from.
///
/// The split had a subtler cost too: the resume offset after the prefix was
/// *recomputed* in the caller rather than returned by the helper, so two places
/// independently asserted where the publisher region continues. A differential
/// is not only two functions disagreeing about a byte's *contents* — it is
/// equally two functions disagreeing about *where the next field starts*.
///
/// Merging removed that surface rather than documenting it. The merge also
/// deleted the wrapper's plumbing, which briefly put the function back under
/// the line threshold; recording `wrapped_key_offset` from the cursor — the
/// same finding one layer down, in `build_view` — pushed it over again. The
/// expectation below is therefore taken deliberately, exactly as the earlier
/// revision of this comment said it should be: **prefer suppressing the lint
/// over splitting the function.** Length here is a linear, step-numbered
/// sequence, not nesting; the single-parser property is worth more than
/// conformance to an arbitrary line count.
///
/// # Invariant 3 (recorded where narrowing would occur)
///
/// Every wire-derived length is accumulated in `usize` via `checked_add`, never
/// in the field's own width, and every narrowing uses a checked conversion
/// sized to the field — never a masking cast, never a bare shift. A `u32`
/// accumulator would be the sharp version here: `payload_length` at
/// `u32::MAX`, plus the wrapped key, the signature and the extension region
/// wraps to a *small* number in release mode and passes a naive bounds check.
/// The header's own field types already match the wire widths, so this is
/// defensive against a future widening; the crate-level
/// `deny(clippy::cast_possible_truncation, …)` makes it a build failure rather
/// than a convention.
#[expect(
    clippy::too_many_lines,
    reason = "Single-parser invariant (ADR-0036 §2, relay-region rewrite): this function is the \
              crate's only parser and the only authority on where each field starts. Splitting it \
              reintroduces a second place that decides that question — the defect this codec has \
              already had twice, once as a prefix helper recomputing the resume offset and once as \
              `build_view` recomputing the wrapped-key offset. A private helper with a \
              do-not-call-twice rustdoc was tried and rejected as a documented rule standing in \
              for a structural guarantee. The walk is linear and step-numbered; the length is \
              legibility, not complexity."
)]
fn parse_layout(data: &[u8]) -> Result<HeaderOutcome, DecodeError> {
    // 1. Version byte, before anything else: that is why it sits at offset 0.
    let Some(&version) = data.get(VERSION_OFFSET) else {
        return Ok(HeaderOutcome::Incomplete(Shortfall {
            region: Region::Version,
            needed: 1,
            available: 0,
        }));
    };
    // 2. Unknown version is terminal even on a one-byte buffer.
    if version != PROTOCOL_VERSION {
        return Err(DecodeError::UnknownVersion {
            version,
            expected: PROTOCOL_VERSION,
        });
    }

    // 3. The always-present publisher prefix.
    let short_prefix = Shortfall {
        region: Region::PublisherPrefix,
        needed: PUBLISHER_FIXED_PREFIX_SIZE,
        available: data.len(),
    };
    if data.len() < PUBLISHER_FIXED_PREFIX_SIZE {
        return Ok(HeaderOutcome::Incomplete(short_prefix));
    }

    // 4. Flags: any undefined bit is a rejection. No masking.
    let Some(&flag_bits) = data.get(FLAGS_OFFSET) else {
        return Ok(HeaderOutcome::Incomplete(short_prefix));
    };
    let Some(flags) = FrameFlags::from_u8_checked(flag_bits) else {
        return Err(DecodeError::ReservedFlagBitSet {
            flags: flag_bits,
            undefined: FrameFlags::undefined_bits(flag_bits),
        });
    };

    // 5. Payload length, bounded before anything is sized from it.
    let Some(declared_payload) = read_u32_be(data, PAYLOAD_LENGTH_OFFSET) else {
        return Ok(HeaderOutcome::Incomplete(short_prefix));
    };
    let payload_length =
        usize::try_from(declared_payload).map_err(|_| DecodeError::PayloadLengthExceedsMax {
            declared: usize::MAX,
            max: MAX_PAYLOAD_BYTES,
        })?;
    if payload_length > MAX_PAYLOAD_BYTES {
        return Err(DecodeError::PayloadLengthExceedsMax {
            declared: payload_length,
            max: MAX_PAYLOAD_BYTES,
        });
    }

    // 6. Stream sequence: the AEAD nonce input, readable before decryption.
    let Some(stream_sequence) = read_u32_be(data, STREAM_SEQUENCE_OFFSET) else {
        return Ok(HeaderOutcome::Incomplete(short_prefix));
    };

    // The cursor is advanced from here on and never recomputed. Every
    // subsequent offset is derived from it, so there is exactly one answer to
    // "where does the next field start".
    let mut cursor = PUBLISHER_FIXED_PREFIX_SIZE;

    // 7. Wrapped transmit key, present iff the key-bearing flag is set. Its
    //    offset is *recorded from the cursor*, never recomputed downstream.
    let mut wrapped_key_offset = None;
    if flags.key_bearing {
        wrapped_key_offset = Some(cursor);
        cursor = match require(
            data,
            cursor,
            WRAPPED_TRANSMIT_KEY_SIZE,
            Region::WrappedTransmitKey,
        ) {
            Ok(end) => end,
            Err(shortfall) => return Ok(HeaderOutcome::Incomplete(shortfall)),
        };
    }

    // 8. Extension length.
    let ext_len_offset = cursor;
    cursor = match require(data, cursor, EXT_LENGTH_FIELD_SIZE, Region::ExtensionLength) {
        Ok(end) => end,
        Err(shortfall) => return Ok(HeaderOutcome::Incomplete(shortfall)),
    };
    let Some(ext_len_u16) = read_u16_be(data, ext_len_offset) else {
        return Ok(HeaderOutcome::Incomplete(Shortfall {
            region: Region::ExtensionLength,
            needed: EXT_LENGTH_FIELD_SIZE,
            available: data.len().saturating_sub(ext_len_offset),
        }));
    };
    let ext_len = usize::from(ext_len_u16);

    // 9. Limit before availability: an over-large declaration is terminal even
    //    on a short buffer, because no extension of the buffer can rescue it.
    if ext_len > MAX_EXT_BYTES {
        return Err(DecodeError::ExtensionsTooLarge {
            declared: ext_len,
            max: MAX_EXT_BYTES,
        });
    }

    // 10. The extension region, then the grammar walk over a fully-present one.
    //     A region shorter than the declared length is a *shortfall*, not a
    //     grammar violation — so on the stream path it is `Ok(None)`, never a
    //     rejection. Conflating the two would kill any video frame whose read
    //     boundary lands inside the extension region.
    let ext_start = cursor;
    cursor = match require(data, cursor, ext_len, Region::Extensions) {
        Ok(end) => end,
        Err(shortfall) => return Ok(HeaderOutcome::Incomplete(shortfall)),
    };
    let ext_bytes = data.get(ext_start..cursor).ok_or(DecodeError::Truncated {
        region: Region::Extensions,
        needed: ext_len,
        available: data.len().saturating_sub(ext_start),
    })?;
    Extensions::validate(ext_bytes)?;

    // 11. The relay region.
    match require(data, cursor, RELAY_REGION_SIZE, Region::RelayRegion) {
        Ok(_) => {}
        Err(shortfall) => return Ok(HeaderOutcome::Incomplete(shortfall)),
    }

    Ok(HeaderOutcome::Complete(Layout {
        flags,
        payload_length,
        stream_sequence,
        wrapped_key_offset,
        ext_start,
        ext_len,
    }))
}

/// Check that the payload and signature are present, given a parsed header.
fn require_body(data: &[u8], layout: Layout) -> Result<(), Shortfall> {
    let payload_start = layout.payload_start();
    let remaining = data.len().saturating_sub(payload_start);
    if remaining < SIGNATURE_SIZE {
        return Err(Shortfall {
            region: Region::Signature,
            needed: SIGNATURE_SIZE,
            available: remaining,
        });
    }
    let available_payload = remaining.saturating_sub(SIGNATURE_SIZE);
    if layout.payload_length > available_payload {
        return Err(Shortfall {
            region: Region::Payload,
            needed: layout.payload_length,
            available: available_payload,
        });
    }
    Ok(())
}

/// Build the borrowed view over a frame whose header and body are both present.
fn build_view(data: &[u8], layout: Layout) -> Result<MediaFrameView<'_>, DecodeError> {
    let frame_len = layout.frame_len();
    let frame = data.get(..frame_len).ok_or(DecodeError::Truncated {
        region: Region::Signature,
        needed: frame_len,
        available: data.len(),
    })?;

    let publisher_region =
        frame
            .get(..layout.publisher_region_end())
            .ok_or(DecodeError::Truncated {
                region: Region::RelayRegion,
                needed: layout.publisher_region_end(),
                available: frame.len(),
            })?;

    // The offset comes from the layout the parser walked to; it is never
    // recomputed here. See `Layout::wrapped_key_offset` for why an absolute
    // recomputation would be a latent differential rather than an obvious one.
    let wrapped = if let Some(base) = layout.wrapped_key_offset {
        let generation = read_u16_be(frame, base).ok_or(DecodeError::Truncated {
            region: Region::WrappedTransmitKey,
            needed: WRAPPED_TRANSMIT_KEY_SIZE,
            available: frame.len().saturating_sub(base),
        })?;
        // `checked_add`, not `saturating_add`: these offsets cannot overflow
        // (fixed addends over an offset bounded by a validated frame length),
        // but a fail-closed parser is worth auditing with a single greppable
        // idiom — and a lone `saturating_add` here is what a future maintainer
        // copies into a wire-derived context, where it IS the hazard.
        let short_key = DecodeError::Truncated {
            region: Region::WrappedTransmitKey,
            needed: WRAPPED_TRANSMIT_KEY_SIZE,
            available: frame.len().saturating_sub(base),
        };
        let key_start = base
            .checked_add(KEK_GENERATION_FIELD_BYTES)
            .ok_or(short_key)?;
        let key_end = key_start
            .checked_add(WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES)
            .ok_or(short_key)?;
        let tag_end = key_end.checked_add(AEAD_TAG_BYTES).ok_or(short_key)?;
        let key: &[u8; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES] = frame
            .get(key_start..key_end)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(DecodeError::Truncated {
                region: Region::WrappedTransmitKey,
                needed: WRAPPED_TRANSMIT_KEY_SIZE,
                available: frame.len().saturating_sub(base),
            })?;
        let tag: &[u8; AEAD_TAG_BYTES] = frame
            .get(key_end..tag_end)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(DecodeError::Truncated {
                region: Region::WrappedTransmitKey,
                needed: WRAPPED_TRANSMIT_KEY_SIZE,
                available: frame.len().saturating_sub(base),
            })?;
        Some(WrappedTransmitKey::new(generation, key, tag))
    } else {
        None
    };

    let ext_bytes = frame
        .get(layout.ext_start..layout.publisher_region_end())
        .ok_or(DecodeError::Truncated {
            region: Region::Extensions,
            needed: layout.ext_len,
            available: frame.len().saturating_sub(layout.ext_start),
        })?;
    // DO NOT "OPTIMISE AWAY" THIS SECOND VALIDATION. It is load-bearing.
    //
    // `parse_layout` already validated these exact bytes, so a profiler will
    // show `validate` called twice on one region and the obvious win is a
    // `from_validated` constructor that skips the check. Do not add one: it
    // would be a permanent path for unvalidated bytes into a type whose entire
    // contract is that iteration cannot fail. Every consumer of
    // `Extensions::iter` relies on that contract to skip error handling, so a
    // bypass constructor does not risk one bad call — it invalidates the
    // invariant every downstream caller is built on.
    //
    // The redundancy exists because `Layout` is `Copy` and carries no borrow,
    // so the validated `Extensions<'a>` cannot ride along with it. `validate`
    // is pure over the same bytes, so the two calls cannot diverge, and the
    // region is at most `MAX_EXT_BYTES` (3) bytes.
    let extensions = Extensions::validate(ext_bytes)?;

    let relay = layout.relay_region_offset();
    let short_relay = DecodeError::Truncated {
        region: Region::RelayRegion,
        needed: RELAY_REGION_SIZE,
        available: frame.len().saturating_sub(relay),
    };
    let stream_id = read_u16_be(frame, relay).ok_or(short_relay)?;
    let hop_offset = relay
        .checked_add(STREAM_ID_FIELD_BYTES)
        .ok_or(short_relay)?;
    let hop_sequence = read_u32_be(frame, hop_offset).ok_or(short_relay)?;

    let payload = frame
        .get(layout.payload_start()..layout.signature_start())
        .ok_or(DecodeError::PayloadLengthExceedsAvailable {
            declared: layout.payload_length,
            available: frame.len().saturating_sub(layout.payload_start()),
        })?;

    let signature: &[u8; SIGNATURE_SIZE] = frame
        .get(layout.signature_start()..frame_len)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(DecodeError::Truncated {
            region: Region::Signature,
            needed: SIGNATURE_SIZE,
            available: frame.len().saturating_sub(layout.signature_start()),
        })?;

    Ok(MediaFrameView::new(
        frame,
        layout,
        wrapped,
        extensions,
        stream_id,
        hop_sequence,
        publisher_region,
        payload,
        signature,
    ))
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Decode one frame from a datagram, which must contain exactly one frame.
///
/// Audio is carried one frame per datagram (§1), so any byte after the
/// signature is a rejection rather than slack: the signature covers the
/// publisher region and the payload, so a tail is covered by **nobody**, and
/// ignoring it would rebuild the reserved-byte covert channel §2 removes.
///
/// # Errors
///
/// Returns [`DecodeError`]; use [`DecodeError::reason`] for the bounded token.
pub fn decode_datagram(data: &[u8]) -> Result<MediaFrameView<'_>, DecodeError> {
    let layout = match parse_layout(data)? {
        HeaderOutcome::Complete(layout) => layout,
        HeaderOutcome::Incomplete(shortfall) => {
            return Err(shortfall.into_datagram_error(0));
        }
    };
    if let Err(shortfall) = require_body(data, layout) {
        return Err(shortfall.into_datagram_error(layout.payload_length));
    }
    let frame_len = layout.frame_len();
    let extra = data.len().saturating_sub(frame_len);
    if extra > 0 {
        return Err(DecodeError::TrailingBytes { extra });
    }
    build_view(data, layout)
}

/// Decode the frame at the start of `data`, which may hold further frames.
///
/// Returns `Ok(None)` when `data` is a strict prefix of a possibly-valid
/// frame. That is **not** a rejection: see the module docs, including the
/// caller obligation it carries. Use [`MediaFrameView::encoded_len`] to
/// advance to the next frame.
///
/// # Errors
///
/// Returns [`DecodeError`] only when no extension of `data` could decode.
pub fn decode_stream_frame(data: &[u8]) -> Result<Option<MediaFrameView<'_>>, DecodeError> {
    let layout = match parse_layout(data)? {
        HeaderOutcome::Complete(layout) => layout,
        HeaderOutcome::Incomplete(_) => return Ok(None),
    };
    if require_body(data, layout).is_err() {
        return Ok(None);
    }
    build_view(data, layout).map(Some)
}

/// Total on-wire length of the frame starting at `data`, once knowable.
///
/// This is the **reader-side pre-allocation boundary**. Call it before
/// reserving buffer space: it enforces [`MAX_PAYLOAD_BYTES`] on the declared
/// length, which is the trust boundary ADR-0036 §2 describes as passing a fuzz
/// test of the decode function while being wrong in the reader that calls it.
/// Decode itself is zero-copy and never allocates, so this is where the bound
/// earns its keep.
///
/// Returns `Ok(None)` while more header bytes are needed; at most
/// [`crate::frame::MAX_HEADER_BYTES`] are ever required to answer.
///
/// # Errors
///
/// Returns [`DecodeError`] for a terminal violation.
pub fn peek_frame_len(data: &[u8]) -> Result<Option<usize>, DecodeError> {
    match parse_layout(data)? {
        HeaderOutcome::Complete(layout) => Ok(Some(layout.frame_len())),
        HeaderOutcome::Incomplete(_) => Ok(None),
    }
}

/// Rewrite the relay region of `frame` in place.
///
/// This is the relay's per-subscriber rewrite (§2): six bytes at an offset
/// **derived from the frame's own header**, never precomputed. The offset is
/// not a constant — it moves with the key-bearing flag and the extension
/// length, both of which a publisher controls — so a hardcoded offset would let
/// one participant's extension byte cause the relay to overwrite three bytes of
/// another participant's *signed* publisher region, failing verification at
/// every receiver.
///
/// The full header is validated to the same standard as decode before any byte
/// is written: a relay must not write into a frame it has not validated.
/// Neither the publisher region nor the signature is touched, because the relay
/// region is excluded from both.
///
/// `frame` may be longer than one frame; only this frame's relay region is
/// written.
///
/// # Errors
///
/// Returns [`DecodeError`] if `frame` does not hold one complete, well-formed
/// frame.
pub fn rewrite_relay_region(
    frame: &mut [u8],
    stream_id: u16,
    hop_sequence: u32,
) -> Result<(), DecodeError> {
    let layout = match parse_layout(frame)? {
        HeaderOutcome::Complete(layout) => layout,
        HeaderOutcome::Incomplete(shortfall) => return Err(shortfall.into_datagram_error(0)),
    };
    if let Err(shortfall) = require_body(frame, layout) {
        return Err(shortfall.into_datagram_error(layout.payload_length));
    }

    let offset = layout.relay_region_offset();
    let available = frame.len().saturating_sub(offset);
    let short = DecodeError::Truncated {
        region: Region::RelayRegion,
        needed: RELAY_REGION_SIZE,
        available,
    };
    let end = offset.checked_add(RELAY_REGION_SIZE).ok_or(short)?;
    let region: &mut [u8; RELAY_REGION_SIZE] = frame
        .get_mut(offset..end)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(short)?;

    // Destructured rather than indexed: no panicking path, and the widths are
    // the field widths by construction (invariant 3).
    let [slot_hi, slot_lo] = stream_id.to_be_bytes();
    let [hop_a, hop_b, hop_c, hop_d] = hop_sequence.to_be_bytes();
    *region = [slot_hi, slot_lo, hop_a, hop_b, hop_c, hop_d];
    Ok(())
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// The fields of a frame to encode.
///
/// `Debug` is hand-rolled and redacts `payload` and `signature`, exactly as
/// [`MediaFrameView`] does. This type holds the same bytes on the encode side
/// and is `pub`, so it crosses into crates that *do* have a `tracing`
/// dependency — a `debug!("{parts:?}")` there would render the `SFrame`
/// ciphertext, which ADR-0036 §11 treats as sensitive because the time-ordered
/// per-frame size sequence is the voice-activity trace.
#[derive(Clone, Copy)]
pub struct MediaFrameParts<'a> {
    /// Frame flags. `key_bearing` must agree with `wrapped_transmit_key`.
    pub flags: FrameFlags,
    /// The AEAD nonce input; see [`MediaFrameView::stream_sequence`].
    pub stream_sequence: u32,
    /// The wrapped transmit key, present iff `flags.key_bearing`.
    pub wrapped_transmit_key: Option<WrappedTransmitKey<'a>>,
    /// Extensions, in any order: they are emitted canonically.
    pub extensions: &'a [Extension<'a>],
    /// Relay region: subscriber slot.
    pub stream_id: u16,
    /// Relay region: per (connection, media stream) transmit count.
    pub hop_sequence: u32,
    /// The opaque `SFrame`-object payload.
    pub payload: &'a [u8],
    /// The Ed25519 signature over publisher region then payload.
    pub signature: &'a [u8; SIGNATURE_SIZE],
}

impl fmt::Debug for MediaFrameParts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaFrameParts")
            .field("flags", &self.flags)
            .field("stream_sequence", &self.stream_sequence)
            .field("wrapped_transmit_key", &self.wrapped_transmit_key)
            .field("extensions", &self.extensions)
            .field("stream_id", &self.stream_id)
            .field("hop_sequence", &self.hop_sequence)
            .field(
                "payload",
                &format_args!("<redacted {} B>", self.payload.len()),
            )
            .field("signature", &format_args!("<redacted {SIGNATURE_SIZE} B>"))
            .finish()
    }
}

/// Encode a frame.
///
/// **Never use this to reconstruct bytes for verification.** The signed and
/// associated-data spans are slices of the *received* buffer — see
/// [`MediaFrameView::publisher_region`] and
/// [`MediaFrameView::signed_ranges`]. Re-serializing a parsed struct and
/// verifying over the result is the defect invariant 2 exists to prevent: it
/// verifies what this encoder would have produced rather than what the peer
/// actually sent.
///
/// Encoding is **canonical**: fields go out in one fixed order and extensions
/// are emitted in ascending type order regardless of input order. Therefore
/// `encode(decode(x)) == x` byte-for-byte for every `x` that decodes, which
/// makes decode injective over the accepted language — the no-covert-channel
/// property, proved independently of the byte-coverage test.
///
/// The whole extension grammar is enforced here, not just ordering: an encoder
/// able to emit bytes the decoder rejects would break that identity and report
/// the failure in the wrong place.
///
/// # Errors
///
/// Returns [`EncodeError`] if the flag and field disagree, the payload is over
/// [`MAX_PAYLOAD_BYTES`], or the extensions violate the grammar.
pub fn encode_frame(parts: &MediaFrameParts<'_>) -> Result<Bytes, EncodeError> {
    let field_present = parts.wrapped_transmit_key.is_some();
    if parts.flags.key_bearing != field_present {
        return Err(EncodeError::KeyBearingFlagMismatch {
            flag_set: parts.flags.key_bearing,
            field_present,
        });
    }
    if parts.payload.len() > MAX_PAYLOAD_BYTES {
        return Err(EncodeError::PayloadTooLarge {
            len: parts.payload.len(),
            max: MAX_PAYLOAD_BYTES,
        });
    }
    // Checked conversion sized to the field (invariant 3). A masking cast here
    // would silently truncate a 4 GiB payload to a small declared length.
    let payload_length =
        u32::try_from(parts.payload.len()).map_err(|_| EncodeError::PayloadTooLarge {
            len: parts.payload.len(),
            max: MAX_PAYLOAD_BYTES,
        })?;

    // Extensions are encoded into a scratch buffer first, because the region's
    // length must be written before its bytes.
    let mut ext_buf = BytesMut::new();
    let ext_len = encode_into(parts.extensions, &mut ext_buf)?;
    let ext_len_field = u16::try_from(ext_len).map_err(|_| {
        EncodeError::Extensions(ExtensionError::EntryHeaderTruncated { offset: ext_len })
    })?;

    let capacity = PUBLISHER_FIXED_PREFIX_SIZE
        .saturating_add(if field_present {
            WRAPPED_TRANSMIT_KEY_SIZE
        } else {
            0
        })
        .saturating_add(EXT_LENGTH_FIELD_SIZE)
        .saturating_add(ext_len)
        .saturating_add(RELAY_REGION_SIZE)
        .saturating_add(parts.payload.len())
        .saturating_add(SIGNATURE_SIZE);
    let mut buf = BytesMut::with_capacity(capacity);

    buf.put_u8(PROTOCOL_VERSION);
    buf.put_u8(parts.flags.to_u8());
    buf.put_u32(payload_length);
    buf.put_u32(parts.stream_sequence);
    if let Some(wrapped) = parts.wrapped_transmit_key {
        buf.put_u16(wrapped.kek_generation());
        buf.put_slice(wrapped.expose_wrapped_key());
        buf.put_slice(wrapped.expose_wrap_tag());
    }
    buf.put_u16(ext_len_field);
    buf.put_slice(&ext_buf);
    buf.put_u16(parts.stream_id);
    buf.put_u32(parts.hop_sequence);
    buf.put_slice(parts.payload);
    buf.put_slice(parts.signature);

    Ok(buf.freeze())
}
