# Test Specialist Review

**Task**: Add MC GC heartbeat metric instrumentation

**Date**: 2026-02-10

**Reviewer**: Test Specialist

---

## Files Reviewed

1. `crates/meeting-controller/src/grpc/gc_client.rs` - Added metric recording to heartbeat methods
2. `infra/grafana/dashboards/mc-overview.json` - Fixed metric name from `mc_gc_heartbeat_total` to `mc_gc_heartbeats_total`

---

## Coverage Analysis

### 1. gc_client.rs Changes

The implementation adds `record_gc_heartbeat()` and `record_gc_heartbeat_latency()` calls to:
- `fast_heartbeat()` method (lines 363-387)
- `comprehensive_heartbeat()` method (lines 456-479)

Both success and error paths are instrumented with:
- `record_gc_heartbeat("success", "fast/comprehensive")` on success
- `record_gc_heartbeat("error", "fast/comprehensive")` on error
- `record_gc_heartbeat_latency("fast/comprehensive", duration)` for both paths

**Existing Test Coverage**:

The following integration tests in `crates/meeting-controller/tests/gc_integration.rs` exercise the heartbeat paths:

| Test | Path Covered |
|------|--------------|
| `test_gc_client_fast_heartbeat` | Success path for fast heartbeat |
| `test_gc_client_comprehensive_heartbeat` | Success path for comprehensive heartbeat |
| `test_gc_client_heartbeat_skipped_when_not_registered` | Early return (no metrics recorded) |
| `test_heartbeat_not_found_detection` | Error path (NOT_FOUND) for fast heartbeat |
| `test_comprehensive_heartbeat_not_found_detection` | Error path (NOT_FOUND) for comprehensive heartbeat |
| `test_attempt_reregistration_after_not_found` | Error then success path |

**Metric Function Coverage**:

The `crates/meeting-controller/src/observability/metrics.rs` file contains dedicated unit tests:

| Test | Coverage |
|------|----------|
| `test_record_gc_heartbeat` | All status/type combinations |
| `test_record_gc_heartbeat_latency` | Both heartbeat types |
| `test_prometheus_metrics_endpoint_integration` | Metrics are recorded to Prometheus |
| `test_cardinality_bounds` | Label bounds validation |

### 2. Dashboard JSON Changes

The metric name was corrected from `mc_gc_heartbeat_total` to `mc_gc_heartbeats_total` (line 1354).

This is a configuration fix that matches the metric name defined in `observability/metrics.rs`:
```rust
counter!("mc_gc_heartbeats_total", "status" => ..., "type" => ...)
```

**Coverage**: Dashboard JSON files are not unit-testable. The change is verified by:
1. Static analysis (metric name matches code)
2. The existing `test_record_gc_heartbeat` test verifies the metric function works

---

## Findings

### TECH_DEBT-1: No Direct Metric Recording Verification in Integration Tests

**Severity**: TECH_DEBT

**Description**: The integration tests (`gc_integration.rs`) verify the RPC behavior but do not directly verify that metrics are recorded. The tests exercise the code paths where `record_gc_heartbeat()` is called, providing code coverage, but do not install a metrics recorder to verify the actual metric values.

**Rationale**: This is acceptable because:
1. The metric functions themselves are unit-tested in `observability/metrics.rs`
2. The integration tests exercise all code paths containing the metric calls
3. Adding metrics verification to integration tests would require significant test infrastructure changes

**Recommendation**: Consider adding a test utility that installs a `DebuggingRecorder` for integration tests in a future iteration. This would allow verifying that specific metrics are recorded during RPC operations.

---

## Summary

| Category | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 0 |
| TECH_DEBT | 1 |

**Test Coverage Assessment**:
- All new code paths are exercised by existing tests
- The heartbeat success paths are tested via `test_gc_client_fast_heartbeat` and `test_gc_client_comprehensive_heartbeat`
- The heartbeat error paths are tested via `test_heartbeat_not_found_detection` and `test_comprehensive_heartbeat_not_found_detection`
- The metric recording functions have dedicated unit tests
- 153 unit tests passed including 13 GC integration tests

---

## Verdict

**APPROVED**

The implementation has adequate test coverage. The existing integration tests exercise all new code paths (success and error paths for both heartbeat types), and the metric functions themselves have comprehensive unit tests. The only finding is a TECH_DEBT item regarding the lack of direct metric value verification in integration tests, which is acceptable given the existing coverage strategy.
