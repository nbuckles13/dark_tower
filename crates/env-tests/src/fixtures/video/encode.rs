//! One video frame through the production codec. Never a hand-rolled header.

use bytes::Bytes;
use media_protocol::codec::{encode_frame, MediaFrameParts};
use media_protocol::frame::{
    FrameFlags, KEY_ID_BYTES, KEY_ID_GENERATION_BITS, KEY_ID_SENDER_ID_BITS, KEY_ID_STREAM_BITS,
};

use crate::fixtures::media::{fixture_signature, HOP_SENTINEL};

/// The payload fill after the key id. Opaque to MH.
const VIDEO_MARKER: u8 = 0x56;

/// Everything one frame's bytes depend on.
#[derive(Clone, Copy)]
pub struct VideoFrameSpec {
    /// Meeting-scoped sender id, packed into the payload's key id.
    pub sender_id: u16,
    /// The DIRECTED send id: the key id's `stream` component (never the label).
    pub key_id_stream: u16,
    /// The uplink relay `stream_id` actually stamped (the label knob).
    pub label: u16,
    /// Per-stream frame sequence.
    pub stream_sequence: u32,
    /// The group's counter.
    pub gop_counter: u32,
    /// Whether this frame starts a group (independently decodable).
    pub group_start: bool,
    /// Payload length in bytes; at least [`KEY_ID_BYTES`].
    pub payload_len: usize,
}

/// Pack `sender_id | stream | generation=0` into the 8-byte key id, from
/// `media_protocol::frame`'s `KEY_ID_*` widths. `generation` is a PLACEHOLDER
/// constant, not a real generation.
///
/// ANCHOR (DRY): a deliberate TWIN of `crates/media-vector-gen/src/kid.rs`
/// (`KeyIdParts::pack`) and `packages/sdk-core/src/media/frame/keyId.ts`, kept
/// equal in shape (checked conversions, sender id 0 refused) rather than by
/// import: vector-gen is a non-production generator scoped to its spec
/// computations and carries crypto dependencies a keyless fixture must not
/// link, and the SDK packer is TypeScript. The shared home would be
/// `media-protocol` (a Guarded Shared Area) — tracked in `docs/TODO.md`
/// §Cross-Service Duplication.
///
/// # Panics
///
/// If `sender_id` is 0 (never a valid sender — `JoinResponse.sender_id`), or
/// `stream` does not fit [`KEY_ID_STREAM_BITS`] (never masked).
#[must_use]
pub fn pack_key_id(sender_id: u16, stream: u16) -> [u8; KEY_ID_BYTES] {
    assert!(sender_id != 0, "sender id 0 never names a sender");
    // The widths are compile-time facts of frame.rs; assert the ones this
    // packing relies on rather than restating them.
    const _: () = assert!(KEY_ID_SENDER_ID_BITS == 16 && KEY_ID_STREAM_BITS == 8);
    let stream = u8::try_from(stream).expect("the key-id stream component must fit its field");
    let sender_shift =
        u32::try_from(KEY_ID_STREAM_BITS + KEY_ID_GENERATION_BITS).expect("shift fits u32");
    let stream_shift = u32::try_from(KEY_ID_GENERATION_BITS).expect("shift fits u32");
    let generation: u64 = 0;
    let packed =
        (u64::from(sender_id) << sender_shift) | (u64::from(stream) << stream_shift) | generation;
    packed.to_be_bytes()
}

/// The key id's `sender_id` component, read from the first [`KEY_ID_BYTES`] of
/// a payload; `None` if fewer bytes are given.
#[must_use]
pub fn key_id_sender(payload_prefix: &[u8]) -> Option<u16> {
    let kid: [u8; KEY_ID_BYTES] = payload_prefix.get(..KEY_ID_BYTES)?.try_into().ok()?;
    let shift = u32::try_from(KEY_ID_STREAM_BITS + KEY_ID_GENERATION_BITS).ok()?;
    u16::try_from(u64::from_be_bytes(kid) >> shift).ok()
}

