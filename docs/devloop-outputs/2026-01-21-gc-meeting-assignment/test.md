# Test Specialist Review: GC Meeting Assignment

**Date**: 2026-01-21
**Reviewer**: Test Specialist
**Task**: GC should assign users to MCs via load balancing per design in ADR-0010

## Summary

The implementation includes comprehensive test coverage for the meeting assignment functionality. The test suite covers repository operations, service layer, load balancing algorithm, and handler integration with mock AC endpoints. Overall, the test coverage is adequate with minor improvements suggested.

## Verdict

**APPROVED**

## Finding Count

| Severity | Count |
|----------|-------|
| BLOCKER | 0 |
| CRITICAL | 0 |
| MAJOR | 2 |
| MINOR | 3 |
| SUGGESTION | 3 |

---

## Detailed Findings

### MAJOR-1: No Concurrent Race Condition Test for atomic_assign

**Location**: `crates/global-controller/tests/meeting_assignment_tests.rs`

**Issue**: While `test_atomic_assign_reuses_existing` tests that a second sequential assignment returns the existing one, there is no test that spawns multiple concurrent tasks attempting to assign the same meeting simultaneously to verify the atomic CTE race condition handling works correctly under actual contention.

**Impact**: The atomic assignment logic is critical for preventing meeting fragmentation (multiple MCs for same meeting). Without concurrent testing, race conditions might not be detected.

**Recommendation**: Add a test that spawns 10+ concurrent tokio tasks all calling `McAssignmentService::assign_meeting` for the same meeting_id and verifies:
1. All tasks complete successfully
2. All tasks return the same mc_id
3. Only one row exists in meeting_assignments table

---

### MAJOR-2: No Test for MC Becomes Unhealthy During Assignment

**Location**: `crates/global-controller/tests/meeting_assignment_tests.rs`

**Issue**: ADR-0010 specifies that if an MC becomes unhealthy, the assignment should be ended and a new one created. There is no test that:
1. Creates an assignment
2. Makes the assigned MC unhealthy (via updating health_status or setting old heartbeat)
3. Attempts to get/assign again and verifies a new MC is selected

**Impact**: This is a core recovery scenario in ADR-0010 (stale assignment handling). The atomic CTE handles this, but it is not tested.

**Recommendation**: Add test `test_reassignment_when_mc_becomes_unhealthy` that:
1. Creates assignment to mc-1
2. Updates mc-1 to unhealthy
3. Calls assign_meeting again
4. Verifies a different healthy MC is assigned

---

### MINOR-1: Statistical Test for Weighted Random Distribution Could Be Flaky

**Location**: `crates/global-controller/src/repositories/meeting_assignments.rs`, line 537-574

**Issue**: `test_weighted_random_select_prefers_lower_load` runs 1000 iterations and asserts `light_count > heavy_count * 10`. While the expected ratio is ~100:1 (weights 1.0 vs 0.01), the test might occasionally fail due to statistical variance, though 10x threshold provides good margin.

**Impact**: Potential CI flakiness (very low probability given margin).

**Recommendation**: Consider using a larger sample size (10,000) or a more relaxed assertion, OR add a comment explaining the statistical basis for the 10x threshold.

---

### MINOR-2: Missing Test for Cleanup with Zero Retention Days

**Location**: `crates/global-controller/tests/meeting_assignment_tests.rs`

**Issue**: `test_cleanup_old_assignments` only tests with 7 days retention. No edge case test for 0 days retention (immediate cleanup) or negative values.

**Recommendation**: Add boundary test for retention_days = 0 to verify immediate cleanup works.

---

### MINOR-3: No Test for Empty Region String

**Location**: Repository and service layer

**Issue**: What happens if region is empty string ""? The code doesn't explicitly validate this, and tests don't cover this edge case.

**Recommendation**: Either add validation that rejects empty region or add a test documenting expected behavior.

---

### SUGGESTION-1: Add Property-Based Testing for weighted_random_select

**Location**: `crates/global-controller/src/repositories/meeting_assignments.rs`

The `weighted_random_select` function is well-tested but would benefit from property-based testing (proptest or quickcheck) to verify invariants:
- Always returns Some for non-empty input
- Never returns index out of bounds
- Distribution approximates expected weights over many samples

---

### SUGGESTION-2: Add Test for MAX Candidates Limit

**Location**: Repository tests

The query limits to 5 candidates (`LOAD_BALANCING_CANDIDATE_COUNT`). Consider adding a test that registers 10+ healthy MCs and verifies only 5 candidates are returned, ordered by load_ratio.

---

