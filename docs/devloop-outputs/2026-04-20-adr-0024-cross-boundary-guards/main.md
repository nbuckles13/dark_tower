# Devloop Output: ADR-0024 §6.8 #1 — Cross-Boundary Guards

**Date**: 2026-04-20
**Task**: Author Layer A scope-drift guard, Layer B classification-sanity guard, ownership manifest, review-protocol Gate 1 checklist update
**Specialist**: operations
**Mode**: Agent Teams — full
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6f81642308e3ebf954853a1fe1ef610ed9f5315f` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-cross-boundary-guards` |
| Implementing Specialist | `operations` |
| Iteration | `1` |
| Security | `security@devloop-cross-boundary-guards` (RESOLVED, 4/4 fixed) |
| Test | `test@devloop-cross-boundary-guards` (CLEAR) |
| Observability | `observability@devloop-cross-boundary-guards` (CLEAR) |
| Code Quality | `code-reviewer@devloop-cross-boundary-guards` (RESOLVED, 3/3 fixed) |
| DRY | `dry-reviewer@devloop-cross-boundary-guards` (RESOLVED, 2/2 fixed) |
| Operations | `N/A (implementer is operations)` |

---

## Task Overview

### Objective

Author the two guards that enforce the ADR-0024 §6 plan-template model:
- `scripts/guards/simple/validate-cross-boundary-scope.sh` — Layer A (Gate 2, diff vs. plan file list)
- `scripts/guards/simple/validate-cross-boundary-classification.sh` — Layer B (Gate 1 primary + Gate 2 safety net; plan-only rules: GSA-not-Mechanical + GSA-paths-have-Owner)
- `scripts/guards/simple/cross-boundary-ownership.yaml` — ownership manifest mapping GSA paths to required specialists per ADR-0024 §6.4
- Update `.claude/skills/devloop/review-protocol.md` Step 0 with a Gate 1 reviewer-checklist item for the Cross-Boundary Classification table

No fixtures — correctness proven during review with ad-hoc manual tests, discarded before commit.

### Scope

- **Service(s)**: None (guard infrastructure + skill doc)
- **Schema**: No
- **Cross-cutting**: Yes — guards affect all future devloops

### Debate Decision

NOT NEEDED — this is Implementation Item #1 from ADR-0024 §6.8, already ratified by the 2026-04-18 debate. See `docs/debates/2026-04-18-devloop-cross-ownership-friction/debate.md` and ADR-0024 §6 at commit `6f81642`.

---

## Cross-Boundary Classification

