# Devloop Output: Glob Trailing-Slash + Template Wording

**Date**: 2026-05-08
**Task**: Two follow-ups bundled — (1) auto-expand trailing-`/` to `/**` in `path_matches_glob`; (2) update devloop-output template wording at `_template/main.md:78-85` to reflect tolerated/recommended glob and parenthetical syntax.
**Specialist**: infrastructure
**Mode**: Agent Teams (light — implementer + security + code-reviewer)
**Branch**: `feature/browser-client-join-task34`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7e08f55704eb19242f94e31996d5ca7da5ce61cd` |
| Branch | `feature/browser-client-join-task34` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `infrastructure` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `CLEAR` |
| Code Quality | `CLEAR` (took voluntary nit on template canonical-form clarity) |

---

## Task Overview

### Objective

Bundle two small follow-ups from the prior two devloops on this branch:

1. **Helper enhancement**: `scripts/guards/common.sh::path_matches_glob` does not auto-expand bare trailing-`/` directory paths. Plan authors naturally write `fixtures/<name>/` for "match this whole tree", but the helper only triggers glob-expansion on entries containing `*`/`?`/`[`. Both Layer A (`validate-cross-boundary-scope.sh`) and Layer B (`validate-cross-boundary-classification.sh`) consume this helper and benefit from the change.

2. **Template wording**: `docs/devloop-outputs/_template/main.md:78-85` currently says "bare backtick-quoted filename only — no parentheticals, no globs". As of commit `7495486`, parentheticals and globs ARE tolerated; with change (1), `dir/` form also works. The wording is stale and misleading.

### Lead-committed posture for change (2)

**RECOMMEND globs and parenthetical annotations as clarity tools when they help.** Update the template prose to actively encourage `dir/**`, `*.svelte`, and `(regen)`/`(cleanup)` suffixes where they clarify intent, with a "prefer the simplest form that is accurate" balancing caveat. The implementer is not authoring the posture decision — Lead has pre-committed it. Implementer's job is to translate the posture into clean prose.

### Scope

- **Service(s)**: Devloop guard infrastructure + devloop-output template scaffolding.
- **Schema**: No.
- **Cross-cutting**: Yes — `path_matches_glob` is the cross-boundary table parser's path-matching primitive used by every devloop. Template scaffolding affects every future devloop's plan-table format.

### Light-Mode Eligibility

Eligible. Touches `scripts/guards/common.sh` (helper enhancement), `docs/devloop-outputs/_template/main.md` (prose update), `docs/TODO.md` (close two queued entries). No auth/crypto/schema/proto/K8s/Docker/Cargo.toml/`crates/common/`/instrumentation paths touched. No GSA paths (`scripts/guards/` is not in ADR-0024 §6.4).

### Reference Context

- Trailing-/ TODO entry (Developer Experience): added by @dry-reviewer in the cleanup devloop closeout, `docs/TODO.md`. Source: `docs/devloop-outputs/2026-05-08-guard-self-test-cleanup/main.md`.
- Template wording TODO entry (Documentation Hygiene): added at parser-fix devloop closeout, `docs/TODO.md`. Source: `docs/devloop-outputs/2026-05-08-layer-a-scope-drift-parser-fix/main.md`.
- Prior helper landing commit: `7495486` (Layer A scope-drift parser fix).
- Prior cleanup commit: `7e08f55` (guard self-test cleanup).

---

## Cross-Boundary Classification

<!-- All rows are "Mine" (infrastructure owns guard infrastructure + devloop-output
     scaffolding). No GSA paths (`scripts/guards/` and `docs/devloop-outputs/_template/`
     are not in the ADR-0024 §6.4 enumerated list). -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/common.sh` | Mine | — |
| `docs/devloop-outputs/_template/main.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-05-08-glob-trailing-slash-and-template-wording/main.md` | Mine | — |

---

## Implementation Sketch

### Change (1): `path_matches_glob` trailing-/ auto-expansion

```bash
path_matches_glob() {
    local path="$1" glob="$2"
    # Auto-expand trailing-/ to /** so 'dir/' matches recursively (plan-author UX).
    [[ "$glob" == */ ]] && glob="${glob%/}/**"
    # ... existing logic unchanged ...
}
```

**Why trim-then-append (`${glob%/}/**`)**: handles pathological `dir//` cleanly (becomes `dir/**`, not `dir//**`). The predicate `[[ "$glob" == */ ]]` is true for any glob ending in `/`; the trim removes the trailing `/`(s) before appending `/**`.

**Idempotence**: existing `dir/**` form remains untouched because `**` doesn't end in `/`, so the predicate is false. Verified during dev-time testing.

**No backward-compat risk**: Layer A operates on diff paths from `git diff --name-only` which never end in `/` (git lists files, not directories). Layer B's GSA manifest already uses `/**` form. Existing `dir/` plan rows currently trip the guard, so making them work is pure improvement.

### Change (2): Template wording

Replace the prose at `docs/devloop-outputs/_template/main.md:78-85` (currently a "no parentheticals, no globs" prohibition) with a "recommend where they clarify intent, prefer simplest form" posture. Approximate target text (implementer to refine for tone/fit):

> Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`, trailing `/`, `/**`) and parenthetical annotations like `` `foo.rs` (regen) `` are tolerated and recommended where they clarify intent — use `dir/**` to scope a whole tree, `*.svelte` for filename glob, and `(regen)` / `(cleanup)` suffixes for per-row context. Prefer the simplest form that is accurate; if a literal path conveys the same information, use that.

### TODO closures

- `docs/TODO.md` Developer Experience section: trailing-/ entry → `[x]` with `(resolved 2026-05-08 via devloop-outputs/2026-05-08-glob-trailing-slash-and-template-wording)`.
- `docs/TODO.md` Documentation Hygiene section: template-wording entry → same `[x]` + completion marker.

### Out-of-scope

- Extending `path_matches_glob` semantics beyond trailing-`/` (e.g., bracket-class extensions, character escapes).
- Refactoring template-main.md sections other than `:78-85` path-column-convention.
- Updating any consumer of `path_matches_glob` beyond what's in scope (the helper change is transparent — no consumer-side edits needed).

---

## Pre-Work

None.

---

## Implementation Summary

### Iteration 2 — Layer A gate-bypass fix (parser-side canonicalization)

**Why iteration 2**: iteration 1's helper-only fix was bash-correct in isolation but did nothing for actual UX. Layer A's caller (`validate-cross-boundary-scope.sh:172`) gates entry into `path_matches_glob` behind `if [[ "$plan_entry" == *[*?[]* ]]; then` — bare `dir/` plan rows fail that predicate, fall through to literal `comm` set-arithmetic, and surface every file in `dir/` as inbound-drift. Same UX failure the `docs/TODO.md:204` entry describes. Helper change was dead code from both consumers' perspective.

**Fix chosen — Option B (parser-side canonicalization)** in `parse_cross_boundary_table`. Rationale: single source of truth, both consumers (Layer A + Layer B) see canonical form, and it slots cleanly next to the existing parse-time canonicalizations (backtick-stripping at common.sh:430, parenthetical-suffix-stripping at common.sh:432). The helper's trailing-/ branch stays as defense-in-depth — the helper is now semantically complete in isolation, callers don't need to know to pre-canonicalize.

```awk
# Canonicalize trailing-/ to /** so consumers see one form (plan-author UX).
# Loop-style trim handles pathological "dir//" cleanly. Idempotent for
# paths already ending in "/**" (predicate false; no double-expansion).
if (path ~ /\/$/) {
    sub(/\/+$/, "", path)
    path = path "/**"
}
```

Note: awk `sub(/\/+$/, "", path)` greedily strips ALL trailing slashes in one pass (unlike bash `${path%/}`), so no loop needed at the awk layer. The helper's loop in bash is still necessary because bash's `${glob%/}` is single-shot.

### Change (1): `path_matches_glob` trailing-/ auto-expansion (defense-in-depth)

Final code shape (added at the top of the function body, before the literal-match line):

```bash
# Auto-expand trailing-/ to /** so 'dir/' matches recursively (plan-author UX).
# Loop strips any pathological trailing slashes ('dir//' -> 'dir/**').
if [[ "$glob" == */ ]]; then
    while [[ "$glob" == */ ]]; do glob="${glob%/}"; done
    glob="$glob/**"
