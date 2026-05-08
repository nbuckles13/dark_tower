# Devloop Output: Guard Self-Test Cleanup

**Date**: 2026-05-08
**Task**: Remove `--self-test` mode + fixtures from `validate-alert-rules.sh`, `validate-dashboard-panels.sh`, `validate-metric-labels.sh` per `docs/TODO.md:250-252` cleanup policy.
**Specialist**: infrastructure
**Mode**: Agent Teams (light — implementer + security + dry-reviewer)
**Branch**: `feature/browser-client-join-task34`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `749548629958fbb953d602d649fd295b0a7153d1` |
| Branch | `feature/browser-client-join-task34` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `infrastructure` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `CLEAR` |
| DRY | `CLEAR` (context reviewer for --light per Lead pick; appended trailing-/ TODO under Developer Experience) |

---

## Task Overview

### Objective

Remove the `--self-test` mode (function body, arg parsing, `FIXTURES_DIR` constant) from three existing fixture-equipped guards, plus the fixture trees themselves. Closes the cleanup policy validated by the 2026-05-08 Layer A scope-drift devloop (which honored the policy by NOT adding fixtures).

### Scope

- **Service(s)**: Devloop guard infrastructure (`scripts/guards/simple/`).
- **Schema**: No.
- **Cross-cutting**: Yes — guard pipeline used by every devloop. Production path (default invocation, no flags) is unaffected; `--self-test` was always opt-in.

### Light-Mode Eligibility

Eligible. Touches only `scripts/guards/simple/*.sh` (3 guards), `scripts/guards/simple/fixtures/*` (3 fixture trees), `docs/specialist-knowledge/test/INDEX.md` (one-line edit), `docs/TODO.md` (close cleanup entry). No auth/crypto/schema/proto/K8s/Docker/Cargo.toml/`crates/common/`/instrumentation paths touched. All paths Mine (infrastructure). No GSA paths.

### Reference Context

- Cleanup policy: `docs/TODO.md:250-252` (Guard Self-Test Cleanup, "new guards don't add fixtures — implementer proves correctness with ad-hoc scripts during the guard-authoring devloop, discarded before commit")
- Validating precedent: `docs/devloop-outputs/2026-05-08-layer-a-scope-drift-parser-fix/main.md` (first devloop to honor the policy — ad-hoc-then-delete workflow proven, semantic-guard SAFE, 22/22 guards pass)

---

## Cross-Boundary Classification

