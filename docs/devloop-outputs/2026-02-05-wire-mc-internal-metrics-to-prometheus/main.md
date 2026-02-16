# Dev-Loop Output: Wire MC Internal Metrics to Prometheus

**Date**: 2026-02-05
**Start Time**: 00:15
**Task**: Wire MC internal metrics (ActorMetrics, ControllerMetrics, MailboxMonitor) to Prometheus observability module
**Branch**: `feature/mc-observability`
**Duration**: ~2h (complete)

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `ad4ed7c` |
| Implementing Specialist | `meeting-controller` |
| Current Step | `complete` |
| Iteration | `2` |
| Security Reviewer | `aee6892` |
| Test Reviewer | `a0ab453` |
| Code Reviewer | `a2368f1` |
| DRY Reviewer | `a35375a` |

---

## Task Overview

### Objective

Wire the existing internal metrics tracking in `crates/meeting-controller/src/actors/metrics.rs` to call the Prometheus metrics wrapper functions in `crates/meeting-controller/src/observability/metrics.rs`.

### Detailed Requirements

#### Context

The previous dev-loop created the observability module with Prometheus metrics wrapper functions. However, the existing internal metrics tracking (`ActorMetrics`, `ControllerMetrics`, `MailboxMonitor`) still operates independently without updating Prometheus gauges/counters.

#### Internal Metrics to Wire

**1. ActorMetrics** (`actors/metrics.rs:301-368`)

| Method | Should Call |
|--------|-------------|
| `meeting_created()` | `observability::metrics::set_meetings_active(count)` |
| `meeting_removed()` | `observability::metrics::set_meetings_active(count)` |
| `connection_created()` | `observability::metrics::set_connections_active(count)` |
| `connection_closed()` | `observability::metrics::set_connections_active(count)` |
| `record_panic(actor_type)` | `observability::metrics::record_actor_panic(actor_type.as_str())` |

**2. MailboxMonitor** (`actors/metrics.rs:78-215`)

| Method | Should Call |
|--------|-------------|
| `record_enqueue()` | `observability::metrics::set_actor_mailbox_depth(actor_type, depth)` |
| `record_dequeue()` | `observability::metrics::set_actor_mailbox_depth(actor_type, depth)` |
| `record_drop()` | `observability::metrics::record_message_dropped(actor_type.as_str())` |

**3. ControllerMetrics** (`actors/metrics.rs:222-298`)

| Method | Should Call |
|--------|-------------|
| `set_meetings()` | `observability::metrics::set_meetings_active(count)` |
| `increment_meetings()` | `observability::metrics::set_meetings_active(count)` |
| `decrement_meetings()` | `observability::metrics::set_meetings_active(count)` |

Note: `ControllerMetrics` and `ActorMetrics` both track meetings. Need to determine if they should both emit to same gauge or if one is authoritative.

#### Implementation Approach

Option A: **Direct calls in methods** - Add `use crate::observability::metrics;` and call wrapper functions directly in each method.

Option B: **Callback/observer pattern** - Create a trait for metric observers and allow registration. More flexible but more complex.

**Recommended**: Option A (direct calls) - simpler, matches the existing pattern, no over-engineering.

#### Files to Modify

- `crates/meeting-controller/src/actors/metrics.rs` - Add Prometheus calls to existing methods

#### Acceptance Criteria

- [ ] `ActorMetrics::meeting_created/removed` updates `mc_meetings_active` gauge
- [ ] `ActorMetrics::connection_created/closed` updates `mc_connections_active` gauge
- [ ] `ActorMetrics::record_panic` increments `mc_actor_panics_total` counter
- [ ] `MailboxMonitor::record_enqueue/dequeue` updates `mc_actor_mailbox_depth` gauge
- [ ] `MailboxMonitor::record_drop` increments `mc_messages_dropped_total` counter
- [ ] Existing unit tests still pass
- [ ] No duplicate metric updates (clarify ActorMetrics vs ControllerMetrics ownership)

### Scope

- **Service(s)**: meeting-controller
- **Schema**: None
- **Cross-cutting**: Observability (wiring only, no new metrics)

### Debate Decision

N/A - Wiring task follows existing implementation pattern

---

## Matched Principles

The following principle categories were matched:
- `docs/principles/observability.md` - Primary: metrics wiring
- `docs/principles/errors.md` - Always included for production code

