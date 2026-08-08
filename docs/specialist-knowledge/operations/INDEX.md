# Operations Navigation

## Architecture & Design
- Infra (Kind, zero-trust) → ADR-0012; Local dev → ADR-0013; Env tests → ADR-0014
- Guard pipeline → ADR-0015; CI gates / Agent-Teams workflow / Cross-Boundary Ownership Model → ADR-0024 §6; Containerized devloop → ADR-0025
- Host-side cluster helper → ADR-0030; Dashboard metrics (counters vs rates) → ADR-0029
- Metric testability (single presence guard, Cat A/B/C rollout, raw `/metrics` evidence, per-service SLO sub-targets) → ADR-0032; service-owned dashboards/alerts (collapsed Phase 4) → ADR-0031
- Guard pipeline as Rust binary `dt-guard` (single `crates/dt-guard/` workspace member; subcommand per policy; `serde_yaml` typed schema + `regex` DFA + `cargo audit` via Layer 6; D-1 promtool-parallel + D-2 grafana-provisioning-dry-run preserved as Wave 4) → ADR-0034

## CI & Guards
- CI pipeline → `.github/workflows/ci.yml`; guard runner + common (`parse_cross_boundary_table()`, `GUARD_TIMEOUT_SECS`) → `scripts/guards/run-guards.sh`, `common.sh`; `dt-guard` CLI (subcommand wrapper, `--explain`) → `crates/dt-guard/src/main.rs`, ADR-0033 §6
- Devloop validation runbook (dt-guard triage §6.3.1, STATUS ladder §3, dep-change gate §6.6, browser-E2E §6.7, symptom catalogue §8) → `docs/runbooks/devloop-validation.md`; STATUS rank SSoT → `scripts/guards/common.sh:__status_rank`
- Layer 7 browser-E2E preconditions + Phase-2 lane → `scripts/layer7.sh`; Layer 6 audit dep-change gate (manifest-only) → `scripts/lang/_audit_gate.sh`, ADR-0033 §3
- Debate of record (supersede debate + predecessor debate + per-guard vendor-coverage matrix) → `docs/debates/2026-05-17-guard-toolchain-supersede/debate.md`, `docs/debates/2026-05-14-python-guard-pipeline-strategy/debate.md`, `guard-vendor-coverage-matrix.md`
- Simple guards: kustomize → `scripts/guards/simple/validate-kustomize.sh`; app metrics → `validate-application-metrics.sh`; alert-rules → `validate-alert-rules.sh` (conventions → `docs/observability/alert-conventions.md`); metric-test coverage → `validate-metric-coverage.sh`, ADR-0032
- Cross-boundary guards (Layer A scope-drift, Layer B classification-sanity; ADR-0024 §6) → `scripts/guards/simple/validate-cross-boundary-scope.sh`, `validate-cross-boundary-classification.sh`, `cross-boundary-ownership.yaml`

