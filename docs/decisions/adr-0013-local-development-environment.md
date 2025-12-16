# ADR-0013: Local Development Environment Architecture

**Status**: Accepted (Revised)
**Date**: 2025-12-11
**Revised**: 2025-12-11
**Deciders**: Infrastructure (lead), AC, Observability, Operations, Test, Security specialists
**Debate**: Multi-agent debate, 3 rounds initial + 1 revision round, 93.7% final consensus

## Context

Dark Tower needs a local development environment that provides:
1. Kubernetes parity with CI and production
2. Full observability (logs, metrics, visualizations)
3. Security validation (NetworkPolicy enforcement)
4. Ability to run and debug chaos tests locally

ADR-0012 specified kind as the primary local development environment.

## Decision

We adopt a **single-tier development environment** using kind with Podman, providing full production parity.

### Single Tier: kind + Podman (Full Stack)

**Purpose**: Development, testing, and CI - same environment everywhere
**Startup time**: ~2-3 minutes (one-time per session; infrastructure stays up while iterating on services)
**Entry point**: `./infra/kind/scripts/setup.sh`
**Container runtime**: Podman (rootless, daemonless)

Components:
- kind cluster with **Calico CNI** (NetworkPolicy enforcement)
- PostgreSQL StatefulSet
- Redis StatefulSet
- AC service Deployment
- Full observability stack:
  - Prometheus (metrics)
  - Grafana (dashboards, pre-configured)
  - Loki (log aggregation)
- LitmusChaos (available for local chaos testing)

### Environment Parity

```
Dev (kind + Podman)     CI (kind)              Production (EKS/GKE)
───────────────────     ─────────────────      ─────────────────────
Calico CNI              Calico CNI             Calico (or similar)
Prometheus + Grafana    Prometheus + Grafana   Prometheus + Grafana
Loki                    Loki                   Loki
LitmusChaos available   LitmusChaos runs       N/A

Same manifests, same observability, same network policies
```

## Rationale

### Why Single Tier?

The initial debate proposed three tiers (Docker Compose / kind basic / kind full). User feedback identified critical issues:

1. **Environment drift**: Three tiers means three configurations to keep in sync
2. **Dev/CI parity**: Issues caught in CI should be reproducible locally
3. **Chaos test debugging**: Need to reproduce and fix chaos test failures locally
4. **Observability**: Grafana's Loki explorer is superior to `kubectl logs`

The ~2 minute startup cost is acceptable because:
- Infrastructure (PostgreSQL, Redis, observability) stays up
- Service iteration is fast (Skaffold rebuilds, `kubectl rollout restart`)
- One-time cost per development session

### Why Podman?

Podman was already selected in earlier project decisions (see `docs/LOCAL_TESTING_SETUP.md`):
- **Rootless**: Better security, no daemon running as root
- **Daemonless**: No background daemon required
- **Docker-compatible**: Drop-in replacement for Docker CLI
- **Production-aligned**: Same container runtime concepts

### Why Calico in Dev?

NetworkPolicy enforcement locally means:
- Developers catch policy violations immediately
- No "works locally, blocked in CI" surprises
- Security testing happens during development, not after

### Why Loki in Dev?

Log aggregation provides:
- Grafana log explorer (filter, search, correlate)
- Historical log analysis across service restarts
- Same debugging experience as production

### Why LitmusChaos Locally?

When chaos tests fail in CI:
- Developers can reproduce failures locally
- Debug and iterate on fixes without pushing to CI
- Validate fixes before creating PR

## Implementation

### Directory Structure

```
infra/
├── k8s/                          # Kubernetes manifests (shared across all envs)
│   ├── namespaces/
│   ├── postgres/
│   ├── redis/
│   ├── observability/            # Prometheus, Grafana, Loki
│   ├── ac-service/
│   └── network-policies/
├── grafana/                      # Shared Grafana configuration
│   ├── provisioning/
│   │   ├── datasources/
│   │   │   └── datasources.yaml  # Prometheus, Loki endpoints
│   │   └── dashboards/
│   │       └── dashboards.yaml   # Dashboard provider config
│   └── dashboards/
│       ├── ac-service.json       # AC service metrics
│       └── infrastructure.json   # PostgreSQL, Redis health
├── kind/
│   ├── kind-config.yaml          # Calico-ready cluster config
│   └── scripts/
│       ├── setup.sh              # Full stack setup
│       └── teardown.sh           # Cluster teardown
└── docker/                       # Dockerfiles only
    └── ac-service/
        └── Dockerfile
```

