# Devloop Output: Narrow Layer 6 audit dep-change gate to TRUE dependency-manifest changes

**Date**: 2026-08-05
**Task**: Narrow the Layer 6 audit dep-change gate so ambient advisories stop failing unrelated devloops — match only true dependency manifests, drop the `packages/` / `crates/` prefix over-trigger.
**Specialist**: security (paired with infrastructure)
**Mode**: Agent Teams (v2), full mode
**Branch**: `feature/user-story-run-test`
**Duration**: ~45m (setup → commit; includes two full `layer-all.sh` runs)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `03319d70475a354ff38704cdf172cca77fa00145` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `security` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test | `test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |
| Paired Infrastructure | `paired-infrastructure` (spawned) |

---

## Task Overview

### Objective
`pnpm audit` / `cargo audit` are ambient and time-varying — the only validation layer
that flips red with zero code change when a CVE is published overnight against an
unchanged lockfile. The current Layer 6 dep-change gate over-triggers:
`audit_dep_changed_ts` / `audit_dep_changed_rust` fall back to `diff_touches_path` on
`packages/` / `crates/`, and `diff_touches_path` is prefix-only (awk `index==1`), so a
source-only diff (`packages/*/src`, `crates/*/src`) RUNS the audit and can FAIL on an
advisory unrelated to the task.

Narrow the gate to fire only on TRUE dependency-manifest changes. Safe, not masking: a
diff touching no dependency manifest provably cannot change the resolved dependency graph.
The ambient/diff-less vector remains covered by `.github/workflows/audit-scheduled.yml`
(weekly cron, `DEVLOOP_AUDIT_FORCE_RUN=1`, files a GitHub issue on drift) + the Layer-3
`audit-suppressions-check` guard.

### Scope
- **Service(s)**: none — validation pipeline tooling (`scripts/lang/`) + docs
- **Schema**: No
- **Cross-cutting**: Yes — affects the Layer 6 audit gate for both rust and ts

### Debate Decision
NOT NEEDED — design intent already exists (task #47 moved ambient coverage to the
scheduled workflow); this narrows an over-trigger within that established design.

---

## Cross-Boundary Classification

<!-- Filled by implementer at planning; Lead validates at Gate 1. -->

Implementer: security. Paired with infrastructure (`--paired-with=infrastructure`) — infrastructure
owns the `scripts/lang/` wrapper plumbing. `scripts/lang/**` is NOT a Guarded Shared Area (ADR-0024
§6.4), so no owner trailer is mechanically required, but pairing satisfies the Domain-judgment
owner-involvement tier for the gate-semantics edits below.

| File | Change | Classification | Owner | Notes |
|------|--------|----------------|-------|-------|
| `scripts/lang/_changed_helpers.sh` | Add `diff_touches_glob` predicate | Domain-judgment | infrastructure | New security-load-bearing predicate (a wrong match = wrong audit run/skip). Anchored full-line glob via bash `[[ == ]]`. Paired. |
| `scripts/lang/_changed_helpers.test.sh` | NEW direct self-test for glob predicate | Minor-judgment | infrastructure | Test plumbing. Paired. |
| `scripts/lang/_audit_gate.sh` | Rewrite `audit_dep_changed_rust`/`_ts` to manifest-only; rewrite predicate + header comments | Domain-judgment | infrastructure (path) + security (audit-gate semantics, ADR-0033 §11) | Narrows the gate; preserves tri-state fail-closed, force-run-only, suppression path. Paired. |
| `scripts/lang/_audit_gate.test.sh` | Update stale over-trigger assertions → SKIP; add glob/manifest/negative cases | Minor-judgment | infrastructure | Paired. |
| `scripts/layer3.sh` | Wire `_changed_helpers.test.sh` into the self-test block | Minor-judgment | infrastructure | One `run_and_emit` line, mirrors `audit-gate-test`. Paired. |
| `scripts/lang/rust/audit.sh` | Fix stale "crates/-source-only edit RUNS audit (over-trigger)" header comment | Mechanical | infrastructure | Comment-only; after narrowing a source-only edit SKIPS. Paired. |
| `scripts/lang/ts/audit.sh` | Fix stale "packages/-source-only edit RUNS audit (over-trigger)" header comment | Mechanical | infrastructure | Comment-only. Paired. |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Amend §3 task-#47 area: gate now matches only true dep manifests + residual | Minor-judgment | shared ADR (security co-owns audit gate) | Doc. |
| `docs/runbooks/devloop-validation.md` | Amend §6.6 DEP-CHANGE-GATED description + residual | Minor-judgment | operations/infra runbook | Doc. |
| `docs/devloop-outputs/2026-08-05-narrow-layer6-audit-dep-gate/main.md` | This output file | Mine | security | — |

**Out of the changeset (PRESERVED, do NOT touch):** `scripts/lang/rust/changed.sh`,
`scripts/lang/ts/changed.sh`, `scripts/lang/_test_changed_predicates.sh` (Layers 1/2/4/5 correctly
keep the `diff_touches_path` prefix match), the `audit_gate()` tri-state mechanism, the suppression
path (`audit_read_pnpm_suppressions`, `.cargo/audit.toml`, `_audit_suppressions_lib.sh`), and
`.github/workflows/audit-scheduled.yml`.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (hard-verify at Gate 3: diff_touches_glob reads cached diff, not fresh git diff; glob-match hygiene; SSoT-drift note) |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (non-blocking review item: verify nested fuzz-manifest match at Gate 3) |
| Semantic Guard | confirmed |
| Paired Infrastructure | confirmed |

Classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files` (exit 0).

### Design summary

The over-trigger lives in two predicates in `scripts/lang/_audit_gate.sh` that fall back to
`diff_touches_path "crates/"` / `"packages/"` — a PREFIX match (awk `index($0,p)==1`). Because
`_changed_helpers.sh` has no glob/suffix predicate, any source edit under those trees currently RUNS
`cargo audit`/`pnpm audit`, which can red on an ambient advisory unrelated to the diff. A diff that
touches no dependency manifest provably cannot change the resolved dependency graph, so narrowing to
manifest-only is SAFE, not masking — the diff-less/ambient vector stays covered by the weekly
`audit-scheduled.yml` scan + the Layer-3 suppressions-check guard (task #47 design intent).

### (1) New predicate `diff_touches_glob` in `_changed_helpers.sh`

Anchored full-line shell-glob match via bash `[[ "$line" == $glob ]]` (glob, NOT regex — distinct
from `diff_touches_path`'s fixed-string `index()` and `diff_touches_root_files`' `grep -qxF`; same
literal-vs-permissive discipline as security finding 2, made explicit in a comment). The comment will
call out that in bash glob matching `*` matches `/` too, so `crates/*/Cargo.toml` matches a
`Cargo.toml` at ANY depth under `crates/` — which is correct (any `Cargo.toml` under `crates/` is a
crate manifest). The match is full-line anchored (no surrounding wildcards), so a prefix-only share
(`crates/foo/src/x.rs` vs `crates/*/Cargo.toml`) does NOT match.

### (2) Narrowed predicates in `_audit_gate.sh`

- `audit_dep_changed_rust`: `diff_touches_root_files "Cargo.toml" "Cargo.lock"` OR
  `diff_touches_glob "crates/*/Cargo.toml"`.
