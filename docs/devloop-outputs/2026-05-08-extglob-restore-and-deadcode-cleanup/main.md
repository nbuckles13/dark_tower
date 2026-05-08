# Devloop Output: Extglob Restore + Dead-Code Cleanup

**Date**: 2026-05-08
**Task**: Two small hardenings to `scripts/guards/common.sh::path_matches_glob`: (1) remove dead-code trailing-`/` branch (now unreachable after parser canonicalization at `a1e80c7`); (2) save/restore `extglob` in the bash extglob fallback branch.
**Specialist**: infrastructure
**Mode**: Agent Teams (light — implementer + security + code-reviewer)
**Branch**: `feature/browser-client-join-task34`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a1e80c784397976d227e5830cb2a20fd3aa22bbe` |
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
| Code Quality | `CLEAR` |

---

## Task Overview

### Objective

Two small hardenings to `scripts/guards/common.sh::path_matches_glob`, both follow-ups from prior devloops on this branch:

1. **Remove dead-code trailing-`/` branch** in `path_matches_glob` (lines ~465-470). After parser-side canonicalization landed in `a1e80c7` (`2026-05-08-glob-trailing-slash-and-template-wording` iteration 2), the helper's own trailing-`/` expansion is unreachable: `parse_cross_boundary_table` canonicalizes `dir/` → `dir/**` before any caller sees it, and Layer B's GSA manifest already uses `/**` form. Per Lead policy preference, prefer no dead code over defense-in-depth here.

2. **Save/restore `extglob`** in `path_matches_glob`'s bash extglob fallback branch. Closes the "`shopt -s extglob` non-restoration" TODO entry under Code Quality (added at close of `2026-05-08-layer-a-scope-drift-parser-fix`). Pre-existing carry was process-local to `validate-cross-boundary-classification.sh`; after promotion to `common.sh`, the side effect became cross-script visible. Operations confirmed in the cleanup devloop that no current guard relies on `extglob` being off, so this is purely defensive hardening.

### Scope

- **Service(s)**: Devloop guard infrastructure (`scripts/guards/common.sh`).
- **Schema**: No.
- **Cross-cutting**: Yes — `path_matches_glob` is consumed by both Layer A and Layer B cross-boundary guards.

### Light-Mode Eligibility

Eligible. Touches `scripts/guards/common.sh` (sole code change) + `docs/TODO.md` (close 1 entry). No auth/crypto/schema/proto/K8s/Docker/Cargo.toml/`crates/common/`/instrumentation. No GSA paths.

### Reference Context

- Item (1) origin: `docs/devloop-outputs/2026-05-08-glob-trailing-slash-and-template-wording/main.md` Lead-flagged question on the dead-code helper branch retained as defense-in-depth.
- Item (2) origin: `docs/TODO.md` Code Quality section, "shopt -s extglob non-restoration" entry added at close of `2026-05-08-layer-a-scope-drift-parser-fix`. Operations safety survey at `docs/devloop-outputs/2026-05-08-guard-self-test-cleanup/main.md` Implementation Summary (no current guard relies on `extglob` off).

---

## Cross-Boundary Classification

<!-- All rows are "Mine" (infrastructure owns guard infrastructure). No GSA paths
     (`scripts/guards/common.sh` is not in the ADR-0024 §6.4 enumerated list). -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/common.sh` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-05-08-extglob-restore-and-deadcode-cleanup/main.md` | Mine | — |

---

## Implementation Sketch

### Change (1): remove dead-code trailing-/ branch

Delete the trailing-/ block at the top of `path_matches_glob` body (lines ~465-470):

```bash
# Auto-expand trailing-/ to /** so 'dir/' matches recursively (plan-author UX).
# Loop strips any pathological trailing slashes ('dir//' -> 'dir/**').
if [[ "$glob" == */ ]]; then
    while [[ "$glob" == */ ]]; do glob="${glob%/}"; done
    glob="$glob/**"
fi
```

This entire block becomes dead code after parser canonicalization in `parse_cross_boundary_table` (lines 432-438). All current callers route through the parser; the helper's own trailing-/ expansion is unreachable. No backstop needed — if any future caller bypasses the parser AND happens to pass a trailing-/ path, that's a bug in the caller, not the helper's responsibility to silently paper over.

### Change (2): save/restore extglob

Wrap the extglob-using fallback branch at the bottom of `path_matches_glob` with save/restore. Approximate target shape (refine for tone/correctness):

```bash
# Save extglob state, enable for the match, restore before returning.
local _prev_extglob
shopt -q extglob && _prev_extglob=on || _prev_extglob=off
shopt -s extglob

local _result
if [[ "$path" == $glob ]]; then
    _result=0
else
    _result=1
fi

