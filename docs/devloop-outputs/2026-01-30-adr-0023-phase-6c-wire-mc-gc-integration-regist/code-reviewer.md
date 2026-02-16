# Code Review: ADR-0023 Phase 6c - Wire MC-GC Integration

**Reviewer**: Code Reviewer Specialist
**Date**: 2026-01-30 (Round 1), 2026-01-31 (Round 2)
**Verdict**: APPROVED

---

## Review Round 2 (2026-01-31)

### Changes Reviewed
1. `main.rs` - Replaced `watch::channel<bool>` with `CancellationToken` hierarchy
2. `gc_client.rs` - Improved retry constants (5->20 retries, 5-minute deadline)
3. `tests/gc_integration.rs` - New integration test file
4. `tests/heartbeat_tasks.rs` - New heartbeat task test file

### CancellationToken Pattern Verification

The refactor from `watch::channel<bool>` to `CancellationToken` is **correctly implemented**:

- `shutdown_token` is created as a child of `controller_handle.child_token()` (line 137)
- Heartbeat tasks use `shutdown_token.child_token()` (lines 145-146)
- gRPC server uses `shutdown_token.child_token()` (line 240)
- Cancellation uses `cancelled().await` pattern in `tokio::select!` (correct usage)
- Shutdown triggers via `shutdown_token.cancel()` (line 269)

**Benefits of this pattern**:
- Consistent with actor system pattern (controller already uses `CancellationToken`)
- Hierarchical cancellation propagation (parent->children)
- Simpler than watch channel (no need to check/compare boolean value)
- Better semantics (cancellation is a one-time event, not a state change)

### Retry Constant Improvements

The gc_client.rs changes are **well-documented and reasonable**:

```rust
const MAX_REGISTRATION_RETRIES: u32 = 20;  // Was 5
const MAX_REGISTRATION_DURATION: Duration = Duration::from_secs(300); // New: 5-minute deadline
```

- Documentation explains rationale (surviving GC rolling updates)
- New test `test_total_retry_duration_sufficient` validates >= 3 minutes of retry time
- `test_backoff_eventually_caps` verifies backoff ceiling behavior

### New Test Files Quality

**`tests/gc_integration.rs`** - Excellent quality:
- Mock GC server with builder pattern for configuration
- Tests registration success/rejection/content
- Tests heartbeat flows (fast, comprehensive, skipped when not registered)
- Tests heartbeat interval configuration from GC
- Concurrent metrics update test
- Proper use of `CancellationToken` for mock server lifecycle

**`tests/heartbeat_tasks.rs`** - Excellent quality:
- Uses `tokio::test(start_paused = true)` for deterministic time control
- Tests interval timing with `tokio::time::advance()`
- Tests cancellation propagation (parent->child token)
- Tests metrics capture during heartbeat
- Tests multiple independent heartbeat tasks
- Correctly uses `MissedTickBehavior::Burst` for test predictability (with comment explaining why)

### No New Issues Introduced

The changes maintain all previous quality standards:
- Error handling remains correct (Result types, no panics)
- Observability unchanged (tracing, no PII)
- Concurrency patterns improved (consistent CancellationToken usage)

### Round 2 Verdict: APPROVED

No new findings. Previous TECH_DEBT items remain acceptable as documented.

---

## Review Round 1 (2026-01-30)

## Summary

This implementation wires the Meeting Controller (MC) to Global Controller (GC) integration including registration, heartbeats, meeting assignment handling, and fencing. The code quality is excellent with proper error handling, consistent patterns, and good observability. No blocking issues were found.

## Files Reviewed

