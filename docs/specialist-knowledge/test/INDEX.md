# Test Navigation

## Architecture & Design
- Integration testing + fuzz testing strategies -> `docs/decisions/adr-0005-integration-testing-strategy.md`, `adr-0006-fuzz-testing-strategy.md`
- Integration test infrastructure -> `docs/decisions/adr-0009-integration-test-infrastructure.md`
- Environment integration tests -> `docs/decisions/adr-0014-environment-integration-tests.md`
- Validation pipeline (guards, coverage) -> `docs/decisions/adr-0024-agent-teams-workflow.md`, `scripts/guards/run-guards.sh`; alert-rules guard (strict + lenient-legacy modes, `--self-test`, fixtures) -> `scripts/guards/simple/validate-alert-rules.sh`, `scripts/guards/simple/fixtures/alert-rules/`, `scripts/guards/simple/alert-rules.legacy-allowlist`
- Coverage thresholds -> `.codecov.yml`
- Client architecture (4-tier testing, test-utils, flaky policy) -> ADR-0028
- Host-side cluster helper (env-test execution, URL config, attempt budgets, cluster networking) -> `docs/decisions/adr-0030-host-side-cluster-helper.md`
- Metric testability + `MetricAssertion` patterns (per-thread recorder, counter-idempotent-on-repeat-read, histogram-drain-on-read, two-fixed-point timing with ONE snapshot, partial-label `assert_delta(0)` subset-match, current_thread flavor load-bearing for `tokio::spawn`; `assert_unobserved` symmetric API across counter/gauge/histogram with `ensure_no_kind_mismatch` hardening — hard form distinct from soft `assert_delta(0)`/`assert_observation_count(0)` by panic-on-cross-kind-regression; closes ADR-0032 §F4; symmetry table + drain-on-read trap proof in module doc; per-cluster `assert_only_<combo>` adjacency-helper pattern catches label-swap-bug regressions; production-truth multi-emission variant asserts ALL real emissions on chained-call paths; reviewer scope-fidelity discipline = grep plan-stage commitments against landed code, don't let API-side polish substitute for integration-side fidelity at close-out; gauge 4-cell adjacency matrix is the canonical reference for `assert_value(0.0)` vs `assert_unobserved` distinction — former asserts metric IS observed at zero via explicit zero-fill writer, latter asserts NEVER observed (conflating masks always-emit refactor regressions); cells: full happy / partial→zero-fill / empty / caller short-circuits; **orphan-recording-site disposition** = metric whose recording fn has zero production callers — driving real seam proves wiring not behavior, so reclassify to wrapper-Cat-C with *distinct* canonical comment block separate from no-business-error-branch variant + dual-variant index in cluster-file header docstring + separate TODO entry from fault-injection-harness debt (fixes differ: caller wiring vs harness build); heuristic on demotion: do the wider `git grep -- 'crates/'` outside the repo's own file to confirm whether the surrounding *struct or trait* is orphan, not just the one fn flagged) -> `docs/decisions/adr-0032-metric-testability.md`, `crates/common/src/observability/testing.rs` (§"Delta semantics", §"Unobserved semantics", per-kind `assert_unobserved` impls, proof-of-trap + kind-mismatch tests), `crates/ac-service/tests/audit_log_failures_integration.rs:106-213` (`assert_only_event_type` + multi-emission), `crates/ac-service/tests/credential_ops_metrics_integration.rs:54-71` (`assert_only_cell`), `crates/gc-service/tests/registered_controllers_metrics_integration.rs` (canonical 4-cell gauge matrix), `crates/gc-service/tests/db_metrics_integration.rs` (file-header §"Per-op drivability classification" + `WRAPPER-CAT-C (orphan recording site)` comment markers), `docs/TODO.md` §Observability Debt "Orphan recording-site audit", `docs/devloop-outputs/2026-04-26-adr-0032-step-4-ac-metric-test-backfill/main.md` (iter-2 closure), `docs/devloop-outputs/2026-04-27-adr-0032-step-5-gc-metric-test-backfill/main.md`

