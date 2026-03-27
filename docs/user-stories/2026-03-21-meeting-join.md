# User Story: Meeting Join Flow

**Date**: 2026-03-21
**Status**: Ready for Review
**Participants**: auth-controller, global-controller, meeting-controller, media-handler, database, protocol, infrastructure, security, test, observability, operations

## Story

As a **logged-in user**, I want **to join a meeting, be assigned to an MC, and connect to that MC** so that **I can participate in video conferences**.

**Scope**: JoinRequest/JoinResponse exchange + basic ongoing bidirectional signaling (ParticipantJoined/ParticipantLeft notifications only). NO mute, layout, or other signaling message types. NO media (MH connection is a separate story).

## Requirements

- R-1: GC join endpoint (`GET /api/v1/meetings/:code`) uses user auth middleware (`require_user_auth` / `UserClaims`) instead of service auth (from: security, global-controller)
- R-2: GC returns meeting token + MC WebTransport endpoint to the client in join response (from: global-controller) — **already implemented, verified**
- R-3: GC verifies user org membership (from `UserClaims.org_id`, no DB lookup) and meeting permissions; uses status allowlist (`active`/`scheduled` only) (from: global-controller, security)
- R-4: `MeetingTokenClaims` and `GuestTokenClaims` structs are published in `crates/common/src/jwt.rs` so MC can parse validated meeting JWTs (from: auth-controller)
- R-5: MC accepts WebTransport connections from clients (HTTP/3 over QUIC with TLS 1.3) on port 4433 (from: meeting-controller)
- R-6: MC validates meeting tokens (JWT) against AC JWKS endpoint with 5-min cached TTL, using shared JWKS/JWT validation code from common crate (from: meeting-controller, security)
- R-7: MC processes JoinRequest, creates participant session with session binding tokens (correlation_id + binding_token per ADR-0023), returns JoinResponse with participant roster and media server info (from: meeting-controller)
- R-8: MC bridges ConnectionActor to MeetingActor for bidirectional signaling — scoped to ParticipantJoined/ParticipantLeft notifications only (from: meeting-controller)
- R-9: ParticipantsRepository tracks active participants per meeting with capacity checks (from: database)
- R-10: Meeting status transitions from `scheduled` to `active` on first join, with audit logging (from: database)
- R-11: MC health probes (liveness + readiness) enabled in K8s deployment manifest (from: infrastructure)
- R-12: GC records join metrics: `gc_meeting_join_total` (counter), `gc_meeting_join_duration_seconds` (histogram), `gc_meeting_join_failures_total` (counter by error_type) (from: global-controller, observability)
- R-13: MC records join metrics: `mc_webtransport_connections_total`, `mc_jwt_validations_total`, `mc_session_joins_total`, `mc_session_join_duration_seconds` (from: meeting-controller, observability)
- R-14: GC join dashboard panels added to `gc-overview.json` (join rate, latency p95, failure breakdown, success rate) (from: observability)
- R-15: MC join dashboard panels added to `mc-overview.json` + alert rules for WebTransport failures, token validation failures, session join failures (from: observability)
- R-16: TLS certificate generation for MC WebTransport in dev environment + K8s Secret volume mount (from: infrastructure)
- R-17: UDP port mapping for QUIC in Kind cluster config (from: infrastructure)
- R-18: GC join integration tests covering auth middleware, failure paths (AC down, MC unavailable), and success path (from: global-controller)
- R-19: MC join integration tests covering WebTransport connection, JWT validation, JoinRequest processing (from: meeting-controller)
- R-20: End-to-end env-tests for join flow in Kind cluster, including WebTransport client connection to MC with JoinRequest/Response and ParticipantJoined verification (from: test)
- R-21: MC runbook covers join flow failure scenarios: WebTransport connection failures, JWT validation failures, Redis/session failures (from: operations)
- R-22: Post-deploy monitoring checklist and smoke test for MC join metrics (from: operations)
- R-23: JWKS client and generic JWT validation logic extracted to `crates/common/` to avoid code duplication between GC and MC (from: auth-controller, global-controller)

---

## Architecture Validation

**Result**: PASS (all 11 specialists confirmed)

