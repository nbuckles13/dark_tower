# Test Navigation

## Architecture & Design (ADRs)
- Integration + fuzz testing strategy → ADR-0005, ADR-0006
- Integration test infrastructure → ADR-0009
- Environment integration tests → ADR-0014
- Agent-teams workflow; veto-blocking §5.7; cross-boundary ownership §6 → ADR-0024
- Client 4-tier testing + zero-retry flaky policy → ADR-0028
- Host-side cluster helper → ADR-0030
- Metric testability + `MetricAssertion` → ADR-0032
- Polyglot validation pipeline → ADR-0033
- Guard pipeline as Rust binary; test-reliability principle §Implementation Notes → ADR-0034
- Deterministic story runner; runner test suite §E → ADR-0035
- sed-test worked example → `.claude/skills/devloop/review-protocol.md`
- Debates of record → `docs/debates/2026-05-17-guard-toolchain-supersede/`, `docs/debates/2026-08-10-adr-0035-story-runner/`

## Validation Pipeline & Script Self-Tests
- Layers + orchestrator → `scripts/layer1.sh`..`scripts/layer7.sh`, `scripts/layer-all.sh`
- Self-test wiring (there is no `*.test.sh` auto-runner) → `scripts/layer3.sh`
- Shared assertion helpers → `scripts/lang/_test_helpers.sh`
- Orchestrator lane integrity + CI-rejection closure → `scripts/layer-all.test.sh`
- Layer-7 lanes; hermeticity precedent (PATH-stub, `DEVLOOP_TEST` sentinel, seam-inertness) → `scripts/layer7.test.sh`
- Disk guard → `scripts/setup.test.sh`; suppression sentinel → `scripts/audit-suppressions-check.test.sh`
- Layer-6 audit dep-gate → `scripts/lang/_changed_helpers.test.sh`, `scripts/lang/_audit_gate.test.sh`
- Gate-2 binding + verdict selftest → `scripts/lang/_gate2_binding.sh`, `scripts/guards/simple/selftest-gate2-verdict.sh`
- Guard runner → `scripts/guards/run-guards.sh`; cross-boundary validators → `scripts/guards/simple/validate-cross-boundary-*.sh`
- Completion gate → `scripts/verify-completion.sh`; runbook → `docs/runbooks/devloop-validation.md`

## Story Runner (ADR-0035)
- Runner → `scripts/workflow/run-story.sh`
- Preflight + substrate probe; stop-hook completion enforcement → `scripts/workflow/preflight-story.sh`, `devloop-stop-hook.sh`
- Manifest engine + parsing + CLI tests → `crates/dt-story/src/engine.rs`, `src/markdown.rs`, `crates/dt-story/tests/cli.rs`

## dt-guard (ADR-0034)
- Veto-blocking suites (`resolve_cited_path` security, extraction parity) → `crates/dt-guard/tests/doc_cite_resolve.rs`, `cite_extract_parity.rs`
- Test-domain guards → `crates/dt-guard/src/test_coverage.rs`, `crates/dt-guard/src/test_registration.rs`, `crates/dt-guard/src/test_rigidity.rs`, `crates/dt-guard/src/ts_test_removal.rs`
- Credential-lifetime guard (full-tree, not diff-scoped) → `crates/dt-guard/src/ts_retained_credentials.rs`
- Cross-boundary exclusion predicate → `crates/dt-guard/src/cross_boundary_scope.rs`

## Metric Testability (ADR-0032)
- Assertion API (delta / unobserved semantics, proof-of-trap tests) → `crates/common/src/observability/testing.rs`
- Canonical gauge 4-cell adjacency matrix → `crates/gc-service/tests/registered_controllers_metrics_integration.rs`
- Per-op drivability markers; adjacency + multi-emission → `crates/gc-service/tests/db_metrics_integration.rs`, `crates/ac-service/tests/audit_log_failures_integration.rs`, `credential_ops_metrics_integration.rs`
- Orphan recording-site audit → `docs/TODO.md` §Observability Debt

