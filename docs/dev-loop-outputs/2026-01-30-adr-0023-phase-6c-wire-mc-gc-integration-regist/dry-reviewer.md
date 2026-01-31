# DRY Reviewer Checkpoint - ADR-0023 Phase 6c

**Task**: Wire MC-GC integration (registration, heartbeats, assignment handling, fencing)
**Date**: 2026-01-30
**Reviewer**: DRY Reviewer Specialist
**Review Round**: 2 (includes test infrastructure)

## Files Reviewed

### Round 1 (Source Code)
- `crates/meeting-controller/Cargo.toml`
- `crates/meeting-controller/src/lib.rs`
- `crates/meeting-controller/src/config.rs`
- `crates/meeting-controller/src/main.rs`
- `crates/meeting-controller/src/actors/mod.rs`
- `crates/meeting-controller/src/actors/metrics.rs`
- `crates/meeting-controller/src/grpc/gc_client.rs`
- `crates/meeting-controller/src/system_info.rs`

### Round 2 (Test Infrastructure)
- `crates/meeting-controller/tests/gc_integration.rs` - **NEW**
- `crates/meeting-controller/tests/heartbeat_tasks.rs` - **NEW**
- `crates/meeting-controller/src/grpc/gc_client.rs` - **UPDATED** (retry constants)
- `crates/meeting-controller/src/main.rs` - **UPDATED** (CancellationToken)

## Comparison Services

- `crates/ac-service/` (Authentication Controller)
- `crates/global-controller/` (Global Controller)
- `crates/gc-test-utils/` (GC Test Utilities)
- `crates/common/` (Shared utilities)

---

## Round 2 Verification

### MockGcServer Analysis

The `MockGcServer` in `tests/gc_integration.rs` is **NOT duplicating existing patterns**:

1. **gc-test-utils provides `TestGcServer`**: This is a **real GC server harness** (spawns actual GC with test database), used for E2E testing the GC service itself.

2. **MC's `MockGcServer`**: This is a **mock gRPC service** implementing `GlobalControllerService` trait, used for testing MC's client-side communication with GC.

**These serve different purposes**:
- `TestGcServer` = Real GC for testing GC itself
- `MockGcServer` = Fake GC for testing MC's integration code

**Verdict**: Appropriate separation - no duplication concern.

---

### Heartbeat Test Utilities Analysis

The `heartbeat_tasks.rs` file provides:
- `run_fast_heartbeat_loop()` - Simulates heartbeat loop for testing
- Time-controlled tests using `tokio::test(start_paused = true)`

**Assessment**: These are MC-specific test utilities for verifying heartbeat task behavior. They use tokio's test-util features appropriately and are properly scoped to MC testing needs.

**Verdict**: Appropriately scoped, no duplication.

---

### gc_client.rs Changes

Retry constants updated:
- `MAX_REGISTRATION_RETRIES`: 5 → 20
- **NEW**: `MAX_REGISTRATION_DURATION`: 300s (5 minute deadline)
- Backoff constants unchanged (1s base, 30s max)

**Assessment**: The increased retry resilience is appropriate for GC rolling updates. The duration deadline is a safety cap. Previous TECH_DEBT-006 still applies (backoff logic could be extracted if needed elsewhere).

---

### main.rs Changes

Shutdown mechanism changed:
- **Before**: `watch::channel` for shutdown signaling
- **After**: `CancellationToken` from tokio-util with child tokens

**Assessment**:
- This is an **improvement** - `CancellationToken` provides cleaner hierarchical cancellation
- GC also uses `CancellationToken` (see `crates/global-controller/src/main.rs:116`)
- MC now aligns with GC's pattern

**Verdict**: This REDUCES duplication by using the same pattern as GC.

---

## Round 1 Findings (Unchanged)

### TECH_DEBT-001: Configuration Pattern Duplication

**Severity**: TECH_DEBT
**Location**: `crates/meeting-controller/src/config.rs`, `crates/global-controller/src/config.rs`, `crates/ac-service/src/config.rs`

**Observation**: Each service has its own config module with similar patterns:
- `Config` struct with `from_env()` and `from_vars()` methods
- `ConfigError` enum with `MissingEnvVar`, `InvalidValue` variants
- Custom `Debug` impl that redacts sensitive fields
- Similar parsing logic for environment variables

**Recommendation**: Consider a config builder pattern or derive macro in `common` crate.

---

### TECH_DEBT-002: Shutdown Signal Handler Duplication