fi
```

**Note on the loop vs. the sketched `${glob%/}/**` one-liner**: bash's `${glob%/}` removes only ONE trailing `/`. Verified empirically: `glob="dir//"; glob="${glob%/}/**"` yields `dir//**`, which then fails to match `dir/inner.txt` because the literal-`/`-after-prefix branch sees `prefix="dir/"` and looks for `dir//x` (two slashes) in the path. The while-loop strips ALL trailing slashes, then appends `/**` once, producing the correct `dir/**`. Same outward UX, correct edge-case handling.

### Dev-time validation (4-case matrix + bonuses)

| Case | Plan-row glob | Expected expansion | Observed expansion | Sample path | Match? | Result |
|------|----------------|---------------------|---------------------|-------------|--------|--------|
| Single-segment trailing-/ | `fixtures/foo/` | `fixtures/foo/**` | `fixtures/foo/**` | `fixtures/foo/bar.txt` | MATCH | PASS |
| Single-segment trailing-/ (negative) | `fixtures/foo/` | `fixtures/foo/**` | `fixtures/foo/**` | `fixtures/other/bar.txt` | NO-MATCH | PASS |
| Nested trailing-/ | `fixtures/foo/sub/` | `fixtures/foo/sub/**` | `fixtures/foo/sub/**` | `fixtures/foo/sub/deep/x.rs` | MATCH | PASS |
| Nested trailing-/ (sibling negative) | `fixtures/foo/sub/` | `fixtures/foo/sub/**` | `fixtures/foo/sub/**` | `fixtures/foo/other/x.rs` | NO-MATCH | PASS |
| Pathological double-slash | `fixtures/foo//` | `fixtures/foo/**` | `fixtures/foo/**` | `fixtures/foo/inner.txt` | MATCH | PASS |
| Idempotent existing /** | `fixtures/foo/**` | `fixtures/foo/**` (unchanged) | `fixtures/foo/**` | `fixtures/foo/x.rs` | MATCH | PASS |
| Bonus: literal still matches | `fixtures/foo.txt` | `fixtures/foo.txt` (unchanged) | `fixtures/foo.txt` | `fixtures/foo.txt` | MATCH | PASS |

All 7 cases PASS. Fixture script `/tmp/test_path_matches_glob.sh` deleted before commit per `docs/TODO.md:250-252` policy.

### Dev-time validation (iteration 2: end-to-end driving Layer A)

To address the Gate 2 finding that iteration 1 only tested the helper in isolation, added a 6-case end-to-end matrix that drives `check_main_md` from `validate-cross-boundary-scope.sh` directly with synthetic plan + diff fixtures (extracted via `awk` so as not to trigger the script's `main()`):

| Case | Plan-row glob | Synthetic diff | Expected | Result |
|------|----------------|-----------------|----------|--------|
| Bare `dir/` plan + diff inside dir/ | `fixtures/dir/` | `fixtures/dir/inner.txt`, `fixtures/dir/sub/deep.txt` | PASS (no drift) | PASS |
| Nested `dir/sub/` + deeper files | `fixtures/dir/sub/` | `fixtures/dir/sub/a.rs`, `fixtures/dir/sub/deep/b.rs` | PASS | PASS |
| Pathological `dir//` | `fixtures/dir//` | `fixtures/dir/inner.txt` | PASS | PASS |
| Idempotent `dir/**` | `fixtures/dir/**` | `fixtures/dir/inner.txt` | PASS (unchanged behavior) | PASS |
| Negative: `dir/` plan + outside diff | `fixtures/dir/` | `fixtures/other/x.txt` | FAIL with `scope-drift-inbound` | PASS (correctly rejected) |
| `parse_cross_boundary_table` canonicalization sanity | mixed: `single/`, `nested/sub/`, `double//`, `idem/**`, `literal.txt` | n/a | All trailing-/ → `/**`; idempotent + literal unchanged | PASS |

All 6 e2e cases PASS. Fixture script `/tmp/test_layer_a_e2e.sh` deleted before commit. The negative case (Test 5) confirms drift detection is preserved — the canonicalization doesn't accidentally make every diff path match.

### Change (2): Template wording — before/after diff snippet

**Before** (`docs/devloop-outputs/_template/main.md:78-85`):

```
     Path column convention: bare backtick-quoted filename only. The
     `validate-cross-boundary-scope` guard parser at scripts/guards/common.sh
     strips backticks but nothing else; parenthetical annotations like
     `path.rs` (new) trip scope-drift-inbound at Gate 2. Per-row file-shape
     context (new vs modify, scope qualifiers, "skeleton-only", etc.)
     belongs in § Implementation Summary or § Files Modified, not in this
     table. The table answers one question per row: whose domain is this,
     and how stringent is the involvement. -->
```

**After**:

```
     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->
```

Posture flip: prohibition (`bare ... only — no parentheticals, no globs`) → tolerated-and-recommended-where-clarifying, with a "prefer simplest form that is accurate" balancing caveat. Note that `dir/` is now equivalent to `dir/**` thanks to Change (1) and is called out explicitly.

### TODO closures

Both entries flipped `[ ]` → `[x]` with a `(resolved 2026-05-08 via devloop-outputs/2026-05-08-glob-trailing-slash-and-template-wording)` suffix appended; entries kept in place as closed historical records (not deleted).

- `docs/TODO.md:182` — Documentation Hygiene: "Devloop-output template wording — parentheticals/globs now tolerated".
- `docs/TODO.md:204` — Developer Experience: "`path_matches_glob` should auto-expand bare directory paths".

---

## Files Modified

```
 docs/TODO.md                           |  4 ++--
 docs/devloop-outputs/_template/main.md | 23 +++++++++++++++--------
 scripts/guards/common.sh               | 14 ++++++++++++++
 3 files changed, 31 insertions(+), 10 deletions(-)
```

(Iteration 2 added the parser-side canonicalization in `parse_cross_boundary_table` — 7 lines net to `common.sh`, on top of the helper's 7-line defense-in-depth block from iteration 1.)

Plus this devloop's own `main.md` (untracked at `git diff --stat HEAD` snapshot time).

---

## Devloop Verification Steps

### Layer 3: `./scripts/guards/run-guards.sh`

**Expected**: 22/22 PASS (production paths unchanged; trailing-`/` expansion is purely additive). The freshly-fixed helper dogfoods against this devloop's own plan + diff.

**Result (iteration 1)**: 22/22 PASS. Elapsed 9.39 seconds. Layer A (`validate-cross-boundary-scope`) and Layer B (`validate-cross-boundary-classification`) both green — confirms the freshly-fixed helper does not regress on this devloop's own diff (which contains no trailing-`/` rows but exercises the unchanged literal/`/**` paths).

**Result (iteration 2, after parser-side canonicalization)**: 22/22 PASS. Elapsed 6.85 seconds. Confirms the parser change does not regress any existing plan files in the repo (Layer A + Layer B re-run cleanly against this devloop's own plan, which itself uses literal paths exclusively).

### Layer 7: semantic-guard

(To be filled in at Gate 2.)

### Other layers

- Layers 1, 2, 4, 5, 6, 8: N/A (no Rust changes, no proto, no kind/infra, no Cargo).

### Dev-time validation (per `docs/TODO.md:250-252` policy)

Implementer runs ad-hoc fixture scripts under `/tmp/` covering the four cases below, deletes before commit. Records per-case PASS/FAIL+observed-result in Implementation Summary at commit time.

| Case | Plan-row glob | Expected expansion | Notes |
|------|----------------|---------------------|-------|
| Single-segment trailing-/ | `fixtures/foo/` | `fixtures/foo/**` | Most common case |
| Nested trailing-/ | `fixtures/foo/sub/` | `fixtures/foo/sub/**` | |
| Pathological double-slash | `fixtures/foo//` | `fixtures/foo/**` | Edge case; trim-then-append handles |
| Idempotent existing /** | `fixtures/foo/**` | `fixtures/foo/**` (unchanged) | Predicate false; no double-expansion |

---

## Code Review Results

(To be filled in at Gate 3.)

### Security Specialist

**Verdict**: TBD

### Code Quality Reviewer (context reviewer for --light)

**Verdict**: TBD

---

## Tech Debt References

(To be filled in at completion. Two TODO entries closed: trailing-/ Developer Experience + template wording Documentation Hygiene. No new debt expected.)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `7e08f55`.
2. Review changes: `git diff 7e08f55..HEAD`.
3. Soft reset: `git reset --soft 7e08f55`.
4. Hard reset: `git reset --hard 7e08f55`.
5. No schema/infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

(To be filled in as the loop progresses.)

---

## Lessons Learned

(To be filled in at completion.)