<!-- Per ADR-0024 §6. Filled by implementer during planning; reviewer verifies at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/simple/validate-cross-boundary-scope.sh` | Mine | — |
| `scripts/guards/simple/validate-cross-boundary-classification.sh` | Mine | — |
| `scripts/guards/simple/cross-boundary-ownership.yaml` | Mine | — |
| `scripts/guards/common.sh` | Mine | — |
| `.claude/skills/devloop/review-protocol.md` | Not mine, Minor-judgment | code-reviewer |
| `.claude/skills/devloop/SKILL.md` | Not mine, Mechanical | — |
| `docs/decisions/adr-0024-agent-teams-workflow.md` | Not mine, Mechanical | — |
| `docs/specialist-knowledge/operations/INDEX.md` | Mine | — |
| `docs/specialist-knowledge/security/INDEX.md` | Not mine, Mechanical | security |
| `docs/specialist-knowledge/test/INDEX.md` | Not mine, Mechanical | test |
| `docs/specialist-knowledge/dry-reviewer/INDEX.md` | Not mine, Mechanical | dry-reviewer |
| `docs/TODO.md` | Mine | — |
| `docs/devloop-outputs/2026-04-20-adr-0024-cross-boundary-guards/main.md` | Mine | — |

**Rationale for classifications**:

- Guard scripts + ownership manifest live under `scripts/guards/simple/` — operations surface (guard pipeline owner per ADR-0015). **Mine**.
- `scripts/guards/common.sh` — added shared `parse_cross_boundary_table()` helper after @dry-reviewer Gate 1 review. Operations-owned shared library, same domain. **Mine**.
- `.claude/skills/devloop/review-protocol.md` is part of the devloop skill surface; edits that shape reviewer workflow sit in code-reviewer's domain (review semantics, Ownership Lens verdict structure). Edit is a narrowly-scoped checklist-item addition to Step 0 — bounded impact, not changing review semantics. **Not mine, Minor-judgment**; code-reviewer confirms at Gate 1 + Gate 3.
- This edit is NOT in a Guarded Shared Area (GSA enumeration in ADR-0024 §6.4 covers wire-format, auth/crypto, migrations — none apply here). So Minor-judgment without intersection-rule escalation is appropriate.
- `docs/specialist-knowledge/operations/INDEX.md` pointer update is Mine (operations owns this INDEX).
- Devloop `main.md` is operations-authored for this loop; Mine.
- `.claude/skills/devloop/SKILL.md` and `docs/decisions/adr-0024-agent-teams-workflow.md` — 4-way anchor-comment sync per @dry-reviewer Gate 3 finding. Change is a pointer-comment update only (from "three locations" to "four locations" naming the manifest as the 4th mirror); no substantive content change. **Not mine, Mechanical** — value-neutral and structure-preserving, covered by reviewer inspection of the anchor-sync micro-change. Neither path is a GSA.
- `docs/specialist-knowledge/{security,test,dry-reviewer}/INDEX.md` — reflection-phase pointer additions authored by the respective specialists (not by operations implementer). Each pointer adds a line referencing this devloop's output under the reviewer's own INDEX. **Not mine, Mechanical** from the implementer's plan lens (pointer-add is structure-preserving, value-neutral); owner-authored from the specialist's own lens. None is a GSA. Flagged by Layer A inbound-drift during reflection — exactly the dog-food catch the guard is designed for.
- `docs/TODO.md` — follow-up observation appended per @team-lead's reflection prompt (reflection-phase INDEX drift pattern, options a/b + operations lean toward option a). Tech-debt ledger is operations-adjacent; this is **Mine**. Not a GSA.

---

## Planning

### Approach summary

Two guards + one YAML manifest + one skill-doc edit + INDEX pointer.

### File 1: `scripts/guards/simple/cross-boundary-ownership.yaml`

Format: flat mapping from GSA path glob → list of valid specialist owners, mirroring the ADR-0024 §6.4 enumerated list exactly. Intersection case (`proto/internal.proto`) gets all three owners; Layer B checks Owner field contains at least one of the listed specialists (not all-three — per ADR-0024 §6.4 the all-three-co-sign is enforced at Gate 1 review, not by the guard — the guard just checks "Owner is a valid owner for this path").

```yaml
# Cross-Boundary Ownership Manifest (ADR-0024 §6.4)
#
# Mirror of ADR-0024 §6.4 enumerated list. Update all four locations together
# (ADR-0024 §6.4, .claude/skills/devloop/SKILL.md §Cross-Boundary Edits,
#  .claude/skills/devloop/review-protocol.md Step 0, this file)
# when extending via micro-debate.
#
# Keys are glob patterns; values list specialists who are valid Owner values.
# When a path matches multiple keys, union the specialist lists.
#
# INCOMPLETE BY DESIGN: ADR-0024 §6.4 also names
# "ADR-0027-approved crypto primitives (wherever referenced)" as a GSA.
# That rule is path-independent and CANNOT be enumerated here — call-site
# usages become GSA wherever they appear. Reviewers must apply the §6.4
# criterion manually at Gate 1 / Gate 3 (see .claude/skills/devloop/
# review-protocol.md Step 0). This manifest captures only the
# path-enumerable subset.
#
# When the Layer B guard reports "Owner ∈ manifest list" as passing, that
# is NOT a certification that all required owners co-signed — all-three
# intersection enforcement (e.g., proto/internal.proto needing protocol +
# auth-controller + security) remains Gate 1 human-review territory per
# §6.6 design rationale.

