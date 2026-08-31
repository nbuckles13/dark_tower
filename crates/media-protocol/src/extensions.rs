//! Publisher-set TLV extensions in the frame's publisher region (ADR-0036 §2,
//! §7).
//!
//! # Encoding
//!
//! ```text
//! entry := ext_type : u8 | value_len : u8 | value : value_len bytes
//! ```
//!
//! ADR-0036 mandates a type-length-value section but does not specify its
//! encoding; this is that specification, and story task 8 pins it
//! cross-language.
//!
//! # The region has zero free bits, and that is the point
//!
//! A decoded region must satisfy **all** of:
//!
//! * every type appears in [`EXT_REGISTRY`] — **unknown types are rejected,
//!   not skipped**;
//! * types appear in strictly ascending order, so no type repeats and the
//!   ordering carries no publisher-chosen information;
//! * each value length equals the length its registry entry declares;
//! * each value lies in the accepted set its registry entry declares;
//! * the region is consumed exactly, with no slack.
//!
//! Together these mean the region is **completely determined by which subset
//! of known types is present**. Two clauses do distinct work and either looks
//! sufficient alone: the fixed/canonical length closes the channel in the
//! *length*, and the accepted value set closes it in the *value*. An entry
//! needs both.
//!
//! # Why unknown types are rejected rather than skipped
//!
//! §7 introduces TLV so that selection signals are not forced into one fixed
//! salience byte spent on every video frame. §2 is decisive about what happens
//! to bytes nobody inspects: *"every byte and every bit is either decoded into
//! a field the receiver inspects, or rejected if set — nothing is skipped"*,
//! and *"the version field is the extension path, and fail-closed is chosen
//! deliberately over forward compatibility"*. A length-prefixed skip is the
//! reserved-byte covert channel with a length field in front of it.
//!
//! # Consequence: adding a registry type is a header-version bump
//!
//! Because unknown types are rejected, a receiver on this version rejects a
//! frame carrying a type it does not know. **Adding an entry to
//! [`EXT_REGISTRY`] therefore requires bumping [`crate::frame::PROTOCOL_VERSION`].**
//!
//! That bump is safe rather than a fleet-skew hazard, for a reason worth
//! spelling out because it is a three-step inference a future author will not
//! reconstruct: per §2 the header version is **meeting-wide and MC-directed**,
//! so MC never composes a mixed-version meeting and a newer publisher can never
//! meet an older receiver. Skip the bump and a rolling deploy has older
//! receivers rejecting a newer publisher while the reject reason confidently
//! reports hostile input — a legible reason that is wrong, during a deploy,
//! with a framing that escalates to security.
//!
//! # Two rules that bind the author of the next type
//!
//! 1. **Extensibility is a new type, never a wider value range on an existing
//!    one.** §7's richer future signals — speech-versus-noise classification,
//!    onset prediction, per-media-kind salience — become their own types with
//!    their own declared sets. They do not become more bits in `0x01`.
//! 2. **A declared value set must be one the consumer actually consumes.**
//!    Declaring wider than consumed reopens the surplus that enforcement exists
//!    to close. For `0x01` this holds by construction — a ranking scalar is
//!    consumed by comparison, so every accepted value can change the ordering.
//!
//! # Values are untrusted
//!
//! Everything here is publisher-supplied and forgeable (§7). Enforcing a
//! declared range is not a trust claim; it removes a free channel. What bounds
//! a lying publisher is MC-assigned priority groups and the per-participant
//! churn rate limit, in the selector — not this codec.

use crate::frame::EXT_LENGTH_FIELD_SIZE;
use bytes::{BufMut, BytesMut};
use core::fmt;

/// Width of a TLV entry's type field.
pub const EXT_TYPE_FIELD_BYTES: usize = 1;
/// Width of a TLV entry's value-length field.
pub const EXT_VALUE_LENGTH_FIELD_BYTES: usize = 1;
/// Bytes an entry costs before its value.
pub const EXT_ENTRY_HEADER_BYTES: usize = EXT_TYPE_FIELD_BYTES + EXT_VALUE_LENGTH_FIELD_BYTES;

