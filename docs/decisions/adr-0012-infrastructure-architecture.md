# ADR-0012: Dark Tower Infrastructure Architecture

**Status**: Accepted
**Date**: 2025-12-07
**Deciders**: Multi-agent debate (AC, GC, MC, MH, Infrastructure, Operations, Security, Observability, Test, Database specialists)
**Debate Rounds**: 4
**Final Consensus**: 91.6% average satisfaction (8/9 specialists ≥90%, MC at 89%)

## Context

The AC Operational Readiness Review (2025-12-07) identified critical infrastructure gaps:
- No container images or Dockerfiles
- No Kubernetes manifests or deployment configurations
- No local development environment with cloud parity
- No secrets management infrastructure
- No deployment strategy or rollback procedures

This ADR establishes the infrastructure architecture for Dark Tower, ensuring cloud-agnostic, reproducible, and secure deployments.

## Decision

We will implement a Kubernetes-native infrastructure with the following key decisions:

### 1. Container Orchestration: Kubernetes

**Choice**: Kubernetes (not ECS, Cloud Run, or Nomad)

**Rationale**:
- Cloud-agnostic: Works on AWS EKS, GCP GKE, Azure AKS, on-prem
- Standard APIs: Avoids provider-specific extensions
- Ecosystem: Rich tooling (Helm, Kustomize, operators)
- Team familiarity: Industry standard

### 2. Local Development: kind with Multi-Region Simulation

**Choice**: kind (Kubernetes in Docker) as primary, Docker Compose as lightweight alternative

**Rationale**:
- **Parity**: Same K8s manifests work locally and in cloud
- **Multi-region simulation**: Namespaces simulate regions (region-us-west, region-us-east)
- **Observability**: Same Prometheus/Grafana/Jaeger stack locally
- **Chaos testing**: Same LitmusChaos experiments work locally
- **Offline capable**: Fully functional once images pulled

**Local Environment Commands**:
```bash
./infra/local/setup.sh      # Create full environment
./infra/local/teardown.sh   # Destroy full environment
./infra/local/restart.sh    # Restart full environment
./infra/local/restart.sh ac # Restart specific service (ac, gc, mc, mh)
./infra/local/status.sh     # Show environment status
```

### 3. Service Mesh / mTLS: Linkerd 2.x

**Choice**: Linkerd (not Istio or manual cert-manager)

**Rationale**:
- Zero-config mTLS: Automatic for all meshed services
- Lightweight: Lower resource overhead than Istio
- Rust-based proxy: Performance aligns with Rust services
- 24-hour cert rotation: Built-in PKI
- Sufficient features: mTLS + basic traffic management covers our needs

### 4. Database: CloudNativePG

**Choice**: CloudNativePG operator (not managed RDS/Cloud SQL)

**Rationale**:
- Cloud-agnostic: Works on any Kubernetes cluster
- GitOps-friendly: Declarative YAML, version controlled
- Cost-effective: No managed service markup (30-50% savings)
- Full control: Direct PostgreSQL access for tuning
- Backup flexibility: S3-compatible storage (any cloud)

**Configuration**:
- 3 instances (1 primary + 2 replicas)
- Synchronous replication to 1 replica (RPO: 5s)
- Async streaming to DR region
- PgBouncer pooler for connection management
- **TLS required**: All client connections use TLS (verify-full mode), in addition to Linkerd mTLS at the network layer

### 5. Redis: Cluster Mode

**Choice**: Redis Cluster (not Sentinel)

**Rationale**:
- Horizontal scaling: Sharding across nodes
- High availability: Automatic failover without Sentinel
- Meeting state distribution: Hash slots partition data
- Handles 10K+ concurrent meetings

**Configuration**:
- 6 nodes (3 masters + 3 replicas)
- Automatic failover
- AOF persistence

### 6. Secrets Management: external-secrets-operator

**Choice**: external-secrets-operator (not direct Vault or cloud-specific)

