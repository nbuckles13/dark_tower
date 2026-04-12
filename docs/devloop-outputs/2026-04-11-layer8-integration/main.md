# Devloop Output: ADR-0030 Step 6 — Layer 8 Env-Test Integration

**Date**: 2026-04-11
**Task**: Add Layer 8 (env-tests against KIND cluster) to the devloop validation pipeline, plus supporting infrastructure changes
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/mc-connect-investigation`
**Duration**: ~30m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6b23c61ec8ce83c6922738092bf48d17a0af89d9` |
| Branch | `feature/mc-connect-investigation` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@layer8-integration` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@layer8-integration` |
| Test | `test@layer8-integration` |
| Observability | `observability@layer8-integration` |
| Code Quality | `code-reviewer@layer8-integration` |
| DRY | `dry-reviewer@layer8-integration` |
| Operations | `operations@layer8-integration` |

---

## Task Overview

### Objective
Implement ADR-0030 Step 6: integrate env-tests as Layer 8 in the devloop validation pipeline so integration boundary regressions are caught before human review.

### Scope
- **Service(s)**: devloop-helper (Rust), devloop.sh (shell), SKILL.md (skill definition)
- **Schema**: No
- **Cross-cutting**: Yes — affects the devloop workflow used by all specialists

### Debate Decision
NOT NEEDED — design decisions already made in conversation with user.

---

## Planning

Design decisions from pre-devloop discussion with user:

