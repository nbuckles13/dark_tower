# User Story: Client-to-MH QUIC Connection

**Date**: 2026-04-12
**Status**: Ready for Review
**Participants**: auth-controller, global-controller, meeting-controller, media-handler, database, protocol, infrastructure, security, test, observability, operations
**Predecessor**: [Meeting Join Flow](2026-03-21-meeting-join.md)

## Story

As a **meeting participant**, I want **to establish QUIC/WebTransport connections to the assigned MH instances** so that **I can send and receive media streams for video conferencing**.

**Scope**: Client connects to MH via WebTransport using meeting JWT for authentication. MC populates `MediaServerInfo` in `JoinResponse` from Redis (no MH call in join path). MC→MH async `RegisterMeeting` establishes coordination channel. MH→MC notifications track participant connections/disconnections. NO media frame forwarding (data plane is a separate story). NO layout subscriptions or stream routing.

**Predecessor context**: The meeting join story (2026-03-21) established client→MC WebTransport, JWT validation, session binding. MC stores MH assignments in Redis (`MhAssignmentData`). This story adds the client→MH media transport layer and MC↔MH coordination.

## Connection Flow

### First participant in a meeting
```
1. User → GC:  join meeting
2. GC:          no existing assignment → select MC + MHs
3. GC → MC:    AssignMeetingWithMh (once — MC stores MH data in Redis, creates meeting actor)
4. GC → AC:    request meeting token
5. GC → User:  MC endpoint + meeting token
6. User → MC:  JoinRequest (WebTransport + JWT)
7. MC → User:  JoinResponse (MH WebTransport URLs from Redis)
8. [ASYNC] MC → MH: RegisterMeeting (once per MH — establishes coordination channel)
9. User → MH:  WebTransport + meeting JWT (connects to all assigned MHs in parallel)
10. MH → MC:   NotifyParticipantConnected (per MH the user connects to)
```

### Subsequent participants
```
1. User → GC:  join meeting
2. GC:          existing assignment found → reuse
3. GC → AC:    request meeting token
4. GC → User:  same MC endpoint + new meeting token
5. User → MC:  JoinRequest
6. MC → User:  JoinResponse (same MH URLs from Redis)
7. User → MH:  WebTransport + meeting JWT (all MHs in parallel)
8. MH → MC:    NotifyParticipantConnected (per MH)
```

GC assignment (steps 2-3 of first flow) and MC→MH RegisterMeeting (step 8 of first flow) are both first-participant-only. Per-participant operations: GC issues token, MC processes join, client connects to MHs, MH notifies MC.

## Requirements

### Connection Establishment
- R-1: `MhAssignment` proto message includes `grpc_endpoint` field so MC can discover MH's gRPC address for coordination (from: protocol, global-controller)
- R-2: Remove `MhRole` enum from proto — MH connections are active/active, not active/standby; remove `role` field from `MhAssignment` (reserve field number) (from: protocol)
- R-3: Remove `connection_token` field from `MediaServerInfo` in `signaling.proto` (reserve field number) — client authenticates to MH with meeting JWT, not a separate token (from: protocol)
- R-4: GC propagates MH `grpc_endpoint` through assignment chain: DB → `MhAssignmentInfo` → `MhAssignment` proto → MC (from: global-controller)
- R-5: MC stores MH gRPC endpoints in Redis alongside WebTransport endpoints in `MhAssignmentData` (from: meeting-controller)
- R-6: MC populates `JoinResponse.media_servers` with MH WebTransport URLs from Redis — NO synchronous MH call in the join path; join FAILS if MH assignment data cannot be read from Redis (from: meeting-controller)
- R-7: MH validates meeting JWTs via AC JWKS endpoint using shared `JwtValidator` from common crate (same validation chain as MC: size check → kid extraction → JWKS lookup → EdDSA verify → exp/iat checks → meeting_id extraction) (from: media-handler, security)
- R-8: MH implements WebTransport server on configured port (default 4434) with TLS 1.3 over QUIC, accepting client connections authenticated by meeting JWT (from: media-handler)
- R-9: Client connects to all assigned MHs in parallel — active/active, not failover (from: media-handler)

### MC→MH Coordination
- R-10: New `RegisterMeeting` RPC on `MediaHandlerService` — MC registers a meeting with MH asynchronously after first participant joins; includes `mc_grpc_endpoint` so MH can call back (from: protocol, meeting-controller)
- R-11: MC creates MH gRPC client for `RegisterMeeting` calls, authenticating with cryptographically validated OAuth service token (from: meeting-controller)
- R-12: MC fires `RegisterMeeting` asynchronously (does not block JoinResponse); retries with backoff on failure (from: meeting-controller)
- R-13: MH tracks registered meetings and associated MC endpoint for callback notifications (from: media-handler)
- R-14: MH enforces `RegisterMeeting` arrival timeout: clients connecting to an unregistered meeting are accepted provisionally for a configurable window (default 15s); if `RegisterMeeting` does not arrive within the window, MH disconnects the client (from: media-handler, security)

### MH→MC Coordination
- R-15: New `MediaCoordinationService` gRPC service on MC for MH→MC calls with `NotifyParticipantConnected` and `NotifyParticipantDisconnected` RPCs (from: protocol, meeting-controller)
- R-16: MH notifies MC when a JWT-authenticated participant establishes a WebTransport connection; notification queued if `RegisterMeeting` hasn't arrived yet, delivered when it arrives (from: media-handler)
- R-17: MH notifies MC when a participant's WebTransport connection drops (from: media-handler)
- R-18: MC maintains per-meeting registry of participant-to-MH connection state for future media routing (from: meeting-controller)

### Client Failure Signaling
- R-19: New `MediaConnectionFailed` signaling message (Client→MC) for client to report MH connection failures, including whether all MH connections have failed (from: protocol)
- R-20: MH reallocation when all MH connections fail is deferred — MC logs the failure and records metric; future story adds MC→GC reallocation mechanism (from: meeting-controller)