**Rationale**:
- Cloud-agnostic: Syncs from AWS Secrets Manager, GCP Secret Manager, Azure Key Vault
- Kubernetes-native: Creates standard K8s Secrets
- Rotation support: Refresh intervals for automatic updates
- Migration path: Easy to switch cloud providers

### 7. Image Security: Trivy

**Choice**: Trivy (not Snyk or Clair)

**Rationale**:
- Open source: No licensing costs
- Comprehensive: OS packages + application dependencies
- SBOM generation: Software Bill of Materials
- CI integration: GitHub Actions support

**Policy**: CRITICAL vulnerabilities block deployment

### 8. Chaos Testing: LitmusChaos

**Choice**: LitmusChaos (not Chaos Mesh or Gremlin)

**Rationale**:
- Kubernetes-native: CRDs for experiments
- CNCF project: Community support, active development
- CI integration: Works in kind clusters for pre-merge testing

### 9. Deployment Strategy: Canary via Flagger

**Choice**: Canary deployment with Flagger (not blue-green or rolling)

**Rationale**:
- Progressive rollout: 5%→10%→50%→100%
- Auto-rollback: Metric-based failure detection
- Minimal blast radius: Issues caught early
- Integrates with Linkerd for traffic splitting

**Configuration**:
- Rollback threshold: Error rate >150% of baseline OR p99 >2x baseline
- Rollback time: <2 minutes

### 10. Feature Flags: etcd with WebSocket Push

**Choice**: Self-hosted etcd with WebSocket real-time updates

**Rationale**:
- <100ms propagation: WebSocket push for instant rollback
- 3-tier fallback: etcd→cache→static defaults (no SPOF)
- GitOps-friendly: Flag definitions in YAML
- No vendor lock-in: Self-hosted

**Ownership** (to be defined in implementation):
- **Operations specialist** owns: Flag catalog documentation, operational procedures for setting/reading flags
- **Service specialists** own: Flag usage within their services, default values
- **Documentation location**: `docs/operations/feature-flags.md` (catalog and procedures)

### 11. Observability Infrastructure: OTel DaemonSet

**Choice**: OpenTelemetry Collector as DaemonSet (not sidecar)

**Rationale**:
- Resource efficient: One collector per node vs per pod
- Centralized sampling: Tail-based across all pods on node
- Simplified config: Single configuration per node

**Sampling Strategy**:
- 100% of errors
- 100% of slow requests (>500ms)
- 10% of successful requests

### 12. CPU Pinning for Media Handler: Deferred

**Decision**: Not implementing CPU pinning initially

**What CPU pinning is**: Dedicates specific CPU cores to a pod exclusively via Kubernetes CPU Manager (`static` policy). Prevents OS scheduler from moving processes between cores, avoiding cache misses and latency spikes.

**Why deferred**:
1. **Passthrough SFU**: MH doesn't decode/transcode media (E2E encrypted), just forwards packets
2. **Forwarding is fast**: Packet forwarding is ~microseconds, doesn't benefit much from cache locality
3. **Resource waste**: CPU pinning prevents overcommit (wasted resources when MH is idle)
4. **Achievable targets**: P99 <50ms achievable without pinning

**Revisit trigger**: If production P99 forwarding latency exceeds 30ms consistently

## Directory Structure

