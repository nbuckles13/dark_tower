# Checkpoint: Meeting Controller Actor Implementation

**Specialist**: meeting-controller
**Phase**: ADR-0023 Phase 6b
**Date**: 2026-01-25

---

## Patterns Discovered

### Pattern: Actor Handle + Actor Task Separation

Each actor consists of:
1. **Handle struct** (`*ActorHandle`) - cloneable, provides public API via channels
2. **Actor struct** - owns state, runs message loop, not directly accessible
3. **JoinHandle** - returned from spawn for supervision

```rust
pub fn spawn(...) -> (Handle, JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel(BUFFER_SIZE);
    let actor = Self::new(receiver, ...);
    let task_handle = tokio::spawn(actor.run());
    let handle = Handle { sender, cancel_token };
    (handle, task_handle)
}
```

### Pattern: CancellationToken Hierarchy

Parent actors pass child tokens to their children for graceful shutdown:
- Controller creates: `let meeting_token = self.cancel_token.child_token();`
- Meeting creates: `let connection_token = self.cancel_token.child_token();`
- Cancelling parent cascades to all children

### Pattern: Borrow Separation for Broadcast

When updating mutable state and then broadcasting:
```rust
// BAD: borrow checker error
if let Some(p) = self.participants.get_mut(id) {
    p.muted = true;
    self.broadcast_update(update_with(p.muted)); // ERROR
}

// GOOD: extract values first
let update = if let Some(p) = self.participants.get_mut(id) {
    p.muted = true;
    Some(ParticipantStateUpdate::MuteChanged { muted: p.muted })
} else {
    None
};
if let Some(update) = update {
    self.broadcast_update(update).await;
}
```

### Pattern: Mailbox Monitoring with Thresholds

Per ADR-0023:
- Meeting actors: normal < 100, warning < 500, critical >= 500
- Connection actors: normal < 50, warning < 200, critical >= 200

Monitor uses atomics for lock-free tracking.

---

## Gotchas Encountered

### Gotcha: Clippy for_kv_map Lint

When iterating over HashMap values only:
```rust
// BAD: triggers clippy::for_kv_map
for (_, managed) in &self.connections { ... }

// GOOD: use values() iterator
for managed in self.connections.values() { ... }
```

### Gotcha: Debug Derive Requires All Fields Debug

When deriving Debug on Participant, ConnectionActorHandle must also be Debug.
Solution: Add `#[derive(Clone, Debug)]` to handle structs.

### Gotcha: Shutdown Test Timing

After `shutdown()`, the actor cancels itself. Subsequent operations fail with channel closed.
Test should check `is_cancelled()` after shutdown rather than attempting more operations.

---

## Key Decisions

### Decision: One Connection Per Meeting Participation

Per ADR-0023 Section 2b: A user in multiple meetings has multiple connections. This simplifies the model (one connection = one participant in one meeting) regardless of MC topology.

### Decision: 30-Second Disconnect Grace Period

Matches binding token TTL. If participant can't reconnect within 30s, their binding token is expired anyway.

### Decision: Session Binding with HMAC-SHA256

Implemented in iteration 2: SessionBindingManager uses HKDF-SHA256 for per-meeting key derivation and HMAC-SHA256 for token generation. Constant-time validation via `ring::hmac::verify`.

### Decision: Participant State in Actor Memory

Participant state lives in MeetingActor memory during implementation. Redis sync for critical state (roster, bindings) to be added in later phase.

---

## Current Status

**Completed**:
- [x] MeetingControllerActor with supervision
- [x] MeetingActor with participant management
- [x] ConnectionActor for signaling
- [x] CancellationToken parent->child propagation
- [x] Mailbox monitoring with ADR-0023 thresholds
- [x] Join/leave/disconnect/reconnect flows
- [x] Self-mute and host-mute (two-tier model)
- [x] Session binding with HMAC-SHA256 (iteration 2)
- [x] Host mute authorization check (iteration 2)
- [x] Time-based grace period tests (iteration 3)
- [x] Real participant count in GetMeeting (iteration 3)
- [x] 64 unit tests passing
- [x] All verification layers passing

**Deferred to later phases**:
- Nonce management in Redis (Phase 6b continued)
- WebTransport actual send/receive (Phase 6g)
- MH enforcement of host-mute (Phase 6d)
- Redis state persistence (Phase 6c)

---

## Patterns Added in Iteration 3

### Pattern: Tokio Time Testing

For testing time-dependent behavior (grace periods, timeouts):
```rust
#[tokio::test(start_paused = true)]
async fn test_timeout_behavior() {
    // Create actors and state
    // ...

    // Advance time precisely
    tokio::time::advance(Duration::from_secs(30)).await;

    // Verify time-dependent behavior occurred
}
```

Requires `tokio = { features = ["test-util"] }` in dev-dependencies.

### Pattern: Async Actor State Queries

When parent actor needs child actor state:
```rust
async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingInfo, McError> {
    let handle = self.meetings.get(meeting_id)?;
    match handle.get_state().await {
        Ok(state) => Ok(MeetingInfo {
            participant_count: state.participants.len(),
            // ...
        }),
        Err(_) => Ok(MeetingInfo::cached_fallback()),
    }
}
```

Handle errors gracefully - actor may have shut down between lookup and query.

---

## Files Created

- `crates/meeting-controller/src/actors/mod.rs` - Actor module organization
- `crates/meeting-controller/src/actors/controller.rs` - MeetingControllerActor
- `crates/meeting-controller/src/actors/meeting.rs` - MeetingActor
- `crates/meeting-controller/src/actors/connection.rs` - ConnectionActor
- `crates/meeting-controller/src/actors/messages.rs` - Message types
- `crates/meeting-controller/src/actors/metrics.rs` - Mailbox monitoring
- `crates/meeting-controller/src/actors/session.rs` - Session binding tokens (iteration 2)

## Files Modified

- `crates/meeting-controller/src/lib.rs` - Added actors module export
- `crates/meeting-controller/Cargo.toml` - Added hex, tokio test-util
