//! Media frame v2 wire format: field sizes, offsets, flags, and the zero-copy
//! decoded view.
//!
//! # Layout (ADR-0036 §2 and its Appendix), big-endian throughout
//!
//! ```text
//! off  size  field                       region
//!   0     1  version (= 2)               publisher  |
//!   1     1  flags                       publisher  | PUBLISHER_FIXED_PREFIX_SIZE
//!   2     4  payload_length : u32        publisher  |
//!   6     4  stream_sequence : u32       publisher  |
//!  10    50  wrapped transmit key        publisher    present IFF flags bit2
//!              kek_generation : u16
//!              wrapped_key    : 32 B
//!              wrap_tag       : 16 B
//!   +     2  ext_length : u16            publisher    EXT_LENGTH_FIELD_SIZE
//!   +     N  TLV extensions              publisher    N = ext_length
//!  ======================================== end of publisher region
//!   +     2  stream_id : u16             relay      | RELAY_REGION_SIZE
//!   +     4  hop_sequence : u32          relay      |
//!   +   len  payload (opaque SFrame object)
//!   +    64  Ed25519 signature
//! ```
//!
//! The **publisher region** is authenticated end-to-end: it is the AEAD
//! associated data (§4) and the first half of the signed input (§3). The
//! **relay region** is excluded from both, because a media handler rewrites it
//! per subscriber.
//!
//! # This crate holds no keys and performs no cryptography
//!
//! Decoding does **not** verify the signature and does **not** decrypt. The
//! payload and the wrapped transmit key are opaque byte spans. Ed25519
//! verification and the `SFrame` open happen in the caller.
//!
//! # Obligations this crate cannot enforce
//!
//! Two properties the format depends on live in callers, named here so the
//! constraint travels with the format rather than only in a task description:
//!
//! 1. **The relay region is unauthenticated by design.** `stream_id` and
//!    `hop_sequence` are 6 bytes per frame that a compromised media handler
//!    writes freely and nobody authenticates (§2). That is accepted and
//!    bounded — but the bound only exists if the **receiver validates
//!    `stream_id` against its own declared slots (§6) and rejects unknown
//!    ones.** Without that check a relay gains 16 free bits per frame and can
//!    place a frame in a slot the subscriber never assigned.
//! 2. **`Ok(None)` from [`crate::codec::decode_stream_frame`] obligates the
//!    caller.** A peer that opens a stream and stalls mid-frame produces
//!    `Ok(None)` forever: no reject reason, no counter, no timeout. A caller
//!    must pair the incomplete outcome with a bound on **both** buffered bytes
//!    per stream (see [`MAX_FRAME_BYTES`]) and time-to-completion, and must
//!    **count the give-up**. This crate sees one buffer and has no notion of
//!    time or stream count, so it cannot enforce this.
//!
//! # Accepted cleartext channel: the relay region
//!
//! One entry, stated rather than left silent.
//!
//! * **Relay region — 48 bits per frame, direction MH -> receiver.** It cannot
//!   be constrained here because the relay must be able to write it. The
//!   direction matters: this is exfiltration *out of* a compromised relay
//!   toward clients, which is the shape §2 calls "a covert channel out of a
//!   compromised MH".
//!
//! For the adversary set, see `docs/decisions/adr-0036-media-flow.md:1056`,
//! which states it for the per-frame metadata these fields sit alongside.
//! Cited rather than restated, because a restated adversary set drifts from
//! its source. One consequence is worth knowing before reading further: these
//! fields sit inside QUIC with TLS terminated at MH, so a **network observer
//! is not in the set**.
//!
//! The extension region is deliberately **not** an entry. Every registry type
//! declares an accepted value set that decode enforces, which drives the
//! surplus of representable-over-declared to zero — so the clause removes the
//! need for a ledger entry rather than sizing one.
//!
//! The reason to enforce that set is **not** capacity. Per-frame payload size
//! is a far wider cleartext channel to the identical adversary set, and §11
//! accepts it explicitly (the time-ordered sequence of sizes for one stream is
//! the voice-activity trace). The reason is **cost**: modulating payload size
//! perturbs frame sizes and therefore media, so it costs the attacker quality,
//! whereas modulating an unenforced header byte is free and changes nothing
//! observable. A zero-cost channel is worth closing where a quality-costing one
//! has already been accepted — and §2's rule applies regardless of marginal
//! magnitude.

