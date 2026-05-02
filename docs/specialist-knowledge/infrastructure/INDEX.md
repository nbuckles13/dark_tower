# Infrastructure Navigation

## Architecture & Design
- Infrastructure architecture (networking, zero-trust) -> `docs/decisions/adr-0012-infrastructure-architecture.md`
- Local dev environment (Kind + Calico) -> `docs/decisions/adr-0013-local-development-environment.md`
- Containerized devloop execution model -> `docs/decisions/adr-0025-containerized-devloop.md`
- Host-side cluster helper for integration testing -> `docs/decisions/adr-0030-host-side-cluster-helper.md`
- Client architecture (CDN deployment, Nx build pipeline, synthetic probe sizing) -> ADR-0028
- Dashboard metric presentation (counters vs rates, $__rate_interval) -> ADR-0029

## Code Locations
- Service Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`
- Service runbooks (per-service `<service>-deployment.md` + `<service>-incident-response.md`) -> `docs/runbooks/`
- PostgreSQL init -> `infra/docker/postgres/init.sql`
- Prometheus config + alert rules -> `infra/docker/prometheus/{prometheus.yml,rules/{gc,mc}-alerts.yaml}`, `infra/kubernetes/observability/prometheus-config.yaml`
- K8s service manifests (Kustomize bases) -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- AC StatefulSet -> `infra/services/ac-service/statefulset.yaml`
- GC Deployment -> `infra/services/gc-service/deployment.yaml`
- MC per-instance Deployments -> `infra/services/mc-service/mc-{0,1}-deployment.yaml`
- MH per-instance Deployments -> `infra/services/mh-service/mh-{0,1}-deployment.yaml`
- MC/MH ConfigMaps (shared + per-instance) -> `infra/services/{mc,mh}-service/configmap.yaml`, `{mc,mh}-{0,1}-configmap.yaml`
- Network policies (per-service ingress/egress) -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- MC/MH per-instance Services (NodePorts) -> `infra/services/{mc,mh}-service/service.yaml`
- MC/MH TLS + MH secrets -> created imperatively by `setup.sh`
- Redis manifests (Kustomize base) -> `infra/services/redis/kustomization.yaml`
- PostgreSQL manifests (Kustomize base) -> `infra/services/postgres/kustomization.yaml`
- K8s observability (Kustomize) -> `infra/kubernetes/observability/kustomization.yaml`
- Grafana manifests (RBAC, deployment, dashboards) -> `infra/kubernetes/observability/grafana/kustomization.yaml`
- Kind overlays -> `infra/kubernetes/overlays/kind/` (services, observability, per-service)
- Grafana dashboards + provisioning -> `infra/grafana/{dashboards,provisioning}/`
- Kind cluster config (static + dynamic template) -> `infra/kind/kind-config.yaml`, `kind-config.yaml.tmpl`
- Kind cluster setup script -> `infra/kind/scripts/setup.sh`
- setup.sh parameterization (DT_CLUSTER_NAME, DT_PORT_MAP, DT_HOST_GATEWAY_IP, --yes, --only, --skip-build) -> ADR-0030
- setup.sh devloop ConfigMap patching (MC/MH advertise addresses) -> `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- setup.sh helpers -> `load_image_to_kind()`, `deploy_only_service()`
- Local iteration (Telepresence) -> `infra/kind/scripts/iterate.sh`
- Cluster teardown -> `infra/kind/scripts/teardown.sh`
- Skaffold dev workflow -> `infra/skaffold.yaml`
- Containerized devloop (health check, eager setup, attach) -> `infra/devloop/devloop.sh`
- dev-cluster client CLI (status display, setup/status output) -> `infra/devloop/dev-cluster`
- Devloop Layer 8 (env-tests in validation pipeline) -> `.claude/skills/devloop/SKILL.md`
- Docker Compose (local tests) -> `docker-compose.test.yml`
- Dev TLS cert generation (CA + MC + MH certs) -> `scripts/generate-dev-certs.sh`
- CI pipeline -> `.github/workflows/ci.yml`
- Fuzz nightly -> `.github/workflows/fuzz-nightly.yml`

## Host-Side Cluster Helper (ADR-0030)
- Devloop helper binary -> `crates/devloop-helper/src/`
- Port allocation -> `crates/devloop-helper/src/ports.rs`
- Helper commands (setup, deploy, rebuild, teardown, status) -> `crates/devloop-helper/src/commands.rs`
- Helper protocol (command parsing, NDJSON types) -> `crates/devloop-helper/src/protocol.rs`
- Status command (cluster health, pod readiness) -> `commands.rs:cmd_status()`, `parse_pod_health()`
- Port-map.env + DT_HOST_GATEWAY_IP -> `commands.rs:write_port_map_shell()`, `cmd_setup()`, `cmd_deploy()`
- Port registry (global, all devloops) -> `~/.cache/devloop/port-registry.json`
- Per-devloop runtime state -> `/tmp/devloop-{slug}/` (PID file, socket, auth token, ports.json, log)
- Port range: 20000-29999, stride 200, hash-preferred with registry collision resolution
- Env-test URL config -> `crates/env-tests/src/cluster.rs:ClusterPorts::from_env()`
- Host-side debate record -> `docs/debates/2026-04-07-host-side-cluster-helper/debate.md`

## Health Probes
- MC health endpoints (liveness + readiness) -> `crates/mc-service/src/observability/health.rs:health_router()`
- MC probe config (K8s Deployment) -> `infra/services/mc-service/mc-{0,1}-deployment.yaml` (livenessProbe / readinessProbe)
- GC probe config (K8s Deployment) -> `infra/services/gc-service/deployment.yaml` (livenessProbe / readinessProbe)
- MH probe config (K8s Deployment) -> `infra/services/mh-service/mh-{0,1}-deployment.yaml` (livenessProbe / readinessProbe on :8083)

## Advertise Address Config (GC Registration)
- MC config fields -> `crates/mc-service/src/config.rs`
- MH config fields -> `crates/mh-service/src/config.rs`
- MC registration -> `crates/mc-service/src/grpc/gc_client.rs:register()`
- MH registration -> `crates/mh-service/src/grpc/gc_client.rs:register()`

## Integration Seams
- CanaryPod (NetworkPolicy testing) -> `crates/env-tests/src/canary.rs`
- Env-tests (cluster health, observability, resilience) -> `crates/env-tests/tests/`
