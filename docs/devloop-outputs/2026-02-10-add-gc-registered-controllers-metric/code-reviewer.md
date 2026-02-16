# Code Review: Add GC Registered Controllers Metric

**Reviewer**: Code Reviewer Specialist
**Date**: 2026-02-10
**Task**: Add GC registered controllers metric to expose count of registered Meeting Controllers

## Files Reviewed

1. `crates/global-controller/src/observability/metrics.rs`
2. `crates/global-controller/src/repositories/meeting_controllers.rs`
3. `crates/global-controller/src/grpc/mc_service.rs`
4. `crates/global-controller/src/tasks/health_checker.rs`
5. `crates/global-controller/src/main.rs`

## Verdict: APPROVED

The implementation is solid, follows ADR-0002 no-panic policy, uses proper error handling, and has safe SQL queries. The only findings are TECH_DEBT items related to code duplication.

## Findings Summary

| Severity | Count | Description |
|----------|-------|-------------|
| BLOCKER | 0 | - |
| CRITICAL | 0 | - |
| MAJOR | 0 | - |
| MINOR | 0 | - |
| TECH_DEBT | 1 | DRY violation: refresh metric logic duplicated 3 times |

## Detailed Findings

### TECH_DEBT-1: Duplicated refresh_controller_metrics Logic

**Files**:
- `crates/global-controller/src/main.rs` (lines 273-311)
- `crates/global-controller/src/tasks/health_checker.rs` (lines 26-54)
- `crates/global-controller/src/grpc/mc_service.rs` (lines 59-78)

**Description**: The logic to refresh the registered controllers gauge is implemented three times with slight variations:

1. **main.rs::init_registered_controllers_metric**: Uses match on HealthStatus enum to convert to string
2. **health_checker.rs::refresh_controller_metrics**: Uses match on HealthStatus enum to convert to string
3. **McService::refresh_controller_metrics**: Uses `status.as_db_str().to_string()` method

All three:
- Query `get_controller_counts_by_status()`
- Convert HealthStatus to string
- Call `update_registered_controller_gauges("meeting", &counts)`

**Recommendation**: Extract to a single utility function, perhaps:
```rust
// In observability/metrics.rs or repositories/meeting_controllers.rs
pub async fn refresh_meeting_controller_gauges(pool: &PgPool) {
    match MeetingControllersRepository::get_controller_counts_by_status(pool).await {
        Ok(counts) => {
            let counts: Vec<(String, u64)> = counts
                .into_iter()
                .map(|(status, count)| (status.as_db_str().to_string(), count as u64))
                .collect();
            update_registered_controller_gauges("meeting", &counts);
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Failed to refresh controller metrics"
            );
        }
    }
}
```

**Severity**: TECH_DEBT (per ADR-0019, DRY violations are documented but not blocking)

**Tracking**: Add to `.claude/TODO.md` for future consolidation

---

## Code Quality Assessment

### ADR-0002 Compliance (No-Panic Policy)

| Check | Status | Notes |
|-------|--------|-------|
| No `unwrap()` in production code | PASS | None found in new code |
| No `expect()` in production code | PASS | None found in new code |
| No `panic!()` | PASS | None found |
| Collection access uses `.get()` | N/A | No direct collection indexing |
| Errors use `?` operator | PASS | All fallible operations propagate errors |

### SQL Query Safety

| Query | File | Parameterized | Notes |
|-------|------|---------------|-------|
| `get_controller_counts_by_status` | meeting_controllers.rs | YES | Uses GROUP BY, no user input in query |

### Error Handling

- All database errors are handled gracefully
- Metric refresh failures log warnings but don't fail the operation
- Startup metric initialization failure logs warning but doesn't prevent startup

### Code Clarity

- Excellent documentation on metric cardinality bounds (10 combinations)
- Clear doc comments explaining when gauges are updated
- Well-named constants (`CONTROLLER_STATUSES`)

### Test Coverage

- New unit tests in `metrics.rs` for `set_registered_controllers`, `update_registered_controller_gauges`, and `CONTROLLER_STATUSES`
- Tests cover:
  - All controller types and statuses
  - Partial counts (missing statuses default to 0)
  - Empty counts
  - Constant verification

---

## Recommendations (Non-Blocking)

1. **Future**: Consolidate the three `refresh_controller_metrics` implementations into a single shared function
2. **Future**: Consider adding a helper method on `HealthStatus` that returns a tuple ready for metrics (already has `as_db_str()`)

---

## Approval

**Verdict**: APPROVED

All findings are TECH_DEBT severity. The implementation is correct, follows project conventions, and has good error handling. The code duplication is minor and does not block the PR.