- `audit_dep_changed_ts`: `diff_touches_root_files "package.json" "pnpm-lock.yaml" "pnpm-workspace.yaml"`
  OR `diff_touches_glob "packages/*/package.json"`.

Rewrite the ~lines 76–96 predicate header comments (currently describing the `crates/`/`packages/`
prefix as a deliberate fail-safe over-trigger — that rationale is now GONE) to document the
narrowed manifest-only match + the residual. **PRESERVED:** `audit_gate()` tri-state (indeterminate
diff OR base-ref-resolution failure still returns 2 → RUN), `DEVLOOP_AUDIT_FORCE_RUN` force-RUN-only,
and the entire suppression-filter path.

### Per-PR coverage completeness (recorded per @operations)

Completeness relies on **BOTH** arms, not one — belt-and-suspenders, so the boundary is not
silently reduced:
- **Glob arm (approach a):** `diff_touches_glob "crates/*/Cargo.toml"` /
  `"packages/*/package.json"` fires DIRECTLY on any per-crate/per-package manifest edit at any depth,
  independent of whether the lockfile was regenerated. This closes @operations' fail-open concern
  about a `crates/foo/Cargo.toml` dep-add landing with a stale/uncommitted lockfile — the manifest
  edit ITSELF triggers the RUN; we do not depend on lockfile regen.
- **Root-lock arm (approach b):** `diff_touches_root_files "Cargo.lock"` / `"pnpm-lock.yaml"` (+ root
  `Cargo.toml` / `package.json` / `pnpm-workspace.yaml`) catches root-level and lock changes.