1. `crates/meeting-controller/Cargo.toml`
2. `crates/meeting-controller/src/lib.rs`
3. `crates/meeting-controller/src/config.rs`
4. `crates/meeting-controller/src/main.rs`
5. `crates/meeting-controller/src/actors/mod.rs`
6. `crates/meeting-controller/src/actors/metrics.rs`
7. `crates/meeting-controller/src/grpc/gc_client.rs`
8. `crates/meeting-controller/src/system_info.rs`
9. `crates/meeting-controller/src/grpc/mod.rs` (referenced)
10. `crates/meeting-controller/src/grpc/mc_service.rs` (referenced)
11. `crates/meeting-controller/src/grpc/auth_interceptor.rs` (referenced)
12. `crates/meeting-controller/src/redis/client.rs` (referenced)
13. `crates/meeting-controller/src/errors.rs` (referenced)
14. `crates/meeting-controller/src/actors/controller.rs` (referenced)

## Principles Verification

### Error Handling (no panics, Result types)
- **PASS**: All operations return `Result<T, McError>` types
- **PASS**: `expect()` usage in `main.rs` is properly documented with `#[expect]` attributes explaining why panic is acceptable (signal handler installation failure is unrecoverable)
- **PASS**: Test code is properly gated with `#[allow(clippy::unwrap_used, clippy::expect_used)]`
- **PASS**: Error mapping is consistent - internal errors are logged but client-safe messages are returned

### Observability (proper tracing, no PII)
- **PASS**: All modules use structured logging with `tracing`
- **PASS**: Sensitive data is redacted in Debug output (Config implements custom Debug that redacts redis_url, binding_token_secret, service_token)
- **PASS**: SecretString is used for all credentials
- **PASS**: Redis URLs are not logged (comment in client.rs line 99-100 documents this)
- **PASS**: Consistent target naming: `mc.grpc.gc_client`, `mc.actor.controller`, etc.

### Concurrency (actor patterns, no blocking)
- **PASS**: Proper actor model with message passing via `tokio::sync::mpsc`
- **PASS**: tonic Channel is documented as cheaply cloneable - no locking needed
- **PASS**: MultiplexedConnection is documented as cheaply cloneable - no locking needed
- **PASS**: Heartbeat tasks use `tokio::select!` for cancellation-aware operation
- **PASS**: Background cleanup tasks spawned with `tokio::spawn` to avoid blocking

## Findings

### TECH_DEBT Findings

#### TECH_DEBT-001: Hardcoded master secret placeholder in main.rs

**File**: `crates/meeting-controller/src/main.rs:105`
**Severity**: TECH_DEBT

```rust
let master_secret = SecretBox::new(Box::new(vec![0u8; 32])); // TODO: Load from config
```

**Observation**: The master secret for session binding tokens is hardcoded to zeros with a TODO comment. This is acceptable for Phase 6c but must be addressed before production use.

**Recommendation**: Wire the master secret to Config (the field `binding_token_secret` exists, just decode base64 and use it).

---

#### TECH_DEBT-002: Auth interceptor not wired to gRPC server

**File**: `crates/meeting-controller/src/main.rs:244-249`
**Severity**: TECH_DEBT

```rust
let grpc_server = tonic::transport::Server::builder()
    .add_service(MeetingControllerServiceServer::new(mc_assignment_service))
    .serve_with_shutdown(grpc_addr, async move {
        ...
    });
```

**Observation**: The `McAuthInterceptor` is implemented but not wired to the gRPC server. The code comment in `auth_interceptor.rs` mentions Phase 6h for full JWKS integration.

**Recommendation**: Wire the interceptor to the server once JWKS integration is complete.

---

#### TECH_DEBT-003: CPU precision loss cast without full documentation

**File**: `crates/meeting-controller/src/main.rs:203-206`
**Severity**: TECH_DEBT

```rust
#[allow(clippy::cast_precision_loss)]
let cpu = sys_info.cpu_percent as f32;
#[allow(clippy::cast_precision_loss)]
let memory = sys_info.memory_percent as f32;
```

**Observation**: The allow attribute has a comment explaining why precision loss is acceptable, but it could be more explicit about the value ranges.

**Recommendation**: Consider adding `// 0-100 range, always fits exactly in f32` or documenting in SystemInfo that values are clamped.

---

## Code Quality Highlights

### Positive Observations

1. **Excellent documentation**: All modules have comprehensive doc comments explaining purpose, architecture decisions, and usage patterns.