/// Encode one video frame through `media_protocol::codec::encode_frame`.
///
/// # Panics
///
/// If `payload_len < KEY_ID_BYTES` or the codec refuses the frame — fixture
/// defects, never data-dependent.
#[must_use]
pub fn encode_video_frame(spec: &VideoFrameSpec) -> Bytes {
    assert!(
        spec.payload_len >= KEY_ID_BYTES,
        "video fixture payload must hold at least the key id"
    );
    let mut payload = vec![VIDEO_MARKER; spec.payload_len];
    payload[..KEY_ID_BYTES].copy_from_slice(&pack_key_id(spec.sender_id, spec.key_id_stream));
    let signature = fixture_signature();
    encode_frame(&MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: spec.group_start,
            discardable: false,
            key_bearing: false,
        },
        stream_sequence: spec.stream_sequence,
        gop_counter: spec.gop_counter,
        wrapped_transmit_key: None,
        extensions: &[],
        stream_id: spec.label,
        hop_sequence: HOP_SENTINEL,
        payload: &payload,
        signature: &signature,
    })
    .expect("video fixture frame must encode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::media::{AUDIO_SEND_ID, VIDEO_SEND_ID};
    use media_protocol::codec::{decode_datagram, decode_stream_frame};

    fn spec() -> VideoFrameSpec {
        VideoFrameSpec {
            sender_id: 0x8001,
            key_id_stream: VIDEO_SEND_ID,
            label: VIDEO_SEND_ID,
            stream_sequence: 41,
            gop_counter: 7,
            group_start: true,
            payload_len: 200,
        }
    }

    #[test]
    fn key_id_packs_sender_then_stream_then_zero_generation() {
        assert_eq!(pack_key_id(0x8001, 1), [0x80, 0x01, 0x01, 0, 0, 0, 0, 0]);
        assert_eq!(pack_key_id(0xFFFF, 0), [0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
        for id in [1_u16, 0x00FF, 0x0100, 0x8000, 0xFFFF] {
            assert_eq!(key_id_sender(&pack_key_id(id, 1)), Some(id));
        }
        assert_eq!(
            key_id_sender(&[0x80, 0x01, 0x01]),
            None,
            "short prefix is no sender"
        );
    }

    #[test]
    #[should_panic(expected = "key-id stream component must fit")]
    fn a_stream_wider_than_its_field_is_never_masked() {
        let _ = pack_key_id(1, 0x0100);
    }

    #[test]
    fn a_group_start_frame_decodes_to_exactly_the_claimed_fields() {
        let raw = encode_video_frame(&spec());
        let view = decode_stream_frame(&raw)
            .expect("decodes")
            .expect("complete");
        assert_eq!(view.encoded_len(), raw.len());
        assert_eq!(
            view.stream_id(),
            VIDEO_SEND_ID,
            "default label = directed send id"
        );
        assert_eq!(view.stream_sequence(), 41);
        assert_eq!(view.gop_counter(), 7);
        assert_eq!(view.hop_sequence(), HOP_SENTINEL);
        let f = view.flags();
        assert!(f.independently_decodable && !f.key_bearing && !f.discardable);
        assert!(view.wrapped_transmit_key().is_none(), "keyless fixture");
        assert_eq!(view.payload().len(), 200);
        assert_eq!(key_id_sender(view.payload()), Some(0x8001));
        assert_eq!(view.payload()[2], 1, "key-id stream = the directed send id");
    }

    #[test]
    fn a_mid_group_frame_sets_no_flag_and_keeps_the_group_counter() {
        let raw = encode_video_frame(&VideoFrameSpec {
            group_start: false,
            ..spec()
        });
        let view = decode_stream_frame(&raw).unwrap().unwrap();
        assert_eq!(view.flags(), FrameFlags::default());
        assert_eq!(view.gop_counter(), 7);
    }

    #[test]
    fn the_label_knob_changes_only_the_relay_id_never_the_key_id_stream() {
        for label in [AUDIO_SEND_ID, 0x7777] {
            let raw = encode_video_frame(&VideoFrameSpec { label, ..spec() });
            let view = decode_stream_frame(&raw).unwrap().unwrap();
            assert_eq!(view.stream_id(), label);
            assert_eq!(
                view.payload()[2],
                1,
                "key-id stream stays the directed send id"
            );
        }
    }

    #[test]
    fn the_same_frame_is_a_valid_datagram() {
        let raw = encode_video_frame(&VideoFrameSpec {
            label: AUDIO_SEND_ID,
            ..spec()
        });
        let view = decode_datagram(&raw).expect("one frame is one datagram");
        assert_eq!(view.stream_id(), AUDIO_SEND_ID);
        assert_eq!(key_id_sender(view.payload()), Some(0x8001));
    }

    #[test]
    #[should_panic(expected = "at least the key id")]
    fn a_payload_shorter_than_the_key_id_is_refused() {
        let _ = encode_video_frame(&VideoFrameSpec {
            payload_len: KEY_ID_BYTES - 1,
            ..spec()
        });
    }
}