The ONLY thing that stops RUNning that used to RUN: source-only edits under `crates/`/`packages/`
(`*.rs`, `*.ts`, etc.) with no manifest/lock touch — provably graph-neutral. The residual (a
dep-changing devloop still full-tree scans and may hit an unrelated ambient advisory; suppression via
`audit-suppressions.toml` is the escape hatch) is documented in ADR-0033 §3 + runbook §6.6.

### No new SKIP-forcing override (recorded per @operations)

The narrowing adds NO env var or flag that can force a SKIP. `DEVLOOP_AUDIT_FORCE_RUN` stays
force-RUN-only (unchanged). The only new SKIP path is the proven graph-neutral diff, evaluated ONLY
after `audit_gate()` resolves `_get_base_ref.sh` (indeterminate/failure → RUN, fail-closed). Fail-loud
discipline preserved.

### `*`-crosses-`/` semantics — CONSCIOUS decision (recorded per @test)

`diff_touches_glob` uses bash `[[ "$line" == $glob ]]`, where `*` matches `/` too (unlike filename
globbing). So `crates/*/Cargo.toml` matches a `Cargo.toml` at ANY depth under `crates/`, **including
the workspace-EXCLUDED fuzz manifests** `crates/ac-service/fuzz/Cargo.toml` and
`crates/media-protocol/fuzz/Cargo.toml` (verified in root `Cargo.toml` `exclude` list; no in-tree
fuzz `Cargo.lock` — only the root `Cargo.lock`).

This is the intended, security-conservative choice — and it CORRECTS @test's planning premise (1),
which assumed `*` would NOT cross `/` and the fuzz manifests would be skipped. They MATCH (→ RUN the
scan). Rationale: matching = the fail-SAFE over-trigger direction (run the cheap scan). A
fuzz-manifest change scans the unchanged root `Cargo.lock`, so it's harmless and adds no false-red
class beyond the already-documented residual. Trying to EXCLUDE nested/excluded manifests is the
fail-OPEN direction, requires distinguishing excluded-vs-included manifests the simple glob can't
express, and is exactly the "advisory-attribution set-diff" over-engineering the task ruled out of
scope. Per @test's request, the `*`-crosses-`/` self-test uses the REAL fuzz path
`crates/ac-service/fuzz/Cargo.toml` as its fixture — but asserts it **DOES match (rc 0)**, locking
the conservative property against the real repo layout.

### (1)/(2) tests

- NEW `scripts/lang/_changed_helpers.test.sh` (direct, hermetic, injected cache mirroring
  `_audit_gate.test.sh::pred_rc`) — **wired into `scripts/layer3.sh`** via its own `run_and_emit`
  line alongside `audit-gate-test` (no `*.test.sh` auto-runner). Cases:
  - match: `crates/ac-service/Cargo.toml` → 0; `packages/sdk-core/package.json` → 0;
  - `*`-crosses-`/` (REAL fuzz path): `crates/ac-service/fuzz/Cargo.toml` → 0 (matches, conservative);
  - anchoring negative (prefix shares, suffix differs): `crates/ac-service/Cargo.toml.bak` → 1;
  - source-only prefix-share: `crates/ac-service/src/lib.rs` → 1;
  - root-file must-NOT-match-glob: `Cargo.toml` → 1 (full-line anchored; root arm handles it);
  - no-match: `docs/x.md` → 1; empty cache → 1; multi-file (one matches among several) → 0.
  - anchoring hardening (per @test): `crates/Cargo.toml` (direct child, no subdir) → 1;
    `packages/crates/x/Cargo.toml` (cross-tree, start-anchored) → 1.
  - the `*`-crosses-`/` assertion carries an inline comment recording WHY it asserts MATCH (bash
    pattern-match `*` crosses `/`; fuzz manifest is workspace-excluded; matching = fail-safe
    over-trigger) — guards a future reader against "fixing" it to no-match under a filename-glob
    mental model.
- Update `scripts/lang/_audit_gate.test.sh`: the two `crate source -> run (fail-safe over-trigger)`
  (line 41) / `packages source -> run` (line 51) assertions flip to **SKIP (1)** with renamed labels;
  keep line 40 (`crates/ac-service/Cargo.toml`→0) and line 50 (`packages/proto-gen/package.json`→0)
  as the gate-fire cases; ADD nested/fuzz-manifest→run (`*` crosses `/`), root-manifest→run,
  source-only→skip, and a negative glob case. Existing force-run/indeterminate/wrapper-proof parts
  unchanged (wrapper proof copies current sources → stays consistent).

