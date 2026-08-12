# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline + cross-boundary ownership model -> ADR-0024, ADR-0024 §6
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates) -> ADR-0029
- Host-side cluster helper (observability access, port discovery, health gating) -> ADR-0030
- Service-owned dashboards and alerts; observability as cross-cutting reviewer -> ADR-0031
- Metric testability (component tests + `MetricAssertion` + presence guard) -> ADR-0032
- Polyglot validation pipeline -> ADR-0033
- Guard pipeline as Rust binary; `dt-guard <subcommand> --explain` -> ADR-0034, ADR-0034 §7; fixtures `crates/dt-guard/tests/fixtures/`; vendor-coverage matrix `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md`
- Deterministic story runner (run-story) -> ADR-0035
- Open observability debt -> `docs/TODO.md` §Observability Debt

## Story Runner Telemetry (ADR-0035)
Evidence artifacts, not monitored signals — ADR-0032's metric-governance regime does not apply here.
- Cost ledger emitter, canary classification, gate loop, run-record dir -> `scripts/workflow/run-story.sh:report_task_cost()`, `canary_probe()`, `canary_classify()`, `RUN_DIR`
- Substrate probe + settings registration; devloop completion enforcement -> `scripts/workflow/preflight-story.sh`, `scripts/workflow/devloop-stop-hook.sh`
- Gate-2 verdict emitter (per-layer `LAYER n STATUS DURATION`; ephemeral-only guard) -> `scripts/lang/_gate2_binding.sh:emit_gate2_verdict()`
- Task manifest state machine -> `crates/dt-story/src/engine.rs`, `crates/dt-story/src/manifest.rs`, `crates/dt-story/src/markdown.rs`
- Per-task devloop records (one dir per devloop) -> `docs/devloop-outputs/`, template `docs/devloop-outputs/_template/main.md`

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `docs/observability/metrics/gc-service.md`, `docs/observability/metrics/mc-service.md`, `docs/observability/metrics/mh-service.md`
- AC -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, `record_meeting_display_name_outcome()`; gauge init `services/key_management_service.rs:init_key_metrics()`; HTTP middleware `middleware/http_metrics.rs`; rate-limit config `config.rs`; emission site `handlers/internal_tokens.rs:resolve_meeting_display_name()`
- GC -> `crates/gc-service/src/observability/metrics.rs`; middleware `middleware/http_metrics.rs:normalize_endpoint()`; join wiring `handlers/meetings.rs:join_meeting()`, `get_guest_token()`; DB `repositories/`; telemetry-proxy + CORS `handlers/telemetry.rs`, `services/telemetry_filter.rs`, `middleware/cors_observer.rs`
- MC -> `crates/mc-service/src/observability/metrics.rs:record_display_name_resolution()`, `record_participant_leave()`, `record_participant_disconnect()`; bounded labels `errors.rs:error_type_label()`; emission sites `actors/meeting.rs:handle_join()`, `handle_disconnect()`, `grpc/media_coordination.rs`
- MH -> `crates/mh-service/src/observability/metrics.rs`; emission sites `grpc/auth_interceptor.rs:MhAuthService`, `grpc/mc_client.rs`