### Authentication
- R-21: MC→MH gRPC uses full JWKS-based cryptographic validation of MC's OAuth service token (upgrade from structural-only `MhAuthInterceptor`) (from: security, media-handler)
- R-22: MH→MC gRPC uses full JWKS-based cryptographic validation of MH's OAuth service token on MC's gRPC auth layer (from: security, meeting-controller)

### Infrastructure
- R-23: MH network policy: add egress to MC gRPC port 50052 (from: infrastructure)
- R-24: MC network policy: add ingress from MH on gRPC port 50052 (from: infrastructure)
- R-25: MH config: add `AC_JWKS_URL` env var for JWT validation via JWKS (from: infrastructure)

### Observability
- R-26: MH records WebTransport metrics: `mh_webtransport_connections_total` (counter by status), `mh_webtransport_handshake_duration_seconds` (histogram), `mh_active_connections` (gauge) (from: media-handler, observability)
- R-27: MH records JWT validation metrics: `mh_jwt_validations_total` (counter by status/token_type) (from: media-handler, observability)
- R-28: MC records MH coordination metrics: `mc_register_meeting_total` (counter by status), `mc_register_meeting_duration_seconds` (histogram), `mc_mh_participant_notifications_total` (counter by event_type) (from: meeting-controller, observability)
- R-29: MH WebTransport dashboard panels + alert rules in `mh-overview.json` (from: observability)
- R-30: MC MH-coordination dashboard panels in `mc-overview.json` (from: observability)

### Testing
- R-31: MH integration tests: WebTransport connection with JWT validation, RegisterMeeting handling, MC notification delivery, RegisterMeeting timeout enforcement (from: media-handler)
- R-32: MC join tests updated: media_servers populated from Redis, mock MH gRPC for RegisterMeeting + notifications, connection registry (from: meeting-controller)
- R-33: End-to-end env-tests: join meeting → receive MediaServerInfo → connect to MH via WebTransport with meeting JWT → verify MH→MC notification arrives (from: test)

### Operations
- R-34: MH deployment runbook covering WebTransport server failures, JWT validation issues, MC notification delivery failures, RegisterMeeting timeout scenarios (from: operations)
- R-35: MC runbook updated with MH coordination failure scenarios (RegisterMeeting, notification handling) (from: operations)
- R-36: Post-deploy monitoring checklist for MH WebTransport and MC↔MH coordination metrics (from: operations)

---

## Architecture Validation

**Result**: PASS (all 11 specialists confirmed)

All changes fit within existing architecture or are natural extensions:
- **Proto**: Field additions/removals on existing messages, new RPCs on existing service, new service on existing MC gRPC server. No new transport protocols.
- **GC**: Propagating existing `grpc_endpoint` data through existing assignment pipeline (field already in DB and `MhCandidate`). Removing `MhRole` from assignment construction.
- **MC**: `media_servers` population from Redis is trivial. `MediaCoordinationService` is a new gRPC service on MC's existing :50052 server (same pattern as `MeetingControllerService`).
- **MH**: WebTransport follows MC's `wtransport` pattern. JWT validation reuses common crate `JwtValidator` (same as MC). `RegisterMeeting` handler is a new RPC on existing `MediaHandlerService`.
- **MH→MC gRPC**: New communication direction, but natural bidirectional extension of existing MC→MH relationship. Network policies updated. Both directions use cryptographic JWKS-based service token validation.
- **Network**: MC→MH egress on :50053 already configured. MH→MC egress on :50052 added by infra task.
- **Auth upgrade**: MH auth interceptor and MC gRPC auth upgraded from structural-only to full JWKS validation. Both services already have JWKS infrastructure.

**Opt-outs (justified)**:
- **auth-controller**: Both MC and MH use existing OAuth service tokens validated via AC JWKS. MH reuses common crate `JwtValidator` for meeting JWTs. No new token types or AC changes needed. Interface validated: `JwtValidator::validate<ServiceClaims>()` and `JwtValidator::validate<MeetingTokenClaims>()` at `crates/common/src/jwt.rs`.
- **database**: No new tables or migrations. MH participant tracking is in-memory. MC uses existing Redis `MhAssignmentData` (extended with gRPC endpoint fields). GC already stores `grpc_endpoint` in `media_handlers` table. Interface validated: `MhAssignmentData` at `crates/mc-service/src/redis/client.rs:42`, `media_handlers.grpc_endpoint` at `crates/gc-service/src/repositories/media_handlers.rs:37`.

---

## Design

### protocol

**Changes to `proto/internal.proto`**:

1. Add `grpc_endpoint` to `MhAssignment`, remove `role` and `MhRole`:
```protobuf
// Remove MhRole enum entirely (was: MH_ROLE_UNSPECIFIED, PRIMARY, BACKUP)
// Reserve enum values for wire compatibility

message MhAssignment {
  string mh_id = 1;
  string webtransport_endpoint = 2;
  reserved 3;                    // was: MhRole role (removed — active/active)
  string grpc_endpoint = 4;     // NEW: MC→MH gRPC endpoint
}
```

2. Add `RegisterMeeting` RPC to existing `MediaHandlerService`:
```protobuf
service MediaHandlerService {
  rpc Register(RegisterParticipant) returns (RegisterParticipantResponse);       // existing stub
  rpc RegisterMeeting(RegisterMeetingRequest) returns (RegisterMeetingResponse); // NEW
  rpc RouteMedia(RouteMediaCommand) returns (RouteMediaResponse);
  rpc StreamTelemetry(stream MediaTelemetry) returns (TelemetryAck);
}

message RegisterMeetingRequest {
  string meeting_id = 1;
  string mc_id = 2;
  string mc_grpc_endpoint = 3;  // MH uses this to call back to MC
}

message RegisterMeetingResponse {
  bool accepted = 1;
}
```

