# Test Navigation

## Architecture & Design (ADRs)
- Integration + fuzz strategy, test infra, env tests → ADR-0005, ADR-0006, ADR-0009, ADR-0014
- Agent-teams workflow; veto-blocking §5.7; cross-boundary ownership §6 → ADR-0024
- Client 4-tier testing + zero-retry flaky policy → ADR-0028; host-side cluster helper (helper-cannot-self-validate) → ADR-0030
- Metric testability + `MetricAssertion` → ADR-0032; polyglot validation pipeline → ADR-0033
- Guard pipeline as Rust binary; test-reliability §Impl Notes → ADR-0034
- Deterministic story runner; runner suite §E; manifest single home §4 → ADR-0035
- Media-flow contract (keyless MH, TS-only frame crypto, multi-party fan-out) → ADR-0036
- sed-test worked example → `.claude/skills/devloop/review-protocol.md`; debates → `docs/debates/`

## Validation Pipeline & Script Self-Tests
- Layers + orchestrator; self-test wiring (no `*.test.sh` auto-runner) → `scripts/layer1.sh`..`scripts/layer7.sh`, `scripts/layer-all.sh`
- Shared assertion helpers (`assert_marker`/`assert_no_marker`/`assert_absent`) → `scripts/lang/_test_helpers.sh`; orchestrator lane integrity + CI-rejection → `scripts/layer-all.test.sh`
- Layer-7 lanes; hermeticity precedent (PATH-stub, `DEVLOOP_TEST` sentinel, seam-inertness) → `scripts/layer7.test.sh`
- Cluster-setup contract (disk guard, `--provision-org`) → `scripts/setup.test.sh`; dev-web N-participant launcher → `scripts/dev-web.test.sh`; suppression sentinel → `scripts/audit-suppressions-check.test.sh`
- Layer-6 audit dep-gate; Gate-2 binding + verdict selftest → `scripts/lang/_changed_helpers.test.sh`, `_audit_gate.test.sh`, `_gate2_binding.sh`, `scripts/guards/simple/selftest-gate2-verdict.sh`
- Guard runner → `scripts/guards/run-guards.sh`; cross-boundary validators → `validate-cross-boundary-*.sh`; completion gate → `scripts/verify-completion.sh`; lane REASON tokens + runbook → `docs/runbooks/devloop-validation.md`

## Story Runner (ADR-0035)
- Runner + `DEVLOOP_TEST`-gated seams + gate-rc lane split, `canary_classify()` → `scripts/workflow/run-story.sh`; hermetic suite → `run-story.test.sh`
- Preflight + substrate probe; stop-hook completion enforcement → `scripts/workflow/preflight-story.sh`, `devloop-stop-hook.sh`
- Manifest schema/`Slug` newtype; engine; CLI → `crates/dt-story/src/`; CLI tests + fixtures → `crates/dt-story/tests/`; manifest + slug-class guards → `scripts/guards/simple/validate-story-manifest.sh`, `validate-slug-class-sync.sh`
- Manifest emission/consumption → `.claude/skills/user-story/SKILL.md`, `close-story/SKILL.md`, `docs/user-stories/_template.md`

## dt-guard (ADR-0034)
- Veto-blocking suites (`resolve_cited_path` security, extraction parity) → `crates/dt-guard/tests/doc_cite_resolve.rs`, `cite_extract_parity.rs`
- Test-domain guards → `crates/dt-guard/src/test_coverage.rs`, `test_registration.rs`, `test_rigidity.rs`, `ts_test_removal.rs`
- Credential-lifetime guard (full-tree); cross-boundary exclusion predicate → `crates/dt-guard/src/ts_retained_credentials.rs`, `cross_boundary_scope.rs`
- Media telemetry-deny guard + e2e; client-metric export fixtures → `crates/dt-guard/src/telemetry_macros.rs`, `tests/media_telemetry_deny_e2e.rs`, `tests/client_metrics_export_fixtures.rs`, `scripts/guards/simple/media-telemetry-deny.{sh,yaml}`
- Release-build-profile + env-config guards + e2e → `crates/dt-guard/src/release_build_profile.rs`, `env_config.rs`, `tests/release_build_profile_e2e.rs`; internal-proto no-key-material self-test → `scripts/guards/validate-internal-proto-no-key-material.test.sh`