/// Publisher-declared salience: a normalized ranking scalar (§7).
pub const EXT_TYPE_SALIENCE: u8 = 0x01;

/// The values a registry entry accepts.
///
/// An enum rather than a bare range so that the first multi-byte extension
/// type must **add a variant** — a decision point a reviewer sees — instead of
/// quietly reaching past a `u8`-shaped field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptedValues {
    /// A single-byte value constrained to this inclusive range.
    ByteRange {
        /// Lowest accepted value.
        min: u8,
        /// Highest accepted value.
        max: u8,
    },
}

impl AcceptedValues {
    /// Whether `value` is accepted. `value` is the entry's whole value span.
    #[must_use]
    pub const fn accepts(self, value: &[u8]) -> bool {
        match self {
            Self::ByteRange { min, max } => match value {
                [byte] => *byte >= min && *byte <= max,
                _ => false,
            },
        }
    }
}

/// One registry entry.
///
/// Every field is required: an entry cannot be added without declaring its
/// value length and its accepted value set. The declaration lives in the type,
/// not in prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionSpec {
    /// The type byte.
    pub ext_type: u8,
    /// Exact on-wire length of this type's value.
    ///
    /// A future variable-length type must declare a **canonical** encoding in
    /// which the length is a function of the value — no two distinct encodings
    /// may carry the same meaning — and enforce canonicality in that type's
    /// parser. A mere maximum would make the length byte a publisher-chosen
    /// free variable, reopening the channel that the fixed length closes.
    pub value_len: usize,
    /// The values this type accepts. Enforced at decode and at encode.
    pub accepted: AcceptedValues,
}

/// The version-2 extension registry.
///
/// ANCHOR (DRY): pinned cross-language in
/// `proto/test-vectors/frame-v2.vectors.json` (story task 8) — the type byte,
/// the value length **and** the accepted range travel together, and both
/// codecs derive [`crate::frame::MAX_EXT_BYTES`] from the pinned registry
/// rather than hardcoding a bound. The pin must include the rejection
/// semantics, not only the entries: a decoder could pin an identical registry
/// and still skip unknown types, which is the §2 defect behind a perfectly
/// matching table.
///
/// # A vector that contradicts a recorded ruling is a defect, not a precedent
///
/// Vectors beat *prose* — a summary can go stale, and the pinned file is what
/// both codecs are checked against. Vectors do **not** beat a recorded ruling.
/// If a vector disagrees with one, escalate it; do not conform to it.
///
/// **Escalate to whom:** the owners recorded for `crates/media-protocol/**` in
/// `scripts/guards/simple/cross-boundary-ownership.yaml` — protocol and
/// media-handler — plus security. Named as a lookup rather than as individuals
/// so it still resolves years from now, and so it stays correct if the
/// ownership manifest changes. An escalation rule without an addressee
/// degrades into "think hard about it", which loses to schedule pressure.
///
/// The reason is the vectors' provenance: they are **generated from this Rust
/// encoder and then frozen**. So a vector contradicting a ruling means this
/// codec implemented that ruling wrongly and the freeze captured the error.
/// Conforming the other implementation to it makes both consistently wrong
/// with the drift guard green — strictly worse than a divergence, because a
/// divergence is loud and this is silent.
///
/// **This bites hardest here, on the extension grammar.** The associated-data
/// span rows are anchored externally against the `SFrame` specification, so
/// they are a real outside check. The TLV registry is ours alone: nothing
/// outside this repository says what type `0x01` means or that its accepted
/// set is `0..=100`. These rows can therefore prove *internal consistency*
/// only, never correctness — they are worth less than the externally anchored
/// rows, and must not be read as if they were worth the same.
pub const EXT_REGISTRY: &[ExtensionSpec] = &[ExtensionSpec {
    ext_type: EXT_TYPE_SALIENCE,
    value_len: 1,
    // Declared by @media-handler as the owner of salience semantics (§7 leaves
    // ranking, debouncing and signal combination to the selector). A
    // normalized 0..=100 scale: a single ranking scalar does not need 256
    // levels, and bounding now is the fail-safe direction because no client
    // emits salience until the SDK lands, so any widening is a clean re-pin
    // before there is traffic to break.
    accepted: AcceptedValues::ByteRange { min: 0, max: 100 },
}];

