# DRY Reviewer - Patterns That Work

This file captures successful patterns and approaches discovered during DRY reviews.

---

## Architectural Alignment vs. Harmful Duplication

**Added**: 2026-01-29
**Related files**: `crates/env-tests/src/cluster.rs`, `crates/ac-service/src/repositories/*.rs`, `crates/gc-service/src/services/*.rs`

**Pattern**: The `.map_err(|e| ErrorType::Variant(format!("context: {}", e)))` error preservation pattern appears across all services (AC, MC, GC, env-tests) with 40+ instances. This is **healthy architectural alignment**, NOT harmful duplication requiring extraction. Each crate should define its own domain-specific error types (`AcError`, `GcError`, `ClusterError`) while following the same error preservation convention. Extracting this to a macro or shared utility would add complexity without reducing maintenance burden.

**Classification per ADR-0019**: Healthy pattern replication (following a convention) vs. harmful duplication (copy-paste code needing extraction).

---

## Mock vs Real Test Server Distinction

**Added**: 2026-01-30
**Related files**: `crates/mc-service/tests/gc_integration.rs`, `crates/gc-test-utils/src/server_harness.rs`

**Pattern**: When reviewing test infrastructure, distinguish between:
- **Mock servers** (e.g., `MockGcServer`): Fake implementations of service interfaces for testing client code
- **Real test servers** (e.g., `TestGcServer`): Actual service instances with test databases for E2E testing

These serve different purposes and are NOT duplication even if both involve the same service. MockGcServer tests MC's client-side GC integration by implementing `GlobalControllerService` trait. TestGcServer tests GC itself by spawning a real GC instance.

**Rule**: If the test server implements a gRPC/HTTP trait to fake behavior, it's a mock. If it spawns the actual service binary/routes, it's a test harness.

---

## CancellationToken for Hierarchical Shutdown

**Added**: 2026-01-30
**Related files**: `crates/mc-service/src/main.rs`, `crates/gc-service/src/main.rs`

**Pattern**: Both MC and GC use `tokio_util::sync::CancellationToken` with child tokens for graceful shutdown propagation. This is a healthy alignment - both services now follow the same shutdown pattern. Child tokens enable hierarchical cancellation where parent token cancellation automatically propagates to all children.

**When reviewing**: If a new service uses `watch::channel` or similar for shutdown, recommend aligning with the CancellationToken pattern used by GC and MC.

---

## MockBehavior Enum for Test Flexibility

**Added**: 2026-01-31
**Related files**: `crates/mc-service/tests/gc_integration.rs:36-46`

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
**Related files**: `crates/mc-service/src/main.rs:199-300`

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
**Related files**: `crates/mc-service/tests/gc_integration.rs:240-283`

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
**Related files**: `crates/mc-service/src/grpc/gc_client.rs:648-658`, `crates/mc-service/tests/gc_integration.rs:268-278`

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

## Check Common Crate First for BLOCKING vs TECH_DEBT Classification

**Added**: 2026-02-04
**Related files**: `crates/common/src/lib.rs`

**Pattern**: When reviewing new service code, always check what modules exist in `crates/common/src/` BEFORE classifying duplication severity:

1. **Read `common/src/lib.rs`** to see exported modules (error, types, config, secret, jwt, token_manager)
2. **If pattern exists in common but wasn't imported** -> BLOCKING (mistake, should have used shared code)
3. **If pattern exists in another service but NOT in common** -> TECH_DEBT (opportunity for future extraction)

**Example from GC metrics review**: Common crate has NO observability module. GC correctly implemented its own metrics. This is TECH_DEBT (extraction opportunity), not BLOCKING (mistake).

**Why this matters**: BLOCKING stops the devloop and requires immediate fixes. TECH_DEBT is documented for follow-up. Incorrect classification wastes time (false BLOCKING) or misses extraction opportunities (missed BLOCKING).

**Checklist before marking BLOCKING**:
- [ ] Pattern exists in `crates/common/src/`
- [ ] Service did not import from common
- [ ] Import would have worked (correct signature/types)

---

## Dual Metrics Facades (Internal Tracking + Prometheus Emission)

**Added**: 2026-02-05
**Related files**: `crates/mc-service/src/actors/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`

**Pattern**: Separate internal atomic metrics tracking (in actor structs) from Prometheus emission (in observability module):
- **Internal tier**: `ActorMetrics`, `MailboxMonitor` - Atomic counters for fast lock-free updates
- **Emission tier**: `prom::set_*()`, `prom::record_*()` - Low-level Prometheus metric calls

Each actor method updates internal state AND calls the corresponding `prom::*()` function:

```rust
pub fn meeting_created(&self) {
    let count = self.active_meetings.fetch_add(1, Ordering::Relaxed) + 1;
    prom::set_meetings_active(count as u64);  // Dual emission
}
```

**Benefits**:
- Clean separation: Business logic owns internal counters, observability owns Prometheus wiring
- No complex cascading calls - direct update + direct emission is clear
- Pattern validated across multiple metric types (gauges, counters, histograms)
- Ordered consistency: Internal state updated first, then Prometheus

**When to use**: Services with actor systems or background tasks that emit metrics. Each lifecycle event (create/destroy/update) should update both tiers.

**When NOT to use**: Simple request/response metrics (no internal state needed) - emit directly to Prometheus.

---

## Metrics Recording at Operation Boundaries

**Added**: 2026-02-10
**Related files**: `crates/mc-service/src/grpc/gc_client.rs:363-388, 453-479`, `crates/gc-service/src/repositories/meeting_controllers.rs:119-158`

