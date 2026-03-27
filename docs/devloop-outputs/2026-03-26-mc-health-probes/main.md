# Devloop Output: Enable MC Health Probes

**Date**: 2026-03-26
**Task**: Enable MC liveness/readiness health probes in K8s deployment.yaml
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/meeting-join-user-story-devloop`
**Duration**: ~10m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7b89d2af36e2e5a11de2d4265262aed555cce658` |
| Branch | `feature/meeting-join-user-story-devloop` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mc-health-probes` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@mc-health-probes` — CLEAR |
| Test | `test@mc-health-probes` — CLEAR |
| Observability | `observability@mc-health-probes` — CLEAR |
| Code Quality | `code-reviewer@mc-health-probes` — CLEAR |
| DRY | `dry-reviewer@mc-health-probes` — CLEAR |
| Operations | `operations@mc-health-probes` — CLEAR |

---

## Task Overview

### Objective
Enable the commented-out liveness and readiness health probes in the MC K8s deployment manifest (`infra/services/mc-service/deployment.yaml`). The MC health endpoints (`/health` for liveness, `/ready` for readiness) are already implemented in `crates/mc-service/src/observability/health.rs` on port 8081. The deployment.yaml has the probes commented out with a TODO referencing Phase 6h.

### Scope
- **Service(s)**: mc-service (infrastructure manifest only)
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - Straightforward enablement of existing health probes in K8s manifest.

---

## Planning

**Plan**: Uncomment liveness/readiness probes in MC deployment.yaml, fix paths from `/health/live` → `/health` and `/health/ready` → `/ready` to match actual code (`health.rs:88-89`), keep port 8081 and timing values matching GC pattern. Remove Phase 6h TODO comment.

**All 6 reviewers confirmed**:
- Security: No concerns. Health endpoints on dedicated port, no auth needed for K8s probes.
- Test: Path mismatch identified as critical fix. Existing test coverage in health.rs is comprehensive.
- Observability: Readiness behavior correct (starts false, set true after GC registration). Existing alerts cover probe failures.
- Code Quality: Noted ADR-0023 vs ADR-0012 path discrepancy — code follows ADR-0012 pattern, probes should match code.
- DRY: Probe config consistent with GC. Port difference (8081 vs 8080) expected.
- Operations: Rolling update strategy provides safe rollback. Pre-existing runbook port issue (8080 vs 8081) noted for future fix.

---

## Pre-Work

None

---

## Implementation Summary

### Health Probe Enablement
| Item | Before | After |
|------|--------|-------|
| Liveness probe | Commented out, path `/health/live` | Active, path `/health` |
| Readiness probe | Commented out, path `/health/ready` | Active, path `/ready` |
| TODO comment | Present (Phase 6h reference) | Removed |
| Inline comments | None | Added (restart/LB removal timing) |

### Probe Configuration (port 8081)
- Liveness: initialDelay=10s, period=10s, timeout=5s, failureThreshold=3 (restart after 30s)
- Readiness: initialDelay=5s, period=5s, timeout=3s, failureThreshold=3 (remove from LB after 15s)

---

## Files Modified

```
infra/services/mc-service/deployment.yaml | 27 +++++++++++++--------------
1 file changed, 13 insertions(+), 14 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `infra/services/mc-service/deployment.yaml` | Uncommented liveness/readiness probes, fixed paths to `/health` and `/ready`, removed Phase 6h TODO, added inline comments |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Output**: Workspace compiles clean (no Rust changes)

### Layer 2: cargo fmt
**Status**: PASS
**Output**: No formatting issues (no Rust changes)

### Layer 3: Simple Guards
**Status**: ALL PASS (14/14)

### Layer 4: Tests
**Status**: PASS
**Output**: All tests pass (no Rust changes)

### Layer 5: Clippy
**Status**: PASS
**Output**: No warnings (no Rust changes)

### Layer 6: Audit
**Status**: PASS (pre-existing)
**Output**: 3 pre-existing vulnerabilities in transitive deps (quinn-proto, ring, rsa) — not introduced by this change

### Layer 7: Semantic Guards
**Status**: PASS
| File | Verdict | Notes |
|------|---------|-------|
| `infra/services/mc-service/deployment.yaml` | SAFE | Paths match code, port correct, timing reasonable |

### Artifact-Specific: K8s Manifests
**Status**: SKIP (kubeconform not available in container)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Health port (8081) isolated from app ports. NetworkPolicy restricts 8081 to Prometheus only; kubelet probes bypass NetworkPolicy. No new attack surface.

### Test Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Paths match code routes. YAML structure and indentation correct. No unintended changes.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Existing MCPodRestartingFrequently and MCDown alerts cover probe failure scenarios. Rolling update with readiness gates ensures zero-downtime deploys.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

ADR-0012 compliant (paths, timing). Noted ADR-0023 has stale path references — code and deployment correctly follow ADR-0012 pattern.

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None
**Extraction opportunities**: None (only 2 services use this pattern; Helm/Kustomize premature)

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Rolling update strategy safe. terminationGracePeriodSeconds (35s) exceeds readiness failure window (15s). Readiness gates GC registration. Shutdown calls set_not_ready() for clean drain.

---

## Tech Debt

### Deferred Findings
No deferred findings — all reviewers CLEAR.

### Cross-Service Duplication (from DRY Reviewer)
No cross-service duplication detected.

### Temporary Code (from Code Reviewer)
No temporary code detected.

### Notes
- Operations noted pre-existing issue: MC runbook smoke tests reference port 8080 instead of 8081 (out of scope, future fix)
- Code Quality noted ADR-0023 has stale health endpoint paths (`/health/live`, `/health/ready`) that don't match the implementation (out of scope, ADR update)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `7b89d2af36e2e5a11de2d4265262aed555cce658`
2. Review all changes: `git diff 7b89d2af..HEAD`
3. Soft reset (preserves changes): `git reset --soft 7b89d2af`
4. Hard reset (clean revert): `git reset --hard 7b89d2af`
5. For infrastructure changes: may require `kubectl delete -f` if manifests were applied

---

## Reflection

All 7 teammates updated their INDEX.md files with pointers to health probe code and K8s deployment locations. Operations fixed a stale Phase 6h reference in their INDEX.

---

## Issues Encountered & Resolutions

None

---

## Lessons Learned

1. Commented-out K8s probe paths (`/health/live`, `/health/ready`) didn't match the actual code (`/health`, `/ready`) — always verify probe paths against the code, not just uncomment blindly.
2. ADR-0023 has stale health endpoint paths that diverge from the ADR-0012 convention the code follows — flagged for future ADR update.

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
./scripts/test.sh --workspace
cargo clippy --workspace -- -D warnings
cargo audit
```
