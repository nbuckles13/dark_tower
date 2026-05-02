# Global Controller Navigation

## Architecture & Design
- GC architecture, dual-server, MC/MH registry, load balancing -> ADR-0010
- API versioning, URL prefix conventions -> ADR-0004
- User auth, meeting access, join dependency chain -> ADR-0020
- Service-to-service auth (OAuth 2.0 client credentials) -> ADR-0003
- gRPC auth scopes, two-layer auth (JWKS + service_type routing) -> ADR-0003
- Metric testability (four-tier pattern, MetricAssertion, per-failure-class mechanism table) -> ADR-0032

## Code Locations
- Entrypoint (HTTP + gRPC dual-server startup) -> `crates/gc-service/src/main.rs`
- Route definitions and AppState -> `crates/gc-service/src/routes/mod.rs:build_routes()`
- Configuration (env vars, thresholds) -> `crates/gc-service/src/config.rs:Config::from_env()`
- Error types -> `crates/gc-service/src/errors.rs`
- JWT validation (validate + validate_raw for gRPC failure_reason classification) -> `crates/gc-service/src/auth/jwt.rs`
- JWKS caching -> `crates/gc-service/src/auth/jwks.rs:JwksClient::get_key()`
- Claims extraction -> `crates/gc-service/src/auth/claims.rs`
- HTTP auth middleware (service + user token: require_auth, require_user_auth) -> `crates/gc-service/src/middleware/auth.rs`
- gRPC auth layer (Tower, two-layer: scope + service_type routing per ADR-0003) -> `crates/gc-service/src/grpc/auth_layer.rs:GrpcAuthLayer`
- gRPC failure_reason classifier for auth metrics -> `crates/gc-service/src/grpc/auth_layer.rs:classify_jwt_error()`
- Meeting handlers (create, join, guest-token, settings) -> `crates/gc-service/src/handlers/meetings.rs`
- Join/settings (user-auth, status allowlist, metrics) -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`, `update_meeting_settings()`
- Join response construction (shared by join + guest-token) -> `crates/gc-service/src/handlers/meetings.rs:JoinMeetingResponse::new()`
- McAssignment -> McAssignmentInfo conversion -> `crates/gc-service/src/handlers/meetings.rs:From<McAssignment>`
- MC gRPC service (register, heartbeat) -> `crates/gc-service/src/grpc/mc_service.rs:McService`
- MH gRPC service (register, load report) -> `crates/gc-service/src/grpc/mh_service.rs:MhService`
- MC assignment + load balancing -> `crates/gc-service/src/services/mc_assignment.rs:McAssignmentService`
- MH selection (active/active `handlers: Vec<MhAssignmentInfo>`, no primary/backup; `grpc_endpoint` propagated DB→info→proto) -> `crates/gc-service/src/services/mh_selection.rs:MhSelectionService`
- MC gRPC client (`assign_meeting` RPC carrying per-handler `webtransport_endpoint` + `grpc_endpoint`) -> `crates/gc-service/src/services/mc_client.rs:McClientTrait`
- AC HTTP client (meeting/guest tokens) -> `crates/gc-service/src/services/ac_client.rs:AcClient`
- MC/MH repositories (register, heartbeat, staleness) -> `crates/gc-service/src/repositories/` (`meeting_controllers.rs`, `media_handlers.rs`)
- Meetings repository (create with limit check, audit log) -> `crates/gc-service/src/repositories/meetings.rs:MeetingsRepository`
- Assignment repository (weighted select, atomic assign, row mapper) -> `crates/gc-service/src/repositories/meeting_assignments.rs`
- Generic health checker loop -> `crates/gc-service/src/tasks/generic_health_checker.rs`
- Assignment cleanup (soft/hard delete) -> `crates/gc-service/src/tasks/assignment_cleanup.rs`
- Observability metrics (incl. join metrics) -> `crates/gc-service/src/observability/metrics.rs`
- Grafana dashboard -> `infra/grafana/dashboards/gc-overview.json`

## Integration Seams
- GC <-> AC (OAuth token refresh) -> `crates/common/src/token_manager.rs:TokenReceiver`
- GC <-> AC (meeting/guest token issuance) -> `crates/gc-service/src/services/ac_client.rs`
- GC <-> AC shared types (MeetingTokenRequest, GuestTokenRequest, TokenResponse, ParticipantType, MeetingRole) -> `crates/common/src/meeting_token.rs`
- home_org_id is always Uuid (not Option) — set to user_org_id for same-org, user_org_id for cross-org -> `handlers/meetings.rs:400`
- GC <-> MC (gRPC registration + heartbeat) -> `crates/gc-service/src/grpc/mc_service.rs`
- GC <-> MC (gRPC assignment RPC, requires service.write.mc per ADR-0003) -> `crates/gc-service/src/services/mc_client.rs`
- GC <-> MH (gRPC registration + load report) -> `crates/gc-service/src/grpc/mh_service.rs`
- MhAssignmentInfo -> MhAssignment proto mapping (mh_id, webtransport_endpoint, grpc_endpoint; `MhRole` removed, field 3 reserved) -> `crates/gc-service/src/services/mc_client.rs:assign_meeting()`
- GC <-> Client (HTTP API /api/v1/*) -> `crates/gc-service/src/routes/mod.rs`
- UserClaims (user JWT claims type) -> `crates/common/src/jwt.rs:UserClaims`
- env-tests GC client fixture -> `crates/env-tests/src/fixtures/gc_client.rs`

## Tests
- Meeting creation tests -> `crates/gc-service/tests/meeting_create_tests.rs`
- Meeting join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- MC/MH assignment tests -> `crates/gc-service/tests/mc_assignment_rpc_tests.rs`, `crates/gc-service/tests/meeting_assignment_tests.rs`
- Auth integration tests -> `crates/gc-service/tests/auth_tests.rs`
- Test harness -> `crates/gc-test-utils/src/server_harness.rs`; per-crate JWT fixtures -> `crates/gc-service/tests/common/jwt_fixtures.rs`
- Metrics catalog (creation + join) -> `docs/observability/metrics/gc-service.md`

### Metric testability (ADR-0032)
- Uncovered sites (~21/186): `main.rs:127` token-refresh closure (Cat B extraction); `handlers/meetings.rs` error branches (validation direct-call / pg-error-code real-DB+fault / non-DB repo-trait); `mh_selection.rs` deeply-nested; `grpc/auth_layer.rs:250` wrapper-only. GC spawns at `main.rs:180/199/207` are lifecycle, not fire-and-forget (no accept-loop-style backfill needed). SLO: 11% → <6% by 2026-07 → <3% by 2026-10
- ADR-0032 Step 5 closeout (2026-04-27, commit `48f1250`): all 25 uncovered GC metrics drained → `validate-metric-coverage.sh` GREEN. Per-cluster `MetricAssertion`-backed integration tests at `crates/gc-service/tests/*_metrics_integration.rs` (13 cluster files); in-src `#[cfg(test)] mod tests` at `crates/gc-service/src/observability/metrics.rs`. Cat B extraction: `record_token_refresh_metrics` parallel sibling pattern (1:1 with MH/MC). Fixture consolidation: `tests/common/jwt_fixtures.rs` consumed by all 3 pre-existing GC integration test files + 13 new cluster files.

### Step 5 Patterns Worth Carrying Forward
- 4-cell gauge adjacency matrix (happy / partial+zero-fill / empty / short-circuit) -> `tests/registered_controllers_metrics_integration.rs`
- Shared metric family + discriminator label (vs `*_user_*`/`*_guest_*` fork) -> `handlers/meetings.rs:512-526`, `tests/meeting_join_metrics_integration.rs:132-145`
- Orphan-recording-site (all tests wrapper-Cat-C when fn has zero prod callers) -> `tests/db_metrics_integration.rs`, `src/repositories/mod.rs:21`
- Catalog aspirational-vs-enforced for unbounded-source labels -> `docs/observability/metrics/gc-service.md:351-365`, `auth_layer.rs:241`
- Per-crate `tests/common/{mod.rs,jwt_fixtures.rs}` fixture consolidation (attack helpers stay file-local) -> `crates/gc-service/tests/common/jwt_fixtures.rs`

### Join endpoint test coverage (R-18)
- Auth (401×4) / access (403, 200 cross-org) / status (404×2, 200×2) / failure (503×2, 404) / success / AC-failure variant -> `crates/gc-service/tests/meeting_tests.rs`
- AC-failure server: `TestMeetingServer::spawn_with_ac_failure()`; same-org home_org_id regression: `test_same_org_join_sends_home_org_id_equal_to_user_org_id`
- Shared type unit tests (15) -> `crates/common/src/meeting_token.rs` (#[cfg(test)])
