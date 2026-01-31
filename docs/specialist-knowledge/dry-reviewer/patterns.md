# DRY Reviewer - Patterns That Work

This file captures successful patterns and approaches discovered during DRY reviews.

---

## Architectural Alignment vs. Harmful Duplication

**Added**: 2026-01-29
**Related files**: `crates/env-tests/src/cluster.rs`, `crates/ac-service/src/repositories/*.rs`, `crates/global-controller/src/services/*.rs`

**Pattern**: The `.map_err(|e| ErrorType::Variant(format!("context: {}", e)))` error preservation pattern appears across all services (AC, MC, GC, env-tests) with 40+ instances. This is **healthy architectural alignment**, NOT harmful duplication requiring extraction. Each crate should define its own domain-specific error types (`AcError`, `GcError`, `ClusterError`) while following the same error preservation convention. Extracting this to a macro or shared utility would add complexity without reducing maintenance burden.

**Classification per ADR-0019**: Healthy pattern replication (following a convention) vs. harmful duplication (copy-paste code needing extraction).

---

## Mock vs Real Test Server Distinction

**Added**: 2026-01-30
**Related files**: `crates/meeting-controller/tests/gc_integration.rs`, `crates/gc-test-utils/src/server_harness.rs`

**Pattern**: When reviewing test infrastructure, distinguish between:
- **Mock servers** (e.g., `MockGcServer`): Fake implementations of service interfaces for testing client code
- **Real test servers** (e.g., `TestGcServer`): Actual service instances with test databases for E2E testing

These serve different purposes and are NOT duplication even if both involve the same service. MockGcServer tests MC's client-side GC integration by implementing `GlobalControllerService` trait. TestGcServer tests GC itself by spawning a real GC instance.

**Rule**: If the test server implements a gRPC/HTTP trait to fake behavior, it's a mock. If it spawns the actual service binary/routes, it's a test harness.

---

## CancellationToken for Hierarchical Shutdown

**Added**: 2026-01-30
**Related files**: `crates/meeting-controller/src/main.rs`, `crates/global-controller/src/main.rs`

**Pattern**: Both MC and GC use `tokio_util::sync::CancellationToken` with child tokens for graceful shutdown propagation. This is a healthy alignment - both services now follow the same shutdown pattern. Child tokens enable hierarchical cancellation where parent token cancellation automatically propagates to all children.

**When reviewing**: If a new service uses `watch::channel` or similar for shutdown, recommend aligning with the CancellationToken pattern used by GC and MC.

---