3. Add `MediaCoordinationService` for MH→MC:
```protobuf
// MH→MC coordination service (hosted on MC's existing gRPC server :50052)
service MediaCoordinationService {
  rpc NotifyParticipantConnected(ParticipantMediaConnected) returns (ParticipantMediaConnectedResponse);
  rpc NotifyParticipantDisconnected(ParticipantMediaDisconnected) returns (ParticipantMediaDisconnectedResponse);
}

message ParticipantMediaConnected {
  string meeting_id = 1;
  string participant_id = 2;  // From meeting JWT sub claim
  string handler_id = 3;
}

message ParticipantMediaConnectedResponse {
  bool acknowledged = 1;
}

message ParticipantMediaDisconnected {
  string meeting_id = 1;
  string participant_id = 2;
  string handler_id = 3;
  string reason = 4;  // "client_closed", "timeout", "error"
}

message ParticipantMediaDisconnectedResponse {
  bool acknowledged = 1;
}
```

**Changes to `proto/signaling.proto`**:

4. Remove `connection_token` from `MediaServerInfo`:
```protobuf
message MediaServerInfo {
  string media_handler_url = 1;
  reserved 2;  // was: connection_token (removed — client uses meeting JWT)
}
```

5. Add `MediaConnectionFailed` to `ClientMessage.oneof`:
```protobuf
message MediaConnectionFailed {
  string media_handler_url = 1;
  string error_reason = 2;       // "timeout", "tls_error", "connection_refused"
  bool all_handlers_failed = 3;  // True when client has no MH connections remaining
}

message ClientMessage {
  oneof message {
    // ... existing fields 1-10 ...
    MediaConnectionFailed media_connection_failed = 11;  // NEW
  }
}
```

6. Clean up `MhRole` references: remove `MhRole` enum, update any code that references `MH_ROLE_PRIMARY`/`MH_ROLE_BACKUP`.

### global-controller

**Changes**:
1. **`MhAssignmentInfo`** (`services/mh_selection.rs`): Add `grpc_endpoint: String` field. Populate from `MhCandidate.grpc_endpoint` during selection. Remove any `role` assignment logic (no more primary/backup distinction).
2. **Assignment construction**: Include `grpc_endpoint` from `MhAssignmentInfo` in proto `MhAssignment`. Remove `role` field population. GC selects MHs by load/AZ — both are peers, not primary/backup.

### meeting-controller

**New modules**:
- `crates/mc-service/src/grpc/mh_client.rs` — gRPC client for calling MH `RegisterMeeting`
- `crates/mc-service/src/grpc/media_coordination.rs` — `MediaCoordinationService` handler for MH→MC notifications
- `crates/mc-service/src/services/mh_connection_registry.rs` — Per-meeting participant→MH connection tracking

**Changes**:
1. **`MhAssignmentData`** (`redis/client.rs`): Add `primary_grpc_endpoint: String` and `backup_grpc_endpoint: Option<String>` fields. (Note: "primary"/"backup" naming is legacy from Redis storage — these are now peer MHs. Renaming the fields is optional cleanup.)
2. **`store_mh_assignments()`** (`grpc/mc_service.rs`): Store gRPC endpoints from `MhAssignment.grpc_endpoint` in Redis. Remove `role` handling.
3. **`build_join_response()`** (`webtransport/connection.rs`): Read `MhAssignmentData` from Redis during join flow. Populate `media_servers` with `MediaServerInfo { media_handler_url }` for each assigned MH. **Fail the join** if Redis read fails or MH assignment data is absent (meeting without media is not useful).
4. **`MhClient`** (`grpc/mh_client.rs`): gRPC client wrapping `MediaHandlerServiceClient`. Takes `TokenReceiver` for Bearer auth. Method: `register_meeting(mh_grpc_endpoint, meeting_id, mc_id, mc_grpc_endpoint) -> Result<(), McError>`. Service token validated cryptographically by MH.
5. **Async RegisterMeeting trigger**: After first participant joins a meeting, MC fires `MhClient::register_meeting()` for each assigned MH (gRPC endpoints from Redis). Non-blocking — runs as a spawned task, retries with backoff on failure.
6. **`MediaCoordinationService`** (`grpc/media_coordination.rs`): Handles `NotifyParticipantConnected` and `NotifyParticipantDisconnected` from MH. Validates MH service token via JWKS. Routes updates to `MhConnectionRegistry`.
7. **`MhConnectionRegistry`** (`services/mh_connection_registry.rs`): `HashMap<meeting_id, HashMap<participant_id, Vec<MhConnectionInfo>>>` tracking which MHs each participant is connected to. Updated by MH notifications. Read by future media routing.
8. **`main.rs`**: Create `MhClient` with `token_rx`, create `MhConnectionRegistry`, register `MediaCoordinationService` on gRPC server alongside existing `MeetingControllerService`. Pass `MhClient` and Redis client to WebTransport connection handler. Add JWKS-based auth layer for MH→MC calls.
9. **`MediaConnectionFailed` handling** (`webtransport/handler.rs` or bridge loop): Log warning, record metric. No automatic reallocation (deferred per R-20).
10. **Metrics** (`observability/metrics.rs`): Add `record_register_meeting()`, `record_mh_notification()`, `record_media_connection_failed()`.

**Join flow** (steps 5→8 add only Redis read):
```
Step 5:  JWT validation ✅
Step 6:  JoinConnection to controller ✅
Step 7:  MeetingActor processes join → JoinResult ✅
Step 8:  [NEW] Read MhAssignmentData from Redis → FAIL join if unavailable
Step 9:  Build JoinResponse with media_servers from Redis
Step 10: Send JoinResponse, enter bridge loop
Step 11: [ASYNC, first participant only] Fire RegisterMeeting to each MH
```