```
infra/
├── base/                          # Base K8s manifests (Kustomize)
│   ├── namespace.yaml
│   ├── service-account.yaml
│   ├── network-policy-default-deny.yaml
│   └── kustomization.yaml
│
├── services/                      # Per-service configs
│   ├── ac-service/
│   │   ├── statefulset.yaml       # StatefulSet for key management
│   │   ├── service.yaml
│   │   ├── hpa.yaml
│   │   ├── pdb.yaml
│   │   ├── network-policy.yaml
│   │   └── service-monitor.yaml
│   ├── global-controller/
│   ├── meeting-controller/
│   └── media-handler/
│
├── platform/                      # Platform infrastructure
│   ├── linkerd/                   # Service mesh
│   ├── external-secrets/          # Secrets operator
│   ├── observability/             # Prometheus, Grafana, Jaeger, OTel
│   ├── database/                  # CloudNativePG, Redis
│   ├── chaos/                     # LitmusChaos
│   └── security/                  # Trivy, PSS policies
│
├── terraform/                     # Cloud infrastructure
│   ├── modules/
│   │   ├── kubernetes-cluster/    # Cloud-agnostic K8s
│   │   ├── node-pool/             # Service-specific node pools
│   │   └── secrets-backend/       # Cloud secret stores
│   └── environments/
│       ├── dev/
│       ├── staging/
│       └── prod/
│
├── local/                         # Local development
│   ├── kind-config.yaml
│   ├── docker-compose.yml         # Lightweight alternative
│   ├── setup.sh                   # Create full environment
│   ├── teardown.sh                # Destroy full environment
│   ├── restart.sh                 # Restart full or partial environment
│   ├── status.sh                  # Show environment status
│   └── README.md
│
└── ci/
    ├── image-scan.yaml            # Trivy CI config
    ├── deploy-canary.yaml         # Flagger config
    └── chaos-tests.yaml           # LitmusChaos CI
```

## Resource Sizing

| Service | CPU Req/Limit | Memory Req/Limit | Type | Notes |
|---------|---------------|------------------|------|-------|
| AC | 500m/2000m | 1Gi/2Gi | StatefulSet | Key management stability |
| GC | 1000m/3000m | 2Gi/4Gi | Deployment | HTTP/3 gateway |
| MC | 2000m/4000m | 6Gi/8Gi | Deployment | Redis-backed state |
| MH | 4000m/8000m | 12Gi/16Gi | Deployment | Dedicated node pool |

## Network Policies

All services expose HTTP/2 on TCP:443 and HTTP/3 on UDP:443. TLS termination happens at the service (not at ingress) for end-to-end encryption via Linkerd mTLS.

Default-deny with explicit allow:

| Service | Ingress | Egress |
|---------|---------|--------|
| AC | GC via HTTPS (TCP:443) | PostgreSQL (TCP:5432) |
| GC | External HTTPS (TCP:443), External HTTP/3 (UDP:443) | AC, MC via HTTPS (TCP:443), PostgreSQL (TCP:5432) |
| MC | GC via HTTPS (TCP:443), Clients WebTransport (UDP:443) | Redis (TCP:6379), MH via HTTPS (TCP:443) |
| MH | MC via HTTPS (TCP:443), Clients media (UDP:443), Other MH media (UDP:443) | MC via HTTPS (TCP:443), Other MH media (UDP:443) |

**Protocol Details**:
- **TCP:443 (HTTPS)**: All service-to-service API calls use HTTP/2 over TLS
- **UDP:443 (HTTP/3/WebTransport)**: Client connections to GC (HTTP/3 API) and MC (WebTransport signaling)
- **UDP:443 (Media)**: MH receives all client and inter-MH media streams on single multiplexed UDP port
- **TCP:5432**: PostgreSQL connections (from AC, GC only)
- **TCP:6379**: Redis connections (from MC only)

**Note**: MH will multiplex all media connections (clients + other MH instances) onto a single UDP:443 port. Design details deferred to MH implementation phase.

## Standard Operational Endpoints

All services MUST implement the following HTTP endpoints for Kubernetes health checks and observability:

### Health Check Endpoints

| Endpoint | Purpose | Kubernetes Probe | Implementation | Status Code |
|----------|---------|------------------|----------------|-------------|
| `/health` | Liveness probe | `livenessProbe` | Simple "OK" response, no dependencies checked | 200 |
| `/ready` | Readiness probe | `readinessProbe` | Check critical dependencies (DB, external services) | 200 (ready) / 503 (not ready) |

