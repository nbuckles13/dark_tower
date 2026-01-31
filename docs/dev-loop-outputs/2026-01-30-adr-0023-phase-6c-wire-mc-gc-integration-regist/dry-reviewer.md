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