"proto/**": [protocol]
"proto-gen/**": [protocol]
"build.rs": [protocol]
"proto/internal.proto": [protocol, auth-controller, security]
"crates/media-protocol/**": [protocol, media-handler]
"crates/common/src/jwt.rs": [auth-controller, security]
"crates/common/src/meeting_token.rs": [auth-controller, security]
"crates/common/src/token_manager.rs": [auth-controller, security]
"crates/common/src/secret.rs": [auth-controller, security]
"crates/common/src/webtransport/**": [meeting-controller, protocol]
"crates/ac-service/src/jwks/**": [auth-controller, security]
"crates/ac-service/src/token/**": [auth-controller]
"crates/ac-service/src/crypto/**": [auth-controller, security]
"crates/ac-service/src/audit/**": [security]
"db/migrations/**": [database]
```

**Intersection decision**: guard uses "any listed specialist is acceptable Owner" semantics. Rationale: Layer B is a mechanical consistency check (narrow rule per §6.6); enforcing all-three-present crosses into judgment territory better done at Gate 1 review. The `proto/internal.proto` key explicitly lists protocol+auth-controller+security so reviewers have a canonical reference when reading the manifest.

**Manifest coverage fixes applied after security Gate 1 review (findings S1, S2, S4)**:
- S1: `meeting_token.rs` and `token_manager.rs` added `security` owner — §6.4 groups them with `jwt.rs`/`secret.rs` under auth/crypto primitives.
- S2: `crates/common/src/webtransport/**` added `protocol` owner — wire-runtime coupling needs protocol co-sign.
- S4: header comment now explicitly flags "ADR-0027-approved crypto primitives (wherever referenced)" as path-independent GSA that cannot be enumerated, and points reviewers to review-protocol.md Step 0 for the criterion-based fallback.

### File 2: `scripts/guards/simple/validate-cross-boundary-classification.sh` (Layer B)

Two invocation modes:
- Explicit: `validate-cross-boundary-classification.sh <path-to-main.md>` (Lead uses this at Gate 1 per SKILL.md Step 5 line 349).
- Default (from run-guards.sh): takes `$SEARCH_PATH`, scans for modified `docs/devloop-outputs/**/main.md` vs `main`.

Per-main.md logic:
1. Extract rows from the `## Cross-Boundary Classification` section. Match on section heading to avoid other tables. Ignore the template placeholder row (the one with `{path}` literal or `TBD during planning`).
2. For each row, parse: path (strip backticks), classification, owner.
3. **Rule (a) GSA-not-Mechanical**: if classification contains `Mechanical` (matches both `Mechanical` and `Not mine, Mechanical`), check path against manifest globs. If any match, fail: `GSA path cannot be Mechanical: <path>`.
4. **Rule (b) GSA-Owner-present**: if path matches a manifest glob AND classification is not `Mine`, Owner must be non-empty (not `—`, not empty), AND the Owner value must appear in the manifest's specialist list for that path (union across matching globs).

Exit 0 if no matching main.md files; exit 1 on violation with clear per-file report.

**YAML parsing approach**: no yq available in project. Use a small bash parser that reads the manifest line-by-line, matching `^"([^"]+)":\s*\[([^\]]+)\]$` and building a path→specialists map. The manifest shape is intentionally simple (flat key-value with array values, no nesting) to allow this. If anyone extends the manifest to nested structures, the parser must be rewritten or yq added as a dep. Inline to Layer B only — not shared (per @dry-reviewer, only Layer B reads the manifest).

**Glob matching approach**: bash `shopt -s extglob` + `[[ $path == $glob ]]`. Expand `/**` suffix to match any depth. Handle `build.rs` (literal) and `proto/internal.proto` (literal) alongside globs. Inline to Layer B only.

**Shared table parser in common.sh** (revised after @dry-reviewer Gate 1 review): both guards parse the `## Cross-Boundary Classification` table identically. Extract to `common.sh` as:

```bash
# Parse the ## Cross-Boundary Classification table from a main.md file.
# Output: "path|classification|owner" per row, one per line. Skips:
#   - header/separator rows
#   - rows where path matches {path} or TBD-like placeholder
# Empty output is valid (no classification rows yet).
parse_cross_boundary_table() { local main_md="$1"; ... }
```

Layer A consumes this for the plan-set; Layer B consumes this for per-row rule evaluation. Single tuple shape across both guards prevents silent parser drift when §6 evolves. Also use `get_diff_base` + `get_modified_files` from common.sh for the "find modified main.md files" work — no reinventing the diff-base resolver.

### File 3: `scripts/guards/simple/validate-cross-boundary-scope.sh` (Layer A)

Runs from run-guards.sh at Gate 2 with `$SEARCH_PATH`.

