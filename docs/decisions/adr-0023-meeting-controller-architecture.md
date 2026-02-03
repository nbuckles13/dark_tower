# ADR-0023: Meeting Controller Architecture

**Status**: Accepted
**Date**: 2026-01-23
**Deciders**: Multi-agent debate (MC, MH, GC, Database, Infrastructure, Security, Test, Observability, Operations specialists)
**Debate Rounds**: 3
**Final Consensus**: 92.75% average satisfaction

## Context

Dark Tower requires a Meeting Controller (MC) to manage WebTransport signaling sessions, coordinate participants within meetings, and orchestrate Media Handler (MH) assignments. The MC must:

1. Manage WebTransport connections for signaling (join, leave, mute, layout)
2. Recover sessions after connection drops (correlation ID + binding token)
3. Assign participants to Media Handlers for media routing
4. Coordinate with other MCs for cross-region meetings
5. Handle MC failures gracefully with minimal participant disruption

### Key Problems Identified

**Problem 1: Session Recovery Security**
- Clients reconnecting after network disruption need to recover their session state
- Correlation IDs alone are vulnerable to hijacking if stolen
- Need secure binding that prevents replay attacks

**Problem 2: Split-Brain Prevention**
- Multiple MCs could claim the same meeting after network partition
- Need fencing mechanism to prevent stale MCs from corrupting state

**Problem 3: Fast Failure Recovery**
- Server-side heartbeat detection is slow (30s typical)
- Need faster failover for good user experience (target: P95 < 15s)

**Problem 4: Media Handler Assignment**
- Participants need redundant MH assignments for resilience
- MH selection must balance load and geographic proximity

## Decision

### 1. Session Binding Token Pattern

**Server-generated correlation IDs** (UUIDv7) with **one-time nonce binding tokens** that rotate on each successful reconnection.

**First Connection Flow**:
```
Client → MC: JoinRequest {
    join_token: <JWT from GC>,
    binding_token: None  // First join
}

MC validates JWT, extracts user_id
MC generates: correlation_id (UUIDv7), participant_id, nonce
MC computes: binding_token = HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)
MC stores: binding in Redis with TTL

MC → Client: JoinResponse {
    correlation_id,
    participant_id,
    binding_token,
    ...
}
```

**Reconnection Flow**:
```
Client → MC: JoinRequest {
    correlation_id: <same>,
    join_token: <fresh JWT from GC>,
    binding_token: <stored binding token>
}

MC validates:
1. JWT signature and claims (fresh token)
2. user_id in JWT matches original binding
3. HMAC verification (constant-time via subtle::ConstantTimeEq)
4. Nonce not already used (atomic Redis SETNX)
5. Token not expired (30s TTL)

If all pass → session recovered, issue new nonce
If binding invalid → new session (no state recovery)
```

**Security Parameters**:

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Binding token TTL | 30 seconds | Balance usability and security |
| Clock skew allowance | 5 seconds | Handle reasonable time drift |
| Master secret rotation | On deploy (~weekly) | Token TTL provides primary protection; simplifies ops |
| Rotation grace period | 1 hour | Cover in-flight tokens during rolling deploy |
| Nonce grace window | 5 seconds | Handle in-flight retransmits |
| Nonce storage TTL | 35 seconds | Token TTL + grace window |

**Key Derivation**:
```rust
meeting_key = HKDF-SHA256(
    ikm: master_secret,  // MC-owned secret, separate from AC signing keys
    salt: meeting_id,
    info: b"session-binding"
)
```

> **Security note**: Binding tokens are defense-in-depth, not primary authentication. A stolen binding token alone cannot hijack a session—the attacker also needs a valid JWT for the same `user_id`. This means binding tokens can be stored in less secure locations (e.g., `localStorage`) compared to JWTs. The 30-second TTL is the primary protection; even with a compromised master secret, attackers have only a 30-second window and still need a valid JWT.

> **Master secret ownership**: The `master_secret` used for binding token HMAC is owned and managed by MC, separate from AC's EdDSA signing keys. MC rotates this secret on each deployment. Emergency rotation is possible via forced redeploy.

### 1a. Participant Disconnect Handling

When a WebTransport connection drops, MC manages participant lifecycle:

```
T=0       Connection closed detected
          Participant marked as "disconnected"

T=0-30s   Disconnected grace period:
          - Participant still visible to others (grayed out in UI)
          - Subscriptions paused but not removed
          - MH slots held for fast recovery
          - Binding token still valid for reconnection

T=30s     If not reconnected:
          - Participant removed from meeting roster
          - Other participants notified: ParticipantLeft { participant_id, reason: TIMEOUT }
          - Subscriptions cleaned up
          - MH slots released
          - Binding token invalidated (nonce consumed or expired)
          - Redis state for participant deleted
```

**Why 30 seconds?** Aligns with binding token TTL. If a participant can't reconnect within 30s, their binding token is expired anyway, so they'd join as a new participant regardless.

**Configurable per meeting type?** Future consideration—some use cases (e.g., webinars) might want longer grace periods. For now, 30s is fixed.

### 2. Actor Model Hierarchy

```
MeetingControllerActor (singleton per MC instance)
├── supervises N MeetingActors
│   └── MeetingActor (one per active meeting)
│       ├── owns meeting state
│       ├── supervises N ConnectionActors
│       │   └── ConnectionActor (one per WebTransport connection)
│       └── handles media routing coordination
└── MhRegistryActor (tracks MH health via heartbeats)
```

