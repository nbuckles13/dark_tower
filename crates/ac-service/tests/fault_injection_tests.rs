//! Fault injection tests for AC service resilience
//!
//! These are **programmatic fault injection tests** that simulate failures
//! within the application (e.g., closing database pool connections).
//!
//! NOTE: These are NOT infrastructure-level chaos tests. For true chaos testing
//! (stopping containers, network partitions, etc.), see ADR-0012 which specifies
//! LitmusChaos for Kubernetes-native chaos experiments in `infra/chaos/`.
//!
//! Test modules are organized in the fault_injection/ subdirectory.

#[path = "fault_injection/db_failure_tests.rs"]
mod db_failure_tests;

#[path = "fault_injection/key_rotation_stress_tests.rs"]
mod key_rotation_stress_tests;
