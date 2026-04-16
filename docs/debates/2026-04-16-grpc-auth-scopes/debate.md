# Debate: Inter-service gRPC Authentication Scopes

**Date**: 2026-04-16
**Status**: Complete
**Participants**: auth-controller, global-controller, meeting-controller, media-handler, security, test, observability, operations

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

Two issues found during MH QUIC user story implementation:

1. **SCOPE NAMING**: ADR-0003 specifies `service.write.mh` / `service.read.gc` convention but implementation uses `meeting:create` / `media:forward` (setup.sh, AC database). New auth interceptors (McAuthLayer, MhAuthLayer) used the ADR-0003 convention which doesn't match what AC actually issues. Need to standardize.

2. **AUTH LAYER SCOPE**: MC's McAuthLayer was applied server-wide (tonic `Server::builder().layer()`), gating both GC→MC (`MeetingControllerService`) and MH→MC (`MediaCoordinationService`). GC→MC already has its own `McAuthInterceptor` with structural validation. Different callers may have different trust levels — should auth layers be per-service rather than server-wide?

## Context

### Current State

**AC client registrations (setup.sh:457-459)**:
- GC: `['meeting:create', 'meeting:read', 'meeting:update', 'internal:meeting-token']`
- MC: `['media:forward', 'session:manage']`
- MH: `['media:receive', 'media:send']`

**Auth interceptors requiring ADR-0003 scopes**:
- MC `McAuthLayer` (auth_interceptor.rs:139): requires `service.write.mc`
- MH `MhAuthLayer` (auth_interceptor.rs:34): requires `service.write.mh`

**ADR-0003 scope format**:
- `service.write.mh` — write access to MH
- `service.read.gc` — read access to GC
- Pattern: `service.{read|write}.{target-service}`

**Who calls whom**:
- GC → MC: `AssignMeetingWithMh` (via `MeetingControllerService`, port 50052)
- GC → MH: (none currently, MH registers with GC)
- MC → MH: `RegisterMeeting` (via `MediaHandlerService`, port 50053)
- MH → MC: `NotifyParticipantConnected/Disconnected` (via `MediaCoordinationService`, port 50052)
- MH → GC: `RegisterMH`, `SendLoadReport` (via gRPC, port 50051)
- MC → GC: `RegisterMc`, `Heartbeat` (via gRPC, port 50051)

**MC gRPC server (main.rs:318-321)**:
```rust
let grpc_server = tonic::transport::Server::builder()
    .layer(mc_auth_layer)  // McAuthLayer requiring service.write.mc
    .add_service(MeetingControllerServiceServer::new(mc_assignment_service))  // GC→MC
    .add_service(MediaCoordinationServiceServer::new(media_coord_service))   // MH→MC
```

**The bug**: McAuthLayer gates both services. GC tokens don't have `service.write.mc` scope → GC→MC assignment calls fail with UNAUTHENTICATED.

## Positions

### Initial Positions

| Specialist | Position | Satisfaction |
|------------|----------|--------------|
| auth-controller | TBD | TBD |
| global-controller | TBD | TBD |
| meeting-controller | TBD | TBD |
| media-handler | TBD | TBD |
| security | TBD | TBD |
| test | TBD | TBD |
| observability | TBD | TBD |
| operations | TBD | TBD |

## Discussion

### Round 1

{Will be populated as debate progresses}

## Consensus

Reached at Round 3. All 8 specialists at 90%+.

| Specialist | Final Score |
|------------|-------------|
| auth-controller | 95% |
| global-controller | 95% |
| meeting-controller | 95% |
| media-handler | 92% |
| security | 95% |
| test | 95% |
| observability | 95% |
| operations | 95% |

## Decision

Folded into ADR-0003: `docs/decisions/adr-0003-service-authentication.md` (Components 2 and 6, Implementation Status)