**Key relationship**: ConnectionActor is 1:1 with meeting participation. One WebTransport connection = one meeting. Users in multiple meetings have multiple connections (even to the same MC). This keeps the model consistent regardless of MC topology.

**Cancellation tokens**: All actors use `CancellationToken` from `tokio-util` for graceful shutdown:

```rust
struct MeetingActor {
    cancel_token: CancellationToken,
    child_tokens: Vec<CancellationToken>,  // For ConnectionActors
    // ...
}

impl MeetingActor {
    async fn run(&self) {
        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    // Cancel all children, flush state, notify participants
                    self.graceful_shutdown().await;
                    return;
                }
                msg = self.mailbox.recv() => {
                    self.handle(msg).await;
                }
            }
        }
    }
}
```

Parent passes child tokens: `let connection_token = meeting_token.child_token();`

**Supervision**: Tokio tasks with panic-catching wrappers. Clean vs error vs panic exits handled differently.

**Inter-Actor Communication**: Route through MeetingActor for isolation. Message types: ClientMessage, ParticipantJoined/Left, LayoutUpdate, MediaRouteRequest.

**Mailbox Monitoring Thresholds**:

| Actor Type | Normal | Warning | Critical |
|------------|--------|---------|----------|
| Meeting | < 100 | 100-500 | > 500 |
| Participant | < 50 | 50-200 | > 200 |

### 2a. Graceful Shutdown with Meeting Migration

On graceful shutdown, MC actively migrates meetings to healthy MCs rather than waiting for them to end:

```
T=0       MC1 receives SIGTERM
          - Sets accepting_new = false
          - Cancels top-level CancellationToken (propagates to children)

T=1s      MC1 → GC: McDraining { mc_id, meeting_count, deadline: 30s }

T=2s      GC selects target MC for each meeting (load-balanced)
          GC → MC1: MigrateMeetings { assignments: [(meeting_1, MC2), ...] }

T=3-25s   For each meeting in parallel:
          1. MC1 snapshots meeting state (participants, subscriptions, MH assignments)
          2. MC1 → MC2: PrepareMeetingMigration { meeting_id, state }
          3. MC2 loads state, acquires fencing token (generation + 1)
          4. MC2 → MC1: MigrationReady { meeting_id }
          5. MC1 → clients: MeetingMigrating { new_mc_endpoint, new_binding_token }
          6. Clients reconnect to MC2 (seamless - same meeting continues)
          7. MC1 releases meeting resources

T=30s     Any remaining meetings force-closed (edge case)

T=35s     MC1 → GC: McDrained { mc_id, migrated: N, closed: M }
          MC1 exits
```

**Why migrate instead of wait?** Waiting 30s for meetings to end naturally causes unnecessary disruption. Migration preserves continuity—participants see a brief reconnect (~2s), not a meeting end.

**Fencing during migration**: MC2 acquires generation+1 fencing token before becoming authoritative. Any late writes from MC1 are rejected.

### 2b. Multi-Meeting Participation (Forward Compatibility)

Users may need to be in multiple meetings simultaneously (e.g., teacher monitoring breakout rooms). Current design supports this:

**One connection per meeting** (required):
```
Teacher in 3 breakout rooms:
├── Connection 1 → MC1 → Breakout-A (as observer)
├── Connection 2 → MC1 → Breakout-B (as observer)
└── Connection 3 → MC2 → Breakout-C (as observer)
```

**Why one connection per meeting?**
- Consistent model whether meetings are on same or different MCs
- Simple lifecycle: close connection = leave meeting
- No message multiplexing complexity
- QUIC connections to same MC share UDP socket (minimal overhead)

**Identity model**:
- `user_id` (from JWT): Identifies the person (same across all meetings)
- `participant_id`: Scoped to one meeting (user has N participant_ids for N meetings)
- `correlation_id` / `binding_token`: Scoped to one connection (one per meeting)

**Roles per meeting** (future consideration):
```rust
enum ParticipantRole {
    Participant,  // Full audio/video send+receive
    Observer,     // Receive-only (teacher listening to breakout)
    Announcer,    // Send-only to multiple meetings (teacher announcement)
}
```

**Media routing for multi-meeting** (deferred):
- Each meeting has independent MH assignments
- Teacher in 3 meetings = 3 separate media paths
- Broadcast/announcement routing TBD in Media Handler ADR

### 2c. Actor Panic Recovery

Actor panics are anomalous (should not happen in normal operation) but must be handled gracefully:

| Actor | On Panic | Recovery Action |
|-------|----------|-----------------|
| **MeetingControllerActor** | Fatal | Trigger graceful shutdown; main loop detects via `JoinHandle::is_finished()` |
| **MeetingActor** | Detected by MeetingControllerActor | Trigger meeting migration to another MC (same flow as graceful shutdown) |
| **ConnectionActor** | Detected by MeetingActor | Mark client as disconnected; 30s grace period for reconnect (no state lost) |

**Detection mechanism**:
```rust
// MeetingControllerActor monitors child MeetingActors
tokio::select! {
    result = meeting_actor_handle => {
        match result {
            Ok(()) => {
                // Clean exit: meeting ended naturally, or CancellationToken triggered graceful shutdown
            }
            Err(join_error) => {
                // Any error is unexpected (we use CancellationTokens, never abort())
                error!("MeetingActor failed unexpectedly: {:?}", join_error);
                self.trigger_meeting_migration(meeting_id).await;
            }
        }
    }
    // ... other branches
}
```

