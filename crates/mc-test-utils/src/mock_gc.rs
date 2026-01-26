//! Mock Global Controller for MC testing.
//!
//! Provides a mock GC implementation that can be configured to:
//! - Accept or reject MC registration
//! - Return specific MH assignments for meetings
//! - Simulate various failure scenarios
//!
//! # Example
//!
//! ```rust,ignore
//! use mc_test_utils::MockGc;
//!
//! let mock_gc = MockGc::builder()
//!     .accept_registration()
//!     .with_mh_assignments("meeting-123", vec!["mh-1", "mh-2"])
//!     .build();
//!
//! // Use mock_gc in your tests...
//! ```

// TODO (Phase 6b): Full implementation

/// Mock Global Controller for testing MC registration and heartbeats.
#[derive(Debug, Default)]
pub struct MockGc {
    accept_registration: bool,
    // TODO: Add more configuration fields
}

impl MockGc {
    /// Create a new MockGc builder.
    #[must_use]
    pub fn builder() -> MockGcBuilder {
        MockGcBuilder::default()
    }

    /// Create a MockGc that accepts all registrations.
    #[must_use]
    pub fn accepting() -> Self {
        Self {
            accept_registration: true,
        }
    }

    /// Create a MockGc that rejects all registrations.
    #[must_use]
    pub fn rejecting() -> Self {
        Self {
            accept_registration: false,
        }
    }

    /// Check if this mock accepts registrations.
    #[must_use]
    pub fn accepts_registration(&self) -> bool {
        self.accept_registration
    }
}

/// Builder for MockGc configuration.
#[derive(Debug, Default)]
pub struct MockGcBuilder {
    accept_registration: bool,
}

impl MockGcBuilder {
    /// Configure the mock to accept MC registration.
    #[must_use]
    pub fn accept_registration(mut self) -> Self {
        self.accept_registration = true;
        self
    }

    /// Configure the mock to reject MC registration.
    #[must_use]
    pub fn reject_registration(mut self) -> Self {
        self.accept_registration = false;
        self
    }

    /// Build the MockGc.
    #[must_use]
    pub fn build(self) -> MockGc {
        MockGc {
            accept_registration: self.accept_registration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_gc_builder() {
        let gc = MockGc::builder().accept_registration().build();
        assert!(gc.accepts_registration());

        let gc = MockGc::builder().reject_registration().build();
        assert!(!gc.accepts_registration());
    }

    #[test]
    fn test_mock_gc_shortcuts() {
        assert!(MockGc::accepting().accepts_registration());
        assert!(!MockGc::rejecting().accepts_registration());
    }
}