## Code Locations: AC Service
- Integration + fault injection + fuzz -> `crates/ac-service/tests/integration/`, `crates/ac-service/tests/fault_injection/`, `crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs`
- Test harness + token builders -> `crates/ac-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/token_builders.rs`
- Rate limit config + tests -> `crates/ac-service/src/config.rs:parse_rate_limit_i64()`, `tests::test_rate_limit_*`
- Metric component-test cluster files (ADR-0032 Step 4; drive direct handler/service/repository fns under `#[sqlx::test]` — separate-task spawn defeats thread-local recorder; `#[sqlx::test]` defaults to `flavor = "current_thread"`). Audit-log drives all 11 production sites via `ALTER TABLE ... CHECK NOT VALID` and `DROP TABLE auth_events CASCADE` seams; credential-ops uses 12-cell adjacency; errors covers 4 `ErrorCategory` mappings; in-src `metrics.rs::tests` replaces hand-rolled `DebuggingRecorder` smoke tests -> `crates/ac-service/tests/` (cluster files: `audit_log_failures_integration.rs`, `bcrypt_metrics_integration.rs`, `credential_ops_metrics_integration.rs`, `db_metrics_integration.rs`, `errors_metric_integration.rs`, `http_metrics_integration.rs`, `internal_token_metrics_integration.rs`, `jwks_metrics_integration.rs`, `key_rotation_metrics_integration.rs`, `rate_limit_metrics_integration.rs`, `token_issuance_service_integration.rs`, `token_issuance_user_integration.rs`, `token_validation_integration.rs`), `crates/ac-service/tests/common/test_state.rs` (`make_app_state`/`seed_signing_key`/`seed_service_credential`/`TEST_CLIENT_SECRET`), `crates/ac-service/src/observability/metrics.rs:tests`

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
- Join flow + WebTransport tests (T1-T13 MH assignment + media_servers + bridge + RegisterMeeting trigger + retry/backoff `tests::test_register_*` + `handle_client_message` w/ MetricAssertion for `mc_media_connection_failures_total`) -> `crates/mc-service/tests/join_tests.rs`, `src/webtransport/connection.rs:tests`
- MH coordination + GC heartbeat (per-(status,type) matrix w/ injected fault) -> `crates/mc-service/src/mh_connection_registry.rs:tests`, `grpc/media_coordination.rs:tests`, `tests/gc_integration.rs`, `heartbeat_tasks.rs`
- Accept-loop component tests (real `bind()+accept_loop()`, `accepted|rejected|error` status, 4 reachable `session_join_failures` `error_type` via real injected faults) -> `crates/mc-service/tests/webtransport_accept_loop_integration.rs`
- Auth-layer integration (5-`failure_reason` JWT cluster + `caller_type_rejected`, real wiremock JWKS) -> `crates/mc-service/tests/auth_layer_integration.rs`
- Media coordination + register-meeting + token-refresh integration (`mh_notifications`, stub MH gRPC, Cat B `record_token_refresh_metrics` per-`error_category` matrix in `crates/mc-service/src/observability/metrics.rs:tests`) -> `crates/mc-service/tests/media_coordination_integration.rs`, `register_meeting_integration.rs`, `token_refresh_integration.rs`
- Per-failure-class wrapper covers (actor metrics, redis ops, orphans) -> `crates/mc-service/tests/actor_metrics_integration.rs`, `redis_metrics_integration.rs`, `orphan_metrics_integration.rs`
- Shared rigs + scaffolding (`AcceptLoopRig`, `TestStackHandles`, `build_test_stack`, `seed_meeting_with_mh`, `MockMhAssignmentStore`, `MockMhRegistrationClient`) -> `crates/mc-service/tests/common/`
- Health + metrics + cross-service test utils (mock Redis/GC/MH) -> `crates/mc-service/src/observability/health.rs`, `metrics.rs`, `crates/mc-test-utils/src/`

## Code Locations: MH Service
- Config tests (env vars, defaults, TLS, debug redaction, advertise addresses, JWKS URL, timeouts) -> `crates/mh-service/src/config.rs:tests`
- Error tests (labels, status codes, client messages, JwtError conversion) -> `crates/mh-service/src/errors.rs:tests`
- Auth tests (legacy structural + JWKS ServiceClaims + scope) -> `crates/mh-service/src/grpc/auth_interceptor.rs:tests`
- JWT validation tests (MhJwtValidator, meeting tokens, wiremock JWKS) -> `crates/mh-service/src/auth/mod.rs:tests`
- gRPC handler tests (RegisterMeeting validation, SessionManagerHandle integration) -> `crates/mh-service/src/grpc/mh_service.rs:tests`
- Session manager actor tests (handle API, registration, connections, pending promotion, notify via oneshot) -> `crates/mh-service/src/session/mod.rs:tests`
- WebTransport server + connection handler -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`
- Health + metrics tests (incl. `record_mc_notification`, WT notify wiring via `spawn_notify_connected()`) -> `crates/mh-service/src/observability/`, `src/webtransport/connection.rs:spawn_notify_connected()`
- McClient tests + MC notification integration (mock MediaCoordinationService, retry, auth short-circuit) -> `crates/mh-service/src/grpc/mc_client.rs:tests`, `tests/mc_client_integration.rs`
- GC integration tests (registration, load reports, NOT_FOUND) -> `crates/mh-service/tests/gc_integration.rs`
- Auth layer integration (MhAuthLayer + MhMediaService, JWKS upgrade, alg-none/HS256 confusion) -> `crates/mh-service/tests/auth_layer_integration.rs`
- RegisterMeeting integration (happy path over wire, InvalidArgument) -> `crates/mh-service/tests/register_meeting_integration.rs`
- WebTransport integration (JWT accept path, provisional timeout ±survival with two-fixed-point counter idempotence pattern, MC connect/disconnect notify, wrong-token-type session-state distinguishing signal) -> `crates/mh-service/tests/webtransport_integration.rs`
- Accept-loop component tests (real `bind()+accept_loop()`, `accepted|rejected|error` status labels, `mh_active_connections` gauge, handshake histogram) + token-refresh integration (ADR-0032 Cat B guard-satisfaction; per-`error_category` matrix under `src/observability/metrics.rs:tests`) -> `crates/mh-service/tests/webtransport_accept_loop_integration.rs`, `token_refresh_integration.rs`
- Shared rigs (TestKeypair, JWKS, gRPC, WebTransport+self-signed TLS via `rcgen` tempdir PEMs, MC mock, token minters, accept_loop_rig) -> `crates/mh-service/tests/common/`

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
