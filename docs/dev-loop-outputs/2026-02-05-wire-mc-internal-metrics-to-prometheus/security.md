# Security Review: Wire MC Internal Metrics

**Reviewer**: Security Specialist
**Verdict**: APPROVED
**Date**: 2026-02-05

## Summary

The Prometheus metric integration in `actors/metrics.rs` is implemented securely. No PII is exposed in metric labels, cardinality is properly bounded, and no credentials or sensitive data flow through the metrics system.

## Findings

### BLOCKER
None.

### CRITICAL
None.

### MAJOR
None.

### MINOR

1. **actor_id logged but not in metrics (Acceptable)**: The `MailboxMonitor` stores `actor_id` (e.g., "meeting-123", "conn-456") internally and logs it via tracing, but correctly does NOT include it in Prometheus labels. This is the right design - meeting/connection IDs could be user-correlated and would cause cardinality explosion. The logging is acceptable for debugging but should not be changed to use actual user-supplied IDs.

2. **No rate limiting on metric emission**: Each enqueue/dequeue triggers a Prometheus gauge update. Under high load with thousands of messages per second, this could have minor performance impact. However, this is acceptable because:
   - The `metrics` crate uses efficient atomic operations
   - The gauge values are simple numeric updates, not label lookups
   - This follows the standard pattern for actor metrics

## Checklist
- [x] No PII in metric labels - Labels only use `actor_type` with 3 fixed enum values
- [x] Cardinality bounds respected - `actor_type` has exactly 3 values (controller, meeting, connection)
- [x] No credential exposure - No secrets, tokens, or auth data flow through metrics
- [x] Proper instrumentation - Functions use `#[must_use]` where appropriate; no `#[instrument]` needed as these are simple counter/gauge updates (not async spans)

## Security Properties Verified

### Label Analysis

| Metric | Label | Values | Bounded |
|--------|-------|--------|---------|
| `mc_meetings_active` | none | n/a | Yes |
| `mc_connections_active` | none | n/a | Yes |
| `mc_actor_mailbox_depth` | `actor_type` | controller, meeting, connection | Yes (3) |
| `mc_actor_panics_total` | `actor_type` | controller, meeting, connection | Yes (3) |
| `mc_messages_dropped_total` | `actor_type` | controller, meeting, connection | Yes (3) |

### Data Flow Review

1. **ActorMetrics** - Only tracks aggregate counts (active meetings, connections). No per-entity identifiers exposed.

2. **MailboxMonitor** - Stores `actor_id` internally but only uses `actor_type.as_str()` for Prometheus labels. The `actor_id` appears only in tracing logs (acceptable for debugging).

3. **ControllerMetrics** - Used for GC heartbeat reporting only, explicitly documented as "no Prometheus emission". Correctly separated from the Prometheus integration.

### Invariant Enforcement

The `ActorType` enum ensures type-safety for labels:
```rust
pub const fn as_str(&self) -> &'static str {
    match self {
        ActorType::Controller => "controller",
        ActorType::Meeting => "meeting",
        ActorType::Connection => "connection",
    }
}
```

This guarantees only these 3 string values can ever be used as labels, preventing accidental PII injection.

## Notes

1. The implementation correctly follows ADR-0011 cardinality guidelines (max 1,000 unique label combinations).

2. The `observability/metrics.rs` module has comprehensive documentation about cardinality bounds, which is excellent for maintainability.

3. The separation between internal metrics (`ControllerMetrics` for GC heartbeats) and Prometheus metrics (`ActorMetrics`, `MailboxMonitor`) is well-designed and prevents accidental exposure.

4. The code uses `Ordering::Relaxed` for most atomic operations, which is appropriate for metrics where strict ordering is not required.

---

## Iteration 2 Review

**Verdict**: APPROVED
**Date**: 2026-02-05

### Summary
Iteration 2 wires the `ControllerMetrics` participant counting into the `MeetingActor` to track join/leave/timeout events for GC heartbeat reporting. The `controller_metrics` field is passed from `main.rs` through `MeetingControllerActorHandle` to each spawned `MeetingActor`.

### Findings
None.

### Notes

1. **No PII exposure**: Participant counts are aggregate integers only. No participant IDs, user IDs, or other identifiable information flows through the `ControllerMetrics`. The calls are:
   - `increment_participants()` on successful join (line 618 in meeting.rs)
   - `decrement_participants()` on voluntary leave (line 868 in meeting.rs)
   - `decrement_participants()` on grace period timeout (line 1109 in meeting.rs)

2. **Thread-safe atomic operations**: The `ControllerMetrics` uses `AtomicU32` with `Ordering::SeqCst` for all participant counter operations (`increment_participants`, `decrement_participants`). The use of `SeqCst` is appropriate here since these counters are read by the heartbeat task and written by multiple `MeetingActor` instances concurrently.

3. **No credential leaks**: The `controller_metrics` is a pure metrics struct containing only `AtomicU32` counters. No secrets, tokens, or authentication data passes through this path.

4. **Proper access control**: The `ControllerMetrics` is shared via `Arc<ControllerMetrics>` and is correctly scoped:
   - Created once in `main.rs` (line 158)
   - Passed to `MeetingControllerActorHandle::new()` (line 194)
   - Passed to each `MeetingActor::spawn()` (line 377 in controller.rs)
   - Used only for GC heartbeat reporting (line 400-403 in main.rs)

5. **Consistent increment/decrement pairing**: Each code path that adds a participant calls `increment_participants()`, and each code path that removes a participant (leave or timeout) calls `decrement_participants()`. This ensures the counter stays accurate over time. Reconnection does NOT double-count since it reuses the existing participant slot.

6. **No Prometheus exposure for ControllerMetrics**: As documented in the metrics module (line 17), `ControllerMetrics` is explicitly for GC heartbeat reporting only and does NOT emit to Prometheus. This is correct design - the aggregate counts go to GC via gRPC heartbeats, not via the public `/metrics` endpoint.