2. **Proper secret handling**: SecretString and SecretBox are used consistently for credentials with custom Debug implementations that redact sensitive data.

3. **Consistent error handling patterns**: McError enum maps cleanly to signaling error codes with client-safe messages.

4. **Well-structured actor model**: Clear separation between handles and actors, proper cancellation token propagation.

5. **Good test coverage**: Unit tests cover key logic paths including edge cases and boundary conditions.

6. **Defensive programming**: Values are clamped (SystemInfo), saturating arithmetic is used (capacity checks), and error paths are handled gracefully.

7. **tonic/redis patterns**: Code correctly documents that Channel and MultiplexedConnection are cheaply cloneable and don't need locking.

8. **Heartbeat implementation**: Proper use of `tokio::select!` for cancellation-aware heartbeat loops with missed tick behavior configured.

## Verdict

**APPROVED**

The implementation demonstrates excellent code quality with proper error handling, observability, and concurrency patterns. All findings are TECH_DEBT level, representing known gaps that are documented with TODO comments or explained in module documentation. These are acceptable for Phase 6c and can be addressed in subsequent phases.

## Finding Summary

| Severity | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 0 |
| TECH_DEBT | 3 |

---

## Reflection (2026-01-31)

### Knowledge File Review

Reviewed existing knowledge files:
- `patterns.md` (23 entries)
- `gotchas.md` (17 entries)
- `integration.md` (7 entries)

### Evaluation Against This Review

**Patterns observed in this review:**
1. CancellationToken hierarchy for shutdown (controller -> shutdown -> tasks)
2. Retry with both count limit and duration deadline
3. Mock server with builder pattern for integration tests
4. `tokio::test(start_paused = true)` for deterministic time tests

**Decision: No changes to knowledge files**

Rationale:
- CancellationToken pattern is already covered in `integration.md` under "Actor Hierarchy (Phase 6b)"
- Retry patterns are standard distributed systems practices, not Dark Tower specific
- Mock server and time control patterns are well-documented tokio patterns
- No stale entries found requiring pruning

### Knowledge Changes

```
added: 0
updated: 0
pruned: 0
```

Existing knowledge coverage is comprehensive for the patterns in this implementation.

---

## Review Round 3 (Iteration 3 Fixes) - 2026-01-31

### Changes Reviewed

**Iteration 3 focused on re-registration support and unified GC task refactor:**

1. `errors.rs` - Added `McError::NotRegistered` variant
2. `gc_client.rs` - NOT_FOUND detection, `attempt_reregistration()` method
3. `main.rs` - Unified GC task refactor (single task owns gc_client, no Arc)
4. `actors/metrics.rs` - Added `ControllerMetrics::snapshot()` helper
5. `actors/mod.rs` - Export `ControllerMetricsSnapshot`

### Code Quality Evaluation

#### 1. McError::NotRegistered (errors.rs)

**PASS**: Clean addition of new error variant
- Properly placed in enum (after Grpc error, logical grouping)
- Correct error_code mapping (6 = INTERNAL_ERROR)
- Client-safe message hides internal state ("An internal error occurred")
- Test updated to cover new variant (line 178)

#### 2. NOT_FOUND Detection (gc_client.rs)

**PASS**: Correct pattern for detecting GC restart/partition
- Checks `e.code() == tonic::Code::NotFound` before generic error handling
- Updates `is_registered` to false atomically
- Returns `McError::NotRegistered` for caller to handle
- Consistent implementation in both `fast_heartbeat()` and `comprehensive_heartbeat()`

#### 3. attempt_reregistration() Method (gc_client.rs)

**PASS**: Well-designed single-attempt re-registration
- Clear doc comment explaining it's for heartbeat loop use (single attempt, caller handles retry)
- Reuses `try_register()` for consistency
- Updates intervals from GC response on success
- Proper logging at info/warn levels
- `#[instrument]` for tracing context

#### 4. Unified GC Task (main.rs)

