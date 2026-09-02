//! Errors for the reference generator. NON-PRODUCTION crate — see crate docs.

use core::fmt;

/// Anything that can go wrong building a vector row.
///
/// Every variant carries **the offending value and its bound**, never a bare
/// "out of range". On the key-id path that is load-bearing rather than hygiene:
/// the reason the packer errors instead of masking is that a `sender_id` of
/// 65536 aliasing to 0 is a key-id collision, therefore (key, nonce) reuse,
/// therefore authentication-key recovery under AES-GCM rather than mere
/// confidentiality loss. An error that drops the value drops the evidence for
/// which field collided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenError {
    /// A key-id component exceeded the width of its field.
    KeyIdFieldOverflow {
        /// Which component: `sender_id`, `stream`, or `generation`.
        field: &'static str,
        /// The value that did not fit.
        value: u64,
        /// Width of the field in bits.
        bits: usize,
        /// One past the largest representable value.
        exclusive_bound: u64,
    },
    /// `sender_id` zero is reserved-invalid (`JoinResponse.sender_id` is
    /// `1..=65535`): a shared zero would be N colliding key ids, i.e. two
    /// senders at one nonce under one KEK.
    SenderIdZero,
    /// A `ring` operation failed. `ring` errors are deliberately opaque, so the
    /// operation name is the only context available and is always supplied.
    Crypto {
        /// The operation that failed.
        operation: &'static str,
    },
    /// The `media-protocol` encoder rejected the parts.
    Encode {
        /// Rendered encoder error. `media-protocol`'s `Debug` redacts key
        /// material, so this cannot carry secrets.
        detail: String,
    },
    /// A span computed from the ADR-0036 specification disagreed with the span
    /// `media-protocol`'s parser walk produced for the same frame.
    ///
    /// **This is the cross-check firing, and it is never routine.** One of the
    /// two derivations of a protected computation is wrong.
    SpanDisagreement {
        /// Which span: `publisher_region` or `signed_input`.
        span: &'static str,
        /// What this crate's ADR-derived arithmetic produced.
        spec_derived: usize,
        /// What `media-protocol`'s parser walk produced.
        codec_derived: usize,
    },
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyIdFieldOverflow {
                field,
                value,
                bits,
                exclusive_bound,
            } => write!(
                f,
                "key-id field `{field}` value {value} does not fit in {bits} bits \
                 (must be < {exclusive_bound}); refusing to mask or shift, because a \
                 masking pack aliases to a colliding key id and a bare shift overflows \
                 into the adjacent field"
            ),
            Self::SenderIdZero => f.write_str(
                "sender_id 0 is reserved-invalid (JoinResponse.sender_id is 1..=65535): \
                 a shared zero is N colliding key ids, i.e. two senders at one nonce \
                 under one KEK",
            ),
            Self::Crypto { operation } => write!(f, "cryptographic operation failed: {operation}"),
            Self::Encode { detail } => write!(f, "frame encode failed: {detail}"),
            Self::SpanDisagreement {
                span,
                spec_derived,
                codec_derived,
            } => write!(
                f,
                "span `{span}` disagreement: ADR-derived arithmetic says {spec_derived}, \
                 media-protocol's parser walk says {codec_derived}. One of the two \
                 derivations of a protected computation is wrong; do NOT reconcile by \
                 changing whichever is easier to edit"
            ),
        }
    }
}

impl std::error::Error for GenError {}