---

## Pre-Work

- Reviewed `actors/metrics.rs` to understand internal tracking structure
- Identified 3 structs: `ActorMetrics`, `ControllerMetrics`, `MailboxMonitor`
- Mapped methods to corresponding Prometheus wrapper functions
- Previous dev-loop created the observability module (dbbd886)

---

## Implementation Summary

Wired internal metrics tracking (`ActorMetrics`, `MailboxMonitor`) to Prometheus via the observability module. The implementation follows the direct-call pattern (Option A) as recommended.

### Key Changes

1. **Added Prometheus import**: `use crate::observability::metrics as prom;`

2. **MailboxMonitor Prometheus integration**:
   - `record_enqueue()` - emits `mc_actor_mailbox_depth` gauge with current depth
   - `record_dequeue()` - emits `mc_actor_mailbox_depth` gauge with new depth
   - `record_drop()` - emits `mc_messages_dropped_total` counter

3. **ActorMetrics Prometheus integration**:
   - `meeting_created()` - emits `mc_meetings_active` gauge with incremented count
   - `meeting_removed()` - emits `mc_meetings_active` gauge with decremented count
   - `connection_created()` - emits `mc_connections_active` gauge with incremented count
   - `connection_closed()` - emits `mc_connections_active` gauge with decremented count
   - `record_panic()` - emits `mc_actor_panics_total` counter

4. **ControllerMetrics NOT wired** (intentional):
   - `ControllerMetrics` is specifically for GC heartbeat reporting
   - `ActorMetrics` is the authoritative source for Prometheus metrics
   - This prevents duplicate emissions to the same gauge

### Pattern Used

Each method:
1. Performs atomic operation (fetch_add/fetch_sub)
2. Calculates new value from return value
3. Emits to Prometheus

Example:
```rust
pub fn meeting_created(&self) {
    let count = self.active_meetings.fetch_add(1, Ordering::Relaxed) + 1;
    prom::set_meetings_active(count as u64);
}
```

---

## Files Modified

### Iteration 1

| File | Changes |
|------|---------|
| `crates/meeting-controller/src/actors/metrics.rs` | Added Prometheus integration to `ActorMetrics` and `MailboxMonitor` methods |

### Iteration 2 - Wire ControllerMetrics.current_participants

| File | Changes |
|------|---------|
| `crates/meeting-controller/src/actors/meeting.rs` | Added `controller_metrics` field, call `increment/decrement_participants()` on join/leave/timeout |
| `crates/meeting-controller/src/actors/controller.rs` | Added `controller_metrics` to actor, pass when spawning meetings |
| `crates/meeting-controller/src/main.rs` | Pass `controller_metrics` when creating controller handle |
| `crates/meeting-controller/tests/gc_integration.rs` | Updated test to pass `controller_metrics` |

---

## Dev-Loop Verification Steps

### Iteration 1

All 7 verification layers passed:

| Layer | Command | Result |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASSED |
| 2 | `cargo fmt --all --check` | PASSED |
| 3 | `./scripts/guards/run-guards.sh` | PASSED (9/9 guards) |
| 4 | `./scripts/test.sh --workspace --lib` | PASSED (153 MC tests) |
| 5 | `./scripts/test.sh --workspace` | PASSED (full suite) |
| 6 | `cargo clippy --workspace -- -D warnings` | PASSED |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASSED (10/10 guards) |

### Iteration 2

All 7 verification layers passed:

| Layer | Command | Result |
|-------|---------|--------|
| 1 | `cargo check --workspace` | PASSED |
| 2 | `cargo fmt --all --check` | PASSED |
| 3 | `./scripts/guards/run-guards.sh` | PASSED (9/9 guards) |
| 4 | `./scripts/test.sh --workspace --lib` | PASSED (153 MC tests) |
| 5 | `./scripts/test.sh --workspace` | PASSED (full suite) |
| 6 | `cargo clippy --workspace -- -D warnings` | PASSED |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASSED (10/10 guards) |

---

## Code Review

### Results Summary

| Reviewer | Verdict | Blockers | Critical | Major | Minor |
|----------|---------|----------|----------|-------|-------|
| Security | APPROVED | 0 | 0 | 0 | 0 |
| Test | APPROVED | 0 | 0 | 0 | 1 |
| Code Reviewer | APPROVED | 0 | 0 | 0 | 2 |
| DRY Reviewer | APPROVED | 0 | 0 | 0 | 0 |

