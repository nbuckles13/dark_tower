//! Media frame types and serialization.

use bytes::Bytes;

/// Type of media frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    /// Audio frame
    Audio = 0x00,
    /// Video keyframe
    VideoKey = 0x01,
    /// Video delta frame
    VideoDelta = 0x02,
}

/// Frame flags
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameFlags {
    /// End of frame marker
    pub end_of_frame: bool,
    /// Frame can be discarded without affecting others
    pub discardable: bool,
}

impl FrameFlags {
    /// Convert flags to u16
    #[must_use]
    pub const fn to_u16(self) -> u16 {
        let mut flags = 0u16;
        if self.end_of_frame {
            flags |= 0x0001;
        }
        if self.discardable {
            flags |= 0x0002;
        }
        flags
    }

    /// Parse flags from u16
    #[must_use]
    pub const fn from_u16(value: u16) -> Self {
        Self {
            end_of_frame: (value & 0x0001) != 0,
            discardable: (value & 0x0002) != 0,
        }
    }
}

/// A media frame with metadata
///
/// Frame format (42 bytes header):
/// - Version: 1 byte
/// - Frame Type: 1 byte
/// - User ID: 8 bytes (participant identifier)
/// - Stream ID: 4 bytes (subscriber-chosen identifier)
/// - Timestamp: 8 bytes (microseconds since epoch)
/// - Sequence Number: 8 bytes
/// - Payload Length: 4 bytes
/// - Flags: 2 bytes
/// - Reserved: 6 bytes
/// - Payload: variable (SFrame encrypted)
#[derive(Debug, Clone)]
pub struct MediaFrame {
    /// Protocol version (currently 1)
    pub version: u8,
    /// User identifier (participant who published this stream)
    pub user_id: u64,
    /// Stream identifier (chosen by subscriber)
    pub stream_id: u32,
    /// Type of frame
    pub frame_type: FrameType,
    /// Timestamp in microseconds since epoch
    pub timestamp: u64,
    /// Sequence number
    pub sequence: u64,
    /// Frame flags
    pub flags: FrameFlags,
    /// Frame payload (encrypted)
    pub payload: Bytes,
}

impl MediaFrame {
    /// Header size in bytes
    pub const HEADER_SIZE: usize = 42;

    /// Current protocol version
    pub const VERSION: u8 = 1;
}
