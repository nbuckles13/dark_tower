# Infrastructure Navigation

## Architecture & Design
- Infrastructure architecture (networking, zero-trust) -> `docs/decisions/adr-0012-infrastructure-architecture.md`
- Local dev environment (Kind + Calico) -> `docs/decisions/adr-0013-local-development-environment.md`
- Containerized devloop execution model -> `docs/decisions/adr-0025-containerized-devloop.md`
- Host-side cluster helper for integration testing; helper-cannot-self-validate corollary -> `docs/decisions/adr-0030-host-side-cluster-helper.md`
- Client architecture (CDN, Nx build, probe sizing) -> ADR-0028; dashboard metric presentation -> ADR-0029
- Service-owned dashboards + alert rules (per-service files, `_template-*` shapes) -> ADR-0031
- Guard pipeline as Rust binary `dt-guard` (single-crate, subcommand-per-policy) -> `docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md`
- Deterministic story runner (serial/single-branch/single-container, substrate, gates) -> `docs/decisions/adr-0035-story-runner.md`
- Media flow; §11 compile-time controls, media-path telemetry deny, control self-tests -> `docs/decisions/adr-0036-media-flow.md`

## Code Locations
- Service Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`; Compose (local tests) -> `docker-compose.test.yml`; dev TLS certs (CA + MC + MH) -> `scripts/generate-dev-certs.sh`
- Service runbooks (`<service>-deployment.md` + `<service>-incident-response.md`) -> `docs/runbooks/`; local web demo bring-up -> `docs/runbooks/client-dev-local.md`
- Prometheus config, kustomization, per-service alert rules -> `infra/docker/prometheus/`; K8s scrape config -> `infra/kubernetes/observability/{prometheus.yml,prometheus-config.yaml}`
- K8s service manifests (Kustomize bases) -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- Service workloads: AC StatefulSet -> `infra/services/ac-service/statefulset.yaml`; GC Deployment -> `infra/services/gc-service/deployment.yaml`; MC/MH per-instance Deployments + PDB -> `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-deployment.yaml`, `pdb.yaml`
- MC/MH ConfigMaps (shared + per-instance; MH transport parameters) -> `infra/services/{mc,mh}-service/configmap.yaml`, `{mc,mh}-{0,1}-configmap.yaml`
- Network policies (per-service ingress/egress) -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`; MC/MH per-instance Services (NodePorts) -> `infra/services/{mc,mh}-service/service.yaml`
- OTel Collector base (configmap, deployment, service, network policy) -> `infra/services/otel-collector/`; smoke payload -> `infra/smoke/`
- Redis + PostgreSQL manifests (Kustomize bases) -> `infra/services/{redis,postgres}/kustomization.yaml`; PostgreSQL init -> `infra/docker/postgres/init.sql`
- K8s observability manifests (Prometheus, Loki/Promtail, kube-state-metrics, node-exporter) -> `infra/kubernetes/observability/kustomization.yaml`
- Grafana manifests, dashboards + provisioning -> `infra/grafana/kustomization.yaml`, `infra/grafana/{dashboards,provisioning}/`
- Kind overlays: cluster root, observability, per-service patches (OTel endpoint, CORS, NodePorts) -> `infra/kubernetes/overlays/kind/{kustomization.yaml,observability/,services/}`
- Kind cluster config + setup script (also creates MC/MH TLS + MH secrets imperatively) -> `infra/kind/kind-config.yaml`, `kind-config.yaml.tmpl`, `infra/kind/scripts/setup.sh`
- setup.sh parameterization (DT_CLUSTER_NAME, DT_PORT_MAP, DT_HOST_GATEWAY_IP, DT_ORG_MAX_CONCURRENT_MEETINGS, --yes, --only, --skip-build, --provision-org) -> ADR-0030
- setup.sh helpers -> `deploy_mc_service()`/`deploy_mh_service()` (ConfigMap advertise-addr patching), `load_image_to_kind()`, `deploy_only_service()`, `dt_psql()` (in-pod psql SSoT), `provision_run_org()`; tests -> `scripts/setup.test.sh`
- Local iteration / teardown / Skaffold -> `infra/kind/scripts/{iterate,teardown}.sh`, `infra/skaffold.yaml`
- Containerized devloop + dev-cluster CLI -> `infra/devloop/{devloop.sh,dev-cluster}`; container start -> `infra/devloop/entrypoint.sh`
- Web-app demo launcher (topology preflight, then Vite) -> `scripts/dev-web.sh`; test `scripts/dev-web.test.sh`

