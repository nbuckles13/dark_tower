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
- Auth/JWT tests -> `crates/gc-service/tests/auth_tests.rs`
- Meeting join/guest/settings tests -> `crates/gc-service/tests/meeting_tests.rs`
- Meeting creation tests -> `crates/gc-service/tests/meeting_create_tests.rs`
- Participant & activation tests -> `crates/gc-service/tests/participant_tests.rs`
- Meeting assignment tests -> `crates/gc-service/tests/meeting_assignment_tests.rs`
- Test token helpers (TestUserClaims, TestClaims) -> `crates/gc-service/tests/meeting_tests.rs:TestUserClaims`
- Join handler (user-auth) -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`
- Guest token handler (public) -> `crates/gc-service/src/handlers/meetings.rs:get_guest_token()`
- Settings handler (user-auth, host-only) -> `crates/gc-service/src/handlers/meetings.rs:update_meeting_settings()`
- Join metrics -> `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- Route definitions (public, user-auth, service-auth) -> `crates/gc-service/src/routes/mod.rs`
- Activation repo -> `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`
- Audit event logging -> `crates/gc-service/src/repositories/meetings.rs:log_audit_event()`
- Test harness (HTTP seam) -> `crates/gc-test-utils/src/server_harness.rs`

## Code Locations: MC Service
- GC integration tests -> `crates/mc-service/tests/gc_integration.rs`
- Heartbeat tests -> `crates/mc-service/tests/heartbeat_tasks.rs`
- Mock Redis -> `crates/mc-test-utils/src/mock_redis.rs`
- Mock GC server (gRPC seam) -> `crates/mc-test-utils/src/mock_gc.rs`

## Code Locations: Environment Tests
- Cluster bootstrap (K8s seam) -> `crates/env-tests/src/cluster.rs`
- GC client fixture -> `crates/env-tests/src/fixtures/gc_client.rs`
- Auth client fixture -> `crates/env-tests/src/fixtures/auth_client.rs`
- Auth flows -> `crates/env-tests/tests/20_auth_flows.rs`
- Cross-service flows -> `crates/env-tests/tests/21_cross_service_flows.rs`
- Meeting creation env-tests -> `crates/env-tests/tests/23_meeting_creation.rs`

## Infrastructure & Shared
- Dev cert generation + MC TLS manifests → `scripts/generate-dev-certs.sh`, `infra/services/mc-service/tls-secret.yaml`
- Kind UDP mapping + setup integration → `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh:create_mc_tls_secret()`
- JWT claims (UserClaims, MeetingTokenClaims, GuestTokenClaims) → `crates/common/src/jwt.rs`
