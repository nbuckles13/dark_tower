# Operations Navigation

## Architecture & Design
- Infra (Kind, zero-trust) → ADR-0012; Local dev → ADR-0013; Env tests → ADR-0014; Guard pipeline → ADR-0015
- CI gates / Agent-Teams devloop / Cross-Boundary Ownership Model → ADR-0024 §6; Containerized devloop → ADR-0025
- Host-side cluster helper → ADR-0030; Dashboard metrics (counters vs rates) → ADR-0029; Service-owned dashboards/alerts → ADR-0031
- Metric testability (single presence guard, Cat A/B/C rollout, per-service SLO sub-targets) → ADR-0032
- Polyglot validation pipeline → ADR-0033; Guard pipeline as `dt-guard` binary → ADR-0034
- Deterministic story runner (`run-story`) → ADR-0035

## Story Workflow (ADR-0035)
- Serial task loop, per-task gate, escalation lanes, session-limit sleep, audit-remediation + suppression gate → `scripts/workflow/run-story.sh`
- Gate rc lane split (rc 1 implementer / any other non-zero operator), git-error lane, incident record → `run-story.sh:escalate()`, `git_error_lane()`, `git_head()`, `tree_dirty()`, `record_infra_incident()`
- Failure classification / cost telemetry / resume pointer → `run-story.sh:canary_probe()`, `canary_classify()`, `report_task_cost()`, `persist_resume_pointer()`, `newest_devloop_output()`, `audit_scan()`, `audit_owner_for()`
- `DEVLOOP_TEST`-gated seams (`STORY_REPO_ROOT`, `DT_STORY`, `DEVLOOP_TMP`) + containment predicate → `run-story.sh:__test_sentinel_active()`, `__any_seam_override_present()`, `__seam_assert_run_dir_isolated()`
- Container-boundary hard fail + `STORY_RUNNER_ALLOW_HOST` hatch, substrate probe, guard-binary + verb-currency assertions → `scripts/workflow/preflight-story.sh`
- Headless completion enforcement → `scripts/workflow/devloop-stop-hook.sh`; devloop state → `scripts/workflow/devloop-status.sh`
- Manifest SSoT (`status` + `slug`; no `branch`) + `next`/`complete --slug`/`escalate`/`validate`/`add-task --deps`/`list-tasks`, dep readiness → `crates/dt-story/src/engine.rs`, `manifest.rs`, `main.rs`
- `Slug` newtype, deserializer-enforced `SLUG_PATTERN` → `crates/dt-story/src/manifest.rs:Slug`; class-drift guard → `scripts/guards/simple/validate-slug-class-sync.sh`
- Fence-collision controls (emit refusal, unterminated-fence bail, orphan `- id:` scan, `save()` round-trip) → `manifest.rs:to_block_body()`, `markdown.rs:find_manifest_block()`, `engine.rs`
- Manifest emitted at planning (Steps 10.4/10.5), read at close → `.claude/skills/{user-story,close-story}/SKILL.md`; Layer-3 validation → `scripts/guards/simple/validate-story-manifest.sh`
- Per-language remediation owner + prompt (`# owner:` line 1) → `scripts/lang/rust/audit-remediation.md`, `scripts/lang/ts/audit-remediation.md`
- Suppression SSoT + sync check → `audit-suppressions.toml`, `scripts/audit-suppressions-check.sh`, `docs/contributor/audit-suppressions.md`, ADR-0033 §11
- Gate-2 verdict producer/validator → `scripts/lang/_gate2_binding.sh:emit_gate2_verdict()`, `gate2_validate_commit()`, `.githooks/pre-commit`
- ADR-before-load-bearing rule + test-debt clause → ADR-0035 §N; review-depth routing (`review_mode`) → ADR-0035 §B
- Workflow skills → `.claude/skills/{devloop,close-story,user-story,debate}/SKILL.md`

