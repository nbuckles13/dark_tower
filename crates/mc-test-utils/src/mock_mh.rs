//! Mock Media Handler for MC testing.
//!
//! Provides a mock MH implementation that can be configured to:
//! - Accept or reject participant registration
//! - Simulate capacity constraints
//! - Return specific load states
//!
//! # Example
//!
//! ```rust,ignore
//! use mc_test_utils::MockMh;
//!
//! let mock_mh = MockMh::builder()
//!     .id("mh-test-1")
//!     .accept_registration()
//!     .with_capacity(100, 50) // max_streams, current_streams
//!     .build();
//!
//! // Use mock_mh in your tests...
//! ```

// TODO (Phase 6d): Full implementation with gRPC service

/// Mock Media Handler for testing MC-MH coordination.
#[derive(Debug)]
pub struct MockMh {
    id: String,
    accept_registration: bool,
    max_streams: u32,
    current_streams: u32,
}

impl Default for MockMh {
    fn default() -> Self {
        Self {
            id: "mh-test-default".to_string(),
            accept_registration: true,
            max_streams: 1000,
            current_streams: 0,
        }
    }
}

impl MockMh {
    /// Create a new MockMh builder.
    #[must_use]
    pub fn builder() -> MockMhBuilder {
        MockMhBuilder::default()
    }

    /// Get the MH ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Check if this mock accepts registrations.
    #[must_use]
    pub fn accepts_registration(&self) -> bool {
        self.accept_registration
    }

    /// Get the maximum stream capacity.
    #[must_use]
    pub fn max_streams(&self) -> u32 {
        self.max_streams
    }

    /// Get the current stream count.
    #[must_use]
    pub fn current_streams(&self) -> u32 {
        self.current_streams
    }

    /// Check if the MH is at capacity.
    #[must_use]
    pub fn is_at_capacity(&self) -> bool {
        self.current_streams >= self.max_streams
    }

    /// Get the capacity utilization percentage.
    #[must_use]
    pub fn utilization_percent(&self) -> f32 {
        if self.max_streams == 0 {
            return 100.0;
        }
        (self.current_streams as f32 / self.max_streams as f32) * 100.0
    }
}

/// Builder for MockMh configuration.
#[derive(Debug, Default)]
pub struct MockMhBuilder {
    id: Option<String>,
    accept_registration: bool,
    max_streams: u32,
    current_streams: u32,
}

impl MockMhBuilder {
    /// Set the MH ID.
    #[must_use]
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Configure the mock to accept participant registration.
    #[must_use]
    pub fn accept_registration(mut self) -> Self {
        self.accept_registration = true;
        self
    }

    /// Configure the mock to reject participant registration.
    #[must_use]
    pub fn reject_registration(mut self) -> Self {
        self.accept_registration = false;
        self
    }

    /// Set capacity configuration.
    #[must_use]
    pub fn with_capacity(mut self, max_streams: u32, current_streams: u32) -> Self {
        self.max_streams = max_streams;
        self.current_streams = current_streams;
        self
    }

    /// Build the MockMh.
    #[must_use]
    pub fn build(self) -> MockMh {
        MockMh {
            id: self.id.unwrap_or_else(|| "mh-test-default".to_string()),
            accept_registration: self.accept_registration,
            max_streams: self.max_streams,
            current_streams: self.current_streams,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_mh_builder() {
        let mh = MockMh::builder()
            .id("mh-test-1")
            .accept_registration()
            .with_capacity(100, 50)
            .build();

        assert_eq!(mh.id(), "mh-test-1");
        assert!(mh.accepts_registration());
        assert_eq!(mh.max_streams(), 100);
        assert_eq!(mh.current_streams(), 50);
        assert!(!mh.is_at_capacity());
        assert!((mh.utilization_percent() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mock_mh_at_capacity() {
        let mh = MockMh::builder().with_capacity(100, 100).build();

        assert!(mh.is_at_capacity());
        assert!((mh.utilization_percent() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mock_mh_default() {
        let mh = MockMh::default();

        assert_eq!(mh.id(), "mh-test-default");
        assert!(mh.accepts_registration());
        assert_eq!(mh.max_streams(), 1000);
        assert_eq!(mh.current_streams(), 0);
    }
}
