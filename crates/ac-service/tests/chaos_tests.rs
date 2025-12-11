//! Chaos tests for AC service infrastructure resilience
//!
//! This is the top-level chaos test harness that Cargo discovers.
//! Test modules are organized in the chaos/ subdirectory.

#[path = "chaos/db_failure_tests.rs"]
mod db_failure_tests;

#[path = "chaos/key_rotation_stress_tests.rs"]
mod key_rotation_stress_tests;
