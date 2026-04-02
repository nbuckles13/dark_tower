# Devloop Output: MH Service Infrastructure

**Date**: 2026-04-01
**Task**: K8s manifests, Dockerfile, Kind overlay, and setup.sh wiring for mh-service
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — Full
**Branch**: `feature/mh-skeleton`
**Duration**: ~18m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a104d70c57e5c7ba188541f07d00b38ede492d34` |
| Branch | `feature/mh-skeleton` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-infra-devloop` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `security@mh-infra-devloop` |
| Test | `test@mh-infra-devloop` |
| Observability | `observability@mh-infra-devloop` |
| Code Quality | `code-reviewer@mh-infra-devloop` |
| DRY | `dry-reviewer@mh-infra-devloop` |
| Operations | `operations@mh-infra-devloop` |

---

## Task Overview

### Objective
Add infrastructure for the MH stub service: Dockerfile, K8s manifests, Kind overlay, and setup.sh wiring so MH runs in the Kind cluster alongside AC, GC, MC.

### Scope
- **Service(s)**: mh-service infrastructure
- **Schema**: No
- **Cross-cutting**: Yes — updates GC, MC, and AC network policies

### Debate Decision
NOT NEEDED — follows existing infrastructure patterns.

---

## Planning

Implementer drafted plan covering Dockerfile, 7 K8s manifests, Kind overlay, setup.sh wiring, TLS certs, and Prometheus config. All 6 reviewers confirmed with detailed input. Key additions from review: NodePort (not ClusterIP), Kind UDP port mapping, cross-service network policy updates (GC, MC, AC), Prometheus scrape job.

---

## Pre-Work

MH service code committed in previous devloop (2026-04-01-mh-stub-service).

---

## Implementation Summary

### New Files (9)
| File | Purpose |
|------|---------|
| `infra/docker/mh-service/Dockerfile` | Multi-stage cargo-chef build, distroless runtime |
| `infra/services/mh-service/kustomization.yaml` | Resource list + managed-by label |
| `infra/services/mh-service/deployment.yaml` | 2 replicas, security hardened, TLS mount, health probes |
| `infra/services/mh-service/service.yaml` | NodePort: 8083/50053/4434(UDP) |
| `infra/services/mh-service/configmap.yaml` | All MH env vars |
| `infra/services/mh-service/secret.yaml` | MH_CLIENT_SECRET (dev value) |
| `infra/services/mh-service/network-policy.yaml` | MC ingress, client UDP, GC/AC egress |
| `infra/services/mh-service/pdb.yaml` | minAvailable: 1 |
| `infra/kubernetes/overlays/kind/services/mh-service/kustomization.yaml` | Kind overlay |

### Modified Files (9)
| File | Change |
|------|--------|
| `infra/kubernetes/overlays/kind/services/kustomization.yaml` | Added mh-service/ |
| `infra/kind/kind-config.yaml` | Added UDP 30434→4434 mapping |
| `infra/services/gc-service/network-policy.yaml` | Added MH ingress on 50051 |
| `infra/services/mc-service/network-policy.yaml` | Added MH egress on 50053 |
| `infra/services/ac-service/network-policy.yaml` | Added MH ingress on 8082 |
| `infra/kubernetes/observability/prometheus-config.yaml` | Added MH scrape job |
| `infra/docker/prometheus/prometheus.yml` | Fixed MH port to 8083 |
| `scripts/generate-dev-certs.sh` | Added MH WebTransport cert |
| `infra/kind/scripts/setup.sh` | Added MH deploy functions, TLS, port-forward |

### Bug Fix
| File | Change |
|------|--------|
| `scripts/guards/simple/validate-env-config.sh` | Fixed awk `\s` → `[[:space:]]` portability bug |

---

## Devloop Verification Steps

### Layers 1-5: PASS
Compile, format, guards (16/16), tests, clippy all pass.

### Layer 6: PASS (pre-existing wtransport advisory only)

### Layer 7: Semantic Guard — PASS

---

## Code Review Results

### Security
**Verdict**: PASS
**Findings**: 1 found, 1 fixed
- AC network policy missing MH ingress → fixed

### Test
**Verdict**: PASS
**Findings**: 6 (2 blocking fixed, 4 non-blocking deferred)
- B1: UDP ingress missing → fixed
- B2: Static handler ID → removed, auto-generated per pod
- N1-N4: Redundant create_mh_secrets, skaffold.yaml, env-tests gap, service-monitor — deferred

### Observability
**Verdict**: PASS
**Findings**: 2 non-blocking (missing ServiceMonitor, stale comment)

### Code Quality
**Verdict**: PASS
**Findings**: 1 must-fix, 1 advisory
- UDP ingress missing → fixed
- MC egress path → advisory, deferred

### DRY
**Verdict**: CLEAR
**Findings**: 1 observation (image-load helper candidate)

### Operations
**Verdict**: PASS
**Findings**: 1 minor (redundant create_mh_secrets)

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Deferral Justification |
|---------|----------|------------------------|
| Redundant create_mh_secrets() | Test, Operations | Harmless idempotent, can clean up later |
| skaffold.yaml not updated | Test | Secondary dev tool, setup.sh is primary |
| env-tests ClusterConnection lacks MH | Test | Address when MH-specific env-tests added |
| Missing service-monitor.yaml | Test, Observability | Requires Prometheus Operator CRD |
| MC egress to MH | Code Quality | MH doesn't need MC egress currently |

---

## Rollback Procedure

1. Start commit: `a104d70c57e5c7ba188541f07d00b38ede492d34`
2. `git diff a104d70..HEAD`
3. `git reset --soft a104d70` or `git reset --hard a104d70`

---

## Reflection

All 7 teammates updated INDEX.md files. Guard portability bug fixed.

---

## Issues Encountered & Resolutions

### Issue 1: validate-env-config guard portability bug
**Problem**: Guard used `\s` in awk regex which works in gawk but not mawk/POSIX awk
**Resolution**: Changed to `[[:space:]]` — fixed all false positives across all services

### Issue 2: INDEX glob-style paths
**Problem**: Brace expansion syntax flagged as stale pointers by INDEX guard
**Resolution**: Expanded to individual paths or used pointer-to-one-plus-comment format

---

## Lessons Learned

1. Cross-service network policy updates are easy to miss — MH needed ingress rules added to GC, MC, AND AC
2. Static handler IDs in configmaps cause replica conflicts — prefer auto-generation for multi-replica deployments
3. Guard scripts need POSIX-compatible regex — `\s` is a gawk extension
