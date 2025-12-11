//! Chaos tests for AC service infrastructure resilience
//!
//! These tests validate service behavior under adverse conditions including:
//! - Database connection loss
//! - Key rotation under load
//! - Slow query timeouts (if testable)
//!
//! Chaos tests help ensure the service degrades gracefully during infrastructure
//! failures and maintains consistency during concurrent operations.

mod db_failure_tests;
mod key_rotation_stress_tests;
