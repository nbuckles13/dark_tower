# Devloop Output: MH→MC Network Policy + AC_JWKS_URL Config

**Date**: 2026-04-13
**Task**: Add MH→MC network policy updates and AC_JWKS_URL env var to MH configmaps/deployments
**Specialist**: infrastructure
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-infra`
**Duration**: ~25m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `799ddac4f2d6452f2cdb1b62bacc374f6e8737fa` |
| Branch | `feature/mh-quic-infra` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mh-mc-network-policy` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `RESOLVED` |
| Operations | `CLEAR` |

---

## Task Overview

### Objective
Add network policy rules for MH→MC gRPC communication (TCP 50052) and configure AC_JWKS_URL env var in MH configmaps and deployments for JWT validation via JWKS.

### Scope
- **Service(s)**: MH (network policy, configmap, deployment), MC (network policy)
- **Schema**: No
- **Cross-cutting**: Yes (network policies span MH and MC services)

### Debate Decision
NOT NEEDED - Straightforward infrastructure config per user story design.

---

## Planning

Implementer drafted approach for 6 files (2 network policies, 2 configmaps, 2 deployments). During planning, DRY and Operations reviewers independently flagged that AC_JWKS_URL should go in the shared `mh-service-config` configmap (not per-instance), matching the MC pattern. Security, observability, and code-reviewer raised the same concern. All reviewers also flagged that the URL should use the FQDN form (`ac-service.dark-tower` vs `ac-service`). Implementer accepted both adjustments, reducing the file count to 5 (shared configmap instead of 2 per-instance). All 6 reviewers confirmed the updated plan.

---

## Pre-Work

None

---

## Implementation Summary

### Network Policies (R-23, R-24)
| Item | File | Change |
|------|------|--------|
| MH egress to MC | `infra/services/mh-service/network-policy.yaml` | Added egress rule: mc-service TCP 50052 |
| MC ingress from MH | `infra/services/mc-service/network-policy.yaml` | Added ingress rule: mh-service TCP 50052 |

### MH AC_JWKS_URL Config (R-25)
| Item | File | Change |
|------|------|--------|
| Shared configmap | `infra/services/mh-service/configmap.yaml` | Added `AC_JWKS_URL: "http://ac-service.dark-tower:8082/.well-known/jwks.json"` |
| mh-0 deployment | `infra/services/mh-service/mh-0-deployment.yaml` | Added AC_JWKS_URL env var from mh-service-config |
| mh-1 deployment | `infra/services/mh-service/mh-1-deployment.yaml` | Added AC_JWKS_URL env var from mh-service-config |

---

## Files Modified

### Key Changes by File
| File | Changes |
|------|---------|
| `infra/services/mh-service/network-policy.yaml` | Added MH egress to mc-service TCP 50052 |
| `infra/services/mc-service/network-policy.yaml` | Added MC ingress from mh-service TCP 50052 |
| `infra/services/mh-service/configmap.yaml` | Added AC_JWKS_URL to shared mh-service-config |
| `infra/services/mh-service/mh-0-deployment.yaml` | Added AC_JWKS_URL env var ref from mh-service-config |
| `infra/services/mh-service/mh-1-deployment.yaml` | Added AC_JWKS_URL env var ref from mh-service-config |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS
**Duration**: ~22s

### Layer 2: cargo fmt
**Status**: PASS
**Duration**: <1s

### Layer 3: Simple Guards
**Status**: ALL PASS (15/15)
**Duration**: ~6s

### Layer 4: Tests
**Status**: PASS
**Duration**: ~60s

### Layer 5: Clippy
**Status**: PASS
**Duration**: ~6s

### Layer 6: Audit
**Status**: PASS (3 pre-existing transitive dependency findings, not introduced by this change)
**Duration**: ~10s

### Layer 7: Semantic Guards
**Status**: SAFE
**Duration**: ~10s

### Layer 8: Env-tests
**Status**: PASS (93/96; 3 pre-existing WebTransport timeout failures verified on base commit)
**Duration**: ~335s (setup 247s + tests 87s)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Network policies correctly scoped with namespace+pod selectors. JWKS URL is non-secret, HTTP OK with Linkerd mTLS. Bidirectional port separation confirmed (50052 vs 50053).

### Test Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

No env-test changes needed for infra-only task. NetworkPolicy connectivity tests will come with Rust implementation (R-31/R-33).

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

Prometheus scrape rules preserved and unaffected. No new observability configuration needed.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

ADR-0012 compliant. All patterns consistent with existing infrastructure conventions.

### DRY Reviewer
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed, 0 deferred

AC_JWKS_URL moved from per-instance configmaps to shared `mh-service-config` to match MC pattern.

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

All changes additive, no existing config modified. Rolling update safe. Rollback via git revert + kubectl apply.

---

## Tech Debt

### Deferred Findings

No deferred findings.

### Cross-Service Duplication (from DRY Reviewer)

No cross-service duplication detected.

### Temporary Code (from Code Reviewer)

No temporary code detected.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `799ddac4f2d6452f2cdb1b62bacc374f6e8737fa`
2. Review all changes: `git diff 799ddac4f2d6452f2cdb1b62bacc374f6e8737fa..HEAD`
3. Soft reset (preserves changes): `git reset --soft 799ddac4f2d6452f2cdb1b62bacc374f6e8737fa`
4. Hard reset (clean revert): `git reset --hard 799ddac4f2d6452f2cdb1b62bacc374f6e8737fa`
5. For infrastructure changes: may require `kubectl delete -f` if manifests were applied

---

## Reflection

All 7 teammates updated their INDEX.md navigation files:
- Infrastructure: updated network policy and configmap pointers
- Security: added MH JWKS config pointer, updated auth chain to include MH
- Test: added NetworkPolicy manifest pointers for canary tests
- Observability: no update needed (no new observability surface area)
- Code Quality: updated MH/MC network policy descriptions
- DRY: added JWKS config and network policy integration seam pointers
- Operations: added MH to cross-service netpol list

---

## Issues Encountered & Resolutions

None

---

## Lessons Learned

1. Shared vs per-instance configmap placement should follow existing service patterns (MC puts AC_JWKS_URL in shared configmap)
2. FQDN convention (`service.namespace`) should be consistent across all K8s service references
3. Multiple reviewers independently catching the same issue validates the review model

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
cargo test --workspace
cargo clippy --workspace --lib --bins -- -D warnings
```