**Observability** (all panics emit):
- Counter: `mc_actor_panic_total{actor_type="meeting"|"connection"|"controller"}`
- ERROR log with panic message (backtrace in debug builds)
- Alert: Any actor panic should trigger investigation (indicates bug)

**Why MeetingActor panic triggers migration, not restart**:
- Panic indicates unexpected state; restarting may repeat the panic
- Migration to fresh MC ensures clean slate
- State is in Redis; new MC loads it without corruption risk

### 3. Fencing Token for Split-Brain Prevention

**Problem**: After network partition, multiple MCs might think they own the same meeting.

**Solution**: Generation-based fencing token validated atomically via Lua script.

```lua
-- Lua script for fenced write
local current_gen = redis.call('GET', KEYS[1] .. ':gen')
local expected_gen = ARGV[1]
local mc_id = ARGV[2]

if current_gen and tonumber(current_gen) > tonumber(expected_gen) then
    return {err = 'FENCED_OUT'}
end

if current_gen and tonumber(current_gen) == tonumber(expected_gen) then
    local current_mc = redis.call('GET', KEYS[1] .. ':mc')
    if current_mc and current_mc ~= mc_id then
        return {err = 'SPLIT_BRAIN'}
    end
end

-- Update generation and perform write
redis.call('SET', KEYS[1] .. ':gen', expected_gen)
redis.call('SET', KEYS[1] .. ':mc', mc_id)
-- ... actual state write ...
return {ok = 'SUCCESS'}
```

All state writes go through fencing validation.

**How Lua scripts work**: Redis has a built-in Lua interpreter. Scripts execute atomically inside the Redis server—no other commands can interleave during script execution. This guarantees the check-and-write is truly atomic (no race between reading generation and writing).

**Script registration**: Each MC registers the Lua script on startup via `SCRIPT LOAD`, which returns a SHA1 hash. Subsequent calls use `EVALSHA <hash>` (just the hash, not the full script). Multiple MCs loading the same script is idempotent—same script text = same hash. No infrastructure setup needed; the script is part of MC's code, registered at runtime.

**No contention in normal operation**: Each meeting is owned by exactly one MC (assigned by GC). MCs write to different keys (`meeting:A:*` vs `meeting:B:*`), so there's no blocking. The fencing token is a **safety net** for rare split-brain scenarios (network partition recovery, failover races), not a coordination mechanism. When split-brain does occur, the stale MC is fenced out on its first write attempt and backs off immediately.

### 4. Client-Reported Unreachability

**Problem**: Server-side heartbeat detection takes 30s. Users experience ~35s recovery.

**Solution**: Clients report MC unreachability to GC, triggering faster failover.

```protobuf
message ReportMcUnreachable {
    string mc_id = 1;
    string reporter_participant_id = 2;
    UnreachableReason reason = 3;
    uint32 affected_meetings = 4;
}

enum UnreachableReason {
    UNREACHABLE_REASON_UNSPECIFIED = 0;
    UNREACHABLE_REASON_CONNECTION_REFUSED = 1;
    UNREACHABLE_REASON_TIMEOUT = 2;
    UNREACHABLE_REASON_TLS_ERROR = 3;
}
```

**Quorum Formula**: `max(1, floor(n/2) + 1)`

| Participants | Quorum | Behavior |
|--------------|--------|----------|
| 1 | 1 | Single participant authoritative |
| 2 | 2 | Both must agree (timestamp tiebreaker) |
| 3+ | majority | Standard majority rule |

**Recovery Timeline**:
```
T=0      MC crashes
T=5s     Clients detect disconnect, report to GC
T=8s     GC reaches quorum (3+ reports), triggers failover
T=10s    New MC loads state from Redis
T=12s    Clients reconnect with binding tokens
T=15s    Session recovered (P95 target)
```

### 5. Media Handler Assignment

**GC assigns MHs alongside MC** - when GC assigns a meeting to an MC, it also assigns MH(s). MC doesn't discover or select MHs directly.

#### 5a. MH Registration with GC

MHs register with their regional GC (not MC). GC maintains global MH registry via cross-region sync:

```
us-west region                         eu-west region
┌──────────────────────────┐           ┌──────────────────────────┐
│  MH-A ──register──► GC   │           │  MH-C ──register──► GC   │
│  MH-B ──load ────► GC    │           │  MH-D ──load ────► GC    │
│           │              │           │           │              │
│     PostgreSQL           │◄─────────►│     PostgreSQL           │
│    (mh_registry)         │  GC-to-GC │    (mh_registry)         │
└──────────────────────────┘   sync    └──────────────────────────┘
```

```protobuf
message RegisterMh {
    string mh_id = 1;
    string region = 2;
    string zone = 3;
    string webtransport_endpoint = 4;
    uint32 max_streams = 5;
}

message MhLoadReport {
    string mh_id = 1;
    uint32 current_streams = 2;
    uint32 max_streams = 3;
    uint32 cpu_percent = 4;
    uint32 bandwidth_ingress_percent = 5;
    uint32 bandwidth_egress_percent = 6;
    uint32 packet_loss_permille = 7;
    MhHealthStatus health_status = 8;
}
```

MH sends load reports to regional GC every 5-10 seconds. GC syncs MH registry cross-region using existing GC-to-GC infrastructure (ADR-0010).

#### 5b. Meeting Assignment Flow (GC → MC)

