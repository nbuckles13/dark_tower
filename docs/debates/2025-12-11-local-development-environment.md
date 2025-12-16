# Multi-Agent Debate: Local Development Environment

**Date**: 2025-12-11
**Topic**: Kind-based Local Development Environment for Dark Tower
**Participants**: Infrastructure (lead), AC, Observability, Operations, Test, Security
**Outcome**: Consensus reached (92.8% average satisfaction)
**ADR**: ADR-0013

## Background

ADR-0012 specified kind as the primary local development environment for K8s parity. However, when attempting to implement this, several specialists raised concerns about the friction this would create for day-to-day development.

The user requested a local development environment with:
- Working logs
- Metrics with basic visualizations (Grafana)
- K8s parity from day one

## Round 1: Initial Proposals

### Infrastructure (Lead) - 85%

**Initial Proposal**:
- kind cluster with raw Prometheus configuration
- Bash scripts for setup/teardown in `infra/local/`
- PostgreSQL as StatefulSet in kind
- NodePort exposure for service access

**Concerns raised**: Startup time, NetworkPolicy enforcement, observability gaps

### AC Specialist - 78%

**Requirements**:
- Database migration automation during setup
- Token validation must work identically to production
- Hot reload for rapid iteration

**Concerns**: Port-forward complexity, migration timing

### Observability Specialist - 75%

**BLOCKER**: No log aggregation solution proposed

**Requirements**:
- Loki or equivalent for log collection
- Pre-built dashboards for AC service
- ServiceMonitor for Prometheus scraping

### Operations Specialist - 45%

**BLOCKER**: 2-3 minute startup time unacceptable

**Requirements**:
- <30 second startup for daily development
- Skaffold or Tilt for hot-reload
- Clear escape hatch to simpler environment

### Test Specialist - 68%

**Requirements**:
- Hybrid approach: Docker Compose for unit/integration, kind for K8s-specific
- Test database management strategy
- Coverage tools must work across tiers

### Security Specialist - 62%

**BLOCKER**: kindnet doesn't enforce NetworkPolicy

**Requirements**:
- Calico CNI for NetworkPolicy enforcement
- No NodePort exposure of PostgreSQL
- CI validation of security policies

**Round 1 Average**: 68.8% (BELOW 90% threshold)

## Round 2: Revised Proposal

### Infrastructure Response

**Key Changes**:
1. **Tiered approach**: Docker Compose (Tier 1), kind + Skaffold (Tier 2), CI (Tier 3)
2. **Skaffold adoption**: Hot-reload development loop
3. **Loki deferred to Tier 3**: Local dev uses kubectl logs
4. **Calico in CI only**: NetworkPolicy validation before merge
5. **Port-forward for DB**: No NodePort exposure

**Specialist Responses**:

| Specialist | Score | Feedback |
|------------|-------|----------|
| AC | 88% | "Need clear tier transition docs" |
| Observability | 90% | "Tiered observability is pragmatic" |
| Operations | 92% | "Docker Compose default solves my blocker" |
| Test | 88% | "Hybrid approach aligns with test needs" |
| Security | 85% | "Need CI validation guarantees" |

**Round 2 Average**: 88.8% (CLOSE to 90% threshold)

## Round 3: Final Refinements

### Infrastructure Final Additions

1. **CI Security Validation Job**: NetworkPolicy enforcement test + auth smoke test
2. **Tier Transition Documentation**: Clear guidance on when to use each tier
3. **Pre-built Grafana Dashboard**: AC service metrics dashboard JSON

### Final Scores

| Specialist | Score |
|------------|-------|
| Infrastructure | 95% |
| AC | 92% |
| Observability | 94% |
| Operations | 94% |
| Test | 92% |
| Security | 90% |
| **Average** | **92.8%** |

**CONSENSUS REACHED** ✅

## Final Design Summary

### Tier 1: Docker Compose (Default)
- Fast startup (<30s)
- Direct database access
- stdout/stderr logs
- Entry: `make dev`

### Tier 2: kind + Skaffold (K8s Validation)
- K8s manifest validation
- Prometheus + Grafana
- Port-forward for DB
- Entry: `make dev-k8s`

### Tier 3: CI (Full Stack)
- Calico CNI (NetworkPolicy enforcement)
- Loki for log aggregation
- LitmusChaos for chaos testing
- Security validation before merge

## Key Decisions

1. **Docker Compose remains the default** for day-to-day development
2. **kind is optional** (Tier 2) for K8s-specific validation
3. **Skaffold** is the development tool for kind
4. **NetworkPolicy enforcement** happens in CI, not local dev
5. **Loki** is CI-only; local dev uses native kubectl logs

## Implementation Tasks

1. Create `infra/local/kind-config.yaml` and `kind-config-ci.yaml`
2. Create `infra/local/skaffold.yaml` with profiles
3. Create setup/teardown scripts
4. Create Grafana dashboard JSON for AC service
5. Add CI job for K8s security validation
6. Write `docs/LOCAL_DEVELOPMENT.md` tier documentation
7. Update Makefile with `dev-k8s` target

