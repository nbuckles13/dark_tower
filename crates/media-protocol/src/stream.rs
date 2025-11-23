//! Media stream management.

/// Media stream configuration
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// User identifier (participant)
    pub user_id: u64,
    /// Stream identifier (subscriber-chosen)
    pub stream_id: u32,
    /// Maximum bitrate in bits per second
    pub max_bitrate: u64,
    /// Whether this is an audio or video stream
    pub is_audio: bool,
}

/// Media stream state
#[derive(Debug)]
pub struct MediaStream {
    /// Stream configuration
    pub config: StreamConfig,
    /// Next expected sequence number
    pub next_sequence: u64,
    /// Number of frames received
    pub frames_received: u64,
    /// Number of bytes received
    pub bytes_received: u64,
}

impl MediaStream {
    /// Create a new media stream
    #[must_use]
    pub const fn new(config: StreamConfig) -> Self {
        Self {
            config,
            next_sequence: 0,
            frames_received: 0,
            bytes_received: 0,
        }
    }

    /// Get the user ID for this stream
    #[must_use]
    pub const fn user_id(&self) -> u64 {
        self.config.user_id
    }

    /// Get the stream ID for this stream
    #[must_use]
    pub const fn stream_id(&self) -> u32 {
        self.config.stream_id
    }
}
