# Meeting Controller Integration Guide

What other services need to know when integrating with the Meeting Controller.

---

## Integration: Actor Hierarchy for MC State Queries
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`, `crates/meeting-controller/src/actors/meeting.rs`

When GC queries MC status (for health checks or load balancing), the controller queries child meeting actors to get accurate state. `MeetingControllerActorHandle::get_status()` returns sync cached counts, but `get_meeting(meeting_id)` calls `MeetingActorHandle::get_state()` to get real-time participant count and fencing generation. This ensures consistency but adds latency. For high-frequency health checks, use `get_status()` which uses cached metrics. For assignment decisions, use `get_meeting()` for accuracy.

---

## Integration: CancellationToken Propagation from GC
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/actors/controller.rs`

When MC receives shutdown signal (SIGTERM or GC command), it cancels the root `CancellationToken`. This propagates to all meetings, then to all connections. Connections have 50ms to send close frames. Meetings wait up to 5s per connection. Controller waits up to 30s per meeting. GC should set appropriate deadline and retry if MC doesn't acknowledge shutdown within deadline.

---

## Integration: Session Binding Flow
**Added**: 2026-01-25
**Related files**: `proto/signaling.proto`, `crates/meeting-controller/src/session/`

Session binding enables reconnection after network disruption:

1. Client sends `JoinRequest` (first time: no session fields)
2. MC creates session, generates `session_token` (opaque, signed)
3. MC responds with `JoinResponse` including `session_token` and `expiry_timestamp`
4. Client stores session token locally
5. On reconnect: Client sends `JoinRequest` with `session_token` and `last_sequence_number`
6. MC validates token, checks sequence continuity, restores session state
7. MC responds with `recovery_data` containing missed events since `last_sequence_number`

Session tokens are bound to participant + meeting. Expiry defaults to 5 minutes (configurable). Sequence gaps beyond buffer size force full rejoin.

---

## Integration: GC-to-MC Assignment Notification
**Added**: 2026-01-25
**Related files**: `proto/internal.proto`, `crates/meeting-controller/src/grpc/`

When GC assigns a meeting to MC, it notifies via gRPC:

1. GC calls `AssignMeeting` RPC with `meeting_id`, `meeting_code`, `settings`
2. MC validates it has capacity (rejects if at max)
3. MC creates internal meeting state (participants map, media state)
4. MC stores assignment in Redis with fencing token
5. MC responds with success or rejection reason

MC must be prepared for duplicate assignments (GC retries) - use fencing tokens to detect stale assignments. If MC is shutting down, reject with `SHUTTING_DOWN` reason so GC can reassign.

---

## Integration: Mute State Synchronization
**Added**: 2026-01-25
**Related files**: `proto/signaling.proto`

Two mute states require different handling:

**Self-mute** (`is_self_muted`):
- Client toggles freely via `MuteRequest`
- MC broadcasts to all participants
- Informational only - client enforces locally
- No permission check required

**Host-mute** (`is_host_muted`):
- Host sends `HostMuteRequest` targeting participant
- MC validates requester is host
- MC broadcasts to all participants
- Target client MUST mute (enforced)
- Target can request unmute, host must approve

UI should distinguish: self-muted shows mute icon, host-muted shows lock icon. Combined state (both true) shows lock icon with tooltip explaining host override.

---

## Integration: Redis Session Storage
**Added**: 2026-01-25
**Related files**: `crates/meeting-controller/src/session/`, `crates/mc-test-utils/src/`

MC uses Redis for ephemeral session state:

**Keys**:
- `session:{session_id}` - Participant session data (TTL: session timeout)
- `meeting:{meeting_id}:participants` - Set of participant IDs
- `meeting:{meeting_id}:fence` - Fencing token for assignment validation
- `nonce:{participant_id}` - Last seen nonce for replay protection

**Patterns**:
- All keys have TTL (no orphaned data)
- Use Lua scripts for atomic multi-key operations
- Fencing tokens prevent split-brain during MC failover

For testing, `mc-test-utils` provides `MockRedis` that simulates these patterns in-memory.

---
