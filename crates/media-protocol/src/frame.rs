//! Media frame types and serialization.

use bytes::Bytes;
use common::types::StreamId;

/// Type of media frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// Audio frame
    Audio,
    /// Video keyframe
    VideoKey,
    /// Video delta frame
    VideoDelta,
}

/// A media frame with metadata
#[derive(Debug, Clone)]
pub struct MediaFrame {
    /// Stream identifier
    pub stream_id: StreamId,
    /// Type of frame
    pub frame_type: FrameType,
    /// Timestamp in microseconds
    pub timestamp: u64,
    /// Sequence number
    pub sequence: u64,
    /// Frame payload
    pub payload: Bytes,
}
