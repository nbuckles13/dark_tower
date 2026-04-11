# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates, increase/rate classification) -> ADR-0029
- Host-side cluster helper (observability access, port discovery, health gating, listenAddress fix) -> ADR-0030

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `docs/observability/metrics/gc-service.md`, `docs/observability/metrics/mc-service.md`, `docs/observability/metrics/mh-service.md`
- AC metrics -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`, gauge init `services/key_management_service.rs:init_key_metrics()`, HTTP middleware `middleware/http_metrics.rs`, rate limit config `config.rs`
- GC metrics -> `crates/gc-service/src/observability/metrics.rs`, HTTP middleware `middleware/http_metrics.rs:normalize_endpoint()`, join wiring `handlers/meetings.rs:join_meeting()`, DB metrics `repositories/`; gap: `get_guest_token()` uninstrumented
- MC metrics -> `crates/mc-service/src/observability/metrics.rs`; recording sites: `webtransport/connection.rs:handle_connection()`, `server.rs:accept_loop()`; bounded labels `errors.rs:error_type_label()`
- MH metrics (registration, heartbeat, token refresh, gRPC) -> `crates/mh-service/src/observability/metrics.rs`

## Auth & JWT Tracing
- Common JWT (JwksClient, JwtValidator, verify_token, PII-redacted Debug) -> `crates/common/src/jwt.rs`
- GC/MC auth wrappers -> `crates/gc-service/src/auth/jwt.rs:JwtValidator`, `crates/mc-service/src/auth/mod.rs:McJwtValidator` (target: `mc.auth`)

## MC WebTransport Tracing
- Server/connection/handler (targets: `mc.webtransport`, `.connection`, `.handler`) -> `crates/mc-service/src/webtransport/server.rs`, `connection.rs`, `handler.rs`
- ParticipantActor (target: `mc.actor.participant`) -> `crates/mc-service/src/actors/participant.rs:run()`

## GC Client Tracing (MC + MH)
- GcClient tracing (registration, heartbeat, re-registration) -> `crates/mc-service/src/grpc/gc_client.rs` (+ mh)

## Health
- MC health state -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health state (ready after GC registration) -> `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards, Alerts & Infrastructure
- Grafana dashboards (per-service overview, errors-overview, SLOs) -> `infra/grafana/dashboards/`, provisioning `infra/grafana/provisioning/`
- Grafana K8s (configMapGenerator, sidecar, RBAC) -> `infra/kubernetes/observability/grafana/`
- Alert rules (GC, MC) -> `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`; docs -> `docs/observability/alerts.md`, `docs/observability/dashboards.md`
- Prometheus config -> `infra/docker/prometheus/prometheus.yml` (compose), `infra/kubernetes/observability/prometheus-config.yaml` (K8s)
- Loki + observability kustomization -> `infra/kubernetes/observability/{loki-config,kustomization}.yaml`
- Kind observability NodePorts (30090/30030/30080) -> `infra/kind/kind-config.yaml`

## Kind Cluster Setup (Observability)
- Setup (deploy_observability, setup_port_forwards, deploy_only_service, DT_CLUSTER_NAME/DT_PORT_MAP) -> `infra/kind/scripts/setup.sh`
- Teardown (parameterized cluster name, scoped pkill) -> `infra/kind/scripts/teardown.sh`

## Devloop Cluster Helper (Observability)
- Kind config template (listenAddress: ${HOST_GATEWAY_IP}, dynamic observability ports) -> `infra/kind/kind-config.yaml.tmpl`
- Port map (prometheus, grafana, loki port discovery) -> `/tmp/devloop-{slug}/ports.json`
- Env-test observability URL config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- Env-tests observability validation -> `crates/env-tests/tests/30_observability.rs`

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Dashboard-to-kustomize coverage (R-20, bidirectional) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`

## Env-Test Observability & Cluster Config
- ClusterPorts/health checks (from_env, parse_host_port) -> `crates/env-tests/src/cluster.rs`; feature-gated tests -> `tests/30_observability.rs`
- MC/MH K8s health probes + metrics scrape -> `infra/services/{mc,mh}-service/deployment.yaml`

## Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service)
- GC scenarios 8-9 + join failure triage -> `docs/runbooks/gc-incident-response.md`, `docs/observability/alerts.md`

## Test Coverage & Integration Seams
- GC/MC/MH metrics tests -> `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`