### media-handler

**New modules**:
- `crates/mh-service/src/auth/` — JWT validation for meeting tokens and gRPC service token upgrade
  - `mod.rs` — `MhJwtValidator` wrapping common `JwksClient` + `JwtValidator::validate<MeetingTokenClaims>`
- `crates/mh-service/src/webtransport/` — WebTransport server for client media connections
  - `mod.rs` — module exports
  - `server.rs` — `wtransport` accept loop, TLS 1.3, CancellationToken wiring (same pattern as MC)
  - `connection.rs` — Per-connection handler: accept session, read meeting JWT from first bidirectional stream message, validate via `MhJwtValidator`, check meeting registration status, establish session or start provisional timer
- `crates/mh-service/src/session/` — Meeting and participant session tracking
  - `mod.rs` — `SessionManager` tracking registered meetings and active participant connections
- `crates/mh-service/src/grpc/mc_client.rs` — gRPC client for calling MC's `MediaCoordinationService`

**Changes**:
1. **`MhMediaService`** (`grpc/mh_service.rs`): Add `RegisterMeeting` handler. Stores meeting registration in `SessionManager` (meeting_id → MC endpoint mapping). Delivers any queued participant notifications for that meeting. Returns `accepted: true`.
2. **`MhAuthInterceptor`** (`grpc/auth_interceptor.rs`): Upgrade from structural-only Bearer validation to full JWKS-based cryptographic validation of service tokens via `JwtValidator<ServiceClaims>`. Uses same `JwksClient` as meeting JWT validation.
3. **`main.rs`**: Initialize `JwksClient` + `MhJwtValidator` (with `AC_JWKS_URL`), create `SessionManager`, start WebTransport server, create `McClient` for MH→MC notifications. Pass shared state to gRPC service and WebTransport handler.
4. **`lib.rs`**: Add `pub mod auth;`, `pub mod webtransport;`, `pub mod session;`.
5. **`config.rs`**: Add `ac_jwks_url: String` (required env var `AC_JWKS_URL`). Add `register_meeting_timeout_seconds: u64` (default 15, env var `MH_REGISTER_MEETING_TIMEOUT_SECONDS`).
6. **`errors.rs`**: Add `JwtValidation`, `WebTransportError`, `McNotificationError`, `MeetingNotRegistered` variants to `MhError`.
7. **`observability/metrics.rs`**: Add WebTransport, JWT validation, and notification metrics.
8. **`Cargo.toml`**: Add `wtransport` (workspace dep), `jsonwebtoken` (via common crate). `reqwest` for JWKS already transitive via common.

**WebTransport connection flow**:
```
1. Client connects to MH WebTransport endpoint (QUIC + TLS 1.3)
2. MH accepts session, accepts bidirectional stream
3. Client sends meeting JWT as first length-prefixed message (same framing as MC)
4. MH validates JWT via MhJwtValidator:
   size check → extract kid → JWKS lookup → EdDSA verify → exp/iat checks
   → extract meeting_id, participant_id (sub claim)
5. MH checks SessionManager: is this meeting registered?
   a. Registered: accept connection, notify MC via NotifyParticipantConnected
   b. Not registered: accept provisionally, start 15s timer
      - If RegisterMeeting arrives before timeout: promote to full connection, notify MC
      - If timeout expires: disconnect client with error "meeting not available"
6. On client disconnect: notify MC via NotifyParticipantDisconnected, clean up session
```

**SessionManager state** (in-memory, no Redis):
- `registered_meetings: HashMap<meeting_id, MeetingRegistration>` where `MeetingRegistration { mc_id, mc_grpc_endpoint, registered_at }`
- `active_connections: HashMap<(meeting_id, participant_id), Vec<ConnectionEntry>>` where `ConnectionEntry { handler_id, connected_at, connection_id }`
- `pending_connections: HashMap<meeting_id, Vec<PendingConnection>>` — connections awaiting RegisterMeeting, each with its own timeout future
- Cleanup: when all connections for a meeting drop AND meeting has been registered for >5 min with no connections, remove meeting registration (prevents unbounded growth)

**MC notification delivery** (`grpc/mc_client.rs`):
- `McClient` wraps `MediaCoordinationServiceClient`, authenticated with MH's OAuth service token
- On participant connect: fire `NotifyParticipantConnected` to MC endpoint from `MeetingRegistration`
- On participant disconnect: fire `NotifyParticipantDisconnected`
- If MC is unreachable: log warning, retry with backoff (3 attempts, 1s/2s/4s), then give up (client connection is not affected — notification is best-effort)
- For pending connections (pre-RegisterMeeting): queue notification, deliver when RegisterMeeting provides MC endpoint

### infrastructure

**Changes**:
1. **MC network policy** (`infra/services/mc-service/network-policy.yaml`): Add ingress rule allowing `mh-service` on TCP 50052:
```yaml
  # Allow ingress from Media Handler (participant connect/disconnect notifications)
  - from:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: dark-tower
      podSelector:
        matchLabels:
          app: mh-service
    ports:
    - protocol: TCP
      port: 50052
```

2. **MH network policy** (`infra/services/mh-service/network-policy.yaml`): Add egress rule allowing `mc-service` on TCP 50052:
```yaml
  # Allow egress to Meeting Controller (participant notifications)
  - to:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: dark-tower
      podSelector:
        matchLabels:
          app: mc-service
    ports:
    - protocol: TCP
      port: 50052
```

3. **MH configmap** (`infra/services/mh-service/mh-{0,1}-configmap.yaml`): Add `AC_JWKS_URL` pointing to AC's JWKS endpoint (e.g., `http://ac-service:8082/.well-known/jwks.json`).

4. **MH deployment** (`infra/services/mh-service/mh-{0,1}-deployment.yaml`): Add `AC_JWKS_URL` env var from configmap.

