# DRY Reviewer Checkpoint

**Task**: Add GC registered controllers metric to expose count of registered Meeting Controllers.

**Date**: 2026-02-10

**Verdict**: APPROVED

---

## Summary

The implementation introduces a new gauge metric `gc_registered_controllers` with proper cardinality control (2 controller types x 5 statuses = 10 label combinations). No BLOCKER-level duplication was found. The implementation correctly uses existing patterns in the codebase and does not duplicate code that exists in `common`. One TECH_DEBT issue identified regarding internal duplication of the `refresh_controller_metrics` helper function.

---

## Files Reviewed

1. **`crates/global-controller/src/observability/metrics.rs`**
   - Added `set_registered_controllers()` - new gauge metric function
   - Added `CONTROLLER_STATUSES` constant for bounded cardinality
   - Added `update_registered_controller_gauges()` helper
   - Tests added for all new functions

2. **`crates/global-controller/src/repositories/meeting_controllers.rs`**
   - Added `get_controller_counts_by_status()` - new query to get counts grouped by status
   - Uses existing `HealthStatus` enum (no duplication)
   - Follows existing repository patterns

3. **`crates/global-controller/src/grpc/mc_service.rs`**
   - Added `refresh_controller_metrics()` private method
   - Calls refresh after registration and heartbeats

4. **`crates/global-controller/src/tasks/health_checker.rs`**
   - Added `refresh_controller_metrics()` standalone function
   - Calls refresh after marking stale controllers

5. **`crates/global-controller/src/main.rs`**
   - Added `init_registered_controllers_metric()` function
   - Called on startup to populate initial counts

---

## Duplication Analysis

### Checked Against `crates/common/`

| Pattern | Found in Common? | Assessment |
|---------|------------------|------------|
| `HealthStatus` enum | No | GC-specific, not shared |
| Gauge metric helpers | No | Service-specific metrics are intentionally not in common |
| `get_*_counts_by_status()` queries | No | First implementation of this pattern |

**Result**: No duplication with `common` crate.

### Checked Against `crates/meeting-controller/`

| Pattern | Found in MC? | Assessment |
|---------|--------------|------------|
| `HealthStatus` enum | Yes - from proto | Uses proto-gen enum, not same as GC's DB enum |
| Gauge metrics | Yes - different metrics | MC has `set_connections_active`, `set_meetings_active` - different domain |
| DB count queries | No | MC uses Redis, not PostgreSQL |

**Result**: No cross-service duplication. MC's `HealthStatus` comes from proto-gen, while GC's is a DB-layer enum.

### Checked Against `crates/ac-service/`

| Pattern | Found in AC? | Assessment |
|---------|--------------|------------|
| Gauge metrics | Yes - different metrics | AC has `set_signing_key_age_days`, `set_active_signing_keys` |
| `normalize_*` functions | Yes - AC has similar path normalization | Different domain (meeting paths vs auth paths) |

**Result**: No actionable duplication. Gauge metric patterns are similar but domain-specific.

### Internal GC Duplication

| Pattern | Locations | Assessment |
|---------|-----------|------------|
| `refresh_controller_metrics()` | `mc_service.rs`, `health_checker.rs`, `main.rs` | **TECH_DEBT** - Similar logic repeated |

**Details**: The `refresh_controller_metrics` function appears in three locations:
1. `mc_service.rs:59-78` - as a method on `McService`
2. `health_checker.rs:26-54` - as a standalone async function
3. `main.rs:273-310` - as `init_registered_controllers_metric()` (slightly different name, same logic)

All three implementations:
- Query `get_controller_counts_by_status()`
- Convert `HealthStatus` to string
- Call `update_registered_controller_gauges("meeting", &counts)`

This is internal GC duplication, not cross-service duplication. It could be extracted to a shared function within GC, but this is minor tech debt, not a BLOCKER.

---

## Findings

### BLOCKER (0)

None.

### TECH_DEBT (1)

**TD-GC-002: Extract `refresh_controller_metrics` to shared location**

- **Severity**: TECH_DEBT (non-blocking per ADR-0019)
- **Description**: The `refresh_controller_metrics` logic is duplicated three times within GC (`mc_service.rs`, `health_checker.rs`, `main.rs`). Could be extracted to `observability/metrics.rs` or a new `services/metrics_refresh.rs` module.
- **Impact**: Low - internal duplication within single crate, not cross-service
- **Recommendation**: In a future cleanup pass, extract to a shared function like:
  ```rust
  pub async fn refresh_controller_metrics_from_db(pool: &PgPool) {
      // ... single implementation
  }
  ```

---

## Verdict Details

```
verdict: APPROVED
finding_count:
  blocker: 0
  critical: 0
  major: 0
  minor: 0
  tech_debt: 1
checkpoint_exists: true
summary: No BLOCKER findings. One TECH_DEBT issue for internal duplication of refresh_controller_metrics helper (non-blocking per ADR-0019). Implementation correctly uses GC-specific patterns without duplicating code from common crate.
```

---

## Reviewer Notes

1. The `HealthStatus` enum in GC's repositories is intentionally separate from the proto-gen `HealthStatus` - they serve different purposes (DB layer vs. proto layer). This is not duplication.

2. The gauge metric pattern (`set_*()` functions) is consistent across services (AC, MC, GC) but domain-specific. This is good architectural consistency, not duplication.

3. The `CONTROLLER_STATUSES` constant provides proper cardinality bounding for the gauge metric, following ADR-0011 guidelines.

4. Tests are comprehensive and follow existing patterns.
