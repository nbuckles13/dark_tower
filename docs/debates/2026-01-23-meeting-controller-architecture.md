# Debate: Meeting Controller Architecture

**Date**: 2026-01-23
**Topic**: MC session management, actor model, multi-MH assignment, crash recovery
**Status**: ✅ CONSENSUS REACHED (92.75%)

---

## Debate Questions

### Question 1: WebTransport Session Architecture with Recovery

**Key design requirement**: Sessions must be **recoverable** after connection drops.

**Correlation ID Pattern**:
```
First connection:
  Client → MC: JoinRequest { correlation_id: <random>, join_token: <JWT> }

Reconnection:
  Client → MC: JoinRequest { correlation_id: <same>, join_token: <new JWT> }
  MC recovers: participant state, mute status, layout preferences
```

**Sub-questions**:
1. Correlation ID security - prevent session hijacking if stolen
2. MH connection recovery - when does participant need new MH connection?
3. Actor model design - supervision, inter-actor communication, participant-meeting relationship

### Question 2: Participant Session State

What state per participant and where to store it?

### Question 3: Media Handler Assignment

- MC maintains local MH registry via heartbeats
- Multi-MH per participant (2 MHs in different datacenters)
- **MC decides** which MH is primary
- Phase 1 focus: redundancy and failover

### Question 7: State Persistence and Crash Recovery

Sunny-day flow:
1. MC writes state to Redis on changes (minimal writes)
2. MC crashes
3. Clients/MHs detect disconnect, media continues via MH
4. Client reconnects via GC → assigned to new MC
5. New MC recovers state from Redis
6. All clients reconnect with correlation_id

---

## Round 1

### Meeting Controller Specialist - Initial Proposal

**Self-Assessment: 78/100**

---

#### 1. Executive Summary

Key design decisions:

1. **Actor-per-connection model with hierarchical supervision**: Each WebTransport connection spawns a dedicated `ConnectionActor` managed by a `MeetingActor` supervisor. Tokio tasks with panic-catching wrappers provide supervision.

2. **Correlation ID + binding token for secure session recovery**: Correlation IDs alone are not secure. On first join, MC issues a short-lived binding token (HMAC of correlation_id + participant_id + timestamp). Reconnection requires valid JWT + matching binding token.

3. **Dual-MH assignment with MC-controlled failover**: Each participant gets primary and secondary MH assignments in different availability zones. MC monitors MH health and triggers failover, client never decides.

4. **Minimal Redis writes with write-through cache**: Hot state (mute, layout) lives in memory with async Redis persistence. Only session establishment and changes to durable state trigger Redis writes.

5. **MH connection recovery based on transport continuity**: MC tracks whether media transport was disrupted alongside signaling. Network change = full recovery. Signaling-only disruption = signaling recovery without MH reconnection.

---

#### 2. Detailed Design

##### 2.1 Correlation ID Security - Binding Token Pattern

**Problem**: Raw correlation_id can be stolen (browser extension, network sniff). Attacker could hijack session.

**Solution**: **Binding Token Pattern**

```rust
struct SessionBinding {
    correlation_id: String,       // Client-generated UUID
    participant_id: String,       // MC-assigned on first join
    binding_token: String,        // HMAC(correlation_id || participant_id || created_at)
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,    // 24 hours after meeting start
}
```

**First Connection Flow**:
```
Client → MC: JoinRequest {
    correlation_id: <client_generated_uuid>,
    join_token: <JWT from GC>,
    binding_token: None  // First join
}

MC validates JWT, extracts user_id from token
MC generates participant_id
MC computes binding_token = HMAC-SHA256(
    key: per-meeting-secret,
    data: correlation_id || participant_id || now()
)
MC stores binding in Redis with TTL
MC → Client: JoinResponse {
    participant_id,
    binding_token,  // Client must store securely
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
1. JWT signature and claims (fresh token prevents replay)
2. user_id in JWT matches user_id from original binding
3. HMAC verification: recompute binding_token, compare
4. binding not expired

If all pass → session recovered
If binding_token missing/invalid → new session (no state recovery)
```

**Why this is secure**:
- Stolen correlation_id alone is useless (attacker needs binding_token)
- Stolen binding_token requires valid JWT for same user_id
- JWT theft requires both JWT and binding_token
- Fresh JWT required on reconnect (prevents replay of old tokens)

##### 2.2 MH Connection Recovery Decision

**Decision Matrix**:

| Scenario | MC Reconnect | MH Reconnect | Detection Method |
|----------|--------------|--------------|------------------|
| WiFi → 5G (network change) | YES | YES | Client sends `transport_continuity: false` |
| MC pod restart | YES | NO | Client sends `transport_continuity: true`, MH still healthy |
| MH failure | YES | YES | MC detects MH unhealthy, tells client to reconnect |
| Signaling path disruption | YES | NO | Client sends `transport_continuity: true` |