## CI & Guards
- CI pipeline → `.github/workflows/ci.yml`; guard runner + common → `scripts/guards/run-guards.sh`, `common.sh`; `dt-guard` CLI → `crates/dt-guard/src/main.rs`, ADR-0033 §6
- Devloop validation runbook (dt-guard triage §6.3.1, STATUS ladder §3, dep-change gate §6.6, browser-E2E §6.7, symptom catalogue §8) → `docs/runbooks/devloop-validation.md`; STATUS rank SSoT → `scripts/guards/common.sh:__status_rank`
- Layer 7 browser-E2E preconditions + Phase-2 lane, per-run org (Phase 1h) → `scripts/layer7.sh:__generate_org_subdomain()`, `precondition_fail()`; Layer 6 audit dep-change gate → `scripts/lang/_audit_gate.sh`, ADR-0033 §3
- Pipeline self-tests (hermetic convention for E) → `scripts/layer-all.test.sh`, `scripts/layer7.test.sh`, `scripts/setup.test.sh`, `scripts/audit-suppressions-check.test.sh`, `scripts/workflow/run-story.test.sh`, `scripts/guards/validate-slug-class-sync.test.sh`, `scripts/guards/validate-subdomain-regex-sync.test.sh`
- Shared self-test assertions (`assert_marker`/`assert_no_marker`/`assert_absent`) → `scripts/lang/_test_helpers.sh`
- Guard-toolchain debates of record + per-guard vendor-coverage matrix → `docs/debates/2026-05-17-guard-toolchain-supersede/`, `docs/debates/2026-05-14-python-guard-pipeline-strategy/`
- Simple guards: kustomize → `scripts/guards/simple/validate-kustomize.sh`; app metrics → `validate-application-metrics.sh`; alert rules → `validate-alert-rules.sh` (`docs/observability/alert-conventions.md`); metric-test coverage → `validate-metric-coverage.sh`; org-subdomain pattern drift → `validate-subdomain-regex-sync.sh`
- Cross-boundary guards (Layer A scope-drift, Layer B classification-sanity) → `scripts/guards/simple/validate-cross-boundary-scope.sh`, `validate-cross-boundary-classification.sh`, `cross-boundary-ownership.yaml`
- Semantic checks (authoritative list) → `scripts/guards/semantic/checks.md`; credential-lifetime guard → `crates/dt-guard/src/ts_retained_credentials.rs`

## Devloop Cluster Helper
- Kind config template → `infra/kind/kind-config.yaml.tmpl`; wrapper → `infra/devloop/devloop.sh`; image → `infra/devloop/Dockerfile`, `entrypoint.sh`; container client → `infra/devloop/dev-cluster`
- Helper commands (setup/deploy/rebuild/teardown/status), `write_port_map_shell()`, DT_HOST_GATEWAY_IP → `crates/devloop-helper/src/commands.rs`, `protocol.rs`; port registry → `~/.cache/devloop/port-registry.json`; per-devloop state → `/tmp/devloop-{slug}/`
- Env-test URL config → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`; per-run org subdomain → `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()` (`ENV_TEST_ORG_SUBDOMAIN`), `packages/web-app/e2e/env.ts` (`E2E_ORG_SUBDOMAIN`, derives `E2E_BASE_URL`)

## Deployment & K8s
- Kind cluster → `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh` (`load_image_to_kind()`, `deploy_only_service()`, `dt_psql()`, `provision_run_org()` for `--provision-org <sub>` / `DT_ORG_MAX_CONCURRENT_MEETINGS`), `teardown.sh`
- Per-service Kustomize bases + manifests (statefulset/deployment, netpol, PDB) → `infra/services/{ac,gc,mc,mh}-service/`; PostgreSQL + Redis → `infra/services/postgres/`, `redis/`
- Dockerfiles → `infra/docker/{ac,gc,mc,mh}-service/`; dev certs → `scripts/generate-dev-certs.sh`
- Alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`, `_template-service-alerts.yaml`
- MC/MH per-instance Deployments + ConfigMaps → `infra/services/mc-service/mc-{0,1}-configmap.yaml`, `mh-service/mh-{0,1}-configmap.yaml`; devloop patching → `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- MH shared config (AC_JWKS_URL FQDN form) → `infra/services/mh-service/configmap.yaml`; per-deployment env ref → `mh-{0,1}-deployment.yaml`
- Per-pod UDP NodePorts `base + ordinal*2` (MC 4433/4435, MH 4434/4436); cross-service netpol → `{gc,mc,mh}-service/network-policy.yaml`; MH→MC gRPC TCP 50052

## Runbooks & Database
- Per-service incident/deployment → `docs/runbooks/{ac,gc,mc,mh}-*.md`
- MH post-deploy checklist + R-36 rollback → `docs/runbooks/mh-deployment.md`; MC↔MH addendum → `docs/runbooks/mc-deployment.md`
- GC deploy smoke (CORS + telemetry-proxy, OTLP fixture `infra/smoke/empty-otlp-metrics.bin`) → `docs/runbooks/gc-deployment.md`
- Meeting-refusal triage (cap / inactive / not-provisioned) → `docs/runbooks/gc-incident-response.md` Scenario 8; alerts → `infra/docker/prometheus/rules/gc-alerts.yaml`
- Client dev-local runbook → `docs/runbooks/client-dev-local.md`, `scripts/dev-web.sh:probe_bundler()`, `.npmrc`
- Participant tracking + meetings → `crates/gc-service/src/repositories/participants.rs`, `meetings.rs`

## Auth & JWT
- Common JWKS + JWT → `crates/common/src/jwt.rs`; shared GC↔AC token types → `crates/common/src/meeting_token.rs`; AC rate limits → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`; service auth → ADR-0003