**Overall Verdict: APPROVED** (with known issue to fix)

### Security Specialist
- No PII in metric labels (only bounded `actor_type`)
- Cardinality properly bounded (max 3 values)
- No credential exposure

### Test Specialist
- All 14 unit tests pass and exercise modified methods
- MINOR: Could add explicit edge case test for `saturating_sub` underflow

### Code Quality Reviewer
- ADR-0002 compliant (no panics/unwraps)
- Proper atomic operations
- MINOR: Could add comment explaining `fetch_sub().saturating_sub(1)` pattern
- MINOR: Warning log condition might miss batched enqueues

### DRY Reviewer
- Follows established AC observability patterns
- Uses `ActorType::as_str()` for type-safe labels
- Clear import alias (`prom::*`)

### Iteration 2 Review

| Reviewer | Verdict |
|----------|---------|
| Security | APPROVED |
| Test | APPROVED |
| Code Reviewer | APPROVED |
| DRY Reviewer | APPROVED |

**Key observations**:
- No PII exposure - only aggregate participant counts
- Thread-safe atomic operations with SeqCst ordering
- Pattern consistent with ActorMetrics wiring
- All affected test paths updated correctly
- Proper decrement placement (leave, timeout) with no double-counting on reconnect

---

## Issues Encountered

### Iteration 1

1. **Formatting**: Initial code had long lines that required reformatting for `saturating_sub` chains. Fixed by running `cargo fmt --all`.

2. **ControllerMetrics.current_participants never updated** (identified, fixed in Iteration 2):
   - `ControllerMetrics.increment_participants()` and `decrement_participants()` were never called in production code (only in tests)
   - Participants were tracked at `MeetingActor` level (`participants: HashMap`) but not propagated to `ControllerMetrics`
   - Result: GC heartbeats always report 0 participants

### Iteration 2

1. **Many test updates required**: Adding `controller_metrics` parameter to `MeetingActor::spawn()` and `MeetingControllerActorHandle::new()` required updating all unit tests and integration tests

2. **Three decrement locations**: `decrement_participants()` needed to be called in:
   - `handle_leave()` - explicit participant leave
   - `check_disconnect_timeouts()` - participant removed after grace period

---

## Lessons Learned

1. **Single source of truth**: When multiple structs track the same metric, designate one as authoritative for external emission to prevent duplicates.

2. **Module alias pattern**: Using `use crate::observability::metrics as prom;` improves code clarity by distinguishing Prometheus calls from local tracking.

3. **saturating_sub for safety**: Always use `saturating_sub(1)` when calculating decremented values to prevent underflow edge cases.

4. **Dual metrics facades**: Separation of internal atomic tracking (ControllerMetrics for GC heartbeats) from Prometheus emission (ActorMetrics via observability module) is intentional architecture, not duplication.

5. **Participant lifecycle tracking**: Decrement must happen in multiple places (leave AND disconnect timeout), but reconnect reuses slot without decrement.

---

## Reflection Summary

| Specialist | Added | Updated | Pruned |
|------------|-------|---------|--------|
| Meeting Controller | 5 | 0 | 0 |
| Security | 2 | 0 | 0 |
| Test | 2 | 1 | 0 |
| Code Reviewer | 4 | 1 | 0 |
| DRY Reviewer | 2 | 1 | 0 |

**Key knowledge captured**:
- Pattern: Shared metrics propagation through actor hierarchy via Arc
- Pattern: Bounded label values for cardinality safety
- Pattern: Dual metrics facades (internal vs Prometheus)
- Gotcha: Participant decrement needed in multiple locations
- Gotcha: fetch_sub returns previous value, not current

---

## Tech Debt

None - BLOCKER fixed in Iteration 2.

---

## Next Steps

1. **Code Review**: Run review with security, test, code-reviewer, and DRY specialists
2. **Integration Testing**: Verify metrics appear correctly in Prometheus/Grafana when MC is running
3. **Consider participants Prometheus metric**: `ControllerMetrics.current_participants` is now updated but not yet wired to Prometheus - may want to add `mc_participants_active` gauge in future