### Recorded security calls on @paired-infrastructure's two questions
- **(a) `.cargo/config.toml` NOT added to the rust root-files arm — out of scope, and no real gap.**
  A source-replacement/registry-redirect that changes which deps RESOLVE also changes `Cargo.lock`
  (the lock records the resolved graph + checksums), so the existing `Cargo.lock` root-files arm
  already catches it. A `.cargo/config.toml` edit that does NOT move the lock cannot change what
  `cargo audit` (which scans the root lock) sees. `Cargo.lock` stays the single source of truth for
  "did the resolved graph move." Pre-existing, non-regressing; not expanding surface in this task.
- **(b) `.github/workflows/audit-scheduled.yml` stays UNTOUCHED — agreed.** It force-runs via
  `DEVLOOP_AUDIT_FORCE_RUN=1`, which short-circuits `audit_gate()` to RUN before any predicate
  eval, so the predicate narrowing cannot affect it. It remains the diff-less-vector safety net.

### (3) docs + wrapper comments

ADR-0033 §3 task-#47 amendment area (narrowed-to-manifest note + residual), runbook §6.6
DEP-CHANGE-GATED paragraph, `_audit_gate.sh` top-of-file block (line 19 `diff_touches_path` →
`diff_touches_glob`), and the stale `rust/audit.sh` + `ts/audit.sh` "source-only edit RUNS audit
(over-trigger)" comments (→ after narrowing, a source-only edit SKIPS).

