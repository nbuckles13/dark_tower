# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates, increase/rate classification) -> ADR-0029
- Host-side cluster helper (observability access, port discovery, health gating) -> ADR-0030

## Metrics
- Metric catalogs -> `docs/observability/metrics/{ac,gc,mc,mh}-service.md`
- AC metrics recording -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- AC gauge init at startup -> `crates/ac-service/src/services/key_management_service.rs:init_key_metrics()`
- AC rate limit config (defaults, bounds, startup logging) -> `crates/ac-service/src/config.rs`, `crates/ac-service/src/main.rs`
- AC HTTP metrics middleware -> `crates/ac-service/src/middleware/http_metrics.rs`
- GC metrics recording (init, meeting creation/join) -> `crates/gc-service/src/observability/metrics.rs`
- GC HTTP metrics middleware + endpoint normalization -> `crates/gc-service/src/middleware/http_metrics.rs`, `observability/metrics.rs:normalize_endpoint()`
- GC join handler metrics wiring -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`
- GC DB metrics -> `crates/gc-service/src/repositories/` (meetings.rs, participants.rs)
- GC guest-token handler: NO metrics instrumentation (gap) -> `crates/gc-service/src/handlers/meetings.rs:get_guest_token()`
- MC metrics recording (join, WebTransport, JWT validation) -> `crates/mc-service/src/observability/metrics.rs`
- MC join metrics recording site (connection handler) -> `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MC connection metrics recording site (accept loop) -> `crates/mc-service/src/webtransport/server.rs:accept_loop()`
- MC error type labels (bounded cardinality) -> `crates/mc-service/src/errors.rs:error_type_label()`
- MH metrics (registration, heartbeat, token refresh, gRPC) -> `crates/mh-service/src/observability/metrics.rs`

## Auth & JWT Tracing
- Common JWT (JwksClient, JwtValidator, verify_token) -> `crates/common/src/jwt.rs`
- GC auth wrapper -> `crates/gc-service/src/auth/jwt.rs:JwtValidator`
- MC auth wrapper (target: `mc.auth`) -> `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- PII-redacted claims Debug impls -> `crates/common/src/jwt.rs`

## MC WebTransport Tracing
- Server lifecycle (target: `mc.webtransport`) -> `crates/mc-service/src/webtransport/server.rs`
- Connection handler (target: `mc.webtransport.connection`) -> `crates/mc-service/src/webtransport/connection.rs`
- Protobuf encoding (target: `mc.webtransport.handler`) -> `crates/mc-service/src/webtransport/handler.rs`
- ParticipantActor (target: `mc.actor.participant`) -> `crates/mc-service/src/actors/participant.rs:run()`

## GC Client Tracing (MC + MH)
- GcClient tracing (registration, heartbeat, re-registration) -> `crates/mc-service/src/grpc/gc_client.rs` (+ mh)

## Health
- MC health state -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health state (ready after GC registration) -> `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards, Alerts & Infrastructure
- Grafana dashboards (per-service overview, SLOs, logs, errors) -> `infra/grafana/dashboards/`
- Grafana K8s base (configMapGenerator, sidecar, RBAC, service) -> `infra/kubernetes/observability/grafana/`
- Per-service overview dashboards (AC, GC, MC, MH) -> `infra/grafana/dashboards/{ac,gc,mc,mh}-overview.json`
- Cross-service error dashboard -> `infra/grafana/dashboards/errors-overview.json`
- Alert rules (GC, MC) -> `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Grafana provisioning + K8s kustomization -> `infra/grafana/provisioning/`, `infra/kubernetes/observability/`
- Prometheus config -> `infra/docker/prometheus/prometheus.yml` (compose), `infra/kubernetes/observability/prometheus-config.yaml` (K8s)
- Loki config -> `infra/kubernetes/observability/loki-config.yaml`
- Observability kustomization -> `infra/kubernetes/observability/kustomization.yaml`
- Kind observability NodePorts (Prometheus=30090, Grafana=30030, Loki=30080, static/manual) -> `infra/kind/kind-config.yaml`
- Alert + dashboard docs -> `docs/observability/alerts.md`, `docs/observability/dashboards.md`

## Kind Cluster Setup (Observability)
- Observability deploy (full stack) -> `infra/kind/scripts/setup.sh:deploy_observability()`
- Observability port-forwards (parameterized via DT_PORT_MAP) -> `infra/kind/scripts/setup.sh:setup_port_forwards()`
- Cluster parameterization (DT_CLUSTER_NAME, DT_PORT_MAP, --yes, --only, --skip-build) -> `infra/kind/scripts/setup.sh`
- Single-service redeploy -> `infra/kind/scripts/setup.sh:deploy_only_service()`
- Teardown (parameterized cluster name, scoped pkill) -> `infra/kind/scripts/teardown.sh`
- Kind observability NodePorts -> `infra/kind/kind-config.yaml`

## Devloop Cluster Helper (Observability)
- Helper binary (status command, observability.available flag) -> `crates/devloop-helper/src/main.rs`
- Kind config template (listenAddress: 127.0.0.1, dynamic observability ports) -> `infra/kind/kind-config.yaml.tmpl`
- Port map (prometheus, grafana, loki port discovery) -> `/tmp/devloop-{slug}/ports.json`
- Env-test observability URL config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- Env-tests observability validation -> `crates/env-tests/tests/30_observability.rs`

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Dashboard-to-kustomize coverage (R-20, bidirectional) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`

## Env-Test Observability & Cluster Config
- ClusterPorts::from_env() (ENV_TEST_{PROMETHEUS,GRAFANA,LOKI}_URL) -> `crates/env-tests/src/cluster.rs:from_env()`
- URL parsing + validation (host:port extraction) -> `crates/env-tests/src/cluster.rs:parse_host_port()`
- ClusterConnection health checks (Prometheus, Grafana, Loki) -> `crates/env-tests/src/cluster.rs`
- Observability feature-gated tests -> `crates/env-tests/tests/30_observability.rs`
- MC/MH K8s health probes + metrics scrape -> `infra/services/{mc,mh}-service/deployment.yaml`

## Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service)
- GC scenarios 8-9 + join failure triage -> `docs/runbooks/gc-incident-response.md`, `docs/observability/alerts.md`

## Test Coverage & Integration Seams
- GC/MC/MH metrics tests -> `crates/gc-service/src/observability/metrics.rs` (+ mc, mh)
- MC/MH K8s health probes + metrics scrape -> `infra/services/{mc,mh}-service/deployment.yaml`
