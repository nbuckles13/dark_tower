# Test Review Checkpoint

**Reviewer**: Test Specialist
**Date**: 2026-02-09
**Task**: Wire GC observability metrics - instrument MC assignment, DB queries, and MH selection code paths
**Iteration**: 2 (Final)

## Verdict: APPROVED

## Finding Summary

| Severity | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 0 |
| MINOR | 0 |
| TECH_DEBT | 2 |

## Iteration 2 Fix Verification

### MAJOR-1 (Iteration 1): Token Refresh Metrics Not Wired

**Status**: FIXED

**Resolution**: Token refresh metrics functions were correctly removed from `metrics.rs`. The cross-crate dependency issue is documented in a clear code comment (lines 174-183) explaining that TokenManager lives in the `common` crate and cannot depend on `global-controller`. The issue is tracked as tech debt TD-GC-001.

### MAJOR-2 (Iteration 1): MH Selection Metrics Not Wired

**Status**: FIXED

**Resolution**: `record_mh_selection` is now properly wired in `mh_selection.rs`:
- Line 76: Records "error" status when no healthy MHs are available
- Lines 135-136: Records "success" status with `has_backup` label on successful selection

The `#[allow(dead_code)]` annotation has been removed from the function in `metrics.rs`.

## Remaining Tech Debt

### TECH_DEBT-1: Token Refresh Metrics Cross-Crate Dependency

**Location**: `crates/common/token_manager.rs` (future work)

**Description**: Token refresh metrics require architectural changes to implement. Options include:
1. Callback mechanism from common crate to consumer
2. Feature flag to optionally include metrics dependency
3. Metrics trait in common crate

**Tracking**: TD-GC-001

### TECH_DEBT-2: Metrics Verification in Integration Tests

**Location**: `crates/global-controller/tests/`

**Description**: Integration tests exercise code paths that record metrics, but do not verify actual metric values using a test recorder (e.g., `metrics-util` crate). While instrumentation is correct based on code review, automated verification would catch regressions.

**Impact**: Low - current implementation is correct, this is a testing enhancement for robustness.

## Coverage Assessment

### MH Selection Metrics

The `mh_selection.rs` file has comprehensive unit test coverage:
- `test_weighted_random_select_empty` - empty candidate list
- `test_weighted_random_select_single` - single candidate
- `test_weighted_random_select_multiple_returns_valid` - multiple candidates
- `test_weighted_random_select_prefers_lower_load` - load-based selection preference
- `test_mh_selection_fields` - struct field verification
- `test_mh_assignment_info_fields` - struct field verification

### MC Assignment Metrics

All MC assignment code paths have metrics:
- Success when reusing existing assignment (line 153)
- Success for new assignment (line 252)
- Rejection reasons: at_capacity, draining, unhealthy, unspecified (lines 288-295)
- Error when no MCs available (line 293)

### DB Query Metrics

All repository operations are instrumented with timing and status labels:

**meeting_assignments.rs**:
- get_healthy_assignment
- get_candidate_mcs
- atomic_assign
- get_current_assignment
- end_assignment
- end_stale_assignments
- cleanup_old_assignments

**meeting_controllers.rs**:
- register_mc
- update_heartbeat
- mark_stale_controllers_unhealthy
- get_controller

**media_handlers.rs**:
- register_mh
- update_load_report
- mark_stale_mh_unhealthy
- get_candidate_mhs
- get_handler

## Summary

All Iteration 1 findings have been addressed. MAJOR-1 (token refresh metrics) was correctly resolved by removing the functions and documenting the cross-crate dependency as tech debt. MAJOR-2 (MH selection metrics) has been fixed with proper wiring of `record_mh_selection` calls. The implementation now has complete metrics coverage for MC assignment, MH selection, and all DB queries as specified in ADR-0010 and ADR-0011.

## Files Reviewed

- `crates/global-controller/src/services/mc_assignment.rs`
- `crates/global-controller/src/services/mh_selection.rs` (UPDATED)
- `crates/global-controller/src/repositories/meeting_assignments.rs`
- `crates/global-controller/src/repositories/meeting_controllers.rs`
- `crates/global-controller/src/repositories/media_handlers.rs`
- `crates/global-controller/src/observability/metrics.rs` (UPDATED)