## Devloop Cluster Helper
- Kind config template (envsubst, host-gateway listenAddress) → `infra/kind/kind-config.yaml.tmpl`
- Devloop wrapper → `infra/devloop/devloop.sh` (health check + eager setup), Dockerfile → `infra/devloop/Dockerfile`; container-side client → `infra/devloop/dev-cluster`
- Helper commands (setup/deploy/rebuild/teardown/status; `write_port_map_shell()`, DT_HOST_GATEWAY_IP propagation) → `crates/devloop-helper/src/commands.rs`, `protocol.rs`; port registry → `~/.cache/devloop/port-registry.json`; per-devloop state → `/tmp/devloop-{slug}/`
- Env-test URL config → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`; Layer 7 → `.claude/skills/devloop/SKILL.md`

## Deployment & K8s
- Kind cluster: `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh` (ADR-0030: `load_image_to_kind()`, `deploy_only_service()`, --yes/--only/--skip-build), `infra/kind/scripts/teardown.sh`
- Per-service Kustomize bases + manifests (statefulset/deployment, netpol, PDB) → `infra/services/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`
- Dockerfiles → `infra/docker/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`; PostgreSQL + Redis → `infra/services/postgres/`, `redis/`
- Dev certs → `scripts/generate-dev-certs.sh`; Alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`, template → `_template-service-alerts.yaml`
- MC/MH per-instance Deployments + ConfigMaps (advertise addresses) → `infra/services/mc-service/mc-{0,1}-configmap.yaml`, `mh-service/mh-{0,1}-configmap.yaml`; devloop patching + DT_HOST_GATEWAY_IP validation → `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- MH shared config (AC_JWKS_URL FQDN form for JWKS-based JWT validation) → `infra/services/mh-service/configmap.yaml`; per-deployment env ref → `mh-{0,1}-deployment.yaml`
- Per-pod UDP NodePorts: `base + ordinal*2` (MC: 4433/4435, MH: 4434/4436); cross-service netpol in `gc-service/network-policy.yaml`, `mc-service/network-policy.yaml`, `mh-service/network-policy.yaml`; MH→MC gRPC TCP 50052 (egress on MH, ingress on MC)

## Runbooks & Database
- Per-service incident/deployment → `docs/runbooks/` (ac, gc, mc, mh); update heuristics — additive `# Note:` + disambiguation PromQL for metric-family reuse, first-emission "is this an incident?" footnotes → `gc-incident-response.md` Scenario 5 (ADR-0032 Step 5)
- MH/MC incident scenarios (MH Sc 13/14 RegisterMeeting-timeout + WT-startup; MC Sc 12/13 RegisterMeeting coordination + unexpected-notifications; MC Sc 14 roster-remove latency + idle-timeout knob `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS` in `crates/mc-service/src/config.rs`) → `docs/runbooks/mh-incident-response.md`, `mc-incident-response.md`
- MH post-deploy checklist (30m/2h/4h/24h + R-36 rollback; `or vector(0)` denominator-guard antipattern — use `and sum(rate(...)) > 0` for rollback ratios) → `docs/runbooks/mh-deployment.md`; MC↔MH addendum (cross-pointers + `mc_mh_notifications_received_total`) → `docs/runbooks/mc-deployment.md`
- GC deploy smoke (CORS + telemetry-proxy stanzas, OTLP fixture `infra/smoke/empty-otlp-metrics.bin`) → `docs/runbooks/gc-deployment.md`; GC telemetry/CORS incident scenarios (Sc 10-13) → `docs/runbooks/gc-incident-response.md`; alerts → `infra/docker/prometheus/rules/gc-alerts.yaml`
- Client dev-local runbook (bundler preflight, engine-strict fail-loud) → `docs/runbooks/client-dev-local.md`, `scripts/dev-web.sh:probe_bundler()`, `.npmrc`
- Participant tracking + meetings → `crates/gc-service/src/repositories/participants.rs`, `meetings.rs`

## Auth & JWT
- Common JWKS + JWT → `crates/common/src/jwt.rs`; shared GC↔AC token types → `crates/common/src/meeting_token.rs`; AC rate limits → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`; Service auth → ADR-0003

## Observability
- Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/` (MH SLO + logs → `mh-slos.json`, `mh-logs.json`; provisioning → `infra/grafana/kustomization.yaml`); Alerts → `docs/observability/alerts.md`; Prometheus → `infra/docker/prometheus/prometheus.yml`; per-service `crates/ac-service/src/observability/metrics.rs`, `gc-service/`, `mc-service/`, `mh-service/`
- Shared `MetricAssertion` testing helper (per-thread `DebuggingRecorder`, `!Send` snapshots, drain-on-read histograms) → `crates/common/src/observability/testing.rs`; `assert_unobserved` (additive across all 3 query types — counter hard-form, gauge gap-fill, histogram observation-count equivalence + kind-mismatch hardening) added in ADR-0032 Step 4, no breaking changes to MH/MC callers
- Adding label to existing metric: 3-step PromQL audit (grep `without()` split-risk; histogram `sum by(le)` drops new label; counter ratio bare `sum(rate)` aggregates over new label) + Cat A canary `/metrics` template (3 curl checks: non-zero counts ≥2 label values, `_count` increment, `wc -l` cardinality) → `docs/devloop-outputs/2026-04-27-adr-0032-step-5-gc-metric-test-backfill/main.md` §Cat A canary acceptance criteria