use crate::extensions::{Extensions, EXT_REGISTRY_TOTAL_BYTES};
use core::fmt;
use core::ops::Range;

// ---------------------------------------------------------------------------
// Single-source atomics
// ---------------------------------------------------------------------------

/// Authentication-tag length of the `SFrame` ciphersuite in use (0x0005,
/// `AES_256_GCM_SHA512_128`, RFC 9605 §8.1).
///
/// Defined exactly once in this workspace. This is the **media wire** tag and
/// is deliberately **not** shared with `ac-service`'s at-rest storage-envelope
/// tag length, which is a different concept that happens to have the same
/// value today; sharing them would falsely couple the media ciphersuite to
/// AC's storage format.
pub const AEAD_TAG_BYTES: usize = 16;

/// Length of the `SFrame` key identifier (RFC 9605 caps the KID at 64 bits).
///
/// Defined exactly once in this workspace. The key-id *layout* — `sender_id`
/// (16 bits) | `stream` (8 bits) | `generation` (40 bits) — belongs to the
/// crypto layer, not to this codec; only the size is needed here.
pub const KEY_ID_BYTES: usize = 8;

/// Size of the `SFrame` object's own clear header plus its tag.
///
/// **Size only — this crate performs no `SFrame` parsing.** Nothing in the
/// codec branches on this value; the payload is fully opaque here. It exists
/// so downstream buffer sizing has one home. `media-protocol` is the only Rust
/// home for it because the `SFrame` cryptography is TypeScript-only.
pub const SFRAME_OBJECT_OVERHEAD_BYTES: usize = KEY_ID_BYTES + AEAD_TAG_BYTES;

// ---------------------------------------------------------------------------
// Field widths
// ---------------------------------------------------------------------------

/// Width of the version field.
pub const VERSION_FIELD_BYTES: usize = 1;
/// Width of the flag field.
pub const FLAGS_FIELD_BYTES: usize = 1;
/// Width of the declared payload-length field.
pub const PAYLOAD_LENGTH_FIELD_BYTES: usize = 4;
/// Width of the stream-sequence field.
pub const STREAM_SEQUENCE_FIELD_BYTES: usize = 4;
/// Width of the KEK-generation field inside the wrapped-transmit-key field.
pub const KEK_GENERATION_FIELD_BYTES: usize = 2;
/// Length of the wrapped transmit key itself (AES-256).
pub const WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES: usize = 32;
/// Width of the extension-region length field.
pub const EXT_LENGTH_FIELD_SIZE: usize = 2;
/// Width of the relay `stream_id` field.
pub const STREAM_ID_FIELD_BYTES: usize = 2;
/// Width of the relay `hop_sequence` field.
pub const HOP_SEQUENCE_FIELD_BYTES: usize = 4;
/// Length of the trailing Ed25519 signature.
pub const SIGNATURE_SIZE: usize = 64;

// ---------------------------------------------------------------------------
// Derived region sizes
// ---------------------------------------------------------------------------

/// Bytes of publisher region that are present on every frame regardless of
/// flags: version, flags, payload length, stream sequence.
pub const PUBLISHER_FIXED_PREFIX_SIZE: usize = VERSION_FIELD_BYTES
    + FLAGS_FIELD_BYTES
    + PAYLOAD_LENGTH_FIELD_BYTES
    + STREAM_SEQUENCE_FIELD_BYTES;

/// Size of the wrapped-transmit-key field, present iff the key-bearing flag is
/// set. Fixed size is what lets presence be signalled by a flag rather than a
/// length (§2).
pub const WRAPPED_TRANSMIT_KEY_SIZE: usize =
    KEK_GENERATION_FIELD_BYTES + WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES + AEAD_TAG_BYTES;

/// Size of the relay region: the span a media handler rewrites per subscriber,
/// excluded from both the signature and the associated data (§2, §4).
pub const RELAY_REGION_SIZE: usize = STREAM_ID_FIELD_BYTES + HOP_SEQUENCE_FIELD_BYTES;

/// Maximum size of the TLV extension region.
///
/// **Derived from the extension registry, not chosen.** Unknown types and
/// duplicates are rejected, so a grammatically valid region is a subset of the
/// registry with each type at most once — whose maximum size is exactly the
/// registry total. A larger fixed bound could never fire on anything the
/// grammar would have accepted, i.e. it would be a dead check.
pub const MAX_EXT_BYTES: usize = EXT_REGISTRY_TOTAL_BYTES;

