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
- AC rate limit config (defaults, bounds, startup logging) -> `crates/ac-service/src/config.rs`, `src/main.rs`
- AC HTTP metrics middleware -> `crates/ac-service/src/middleware/http_metrics.rs`
- GC metrics recording -> `crates/gc-service/src/observability/metrics.rs:init_metrics_recorder()`
- GC meeting metrics + endpoint normalization -> `crates/gc-service/src/observability/metrics.rs`
- GC HTTP metrics middleware -> `crates/gc-service/src/middleware/http_metrics.rs`
- GC join handler metrics -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`
- GC DB metrics -> `crates/gc-service/src/repositories/` (meetings.rs, participants.rs)
- MC metrics recording -> `crates/mc-service/src/observability/metrics.rs:init_metrics_recorder()`
- MC join/WebTransport/JWT metrics -> `crates/mc-service/src/observability/metrics.rs`
- MC connection handler metrics -> `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MC accept loop metrics -> `crates/mc-service/src/webtransport/server.rs:accept_loop()`
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
- Grafana dashboards -> `infra/grafana/dashboards/` (overview per service, SLOs, logs, errors)
- Grafana K8s base -> `infra/kubernetes/observability/grafana/`
- Alert rules -> `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Prometheus config -> `infra/docker/prometheus/prometheus.yml`, `infra/kubernetes/observability/prometheus-config.yaml`
- Loki config -> `infra/kubernetes/observability/loki-config.yaml`
- Observability kustomization -> `infra/kubernetes/observability/kustomization.yaml`
- Kind observability NodePorts (Prometheus=30090, Grafana=30030, Loki=30080) -> `infra/kind/kind-config.yaml`
- Alert + dashboard docs -> `docs/observability/alerts.md`, `docs/observability/dashboards.md`

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Dashboard-to-kustomize coverage (R-20) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`

## Env-Test Observability & Cluster Config
- ClusterPorts::from_env() (ENV_TEST_{PROMETHEUS,GRAFANA,LOKI}_URL) -> `crates/env-tests/src/cluster.rs:from_env()`
- URL parsing + validation (host:port extraction) -> `crates/env-tests/src/cluster.rs:parse_host_port()`
- ClusterConnection health checks (Prometheus, Grafana, Loki) -> `crates/env-tests/src/cluster.rs`
- Observability feature-gated tests -> `crates/env-tests/tests/30_observability.rs`
- MC/MH K8s health probes + metrics scrape -> `infra/services/{mc,mh}-service/deployment.yaml`

## Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service)
