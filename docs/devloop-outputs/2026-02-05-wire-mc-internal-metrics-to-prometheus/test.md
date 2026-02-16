# Test Review: Wire MC Internal Metrics

**Reviewer**: Test Specialist
**Verdict**: APPROVED
**Date**: 2026-02-05

## Summary

The Prometheus wiring implementation is adequately tested. Existing unit tests cover all modified methods and verify core functionality. The Prometheus calls themselves are exercised indirectly through these tests, and the observability module has its own comprehensive test suite that validates metric recording.

## Findings

### BLOCKER

None.

### MAJOR

None - existing coverage is sufficient given the implementation approach.

### MINOR

1. **No explicit Prometheus verification in actors/metrics tests**: The unit tests in `actors/metrics.rs` verify internal state changes (counters, depths) but do not explicitly verify that Prometheus functions were called. However, this is acceptable because:
   - The Prometheus wrapper functions in `observability/metrics.rs` have their own comprehensive test suite (14 tests)
   - The wiring is simple (direct function calls, no conditional logic)
   - Integration testing would require a `DebuggingRecorder` which complicates test isolation

2. **Consider adding underflow edge case test**: While `saturating_sub(1)` is used for safety (lines 169, 362, 385), there's no explicit test that verifies the underflow protection works correctly when calling `meeting_removed()` or `connection_closed()` on a zero count.

## Test Coverage Analysis

### Existing Test Coverage

| Test | Coverage |
|------|----------|
| `test_mailbox_monitor_enqueue_dequeue` | Exercises `record_enqueue()` and `record_dequeue()` which now emit Prometheus |
| `test_mailbox_monitor_levels` | Exercises `record_enqueue()` at warning/critical thresholds |
| `test_mailbox_monitor_connection_thresholds` | Exercises `record_enqueue()` for connection actor type |
| `test_mailbox_monitor_drop` | Exercises `record_drop()` which now emits Prometheus counter |
| `test_actor_metrics` | Exercises `meeting_created/removed`, `connection_created/closed` |
| `test_actor_metrics_panics` | Exercises `record_panic()` which now emits Prometheus counter |

### Prometheus Module Coverage

The `observability::metrics` module has 14 tests that verify:
- All metric recording functions execute without panic
- Cardinality bounds are respected
- Integration test with `DebuggingRecorder` confirms metrics are captured
- ADR-0023 Section 11 metric names are callable

### Coverage Assessment

- **Modified methods**: 100% covered by existing tests
- **Prometheus call paths**: Exercised indirectly (functions called = Prometheus emitted)
- **Edge cases**: `saturating_sub` underflow protection present but not explicitly tested

## Notes

1. **Implicit vs Explicit Testing Trade-off**: The tests verify behavior (internal counters update correctly) rather than implementation (Prometheus was called). This is generally good practice, but for observability wiring, explicit verification could catch missed wiring. The comprehensive `observability::metrics` tests mitigate this risk.

2. **Test Isolation**: The observability module tests use `DebuggingRecorder` which replaces the global metrics recorder. This could cause test ordering issues if run in parallel. Tests currently use `--test-threads=1` to avoid this.

3. **Known Issue Acknowledged**: The `ControllerMetrics.current_participants` not being wired in production is documented as tech debt in `main.md`. This is a correctness issue, not a test coverage issue - the increment/decrement methods themselves are tested.

4. **Verification Completed**:
   - All 14 `actors::metrics::tests` pass
   - All 14 `observability::metrics::tests` pass
   - Full test suite (153+ tests) passes per validation layer results

---

## Iteration 2 Review

**Verdict**: APPROVED
**Date**: 2026-02-05

### Summary

The participant metric wiring in iteration 2 is properly tested through existing unit tests. All affected tests have been updated to pass the new `controller_metrics` parameter, and the join/leave/timeout code paths that call `increment/decrement_participants()` are exercised by existing test coverage.

### Findings

None.

### Test Coverage Analysis

**Files Changed:**
| File | Change | Test Coverage |
|------|--------|---------------|
| `meeting.rs` | Added `controller_metrics` field, calls `increment/decrement_participants()` on join/leave/timeout | Covered by 15+ unit tests in `meeting.rs::tests` |
| `controller.rs` | Added `controller_metrics`, passes to `MeetingActor::spawn()` | Covered by 8 unit tests in `controller.rs::tests` |
| `main.rs` | Passes `controller_metrics` to controller handle | Integration testing (startup flow) |
| `gc_integration.rs` | Updated `test_actor_handle_creation` signature | Test itself updated correctly |

**Participant Metric Call Sites:**
| Location | Method Called | Test Coverage |
|----------|---------------|---------------|
| `handle_join()` line 618 | `increment_participants()` | `test_meeting_actor_join`, `test_meeting_actor_duplicate_join`, etc. |
| `handle_leave()` line 868 | `decrement_participants()` | `test_meeting_actor_leave` |
| `check_disconnect_timeouts()` line 1109 | `decrement_participants()` | `test_disconnect_grace_period_expires` |

**Key Observations:**

1. **All unit tests updated**: Every test in `meeting.rs` and `controller.rs` that spawns actors now passes the required `controller_metrics` parameter via `ControllerMetrics::new()`.

2. **Timeout path is tested**: The `test_disconnect_grace_period_expires` test (line 1665) uses `tokio::time::pause()` to verify participants are removed after 30 seconds, which exercises the `decrement_participants()` call in `check_disconnect_timeouts()`.

3. **Reconnect path does NOT decrement**: Correctly, when a participant reconnects within the grace period (`test_reconnect_within_grace_period`), there is no decrement call since the participant was never removed.

4. **ControllerMetrics methods are unit tested**: The `increment_participants()` and `decrement_participants()` methods are tested in `metrics.rs::test_controller_metrics_participants` (line 623).

**Regression Risk**: Low - the wiring is straightforward (pass shared metrics, call increment/decrement at appropriate lifecycle points), and all existing tests exercise these paths.
