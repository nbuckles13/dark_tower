# Infrastructure Specialist - Patterns

Infrastructure patterns worth documenting for Dark Tower codebase.

---

## Pattern: CanaryPod for NetworkPolicy Testing
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Deploy minimal test pods to validate NetworkPolicy enforcement from within the cluster. Uses busybox:1.36 with wget for HTTP probes, sleep 3600 to keep pod alive for testing duration, and AtomicBool to prevent double-delete on Drop + explicit cleanup(). Key design: create bare pod (not Deployment) with unique name canary-{uuid8}.

---

## Pattern: Multi-Stage Dockerfile with cargo-chef
**Added**: 2026-02-11
**Related files**: `infra/docker/ac-service/Dockerfile`, `infra/docker/gc-service/Dockerfile`, `infra/docker/mc-service/Dockerfile`

Use cargo-chef for efficient dependency caching in multi-stage Docker builds. Four stages: chef (install cargo-chef + build deps), planner (generate recipe.json), builder (cache deps separately from source), runtime (minimal distroless). Dependencies are cached in a separate layer that only rebuilds when Cargo.toml/Cargo.lock changes. Binary is stripped to reduce size. Production uses gcr.io/distroless/cc-debian12 (minimal, no HEALTHCHECK), debug variant uses :debug tag with busybox for HEALTHCHECK support.

---

## Pattern: Kubernetes Service Manifests - Standard Seven Files
**Added**: 2026-02-11
**Related files**: `infra/services/ac-service/`, `infra/services/gc-service/`, `infra/services/mc-service/`, `infra/services/redis/`

Every Dark Tower service follows a standard 7-file manifest pattern: deployment.yaml or statefulset.yaml (workload), service.yaml (ClusterIP for internal routing), configmap.yaml (non-secret config), secret.yaml (credentials, generated from templates), network-policy.yaml (ingress/egress rules per ADR-0012), service-monitor.yaml (Prometheus scraping, commented until metrics implemented), pdb.yaml (PodDisruptionBudget with minAvailable: 1 for HA). This pattern ensures consistency across all services and simplifies operations.

---

## Pattern: Kind Cluster Setup with Calico CNI
**Added**: 2026-02-11
**Related files**: `infra/kind/scripts/setup.sh`, `infra/kind/kind-config.yaml`

Single-tier development environment using Kind with Calico CNI for NetworkPolicy enforcement (ADR-0013). Setup script creates cluster with disableDefaultCNI: true in kind-config.yaml, then installs Calico v3.27.0 before waiting for nodes Ready (nodes cannot be Ready until CNI is installed). Script includes comprehensive error handling, detects Podman vs Docker runtime, deploys full stack (PostgreSQL, Redis, Prometheus, Grafana, Loki, Promtail, AC/GC/MC services), runs migrations, seeds test credentials, and establishes port-forwards. This provides production parity locally for development and testing.

---

## Pattern: Skaffold for Local Development Iteration
**Added**: 2026-02-11
**Related files**: `infra/skaffold.yaml`

Skaffold manages local development workflow with hot-reload. Defines 3 build artifacts (ac-service, global-controller, meeting-controller) using local Docker/Podman with buildkit (push: false loads directly into Kind). Deploy section uses kubectl manifests from infra/services/. Port-forward section exposes services locally (AC on 8083 to avoid conflict with local cargo run on 8082, GC HTTP on 8080, GC gRPC on 50051, MC health on 8084, plus observability stack). Use skaffold dev for watch mode or skaffold run for one-time deploy.

---

## Pattern: Idempotent Namespace Creation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

Handle namespace creation race conditions in parallel test runs. Check if namespace exists (kubectl get namespace), if not create it (kubectl create namespace), but treat "already exists" error as success to handle parallel test race. This allows multiple tests to safely ensure a namespace exists without coordination.

---

## Pattern: Deployment Strategies - StatefulSet vs Deployment
**Added**: 2026-02-11
**Related files**: `infra/services/ac-service/statefulset.yaml`, `infra/services/gc-service/deployment.yaml`, `infra/services/redis/statefulset.yaml`

AC service uses StatefulSet (not Deployment) because it needs stable identity for key management and coordination, even though it doesn't use persistent volumes. GC and MC use Deployment because they are stateless and benefit from flexible scheduling. Redis and PostgreSQL use StatefulSet for stable network identity and persistent storage. Rule: Use StatefulSet only when you need stable pod identity or persistent volumes, otherwise prefer Deployment for better scheduling flexibility.

---

## Pattern: Security Context - Distroless and Rootless
**Added**: 2026-02-11
**Related files**: `infra/services/gc-service/deployment.yaml`, `infra/services/redis/statefulset.yaml`

All service pods enforce security hardening: runAsNonRoot: true, runAsUser: 65532 (distroless nonroot) or 999 (Redis), readOnlyRootFilesystem: true, allowPrivilegeEscalation: false, capabilities drop ALL. Volumes use emptyDir for /tmp since root filesystem is read-only. This defense-in-depth approach limits container escape even if the application is compromised.

---

## Pattern: Observability Stack Deployment - Inline Manifests
**Added**: 2026-02-11
**Related files**: `infra/kind/scripts/setup.sh` (lines 262-758)

Setup script deploys Prometheus, Loki, Promtail, and Grafana using inline YAML (kubectl apply -f - <<EOF). Prometheus uses Kubernetes service discovery with pod role to scrape AC/GC/MC metrics (relabel_configs filter by app label and port number). Promtail DaemonSet mounts /var/log from host to ship logs to Loki. Grafana ConfigMaps are created from files in infra/grafana/ (provisioning/datasources, provisioning/dashboards, dashboards/*.json). Services use NodePort for local access via Kind port mappings. This approach keeps setup.sh self-contained and avoids external dependencies.