**Liveness Probe** (`/health`):
- **Purpose**: Detects if the process is hung or deadlocked
- **Implementation**: Returns "OK" immediately without checking any dependencies
- **Failure action**: Kubernetes kills and restarts the pod
- **Example**:
  ```rust
  async fn health_check() -> &'static str {
      "OK"
  }
  ```

**Readiness Probe** (`/ready`):
- **Purpose**: Determines if the service can handle traffic
- **Implementation**: Checks critical dependencies are available
- **Failure action**: Kubernetes removes pod from service load balancer
- **Response format**: JSON with component status
- **Example response**:
  ```json
  {
    "status": "ready",
    "database": "healthy",
    "signing_key": "available"
  }
  ```

**Service-Specific Readiness Checks**:

| Service | Dependencies Checked |
|---------|---------------------|
| AC | Database connectivity, Active signing key availability |
| GC | Database connectivity, AC JWKS endpoint reachable |
| MC | Redis connectivity, GC registration successful |
| MH | MC registration successful, Media port binding |

### Observability Endpoints

| Endpoint | Purpose | Authentication | Format | Cardinality |
|----------|---------|----------------|--------|-------------|
| `/metrics` | Prometheus scraping | None (internal network only) | Prometheus text format | Per ADR-0011 limits |

**Metrics Endpoint** (`/metrics`):
- **Purpose**: Export operational metrics for Prometheus scraping
- **Authentication**: None (Prometheus ServiceMonitor runs in-cluster on internal network)
- **Format**: Prometheus text format
- **Cardinality**: Must comply with ADR-0011 limits (<1,000 unique label combinations per metric)
- **Security**: No PII in metric labels (see ADR-0011 privacy-by-default)

**Network Policy**: The `/metrics` endpoint is accessible only within the Kubernetes cluster via ServiceMonitor. External access is blocked by default-deny NetworkPolicy.

### Probe Configuration

**Liveness Probe Settings** (all services):
```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080  # or service-specific HTTP port
    scheme: HTTP
  initialDelaySeconds: 10
  periodSeconds: 10
  timeoutSeconds: 5
  successThreshold: 1
  failureThreshold: 3  # Restart after 30s of failures
```

**Readiness Probe Settings** (all services):
```yaml
readinessProbe:
  httpGet:
    path: /ready
    port: 8080  # or service-specific HTTP port
    scheme: HTTP
  initialDelaySeconds: 5
  periodSeconds: 5
  timeoutSeconds: 3
  successThreshold: 1
  failureThreshold: 3  # Remove from LB after 15s of failures
```

**Timing Rationale**:
- Liveness has longer timeout (5s vs 3s) since dependency checks may be slow
- Readiness has shorter period (5s vs 10s) for faster traffic routing decisions
- Readiness has lower initial delay (5s vs 10s) since services should be ready quickly

### Implementation Status

| Service | /health | /ready | /metrics | Notes |
|---------|---------|--------|----------|-------|
| AC | ✅ | ✅ | ✅ | Reference implementation |
| GC | ✅ | ⚠️ | ✅ | Missing /ready endpoint (uses /health for both probes) |
| MC | ❌ | ❌ | ❌ | Not yet implemented |
| MH | ❌ | ❌ | ❌ | Not yet implemented |

**Action Items**:
- GC: Add `/ready` endpoint with database + AC JWKS checks
- MC/MH: Implement all operational endpoints when services reach deployment phase

## Operational Thresholds

| Threshold | Value | Action |
|-----------|-------|--------|
| CPU sustained | >80% for 5min | Scale out, alert |
| Memory utilization | >85% | Scale out, alert |
| iowait | >20% sustained | Investigate disk, alert |
| CPU steal | >10% | Noisy neighbor, consider dedicated nodes |
| MH bandwidth warning | 70% | Emit metric, prepare to scale |
| MH bandwidth hard limit | 80% | Reject new participants, trigger immediate scale |
| Canary error threshold | 150% of baseline | Auto-rollback |
| Connection pool scale-up | 80% utilization | Add connections |
| Connection pool scale-down | 30% utilization | Reduce connections |
| WAL archive lag warning | 300s | Alert |
| WAL archive lag critical | 600s | Page on-call |

