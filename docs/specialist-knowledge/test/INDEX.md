# Test Navigation

## Architecture & Design (ADRs)
- Integration + fuzz strategy, test infra, env tests → ADR-0005, ADR-0006, ADR-0009, ADR-0014
- Agent-teams workflow; veto-blocking §5.7; cross-boundary ownership §6 → ADR-0024
- Client 4-tier testing + zero-retry flaky policy → ADR-0028; host-side cluster helper (helper-cannot-self-validate) → ADR-0030
- Metric testability + `MetricAssertion` → ADR-0032; polyglot validation pipeline → ADR-0033
- Guard pipeline as Rust binary; test-reliability §Impl Notes → ADR-0034
- Deterministic story runner; runner suite §E; manifest single home §4 → ADR-0035
- Media-flow contract (keyless MH, TS-only frame crypto) → ADR-0036
- sed-test worked example → `.claude/skills/devloop/review-protocol.md`; debates → `docs/debates/`

## Validation Pipeline & Script Self-Tests
- Layers + orchestrator; self-test wiring (no `*.test.sh` auto-runner) → `scripts/layer1.sh`..`scripts/layer7.sh`, `scripts/layer-all.sh`
- Shared assertion helpers (`assert_marker`/`assert_no_marker`/`assert_absent`) → `scripts/lang/_test_helpers.sh`; orchestrator lane integrity + CI-rejection → `scripts/layer-all.test.sh`
- Layer-7 lanes; hermeticity precedent (PATH-stub, `DEVLOOP_TEST` sentinel, seam-inertness) → `scripts/layer7.test.sh`
- Cluster-setup contract (disk guard, `--provision-org`) → `scripts/setup.test.sh`; suppression sentinel → `scripts/audit-suppressions-check.test.sh`
- Layer-6 audit dep-gate; Gate-2 binding + verdict selftest → `scripts/lang/_changed_helpers.test.sh`, `_audit_gate.test.sh`, `_gate2_binding.sh`, `scripts/guards/simple/selftest-gate2-verdict.sh`
- Guard runner → `scripts/guards/run-guards.sh`; cross-boundary validators → `validate-cross-boundary-*.sh`; completion gate → `scripts/verify-completion.sh`; lane REASON tokens + runbook → `docs/runbooks/devloop-validation.md`

## Story Runner (ADR-0035)
- Runner + `DEVLOOP_TEST`-gated seams + gate-rc lane split, `canary_classify()` → `scripts/workflow/run-story.sh`; hermetic suite → `run-story.test.sh`
- Preflight + substrate probe; stop-hook completion enforcement → `scripts/workflow/preflight-story.sh`, `devloop-stop-hook.sh`
- Manifest schema/`Slug` newtype; block find/emit; engine; CLI → `crates/dt-story/src/`; CLI tests + fixtures → `crates/dt-story/tests/`
- Manifest + slug-class guards, with self-test → `scripts/guards/simple/validate-story-manifest.sh`, `validate-slug-class-sync.sh`, `scripts/guards/validate-slug-class-sync.test.sh`
- Manifest emission/consumption → `.claude/skills/user-story/SKILL.md`, `close-story/SKILL.md`, `docs/user-stories/_template.md`

## dt-guard (ADR-0034)
- Veto-blocking suites (`resolve_cited_path` security, extraction parity) → `crates/dt-guard/tests/doc_cite_resolve.rs`, `cite_extract_parity.rs`
- Test-domain guards → `crates/dt-guard/src/test_coverage.rs`, `test_registration.rs`, `test_rigidity.rs`, `ts_test_removal.rs`
- Credential-lifetime guard (full-tree); cross-boundary exclusion predicate → `crates/dt-guard/src/ts_retained_credentials.rs`, `cross_boundary_scope.rs`
- Media telemetry-deny guard + policy + e2e → `crates/dt-guard/src/telemetry_macros.rs`, `tests/media_telemetry_deny_e2e.rs`, `scripts/guards/simple/media-telemetry-deny.{sh,yaml}`
- Release-build-profile + env-config guards + e2e → `crates/dt-guard/src/release_build_profile.rs`, `env_config.rs`, `tests/release_build_profile_e2e.rs`, `scripts/guards/simple/validate-release-build-profile.sh`, `validate-env-config.sh`

## Media Path Tests (ADR-0036)
- Cross-language frame-v2 vector generator (non-production reference crypto; only independent check on TS) + suites → `crates/media-vector-gen/src/lib.rs`, `crates/media-vector-gen/tests/`
- Committed vectors + pinned external SFrame-WG anchor + shared row manifest → `proto/test-vectors/frame-v2.vectors.json`, `proto/test-vectors/external/sframe-wg/`
- Frame-vectors freshness + workspace-membership guard (g11) → `scripts/guards/simple/validate-frame-vectors.sh`
- SDK frame/sframe codec + vector conformance; audio pipeline lifecycle/stages/setup; hot-path layout → `packages/sdk-core/src/media/frame/__tests__/`, `media/lifecycle/__tests__/`, `media/pipeline/__tests__/`, `media/setup/__tests__/`, `media/__tests__/hotPathLayout.test.ts`
- MC media routing/admission/signaling/policy/coordination → `crates/mc-service/tests/media_*_integration.rs`
- MH media forward + backpressure + session binding + transport seam (real-impl vs seam-reachability) + rig → `crates/mh-service/tests/media_forward_integration.rs`, `media_backpressure_integration.rs`, `media_session_binding_integration.rs`, `transport_real_impl.rs`, `transport_seam_reachability.rs`, `common/media_rig.rs`
- Internal + signaling proto roundtrip → `crates/proto-gen/tests/internal_roundtrip.rs`, `signaling_roundtrip.rs`