### security (cross-cutting — no separate tasks)

**Security checklist**:
1. **Authentication**: Client→MH uses meeting JWT (EdDSA, 15-min TTL) validated via AC JWKS — same mechanism and security model as client→MC. MC→MH gRPC uses OAuth service token validated cryptographically via AC JWKS (upgraded from structural-only). MH→MC gRPC uses OAuth service token validated cryptographically via AC JWKS.
2. **Authorization**: Meeting JWT is scoped to `meeting_id` + `participant_type` + `role`. MH validates JWT claims including `meeting_id`. Additionally, MH enforces RegisterMeeting timeout — connections to unregistered meetings are disconnected after 15s, limiting the window for use of a stolen JWT against an arbitrary MH. A stolen JWT carries the same risk at MH as at MC — this is the established bearer-token security model per ADR-0020, mitigated by TLS (token not observable in transit) and short TTL.
3. **Input validation**: JWT 8KB size limit. WebTransport message framing (64KB max, same as MC). `RegisterMeeting` field validation (non-empty meeting_id, mc_id, mc_grpc_endpoint).
4. **Data protection**: No PII in metrics labels. Participant IDs (UUIDs) in logs are not PII. JWTs not logged. Service token secrets are `SecretString`.
5. **Error handling**: Generic error messages to clients ("Invalid or expired token", "Meeting not available"). Detailed errors in structured logs only.
6. **Cryptography**: EdDSA (Ed25519) for JWT validation (via common crate). TLS 1.3 required by QUIC. JWKS cache with 5-min TTL.

### observability

**MH instrumentation**:

```
Operation: WebTransport connection accept
  Success/failure metric: mh_webtransport_connections_total{status=success|failure|timeout}
  Latency metric: mh_webtransport_handshake_duration_seconds (p50, p95, p99)
  Gauge: mh_active_connections (current open connections)
  Dashboard: MH overview — connection rate, handshake latency, active connections
  Alert: handshake failure rate >5% for 5m (P2)
  Logs: connection accepted/rejected/timed_out (connection_id, meeting_id — no JWT)

Operation: JWT validation
  Success/failure metric: mh_jwt_validations_total{status=success|failure,reason=expired|invalid_sig|...}
  Dashboard: MH overview — validation rate, failure breakdown
  Alert: validation failure rate >10% for 5m (P2, may indicate key rotation issue)
  Logs: validation result (status, reason — no token value)

Operation: RegisterMeeting received (MH side)
  Metric: mh_register_meeting_total{status=success|failure}
  Dashboard: MH overview — registration rate
  Logs: meeting registered (meeting_id, mc_id)

Operation: RegisterMeeting timeout (provisional client kicked)
  Metric: mh_register_meeting_timeouts_total
  Alert: timeout rate >0 sustained for 5m (P2, indicates MC→MH coordination failure)
  Logs: client disconnected due to RegisterMeeting timeout (meeting_id, participant_id)

Operation: MC notification delivery (MH side)
  Metric: mh_mc_notifications_total{event=connected|disconnected,status=success|failure}
  Alert: notification delivery failure rate >5% for 5m (P3)
  Logs: notification sent/failed (meeting_id, participant_id, event)
```

**MC instrumentation**:
```
Operation: RegisterMeeting sent (MC side)
  Metric: mc_register_meeting_total{status=success|failure}
  Latency: mc_register_meeting_duration_seconds
  Dashboard: MC overview — MH registration rate, latency
  Alert: failure rate >5% for 5m (P3)

Operation: MH notification received (MC side)
  Metric: mc_mh_notifications_received_total{event=connected|disconnected}
  Dashboard: MC overview — notification rate by event type
  Logs: participant connected/disconnected to MH (meeting_id, participant_id, handler_id)

Operation: MediaConnectionFailed received from client (MC side)
  Metric: mc_media_connection_failures_total{all_failed=true|false}
  Alert: mc_media_connection_failures with all_failed=true >0 for 5m (P2)
  Logs: client reports MH connection failure (meeting_id, handler_url, all_failed)
```

### test

**Env-tests**: End-to-end in Kind cluster:
1. Create meeting → join → verify `media_servers` non-empty in JoinResponse with MH WebTransport URLs
2. Connect to MH WebTransport with meeting JWT → verify connection accepted
3. Connect to MH with invalid/expired JWT → verify rejection
4. Verify MH→MC notification: join + connect to MH → verify MC receives `NotifyParticipantConnected`
5. Disconnect from MH → verify MC receives `NotifyParticipantDisconnected`
6. Connect to MH for unregistered meeting (no RegisterMeeting sent) → verify client disconnected after timeout

Integration tests are owned by their respective domain specialists (MC by meeting-controller, MH by media-handler).

### operations

**New MH runbook scenarios**:
- Scenario: WebTransport server failures (TLS cert issues, QUIC listener, port binding)
- Scenario: JWT validation failures (JWKS endpoint unreachable, key rotation, clock skew)
- Scenario: MC notification delivery failures (MC unreachable, auth failures, timeouts)
- Scenario: RegisterMeeting timeout — clients being kicked (MC→MH coordination broken)

**MC runbook update**:
- Scenario: RegisterMeeting failures (MH unreachable, timeout, all MHs down for a meeting)
- Scenario: MH notification handling failures (unexpected notifications, unknown meetings)
- Scenario: MediaConnectionFailed reports (all MHs failed for a client)

**Post-deploy checklist** (30-min/2-hour/4-hour/24-hour):
- MH WebTransport handshake success rate >95%
- MH JWT validation success rate >99%
- MH RegisterMeeting timeout count = 0 (indicates healthy MC→MH coordination)
- MC RegisterMeeting success rate >95%
- MH→MC notification delivery success rate >95%
- MH active connections gauge is non-zero (clients are connecting)
- MC MediaConnectionFailed(all_failed=true) count = 0
- Rollback criteria: MH WebTransport failure >10% for 10m, JWT validation failure >20% for 5m, RegisterMeeting timeouts >0 sustained for 10m