<!-- All rows are "Mine" (infrastructure owns guard infrastructure). No GSA paths
     (scripts/guards/simple/ is not in the ADR-0024 §6.4 enumerated list).
     test/INDEX.md is owned by infrastructure for navigation under specialist-knowledge.
     docs/TODO.md is the canonical infrastructure-tracked debt list. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/simple/validate-alert-rules.sh` | Mine | — |
| `scripts/guards/simple/validate-dashboard-panels.sh` | Mine | — |
| `scripts/guards/simple/validate-metric-labels.sh` | Mine | — |
| `scripts/guards/simple/fixtures/alert-rules/**` | Mine | — |
| `scripts/guards/simple/fixtures/dashboard-panels/**` | Mine | — |
| `scripts/guards/simple/fixtures/metric-labels/**` | Mine | — |
| `docs/specialist-knowledge/test/INDEX.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-05-08-guard-self-test-cleanup/main.md` | Mine | — |

(Fixture-tree entries listed at directory granularity. Layer A scope-drift parser handles trailing `/` and globs as of `7495486`. The implementer's diff will list individual deleted files; that's expected and not drift.)

---

## Sizing & Approach

### Sizing snapshot (from Lead pre-flight)

| Guard | Total LOC | `self_test()` body | `--self-test` refs |
|-------|-----------|---------------------|--------------------|
| validate-alert-rules.sh | 556 | 59 | 2 |
| validate-dashboard-panels.sh | 691 | 59 | 2 |
| validate-metric-labels.sh | 1078 | 59 | 2 |

| Fixture tree | Files | Size |
|--------------|-------|------|
| fixtures/alert-rules/ | 25 | 104K |
| fixtures/dashboard-panels/ | 12 | 52K |
| fixtures/metric-labels/ | 24 | 100K |

Expected diff: ~200 LOC code deletion + 61 fixture files + 1 INDEX line edit + 1 TODO entry close.

### Per-guard deletion shape (identical across all three)

For each guard:
1. Delete `self_test()` function body (~59 lines).
2. Delete `--self-test` arg parsing in `main()` (the `if [[ "${1:-}" == "--self-test" ]]; then self_test; fi` branch — typically 3-4 lines).
3. Delete `FIXTURES_DIR=` constant + its surrounding comment (~3 lines).
4. Verify production path (`main()` minus the `--self-test` branch) is unchanged.

### Illustrative-examples carve-out (Lead direction)

The `docs/TODO.md:252` entry says: *"Preserve any illustrative examples as top-of-file guard comments if still useful."*

**Default posture**: most fixtures are regression cases, not pedagogy. Delete without preservation unless a specific fixture genuinely teaches the guard's contract better than a comment could. Don't triage all 61 fixtures one-by-one. Quick scan only — if nothing jumps out as obviously illustrative, delete cleanly. Flag any kept-as-comment preservation in the Implementation Summary so DRY + security reviewers can sanity-check.

### Out-of-scope (intentionally not in this devloop)

- Removing the policy text itself from `docs/TODO.md:250-252`. The cleanup *entry* gets marked `[x]` (done); the policy paragraph at line 252 ("Policy going forward: new guards don't add fixtures") **stays** — it's a forward-looking governance rule, not retrospective tracking.
- Refactoring or extending the surviving guards' production paths.
- Updating `docs/devloop-outputs/_template/main.md:78-85` template wording (separate Tech Debt entry from prior devloop, owner = operations + code-reviewer).

---

## Pre-Work

None.

---

## Implementation Summary

### Per-guard deletions

| Guard | LOC before | LOC after | Δ | Self-test body | Main branch + comment | FIXTURES_DIR + comment | Usage line |
|-------|-----------:|----------:|--:|---------------:|----------------------:|-----------------------:|-----------:|
| validate-alert-rules.sh      | 556  | 487  | -69 | -59 (lines 436-494) | -7 (separator + main if-branch) | -2 (FIXTURES_DIR) | -1 (`--self-test` line in usage) |
| validate-dashboard-panels.sh | 691  | 622  | -69 | -59 (lines 560-618) | -7                              | -2                | -1 |
| validate-metric-labels.sh    | 1078 | 1009 | -69 | -59 (lines 963-1021)| -7                              | -2                | -1 |

Per-guard delta `-69` = 59 (`self_test()` body) + 4 (`--self-test` arg parsing in `main()`) + 3 (separator comment above `self_test()`) + 2 (FIXTURES_DIR + nothing surrounding) + 1 (usage comment). Actual exceeds the plan's ~65 estimate by ~4 lines because the separator banner above each `self_test()` was also dropped.

### Fixture trees deleted

| Tree | Files | Bytes |
|------|------:|------:|
| `scripts/guards/simple/fixtures/alert-rules/`     | 25 | 104K |
| `scripts/guards/simple/fixtures/dashboard-panels/`| 12 |  52K |
| `scripts/guards/simple/fixtures/metric-labels/`   | 24 | 100K |

Empty `scripts/guards/simple/fixtures/` parent directory also removed (no remaining children).

### Illustrative-examples disposition

**No fixtures preserved.** Quick-scanned all 61 filenames (e.g. `fail-bad-severity.yaml`, `fail-pii-compound.rs`, `pass-multiline-macro.rs`) — all are pass/fail regression cases mirroring rule-by-rule coverage. The pedagogy they encode is already documented inside each guard's Python validator comments and the surrounding observability conventions docs (`docs/observability/alert-conventions.md`, `dashboard-conventions.md`, `label-taxonomy.md`). Nothing surfaced as uniquely teaching the guard contract better than a comment could. Clean deletion.

### Doc updates

- `docs/specialist-knowledge/test/INDEX.md:7` — removed the `(--self-test, fixtures)` parenthetical and the trailing `, scripts/guards/simple/fixtures/alert-rules/` reference. Line now reads: "Validation pipeline (guards, coverage) -> `docs/decisions/adr-0024-agent-teams-workflow.md`, `scripts/guards/run-guards.sh`".
- `docs/TODO.md:256` — flipped checkbox `[ ]` → `[x]` and appended "(resolved 2026-05-08 via devloop-outputs/2026-05-08-guard-self-test-cleanup)" to the cleanup entry. The forward-looking policy paragraph at the same entry ("Policy going forward: new guards don't add fixtures...") is preserved verbatim.

### Plan-table fix during Gate 2

The Cross-Boundary Classification table originally listed fixture trees with trailing-`/` directory granularity (`scripts/guards/simple/fixtures/alert-rules/`). Layer A scope-drift's glob expander only triggers on entries containing `*`, `?`, or `[` (per `path_matches_glob` in `scripts/guards/common.sh:454-475`); a bare trailing `/` is treated as a literal and never matches the individual deleted files in the diff. First Gate 2 run flagged 25+ inbound-drift violations.

Fix: changed the three fixture-tree rows to use the supported `**` glob form (`scripts/guards/simple/fixtures/alert-rules/**` etc.). `path_matches_glob` has an explicit branch for trailing `/**` that matches `prefix/<anything>` plus `prefix` itself. Re-run cleanly: 22/22 PASS.

This is a minor plan-side correction, not a guard-code change. The `7495486` parser fix the plan referenced does support globs — the plan just needed to use one. Worth flagging upstream that Layer A's plan-format expectation is glob-or-literal, not trailing-slash-as-directory.

---

## Files Modified

`git diff --stat HEAD` summary: 66 files changed, 2 insertions(+), 1266 deletions(-).

**Modified (5)**:
- `docs/TODO.md` (checkbox flip + resolved tail)
- `docs/specialist-knowledge/test/INDEX.md` (Validation pipeline line)
- `scripts/guards/simple/validate-alert-rules.sh` (-69)
- `scripts/guards/simple/validate-dashboard-panels.sh` (-69)
- `scripts/guards/simple/validate-metric-labels.sh` (-69)

**Deleted (61 fixtures + 0 empty parent)**: 25 under `fixtures/alert-rules/`, 12 under `fixtures/dashboard-panels/`, 24 under `fixtures/metric-labels/`. Parent `scripts/guards/simple/fixtures/` directory removed via `rmdir` once empty.

**Added (1)**: `docs/devloop-outputs/2026-05-08-guard-self-test-cleanup/main.md` (this file).

---

## Devloop Verification Steps

### Layer 3: `./scripts/guards/run-guards.sh`

**Expected**: 22/22 guards pass (production path unchanged; `--self-test` was opt-in).

**Result (after plan-table fix)**: 22/22 PASS, 7.06s elapsed. All three deletion-target guards (`validate-alert-rules`, `validate-dashboard-panels`, `validate-metric-labels`) PASS — production paths intact. Layer A (`validate-cross-boundary-scope`) and Layer B (`validate-cross-boundary-classification`) both PASS once the plan globs were corrected from `fixtures/<dir>/` to `fixtures/<dir>/**`.

First-attempt failure (before plan fix): 21/22, with `validate-cross-boundary-scope` reporting 25+ scope-drift-inbound violations against individual deleted fixture files. Resolved as described in Implementation Summary §"Plan-table fix during Gate 2".

### Layer 7: semantic-guard

Layer 7 is an agent task spawned during devloop validation (per `scripts/guards/run-guards.sh:5-6` and `.claude/agents/semantic-guard.md`), not a locally invokable script. For a deletion-only diff that removes opt-in code paths and unreferenced fixture data, semantic concerns are limited. Lead/security spawn this as a separate review at Gate 3 if warranted.

### Other layers

- Layers 1, 2, 4, 5, 6, 8: N/A (no Rust changes, no proto, no kind/infra, no Cargo).

---

## Code Review Results

(To be filled in at Gate 3.)

### Security Specialist

**Verdict**: TBD

### DRY Reviewer (context reviewer for --light)

**Verdict**: TBD

---

## Tech Debt References

(To be filled in at completion. Expected: none — pure deletion of already-flagged debt.)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `7495486`.
2. Review changes: `git diff 7495486..HEAD`.
3. Soft reset: `git reset --soft 7495486`.
4. Hard reset: `git reset --hard 7495486`.
5. No schema or infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

(To be filled in as the loop progresses.)

---

## Lessons Learned

(To be filled in at completion.)