## Guard Pipeline (ADR-0034)
- Binary + wrapper shape (subcommand-per-policy, clap dispatcher, STATUS line per ADR-0033 §6; wrappers source the shared prelude per ADR-0034 §3) -> `crates/dt-guard/`, `scripts/guards/simple/*.sh`; canonical-home `Lazy<Regex>` convention -> `clippy.toml`
- Canonical `{ac,gc,mc,mh}` service enumeration SSoT; Bash mirror -> `crates/dt-guard/src/common/services.rs`, `scripts/guards/common.sh`
- Kustomize policy (build, orphan manifests, kubeconform, securityContext) -> `crates/dt-guard/src/kustomize.rs`, `kustomize_tools.rs`
- Env-config policy (per-workload env-var coverage, `configMapKeyRef` resolution) -> `crates/dt-guard/src/env_config.rs`
- Observability policy modules -> `crates/dt-guard/src/alert_rules.rs`, `dashboard_panels.rs`, `infrastructure_metrics.rs`, `metric_labels.rs`, `metric_coverage.rs`
- Release-artifact premise for the ADR-0036 §11 compile gate -> `crates/dt-guard/src/release_build_profile.rs`; real-build demo `scripts/release-feature-gate.test.sh`
- Insecure browser/TLS flag prohibition -> `crates/dt-guard/src/no_insecure_browser_flags.rs`
- Media-path telemetry deny (config-driven scope, distinct scope-liveness tokens, no suppression) -> `crates/dt-guard/src/media_telemetry_deny.rs`, manifest `scripts/guards/simple/media-telemetry-deny.yaml`, self-test `scripts/guards/media-telemetry-deny.test.sh`
- Macro-family vocabularies (`ALL` derives the alternation) -> `crates/dt-guard/src/metric_macros.rs`, `telemetry_macros.rs`; scope-liveness predicate -> `crates/dt-guard/src/common/scope.rs`
- Client credential-retention guard (field vocabulary from `common::pii_vocabulary`) -> `crates/dt-guard/src/ts_retained_credentials.rs`, wrapper `scripts/guards/simple/ts/no-retained-credentials.sh`
- Cross-language frame-vector fixtures -> `scripts/guards/simple/validate-frame-vectors.sh`, self-test `scripts/guards/validate-frame-vectors.test.sh`

## Validation Pipeline & Toolchain (ADR-0033)
- Layer scripts -> orchestrator `scripts/layer-all.sh`; Layer 7 (env-tests + browser E2E, one live cluster) -> `scripts/layer7.sh`, failure-map `docs/runbooks/devloop-validation.md` §6.7, Lead attempt-policy `.claude/skills/devloop/SKILL.md`
- Per-run layer-7 organization (Phase 1h generation, `setup.sh --provision-org`, AC resolution probe) -> `scripts/layer7.sh:__generate_org_subdomain()`; consumers -> `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`, `packages/web-app/e2e/env.ts`; pattern drift guard -> `scripts/guards/simple/validate-subdomain-regex-sync.sh`, self-test `scripts/guards/validate-subdomain-regex-sync.test.sh`
- Hermetic script self-tests + wiring -> `scripts/layer3.sh`; suites -> `scripts/layer7.test.sh`, `scripts/setup.test.sh`, `scripts/layer-all.test.sh`, `scripts/workflow/run-story.test.sh`; shared helper SSoT -> `scripts/lang/_test_helpers.sh`
- Layer 6 audit dep-change gate -> `scripts/lang/_audit_gate.sh:audit_dep_changed_{rust,ts}`, glob predicate `scripts/lang/_changed_helpers.sh:diff_touches_glob`; design -> ADR-0033 §3/§11
- Audit suppressions (pnpm + cargo) -> `.pnpm-audit-ignore.json`, `.cargo/audit.toml`, lib `scripts/lang/_audit_suppressions_lib.sh`, drift check `scripts/audit-suppressions-check.sh`, wrappers `scripts/lang/rust/audit.sh`, `scripts/lang/ts/audit.sh`
- Node toolchain pin (SSoT `.nvmrc` -> Dockerfile ARG + lockfile engines floor) -> `.nvmrc`, `.npmrc`, root `package.json`, `infra/devloop/Dockerfile`, `infra/devloop/devloop.sh:read_node_version()`
- Gate-2 tree-bound verdict (producer + pre-commit validator) -> `scripts/lang/_gate2_binding.sh:emit_gate2_verdict()`, `gate2_validate_commit()`, selftest `scripts/guards/simple/selftest-gate2-verdict.sh`
- CI pipelines -> `.github/workflows/{ci.yml,ci-client.yml}`; scheduled ambient audit -> `audit-scheduled.yml`; fuzz nightly -> `fuzz-nightly.yml`