**PASS**: Excellent refactor addressing Round 1/2 feedback
- `gc_client` owned directly by task (no Arc needed)
- Correct startup order: gRPC server starts BEFORE GC registration
- Never-exit resilience in registration loop (logs and retries on failure)
- Single `tokio::select!` for dual heartbeat intervals
- Uses `MissedTickBehavior::Skip` (correct for production)
- `handle_heartbeat_error()` cleanly encapsulates re-registration logic

**Specific quality observations:**
```rust
// Line 217-238: Registration loop never exits on error
loop {
    tokio::select! {
        () = cancel_token.cancelled() => { return; }
        result = gc_client.register() => {
            match result {
                Ok(()) => break, // Proceed to heartbeat loop
                Err(e) => {
                    warn!(error = %e, "GC task: Initial registration failed, will retry");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}
```
This is the correct pattern: protects active meetings during GC outages.

#### 5. ControllerMetrics::snapshot() (metrics.rs)

**PASS**: Clean helper for atomic metric capture
- Returns `ControllerMetricsSnapshot` struct (immutable copy)
- Uses `SeqCst` ordering for both reads (consistent with write ordering)
- Well-documented purpose in doc comment
- `#[must_use]` attribute (good practice)

#### 6. Export in mod.rs

**PASS**: Properly exports `ControllerMetricsSnapshot` for use in main.rs

### Previous TECH_DEBT Items Status

Reviewing Round 1 TECH_DEBT items:

| Item | Status | Notes |
|------|--------|-------|
| TECH_DEBT-001: Hardcoded master secret | **Still present** | Line 105 in main.rs still has `vec![0u8; 32]` placeholder |
| TECH_DEBT-002: Auth interceptor not wired | **Still present** | gRPC server still doesn't use `McAuthInterceptor` |
| TECH_DEBT-003: CPU precision loss cast | **IMPROVED** | Now has better comment: "CPU and memory are 0-100, no precision loss in f32 range" (line 277) |

All previous TECH_DEBT items are acceptable for Phase 6c.

### New Observations

#### Positive Highlights

1. **Clean separation of concerns**: `handle_heartbeat_error()` function keeps the heartbeat loop clean
2. **Consistent error handling**: NOT_FOUND check before generic error in both heartbeat methods
3. **No Arc overhead**: Unified task design avoids unnecessary Arc for gc_client
4. **Resilient by design**: Never-exit loops protect active meetings during GC issues
5. **Good tracing**: Appropriate log levels (info for success, warn for failures)

#### No New Issues

No BLOCKER, CRITICAL, MAJOR, or MINOR issues found. The iteration 3 changes maintain the high code quality established in iterations 1-2.

### Round 3 Verdict: APPROVED

The iteration 3 changes are well-implemented with correct patterns for:
- Re-registration support (NOT_FOUND detection, single-attempt re-registration)
- Unified GC task (no Arc, correct startup order, never-exit resilience)
- Atomic metrics snapshot for consistent heartbeat reporting

Previous TECH_DEBT items remain acceptable for Phase 6c scope.

---

## Finding Summary (Cumulative)

| Severity | Round 1 | Round 2 | Round 3 | Total |
|----------|---------|---------|---------|-------|
| BLOCKER | 0 | 0 | 0 | 0 |
| CRITICAL | 0 | 0 | 0 | 0 |
| MAJOR | 0 | 0 | 0 | 0 |
| MINOR | 0 | 0 | 0 | 0 |
| TECH_DEBT | 3 | 0 | 0 | 3 |

**Final Verdict: APPROVED**

---

## Review Round 4 (Iteration 4 - Test Code Quality) - 2026-01-31

### Changes Reviewed

**Iteration 4 focused on test infrastructure enhancements:**

1. `errors.rs` - Enhanced test for `NotRegistered` client message
2. `actors/metrics.rs` - New `test_controller_metrics_snapshot()` test
3. `tests/gc_integration.rs` - `MockBehavior` enum, 4 new re-registration tests

### Test Code Quality Evaluation

#### 1. Enhanced Error Test (errors.rs)