/// Maximum payload size accepted by this wire format.
///
/// **A wire-format constant, not an operational knob.** It is enforced before
/// any allocation, and exposed through [`crate::codec::peek_frame_len`] so a
/// stream reader can bound its buffer *before* reserving — which is the
/// parsing trust boundary ADR-0036 §2 describes as passing fuzzing of the
/// decode function while being wrong in the reader that calls it. The tunable
/// levers for load are §11's ingress admission controls, not this value.
///
/// ANCHOR (DRY): the cross-language single source of truth is
/// `proto/test-vectors/frame-v2.vectors.json`, key `max_payload_bytes`, landed
/// by story task 8; its drift guard asserts this constant equals that value.
/// (The anchor deliberately names a file created by a later task that depends
/// on this one.)
///
/// Vector precedence: vectors beat prose, but a vector contradicting a recorded
/// ruling is a defect to escalate rather than conform to — see the note at
/// [`crate::extensions::EXT_REGISTRY`].
pub const MAX_PAYLOAD_BYTES: usize = 1_048_576;

/// Bytes a reader must buffer before [`crate::codec::peek_frame_len`] can
/// possibly determine a frame's total length.
///
/// This is the whole header including the relay region, because the parser
/// validates through the relay region before reporting a length: every
/// conditional region at its maximum. A reader that has buffered this many
/// bytes and still gets `Ok(None)` has been handed something that is not a
/// frame.
pub const MAX_HEADER_BYTES: usize = PUBLISHER_FIXED_PREFIX_SIZE
    + WRAPPED_TRANSMIT_KEY_SIZE
    + EXT_LENGTH_FIELD_SIZE
    + MAX_EXT_BYTES
    + RELAY_REGION_SIZE;

/// Largest frame this wire format can represent. Use it to bound a stream
/// reader's buffer (see the `Ok(None)` obligation in the module docs).
pub const MAX_FRAME_BYTES: usize = MAX_HEADER_BYTES + MAX_PAYLOAD_BYTES + SIGNATURE_SIZE;

// ---------------------------------------------------------------------------
// Fixed field offsets (exported so tests mutate at named constants, never at
// literal positions that would become a second home for the layout)
// ---------------------------------------------------------------------------

/// Offset of the version byte. Version sits at byte 0 so that version dispatch
/// precedes every other check.
pub const VERSION_OFFSET: usize = 0;
/// Offset of the flag byte.
pub const FLAGS_OFFSET: usize = VERSION_OFFSET + VERSION_FIELD_BYTES;
/// Offset of the declared payload length.
pub const PAYLOAD_LENGTH_OFFSET: usize = FLAGS_OFFSET + FLAGS_FIELD_BYTES;
/// Offset of the stream sequence.
pub const STREAM_SEQUENCE_OFFSET: usize = PAYLOAD_LENGTH_OFFSET + PAYLOAD_LENGTH_FIELD_BYTES;

// ---------------------------------------------------------------------------
// Version and flags
// ---------------------------------------------------------------------------

/// The frame header version this codec implements.
///
/// **There is deliberately no version-1 decode path and no version
/// negotiation.** v1 is deleted rather than retained: at the commit that
/// introduced v2, `media-protocol` had no consumer in any service and nothing
/// deployed spoke v1, so a live v1 branch would have been a downgrade surface
/// with no legitimate caller. Per §2 the version is meeting-wide and
/// MC-directed, so a mixed-version meeting cannot arise and a relay can never
/// translate between versions (the field is signed).
pub const PROTOCOL_VERSION: u8 = 2;

/// Flag bit 0: this frame can be decoded without its predecessors.
///
/// This flag **replaces a media-type field** (§2). A media handler must be able
/// to tell that a frame is a switch point without learning whether it is audio
/// or video (§7); the subscriber learns media kind from its own slot
/// declaration, never from the frame.
pub const FLAG_INDEPENDENTLY_DECODABLE: u8 = 0b0000_0001;

/// Flag bit 1: the sender believes this frame can be dropped without affecting
/// others. Publisher-declared and therefore untrusted (§2, §7).
pub const FLAG_DISCARDABLE: u8 = 0b0000_0010;

