# Devloop Output: Split Grafana Dashboards ConfigMap

**Date**: 2026-02-17
**Task**: Split monolithic grafana-dashboards ConfigMap into per-service ConfigMaps
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — Light
**Branch**: `feature/gc-registered-mc-metrics`
**Duration**: ~10m (2 iterations)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `3ef8e8418016a097878d85668508f92cb6d14c8b` |
| Branch | `feature/gc-registered-mc-metrics` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@split-grafana-configmap-v2` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `security@split-grafana-configmap-v2` |
| Operations | `operations@split-grafana-configmap-v2` |

---

## Task Overview

### Objective
Split the monolithic `grafana-dashboards` ConfigMap in `infra/kind/scripts/setup.sh` into per-service ConfigMaps to fix the Kubernetes 262144-byte annotation limit error and prepare for per-datacenter deployment topology where services deploy independently.

### Scope
- **Service(s)**: Infrastructure (Kind local dev setup)
- **Schema**: No
- **Cross-cutting**: No (infrastructure only)

### Debate Decision
NOT NEEDED - Single-component infrastructure change

---

## Planning

Skipped (--light mode)

---

## Pre-Work

None

---

## Implementation Summary

### Iteration 1: Per-Service ConfigMap Split
| Item | Before | After |
|------|--------|-------|
| Dashboard ConfigMaps | 1 monolithic (~292KB) | 4 per-service (hardcoded) |
| Volume type | configMap | projected (4 hardcoded sources) |
| Apply method | client-side | client-side |

### Iteration 2: Dynamic Discovery + Sidecar
| Item | Before (Iteration 1) | After (Iteration 2) |
|------|----------------------|---------------------|
| ConfigMap creation | 4 hardcoded blocks | Dynamic loop discovering prefixes from filenames |
| ConfigMap labeling | None | `grafana_dashboard=1` on all dashboard ConfigMaps |
| Volume type | projected (hardcoded sources) | emptyDir populated by k8s-sidecar |
| Dashboard discovery | Manual projected volume entries | k8s-sidecar auto-discovers by label |
| Apply method | client-side | `--server-side` (no 262KB annotation limit) |
| RBAC | None | ServiceAccount + Role (read-only configmaps) + RoleBinding |
| Security | No securityContext | readOnlyRootFilesystem, runAsNonRoot, no privilege escalation |
| Stale cleanup | None | `kubectl delete configmap grafana-dashboards --ignore-not-found` |
| Edge cases | None | nullglob for empty directory, newline-separated iteration |

### Documentation
Updated `docs/observability/dashboards.md` with dynamic discovery pattern and zero-config workflow for adding dashboards.

---

## Files Modified

```
 docs/observability/dashboards.md |  21 ++++---
 infra/kind/scripts/setup.sh      | 125 ++++++++++++++++++++++++++++++++++++---
 2 files changed, 131 insertions(+), 15 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `infra/kind/scripts/setup.sh` | Dynamic ConfigMap loop, k8s-sidecar init container, RBAC, server-side apply, stale cleanup, nullglob, securityContext |
| `docs/observability/dashboards.md` | Dynamic discovery docs, zero-config dashboard addition workflow |

---

## Devloop Verification Steps

### Iteration 1
All layers PASS (see original results above).

### Iteration 2

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Guards
**Status**: ALL PASS (12/12)

### Layer 4: Tests
**Status**: N/A (no Rust changes)

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: PASS (2 pre-existing vulns: ring 0.16.20, rsa 0.9.10 — unrelated)

### Artifact-Specific: Shell
- `bash -n`: PASS (syntax valid)
- `shellcheck`: SKIP (not installed in environment)

---

## Code Review Results

### Iteration 1
- Security: CLEAR (0 findings)
- Operations: CLEAR (0 findings)

### Iteration 2

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

1. **Word-splitting / glob expansion risk** — Fixed: switched to newline-separated strings with `while IFS= read -r f` iteration
2. **Missing securityContext on sidecar** — Fixed: added readOnlyRootFilesystem, runAsNonRoot, allowPrivilegeEscalation: false

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed, 0 deferred

1. **Stale ConfigMap cleanup** — Fixed: added `kubectl delete configmap grafana-dashboards --ignore-not-found` before dynamic loop
2. **Empty directory edge case** — Fixed: added `shopt -s nullglob` with save/restore around glob loop

---

## Tech Debt

### Deferred Findings
No deferred findings — all findings fixed.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `3ef8e8418016a097878d85668508f92cb6d14c8b`
2. Review changes: `git diff 3ef8e84..HEAD`
3. Soft reset: `git reset --soft 3ef8e84`
4. Hard reset: `git reset --hard 3ef8e84`

---

## Reflection

Skipped (--light mode)

---

## Issues Encountered & Resolutions

None

---

## Lessons Learned

1. Kubernetes `kubectl apply` stores the full resource in the `last-applied-configuration` annotation, which has a 262144-byte limit — `--server-side` apply eliminates this entirely.
2. kiwigrid/k8s-sidecar with `METHOD=LIST` as an init container is the cleanest pattern for loading ConfigMap-based dashboards — one-shot copy, no race conditions, no projected volume maintenance.
3. Dynamic prefix discovery from filenames eliminates script edits when adding dashboards.
4. Shell loops over file lists need nullglob and newline-separated iteration to avoid word-splitting and empty-glob edge cases.

---

## Human Review (Iteration 2)

**Feedback**: "Replace hardcoded per-service ConfigMaps and projected volume with dynamic discovery using Grafana k8s-sidecar. The creation loop should discover dashboard prefixes from filenames, create labeled ConfigMaps (grafana_dashboard=1), and the Grafana Deployment should use kiwigrid/k8s-sidecar to auto-discover them. This eliminates maintaining the ConfigMap list in two places. Also switch to --server-side apply to avoid the 262KB annotation limit entirely."