/// Look up a registry entry.
#[must_use]
pub fn spec_for(ext_type: u8) -> Option<&'static ExtensionSpec> {
    EXT_REGISTRY.iter().find(|spec| spec.ext_type == ext_type)
}

/// Largest grammatically-valid extension region: every registry type present
/// exactly once.
///
/// This is what [`crate::frame::MAX_EXT_BYTES`] is derived from. A larger
/// fixed bound could never fire on a frame the grammar would have accepted.
pub const EXT_REGISTRY_TOTAL_BYTES: usize = registry_total_bytes();

const fn registry_total_bytes() -> usize {
    let mut total = 0usize;
    let mut i = 0usize;
    while i < EXT_REGISTRY.len() {
        // Const-evaluated; `<[T]>::get` is not available in const context and
        // the index is bounded by the loop condition, so this cannot panic.
        #[expect(
            clippy::indexing_slicing,
            reason = "const context: `<[T]>::get()` is not const-callable here, and the index is \
                      bounded by the loop guard `i < EXT_REGISTRY.len()`."
        )]
        let value_len = EXT_REGISTRY[i].value_len;
        total += EXT_ENTRY_HEADER_BYTES + value_len;
        i += 1;
    }
    total
}

// Structural guarantees about the registry, checked at compile time.
const _: () = {
    // A zero-length entry would make "the region is determined by which types
    // are present" false, because a zero-length value carries no information
    // while still consuming a type slot.
    let mut i = 0usize;
    while i < EXT_REGISTRY.len() {
        #[expect(
            clippy::indexing_slicing,
            reason = "const context: `<[T]>::get()` is not const-callable here, and the index is \
                      bounded by the loop guard `i < EXT_REGISTRY.len()`."
        )]
        let spec = EXT_REGISTRY[i];
        assert!(
            spec.value_len >= 1,
            "registry entries must declare a non-zero value length"
        );
        // Ascending, unique type bytes: the decoder enforces strictly
        // ascending order on the wire, which is only satisfiable if the
        // registry itself is ordered.
        if i > 0 {
            #[expect(
                clippy::indexing_slicing,
                reason = "const context: `<[T]>::get()` is not const-callable here, and the index is \
                      bounded by the loop guard `i < EXT_REGISTRY.len()`."
            )]
            let previous = EXT_REGISTRY[i - 1];
            assert!(
                previous.ext_type < spec.ext_type,
                "registry must be strictly ascending by type"
            );
        }
        i += 1;
    }
    // The region length must fit the u16 length field.
    assert!(EXT_REGISTRY_TOTAL_BYTES <= 0xFFFF);
    assert!(EXT_LENGTH_FIELD_SIZE == 2);
};

