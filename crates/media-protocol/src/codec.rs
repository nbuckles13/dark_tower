//! Codec for encoding and decoding media frames.

use crate::frame::{FrameFlags, FrameType, MediaFrame};
use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Error type for codec operations
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// Insufficient data to decode
    #[error("Insufficient data")]
    InsufficientData,

    /// Invalid frame format
    #[error("Invalid frame format: {0}")]
    InvalidFormat(String),

    /// Unsupported version
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u8),

    /// Invalid frame type
    #[error("Invalid frame type: {0}")]
    InvalidFrameType(u8),
}

/// Encode a media frame to bytes
///
/// # Errors
///
/// Returns an error if encoding fails
pub fn encode_frame(frame: &MediaFrame) -> Result<Bytes, CodecError> {
    let payload_len = frame.payload.len();
    let total_len = MediaFrame::HEADER_SIZE + payload_len;

    let mut buf = BytesMut::with_capacity(total_len);

    // Version (1 byte)
    buf.put_u8(frame.version);

    // Frame Type (1 byte)
    buf.put_u8(frame.frame_type as u8);

    // User ID (8 bytes)
    buf.put_u64(frame.user_id);

    // Stream ID (4 bytes)
    buf.put_u32(frame.stream_id);

    // Timestamp (8 bytes)
    buf.put_u64(frame.timestamp);

    // Sequence Number (8 bytes)
    buf.put_u64(frame.sequence);

    // Payload Length (4 bytes)
    buf.put_u32(payload_len as u32);

    // Flags (2 bytes)
    buf.put_u16(frame.flags.to_u16());

    // Reserved (6 bytes)
    buf.put_bytes(0, 6);

    // Payload
    buf.extend_from_slice(&frame.payload);

    Ok(buf.freeze())
}

/// Decode a media frame from bytes
///
/// # Errors
///
/// Returns an error if decoding fails
pub fn decode_frame(data: &mut impl Buf) -> Result<MediaFrame, CodecError> {
    // Check if we have enough data for the header
    if data.remaining() < MediaFrame::HEADER_SIZE {
        return Err(CodecError::InsufficientData);
    }

    // Version (1 byte)
    let version = data.get_u8();
    if version != MediaFrame::VERSION {
        return Err(CodecError::UnsupportedVersion(version));
    }

    // Frame Type (1 byte)
    let frame_type = match data.get_u8() {
        0x00 => FrameType::Audio,
        0x01 => FrameType::VideoKey,
        0x02 => FrameType::VideoDelta,
        other => return Err(CodecError::InvalidFrameType(other)),
    };

    // User ID (8 bytes)
    let user_id = data.get_u64();

    // Stream ID (4 bytes)
    let stream_id = data.get_u32();

    // Timestamp (8 bytes)
    let timestamp = data.get_u64();

    // Sequence Number (8 bytes)
    let sequence = data.get_u64();

    // Payload Length (4 bytes)
    let payload_len = data.get_u32() as usize;

    // Flags (2 bytes)
    let flags = FrameFlags::from_u16(data.get_u16());

    // Reserved (6 bytes) - skip
    data.advance(6);

    // Check if we have enough data for the payload
    if data.remaining() < payload_len {
        return Err(CodecError::InsufficientData);
    }

    // Payload
    let mut payload_buf = vec![0u8; payload_len];
    data.copy_to_slice(&mut payload_buf);
    let payload = Bytes::from(payload_buf);

    Ok(MediaFrame {
        version,
        user_id,
        stream_id,
        frame_type,
        timestamp,
        sequence,
        flags,
        payload,
    })
}