## Observability
- Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/` (`mh-slos.json`, `mh-logs.json`), `infra/grafana/kustomization.yaml`; alerts → `docs/observability/alerts.md`; Prometheus → `infra/docker/prometheus/prometheus.yml`
- Per-service metrics → `crates/ac-service/src/observability/metrics.rs`, `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`; AC metric catalog → `docs/observability/metrics/ac-service.md`
- Shared `MetricAssertion` helper + `assert_unobserved` → `crates/common/src/observability/testing.rs`
- Metric-label PromQL audit + Cat A canary acceptance criteria → `docs/devloop-outputs/2026-04-27-adr-0032-step-5-gc-metric-test-backfill/main.md`

## Services
- AC: startup/config → `crates/ac-service/src/config.rs` (`DEFAULT_BCRYPT_COST` load-bearing for `ac_bcrypt_duration_seconds` buckets → `tests/bcrypt_metrics_integration.rs`); K8s → `infra/services/ac-service/`; tests → `crates/ac-service/tests/`, `tests/common/test_state.rs`
- MH: startup/config/health → `crates/mh-service/src/main.rs`, `config.rs`, `observability/health.rs`; gRPC → `src/grpc/{mh_service,gc_client,mc_client,auth_interceptor}.rs`; WebTransport → `src/webtransport/{server,connection}.rs`, `src/session/mod.rs`; rigs → `tests/common/accept_loop_rig.rs`
- MC: startup/gRPC wiring → `crates/mc-service/src/main.rs`, `config.rs` (`MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS`); WebTransport → `src/webtransport/{server,connection}.rs`; gRPC → `src/grpc/{mc_service,media_coordination,gc_client,mh_client,auth_interceptor}.rs`; registry → `src/mh_connection_registry.rs`; Redis → `src/redis/client.rs`; actors → `src/actors/{controller,meeting,participant}.rs`
- GC: routes + handlers → `crates/gc-service/src/routes/mod.rs`, `handlers/meetings.rs`; meeting-refusal outcomes → `src/repositories/meetings.rs:CreateMeetingOutcome`, `MeetingRefusal`, `classify_insert_error()`, wire mapping → `src/errors.rs:GcError::{OrgMeetingLimitExceeded,OrgInactive,OrgNotProvisioned}`; tests → `crates/gc-service/tests/meeting_tests.rs`, `meeting_create_tests.rs`, `crates/mc-service/tests/join_tests.rs`, `crates/mc-test-utils/src/jwt_test.rs`, `crates/env-tests/`
- Test-build dev-dep feature-flag pattern (`common` w/ `test-utils`) → `crates/ac-service/Cargo.toml`, `crates/mc-service/Cargo.toml`, `crates/mh-service/Cargo.toml`