**Protocol Extension**:
```protobuf
message JoinRequest {
    // ... existing fields ...
    string correlation_id = 5;
    optional string binding_token = 6;
    optional bool transport_continuity = 7;  // true = media stream still alive
    optional string last_mh_connection_id = 8;
}
```

##### 2.3 Actor Model Design

**Hierarchy**:
```
MeetingControllerActor (singleton per MC instance)
├── supervises N MeetingActors
│   └── MeetingActor (one per active meeting)
│       ├── owns meeting state
│       ├── supervises N ConnectionActors
│       │   └── ConnectionActor (one per WebTransport connection)
│       └── InterMeetingBridge (for cross-MC coordination)
└── MhRegistryActor (tracks MH health via heartbeats)
```

**Supervision with Tokio Tasks**:
- Spawn with `catch_unwind` for panic handling
- Supervisor channel to MeetingActor for lifecycle events
- Clean vs error vs panic exits handled differently

**Inter-Actor Communication**:
- Route through MeetingActor for isolation
- Message types: ClientMessage, ParticipantJoined/Left, LayoutUpdate, MediaRouteRequest

##### 2.4 Participant Session State

| State | Location | Persistence |
|-------|----------|-------------|
| Connection state | Memory only | None |
| Participant identity | Redis | Durable |
| Mute status | Memory + Redis | Async write |
| Layout preferences | Memory + Redis | Async write |
| Published streams | Memory + Redis | Sync write |
| MH assignments | Redis | Durable |
| Subscriptions | Memory + Redis | Async write |

**Write-Through Cache**: Batch async writes every 100ms or 50 items, deduplicate per participant.

##### 2.5 Media Handler Assignment

**MH Registration Flow**:
- MH → MC: RegisterMh gRPC with endpoints, region, AZ, capacity
- MH sends heartbeat every 5s
- MC maintains MhRegistry with health status

**Selection Algorithm**: Weighted selection with AZ diversity
- Score by 70% load + 30% latency
- Primary: best score
- Secondary: best score in DIFFERENT AZ

**Failover** (MC decides, never client):
- On MH unhealthy: promote secondary, assign new secondary
- Notify client with FailoverNotification message

##### 2.6 Crash Recovery Flow

```
T=0      MC crashes (state in Redis)
T=50ms   Clients detect disconnect, media continues via MH
T=30s    GC heartbeat timeout, marks MC unhealthy
T=35s    Client reconnects via GC → assigned to new MC
T=35.5s  New MC loads state from Redis
T=36s    Client sends JoinRequest with binding_token
T=36.5s  Session recovered, meeting continues
```

**Timeout Values**:
| Event | Timeout |
|-------|---------|
| Client reconnect window | 60 seconds |
| Binding token validity | 24 hours |
| Redis state TTL | Meeting duration + 1 hour |
| GC detects MC unhealthy | 30 seconds |
| Client reconnect backoff | 1s, 2s, 4s, 8s, 16s max |

---

#### 3. Redis Schema

```
meeting:{meeting_id}                     → MeetingState (JSON)
meeting:{meeting_id}:participants        → SET of participant_ids
meeting:{meeting_id}:participant:{pid}   → ParticipantState (JSON)
session:binding:{correlation_id}         → SessionBinding (JSON)
meeting:{meeting_id}:mh_assignments      → HASH { participant_id → MhAssignment }
meeting:{meeting_id}:lock                → MC instance ID (NX + 30s TTL)
```

---

#### 4. Open Questions for Other Specialists

**For Security Specialist**:
1. Is HMAC-SHA256 sufficient for binding tokens, or should we use signed JWT?
2. How should we generate and store the per-meeting HMAC key?
3. Should we mandate UUIDv4 for correlation ID, or allow client-chosen IDs?

**For Infrastructure Specialist**:
1. Redis Cluster vs Sentinel for HA?
2. MH discovery mechanism (K8s service vs env var)?
3. Graceful shutdown drain time (currently 30s)?

**For Database Specialist**:
1. Should any MC state go to PostgreSQL for stronger durability?
2. Concern about async writes causing Redis load - throttling needed?

**For Test Specialist**:
1. What failure modes to test (MC crash, Redis crash, MH crash)?
2. Recovery time SLO target (currently ~35s)?

**For Operations Specialist**:
1. Key metrics for MC observability?
2. Alerting thresholds for participant count / latency?

**For MH Specialist**:
1. Is MH comfortable validating connection tokens via JWKS (not calling MC)?
2. Should MH inform MC when it detects client disconnect?

**For GC Specialist**:
1. Is 30s heartbeat timeout acceptable before re-routing?
2. How does GC handle concurrent joins during MC failover?

---

#### 5. Concerns Identified

- Binding token adds complexity to reconnection flow
- `transport_continuity` flag requires client implementation trust
- Async Redis writes could lose 100ms of state on crash
- No explicit handling of split-brain (two MCs think they own same meeting)
- MH heartbeat interval (5s) may be too aggressive for large deployments

---

### Domain Specialist Reactions

#### Media Handler Specialist - 78/100