When a client joins, GC assigns both MC and MH(s), then notifies MC:

```
T=0    Client → GC: JoinMeeting { meeting_id }

T=1    GC: Selects MC-1 (load balancing)
       GC: Selects MH-A, MH-B for this region (2 MHs in different AZs)

T=2    GC → MC-1: AssignMeeting {
           meeting_id,
           mh_assignments: [
               { mh_id: "mh-a", endpoint: "...", role: PRIMARY },
               { mh_id: "mh-b", endpoint: "...", role: BACKUP }
           ]
       }

T=3    MC-1: Accepts or rejects

T=4    If accepted:
           GC → Client: JoinResponse { mc_endpoint }
           Client connects to MC-1, MC-1 already knows MH assignments

       If rejected:
           GC selects MC-2, repeats from T=2
```

**MC can reject meeting** (backpressure):
```protobuf
message AssignMeeting {
    string meeting_id = 1;
    repeated MhAssignment mh_assignments = 2;  // Ranked list: [primary, backup1, backup2, ...]
    string requesting_gc_id = 3;
}

message AssignMeetingResponse {
    bool accepted = 1;
    RejectionReason rejection_reason = 2;  // If not accepted
}

enum RejectionReason {
    REJECTION_REASON_UNSPECIFIED = 0;
    REJECTION_REASON_AT_CAPACITY = 1;
    REJECTION_REASON_DRAINING = 2;
    REJECTION_REASON_UNHEALTHY = 3;
}
```

**Ranked MH list**: GC provides MH assignments as a ranked list (primary, backup1, backup2, ...). MC tries in order without round-trip to GC. Only contacts GC if all options exhausted.

**Why MC can reject:**
- MC knows its true capacity better than GC's view (heartbeats can lag)
- Provides backpressure during load spikes
- Prevents overload cascades

#### 5c. MH Proactive Notifications

MHs proactively notify MCs about load/health changes, enabling preemptive switching before rejection:

```protobuf
message MhLoadNotification {
    string mh_id = 1;
    uint32 capacity_percent = 2;
    MhHealthStatus health = 3;
    uint32 estimated_available_streams = 4;
}
```

MH sends notifications to all MCs it has active streams with. MC can:
- Preemptively switch new participants to backup MH before primary rejects
- Mark MH as degraded and prefer alternatives
- Reduce latency vs. waiting for rejection

#### 5d. MH Rejection and Replacement

If MH rejects or all ranked options exhausted, MC requests replacement from GC:

```
T=0    MC tries MH-A (primary) → rejects "at capacity"
T=1    MC tries MH-B (backup1) → rejects "at capacity"
T=2    MC → GC: RequestMhReplacement {
           meeting_id,
           failed_mh_ids: ["mh-a", "mh-b"],
           reason: CAPACITY_EXCEEDED
       }
T=3    GC selects MH-C, MH-D as new options
T=4    GC → MC: MhReplacement { meeting_id, new_mhs: ["mh-c", "mh-d"] }
T=5    MC routes participant to MH-C
```

This should be rare—ranked list + proactive notifications handle most cases without GC round-trip.

#### 5e. Cross-Region Meetings

For cross-region meetings, each region's GC assigns MHs for its participants:

```
Meeting-123 (cross-region):
├── us-west participants → MH-A, MH-B (assigned by GC-us-west)
└── eu-west participants → MH-C, MH-D (assigned by GC-eu-west)

MH-A ←──peer──► MH-C  (cross-region media relay)
```

MC tells MH about cross-region peers so MHs can establish media relay connections.

#### 5f. MH Selection Algorithm (in GC)

GC uses weighted scoring to select MHs:

```rust
fn score_mh(mh: &MhLoadReport, criteria: &MhSelectionCriteria) -> f32 {
    let mut score = 1.0;

    // Health penalty
    score *= match mh.health_status {
        MhHealthStatus::Healthy => 1.0,
        MhHealthStatus::Degraded => 0.5,
        _ => 0.0,
    };

    // Capacity score (prefer less loaded)
    let utilization = mh.current_streams as f32 / mh.max_streams as f32;
    score *= 1.0 - (utilization * 0.5);

    // Geographic preference (same AZ > same region > other region)
    if mh.zone == criteria.preferred_zone {
        score *= 1.2;
    } else if mh.region == criteria.preferred_region {
        score *= 1.1;
    }

    score.min(1.0)
}
```

**Selection rules:**
- Assign 2 MHs per meeting (primary + backup)
- Primary and backup must be in different AZs
- Prefer MHs in same region as participants

### 6. State Persistence

**Hybrid Write Strategy**:

| State Type | Write Mode | Examples |
|------------|------------|----------|
| Session binding | **Sync** (WAIT for replica) | meeting_id → mc_id |
| Participant roster | **Sync** | Join/leave events |
| MH assignments | **Sync** | Routing tables |
| Host-mute status | **Sync** | Enforced mute (see below) |
| Self-mute status | Async | Informational only |
| Subscriptions | Async | Reconstructable |

**Mute State Model**:

Two distinct mute types with different enforcement semantics:

| Mute Type | Who Sets | Enforcement | Purpose |
|-----------|----------|-------------|---------|
| **Self-mute** | Participant | None (informational) | UI indicator, bandwidth hint |
| **Host-mute** | Host/Moderator | MC + MH | Moderation, forced silence |

