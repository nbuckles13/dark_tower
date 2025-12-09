# Infrastructure Architecture Debate

**Date**: 2025-12-07
**Topic**: Dark Tower Infrastructure Architecture
**Rounds**: 4
**Final Consensus**: 91.6% average (8/9 specialists ≥90%, 1 at 89%)
**Status**: Consensus Reached

## Participants

| Specialist | Role | Final Score |
|------------|------|-------------|
| Infrastructure | Lead proposer | 95% (self-score) |
| AC (Auth Controller) | Service representative | 92% |
| GC (Global Controller) | Service representative | 91% |
| MC (Meeting Controller) | Service representative | 89% |
| MH (Media Handler) | Service representative | 92% |
| Operations | Cross-cutting | 93% |
| Security | Cross-cutting | 92% |
| Observability | Cross-cutting | 92% |
| Test | Cross-cutting | 92% |
| Database | Cross-cutting | 91% |

## Context

The AC Operational Readiness Review identified critical infrastructure gaps:
- No Dockerfile or container image definitions
- No Kubernetes manifests
- No deployment strategy
- No local development environment parity with cloud
- No secrets management infrastructure

This debate established the comprehensive infrastructure architecture for Dark Tower.

## Round Summary

### Round 1: Initial Proposal (Infrastructure)

**Infrastructure Self-Score**: 85%

**Key Proposals**:
- Directory structure: `infra/base/`, `infra/services/{service}/`, `infra/terraform/`, `infra/local/`
- Local dev: Docker Compose OR kind/k3d with multi-region simulation
- Cloud: Standard K8s Deployment pattern with HPA, PDB, SecurityContext
- Secrets: external-secrets-operator with cloud backends
- Network: Default-deny NetworkPolicies
- Chaos testing: LitmusChaos

**Round 1 Scores**:
| Specialist | Score | Key Concerns |
|------------|-------|--------------|
| AC | 85% | Health endpoints, master key rotation, connection pool sizing |
| GC | 78% | HTTP/3 support, geographic routing, session affinity |
| MC | 72% | UDP support, memory sizing, Redis HA |
| MH | 68% | Memory insufficient, UDP port range, dedicated node pool |
| Operations | 78% | Deployment strategy, rollback, graceful shutdown |
| Security | 82% | Image scanning, mTLS, PSS |
| Observability | 75% | SLO-HPA integration, dashboard parity |
| Test | 80% | Chaos CI integration, test isolation |
| Database | 79% | CloudNativePG vs managed, connection pools, backups |

### Round 2: Addressing Core Concerns

**Key Updates**:
- AC: StatefulSet for key management, AC_MASTER_KEY rotation procedure
- GC: HTTP/3 via L4 UDP load balancer, GeoDNS routing
- MC: Memory increased to 6Gi, Redis Cluster mode
- MH: Memory increased to 12Gi, dedicated node pool with taints
- Operations: Canary deployment via Flagger, preStop hooks
- Security: Trivy scanning, Linkerd mTLS, Restricted PSS
- Observability: OTel DaemonSet, ConfigMap dashboards
- Database: CloudNativePG chosen, PgBouncer sidecar, backup to S3

**Round 2 Scores** (partial - some interrupted):
| Specialist | Score | Status |
|------------|-------|--------|
| AC | 92% | ✅ Above threshold |
| GC | 82% | Needs more detail |
| MH | 82% | Needs more detail |
| Operations | 88% | Close to threshold |
| Test | 92% | ✅ Above threshold |
| Database | 85% | Needs more detail |

### Round 3: Detailed Specifications

**Key Updates**:
- GC: Connection ID format (8-byte timestamp + 12-byte random), 5s health check SLA, 3s HTTP/1.1 fallback timeout
- MH: Per-stage latency monitoring, NetworkPolicy CI validation, cascading bandwidth assumptions (max 2 hops, 2.5Mbps/stream)
- Operations: Cost estimation ($35K/month baseline), runbook templates, feature flag strategy
- Database: RTO 30s/RPO 5s, cross-region async replication, backup restore CI testing

**Round 3 Scores**:
| Specialist | Score | Notes |
|------------|-------|-------|
| GC | 91% | ✅ Concerns addressed |
| MH | 78% | Regressed - found new concerns in detail |
| Operations | 78% | Regressed - wants more specifics |
| Database | 88% | Improved but needs more |

### Round 4: Final Refinements

**Key Updates**:

**MH Concerns Addressed**:
- Bandwidth thresholds: 70% warning, 80% hard limit (down from 83%)
- Backpressure: Redis-based regional coordination with least-loaded redirect
- P99 handling: 4-stage quality degradation (optimal→warning→degraded→critical)
- NetworkPolicy: Full Kubernetes integration test in CI

**Operations Concerns Addressed**:
- Cost sensitivity: 4 scenarios (baseline, failover +37%, 2x spike +84%, low -72%)
- etcd failover: 3-tier fallback (etcd→cache→static), 3-replica StatefulSet
- K8s runbooks: Detailed drain/uncordon, pod eviction, session migration
- Rollback latency: WebSocket push <100ms + 5s poll fallback
- Escalation: 3-level matrix with auto-escalation rules