**File**: `crates/meeting-controller/src/errors.rs` lines 239-245

**PASS**: Clean test addition for `NotRegistered` client message hiding

```rust
// NotRegistered should also hide internal details
let not_registered_err = McError::NotRegistered;
assert_eq!(
    not_registered_err.client_message(),
    "An internal error occurred"
);
```

- Follows existing test pattern in `test_client_messages_hide_internal_details`
- Clear comment explaining intent
- Appropriate assertion (exact string match for security-critical behavior)

#### 2. New Snapshot Test (metrics.rs)

**File**: `crates/meeting-controller/src/actors/metrics.rs` lines 539-563

**PASS**: Comprehensive test for `snapshot()` method

```rust
#[test]
fn test_controller_metrics_snapshot() {
    let metrics = ControllerMetrics::new();

    // Initial snapshot should be zero
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.meetings, 0);
    assert_eq!(snapshot.participants, 0);

    // Update metrics
    metrics.set_meetings(5);
    metrics.set_participants(42);

    // Snapshot should reflect current values
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.meetings, 5);
    assert_eq!(snapshot.participants, 42);

    // Test atomic operations through snapshot
    metrics.increment_meetings();
    metrics.increment_participants();

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.meetings, 6);
    assert_eq!(snapshot.participants, 43);
}
```

**Quality observations**:
- Tests initial state (zero values)
- Tests `set_*` followed by `snapshot()`
- Tests atomic `increment_*` followed by `snapshot()`
- Clear comments separating test phases
- Follows existing test style in the module

#### 3. MockBehavior Enum (gc_integration.rs)

**File**: `crates/meeting-controller/tests/gc_integration.rs` lines 35-46

**PASS**: Clean enum design for mock behavior control

```rust
#[derive(Debug, Clone, Copy)]
enum MockBehavior {
    /// Accept all requests normally.
    Accept,
    /// Reject registrations.
    Reject,
    /// Return NOT_FOUND for heartbeats (simulates MC not registered).
    NotFound,
    /// Return NOT_FOUND for first heartbeat, then accept (simulates re-registration).
    NotFoundThenAccept,
}
```

**Quality observations**:
- `Debug, Clone, Copy` traits (correct for small enum)
- Clear doc comments for each variant
- Semantic naming (describes behavior, not implementation)
- `NotFoundThenAccept` enables stateful mock behavior testing

**Mock server refactoring**:
- `new()` and `new_with_behavior()` constructors (line 71-90)
- `accepting()` and `rejecting()` retain backward compatibility
- Behavior-based dispatch in `register_mc`, `fast_heartbeat`, `comprehensive_heartbeat`
- Counter-based state for `NotFoundThenAccept` (first call returns NOT_FOUND, subsequent accept)

#### 4. New Re-registration Tests (gc_integration.rs)

**File**: `crates/meeting-controller/tests/gc_integration.rs` lines 542-676

**Test 1: `test_heartbeat_not_found_detection`** (lines 546-578)

**PASS**: Tests fast heartbeat NOT_FOUND -> NotRegistered error

- Uses `MockBehavior::NotFound`
- Verifies error type with `matches!(err, McError::NotRegistered)`
- Includes helpful error message in assertion: `"Expected NotRegistered, got: {err:?}"`
- Verifies `is_registered()` becomes false after NOT_FOUND

**Test 2: `test_comprehensive_heartbeat_not_found_detection`** (lines 580-611)

**PASS**: Tests comprehensive heartbeat NOT_FOUND -> NotRegistered error

- Mirrors fast heartbeat test structure (consistency)
- Complete test of both heartbeat types

**Test 3: `test_attempt_reregistration_success`** (lines 613-641)

**PASS**: Tests successful re-registration flow

- Uses default `MockBehavior::Accept`
- Doesn't call `register()` initially (simulates lost registration)
- Verifies `attempt_reregistration()` succeeds
- Verifies subsequent heartbeats work
- Clean separation of test phases with comments

**Test 4: `test_attempt_reregistration_after_not_found`** (lines 643-676)

