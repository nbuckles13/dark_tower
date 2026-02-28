# User Story: Create a Meeting

**Date**: 2026-02-24
**Status**: Ready
**Participants**: auth-controller, global-controller, meeting-controller, media-handler, database, protocol, infrastructure, security, test, observability, operations

## Story

As an **authenticated user**, I want **to create a meeting** so that **I can host a video conference with other participants**.

## Requirements

- [ ] R-1: `POST /api/v1/meetings` endpoint creates a meeting scoped to the authenticated user's organization, returns 201 with meeting details (meeting_id, meeting_code, display_name, status, settings, created_at). (from: global-controller, protocol)
- [ ] R-2: Endpoint requires a valid user JWT (validated via AC JWKS). GC uses shared `common::jwt::UserClaims` to deserialize user tokens with `org_id` and `roles`. (from: security, auth-controller)
- [ ] R-3: Role enforcement — user token must contain at least `user` role (or `admin`/`org_admin`). Return 403 if missing. (from: security)
- [ ] R-4: Meeting code generated server-side with 72 bits entropy (12 base62 chars) using CSPRNG. Uniqueness enforced by DB constraint; collision retried (up to 3 attempts). (from: global-controller, protocol, security)
- [ ] R-5: `join_token_secret` generated server-side using CSPRNG, stored in meetings row for later join flows. (from: global-controller, security)
- [ ] R-6: Organization concurrent meeting limit enforced — count of `status IN ('scheduled', 'active')` meetings below `organizations.max_concurrent_meetings`. Atomic check+insert. Returns 403 if exceeded. (from: global-controller, database, security)
- [ ] R-7: Input validation — `display_name` required 1-255 chars trimmed; `max_participants` 2 to org's `max_participants_per_meeting` (default 100); secure defaults: `enable_e2e_encryption=true`, `require_auth=true`, `recording_enabled=false`, `allow_guests=false`, `allow_external_participants=false`, `waiting_room_enabled=true`. Request uses `#[serde(deny_unknown_fields)]`. (from: global-controller, security)
- [ ] R-8: Response excludes `join_token_secret` and internal fields. Standard error responses (400/401/403/409/500) with generic messages. (from: security, protocol)
- [ ] R-9: Audit log entry with `action='meeting_created'`. (from: database)
- [ ] R-10: Business metrics — `gc_meetings_created_total{status, error_type}` counter (7 bounded error_type values: none, bad_request, unauthorized, forbidden, db_error, code_collision, internal) and `gc_meeting_creation_duration_seconds{status}` histogram. Endpoint normalization for `/api/v1/meetings`. Structured logging (success: info with meeting_id, code, user_id, org_id; failure: warn — excluding PII/secrets). Tracing span `gc.meeting.create`. (from: observability)
- [ ] R-11: 3 dedicated alert rules — `GCMeetingCreationFailureRate` (>5% for 5m, warning), `GCMeetingCreationLatencyHigh` (p95 >500ms for 5m, warning), `GCMeetingCreationStopped` (zero traffic vs prior hour, 15m, critical). In `gc-alerts.yaml`. (from: observability)
- [ ] R-12: Dashboard panel "Meeting Creation Rate by Status" + "Meeting Creation Latency (P50/P95/P99)" on GC Overview. Metrics catalog updated in `docs/observability/metrics/gc-service.md`. (from: observability)
- [ ] R-13: GC NetworkPolicy egress rule to MC pods on TCP 50052. (from: infrastructure)
- [ ] R-14: GC ServiceMonitor uncommented for Prometheus scraping. (from: operations)
- [ ] R-15: Unit and integration tests (~16-20) covering validation, auth, role enforcement, DB persistence, code generation, error mapping, secure defaults, audit logging, metrics. (from: test)
- [ ] R-16: Env-test coverage — `GcClient::create_meeting()` fixture + 4-6 env-test scenarios (authenticated create, round-trip joinable, unauthenticated rejection, invalid body, unique codes). (from: test)
- [ ] R-17: Runbook scenarios in `gc-incident-response.md` (Scenario 8: Limit Exhaustion, Scenario 9: Code Collision) and post-deploy smoke test + monitoring checklist in `gc-deployment.md`. Alert catalog updated in `docs/observability/alerts.md`. (from: operations)

---

## Architecture Validation

**Result**: PASS (all 11 specialists confirmed)

---

## Design

