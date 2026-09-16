# Operations Navigation

## Architecture & Design
- Infra (Kind, zero-trust) → ADR-0012; Local dev → ADR-0013; Env tests → ADR-0014; Guard pipeline → ADR-0015
- CI gates / Agent-Teams devloop / Cross-Boundary Ownership Model → ADR-0024 §6; Containerized devloop → ADR-0025
- Host-side cluster helper → ADR-0030; Dashboard metrics (counters vs rates) → ADR-0029; Service-owned dashboards/alerts → ADR-0031
- Metric testability (presence guard, Cat A/B/C rollout, per-service SLO sub-targets) → ADR-0032
- Polyglot validation pipeline → ADR-0033; guard pipeline as `dt-guard` binary → ADR-0034; deterministic story runner → ADR-0035
- Media flow — transport parameters §1, MC→MH control plane §8, verification §10, telemetry + operational config §11 → ADR-0036

## Media Path (ADR-0036)
- MH transport parameters, startup validation, derived drain window → `crates/mh-service/src/config.rs:from_vars()`, `QuicTransportParams`, `DrainWindowSource`, `SHUTDOWN_SETTLE_TARGET_SECONDS`, `SHUTDOWN_MARGIN_SECONDS`
- Transport build + graceful drain → `crates/mh-service/src/webtransport/server.rs:build_transport_config()`, `crates/mh-service/src/main.rs:shutdown_signal()`
- MH transport ConfigMap keys; per-instance `MH_TERMINATION_GRACE_SECONDS` via kustomize `replacements:` → `infra/services/mh-service/configmap.yaml`, `infra/services/mh-service/kustomization.yaml`
- Pod-grace ↔ env drift env-test → `crates/env-tests/tests/01_mh_deployment_config.rs`
- MH hot path (no telemetry macro reachable under it) → `crates/mh-service/src/media/`; transport seam → `crates/mh-service/src/webtransport/media_transport.rs`, `crates/mh-service/src/transport/mod.rs`; sender bindings → `crates/mh-service/src/session/mod.rs`; routing → `crates/mh-service/src/routing/mod.rs`
- MC admission (meeting KEK, identity key, sender-id allocator, binding outcome) → `crates/mc-service/src/media_admission/`
- MC routing + client signaling; handler URL is server-chosen → `crates/mc-service/src/media_routing/`, `crates/mc-service/src/media_signaling/assignments.rs`
- Media-path telemetry deny scope → `scripts/guards/simple/media-telemetry-deny.yaml`, `crates/dt-guard/src/media_telemetry_deny.rs`
- Frame v2 cross-language vectors + gate → `proto/test-vectors/frame-v2.vectors.json`, `scripts/guards/simple/validate-frame-vectors.sh`, `crates/media-vector-gen/`
- Release feature-gate self-test → `scripts/release-feature-gate.test.sh`; release profile guard → `crates/dt-guard/src/release_build_profile.rs`
- Media metric hygiene kernel + live assertion → `crates/env-tests/src/fixtures/metric_hygiene.rs`, `crates/env-tests/tests/32_media_metric_hygiene.rs`
- Alert-rules-actually-loaded check → `crates/env-tests/src/fixtures/alert_rules_loaded.rs`, `crates/env-tests/tests/33_alert_rules_loaded.rs`; MH datagram round trip → `crates/env-tests/tests/26_mh_quic.rs`, `crates/env-tests/src/fixtures/media.rs`
- MH alerts → `infra/docker/prometheus/rules/mh-alerts.yaml`; media dashboards → `infra/grafana/dashboards/mh-media.json`, `infra/grafana/dashboards/client-media.json`; open obligations → `docs/TODO.md` §Media Path Obligations