**What they like**:
- JWKS-based token validation (no MC call in hot path)
- MC controls failover, not client
- MH continues forwarding during MC crash

**Concerns**:
- Primary/secondary semantics underspecified - MH shouldn't track role state
- 5s heartbeat may be too aggressive for 1000+ MH instances
- `transport_continuity` flag might be misleading if MH restarted

**Recommendations**:
1. Eliminate primary/secondary terminology for MH - use stateless routing tables only
2. Increase steady-state heartbeat to 15s (5s only for first minute)
3. Add optional MH→MC disconnect notification for faster failover
4. Define load reporting format (cpu, bandwidth, streams, pps)

---

#### Global Controller Specialist - 78/100

**What they like**:
- Actor model hierarchy is clean
- Session binding token preserves GC statelessness
- State recovery from Redis is architecturally correct

**Concerns**:
- Failover coordination gap - how does new MC know to load state?
- Binding token scope ambiguity (MC-scoped vs meeting-scoped)
- Concurrent joins race condition before GC detects unhealthy MC

**Recommendations**:
1. Add `PrepareMeetingMigration` gRPC for explicit handoff
2. Make binding tokens meeting-scoped and Redis-validated
3. Add client-reported unreachability endpoint for faster failover
4. Define `RequestDrain` protocol for graceful MC shutdown

---

#### Database Specialist - 72/100

**What they like**:
- Clear state classification (ephemeral vs durable)
- Appropriate Redis data structures
- Sound TTL strategy

**Concerns**:
- 100ms async write window too wide for critical state
- Redis key structure has scan problems for bulk reads
- Session bindings have no PostgreSQL backup (single point of failure)
- Split-brain lock is insufficient (no fencing token)

**Recommendations**:
1. Sync write for published streams and MH assignments
2. Use HASH instead of separate keys: `meeting:{id}:participants:state`
3. Add PostgreSQL backup for session bindings (async write)
4. Implement fencing token pattern for split-brain prevention
5. Add event log in PostgreSQL for audit trail

---

#### Infrastructure Specialist - 72/100

**What they like**:
- Actor model for meeting state isolation
- Redis-only (no PostgreSQL for MC)
- Dual-MH with AZ diversity

**Concerns**:
- Redis HA strategy unspecified
- MH discovery mechanism unclear
- Graceful shutdown not defined
- AZ distribution strategy missing

**Recommendations**:
1. Use Redis Sentinel (not Cluster) for HA
2. Use Kubernetes Service DNS for MH discovery
3. 60-second graceful shutdown with DRAINING health status
4. Use pod anti-affinity + topology spread constraints for AZ distribution
5. Add Network Policy and health probes

---

### Cross-Cutting Specialist Reactions

#### Security Specialist - 62/100

**Security gaps identified**:
- Missing nonce/challenge-response in binding token (replay attack possible)
- No binding token expiration enforcement specified
- HMAC key lifecycle undefined
- Transport continuity flag trust violates zero-trust
- Split-brain lock has no fencing token

**Threat analysis**:
1. Session hijacking via binding token replay
2. Correlation ID collision attack (if client-chosen)
3. Split-brain participant duplication
4. HMAC key compromise (Redis at rest)
5. Timing attack on HMAC verification

**Critical recommendations**:
1. Add fencing tokens to Redis locks
2. Server-generated correlation IDs (UUIDv7)
3. One-time binding tokens with nonce (changes on each reconnection)
4. Use `subtle::ConstantTimeEq` for HMAC comparison
5. **Delete transport continuity flag** - never trust client

**Key derivation recommendation**:
```rust
meeting_key = HKDF-SHA256(
    ikm: master_secret,
    salt: meeting_id,
    info: b"session-binding"
)
```

---

#### Test Specialist - 74/100

**Testability assessment**: Actor model is testable, but failure modes underspecified

**Recovery SLO concern**: 35s is too slow
- Recommended: P50 < 5s, P95 < 15s, P99 < 30s
- 30s GC heartbeat timeout dominates - need client-reported unreachability

**Failure modes to test (P0)**:
- MC crash with active meetings
- Binding token validation (all attack vectors)
- MH failover (primary → secondary)
- Actor panic recovery

**Recommendations**:
1. Create McTestUtils crate (mock clients, panic injection)
2. Sync write for published_streams and mh_assignments
3. Add explicit split-brain chaos tests
4. Reduce GC heartbeat timeout or add client-reported unreachability

---

#### Observability Specialist - 72/100

**Metrics recommendations** (following `{service}_{subsystem}_{metric}_{unit}` convention):
- `mc_signaling_join_duration_seconds` - SLO: p99 < 100ms
- `mc_sessions_recovery_duration_seconds` - SLO: p95 < 5s
- `mc_mh_failover_duration_seconds` - SLO: p99 < 3s
- `mc_actors_mailbox_depth` - backpressure indicator

