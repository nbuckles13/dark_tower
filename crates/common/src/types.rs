//! Common data types for Dark Tower components.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an organization (tenant)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrganizationId(pub Uuid);

impl OrganizationId {
    /// Create a new random organization ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OrganizationId {
    fn default() -> Self {
        Self::new()
    }
}

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

/// Unique identifier for a participant (session-level, UUID for DB)
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

/// User identifier for media frames (8 bytes, assigned on join)
///
/// This is a random 64-bit number assigned by the Meeting Controller
/// when a participant joins. It uniquely identifies the participant
/// within the meeting and is shared with all other participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub u64);

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl UserId {
    /// Create a new random user ID
    #[must_use]
    pub fn new() -> Self {
        use rand::Rng;
        Self(rand::thread_rng().gen())
    }

    /// Get the underlying u64 value
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stream identifier chosen by subscriber (4 bytes)
///
/// This is a local identifier chosen by the subscriber for their
/// own bookkeeping. The Meeting Controller maintains a mapping
/// between the subscriber's `stream_id` and the actual media source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamId(pub u32);

impl StreamId {
    /// Create a stream ID from a u32
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the underlying u32 value
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}