## Story Workflow (ADR-0035)
- Serial task loop, per-task gate, escalation lanes, audit-remediation + suppression gate → `scripts/workflow/run-story.sh`
- Gate rc lane split, git-error lane, incident record → `run-story.sh:escalate()`, `git_error_lane()`, `record_infra_incident()`
- Failure classification / cost telemetry / resume pointer → `run-story.sh:canary_probe()`, `canary_classify()`, `report_task_cost()`, `persist_resume_pointer()`, `audit_scan()`, `audit_owner_for()`
- `DEVLOOP_TEST`-gated seams + containment predicate → `run-story.sh:__test_sentinel_active()`, `__any_seam_override_present()`, `__seam_assert_run_dir_isolated()`
- Container-boundary hard fail, `STORY_RUNNER_ALLOW_HOST` hatch, guard-binary + verb-currency assertions → `scripts/workflow/preflight-story.sh`
- Headless completion enforcement → `scripts/workflow/devloop-stop-hook.sh`; devloop state → `scripts/workflow/devloop-status.sh`
- Manifest SSoT + `next`/`complete --slug`/`escalate`/`validate`/`add-task --deps`, dep readiness → `crates/dt-story/src/engine.rs`, `manifest.rs`, `main.rs`
- `Slug` newtype + fence-collision controls → `crates/dt-story/src/manifest.rs:Slug`, `markdown.rs:find_manifest_block()`; class-drift guard → `scripts/guards/simple/validate-slug-class-sync.sh`
- Manifest emitted at planning, read at close → `.claude/skills/{devloop,close-story,user-story,debate}/SKILL.md`; Layer-3 validation → `scripts/guards/simple/validate-story-manifest.sh`
- Remediation owner + prompt → `scripts/lang/rust/audit-remediation.md`, `scripts/lang/ts/audit-remediation.md`; suppression SSoT → `audit-suppressions.toml`, `scripts/audit-suppressions-check.sh`; Gate-2 verdict → `scripts/lang/_gate2_binding.sh:emit_gate2_verdict()`, `.githooks/pre-commit`

## CI & Guards
- CI pipeline → `.github/workflows/ci.yml`; guard runner → `scripts/guards/run-guards.sh`, `common.sh`; `dt-guard` CLI → `crates/dt-guard/src/main.rs`, ADR-0033 §6
- Devloop validation runbook (dt-guard triage, STATUS ladder, dep-change gate, browser-E2E, symptom catalogue) → `docs/runbooks/devloop-validation.md`; STATUS rank SSoT → `scripts/lang/_common.sh:__status_rank`; guard timeout lane → `scripts/guards/run-guards.sh:classify_guard_exit`
- Layer 7 browser-E2E preconditions + per-run org → `scripts/layer7.sh:__generate_org_subdomain()`, `precondition_fail()`; Layer 6 audit dep-change gate → `scripts/lang/_audit_gate.sh`
- Pipeline self-tests → `scripts/layer-all.test.sh`, `scripts/layer7.test.sh`, `scripts/setup.test.sh`, `scripts/dev-web.test.sh`, `scripts/workflow/run-story.test.sh`; shared assertions → `scripts/lang/_test_helpers.sh`
- Metric/dashboard guards → `scripts/guards/simple/validate-application-metrics.sh`, `validate-alert-rules.sh`, `validate-metric-coverage.sh`, `validate-dashboard-panels.sh`, `validate-counter-zero-init.sh`, `validate-histogram-buckets.sh`
- Deploy guards → `scripts/guards/simple/validate-kustomize.sh`, `validate-env-config.sh` (`crates/dt-guard/src/env_config.rs`), `validate-no-insecure-browser-flags.sh`, `validate-release-build-profile.sh`
- Cross-boundary guards → `scripts/guards/simple/validate-cross-boundary-scope.sh`, `validate-cross-boundary-classification.sh`, `cross-boundary-ownership.yaml`; semantic checks → `scripts/guards/semantic/checks.md`

