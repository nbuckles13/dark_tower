# Global Controller Navigation

## Architecture & Design
- GC architecture, dual-server, MC/MH registry, load balancing -> ADR-0010
- API versioning, URL prefix conventions -> ADR-0004
- User auth, meeting access, join dependency chain -> ADR-0020
- Service-to-service auth (OAuth 2.0 client credentials) -> ADR-0003

## Code Locations
- Entrypoint (HTTP + gRPC dual-server startup) -> `crates/gc-service/src/main.rs`
- Route definitions and AppState -> `crates/gc-service/src/routes/mod.rs:build_routes()`
- Configuration (env vars, thresholds) -> `crates/gc-service/src/config.rs:Config::from_env()`
- Error types -> `crates/gc-service/src/errors.rs`
- JWT validation -> `crates/gc-service/src/auth/jwt.rs:JwtValidator::validate()`
- JWKS caching -> `crates/gc-service/src/auth/jwks.rs:JwksClient::get_key()`
- Claims extraction -> `crates/gc-service/src/auth/claims.rs`
- HTTP auth middleware (service token) -> `crates/gc-service/src/middleware/auth.rs:require_auth()`
- HTTP auth middleware (user token) -> `crates/gc-service/src/middleware/auth.rs:require_user_auth()`
- gRPC auth layer (Tower) -> `crates/gc-service/src/grpc/auth_layer.rs:GrpcAuthLayer`
- Meeting handlers (create, join, guest-token, settings) -> `crates/gc-service/src/handlers/meetings.rs`
- Join/settings (user-auth, status allowlist, metrics) -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`, `update_meeting_settings()`
- MC gRPC service (register, heartbeat) -> `crates/gc-service/src/grpc/mc_service.rs:McService`
- MH gRPC service (register, load report) -> `crates/gc-service/src/grpc/mh_service.rs:MhService`
- MC assignment + load balancing -> `crates/gc-service/src/services/mc_assignment.rs:McAssignmentService`
- MH selection (primary + backup AZ) -> `crates/gc-service/src/services/mh_selection.rs:MhSelectionService`
- MC gRPC client (assign_meeting RPC) -> `crates/gc-service/src/services/mc_client.rs:McClientTrait`
- AC HTTP client (meeting/guest tokens) -> `crates/gc-service/src/services/ac_client.rs:AcClient`
- MC repository (register, heartbeat, staleness) -> `crates/gc-service/src/repositories/meeting_controllers.rs`
- MH repository -> `crates/gc-service/src/repositories/media_handlers.rs`
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
- GC <-> MC (gRPC assignment RPC) -> `crates/gc-service/src/services/mc_client.rs`
- GC <-> MH (gRPC registration + load report) -> `crates/gc-service/src/grpc/mh_service.rs`
- GC <-> Client (HTTP API /api/v1/*) -> `crates/gc-service/src/routes/mod.rs`
- UserClaims (user JWT claims type) -> `crates/common/src/jwt.rs:UserClaims`
- env-tests GC client fixture -> `crates/env-tests/src/fixtures/gc_client.rs`

## Tests
- Meeting creation tests -> `crates/gc-service/tests/meeting_create_tests.rs`
- Meeting join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- Auth integration tests -> `crates/gc-service/tests/auth_tests.rs`
- Test harness -> `crates/gc-test-utils/src/server_harness.rs`
- Test token helpers -> `crates/gc-service/tests/meeting_tests.rs:TestUserClaims`
- Metrics catalog (creation + join) -> `docs/observability/metrics/gc-service.md`

### Join endpoint test coverage (R-18)
- Auth: 401 no token, 401 expired, 401 service token, 401 wrong key/alg/tampered
- Access: 403 cross-org denied, 200 cross-org allowed (allow_external=true)
- Status: 404 cancelled, 404 ended, 200 scheduled, 200 active
- Failure: 503 AC unavailable, 503 no MC available, 404 meeting not found
- Success: token + MC assignment (mc_id, grpc_endpoint, webtransport_endpoint)
- AC failure variant: `TestMeetingServer::spawn_with_ac_failure()` (500 on meeting-token)
- Regression: same-org join home_org_id invariant -> `meeting_tests.rs:test_same_org_join_sends_home_org_id_equal_to_user_org_id`
- Shared type unit tests (15 tests) -> `crates/common/src/meeting_token.rs` (#[cfg(test)])
