# Devloop Output: Parameterize setup.sh per ADR-0030

**Date**: 2026-04-07
**Task**: Add DT_CLUSTER_NAME, DT_PORT_MAP env vars, --context kind-${CLUSTER_NAME} for kubectl, --yes flag, TTY detection, --only <service>, --skip-build to setup.sh
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) -- full
**Branch**: `feature/adr0030-setup-sh-params`
**Duration**: ~20m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `29011b3db67e4c3c6296ff8d171fb8891a85ff36` |
| Branch | `feature/adr0030-setup-sh-params` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `infrastructure` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `RESOLVED` |
| DRY | `CLEAR` |
| Operations | `RESOLVED` |

---

## Task Overview

### Objective
Parameterize infra/kind/scripts/setup.sh to support ADR-0030's host-side cluster helper. Add environment variables for cluster name and port map, context-aware kubectl, non-interactive mode, single-service rebuild, and manifest-only apply.

### Scope
- **Service(s)**: Infrastructure (setup.sh, teardown.sh)
- **Schema**: No
- **Cross-cutting**: No (shell scripts only)

### Debate Decision
NOT NEEDED - Implementation follows accepted ADR-0030 specification.

---

## Planning

Implementer proposed parameterizing setup.sh with env vars, KUBECTL helper variable, argument parsing, and helper function extractions. All 6 reviewers confirmed the plan with feedback incorporated:
- Security: validate CLUSTER_NAME as DNS label, don't blindly source DT_PORT_MAP
- DRY: extract load_image_to_kind helper (5 copies), reuse deploy functions for --only
- Operations: update teardown.sh, fix pkill pattern, check cluster existence for --only
- Test: fail on missing DT_PORT_MAP file, validate flag combinations
- Code-reviewer: strict service name validation for --only

---

## Pre-Work

None

---

## Implementation Summary

### Environment Variables
| Item | Before | After |
|------|--------|-------|
| CLUSTER_NAME | Hardcoded `"dark-tower"` | `"${DT_CLUSTER_NAME:-dark-tower}"` with DNS label validation |
| DT_PORT_MAP | N/A | Source port variables after line-by-line `^[A-Z_][A-Z0-9_]*=[0-9]+$` validation |
| kubectl context | Implicit (default) | Explicit `--context kind-${CLUSTER_NAME}` via KUBECTL helper |

### CLI Flags
| Flag | Behavior |
|------|----------|
| `--yes` | Auto-answer yes to interactive prompts |
| `--only <svc>` | Single-service rebuild+redeploy (ac, gc, mc, mh) |
| `--skip-build` | Skip image builds, apply manifests only |
| `--help` | Print usage |

### DRY Extractions
- `load_image_to_kind()`: Centralizes podman save/kind load pattern (was 5 copies)
- `deploy_only_service()`: Dispatches to existing deploy functions for --only flag

### Additional Changes
- TTY detection: auto-yes when stdin is not a TTY
- teardown.sh: reads DT_CLUSTER_NAME, updated pkill pattern
- Cluster name validation: `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$`, max 63 chars
- Port-forwards use DT_PORT_MAP variables with defaults
- print_access_info uses parameterized ports

---

## Files Modified

```
 docs/TODO.md                                      |   2 +
 docs/specialist-knowledge/code-reviewer/INDEX.md  |   2 +-
 docs/specialist-knowledge/dry-reviewer/INDEX.md   |   8 +-
 docs/specialist-knowledge/infrastructure/INDEX.md |  35 +-
 docs/specialist-knowledge/observability/INDEX.md  |  64 ++--
 docs/specialist-knowledge/operations/INDEX.md     |   6 +-
 docs/specialist-knowledge/security/INDEX.md       |  50 ++-
 docs/specialist-knowledge/semantic-guard/INDEX.md |   3 +-
 docs/specialist-knowledge/test/INDEX.md           |  10 +-
 infra/kind/scripts/setup.sh                       | 381 +++++++++++++++-------
 infra/kind/scripts/teardown.sh                    |  10 +-
```

