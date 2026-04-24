//! Observability utilities shared across Dark Tower services.
//!
//! This module is a home for cross-service observability primitives that
//! must not live inside a single service crate. At present it hosts only
//! the test-side `MetricAssertion` helper (see [`testing`]).
//!
//! The `testing` submodule is only compiled when `cfg(test)` is active or
//! the `test-utils` feature is enabled, so production builds of consumer
//! services do not pull in `metrics-util` or the `metrics` facade through
//! this crate.

#[cfg(any(test, feature = "test-utils"))]
pub mod testing;