**SLO recommendations**:
| SLO | Objective |
|-----|-----------|
| Signaling Availability | 99.9% |
| Signaling Latency (p99) | < 100ms |
| Session Recovery Success | 99.5% |
| MH Failover Latency (p99) | < 3s |

**Tracing recommendation**: Add `trace_context` field to signaling protobuf for W3C Trace Context propagation across MC-MH-Client boundaries.

---

#### Operations Specialist - 68/100

**Operational readiness gaps**:
- No runbook for MC crash recovery
- No circuit breaker on MC→Redis path
- No load shedding strategy
- No cost estimate for dual-MH

**Required runbooks**:
1. MC Instance Crash (P2)
2. MC Deployment Rollback
3. Redis State Recovery Failure
4. Reconnection Storm

**Alerting thresholds**:
- Participant count: 80% capacity → P3, 95% → P2
- Latency: p99 > 100ms for 5m → P3, > 200ms → P2, > 500ms → P1

**Cost concern**: Dual-MH could be ~2x compute cost. Recommend making it configurable per meeting type.

**Blocking requirements**:
1. Add timeout on Redis state load with fallback
2. Define circuit breaker on MC→Redis path
3. Document client reconnection rate limiting at GC
4. Specify MC health endpoint contract

---

## Round 1 Summary

### Satisfaction Scores

| Specialist | Score | Key Concern |
|------------|-------|-------------|
| Meeting Controller | 78 | Self-identified: split-brain, async write loss |
| Media Handler | 78 | Primary/secondary semantics, heartbeat interval |
| Global Controller | 78 | Failover coordination gap, binding token scope |
| Database | 72 | 100ms data loss, split-brain lock insufficient |
| Infrastructure | 72 | Redis HA, service discovery, graceful shutdown |
| Security | 62 | **Binding token security gaps, fencing tokens** |
| Test | 74 | 35s recovery too slow, chaos testing needed |
| Observability | 72 | Missing metrics/SLOs, trace propagation |
| Operations | 68 | **No runbooks, circuit breakers, cost analysis** |

**Average Score: 72.7/100** (below 90% consensus target)

### Critical Issues to Address in Round 2

1. **Security (BLOCKER)**: Fencing tokens, one-time binding tokens with nonce, server-generated correlation IDs
2. **Operations (BLOCKER)**: Runbooks, circuit breakers, Redis timeouts
3. **Recovery Time**: Reduce from 35s to <15s P95
4. **Split-Brain**: Fencing token pattern required
5. **MH Semantics**: Clarify stateless routing vs primary/secondary

### Consensus Items

All specialists agree on:
- Actor model is appropriate
- Redis-only state (no PostgreSQL for MC)
- MC controls MH failover (not client)
- JWKS validation at MH (no MC call)
- Dual-MH with AZ diversity

---

## Round 2

### Meeting Controller Specialist - Revised Proposal

**Self-Assessment: 88/100** (up from 78/100)

#### Key Changes from Round 1

| Round 1 | Round 2 |
|---------|---------|
| Client-generated correlation ID | **Server-generated UUIDv7** |
| Static binding token | **One-time nonce, rotates on reconnect** |
| Simple HMAC key | **HKDF derivation from master secret** |
| `transport_continuity` flag | **DELETED** |
| Timing-vulnerable comparison | **`subtle::ConstantTimeEq`** |
| Simple Redis lock | **Fencing token with Lua validation** |
| 35s recovery time | **P95 < 15s with client-reported unreachability** |
| Primary/secondary MH | **Stateless routing tables** |
| No circuit breaker | **Circuit breaker on Redis path** |
| No health specification | **`/health/live` + `/health/ready` contracts** |

#### 1. Revised Binding Token Design

- Server-generated UUIDv7 correlation ID
- One-time nonce that rotates on each successful reconnection
- HKDF key derivation: `meeting_key = HKDF(master_secret, meeting_id, "session-binding")`
- Constant-time HMAC verification via `subtle::ConstantTimeEq`
- Previous nonce grace window for in-flight reconnects

#### 2. Fencing Token for Split-Brain

Lua script validates fencing token on every Redis write:
```lua
-- If current_gen > expected_gen, reject as FENCED_OUT
-- If current_gen == expected_gen but different MC, reject as SPLIT_BRAIN
```

All state writes go through fencing validation.

#### 3. Faster Recovery (P95 < 15s)

- New GC endpoint: `ReportMcUnreachable` for client-reported unreachability
- Quorum: 3+ clients reporting triggers immediate failover
- Revised timeline: T=0 crash → T=10s recovered (vs 35s before)
- Circuit breaker on Redis with 5s timeout and fallback state

#### 4. Stateless MH Model

- Eliminated primary/secondary terminology
- MH receives routing tables from MC
- On failover: MC updates routing tables, MH doesn't track roles
- Both assigned MHs are equivalent, just in different AZs

#### 5. Health Endpoint Specification

**`/health/live`**: Process alive (tokio runtime, memory < 90%)
**`/health/ready`**: Can accept traffic (Redis ok, MH available, not draining, < 95% capacity)

