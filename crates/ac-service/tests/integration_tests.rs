//! Integration tests for AC service
//!
//! This is the top-level integration test harness that Cargo discovers.
//! Test modules are organized in the integration/ subdirectory.

#[path = "integration/key_rotation_tests.rs"]
mod key_rotation_tests;

#[path = "integration/health_tests.rs"]
mod health_tests;

#[path = "integration/admin_auth_tests.rs"]
mod admin_auth_tests;

#[path = "integration/clock_skew_tests.rs"]
mod clock_skew_tests;

#[path = "integration/user_auth_tests.rs"]
mod user_auth_tests;

#[path = "integration/internal_token_tests.rs"]
mod internal_token_tests;