/// Why an extension region was rejected.
///
/// Every variant maps to the `extensions_malformed` reject reason; the detail
/// is available through `Display` for local diagnosis only. Per ADR-0036 §11
/// this detail must not become a per-frame log field or metric label on the
/// media path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionError {
    /// The region ended part-way through an entry header.
    #[error("entry header truncated at offset {offset} of the extension region")]
    EntryHeaderTruncated {
        /// Offset within the extension region.
        offset: usize,
    },
    /// An entry declared more value bytes than the region contains.
    #[error("type 0x{ext_type:02x} declares a {declared}-byte value but only {available} remain")]
    ValueTruncated {
        /// The entry's type byte.
        ext_type: u8,
        /// Length the entry declared.
        declared: usize,
        /// Bytes actually remaining in the region.
        available: usize,
    },
    /// The type byte is not in the registry.
    #[error("unknown extension type 0x{ext_type:02x}")]
    UnknownType {
        /// The unrecognised type byte.
        ext_type: u8,
    },
    /// The same type appeared twice.
    #[error("duplicate extension type 0x{ext_type:02x}")]
    DuplicateType {
        /// The repeated type byte.
        ext_type: u8,
    },
    /// Types were not in strictly ascending order.
    #[error("extension types out of ascending order: 0x{previous:02x} precedes 0x{ext_type:02x}")]
    NotAscending {
        /// The preceding type byte.
        previous: u8,
        /// The out-of-order type byte.
        ext_type: u8,
    },
    /// The declared value length disagrees with the registry.
    #[error("type 0x{ext_type:02x} declares length {declared}, registry requires {required}")]
    WrongValueLength {
        /// The entry's type byte.
        ext_type: u8,
        /// Length the entry declared.
        declared: usize,
        /// Length the registry requires.
        required: usize,
    },
    /// The value is outside the accepted set the registry declares.
    #[error("type 0x{ext_type:02x} carries a value outside its declared accepted set")]
    ValueOutOfRange {
        /// The entry's type byte.
        ext_type: u8,
    },
}

/// A single decoded extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extension<'a> {
    /// The type byte.
    pub ext_type: u8,
    /// The value bytes.
    pub value: &'a [u8],
}

/// A validated extension region.
///
/// Constructing one runs the full grammar check, so iteration cannot fail.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Extensions<'a> {
    bytes: &'a [u8],
}

impl<'a> Extensions<'a> {
    /// The empty region.
    #[must_use]
    pub const fn empty() -> Self {
        Self { bytes: &[] }
    }

