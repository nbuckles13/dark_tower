# Global Controller Specialist Checkpoint

**Date**: 2026-02-09
**Task**: Wire GC Observability Metrics
**Specialist**: global-controller

---

## Patterns Discovered

### Pattern: Metric Recording with Instant::now()

Successfully applied the pattern from `docs/specialist-knowledge/observability/patterns.md`:

```rust
use std::time::Instant;
use crate::observability::metrics;

async fn some_db_operation(pool: &PgPool) -> Result<T, GcError> {
    let start = Instant::now();

    let result = sqlx::query(...).execute(pool).await;

    // Record metrics after operation completes
    let (status, res) = match result {
        Ok(r) => ("success", Ok(r)),
        Err(e) => ("error", Err(e)),
    };
    metrics::record_db_query("operation_name", status, start.elapsed());

    res?
}
```

This pattern:
- Captures timing BEFORE the operation
- Records metrics AFTER completion (regardless of success/failure)
- Preserves the original Result for proper error propagation
- Uses match to extract both status and result

### Pattern: MC Assignment Status Classification

The MC assignment metrics use a three-level status classification:
- `success` - Assignment completed successfully
- `rejected` - MC explicitly rejected (at_capacity, draining, unhealthy)
- `error` - Technical failure (no MCs available, RPC failed)

This aligns with the Grafana dashboard queries that filter by `status` label.

---

## Gotchas Encountered

### Gotcha: Result Splitting for Metrics

When recording metrics, we need to:
1. Check if the operation succeeded/failed
2. Record the metric with appropriate status
3. Return the original error if it failed

The pattern `let (status, res) = match result { ... }` allows clean separation of:
- Status determination for metrics
- Result preservation for caller

Incorrect approaches would either:
- Lose the error by converting to string too early
- Record metrics only on success (missing error cases)

### Gotcha: Token Refresh Metrics Require Cross-Crate Changes

The `record_token_refresh` function is defined in GC's metrics module, but the actual token refresh happens in the `common` crate's `TokenManager`. Instrumenting token refresh would require either:

1. Adding `metrics` dependency to `common` crate (with feature flag)
2. Adding a callback mechanism to `TokenManager`
3. Using `TokenReceiver::changed()` to observe refreshes from GC side

This was noted as out-of-scope for this implementation but documented for future work.

---

## Key Decisions

### Decision: Instrument Repository Layer, Not Service Layer for DB Metrics

**Rationale**: Database metrics are recorded in the repository layer because:
1. Repositories are the single source of truth for DB operations
2. Services may call multiple repository methods per business operation
3. Operation names map directly to repository method names
4. Allows accurate per-query timing without service overhead

### Decision: Record MC Assignment at Service Layer

**Rationale**: MC assignment metrics are recorded in `McAssignmentService` because:
1. The assignment operation spans multiple DB queries and RPC calls
2. The service layer orchestrates the full assignment flow
3. Recording at service level captures the user-perceived latency
4. Status (success/rejected/error) is determined by service logic

### Decision: Include Metrics for Background Tasks

Database operations in background tasks (health checker, cleanup) also record metrics:
- `mark_stale_controllers_unhealthy`
- `mark_stale_mh_unhealthy`
- `end_stale_assignments`
- `cleanup_old_assignments`

This provides visibility into maintenance operations that could affect overall system performance.

---

## Current Status

**Completed (Iteration 1)**:
- [x] MC assignment metrics instrumented (`assign_meeting_with_mh`)
- [x] Database query metrics instrumented (all repository methods)
- [x] Removed `#[allow(dead_code)]` from used metric functions

**Completed (Iteration 2 - Code Review Fixes)**:
- [x] MH selection metrics instrumented (`select_mhs_for_meeting`)
- [x] Removed token refresh metric functions (cross-crate dependency issue)
- [x] Removed `#[allow(dead_code)]` from `record_mh_selection`
- [x] All 7 verification layers pass:
  - Layer 1: `cargo check --workspace` - PASS
  - Layer 2: `cargo fmt --all --check` - PASS
  - Layer 3: `./scripts/guards/run-guards.sh` - PASS (9/9 guards)
  - Layer 4: `./scripts/test.sh --workspace --lib` - PASS
  - Layer 5: `./scripts/test.sh --workspace` - PASS
  - Layer 6: `cargo clippy --workspace -- -D warnings` - PASS
  - Layer 7: `./scripts/guards/run-guards.sh --semantic` - PASS (10/10 guards)

**Not Completed (Out of Scope)**:
- Token refresh metrics - requires architectural changes to `common` crate (TD-GC-001)

---

## Files Modified

### Iteration 1

1. `crates/global-controller/src/services/mc_assignment.rs`
   - Added `metrics` import and `Instant` for timing
   - Instrumented `assign_meeting_with_mh` with `record_mc_assignment()`

2. `crates/global-controller/src/repositories/meeting_assignments.rs`
   - Added `metrics` import and `Instant` for timing
   - Instrumented: `get_healthy_assignment`, `get_candidate_mcs`, `atomic_assign`,
     `get_current_assignment`, `end_assignment`, `end_stale_assignments`,
     `cleanup_old_assignments`