Returns `degraded` (still serves) for high latency or 80-95% capacity.

---

### Round 2 Domain Specialist Reactions

#### Media Handler Specialist - 89/100 (up from 78/100)

**Satisfied with**:
- Stateless routing tables model (eliminates role tracking complexity)
- Eliminated primary/secondary terminology
- Routing table updates from MC

**Remaining concerns**:
- Load reporting format still needs definition (cpu%, bandwidth Mbps, active streams, packets/sec)
- Heartbeat adaptive interval unclear (15s steady-state mentioned in Round 1 but not in revised proposal)

**Recommendation**: Define `MhLoadReport` protobuf message with concrete fields.

---

#### Global Controller Specialist - 87/100 (up from 78/100)

**Satisfied with**:
- Client-reported unreachability (`ReportMcUnreachable`) addresses failover speed
- Quorum-based failover (3+ clients) prevents false positives
- Server-generated correlation ID simplifies GC statelessness

**Remaining concerns**:
- GC-MC contract for `ReportMcUnreachable` needs specification
- What happens if < 3 clients in meeting? Fallback to heartbeat?
- Need explicit `RequestDrain` contract for graceful shutdown

**Recommendation**: Add protocol section defining GC-MC contracts for unreachability reporting and drain.

---

#### Database Specialist - 82/100 (up from 72/100)

**Satisfied with**:
- Fencing token pattern with Lua script validation
- HKDF key derivation (proper cryptographic practice)
- Circuit breaker on Redis path

**Remaining concerns**:
- Async writes for mute/subscriptions still have data loss window
- No PostgreSQL backup for session bindings (Redis is still SPOF for identity)
- Event log for audit trail not addressed

**Recommendation**: Consider sync writes for participant identity changes; async acceptable for preferences.

---

#### Infrastructure Specialist - 78/100 (up from 72/100)

**Satisfied with**:
- Health endpoint specification (`/health/live`, `/health/ready`, `degraded` state)
- Circuit breaker on Redis path with 5s timeout
- Capacity-based readiness (< 95%)

**Remaining concerns**:
- Redis HA strategy still unspecified (Sentinel recommendation from Round 1)
- MH discovery mechanism not addressed
- Graceful shutdown drain procedure not detailed
- AZ distribution strategy for MC pods missing

**Recommendation**: Add infrastructure section covering Redis HA, service discovery, and drain procedure.

---

### Round 2 Cross-Cutting Specialist Reactions

#### Security Specialist - 88/100 (up from 62/100)

**Resolved concerns**:
- Replay attack prevention via one-time nonce rotation
- Zero-trust compliance (deleted `transport_continuity` flag)
- Proper key management with HKDF derivation
- Timing attack protection with `subtle::ConstantTimeEq`
- Split-brain safety with fencing token pattern

**Remaining concerns**:
- Binding token expiration TTL not explicitly specified (recommend 30s max)
- Master secret rotation schedule undefined (recommend 24h with 2h grace)
- Previous nonce grace window duration unquantified (recommend 5s max)

**New concerns**:
- UUIDv7 embeds timestamp (minor info leakage)
- Nonce storage needs atomic operations to prevent race conditions

**Recommendation**: Specify TTL, rotation schedule, and grace window parameters.

---

#### Test Specialist - 88/100 (up from 74/100)

**Resolved concerns**:
- Recovery timeline now P95 < 15s (acceptable)
- Split-brain fencing token is testable
- Circuit breaker has defined 5s timeout

**Remaining concerns**:
- Quorum edge cases need specification (what if < 3 clients?)
- Fencing token overflow behavior unspecified
- McTestUtils crate still needed (FakeMc, MaliciousMc, PartitionedMc)

**New test requirements**:
- `ReportMcUnreachable`: rate limiting, deduplication, validation
- Quorum-based failover: threshold tests, timing, false positive resistance
- Fencing token Lua script: atomicity, rejection, Redis failure
- Health endpoints: degraded state transitions, cascading checks

**Recommendation**: Add McTestUtils to architecture, specify quorum formula, document degraded state behavior.

---

#### Observability Specialist - 85/100 (up from 72/100)

**Resolved concerns**:
- Health endpoints provide service health visibility
- P95 < 15s recovery is a measurable SLO
- Circuit breaker timeout gives latency boundaries

**Remaining concerns**:
- Metrics specification incomplete (how to measure recovery time?)
- Trace context propagation undefined for new paths
- FENCED_OUT event monitoring not specified
- Actor mailbox depth still not mentioned

**Required metrics/traces**:
| Feature | Metric |
|---------|--------|
| Health | `mc_health_status{state}` gauge |
| Recovery | `mc_recovery_duration_seconds` histogram |
| Circuit breaker | `mc_redis_circuit_breaker_state{state}` gauge |
| Fencing | `mc_fencing_token_rejected_total` counter |

**Recommendation**: Define metric naming convention, specify trace context headers, add structured logging requirements.

