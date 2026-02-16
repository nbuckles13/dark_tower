# Meeting Controller Specialist Checkpoint

**Date**: 2026-02-10
**Task**: Add MC GC heartbeat metric instrumentation

---

## Patterns Discovered

1. **Metric Recording Pattern**: The codebase follows a consistent pattern for metrics recording where both counter and histogram are recorded for each operation (counter for status, histogram for latency). This aligns with ADR-0011 requirements.

2. **Import Re-exports**: The `observability` module re-exports all metric functions for convenience, so importing `use crate::observability::{record_gc_heartbeat, record_gc_heartbeat_latency}` works cleanly.

3. **Latency Measurement Pattern**: Use `std::time::Instant::now()` before the operation and `start.elapsed()` after to capture duration. Record latency in both success and error paths.

4. **Dashboard-Metric Alignment**: Metric names in Grafana dashboards must exactly match the names defined in code. The `metrics` crate uses the exact string passed to `counter!()` or `histogram!()` macros.

---

## Gotchas Encountered

1. **Plural vs Singular Metric Names**: The metric was named `mc_gc_heartbeats_total` (plural) in code but the dashboard queried `mc_gc_heartbeat_total` (singular). Always verify metric names match exactly between code and dashboards.

2. **Import Order**: When adding new imports, maintain alphabetical order and group by source (crate imports first, then std library).

---

## Key Decisions

1. **Timer Placement**: Started the timer (`Instant::now()`) after the authorization is added but before the gRPC call. This measures the actual RPC latency, not including auth setup time.

2. **Metric Recording Before Error Handling**: Record metrics immediately after getting the result, before any additional error processing. This ensures metrics are always recorded even if subsequent processing fails.

3. **Consistent Label Values**: Used exact label values that match existing test expectations:
   - Status: "success" or "error"
   - Type: "fast" or "comprehensive"

---

## Current Status

**Completed**:
- Added `record_gc_heartbeat()` and `record_gc_heartbeat_latency()` calls to `fast_heartbeat()` method
- Added `record_gc_heartbeat()` and `record_gc_heartbeat_latency()` calls to `comprehensive_heartbeat()` method
- Fixed dashboard metric name from `mc_gc_heartbeat_total` to `mc_gc_heartbeats_total`
- All 7 verification layers passed

**Files Modified**:
1. `crates/meeting-controller/src/grpc/gc_client.rs` - Added metric recording
2. `infra/grafana/dashboards/mc-overview.json` - Fixed metric name

**Verification Results**:
- Layer 1 (cargo check): PASSED
- Layer 2 (cargo fmt): PASSED
- Layer 3 (guards): PASSED (9/9)
- Layer 4 (unit tests): PASSED (153 tests)
- Layer 5 (all tests): PASSED (all integration tests)
- Layer 6 (clippy): PASSED
- Layer 7 (semantic guards): PASSED (10/10)

---

## Reflection

**Knowledge Updates**:
- Added 1 gotcha: Dashboard metric names must match code exactly (singular vs plural is common mistake)
- Added 1 pattern: Metric recording in both success and error paths with consistent labels
- Both entries address real issues encountered in this implementation and will help future work

**Key Insight**:
The dashboard-code metric name mismatch was a subtle bug that would have been caught earlier with better naming conventions or automated validation. The pattern of recording metrics in both success/error paths is well-established in the codebase but wasn't explicitly documented - documenting it now prevents future omissions.

**Reusability**:
Both new entries are reusable across any metric instrumentation work, not just heartbeat-specific. The gotcha applies to all dashboard maintenance, and the pattern applies to any instrumented operation.