**Self-mute** (informational):
- Participant mutes themselves via client UI
- MC distributes status to other participants (mute icon in UI)
- Client is *expected* to stop sending media (bandwidth optimization)
- MH does NOT enforce—if client still sends, media is routed
- Stored async, best-effort (reconstructable from client state)

**Host-mute** (enforced):
- Host/moderator mutes a participant
- MC records enforced mute status (sync write to Redis)
- MC notifies MH: `EnforceMute { participant_id, muted: true }`
- MH drops/discards any media from muted participant
- Participant cannot bypass by unmuting client-side
- Participant can "request unmute" which host approves/denies

```rust
enum MuteSource {
    SelfMuted,           // Informational, client-controlled
    HostMuted {          // Enforced, server-controlled
        muted_by: UserId,
        reason: Option<String>,
    },
}

struct ParticipantMuteState {
    audio_self_muted: bool,      // Informational
    video_self_muted: bool,      // Informational
    audio_host_muted: Option<MuteSource>,  // Enforced if Some
    video_host_muted: Option<MuteSource>,  // Enforced if Some
}
```

**MH enforcement** (per ADR-MH, future):
```protobuf
message EnforceMute {
    string participant_id = 1;
    bool audio_muted = 2;
    bool video_muted = 3;
}
```

MH maintains enforced mute list. On receiving media from muted participant, MH either:
- Drops packets silently (preferred—saves bandwidth to subscribers)
- Or routes to null destination (if drop not feasible)

### 7. Infrastructure

**Redis HA**: Redis Sentinel with 3-node quorum

| Parameter | Value |
|-----------|-------|
| Sentinel nodes | 3 |
| Quorum | 2 |
| `down-after-milliseconds` | 5000 |
| `failover-timeout` | 30000 |

**MH Discovery**: Kubernetes headless Service with DNS SRV lookup
- Refresh interval: 10s with jitter

**Graceful Shutdown Drain** (60s total):

**Meeting Migration State Machine** (per meeting on draining MC):

```
┌─────────┐    drain     ┌──────────────┐   MC2 ready   ┌────────────┐   all clients   ┌──────────┐
│  Active │───started───►│ ShuttingDown │──────────────►│  Migrating │───moved────────►│ Complete │
└─────────┘              └──────────────┘               └────────────┘                 └──────────┘
     │                          │                              │
     │ normal ops               │ read-only                    │ redirect all
     │ read+write               │ no new state                 │ requests to MC2
```

| State | MC1 Behavior | Client Experience |
|-------|--------------|-------------------|
| **Active** | Normal read/write operations | Normal |
| **ShuttingDown** | Read-only, rejects writes with `MIGRATING` | Client retries after 500ms |
| **Migrating** | Redirect all requests: `REDIRECT { new_mc_endpoint }` | Client reconnects to MC2 |
| **Complete** | Connection closed, resources freed | On MC2 |

**Drain Timeline**:

```
T=0       MC1 receives SIGTERM
          - Set accepting_new = false
          - All meetings → ShuttingDown state
          - MC1 rejects any state-mutating requests with MIGRATING (client retries 500ms)

T=1s      MC1 → GC: McDraining { mc_id, meeting_count, deadline: 60s }

T=2s      GC selects target MC for each meeting
          GC → MC1: MigrateMeetings { assignments: [(meeting_1, MC2), ...] }

T=3-25s   For each meeting:
          1. MC1 → MC2: PrepareMeetingMigration { meeting_id, current_generation }
             (MC2 reads state from Redis, not from message—Redis is source of truth)
          2. MC2 → MC1: MigrationAccepted { meeting_id }
             (MC2 has received handoff, will load state from Redis)
          3. MC1 transitions: ShuttingDown → Migrating
          4. MC1 → clients: REDIRECT { new_mc_endpoint }

T=25-55s  Migrating state window:
          - Clients reconnect to MC2 at varying times
          - Any request to MC1 returns: REDIRECT { new_mc_endpoint }
          - MC2 loads state from Redis, acquires fencing token (generation + 1)
          - MC2 handles all state mutations once MeetingActor initialized

          Client arrival before MC2 ready:
          - ConnectionActor sends join message to MeetingActor
          - MeetingActor not yet initialized → message waits in mailbox
          - Once MeetingActor loads state, processes queued messages
          - Timeout: 500ms → ask client to reconnect

T=55s     MC1 force-closes remaining connections (stragglers)
          Meeting → Complete

T=60s     MC1 → GC: McDrained { mc_id, migrated: N, closed: M }
          MC1 exits
```

**Key invariant**: MC1 stops all state mutations (ShuttingDown) BEFORE sending `PrepareMeetingMigration`. This ensures the state MC2 reads from Redis is final—no race with in-flight MC1 writes.

**Redirect message** (sent by MC1 in Migrating state):
```protobuf
message RedirectToMc {
    string new_mc_endpoint = 1;
    string meeting_id = 2;
    string reason = 3;  // "migration", "rebalance", etc.
}
```

**Why state machine matters**:
- Prevents split-brain: Only MC2 mutates state once in Migrating
- Clear client behavior: Redirect is unambiguous
- Graceful handoff: No lost state mutations during transition

**RequestDrain Contract**:
```protobuf
message RequestDrain {
    string mc_id = 1;
    DrainReason reason = 2;
    google.protobuf.Duration deadline = 3;
}

enum DrainReason {
    DRAIN_REASON_MAINTENANCE = 1;
    DRAIN_REASON_SCALE_DOWN = 2;
    DRAIN_REASON_UNHEALTHY = 3;
    DRAIN_REASON_VERSION_UPGRADE = 4;
}
```