**Database Concerns Addressed**:
- Sync failover: FIRST 1 priority list, fallback_to_async=true
- Cross-region: Async streaming to DR region with PITR
- Pool auto-remediation: Dynamic scaling 10→50, circuit breaker
- Key rotation testing: 3 comprehensive test scenarios
- Monitoring: Full Prometheus exporter spec

**Round 4 Final Scores**:
| Specialist | Score |
|------------|-------|
| AC | 92% |
| GC | 91% |
| MC | 89% |
| MH | 92% |
| Operations | 93% |
| Security | 92% |
| Observability | 92% |
| Test | 92% |
| Database | 91% |

## Decisions Made

### Selected Approaches

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Container orchestration | Kubernetes | Cloud-agnostic, portable |
| Local dev environment | kind with multi-region namespaces | K8s parity, same manifests |
| Service mesh / mTLS | Linkerd 2.x | Zero-config mTLS, Rust proxy, lightweight |
| Database | CloudNativePG | Cloud-agnostic, GitOps-friendly, cost-effective |
| Redis HA | Redis Cluster | Sharding + HA, no Sentinel complexity |
| Secrets management | external-secrets-operator | Cloud-agnostic, syncs from any provider |
| Image scanning | Trivy | Open source, SBOM support |
| Chaos testing | LitmusChaos | K8s-native, CNCF project |
| Deployment strategy | Canary via Flagger | Progressive rollout, auto-rollback |
| Feature flags | etcd + WebSocket push | <100ms propagation, 3-tier fallback |
| Observability | OTel DaemonSet + Prometheus | Resource efficient, tail-based sampling |

### Considered But Not Selected

| Alternative | Why Not Selected |
|-------------|------------------|
| **Docker Compose only** | Doesn't provide K8s parity, can't test NetworkPolicies |
| **Istio for mTLS** | Higher resource overhead, more complex than needed |
| **Redis Sentinel** | No sharding support, less scalable for meeting state |
| **Managed PostgreSQL (RDS/Cloud SQL)** | Cloud lock-in, higher cost, less GitOps-friendly |
| **Sidecar OTel Collector** | Higher resource usage (one per pod vs one per node) |
| **Head-based trace sampling** | Can't sample 100% of errors retroactively |
| **Blue-green deployment** | More resource intensive, less granular than canary |
| **CPU pinning for MH** | Deferred - passthrough SFU doesn't need it initially |
| **GPU for MH** | Not needed for E2E encrypted passthrough |
| **etcd polling only** | 5s delay unacceptable for rollback; WebSocket push added |

### Resource Sizing Decisions

| Service | CPU Req/Limit | Memory Req/Limit | Notes |
|---------|---------------|------------------|-------|
| AC | 500m/2000m | 1Gi/2Gi | StatefulSet for key management |
| GC | 1000m/3000m | 2Gi/4Gi | HTTP/3 gateway |
| MC | 2000m/4000m | 6Gi/8Gi | Increased from 4Gi |
| MH | 4000m/8000m | 12Gi/16Gi | Increased from 8Gi, dedicated node pool |

### Threshold Decisions

| Threshold | Value | Rationale |
|-----------|-------|-----------|
| MH bandwidth warning | 70% | Conservative for quality headroom |
| MH bandwidth hard limit | 80% | Reject new participants, trigger scale |
| Pool scale-up | 80% utilization | Dynamic pool sizing |
| Pool scale-down | 30% utilization | Avoid over-provisioning |
| Canary error threshold | 150% of baseline | Auto-rollback trigger |
| WAL archive lag warning | 300s | RPO risk indicator |
| WAL archive lag critical | 600s | Immediate action required |

## Open Items

These concerns were noted but deferred for implementation phase:

### MC (89% - Below 90% Threshold)

1. **Redis connection pooling specifics**: Document connection pool size per MC replica and failover behavior when connections exhaust
2. **Backpressure coordination algorithm**: Document exponential backoff strategy and timeout values for MC-to-MC coordination
3. **Signaling latency SLO alerting**: Add alerting when signaling latency exceeds 100ms threshold

### Minor Refinements from Other Specialists

- **Security**: Clarify K8s version requirement for PSS (1.25+ uses PSA, not PSP)
- **Security**: Document mTLS certificate rotation recovery for long-lived connections
- **Observability**: Add Prometheus cardinality monitoring alerts
- **Observability**: CI gate for dashboard JSON validation
- **Database**: Document behavior when sync replica AND async replicas all fail

## Outcome

**Consensus**: 91.6% average satisfaction across all specialists.

**ADR Created**: ADR-0012 documents the architectural decisions.

**Next Steps**:
1. Create directory structure per ADR
2. Implement Dockerfiles for all services
3. Create base Kubernetes manifests
4. Set up local dev environment (kind + namespaces)
5. Implement CI/CD pipeline with Trivy scanning
6. Address MC open items during Phase 6 implementation
