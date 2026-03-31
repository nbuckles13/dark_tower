# Infrastructure Navigation

## Architecture & Design
- Infrastructure architecture (networking, zero-trust) -> `docs/decisions/adr-0012-infrastructure-architecture.md`
- Local dev environment (Kind + Calico) -> `docs/decisions/adr-0013-local-development-environment.md`
- Containerized devloop execution model -> `docs/decisions/adr-0025-containerized-devloop.md`
- Client architecture (CDN deployment, Nx build pipeline, synthetic probe sizing) -> ADR-0028

## Code Locations
- Service Dockerfiles -> `infra/docker/{ac,gc,mc}-service/Dockerfile`
- PostgreSQL init -> `infra/docker/postgres/init.sql`
- Prometheus config + alert rules -> `infra/docker/prometheus/`
- K8s service manifests (Kustomize bases) -> `infra/services/{ac,gc,mc}-service/kustomization.yaml`
- MC TLS Secret -> created imperatively by `infra/kind/scripts/setup.sh:create_mc_tls_secret()`
- Redis manifests (Kustomize base) -> `infra/services/redis/kustomization.yaml`
- PostgreSQL manifests (Kustomize base) -> `infra/services/postgres/kustomization.yaml`
- K8s observability (Kustomize) -> `infra/kubernetes/observability/kustomization.yaml`
- Grafana manifests (RBAC, deployment, dashboards) -> `infra/kubernetes/observability/grafana/kustomization.yaml`
- Kind overlay (top-level) -> `infra/kubernetes/overlays/kind/kustomization.yaml`
- Kind overlay (services aggregator) -> `infra/kubernetes/overlays/kind/services/kustomization.yaml`
- Kind overlay (per-service) -> `infra/kubernetes/overlays/kind/services/{ac,gc,mc}-service/kustomization.yaml`
- Kind overlay (postgres) -> `infra/kubernetes/overlays/kind/services/postgres/kustomization.yaml`
- Kind overlay (redis) -> `infra/kubernetes/overlays/kind/services/redis/kustomization.yaml`
- Kind overlay (observability) -> `infra/kubernetes/overlays/kind/observability/kustomization.yaml`
- Grafana dashboards -> `infra/grafana/dashboards/`
- Grafana provisioning -> `infra/grafana/provisioning/`
- Kind cluster config -> `infra/kind/kind-config.yaml`
- Kind cluster setup script -> `infra/kind/scripts/setup.sh`
- Local iteration (Telepresence) -> `infra/kind/scripts/iterate.sh`
- Cluster teardown -> `infra/kind/scripts/teardown.sh`
- Skaffold dev workflow -> `infra/skaffold.yaml`
- Containerized devloop -> `infra/devloop/devloop.sh`
- Docker Compose (local tests) -> `docker-compose.test.yml`
- Dev TLS cert generation (CA + service certs) -> `scripts/generate-dev-certs.sh`
- CI pipeline -> `.github/workflows/ci.yml`
- Fuzz nightly -> `.github/workflows/fuzz-nightly.yml`

## Health Probes
- MC health endpoints (liveness + readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`
- MC probe config (K8s deployment) -> `infra/services/mc-service/deployment.yaml` (livenessProbe / readinessProbe)
- GC probe config (K8s deployment) -> `infra/services/gc-service/deployment.yaml` (livenessProbe / readinessProbe)

## Integration Seams
- CanaryPod (NetworkPolicy testing) -> `crates/env-tests/src/canary.rs`
- Cluster health env-tests -> `crates/env-tests/tests/00_cluster_health.rs`
- Observability env-tests -> `crates/env-tests/tests/30_observability.rs`
- Resilience / NetworkPolicy env-tests -> `crates/env-tests/tests/40_resilience.rs`
- NetworkPolicy definitions -> `infra/services/{ac,gc,mc}-service/network-policy.yaml`, `infra/services/{redis,postgres}/network-policy.yaml`
