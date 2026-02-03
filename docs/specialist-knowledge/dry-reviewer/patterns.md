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

## MockBehavior Enum for Test Flexibility

**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs:36-46`

**Pattern**: Use an enum to centralize mock server behavior configuration instead of creating separate mock implementations. The `MockBehavior` enum allows a single `MockGcServer` to simulate different scenarios:
- `Accept` - Normal operation
- `Reject` - Reject registrations
- `NotFound` - Return NOT_FOUND for heartbeats
- `NotFoundThenAccept` - First heartbeat fails, then succeeds (re-registration flow)

**Benefits**:
- Eliminates need for separate mock classes (`MockGcServerReject`, `MockGcServerNotFound`, etc.)
- Tests specify exact behavior declaratively: `MockGcServer::new_with_behavior(MockBehavior::NotFound)`
- Implementation uses pattern matching on the enum in `fast_heartbeat()` and `comprehensive_heartbeat()`
- Easy to add new behaviors without creating new mock types

**Alternative (anti-pattern)**: Creating separate mock structs for each scenario or using conditional flags scattered in tests.

**When reviewing**: If a test suite creates multiple mock implementations of the same trait, recommend consolidating into a single mock with behavior enum.

---

## Unified Task Pattern for Concurrent Responsibilities

**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/src/main.rs:199-300`

**Pattern**: When a component has multiple related responsibilities (e.g., registration + dual heartbeats), use a unified task with `tokio::select!` instead of spawning separate tasks. The MC's `run_gc_task()` demonstrates:
- Single task owns `GcClient` directly (no Arc needed)
- Initial registration loop with cancellation
- Dual heartbeat loop (fast + comprehensive) in single select with separate tickers
- Never exits on transient errors (protects active meetings during GC outages)
- Helper function (`handle_heartbeat_error`) centralizes error handling for both heartbeat types

**Benefits**:
- Reduces duplication (single registration/heartbeat logic)
- Simplifies ownership (task owns client, no Arc)
- Centralizes error handling (NOT_FOUND detection in one place)
- Clear lifecycle (registration → heartbeat loop → shutdown)

**When NOT to use**: If responsibilities are truly independent and don't share state/client.

**When reviewing**: If you see multiple spawned tasks sharing Arc-wrapped clients and handling similar errors, suggest unifying into single task with select.

---

## Test Helper Functions for Setup Boilerplate

**Added**: 2026-01-31
**Related files**: `crates/meeting-controller/tests/gc_integration.rs:240-283`

**Pattern**: Extract common test setup into helper functions to eliminate boilerplate:
- `test_config(gc_url: &str) -> Config` - Creates test configuration with consistent defaults
- `start_mock_gc_server(mock: MockGcServer) -> (SocketAddr, CancellationToken)` - Starts mock server, returns address and cleanup token

**Benefits**:
- Tests focus on behavior, not setup
- Consistent configuration across tests
- Easy to update defaults (change in one place)
- Automatic cleanup via CancellationToken

**When NOT to use**: For test fixtures requiring complex state or lifecycle management (consider `TestContext` struct with `Drop` impl instead).

**When reviewing**: If tests have identical setup patterns (>5 lines), recommend extracting to helper function. But don't recommend over-engineering (e.g., builder patterns for simple config).

---

## OnceLock for Static Test Fixtures

**Added**: 2026-02-02
**Related files**: `crates/meeting-controller/src/grpc/gc_client.rs:648-658`, `crates/meeting-controller/tests/gc_integration.rs:268-278`

**Pattern**: When creating test helper functions that return cloneable handles (e.g., `TokenReceiver`, channel receivers), use `std::sync::OnceLock` to hold a static sender that keeps the channel alive across test invocations:

```rust
fn mock_token_receiver() -> TokenReceiver {
    use std::sync::OnceLock;
    static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();
    let sender = TOKEN_SENDER.get_or_init(|| {
        let (tx, _rx) = watch::channel(SecretString::from("test-token"));
        tx
    });
    TokenReceiver::from_test_channel(sender.subscribe())
}
```

**Benefits**:
- Avoids memory leaks from `mem::forget` (anti-pattern)
- Sender lives for process lifetime (static)
- Multiple test invocations share the same channel
- Thread-safe initialization via OnceLock

**When to use**: Test helpers returning watch/broadcast receivers, test fixtures needing process-wide singletons.

**When NOT to use**: Production code (prefer explicit lifetime management), test fixtures needing per-test isolation.

---

## Re-Export with Rename for Backwards Compatibility

**Added**: 2026-01-31
**Related files**: `crates/ac-service/src/config.rs`, `crates/common/src/jwt.rs`

When extracting duplicated code to a common crate, use `pub use` with rename to maintain backwards compatibility in consuming crates:

```rust
// In common/src/jwt.rs (new canonical location)
pub const DEFAULT_CLOCK_SKEW_SECONDS: i64 = 300;
pub const MAX_CLOCK_SKEW_SECONDS: i64 = 600;

// In ac-service/src/config.rs (backwards compat re-export)
pub use common::jwt::{
    DEFAULT_CLOCK_SKEW_SECONDS as DEFAULT_JWT_CLOCK_SKEW_SECONDS,
    MAX_CLOCK_SKEW_SECONDS as MAX_JWT_CLOCK_SKEW_SECONDS,
};
```

**Benefits**:
1. Existing code using `ac_service::config::DEFAULT_JWT_CLOCK_SKEW_SECONDS` continues to work
2. New code can use canonical `common::jwt::DEFAULT_CLOCK_SKEW_SECONDS`
3. No breaking changes to public API
4. Clear migration path - rename can be deprecated later

**When to use**: Consolidating constants, types, or functions into common while maintaining API stability.

---
