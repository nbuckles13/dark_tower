# Operations Navigation

## Architecture & Design
- Infrastructure architecture (Kind, Skaffold, zero-trust) → ADR-0012
- Local development environment → ADR-0013
- Environment integration tests → ADR-0014
- Guard pipeline methodology → ADR-0015
- Validation pipeline (CI gates) → ADR-0024 (Section: Validation Pipeline)
- Containerized devloop execution → ADR-0025

## Code Locations — CI & Guards
- CI pipeline → `.github/workflows/ci.yml`
- Guard runner → `scripts/guards/run-guards.sh`
- Guard common lib → `scripts/guards/common.sh`
- Application metrics guard → `scripts/guards/simple/validate-application-metrics.sh`

## Code Locations — Deployment
- Skaffold config → `infra/skaffold.yaml`
- Docker compose (test) → `docker-compose.test.yml`
- Kind cluster config → `infra/kind/kind-config.yaml`
- Kind setup/iterate/teardown → `infra/kind/scripts/`
- Containerized devloop → `infra/devloop/devloop.sh`
- Service Dockerfiles → `infra/docker/{ac,gc,mc}-service/`

## Code Locations — K8s Manifests
- Per-service manifests (deployment, netpol, PDB) → `infra/services/{ac,gc,mc}-service/`
- Redis manifests + NetworkPolicy → `infra/services/redis/`
- Observability stack (Prometheus, Loki, Promtail) → `infra/kubernetes/observability/`
- Prometheus scrape config (docker) → `infra/docker/prometheus/prometheus.yml`
- Alert rules → `infra/docker/prometheus/rules/`
- Grafana dashboards → `infra/grafana/dashboards/`
- Grafana provisioning → `infra/grafana/provisioning/`
- Database migrations → `migrations/`

## Code Locations — Operational Scripts
- Dev cert generation → `scripts/generate-dev-certs.sh`
- Master key generation → `scripts/generate-master-key.sh`
- Service registration → `scripts/register-service.sh`
- Devloop status check → `scripts/workflow/devloop-status.sh`

## Code Locations — GC Observability
- GC metrics recorder → `crates/gc-service/src/observability/metrics.rs:init_metrics_recorder()`
- GC meeting creation metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_creation()`
- GC metrics catalog → `docs/observability/metrics/gc-service.md`

## Integration Seams
- Env-tests (cluster validation) → `crates/env-tests/`
- Metric catalogs (guard cross-ref) → `docs/observability/metrics/`
- NetworkPolicy cross-refs → `infra/services/*/network-policy.yaml`
- ServiceMonitor (Prometheus scrape) → `infra/services/*/service-monitor.yaml`