**Severity**: TECH_DEBT
**Location**: `crates/meeting-controller/src/main.rs`, `crates/global-controller/src/main.rs`, `crates/ac-service/src/main.rs`

**Observation**: All three services have nearly identical `shutdown_signal()` async functions.

**Note**: MC's version is slightly simpler (no drain period), but the core pattern is the same.

**Recommendation**: Extract to `common::shutdown::shutdown_signal()`.

---

### TECH_DEBT-003: Tracing Initialization Duplication

**Severity**: TECH_DEBT
**Location**: All main.rs files

**Observation**: Identical tracing_subscriber initialization across all services.

**Recommendation**: Extract to `common::observability::init_tracing(default_filter: &str)`.

---

### TECH_DEBT-004: Database Query Timeout Helper Duplication

**Severity**: TECH_DEBT
**Location**: AC and GC main.rs

**Note**: MC does not use PostgreSQL (uses Redis), so no new duplication.

---

### TECH_DEBT-005: Controller ID Generation Pattern

**Severity**: TECH_DEBT
**Location**: MC and GC config.rs

**Observation**: Similar hostname+UUID ID generation pattern.

**Recommendation**: Extract to `common::id::generate_service_id(prefix: &str)`.

---

### TECH_DEBT-006: Exponential Backoff Constants

**Severity**: TECH_DEBT (Minor)
**Location**: `crates/meeting-controller/src/grpc/gc_client.rs`

**Observation**: Backoff constants are hardcoded. Updated in Round 2 with extended retry parameters.

**Note**: Self-contained, future consolidation candidate.

---

## Positive Observations

1. **Proper use of `common::secret::SecretString`**: MC correctly uses the shared secret handling.

2. **No copy-paste of errors module**: MC's `McError` is appropriately different from GC's `GcError`.

3. **Actor metrics are MC-specific**: No inappropriate duplication.

4. **MockGcServer is distinct from TestGcServer**: Different purposes, no duplication.

5. **CancellationToken aligns with GC**: MC now uses the same shutdown pattern as GC.

6. **Heartbeat tests are appropriately scoped**: Use tokio test-util features correctly.

7. **test_config() helper is local**: Only used in MC tests, appropriate scope.

---

## Summary

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | None |
| CRITICAL | 0 | None |
| MAJOR | 0 | None |
| MINOR | 0 | None |
| TECH_DEBT | 6 | Existing patterns for future consolidation |

**New items in Round 2**: 0 (no new duplication introduced)

---

## Verdict: APPROVED

**Rationale**:
- No BLOCKER findings
- Test infrastructure is appropriately scoped and does not duplicate existing patterns
- MockGcServer serves a different purpose than TestGcServer
- CancellationToken change aligns MC with GC's pattern (reduces divergence)
- Previous TECH_DEBT items remain accurate and unchanged

**Previous tech debt items remain valid**:
1. Shutdown signal handling
2. Tracing initialization
3. Service ID generation
4. Config error patterns
5. Database query timeout (AC/GC only)
6. Exponential backoff (self-contained)

These can be addressed in a dedicated "DRY infrastructure cleanup" task after Phase 6 completes.

---

## Reflection Summary

**Knowledge file updates**:
- **patterns.md**: Added 2 entries (Mock vs Real Test Server distinction, CancellationToken pattern)
- **integration.md**: Updated TD-6 file path (`session/actor.rs` -> `actors/metrics.rs`), added TD-11 (Shutdown Signal Handler), added TD-12 (Tracing Initialization)

**Key learnings**:
1. Mock servers (fake trait implementations) vs test harnesses (real service instances) serve different purposes and should not be flagged as duplication
2. CancellationToken with child tokens is now the established shutdown pattern for MC and GC
3. Infrastructure patterns (shutdown, tracing init) are consistent tech debt candidates but low priority

**No entries pruned**: All existing entries remain valid.

---

## Round 3 (Iteration 3 - PR #34 Fixes)

**Date**: 2026-01-31
**Scope**: Re-registration support, unified GC task refactor, metrics snapshot helper

### Files Reviewed (Iteration 3)

1. `crates/meeting-controller/src/errors.rs` - Added `McError::NotRegistered` variant
2. `crates/meeting-controller/src/grpc/gc_client.rs` - NOT_FOUND detection, `attempt_reregistration()` method
3. `crates/meeting-controller/src/main.rs` - Unified GC task refactor (removed Arc)
4. `crates/meeting-controller/src/actors/metrics.rs` - Added `snapshot()` method
5. `crates/meeting-controller/src/actors/mod.rs` - Export `ControllerMetricsSnapshot`