**Residual documented:** a dep-changing devloop still gets a FULL-TREE scan and may still hit an
unrelated ambient advisory; suppression (tasks #47/#48) is the escape hatch; `audit-scheduled.yml`
remains the diff-less-vector safety net.

### Verification matrix
- source-only edits under `packages/`/`crates/` → `STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes`;
- any true dep manifest → RUN (still FAILs on unsuppressed high/critical);
- indeterminate diff → still RUN (fail-closed preserved);
- `audit-scheduled.yml` unchanged;
- green: `_audit_gate.test.sh`, `_dispatch.test.sh`, `_test_changed_predicates.sh`, new
  `_changed_helpers.test.sh`.

---

## Implementation Summary

Narrowed the Layer 6 audit dep-change gate to true dependency manifests only.

- **New predicate `diff_touches_glob`** (`_changed_helpers.sh`): anchored full-line bash `[[ == ]]`
  glob (RHS unquoted, SC2053-waived), variadic, consuming the SAME `__changed_files` base-ref-validated
  cache as the other predicates (no internal `git diff` → cannot fail-open to an empty set). `*` crosses
  `/`, so `crates/*/Cargo.toml` matches manifests at any depth (incl. workspace-excluded `fuzz`) — the
  fail-safe RUN direction — while remaining suffix/anchor-strict.
- **Narrowed predicates** (`_audit_gate.sh`): `audit_dep_changed_rust` = root `Cargo.toml`/`Cargo.lock`
  + `diff_touches_glob "crates/*/Cargo.toml"`; `audit_dep_changed_ts` = root `package.json`/`pnpm-lock.yaml`/
  `pnpm-workspace.yaml` + `diff_touches_glob "packages/*/package.json"`. Dropped the `diff_touches_path`
  `crates/`/`packages/` PREFIX over-trigger. `audit_gate()` tri-state (force-run, indeterminate→RUN,
  base-ref-failure→RUN) and the suppression path are UNTOUCHED.
- **Tests**: new hermetic `_changed_helpers.test.sh` (15 cases: match, `*`-crosses-`/` on the REAL fuzz
  path asserting MATCH with an inline WHY-comment, anchoring negatives incl. `.bak`/direct-child/cross-tree/
  root, empty cache, multi-file) wired into `layer3.sh`. `_audit_gate.test.sh`: the two stale
  `source → run (over-trigger)` assertions flipped to `→ skip`; added nested/fuzz→run + `.bak`→skip; plus
  an **SSoT-drift guard** asserting every git-tracked `Cargo.toml`/`package.json` (via `git ls-files`) still
  fires its audit predicate — fails loudly if a future member lands outside `crates/*` / `packages/*`.
- **Docs/comments**: ADR-0033 §3 (narrowing sub-note + residual), runbook §6.6 (RUN-vs-SKIP boundary +
  residual for on-call), `_audit_gate.sh` header, and the stale `rust/audit.sh`/`ts/audit.sh`
  "source-only edit RUNS audit (over-trigger)" comments (→ now SKIPS).

**Verification (all green):**
- Self-tests: `_changed_helpers.test.sh` 15/15, `_audit_gate.test.sh` 43/43 (incl. drift guard over all 22
  tracked manifests), `_dispatch.test.sh` 53/53, `_test_changed_predicates.sh` 23/23. `bash -n` clean on all.
- End-to-end wrapper (real git diffs, PATH-stubbed cargo): source-only `crates/*/src` edit → SKIPPED-NO-DIFF
  (cargo NOT called); `crates/*/Cargo.toml` edit → RUN; root `Cargo.toml` → RUN; docs-only → SKIPPED-NO-DIFF.
- `changed.sh` classifiers, `_test_changed_predicates.sh`, and `.github/workflows/audit-scheduled.yml` untouched.

---

## Files Modified

| File | Change |
|------|--------|
| `scripts/lang/_changed_helpers.sh` | Added `diff_touches_glob` predicate (anchored bash glob; commented `*`-crosses-`/` + same-cache-source guarantees) |
| `scripts/lang/_changed_helpers.test.sh` | **NEW** — 15 hermetic direct self-tests for `diff_touches_glob` |
| `scripts/lang/_audit_gate.sh` | Narrowed `audit_dep_changed_rust`/`_ts` to manifest-only via glob; rewrote predicate + top-of-file header comments |
| `scripts/lang/_audit_gate.test.sh` | Flipped stale over-trigger assertions → skip; added nested/`.bak` cases + SSoT-drift guard |
| `scripts/layer3.sh` | Wired `changed-helpers-test` `run_and_emit` line |
| `scripts/lang/rust/audit.sh` | Fixed stale "crates/-source-only edit RUNS audit" header comment (→ now SKIPS) |
| `scripts/lang/ts/audit.sh` | Fixed stale "packages/-source-only edit RUNS audit" header comment (→ now SKIPS) |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | §3: narrowing follow-up sub-note + residual + preserved-mechanism list |
| `docs/runbooks/devloop-validation.md` | §6.6: RUN-vs-SKIP boundary + residual for on-call |
| `docs/devloop-outputs/2026-08-05-narrow-layer6-audit-dep-gate/main.md` | This output file |

---

## Devloop Verification (Gate 2)

`./scripts/layer-all.sh` — exit 0, all seven layers green (diff vs branch merge-base
`58fc6bd…`, 465 files): L1 compile OK · L2 fmt OK · L3 guards + self-tests OK (incl.
`changed-helpers-test-passed`, `audit-gate-test-passed`) · L4 test OK · L5 lint OK ·
L6 audit **ran and passed** (`cargo-audit-passed`, `pnpm-audit-passed`, `buf-breaking-passed`
— the branch diff contains dep manifests, so the narrowed gate correctly FIRED rather than
skipped) · L7 env-tests + browser-e2e OK.

---

## Code Review Results (Gate 3)

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | CLEAR | 0 | 0 | 0 |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 |
| Code Quality | CLEAR | 0 | 0 | 0 |
| DRY | CLEAR | 0 | 0 | 0 |
| Operations | CLEAR | 0 | 0 | 0 |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 |
| Paired Infrastructure | CLEAR | 0 | 0 | 0 |

Observability's single finding — the runbook `SKIPPED-NO-DIFF` legend row (§ line 83) omitted the
Layer-6 audit dep-gate producer + `no-dep-changes` REASON, a 3am-legibility regression since the
narrowing promotes that token to the common case — was **fixed in-diff**, not deferred. DRY logged
zero extraction bullets (the new matcher does not reimplement `scripts/guards/common.sh`
`path_matches_glob`). Non-blocking observation recorded (no fix required): security noted the
SSoT-drift guard is intentionally strict — a future tracked manifest OUTSIDE the workspace (e.g. a
fixture `package.json`) would trip it loudly; that is the safe/loud bias, and zero such paths exist today.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `03319d70475a354ff38704cdf172cca77fa00145`
2. Review changes: `git diff 03319d70475a354ff38704cdf172cca77fa00145..HEAD`
3. Soft reset: `git reset --soft 03319d70475a354ff38704cdf172cca77fa00145`
4. No schema/infra manifests applied — plain reset suffices.
