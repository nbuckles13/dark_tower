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
- Meeting + assignment tests -> `crates/gc-service/tests/meeting_tests.rs`, `meeting_create_tests.rs`, `meeting_assignment_tests.rs`, `mc_assignment_rpc_tests.rs`
- MH selection unit tests -> `crates/gc-service/src/services/mh_selection.rs:tests`
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
- Config tests (env vars, defaults, TLS, debug redaction, advertise addresses, JWKS URL, timeouts) -> `crates/mh-service/src/config.rs:tests`
- Error tests (labels, status codes, client messages, JwtError conversion) -> `crates/mh-service/src/errors.rs:tests`
- Auth tests (legacy structural + JWKS ServiceClaims + scope) -> `crates/mh-service/src/grpc/auth_interceptor.rs:tests`
- JWT validation tests (MhJwtValidator, meeting tokens, wiremock JWKS) -> `crates/mh-service/src/auth/mod.rs:tests`
- Session manager tests (registration, connections, pending promotion, notify) -> `crates/mh-service/src/session/mod.rs:tests`
- WebTransport server + connection handler -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`
- Health + metrics tests -> `crates/mh-service/src/observability/health.rs:tests`, `metrics.rs:tests`
- GC integration tests (registration, load reports, NOT_FOUND) -> `crates/mh-service/tests/gc_integration.rs`

## Code Locations: Environment Tests
- Cluster bootstrap + fixtures → `crates/env-tests/src/`, flows (20-24) → `crates/env-tests/tests/`
- Cluster connection + port config (ADR-0030) → `crates/env-tests/src/cluster.rs:ClusterPorts`, `ClusterConnection`, `from_env()`, `parse_host_port()`
- GC client fixture (join, guest token, mc_assignment) → `crates/env-tests/src/fixtures/gc_client.rs`
- Auth client fixture → `crates/env-tests/src/fixtures/auth_client.rs`
- Prometheus client fixture → `crates/env-tests/src/fixtures/metrics.rs`
- Join flow tests (AC→GC→MC e2e) → `crates/env-tests/tests/24_join_flow.rs`
- CanaryPod + NetworkPolicy manifests → `crates/env-tests/src/canary.rs`, `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Cluster health + kubectl security checks → `crates/env-tests/tests/00_cluster_health.rs`
- Loki availability (HTTP /ready, deferred from init) → `crates/env-tests/src/cluster.rs:is_loki_available()`
- Observability validation (EXPECTED_SERVICES constant) → `crates/env-tests/tests/30_observability.rs`

## Code Locations: Cluster Setup & Helper (ADR-0030)
- Setup script (arg parsing, TTY detection, cluster name validation) → `infra/kind/scripts/setup.sh`
- Single-service rebuild+redeploy → `infra/kind/scripts/setup.sh:deploy_only_service()`
- Image loading (podman/docker) → `infra/kind/scripts/setup.sh:load_image_to_kind()`
- Env vars + ConfigMap patching (DT_CLUSTER_NAME, DT_PORT_MAP, advertise addresses) → `infra/kind/scripts/setup.sh`
- Teardown with cluster name support → `infra/kind/scripts/teardown.sh`
- Kind config template (listenAddress: HOST_GATEWAY_IP per ADR-0030) → `infra/kind/kind-config.yaml.tmpl`
- Port map + gateway IP → `crates/devloop-helper/src/commands.rs:write_port_map_shell()`, `validate_gateway_ip()`
- Port map file → `/tmp/devloop-{slug}/ports.json`
- Task slug validation → `infra/devloop/devloop.sh` (line 66)

## Code Locations: Common & Infrastructure
- JWT (claims, JwtError, JwksClient, JwtValidator, round-trip tests) -> `crates/common/src/jwt.rs`
- Shared meeting token types (GC<->AC contract, serde, defaults) -> `crates/common/src/meeting_token.rs:tests`
- MC/MH per-pod Services, ConfigMaps, Kind port mappings → `infra/services/{mc,mh}-service/`, `infra/kind/kind-config.yaml`
- Dev certs → `scripts/generate-dev-certs.sh`