---

### McError::NotRegistered Analysis

**Location**: `crates/meeting-controller/src/errors.rs:27-29`

**Observation**: New unit variant `McError::NotRegistered` added to indicate MC is not registered with GC.

**Cross-service comparison**:
- GC's `GcError` does not have an equivalent variant - appropriate since GC is the server (returns NOT_FOUND via gRPC status)
- AC's `AcError` does not have registration-related errors (AC authenticates via OAuth, not registration)

**Verdict**: This is MC-specific error for client-side detection of NOT_FOUND responses. No duplication.

---

### Re-registration Logic Analysis

**Location**: `crates/meeting-controller/src/grpc/gc_client.rs:468-530`

**New method**: `attempt_reregistration()` - Single-attempt re-registration when heartbeat returns NOT_FOUND.

**Duplication concern**: The `RegisterMcRequest` construction is duplicated between `register()` (line 178-195) and `attempt_reregistration()` (line 471-488).

```rust
// In register()
let request = RegisterMcRequest {
    id: self.config.mc_id.clone(),
    region: self.config.region.clone(),
    grpc_endpoint: format!("http://{}",
        self.config.grpc_bind_address.replace("0.0.0.0", "localhost")),
    webtransport_endpoint: format!("https://{}",
        self.config.webtransport_bind_address.replace("0.0.0.0", "localhost")),
    max_meetings: self.config.max_meetings,
    max_participants: self.config.max_participants,
};

// In attempt_reregistration() - IDENTICAL
let request = RegisterMcRequest { ... }; // Same construction
```

**Assessment**: This is **internal duplication** within `gc_client.rs`. A private helper `fn build_registration_request(&self) -> RegisterMcRequest` could eliminate this.

**Severity**: **TECH_DEBT** (not BLOCKER)
- Only 2 call sites within the same file
- Change together (if config fields change, both would need updating)
- Self-contained, low risk

**Tracking**: TECH_DEBT-007

---

### Response Handling Duplication Analysis

**Observation**: The response handling after `try_register()` is also similar between `register()` and `attempt_reregistration()`:
- Both store heartbeat intervals from response
- Both update `is_registered` flag
- Both log success/failure

**Assessment**: The methods have different retry semantics:
- `register()`: Multi-retry with exponential backoff
- `attempt_reregistration()`: Single-attempt, returns immediately

Extracting common response handling would require careful design to preserve these different behaviors. The duplication is acceptable for clarity.

**Severity**: Not flagged (acceptable duplication for clarity)

---

### Unified GC Task Refactor Analysis

**Location**: `crates/meeting-controller/src/main.rs:199-300`

**Changes**:
- Removed `Arc<GcClient>` wrapping
- Single `run_gc_task()` function owns `gc_client` directly
- Unified registration + dual heartbeat loop
- `handle_heartbeat_error()` helper for NOT_FOUND detection

**Assessment**: This refactor **REDUCES duplication**:
- Previous design may have had separate heartbeat tasks
- Now unified in single `select!` loop
- Helper function `handle_heartbeat_error()` avoids duplicating NOT_FOUND handling in both heartbeat branches

**Verdict**: Positive DRY improvement.

---

### ControllerMetrics::snapshot() Analysis

**Location**: `crates/meeting-controller/src/actors/metrics.rs:291-297`

```rust
pub fn snapshot(&self) -> ControllerMetricsSnapshot {
    ControllerMetricsSnapshot {
        meetings: self.current_meetings.load(Ordering::SeqCst),
        participants: self.current_participants.load(Ordering::SeqCst),
    }
}
```

**Assessment**: This is a simple getter that returns both metrics atomically. The `ControllerMetricsSnapshot` struct is MC-specific.

