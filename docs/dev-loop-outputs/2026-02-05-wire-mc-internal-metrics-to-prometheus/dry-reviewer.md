# DRY Review: Wire MC Internal Metrics

**Reviewer**: DRY Reviewer
**Verdict**: APPROVED
**Date**: 2026-02-05

## Summary

The Prometheus integration in `actors/metrics.rs` follows established patterns well, with clean separation between internal tracking and Prometheus emission. No DRY blockers found; minor improvements identified for future consideration.

## Findings

### BLOCKER

None.

### NON-BLOCKER

1. **Increment/Decrement Pattern Repetition** (Low priority)
   - `ActorMetrics::meeting_created()`, `meeting_removed()`, `connection_created()`, `connection_closed()` all follow the same pattern:
     ```rust
     let count = self.field.fetch_add/sub(1, Ordering::Relaxed) +/- 1;
     prom::set_*_active(count as u64);
     ```
   - Could potentially be abstracted into a generic helper, but the current approach is clear and type-safe.
   - **Recommendation**: Document in tech debt; no immediate action needed.

2. **ControllerMetrics Does Not Emit to Prometheus** (Design decision, not violation)
   - `ControllerMetrics` is documented as for GC heartbeat reporting only (line 17).
   - `ActorMetrics` handles Prometheus emission for meetings/connections separately.
   - The known issue about `current_participants` not being updated is noted and will be addressed in dev-loop-fix.
   - **Recommendation**: Verify during fix phase that participant counts are wired correctly.

## Pattern Analysis

### Excellent Alignment with AC Observability Patterns

| Pattern | AC Service | MC Service (actors/metrics.rs) | Match |
|---------|------------|--------------------------------|-------|
| Single point of emission | Functions in `observability/metrics.rs` | Functions in `observability/metrics.rs` called from actors | Yes |
| Metric prefix | `ac_` | `mc_` | Yes |
| Cardinality documentation | Header comments in metrics.rs | Header comments in metrics.rs | Yes |
| Counter naming | `_total` suffix | `_total` suffix | Yes |
| Gauge usage | Direct `set()` calls | Direct `set()` calls | Yes |
| Doc comments with metric names | Present on all functions | Present on all functions | Yes |

### Clean Separation of Concerns

The implementation correctly maintains the separation:

1. **Internal tracking** (`actors/metrics.rs`): Business logic metrics with atomic counters
2. **Prometheus emission** (`observability/metrics.rs`): Low-level metric primitives

The `ActorMetrics` and `MailboxMonitor` structs act as facades that:
- Maintain internal atomic counters for fast access
- Delegate to `prom::*` functions for Prometheus emission
- Log warnings at appropriate thresholds

This is the same pattern used successfully in AC's auth flows.

### Consistent with ADR-0023 Section 11

All seven required MC metrics are wired through this integration:
- `mc_connections_active` via `ActorMetrics::connection_created/closed`
- `mc_meetings_active` via `ActorMetrics::meeting_created/removed`
- `mc_actor_mailbox_depth` via `MailboxMonitor::record_enqueue/dequeue`
- `mc_actor_panics_total` via `ActorMetrics::record_panic`
- `mc_messages_dropped_total` via `MailboxMonitor::record_drop`

## Cross-Service Alignment

### Comparison with AC Service

| Aspect | AC Pattern | MC Implementation | Verdict |
|--------|------------|-------------------|---------|
| Import alias | N/A (direct calls) | `use crate::observability::metrics as prom` | Good: Clear alias |
| Call sites | Scattered in handlers | Centralized in actor structs | Better: Single responsibility |
| Type safety | String labels | `ActorType::as_str()` for labels | Better: Enum-based labels |

The MC implementation actually improves on the AC pattern by centralizing metric emission in actor lifecycle methods rather than scattering calls throughout handlers.

## Notes

1. **Test coverage**: The existing tests in `actors/metrics.rs` exercise the Prometheus emission paths, which is good for coverage.

2. **No `#[allow(dead_code)]` needed**: Unlike AC's `observability/metrics.rs` where some functions are pre-defined for future use, all MC actor metric functions are actively called.

3. **Ordering consistency**: MC uses `Ordering::Relaxed` for most operations (appropriate for metrics), while `ControllerMetrics` uses `SeqCst` for heartbeat reporting (appropriate for cross-thread visibility). This is intentional and correct.

4. **Known issue tracking**: The `ControllerMetrics.current_participants` not being updated is correctly identified and scoped for the fix phase. This is not a DRY concern but a completeness issue.

---

## Iteration 2 Review

**Verdict**: APPROVED
**Date**: 2026-02-05

### Summary

The iteration 2 changes correctly wire `ControllerMetrics` through the actor hierarchy for participant tracking. The pattern mirrors the established `ActorMetrics` propagation approach with no DRY violations.

### Findings

None. The implementation is clean and follows established patterns.

### Pattern Analysis

The `ControllerMetrics` integration follows the exact same pattern as `ActorMetrics`:

| Aspect | ActorMetrics | ControllerMetrics | Match |
|--------|--------------|-------------------|-------|
| Creation point | `main.rs` via `ActorMetrics::new()` | `main.rs` via `ControllerMetrics::new()` | Yes |
| Wrapping | `Arc<ActorMetrics>` | `Arc<ControllerMetrics>` | Yes |
| Passed to controller | `MeetingControllerActorHandle::new(..., metrics, ...)` | `MeetingControllerActorHandle::new(..., controller_metrics, ...)` | Yes |
| Stored in controller | `metrics: Arc<ActorMetrics>` field | `controller_metrics: Arc<ControllerMetrics>` field | Yes |
| Passed to meetings | `MeetingActor::spawn(..., Arc::clone(&self.metrics), ...)` | `MeetingActor::spawn(..., Arc::clone(&self.controller_metrics), ...)` | Yes |
| Stored in meeting | `metrics: Arc<ActorMetrics>` field | `controller_metrics: Arc<ControllerMetrics>` field | Yes |
| Update calls | `self.metrics.connection_created()` etc. | `self.controller_metrics.increment_participants()` etc. | Yes |

**Consistency observations**:

1. **Parallel structure**: Both metrics types follow the same `Arc<T>` pattern for shared ownership across actors.

2. **Naming consistency**: The field names (`metrics` vs `controller_metrics`) clearly distinguish the two purposes:
   - `metrics` = `ActorMetrics` for Prometheus emission (connections, meetings, panics)
   - `controller_metrics` = `ControllerMetrics` for GC heartbeat reporting (participant count)

3. **Increment/decrement placement**: Participant count changes are correctly placed at the semantic boundaries:
   - `increment_participants()` in `handle_join()` after successful join (line 618 in meeting.rs)
   - `decrement_participants()` in `handle_leave()` after participant removal (line 868)
   - `decrement_participants()` in `check_disconnect_timeouts()` after grace period expiry (line 1109)

4. **No duplication**: The changes add the new field and calls without duplicating existing logic. Each actor type maintains both metrics references independently.

**Doc comment quality**: The `controller_metrics` field is documented at each level:
- `MeetingControllerActorHandle::new()` docstring updated
- `MeetingActor::spawn()` docstring updated
- Inline comments explain the purpose (GC heartbeat reporting)