## Media Path Tests (ADR-0036)
- Cross-language frame-v2 vector generator (non-production reference crypto; only independent check on TS) + suites → `crates/media-vector-gen/src/lib.rs`, `crates/media-vector-gen/tests/`
- Committed vectors + pinned external SFrame-WG anchor + row manifest; freshness guard (g11) → `proto/test-vectors/frame-v2.vectors.json`, `proto/test-vectors/external/sframe-wg/`, `scripts/guards/simple/validate-frame-vectors.sh`
- SDK frame/receive-path, multi-sender pipeline, KEK retention/source, test tone, hot-path layout → `packages/sdk-core/src/media/{frame,lifecycle,pipeline,setup}/__tests__/`, `src/config/__tests__/kekRetention.test.ts`, `src/media/__tests__/hotPathLayout.test.ts`
- SDK source-scan suites (join-label spread, server-message sinks) + bundle tone-marker check → `packages/sdk-core/src/__tests__/joinLabelSpread.test.ts`, `serverMessageSinkScan.test.ts`, `packages/sdk-core/tests/bundle-content.test.ts`
- MC slot placement, admission, KEK rotation, server mute, teardown, coordination + rig → `crates/mc-service/tests/slot_placement_integration.rs`, `media_*_integration.rs`, `kek_rotation_integration.rs`, `server_mute_integration.rs`, `meeting_teardown_integration.rs`, `tests/common/media_session.rs`
- MH forward/backpressure/session binding, egress admission, end-meeting, server mute, transport seam + rig → `crates/mh-service/tests/media_*_integration.rs`, `stream_admission_integration.rs`, `end_meeting_integration.rs`, `transport_real_impl.rs`, `transport_seam_reachability.rs`, `common/media_rig.rs`
- Internal + signaling proto roundtrip → `crates/proto-gen/tests/internal_roundtrip.rs`, `signaling_roundtrip.rs`

## Metric Testability (ADR-0032)
- Assertion API (delta / unobserved semantics, proof-of-trap tests) → `crates/common/src/observability/testing.rs`
- Canonical gauge 4-cell matrix; per-op drivability; adjacency + multi-emission → `crates/gc-service/tests/registered_controllers_metrics_integration.rs`, `db_metrics_integration.rs`, `crates/ac-service/tests/audit_log_failures_integration.rs`, `credential_ops_metrics_integration.rs`; orphan recording-site audit → `docs/TODO.md` §Observability Debt

## Service Test Suites
- AC integration + fault injection + fuzz + metric clusters + shared state → `crates/ac-service/tests/`, `tests/integration/`, `tests/fault_injection/`, `tests/common/test_state.rs`, `fuzz/fuzz_targets/jwt_validation.rs`, `src/observability/metrics.rs:tests`
- GC auth + meeting + assignment / join stickiness → `crates/gc-service/tests/auth_tests.rs`, `src/auth/jwt.rs:tests`, `meeting_tests.rs`, `meeting_create_tests.rs`, `meeting_assignment_tests.rs`, `mc_assignment_rpc_tests.rs`
- GC refusal causes (`CreateMeetingOutcome`, `MeetingRefusal`, `classify_insert_error`) → `crates/gc-service/src/repositories/meetings.rs`, `src/errors.rs`
- MC auth + actors → `crates/mc-service/src/auth/mod.rs:tests`, `src/grpc/auth_interceptor.rs:tests`, `tests/auth_layer_integration.rs`, `src/actors/`
- MC join + WebTransport + coordination / heartbeat / token refresh → `crates/mc-service/tests/join_tests.rs`, `webtransport_accept_loop_integration.rs`, `src/webtransport/connection.rs:tests`, `src/media_routing/connectivity.rs:tests`, `tests/register_meeting_integration.rs`, `token_refresh_integration.rs`, `gc_integration.rs`, `disconnect_latency_integration.rs`
- MH WebTransport + accept loop + token refresh + McClient → `crates/mh-service/tests/webtransport_integration.rs`, `webtransport_accept_loop_integration.rs`, `token_refresh_integration.rs`, `mc_client_integration.rs`, `crates/mh-service/src/grpc/mc_client.rs:tests`
- Harnesses + rigs → `crates/ac-test-utils/src/`, `crates/gc-test-utils/src/`, `crates/mc-test-utils/src/`, `crates/mc-service/tests/common/`, `crates/mh-service/tests/common/`