    /// Validate `bytes` as a complete extension region.
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionError`] if the region violates the grammar. The
    /// caller maps every variant to the `extensions_malformed` reject reason.
    pub fn validate(bytes: &'a [u8]) -> Result<Self, ExtensionError> {
        let mut offset = 0usize;
        let mut previous: Option<u8> = None;

        while offset < bytes.len() {
            // `checked_add` throughout, matching how `value_end` is computed a
            // few lines down: two idioms in one loop had no principled reason.
            let header_end = offset
                .checked_add(EXT_ENTRY_HEADER_BYTES)
                .ok_or(ExtensionError::EntryHeaderTruncated { offset })?;
            let header = bytes
                .get(offset..header_end)
                .ok_or(ExtensionError::EntryHeaderTruncated { offset })?;
            let (ext_type, declared) = match header {
                [ext_type, declared] => (*ext_type, usize::from(*declared)),
                _ => return Err(ExtensionError::EntryHeaderTruncated { offset }),
            };

            if let Some(previous) = previous {
                if previous == ext_type {
                    return Err(ExtensionError::DuplicateType { ext_type });
                }
                if previous > ext_type {
                    return Err(ExtensionError::NotAscending { previous, ext_type });
                }
            }

            let spec = spec_for(ext_type).ok_or(ExtensionError::UnknownType { ext_type })?;
            if declared != spec.value_len {
                return Err(ExtensionError::WrongValueLength {
                    ext_type,
                    declared,
                    required: spec.value_len,
                });
            }

            let value_start = offset
                .checked_add(EXT_ENTRY_HEADER_BYTES)
                .ok_or(ExtensionError::EntryHeaderTruncated { offset })?;
            let value_end =
                value_start
                    .checked_add(declared)
                    .ok_or(ExtensionError::ValueTruncated {
                        ext_type,
                        declared,
                        available: 0,
                    })?;
            let value =
                bytes
                    .get(value_start..value_end)
                    .ok_or(ExtensionError::ValueTruncated {
                        ext_type,
                        declared,
                        available: bytes.len().saturating_sub(value_start),
                    })?;

            if !spec.accepted.accepts(value) {
                return Err(ExtensionError::ValueOutOfRange { ext_type });
            }

            previous = Some(ext_type);
            offset = value_end;
        }

        Ok(Self { bytes })
    }

    /// The region's raw bytes, which are part of the signed publisher region.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// On-wire length of the region.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the region carries no extensions.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Iterate the validated entries.
    #[must_use]
    pub const fn iter(&self) -> ExtensionIter<'a> {
        ExtensionIter {
            bytes: self.bytes,
            offset: 0,
        }
    }

    /// The publisher-declared salience, if carried.
    ///
    /// Forgeable by design (§7). It is bounded by MC-assigned priority groups
    /// in the selector, never by trusting this value.
    #[must_use]
    pub fn declared_salience(&self) -> Option<u8> {
        self.iter().find_map(|ext| match (ext.ext_type, ext.value) {
            (EXT_TYPE_SALIENCE, [value]) => Some(*value),
            _ => None,
        })
    }
}

impl<'a> IntoIterator for Extensions<'a> {
    type Item = Extension<'a>;
    type IntoIter = ExtensionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &Extensions<'a> {
    type Item = Extension<'a>;
    type IntoIter = ExtensionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl fmt::Debug for Extensions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Iterator over a validated extension region. Cannot fail.
///
/// Deliberately not `Copy`: a `Copy` iterator silently duplicates rather than
/// moves, so an accidental copy would restart iteration from the same offset.
#[derive(Debug, Clone)]
pub struct ExtensionIter<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for ExtensionIter<'a> {
    type Item = Extension<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.bytes.len() {
            return None;
        }
        let header = self
            .bytes
            .get(self.offset..self.offset.checked_add(EXT_ENTRY_HEADER_BYTES)?)?;
        let (ext_type, declared) = match header {
            [ext_type, declared] => (*ext_type, usize::from(*declared)),
            _ => return None,
        };
        let value_start = self.offset.checked_add(EXT_ENTRY_HEADER_BYTES)?;
        let value_end = value_start.checked_add(declared)?;
        let value = self.bytes.get(value_start..value_end)?;
        self.offset = value_end;
        Some(Extension { ext_type, value })
    }
}

/// Validate and write extensions in canonical order.
///
/// Entries are emitted in ascending type order regardless of the order given,
/// so encoding is canonical and `encode(decode(x)) == x` holds byte-for-byte.
/// The whole grammar is enforced here, not merely the ordering: an encoder
/// that could emit bytes the decoder rejects would break that identity and
/// would report the failure in the wrong place.
///
/// # Errors
///
/// Returns [`ExtensionError`] if the entries violate the grammar.
pub(crate) fn encode_into(
    extensions: &[Extension<'_>],
    out: &mut BytesMut,
) -> Result<usize, ExtensionError> {
    let mut ordered: Vec<&Extension<'_>> = extensions.iter().collect();
    ordered.sort_by_key(|ext| ext.ext_type);

    let mut written = 0usize;
    let mut previous: Option<u8> = None;
    for ext in ordered {
        let ext_type = ext.ext_type;
        if previous == Some(ext_type) {
            return Err(ExtensionError::DuplicateType { ext_type });
        }
        let spec = spec_for(ext_type).ok_or(ExtensionError::UnknownType { ext_type })?;
        if ext.value.len() != spec.value_len {
            return Err(ExtensionError::WrongValueLength {
                ext_type,
                declared: ext.value.len(),
                required: spec.value_len,
            });
        }
        if !spec.accepted.accepts(ext.value) {
            return Err(ExtensionError::ValueOutOfRange { ext_type });
        }
        let declared =
            u8::try_from(ext.value.len()).map_err(|_| ExtensionError::WrongValueLength {
                ext_type,
                declared: ext.value.len(),
                required: spec.value_len,
            })?;
        out.put_u8(ext_type);
        out.put_u8(declared);
        out.put_slice(ext.value);
        written = written
            .checked_add(EXT_ENTRY_HEADER_BYTES)
            .and_then(|w| w.checked_add(ext.value.len()))
            .ok_or(ExtensionError::ValueOutOfRange { ext_type })?;
        previous = Some(ext_type);
    }
    Ok(written)
}