## Metric Testability (ADR-0032)
- Assertion API (delta / unobserved semantics, proof-of-trap tests) → `crates/common/src/observability/testing.rs`
- Canonical gauge 4-cell matrix; per-op drivability; adjacency + multi-emission → `crates/gc-service/tests/registered_controllers_metrics_integration.rs`, `db_metrics_integration.rs`, `crates/ac-service/tests/audit_log_failures_integration.rs`, `credential_ops_metrics_integration.rs`; orphan recording-site audit → `docs/TODO.md` §Observability Debt

## Service Test Suites
- AC integration + fault injection + fuzz + metric clusters + shared state → `crates/ac-service/tests/`, `tests/integration/`, `tests/fault_injection/`, `tests/common/test_state.rs`, `fuzz/fuzz_targets/jwt_validation.rs`, `src/observability/metrics.rs:tests`
- GC auth + meeting + assignment → `crates/gc-service/tests/auth_tests.rs`, `src/auth/jwt.rs:tests`, `meeting_tests.rs`, `meeting_create_tests.rs`, `meeting_assignment_tests.rs`, `mc_assignment_rpc_tests.rs`
- GC refusal causes (`CreateMeetingOutcome`, `MeetingRefusal`, `classify_insert_error`) → `crates/gc-service/src/repositories/meetings.rs`, `src/errors.rs`
- MC auth → `crates/mc-service/src/auth/mod.rs:tests`, `src/grpc/auth_interceptor.rs:tests`, `tests/auth_layer_integration.rs`
- MC actors → `crates/mc-service/src/actors/`
- MC join + WebTransport + coordination / heartbeat / token refresh → `crates/mc-service/tests/join_tests.rs`, `webtransport_accept_loop_integration.rs`, `src/webtransport/connection.rs:tests`, `src/mh_connection_registry.rs:tests`, `tests/register_meeting_integration.rs`, `token_refresh_integration.rs`, `gc_integration.rs`, `disconnect_latency_integration.rs`
- MH WebTransport + accept loop + token refresh + McClient → `crates/mh-service/tests/webtransport_integration.rs`, `webtransport_accept_loop_integration.rs`, `token_refresh_integration.rs`, `mc_client_integration.rs`, `crates/mh-service/src/grpc/mc_client.rs:tests`
- Harnesses + rigs → `crates/ac-test-utils/src/`, `crates/gc-test-utils/src/`, `crates/mc-test-utils/src/`, `crates/mc-service/tests/common/`, `crates/mh-service/tests/common/`

## Browser E2E (ADR-0028 env-test tier)
- Harness config (Chromium-only, `retries: 0`) → `packages/web-app/playwright.config.ts`
- Topology (required `E2E_ORG_SUBDOMAIN`, derived base URL) + global-setup → `packages/web-app/e2e/env.ts`, `global-setup.ts`; node-tier contract → `packages/web-app/tests/e2e-env.test.ts`
- Fixtures + test bus → `packages/web-app/e2e/fixtures.ts`, `src/lib/e2eBus.ts`, `vite.config.ts`; Prometheus helpers (one PromQL home) → `packages/web-app/e2e/mcMetrics.ts`
- Specs (incl. `media-loopback.spec.ts`) + division of responsibility vs Rust env-tests → `packages/web-app/e2e/`, `packages/web-app/e2e/README.md`
- Layer-7 browser lane (always-runs since 2026-08-20; gated only by Phase-1g dev-cert + Playwright Chromium preconditions) → `scripts/layer7.sh`

## Environment Tests & Cluster (ADR-0030)
- Bootstrap + fixtures + cluster config → `crates/env-tests/src/`, `src/cluster.rs`, `src/fixtures/`
- Flows → `crates/env-tests/tests/` (`24_join_flow.rs`, `26_mh_quic.rs`, `30_observability.rs`, media `01_mh_deployment_config.rs`, `32_media_metric_hygiene.rs`, `33_alert_rules_loaded.rs`) + hygiene fixtures `src/fixtures/metric_hygiene.rs`
- Per-run org provisioning (Phase 1h) → `scripts/layer7.sh:__generate_org_subdomain`, `infra/kind/scripts/setup.sh:provision_run_org()`
- Org subdomain resolution, no default → `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`
- Subdomain-pattern drift guard + self-test → `scripts/guards/simple/validate-subdomain-regex-sync.sh`, `scripts/guards/validate-subdomain-regex-sync.test.sh`
- CanaryPod + NetworkPolicy → `crates/env-tests/src/canary.rs`, `infra/services/mc-service/network-policy.yaml`
- Setup / teardown / Kind config → `infra/kind/scripts/setup.sh`, `teardown.sh`, `kind-config.yaml.tmpl`; devloop wrapper → `infra/devloop/devloop.sh`; port map → `crates/devloop-helper/src/commands.rs`

## Common
- JWT + meeting token → `crates/common/src/jwt.rs`, `meeting_token.rs:tests`; per-pod Services/ConfigMaps → `infra/services/mc-service/`, `infra/services/mh-service/`; dev certs → `scripts/generate-dev-certs.sh`
