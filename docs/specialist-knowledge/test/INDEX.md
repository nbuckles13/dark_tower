# Test Navigation

## Architecture & Design
- Integration testing strategy -> `docs/decisions/adr-0005-integration-testing-strategy.md`
- Fuzz testing -> `docs/decisions/adr-0006-fuzz-testing-strategy.md`
- Integration test infrastructure -> `docs/decisions/adr-0009-integration-test-infrastructure.md`
- Environment integration tests -> `docs/decisions/adr-0014-environment-integration-tests.md`
- Validation pipeline (guards, coverage) -> `docs/decisions/adr-0024-agent-teams-workflow.md`
- Coverage thresholds -> `.codecov.yml`
- Client architecture (4-tier testing, test-utils, flaky policy) -> ADR-0028
- Host-side cluster helper (env-test execution, URL config, attempt budgets, cluster networking) -> `docs/decisions/adr-0030-host-side-cluster-helper.md`

## Code Locations: AC Service
- Integration + fault injection tests -> `crates/ac-service/tests/integration/`, `crates/ac-service/tests/fault_injection/`
- Fuzz targets -> `crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs`
- Test harness + token builders -> `crates/ac-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/token_builders.rs`
- Rate limit config + tests -> `crates/ac-service/src/config.rs:parse_rate_limit_i64()`, `tests::test_rate_limit_*`

## Code Locations: GC Service
- Auth tests (HTTP + wiremock JWKS, jwt wrapper) -> `crates/gc-service/tests/auth_tests.rs`, `crates/gc-service/src/auth/jwt.rs:tests`
- Meeting tests (join, guest, settings, creation, assignment) -> `crates/gc-service/tests/meeting_tests.rs`, `meeting_create_tests.rs`, `meeting_assignment_tests.rs`
- Participant & activation tests -> `crates/gc-service/tests/participant_tests.rs`
- Meeting handlers + routes -> `crates/gc-service/src/handlers/meetings.rs`, `crates/gc-service/src/routes/mod.rs`
- Metrics + observability -> `crates/gc-service/src/observability/metrics.rs`, `docs/observability/metrics/gc-service.md`
- Test harness -> `crates/gc-test-utils/src/server_harness.rs`

## Code Locations: MC Service
- Auth + token validation (meeting, guest, confusion tests) -> `crates/mc-service/src/auth/mod.rs:tests`
- Config + error tests -> `crates/mc-service/src/config.rs:tests`, `crates/mc-service/src/errors.rs:tests`
- Actor tests (controller, meeting, participant, session) -> `crates/mc-service/src/actors/controller.rs:tests`, `meeting.rs`, `participant.rs`, `session.rs`
- WebTransport tests (encoding, connection) -> `crates/mc-service/src/webtransport/handler.rs:tests`, `connection.rs`
- MC error code mapping (JwtValidation → Unauthorized) → `crates/mc-service/src/webtransport/connection.rs:send_error()`
- GC integration + heartbeat tests -> `crates/mc-service/tests/gc_integration.rs`, `heartbeat_tasks.rs`
- Health + metrics -> `crates/mc-service/src/observability/health.rs`, `metrics.rs`
- Test utils (mock Redis, mock GC) -> `crates/mc-test-utils/src/mock_redis.rs`, `mock_gc.rs`

## Code Locations: MH Service
- Config tests (env vars, defaults, TLS, debug redaction, advertise addresses, StatefulSet ordinal parsing) -> `crates/mh-service/src/config.rs:tests`
- Error tests (labels, status codes, client messages) -> `crates/mh-service/src/errors.rs:tests`
- Auth interceptor tests (Bearer, size limits) -> `crates/mh-service/src/grpc/auth_interceptor.rs:tests`
- RegisterMeeting stub (input validation) -> `crates/mh-service/src/grpc/mh_service.rs:register_meeting()`
- Health state & router tests -> `crates/mh-service/src/observability/health.rs:tests`
- GC integration tests (registration, load reports, NOT_FOUND) -> `crates/mh-service/tests/gc_integration.rs`

## Code Locations: Environment Tests
- Cluster bootstrap + fixtures → `crates/env-tests/src/`, flows (20-24) → `crates/env-tests/tests/`
- Cluster connection + port config → `crates/env-tests/src/cluster.rs:ClusterPorts`, `ClusterConnection`
- URL env var entry point (ADR-0030) → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- URL parsing + validation → `crates/env-tests/src/cluster.rs:parse_host_port()`
- GC client fixture (join, guest token, mc_assignment) → `crates/env-tests/src/fixtures/gc_client.rs`
- Auth client fixture → `crates/env-tests/src/fixtures/auth_client.rs`
- Prometheus client fixture → `crates/env-tests/src/fixtures/metrics.rs`
- Join flow tests (AC→GC→MC e2e) → `crates/env-tests/tests/24_join_flow.rs`
- Cluster health + kubectl security checks → `crates/env-tests/tests/00_cluster_health.rs`
- Observability validation (Loki, Prometheus) → `crates/env-tests/tests/30_observability.rs`, `src/cluster.rs:is_loki_available()`

## Code Locations: Cluster Setup & Helper (ADR-0030)
- Setup/teardown scripts → `infra/kind/scripts/setup.sh`, `teardown.sh`
- Single-service rebuild+redeploy → `infra/kind/scripts/setup.sh:deploy_only_service()`
- Kind config template → `infra/kind/kind-config.yaml.tmpl`
- Port map + gateway IP → `crates/devloop-helper/src/commands.rs:write_port_map_shell()`, `validate_gateway_ip()`
- Task slug validation → `infra/devloop/devloop.sh`

## Code Locations: Proto & Contracts
- MH assignment (grpc_endpoint, reserved role field) -> `proto/internal.proto:MhAssignment`
- MC->MH RegisterMeeting RPC -> `proto/internal.proto` (MediaHandlerService)
- MH->MC coordination (connect/disconnect notifications) -> `proto/internal.proto:MediaCoordinationService`
- Client media failure signaling -> `proto/signaling.proto:MediaConnectionFailed`
- GC MH assignment (grpc_endpoint propagation) -> `crates/gc-service/src/services/mh_selection.rs:MhAssignmentInfo`, `mc_client.rs:assign_meeting()`

## Code Locations: Common & Infrastructure
- JWT (claims, JwtError, JwksClient, JwtValidator, round-trip tests) -> `crates/common/src/jwt.rs`
- Shared meeting token types (GC<->AC contract, serde, defaults) -> `crates/common/src/meeting_token.rs:tests`
- MC/MH per-pod Services, ConfigMaps, Kind port mappings → `infra/services/{mc,mh}-service/`, `infra/kind/kind-config.yaml`
- Dev certs → `scripts/generate-dev-certs.sh`
