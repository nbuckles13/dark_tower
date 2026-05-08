# Devloop Output: Layer A Scope-Drift Parser Fix

**Date**: 2026-05-08
**Task**: Fix Layer A cross-boundary scope-drift guard parser so it tolerates `.ts/.tsx/.svelte/.proto` path syntax (parenthetical annotations + globs); add fixture self-test exercising Rust + TS path patterns.
**Specialist**: infrastructure
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task34`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `b552b91c139c96a5379f4719daec5f1217a8948e` |
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
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `CLEAR` |
| Operations | `CLEAR` |

---

## Task Overview

### Objective

Fix the cross-boundary scope-drift guard parser at `scripts/guards/simple/validate-cross-boundary-scope.sh` (and the shared `parse_cross_boundary_table` in `scripts/guards/common.sh`) so that it handles `.ts/.tsx/.svelte/.proto` path syntax conventions that tripped 2 of 3 Gate 2 attempts on the test-utils devloop. Add a fixture-based self-test under `scripts/guards/simple/fixtures/scope-drift/` exercising both Rust and TS path patterns.

### Scope

- **Service(s)**: Devloop guard infrastructure (`scripts/guards/`)
- **Schema**: No
- **Cross-cutting**: Yes — guard pipeline used by every devloop. No service code touched.

### Debate Decision

NOT NEEDED — task #34 is a self-contained Wave 1 spin-out from ADR-0033 (R-62), independent of the dispatcher refactor (#32) and TS lang dir (#33). Implementation strategy is a focused parser-bug fix with regression fixtures, not a design choice.

### Reference Context

- ADR-0033 §"Wave 1 Spin-Out Devloops" item 3 — `docs/decisions/adr-0033-polyglot-validation-pipeline.md:388`
- User story task #34 — `docs/user-stories/2026-05-02-browser-client-join.md:525`, `:727`
- Triggering Gate 2 trips — `docs/devloop-outputs/2026-05-06-test-utils-package/main.md:1031` (Issue 1, 2)
- Lesson learned that motivated this task — `docs/devloop-outputs/2026-05-06-test-utils-package/main.md:1052`

---

## Cross-Boundary Classification

<!-- All rows are "Mine" (infrastructure owns guard infrastructure). The parser
     in scripts/guards/common.sh is consumed by both validate-cross-boundary-scope.sh
     and validate-cross-boundary-classification.sh; the change is parser-side
     and affects both consumers identically. No GSA paths touched
     (scripts/guards/ is not in the ADR-0024 §6.4 enumerated list). -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/common.sh` | Mine | — |
| `scripts/guards/simple/validate-cross-boundary-scope.sh` | Mine | — |
| `scripts/guards/simple/validate-cross-boundary-classification.sh` | Mine | — |
| `docs/devloop-outputs/2026-05-08-layer-a-scope-drift-parser-fix/main.md` | Mine | — |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine | — |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mine | — |