/// Flag bit 2: a fixed-size wrapped transmit key follows the stream sequence.
pub const FLAG_KEY_BEARING: u8 = 0b0000_0100;

/// The only flag bits this version defines. Any other bit set is a decode
/// rejection — §2's "every byte and every bit is either decoded into a field
/// the receiver inspects, or rejected if set". There are no reserved bytes and
/// no ignored bits anywhere in this format.
///
/// ANCHOR (DRY): pinned cross-language in
/// `proto/test-vectors/frame-v2.vectors.json` (story task 8); the TypeScript
/// codec derives its mask from that file rather than hardcoding one.
///
/// Vector precedence: vectors beat prose, but a vector contradicting a recorded
/// ruling is a defect to escalate rather than conform to — see the note at
/// [`crate::extensions::EXT_REGISTRY`].
pub const LEGAL_FLAG_MASK: u8 = FLAG_INDEPENDENTLY_DECODABLE | FLAG_DISCARDABLE | FLAG_KEY_BEARING;

/// The three defined frame flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameFlags {
    /// Decodable without predecessors; the switch point for §7.
    pub independently_decodable: bool,
    /// Droppable without affecting other frames. Untrusted.
    pub discardable: bool,
    /// A wrapped transmit key follows the stream sequence.
    pub key_bearing: bool,
}

impl FrameFlags {
    /// Pack into the wire byte.
    #[must_use]
    pub const fn to_u8(self) -> u8 {
        let mut bits = 0u8;
        if self.independently_decodable {
            bits |= FLAG_INDEPENDENTLY_DECODABLE;
        }
        if self.discardable {
            bits |= FLAG_DISCARDABLE;
        }
        if self.key_bearing {
            bits |= FLAG_KEY_BEARING;
        }
        bits
    }

    /// Parse the wire byte, returning `None` if any undefined bit is set.
    ///
    /// Fail-closed by construction: there is no masking variant of this
    /// function. The predecessor idiom (mask away unknown bits and continue)
    /// is what made v1's flag field a covert channel.
    #[must_use]
    pub const fn from_u8_checked(bits: u8) -> Option<Self> {
        if bits & !LEGAL_FLAG_MASK != 0 {
            return None;
        }
        Some(Self {
            independently_decodable: bits & FLAG_INDEPENDENTLY_DECODABLE != 0,
            discardable: bits & FLAG_DISCARDABLE != 0,
            key_bearing: bits & FLAG_KEY_BEARING != 0,
        })
    }

    /// The undefined bits set in `bits`, for error reporting.
    #[must_use]
    pub const fn undefined_bits(bits: u8) -> u8 {
        bits & !LEGAL_FLAG_MASK
    }
}

// ---------------------------------------------------------------------------
// Wrapped transmit key
// ---------------------------------------------------------------------------

/// The fixed-size wrapped transmit key carried in the publisher region when
/// the key-bearing flag is set (§2, §4).
///
/// Borrowed, not owned: the key material is a slice of the caller's receive
/// buffer. `Debug` is hand-rolled and redacts the key material and the wrap
/// tag; the accessors are named `expose_*` so reaching for the bytes is a
/// visible act at the call site.
///
/// This is **ciphertext under the meeting KEK**, not raw key material, and the
/// borrowed buffer is not this crate's to zeroize — which is why
/// `common::secret::SecretBox` is not used here: it is owned, and boxing this
/// would allocate on every key-bearing frame, which under §4's audio cadence
/// is every audio frame.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct WrappedTransmitKey<'a> {
    kek_generation: u16,
    wrapped_key: &'a [u8; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES],
    wrap_tag: &'a [u8; AEAD_TAG_BYTES],
}

impl<'a> WrappedTransmitKey<'a> {
    /// Construct from its three parts.
    #[must_use]
    pub const fn new(
        kek_generation: u16,
        wrapped_key: &'a [u8; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES],
        wrap_tag: &'a [u8; AEAD_TAG_BYTES],
    ) -> Self {
        Self {
            kek_generation,
            wrapped_key,
            wrap_tag,
        }
    }

    /// The KEK generation this key is wrapped under. A receiver inspects this
    /// to select a KEK across a §4 rotation; a media handler ignores it.
    #[must_use]
    pub const fn kek_generation(self) -> u16 {
        self.kek_generation
    }