---

#### Operations Specialist - 84/100 (up from 68/100)

**Resolved concerns**:
- Circuit breaker addresses cascading Redis failures
- Health endpoint contract enables proper K8s probes
- P95 < 15s recovery is measurable SLO
- Client-reported unreachability catches network partitions

**Remaining concerns**:
- No runbook documentation yet
- Dual-MH cost estimate needed for capacity planning
- Load shedding behavior undefined (what happens at 95% capacity?)
- Alerting thresholds not specified

**New requirements**:
- Fencing token mismatch should be P1 alert (split-brain indicator)
- Circuit breaker state needs Prometheus metrics
- Client unreachability quorum threshold needs definition

**Recommendation**: Document runbook skeleton, define load shedding behavior, specify alerting SLOs.

---

## Round 2 Summary

### Satisfaction Scores

| Specialist | Round 1 | Round 2 | Change | Key Remaining Concern |
|------------|---------|---------|--------|----------------------|
| Meeting Controller | 78 | 88 | +10 | (Proposer) |
| Media Handler | 78 | 89 | +11 | Load reporting format |
| Global Controller | 78 | 87 | +9 | GC-MC contract specification |
| Database | 72 | 82 | +10 | Async write data loss window |
| Infrastructure | 72 | 78 | +6 | Redis HA, discovery, drain |
| Security | 62 | 88 | +26 | TTL/rotation parameters |
| Test | 74 | 88 | +14 | McTestUtils, quorum formula |
| Observability | 72 | 85 | +13 | Metrics specification |
| Operations | 68 | 84 | +16 | Runbooks, load shedding |

**Average Score: 85.4/100** (up from 72.7/100, +12.7)

### Progress Toward Consensus

| Threshold | Status |
|-----------|--------|
| 90% consensus | NOT REACHED (85.4%) |
| All scores ≥ 80 | NOT MET (Infra: 78) |
| No blockers | ACHIEVED (no scores < 70) |

### Items for Round 3 (if needed)

**Specification Gaps** (can be resolved with documentation):
1. Binding token TTL (30s), master secret rotation (24h), nonce grace window (5s)
2. Quorum formula for client-reported unreachability
3. Load shedding behavior at 95% capacity
4. Metric names and trace context propagation
5. Runbook skeleton

**Architectural Gaps** (may need design discussion):
1. Redis HA strategy (Sentinel recommendation pending acceptance)
2. MH discovery mechanism
3. McTestUtils crate inclusion in architecture
4. GC-MC contract for `ReportMcUnreachable` and `RequestDrain`

### Consensus Items (All Agree)

- Actor model with MeetingControllerActor → MeetingActor → ConnectionActor
- Server-generated UUIDv7 correlation IDs
- One-time nonce rotation for binding tokens
- HKDF key derivation for meeting-scoped secrets
- Fencing token pattern with Lua validation
- Client-reported unreachability for faster failover
- Stateless MH routing tables (no primary/secondary)
- Health endpoints with degraded state
- Circuit breaker on Redis path with 5s timeout
- P95 < 15s recovery target

---

## Round 3

### Meeting Controller Specialist - Comprehensive Revision

**Self-Assessment: 94/100** (up from 88/100)

This revision addresses ALL remaining specification gaps with concrete values, protobuf definitions, and implementation details.

---

#### 1. Infrastructure Specifications

**Redis HA**: Redis Sentinel with 3-node quorum
- `down-after-milliseconds`: 5000
- `failover-timeout`: 30000
- Client uses Sentinel-aware connection with exponential backoff

**MH Discovery**: Kubernetes headless Service with DNS SRV lookup
- Service: `media-handler-headless`
- Refresh interval: 10s with jitter

**Graceful Shutdown Drain** (60s total):
1. Set `accepting_new = false`
2. Notify GC: `McDraining`
3. Wait for meetings (30s max)
4. Send `MeetingMigrating` to clients
5. Flush Redis, close connections
6. Notify GC: `McDrained`

**AZ Distribution**: Topology spread constraints with `maxSkew: 1`

---

#### 2. Database Specifications

**Hybrid Write Strategy**:
- **Sync** (with `WAIT` for replica): Session binding, participant roster, MH assignments
- **Async**: Mute status, subscriptions, ephemeral metrics

**PostgreSQL Backup Table**:
```sql
CREATE TABLE session_bindings (
    meeting_id UUID PRIMARY KEY,
    mc_id UUID NOT NULL,
    last_heartbeat TIMESTAMPTZ NOT NULL
);
```
- Write: Async after Redis (best-effort backup)
- Read: Fallback if Redis unavailable

---

#### 3. Operations Specifications

**Runbook Skeleton** (`docs/runbooks/meeting-controller/`):
- `mc-high-memory.md`, `mc-redis-failover.md`, `mc-split-brain.md`, `mc-high-latency.md`, `mc-drain-stuck.md`

