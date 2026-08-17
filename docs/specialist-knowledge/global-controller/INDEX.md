# Global Controller Navigation

## Architecture & Design
- GC architecture, dual-server, MC/MH registry, load balancing -> ADR-0010
- API versioning, URL prefix conventions -> ADR-0004
- User auth, meeting access, join dependency chain -> ADR-0020
- Service-to-service auth (OAuth 2.0 client credentials); gRPC scopes, two-layer auth (JWKS + service_type routing) -> ADR-0003
- No-panic policy (DB row decoding must use `try_get`) -> ADR-0002
- Metric testability (four-tier pattern, MetricAssertion, per-failure-class mechanism table) -> ADR-0032

## Code Locations
- Entrypoint (HTTP + gRPC dual-server startup) -> `crates/gc-service/src/main.rs`
- Route definitions and AppState -> `crates/gc-service/src/routes/mod.rs:build_routes()`
- Configuration (env vars, thresholds) -> `crates/gc-service/src/config.rs:Config::from_env()`
- Error enum: HTTP status, `error.code`, `error_type` metric label -> `crates/gc-service/src/errors.rs:GcError`
- JWT validation (validate + validate_raw for gRPC failure_reason classification) -> `crates/gc-service/src/auth/jwt.rs`
- JWKS caching -> `crates/gc-service/src/auth/jwks.rs:JwksClient::get_key()`
- Claims extraction -> `crates/gc-service/src/auth/claims.rs`
- HTTP auth middleware (service + user token: require_auth, require_user_auth) -> `crates/gc-service/src/middleware/auth.rs`
- gRPC auth layer (Tower, two-layer: scope + service_type routing per ADR-0003) -> `crates/gc-service/src/grpc/auth_layer.rs:GrpcAuthLayer`
- gRPC failure_reason classifier for auth metrics -> `crates/gc-service/src/grpc/auth_layer.rs:classify_jwt_error()`
- Meeting handlers (create, join, guest-token, settings) -> `crates/gc-service/src/handlers/meetings.rs`
- Create (exhaustive `CreateMeetingOutcome` match, per-cause log + metric label) -> `crates/gc-service/src/handlers/meetings.rs:create_meeting()`
- Join/settings (user-auth, status allowlist, `home_org_id`, metrics) -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`, `update_meeting_settings()`
- Join response construction (shared by join + guest-token) -> `crates/gc-service/src/handlers/meetings.rs:JoinMeetingResponse::new()`
- McAssignment -> McAssignmentInfo conversion -> `crates/gc-service/src/handlers/meetings.rs:From<McAssignment>`
- MC gRPC service (register, heartbeat) -> `crates/gc-service/src/grpc/mc_service.rs:McService`
- MH gRPC service (register, load report) -> `crates/gc-service/src/grpc/mh_service.rs:MhService`
- MC assignment + load balancing -> `crates/gc-service/src/services/mc_assignment.rs:McAssignmentService`
- MH selection (active/active `handlers: Vec<MhAssignmentInfo>`; `grpc_endpoint` propagated DB→info→proto) -> `crates/gc-service/src/services/mh_selection.rs:MhSelectionService`
- MC gRPC client (`assign_meeting` RPC carrying per-handler `webtransport_endpoint` + `grpc_endpoint`) -> `crates/gc-service/src/services/mc_client.rs:McClientTrait`
- AC HTTP client (meeting/guest tokens) -> `crates/gc-service/src/services/ac_client.rs:AcClient`
- MC/MH repositories (register, heartbeat, staleness) -> `crates/gc-service/src/repositories/` (`meeting_controllers.rs`, `media_handlers.rs`)
- Meetings repository (create with limit check, audit log) -> `crates/gc-service/src/repositories/meetings.rs:MeetingsRepository::create_meeting_with_limit_check()`
- Assignment repository (weighted select, atomic assign, row mapper) -> `crates/gc-service/src/repositories/meeting_assignments.rs`
- Generic health checker loop -> `crates/gc-service/src/tasks/generic_health_checker.rs`
- Assignment cleanup (soft/hard delete) -> `crates/gc-service/src/tasks/assignment_cleanup.rs`
- Observability metrics (incl. join metrics) -> `crates/gc-service/src/observability/metrics.rs`
- Grafana dashboard -> `infra/grafana/dashboards/gc-overview.json`

## Meeting-refusal taxonomy
- Cause enum, metric label, SQL discriminant -> `crates/gc-service/src/repositories/meetings.rs:MeetingRefusal`, `metric_label()`, `from_discriminant()`
- Repository outcome type (Created / Refused / MeetingCodeTaken) -> `crates/gc-service/src/repositories/meetings.rs:CreateMeetingOutcome`
- Unique-violation classification by SQLSTATE + constraint -> `crates/gc-service/src/repositories/meetings.rs:classify_insert_error()`
- Row decoding (`try_get`, ADR-0002) -> `crates/gc-service/src/repositories/meetings.rs:map_row_to_meeting()`
- Cause -> HTTP mapping (single site) -> `crates/gc-service/src/errors.rs:impl From<MeetingRefusal> for GcError`
- Wire contract (error codes, statuses, retry guidance) -> `docs/API_CONTRACTS.md`
- Per-cause operator response -> `docs/runbooks/gc-incident-response.md` (Scenario 8)
- Alert rules -> `infra/docker/prometheus/rules/gc-alerts.yaml`, `docs/observability/alerts.md`

## Integration Seams
- GC <-> AC (OAuth token refresh) -> `crates/common/src/token_manager.rs:TokenReceiver`
- GC <-> AC (meeting/guest token issuance) -> `crates/gc-service/src/services/ac_client.rs`
- GC <-> AC shared types (MeetingTokenRequest, GuestTokenRequest, TokenResponse, ParticipantType, MeetingRole) -> `crates/common/src/meeting_token.rs`
- GC <-> MC (gRPC registration + heartbeat) -> `crates/gc-service/src/grpc/mc_service.rs`
- GC <-> MC (gRPC assignment RPC, requires service.write.mc per ADR-0003) -> `crates/gc-service/src/services/mc_client.rs`
- GC <-> MH (gRPC registration + load report) -> `crates/gc-service/src/grpc/mh_service.rs`
- MhAssignmentInfo -> MhAssignment proto mapping (mh_id, webtransport_endpoint, grpc_endpoint) -> `crates/gc-service/src/services/mc_client.rs:assign_meeting()`
- GC <-> Client (HTTP API /api/v1/*) -> `crates/gc-service/src/routes/mod.rs`
- UserClaims (user JWT claims type) -> `crates/common/src/jwt.rs:UserClaims`
- env-tests GC client fixture -> `crates/env-tests/src/fixtures/gc_client.rs`
- Org the env-test/browser suites create meetings in (`ENV_TEST_ORG_SUBDOMAIN`) -> `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`
- Per-run org provisioning (Layer 7 Phase 1h, operator lane) -> `scripts/layer7.sh`, `infra/kind/scripts/setup.sh --provision-org`

## Tests
- Meeting creation + the three refusal causes -> `crates/gc-service/tests/meeting_create_tests.rs`
- Meeting join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- MC/MH assignment tests -> `crates/gc-service/tests/mc_assignment_rpc_tests.rs`, `crates/gc-service/tests/meeting_assignment_tests.rs`
- Auth integration tests -> `crates/gc-service/tests/auth_tests.rs`
- Per-metric-cluster integration tests (ADR-0032) -> `crates/gc-service/tests/*_metrics_integration.rs`
- `error_type` label coverage -> `crates/gc-service/tests/meeting_creation_metrics_integration.rs`, `crates/gc-service/tests/errors_metric_integration.rs`
- Metric unit tests -> `crates/gc-service/src/observability/metrics.rs` (`#[cfg(test)] mod tests`)
- Test harness -> `crates/gc-test-utils/src/server_harness.rs`; per-crate JWT fixtures -> `crates/gc-service/tests/common/jwt_fixtures.rs`
- Shared meeting-token type unit tests -> `crates/common/src/meeting_token.rs` (`#[cfg(test)]`)
- Metrics catalog (creation + join, `error_type` value list) -> `docs/observability/metrics/gc-service.md`
