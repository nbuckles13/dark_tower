//! Common test utilities
//!
//! Re-exports from ac-test-utils for convenience in integration and E2E tests.
//! Plus own-crate test scaffolding module (`test_state`) introduced for
//! ADR-0032 Step 4 metric-test backfill.

// Re-export everything from ac-test-utils
#[allow(unused_imports)]
pub use ac_test_utils::*;

pub mod jwt_fixtures;
pub mod test_state;
