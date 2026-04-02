# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028
- Dashboard metric presentation (counters vs rates, increase/rate classification) -> ADR-0029

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `gc-service.md`, `mc-service.md`, `mh-service.md`
- AC metrics recording -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- AC gauge initialization at startup -> `crates/ac-service/src/services/key_management_service.rs:init_key_metrics()`
- AC rate limit config startup logging -> `crates/ac-service/src/main.rs` (lines 60-75)
- AC rate limit config (defaults, bounds, parsing, non-default warnings) -> `crates/ac-service/src/config.rs`
- AC HTTP metrics middleware -> `crates/ac-service/src/middleware/http_metrics.rs`
- GC metrics recording -> `crates/gc-service/src/observability/metrics.rs:init_metrics_recorder()`
- GC meeting metrics (creation, join) -> `crates/gc-service/src/observability/metrics.rs`
- GC HTTP metrics middleware -> `crates/gc-service/src/middleware/http_metrics.rs:http_metrics_middleware()`
- GC endpoint normalization -> `crates/gc-service/src/observability/metrics.rs:normalize_endpoint()`
- GC join handler metrics wiring -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`
- GC DB metrics -> `crates/gc-service/src/repositories/` (meetings.rs, participants.rs)
- MC metrics recording -> `crates/mc-service/src/observability/metrics.rs:init_metrics_recorder()`
- MC metrics (join, WebTransport, JWT validation) -> `crates/mc-service/src/observability/metrics.rs`
- MC join metrics recording site (connection handler) -> `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MC connection metrics recording site (accept loop) -> `crates/mc-service/src/webtransport/server.rs:accept_loop()`
- MC error type labels (bounded cardinality) -> `crates/mc-service/src/errors.rs:error_type_label()`
- MH metrics (registration, heartbeat, token refresh, gRPC, errors) -> `crates/mh-service/src/observability/metrics.rs`

## Auth & JWT Tracing
- Common JWT (JwksClient, JwtValidator, verify_token) -> `crates/common/src/jwt.rs`
- GC auth wrapper (delegates logging to common) -> `crates/gc-service/src/auth/jwt.rs:JwtValidator`
- MC auth wrapper (target: `mc.auth`, delegates to common) -> `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- PII-redacted claims Debug impls -> `crates/common/src/jwt.rs` (ServiceClaims, UserClaims, MeetingTokenClaims, GuestTokenClaims)

## MC WebTransport Tracing
- WebTransport server lifecycle (target: `mc.webtransport`) -> `crates/mc-service/src/webtransport/server.rs`
- WebTransport connection handler (target: `mc.webtransport.connection`) -> `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- Protobuf encoding utilities (target: `mc.webtransport.handler`) -> `crates/mc-service/src/webtransport/handler.rs:encode_participant_update()`
- ParticipantActor tracing (target: `mc.actor.participant`) -> `crates/mc-service/src/actors/participant.rs:run()`

## GC Client Tracing (MC + MH)
- GcClient tracing (registration, heartbeat, re-registration) -> `crates/mc-service/src/grpc/gc_client.rs` (+ mh)
- Rule: GcClient::new() must NOT log raw endpoint URLs (IP/DNS leakage); advertise addresses logged at startup only

## Health
- MC health state (liveness/readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health state (ready after GC registration) -> `crates/mh-service/src/observability/health.rs:health_router()`

## Dashboards & Alerts
- Grafana dashboards -> `infra/grafana/dashboards/` (overview, SLOs, logs per service)
- Grafana K8s base (configMapGenerator, sidecar, RBAC, service) -> `infra/kubernetes/observability/grafana/`
- GC overview dashboard (Traffic Summary stat row, creation + join panels) -> `infra/grafana/dashboards/gc-overview.json`
- GC join alert rules (GCHighJoinFailureRate, GCHighJoinLatency) -> `infra/docker/prometheus/rules/gc-alerts.yaml`
- MC overview dashboard (join flow panels, Traffic Summary + Security Events stat rows) -> `infra/grafana/dashboards/mc-overview.json`
- MC join alert rules (MCHighJoinFailureRate, MCHighWebTransportRejections, MCHighJwtValidationFailures, MCHighJoinLatency) -> `infra/docker/prometheus/rules/mc-alerts.yaml`
- AC overview dashboard (Traffic Summary + Security Events stat rows) -> `infra/grafana/dashboards/ac-overview.json`
- MH overview dashboard (registration, heartbeat, token refresh panels, ADR-0029 compliant) -> `infra/grafana/dashboards/mh-overview.json`
- Cross-service error dashboard -> `infra/grafana/dashboards/errors-overview.json`
- Grafana provisioning + K8s kustomization -> `infra/grafana/provisioning/`, `infra/kubernetes/observability/`
- Prometheus config -> `infra/docker/prometheus/prometheus.yml` (compose), `infra/kubernetes/observability/prometheus-config.yaml` (K8s)
- Alert + dashboard docs -> `docs/observability/alerts.md`, `docs/observability/dashboards.md`

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Dashboard-to-kustomize coverage (R-20, bidirectional) -> `scripts/guards/simple/validate-kustomize.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`

## Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service)
- GC scenarios 8-9 + join failure triage -> `docs/runbooks/gc-incident-response.md`, `docs/observability/alerts.md`

## Test Coverage & Integration Seams
- GC/MC/MH metrics tests -> `crates/gc-service/src/observability/metrics.rs` (+ mc, mh)
- Env-tests observability validation -> `crates/env-tests/tests/30_observability.rs`
- MC/MH K8s health probes + metrics scrape -> `infra/services/{mc,mh}-service/deployment.yaml`
