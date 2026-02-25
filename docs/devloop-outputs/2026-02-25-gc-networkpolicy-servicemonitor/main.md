# Devloop Output: Fix GC NetworkPolicy MC Egress + Enable ServiceMonitor

**Date**: 2026-02-25
**Task**: Fix GC NetworkPolicy to allow MC egress on TCP 50052 and enable GC ServiceMonitor for Prometheus scraping
**Specialist**: infrastructure
**Mode**: Agent Teams (full)
**Branch**: `feature/meeting-create-task0`
**Duration**: ~7m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d9277416dc59e9515e6e90c71a4949efc8f8a04a` |
| Branch | `feature/meeting-create-task0` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-gc-netpol` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@devloop-gc-netpol` |
| Test | `test@devloop-gc-netpol` |
| Observability | `observability@devloop-gc-netpol` |
| Code Quality | `code-reviewer@devloop-gc-netpol` |
| DRY | `dry-reviewer@devloop-gc-netpol` |
| Operations | `operations@devloop-gc-netpol` |

---

## Task Overview

### Objective
Fix GC NetworkPolicy to add egress rule allowing traffic to MC pods on TCP 50052, and enable GC ServiceMonitor by uncommenting its spec section for Prometheus scraping.

### Scope
- **Service(s)**: GC (infrastructure manifests)
- **Schema**: No
- **Cross-cutting**: Yes (networking between GC and MC, observability pipeline)

### Debate Decision
NOT NEEDED - Straightforward infrastructure changes per existing architecture (ADR-0012)

---

## Planning

Implementer proposed two changes:
1. Add MC egress rule after existing AC egress rule in GC NetworkPolicy, targeting `app: mc-service` on TCP 50052 in `dark-tower` namespace
2. Uncomment GC ServiceMonitor spec and remove stale Phase 3 comments

All 6 reviewers confirmed the plan on first round. No revision needed.

---

## Pre-Work

None

---

## Implementation Summary

### R-13: GC NetworkPolicy MC Egress
| Item | Before | After |
|------|--------|-------|
| GC→MC egress | Not allowed | Allowed on TCP 50052 to `app: mc-service` in `dark-tower` namespace |

Added egress rule (lines 58-68) following the identical pattern as the existing GC→AC egress rule. Symmetric with MC's ingress rule (mc-service/network-policy.yaml lines 19-28).

### R-14: GC ServiceMonitor
| Item | Before | After |
|------|--------|-------|
| ServiceMonitor spec | Commented out (Phase 3 placeholder) | Active — scrapes `/metrics` on port `http` every 30s |

Uncommented the spec section, removed stale Phase 3 TODO comments. GC is now the first service with an active ServiceMonitor.

---

## Files Modified

```
 docs/specialist-knowledge/dry-reviewer/INDEX.md |  6 +++++
 docs/specialist-knowledge/security/INDEX.md     |  1 +
 infra/services/gc-service/network-policy.yaml   | 11 +++++++++
 infra/services/gc-service/service-monitor.yaml  | 30 +++++++++----------------
 4 files changed, 29 insertions(+), 19 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `infra/services/gc-service/network-policy.yaml` | Added MC egress rule on TCP 50052 |
| `infra/services/gc-service/service-monitor.yaml` | Uncommented spec, removed Phase 3 comments |
| `docs/specialist-knowledge/security/INDEX.md` | Added GC-to-MC NetworkPolicy egress seam |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` | Added K8s manifest patterns and egress/ingress pairs |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Output**: Workspace compiles cleanly (no Rust changes)

### Layer 2: cargo fmt
**Status**: PASS
**Output**: No formatting issues

### Layer 3: Simple Guards
**Status**: ALL PASS
**Duration**: ~5s

| Guard | Status |
|-------|--------|
| api-version-check | PASS |
| grafana-datasources | PASS |
| instrument-skip-all | PASS |
| no-hardcoded-secrets | PASS |
| no-pii-in-logs | PASS |
| no-secrets-in-logs | PASS |
| no-test-removal | PASS |
| test-coverage | PASS |
| test-registration | PASS |
| test-rigidity | PASS |
| validate-application-metrics | PASS |
| validate-histogram-buckets | PASS |
| validate-infrastructure-metrics | PASS |
| validate-knowledge-index | PASS |

### Layer 4: Tests
**Status**: PASS
**Output**: All tests pass

### Layer 5: Clippy
**Status**: PASS
**Output**: No warnings

### Layer 6: Audit
**Status**: PASS (pre-existing only)
**Output**: 2 pre-existing vulnerabilities (ring 0.16.20, rsa 0.9.10) — transitive deps, no new deps added

### Layer 7: Semantic Guards
**Status**: PASS

| File | Verdict | Notes |
|------|---------|-------|
| `network-policy.yaml` | SAFE | Minimally scoped egress, correct port/selector |
| `service-monitor.yaml` | SAFE | Standard spec, no security concerns |

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Egress rule minimally scoped. ServiceMonitor exposes only operational metrics. No security concerns.

### Test Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Existing env-tests (`test_all_services_scraped_by_prometheus`) cover ServiceMonitor. GC→MC canary test deferred as acceptable.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

ServiceMonitor config correct and consistent with AC/MC patterns. Pre-existing note: prometheus.yml targets gc-service:8000 (should be 8080) — out of scope.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

ADR-0012 compliant. Follows existing manifest patterns. YAML structure consistent.

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None
**Extraction opportunities**: None new

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Both changes are additive, safe, and easily reversible. No pod restarts needed.

---

## Tech Debt

### Deferred Findings

No deferred findings — all reviewers CLEAR.

### Cross-Service Duplication (from DRY Reviewer)

No cross-service duplication detected.

### Pre-Existing Issues Noted

| Item | Location | Note |
|------|----------|------|
| Prometheus port mismatch | `infra/docker/prometheus/prometheus.yml:28` | Targets gc-service:8000, actual port is 8080 |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `d9277416dc59e9515e6e90c71a4949efc8f8a04a`
2. Review all changes: `git diff d9277416dc59e9515e6e90c71a4949efc8f8a04a..HEAD`
3. Soft reset (preserves changes): `git reset --soft d9277416dc59e9515e6e90c71a4949efc8f8a04a`
4. Hard reset (clean revert): `git reset --hard d9277416dc59e9515e6e90c71a4949efc8f8a04a`
5. For infrastructure changes: may require `kubectl delete -f` if manifests were applied

---

## Reflection

All teammates confirmed no significant INDEX.md updates needed. Two reviewers added navigation entries:
- **Security**: Added GC-to-MC NetworkPolicy egress seam pointer
- **DRY**: Added K8s manifest patterns section and egress/ingress pair cross-references

Key observation: GC is now the first service with an active ServiceMonitor. AC and MC should follow the same uncomment pattern when their /metrics endpoints are implemented.

---

## Issues Encountered & Resolutions

None — clean single-iteration pass.

---

## Lessons Learned

1. Infrastructure-only YAML changes pass the full Rust validation pipeline trivially (no code changes = no regressions)
2. NetworkPolicy egress/ingress symmetry should always be verified bidirectionally
3. GC ServiceMonitor is now the reference pattern for enabling AC and MC ServiceMonitors
