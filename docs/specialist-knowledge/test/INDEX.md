# Test Navigation

## Architecture & Design
- Integration testing strategy -> `docs/decisions/adr-0005-integration-testing-strategy.md`
- Fuzz testing -> `docs/decisions/adr-0006-fuzz-testing-strategy.md`
- Integration test infrastructure -> `docs/decisions/adr-0009-integration-test-infrastructure.md`
- Environment integration tests -> `docs/decisions/adr-0014-environment-integration-tests.md`
- Validation pipeline (guards, coverage) -> `docs/decisions/adr-0024-agent-teams-workflow.md`
- Coverage thresholds -> `.codecov.yml`
- Client architecture (4-tier testing, test-utils, flaky policy) -> ADR-0028

## Code Locations: AC Service
- Integration tests -> `crates/ac-service/tests/integration/`
- Fault injection tests -> `crates/ac-service/tests/fault_injection/`
- Fuzz targets -> `crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs`
- Test harness (HTTP seam) -> `crates/ac-test-utils/src/server_harness.rs`
- Token builders -> `crates/ac-test-utils/src/token_builders.rs`

## Code Locations: GC Service
- Auth integration tests (HTTP layer, wiremock JWKS) -> `crates/gc-service/tests/auth_tests.rs`
- Auth thin wrapper tests (From<JwtError> for GcError) -> `crates/gc-service/src/auth/jwt.rs:tests`
- Meeting join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- Meeting creation tests -> `crates/gc-service/tests/meeting_create_tests.rs`
- Participant & activation tests -> `crates/gc-service/tests/participant_tests.rs`
- Meeting assignment tests -> `crates/gc-service/tests/meeting_assignment_tests.rs`
- Test token helpers (TestUserClaims, TestClaims) -> `crates/gc-service/tests/meeting_tests.rs:TestUserClaims`
- Join handler (user-auth) -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`
- Guest token handler (public) -> `crates/gc-service/src/handlers/meetings.rs:get_guest_token()`
- Settings handler (user-auth, host-only) -> `crates/gc-service/src/handlers/meetings.rs:update_meeting_settings()`
- Join metrics -> `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- GC metrics tests -> `crates/gc-service/src/observability/metrics.rs:tests`
- GC overview dashboard (join panels: ids 35-38) -> `infra/grafana/dashboards/gc-overview.json`
- GC alert rules (join: GCHighJoinFailureRate, GCHighJoinLatency) -> `infra/docker/prometheus/rules/gc-alerts.yaml`
- GC metrics catalog -> `docs/observability/metrics/gc-service.md`
- Route definitions (public, user-auth, service-auth) -> `crates/gc-service/src/routes/mod.rs`
- Activation repo -> `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`
- Audit event logging -> `crates/gc-service/src/repositories/meetings.rs:log_audit_event()`
- Test harness (HTTP seam) -> `crates/gc-test-utils/src/server_harness.rs`

## Code Locations: MC Service
- Auth module (McJwtValidator wrapper) -> `crates/mc-service/src/auth/mod.rs`
- Meeting/guest token validation (wiremock + Ed25519) -> `crates/mc-service/src/auth/mod.rs:tests::test_validate_*_token_*`
- Token confusion (bidirectional: meeting-as-guest, guest-as-meeting, wrong token_type) -> `crates/mc-service/src/auth/mod.rs:tests`
- From<JwtError> for McError (7 variants, ServiceUnavailable->Internal) -> `crates/mc-service/src/errors.rs:tests::test_jwt_error_to_mc_error_*`
- Config ac_jwks_url (scheme validation) -> `crates/mc-service/src/config.rs:tests::test_ac_jwks_url_*`
- Config TLS paths (fail-fast validation) -> `crates/mc-service/src/config.rs:tests::test_from_vars_*tls*`
- Controller actor tests -> `crates/mc-service/src/actors/controller.rs:tests`
- Meeting actor tests (join, leave, reconnect, mute, grace period) -> `crates/mc-service/src/actors/meeting.rs:tests`
- ParticipantActor tests (spawn, send, ping, close, stream wiring) -> `crates/mc-service/src/actors/participant.rs:tests`
- Session binding tests (HMAC, correlation ID, expiration) -> `crates/mc-service/src/actors/session.rs:tests`
- WebTransport encoding tests (encode_participant_update) -> `crates/mc-service/src/webtransport/handler.rs:tests`
- WebTransport connection tests (build_join_response) -> `crates/mc-service/src/webtransport/connection.rs:tests`
- MC metrics tests (unit + DebuggingRecorder integration) -> `crates/mc-service/src/observability/metrics.rs:tests`
- MC join flow metrics + catalog -> `crates/mc-service/src/observability/metrics.rs:record_webtransport_connection()`, catalog: `docs/observability/metrics/mc-service.md`
- MC alert rules (join: MCHighJoinFailureRate, MCHighWebTransportRejections, MCHighJwtValidationFailures, MCHighJoinLatency) -> `infra/docker/prometheus/rules/mc-alerts.yaml`
- GC integration tests -> `crates/mc-service/tests/gc_integration.rs`
- Heartbeat tests -> `crates/mc-service/tests/heartbeat_tasks.rs`
- Health state & router tests -> `crates/mc-service/src/observability/health.rs:health_router()`
- Mock Redis -> `crates/mc-test-utils/src/mock_redis.rs`
- Mock GC server (gRPC seam) -> `crates/mc-test-utils/src/mock_gc.rs`

## Code Locations: Environment Tests
- Cluster health smoke tests -> `crates/env-tests/tests/00_cluster_health.rs`
- Cluster bootstrap (K8s seam, ClusterPorts with MC WebTransport) -> `crates/env-tests/src/cluster.rs`
- GC client fixture -> `crates/env-tests/src/fixtures/gc_client.rs`
- Auth client fixture -> `crates/env-tests/src/fixtures/auth_client.rs`
- Env-test flows (20-24) -> `crates/env-tests/tests/` (auth, cross-service, meeting creation, join flow)
- Join flow E2E (GC join API, MC WebTransport, protobuf signaling, bridge) -> `crates/env-tests/tests/24_join_flow.rs`

## Code Locations: Common Crate
- JWT (claims, JwtError, JwksClient, JwtValidator, round-trip tests) -> `crates/common/src/jwt.rs`
- GC JwtError->GcError mapping tests (all 7 variants) -> `crates/gc-service/src/auth/jwt.rs:tests`
- MC JwtError->McError mapping tests (all 7 variants) -> `crates/mc-service/src/errors.rs:tests::test_jwt_error_to_mc_error_*`

## Infrastructure & Shared
- MC K8s health probes (liveness/readiness) → `infra/services/mc-service/deployment.yaml`
- Dev cert generation + MC TLS manifests → `scripts/generate-dev-certs.sh`, `infra/services/mc-service/tls-secret.yaml`
- Kind UDP mapping + setup integration → `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh:create_mc_tls_secret()`
- JWT claims (UserClaims, MeetingTokenClaims, GuestTokenClaims) → `crates/common/src/jwt.rs`