### Key Changes by File
| File | Changes |
|------|---------|
| `infra/kind/scripts/setup.sh` | Full parameterization: env vars, KUBECTL helper, arg parsing, load_image_to_kind extraction, deploy_only_service, --skip-build gating |
| `infra/kind/scripts/teardown.sh` | DT_CLUSTER_NAME support, updated pkill pattern |
| `docs/TODO.md` | Added SKIP_BUILD and TLS secret DRY tech debt |
| `docs/specialist-knowledge/*/INDEX.md` | Updated navigation pointers for new setup.sh functions |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: 15/16 PASS (pre-existing INDEX.md size violation, not from this PR)

### Layer 4: Unit Tests
**Status**: PASS

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: 3 pre-existing dependency vulnerabilities (not from this PR)

### Layer 7: Semantic Guards
**Status**: PASS

| File | Verdict | Notes |
|------|---------|-------|
| `infra/kind/scripts/setup.sh` | SAFE | Good input validation, no injection vectors |
| `infra/kind/scripts/teardown.sh` | SAFE | Consistent parameterization |

### Artifact-Specific: Shell Syntax
**Status**: PASS (bash -n on both files)

### Artifact-Specific: Bare kubectl
**Status**: PASS (zero bare kubectl calls in function bodies)

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 3 found, 2 fixed, 1 deferred

Fixed: teardown.sh CLUSTER_NAME validation, deploy_only_service fallback case. Deferred: leading-underscore var names in port map regex (no security impact, values digit-only).

### Test Specialist
**Verdict**: CLEAR
**Findings**: 5 found, 5 fixed

Fixed: AUTO_YES defaults to non-destructive, print_access_info parameterized ports, kubectl hints with --context, main "$@" restored, DT_PORT_MAP trailing newline handling.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0

All observability kubectl calls use KUBECTL, port-forwards correctly parameterized.

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 3 found, 1 fixed, 2 deferred

Fixed: cluster name regex rejects trailing hyphens. Deferred: array-based KUBECTL (safe due to regex, 49-site change), pkill full path matching (pre-existing).

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None
**Extraction opportunities** (tech debt): SKIP_BUILD conditional pattern (4x), TLS secret creation (2x)

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed (via test reviewer's findings)

Fixed: print_access_info parameterized ports, main "$@" restored.

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification | Follow-up Task |
|---------|----------|----------|------------------------|----------------|
| Array-based KUBECTL | code-reviewer | setup.sh:66 | Cluster name regex prevents spaces, 49-site mechanical change for no functional benefit | Convert if validation changes |
| pkill full path matching | code-reviewer | setup.sh:652 | Pre-existing behavior, port-forwards die with cluster deletion | PID-file tracking cleanup |
| Leading-underscore port map vars | security | setup.sh:57 | Cosmetic, values are digit-only regardless | N/A |

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| SKIP_BUILD conditional | deploy_{ac,gc,mc,mh}_service | Same file (4 copies) | Low priority extraction |
| TLS secret creation | create_mc_tls_secret | create_mh_tls_secret | Parameterize into single function |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `29011b3db67e4c3c6296ff8d171fb8891a85ff36`
2. Review all changes: `git diff 29011b3..HEAD`
3. Soft reset (preserves changes): `git reset --soft 29011b3`
4. Hard reset (clean revert): `git reset --hard 29011b3`

---

## Reflection

All teammates updated their INDEX.md navigation files with pointers to new setup.sh functions and parameterization. DRY reviewer documented tech debt in TODO.md. INDEX files trimmed to meet 75-line limit.

---

## Issues Encountered & Resolutions

### Issue 1: Pre-commit hook requires complete main.md
**Problem**: Commit rejected because main.md had unfilled TBD sections
**Resolution**: Filled in all sections before re-committing

---

## Lessons Learned

1. Security review of DT_PORT_MAP sourcing led to robust line-by-line validation pattern
2. load_image_to_kind extraction was overdue (5 copies) -- --only flag was the ideal trigger
3. Explicit --context on all kubectl calls is more correct even for single-cluster setups

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
bash -n infra/kind/scripts/setup.sh infra/kind/scripts/teardown.sh
grep -n '^\s*kubectl ' infra/kind/scripts/setup.sh  # should be empty
```