All changes fit within existing architecture:
- New WebTransport server module in MC (Phase 6g per ADR-0023)
- New auth module in MC for JWKS/JWT validation (Phase 6c per ADR-0023)
- Route migration in GC (existing middleware, existing handler)
- New claims types in common crate (extending existing pattern)
- JWKS client extraction to common (DRY refactor, no new patterns)
- Existing proto definitions sufficient (signaling.proto, internal.proto)

**Opt-outs (justified)**:
- **media-handler**: MH is passive in the join flow. GC selects MHs and includes them in the assignment, but MH code doesn't change. Interface validated: `MhAssignment` in `internal.proto` and `MediaServerInfo` in `signaling.proto` are correct.
- **protocol**: Existing protobuf definitions (`JoinRequest`, `JoinResponse`, `ParticipantJoined`, `ParticipantLeft`, `ClientMessage`, `ServerMessage`) are complete for this story's scope. Interface validated against `proto/signaling.proto` and `proto/internal.proto`.

---

## Design

### auth-controller

**Task 1 — Claims types**: Add `MeetingTokenClaims` and `GuestTokenClaims` structs to `crates/common/src/jwt.rs`. These are the claim structures that AC embeds in meeting/guest tokens (per ADR-0020). MC needs these to deserialize validated JWTs.

Fields for `MeetingTokenClaims`: `sub` (user UUID), `meeting_id`, `org_id`, `participant_type` (member/external), `role` (host/participant), `capabilities`, `iat`, `exp`, `jti`. PII-redacted Debug impl.

Fields for `GuestTokenClaims`: `sub` (guest UUID), `meeting_id`, `guest_name`, `capabilities`, `iat`, `exp`, `jti`. PII-redacted Debug impl.

**Task 7 — JWKS extraction (R-23)**: Extract `JwksClient` and generic `JwtValidator` from `crates/gc-service/src/auth/` into `crates/common/`. Depends on task 1 (claims types must be in common first for validator tests). The common code provides:
- `Jwk`, `JwksResponse` structs — JWK key representation
- `JwksClient` — fetches `.well-known/jwks.json`, `Arc<RwLock<Option<CachedJwks>>>` cache with configurable TTL, `get_key(kid)`, `force_refresh()`, `clear_cache()`
- `JwtValidator` — wraps `JwksClient` + `clock_skew_seconds`, provides `validate<T: DeserializeOwned>(token) -> Result<T, JwtError>`
- `JwtError` enum — `TokenTooLarge`, `MalformedToken`, `MissingKid`, `IatTooFarInFuture`, `InvalidSignature`, `KeyNotFound`, `ServiceUnavailable(String)` (extends existing `JwtValidationError`)
- Wiremock-based integration tests for cache hit/miss/expiry/refresh and verify_token edge cases

New dependency in `common/Cargo.toml`: `jsonwebtoken = { workspace = true }` (already in workspace root). `reqwest` and `tokio` already present.

GC and MC both import from common. Service-specific validation (meeting_id match, token_type check, scope checks) remains in each service. Each service maps `JwtError` to their own error type via `From<JwtError>`.

### global-controller

**Changes**:
1. **Auth migration** (`routes/mod.rs`): Move `GET /api/v1/meetings/:code` and `PATCH /api/v1/meetings/:id/settings` from `protected_routes` to `user_auth_routes`
2. **Handler fix** (`handlers/meetings.rs`): Change `join_meeting` and `update_meeting_settings` signatures from `Extension<Claims>` to `Extension<UserClaims>`. Replace `get_user_org_id()` DB lookup with `UserClaims.org_id`. Remove dead `get_user_org_id()` function.
3. **Status allowlist**: Replace blocklist check (`!= "cancelled" && != "ended"`) with allowlist (`== "active" || == "scheduled"`)
4. **Join metrics** (`observability/metrics.rs`): Add `record_meeting_join(status, error_type, duration)` following `record_meeting_creation` pattern. Histogram buckets extended to 5s (join includes MC assignment + AC token request). Error type labels bounded to 7 values.
5. **Common JWKS conversion** (R-23): Replace `gc-service/src/auth/jwks.rs` and JWT validation with imports from `common`. GC-specific scope checks remain in `gc-service/src/auth/jwt.rs`.

**Interface note**: `update_meeting_settings` has the same auth bug and is fixed in the same task.

### meeting-controller