### Setup Script

```bash
./infra/kind/scripts/setup.sh
```

This script deploys **infrastructure only**:
1. Creates kind cluster with Calico CNI
2. Deploys PostgreSQL and Redis
3. Deploys observability stack (Prometheus, Grafana, Loki, Promtail)
4. Runs database migrations
5. Sets up port-forwarding

**Note**: The AC service is NOT deployed by setup.sh. Developers run it locally.

### Service Access

| Service | Access Method | URL/Connection |
|---------|---------------|----------------|
| AC Service (local) | cargo run | http://localhost:8082 |
| AC Service (Skaffold) | Port-forward | http://localhost:8083 |
| Grafana | Port-forward | http://localhost:3000 (admin/admin) |
| Prometheus | Port-forward | http://localhost:9090 |
| PostgreSQL | Port-forward | localhost:5432 |

### Grafana Pre-Configuration

Grafana starts with:
- **Datasources**: Prometheus, Loki auto-configured
- **Dashboards**: AC service dashboard pre-loaded
- **No manual setup**: Open browser, everything works

### Development Workflow

**Primary workflow: Local `cargo run`** (fast iteration)

```bash
# One-time setup (per session)
./infra/kind/scripts/setup.sh

# Run AC service locally
export DATABASE_URL="postgresql://darktower:dev_password_change_in_production@localhost:5432/dark_tower"
export AC_MASTER_KEY="$(./scripts/generate-master-key.sh)"
cargo run --bin auth-controller

# Metrics appear in Grafana (Prometheus scrapes localhost:8082)
# Logs go to stdout

# Teardown when done
./infra/kind/scripts/teardown.sh
```

**Alternative: Skaffold** (full K8s observability)

```bash
# Use when you need logs in Loki or K8s-specific testing
cd infra
skaffold dev  # AC available at localhost:8083
```

### Observability Matrix

| Mode | Metrics | Logs |
|------|---------|------|
| Local `cargo run` | Grafana (Prometheus scrapes localhost) | stdout |
| Skaffold in-cluster | Grafana (Prometheus) | Grafana/Loki |

## Consequences

### Positive

1. **Full parity**: Dev environment matches CI and production
2. **No drift**: One set of manifests, one configuration
3. **Early detection**: NetworkPolicy issues caught in dev
4. **Better debugging**: Grafana + Loki for log analysis
5. **Chaos testing**: Reproduce and fix CI failures locally

### Negative

1. **Startup time**: ~2-3 minutes for full stack (one-time per session)
2. **Resource usage**: More memory than Docker Compose (~2-4GB)
3. **Learning curve**: Developers need basic kubectl knowledge

### Mitigation

- Infrastructure stays up; only services are rebuilt during iteration
- Clear documentation in `docs/LOCAL_DEVELOPMENT.md`
- Setup script handles all complexity

## Files Removed

The following files from the initial implementation are no longer needed:

- `Makefile` - Removed (minimal value, just indirection)
- `infra/local/kind-config.yaml` - Moved to `infra/kind/kind-config.yaml`
- `infra/local/kind-config-ci.yaml` - Removed (single config for all envs)
- `infra/local/skaffold.yaml` - Moved to `infra/skaffold.yaml`
- `infra/local/grafana/` - Moved to `infra/grafana/`
- `docs/LOCAL_TESTING_SETUP.md` - Merged into `docs/LOCAL_DEVELOPMENT.md`

## Compliance

- **ADR-0012**: kind as primary local environment (now single tier, not tiered)
- **Security**: NetworkPolicy enforced in dev and CI
- **Podman**: Rootless containers as previously decided

## References

- ADR-0012: Infrastructure Architecture
- Multi-agent debate: 2025-12-11-local-development-environment.md
- Skaffold documentation: https://skaffold.dev/
- Calico documentation: https://docs.projectcalico.io/
- Podman documentation: https://podman.io/
