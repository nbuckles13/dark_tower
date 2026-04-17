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
- Auth (meeting/guest JWT, McAuthInterceptor, JWKS McAuthLayer+scope) -> `crates/mc-service/src/auth/mod.rs:tests`, `grpc/auth_interceptor.rs:tests`
- Config + error tests (incl. MhAssignmentMissing) -> `crates/mc-service/src/config.rs:tests`, `crates/mc-service/src/errors.rs:tests`
- Actor tests (controller, meeting, participant, session) -> `crates/mc-service/src/actors/controller.rs:tests`, `meeting.rs`, `participant.rs`, `session.rs`
- Join flow integration tests (T1-T15, MH assignment, media_servers, bridge, RegisterMeeting trigger, multi-MH, mixed grpc_endpoint) -> `crates/mc-service/tests/join_tests.rs`
- WebTransport tests (encoding, connection, handle_client_message) -> `crates/mc-service/src/webtransport/connection.rs:tests`
- RegisterMeeting retry/backoff unit tests -> `crates/mc-service/src/webtransport/connection.rs:tests::test_register_*`
- MH data (MhAssignmentStore trait, MhRegistrationClient trait, MockMhAssignmentStore, MockMhRegistrationClient + `wait_for_calls(expected, timeout)`) -> `crates/mc-service/src/redis/client.rs`, `src/grpc/mh_client.rs`, `tests/join_tests.rs`
- Join test harness builders (`TestServer::create_meeting`, `create_meeting_with_handlers(id, handlers)`) -> `crates/mc-service/tests/join_tests.rs`
- MH coordination (registry, MediaCoordinationService, connect/disconnect round-trip + idempotent retry) -> `crates/mc-service/src/mh_connection_registry.rs:tests`, `grpc/media_coordination.rs:tests::test_coordination_flow_connect_disconnect_round_trip`
- GC integration + heartbeat tests -> `crates/mc-service/tests/gc_integration.rs`, `heartbeat_tasks.rs`
- Health + metrics (incl. RegisterMeeting, MH coordination + media connection failure) -> `crates/mc-service/src/observability/health.rs`, `metrics.rs`
- Test utils (mock Redis, mock GC, mock MH) -> `crates/mc-test-utils/src/mock_redis.rs`, `mock_gc.rs`, `mock_mh.rs`

## Code Locations: MH Service
- Config tests (env vars, defaults, TLS, debug redaction, advertise addresses, JWKS URL, timeouts) -> `crates/mh-service/src/config.rs:tests`
- Error tests (labels, status codes, client messages, JwtError conversion) -> `crates/mh-service/src/errors.rs:tests`
- Auth tests (legacy structural + JWKS ServiceClaims + scope) -> `crates/mh-service/src/grpc/auth_interceptor.rs:tests`
- JWT validation tests (MhJwtValidator, meeting tokens, wiremock JWKS) -> `crates/mh-service/src/auth/mod.rs:tests`
- gRPC handler tests (RegisterMeeting validation, SessionManagerHandle integration) -> `crates/mh-service/src/grpc/mh_service.rs:tests`
- Session manager actor tests (handle API, registration, connections, pending promotion, notify via oneshot) -> `crates/mh-service/src/session/mod.rs:tests`
- WebTransport server + connection handler -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`
- WebTransport provisional-accept select arms (await_meeting_registration timeout/cancel/registered, local DebuggingRecorder per-test) -> `crates/mh-service/src/webtransport/connection.rs:tests`
- Health + metrics tests -> `crates/mh-service/src/observability/health.rs:tests`, `metrics.rs:tests`
- McClient tests (construction, auth, retry constants, endpoint errors) -> `crates/mh-service/src/grpc/mc_client.rs:tests`
- MC notification integration tests (mock MediaCoordinationService, retry, auth short-circuit) -> `crates/mh-service/tests/mc_client_integration.rs`
- WebTransport notification wiring (connect, disconnect, fire-and-forget) -> `crates/mh-service/src/webtransport/connection.rs:spawn_notify_connected()`
- MC notification metrics -> `crates/mh-service/src/observability/metrics.rs:record_mc_notification()`
- GC integration tests (registration, load reports, NOT_FOUND) -> `crates/mh-service/tests/gc_integration.rs`

## Code Locations: Environment Tests
- Cluster bootstrap + fixtures → `crates/env-tests/src/`, flows (20-24) → `crates/env-tests/tests/`
- Cluster connection + port config (ADR-0030) → `crates/env-tests/src/cluster.rs`
- Client fixtures (GC, Auth, Prometheus) → `crates/env-tests/src/fixtures/`
- Join flow tests (AC→GC→MC e2e) → `crates/env-tests/tests/24_join_flow.rs`
- CanaryPod + NetworkPolicy manifests → `crates/env-tests/src/canary.rs`, `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Cluster health + kubectl security checks → `crates/env-tests/tests/00_cluster_health.rs`
- Observability validation (Loki, metrics) → `crates/env-tests/tests/30_observability.rs`, `src/cluster.rs:is_loki_available()`

## Code Locations: Cluster Setup & Helper (ADR-0030)
- Setup script (arg parsing, deploy_only_service, load_image_to_kind) → `infra/kind/scripts/setup.sh`
- Teardown → `infra/kind/scripts/teardown.sh`; Kind config → `infra/kind/kind-config.yaml.tmpl`
- Port map + gateway IP → `crates/devloop-helper/src/commands.rs`; Port map file → `/tmp/devloop-{slug}/ports.json`
- Env vars + ConfigMap patching → `infra/kind/scripts/setup.sh`; Wrapper → `infra/devloop/devloop.sh`

## Code Locations: Common & Infrastructure
- JWT (claims, JwksClient, JwtValidator, round-trip tests) -> `crates/common/src/jwt.rs`; meeting token -> `meeting_token.rs:tests`
- MC/MH per-pod Services, ConfigMaps, Kind port mappings → `infra/services/{mc,mh}-service/`; Dev certs → `scripts/generate-dev-certs.sh`