## Browser E2E (ADR-0028 env-test tier)
- Harness config (Chromium-only, `retries: 0`); topology + global-setup → `packages/web-app/playwright.config.ts`, `e2e/env.ts`, `e2e/global-setup.ts`, `e2e/configEnv.ts`
- Fixtures + test bus → `packages/web-app/e2e/fixtures.ts`, `src/lib/e2eBus.ts`, `vite.config.ts`; Prometheus helpers (one PromQL home) + client metric names → `e2e/mcMetrics.ts`, `e2e/clientMetricNames.ts`
- Multi-party cohort + per-instance counters + tone detection + S1 diagnostic + credential scan → `packages/web-app/e2e/cohort.ts`, `cohortContexts.ts`, `instanceCounters.ts`, `toneDetector.ts`, `s1Diagnostic.ts`, `credentialScan.ts`
- Window stop rule + same-page mover → `packages/web-app/e2e/windowSampling.ts`, `receiveEvidence.ts:observeWindow`
- Node-tier tests of e2e helpers (one per helper) → `packages/web-app/tests/`
- Specs (solo + `multi-party-hear` / `over-subscription` / `server-mute` / `kek-rotation` / `partial-connectivity`) + division vs Rust env-tests → `packages/web-app/e2e/*.spec.ts`, `e2e/README.md`; pre-close human pass → `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md`
- Layer-7 browser lane (gated only by Phase-1g dev-cert + Playwright Chromium preconditions) → `scripts/layer7.sh`

## Environment Tests & Cluster (ADR-0030)
- Bootstrap + cluster config + eventual-consistency helper; evidence rule (public APIs, per-entity evidence, no shared-gauge values) → `crates/env-tests/src/`, `src/cluster.rs`, `src/eventual.rs`, `crates/env-tests/README.md`
- Flows → `crates/env-tests/tests/` (`24_join_flow.rs`, `26_mh_quic.rs`, `27_mc_slot_placement.rs`, `28_mh_egress_admission.rs`, `29_mh_meeting_teardown.rs`, `34_mc_kek_rotation.rs`, `35_mc_server_mute_teardown.rs`, `01_mh_deployment_config.rs`, `30`/`32`/`33` observability + alert rules)
- Media fixtures: MC/MH WebTransport client, participant, media, MH gRPC, egress admission, kube, metrics, metric hygiene, alert rules → `crates/env-tests/src/fixtures/mc_session.rs`, `participant.rs`, `media.rs`, `mh_grpc.rs`, `egress_admission.rs`, `kube.rs`, `metrics.rs`, `metric_hygiene.rs`, `alert_rules_loaded.rs`
- Per-run org provisioning; resolution with no default → `scripts/layer7.sh:__generate_org_subdomain`, `infra/kind/scripts/setup.sh:provision_run_org()`, `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`
- Subdomain-pattern drift guard + self-test → `scripts/guards/simple/validate-subdomain-regex-sync.sh`, `scripts/guards/validate-subdomain-regex-sync.test.sh`
- CanaryPod + NetworkPolicy → `crates/env-tests/src/canary.rs`, `infra/services/mc-service/network-policy.yaml`
- Setup / teardown / Kind config → `infra/kind/scripts/setup.sh`, `teardown.sh`, `kind-config.yaml.tmpl`; devloop wrapper → `infra/devloop/devloop.sh`; port map → `crates/devloop-helper/src/commands.rs`

## Common
- JWT + meeting token → `crates/common/src/jwt.rs`, `meeting_token.rs:tests`; per-pod Services/ConfigMaps → `infra/services/mc-service/`, `infra/services/mh-service/`; dev certs → `scripts/generate-dev-certs.sh`