3. `crates/global-controller/src/repositories/meeting_controllers.rs`
   - Added `metrics` import and `Instant` for timing
   - Instrumented: `register_mc`, `update_heartbeat`, `mark_stale_controllers_unhealthy`,
     `get_controller`

4. `crates/global-controller/src/repositories/media_handlers.rs`
   - Added `metrics` import and `Instant` for timing
   - Instrumented: `register_mh`, `update_load_report`, `mark_stale_handlers_unhealthy`,
     `get_candidate_mhs`, `get_handler`

5. `crates/global-controller/src/observability/metrics.rs`
   - Removed `#[allow(dead_code)]` from `record_mc_assignment` and `record_db_query`

### Iteration 2 (Code Review Fixes)

6. `crates/global-controller/src/services/mh_selection.rs`
   - Added `use crate::observability::metrics;` and `use std::time::Instant;`
   - Added `let start = Instant::now();` at function entry
   - Added error path metric: `metrics::record_mh_selection("error", false, start.elapsed());`
   - Added success path metric: `metrics::record_mh_selection("success", has_backup, start.elapsed());`

7. `crates/global-controller/src/observability/metrics.rs` (additional changes)
   - Removed `record_token_refresh` function (cross-crate dependency issue)
   - Removed `record_token_refresh_failure` function (cross-crate dependency issue)
   - Added comment explaining TD-GC-001 tech debt
   - Removed associated unit tests for removed functions
   - Removed `#[allow(dead_code)]` from `record_mh_selection`

---

## Metrics Now Recording

### MC Assignment Metrics

| Metric | Labels | Recorded In |
|--------|--------|-------------|
| `gc_mc_assignment_duration_seconds` | status | `McAssignmentService::assign_meeting_with_mh` |
| `gc_mc_assignments_total` | status, rejection_reason | `McAssignmentService::assign_meeting_with_mh` |

### MH Selection Metrics (Added Iteration 2)

| Metric | Labels | Recorded In |
|--------|--------|-------------|
| `gc_mh_selection_duration_seconds` | status | `MhSelectionService::select_mhs_for_meeting` |
| `gc_mh_selections_total` | status, has_backup | `MhSelectionService::select_mhs_for_meeting` |

### Database Query Metrics

| Metric | Labels | Operations |
|--------|--------|------------|
| `gc_db_query_duration_seconds` | operation | All repository methods |
| `gc_db_queries_total` | operation, status | All repository methods |

Operations tracked:
- Meeting Assignments: `get_healthy_assignment`, `get_candidate_mcs`, `atomic_assign`, `get_current_assignment`, `end_assignment`, `end_stale_assignments`, `cleanup_old_assignments`
- Meeting Controllers: `register_mc`, `update_heartbeat`, `mark_stale_controllers_unhealthy`, `get_controller`
- Media Handlers: `register_mh`, `update_load_report`, `mark_stale_mh_unhealthy`, `get_candidate_mhs`, `get_handler`

---

## Reflection Phase (2026-02-09)

### Knowledge Files Updated

**Iteration 1** - Added learnings to `docs/specialist-knowledge/global-controller/`:

**patterns.md** (3 new patterns):
1. **Instant::now() for Operation Duration Metrics** - Timing pattern using `start.elapsed()` without needing Duration import
2. **Repository Layer for DB Query Metrics** - Why DB metrics belong in repository methods, not services
3. **Status Label Determination from Result** - Pattern for deriving status labels from operation outcomes with rich rejection reasons

**gotchas.md** (2 new gotchas):
1. **Cross-Crate Metrics Cannot Use Crate-Local Recording Functions** - TokenManager in `common` cannot call GC's metrics functions
2. **Duration Import Not Needed with start.elapsed()** - Avoid unused import warnings

**integration.md** (1 new section):
1. **Observability Metrics Layering** - Documents where different metric types are recorded and why

**Iteration 2** - Updated entries after code review fixes:

**gotchas.md** (1 updated):
1. **Cross-Crate Metrics** - Updated to reflect that unwireable functions were removed (not just "not instrumented")

**integration.md** (1 updated):
1. **Observability Metrics Layering** - Added MH selection metrics, corrected token refresh status to "Out of Scope" (functions removed)

### Tech Debt Created

**TD-GC-001**: Token refresh metrics not instrumented
- **Reason**: TokenManager lives in `common` crate, cannot import GC's metrics module
- **Resolution**: Metric functions removed from `metrics.rs` to avoid dead code
- **Future**: Requires architectural decision on cross-crate metrics (callback, trait, or observer pattern)
- **Impact**: Token refresh dashboard panel will show "No data"

### Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Instant::now() timing pattern | HIGH | Standard Rust idiom, works correctly |
| Repository-layer instrumentation | HIGH | Consistent with ADR-0011 guidance |
| Status label cardinality | HIGH | Matches Grafana dashboard expectations |
| MH selection instrumentation | HIGH | Same pattern as MC assignment |
| Cross-crate metrics challenge | MEDIUM | Solution approach not yet decided |

### Recommendations for Future Work

1. **Token refresh metrics**: Design cross-crate metrics architecture (affects all services)
2. **HTTP request metrics**: Consider tower-http metrics layer for automatic latency tracking
3. **Metrics testing**: Add integration tests that verify metrics are recorded correctly