**AZ Distribution**: Topology spread constraints with `maxSkew: 1`

### 8. Health Endpoints

> **Note**: Health endpoints use `/health/...` paths (not `/api/v1/health/...`) as an infrastructure exception per ADR-0004. Like well-known URIs, these are operational endpoints that shouldn't change when API versions bump. Kubernetes probes and load balancers are configured once.

**`/health/live`**: Process alive (Kubernetes liveness probe)
- Tokio runtime responding
- Memory < 90%

**`/health/ready`**: Can accept traffic (Kubernetes readiness probe)
- Redis connection OK
- MH available
- Not draining
- Capacity < 95%

**Degraded State** (still serves, stops accepting new):
- High latency (P99 > 100ms)
- Capacity 80-95%

### 9. Circuit Breaker

**Redis Circuit Breaker**:
- Timeout: 5 seconds
- Fallback: Reject new joins, existing meetings continue with in-memory state

**States**: closed → open → half-open → closed

### 10. Load Shedding

| Capacity % | Action |
|------------|--------|
| 0-85% | Normal operation |
| 85-95% | Reject new meetings, allow joins to existing |
| 95-100% | Proactive migration (see below) |
| 100%+ | Emergency drain |

**Proactive Migration at 95%+ Capacity**:

At 95%+ capacity, rejecting all new connections would block users from joining existing meetings—poor UX. Instead, MC proactively migrates meetings to reduce load.

**Hybrid selection model**: MC selects *what* to migrate (local knowledge), GC selects *where* (global capacity):

```
T=0       MC detects capacity ≥ 95%

T=1s      MC selects migration candidates based on local knowledge:
          - Smallest participant count (less disruption to fewer users)
          - Lowest activity rate (idle meetings, minimal signaling traffic)
          - Longest time since last join/leave (stable, likely winding down)

          MC → GC: McOverloaded {
              mc_id,
              capacity_percent,
              migration_candidates: [
                  { meeting_id, participant_count, messages_per_minute, last_join_ms, priority },
                  ...
              ]
          }

          Activity measurement: MC tracks signaling message rate per meeting.
          Low activity = good migration candidate (less state churn during handoff).

T=2s      GC selects target MCs based on global capacity:
          - Avoids near-full MCs (prevents cascading migrations)
          - Prefers same-region MCs

          GC → MC: MigrateMeetings {
              assignments: [(meeting_1, MC2), (meeting_3, MC4), ...]
          }

T=3-30s   Migration proceeds using same flow as graceful shutdown (Section 7):
          - Meeting state machine: Active → ShuttingDown → Migrating → Complete
          - Clients receive REDIRECT to new MC
          - Only new MC is authoritative during Migrating state

T=30s+    If capacity drops < 90%:
          - Resume normal operation
```

**Why hybrid?**
- MC has real-time per-meeting metrics (CPU, memory, message rate, activity)
- GC has global view (prevents migrating to another nearly-full MC)
- Avoids cascading migrations where MC1 → MC2 → MC3

**Why not just reject?** Users trying to join an existing meeting get a confusing error. Migration is transparent—same meeting, different MC.

### 11. Observability

**Metrics** (`mc_{subsystem}_{metric}_{unit}`):

| Metric | Type | Purpose |
|--------|------|---------|
| `mc_connections_active` | Gauge | Current connections |
| `mc_meetings_active` | Gauge | Current meetings |
| `mc_message_latency_seconds` | Histogram | Processing latency |
| `mc_actor_mailbox_depth{actor_type}` | Gauge | Backpressure indicator |
| `mc_redis_latency_seconds` | Histogram | Cache performance |
| `mc_fenced_out_total{reason}` | Counter | Split-brain events |
| `mc_recovery_duration_seconds` | Histogram | Session recovery time |

**W3C Trace Context Propagation**:

