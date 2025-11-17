//! Codec for encoding and decoding media frames.

use crate::frame::MediaFrame;
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
}

/// Encode a media frame to bytes
///
/// # Errors
///
/// Returns an error if encoding fails
pub fn encode_frame(_frame: &MediaFrame) -> Result<Bytes, CodecError> {
    // TODO: Implement frame encoding
    let mut buf = BytesMut::new();
    buf.put_u8(0); // Placeholder
    Ok(buf.freeze())
}

/// Decode a media frame from bytes
///
/// # Errors
///
/// Returns an error if decoding fails
pub fn decode_frame(_data: &mut impl Buf) -> Result<MediaFrame, CodecError> {
    // TODO: Implement frame decoding
    Err(CodecError::InsufficientData)
}
