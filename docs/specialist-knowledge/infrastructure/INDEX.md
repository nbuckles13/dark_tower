# Infrastructure Navigation

## Architecture & Design
- Infrastructure architecture (networking, zero-trust) -> `docs/decisions/adr-0012-infrastructure-architecture.md`
- Local dev environment (Kind + Calico) -> `docs/decisions/adr-0013-local-development-environment.md`
- Containerized devloop execution model -> `docs/decisions/adr-0025-containerized-devloop.md`

## Code Locations
- Service Dockerfiles -> `infra/docker/{ac,gc,mc}-service/Dockerfile`
- PostgreSQL init -> `infra/docker/postgres/init.sql`
- Prometheus config + alert rules -> `infra/docker/prometheus/`
- K8s service manifests (7-file pattern) -> `infra/services/{ac,gc,mc}-service/`
- Redis manifests -> `infra/services/redis/`
- K8s observability (kustomize) -> `infra/kubernetes/observability/`
- Grafana dashboards -> `infra/grafana/dashboards/`
- Grafana provisioning -> `infra/grafana/provisioning/`
- Kind cluster config -> `infra/kind/kind-config.yaml`
- Kind cluster setup script -> `infra/kind/scripts/setup.sh`
- Local iteration (Telepresence) -> `infra/kind/scripts/iterate.sh`
- Cluster teardown -> `infra/kind/scripts/teardown.sh`
- Skaffold dev workflow -> `infra/skaffold.yaml`
- Containerized devloop -> `infra/devloop/devloop.sh`
- Docker Compose (local tests) -> `docker-compose.test.yml`
- CI pipeline -> `.github/workflows/ci.yml`
- Fuzz nightly -> `.github/workflows/fuzz-nightly.yml`

## Integration Seams
- CanaryPod (NetworkPolicy testing) -> `crates/env-tests/src/canary.rs`
- Cluster health env-tests -> `crates/env-tests/tests/00_cluster_health.rs`
- Observability env-tests -> `crates/env-tests/tests/30_observability.rs`
- Resilience / NetworkPolicy env-tests -> `crates/env-tests/tests/40_resilience.rs`
- NetworkPolicy definitions -> `infra/services/*/network-policy.yaml`