All services must propagate W3C Trace Context headers ([spec](https://www.w3.org/TR/trace-context/)) for end-to-end distributed tracing:

```
traceparent: 00-{trace-id}-{parent-id}-{flags}
             │   │          │           └─ 01 = sampled
             │   │          └─ 16 hex chars (parent span ID)
             │   └─ 32 hex chars (trace ID, same across all services)
             └─ version (always 00)

Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
```

**Propagation flow**:
```
Client → MC: traceparent in WebTransport CONNECT headers (HTTP/3)
MC → GC:     traceparent in gRPC metadata
MC → MH:     traceparent in gRPC metadata
MC → Redis:  trace_id stored in command context (for async correlation)
```

**Connection-level tracing**: WebTransport is a long-lived QUIC connection. The `traceparent` header in the initial HTTP/3 CONNECT request provides a **per-connection** trace ID. This is sufficient for MC since one connection = one participant in one meeting.

**Message correlation within a connection**: To correlate specific signaling messages (e.g., client asks "did you receive my mute request?"), each message includes a sequence number:

```protobuf
message SignalingMessage {
    uint64 seq = 1;           // Client-assigned, monotonically increasing
    oneof payload {
        MuteRequest mute = 2;
        // ...
    }
}
```

Server logs include both: `[trace_id=abc123] seq=42 type=MuteRequest processed`

Client can correlate: "trace_id=abc123, seq=42" → find exact server log entry.

**Implementation** (per ADR-0011):
```rust
// Extract from incoming request
let parent_context = TraceContextPropagator::extract(&headers);

// Create span with parent
let span = tracing::info_span!(
    "mc.session.join",
    trace_id = %parent_context.trace_id(),
);

// Propagate to outgoing request
let mut outgoing_headers = HashMap::new();
TraceContextPropagator::inject(&span.context(), &mut outgoing_headers);
```

**Service updates needed**:

| Service | Current State | Update Required |
|---------|---------------|-----------------|
| AC | Basic `#[instrument]` | Add `tracing-opentelemetry` layer, OTLP exporter |
| GC | Basic `#[instrument]` | Extract from HTTP, propagate to MC gRPC |
| MC | New | Full implementation (this ADR) |
| MH | New | Extract from MC gRPC, correlate media spans |

> **Note**: AC and GC currently have `tracing` instrumentation but not W3C Trace Context propagation. TODO items tracked in `.claude/TODO.md` under "Observability (ADR-0011 Phase 5)".

**Alerting SLOs**:

| SLO | Target | Alert | Page |
|-----|--------|-------|------|
| Availability | 99.9% | < 99.5% 5m | < 99% 2m |
| Join latency P99 | < 500ms | > 1s 5m | > 2s 2m |
| Session continuity | 99.95% | < 99.9% 10m | < 99.5% 5m |

### 12. Runbooks

Location: `docs/runbooks/meeting-controller/`

| Runbook | Trigger |
|---------|---------|
| `mc-high-memory.md` | Memory > 80% |
| `mc-redis-failover.md` | Sentinel failover |
| `mc-split-brain.md` | FENCED_OUT events |
| `mc-high-latency.md` | P99 > 100ms |
| `mc-drain-stuck.md` | Drain timeout |

### 13. Test Infrastructure

**McTestUtils Crate** (`crates/mc-test-utils/`):

```
crates/mc-test-utils/
├── src/
│   ├── mock_gc.rs          # Mock GC for MC testing
│   ├── mock_mh.rs          # Mock MH for MC testing
│   ├── mock_redis.rs       # In-memory Redis mock
│   ├── mock_webtransport.rs # Mock WebTransport client
│   ├── fixtures/           # Pre-configured test data
│   └── assertions/         # State verification helpers
```

## Consequences

### Positive

- **Secure session recovery**: One-time nonce prevents replay attacks
- **Fast failover**: P95 < 15s via client-reported unreachability
- **Split-brain safe**: Fencing tokens prevent stale MC writes
- **Operationally sound**: Health endpoints, circuit breakers, runbooks defined
- **Testable**: Mock infrastructure and clear SLOs

### Negative

- **Redis dependency**: Session state requires Redis availability. Mitigated by Redis Sentinel (3 nodes, quorum 2) which provides automatic failover within ~5s. If Redis is completely unavailable, MC rejects new operations and alerts ops.
- **Complexity**: Fencing token validation adds overhead to all writes
- **Client changes**: Clients must implement unreachability reporting

### Neutral

- **Nonce validation overhead**: Redis `SETNX` ("Set if Not Exists") used for one-time nonce validation—returns success if key didn't exist (nonce valid), failure if it did (replay attempt). ~1ms per operation, but reconnections are rare (only after network blips), so acceptable tradeoff for replay attack prevention.
- **Dual-MH connection cost**: Maintaining backup MH connections has minor overhead (keepalives, connection state). Any MC can use any MH—MHs are not bound to specific MCs. Total active media bandwidth is the same; cost is idle backup connection overhead.

## Implementation Status

### Phase 6a: Foundation (Proto + Skeleton)

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| MC proto messages | ✅ Done | 2026-01-25 | JoinRequest/Response session binding, mute messages |
| Migration proto messages | ❌ Pending | | McDraining, MigrateMeetings, PrepareMeetingMigration (Phase 6e) |
| Load management proto | ❌ Pending | | McOverloaded, RequestDrain (Phase 6f) |
| MH coordination proto | ❌ Pending | | MhLoadNotification, EnforceMute (Phase 6d) |
| Client reporting proto | ❌ Pending | | ReportMcUnreachable (Phase 6g) |
| MC crate skeleton | ✅ Done | 2026-01-25 | config.rs, errors.rs, lib.rs, main.rs |
| McTestUtils crate | ✅ Done | 2026-01-25 | mock_gc, mock_mh, mock_redis, fixtures |

### Phase 6b: Actor Model + Session Management

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| MeetingControllerActor | ✅ Done | 2026-01-25 | Singleton, supervises MeetingActors |
| MeetingActor | ✅ Done | 2026-01-25 | One per meeting, owns state |
| ConnectionActor | ✅ Done | 2026-01-25 | One per WebTransport connection |
| CancellationToken propagation | ✅ Done | 2026-01-25 | Parent→child token hierarchy |
| Mailbox monitoring | ✅ Done | 2026-01-25 | Depth thresholds (Meeting: 100/500, Connection: 50/200) |
| Session binding tokens | ✅ Done | 2026-01-25 | HMAC-SHA256, HKDF key derivation in session.rs (in-memory only) |
| Nonce management | ❌ Pending | | Redis SETNX, TTL handling (requires Phase 6c Redis integration) |
| Reconnection validation | ⚠️ Partial | 2026-01-25 | In-memory validation done; Redis nonce check pending |
| Participant disconnect handling | ⚠️ Partial | 2026-01-25 | 30s grace period logic + tests; Redis state cleanup pending |

### Phase 6c: GC Integration

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| MC registration with GC | ✅ Done | 1175888 | RegisterMc RPC wiring |
| MC heartbeat to GC | ✅ Done | 1175888 | Heartbeat RPC wiring |
| AssignMeeting handling | ✅ Done | ddb6ddc | Accept/reject logic, MH assignment storage, resilience |
| Fencing token validation | ✅ Done | ddb6ddc | Lua script, generation checks |
| OAuth TokenManager integration | ✅ Done | 2026-02-02 | Dynamic token acquisition from AC, replaced MC_SERVICE_TOKEN |
| env-tests for TokenManager startup failures | ❌ Pending | | AC unreachable (timeout), HTTP endpoint rejected (HTTPS enforcement), invalid credentials, token refresh failures |

### Phase 6d: MH Integration

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| MhLoadNotification handling | ❌ Pending | | Proactive MH health updates |
| RequestMhReplacement RPC | ❌ Pending | | When all ranked MHs exhausted |
| EnforceMute to MH | ❌ Pending | | Host-mute enforcement |
| Cross-region MH coordination | ❌ Pending | | Peer MH connections |

### Phase 6e: Graceful Shutdown + Migration

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| McDraining notification | ❌ Pending | | MC → GC on SIGTERM |
| Meeting migration state machine | ❌ Pending | | Active → ShuttingDown → Migrating → Complete |
| PrepareMeetingMigration RPC | ❌ Pending | | MC → MC state handoff |
| RedirectToMc client notification | ❌ Pending | | Client reconnect to new MC |
| McDrained final notification | ❌ Pending | | MC → GC on exit |

### Phase 6f: Resilience

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| Redis circuit breaker | ❌ Pending | | 5s timeout, fallback behavior |
| Load shedding | ❌ Pending | | Capacity-based rejection |
| Proactive migration | ❌ Pending | | McOverloaded → GC at 95%+ |
| Actor panic recovery | ❌ Pending | | JoinHandle monitoring, migration trigger |

### Phase 6g: Signaling + Client Features

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| WebTransport connection handling | ❌ Pending | | QUIC/HTTP3 setup |
| SignalingMessage routing | ❌ Pending | | Sequence numbers, message types |
| Mute state model | ❌ Pending | | Self-mute (informational) vs host-mute (enforced) |
| Client unreachability handling | ❌ Pending | | GC forwards reports, MC validates |

### Phase 6h: Observability

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| Health endpoints | ❌ Pending | | /health, /ready |
| Degraded state handling | ❌ Pending | | High latency, capacity 80-95% |
| Metrics | ❌ Pending | | mc_connections_active, mc_meetings_active, etc. |
| W3C trace context | ❌ Pending | | traceparent propagation |

### Infrastructure

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| Redis Sentinel setup | ❌ Pending | | 3 nodes, quorum 2 |
| AZ distribution | ❌ Pending | | Topology spread constraints |

### Post-Implementation

| Task | Status | Commit | Notes |
|------|--------|--------|-------|
| Runbook: mc-high-memory | ❌ Pending | | Memory > 80% |
| Runbook: mc-redis-failover | ❌ Pending | | Sentinel failover |
| Runbook: mc-split-brain | ❌ Pending | | FENCED_OUT events |
| Runbook: mc-high-latency | ❌ Pending | | P99 > 100ms |
| Runbook: mc-drain-stuck | ❌ Pending | | Drain timeout |
| env-tests for MC | ❌ Pending | | End-to-end with real GC/MC/MH |

## Debate Summary

**Round 1** (72.7% average):
- Key concerns: Binding token replay attacks, split-brain, 35s recovery too slow
- Security: 62/100 (critical gaps in token security)
- Operations: 68/100 (no runbooks, circuit breakers)

**Round 2** (85.4% average):
- Revisions: One-time nonce, fencing tokens, client-reported unreachability, circuit breaker
- Security: 88/100 (+26, major improvement)
- Infrastructure: 78/100 (still lowest, Redis HA unspecified)

**Round 3** (92.75% average):
- Final revisions: All parameter values specified, Redis Sentinel config, runbook skeleton, McTestUtils structure
- All specialists: 92-94/100
- Verdicts: 4 APPROVE, 5 CONDITIONAL APPROVE (minor documentation items)

**Consensus Items** (all agreed):
- Actor model hierarchy
- Server-generated UUIDv7 correlation IDs
- One-time nonce binding tokens
- HKDF key derivation
- Fencing token pattern
- Client-reported unreachability
- Stateless MH routing tables
- Circuit breaker on Redis (5s timeout)
- P95 < 15s recovery target

## Related ADRs

- [ADR-0010: Global Controller Architecture](adr-0010-global-controller-architecture.md) - GC-MC communication
- [ADR-0003: Service Authentication](adr-0003-service-authentication.md) - JWT format for meeting tokens
- [ADR-0001: Actor Pattern](adr-0001-actor-pattern.md) - Actor model foundations
- [ADR-0007: Token Lifetime Strategy](adr-0007-token-lifetime-strategy.md) - Token TTL decisions

## Files to Create/Modify

1. `proto/internal.proto` - Add MC-specific messages (MhLoadReport, RequestDrain, etc.)
2. `crates/meeting-controller/src/` - MC implementation
3. `crates/mc-test-utils/` - Test utilities crate
4. `docs/runbooks/meeting-controller/` - Runbook directory

---

*Generated by multi-agent debate on 2026-01-23*