**New modules**:
- `crates/mc-service/src/auth/` — JWT validator (`jwt.rs`) using common `JwksClient` + `validate_token<MeetingTokenClaims>`, plus MC-specific config (`ac_jwks_url`)
- `crates/mc-service/src/webtransport/` — WebTransport server (`server.rs`) + connection handler (`handler.rs`)

**Layer architecture** (bottom-up):
1. Actor system (existing) — `MeetingActor::handle_join()`, session binding tokens, participant broadcast
2. JWT validation (new) — Uses common `JwksClient` + `validate_token`, adds MC-specific validation (meeting_id match, token_type check)
3. Connection handler (new) — Per-connection task: read JoinRequest from bidirectional stream, validate JWT, look up meeting via `MeetingControllerActorHandle`, call `MeetingActorHandle::join()`, serialize JoinResponse, spawn ConnectionActor wired to WebTransport stream
4. WebTransport server (new) — `wtransport` crate, TLS 1.3 termination, accept loop spawning connection tasks, CancellationToken wiring

**File changes**:
- New: `src/auth/mod.rs`, `src/auth/jwt.rs`
- New: `src/webtransport/mod.rs`, `src/webtransport/server.rs`, `src/webtransport/handler.rs`
- Modified: `src/config.rs` — add `tls_cert_path`, `tls_key_path`, `ac_jwks_url`
- Modified: `src/main.rs` — replace `TODO (Phase 6g)` at line 310 with WebTransport server startup
- Modified: `src/actors/connection.rs` — wire WebTransport stream to `handle_send()`/`handle_update()`/`graceful_close()` (replacing Phase 6g TODOs)
- Modified: `src/observability/metrics.rs` — add join flow metrics
- Modified: `Cargo.toml` — add `jsonwebtoken`, `wiremock` (dev); common crate already provides `reqwest` via JWKS client

**Signaling bridge scope**: Only `ParticipantJoined` and `ParticipantLeft` are sent over the wire. Other `ParticipantStateUpdate` variants (MuteChanged, Disconnected, Reconnected) are logged but not serialized.

**JWT validation chain**: size check -> extract kid -> JWKS lookup -> EdDSA signature verify -> exp check -> iat check (with clock skew) -> meeting_id match -> token_type check ("meeting" or "guest")

**max_participants enforcement**: Each MC enforces `max_participants` locally using its own participant count plus the last-known counts from peer MCs (received via the cross-region gRPC peer connections per ADR-0010 Section 5). Participant count updates are exchanged as part of the roster sync that peers already need for ParticipantJoined/ParticipantLeft relay. Enforcement is eventually consistent — simultaneous joins on different MCs at nearly the same instant may briefly exceed the limit, which is acceptable for a business constraint.

### database

**Changes**:
1. New migration + `ParticipantsRepository` — `count_active_participants(pool, meeting_id)`, `add_participant(pool, meeting_id, user_id, participant_type, role)`, `remove_participant(pool, meeting_id, user_id)`
2. `MeetingsRepository::activate_meeting(pool, meeting_id)` — transitions `scheduled` -> `active` on first join, with audit log entry

### infrastructure

**Changes**:
1. TLS cert generation script addition to `scripts/generate-dev-certs.sh` + `mc-service-tls` Secret in `infra/services/mc-service/`
2. Kind config: UDP port mapping for port 4433 (QUIC/WebTransport)
3. MC deployment: Enable commented-out liveness/readiness probes, add volume mount for TLS Secret, add `AC_JWKS_URL` env var to configmap (for MC JWT validation, task 9)

### security (cross-cutting — no separate tasks)

**Critical findings addressed**:
- R-1 fixes privilege escalation: service tokens could access join endpoint (now requires user token)
- R-6 ensures meeting tokens are cryptographically validated at MC (EdDSA + JWKS)
- Session binding tokens use HMAC-SHA256 with HKDF key derivation (constant-time comparison, per ADR-0023)
- All random values generated via CSPRNG (`SystemRandom` / `OsRng`)

### observability

**GC instrumentation**: `gc_meeting_join_total`, `gc_meeting_join_duration_seconds`, `gc_meeting_join_failures_total` — dashboard panels in `gc-overview.json`, alert rules for high failure rate (>5% for 5m, P2) and high latency (p95 >2s for 5m, P3).