## Service Test Suites
- AC integration + fault injection + fuzz → `crates/ac-service/tests/integration/`, `tests/fault_injection/`, `fuzz/fuzz_targets/jwt_validation.rs`
- AC metric clusters + shared state → `crates/ac-service/tests/`, `tests/common/test_state.rs`, `src/observability/metrics.rs:tests`
- GC auth → `crates/gc-service/tests/auth_tests.rs`, `src/auth/jwt.rs:tests`
- GC meeting + assignment → `crates/gc-service/tests/meeting_tests.rs`, `meeting_create_tests.rs`, `meeting_assignment_tests.rs`, `mc_assignment_rpc_tests.rs`
- MC auth → `crates/mc-service/src/auth/mod.rs:tests`, `src/grpc/auth_interceptor.rs:tests`, `tests/auth_layer_integration.rs`
- MC actors → `crates/mc-service/src/actors/`
- MC join + WebTransport + accept loop → `crates/mc-service/tests/join_tests.rs`, `webtransport_accept_loop_integration.rs`, `src/webtransport/connection.rs:tests`
- MC coordination / heartbeat / token refresh → `crates/mc-service/src/mh_connection_registry.rs:tests`, `tests/media_coordination_integration.rs`, `register_meeting_integration.rs`, `token_refresh_integration.rs`, `tests/gc_integration.rs`, `disconnect_latency_integration.rs`
- MH WebTransport + accept loop + token refresh + McClient → `crates/mh-service/tests/webtransport_integration.rs`, `webtransport_accept_loop_integration.rs`, `token_refresh_integration.rs`, `mc_client_integration.rs`, `crates/mh-service/src/grpc/mc_client.rs:tests`
- Harnesses + rigs → `crates/ac-test-utils/src/`, `crates/gc-test-utils/src/`, `crates/mc-test-utils/src/`, `crates/mc-service/tests/common/`, `crates/mh-service/tests/common/`

## Browser E2E (ADR-0028 env-test tier)
- Harness config (Chromium-only, `retries: 0`) → `packages/web-app/playwright.config.ts`
- Topology + global-setup preconditions → `packages/web-app/e2e/env.ts`, `global-setup.ts`
- Fixtures → `packages/web-app/e2e/fixtures.ts`; Prometheus helpers (one PromQL home) → `packages/web-app/e2e/mcMetrics.ts`
- Specs + division of responsibility vs Rust env-tests → `packages/web-app/e2e/`, `packages/web-app/e2e/README.md`
- Test bus + types → `packages/web-app/src/lib/e2eBus.ts`, `packages/web-app/vite.config.ts`, `src/globals.d.ts`
- Layer-7 browser lane trigger → `scripts/layer7.sh:__browser_e2e_triggered`

## Environment Tests & Cluster (ADR-0030)
- Bootstrap + fixtures + cluster config → `crates/env-tests/src/`, `src/cluster.rs`, `src/fixtures/`
- Flows → `crates/env-tests/tests/` (`00_cluster_health.rs`, `24_join_flow.rs`, `26_mh_quic.rs`, `30_observability.rs`)
- CanaryPod + NetworkPolicy → `crates/env-tests/src/canary.rs`, `infra/services/mc-service/network-policy.yaml`
- Setup / teardown / Kind config → `infra/kind/scripts/setup.sh`, `teardown.sh`, `infra/kind/kind-config.yaml.tmpl`
- Devloop wrapper → `infra/devloop/devloop.sh`; port map → `crates/devloop-helper/src/commands.rs`

## Common
- JWT + meeting token → `crates/common/src/jwt.rs`, `meeting_token.rs:tests`
- Per-pod Services + ConfigMaps → `infra/services/mc-service/`, `infra/services/mh-service/`; dev certs → `scripts/generate-dev-certs.sh`