## AC Service
- AC K8s manifests → `infra/services/ac-service/configmap.yaml`, `statefulset.yaml`, `service.yaml`, `service-monitor.yaml`, `network-policy.yaml`, `pdb.yaml`, `kustomization.yaml`
- AC runbooks → `docs/runbooks/ac-service-deployment.md`, `docs/runbooks/ac-service-incident-response.md`
- AC metric catalog → `docs/observability/metrics/ac-service.md` (1:1 with `metrics.rs` emissions; runbook PromQL/curl examples reference metric names by string — production wrapper signatures must stay byte-identical to keep runbooks valid)
- AC metric-test backfill (ADR-0032 Step 4, Pure Cat C, 17 metrics → 0; per-cluster `MetricAssertion`-backed; production wrappers untouched) → `crates/ac-service/src/observability/metrics.rs`, `crates/ac-service/tests/` (13 `*_integration.rs` cluster files), fixtures → `crates/ac-service/tests/common/test_state.rs`
- AC bcrypt cost-12 (`DEFAULT_BCRYPT_COST`) load-bearing for `ac_bcrypt_duration_seconds` histogram-bucket fidelity; `MIN_BCRYPT_COST` (10) for incidental scaffolding only → `crates/ac-service/tests/bcrypt_metrics_integration.rs:12-22`
- Test-build dev-dep feature-flag pattern (`common` w/ `test-utils` in `[dev-dependencies]`) → `crates/ac-service/Cargo.toml`, `crates/mc-service/Cargo.toml`, `crates/mh-service/Cargo.toml`

## MH Service
- MH startup + config + health → `crates/mh-service/src/main.rs`, `config.rs`, `observability/health.rs`
- MH gRPC (service, GC/MC clients, JWKS auth) → `crates/mh-service/src/grpc/mh_service.rs`, `gc_client.rs`, `mc_client.rs`, `auth_interceptor.rs`; MH→MC fire-and-forget notify → `crates/mh-service/src/webtransport/connection.rs:spawn_notify_connected()`
- MH WebTransport + session mgmt → `crates/mh-service/src/webtransport/server.rs`, `connection.rs`, `crates/mh-service/src/session/mod.rs`
- MH crate integration tests + shared rigs (RAII Drop, `127.0.0.1:0`) → `crates/mh-service/tests/`; accept-loop component rig (`WebTransportServer::bind()`, runtime `rcgen` PEMs) → `tests/common/accept_loop_rig.rs`
- MH token-refresh metric extraction (ADR-0032 Cat B) → `crates/mh-service/src/observability/metrics.rs:record_token_refresh_metrics()`

## MC Service
- MC startup + gRPC wiring → `crates/mc-service/src/main.rs`, `config.rs`; WebTransport → `crates/mc-service/src/webtransport/server.rs`, `connection.rs`
- MC GC client → `crates/mc-service/src/grpc/gc_client.rs`; MH client (MhRegistrationClient trait) → `crates/mc-service/src/grpc/mh_client.rs`; async RegisterMeeting trigger (first-participant, retry+backoff, cancel-aware) → `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`
- MC gRPC services (GC→MC assignments, MH→MC MediaCoordination) → `crates/mc-service/src/grpc/mc_service.rs`, `media_coordination.rs`; JWKS auth → `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer`
- MhConnectionRegistry (cleanup wired in `actors/controller.rs:remove_meeting()`) → `crates/mc-service/src/mh_connection_registry.rs`; idempotent MH-retry invariant (disconnect after registry-clear returns Ok) → `crates/mc-service/src/grpc/media_coordination.rs:test_coordination_flow_connect_disconnect_round_trip()`
- Redis (fenced writes, MhAssignmentData, MhAssignmentStore trait) → `crates/mc-service/src/redis/client.rs`; actors → `crates/mc-service/src/actors/controller.rs`, `meeting.rs`, `participant.rs`
- MC token-refresh metric (ADR-0032 Cat B) → `crates/mc-service/src/observability/metrics.rs:record_token_refresh_metrics()`; accept-loop rig → `crates/mc-service/tests/common/accept_loop_rig.rs`

## GC Service + Tests
- GC routes + handlers → `crates/gc-service/src/routes/mod.rs`, `handlers/meetings.rs`; MC/GC join tests → `crates/mc-service/tests/join_tests.rs`, `crates/gc-service/tests/meeting_tests.rs`; TestKeypair → `crates/mc-test-utils/src/jwt_test.rs`; Env-tests → `crates/env-tests/`