**Load Shedding**:
| Capacity | Action |
|----------|--------|
| 0-85% | Normal |
| 85-95% | Reject new meetings, allow joins |
| 95-100% | Reject all new connections |
| 100%+ | Emergency drain |

**Alerting SLOs**:
| SLO | Target | Alert | Page |
|-----|--------|-------|------|
| Availability | 99.9% | < 99.5% 5m | < 99% 2m |
| Join latency P99 | < 500ms | > 1s 5m | > 2s 2m |
| Session continuity | 99.95% | < 99.9% 10m | < 99.5% 5m |

**Dual-MH Cost**: ~$5/MC/month ($500/month at 100 MCs)

---

#### 4. Observability Specifications

**Metrics** (`mc_{subsystem}_{metric}_{unit}`):
- `mc_connections_active`, `mc_meetings_active`
- `mc_message_latency_seconds`, `mc_actor_mailbox_depth{actor_type}`
- `mc_redis_latency_seconds`, `mc_fenced_out_total{reason}`

**W3C Trace Context**: `traceparent` header propagated across MC→GC, MC→MH, Client→MC

**FENCED_OUT Monitoring**: Counter + WARN log with meeting_id, epoch values, affected participants

**Mailbox Thresholds**:
| Actor | Normal | Warning | Critical |
|-------|--------|---------|----------|
| Meeting | < 100 | 100-500 | > 500 |
| Participant | < 50 | 50-200 | > 200 |

---

#### 5. GC-MC Contracts

**ReportMcUnreachable** protobuf:
```protobuf
message ReportMcUnreachable {
  string mc_id = 1;
  UnreachableReason reason = 2;
  uint32 affected_meetings = 3;
}
```

**Quorum Formula**: `max(1, floor(n/2) + 1)`
- 1 participant: 1 (single source of truth)
- 2 participants: 2 (timestamp tiebreaker)
- 3+ participants: majority

**RequestDrain** protobuf:
```protobuf
message RequestDrain {
  string mc_id = 1;
  DrainReason reason = 2;
  Duration deadline = 3;
}
```

---

#### 6. Security Specifications

| Parameter | Value |
|-----------|-------|
| Binding token TTL | **30 seconds** |
| Clock skew allowance | 5 seconds |
| Master secret rotation | **24 hours** |
| Rotation grace period | **2 hours** |
| Nonce grace window | **5 seconds** |
| Nonce storage TTL | 35 seconds |

**Atomic Nonce**: Redis `SETNX` with TTL in single operation

---

#### 7. Test Specifications

**McTestUtils Crate** (`crates/mc-test-utils/`):
- `mock_gc.rs`, `mock_mh.rs`, `mock_redis.rs`, `mock_webtransport.rs`
- `fixtures/`: meetings, participants, tokens
- `assertions/`: meeting state, protocol messages

**Degraded State Behavior**:
| State | Detection | Client Experience | Recovery |
|-------|-----------|-------------------|----------|
| Redis unavailable | Connection error | Existing meetings continue, no new joins | Auto-reconnect |
| GC unreachable | gRPC timeout | New meetings fail | Report via peer MCs |
| MH unreachable | Assignment failure | Failover to backup MH | Automatic |

---

#### 8. MH Specifications

**MhLoadReport** protobuf:
```protobuf
message MhLoadReport {
  string mh_id = 1;
  uint32 current_streams = 2;
  uint32 max_streams = 3;
  uint32 cpu_percent = 4;
  uint32 bandwidth_ingress_percent = 5;
  uint32 packet_loss_permille = 6;
  MhHealthStatus health_status = 7;
}
```

**Selection Algorithm**: Score = health × (1 - utilization × 0.5) × geo_bonus

---

### Round 3 Specialist Reactions

#### Media Handler Specialist - 94/100 (up from 89/100)

**Resolved**: Load report format with all essential metrics (streams, CPU, bandwidth, packet loss, health status)

**Remaining concerns**:
- Missing egress bandwidth percentage (SFUs are egress-constrained)
- Health status computation thresholds not defined
- Per-codec capacity not captured

**Final verdict**: **CONDITIONAL APPROVE** - Add `bandwidth_egress_percent` to MhLoadReport

---

#### Global Controller Specialist - 94/100 (up from 87/100)

**Resolved**: ReportMcUnreachable contract, quorum formula with fallbacks, RequestDrain contract

**Remaining concerns**: None - all Round 2 issues addressed

**Final verdict**: **APPROVE** - Protocols integrate cleanly with GC's existing infrastructure

---

#### Database Specialist - 92/100 (up from 82/100)

**Resolved**: Hybrid write strategy (sync for critical), PostgreSQL backup table

**Remaining concerns**:
- Session binding table should use composite key `(meeting_id, participant_id)`
- Redis→Postgres reconciliation strategy needs documentation
- Backup table cleanup job (hourly, delete older than 25 hours)

**Final verdict**: **CONDITIONAL APPROVE** - Implementation details, not architectural changes

---

#### Infrastructure Specialist - 92/100 (up from 78/100)