**PASS**: Tests full NOT_FOUND -> re-registration flow

- Uses `MockBehavior::NotFoundThenAccept` (stateful mock)
- Tests complete sequence:
  1. Initial registration succeeds
  2. First heartbeat gets NOT_FOUND
  3. Re-registration succeeds
  4. Subsequent heartbeats work
- Comprehensive end-to-end scenario test

### Test Infrastructure Quality Summary

| Aspect | Assessment |
|--------|------------|
| Readability | Excellent - clear test names, comments, assertions |
| Maintainability | Good - MockBehavior enum centralizes behavior control |
| Coverage | Complete - tests both heartbeat types, all error paths |
| Assertions | Appropriate - uses `matches!` for error types, exact equality for values |
| Patterns | Consistent - follows existing test style in codebase |

### No Issues Found

**No BLOCKER, CRITICAL, MAJOR, MINOR, or TECH_DEBT issues** in the test code additions.

The test infrastructure is well-designed with:
- Clear MockBehavior enum for test configuration
- Stateful mock behavior for complex scenarios
- Comprehensive coverage of re-registration flows
- Consistent style with existing tests

### Round 4 Verdict: APPROVED

Test code quality is excellent. The MockBehavior enum provides clean test configuration, and all four new tests are well-structured with appropriate assertions. No new issues introduced.

---

## Finding Summary (Cumulative)

| Severity | Round 1 | Round 2 | Round 3 | Round 4 | Total |
|----------|---------|---------|---------|---------|-------|
| BLOCKER | 0 | 0 | 0 | 0 | 0 |
| CRITICAL | 0 | 0 | 0 | 0 | 0 |
| MAJOR | 0 | 0 | 0 | 0 | 0 |
| MINOR | 0 | 0 | 0 | 0 | 0 |
| TECH_DEBT | 3 | 0 | 0 | 0 | 3 |

**Final Verdict: APPROVED**

---

## Reflection (2026-01-31 - Post Round 4)

### Knowledge File Updates

Reviewed all knowledge files for learnings from 4-round code review:

**Patterns Added (3 new entries)**:
1. **Unified Task Ownership (No Arc for Single Consumer)** - Iteration 3 refactor eliminated Arc when task is sole owner. Benefits: clearer ownership, less cognitive load, no reference counting overhead.
2. **Never-Exit Resilience for Critical Background Tasks** - GC task never exits on transient failures, protecting active meetings. Infinite retry with exponential backoff, re-registration on NOT_FOUND.
3. **MockBehavior Enum for Test Configuration** - Iteration 4 test infrastructure. Replaces boolean flags with semantic enum variants, enables stateful mock behavior.

**Integration Notes Updated (1 entry)**:
- Added "Phase 6c GC Integration Patterns" section documenting Round 3 refactor quality and Round 4 test patterns

### Key Learnings

**Iteration quality progression**:
- Round 1-2: Solid foundation, 3 TECH_DEBT items (acceptable placeholders)
- Round 3: Major refactor improved design (Arc removal, unified task)
- Round 4: Test infrastructure enhancements (MockBehavior pattern)

**Refactoring observation**: The unified GC task refactor (Round 3) was a significant quality improvement over Round 2's separate tasks. Removing Arc made ownership clear and simplified the code. This demonstrates that even approved code can benefit from continued refinement.

**Test quality**: MockBehavior enum is a reusable pattern for any mock that needs to simulate protocol-level failures (NOT_FOUND, retry scenarios). The stateful behavior (NotFoundThenAccept) enabled end-to-end re-registration testing.

### Knowledge Changes Summary

```
patterns.md:
  - Added: Unified Task Ownership (No Arc for Single Consumer)
  - Added: Never-Exit Resilience for Critical Background Tasks
  - Added: MockBehavior Enum for Test Configuration

integration.md:
  - Updated: Phase 6c GC Integration Patterns (Round 3/4 insights)
```

All changes preserve existing knowledge while adding new patterns that will benefit future MC and GC development phases.
