# Global Controller Integration Guide

What other services need to know when integrating with the Global Controller.

---

## Integration: JWT Validation Flow
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/`, `crates/global-controller/src/middleware/auth.rs`

End-to-end JWT validation in GC:
1. Client sends: `Authorization: Bearer <token>`
2. Middleware extracts token, calls `JwtValidator::validate(token)`
3. JwtValidator:
   - Checks token size (< 8KB)
   - Extracts kid from header
   - Fetches JWK from cached JWKS (5 min TTL)
   - Validates JWK (kty=OKP, alg=EdDSA)
   - Verifies EdDSA signature using jsonwebtoken
   - Validates iat claim (with clock skew tolerance)
4. On success: Claims injected into request.extensions
5. Handler calls `req.extensions().get::<Claims>()`

---

## Integration: Protected Routes Pattern
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/routes/mod.rs`

Protected routes use `middleware::from_fn_with_state`:
```rust
.route(
    "/v1/me",
    get(handlers::me::get_me)
        .layer(middleware::from_fn_with_state(
            Arc::new(auth_state),
            require_auth,
        )),
)
```

The middleware chain:
- Layer wraps handler
- Middleware runs before handler (extracts/validates token)
- Handler receives Request with claims in extensions
- If middleware returns Err (401), handler never runs

---

## Integration: Claims Structure
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/claims.rs`

JWT claims struct from AC tokens:
```
sub: String       # Subject (user ID)
exp: i64          # Expiration time (Unix timestamp)
iat: i64          # Issued at (Unix timestamp)
scopes: Vec<String> # Authorization scopes
```

Handlers extract via request extensions. Claims implement Debug with redacted `sub` field to prevent leaking user IDs in logs.

---

## Integration: Testing with Mocked JWKS
**Added**: 2026-01-14
**Related files**: `crates/global-controller/tests/auth_tests.rs`

Integration tests use wiremock to mock AC's JWKS endpoint:
- Start mock server with `wiremock::MockServer::new()`
- Register JWKS endpoint response
- Pass mock URL to GC config
- GC fetches and caches JWKS from mock
- Tests verify auth behavior without depending on real AC

---

## Integration: AC Internal Token Endpoints
**Added**: 2026-01-15, **Updated**: 2026-02-11
**Related files**: `crates/global-controller/src/services/ac_client.rs`, `crates/common/src/token_manager.rs`

GC calls AC internal endpoints for meeting tokens:
- `POST /api/v1/auth/internal/meeting-token` - Issue token for authenticated user joining meeting
- `POST /api/v1/auth/internal/guest-token` - Issue token for guest participant

GC authenticates using OAuth 2.0 client credentials. At startup, TokenManager acquires initial token from `POST /api/v1/auth/service/token` using GC_CLIENT_ID/GC_CLIENT_SECRET and auto-refreshes before expiration (background task). AcClient uses `self.token_receiver.token().expose_secret()` for Authorization header. Request body includes `subject_user_id`/`guest_id`, `meeting_id`, `meeting_org_id`, `participant_type`, `role`, `capabilities`, `ttl_seconds`. Response contains signed JWT for WebTransport connection to MC (default 900s TTL).

---

## Integration: Meeting API Endpoints
**Added**: 2026-01-15
**Related files**: `crates/global-controller/src/routes/mod.rs`, `crates/global-controller/src/handlers/meetings.rs`

Meeting API endpoints:
- `GET /v1/meetings/{code}` - Join meeting (authenticated, returns meeting token)
- `POST /v1/meetings/{code}/guest-token` - Get guest token (public, requires captcha)
- `PATCH /v1/meetings/{id}/settings` - Update meeting settings (host only)

Join endpoint returns AC-issued meeting token for WebTransport connection. Guest endpoint allows unauthenticated access with captcha verification (placeholder). Settings endpoint allows host to toggle allow_guests, allow_external_participants, waiting_room_enabled.

---

## Integration: gRPC Auth Layer for MC Communication
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/auth_layer.rs`, `crates/global-controller/src/grpc/mc_service.rs`

MCs authenticate to GC gRPC endpoints using AC-issued JWT tokens:
1. MC obtains service token from AC (Client Credentials flow)
2. MC sends gRPC request with `authorization: Bearer <token>` metadata
3. GC's `AuthLayer` extracts token, validates async via `JwksClient`
4. Validated claims injected into request extensions
5. gRPC handlers extract claims: `req.extensions().get::<Claims>()`

