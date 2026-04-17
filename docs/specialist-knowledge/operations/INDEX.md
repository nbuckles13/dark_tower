# Operations Navigation

## Architecture & Design
- Infra (Kind, zero-trust) → ADR-0012; Local dev → ADR-0013; Env tests → ADR-0014
- Guard pipeline → ADR-0015; CI gates → ADR-0024; Containerized devloop → ADR-0025
- Host-side cluster helper → ADR-0030; Dashboard metrics (counters vs rates) → ADR-0029
- Metric testability (single presence guard, Cat A/B/C rollout, raw `/metrics` evidence, per-service SLO sub-targets) → ADR-0032; service-owned dashboards/alerts (collapsed Phase 4) → ADR-0031

## CI & Guards
- CI pipeline → `.github/workflows/ci.yml`; runner + common → `scripts/guards/run-guards.sh`, `common.sh`
- Kustomize → `scripts/guards/simple/validate-kustomize.sh`; app metrics (metric↔dashboard) → `validate-application-metrics.sh`; alert-rules → `validate-alert-rules.sh`, `alert-rules.legacy-allowlist`, conventions → `docs/observability/alert-conventions.md`
- Metric-test coverage guard (`validate-metric-coverage.sh`, single presence check; lead sequences per-service backfill PRs during phasing window; MH ✓ + MC ✓ + AC ✓ + GC ✓ (all four `0 uncovered` after ADR-0032 Step 5, 2026-04-27 — `run-guards.sh` fully GREEN on `feature/mh-quic-mh-tests`, branch ready to merge) → ADR-0032

## Devloop Cluster Helper
- Kind config template (envsubst, host-gateway listenAddress) → `infra/kind/kind-config.yaml.tmpl`
- Devloop wrapper → `infra/devloop/devloop.sh` (health check + eager setup), Dockerfile → `infra/devloop/Dockerfile`; container-side client → `infra/devloop/dev-cluster`
- Helper commands (setup, deploy, rebuild, teardown, status; `write_port_map_shell()`, DT_HOST_GATEWAY_IP propagation in `cmd_setup`/`cmd_deploy`) → `crates/devloop-helper/src/commands.rs`; protocol → `crates/devloop-helper/src/protocol.rs`
- Port registry → `~/.cache/devloop/port-registry.json`; per-devloop runtime state → `/tmp/devloop-{slug}/` (PID, socket, auth token, ports.json, setup.pid, eager-setup.log)
- Env-test URL config → `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`; Layer 8 → `.claude/skills/devloop/SKILL.md`

## Deployment & K8s
- Kind cluster: `infra/kind/kind-config.yaml`, `infra/kind/scripts/setup.sh` (ADR-0030: `load_image_to_kind()`, `deploy_only_service()`, --yes/--only/--skip-build), `infra/kind/scripts/teardown.sh`
- Kind overlay (per-service, observability) → `infra/kubernetes/overlays/kind/`
- Per-service Kustomize bases + manifests (statefulset/deployment, netpol, PDB) → `infra/services/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`
- Dockerfiles → `infra/docker/ac-service/`, `gc-service/`, `mc-service/`, `mh-service/`; PostgreSQL + Redis → `infra/services/postgres/`, `redis/`
- Dev certs → `scripts/generate-dev-certs.sh`; Alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`, template → `_template-service-alerts.yaml`
- MC/MH per-instance Deployments + ConfigMaps (advertise addresses) → `infra/services/mc-service/mc-{0,1}-configmap.yaml`, `mh-service/mh-{0,1}-configmap.yaml`; devloop patching + DT_HOST_GATEWAY_IP validation → `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- Per-pod UDP NodePorts: `base + ordinal*2` (MC: 4433/4435, MH: 4434/4436); cross-service netpol in `gc-service/network-policy.yaml`, `mc-service/network-policy.yaml`, `mh-service/network-policy.yaml`
- Downward API: `status.podIP` → `POD_IP`; WebTransport advertise from per-instance ConfigMap
- Port map: AC=8082, GC=8080/50051, MC=8081/50052/4433, MH=8083/50053/4434; scaling requires per-pod Services + Kind port mappings

## Runbooks & Database
- Per-service incident/deployment → `docs/runbooks/` (ac, gc, mc); update heuristics — additive `# Note:` + disambiguation PromQL for metric-family reuse, footnote new label values to pre-empt first-emission "is this an incident?" → `gc-incident-response.md` Scenario 5 (ADR-0032 Step 5 reference)
- Participant tracking + meetings → `crates/gc-service/src/repositories/participants.rs`, `meetings.rs`

## Auth & JWT
- Common JWKS + JWT → `crates/common/src/jwt.rs`
- Shared GC↔AC token types → `crates/common/src/meeting_token.rs`
- AC rate limits → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`; Service auth → ADR-0003

## Observability
- Kustomize + Grafana → `infra/kubernetes/observability/`, `infra/grafana/dashboards/`; Alerts → `docs/observability/alerts.md`; Prometheus → `infra/docker/prometheus/prometheus.yml`; per-service `crates/ac-service/src/observability/metrics.rs`, `gc-service/`, `mc-service/`, `mh-service/`
- Shared `MetricAssertion` testing helper (per-thread `DebuggingRecorder`, `!Send` snapshots, drain-on-read histograms) → `crates/common/src/observability/testing.rs`; `assert_unobserved` (additive across all 3 query types — counter hard-form, gauge gap-fill, histogram observation-count equivalence + kind-mismatch hardening) added in ADR-0032 Step 4, no breaking changes to MH/MC callers
- Adding label to existing metric: 3-step PromQL audit (grep `without()` split-risk; histogram `sum by(le)` drops new label; counter ratio bare `sum(rate)` aggregates over new label) + Cat A canary `/metrics` template (3 curl checks: non-zero counts ≥2 label values, `_count` increment, `wc -l` cardinality) → `docs/devloop-outputs/2026-04-27-adr-0032-step-5-gc-metric-test-backfill/main.md` §Cat A canary acceptance criteria

## AC Service
- AC K8s manifests → `infra/services/ac-service/configmap.yaml`, `statefulset.yaml`, `service.yaml`, `service-monitor.yaml`, `network-policy.yaml`, `pdb.yaml`, `kustomization.yaml`
- AC runbooks → `docs/runbooks/ac-service-deployment.md`, `docs/runbooks/ac-service-incident-response.md`
- AC metric catalog → `docs/observability/metrics/ac-service.md` (1:1 with `metrics.rs` emissions; runbook PromQL/curl examples reference metric names by string — production wrapper signatures must stay byte-identical to keep runbooks valid)
- AC metric-test backfill (ADR-0032 Step 4, Pure Cat C, 17 metrics drained to 0; `#[cfg(test)] mod tests` block migrated from no-recorder smoke tests to per-cluster `MetricAssertion`-backed; production `record_*`/`set_*` wrappers untouched) → `crates/ac-service/src/observability/metrics.rs`, `crates/ac-service/tests/` (13 cluster files: `audit_log_failures_integration.rs`, `bcrypt_metrics_integration.rs`, `credential_ops_metrics_integration.rs`, `db_metrics_integration.rs`, `errors_metric_integration.rs`, `http_metrics_integration.rs`, `internal_token_metrics_integration.rs`, `jwks_metrics_integration.rs`, `key_rotation_metrics_integration.rs`, `rate_limit_metrics_integration.rs`, `token_issuance_service_integration.rs`, `token_issuance_user_integration.rs`, `token_validation_integration.rs`); test fixtures → `crates/ac-service/tests/common/test_state.rs`
- AC bcrypt cost-12 (`DEFAULT_BCRYPT_COST`) load-bearing for `ac_bcrypt_duration_seconds` histogram-bucket fidelity; `MIN_BCRYPT_COST` (10) for incidental scaffolding only → `crates/ac-service/tests/bcrypt_metrics_integration.rs:12-22`, `crates/ac-service/tests/common/test_state.rs`
- Test-build dev-dependency feature-flag pattern (`common = { path = "../common", features = ["test-utils"] }` in `[dev-dependencies]`, confined to test-build, no production-dep-tree impact) → `crates/ac-service/Cargo.toml`, `crates/mc-service/Cargo.toml`, `crates/mh-service/Cargo.toml`

## MH Service
- MH startup + config + health → `crates/mh-service/src/main.rs`, `config.rs`, `observability/health.rs`
- MH gRPC (service, GC client, MC client, JWKS auth) → `crates/mh-service/src/grpc/mh_service.rs`, `gc_client.rs`, `mc_client.rs`, `auth_interceptor.rs`
- MH→MC notifications (fire-and-forget) → `crates/mh-service/src/webtransport/connection.rs:spawn_notify_connected()`; tests → `tests/mc_client_integration.rs`
- MH WebTransport + session mgmt → `crates/mh-service/src/webtransport/server.rs`, `connection.rs`, `session/mod.rs`
- MH crate integration tests + shared rigs (RAII Drop, `127.0.0.1:0`) → `crates/mh-service/tests/` (`auth_layer_integration.rs`, `register_meeting_integration.rs`, `webtransport_integration.rs`, `webtransport_accept_loop_integration.rs`, `token_refresh_integration.rs`, `common/`); accept-loop component rig (real `WebTransportServer::bind()`, runtime `rcgen`-generated PEMs to `tempfile::TempDir`) → `tests/common/accept_loop_rig.rs`
- MH token-refresh metric extraction (ADR-0032 Cat B, stateless, byte-identical emission) → `crates/mh-service/src/observability/metrics.rs:record_token_refresh_metrics()`

## MC Service
- MC startup + gRPC server wiring → `crates/mc-service/src/main.rs`; config → `crates/mc-service/src/config.rs`
- MC WebTransport → `crates/mc-service/src/webtransport/server.rs`, `connection.rs`
- MC GC client → `crates/mc-service/src/grpc/gc_client.rs`; MH client (MhRegistrationClient trait) → `crates/mc-service/src/grpc/mh_client.rs`
- Async RegisterMeeting trigger (first-participant, retry+backoff, cancel-aware) → `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`
- MC gRPC services (GC→MC assignments, MH→MC MediaCoordination) → `crates/mc-service/src/grpc/mc_service.rs`, `media_coordination.rs`; JWKS auth → `auth_interceptor.rs:McAuthLayer`
- MhConnectionRegistry (cleanup wired in controller.rs `remove_meeting()`) → `crates/mc-service/src/mh_connection_registry.rs`
- Idempotent MH-retry invariant (disconnect after registry-clear returns Ok, not gRPC error) → `crates/mc-service/src/grpc/media_coordination.rs:test_coordination_flow_connect_disconnect_round_trip()`
- Redis (fenced writes, MhAssignmentData, MhAssignmentStore trait) → `crates/mc-service/src/redis/client.rs`
- Actors → `crates/mc-service/src/actors/controller.rs`, `meeting.rs`, `participant.rs`
- MCMediaConnectionAllFailed alert → `infra/docker/prometheus/rules/mc-alerts.yaml`; MC token-refresh metric extraction (ADR-0032 Cat B, byte-identical emission) → `crates/mc-service/src/observability/metrics.rs:record_token_refresh_metrics()`; accept-loop component rig + integration tests → `crates/mc-service/tests/common/accept_loop_rig.rs`, `crates/mc-service/tests/`

## GC Service + Tests
- GC routes + handlers → `crates/gc-service/src/routes/mod.rs`, `handlers/meetings.rs`
- MC/GC join tests → `crates/mc-service/tests/join_tests.rs`, `crates/gc-service/tests/meeting_tests.rs`; TestKeypair → `crates/mc-test-utils/src/jwt_test.rs`; Env-tests → `crates/env-tests/`