**Pattern**: Record metrics immediately at success/error branches with timing captured via `Instant::now()` before operation and `start.elapsed()` after. Common for gRPC calls, database queries, and external service interactions:

```rust
let start = Instant::now();
match operation().await {
    Ok(result) => {
        let duration = start.elapsed();
        record_counter("success", type);
        record_histogram(type, duration);
        // handle success
    }
    Err(e) => {
        let duration = start.elapsed();
        record_counter("error", type);
        record_histogram(type, duration);
        // handle error
    }
---

## Test Fixture Helper Variants for Precondition Splits

**Added**: 2026-02-18
**Related files**: `crates/env-tests/tests/21_cross_service_flows.rs:29-38`, `crates/env-tests/tests/22_mc_gc_integration.rs:30-39`, `crates/env-tests/tests/00_cluster_health.rs:12-16`

**Pattern**: When integration test files share a common setup helper (`cluster()`) but different files require different service preconditions, create two variants of the helper rather than a single parameterized version:

- **Simple variant** (AC-only tests): `ClusterConnection::new().await.expect("...")`
- **Extended variant** (GC-dependent tests): Simple variant + `cluster.check_gc_health().await.expect("GC must be running...")`

**Benefits**:
- Each test file declares its preconditions upfront in the helper
- No silent skips -- missing services cause immediate test failure
- No unnecessary precondition checks in tests that don't need them
- The two variants are self-documenting about which services each test file requires

**This is NOT duplication** because:
- The variants differ semantically (different precondition sets)
- Extracting a parameterized `cluster_with_services(&[Service::GC])` adds complexity for 2 variants
- Each variant is used by multiple test files (5 files use simple, 2 use extended)

**When to extract**: If a third variant appears (e.g., MC-dependent tests needing `check_mc_health()`), consider a builder pattern or parameterized helper. Two variants is the sweet spot where explicit code is clearer than abstraction.

---

## Closure-Based Generic Extraction for Same-Crate Duplication

**Added**: 2026-02-12 | **Updated**: 2026-02-12 (iteration 2: simplified API)
**Related files**: `crates/gc-service/src/tasks/generic_health_checker.rs`, `crates/gc-service/src/tasks/health_checker.rs`, `crates/gc-service/src/tasks/mh_health_checker.rs`

**Pattern**: When two modules in the same crate have 90%+ structural similarity differing only in which repository/function they call, extract the shared logic into a generic function parameterized by a closure, with thin wrapper functions preserving the original API. Pass domain-differentiating values as plain parameters, not config structs:

```rust
// Generic: closure-based, zero-cost abstraction, plain parameters
pub async fn start_generic_health_checker<F, Fut>(
    pool: PgPool,
    staleness_threshold_seconds: u64,
    cancel_token: CancellationToken,
    entity_name: &'static str,
    mark_stale_fn: F,
) where
    F: Fn(PgPool, i64) -> Fut + Send,
    Fut: Future<Output = Result<u64, GcError>> + Send,
{ /* shared loop logic */ }

// Thin wrapper: preserves original signature, uses .instrument() chaining
pub async fn start_health_checker(pool: PgPool, threshold: u64, cancel: CancellationToken) {
    // lifecycle log with literal target:
    info!(target: "gc.task.health_checker", "Starting health checker task");
    start_generic_health_checker(pool, threshold, cancel, "controllers",
        |pool, threshold| async move {
            MeetingControllersRepository::mark_stale_controllers_unhealthy(&pool, threshold).await
        },
    )
    .instrument(tracing::info_span!("gc.task.health_checker"))
    .await;
    // lifecycle log with literal target:
    info!(target: "gc.task.health_checker", "Health checker task stopped");
}
```

**Benefits**:
- Records both success and error latencies (important for SLO tracking)
- Counter increments track operation frequency
- Histogram captures latency distribution
- No metrics missed even on error paths

**Variations**:
- **Combined function** (AC/GC): Single `record_http_request(method, endpoint, status, duration)` records counter + histogram
- **Separate functions** (MC): `record_gc_heartbeat(status, type)` for counter, `record_gc_heartbeat_latency(type, duration)` for histogram

**When reviewing**: Both patterns are valid per ADR-0011. Separate functions allow recording only counter OR only histogram if needed. Combined functions reduce duplication at call sites. Note as TECH_DEBT (not BLOCKER) if inconsistent across services.
- Zero-cost: monomorphized at compile time, no trait objects or dynamic dispatch
- No breaking changes: wrapper signatures identical to originals, call sites untouched
- Tests remain in wrapper modules, exercising full pipeline through the wrappers
- Plain `&'static str` parameter instead of config struct -- simpler API, no boilerplate

**When to use**: Same-crate modules with 90%+ structural similarity where differences are limited to which function is called and what strings appear in logs. Repository method signatures must be compatible.

**When NOT to use**: Cross-crate duplication (use trait in `common` instead). Modules with <80% similarity or different control flow (not just different function calls). Cases where the generic function would need complex trait bounds or lifetime gymnastics.

**Design notes**:
- Use `.instrument(tracing::info_span!("name"))` chaining on wrappers, NOT `#[instrument]` on the generic function -- avoids nested spans and keeps the generic function free of tracing concerns
- Use `&'static str` for log targets since tracing macros require static strings
- Lifecycle logs (startup/shutdown) stay in wrappers with literal `target:` values; inner loop logs use structured fields (`entity = entity_name`)
- Prefer plain parameters over config structs when 1-2 domain values differentiate the generic -- config structs add indirection for no benefit (see "Config Struct vs Plain Parameters" gotcha)

---
