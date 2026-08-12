# Infrastructure Navigation

## Architecture & Design
- Infrastructure architecture (networking, zero-trust) -> `docs/decisions/adr-0012-infrastructure-architecture.md`
- Local dev environment (Kind + Calico) -> `docs/decisions/adr-0013-local-development-environment.md`
- Containerized devloop execution model -> `docs/decisions/adr-0025-containerized-devloop.md`
- Host-side cluster helper for integration testing -> `docs/decisions/adr-0030-host-side-cluster-helper.md`
- Client architecture (CDN, Nx build, probe sizing) -> ADR-0028; dashboard metric presentation -> ADR-0029
- Guard pipeline as Rust binary `dt-guard` (single-crate, subcommand-per-policy) -> `docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md`
- Deterministic story runner (serial/single-branch/single-container, substrate, gates) -> `docs/decisions/adr-0035-story-runner.md`

## Code Locations
- Service Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`
- Service runbooks (per-service `<service>-deployment.md` + `<service>-incident-response.md`) -> `docs/runbooks/`
- PostgreSQL init -> `infra/docker/postgres/init.sql`
- Prometheus config + alert rules -> `infra/docker/prometheus/{prometheus.yml,rules/{gc,mc}-alerts.yaml}`, `infra/kubernetes/observability/prometheus-config.yaml`
- K8s service manifests (Kustomize bases) -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- Service workloads: AC StatefulSet -> `infra/services/ac-service/statefulset.yaml`; GC Deployment -> `infra/services/gc-service/deployment.yaml`; MC/MH per-instance Deployments -> `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-deployment.yaml`
- MC/MH ConfigMaps (shared + per-instance) -> `infra/services/{mc,mh}-service/configmap.yaml`, `{mc,mh}-{0,1}-configmap.yaml`
- Network policies (per-service ingress/egress) -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- MC/MH per-instance Services (NodePorts) -> `infra/services/{mc,mh}-service/service.yaml`
- MC/MH TLS + MH secrets -> created imperatively by `setup.sh`
- Redis + PostgreSQL manifests (Kustomize bases) -> `infra/services/{redis,postgres}/kustomization.yaml`
- K8s observability + Grafana manifests (Kustomize; RBAC, deployment, dashboards) -> `infra/kubernetes/observability/kustomization.yaml`, `.../observability/grafana/kustomization.yaml`
- Kind overlays -> `infra/kubernetes/overlays/kind/` (services, observability, per-service)
- Grafana dashboards + provisioning -> `infra/grafana/{dashboards,provisioning}/`
- Kind cluster config + setup script -> `infra/kind/kind-config.yaml`, `kind-config.yaml.tmpl`, `infra/kind/scripts/setup.sh`
- setup.sh parameterization (DT_CLUSTER_NAME, DT_PORT_MAP, DT_HOST_GATEWAY_IP, --yes, --only, --skip-build) -> ADR-0030
- setup.sh helpers -> `deploy_mc_service()`/`deploy_mh_service()` (ConfigMap advertise-addr patching), `load_image_to_kind()`, `deploy_only_service()`
- Local iteration / teardown / Skaffold -> `infra/kind/scripts/{iterate,teardown}.sh`, `infra/skaffold.yaml`
- Containerized devloop + dev-cluster CLI -> `infra/devloop/{devloop.sh,dev-cluster}`; container start -> `infra/devloop/entrypoint.sh`
- Guard policy binary + wrapper shape (ADR-0034; subcommand-per-policy, clap dispatcher, STATUS line per ADR-0033 §6; wrappers source the shared prelude per ADR-0034 §3) -> `crates/dt-guard/`, `scripts/guards/simple/*.sh`
- Client credential-retention guard (retention-gated, segment-matched field vocabulary from `common::pii_vocabulary`, full-tree `packages/**`) -> `crates/dt-guard/src/ts_retained_credentials.rs`, wrapper `scripts/guards/simple/ts/no-retained-credentials.sh`
- Workspace `disallowed_methods` convention (canonical-home `Lazy<Regex>` enforcement per ADR-0034 §6) -> `clippy.toml`
- Docker Compose (local tests) -> `docker-compose.test.yml`
- Dev TLS cert generation (CA + MC + MH certs) -> `scripts/generate-dev-certs.sh`

## Validation Pipeline & Toolchain (ADR-0033)
- Layer scripts -> orchestrator `scripts/layer-all.sh`; Layer 7 (env-tests + browser E2E, one live cluster) -> `scripts/layer7.sh`, failure-map `docs/runbooks/devloop-validation.md` §6.7, Lead attempt-policy `.claude/skills/devloop/SKILL.md`
- Layer 6 audit dep-change gate (manifest-only match) -> `scripts/lang/_audit_gate.sh:audit_dep_changed_{rust,ts}`, glob predicate `scripts/lang/_changed_helpers.sh:diff_touches_glob`; design -> ADR-0033 §3/§11
- Audit suppressions (pnpm + cargo) -> `.pnpm-audit-ignore.json`, `.cargo/audit.toml`, lib `scripts/lang/_audit_suppressions_lib.sh`, drift check `scripts/audit-suppressions-check.sh`, wrappers `scripts/lang/rust/audit.sh`, `scripts/lang/ts/audit.sh`
- Node toolchain pin (SSoT `.nvmrc` -> Dockerfile ARG + lockfile engines floor) -> `.nvmrc`, `.npmrc` (engine-strict), root `package.json` engines.node, `infra/devloop/Dockerfile` ARG NODE_VERSION, `infra/devloop/devloop.sh:read_node_version()`
- Gate-2 tree-bound verdict (producer + pre-commit validator, per-layer `LAYER` lines) -> `scripts/lang/_gate2_binding.sh:emit_gate2_verdict()`, `gate2_validate_commit()`, selftest `scripts/guards/simple/selftest-gate2-verdict.sh`
- CI pipelines -> `.github/workflows/{ci.yml,ci-client.yml}`; scheduled ambient audit -> `audit-scheduled.yml`; fuzz nightly -> `fuzz-nightly.yml`

## Story Runner (ADR-0035)
- Serial loop, canary classification, session-limit lane, gates, resume -> `scripts/workflow/run-story.sh`
- Preconditions: container-boundary guard (`/run/.containerenv` ∪ `/.dockerenv`, `STORY_RUNNER_ALLOW_HOST`), version-cached substrate probe, Stop-hook + settings registration -> `scripts/workflow/preflight-story.sh`
- Headless-devloop completion enforcement (blocks premature turn-end) -> `scripts/workflow/devloop-stop-hook.sh`
- Task manifest SSoT (`next`/`complete`/`escalate`/`validate`/`add-task`) -> `crates/dt-story/src/manifest.rs`, `crates/dt-story/src/markdown.rs`; dep readiness -> `crates/dt-story/src/engine.rs:next()`
- Per-language audit remediation owner + prompt -> `scripts/lang/rust/audit-remediation.md`, `scripts/lang/ts/audit-remediation.md`
- Run record (task/gate logs, cost ledger, escalation + infra-incident records, canaries) -> `${DEVLOOP_TMP:-/tmp/devloop}/story-runner/<story>/`

## Host-Side Cluster Helper (ADR-0030)
- Devloop helper binary -> `crates/devloop-helper/src/`
- Port allocation -> `crates/devloop-helper/src/ports.rs`
- Helper commands (setup, deploy, rebuild, teardown, status) -> `crates/devloop-helper/src/commands.rs`
- Helper protocol (command parsing, NDJSON types) -> `crates/devloop-helper/src/protocol.rs`
- Status command (cluster health, pod readiness) -> `commands.rs:cmd_status()`, `parse_pod_health()`
- Port-map.env + DT_HOST_GATEWAY_IP -> `commands.rs:write_port_map_shell()`, `cmd_setup()`, `cmd_deploy()`
- Port registry (global, all devloops) -> `~/.cache/devloop/port-registry.json`
- Per-devloop runtime state (PID, socket, auth token, ports.json, log) -> `/tmp/devloop-{slug}/`; container mount -> `devloop.sh:500`
- Env-test URL config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`

## Health Probes
- MC health endpoints (liveness + readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`
- MC/GC/MH probe config -> `infra/services/{mc,gc,mh}-service/*deployment.yaml` (livenessProbe / readinessProbe)

## Advertise Address Config (GC Registration)
- MC/MH config fields -> `crates/mc-service/src/config.rs`, `crates/mh-service/src/config.rs`
- MC/MH registration -> `crates/mc-service/src/grpc/gc_client.rs:register()`, `crates/mh-service/src/grpc/gc_client.rs:register()`

## Integration Seams
- CanaryPod (NetworkPolicy testing) -> `crates/env-tests/src/canary.rs`
- Env-tests (cluster health, observability, resilience) -> `crates/env-tests/tests/`