**Gate 1 decision (Lead, 2026-05-08)**: Per `docs/TODO.md:250-252` Guard Self-Test Cleanup policy ("new guards don't add fixtures — implementer proves correctness with ad-hoc scripts during the guard-authoring devloop, discarded before commit"), this devloop does NOT commit fixtures or a `--self-test` mode. Implementer may write fixture-shaped ad-hoc test scripts during development for correctness validation; those are deleted before commit. Durable correctness signal comes from dogfooding (the freshly-fixed Layer A guard runs against THIS devloop's plan + diff at Gate 2) plus reviewer scrutiny of the parser change. The fixture matrix below is preserved as a development-time validation checklist, NOT a fixture-tree spec. Resolves @dry-reviewer Gate 1 hold + @security `fail-glob-overbroad` ask (covered by ad-hoc test during dev).

---

## Planning

### Bug analysis

Two parser/comparison limitations tripped Gate 2 on the test-utils devloop:

1. **Parenthetical annotations**: A row like ``| `pnpm-lock.yaml (regen)` | Mine | — | Notes |`` is parsed as path = `pnpm-lock.yaml (regen)` after backtick stripping. The diff has `pnpm-lock.yaml`, so `comm` reports both an inbound-drift (`pnpm-lock.yaml` not in plan) and a planned-untouched (`pnpm-lock.yaml (regen)` not in diff). Two violations from one cosmetic annotation.

2. **Glob paths**: A row like ``| `packages/test-utils/src/__tests__/*.test.ts` | Mine | — |`` is parsed as a literal string. The diff has 6 individual `*.test.ts` files. The literal string matches none of them, so the parser reports 6 inbound-drifts (each test file as "not in plan") + 1 planned-untouched (the glob row as "not in diff"). Seven violations from one legitimately-scoped row.

### Proposed fix

**(a) Parser change in `scripts/guards/common.sh::parse_cross_boundary_table`** — after backtick stripping, also strip a trailing parenthetical annotation. Minimal-diff discipline (per @code-reviewer Gate 1 input): leave the existing `gsub(/`/, "", path)` line untouched, add the new `sub()` immediately after it, single one-line comment above:

```awk
gsub(/`/, "", path)
# Strip one trailing parenthetical annotation (e.g., "foo.ts (regen)" -> "foo.ts").
sub(/[[:space:]]*\([^)]*\)[[:space:]]*$/, "", path)
```

Anchored at end-of-string only. Single-level (nested `foo (bar (baz))` would only strip the outermost — acceptable since paths-with-paren are themselves rare).

**(b) Comparison change in `scripts/guards/simple/validate-cross-boundary-scope.sh::check_main_md`** — before computing `comm`-based set differences, expand each plan path that contains a glob character (`*`, `?`, `[`) by matching it against `diff_paths` and:
- Replacing the glob in `plan_paths` with the matched diff paths (if any), so set arithmetic stays purely literal.
- If a glob matches zero diff paths, keep the literal glob in `plan_paths` so it reports as planned-untouched (correct behavior — the glob promised paths that don't exist).

**Globstar handling** (per @test Gate 1 input): `**` (any-depth) is required for legitimate patterns like `apps/web/**/*` and `proto-gen/**/*.rs`. Bash `globstar` shopt is process-global and would surprise other shell code; instead, mirror the existing `path_matches_glob` helper in `validate-cross-boundary-classification.sh:78-99` which handles `**` via explicit trailing-`/**` prefix logic + falls back to bash extglob for everything else. Move `path_matches_glob` from `validate-cross-boundary-classification.sh` to `scripts/guards/common.sh` (DRY: both Layer A and Layer B will use it). Layer B's existing call sites at `classification.sh:108` (inside `specialists_for_path`) and `:123` (`path_is_gsa`) stay byte-identical — only the definition location moves. Layer A's new glob-expansion loop calls `path_matches_glob` per (plan_glob, diff_path) pair.

**Relocation discipline** (per @code-reviewer Gate 1):
- Place under a new `## Path Matching` banner section in common.sh, immediately after the `Cross-Boundary Classification Table Parser` section (both are cross-boundary-related; parser produces input the matcher consumes).
- Move the header comment with the function. Add a "Used by:" block mirroring `parse_cross_boundary_table`'s "Consumed by:" style at common.sh:362-368, listing both classification.sh and scope.sh.
- **Preserve `# shellcheck disable=SC2053`** comment from classification.sh:96 — SC2053 ("pattern in word part of comparison should not be quoted") would re-fire on unquoted-RHS pattern match if dropped.
- **`shopt -s extglob` without restore** is pre-existing behavior in the function body. Preserve verbatim — don't fix here. **Cross-script visibility note** (per @dry-reviewer Gate 1): the side-effect was previously process-local to classification.sh; post-move via common.sh, it becomes cross-script-visible — any guard that sources common.sh AND triggers `path_matches_glob` will leave `extglob` on for the rest of that script. No current guard depends on `extglob` being off, so status-quo carry is safe today. Flag in Tech Debt References at completion as future hardening opportunity (save/restore via `shopt -q extglob` + conditional `shopt -u` post-test).
- Post-move sanity: `git diff scripts/guards/simple/validate-cross-boundary-classification.sh` should show ONLY the function deletion — no call-site changes. Verified at Gate 2.

Implementation uses bash's `[[ "$diff_path" == $glob ]]` for the extglob fallback path (`*`, `?`, `[]`, mid-path `**`), with `set -f` discipline preserved (we read into arrays explicitly so word-splitting is safe). The trailing-`/**` branch is pure string-prefix arithmetic, no shell expansion involved.

**(c) Development-time validation** (NOT a committed `--self-test` mode — per Gate 1 Lead decision aligning with `docs/TODO.md:250-252` policy). Implementer writes ad-hoc test scripts under any local-only path (e.g., `/tmp/scope-drift-fixtures/{pass,fail}-{name}/{main.md,diff.txt}`) and a one-shot driver to validate the new logic exhaustively, then deletes them before commit. The matrix below is the validation checklist the implementer should cover during development. Durable correctness signal at Gate 2 = dogfooding (the freshly-fixed Layer A guard runs against THIS devloop's plan + diff via `run-guards.sh`) + reviewer scrutiny of the parser change.

**Validation checklist** (refined post-Gate-1 reviewer input — implementer covers each case during development, then discards the test artifacts):
- `pass-rust-literal/` — baseline: pure literal Rust paths, no globs.
- `pass-ts-glob/` — glob `packages/test-utils/src/__tests__/*.test.ts` matches multiple individual TS files in diff. Includes a `.tsx` row (e.g., `apps/web/src/lib/components/Foo.tsx` literal) so `.tsx` extension has explicit coverage alongside `.ts` (same parser code path; explicit fixture prevents future regression). Per @test request.
- `pass-parenthetical-annotation/` — `pnpm-lock.yaml (regen)` parenthetical strips cleanly.
- `pass-proto-path/` — `proto/internal.proto`, `proto-gen/internal/v1/foo.rs` (proto-codegen pattern).
- `pass-svelte-path/` — `apps/web/src/lib/components/*.svelte` glob.
- `pass-empty-plan-empty-diff/` — empty Cross-Boundary table + empty `diff.txt`. Defensive: degenerate input must not blow up (e.g., `[[ == * ]]` with empty operand) and must not falsely trip. Per @test request — also exercises the case where a devloop's main.md has no table yet.
- `pass-glob-overbroad-mechanically-covers/` — plan glob `apps/web/**/*` mechanically matches `apps/web/src/lib/foo.svelte` AND `apps/web/src/secret-thing.ts`. Expected: NO trip — both diff paths are mechanically covered by the plan's glob via the extglob fallback branch in `path_matches_glob` (where `*` in `[[`-pattern matching is not path-segment-aware, so `**/*` effectively matches any depth). **Pinned design choice**: glob breadth is a Gate 1 human-judgment call (reviewers reject overbroad globs in plan review), not a guard responsibility. The guard's contract is mechanical-string-match exactness only. This fixture, paired with `fail-glob-doesnt-bless-sibling/`, fully pins the glob semantics: covered = scoped, uncovered = drift. Per @security request to pin the chosen behavior under "what if plan-author writes an overbroad glob".
- `pass-trailing-doublestar/` — plan glob `apps/web/**` matches `apps/web/src/lib/foo.svelte` AND `apps/web/src/lib/components/Bar.svelte` AND `apps/web/index.html` at varying depths. Pins the **trailing `/**` branch** in `path_matches_glob:86-92` — the only path-aware special-case in the helper, with explicit edge handling for both `$prefix/*` (any depth under) and bare `$prefix` (the prefix itself as a file). Without this branch, plain extglob would still match for most inputs but with surprising semantics on the bare-prefix edge case. Per @test request — explicit coverage for the documented contract of the helper.
- `pass-nested-glob/` — plan glob `crates/**/src/lib.rs` matches `crates/ac-service/src/lib.rs` and `crates/common/src/lib.rs` at varying depths via the **extglob fallback branch** (`path_matches_glob:94-97`). The mid-path `**` does not end in `/**` so it does NOT trigger the trailing-`/**` branch — instead it falls through to bash extglob matching, where `*` in `[[`-pattern matching is not path-segment-aware (no `globstar` shopt needed) and matches across `/` characters freely. This effectively gives `**` the desired "any depth" behavior via the fallback. Per @test request — pins the extglob fallback's depth-traversal behavior, distinct from the path-aware trailing-`/**` branch. Both branches are observable behavior; either could regress independently.
- `pass-mixed/` — combines all three patterns in one Cross-Boundary table: a literal Rust path (`crates/foo/src/main.rs`), a glob (`packages/test-utils/src/__tests__/*.test.ts`), and a parenthetical-annotated path (`pnpm-lock.yaml (regen)`). Diff has matching content. Expected: NO trip. Per @test request — pins ordering correctness (parenthetical strip must run BEFORE glob detection, otherwise `foo.ts (annotated)` would not be detected as containing `*`/glob characters). Mirrors the actual test-utils-devloop scenario that motivated this fix.
- `fail-inbound-drift/` — diff has a path not listed in plan (genuine drift, must still trip).
- `fail-planned-untouched/` — plan lists a path not in diff (genuine planned-untouched, must still trip).
- `fail-glob-zero-matches/` — plan lists `apps/web/src/lib/components/*.svelte` but `diff.txt` has zero `.svelte` files (only Rust paths). Expected: glob falls through to literal in plan_paths and trips `planned-untouched` on the literal glob string. Per @test request — pins the explicit zero-match branch in the new comparison code (distinct from the matches-multiple case in `pass-ts-glob`).
- `fail-glob-doesnt-bless-sibling/` — plan has glob `apps/web/src/lib/components/*.svelte` AND diff has `apps/web/src/lib/components/Foo.svelte` (legitimately scoped) PLUS `apps/web/src/secret-thing.ts` (NOT covered by the glob, not listed elsewhere). Expected: guard reports inbound-drift on `secret-thing.ts`. Per @security request — pins the invariant that glob expansion is a convenience for legitimately-scoped patterns, not a way to silently absorb un-disclosed inbound paths. Glob coverage is mechanical (does the path string-match the pattern?), not semantic — the guard cannot enforce "this glob is too broad" since that's a Gate 1 human judgment, but it CAN and DOES enforce "anything outside the glob's mechanical match is still inbound-drift".

The four `fail-*` cases are crucial: they prove the fix loosens the parser without disabling its primary purpose. The pair `pass-glob-overbroad-mechanically-covers/` + `fail-glob-doesnt-bless-sibling/` together fully pin the glob-breadth semantics: a glob covers what it mechanically matches, no more, no less. Glob breadth itself is Gate 1 human judgment, not the guard's call. The pair `pass-trailing-doublestar/` + `pass-nested-glob/` exercises both branches of `path_matches_glob` (the path-aware trailing-`/**` branch and the extglob fallback) so a regression to either is caught. Total: 10 pass + 4 fail (development-time only; not committed).

**Implementation shape** — `check_main_md` keeps its existing signature; the new glob-aware logic is added inline before the `comm -23`/`comm -13` set arithmetic. No `compare_plan_vs_diff` extraction is needed without committed fixtures, since there is no second caller. Implementer may still factor a small helper internally if it makes the diff cleaner, at their discretion. `find_active_main_md` and `resolve_scope` stay untouched.

**Inline-logic documentation** (per @code-reviewer Gate 1 re-confirm): the inline glob-expansion block carries a 2-3 line comment block explaining intent, substituting for the helper's self-documenting name. Suggested wording:

```bash
# Expand plan globs against diff paths so the comm-based set arithmetic stays
# purely literal: a glob that matches >=1 diff path is replaced by those paths;
# a glob matching zero paths stays literal so it surfaces as planned-untouched.
```

Not a docstring; just enough context that a future reader doesn't reverse-engineer the loop's purpose.

**`run-guards.sh` compatibility**: No new flags surfaced. Layer 3 keeps invoking the script in its default mode against the active devloop's diff/plan exactly as today.

### Out-of-scope (intentionally not in this devloop)

- Behavioral changes to `validate-cross-boundary-classification.sh` itself: code-side NONE. The only edit there is removing the local `path_matches_glob` definition (lines 78-99) — its body moves verbatim to `scripts/guards/common.sh` so Layer A can reuse it (per Gate 1 reviewer input on globstar handling). Layer B's call sites are unchanged: it still calls `path_matches_glob "$path" "$glob"` with identical semantics. **Layer B observable-effect note (per @operations Gate 1)**: the parenthetical strip in the shared parser tightens Layer B's GSA detection as a side effect: a row like ``| `proto/internal.proto (cleanup)` | Not mine, Mechanical | platform |`` would previously parse path = `proto/internal.proto (cleanup)` and fail to match any manifest glob, so Rule (a) wouldn't trip. After the fix, path normalizes to `proto/internal.proto`, matches the manifest, and Rule (a) correctly trips on the Mechanical classification of a GSA. This is net-positive (Layer B was leaking GSAs through cosmetic annotations) — flag it in the Implementation Summary at commit time so the next devloop using parentheticals isn't surprised. Layer 3 dogfooding plus the existing classification guard's behavior on this devloop's plan is the regression check.
- Template wording in `docs/devloop-outputs/_template/main.md:78-85` ("bare backtick-quoted filename only — no parentheticals, no globs"). With this fix, parentheticals and globs *are* tolerated. The wording update is a one-line edit but the discussion ("should we *recommend* parentheticals/globs or just *tolerate* them?") is a separate decision. Per @code-reviewer Gate 1 input: spin out as a follow-up devloop, flag in this devloop's "Tech Debt References" at completion. Do not bundle.
- Wave 1 items #32, #33 (dispatcher refactor, ts lang dir). They are independent per ADR-0033.

### Validation expectations

- Layer 1 (`cargo check --workspace`): N/A — no Rust changes. Will pass trivially.
- Layer 2 (`cargo fmt`): N/A — no Rust changes.
- Layer 3 (`./scripts/guards/run-guards.sh`): RUNS — must pass, including the freshly-fixed Layer A guard against this very devloop's plan + diff (dogfooding).
- Layer 4 (`./scripts/test.sh --workspace`): N/A — no Rust changes. Skip per Layer 4 N/A pattern documented in the test-utils devloop.
- Layer 5 (`cargo clippy`): N/A — no Rust changes.
- Layer 6 (`cargo audit`): N/A — no Cargo dependency changes.
- Layer 7 (semantic-guard): RUNS against the diff.
- Layer 8 (env-tests): N/A — no `infra/kind/**`, no service code, no proto change.
- Artifact-specific: `shellcheck` on the modified shell scripts (Layer 3 pre-existing trigger).
- No fixture self-test committed (Gate 1 decision per `docs/TODO.md:250-252` policy). Implementer's ad-hoc dev-time validation results are summarized in Implementation Summary at commit time as a per-case PASS/FAIL table (case name | outcome | violation count) covering all 14 cases in the validation checklist above. This table IS the test report in lieu of committed fixtures (per @test Gate 1 input — single line per case sufficient for Gate 3 reviewer audit).

---

## Pre-Work

None.

---

## Implementation Summary

Three production-code changes per the Gate 1 plan, plus the two commit-time tracking updates:

1. **Parser change** in `scripts/guards/common.sh::parse_cross_boundary_table` — added `sub(/[[:space:]]*\([^)]*\)[[:space:]]*$/, "", path)` immediately after the existing backtick-strip `gsub(/`/, "", path)`. One-line comment above. End-of-string anchored, single-level. Minimal-diff per @code-reviewer Gate 1.

2. **Comparison change** in `scripts/guards/simple/validate-cross-boundary-scope.sh::check_main_md` — added inline glob-expansion loop (24 LOC) between `diff_paths` derivation and the `comm` set arithmetic. For each plan path containing `*`/`?`/`[`, matches against `diff_paths` via `path_matches_glob`; replaces with matched paths if ≥1, keeps literal glob if zero matches (correct planned-untouched behavior). 3-line intent-comment above the block per @code-reviewer Gate 1.

3. **`path_matches_glob` relocation** from `validate-cross-boundary-classification.sh:78-99` to `scripts/guards/common.sh` under a new `## Path Matching` banner section, immediately after `Cross-Boundary Classification Table Parser`. Function body verbatim — `# shellcheck disable=SC2053` preserved (per @code-reviewer); `shopt -s extglob` non-restoration preserved (logged in Tech Debt References). Header comment moved with the function, with a `Used by:` block listing both Layer A and Layer B mirroring the existing `parse_cross_boundary_table` "Consumed by:" style. `git diff scripts/guards/simple/validate-cross-boundary-classification.sh` shows ONLY the function deletion — no call-site changes (verified at `classification.sh:108` `specialists_for_path` and `:123` `path_is_gsa`).

**Layer B observable-effect (per @operations Gate 1, surfaced for future implementer awareness)**: the parenthetical-strip in the shared parser tightens Layer B's GSA detection. A row like ``| `proto/internal.proto (cleanup)` | Not mine, Mechanical | platform |`` previously parsed path = `proto/internal.proto (cleanup)`, failed manifest match, and skipped Rule (a). Post-fix, path normalizes to `proto/internal.proto`, matches the manifest, and Rule (a) correctly trips on Mechanical-classification of a GSA. Net-positive — Layer B was leaking GSAs through cosmetic annotations. Worth knowing if your next devloop uses parenthetical annotations on GSA-adjacent paths.

**Tracking updates** (commit-time, per task brief):
- `docs/decisions/adr-0033-polyglot-validation-pipeline.md:358` — Wave 1 status row 8 (Layer A scope-drift parser fix) status flipped from ❌ Pending to ✅ Done with 2026-05-08 date and devloop-output reference.
- `docs/user-stories/2026-05-02-browser-client-join.md:727` — Devloop Tracking row #34: Devloop Output column populated, Status flipped from Pending to Completed.

### Dev-time validation results (per @test Gate 1 — fixtures NOT committed)

All 14 cases in the validation checklist (lines 124-137 of this main.md) covered via ad-hoc fixtures under `/tmp/scope-drift-fixtures/` + a throwaway `run.sh` driver that sources `common.sh` and runs an inlined copy of `check_main_md` against each fixture's `main.md` + `diff.txt`. Per Gate 1 decision, those artifacts are not committed.

| Case | Expected | Actual | Violations |
|------|----------|--------|------------|
| pass-empty-plan-empty-diff | PASS | PASS | 0 |
| pass-glob-overbroad-mechanically-covers | PASS | PASS | 0 |
| pass-mixed | PASS | PASS | 0 |
| pass-nested-glob | PASS | PASS | 0 |
| pass-parenthetical-annotation | PASS | PASS | 0 |
| pass-proto-path | PASS | PASS | 0 |
| pass-rust-literal | PASS | PASS | 0 |
| pass-svelte-path | PASS | PASS | 0 |
| pass-trailing-doublestar | PASS | PASS | 0 |
| pass-ts-glob | PASS | PASS | 0 |
| fail-glob-doesnt-bless-sibling | FAIL | FAIL | 1 |
| fail-glob-zero-matches | FAIL | FAIL | 2 |
| fail-inbound-drift | FAIL | FAIL | 1 |
| fail-planned-untouched | FAIL | FAIL | 1 |

14/14 PASS. Notes:
- `fail-glob-zero-matches` reports 2 violations: 1 inbound-drift (the unrelated diff path) + 1 planned-untouched (the literal glob string surfaces because it matched zero). Both correct.
- `fail-glob-doesnt-bless-sibling` reports 1 inbound-drift on `apps/web/src/secret-thing.ts` — pins the security-critical "mechanically uncovered = drift" invariant per @security Gate 1 ask.

---

## Files Modified

```
 docs/decisions/adr-0033-polyglot-validation-pipeline.md       |  2 +-
 docs/user-stories/2026-05-02-browser-client-join.md           |  2 +-
 scripts/guards/common.sh                                      | 40 ++++++++++++++++++++++
 scripts/guards/simple/validate-cross-boundary-classification.sh | 27 ---------------
 scripts/guards/simple/validate-cross-boundary-scope.sh        | 25 ++++++++++++++
 5 files changed, 67 insertions(+), 29 deletions(-)
```

Plus this main.md (untracked at start of devloop, will be added at commit time).

---

## Devloop Verification Steps

| Layer | Check | Status |
|-------|-------|--------|
| 1 | `cargo check --workspace` | N/A — no Rust changes |
| 2 | `cargo fmt` | N/A — no Rust changes |
| 3 | `./scripts/guards/run-guards.sh` | RUN at Gate 2; includes Layer A + Layer B dogfooding against this devloop's plan + diff |
| 4 | `./scripts/test.sh --workspace` | N/A — no Rust changes |
| 5 | `cargo clippy` | N/A — no Rust changes |
| 6 | `cargo audit` | N/A — no Cargo dependency changes |
| 7 | semantic-guard | RUN at Gate 2 against the diff |
| 8 | env-tests | N/A — no `infra/kind/**`, no service code, no proto change |
| Artifact | `shellcheck` on modified `.sh` files | RUN at Gate 2 (shellcheck not available locally; relies on Layer 3 trigger or CI) |
| Dogfood | Production scope guard against this devloop's pending diff | ✅ PASS (no scope drift after tracking updates landed) |
| Dogfood | Production classification guard against this devloop's plan | ✅ PASS (no classification violations) |
| Custom | 14-case ad-hoc validation harness | ✅ 14/14 PASS (results table above) |
| Cleanup | `/tmp/scope-drift-fixtures/` ad-hoc artifacts | DELETED before commit |

---

## Code Review Results

(To be filled in during Gate 3.)

---

## Tech Debt References

1. **`shopt -s extglob` non-restoration in `path_matches_glob`** (post-relocation, in `scripts/guards/common.sh` `## Path Matching` section). Pre-existing behavior preserved verbatim per @code-reviewer Gate 1. Cross-script visibility note (@dry-reviewer Gate 1): the side-effect was previously process-local to classification.sh; post-move it becomes cross-script-visible — any guard that sources `common.sh` AND triggers `path_matches_glob` will leave `extglob` on for the rest of that script. No current guard depends on `extglob` being off (the four other guards using `shopt` only touch `nullglob`). Future hardening: save/restore via `shopt -q extglob` + conditional `shopt -u` post-test. Single-line fix when a future guard author hits it.

2. **Devloop-output template wording at `docs/devloop-outputs/_template/main.md:78-85`** ("bare backtick-quoted filename only — no parentheticals, no globs"). With this devloop's parser fix, parentheticals AND globs are now tolerated. The wording update itself is a one-line edit, but the broader question ("should we *recommend* parentheticals/globs or just *tolerate* them?") is a separate decision. Per @code-reviewer + @operations Gate 1 input: spin out as a follow-up devloop, do not bundle here.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `b552b91c139c96a5379f4719daec5f1217a8948e`
2. Review changes: `git diff b552b91..HEAD`
3. Soft reset (preserves changes): `git reset --soft b552b91`
4. Hard reset (clean revert): `git reset --hard b552b91`
5. No schema or infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

(To be filled in as the loop progresses.)

---

## Lessons Learned

(To be filled in at completion.)