The same `JwksClient` and validation logic used for HTTP auth is reused for gRPC. This ensures consistent security policy across transport protocols.

---

## Integration: MC Registration Flow
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/mc_service.rs`, `crates/global-controller/src/repositories/meeting_controllers.rs`

MC registration with GC:
1. MC calls `RegisterMc` RPC with: hostname, grpc_port, region, version, max_capacity
2. GC validates input (character whitelist, length limits)
3. GC upserts into `meeting_controllers` table (atomic insert-or-update)
4. GC returns registration_id (UUID) for future reference

On MC restart, re-registration updates existing row (matched by hostname). Health status set to `healthy` on registration. GC assigns MC to appropriate region pool for load balancing.

---

## Integration: Heartbeat Protocols
**Added**: 2026-01-20
**Related files**: `crates/global-controller/src/grpc/mc_service.rs`

Two heartbeat types for MC health reporting:

**FastHeartbeat** (10s interval):
- Request: `mc_id`, `current_participants`, `max_capacity`
- Updates: `last_heartbeat`, capacity fields
- Use case: Load balancing needs fresh capacity data

**ComprehensiveHeartbeat** (30s interval):
- Request: All of fast heartbeat plus: `cpu_usage`, `memory_usage`, `bandwidth_usage`, `error_rate`, `latency_p50/p95/p99`
- Updates: All fields including metrics
- Use case: Observability dashboards, alerting

Both maintain the `last_heartbeat` timestamp. Health checker marks MCs unhealthy if no heartbeat received within staleness threshold (default 60s).

---

## Integration: Health Checker Tasks (MC + MH, Generic)
**Added**: 2026-01-20, **Updated**: 2026-02-12
**Related files**: `crates/global-controller/src/tasks/generic_health_checker.rs`, `crates/global-controller/src/tasks/health_checker.rs`, `crates/global-controller/src/tasks/mh_health_checker.rs`

Both MC and MH health checkers use a shared generic health checker loop (`start_generic_health_checker`). Each is a thin wrapper that provides:
- The repository staleness-check function as a closure
- `entity_name` as a `&'static str` parameter (e.g., `"controllers"`, `"handlers"`)
- `.instrument(tracing::info_span!("gc.task.health_checker"))` chaining for span context
- Lifecycle logs (start/stop) with literal `target:` values (outside the span)

Common behavior:
- Runs every 5 seconds (`DEFAULT_CHECK_INTERVAL_SECONDS`)
- Marks stale entities unhealthy via repository method
- Staleness threshold configurable (default 60s via `mc_staleness_threshold_seconds`)
- Graceful shutdown via `CancellationToken`
- Error resilience: logs DB errors but continues loop
- All loop logs include `entity = entity_name` structured field for filtering

Other services querying healthy MCs/MHs should filter: `WHERE health_status = 'healthy'`. The health checkers are the single source of truth for health status transitions.

---

## Integration: Meeting-to-MC Assignment via Load Balancing
**Added**: 2026-01-21
**Related files**: `crates/global-controller/src/services/mc_assignment.rs`, `crates/global-controller/src/repositories/meeting_assignments.rs`

When a participant joins a meeting, GC assigns an MC using weighted random selection:
1. Query healthy MCs with available capacity (`current_participants < max_capacity`)
2. Apply weighted random selection using CSPRNG (weight based on available capacity)
3. Atomic assignment via INSERT ON CONFLICT (handles concurrent joins)
4. Return assigned MC's connection info (hostname, grpc_port)

Prerequisites for tests: Register at least one healthy MC before attempting to join a meeting. The legacy `endpoint` column in `meeting_controllers` is NOT NULL, so test helpers must populate it even though it's deprecated.

---

## Integration: Assignment Cleanup Lifecycle
**Added**: 2026-01-23
**Related files**: `crates/global-controller/src/tasks/assignment_cleanup.rs`, `crates/global-controller/src/repositories/meeting_assignments.rs`

Meeting assignments follow a soft-delete then hard-delete lifecycle:

**Soft-delete (end_assignment)**: Sets `ended_at` timestamp. Triggered by:
- `end_stale_assignments()`: Assignments where MC is unhealthy AND assigned > N hours ago
- Direct `end_assignment()` call when meeting ends normally