## Cost Estimation

**Minimal Deployment (~10 concurrent meetings, single region, dev/staging)**:

| Category | Monthly Cost |
|----------|--------------|
| Compute (1 node, shared services) | ~$150 |
| Database (single PostgreSQL, single Redis) | ~$50 |
| Bandwidth | ~$10 |
| **Total** | **~$210** |

**Baseline (1,000 concurrent meetings, 10 participants avg)**:

| Category | Monthly Cost |
|----------|--------------|
| Compute (GC, MC, MH, AC) | $15,395 |
| Database (PostgreSQL, Redis) | $1,009 |
| Bandwidth | $591 |
| **Total** | **$16,995** |

**Sensitivity Analysis**:
- Regional failover: +37% ($23,360/month)
- 2x traffic spike: +84% ($31,268/month)
- Low utilization (20%): -72% ($4,756/month)

## Open Items

Deferred to implementation phase:

1. **MC Redis connection pooling**: Document pool size per replica and failover behavior
2. **MC backpressure algorithm**: Document exponential backoff and timeout values
3. **MC signaling latency SLO**: Add alerting when latency exceeds 100ms
4. **Security PSS migration**: Clarify K8s 1.25+ PSA requirements
5. **Prometheus cardinality**: Add cardinality monitoring alerts
6. **MC PostgreSQL access**: Debate whether MC needs direct PostgreSQL access, or if GC should own all meeting metadata (preferred for cross-region sync simplicity)
7. **MH UDP multiplexing**: Design how MH multiplexes client and inter-MH media streams onto single UDP:443 port

## Consequences

### Positive

- **Cloud-agnostic**: Deploy to any major cloud or on-prem
- **Reproducible**: Same manifests work locally and in production
- **Secure by default**: mTLS, PSS, image scanning, default-deny network
- **Observable**: Same dashboards in dev and prod
- **Cost-effective**: Self-managed PostgreSQL saves 30-50%
- **Fast rollback**: <100ms feature flag propagation, <2min canary rollback

### Negative

- **Operational complexity**: Kubernetes expertise required (CKA-level knowledge recommended for on-call)
- **Self-managed databases**: Need PostgreSQL/Redis operational knowledge (DBA experience or CloudNativePG familiarity)
- **Initial setup time**: Creating all manifests, operators, CI pipelines (platform engineering work)

### Risks

- **CloudNativePG maturity**: Less battle-tested than managed RDS (mitigated by operator community)
- **Linkerd adoption**: Smaller community than Istio (mitigated by simpler architecture)
- **kind limitations**: Not perfect cloud parity for edge cases (mitigated by staging environment)

## Implementation Plan

1. **Phase 1**: Create directory structure and base manifests
2. **Phase 2**: Implement Dockerfiles for all services (AC first)
3. **Phase 3**: Set up local dev environment (kind + setup.sh)
4. **Phase 4**: Create per-service Kubernetes manifests
5. **Phase 5**: Implement CI/CD with Trivy scanning and canary deployment
6. **Phase 6**: Deploy platform infrastructure (Linkerd, CloudNativePG, etc.)
7. **Phase 7**: Create Terraform modules for cloud deployment

## References

- [Infrastructure Architecture Debate](../debates/2025-12-07-infrastructure-architecture.md)
- [AC Operational Readiness Review](../reviews/2025-12-07-ac-service-operational-readiness.md)
- [ADR-0011: Observability Framework](./adr-0011-observability-framework.md)
- [Kubernetes Documentation](https://kubernetes.io/docs/)
- [CloudNativePG](https://cloudnative-pg.io/)
- [Linkerd](https://linkerd.io/)
- [LitmusChaos](https://litmuschaos.io/)