### SUGGESTION-3: Consider Adding Integration Test with Meeting Join Flow

**Location**: `crates/global-controller/tests/meeting_tests.rs`

The meeting tests (`test_join_meeting_authenticated_success`, `test_guest_token_success`) already verify MC assignment is returned in the response. Consider adding a test that:
1. Joins meeting twice with different users
2. Verifies both get the same MC assignment
3. Verifies mc_assignment count in database is 1

This would be an end-to-end verification of the assignment sticky-session behavior.

---

## Coverage Analysis

### Critical Paths Tested

| Path | Tested | Test Name |
|------|--------|-----------|
| Get healthy assignment (none exists) | Yes | `test_get_healthy_assignment_none_when_empty` |
| Get healthy assignment (exists) | Yes | via `test_service_assign_meeting_reuses_healthy` |
| Get candidates (none healthy) | Yes | `test_get_candidate_mcs_empty_when_no_healthy` |
| Get candidates (mixed health) | Yes | `test_get_candidate_mcs_returns_healthy_only` |
| Get candidates (ordered by load) | Yes | `test_get_candidate_mcs_ordered_by_load` |
| Get candidates (excludes full) | Yes | `test_get_candidate_mcs_excludes_full` |
| Get candidates (region filter) | Yes | `test_get_candidate_mcs_filters_by_region` |
| Atomic assign (new) | Yes | `test_atomic_assign_creates_assignment` |
| Atomic assign (reuses existing) | Yes | `test_atomic_assign_reuses_existing` |
| End assignment (by region) | Yes | `test_end_assignment_by_region` |
| End assignment (all regions) | Yes | `test_end_assignment_all_regions` |
| Cleanup old assignments | Yes | `test_cleanup_old_assignments` |
| Service: no healthy MCs error | Yes | `test_service_assign_meeting_no_healthy_mcs` |
| Service: successful assignment | Yes | `test_service_assign_meeting_success` |
| Service: reuse healthy assignment | Yes | `test_service_assign_meeting_reuses_healthy` |
| Service: end assignment | Yes | `test_service_end_assignment` |
| Service: get assignment | Yes | `test_service_get_assignment` |
| Weighted selection: empty | Yes | `test_weighted_random_select_empty` |
| Weighted selection: single | Yes | `test_weighted_random_select_single` |
| Weighted selection: multiple | Yes | `test_weighted_random_select_multiple_returns_valid` |
| Weighted selection: prefers low load | Yes | `test_weighted_random_select_prefers_lower_load` |

### Error Paths Tested

| Error Path | Tested | Test Name |
|------------|--------|-----------|
| No healthy MCs available | Yes | `test_service_assign_meeting_no_healthy_mcs` |
| Database error | No | Would require mock injection |

### Edge Cases

| Edge Case | Tested |
|-----------|--------|
| Empty candidates list | Yes |
| Single candidate | Yes |
| All candidates at 0% load | No (partial - tested with varied loads) |
| All candidates at 99% load | No |
| Concurrent race condition | No (MAJOR-1) |
| MC becomes unhealthy | No (MAJOR-2) |
| Empty region string | No (MINOR-3) |

---

## Handler Integration Tests

The `meeting_tests.rs` file provides excellent integration coverage including:
- MC assignment returned in join response
- MC assignment returned in guest token response
- Concurrent guest requests all succeed
- JWT validation (multiple attack vectors tested)
- Inactive user handling
- Cross-org permissions

These tests properly use `register_healthy_mc_for_region()` helper to ensure assignments can succeed.

---

## Test Quality Assessment

### Strengths

1. **Deterministic setup**: Tests use `#[sqlx::test(migrations)]` for clean database state
2. **Helper functions**: `register_healthy_mc` reduces test boilerplate
3. **Clear assertions**: Tests have good assertion messages
4. **Coverage breadth**: Both repository and service layers tested
5. **Statistical testing**: Load balancing distribution is verified statistically
6. **CSPRNG usage**: Tests use ring::rand::SystemRandom correctly

### Areas for Improvement

1. **Concurrent testing**: Missing race condition tests
2. **Failure injection**: No tests for MC health transitions
3. **Boundary testing**: Some edge cases not covered

---

## Conclusion

The test suite provides solid coverage of the meeting assignment functionality. The implementation follows ADR-0010 design closely, and the tests verify the core behaviors. The two MAJOR findings relate to race condition testing and MC health transition testing - both are important scenarios from ADR-0010 that should be added before production deployment.

**Verdict: APPROVED** - The current coverage is adequate for the initial implementation. The MAJOR findings are not blockers but should be addressed in a follow-up PR.