**Hard-delete (cleanup_old_assignments)**: Removes row entirely. Only deletes assignments where `ended_at` is older than retention period (default 7 days).

Background task `start_assignment_cleanup()` runs both operations periodically. Uses batch limits to prevent large transactions. Important: stale detection requires MC health status join - only ends assignments where the MC has become unhealthy, not just old assignments.

---

## Integration: GC-to-MC Assignment RPC Flow with MH Selection
**Added**: 2026-01-24, **Updated**: 2026-02-11
**Related files**: `crates/global-controller/src/services/mc_client.rs`, `crates/global-controller/src/services/mc_assignment.rs`, `crates/global-controller/src/handlers/meetings.rs`

Meeting join triggers MC assignment with MH selection (ADR-0010 Section 4a):

1. GC selects MHs for the meeting via `MhSelectionService::select_mhs_for_meeting()` (primary + backup in different AZs)
2. GC selects MC via load balancer (weighted random by available capacity)
3. GC calls `McClient::assign_meeting(mc_endpoint, meeting_id, mh_assignments, gc_id)` - notifies MC BEFORE DB write
4. On MC acceptance: GC persists assignment in `meeting_assignments` table
5. On MC rejection: GC retries with different MC (up to 3 attempts)

**Key change from legacy**: `assign_meeting()` now includes MH assignments as parameter. The old function signature (no MH selection, no MC RPC) was removed. All handlers now use the new flow.

**Client configuration**: MC client uses `TokenReceiver` for dynamic OAuth tokens, eager channel connection (`.connect().await`), and cached channels per endpoint (Arc<RwLock<HashMap<String, Channel>>>). Inject `MockMcClient::accepting()` for tests.

---

## Integration: MH (Media Handler) Registry
**Added**: 2026-02-11
**Related files**: `crates/global-controller/src/grpc/mh_service.rs`, `crates/global-controller/src/services/mh_selection.rs`, `crates/global-controller/src/tasks/mh_health_checker.rs`

MHs register with GC via gRPC similar to MC registration:

**Registration Flow**:
1. MH calls `RegisterMh` RPC with: hostname, grpc_port, webtransport_port, region, availability_zone, version, max_capacity
2. GC validates input (character whitelist, length limits)
3. GC upserts into `media_handlers` table (atomic insert-or-update by hostname)
4. GC returns registration_id (UUID)

**Health Reporting**:
- MH sends periodic load reports: current_sessions, max_sessions, bandwidth metrics
- GC health checker marks MHs unhealthy if no report within staleness threshold (default 60s)

**MH Selection**:
- GC selects primary + backup MHs per meeting in different AZs (anti-affinity)
- Load-based selection: prefer MHs with lower `current_sessions / max_sessions` ratio
- Region-aware: prefer MHs in same region as MC

**Database Schema**: `media_handlers` table mirrors `meeting_controllers` with additional `availability_zone` field for anti-affinity.

---

## Integration: Observability Metrics Layering
**Added**: 2026-02-09, **Updated**: 2026-02-09
**Related files**: `crates/global-controller/src/observability/metrics.rs`, `crates/global-controller/src/repositories/*.rs`, `crates/global-controller/src/services/mc_assignment.rs`, `crates/global-controller/src/services/mh_selection.rs`

Metrics are recorded at different layers depending on what they measure:

**Repository Layer** (DB operations):
- `record_db_query(operation, status, duration)` - Called in each repository method
- Operations: `get_healthy_assignment`, `get_candidate_mcs`, `atomic_assign`, `register_mc`, `update_heartbeat`, etc.
- Captures actual database latency without service-layer overhead

**Service Layer** (business operations):
- `record_mc_assignment(status, rejection_reason, duration)` - Called in `assign_meeting_with_mh()`
- `record_mh_selection(status, has_backup, duration)` - Called in `select_mhs_for_meeting()`
- Captures end-to-end operation time including RPCs, retries, and DB writes
- Status values: `success`, `rejected`, `error`

**Out of Scope** (cross-crate dependency - TD-GC-001):
- Token refresh metrics were removed from `metrics.rs` because TokenManager lives in `common` crate
- See gotchas.md for cross-crate metrics challenge and solution options

**Dashboard panels**: Grafana dashboard at `infra/grafana/dashboards/gc-overview.json` has panels for each metric type.

---
