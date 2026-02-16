# Code Review: Wire MC Internal Metrics to Prometheus

**Reviewer**: Code Quality Reviewer
**Verdict**: APPROVED
**Date**: 2026-02-05

## Summary

The Prometheus integration in `actors/metrics.rs` is well-implemented with clean separation between internal tracking and Prometheus emission. The code follows ADR-0002 (no-panic policy) correctly and uses proper Rust idioms for atomic operations.

## Findings

### BLOCKER

None.

### MAJOR

None.

### MINOR

1. **Inconsistent depth calculation in `record_dequeue`** (lines 168-174):
   ```rust
   let new_depth = self.depth.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
   ```
   The `fetch_sub` returns the *previous* value, so `saturating_sub(1)` on it correctly calculates the new value. However, this differs from `record_enqueue` which does `fetch_add(1) + 1`. Consider using a consistent pattern:
   ```rust
   // Option A: Match enqueue style
   let new_depth = self.depth.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
   // Option B: Add comment explaining why these differ
   ```
   The current implementation is correct, but a comment would improve clarity.

2. **Warning log condition may be off-by-one** (lines 152-153):
   ```rust
   else if level == MailboxLevel::Warning && new_depth == self.actor_type.normal_threshold()
   ```
   The check `new_depth == normal_threshold` will only log when depth exactly equals the normal threshold. However, if messages are enqueued in batches, this exact value might be skipped. Consider logging when `new_depth == normal_threshold + 1` (the first value in Warning range).

### TECH_DEBT

1. **Known issue: `ControllerMetrics.current_participants` never updated in production**
   - The increment/decrement participant methods exist but are not called by MeetingActor on join/leave
   - Documented in task scope - will be addressed in dev-loop-fix phase

2. **Test module allows clippy panics** (line 428):
   ```rust
   #[allow(clippy::unwrap_used, clippy::expect_used)]
   ```
   This is acceptable per ADR-0002 (Exception: Tests), but consider using `#[expect]` instead of `#[allow]` for better lint hygiene as recommended in ADR-0002.

3. **String allocation on every metric emission** (e.g., line 65 in metrics.rs):
   ```rust
   gauge!("mc_actor_mailbox_depth", "actor_type" => actor_type.to_string()).set(depth as f64);
   ```
   The `to_string()` creates a new allocation per emission. For high-frequency metrics, consider using `&'static str` labels directly (which the code already does by using `ActorType::as_str()`). The current pattern is acceptable for the expected call frequency.

## Notes

### Positive Observations

1. **ADR-0002 Compliance**: No `unwrap()`, `expect()`, `panic!()`, or index operations in production code. The test module correctly uses `#[allow]` for test conveniences.

2. **Atomic Operations**: Correct use of `Ordering::Relaxed` for statistics-only counters where strict ordering isn't required, and `Ordering::SeqCst` for `ControllerMetrics` where consistency matters for heartbeat snapshots.

3. **Safe Integer Casting**: The `usize as u64` and `usize as f64` casts are documented as safe with comments explaining the realistic bounds (lines 351-352, 364-365).

4. **Compare-and-swap for peak tracking** (lines 125-136): Proper lock-free CAS loop for updating peak depth without data races.

5. **Clear documentation**: Module-level documentation explains the Prometheus integration and which metrics are wired where.

6. **Separation of concerns**: `ControllerMetrics` is explicitly documented as GC heartbeat-only (not Prometheus), which is a good design choice avoiding duplicate metric sources.

### Code Quality Assessment

| Criterion | Rating | Notes |
|-----------|--------|-------|
| ADR-0002 Compliance | Excellent | No panics in production code |
| Error Handling | N/A | No fallible operations in this module |
| Documentation | Good | Clear module docs and function docs |
| Naming | Good | Descriptive function and variable names |
| Rust Idioms | Good | Proper atomics usage, const fn where applicable |
| Test Coverage | Good | Comprehensive unit tests covering all paths |

### Files Reviewed

- `/home/nathan/code/feature/mc-observability/crates/meeting-controller/src/actors/metrics.rs`
- `/home/nathan/code/feature/mc-observability/crates/meeting-controller/src/observability/metrics.rs` (for integration context)

---

## Iteration 2 Review

**Verdict**: APPROVED
**Date**: 2026-02-05

### Summary

The `controller_metrics` field has been correctly wired through the actor hierarchy (main.rs -> controller.rs -> meeting.rs), with increment/decrement calls placed at appropriate participant lifecycle points.

### Findings

None. The iteration 2 changes are clean and correct.

### Notes

**Arc Usage - Correct**:
- `ControllerMetrics::new()` in `main.rs` returns `Arc<ControllerMetrics>` directly
- Passed via `Arc::clone()` to `MeetingControllerActorHandle::new()`
- Controller stores as `controller_metrics: Arc<ControllerMetrics>` field
- Further cloned via `Arc::clone(&self.controller_metrics)` when spawning each `MeetingActor`
- Meeting actor stores its own clone as `controller_metrics: Arc<ControllerMetrics>`
- This enables multiple MeetingActors to share the same underlying metrics atomically

**Atomic Operations - Correct Placement**:
| Event | Location | Call |
|-------|----------|------|
| Participant joins | `handle_join()` line 618 | `increment_participants()` |
| Participant leaves (voluntary) | `handle_leave()` line 868 | `decrement_participants()` |
| Disconnect grace period expires | `check_disconnect_timeouts()` line 1109 | `decrement_participants()` |

The increment is called *after* successful participant insertion into `self.participants`, ensuring the count reflects actual state. The decrement calls are correctly placed *after* `self.participants.remove()` succeeds.

**ADR-0002 Compliance**:
- No new panics, unwraps, or expects introduced in production code
- Test code correctly uses `#[allow(clippy::unwrap_used, clippy::expect_used)]` for test convenience

**Missing Decrement Cases - Confirmed None**:
- `handle_end_meeting()` cancels all connections and triggers shutdown, but does not decrement per-participant. This is intentional - the entire MC is shutting down and participant counts are no longer relevant.
- Reconnection via `handle_reconnect()` does NOT decrement on disconnect or increment on reconnect. This is correct because the participant count tracks *logical* participants, not connections. A disconnected participant in grace period is still counted.

**Documentation Quality**:
- Field doc: `/// Controller metrics for GC heartbeat reporting (participant count).`
- Parameter doc in `spawn()`: `/// * `controller_metrics` - Controller metrics for GC heartbeat reporting (participant count)`
- Clear and accurate

**Files Reviewed (Iteration 2)**:
- `/home/nathan/code/feature/mc-observability/crates/meeting-controller/src/actors/meeting.rs`
- `/home/nathan/code/feature/mc-observability/crates/meeting-controller/src/actors/controller.rs`
- `/home/nathan/code/feature/mc-observability/crates/meeting-controller/src/main.rs`