## References

- ADR-0012: Infrastructure Architecture
- ADR-0013: Local Development Environment Architecture (this debate's output)

---

# Addendum: Revision Round (User Feedback)

**Date**: 2025-12-11 (same day)
**Trigger**: User feedback on initial implementation
**Outcome**: Revised to single-tier approach (93.7% consensus)

## User Feedback Summary

After initial implementation, the user raised several concerns:

| Topic | Initial Decision | User Feedback |
|-------|------------------|---------------|
| **Tiers** | 3 tiers (Docker Compose / kind / CI) | Single tier preferred - avoid drift |
| **Podman** | Not considered | Already decided earlier; should use Podman |
| **Calico** | CI-only | Include in dev - catch issues early |
| **Loki** | CI-only | Include in dev - Grafana explorer is better UX |
| **LitmusChaos** | CI-only | Available in dev - need to debug CI failures locally |
| **Grafana** | Basic in dev | Fully provisioned with datasources + dashboards |
| **Dashboards** | `infra/local/grafana/` | Shared `infra/grafana/` for all environments |
| **Makefile** | Created | Remove - minimal value |
| **Docker Compose** | Tier 1 default | Remove as dev environment |

### Key User Insights

1. **Development workflow**: Infrastructure stays up, iterate on services. The 2-min startup is one-time per session.
2. **Environment drift risk**: Three tiers = three things to keep in sync = drift.
3. **Debugging chaos tests**: Need to reproduce CI failures locally.
4. **Observability UX**: Grafana + Loki is better than `kubectl logs`.

## Revision Round: Specialist Responses

### Infrastructure Specialist (Lead) - 95%

**Revised to single-tier approach:**
- kind + Podman everywhere
- Calico CNI for NetworkPolicy enforcement
- Full observability (Prometheus, Grafana, Loki)
- LitmusChaos available locally

"User's points are valid. Single tier eliminates drift and provides full parity."

### AC Specialist - 92%

"Actually *better* for AC development. Consistent DATABASE_URL, NetworkPolicy testing locally, Loki for debugging."

### Observability Specialist - 98%

"Strongly support. Grafana provisioning with auto-loaded datasources and dashboards is the right approach."

### Operations Specialist - 88%

"Reconsidering my position. If infrastructure stays up and you iterate on services, startup time is one-time cost. Single-tier is more maintainable long-term."

**Note**: Score dropped from 92% (lost fast Docker Compose) but acknowledges single-tier benefits.

### Test Specialist - 94%

"Simplifies test matrix significantly. One environment, no drift, chaos tests reproducible locally."

### Security Specialist - 95%

"Addresses my previous blockers. Calico in dev means NetworkPolicy enforcement locally. Podman provides rootless containers."

## Revised Consensus

| Specialist | Round 3 Score | Revision Score | Delta |
|------------|---------------|----------------|-------|
| Infrastructure | 95% | 95% | 0 |
| AC | 92% | 92% | 0 |
| Observability | 94% | 98% | +4 |
| Operations | 94% | 88% | -6 |
| Test | 92% | 94% | +2 |
| Security | 90% | 95% | +5 |
| **Average** | **92.8%** | **93.7%** | **+0.9** |

**CONSENSUS MAINTAINED** ✅

## Revised Design Summary

### Single Tier: kind + Podman (Full Stack)

**Used for**: Development, CI, and as template for production

**Components**:
- kind cluster with Calico CNI
- PostgreSQL StatefulSet
- Redis StatefulSet
- Full observability (Prometheus, Grafana with pre-loaded dashboards, Loki)
- LitmusChaos (available for local testing)

**Entry point**: `./infra/kind/scripts/setup.sh`

### Environment Parity

```
Dev (kind + Podman)  ≈  CI (kind)  →  Prod (EKS/GKE)
Same manifests, same observability, same network policies
```

## Files to Remove/Relocate

| File | Action |
|------|--------|
| `Makefile` | Remove |
| `docs/LOCAL_TESTING_SETUP.md` | Remove (merge into LOCAL_DEVELOPMENT.md) |
| `infra/local/kind-config.yaml` | Move to `infra/kind/kind-config.yaml` |
| `infra/local/kind-config-ci.yaml` | Remove (single config) |
| `infra/local/skaffold.yaml` | Move to `infra/skaffold.yaml` |
| `infra/local/scripts/` | Move to `infra/kind/scripts/` |
| `infra/local/grafana/` | Move to `infra/grafana/` |
| `docker-compose.yml` | Keep for test DB in CI only |

## Revised Implementation Tasks

1. Restructure `infra/` directory per new layout
2. Update kind-config.yaml for Calico CNI
3. Update setup.sh for full stack (Loki, Calico)
4. Move Grafana dashboards to shared location
5. Create Grafana provisioning config
6. Update LOCAL_DEVELOPMENT.md
7. Remove obsolete files (Makefile, LOCAL_TESTING_SETUP.md, tiered configs)