Logic:
1. Find `docs/devloop-outputs/**/main.md` files modified vs merge-base with `main`. Use `git merge-base HEAD main` then `git diff --name-only <base>..HEAD`. If merge-base resolution fails (e.g., on `main` itself), exit 0.
2. If zero main.md files match, exit 0.
3. For each main.md:
   - Parse Cross-Boundary Classification table → set of planned paths (normalize: strip backticks, skip placeholder rows).
   - Get actual diff paths: `git diff --name-only <merge-base>..HEAD`.
   - Scope drift inbound: actual-diff paths NOT in plan (excluding main.md itself, which plans its own presence).
   - Planned but untouched: plan paths NOT in diff.
4. If either set non-empty, emit report and exit 1.

**Main.md exclusion**: plan rows typically don't list main.md itself (avoids tautological self-reference). The guard must exclude main.md from the "inbound drift" comparison automatically. Same for the specialist.md reflection file if used.

**Branch detection**: if current branch is `main` or no diverging commits exist, exit 0 (nothing to check).

### File 4: `.claude/skills/devloop/review-protocol.md` — Gate 1 reviewer checklist

Add to the `## Plan Confirmation Checklist (Gate 1)` section (not Step 0 — that's reviewer scoping for code review; Gate 1 checklist is the right home for plan-review items). Add a new numbered item:

> **Cross-Boundary Classification review**: For every row in the plan's `## Cross-Boundary Classification` table with category other than `Mine`, verify the classification looks correct for the change-pattern and impact. Ensure there is a row for every file the plan touches (not only cross-boundary rows). If uncertain, challenge via **upgrade** (Mechanical → Minor-judgment → Domain-judgment) — downgrade is disallowed per ADR-0024 §6.2 and auto-routes to ESCALATE. See ADR-0024 §6.3 (owner-involvement) and §6.4 (Guarded Shared Areas).

Also add a brief note to Step 0 pointing back to the Gate 1 checklist item, so reviewers don't miss it when they start code review at Gate 3 (they may revisit classification if the diff reveals new paths).

### File 5: `docs/specialist-knowledge/operations/INDEX.md`

Add one-line pointer under "CI & Guards":
- Cross-boundary guards (Layer A scope-drift, Layer B classification-sanity) + ownership manifest → `scripts/guards/simple/validate-cross-boundary-{scope,classification}.sh`, `cross-boundary-ownership.yaml`

### Ad-hoc test strategy (discarded before commit)

No committed fixtures. During review, I'll write temporary test scripts that:
- **Layer B positive**: real main.md (this devloop's main.md) passes.
- **Layer B negative (rule a)**: hand-crafted main.md with `proto/foo.proto | Not mine, Mechanical | —` → fails with GSA-not-Mechanical error.
- **Layer B negative (rule b)**: hand-crafted main.md with `db/migrations/add_x.sql | Not mine, Minor-judgment | <empty>` → fails with missing-Owner error.
- **Layer B negative (rule b mismatch)**: hand-crafted main.md with `crates/common/src/jwt.rs | Not mine, Minor-judgment | observability` → fails with owner-not-in-manifest error.
- **Layer A positive**: branch where plan and diff agree → passes.
- **Layer A scope-drift**: modify a file not in plan → fails with inbound drift.
- **Layer A planned-untouched**: plan a file but don't modify → fails with planned-untouched.
- **Both guards**: run on a branch with no main.md modifications → exit 0 (inert).

**Additional cases added after @test Gate 1 review** (all seven confirmed):
- GSA row classified `Mine` (protocol editing own `proto/foo.proto`) → passes (classification checked before rules).
- Intersection-path Owner union: `proto/internal.proto` with `auth-controller` passes; with `database` fails.
- Template placeholder row (`{path}` or `TBD during planning`) skipped.
- Layer A on main.md-only diff → exits 0 (main.md self-excluded).
- Bare `Mechanical` (no `Not mine,` prefix) still trips rule (a).
- Trailing/inconsistent cell whitespace → parser trims before compare.
- Gate 1 explicit-path invocation works without run-guards.sh context.

**Additional cases added after @security Gate 1 review** (S1, S2 coverage):
- S1: `crates/common/src/meeting_token.rs | Not mine, Minor-judgment | auth-controller` → passes; same row with owner `code-reviewer` → fails.
- S2: `crates/common/src/webtransport/connection.rs | Not mine, Minor-judgment | meeting-controller` → passes; same row with owner `database` → fails.

Per @test: per-case pass/fail output pasted into Implementation Summary pre-commit. Test scripts deleted before commit.

### Integration points checked

- `run-guards.sh` iterates `simple/*.sh` automatically — new guards are picked up when made executable.
- `common.sh` helpers available but the path-scanning work here is main.md-specific, not the standard changed-files pattern, so minimal reuse.
- Lead's Gate 1 invocation in `.claude/skills/devloop/SKILL.md` line 349-355 already references `validate-cross-boundary-classification.sh` by name — I'm implementing the guard the doc already points to.

### Risks / open questions

- **Markdown table parsing fragility**: implementer-authored tables may have inconsistent spacing, missing trailing pipes, HTML comments inline. Mitigation: parser is permissive (splits on `|`, trims each cell) and skips rows that don't have 3 cells after trim. Will validate during ad-hoc tests against the real template.
- **Yq vs bash parser**: the manifest shape is flat enough that a ~20-line bash parser is acceptable. Adding yq as a dependency would require operations work to install it across CI + devloop containers — disproportionate to the benefit.
- **Merge-base resolution on non-devloop branches**: Layer A must handle the case where `HEAD == main` or `main` doesn't exist (e.g., worktree with unusual ref setup). Guard exits 0 gracefully in these cases.

---

## Implementation Summary

### Files created / modified

| File | Role | LOC |
|------|------|-----|
| `scripts/guards/simple/validate-cross-boundary-scope.sh` | Layer A (new) | 277 |
| `scripts/guards/simple/validate-cross-boundary-classification.sh` | Layer B (new) | 288 |
| `scripts/guards/simple/cross-boundary-ownership.yaml` | Ownership manifest (new) | 39 |
| `scripts/guards/common.sh` | `parse_cross_boundary_table()` helper (appended) | +96 |
| `.claude/skills/devloop/review-protocol.md` | Gate 1 checklist item + Step 0 pointer (modified) | +6 |
| `docs/specialist-knowledge/operations/INDEX.md` | Pointer row under CI & Guards (modified) | +0 net (consolidated into existing line) |

### Design highlights / deviations from plan

- **Start Commit scoping for Layer A** — discovered during implementation that on long-lived feature branches (like `feature/dashboard-owner-debate`, which accumulates multiple devloops), a naive `merge-base with main` diff pulls in hundreds of files from prior devloops, flooding inbound-drift. The plan's design assumption was one-devloop-per-branch. Implementation enhancement: Layer A now reads the `| Start Commit | \`<sha>\` |` row from the plan's Loop Metadata table (already in the template) and uses that as the diff base per-plan. Falls back to merge-base with main (or origin/main) when Start Commit is absent or unresolvable. Keeps the guard correct on both the narrow-branch and wide-branch cases.
- **Current-plan selection** — `find_current_main_md()` picks the newest-mtime main.md among added-since-base + untracked candidates. Handles multi-devloop branches cleanly.
- **Main.md self-exclusion symmetry** — main.md self-excluded from BOTH plan_paths and diff_paths before comparison. Prevents false planned-untouched reports on untracked plans.
- **origin/main fallback** — when local `main` ref is absent (common in worktrees), resolve against `origin/main`.

### Ad-hoc test output

Test scripts (`/tmp/xbt/*`) deleted pre-commit per brief. Output captured:

**Layer B (classification-sanity), 16 cases — all PASS:**
```
PASS case1: GSA+Mine exempt from rules (exit=0)
PASS case2a: proto/internal.proto owner=auth-controller accepted (exit=0)
PASS case2b: proto/internal.proto owner=database rejected (exit=1)
PASS case3: template placeholder row skipped (exit=0)
PASS case4: empty table → pass (exit=0)
PASS case7: bare Mechanical on GSA fails (exit=1)
PASS case8: whitespace-padded GSA Mechanical fails (exit=1)
PASS case10: Gate 1 explicit-path invocation works (exit=0)
PASS caseS1a: meeting_token.rs owner=auth-controller accepted (exit=0)
PASS caseS1b: meeting_token.rs owner=code-reviewer rejected (exit=1)
PASS caseS2a: webtransport/** owner=meeting-controller accepted (exit=0)
PASS caseS2b: webtransport/** owner=database rejected (exit=1)
PASS caseRA: 'Not mine, Mechanical' on GSA fails (exit=1)
PASS caseRBe: GSA non-Mine with '—' owner fails (exit=1)
PASS caseRBm: jwt.rs owner=observability rejected (not in manifest) (exit=1)
PASS caseNon: non-GSA Mechanical row passes (rule a only fires on GSA) (exit=0)
```

**Layer A (scope-drift), 6 cases — all PASS:**
```
PASS case5: zero main.md → inert (exit 0)
PASS case6: main.md-only diff (empty table) → exit 0
PASS caseLA: plan matches diff → exit 0
PASS caseLAin: unplanned file flagged as inbound drift
PASS caseLAun: planned file not in diff flagged as planned-untouched
PASS case11: both guards executable (run-guards.sh will pick up)
```

**Full guard pipeline** (`./scripts/guards/run-guards.sh`): 20/20 PASSED, 7.19s elapsed. Both new guards registered, no regressions to existing guards.

**Layer B against this devloop's real plan**: PASS.
**Layer A against this devloop's real diff** (Start Commit=`6f81642`): PASS.

---

## Code Review Results

TBD.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `6f81642308e3ebf954853a1fe1ef610ed9f5315f`
2. Review all changes: `git diff 6f81642..HEAD`
3. Soft reset (preserves changes): `git reset --soft 6f81642`
4. Hard reset (clean revert): `git reset --hard 6f81642`

---

## Reflection

### Dog-food catch: reflection-phase INDEX drift

Layer A flagged three reviewer-authored INDEX updates during the reflection phase (security, test, dry-reviewer each adding a pointer to this devloop's output under their own INDEX). The plan's Cross-Boundary Classification table hadn't listed these — they're a reflection-phase addition not foreseen at plan time. Resolved by patching the table mid-flight with three `Not mine, Mechanical` rows naming each specialist as Owner. Both guards green again before commit.

This is exactly the pattern Layer A is designed to catch: unplanned paths in the diff forcing the plan to stay honest. Happy with the behavior.

### Follow-up observation (not fixed here)

Reflection-phase pointer additions to reviewer INDEX files are a recurring scope-drift class — most devloops that produce a notable output will trigger reviewer INDEX updates. Two possible mitigations for a future devloop to consider (not fixed in this one):

1. **Plan-template pre-listing**: update `docs/devloop-outputs/_template/main.md` to include an optional section or reminder that reviewer INDEX pointer additions should be pre-listed in the Cross-Boundary Classification table when they're anticipated. Keeps the plan honest from the start; no guard change needed.
2. **Layer A auto-exclude**: treat `docs/specialist-knowledge/**/INDEX.md` as expected-mechanical-additions and skip them in inbound-drift comparison. Simpler operationally but weakens the "plan lists everything" invariant — and hides a real (if small) category of edits from Gate 1 review.

My lean is toward option 1 (pre-listing) over option 2 (auto-exclude): it preserves the guard's invariant that the plan enumerates every file touched, and pushes the foresight-prompting work to the plan-authoring moment. But the call belongs in a future devloop (likely owned by test or code-reviewer since it touches plan semantics). Adding to operations' tech-debt list informally rather than spawning a new devloop now.

### What worked

- **Start Commit scoping**: reading the Loop Metadata Start Commit to scope Layer A's diff base was a mid-implementation discovery that rescued correctness on long-lived feature branches. Would have shipped a broken guard without it.
- **Specialist Gate 1 round-trips**: @security (S1/S2/S4), @dry-reviewer (parser extraction + 4-way anchor), @test (7 extra edge cases), @code-reviewer (3 minor cleanups) all produced real quality wins. The multi-pass Gate 1 review caught manifest-coverage gaps (`meeting_token.rs` + `security`, `webtransport/**` + `protocol`) that pure ADR-text reading would have missed.
- **Narrow-rule guard scope**: keeping Layer B to the two mechanical rules (a/b) and leaving intersection-rule judgment with Gate 1 human review turned out right — every path that felt like "should the guard enforce this too?" led back to "judgment territory, §6.6 says human." Worth the discipline.

### What was surprising

- Guards caught their own devloop's reflection-phase drift three times over before commit. Stronger validation than any synthetic test could produce.
- Multi-devloop feature branches are less common than the spec assumes; the Start Commit scoping pattern should probably be documented for future guard authors so they don't hit the same "why is the diff 600 files" surprise.
