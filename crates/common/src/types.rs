//! Common data types for Dark Tower components.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a meeting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MeetingId(pub Uuid);

impl MeetingId {
    /// Create a new random meeting ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MeetingId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a participant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParticipantId(pub Uuid);

impl ParticipantId {
    /// Create a new random participant ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ParticipantId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a media stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamId(pub Uuid);

impl StreamId {
    /// Create a new random stream ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for StreamId {
    fn default() -> Self {
        Self::new()
    }
}
