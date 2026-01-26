//! # MC Test Utilities
//!
//! Shared test utilities for the Meeting Controller (MC) service.
//!
//! This crate provides mock implementations and test fixtures for
//! isolated MC testing without requiring real infrastructure.
//!
//! ## Modules (ADR-0023 Section 13)
//!
//! - `mock_gc` - Mock Global Controller for MC testing
//! - `mock_mh` - Mock Media Handler for MC testing
//! - `mock_redis` - In-memory Redis mock for state testing
//! - `mock_webtransport` - Mock WebTransport client for signaling tests
//! - `fixtures` - Pre-configured test data (meetings, participants, tokens)
//! - `assertions` - State verification helpers
//!
//! ## Usage
//!
//! ```rust,ignore
//! use mc_test_utils::*;
//!
//! #[tokio::test]
//! async fn test_example() {
//!     // Create mock GC that accepts all registrations
//!     let mock_gc = MockGc::builder()
//!         .accept_registration()
//!         .build();
//!
//!     // Create mock Redis with empty state
//!     let mock_redis = MockRedis::new();
//!
//!     // Create test fixtures
//!     let meeting = TestMeeting::new("meeting-123");
//!     let participant = TestParticipant::new("alice");
//!
//!     // Run your test...
//! }
//! ```
//!
//! ## Test Patterns
//!
//! ### Session Binding Tests
//!
//! ```rust,ignore
//! use mc_test_utils::fixtures::*;
//!
//! let binding_token = TestBindingToken::new()
//!     .with_correlation_id("corr-123")
//!     .with_user_id("user-456")
//!     .build();
//!
//! // Test reconnection flow
//! let reconnect_request = JoinRequestBuilder::new()
//!     .meeting_id("meeting-789")
//!     .correlation_id(binding_token.correlation_id())
//!     .binding_token(binding_token.token())
//!     .build();
//! ```
//!
//! ### Fencing Token Tests
//!
//! ```rust,ignore
//! let mock_redis = MockRedis::new()
//!     .with_fencing_generation("meeting-123", 5);
//!
//! // Attempt write with stale generation
//! let result = mock_redis.fenced_write("meeting-123", 4, "data");
//! assert!(result.is_err()); // Should be fenced out
//! ```

// TODO (Phase 6b): Implement these modules
// pub mod mock_gc;
// pub mod mock_mh;
// pub mod mock_redis;
// pub mod mock_webtransport;
// pub mod fixtures;
// pub mod assertions;

// Placeholder modules for skeleton
pub mod fixtures;
pub mod mock_gc;
pub mod mock_mh;
pub mod mock_redis;

// Re-export commonly used items
pub use fixtures::*;
pub use mock_gc::*;
pub use mock_mh::*;
pub use mock_redis::*;