1. **Always run env-tests** — not just for infra changes; business logic can break integration tests too
2. **Run all test features** — no smoke/full split, keep it simple
3. **Rebuild all services** before env-tests — keep it simple even though slower
4. **Env-tests run inside devloop container** via `cargo test -p env-tests --features all`
5. **Eager cluster setup (Option C)**: devloop.sh kicks off `dev-cluster setup` in background at container start; at Gate 2, if `infra/kind/` files changed, tear down and rebuild; otherwise reuse
6. **Re-entrancy**: devloop.sh checks `/tmp/devloop-{slug}/ports.json` before starting setup; if cluster exists, verify health instead
7. **New `status` helper command**: lightweight read-only query for skill to check cluster readiness
8. **Infrastructure health in devloop.sh**: on re-entry, verify helper alive, KIND cluster exists, pods healthy — fix if broken
9. **ports.json only in `/tmp/devloop-{slug}/`** — drop `~/.cache` duplicate (KIND doesn't survive reboots anyway)
10. **port-registry.json stays in `~/.cache/devloop/`** — global allocation state benefits from persistence
11. **Infra change detection**: `git diff --name-only ${START_COMMIT}..HEAD -- infra/kind/` — if any files changed, tear down and rebuild cluster
12. **Split attempt budgets**: 3 for layers 1-7, 2 for Layer 8; infrastructure failures don't consume attempts

---

## Pre-Work

Previous devloops on this branch completed:
- MC/MH advertise address fix (ConfigMap patching)
- MC rejection test rewrite (full AC→GC→MC flow)
- Observability env-test hardening (all 4 services, Loki init fix)

---

## Implementation Summary

### Part 1: New `status` helper command
- Added `Status` variant to `HelperCommand` in protocol.rs with parse/display/tests
- Added `cmd_status()` in commands.rs — returns cluster existence, pod health, ports.json, setup.pid status, `checked_at` timestamp
- Extracted `parse_pod_health()` as pure testable function with 6 unit tests
- Added status command to `dev-cluster` CLI with `display_cluster_info()` extraction

### Part 2: Remove `~/.cache` ports.json duplicate
- Removed cache write in `cmd_setup()` and cache cleanup in `cmd_teardown()`
- Updated 3 INDEX.md files to correct stale path references

### Part 3: devloop.sh health check and eager setup
- Added infrastructure health check section: verifies helper PID, KIND cluster, ports.json
- Eager `dev-cluster setup` in background with output to `eager-setup.log`
- `setup.pid` written via subshell pattern (auto-cleanup on exit)
- `NEEDS_SETUP` flag deduplicates trigger logic

### Part 4: Layer 8 in SKILL.md
- Added Layer 8 to validation pipeline: cluster readiness, infra change detection, rebuild-all, env-tests, exit code classification
- Split attempt budget: 3 for layers 1-7, 2 for Layer 8
- Infrastructure flake detection (connection/timeout patterns)
- 10-minute timeout, full output capture, sub-step timing
- Deviation note documenting "always run" vs ADR-0030 trigger paths

---

## Files Modified

```
 .claude/skills/devloop/SKILL.md                    |  50 +++-
 crates/devloop-helper/src/commands.rs              | 293 ++++++++++++++++++++-
 crates/devloop-helper/src/protocol.rs              |  38 ++-
 crates/env-tests/tests/24_join_flow.rs             |  10 +-
 infra/devloop/dev-cluster                          | 160 ++++++++----
 infra/devloop/devloop.sh                           |  50 ++++
 docs/TODO.md                                       |   1 +
 docs/specialist-knowledge/{8 INDEX files}          |  ~90 +-
```

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: ALL PASS (16/16)

### Layer 4: Tests
**Status**: PASS (1170 tests, 0 failures, 8 new)

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: PASS (3 pre-existing vulnerabilities, none from this change)

### Layer 7: Semantic Guards
**Status**: SAFE

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0

### Test Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed (tightened MC rejection error code assertion)

### Observability Specialist
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed (status health display in dev-cluster)

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 3 found, 1 fixed, 2 deferred (accepted)

### DRY Reviewer
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed (eager-setup dedup with NEEDS_SETUP flag)

### Operations Reviewer
**Verdict**: RESOLVED
**Findings**: 2 found, 2 fixed (stale PID cleanup, ADR deviation note)

---

## Tech Debt

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification |
|---------|----------|----------|------------------------|
| setup.pid stale after kill -9 | Code Quality | devloop.sh, commands.rs | Mitigated by subshell cleanup + cleanup() removes runtime dir + /tmp is ephemeral. Only gap is kill -9 on subshell — edge case for dev tooling. |
| Layer 8 "always run" vs ADR-0030 triggers | Code Quality | SKILL.md:371 | Explicit design decision: running extra env-tests (2-4 min) is cheaper than missing integration regression. Documented with deviation note. |

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | Locations | Follow-up Task |
|---------|-----------|----------------|
| `kind get clusters \| grep` cluster existence | commands.rs, devloop.sh (2 places) | Not actionable — different architectural roles (Rust helper vs host-side shell) |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `6b23c61ec8ce83c6922738092bf48d17a0af89d9`
2. Review all changes: `git diff 6b23c61..HEAD`
3. Soft reset (preserves changes): `git reset --soft 6b23c61`
4. Hard reset (clean revert): `git reset --hard 6b23c61`

---

## Reflection

All teammates updated INDEX.md files with pointers for:
- `cmd_status()`, `parse_pod_health()` in commands.rs
- `display_cluster_info()` in dev-cluster
- devloop.sh health check and eager setup sections
- Layer 8 in SKILL.md
- DRY extraction opportunity logged in TODO.md

---

## Issues Encountered & Resolutions

None.

---

## Lessons Learned

1. The `status` helper command as a read-only, non-streaming query is a good pattern for health checks — always succeeds at protocol level, reports unhealthy state in data.
2. Subshell pattern `(cmd; rm -f pidfile) &` is cleaner than trap-based cleanup for background processes.
3. "Always run" for env-tests is the right default — the cost of a few extra minutes is much less than the cost of missing an integration regression.

---

## Appendix: Verification Commands

```bash
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
./scripts/test.sh --workspace
cargo clippy --workspace -- -D warnings
dev-cluster status  # verify cluster health
```