---

## Cross-Cutting Requirements

### Security Checklist
1. **Authentication**: Meeting JWT (EdDSA, 15-min TTL) for client→MH; JWKS-validated OAuth service tokens for MC↔MH gRPC (upgraded from structural-only)
2. **Authorization**: JWT scoped to meeting_id + participant_type + role; MH validates meeting_id from JWT; RegisterMeeting timeout limits unregistered meeting access to 15s
3. **Input validation**: JWT 8KB size limit; WebTransport 64KB message framing; gRPC field validation
4. **Data protection**: No PII in metrics; JWT not logged; SecretString for sensitive config
5. **Error handling**: Generic errors to clients; detailed errors in structured logs only
6. **Cryptography**: EdDSA (Ed25519) JWT validation; TLS 1.3 (QUIC); JWKS 5-min cache TTL

### Test Checklist
1. **Unit tests**: MH JWT validation edge cases, SessionManager lifecycle (register/connect/timeout/cleanup), MhConnectionRegistry operations, MhClient/McClient error handling, auth interceptor upgrade
2. **Integration tests**: MH WebTransport connection with JWT + RegisterMeeting timeout (MH-side); MC join with media_servers + RegisterMeeting + notifications (MC-side)
3. **Env-tests**: E2E join → MH connect → notification verification in Kind cluster
4. **Test infrastructure**: Mock MH gRPC server for MC tests; MH WebTransport test helpers; mock MC gRPC for MH notification tests

### Observability Checklist
1. **Business metrics**: MH WebTransport connections + JWT validations + RegisterMeeting timeouts; MC RegisterMeeting + notifications + MediaConnectionFailed
2. **Dashboard panels**: 5+ MH panels + 3+ MC panels in Grafana
3. **Structured logs**: Connection events with correlation IDs (no PII, no tokens)
4. **Alert rules**: MH handshake failures, JWT validation failures, RegisterMeeting timeouts, MC RegisterMeeting failures, all-MH-failed client reports

### Operations Checklist
1. **New failure modes**: MH WebTransport fails to start, JWT validation fails (JWKS), MC↔MH gRPC connectivity, notification delivery failures, RegisterMeeting timeout (clients kicked), all MH connections lost for a client
2. **Rollback**: All changes additive — no data migrations. Rollback = redeploy previous versions. RegisterMeeting is new RPC — old MH won't receive it (MC retries then gives up, clients still connect via JWT).
3. **Monitoring**: MH WebTransport success rate, JWT validation rate, active connections, RegisterMeeting timeout count, MC RegisterMeeting latency, notification delivery rate
4. **Runbook**: New MH runbook (4 scenarios) + MC runbook update (3 scenarios)

---

## Assumptions

| # | Assumption | Made By | Reason Not Blocked |
|---|-----------|---------|-------------------|
| 1 | `wtransport` crate is in workspace deps (MC already uses it) | media-handler | Verified: MC depends on it |
| 2 | MH WebTransport port is 4434 (matching existing config/infra) | infrastructure | Already configured in Kind, K8s, MH config |
| 3 | Client sends meeting JWT as first message on MH WebTransport bidirectional stream (same framing as MC) | media-handler | Consistent with MC pattern. Avoids token in URL/logs. |
| 4 | MH participant/meeting state is in-memory only (no Redis) — if MH restarts, clients reconnect with fresh JWTs | media-handler | MH is a stateless SFU. Restart = clients get disconnected, reconnect naturally. |
| 5 | RegisterMeeting timeout of 15s is sufficient for MC async delivery under normal conditions | media-handler | MC fires RegisterMeeting immediately after JoinResponse. Same-cluster latency is single-digit ms. 15s covers retries. Configurable via env var. |
| 6 | MH→MC notification delivery is best-effort (3 retries then give up) — client connection not affected by notification failure | media-handler | Notification is for MC's routing state. If it fails, MC won't know about the connection, but the client is still connected to MH. MC can recover state when it queries MH (future story). |

## Clarification Questions

| # | Question | Asked By | Status | Answer |
|---|---------|----------|--------|--------|

---

## Implementation Plan

### Task Dependency Graph

```
Phase 1 (parallel, no deps):
   1 (proto)
   3 (MH: JWT + WebTransport)
   9 (infra: netpol + config)

Phase 2 (after 1):
   2 (GC: grpc_endpoint)
   4 (MC: Redis + media_servers + MhClient)
   5 (MH: RegisterMeeting + SessionManager)
   7 (MC: MediaCoordinationService + registry)

Phase 3 (after prereqs):
   6 (MH: MC notification client)     deps: 1, 3, 5
   8 (MC: async RegisterMeeting)      deps: 4, 5

Phase 4 (after implementation):
   10 (MH: metrics)                   deps: 3, 6
   11 (MC: metrics)                   deps: 7, 8
   14 (MH: integration tests)         deps: 3, 5, 6
   15 (MC: join + coordination tests) deps: 4, 7, 8

Phase 5 (after metrics):
   12 (MH: dashboard + alerts)        deps: 10
   13 (MC: dashboard + alerts)        deps: 11
   16 (env-tests)                     deps: 3, 4, 6, 7

Phase 6 (after dashboards):
   17 (runbooks)                      deps: 10, 12
   18 (post-deploy checklist)         deps: 10, 11
```

### Ordered Task List

