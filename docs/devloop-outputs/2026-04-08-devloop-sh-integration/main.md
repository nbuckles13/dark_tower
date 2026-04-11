# Devloop Output: devloop.sh integration (ADR-0030 Step 5)

**Date**: 2026-04-08
**Task**: ADR-0030 step 5 — devloop.sh changes (launch helper, bind-mount socket + kubeconfig, port map, kubectl in image)
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) - full
**Branch**: `feature/adr0030-helper-binary`
**Duration**: ~45m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `4ddb88169ed4ea1d2b707760d63734bc5e30f046` |
| Branch | `feature/adr0030-helper-binary` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-sh-integration` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@devloop-sh-integration` |
| Test | `test@devloop-sh-integration` |
| Observability | `observability@devloop-sh-integration` |
| Code Quality | `code-reviewer@devloop-sh-integration` |
| DRY | `dry-reviewer@devloop-sh-integration` |
| Operations | `operations@devloop-sh-integration` |

---

## Task Overview

### Objective
Integrate the devloop-helper binary into devloop.sh: build helper on first use, launch as background process, bind-mount socket + kubeconfig + port map into dev container, add kubectl to Dockerfile, create dev-cluster client CLI.

### Scope
- **Service(s)**: infra/devloop/ (devloop.sh, Dockerfile, dev-cluster), crates/devloop-helper/src/commands.rs
- **Schema**: No
- **Cross-cutting**: No (dev tooling only)

### Debate Decision
NOT NEEDED - ADR-0030 already accepted

---

## Planning

All 6 reviewers confirmed after addressing feedback on: jq-based JSON construction for dev-cluster (security), PID recycling guard with /proc/cmdline (security/operations), socat for unix socket communication (code-reviewer), backward compatibility gate via `command -v kind` (operations), runtime dir 0700 permissions (security), and kubeconfig server address rewriting (test/observability).

---

## Pre-Work

Step 4 (devloop-helper binary) completed on this branch.

---

## Implementation Summary

### New file: `infra/devloop/dev-cluster` (224 lines)
Shell client CLI for use inside the dev container. Reads auth token, sends JSON commands via socat, parses NDJSON streaming responses, displays post-setup URL summary.

### Modified: `infra/devloop/Dockerfile`
Added socat package, kubectl v1.32.3 (pinned, sha256 verified), COPY dev-cluster.

### Modified: `infra/devloop/devloop.sh` (+147 lines)
Helper lifecycle: build_helper(), launch_helper(), detect_orphan_clusters(), extended cleanup(). Single directory bind-mount. KUBECONFIG env var. Backward compatible via `command -v kind` gate.

### Modified: `crates/devloop-helper/src/commands.rs` (+66 lines)
generate_container_kubeconfig(): runs `kind get kubeconfig`, rewrites server address for container access, writes with 0600 permissions. Extracted rewrite_kubeconfig_server() as testable pure function with 5 unit tests.

---

## Devloop Verification Steps

### Layer 1-5: PASS
cargo check, fmt, guards (16/16), tests (99 pass), clippy clean.

### Layer 6: Skipped (pre-existing advisories only)

### Layer 7: Semantic Guard PASS

### Artifact-specific: shellcheck/hadolint unavailable in container (host testing needed)

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED
**Findings**: 3 found, 2 fixed, 1 informational
- PID recycling guard with /proc/cmdline check
- Runtime dir permissions race (mkdir -m 0700)

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 3 found, 3 fixed
- Extracted rewrite_kubeconfig_server() with 5 unit tests
- socat startup race increased to 0.5s
- PID kill identity check verified

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed
- Kubeconfig rewrite refactored for robustness
- Connection-lost error includes helper PID

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 5 found, 4 fixed, 1 deferred
- Unquoted env var expansion moved to array
- FIFO TOCTOU fixed with mktemp -d
- socat startup check increased
- Runtime dir permissions fixed
- bc dependency deferred (already in image)

### DRY Reviewer
**Verdict**: CLEAR
No duplication found. dev-cluster is a thin client delegating to helper.

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 4 found, 4 fixed
- Empty array expansion under set -u
- launch_helper failure graceful degradation
- Kubeconfig server replacement made port-agnostic
- Orphan deletion cleans port registry

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification |
|---------|----------|----------|------------------------|
| bc dependency in dev-cluster | Code Quality | dev-cluster:224 | bc already in Dockerfile, removing requires separate change |

### Cross-Service Duplication (from DRY Reviewer)

No cross-service duplication detected.

---

## Rollback Procedure

1. Verify start commit: `4ddb88169ed4ea1d2b707760d63734bc5e30f046`
2. `git diff 4ddb881..HEAD`
3. `git reset --soft 4ddb881` or `git reset --hard 4ddb881`

---

## Reflection

All 7 teammates updated INDEX.md files. INDEX guard passed after fixing a glob pattern in dry-reviewer's INDEX.

---

## Issues Encountered & Resolutions

### Issue 1: Dry-reviewer INDEX used glob pattern (again)
**Problem**: `crates/{mc,mh}-service/...` is not a valid file path
**Resolution**: Expanded to individual path

---

## Lessons Learned

1. Single directory bind-mount is simpler than individual file mounts — new files written by helper are automatically visible
2. jq -n --arg is the safe way to construct JSON in shell scripts (no interpolation)
3. PID recycling requires /proc/cmdline verification, not just kill -0
4. Kubeconfig server rewriting should be a testable pure function, not embedded in side-effectful code
