# Infrastructure Navigation

## Architecture & Design
- Infrastructure architecture (networking, zero-trust) -> `docs/decisions/adr-0012-infrastructure-architecture.md`
- Local dev environment (Kind + Calico) -> `docs/decisions/adr-0013-local-development-environment.md`
- Containerized devloop execution model -> `docs/decisions/adr-0025-containerized-devloop.md`
- Client architecture (CDN deployment, Nx build pipeline, synthetic probe sizing) -> ADR-0028
- Dashboard metric presentation (counters vs rates, $__rate_interval) -> ADR-0029

## Code Locations
- Service Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`
- PostgreSQL init -> `infra/docker/postgres/init.sql`
- Prometheus config (Docker) -> `infra/docker/prometheus/prometheus.yml`
- Prometheus alert rules (GC) -> `infra/docker/prometheus/rules/gc-alerts.yaml`
- Prometheus alert rules (MC) -> `infra/docker/prometheus/rules/mc-alerts.yaml`
- Prometheus config (K8s) -> `infra/kubernetes/observability/prometheus-config.yaml`
- K8s service manifests (Kustomize bases) -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- MC TLS Secret -> created imperatively by `infra/kind/scripts/setup.sh:create_mc_tls_secret()`
- MH TLS Secret -> created imperatively by `infra/kind/scripts/setup.sh:create_mh_tls_secret()`
- MH secrets -> created imperatively by `infra/kind/scripts/setup.sh:create_mh_secrets()`
- Redis manifests (Kustomize base) -> `infra/services/redis/kustomization.yaml`
- PostgreSQL manifests (Kustomize base) -> `infra/services/postgres/kustomization.yaml`
- K8s observability (Kustomize) -> `infra/kubernetes/observability/kustomization.yaml`
- Grafana manifests (RBAC, deployment, dashboards) -> `infra/kubernetes/observability/grafana/kustomization.yaml`
- Kind overlay (top-level) -> `infra/kubernetes/overlays/kind/kustomization.yaml`
- Kind overlay (services aggregator) -> `infra/kubernetes/overlays/kind/services/kustomization.yaml`
- Kind overlay (per-service) -> `infra/kubernetes/overlays/kind/services/{ac,gc,mc,mh}-service/kustomization.yaml`
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
- Dev TLS cert generation (CA + MC + MH certs) -> `scripts/generate-dev-certs.sh`
- CI pipeline -> `.github/workflows/ci.yml`
- Fuzz nightly -> `.github/workflows/fuzz-nightly.yml`

## Health Probes
- MC health endpoints (liveness + readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`
- MC probe config (K8s deployment) -> `infra/services/mc-service/deployment.yaml` (livenessProbe / readinessProbe)
- GC probe config (K8s deployment) -> `infra/services/gc-service/deployment.yaml` (livenessProbe / readinessProbe)
- MH probe config (K8s deployment) -> `infra/services/mh-service/deployment.yaml` (livenessProbe / readinessProbe on :8083)

## Advertise Address Config (GC Registration)
- MC config fields (`grpc_advertise_address`, `webtransport_advertise_address`) -> `crates/mc-service/src/config.rs`
- MH config fields (same names) -> `crates/mh-service/src/config.rs`
- MC registration uses advertise addresses -> `crates/mc-service/src/grpc/gc_client.rs:register()`, `attempt_reregistration()`
- MH registration uses advertise addresses -> `crates/mh-service/src/grpc/gc_client.rs:register()`, `attempt_reregistration()`
- MC deployment (POD_IP downward API + advertise env vars) -> `infra/services/mc-service/deployment.yaml`
- MH deployment (POD_IP downward API + advertise env vars) -> `infra/services/mh-service/deployment.yaml`
- Pattern: pod-specific values via `$(POD_IP)` in deployment.yaml, NOT in configmap
- MC env vars: `MC_GRPC_ADVERTISE_ADDRESS`, `MC_WEBTRANSPORT_ADVERTISE_ADDRESS` (required, no default)
- MH env vars: `MH_GRPC_ADVERTISE_ADDRESS`, `MH_WEBTRANSPORT_ADVERTISE_ADDRESS` (required, no default)
- MC schemes: `http://` for gRPC, `https://` for WebTransport
- MH schemes: `grpc://` for gRPC, `https://` for WebTransport

## Integration Seams
- CanaryPod (NetworkPolicy testing) -> `crates/env-tests/src/canary.rs`
- Cluster health env-tests -> `crates/env-tests/tests/00_cluster_health.rs`
- Observability env-tests -> `crates/env-tests/tests/30_observability.rs`
- Resilience / NetworkPolicy env-tests -> `crates/env-tests/tests/40_resilience.rs`
- NetworkPolicy definitions -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`, `infra/services/{redis,postgres}/network-policy.yaml`