[[ "$_prev_extglob" == "off" ]] && shopt -u extglob
return $_result
```

Wrap only this branch; earlier branches (literal match, trailing-/** branch) don't use extglob and shouldn't pay the save/restore cost. Implementer may pick a different shape (e.g., trap, sub-function) if it reads more cleanly — the policy is "extglob state isolated to this branch's invocation," the form is implementer's call.

### TODO closure

`docs/TODO.md` Code Quality section: "**shopt -s extglob non-restoration in path_matches_glob (cross-script visibility)**" entry. Flip `[ ]` → `[x]` with `(resolved 2026-05-08 via devloop-outputs/2026-05-08-extglob-restore-and-deadcode-cleanup)` suffix. Keep entry as historical record.

Item (1) has no TODO entry (it was a defense-in-depth choice flagged in the prior devloop's main.md as a Lead-flagged question, not tracked debt).

### Out-of-scope

- Modifying any caller of `path_matches_glob`.
- Refactoring other parts of `common.sh`.
- Changes to the parser's canonicalization (the load-bearing piece) — that's done.

---

## Pre-Work

None.

---

## Implementation Summary

### Item 1 — dead-code trailing-`/` branch removed

Deleted the 5-line block at the top of `path_matches_glob` (formerly lines 465-470). All current callers route through `parse_cross_boundary_table`, which canonicalizes `dir/` → `dir/**` at the parser layer (`common.sh:436-439`); Layer B's GSA manifest already uses `/**` form. With the parser as the sole canonicalization site, the helper-internal expansion was unreachable. The function now starts its match logic with the literal-equality check.

### Item 2 — `extglob` save/restore in fallback branch

Wrapped only the bash extglob fallback branch (the final `[[ "$path" == $glob ]]` pattern-match) with state save/restore. The literal-match branch and trailing-`/**` branch return earlier without ever invoking `shopt -s extglob`, so they pay no save/restore cost. The save/restore form mirrors the sketch in the task brief: capture prior state via `shopt -q extglob && _prev_extglob=on || _prev_extglob=off`, force-enable for the match, then `shopt -u extglob` only if the prior state was `off`. The branch is now the function's last block — a single `_result` variable carries the match outcome to the unified return.

### Final shape of `path_matches_glob`

```bash
path_matches_glob() {
    local path="$1"
    local glob="$2"

    # Literal match.
    [[ "$path" == "$glob" ]] && return 0

    # Trailing /** — match $prefix/ followed by anything.
    if [[ "$glob" == *"/**" ]]; then
        local prefix="${glob%/**}"
        [[ "$path" == "$prefix"/* ]] && return 0
        [[ "$path" == "$prefix" ]] && return 0
        return 1
    fi

    # Fall back to bash extglob matching. Save extglob state so the caller's
    # setting is preserved across the call (this helper is sourced via common.sh
    # and called from many guards; cross-script visibility was the motivating bug).
    local _prev_extglob
    shopt -q extglob && _prev_extglob=on || _prev_extglob=off
    shopt -s extglob

    local _result=1
    # shellcheck disable=SC2053
    [[ "$path" == $glob ]] && _result=0

    [[ "$_prev_extglob" == "off" ]] && shopt -u extglob
    return $_result
}
```

Net LOC change: -5 (Item 1 removed 5 lines) + 10 (Item 2 added 10 lines around the existing 2-line fallback) = +5 in `common.sh`. Per `git diff --stat`: 21 lines changed in common.sh (12+/9-), reflecting the structural rewrite of the fallback block.

### TODO entry closed

`docs/TODO.md:220` — entry "**`shopt -s extglob` non-restoration in `path_matches_glob` (cross-script visibility)**" flipped `[ ]` → `[x]` with suffix `(resolved 2026-05-08 via devloop-outputs/2026-05-08-extglob-restore-and-deadcode-cleanup)`. Body text retained as historical record per Lead policy. Item 1 had no corresponding TODO entry.

### Dev-time validation matrix results

Ad-hoc fixture script `/tmp/test_path_matches_glob.sh` (deleted before commit per `docs/TODO.md:250-252` policy). 18 sub-checks across 6 case categories — all PASS.

| Case | Setup | Expected | Observed | Result |
|------|-------|----------|----------|--------|
| Layer 3 dogfood | `./scripts/guards/run-guards.sh` against this devloop's diff | 22/22 PASS | 22/22 PASS (7.49s) | PASS |
| Extglob: off-on-off | `shopt -u extglob; path_matches_glob ...; shopt -q extglob && echo ON \|\| echo OFF` — over both `/**` and extglob-fallback callsites | OFF | OFF (both branches) | PASS |
| Extglob: on-on-on | `shopt -s extglob; path_matches_glob ...; shopt -q extglob && echo ON \|\| echo OFF` — over both `/**` and extglob-fallback callsites | ON | ON (both branches) | PASS |
| Extglob: off→call→on→call→off | Mixed sequence to verify state isolation across calls (3 calls alternating entry-state) | Each call respects entry-state | call1=OFF, call2=ON, call3=OFF | PASS |
| Helper branch coverage | Literal match + non-match; trailing-`/**` deep + prefix-as-file + non-match; extglob fallback simple-glob match/non-match; extglob fallback `*.@(bar\|baz)` match/non-match | All preserved | All 9 sub-checks correct | PASS |
| Direct trailing-`/` call | `path_matches_glob "dir/foo" "dir/"` (post Item 1 removal) and `path_matches_glob "dir/foo" "dir/**"` (canonical form still works) | bare-`dir/` → no match; `dir/**` → match | bare-`dir/` returned nomatch; `dir/**` returned match | PASS |

### Layer 3 result

```
Total guards run: 22
Passed: 22
Failed: 0
Elapsed time: 7.49 seconds
All guards passed!
```

Both Layer A (`validate-cross-boundary-scope`) and Layer B (`validate-cross-boundary-classification`) pass — production paths unaffected.

---

## Files Modified

```
 docs/TODO.md             |  2 +-
 scripts/guards/common.sh | 21 +++++++++++----------
 2 files changed, 12 insertions(+), 11 deletions(-)
```

(main.md edits not yet staged at the time of this snapshot; will appear after Gate 3.)

---

## Devloop Verification Steps

### Layer 3: `./scripts/guards/run-guards.sh`

**Expected**: 22/22 PASS (no production behavior change; dead-code removal is no-op, extglob save/restore preserves match semantics on every branch).

**Observed (implementer pre-handoff run)**: 22/22 PASS, 7.49s. Final summary line: `All guards passed!`. Lead/observability re-runs at Gate 2 are authoritative.

### Layer 7: semantic-guard

(To be filled in at Gate 2.)

### Other layers

- Layers 1, 2, 4, 5, 6, 8: N/A (no Rust changes, no proto, no kind/infra, no Cargo).

### Dev-time validation matrix

Implementer covers, deletes before commit:

| Case | Setup | Expected | Notes |
|------|-------|----------|-------|
| Layer 3 dogfood | `run-guards.sh` against this devloop's diff | 22/22 PASS | Production-path regression check |
| Extglob: off-on-off | `shopt -u extglob; path_matches_glob ... ; shopt -q extglob && echo ON || echo OFF` | OFF | Save off → enable → restore off |
| Extglob: on-on-on | `shopt -s extglob; path_matches_glob ... ; shopt -q extglob && echo ON || echo OFF` | ON | Save on → enable (idempotent) → leave on |
| Extglob: off-call-on-call-off | Mixed sequence to verify state isolation across calls | Each call respects entry-state | |
| Helper branch coverage | Literal match, trailing-/** match, extglob fallback (`*.test.ts` style) | All preserved | Dead-code removal must not regress these |
| Direct trailing-/ call | `path_matches_glob "dir/foo" "dir/"` | Returns 1 (false; bare `dir/` is not the canonical form) | Confirms removed branch's prior behavior is gone — caller is now responsible for canonical form |

---

## Code Review Results

(To be filled in at Gate 3.)

### Security Specialist

**Verdict**: TBD

### Code Quality Reviewer (context reviewer for --light)

**Verdict**: CLEAR — no findings (2026-05-08).

- **Item 1**: Trailing-`/` if-block + comment removed cleanly; no orphaned syntax. Parser-side canonicalization at `a1e80c7` confirmed to make helper-side expansion unreachable.
- **Item 2**: Save/restore form classic-correct. `_prev_extglob` capture via `shopt -q extglob && _prev_extglob=on || _prev_extglob=off` is robust regardless of `set -e` posture (common.sh isn't `set -e`'d). `_result=1` default + `&& _result=0` avoids false-match leak; no early `return` between save and restore. Restore is idempotent.
- **Locals**: Both `_prev_extglob` and `_result` `local`-declared, underscore-prefixed; no caller-namespace leak.
- **Branch isolation verified**: literal-match and trailing-`/**` branches `return` before the save block; only the extglob fallback pays the save/restore cost.
- **Comments**: explain WHY (caller-state preservation, sourced-helper cross-script visibility, motivating bug) without restating WHAT.
- **Caller audit**: both call sites (`validate-cross-boundary-classification.sh:81`, `validate-cross-boundary-scope.sh:176`) use `path_matches_glob` as an if-predicate; neither relies on extglob persisting after the call. Pure improvement, no behavioral change for current consumers.
- **TODO closure**: entry correctly moved to `[x]` with resolution-source pointer. No stranded follow-up items.

---

## Tech Debt References

(To be filled in at completion. Closes 1 TODO entry; no new debt expected.)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `a1e80c7`.
2. Review changes: `git diff a1e80c7..HEAD`.
3. Soft reset: `git reset --soft a1e80c7`.
4. Hard reset: `git reset --hard a1e80c7`.
5. No schema/infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

(To be filled in as the loop progresses.)

---

## Lessons Learned

(To be filled in at completion.)