**MC instrumentation**: `mc_webtransport_connections_total`, `mc_jwt_validations_total`, `mc_session_joins_total`, `mc_session_join_duration_seconds` — dashboard panels in `mc-overview.json`, alert rules for WebTransport handshake failures, token validation failures, session join failures.

### test

**Env-tests**: End-to-end join flow in Kind cluster — create meeting then join, 401 without token, 404 unknown meeting, guest join. Integration tests are owned by their respective domain specialists (GC tests by global-controller, MC tests by meeting-controller).

### operations

**Three new MC runbook scenarios**:
- Scenario 8: WebTransport connection failures (TLS cert, UDP, QUIC listener)
- Scenario 9: Token validation failures (JWKS, clock skew, key rotation)
- Scenario 10: Redis/session failures (Redis down, binding token secret, connection pool)

**Post-deploy checklist**: 30-min/2-hour/4-hour/24-hour checks for join success rate, latency, WebTransport handshake rate, token validation rate. Rollback criteria: join error rate >5% for 10m, WebTransport failure >5% for 5m.

---

## Cross-Cutting Requirements

### Security Checklist
1. **Authentication**: User JWT (EdDSA, 1hr TTL) at GC; Meeting JWT (EdDSA, 15min TTL) at MC
2. **Authorization**: Org membership checked at GC; meeting-scoped token at MC; `allow_external_participants` setting enforced
3. **Input validation**: JWT size check (8KB), status allowlist, protobuf deserialization bounds
4. **Data protection**: PII-redacted Debug on all claims types; no PII in metrics labels
5. **Error handling**: Generic error messages to clients; detailed errors in structured logs only
6. **Cryptography**: EdDSA (Ed25519) for JWTs; HMAC-SHA256 + HKDF for session binding; CSPRNG for all random values

### Test Checklist
1. **Unit tests**: JWT validation edge cases, session binding token generation, metrics recording
2. **Integration tests**: GC join handler (auth, failure paths) owned by global-controller; MC connection handler (JWT, signaling) owned by meeting-controller
3. **Env-tests**: End-to-end join flow in Kind cluster (owned by test specialist)
4. **Test infrastructure**: GC harness updated for user auth, MC test utils for mock WebTransport

### Observability Checklist
1. **Business metrics**: Join success/failure counters at both GC and MC
2. **Dashboard panels**: 4 GC panels + 4 MC panels in Grafana
3. **Structured logs**: Join events with correlation IDs (no PII)
4. **Alert rules**: 2 GC alerts + 3 MC alerts with runbook_url annotations

### Operations Checklist
1. **New failure modes**: 3 new MC scenarios (WebTransport, token validation, Redis/session)
2. **Rollback**: Additive changes, GC first then MC; Redis state expires via TTL
3. **Monitoring**: 6 key metrics for first 24 hours post-deploy
4. **Runbook**: 3 new scenarios + post-deploy checklist + expanded smoke test

---

## Assumptions

| # | Assumption | Made By | Reason Not Blocked |
|---|-----------|---------|-------------------|
| 1 | `wtransport` crate is already in workspace deps | meeting-controller | Can be added in MC task if not |
| 2 | MC WebTransport port is 4433 (matching ADR-0023) | infrastructure | Standard QUIC port, configurable via env var |
| 3 | JWKS cache TTL of 5 minutes is sufficient for MC | meeting-controller | Matches GC pattern; key rotation has grace period per ADR-0008 |
| 4 | Self-signed certs acceptable for dev/Kind environment | infrastructure | Production uses real certs via cert-manager |
| 5 | `max_participants` check at GC can be deferred if DB task delays | global-controller | GC core join flow works without it; capacity is also checked at MC actor level |
| 6 | `max_participants` enforcement is eventually consistent across MCs | meeting-controller | MC-to-MC peer connections exchange participant counts; simultaneous cross-MC joins may briefly exceed limit. Acceptable tradeoff for architectural simplicity. |

## Clarification Questions

| # | Question | Asked By | Status | Answer |
|---|---------|----------|--------|--------|
| 1 | What should a user client see after connecting to MC? | team-lead | Answered | JoinResponse + ParticipantJoined/Left only. No mute, layout, or other signaling. |

---

## Implementation Plan

### Task Dependency Graph