    /// The wrapped key bytes.
    #[must_use]
    pub const fn expose_wrapped_key(self) -> &'a [u8; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES] {
        self.wrapped_key
    }

    /// The wrap authentication tag.
    #[must_use]
    pub const fn expose_wrap_tag(self) -> &'a [u8; AEAD_TAG_BYTES] {
        self.wrap_tag
    }
}

impl fmt::Debug for WrappedTransmitKey<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WrappedTransmitKey")
            .field("kek_generation", &self.kek_generation)
            .field(
                "wrapped_key",
                &format_args!("<redacted {WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES} B>"),
            )
            .field("wrap_tag", &format_args!("<redacted {AEAD_TAG_BYTES} B>"))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Offsets and scalars produced by the crate's single layout parser.
///
/// Every public entry point in [`crate::codec`] is built on one parser
/// producing this value. There is deliberately no second, weaker parse of
/// these bytes anywhere: two parsers agree on every well-formed input and
/// diverge on exactly the malformed ones an attacker constructs, which for the
/// relay-region write path would mean a media handler writing six bytes at an
/// offset the receiver does not read them from — malformed only from the
/// receiver's point of view, and invisible to every counter the handler owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) flags: FrameFlags,
    pub(crate) payload_length: usize,
    pub(crate) stream_sequence: u32,
    /// Offset of the wrapped-transmit-key field, present iff key-bearing.
    ///
    /// **Recorded from the cursor during the walk, never recomputed.** It
    /// happens to equal `PUBLISHER_FIXED_PREFIX_SIZE` today because nothing
    /// conditional precedes it — which is exactly why recomputing it would be
    /// a latent differential rather than an obvious one: the two values would
    /// agree until the first conditional field is added ahead of it, and then
    /// silently diverge.
    pub(crate) wrapped_key_offset: Option<usize>,
    pub(crate) ext_start: usize,
    pub(crate) ext_len: usize,
}

impl Layout {
    /// One-past-the-end of the publisher region: the AEAD associated data span
    /// and the start of the relay region.
    pub(crate) const fn publisher_region_end(self) -> usize {
        self.ext_start + self.ext_len
    }

    pub(crate) const fn relay_region_offset(self) -> usize {
        self.publisher_region_end()
    }

    pub(crate) const fn payload_start(self) -> usize {
        self.relay_region_offset() + RELAY_REGION_SIZE
    }

    pub(crate) const fn signature_start(self) -> usize {
        self.payload_start() + self.payload_length
    }

    pub(crate) const fn frame_len(self) -> usize {
        self.signature_start() + SIGNATURE_SIZE
    }
}

// ---------------------------------------------------------------------------
// Decoded view
// ---------------------------------------------------------------------------

/// A decoded frame: a **borrowed** view over the caller's buffer.
///
/// Every field is a sub-slice of the input. There is no code path in this
/// crate that copies a payload — a copy is unrepresentable here, not merely
/// absent.
///
/// `PartialEq` is **not constant-time** and exists for tests and for the
/// canonical-roundtrip property. It must never back an authentication or
/// authorization decision.
///
/// `Debug` is hand-rolled and redacts the payload, the signature and the
/// wrapped key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MediaFrameView<'a> {
    frame: &'a [u8],
    layout: Layout,
    wrapped: Option<WrappedTransmitKey<'a>>,
    extensions: Extensions<'a>,
    stream_id: u16,
    hop_sequence: u32,
    publisher_region: &'a [u8],
    payload: &'a [u8],
    signature: &'a [u8; SIGNATURE_SIZE],
}

