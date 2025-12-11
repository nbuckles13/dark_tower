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
