# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024
- Client architecture (telemetry, metrics, dashboards, synthetic probe) -> ADR-0028

## Metrics
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `gc-service.md`, `mc-service.md`
- AC metrics recording -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- AC gauge initialization at startup -> `crates/ac-service/src/services/key_management_service.rs:init_key_metrics()`
- AC HTTP metrics middleware -> `crates/ac-service/src/middleware/http_metrics.rs`
- GC metrics recording -> `crates/gc-service/src/observability/metrics.rs:init_metrics_recorder()`
- GC meeting creation metrics -> `crates/gc-service/src/observability/metrics.rs:record_meeting_creation()`
- GC meeting join metrics -> `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- GC HTTP metrics middleware -> `crates/gc-service/src/middleware/http_metrics.rs:http_metrics_middleware()`
- GC endpoint normalization -> `crates/gc-service/src/observability/metrics.rs:normalize_endpoint()`
- GC join handler metrics wiring -> `crates/gc-service/src/handlers/meetings.rs:join_meeting()`
- GC DB metrics (meetings) -> `crates/gc-service/src/repositories/meetings.rs:MeetingsRepository`
- GC DB metrics (participants) -> `crates/gc-service/src/repositories/participants.rs:ParticipantsRepository`
- MC metrics recording -> `crates/mc-service/src/observability/metrics.rs:init_metrics_recorder()`

## Auth & JWT Tracing
- Common JWKS client (target: `common.jwt.jwks`) -> `crates/common/src/jwt.rs:JwksClient`
- Common JWT validator (target: `common.jwt`) -> `crates/common/src/jwt.rs:JwtValidator`
- Common JWT verify_token (target: `common.jwt`) -> `crates/common/src/jwt.rs:verify_token()`
- GC auth wrapper (delegates logging to common) -> `crates/gc-service/src/auth/jwt.rs:JwtValidator`
- MC auth wrapper (target: `mc.auth`, delegates to common) -> `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- PII-redacted claims Debug impls -> `crates/common/src/jwt.rs` (ServiceClaims, UserClaims, MeetingTokenClaims, GuestTokenClaims)

## MC WebTransport Tracing
- WebTransport server lifecycle (target: `mc.webtransport`) -> `crates/mc-service/src/webtransport/server.rs`
- WebTransport connection handler (target: `mc.webtransport.connection`) -> `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- Protobuf encoding utilities (target: `mc.webtransport.handler`) -> `crates/mc-service/src/webtransport/handler.rs:encode_participant_update()`
- ParticipantActor tracing (target: `mc.actor.participant`) -> `crates/mc-service/src/actors/participant.rs:run()`

## Health
- MC health state (liveness/readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`

## Dashboards & Alerts
- Grafana dashboards -> `infra/grafana/dashboards/` (overview, SLOs, logs per service)
- GC overview dashboard (meeting creation + join panels) -> `infra/grafana/dashboards/gc-overview.json`
- Cross-service error dashboard -> `infra/grafana/dashboards/errors-overview.json`
- Grafana provisioning -> `infra/grafana/provisioning/datasources/datasources.yaml`
- Prometheus config (docker-compose) -> `infra/docker/prometheus/prometheus.yml`
- Prometheus config (K8s in-cluster) -> `infra/kubernetes/observability/prometheus-config.yaml`
- Alert rules -> `infra/docker/prometheus/rules/gc-alerts.yaml`, `mc-alerts.yaml`
- Alert design docs -> `docs/observability/alerts.md`
- Dashboard docs -> `docs/observability/dashboards.md`

## Guards
- Metric-to-dashboard coverage -> `scripts/guards/simple/validate-application-metrics.sh`
- Instrument skip_all enforcement -> `scripts/guards/simple/instrument-skip-all.sh`

## Runbooks
- Per-service deployment + incident response -> `docs/runbooks/` (two per service)
- GC meeting creation limit exhaustion -> `docs/runbooks/gc-incident-response.md#scenario-8-meeting-creation-limit-exhaustion`
- GC meeting code collision -> `docs/runbooks/gc-incident-response.md#scenario-9-meeting-code-collision`
- GC post-deploy meeting creation checklist -> `docs/runbooks/gc-deployment.md` (Post-Deploy Monitoring Checklist section)

## Integration Seams
- Env-tests observability validation -> `crates/env-tests/tests/30_observability.rs`
- Observability mod re-exports (stale export risk) -> `crates/*/src/observability/mod.rs`
- MC TLS volume mount (affects health port availability) -> `infra/services/mc-service/deployment.yaml`
- MC WebTransport UDP NodePort (Kind port mapping) -> `infra/kind/kind-config.yaml`, `infra/services/mc-service/service.yaml`
