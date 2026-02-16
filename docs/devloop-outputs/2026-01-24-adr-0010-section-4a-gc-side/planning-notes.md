# ADR-0010 Section 4a GC-Side Implementation - Planning Notes

## Task Overview

Implement the GC-side components specified in ADR-0010 Section 4a:
1. GC->MC AssignMeeting RPC (GC sends meeting assignment to MC)
2. MC Rejection Handling in GC (Retry with different MC on rejection)
3. MH Registry in GC (MH registration + load reports)
4. MH Cross-Region Sync (Sync MH registry via GC-to-GC)
5. RequestMhReplacement RPC (GC handles MC's request for MH replacement)

## Codebase Exploration Summary

### Current State

**GC Infrastructure Already Implemented:**
- MC registration via gRPC (`RegisterMC`, heartbeats)
- MC assignment via database (`meeting_assignments` table)
- Meeting join handlers that assign meetings to MCs
- gRPC auth layer (Tower-based JWT validation)
- Health checker background task
- Assignment cleanup background task

**Proto Messages Available:**
- `AssignMeeting` / `AssignMeetingResponse` - Basic meeting assignment (legacy)
- `MeetingControllerService` trait with `Assign` method (legacy)
- `MeetingControllerServiceClient` for GC->MC calls

**Missing from Proto (per ADR-0010 Section 4a):**
- `AssignMeetingRequest` with `mh_assignments` field
- `MhAssignment` message with `mh_id`, `webtransport_endpoint`, `role`
- `MhRole` enum (PRIMARY, BACKUP)
- `RejectionReason` enum
- `MhReplacementRequest` / `MhReplacementResponse`
- `McAssignment` service (separate from `MeetingControllerService`)
- MH registration messages (`RegisterMh`, `MhLoadReport`)
- Cross-region GC sync messages

### Architecture Implications

**GC->MC AssignMeeting Flow Change:**
Current flow:
1. Client joins meeting -> GC handler
2. GC selects MC via load balancing (database query)
3. GC atomically inserts assignment into database
4. GC returns MC endpoint to client

ADR-0010 Section 4a flow:
1. Client joins meeting -> GC handler
2. GC selects MC via load balancing
3. **GC selects MHs via load balancing (NEW)**
4. **GC calls MC via gRPC: AssignMeeting with MH assignments (NEW)**
5. MC accepts or rejects
6. **If rejected, GC selects different MC and retries (NEW)**
7. On accept, GC inserts assignment into database
8. GC returns MC endpoint to client

This is a significant change to the meeting join flow.

## Scope Assessment

This task involves:
- **Protocol changes**: New proto messages (crosses service boundaries)
- **Database schema changes**: MH registry table needed
- **Service layer changes**: New MH selection service
- **Handler changes**: Modified meeting join flow
- **Background task changes**: MH health checking
- **Cross-region gRPC**: New peer-to-peer service

### Escalation Analysis

**Crosses service boundaries**: Yes
- Proto changes affect both GC and MC
- MH registry affects GC, MC, and MH
- Cross-region sync affects GC-to-GC communication

**Architectural decisions needed**:
- MH selection algorithm details (beyond ADR-0010/0023 spec)
- Cross-region MH sync mechanism
- Error handling for MC rejection cascades

**ADR status**: ADR-0010 and ADR-0023 already define the high-level design with protocol messages. The implementation follows established patterns.

### Recommendation: Proceed with Dev-Loop (No Escalation)

**Rationale**:
1. ADR-0010 Section 4a specifies the protocol messages explicitly
2. ADR-0023 Section 5 details MH selection algorithm
3. This is implementation of accepted designs, not new architectural decisions
4. Proto changes are additive (backward compatible)
5. MC specialist will review protocol choices during code review

However, this is a **large task** that should be broken into sub-tasks.

## Proposed Sub-Task Breakdown

### Phase 1: Proto and Database Schema (Foundation)
1. Add new proto messages to `internal.proto`
2. Create MH registry migration
3. Regenerate proto-gen crate

### Phase 2: MH Registry in GC
1. MH registration repository
2. MH registration gRPC handlers
3. MH health/load tracking
4. MH selection service (weighted scoring)

### Phase 3: GC->MC AssignMeeting RPC
1. MC client wrapper with connection pooling
2. AssignMeeting RPC implementation
3. Modify meeting join handler to use RPC
4. MC rejection handling with retry logic

### Phase 4: MH Cross-Region Sync
1. Cross-region GC peer service
2. MH registry sync messages
3. Sync background task

### Phase 5: RequestMhReplacement RPC
1. GC handler for MC replacement requests
2. MH failover selection logic

## Key Implementation Decisions

### 1. MC Client Connection Management

**Options**:
A. Create new connection per request
B. Connection pool per MC
C. Single long-lived connection per MC

**Recommendation**: Option B - Connection pool per MC
- Use `tonic::transport::Channel` with built-in HTTP/2 connection pooling
- Cache channels by MC endpoint
- Invalidate on MC unregistration or health failure

### 2. MH Registry Table Design

```sql
CREATE TABLE media_handlers (
    mh_id VARCHAR(255) PRIMARY KEY,
    region TEXT NOT NULL,
    zone TEXT NOT NULL,
    webtransport_endpoint TEXT NOT NULL,
    max_streams INTEGER NOT NULL,
    current_streams INTEGER NOT NULL DEFAULT 0,
    cpu_percent INTEGER NOT NULL DEFAULT 0,
    bandwidth_ingress_percent INTEGER NOT NULL DEFAULT 0,
    bandwidth_egress_percent INTEGER NOT NULL DEFAULT 0,
    packet_loss_permille INTEGER NOT NULL DEFAULT 0,
    health_status TEXT NOT NULL DEFAULT 'pending',
    last_load_report_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### 3. Retry Logic for MC Rejection

**Per ADR-0010**: Max 3 retries before returning 503

```rust
const MAX_MC_ASSIGNMENT_RETRIES: usize = 3;

async fn assign_meeting_with_retries(...) -> Result<McAssignment, GcError> {
    let mut attempted_mcs = HashSet::new();

    for attempt in 0..MAX_MC_ASSIGNMENT_RETRIES {
        let mc = select_mc_excluding(&attempted_mcs)?;
        attempted_mcs.insert(mc.id.clone());

        match notify_mc_assignment(&mc, meeting_id, mh_assignments).await {
            Ok(()) => {
                // MC accepted, write to database
                return atomic_assign_to_db(...).await;
            }
            Err(RejectionReason::AtCapacity | RejectionReason::Draining) => {
                // Transient, try another MC
                continue;
            }
            Err(RejectionReason::Unhealthy) => {
                // Mark MC unhealthy, try another
                mark_mc_unhealthy(&mc).await;
                continue;
            }
        }
    }

    Err(GcError::ServiceUnavailable("No available meeting controllers"))
}
```

### 4. Cross-Region MH Sync Mechanism

Use existing outbox pattern from ADR-0010 Section 5:
- MH registration writes to `mh_registry_outbox` table
- Publisher broadcasts to peer GCs
- Peer GCs write to local MH registry

## Files to Modify

### Proto
- `proto/internal.proto` - Add new messages

### Database
- `migrations/YYYYMMDD_mh_registry.sql` - MH registry table

### GC Crate
- `src/repositories/mod.rs` - Export new module
- `src/repositories/media_handlers.rs` - NEW: MH repository
- `src/services/mod.rs` - Export new modules
- `src/services/mh_selection.rs` - NEW: MH selection service
- `src/services/mc_client.rs` - NEW: MC gRPC client wrapper
- `src/services/mc_assignment.rs` - Modify for RPC flow
- `src/grpc/mod.rs` - Export new modules
- `src/grpc/mh_service.rs` - NEW: MH registration handlers
- `src/grpc/gc_peer_service.rs` - NEW: Cross-region sync (if implementing Phase 4)
- `src/handlers/meetings.rs` - Modify join flow
- `src/tasks/mod.rs` - Export new tasks
- `src/tasks/mh_health_checker.rs` - NEW: MH health tracking

### Tests
- `tests/mh_registry_tests.rs` - NEW
- `tests/mc_assignment_with_retry_tests.rs` - NEW

## Questions for User

1. **Scope confirmation**: Should all 5 sub-components be implemented together, or should we start with a subset (e.g., just Phase 1-3)?

2. **Cross-region sync priority**: MH cross-region sync (Phase 4) is more complex. Should it be deferred to a follow-up task?

3. **Integration testing**: The meeting join flow change requires MC to be running for full integration tests. Should we implement mock MC client for unit testing?

4. **Proto evolution**: ADR-0010 defines `McAssignment` as a new service separate from `MeetingControllerService`. Should we keep both services or consolidate?