**Resolved**: Redis Sentinel config, K8s Service DNS discovery, drain procedure, AZ topology

**Remaining concerns** (documentation, non-blocking):
- Redis persistence strategy (RDB/AOF)
- Sentinel deployment topology
- MH discovery edge cases
- Circuit breaker + Sentinel failover interaction

**Final verdict**: **APPROVE** - Infrastructure design is operationally complete

---

#### Security Specialist - 94/100 (up from 88/100)

**Resolved**: 30s token TTL, 24h rotation with 2h grace, 5s nonce window, atomic SETNX

**Remaining concerns** (documentation):
- Emergency master secret rotation procedure
- Nonce update failure recovery strategy
- UUIDv7 timestamp leakage (minor, acceptable)

**Final verdict**: **APPROVE** - Parameters proven in OAuth 2.0 and AC's JWT implementation

---

#### Test Specialist - 92/100 (up from 88/100)

**Resolved**: Quorum formula with edge cases, McTestUtils crate structure, degraded state behavior

**Remaining concerns** (minor specifications):
- Explicit timeout values for degraded state recovery
- Fencing generation type (recommend uint64)
- Nonce grace window is per-correlation_id (clarify)

**Final verdict**: **CONDITIONAL APPROVE** - Architecture is testable, minor clarifications needed

---

#### Observability Specialist - 92/100 (up from 85/100)

**Resolved**: Metrics naming convention, W3C trace context, FENCED_OUT monitoring, mailbox thresholds

**Remaining concerns**:
- Explicit SLO targets needed (availability, latency, error budget)
- Trace sampling strategy undefined
- Alert firing logic for mailbox thresholds

**Final verdict**: **CONDITIONAL APPROVAL** - Foundation solid, ops details for implementation phase

---

#### Operations Specialist - 92/100 (up from 84/100)

**Resolved**: Runbook skeleton, load shedding tiers, alerting SLOs, dual-MH cost estimate

**Remaining concerns**:
- Drain timeout may need increase to 45s for long streams
- FENCED_OUT alerting threshold (recommend > 5 in 5m = P2)
- Circuit breaker fallback behavior needs clarification

**Final verdict**: **CONDITIONAL APPROVE** - Specification refinements, not blockers

---

## Round 3 Summary

### Satisfaction Scores

| Specialist | Round 1 | Round 2 | Round 3 | Change | Verdict |
|------------|---------|---------|---------|--------|---------|
| Meeting Controller | 78 | 88 | 94 | +16 | (Proposer) |
| Media Handler | 78 | 89 | 94 | +16 | CONDITIONAL |
| Global Controller | 78 | 87 | 94 | +16 | APPROVE |
| Database | 72 | 82 | 92 | +20 | CONDITIONAL |
| Infrastructure | 72 | 78 | 92 | +20 | APPROVE |
| Security | 62 | 88 | 94 | +32 | APPROVE |
| Test | 74 | 88 | 92 | +18 | CONDITIONAL |
| Observability | 72 | 85 | 92 | +20 | CONDITIONAL |
| Operations | 68 | 84 | 92 | +24 | CONDITIONAL |

**Average Score: 92.75/100** ✅ **CONSENSUS REACHED** (target: 90%)

### Consensus Status

| Threshold | Status |
|-----------|--------|
| 90% consensus | ✅ **ACHIEVED** (92.75%) |
| All scores ≥ 80 | ✅ **MET** (lowest: 92) |
| All scores ≥ 90 | ✅ **MET** (all 92-94) |
| No blockers | ✅ **ACHIEVED** |

### Approval Summary

| Verdict | Count | Specialists |
|---------|-------|-------------|
| APPROVE | 4 | GC, Infra, Security, MC (proposer) |
| CONDITIONAL APPROVE | 5 | MH, DB, Test, Observability, Operations |

### Conditions for Final ADR (All Minor)

**Implementation Details** (non-blocking):
1. Add `bandwidth_egress_percent` to MhLoadReport (MH)
2. Session binding composite key `(meeting_id, participant_id)` (DB)
3. Explicit degraded state timeout values (Test)
4. Fencing generation as uint64 (Test)
5. SLO targets in documentation (Observability)
6. FENCED_OUT alerting threshold (Operations)
7. Circuit breaker fallback behavior clarification (Operations)

**Documentation Tasks** (post-ADR):
- Redis persistence strategy
- Emergency master secret rotation procedure
- Trace sampling strategy
- Operations guide for MC

---

## Consensus Decision

**The Meeting Controller Architecture proposal has reached consensus.**

All specialists have approved (4 APPROVE, 5 CONDITIONAL APPROVE with minor implementation details).

**ADR Created**: [ADR-0023: Meeting Controller Architecture](../decisions/adr-0023-meeting-controller-architecture.md)

**Next Steps**:
1. ~~Create ADR-0023: Meeting Controller Architecture~~ ✅ Done
2. Begin implementation with Phase 5
3. Address minor conditions during implementation