### auth-controller

**Task 0 — Shared UserClaims + scope fix:**

Move `UserClaims` struct from `crates/ac-service/src/crypto/mod.rs:362-378` to `crates/common/src/jwt.rs` (alongside existing `ServiceClaims`). Add `pub use common::jwt::UserClaims;` alias in AC's crypto/mod.rs (same pattern as ServiceClaims). `verify_user_jwt()` and `sign_user_jwt()` stay in AC — they have AC-specific dependencies (AcError, metrics).

Add `"internal:meeting-token"` to `GlobalController::default_scopes()` in `crates/ac-service/src/models/mod.rs:304`. This scope is required by AC's `POST /api/v1/auth/internal/meeting-token` endpoint but is currently missing from GC's default registration — a latent bug affecting the join flow.

### global-controller

**New `require_user_auth` middleware** in `crates/gc-service/src/middleware/auth.rs`:
- Imports `common::jwt::UserClaims`
- Makes GC's `verify_token()` generic: `fn verify_token<T: DeserializeOwned>()` — signature verification and key lookup are claims-type-independent
- Adds `validate_user()` method to `JwtValidator` that calls `verify_token::<UserClaims>()`
- Dedicated middleware that deserializes into `UserClaims` and injects into extensions
- Existing `require_auth` middleware unchanged (continues using `Claims` for service tokens)

**New handler**: `create_meeting` in `crates/gc-service/src/handlers/meetings.rs`
- Extracts `UserClaims` from `require_user_auth` middleware — `org_id` is a required `String` (not Option), `roles` is a required `Vec<String>`
- Validates `roles` contains `user`/`admin`/`org_admin` (R-3)
- Parses `CreateMeetingRequest` with `#[serde(deny_unknown_fields)]` (R-7)
- Validates input: display_name 1-255 chars trimmed, max_participants 2..=org limit
- Applies secure defaults: require_auth=true, e2e_encryption=true, allow_guests=false, allow_external=false, waiting_room=true, recording=false
- Generates meeting_code: 12 base62 chars via `ring::rand::SystemRandom` with 3 retry attempts on collision
- Generates join_token_secret: 32 CSPRNG bytes, hex-encoded
- Calls `MeetingsRepository::create_meeting_with_limit_check()` (atomic CTE)
- Calls `MeetingsRepository::log_audit_event()` (fire-and-forget)
- Records `gc_meetings_created_total` + `gc_meeting_creation_duration_seconds` metrics
- Returns 201 `CreateMeetingResponse` (excludes join_token_secret)