## Devloop Cluster Helper
- Kind config template → `infra/kind/kind-config.yaml.tmpl`; wrapper → `infra/devloop/devloop.sh`; image → `infra/devloop/Dockerfile`, `entrypoint.sh`; container client → `infra/devloop/dev-cluster`
- Helper commands (setup/deploy/rebuild/teardown/status), `write_port_map_shell()`, DT_HOST_GATEWAY_IP → `crates/devloop-helper/src/commands.rs`, `protocol.rs`; port registry → `~/.cache/devloop/port-registry.json`
- Env-test URL config → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`; per-run org subdomain → `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`, `packages/web-app/e2e/env.ts`

## Deployment & K8s
- Kind cluster → `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh` (`load_image_to_kind()`, `deploy_only_service()`, `dt_psql()`, `provision_run_org()`), `teardown.sh`
- Per-service Kustomize bases (statefulset/deployment, netpol, PDB) → `infra/services/{ac,gc,mc,mh}-service/`; PostgreSQL + Redis → `infra/services/postgres/`, `redis/`
- Dockerfiles → `infra/docker/{ac,gc,mc,mh}-service/`; dev certs → `scripts/generate-dev-certs.sh`
- MC/MH per-instance Deployments + ConfigMaps → `infra/services/mc-service/mc-0-configmap.yaml`, `infra/services/mh-service/mh-0-configmap.yaml`; devloop patching → `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- Per-pod UDP NodePorts `base + ordinal*2` (MC 4433/4435, MH 4434/4436) → `infra/services/mc-service/service.yaml`, `infra/services/mh-service/service.yaml`; netpol → `infra/services/mh-service/network-policy.yaml`; MH→MC gRPC TCP 50052
- Alert rules → `infra/docker/prometheus/rules/` (`gc-alerts.yaml`, `mc-alerts.yaml`, `mh-alerts.yaml`, `otel-alerts.yaml`, `_template-service-alerts.yaml`)

## Runbooks
- Per-service incident + deployment → `docs/runbooks/mh-deployment.md` (post-deploy checklist, R-36 rollback), `docs/runbooks/mc-deployment.md`, `docs/runbooks/gc-deployment.md`, `docs/runbooks/ac-service-deployment.md`
- MH media scenarios (sender binding, datagrams never read, datagram drop) → `docs/runbooks/mh-incident-response.md` Scenarios 15-17
- MC media scenarios (connection failure, RegisterMeeting, generation divergence, missing key material) → `docs/runbooks/mc-incident-response.md` Scenarios 11-16
- Meeting-refusal triage → `docs/runbooks/gc-incident-response.md` Scenario 8; client dev-local → `docs/runbooks/client-dev-local.md`, `scripts/dev-web.sh:probe_bundler()`

## Observability
- Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/`, `infra/grafana/kustomization.yaml`; Prometheus → `infra/docker/prometheus/prometheus.yml`
- Conventions and objectives → `docs/observability/alert-conventions.md`, `dashboard-conventions.md`, `label-taxonomy.md`, `slos.md`; catalogs → `docs/observability/metrics/`
- Per-service metric handles → `crates/mh-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`; shared `MetricAssertion` → `crates/common/src/observability/testing.rs`

## Services
- AC: config → `crates/ac-service/src/config.rs`; JWKS/JWT → `crates/common/src/jwt.rs`; GC↔AC token types → `crates/common/src/meeting_token.rs`; service auth → ADR-0003
- MH: startup/health → `crates/mh-service/src/main.rs`, `observability/health.rs`; gRPC → `crates/mh-service/src/grpc/`; WebTransport → `crates/mh-service/src/webtransport/connection.rs`; rigs → `crates/mh-service/tests/common/accept_loop_rig.rs`
- MC: startup/config → `crates/mc-service/src/main.rs`, `config.rs`; gRPC → `crates/mc-service/src/grpc/`; MH registry → `crates/mc-service/src/mh_connection_registry.rs`; Redis → `crates/mc-service/src/redis/client.rs`; actors → `crates/mc-service/src/actors/`
- GC: routes + handlers → `crates/gc-service/src/routes/mod.rs`, `handlers/meetings.rs`; refusal outcomes → `crates/gc-service/src/repositories/meetings.rs:CreateMeetingOutcome`, `crates/gc-service/src/errors.rs`