```
Tasks 2-6 can start in parallel (no mutual dependencies)
Task 1 also starts immediately; task 7 waits for task 1

   1 (AC claims) ──> 7 (JWKS→common) ──> 8 (GC→common) ────┐
                                     ──> 9 (MC JWT) ────────┤
   2 (DB participants) ─── optional ────────────────────────┤
   3 (DB activation) ──── optional ─────────────────────────┤
   5 (Infra TLS+UDP) ──────────────────────────────────────┤──> 10 (MC WebTransport) ──> 11 (MC metrics)
   6 (Infra health probes) ────────────────────────────────┤          │                       │
                                                            │          │                       │
   4 (GC fix) ──> 12 (GC dashboard) ──────────────────────┤          │                       │
          4, 8 ──> 14 (GC integration tests) ─────────────┤          │                       │
                                                            │          │                       │
                                                11 ──> 13 (MC dash) ─┤                       │
                                                10 ──> 15 (MC tests) ┤                       │
                                             4, 10 ──> 16 (env-tests)┤                       │
                                            11, 13 ──> 17 (runbook) ─┤                       │
                                             4, 11 ──> 18 (checklist)┘
```

### Ordered Task List

| # | Task | Specialist | Deps | Covers |
|---|------|-----------|------|--------|
| 1 | Add `MeetingTokenClaims` and `GuestTokenClaims` to `crates/common/src/jwt.rs` | auth-controller | — | code |
| 2 | Implement `ParticipantsRepository` with migration for participant tracking + capacity checks | database | — | migration, code |
| 3 | Implement meeting activation (`scheduled`->`active` on first join) + audit logging | database | — | migration, code |
| 4 | Fix GC join/settings auth middleware (`UserClaims`), add status allowlist, add `record_meeting_join` metrics | global-controller | — | code, metrics |
| 5 | Add TLS cert generation to dev scripts + MC K8s Secret volume mount + Kind UDP port mapping for 4433 | infrastructure | — | deploy |
| 6 | Enable MC liveness/readiness health probes in `deployment.yaml` + add `AC_JWKS_URL` env var to MC configmap/deployment (needed by task 9) | infrastructure | — | deploy |
| 7 | Extract JWKS client + generic JWT validator (`JwtValidator::validate<T>`) from GC to `crates/common/`, with `JwtError` enum and wiremock tests (R-23) | auth-controller | 1 | code |
| 8 | Convert GC auth to use common JWKS/JWT code (delete `gc-service/src/auth/jwks.rs`, thin wrapper mapping `JwtError` to `GcError`) | global-controller | 7 | code |
| 9 | Implement MC JWT validation using common `JwksClient` + `JwtValidator::validate<MeetingTokenClaims>` + MC-specific config (`ac_jwks_url`) | meeting-controller | 7 | code |
| 10 | Implement MC WebTransport server + join flow connection handler (wtransport TLS, accept loop, JoinRequest/Response, ParticipantJoined/Left bridge, CancellationToken wiring) | meeting-controller | 5, 9 | code |
| 11 | Add MC join flow observability metrics (WebTransport connections, JWT validations, session joins, latency histogram) | meeting-controller | 10 | code, metrics |
| 12 | Add GC join dashboard panels + alert rules + update metrics catalog | observability | 4 | dashboard, alerts, docs |
| 13 | Add MC join dashboard panels + alert rules + update metrics catalog | observability | 11 | dashboard, alerts, docs |
| 14 | GC join integration tests + test harness updates for user auth | global-controller | 4, 8 | tests |
| 15 | MC join integration tests (WebTransport, JWT, signaling bridge) | meeting-controller | 10 | tests |
| 16 | Join flow end-to-end env-tests in Kind cluster (including WebTransport client E2E: connect to MC, JoinRequest/Response, ParticipantJoined) | test | 4, 10 | tests |
| 17 | Add MC runbook scenarios 8-10 (WebTransport, token validation, Redis/session) + TOC update | operations | 11, 13 | docs |
| 18 | Add post-deploy monitoring checklist + expand smoke test 5 for join flow | operations | 4, 11 | docs |
| 19 | Move JWT auth from JoinRequest protobuf to HTTP/3 CONNECT headers (enables off-box auth termination), update `signaling.proto`, handler, client SDK, amend ADR-0023 | meeting-controller | 10 | code, proto, docs |

### Specialist Task Summary