**New repository**: `crates/gc-service/src/repositories/meetings.rs`
- `create_meeting_with_limit_check()`: Single CTE query that atomically counts active/scheduled meetings for the org, validates against `max_concurrent_meetings`, caps `max_participants` at org's `max_participants_per_meeting`, and inserts meeting row with `RETURNING *`. Returns `Option<MeetingRow>` — None means limit exceeded.
- `log_audit_event()`: INSERT into `audit_logs` with action='meeting_created'. Fire-and-forget (failure logged but doesn't block creation).

**New models** in `crates/gc-service/src/models/mod.rs`:
- `CreateMeetingRequest`: display_name, max_participants, settings (all optional booleans)
- `CreateMeetingResponse`: meeting_id, meeting_code, display_name, status, all settings, created_at (separate from JoinMeetingResponse)

**Route** in `crates/gc-service/src/routes/mod.rs`:
- `post("/api/v1/meetings", handlers::create_meeting)` with `require_user_auth` middleware

**Metrics** in `crates/gc-service/src/observability/metrics.rs`:
- `record_meeting_creation(status, error_type, duration)` function
- Histogram buckets: [0.005, 0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.300, 0.500, 1.000]
- Endpoint normalization: `"/api/v1/meetings" => "/api/v1/meetings"`

### meeting-controller

N/A — MC assignment happens during join, not create. Interface validated: MC gRPC port 50052 confirmed for R-13 NetworkPolicy; join_token_secret is not consumed by MC.

### media-handler

N/A — MH involved in join/media flows only. All existing MH interfaces unaffected.

### Database Changes

**No migrations needed.** All columns exist in the `meetings` and `audit_logs` tables.

Query designs (implemented by global-controller specialist):
- Atomic CTE for `create_meeting_with_limit_check()` — counts active/scheduled meetings, validates limit, caps max_participants, inserts with `RETURNING *`
- Fire-and-forget audit log INSERT into `audit_logs`

### Protocol Changes

N/A — No protobuf changes needed. Existing `AssignMeetingWithMhRequest`/`Response` and `MeetingControllerService.AssignMeetingWithMh` RPC are sufficient for join-time assignment.

Notes for GC implementer:
- Create a separate `CreateMeetingResponse` struct (don't reuse `JoinMeetingResponse`)
- `require_auth` default changed to `true` (API_CONTRACTS.md to be updated post-implementation)
- Org-scoped meeting code uniqueness (`UNIQUE(org_id, meeting_code)`) — collision retry within same org only

---

## Cross-Cutting Requirements

### Security

**Authentication**: User JWT validated via AC JWKS. GC imports shared `common::jwt::UserClaims` — no optional field hacks, `org_id` and `roles` are required fields. Dedicated `require_user_auth` middleware for user-authenticated endpoints. Existing service-token endpoints unchanged.

**Authorization**: Minimum `user` role required (R-3). Org concurrent meeting limit enforced (R-6). Multi-tenant isolation via `org_id` from token.

**Input Validation**: `#[serde(deny_unknown_fields)]`, display_name trimmed and length-checked, max_participants bounded by org limit. Clients cannot supply meeting codes.

**Data Protection**: `join_token_secret` excluded from response. No PII in metrics labels. `UserClaims` Debug impl (in common) redacts sub, email, jti.

**Error Handling**: Generic error messages. Code collision retries transparent to client (500 if all 3 fail, not "collision" message).

**Cryptography**: CSPRNG via `ring::rand::SystemRandom` for meeting_code and join_token_secret. No new signing/encryption.

### Observability

- **Metrics**: `gc_meetings_created_total{status, error_type}` (counter, 7 error_type values), `gc_meeting_creation_duration_seconds{status}` (histogram, buckets 5ms-1s)
- **Logs**: Success: info with meeting_id, meeting_code, user_id, org_id. Failure: warn with user_id, error. Excluded: display_name, email, join_token_secret.
- **Traces**: `#[instrument(skip_all, name = "gc.meeting.create")]` span on handler
- **Dashboards**: 2 panels on GC Overview — creation rate by status + latency percentiles
- **Alerts**: 3 rules in gc-alerts.yaml (failure rate, latency, zero-traffic)

### Test

- **Unit Tests** (~8-10): Request validation, meeting code format, join_token_secret generation, secure defaults, deny_unknown_fields, error mapping
- **Integration Tests** (~8-10): Happy path, DB persistence, auth scenarios (no token, expired, valid), role enforcement, response excludes secrets, audit log entry, metrics incremented
- **Env-Tests** (~5): Authenticated create, round-trip joinable by code, unauthenticated rejection, invalid body, unique codes
- **Test Infrastructure**: GcClient fixture extended with create_meeting() + raw_create_meeting() + request/response types

### Deployment

- GC NetworkPolicy: Add egress rule to MC pods on TCP 50052 (R-13)
- GC ServiceMonitor: Uncomment spec section for Prometheus scraping (R-14)
- No new Dockerfiles, manifests, or infrastructure components needed

### Operations

- **Runbook updates**: Scenario 8 (Meeting Creation Limit Exhaustion) and Scenario 9 (Meeting Code Collision) added to `docs/runbooks/gc-incident-response.md`. Alert mapping updated in `docs/observability/runbooks.md`.
- **Monitoring/Alerts**: 3 dedicated alerts (failure rate >5% warning, latency p95 >500ms warning, zero-traffic critical). Post-deploy monitoring checklist (30min/2hr/4hr/24hr steps) with 1-hour observation window.
- **Rollback**: Standard `kubectl rollout undo deployment/gc-service`. No data migration needed — created meetings remain as inert rows. Rollback criteria: error rate >5% for 10min, p95 >200ms for 5min, pod restarts >1/hr.

---

## Assumptions

| # | Assumption | Made By | Reason Not Blocked |
|---|-----------|---------|-------------------|
| 1 | Both 'scheduled' and 'active' meetings count toward org concurrent limit | database | Conservative approach — prevents scheduling unlimited future meetings |
| 2 | Meeting code format is plain 12 base62 chars (no separators like abc-defg-hijk) | global-controller | ADR-0020 specifies "12 base62 chars" without formatting; cosmetic change can be added later |
| 3 | Optional `scheduled_start_time` field accepted in request (NULL = ad-hoc meeting) | database | meetings table already has this column; natural to expose it |
| 4 | Audit log failure does not fail meeting creation (fire-and-forget pattern) | database | Matches AC pattern in token_service.rs; audit is secondary to the business operation |
| 5 | org_id from user token claims is authoritative for meeting org scoping | global-controller | ADR-0020 embeds org_id in user tokens at issuance time |

## Clarification Questions

| # | Question | Asked By | Status | Answer |
|---|---------|----------|--------|--------|
| 1 | Should display_name length check use bytes or chars for Unicode? | security | Answered | Either acceptable; byte-length is stricter (documented) |
| 2 | Should post-deploy monitoring extend to 1 hour for new endpoint? | operations | Answered | Yes — new endpoint warrants longer monitoring |

---

## Implementation Plan

| # | Task | Specialist | Dependencies | Covers | Status |
|---|------|-----------|--------------|--------|--------|
| 0 | Move UserClaims to common::jwt, add internal:meeting-token to GC default scopes, update AC to use shared type | auth-controller | — | R-2 (shared type) | Pending |
| 1 | Fix GC NetworkPolicy to allow MC egress and enable GC ServiceMonitor | infrastructure | — | R-13, R-14 | Pending |
| 2 | Implement POST /api/v1/meetings endpoint with require_user_auth middleware (using common::UserClaims), meetings repository (atomic CTE), role enforcement, meeting code generation, metrics code, and unit/integration tests | global-controller | 0 | R-1, R-2, R-3, R-4, R-5, R-6, R-7, R-8, R-9, R-10, R-15 | Pending |
| 3 | Add meeting creation alert rules, dashboard panels, and metrics catalog | observability | 2 | R-11, R-12 | Pending |
| 4 | Add create-meeting env-test scenarios and GcClient fixture | test | 2 | R-16 | Pending |
| 5 | Add meeting creation runbook scenarios and post-deploy checklist | operations | 2, 3 | R-17 | Pending |

### Parallelization

- Tasks 0 and 1 run in parallel (no mutual dependencies)
- Task 2 starts after Task 0 completes
- Tasks 3 and 4 run in parallel after Task 2
- Task 5 starts after Task 3

### Requirements Coverage

| Req | Covered By Tasks |
|-----|-----------------|
| R-1 | 2 |
| R-2 | 0, 2 |
| R-3 | 2 |
| R-4 | 2 |
| R-5 | 2 |
| R-6 | 2 |
| R-7 | 2 |
| R-8 | 2 |
| R-9 | 2 |
| R-10 | 2 |
| R-11 | 3 |
| R-12 | 3 |
| R-13 | 1 |
| R-14 | 1 |
| R-15 | 2 |
| R-16 | 4 |
| R-17 | 5 |

### Aspect Coverage

| Aspect | Covered By Tasks | N/A? |
|--------|-----------------|------|
| Code | 0, 2 | |
| Database | 2 | |
| Tests | 2, 4 | |
| Observability | 2, 3 | |
| Deployment | 1 | |
| Operations | 5 | |

---

## Devloop Tracking

| # | Task | Devloop Output | PR | Status |
|---|------|---------------|-----|--------|
| 0 | Move UserClaims to common::jwt + GC default scopes fix | `docs/devloop-outputs/2026-02-25-userclaims-common-jwt/` | | Done |
| 1 | Fix GC NetworkPolicy MC egress + enable ServiceMonitor | `docs/devloop-outputs/2026-02-25-gc-networkpolicy-servicemonitor/` | | Done |
| 2 | Implement POST /api/v1/meetings endpoint + repository + middleware + metrics code + tests | `docs/devloop-outputs/2026-02-27-create-meeting-endpoint/` | | Done |
| 3 | Add meeting creation alert rules, dashboard panels, and metrics catalog | `docs/devloop-outputs/2026-02-28-meeting-creation-alerts/` | | Done |
| 4 | Add create-meeting env-test scenarios and GcClient fixture | `docs/devloop-outputs/2026-02-28-create-meeting-env-tests/` | | Done |
| 5 | Add meeting creation runbook scenarios and post-deploy checklist | `docs/devloop-outputs/2026-02-28-meeting-creation-runbooks/` | | Done |

---

## Revisions