## Auth & JWT Tracing
- Common JWT (JwksClient, JwtValidator, verify_token, PII-redacted Debug) -> `crates/common/src/jwt.rs`
- Service wrappers -> `crates/gc-service/src/auth/jwt.rs:JwtValidator`, `crates/mc-service/src/auth/mod.rs:McJwtValidator`, `crates/mh-service/src/auth/mod.rs:MhJwtValidator`
- MH gRPC auth (JWKS ServiceClaims validation, tower Layer) -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`

## Service Tracing & Health
- MC WebTransport -> `crates/mc-service/src/webtransport/server.rs`, `connection.rs`, `handler.rs`; ParticipantActor `crates/mc-service/src/actors/participant.rs:run()`
- MH WebTransport -> `crates/mh-service/src/webtransport/server.rs`, `crates/mh-service/src/webtransport/connection.rs`
- MC gRPC clients -> `crates/mc-service/src/grpc/gc_client.rs`, `crates/mc-service/src/grpc/mh_client.rs`; RegisterMeeting trigger `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`
- Health routers -> `crates/mc-service/src/observability/health.rs:health_router()`, `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards & Alerts
- Dashboards + provisioning + K8s wiring -> `infra/grafana/dashboards/`, `infra/grafana/provisioning/`, `infra/grafana/kustomization.yaml`
- Per-service overviews -> `infra/grafana/dashboards/ac-overview.json`, `gc-overview.json`, `mc-overview.json`, `mh-overview.json`; template `_template-service-overview.json`; MH logs + SLOs `mh-logs.json`, `mh-slos.json`
- Grafana K8s (configMapGenerator, sidecar, RBAC) -> `infra/kubernetes/observability/grafana/`
- Alert rules -> `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`, `mh-alerts.yaml`, template `_template-service-alerts.yaml`
- Conventions + docs -> `docs/observability/alert-conventions.md`, `alerts.md`, `dashboards.md`

## Observability Infrastructure
- Prometheus + Loki -> `infra/docker/prometheus/prometheus.yml`, `infra/kubernetes/observability/prometheus-config.yaml`, `infra/kubernetes/observability/loki-config.yaml`, `infra/kubernetes/observability/kustomization.yaml`
- Kind NodePorts -> `infra/kind/kind-config.yaml`; template `infra/kind/kind-config.yaml.tmpl`
- MC/MH probes + scrape config (per-instance workloads) -> `infra/services/mc-service/mc-0-deployment.yaml`, `mc-1-deployment.yaml`, `infra/services/mh-service/mh-0-deployment.yaml`, `mh-1-deployment.yaml`
- Cluster setup / teardown -> `infra/kind/scripts/setup.sh`, `infra/kind/scripts/teardown.sh`
- Devloop helper (port map, status, audit log) -> `crates/devloop-helper/src/commands.rs:write_port_map_shell()`, `cmd_status()`, `parse_pod_health()`, `crates/devloop-helper/src/logging.rs:AuditLog`, `/tmp/devloop-{slug}/ports.json`, `infra/devloop/dev-cluster`, `infra/devloop/devloop.sh`
- Env-tests observability -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`, `crates/env-tests/tests/30_observability.rs`; Layer 7 -> `.claude/skills/devloop/SKILL.md`

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Metric-test coverage (src emission vs test reference) -> `scripts/guards/simple/validate-metric-coverage.sh`
- Dashboard-to-kustomize coverage (bidirectional) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`
- Alert-rules lint -> `scripts/guards/simple/validate-alert-rules.sh`
- PII / secret identifier vocabulary -> `crates/dt-guard/src/common/pii_vocabulary.rs`, `scripts/guards/simple/no-pii-in-logs.sh`, `scripts/guards/simple/ts/no-pii-in-logs-ts.sh`
- Metric label + naming policies -> `crates/dt-guard/src/metric_labels.rs`, `crates/dt-guard/src/ts_metric_naming.rs`; `MetricAssertion` test helper `crates/common/src/observability/testing.rs`

## Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service); GC join-failure triage, telemetry-proxy + CORS scenarios -> `docs/runbooks/gc-incident-response.md`, `docs/runbooks/gc-deployment.md`; OTLP smoke fixture `infra/smoke/empty-otlp-metrics.bin`

## Test Coverage & Integration Seams
- MH/MC accept-loop rigs (ADR-0032 Steps 2-3) -> `crates/mh-service/tests/common/accept_loop_rig.rs`, `crates/mh-service/tests/webtransport_accept_loop_integration.rs`, `crates/mc-service/tests/common/accept_loop_rig.rs`, `crates/mc-service/tests/webtransport_accept_loop_integration.rs`
- AC cluster component tests (ADR-0032 Step 4) -> `crates/ac-service/tests/*_integration.rs`
- GC cluster component tests (ADR-0032 Step 5) -> `crates/gc-service/tests/*_integration.rs`, `crates/gc-service/src/handlers/meetings.rs`