| Specialist | Tasks | Count |
|-----------|-------|-------|
| auth-controller | 1, 7 | 2 |
| database | 2, 3 | 2 |
| global-controller | 4, 8, 14 | 3 |
| meeting-controller | 9, 10, 11, 15 | 4 |
| infrastructure | 5, 6 | 2 |
| observability | 12, 13 | 2 |
| test | 16 | 1 |
| operations | 17, 18 | 2 |
| security | — (cross-cutting criteria) | 0 |
| protocol | — (opt-out, interface validated) | 0 |
| media-handler | — (opt-out, interface validated) | 0 |

### Parallelization Opportunities

- **Phase 1** (all parallel): Tasks 1, 2, 3, 4, 5, 6
- **Phase 2** (after 1): Task 7 (JWKS extraction — critical path)
- **Phase 3** (after 7): Tasks 8 and 9 can run in parallel; task 12 can start after task 4
- **Phase 4** (after 5+9): Task 10 (MC WebTransport + handler — critical path)
- **Phase 5** (after 10): Task 11; tasks 14 (needs 4+8), 15 (needs 10), 16 (needs 4+10)
- **Phase 6** (after 11): Tasks 13, 17, 18 can run in parallel

---

## Devloop Tracking

| # | Task | Devloop Output | Commit | Status |
|---|------|---------------|--------|--------|
| 1 | Add MeetingTokenClaims/GuestTokenClaims to common | docs/devloop-outputs/2026-03-21-meeting-claims | 3a22a51 | Completed |
| 2 | Implement ParticipantsRepository + migration | docs/devloop-outputs/2026-03-21-participants-repo | 3c58e10 | Completed |
| 3 | Implement meeting activation + audit logging | docs/devloop-outputs/2026-03-21-meeting-activation | 15e7b15 | Completed |
| 4 | Fix GC join auth + add join metrics | docs/devloop-outputs/2026-03-23-gc-join-auth-metrics | 47bfb59 | Completed |
| 5 | Infra: TLS certs + MC Secret + Kind UDP | docs/devloop-outputs/2026-03-23-infra-tls-udp | a09ce18 | Completed |
| 6 | Infra: Enable MC health probes | | | Pending |
| 7 | Extract JWKS client + JWT validation to common | docs/devloop-outputs/2026-03-26-jwks-extraction | 375be71 | Completed |
| 8 | Convert GC auth to common JWKS/JWT | docs/devloop-outputs/2026-03-26-jwks-extraction | aa99fee | Completed (in task 7) |
| 9 | MC JWT validation on common code | docs/devloop-outputs/2027-03-27-mc-jwt-validation | b3a4c8d | Completed |
| 10 | MC WebTransport server + join flow handler | docs/devloop-outputs/2026-03-27-mc-webtransport | dcee11c | Completed |
| 11 | MC join flow observability metrics | docs/devloop-outputs/2026-03-27-mc-join-metrics | 7213b17 | Completed |
| 12 | GC join dashboard + alerts + catalog | | | Pending |
| 13 | MC join dashboard + alerts + catalog | | | Pending |
| 14 | GC join integration tests | | | Pending |
| 15 | MC join integration tests | | | Pending |
| 16 | Join flow env-tests | | | Pending |
| 17 | MC runbook scenarios 8-10 | | | Pending |
| 18 | Post-deploy checklist + smoke test | | | Pending |

---

## Revisions

### Revision 1 — 2026-03-21

**Feedback**: Three adjustments requested:
1. Integration tests (tasks 12+13) should be owned by domain specialists, not test specialist. Only env-tests stay with test.
2. JWKS client + JWT validation should be extracted to common crate to avoid duplicating GC code. Creates 3 tasks: common extraction, GC conversion, MC implementation on top.
3. Add max_participants eventual consistency assumption and design note for MC peer-based enforcement.

**Changes**:
- Reassigned GC integration tests to global-controller (task 14), MC integration tests to meeting-controller (task 15)
- Added task 7 (extract JWKS to common, auth-controller), task 8 (convert GC, global-controller), task 9 (MC on common, meeting-controller)
- Added R-23 (JWKS extraction requirement)
- Added assumption #6 (max_participants eventual consistency)
- Added max_participants enforcement subsection to meeting-controller design
- Updated R-18/R-19 ownership from test to domain specialists
- Total tasks: 16 -> 18