| # | Task | Specialist | Deps | Covers |
|---|------|-----------|------|--------|
| 1 | Proto: add `grpc_endpoint` to `MhAssignment`, remove `MhRole` enum + `role` field, remove `connection_token` from `MediaServerInfo`, add `RegisterMeeting` RPC, add `MediaCoordinationService`, add `MediaConnectionFailed` signaling message | protocol | — | code |
| 2 | GC: propagate `grpc_endpoint` through `MhAssignmentInfo` → `MhAssignment`, remove `role`/primary/backup assignment logic | global-controller | 1 | code |
| 3 | MH: JWKS JWT validation (`MhJwtValidator` using common crate) + WebTransport server (`wtransport` + TLS) + connection handler (JWT auth, provisional accept with RegisterMeeting timeout) + upgrade `MhAuthInterceptor` to JWKS-based service token validation | media-handler | — | code |
| 4 | MC: add gRPC endpoint fields to `MhAssignmentData` in Redis, populate `media_servers` in `JoinResponse` from Redis (fail join if unavailable), create `MhClient` gRPC client for MH RegisterMeeting calls | meeting-controller | 1 | code |
| 5 | MH: `RegisterMeeting` gRPC handler + `SessionManager` (meeting registration, pending connection promotion, meeting cleanup) | media-handler | 1 | code |
| 6 | MH: MC notification client (`McClient`) — `NotifyParticipantConnected`/`Disconnected` calls to MC, queuing for pre-RegisterMeeting connections, retry with backoff | media-handler | 1, 3, 5 | code |
| 7 | MC: `MediaCoordinationService` gRPC handler with JWKS-based MH service token validation + `MhConnectionRegistry` (per-meeting participant→MH connection state) + `MediaConnectionFailed` handler (log + metric, no reallocation) | meeting-controller | 1 | code |
| 8 | MC: async `RegisterMeeting` trigger on first participant join (spawned task, retry with backoff, does not block JoinResponse) | meeting-controller | 4, 5 | code |
| 9 | Infra: MH→MC network policy updates (MH egress + MC ingress on TCP 50052) + MH `AC_JWKS_URL` config in configmap/deployment | infrastructure | — | deploy |
| 10 | MH: observability metrics (WebTransport connections, handshake latency, active connections gauge, JWT validations, RegisterMeeting receipt, RegisterMeeting timeouts, MC notification delivery) | media-handler | 3, 6 | code, metrics |
| 11 | MC: MH coordination metrics (RegisterMeeting sent, MH notifications received, MediaConnectionFailed received) | meeting-controller | 7, 8 | code, metrics |
| 12 | MH WebTransport dashboard panels + alert rules in `mh-overview.json` + MH Prometheus alert rules file | observability | 10 | dashboard, alerts |
| 13 | MC MH-coordination dashboard panels in `mc-overview.json` + alert rule for all-MH-failed | observability | 11 | dashboard, alerts |
| 14 | MH integration tests: WebTransport + JWT validation, RegisterMeeting handling, MC notification delivery, RegisterMeeting timeout enforcement, auth interceptor JWKS upgrade | media-handler | 3, 5, 6 | tests |
| 15 | MC join + coordination tests: media_servers from Redis (success + failure), mock MH for RegisterMeeting + notifications, MhConnectionRegistry, MediaConnectionFailed handling | meeting-controller | 4, 7, 8 | tests |
| 16 | End-to-end env-tests: join → MH WebTransport with JWT → MH→MC notification → disconnect notification → RegisterMeeting timeout verification | test | 3, 4, 6, 7 | tests |
| 17 | MH deployment runbook (WebTransport, JWT, notification, RegisterMeeting timeout scenarios) + MC runbook update (RegisterMeeting, notifications, MediaConnectionFailed) | operations | 10, 12 | docs |
| 18 | Post-deploy monitoring checklist for MH WebTransport + MC↔MH coordination | operations | 10, 11 | docs |

### Requirements Coverage

| Req | Covered By Tasks |
|-----|-----------------|
| R-1 | 1 |
| R-2 | 1 |
| R-3 | 1 |
| R-4 | 2 |
| R-5 | 4 |
| R-6 | 4 |
| R-7 | 3 |
| R-8 | 3 |
| R-9 | 3 |
| R-10 | 1, 5 |
| R-11 | 4 |
| R-12 | 8 |
| R-13 | 5 |
| R-14 | 3, 5 |
| R-15 | 1, 7 |
| R-16 | 6 |
| R-17 | 6 |
| R-18 | 7 |
| R-19 | 1 |
| R-20 | 7 |
| R-21 | 3 |
| R-22 | 7 |
| R-23 | 9 |
| R-24 | 9 |
| R-25 | 9 |
| R-26 | 10 |
| R-27 | 10 |
| R-28 | 11 |
| R-29 | 12 |
| R-30 | 13 |
| R-31 | 14 |
| R-32 | 15 |
| R-33 | 16 |
| R-34 | 17 |
| R-35 | 17 |
| R-36 | 18 |

### Aspect Coverage

| Aspect | Covered By Tasks | N/A? |
|--------|-----------------|------|
| Code | 1, 2, 3, 4, 5, 6, 7, 8 | |
| Database | — | N/A — no new tables; MH in-memory, MC existing Redis, GC existing `media_handlers` table |
| Tests | 14, 15, 16 | |
| Observability | 10, 11, 12, 13 | |
| Deployment | 9 | |
| Operations | 17, 18 | |

### Specialist Task Summary

| Specialist | Tasks | Count |
|-----------|-------|-------|
| protocol | 1 | 1 |
| global-controller | 2 | 1 |
| meeting-controller | 4, 7, 8, 11, 15 | 5 |
| media-handler | 3, 5, 6, 10, 14 | 5 |
| infrastructure | 9 | 1 |
| observability | 12, 13 | 2 |
| test | 16 | 1 |
| operations | 17, 18 | 2 |
| auth-controller | — (opt-out, interface validated) | 0 |
| database | — (opt-out, interface validated) | 0 |
| security | — (cross-cutting criteria, no separate tasks) | 0 |