**Cross-service comparison**: No similar pattern exists in GC or AC (they don't have heartbeat metrics).

**Verdict**: Appropriately scoped, no duplication.

---

### NOT_FOUND Detection Pattern Analysis

**Location**: `gc_client.rs:357-363` (fast_heartbeat), `gc_client.rs:436-442` (comprehensive_heartbeat)

**Observation**: The NOT_FOUND detection code is duplicated in both heartbeat methods:

```rust
if e.code() == tonic::Code::NotFound {
    warn!(target: "mc.grpc.gc_client", "GC returned NOT_FOUND - MC not registered");
    self.is_registered.store(false, Ordering::SeqCst);
    return Err(McError::NotRegistered);
}
```

**Assessment**: This pattern appears twice (in `fast_heartbeat` and `comprehensive_heartbeat`).

**Mitigation**: The caller (`handle_heartbeat_error`) handles both cases uniformly. The duplication within gc_client.rs could be refactored to a helper, but:
- Only 2 occurrences
- In the same file
- Behavior is identical

**Severity**: **TECH_DEBT** (not BLOCKER) - Minor internal duplication

**Tracking**: TECH_DEBT-008

---

## Round 3 Findings Summary

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| TECH_DEBT-007 | TECH_DEBT | `RegisterMcRequest` construction duplicated in register() and attempt_reregistration() | gc_client.rs |
| TECH_DEBT-008 | TECH_DEBT | NOT_FOUND detection duplicated in fast_heartbeat() and comprehensive_heartbeat() | gc_client.rs |

**No BLOCKER findings**

---

## Positive Observations (Round 3)

1. **Unified GC task reduces complexity**: Single task with select loop is cleaner than multiple separate tasks
2. **handle_heartbeat_error() centralizes re-registration logic**: Avoids duplicating in caller
3. **McError::NotRegistered is appropriately MC-specific**: No cross-service duplication
4. **ControllerMetricsSnapshot is well-scoped**: Simple data struct for atomic reads
5. **Arc removal simplifies ownership**: GcClient now has single owner (the task)

---

## Cumulative Summary (Rounds 1-3)

| Severity | Count | New in Round 3 |
|----------|-------|----------------|
| BLOCKER | 0 | 0 |
| TECH_DEBT | 8 | 2 |

**Tech debt items by category**:
- Infrastructure (1-5): Shutdown signal, tracing init, config pattern, service ID, DB timeout
- MC gRPC client (6-8): Exponential backoff, RegisterMcRequest construction, NOT_FOUND detection

---

## Verdict: APPROVED

**Rationale**:
- No BLOCKER findings in Round 3
- New TECH_DEBT items are internal to gc_client.rs (low coupling risk)
- Unified GC task refactor improves DRY (single registration/heartbeat loop)
- McError::NotRegistered is appropriately service-specific
- All duplication is within-file (easy to refactor later if needed)

**Recommendation**: Consider extracting `build_registration_request()` helper when gc_client.rs is next modified.

---

## Round 4 (Iteration 4 - Test Code Review)

**Date**: 2026-01-31
**Scope**: Test code duplication analysis for re-registration tests

### Files Reviewed (Iteration 4)

1. `crates/meeting-controller/src/errors.rs` - Enhanced test (NotRegistered client_message test)
2. `crates/meeting-controller/src/actors/metrics.rs` - New test (test_controller_metrics_snapshot)
3. `crates/meeting-controller/tests/gc_integration.rs` - MockBehavior enum, 4 new re-registration tests

---

### errors.rs Test Enhancement Analysis

**Location**: `crates/meeting-controller/src/errors.rs:239-244`

**Change**: Added test assertion for `McError::NotRegistered.client_message()`:
```rust
// NotRegistered should also hide internal details
let not_registered_err = McError::NotRegistered;
assert_eq!(not_registered_err.client_message(), "An internal error occurred");
```

**Assessment**: This is a single test assertion added to the existing `test_client_messages_hide_internal_details` test. Appropriately extends existing coverage rather than creating a duplicate test function.

**Verdict**: Good - follows existing test structure.

---

### metrics.rs New Test Analysis

**Location**: `crates/meeting-controller/src/actors/metrics.rs:539-563`

**New test**: `test_controller_metrics_snapshot` - Tests the new `snapshot()` method.

**Assessment**:
- Tests initial state, updates via setters, and updates via atomic operations
- Verifies snapshot consistency after each state change
- Does NOT duplicate existing `test_controller_metrics_meetings` or `test_controller_metrics_participants` tests

**Comparison with existing tests**:
- `test_controller_metrics_meetings` (line 523-536): Tests `set_meetings`, `increment_meetings`, `decrement_meetings`
- `test_controller_metrics_participants` (line 566-579): Tests `set_participants`, `increment_participants`, `decrement_participants`
- `test_controller_metrics_snapshot` (new): Tests `snapshot()` returns correct combined values

**Verdict**: Appropriately distinct - new test focuses on snapshot behavior specifically.

---

### MockBehavior Enum Analysis

**Location**: `crates/meeting-controller/tests/gc_integration.rs:36-46`

```rust
enum MockBehavior {
    Accept,        // Normal operation
    Reject,        // Reject registrations
    NotFound,      // Return NOT_FOUND for heartbeats
    NotFoundThenAccept,  // First heartbeat NOT_FOUND, then accept
}
```

**Assessment**: This is an **excellent DRY pattern**:
- Centralizes mock behavior configuration
- Eliminates need for separate mock implementations
- Allows tests to specify exact behavior via `MockGcServer::new_with_behavior()`
- The `fast_heartbeat` and `comprehensive_heartbeat` implementations use pattern matching on this enum

**Comparison to test duplication alternative**: Without `MockBehavior`, each test scenario would require:
- A separate mock struct (e.g., `MockGcServerReject`, `MockGcServerNotFound`)
- Or conditional logic scattered in tests
- The enum approach is clearly superior

**Verdict**: **Positive DRY pattern** - this is exemplary test design.

---

### Re-registration Test Suite Analysis

The 4 new tests are:

1. **test_heartbeat_not_found_detection** (line 547-578)
   - Tests fast heartbeat returning NOT_FOUND
   - Verifies McError::NotRegistered is returned
   - Verifies is_registered flag is cleared

2. **test_comprehensive_heartbeat_not_found_detection** (line 580-611)
   - Tests comprehensive heartbeat returning NOT_FOUND
   - Same assertions as #1 but for different heartbeat type

3. **test_attempt_reregistration_success** (line 613-641)
   - Tests re-registration when not previously registered
   - Verifies registration succeeds and heartbeats work

4. **test_attempt_reregistration_after_not_found** (line 643-676)
   - Tests the full flow: heartbeat NOT_FOUND -> re-register -> heartbeat works
   - Uses NotFoundThenAccept behavior

**Distinctness Analysis**:

| Test | Heartbeat Type | Initial State | Mock Behavior | Verifies |
|------|---------------|---------------|---------------|----------|
| #1 | Fast | Registered | NotFound | Error detection |
| #2 | Comprehensive | Registered | NotFound | Error detection |
| #3 | N/A | Unregistered | Accept | Re-reg alone works |
| #4 | Fast | Registered | NotFoundThenAccept | Full recovery flow |

**Assessment**: Tests #1 and #2 are structurally similar but test different code paths (fast vs comprehensive heartbeat). This is appropriate - both heartbeat methods have NOT_FOUND detection logic and both should be tested.

**Potential Concern**: Tests #1 and #2 share nearly identical structure:
```rust
// Both tests follow this pattern:
// 1. Create mock with NotFound behavior
// 2. Start server, create client, register
// 3. Call heartbeat (fast or comprehensive)
// 4. Assert NotRegistered error
// 5. Assert is_registered is false
// 6. Cancel token
```

**Assessment**: The structural similarity is acceptable because:
- Each tests a distinct method (`fast_heartbeat` vs `comprehensive_heartbeat`)
- The NOT_FOUND detection code is duplicated in gc_client.rs (TECH_DEBT-008)
- Until that code is refactored, both code paths need separate tests
- Test clarity trumps DRY for test code

**Verdict**: Tests are appropriately distinct. Structural similarity is justified.

---

### Test Setup/Teardown Duplication Analysis

**Pattern observed**: All integration tests follow this setup pattern:
```rust
let mock_gc = MockGcServer::...;
let (addr, cancel_token) = start_mock_gc_server(mock_gc).await;
let gc_url = format!("http://{addr}");
let config = test_config(&gc_url);
let gc_client = GcClient::new(gc_url, SecretString::from("test-token"), config).await.unwrap();
// ... test logic ...
cancel_token.cancel();
```

**Assessment**:
- `test_config()` helper (line 240-258) eliminates Config construction duplication - **good**
- `start_mock_gc_server()` helper (line 260-283) eliminates server setup duplication - **good**
- `cancel_token.cancel()` is a 1-line teardown - acceptable

**Potential improvement**: A test fixture or `TestContext` struct could further consolidate:
```rust
struct TestContext {
    gc_client: GcClient,
    cancel_token: CancellationToken,
}
impl Drop for TestContext { ... } // Auto-cancel
```

**Severity**: Not flagged (current helpers are sufficient, fixture would be over-engineering)

---

## Round 4 Findings Summary

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| - | - | No new findings | - |

**No BLOCKER or TECH_DEBT findings in Round 4**

---

## Positive Observations (Round 4)

1. **MockBehavior enum is excellent DRY design**: Centralizes mock configuration, eliminates separate mock classes
2. **test_config() helper**: Eliminates Config construction boilerplate
3. **start_mock_gc_server() helper**: Eliminates server setup boilerplate
4. **Tests are appropriately distinct**: Each tests a different scenario/code path
5. **New snapshot test doesn't duplicate existing metric tests**: Focused on new functionality
6. **Error test enhancement integrated into existing test**: No new test function needed

---

## Cumulative Summary (Rounds 1-4)

| Severity | Count | New in Round 4 |
|----------|-------|----------------|
| BLOCKER | 0 | 0 |
| TECH_DEBT | 8 | 0 |

---

## Verdict: APPROVED

**Rationale**:
- No BLOCKER findings in Round 4
- No new TECH_DEBT introduced
- MockBehavior enum is an exemplary DRY pattern for test configuration
- Four re-registration tests are appropriately distinct (different heartbeat types, different scenarios)
- Test helpers (test_config, start_mock_gc_server) appropriately consolidate common setup
- Structural similarity between tests #1 and #2 is justified (testing different code paths)

---

## Final Reflection (Post-Review)

**Date**: 2026-01-31
**Rounds Completed**: 4 (all APPROVED)
**Total Findings**: 8 TECH_DEBT, 0 BLOCKER

### Knowledge File Updates

**patterns.md**:
- Added "MockBehavior Enum for Test Flexibility" - Enum-based mock configuration eliminates separate mock classes
- Added "Unified Task Pattern for Concurrent Responsibilities" - Single task with select! reduces Arc duplication and centralizes error handling
- Added "Test Helper Functions for Setup Boilerplate" - Extract common setup (test_config, start_mock_gc_server) to reduce test boilerplate

**gotchas.md**:
- Added "Acceptable Internal Duplication (Same-File, Same-Purpose)" - 2 occurrences in same file = TECH_DEBT (not BLOCKER). Examples: NOT_FOUND detection in both heartbeat methods, RegisterMcRequest construction in register() and attempt_reregistration()
- Added "Test Code Structural Similarity is Often Justified" - Tests prioritize clarity over DRY. Structural similarity acceptable if testing different code paths (e.g., fast vs comprehensive heartbeat)

**integration.md**:
- Added "Refactors That Improve DRY" - Track DRY improvements across iterations (Round 3 removed Arc duplication, centralized re-registration logic)

### Key Learnings from ADR-0023 Phase 6c

1. **Unified task pattern is superior to multiple spawned tasks**: MC's `run_gc_task()` demonstrates that consolidating related responsibilities (registration + dual heartbeats) into a single select! loop eliminates Arc duplication and centralizes error handling. This is a pattern worth replicating in other services.

2. **MockBehavior enum is exemplary test design**: Instead of creating separate mock implementations (MockGcServerReject, MockGcServerNotFound), a single mock with behavior enum provides flexibility without duplication. This is now the recommended pattern for test infrastructure.

3. **Acceptable internal duplication threshold**: 2 occurrences within the same file (e.g., NOT_FOUND detection in fast_heartbeat and comprehensive_heartbeat) should be flagged as TECH_DEBT but not block. The pattern must appear 3+ times or cross file boundaries to escalate to BLOCKER.

4. **Test clarity trumps DRY**: Tests #1 (fast_heartbeat NOT_FOUND) and #2 (comprehensive_heartbeat NOT_FOUND) are structurally similar but appropriately distinct - they test different code paths. Combining them would reduce clarity.

5. **Track DRY improvements in iterations**: Round 3 refactor actively improved DRY (removed Arc, unified task, centralized error handling). Explicitly noting this in checkpoints reinforces good refactoring patterns.

### Pattern Evolution

This review reinforced the distinction between:
- **Harmful duplication**: Copy-pasted business logic requiring extraction (BLOCKER)
- **Acceptable internal duplication**: Small patterns (2 occurrences, same file) that could be refactored but don't block (TECH_DEBT)
- **Healthy alignment**: Convention-based patterns that should remain service-specific (not flagged)

The unified task pattern from Round 3 is now documented as a recommended approach for services with multiple concurrent responsibilities.

---

## Cumulative Tech Debt Summary

All 8 TECH_DEBT items remain valid and tracked for future consolidation:

**Infrastructure (TD-1 to TD-5, TD-11, TD-12)**: Shutdown signal, tracing init, config patterns, service ID generation, DB timeout
**MC gRPC Client (TD-7, TD-8)**: RegisterMcRequest construction duplication, NOT_FOUND detection duplication

These can be addressed in a dedicated "DRY infrastructure cleanup" task after Phase 6 completes.
