//! # AC Test Utilities
//!
//! Shared test utilities for the Authentication Controller (AC) service.
//!
//! This crate provides:
//! - Deterministic crypto fixtures (fixed keys for reproducible tests)
//! - Test data builders (TestTokenBuilder, etc.)
//! - Server test harness (TestAuthServer for E2E tests)
//! - Fixed test IDs (UUIDs, constants)
//! - Custom assertions (TokenAssertions trait)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ac_test_utils::*;
//!
//! #[tokio::test]
//! async fn test_example() {
//!     // Use deterministic crypto fixtures
//!     let (public_key, private_key) = test_signing_key(1)?;
//!
//!     // Use builder patterns
//!     let token = TestTokenBuilder::new()
//!         .for_user("alice")
//!         .with_scope("user.read.gc")
//!         .build();
//!
//!     // Use custom assertions
//!     token.assert_valid_jwt()
//!          .assert_has_scope("user.read.gc");
//! }
//! ```

pub mod assertions;
pub mod crypto_fixtures;
pub mod server_harness;
pub mod test_ids;
pub mod token_builders;

// Re-export commonly used items
pub use assertions::*;
pub use crypto_fixtures::*;
pub use server_harness::*;
pub use test_ids::*;
pub use token_builders::*;
