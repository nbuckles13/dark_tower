# Observability Navigation

## Architecture & Design
- Observability framework (metrics, tracing, dashboards, alerts, SLOs) -> ADR-0011
- Validation pipeline (guards including metric guards) -> ADR-0024

## Code Locations
- AC metrics recording -> `crates/ac-service/src/observability/metrics.rs:init_metrics_recorder()`
- AC gauge initialization at startup -> `crates/ac-service/src/services/key_management_service.rs:init_key_metrics()`
- AC HTTP metrics middleware -> `crates/ac-service/src/middleware/http_metrics.rs`
- GC metrics recording -> `crates/gc-service/src/observability/metrics.rs:init_metrics_recorder()`
- GC meeting creation metrics -> `crates/gc-service/src/observability/metrics.rs:record_meeting_creation()`
- GC HTTP metrics middleware -> `crates/gc-service/src/middleware/http_metrics.rs:http_metrics_middleware()`
- GC endpoint normalization -> `crates/gc-service/src/observability/metrics.rs:normalize_endpoint()`
- GC meetings repository DB metrics -> `crates/gc-service/src/repositories/meetings.rs:MeetingsRepository`
- MC metrics recording -> `crates/mc-service/src/observability/metrics.rs:init_metrics_recorder()`
- MC health state (liveness/readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`
- Metric catalogs -> `docs/observability/metrics/ac-service.md`, `gc-service.md`, `mc-service.md`

## Dashboards & Alerts
- Grafana dashboards -> `infra/grafana/dashboards/` (overview, SLOs, logs per service)
- GC overview dashboard (meeting creation panels) -> `infra/grafana/dashboards/gc-overview.json`
- Cross-service error dashboard -> `infra/grafana/dashboards/errors-overview.json`
- Grafana provisioning -> `infra/grafana/provisioning/datasources/datasources.yaml`
- Prometheus config -> `infra/docker/prometheus/prometheus.yml`
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