## Story Runner (ADR-0035)
- Serial loop, canary classification, gate lane split, session-limit lane, slug capture, resume -> `scripts/workflow/run-story.sh`
- Runner test seams (`STORY_REPO_ROOT`, `DT_STORY`, `DEVLOOP_TEST` sentinel, containment predicate) -> seam block at the top of `scripts/workflow/run-story.sh`; hermetic suite -> `scripts/workflow/run-story.test.sh`
- Preconditions: container-boundary guard, version-cached substrate probe, Stop-hook + settings registration -> `scripts/workflow/preflight-story.sh`; completion enforcement -> `scripts/workflow/devloop-stop-hook.sh`
- Manifest schema (`slug`, `Slug` newtype, no `branch`) -> `crates/dt-story/src/manifest.rs`; block discovery -> `crates/dt-story/src/markdown.rs:find_manifest_block()`; dep readiness -> `crates/dt-story/src/engine.rs:next()`; CLI verbs -> `crates/dt-story/src/main.rs`
- Manifest producers/consumers -> `.claude/skills/user-story/SKILL.md`, `.claude/skills/close-story/SKILL.md`; guards -> `scripts/guards/simple/validate-story-manifest.sh`, `validate-slug-class-sync.sh`
- Per-language audit remediation owner + prompt -> `scripts/lang/rust/audit-remediation.md`, `scripts/lang/ts/audit-remediation.md`
- Run record (task/gate logs, cost ledger, escalation + infra-incident records, canaries) -> `${DEVLOOP_TMP:-/tmp/devloop}/story-runner/<story>/`

## Host-Side Cluster Helper (ADR-0030)
- Helper binary: port allocation -> `crates/devloop-helper/src/ports.rs`; commands -> `crates/devloop-helper/src/commands.rs`; NDJSON protocol -> `crates/devloop-helper/src/protocol.rs`
- Cluster health + port-map writers -> `commands.rs:cmd_status()`, `parse_pod_health()`, `write_port_map_shell()`, `cmd_setup()`, `cmd_deploy()`
- Port registry (global) -> `~/.cache/devloop/port-registry.json`; per-devloop runtime state (PID, socket, auth token, ports.json, log) -> `/tmp/devloop-{slug}/`; container mount -> `devloop.sh:500`
- Env-test URL config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`

## Service Config, Probes & Integration Seams
- MC health endpoints (liveness + readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`; probe config -> `infra/services/{mc,gc,mh}-service/*deployment.yaml`
- MC/MH advertise-address config -> `crates/mc-service/src/config.rs`, `crates/mh-service/src/config.rs`; GC registration -> `crates/mc-service/src/grpc/gc_client.rs:register()`, `crates/mh-service/src/grpc/gc_client.rs:register()`
- CanaryPod (NetworkPolicy testing) -> `crates/env-tests/src/canary.rs`; env-tests (cluster health, observability, resilience) -> `crates/env-tests/tests/`