### Parallelization Opportunities

- **Phase 1** (all parallel): Tasks 1, 3, 9
- **Phase 2** (after 1): Tasks 2, 4, 5, 7 can all run in parallel
- **Phase 3** (after 1+3+5): Task 6; (after 4+5): Task 8
- **Phase 4** (after implementation): Tasks 10, 11, 14, 15 can run in parallel
- **Phase 5** (after metrics): Tasks 12, 13, 16 can run in parallel
- **Phase 6** (after dashboards): Tasks 17, 18

---

## Devloop Tracking

| # | Task | Devloop Output | Commit | Status |
|---|------|---------------|--------|--------|
| 1 | Proto: MhAssignment grpc_endpoint, remove MhRole, remove connection_token, RegisterMeeting, MediaCoordinationService, MediaConnectionFailed | `docs/devloop-outputs/2026-04-13-mh-quic-proto/` | `703f2ca` | Completed |
| 2 | GC: propagate grpc_endpoint, remove role assignment | `docs/devloop-outputs/2026-04-14-gc-propagate-grpc-endpoint/` | `27ea1b3` | Completed |
| 3 | MH: JWT validation + WebTransport server + connection handler + auth interceptor upgrade | `docs/devloop-outputs/2026-04-13-mh-quic-webtransport/main.md` | `e95e315` | Completed |
| 4 | MC: Redis gRPC endpoints + media_servers + MhClient | `docs/devloop-outputs/2026-04-14-mc-mh-grpc-client/` | `442ac92` | Completed |
| 5 | MH: RegisterMeeting handler + SessionManager | `docs/devloop-outputs/2026-04-14-mh-register-meeting/` | `e6b02b1` | Completed |
| 6 | MH: MC notification client | `docs/devloop-outputs/2026-04-15-mh-mc-client-notifications/` | `001ede8` | Completed |
| 7 | MC: MediaCoordinationService + MhConnectionRegistry + MediaConnectionFailed handler | `docs/devloop-outputs/2026-04-14-mc-media-coordination-service/` | `938f4cb` | Completed |
| 8 | MC: async RegisterMeeting trigger | `docs/devloop-outputs/2026-04-15-mc-async-register-meeting/` | `a45f746` | Completed |
| 9 | Infra: network policies + MH AC_JWKS_URL | docs/devloop-outputs/2026-04-13-mh-mc-network-policy/ | 93aa29b | Completed |
| 10 | MH: observability metrics | `docs/devloop-outputs/2026-04-17-mh-metrics/` | `c27cb11` | Completed |
| 11 | MC: MH coordination metrics | (absorbed by tasks 4, 7) | `442ac92`, `f03f788` | Completed |
| 12 | MH: WebTransport dashboard + alerts | | | Pending |
| 13 | MC: MH coordination dashboard + alerts | | | Pending |
| 14 | MH: integration tests | | | Pending |
| 15 | MC: join + coordination tests | `docs/devloop-outputs/2026-04-17-mc-join-coordination-tests/` | `12c7628` | Completed |
| 16 | Env-tests: MH connection E2E | | | Pending |
| 17 | MH + MC runbooks | | | Pending |
| 18 | Post-deploy monitoring checklist | | | Pending |

---

## Revisions

### Revision 1 — 2026-04-12

**Feedback**: Three architecture changes from original design:
1. Client authenticates to MH with meeting JWT (via JWKS), not per-participant connection tokens. MC does not call MH during join — no synchronous MH dependency in join path.
2. MC→MH coordination via async `RegisterMeeting` (once per meeting per MH, not per participant). Triggered after first participant joins.
3. MH→MC bidirectional coordination: MH notifies MC when participants connect/disconnect. MC maintains per-meeting connection registry for future media routing.

Additional clarifications:
- MH connections are active/active (not active/standby). Client connects to all assigned MHs in parallel.
- When all MH connections fail, client reports to MC via signaling. MH reallocation mechanism (MC→GC) deferred to future story.

**Changes**:
- Replaced connection token design with JWT validation at MH
- Removed synchronous MC→MH RegisterParticipant from join path
- Added RegisterMeeting RPC (async, once per meeting per MH)
- Added MediaCoordinationService (MH→MC notifications)
- Added MhConnectionRegistry in MC
- Added MediaConnectionFailed signaling message
- Added infrastructure tasks (network policies for MH→MC, AC_JWKS_URL config)
- Total tasks: 14 → 18

### Revision 2 — 2026-04-12

**Feedback**: Six design tightening requests:
1. Remove `MhRole` enum — active/active means no primary/backup distinction
2. Enforce RegisterMeeting arrival timeout — kick provisionally-accepted clients after 15s if RegisterMeeting doesn't arrive
3. Upgrade MC↔MH gRPC auth from structural-only to full JWKS-based cryptographic validation — no deferred auth
4. Remove `connection_token` from `MediaServerInfo` — don't ship dead fields
5. Fail the join if MH assignment data unavailable from Redis — join without media is not useful
6. Clarified connection flow: GC assigns MC+MH on first participant only; RegisterMeeting also first-participant-only

**Changes**:
- Added R-2 (remove MhRole), R-3 (remove connection_token), R-14 (RegisterMeeting timeout), R-21/R-22 (JWKS auth upgrade)
- Updated R-6 to fail join on missing MH data
- Updated MH design: provisional accept with 15s timeout, pending connection queue
- Updated auth design: both MH auth interceptor and MC gRPC auth use JWKS validation
- Updated proto design: reserve removed field numbers for wire compatibility
- Removed assumptions 3 (deferred auth), 4 (accept without RegisterMeeting indefinitely), 7 (primary/backup), 9 (empty connection_token), 10 (graceful degradation)
- Added connection flow documentation section
- Total tasks: 18 (unchanged — scope adjustments within existing tasks)