impl<'a> MediaFrameView<'a> {
    #[expect(
        clippy::too_many_arguments,
        reason = "The borrowed view caches all nine parsed spans and scalars so accessors are \
                  infallible; a params struct would only relocate the arity. `pub(crate)` with a \
                  single call site in `codec::build_view`."
    )]
    pub(crate) const fn new(
        frame: &'a [u8],
        layout: Layout,
        wrapped: Option<WrappedTransmitKey<'a>>,
        extensions: Extensions<'a>,
        stream_id: u16,
        hop_sequence: u32,
        publisher_region: &'a [u8],
        payload: &'a [u8],
        signature: &'a [u8; SIGNATURE_SIZE],
    ) -> Self {
        Self {
            frame,
            layout,
            wrapped,
            extensions,
            stream_id,
            hop_sequence,
            publisher_region,
            payload,
            signature,
        }
    }

    /// Always [`PROTOCOL_VERSION`]; any other value is rejected at decode.
    #[must_use]
    pub const fn version(&self) -> u8 {
        PROTOCOL_VERSION
    }

    /// The three defined flags. Undefined bits cannot reach here.
    #[must_use]
    pub const fn flags(&self) -> FrameFlags {
        self.layout.flags
    }

    /// The declared payload length, which is authoritative: it is what
    /// delimits this frame within a group's stream (§2).
    #[must_use]
    pub const fn payload_length(&self) -> usize {
        self.layout.payload_length
    }

    /// The end-to-end, publisher-set sequence — **the AEAD nonce input** (§2,
    /// §4).
    ///
    /// # Invariant 1 (recorded at the field it governs)
    ///
    /// This counter is per **(sender, stream)** and **must never be reset**,
    /// and in particular must not reset at a transmit-key generation boundary.
    /// The transmit-key generation is a *different* counter with a *different*
    /// scope: monotonic per **sender**, shared across all of that sender's
    /// streams, never reset. Two counters, two scopes.
    ///
    /// Resetting the sequence at a generation bump is §2's forbidden
    /// silent-nonce-repeat path. Under AES-GCM a repeated (key, nonce) pair
    /// does not merely expose two frames: it leaks the authentication subkey
    /// and permits forgery. A generation boundary *permits* a reset because
    /// the key changes; §2's rule is to keep counting regardless, both because
    /// resetting at the wrong moment relative to the key swap is exactly how a
    /// silent repeat happens, and because every reset is a discontinuity the
    /// receiver must special-case rather than read as loss.
    ///
    /// **No counter wrapper for this field may be built with a reset API.**
    /// Contrast [`Self::hop_sequence`], where resetting is harmless — the
    /// obvious symmetric type is the catastrophic one.
    #[must_use]
    pub const fn stream_sequence(&self) -> u32 {
        self.layout.stream_sequence
    }

    /// The wrapped transmit key, present iff the key-bearing flag is set.
    #[must_use]
    pub const fn wrapped_transmit_key(&self) -> Option<WrappedTransmitKey<'a>> {
        self.wrapped
    }

    /// The validated TLV extension region. Iteration cannot fail: the region
    /// was fully validated at decode.
    #[must_use]
    pub const fn extensions(&self) -> Extensions<'a> {
        self.extensions
    }

    /// Which of the subscriber's slots this frame fills.
    ///
    /// **Relay region: unauthenticated.** A receiver must validate this
    /// against its own declared slots (§6) and reject unknown values; see the
    /// module docs.
    #[must_use]
    pub const fn stream_id(&self) -> u16 {
        self.stream_id
    }

    /// Per (connection, media stream) count of what the transmitter actually
    /// sent, so intentional gapping by selection is not misread as loss (§2).
    ///
    /// **Resetting this is harmless**: it is unauthenticated, rides inside
    /// QUIC/TLS, and a transmitter lying about its own send count conceals only
    /// drops it could already perform. That is *not* true of
    /// [`Self::stream_sequence`], which is the AEAD nonce input — do not build
    /// a symmetric counter type for the two.
    #[must_use]
    pub const fn hop_sequence(&self) -> u32 {
        self.hop_sequence
    }

    /// The opaque `SFrame`-object payload. Not decrypted, not interpreted.
    #[must_use]
    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }

    /// The trailing Ed25519 signature. Not verified by this crate.
    #[must_use]
    pub const fn signature(&self) -> &'a [u8; SIGNATURE_SIZE] {
        self.signature
    }

    /// The publisher region — **exactly** the AEAD associated data (§4).
    ///
    /// # Invariant 2 (recorded where the spans are produced)
    ///
    /// This is a **slice of the received buffer**, never a re-serialization of
    /// a parsed struct. The same holds for [`Self::payload`]. Do not
    /// reconstruct these bytes by re-encoding a decoded view: this type
    /// deliberately exposes no `to_bytes()` / `signed_bytes()` method, and
    /// [`crate::codec::encode_frame`] must not be used for that purpose.
    ///
    /// The associated data is the publisher region **alone** — a deliberate
    /// strict *subset* of the signed input.
    ///
    /// # This crate cannot establish that the span is *correct*
    ///
    /// Every test in this crate builds its fixtures with this crate's own
    /// encoder, so the suite proves the codec **self-consistent and
    /// fail-closed** — not that the span is the right one. If the encoder and
    /// the decoder computed this region wrongly but *identically* — ending it
    /// one byte short of the extension region, say — then signatures would
    /// **verify successfully while covering the wrong bytes**, leaving a byte
    /// unauthenticated in a header whose entire premise is that a relay cannot
    /// alter it. Nothing here would fail: the roundtrip stays byte-identical,
    /// byte coverage still finds every position load-bearing (it is — just
    /// under the wrong span), and every prefix still returns `Ok(None)`.
    ///
    /// ADR-0036 §2 describes only the loud half of this, where the two
    /// implementations *disagree* and verification fails. The self-consistent
    /// case is silent.
    ///
    /// Correctness of this span is established by the **frozen, externally
    /// anchored** cross-language vectors — specifically the four
    /// associated-data-span rows (key-bearing x extensions-present) that story
    /// task 8 pins. Those rows must not be dropped as redundant with the range
    /// tests in this crate: the range tests cannot see this failure, and
    /// neither could a vector regenerated at test time from this same codec.
    #[must_use]
    pub const fn publisher_region(&self) -> &'a [u8] {
        self.publisher_region
    }

    /// The signed input (§3), in order: publisher region, then payload.
    ///
    /// These two spans are **not contiguous** — the relay region sits between
    /// them, excluded from the signature precisely because a relay rewrites it.
    /// Concatenating them is unambiguous only because `payload_length` and
    /// `ext_length` are themselves inside the signed publisher region, so the
    /// split point is authenticated. Do not "fix" this later by adding a length
    /// prefix or reordering the spans.
    #[must_use]
    pub const fn signed_ranges(&self) -> [&'a [u8]; 2] {
        [self.publisher_region, self.payload]
    }

    /// Byte range of the publisher region within the frame.
    #[must_use]
    pub const fn publisher_region_range(&self) -> Range<usize> {
        0..self.layout.publisher_region_end()
    }

    /// Offset of the relay region within the frame.
    ///
    /// This is **not** a constant: it varies with the key-bearing flag and the
    /// extension length, both of which a publisher controls. Never precompute
    /// it.
    #[must_use]
    pub const fn relay_region_offset(&self) -> usize {
        self.layout.relay_region_offset()
    }

    /// Byte range of the extension-length field within the frame.
    ///
    /// Exposed so that callers which need to address the header's conditional
    /// regions — tests that mutate them, and story task 8's vector generator —
    /// read the offset from a decoded frame instead of reimplementing
    /// "publisher prefix, then the wrapped-key field iff key-bearing". That
    /// reimplementation is a parallel derivation of a conditional layout rule,
    /// and it stays correct only until a conditional field is added ahead of
    /// it, at which point it diverges silently.
    #[must_use]
    pub const fn ext_length_field_range(&self) -> Range<usize> {
        let start = self.layout.ext_start.saturating_sub(EXT_LENGTH_FIELD_SIZE);
        start..self.layout.ext_start
    }

    /// Byte range of the TLV extension region within the frame.
    #[must_use]
    pub const fn extensions_range(&self) -> Range<usize> {
        self.layout.ext_start..self.layout.publisher_region_end()
    }

    /// Byte range of the relay region within the frame.
    #[must_use]
    pub const fn relay_region_range(&self) -> Range<usize> {
        self.layout.relay_region_offset()..self.layout.payload_start()
    }

    /// Byte range of the payload within the frame. Use this with an owning
    /// `Bytes` to obtain a refcount-sharing slice.
    #[must_use]
    pub const fn payload_range(&self) -> Range<usize> {
        self.layout.payload_start()..self.layout.signature_start()
    }

    /// Byte range of the signature within the frame.
    #[must_use]
    pub const fn signature_range(&self) -> Range<usize> {
        self.layout.signature_start()..self.layout.frame_len()
    }

    /// Total on-wire length of this frame.
    #[must_use]
    pub const fn encoded_len(&self) -> usize {
        self.frame.len()
    }

    /// This frame's own bytes, exactly.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.frame
    }
}

impl fmt::Debug for MediaFrameView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaFrameView")
            .field("version", &PROTOCOL_VERSION)
            .field("flags", &self.layout.flags)
            .field("payload_length", &self.layout.payload_length)
            .field("stream_sequence", &self.layout.stream_sequence)
            .field("wrapped_transmit_key", &self.wrapped)
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
