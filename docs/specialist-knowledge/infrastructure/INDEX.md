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
- Service Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`; migrations image (sqlx-cli + `migrations/`) -> `infra/docker/db-migrate/Dockerfile`; Compose (local tests) -> `docker-compose.test.yml`; dev TLS certs (CA + MC + MH) -> `scripts/generate-dev-certs.sh`
- Service runbooks (`<service>-deployment.md` + `<service>-incident-response.md`) -> `docs/runbooks/`; local web demo bring-up -> `docs/runbooks/client-dev-local.md`
- Prometheus config, kustomization, per-service alert rules -> `infra/docker/prometheus/`; K8s scrape config -> `infra/kubernetes/observability/{prometheus.yml,prometheus-config.yaml}`
- K8s service manifests (Kustomize bases) -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- Service workloads: AC StatefulSet -> `infra/services/ac-service/statefulset.yaml`; GC Deployment -> `infra/services/gc-service/deployment.yaml`; MC/MH per-instance Deployments + PDB -> `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-deployment.yaml`, `pdb.yaml`
- MC/MH ConfigMap generator sources (shared + per-instance; MH transport parameters) -> `infra/services/{mc,mh}-service/config.env`, `{mc,mh}-{0,1}-config.env`
- Network policies (per-service ingress/egress) -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`; MC/MH per-instance Services (NodePorts) -> `infra/services/{mc,mh}-service/service.yaml`
- OTel Collector base (configmap, deployment, service, network policy) -> `infra/services/otel-collector/`; smoke payload -> `infra/smoke/`
- Redis + PostgreSQL manifests (Kustomize bases) -> `infra/services/{redis,postgres}/kustomization.yaml`; PostgreSQL init -> `infra/docker/postgres/init.sql`
- K8s observability manifests (Prometheus, Loki/Promtail, kube-state-metrics, node-exporter) -> `infra/kubernetes/observability/kustomization.yaml`
- Grafana manifests, dashboards + provisioning -> `infra/grafana/kustomization.yaml`, `infra/grafana/{dashboards,provisioning}/`
- Kind ENVIRONMENT ROOT (ADR-0038; the one thing deploy.sh applies), observability, per-service patches (OTel endpoint, CORS, NodePorts, MH egress budget) -> `infra/kubernetes/overlays/kind/{kustomization.yaml,observability/,services/}`
- Content-addressed ConfigMaps (ADR-0038 §2): every pod-consumed ConfigMap is a `configMapGenerator` (service `*.env`, `otel-collector/collector.yaml`, `redis/redis.conf`, `observability/{loki,promtail,prometheus}.yml`, `infra/grafana/` (dashboards via one projected volume)); guard R-21 -> `crates/dt-guard/src/kustomize_content_addressing.rs`; the ONE generator parser + env-file reader + containment-gated source resolver (`generator_data`; used by R-16, R-21, env-config and the annotation-size rule) -> `crates/dt-guard/src/common/kustomize_generators.rs`; pod-template ConfigMap-ref walker -> `crates/dt-guard/src/common/pod_spec.rs`
- Kind scripts, split by lifecycle (ADR-0038 step 3): PROVISION (platform; blueprint render/check/record, AC + MC/MH TLS Secrets, rebuilt only on a blueprint change) -> `infra/kind/scripts/provision.sh` (`blueprint_render()` — the ONE render shared by build, `--check` and deploy's guard; `PROVISION_INPUTS`/`provision_input()`; `record_blueprint()`; `create_tls_secret()`); DEPLOY (application, every run) -> `infra/kind/scripts/deploy.sh`; host one-stop entry + `--provision-org` -> `infra/kind/scripts/setup.sh`; shared prelude (hashed, definitions-only) -> `infra/kind/scripts/lib/common.sh` (`dt_init_cluster_env()`, `validate_cluster_name()`, `cluster_exists()`); cluster-DB helpers (not hashed) -> `lib/cluster-db.sh` (`dt_psql()`); TLS recipe + its read-only `--check-renewal` -> `scripts/generate-dev-certs.sh`; kind config -> `infra/kind/kind-config.yaml`, `kind-config.yaml.tmpl`; iterate/teardown -> `infra/kind/scripts/{iterate,teardown}.sh`
- Content-tagged images (ADR-0038 §2; the ONE tag derivation) -> `deploy.sh:content_tag()` (tag = `sha-<image id>`), `build_content_tagged_image()` (kind-loaded only when the node lacks the ref), `first_party_repos()` (derived from the root render), `deployed_refs()`/`resolve_image_refs()` (every ref built this run; the pre-deploy ref is remembered), `prune_superseded_images()` (keeps current + previous generation); bases carry `localhost/<svc>-service:render-required` + `imagePullPolicy: Never`; migration Job (ADR-0038 §2 step 3; Complete BEFORE the root) -> base `infra/services/db-migrate/`, `deploy.sh:render_migration_job()`/`migration_job_name()` (hash-named), `run_migration_job()` (`REASON=migration-failed|migration-timeout`); Cargo.lock version reader (sqlx-cli pin, shared with devloop.sh) -> `infra/lib/cargo-lock-version.sh`
- Kind script parameterization (DT_CLUSTER_NAME, DT_PORT_MAP, DT_HOST_GATEWAY_IP, DT_KIND_CONFIG, DT_ORG_MAX_CONCURRENT_MEETINGS; provision.sh `--yes|--check|--blueprint`, deploy.sh `--yes`, setup.sh `--yes|--provision-org`) -> ADR-0030/0038; deploy.sh helpers -> `main()` (the converge), `check_blueprint()` (refuses a stale/missing platform), `apply_env_root()` (the ONLY root apply; devloop wrapper choice inside), `render_env_overlay()` (per-cluster advertise addresses as a render input), `advertise_instances()`, `env_root_workloads()`/`wait_for_env_root()` (render-derived wait set; `DEPLOY_FAILED REASON=… WORKLOADS=…`), `deploy_otel_collector()` (collector Ready before the root; `collector-rollout-failed`), `preload_third_party_images()`; `setup.sh:provision_run_org()`; tests -> `scripts/setup.test.sh`; verb/flag vocabulary drift guard -> `scripts/guards/simple/validate-dev-cluster-verbs.sh`
- Containerized devloop + dev-cluster CLI -> `infra/devloop/{devloop.sh,dev-cluster}`; container start -> `infra/devloop/entrypoint.sh`
- Web-app demo launcher (topology preflight, then Vite) -> `scripts/dev-web.sh`; test `scripts/dev-web.test.sh`

## Guard Pipeline (ADR-0034)
- Binary + wrapper shape (subcommand-per-policy, clap dispatcher, STATUS line per ADR-0033 §6; wrappers source the shared prelude per ADR-0034 §3) -> `crates/dt-guard/`, `scripts/guards/simple/*.sh`; canonical-home `Lazy<Regex>` convention -> `clippy.toml`
- Canonical `{ac,gc,mc,mh}` service enumeration SSoT; Bash mirror -> `crates/dt-guard/src/common/services.rs`, `scripts/guards/common.sh`
- Kustomize policy (build, orphan manifests, kubeconform, securityContext) -> `crates/dt-guard/src/kustomize.rs`, `kustomize_tools.rs`; ConfigMap annotation-size cap -> `kustomize_configmaps.rs`; R-18 kinds (security-owned policy: Deployment, StatefulSet, Job, CronJob) -> `kustomize_tools.rs:SECURITY_CONTEXT_KINDS`
- Env-config policy (per-workload env-var coverage, `configMapKeyRef` resolution, key==env-name) -> `crates/dt-guard/src/env_config.rs`; live-cluster twin for MH -> `crates/env-tests/tests/01_mh_deployment_config.rs`
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
- Hermetic script self-tests + wiring -> `scripts/layer3.sh`; suites -> `scripts/layer7.test.sh`, `scripts/setup.test.sh`, `scripts/lang/rust/behavior-equivalence.test.sh` (Rust test verb + unit-test DB migration fails loudly), `scripts/layer-all.test.sh`, `scripts/workflow/run-story.test.sh`; shared helper SSoT -> `scripts/lang/_test_helpers.sh`
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
- Helper binary: port allocation -> `crates/devloop-helper/src/ports.rs`; commands -> `crates/devloop-helper/src/commands.rs` (verbs `VERBS` in protocol.rs — provision/deploy/teardown/recreate/restore-kubeconfig/status/cancel, none takes an argument; provision/deploy run the Kind scripts via `script_command()`; status exempts Job-owned Succeeded pods in `parse_pod_health()`); NDJSON protocol -> `crates/devloop-helper/src/protocol.rs`
- Cluster health + port-map writers -> `commands.rs:cmd_status()`, `parse_pod_health()`, `write_port_map_shell()`, `cmd_provision()`, `cmd_deploy()`
- Port registry (global) -> `~/.cache/devloop/port-registry.json`; per-devloop runtime state (PID, socket, auth token, ports.json, log) -> `/tmp/devloop-{slug}/`; container mount -> `devloop.sh:500`
- Env-test URL config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`

## Service Config, Probes & Integration Seams
- MC health endpoints (liveness + readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`; probe config -> `infra/services/{mc,gc,mh}-service/*deployment.yaml`
- MC/MH advertise-address config -> `crates/mc-service/src/config.rs`, `crates/mh-service/src/config.rs`; GC registration -> `crates/mc-service/src/grpc/gc_client.rs:register()`, `crates/mh-service/src/grpc/gc_client.rs:register()`
- CanaryPod (NetworkPolicy testing) -> `crates/env-tests/src/canary.rs`; env-tests (cluster health, observability, resilience) -> `crates/env-tests/tests/`
