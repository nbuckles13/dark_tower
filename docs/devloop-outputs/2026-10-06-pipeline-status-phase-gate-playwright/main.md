# Devloop Output: Pipeline fixes — green total OK, Phase gate, Playwright SSoT

**Date**: 2026-10-06
**Task**: (A) N/A ranks below OK so a green run totals OK; (B) pre-commit refuses devloop main.md at Phase≠complete staged with non-devloop-output changes; (C) devloop image Playwright version derived from pnpm-lock.yaml, bounded+retried browser download
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present; --paired-with=security
**Branch**: `feature/6-pipeline-status-and-phase-gate`
**Duration**: ~1h50m (setup 20:30 → commit ~22:20 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d8faf23b684a7182a63610eb56671514cabb784f` |
| Branch | `feature/6-pipeline-status-and-phase-gate` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `paired-security` (--paired-with) |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned` — diff is pipeline shell/Dockerfile/docs; no checks.md surface (no credential types, Debug impls, client credential lifetime) |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
Three pipeline fixes (verbatim task statement):

**(A) A green run must total OK, not N/A.** N/A ranks above OK in the status ladder (`__status_rank` in scripts/lang/_common.sh), and the only N/A emitters are proto's intentional-gap placeholders (scripts/lang/proto/test.sh, scripts/lang/proto/audit.sh) and _dispatch.sh's no-languages-registered case. So Layers 4 and 6, and TOTAL_RESULT, read N/A on every fully green run, and a clean pass never says OK. ADR-0033 already avoided this for dependency audits (SKIPPED-NO-DIFF instead of N/A, because N/A 'would mask a passing layer'). Change: rank N/A just below OK (with the SKIPPED-* statuses), so N/A beside a real OK aggregates to OK, and a layer where nothing ran still reads N/A. Exit codes are unchanged (N/A already maps to 0). FAIL, PRECONDITION_FAILURE and FAIL-MISSING-VERB still outrank OK.
- Amend ADR-0033: the N/A ranking statements (§175 and the placeholder/§STATUS aggregation text), the worst-child order, and the status table.
- Update expected totals in _common.test.sh, layer-all.test.sh, run-story.test.sh and anything else that pins N/A-dominates.
- Check the CI aggregator added by the ci-speed devloop (`layer-all.sh --aggregate`) and any parser of TOTAL_RESULT.
- Update docs that explain a green N/A total (SKILL.md, runbooks).

**(B) The Gate-2 hook must not be skippable by leaving main.md's Phase unset.** The pre-commit hook (`.githooks/pre-commit` -> scripts/lang/_gate2_binding.sh) requires a Gate-2 verdict only when a staged devloop main.md is at Phase=complete. The ci-speed devloop committed its whole change with the Loop State Phase still `setup`, so the hook checked no verdict (Gate 2 had in fact passed; the record was fixed by amend). Change: refuse a commit that stages a devloop docs/devloop-outputs/<slug>/main.md whose Phase is not `complete` together with any other change outside docs/devloop-outputs/, with a message telling the Lead to set Phase to `complete` (which then requires the verdict). A commit staging only devloop-outputs files is not blocked. Keep the replay skip (cherry-pick/rebase) behaviour unchanged. Decide and test how the new refusal interacts with replays of historical commits whose main.md was never at `complete`.
- Tests in scripts/guards/simple/selftest-gate2-verdict.sh, including the ci-speed shape (Phase setup + code changes) blocked, a docs-only main.md edit allowed, and a complete main.md path unchanged.
- Update docs/runbooks/devloop-validation.md §8.5 (failure-shapes table).
- Check whether SKILL.md Step 8 should state 'set Phase to complete before committing' more prominently.

**(C) The devloop image's Playwright version comes from the lockfile, and its browser download cannot hang.** (infra/devloop/Dockerfile, `ARG PLAYWRIGHT_VERSION` + `npx playwright@... install chromium`):
- Single source of truth: the version is hand-typed in the Dockerfile ARG default and in packages/web-app and packages/sdk-svelte package.json, with no guard; it drifted before (16fb5e6) and Dependabot npm bumps will drift it again. Derive it from pnpm-lock.yaml through one reader, the way SQLX_CLI_VERSION and RUST_VERSION are (devloop.sh passes a default-less build arg; the build fails loudly without it). Fail loudly if the lockfile resolves more than one playwright version. Add a test for the reader and for the two package.json pins agreeing.
- Hang: on 2026-10-06 the step hung ~40 min after the Chromium download, its CDN connections open but idle (no timeout in Playwright's downloader). Bound it (`timeout`) and retry a small fixed number of times, then fail loudly with a message naming the step.
- The "running npx playwright install without first installing your project's dependencies" warning is expected (the image has no repo at build time); a short comment saying so is enough.

This is the Gate-2 authority control: security must review (B). Run with `--paired-with=security`.

### Scope
- **Service(s)**: none (validation pipeline scripts, pre-commit hook, devloop image, ADR-0033, runbooks/SKILL.md)
- **Schema**: No
- **Cross-cutting**: Yes (pipeline used by every devloop / CI)

### Debate Decision
NOT NEEDED - task statement fixes the decisions; ADR-0033 amended in place.


---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

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

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/_common.sh` | Mine | — |
| `scripts/lang/_common.test.sh` | Mine | — |
| `scripts/lang/_dispatch.sh` | Mine | — |
| `scripts/lang/_dispatch.test.sh` | Mine | — |
| `scripts/lang/_wrapper_trap.test.sh` (comment) | Mine | — |
| `scripts/lang/rust/audit.sh` (comment) | Mine | — |
| `scripts/lang/proto/test.sh` (comment) | Mine | — |
| `scripts/lang/proto/audit.sh` (comment) | Mine | — |
| `scripts/audit.sh` (comment) | Mine | — |
| `scripts/layer-all.sh` (comment) | Mine | — |
| `scripts/layer-all.test.sh` | Mine | — |
| `scripts/layer3.sh` (wire `_dispatch`/`_wrapper_trap`/`_get_base_ref`/`_layer_skeleton` unit tests) | Mine | — |
| `scripts/guards/run-guards.sh` (comment) | Mine | — |
| `scripts/workflow/run-story.test.sh` (comment) | Mine | — |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mine | — |
| `scripts/lang/_gate2_binding.sh` | Not mine, Domain-judgment (Gate-2 authority policy; --paired-with=security) | security |
| `scripts/guards/simple/selftest-gate2-verdict.sh` | Not mine, Domain-judgment (tests of the security control; paired) | security |
| `.githooks/pre-commit` (comment only) | Mine | — |
| `infra/lib/pnpm-lock-version.sh` (new) | Mine | — |
| `infra/devloop/playwright-install.sh` (new) | Mine | — |
| `infra/devloop/devloop.sh` | Mine | — |
| `infra/devloop/Dockerfile` | Mine | — |
| `scripts/setup.test.sh` | Mine | — |
| `docs/runbooks/devloop-validation.md` (§3/§6/§7/§8 N/A text; §8.5 Phase refusal + residuals) | Not mine, Minor-judgment | operations |
| `.claude/skills/devloop/SKILL.md` | Not mine, Minor-judgment | operations |
| `docs/TODO.md` (close the `TOTAL_RESULT=N/A` entry; add npx-integrity entry) | Not mine, Minor-judgment | observability, security |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `package.json` (nx 23.1.1 → 23.1.3) | Not mine, Minor-judgment | client |
| `pnpm-workspace.yaml` (source-map-js override) | Not mine, Minor-judgment | client |
| `pnpm-lock.yaml` (regen) | Not mine, Minor-judgment | client |

---

## Planning

(Revision 3: folds in all Gate-1 input and the Lead rulings, including the Lead's CORRECTION of ruling 3 — N/A sits just below OK, above the SKIPPED-* statuses, per the task text.)

### Mechanism restatement (not instance)

- **(A)** "An exit-0 status that says nothing about this run's real work must never outrank a real `OK`, and a wiring fault must never hide among them." `SKIPPED-NO-DIFF`/`SKIPPED-NO-CLUSTER` already obey the first half; `N/A` is the one exit-0 enum that does not. The second half has one instance: `_dispatch.sh` `no-languages-registered` (an empty/misdirected lang root) is emitted as `N/A` — after the re-rank it would fold silently into a green total. Mirrors of the ladder: every place it is *restated* in prose; several rank-number comments are already stale (FAIL-MISSING-VERB "rank 5" — real 7).
- **(B)** "No main.md field the committer controls may decide whether the Gate-2 verdict is required." Phase is that knob. A second instance found by @paired-security: git rename detection hides a bound file's deletion from the binding (`--name-only` lists only the destination) — fixed with `--no-renames` on every enumeration (Lead ruling 1).
- **(C)** "Every image pin is derived from the repo SSoT through one reader; no Dockerfile default." `PLAYWRIGHT_VERSION` is the last literal pin with an in-repo SSoT. `KUBECTL_VERSION=v1.32.3` has no in-repo SSoT — different class, not touched.

### (A) N/A just below OK, above the SKIPPED-* statuses (task text; Lead ruling 3 as corrected)

1. `__status_rank`: **`SKIPPED-NO-VERB 0 < SKIPPED-NO-DIFF 1 < SKIPPED-NO-CLUSTER 2 < N/A 3 < OK 4`** `< FAIL 5 < PRECONDITION_FAILURE 6 < FAIL-MISSING-VERB 7 < UNKNOWN 8`, `*)` 8. Only OK and N/A swap; everything from FAIL up is unchanged, so `--aggregate`'s `rank ≥ rank(UNKNOWN)` unknown-status check (layer-all.sh:130) and the NOT_RUN fail-closed backstop are unaffected. `status_to_exit_code` is **not touched** — the change is display-only for every exit code (N/A was and stays 0).
   - Consequences: N/A beside OK → OK. Layer 6 on a no-dep-change run (rust/ts `SKIPPED-NO-DIFF` + proto `N/A`) → **`N/A`** (`audit-aggregate-na`) — "a layer where nothing ran still reads N/A"; the per-run dep-gate fact is in the rust/ts child `STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes` lines. `TOTAL_RESULT` is still OK.
   - `aggregate_worst_status` with zero args → OK: unchanged, pre-existing, noted.
2. `no-languages-registered` → **`FAIL-MISSING-VERB REASON=no-languages-registered`**, exit 2 (Lead ruling 2; @observability/@paired-security/@operations; wiring-fault precedent). Checked: it fires on the directory listing before the INCLUDE/EXCLUDE filter, and no caller, fixture or fast lane dispatches against an empty lang root expecting 0 (only producer `_dispatch.sh:84`; no test pins it). The filter-to-zero path is unchanged: `SKIPPED-NO-VERB all-langs-filtered`, exit 0 (operator intent).
3. **N/A emitter allow-list test** (`_dispatch.test.sh`; Lead-approved): scans `scripts/**/*.sh` (excluding `*.test.sh`) for every spelling — `emit_status N/A`, `emit_status "N/A"`, `emit_status 'N/A'`, `STATUS=N/A`, `STATUS="N/A"` — with a fixture tree proving each spelling is caught, and asserts the set is **exactly** `{lang/proto/test.sh, lang/proto/audit.sh}` plus the `_dispatch.sh` `${verb}-aggregate-na` passthrough arm. Positive control: it must find both proto placeholders (distinct failure token `n/a-allowlist-scan-vacuous` vs `n/a-unexpected-emitter`).
4. Consumers (checked, all "none" beyond the shared helper): `layer-all.sh` `compute_layer_totals` + `aggregate_shard_summaries` both use `aggregate_worst_status`/`__status_rank`, no second ladder; `run-story.sh`, `scripts/workflow/devloop-stop-hook.sh`, `crates/dt-story`, `.github/workflows/*.yml`: no `N/A`/`TOTAL_RESULT=` parse in a gate position (run-story keys on exit codes); `run-guards.sh:335` ANCHOR mirrors only FAIL/PRECONDITION order (unchanged; rank numbers removed).
5. Tests: `_common.test.sh` — full pairwise ladder: each SKIPPED-* < N/A < OK; OK+N/A→OK; N/A alone→N/A; the L6 triple `SKIPPED-NO-DIFF SKIPPED-NO-DIFF N/A`→`N/A`; FAIL/PRECONDITION/FMV/UNKNOWN/unknown-token(`*)`) each beat OK and N/A; N/A exit 0. `_dispatch.test.sh` — via the dispatcher: proto-like N/A + real OK → `${verb}-all-langs-ok`; L6 shape (2×SKIPPED-NO-DIFF + N/A) → `N/A ${verb}-aggregate-na`; empty lang root → rc 2 `FAIL-MISSING-VERB no-languages-registered`; INCLUDE/EXCLUDE filtering every lang out → `SKIPPED-NO-VERB all-langs-filtered`, exit 0, and NO `FAIL-MISSING-VERB` in the output (extends the existing `test_filter_empty_after_filter` with the negative assertion, and adds an EXCLUDE twin). `layer-all.test.sh` — green run with one layer N/A → `TOTAL_RESULT=OK`, exit 0, and the per-layer line still shows `RESULT=N/A`; same input through `--aggregate` → OK.
6. **Rank numbers removed** from comments/docs everywhere (Lead ruling 8): replaced by named order + pointer to `_common.sh::__status_rank` (`_common.sh`, `_dispatch.sh`, `audit.sh`, `layer-all.sh`, `run-guards.sh`, `_wrapper_trap.test.sh`, `_dispatch.test.sh`, `_common.test.sh`, `run-story.test.sh`, runbook :524/633/659/893/942). Runbook printed ladder (:182) and intuition paragraph (:185) collapse to a citation.
7. ADR-0033: §3 #47 amendment (:175), §4 worst-child order (:200 — wrong today; replaced by a citation of the ladder), §6 status table (:236), placeholder prose (:254/256/304-310), §6 #56 amendment (:276), plus a dated **2026-10-06 amendment**: the re-rank, the `no-languages-registered` reclassification, the allow-list test as the replacement detection, and the residual (an unexpected N/A beside a real OK now reads OK at layer level; caught statically by the allow-list instead).
8. Runbook (operations): :165 enum row (N/A meaning; no-languages-registered moves to FMV), :182/:185 citation, :593 L4 row, :632 + :658-677 L6 rows/worked example — explicitly "L6=N/A on a no-dep-change run = no audit ran (rust/ts child lines read `SKIPPED-NO-DIFF no-dep-changes`, proto is the placeholder); TOTAL still OK", :766-767 verb discovery, :788-792 rewritten to state the **allow-list property**, :835 symptom row; new symptom row "`TOTAL_RESULT=N/A` = no layer did real work anywhere — investigate". SKILL.md :490-492 restated as the allow-list property ("an unexpected N/A shows only in child `STATUS=` lines and is caught by the allow-list test"). TODO.md: close the 2026-08-20/09-09 entry (resolved: re-rank chosen by the user; residual recorded in ADR).

### (B) Pre-commit refuses Phase≠complete + bound changes

Design (all in `_gate2_binding.sh`; a separate `gate2_refuse_incomplete_mainmds` function returning rc, called from `gate2_validate_commit`; hook body gets a comment). **One enumerator** (@dry-reviewer B-2): a shared `__gate2_staged_mainmds` does the fail-closed pass (`diff --cached -z --no-renames --diff-filter=d` → `gate2_is_devloop_mainmd` → `git show :<f>` rc-checked) and yields (path, staged content) — it makes NO empty/complete decision; each caller applies its own policy (the trigger keeps skipping empty content, the refusal treats empty as not-complete); both consume it, so they cannot disagree about what is a devloop record or how a read error fails closed. The trigger keeps its existing per-commit replay-state reset; the refusal writes no replay state.

- **SD-1 partition** (@paired-security B1/B2): for every staged, non-deleted (`--diff-filter=d`), non-template (`gate2_is_devloop_mainmd`) main.md, read the STAGED blob (`git show :<f>`, rc-checked); not-complete ≡ `! __gate2_is_complete "$staged"` — the one predicate. An **empty** staged main.md is not-complete (the new check does not sit behind the trigger's `[[ -n "$staged" ]] || continue`). No third state allows.
- **SD-2 universe** (Lead ruling, recorded divergence from the task's literal wording): refuse iff ≥1 not-complete main.md AND `gate2_changeset_staged` (the binding universe = conjunct 2, via `__gate2_capture_nul`, rc-checked) is non-empty. Invariant: the refusal fires exactly when setting Phase=complete would make the verdict required — never stricter than the complete path. Why not literal: a setup-phase main.md + `docs/TODO.md` would be refused with advice ("set complete → verdict required") that is false for that commit; and a second exclusion list would drift from the binding's. @paired-security confirmed no excluded path carries executable/pipeline-affecting content (residual line below).
- **SD-3 ordering** (B4): the check runs first in `gate2_validate_commit`, independent of the trigger slug, vouched replays, or `[[ -z "$staged_slug" ]] && return 0`. It therefore also blocks complete(+verdict) + another setup main.md + code, and vouched-complete-replay + setup main.md + code.
- **SD-4 replays** (@paired-security B5; supersedes my rev-1 exemption and @operations' OID-vouching suggestion): **no exemption** — the refusal applies during replays. Evidence: 27 historical commits carry a non-complete main.md with bound files (latest 50459f8a, 2026-09-22); all are ancestors of main, replayed only in unusual rebases; new commits cannot acquire the shape without `--no-verify`. An exemption would let a hand-written CHERRY_PICK_HEAD at any of those 27 commits pass arbitrary code — no stronger than `--no-verify`, but new surface for no real use. The existing replay skip (complete main.md only) is byte-unchanged. When a replay is in progress, the refusal adds a scoped line (@operations): "only if you are replaying a historical commit whose record never reached `complete`, after resolving the conflict, with this main.md byte-identical to the replayed commit: `git commit --no-verify` is your decision — it skips ALL pre-commit checks; CI on PRs into main/develop re-runs the pipeline (it does not check the record)".
- **SD-5 message** (B7, @operations, @observability): first line, greppable token: `❌ Gate-2: devloop main.md staged at Phase≠complete with validated files`. Then per offending main.md: sanitized path (`__gate2_printable`) + `Phase reads '<v>'` or `Phase row missing/unparseable` (distinct wording); bound files listed (first 5, then `+N more`); remedy: "If this is the devloop you are committing: set its Loop State row to `| Phase | \`complete\` |` — the hook then requires the Gate-2 verdict (devloop SKILL.md Step 8). If you are editing an older record: commit docs/devloop-outputs/ changes separately from the code. See docs/runbooks/devloop-validation.md §8.5." `--no-verify` is **not** advertised as break-glass in this message except in the replay case above (consistent with the existing replay refusal lines).
- **SD-6 fail-closed**: every enumeration via `__gate2_capture_nul`, every `git show` rc-checked; any error ⇒ block with "error evaluating the Phase check".
- **SD-7 `--no-renames`** (Lead ruling 1, @paired-security B6): added to every file-list diff — `gate2_changeset_worktree`, `gate2_changeset_staged`, `gate2_deletions_worktree/_staged`, `__gate2_raw_worktree_paths`, trigger candidates, `__gate2_replay_unvalidated_counts`; `diff-tree` (plumbing, already rename-free) gets it explicitly for clarity. Producer and hook change symmetrically; independent of the user's `diff.renames`.
- **SD-8 residuals, recorded** (Lead ruling 5; B8): the hook judges the staged delta vs HEAD, so (a) code committed with no main.md, then main.md alone at complete, and (b) `git commit --amend` that adds/advances main.md over an earlier code-only commit are not caught locally. **There is no CI-side verdict or record check**: CI re-runs `layer-all.sh` from scratch on PRs into main/develop (validating the tree), but nothing in CI checks that a devloop record reached `complete` or that a verdict existed. A deleted main.md ≡ not staging one (allowed). Excluded paths are guard inputs (story manifests → validate-story-manifest, TODO.md frame-vectors marker, INDEX → knowledge-index, main.md → cross-boundary guards); edits after validation are not re-gated locally except dt-guard todo-tracking in the hook; CI re-runs those guards.
- Pre-work/run-story shapes checked (@operations 7, @test): run-story's manifest bump stages only the story doc (no main.md) and `--finish` stages main.md at complete — both unaffected. Devloop Step 8's `git add -A` follows Phase=complete — unaffected. **One shape now blocks**: a Lead Pre-Work commit made with `git add -A` after Step 2 created main.md (Phase=`setup`) would sweep it in alongside code. SKILL.md Step 2/Pre-Work gets one sentence: stage Pre-Work commits by path, never `-A`.
- **Tests** (`selftest-gate2-verdict.sh`; each refusal asserts rc=1 AND the token AND message content — path, `Phase reads '<v>'`/row-missing wording, the remedy; each allow asserts rc=0 AND token absent):
  - `f2` rewritten — ci-speed shape: Phase=`setup` + `crates/x/src.rs` → block; `f2b` the same WITH a valid PASS verdict present → still block (refusal does not depend on verdict absence).
  - `s1` table-driven Phase variants, each + code, driven through the real entry `gate2_validate_commit` → block: no Phase row, empty cell `| Phase | |`, empty main.md, `COMPLETE`, `Complete`, `|Phase|complete|`.
  - `s4` staged setup while worktree says complete → block (staged blob read).
  - `s5a` setup main.md alone → allow; `s5b` setup main.md + other `docs/devloop-outputs/**` + `docs/TODO.md` + `docs/specialist-knowledge/x/INDEX.md` → allow.
  - `s6` `_template/main.md` (setup) + code → not refused.
  - `s7` complete(+valid verdict) + another setup main.md + code → block; `s8` vouched complete replay + setup main.md + code → block.
  - `s9` conflicted cherry-pick of a historical setup-phase commit, main.md untouched → block with the replay `--no-verify` line; existing `q1` (complete replay) still skips — unchanged.
  - `s10` staged deletion of a setup main.md + code → no refusal (pinned).
  - `s11` git-shim failure on `show :<main.md>` → block (fail-closed).
  - `s12` (B6-i) validate, `git mv` a bound file into `docs/devloop-outputs/`, complete main.md → block, "validated but not staged"; `s13` (B6-ii) setup main.md + rename of a bound file into devloop-outputs → refusal fires; `s14` (B6-iii) bound→bound rename → records stream holds `DELETED <old>` + `<blob> <new>`.
  - Complete path: cases a–p unchanged and still pass; trigger rc semantics (2 = ambiguous, 3 = read error) unchanged.
- Runbook §8.5: trigger section ("Phase is no longer a bypass"), new "Phase not complete" subsection with message, remedy, replay note, residuals (SD-8), failure-shape rows (refusal, error). SKILL.md Step 8: bold step 0 "Set Phase to `complete` BEFORE `git add -A` — the pre-commit hook refuses this main.md at any other Phase staged with code." (assessment: yes, needed — today a clause in the lead-in sentence; the ci-speed devloop missed it).

**Security Decisions**:

| # | Decision | Rationale | Status |
|---|----------|-----------|--------|
| SD-1 | Partition: every staged non-template non-deleted main.md is complete (`__gate2_is_complete`) or not; empty/rowless = not complete | One predicate; no third allowing state | per @paired-security B1/B2 |
| SD-2 | Universe = `gate2_changeset_staged` (verdict-required predicate), not literal `docs/devloop-outputs/` | Refusal ≡ "verdict would be required"; one exclusion set; exclusion set confirmed content-safe | Lead ruling |
| SD-3 | Check runs first, independent of trigger slug/replay skip; mixed shapes block | No ordering bypass | per B4 |
| SD-4 | No replay exemption for not-complete records; replay skip for complete records unchanged | 27 historical shapes; exemption = new forgery surface for no use | per B5 |
| SD-5 | Message: greppable token, sanitized path, Phase value read, bound files (≤5 + count), remedy, §8.5 | Actionable; untrusted content sanitized | per B7/ops/obs |
| SD-6 | All enumerations/reads rc-checked; error ⇒ block | Fail-closed contract | — |
| SD-7 | `--no-renames` on every file-list diff, producer + hook | Rename hid bound deletions from the binding | Lead ruling 1 |
| SD-8 | Residuals: post-hoc main.md commit, `--amend`; no CI record/verdict check exists | Hook sees staged delta only | Lead ruling 5 — accepted |

### (C) Playwright pin from pnpm-lock.yaml; bounded download

1. `infra/lib/pnpm-lock-version.sh`: `pnpm_lock_version <pnpm-lock.yaml> <package>` — reads only keys of the top-level `packages:` section, anchored `^  <pkg>@<ver>:` (and the quoted `'<pkg>@<ver>':` form; peer suffix `(...)` stripped), so `playwright-core@…`, `'@vitest/browser-playwright@…(playwright@…)'` and importer entries never match; `sort -u`; fails `ERROR: pnpm_lock_version:` on unreadable / absent / >1 distinct versions (prints them) / version not matching `^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$`.
2. `devloop.sh`: `read_playwright_version()` → `--build-arg "PLAYWRIGHT_VERSION=${PLAYWRIGHT_VERSION}"` at the ONE build site. `build_devloop_image` already removes the old image only after a successful, different build — a failed/timed-out Playwright step leaves the working image intact (recorded in Rollback).
3. `infra/devloop/playwright-install.sh <version>` (COPYed into the image, testable): validates the version shape; `for attempt in 1..ATTEMPTS`: `timeout --kill-after=KILL_AFTER TIMEOUT npx -y "playwright@<v>" install chromium`; logs `playwright-install: attempt i/N rc=<rc> (124=timeout, 137=killed)` per attempt; between attempts `rm -rf "${PLAYWRIGHT_BROWSERS_PATH:?}"` (the ENV stays the single source) (clears partial extracts and Playwright's `__dirlock`) + fixed sleep; final failure: `ERROR: devloop image step 'npx playwright@<v> install chromium' failed after N attempt(s) (timeout Ts each, last rc=R)` → exit 1; on success `chmod -R a+rX`. Knobs are env vars with defaults in the script (`PLAYWRIGHT_INSTALL_TIMEOUT_SECS=600`, `_ATTEMPTS=3`, `_KILL_AFTER_SECS=30`, `_RETRY_SLEEP_SECS=10`); comment states worst case ≈ ATTEMPTS×(T+KILL_AFTER)+sleeps ≈ 32 min, and that changing them means editing the defaults (devloop.sh does not pass them — @operations 1). Comment: the "running npx playwright install without first installing your project's dependencies" warning is expected (no repo in the build context). Partial-state check: the existing post-install `find … chrome` smoke runs after a *successful* attempt only; the wipe between attempts means no killed attempt's tree survives into it.
4. `Dockerfile`: `ARG PLAYWRIGHT_VERSION` (no default), `: "${PLAYWRIGHT_VERSION:?…}"`, `COPY playwright-install.sh`, `RUN … playwright-install.sh "${PLAYWRIGHT_VERSION}"`; header lines 7-10 list the new derived arg; the "bump this ARG in the SAME commit" comment rewritten (derived now; only the rebake is host-side).
5. Tests, all in `scripts/setup.test.sh` group (F) (Lead ruling 7 / @test C1), PATH-stub `npx` counting its invocations, tiny timeouts via env: hang → killed, exactly N attempts, rc≠0, message names the step + last rc; TERM-ignoring hang → still bounded (kill-after); fail-then-succeed → 2 attempts, rc 0; immediate success → 1 attempt; bad version shape → refused before npx runs; partial dir wiped between attempts. `scripts/setup.test.sh` group (F): reader fixtures (exact; differing `playwright-core@9.9.9`; peer-suffix snapshot key; quoted key; section scoping — a different `playwright@9.9.9` only under `snapshots:` and in an importer `version:` is ignored; two versions → rc 1 "more than one version"; absent; unreadable; bad shape) + real-lock control; every `packages/*/package.json` declaring `playwright` pins an exact semver equal to the reader's output on the real lock (set by glob, ≥2); devloop.sh derives + passes the arg from the reader; Dockerfile has bare `ARG PLAYWRIGHT_VERSION` and runs the script with `"${PLAYWRIGHT_VERSION}"`; no `playwright@[0-9]` and no `PLAYWRIGHT_VERSION=[0-9]` anywhere in the Dockerfile or devloop.sh.
6. Residual → `docs/TODO.md` (Lead ruling 6): `npx playwright@V` is version-pinned but not integrity-pinned (no lockfile at build time); browser archives are trusted on HTTPS alone. Task-sized.

### Rollback / safe-revert unit
(A), (B) and (C) are independently revertible. (A) must revert together with its ADR-0033/runbook/SKILL.md text and the `no-languages-registered` reclassification. (B)'s `--no-renames` may stay if the refusal is reverted (it only tightens). (C): a failed build keeps the old image; recovery = fix + `devloop.sh --rebuild` (host-side).

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | CLEAR | 0 | 0 | 0 | selftest 62/62 → re-checked final state 65/65, mutation-tested; dep change integrity-verified vs registry |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | producer-side --no-renames tests (s12b/s12c); unwired dispatch test (co-signed obs F1) |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | unwired *.test.sh wired into L3; [phase-not-complete] token |
| Code Quality | RESOLVED-FIXED | 6 | 5 | 0 | 1 withdrawn (dep changes = Lead ruling); selftest 63/63 |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | regex-copies equality test; ADR ladder drift test |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | fractional test timeouts; SKILL.md list split |

---

## Pre-Work

None

---

## Implementation Summary

### (A) Green run totals OK
| Item | Before | After |
|------|--------|-------|
| `__status_rank` | `… SKIPPED-NO-CLUSTER 2 < OK 3 < N/A 4 < FAIL 5 …` | `… SKIPPED-NO-CLUSTER 2 < N/A 3 < OK 4 < FAIL 5 …` (FAIL and above unchanged; `status_to_exit_code` untouched) |
| `_dispatch.sh` empty lang root | `N/A no-languages-registered`, exit 0 | `FAIL-MISSING-VERB no-languages-registered`, exit 2 |
| N/A detection | the ladder (N/A dominated OK) | static N/A emitter allow-list (`_dispatch.test.sh:test_na_emitter_allowlist`, positive-controlled, all quoted spellings) |
| Rank integers in prose | restated in ~15 comments/docs (several already stale) | removed; named order + citation of `__status_rank` |

Docs: ADR-0033 §3/§4/§6 + 2026-10-06 amendment; runbook §3 (ladder → citation, `TOTAL_RESULT=N/A` meaning), §6.4/§6.6 (L6=N/A on no-dep-change runs = no audit ran), §7 (item 4, allow-list property), §8 rows; SKILL.md N/A paragraph; TODO entry closed (re-rank, not split).

### (B) Phase≠complete refusal
`gate2_refuse_incomplete_mainmds` (SD-1..SD-6) runs first in `gate2_validate_commit`; `__gate2_staged_mainmds` is the one fail-closed staged-record enumerator (trigger + refusal); `__gate2_phase_cell` is the one Phase-row parser (`__gate2_is_complete` rebuilt on it, same semantics). `--no-renames` on every file-list diff (SD-7). A read/enumeration error in the refusal reports the existing `error evaluating the commit trigger (rc=3)` message (it is the same failure, shared enumerator), so r13/r19 and the runbook row are unchanged. Runbook §8.5 "Phase not complete" + residuals (SD-8) + failure rows; SKILL.md Step 2 (Pre-Work by path) + Step 8 step 0; hook comment.

### (C) Playwright
`infra/lib/pnpm-lock-version.sh` (packages:-section keys only, shape-checked, fails on 0/>1); `devloop.sh:read_playwright_version()` + build arg; Dockerfile bare `ARG PLAYWRIGHT_VERSION` + `:?` guard + `COPY`/`RUN infra/devloop/playwright-install.sh` (timeout --kill-after per attempt, 3 attempts, per-attempt rc log, `rm -rf "${PLAYWRIGHT_BROWSERS_PATH:?}"` between attempts, final `ERROR: devloop image step '…' failed after N attempt(s) (timeout Ts each, last rc=R)`). Tests in `scripts/setup.test.sh` group (F); mutation-checked (attempt off-by-one, no `--kill-after`, no wipe all go red).

### Additional Changes
**Review round 1 fixes.** Wired `_dispatch.test.sh` (which carries the N/A allow-list), `_wrapper_trap.test.sh`, `_get_base_ref.test.sh` and `_layer_skeleton.test.sh` into `scripts/layer3.sh` — all four were unrun, so the allow-list the ADR/runbook/SKILL.md describe was not enforced until now (@observability F1). Round 2: producer-side rename tests s12b (worktree records) + s12c (rename → emit → validate → allow), mutation-checked against dropping the producer `--no-renames` (@test T1); stale TODO inventories of unwired tests updated (@observability F4). `_get_base_ref.behavior-equivalence.test.sh` investigated: not hung, 6m51s (runs the whole real pipeline in a fixture) and 2/11 cases stale → TODO, not wired. Added `[phase-not-complete]` token; an accepting-forms mirror table (s13) for the rebuilt Phase parser (note: the rebuilt parser trims tabs as well as spaces, so a tab-padded `complete` row now counts as complete — strictly toward requiring a verdict); regex drift guard + ADR ladder-order drift guard (DRY F1/F2; runbook no longer restates the order); fractional test timeouts (setup.test.sh back to ~29 s); comment fixes. Selftest cases renumbered contiguous: plan s4→s2, s5a/b→s3a/b, s6→s4, s7→s5, s8→s6, s9→s7, s10→s8, s11→s9, s12→s10, s13→s11, s14→s12; s13 is new.

**Pre-existing advisories fixed (Lead ruling, option a).** Layer 6 went red on two high `pnpm audit` advisories not introduced by this diff — they surfaced because the branch-wide dependency gate (BASE_REF = merge-base with origin/main, which spans this branch's earlier dependency-upgrade commits) ran the scan: GHSA-w3vv-58gj-gw77 (`nx` <23.1.2; root devDep bumped `23.1.1` → `23.1.3`, no `@nx/*` packages are declared) and GHSA-68fv-2mgg-jv7q (`source-map-js` <1.2.2, transitive via vite>postcss; `source-map-js: '>=1.2.2 <2'` added to `pnpm-workspace.yaml` `overrides:`). `pnpm install` regenerated the lockfile; `pnpm audit --audit-level high` now reports 0 high (7 moderate, below the gate). The lockfile still resolves one `playwright` (1.62.1). @paired-security reviews the dependency change at Gate 3.

Mutation checks run during implementation (all restored): removing `--no-renames` reds s12/s13/s14; the retry-loop mutations above red 2–5 `pwi-*` cases.

---

## Files Modified

```
 .claude/skills/devloop/SKILL.md, .githooks/pre-commit, docs/TODO.md,
 docs/decisions/adr-0033-polyglot-validation-pipeline.md, docs/runbooks/devloop-validation.md,
 docs/specialist-knowledge/infrastructure/INDEX.md, infra/devloop/{Dockerfile,devloop.sh},
 infra/devloop/playwright-install.sh (new), infra/lib/pnpm-lock-version.sh (new),
 scripts/{audit.sh,layer-all.sh,layer-all.test.sh,setup.test.sh}, scripts/guards/run-guards.sh,
 scripts/guards/simple/selftest-gate2-verdict.sh, scripts/lang/{_common.sh,_common.test.sh,
 _dispatch.sh,_dispatch.test.sh,_gate2_binding.sh,_wrapper_trap.test.sh},
 scripts/lang/{proto/audit.sh,proto/test.sh,rust/audit.sh}, scripts/workflow/run-story.test.sh
 (24 tracked files changed + 2 new)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `scripts/lang/_common.sh` | N/A/OK rank swap; precedence comment rewritten |
| `scripts/lang/_dispatch.sh` | empty lang root → FAIL-MISSING-VERB exit 2 |
| `scripts/lang/_gate2_binding.sh` | Phase refusal, shared enumerator, Phase-row parser, `--no-renames` |
| `scripts/guards/simple/selftest-gate2-verdict.sh` | f2 rewritten; f2b, s1 (7 variants), s4–s14 |
| `infra/lib/pnpm-lock-version.sh` | new lockfile reader |
| `infra/devloop/playwright-install.sh` | new bounded/retried install |
| `scripts/setup.test.sh` | reader, pin-agreement, derivation, retry-loop tests |

---

## Devloop Verification Steps

Gate 2 — `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the final reviewed tree (run-all, exit 0):

| Layer | Result | Duration |
|-------|--------|----------|
| 1 Compile | OK | 4s |
| 2 Format | OK | 7s |
| 3 Guards (incl. newly wired `_dispatch`/`_wrapper_trap`/`_get_base_ref`/`_layer_skeleton` selftests, Gate-2 selftest 65/65) | OK | 132s |
| 4 Test | OK | 169s |
| 5 Lint | OK | 1s |
| 6 Audit (pnpm audit 0 high after nx/source-map-js fix) | OK | 2s |
| 7 Env-tests (Rust env-tests + browser E2E) | OK | 859s |
| **TOTAL_RESULT** | **OK** (previously read N/A on a green run — fix (A) visible) | 1174s |

Fast checks: layer-fast green before review and after each fix round. L3/L4 exceed the 20s fast-lane warning budget; pre-existing per @operations' baseline measurement (this diff's net cost after fractional test timeouts: ~+1s L3).

Host-side remaining action: `./infra/devloop/devloop.sh --rebuild` — the only real exercise of `playwright-install.sh` under podman (hermetic stub tests only in-container). A failed build leaves the previous image in place.

---

## Code Review Results

See §Gate 3 — Verdicts above (Security CLEAR 0/0/0; Test RESOLVED-FIXED 2/2/0; Observability RESOLVED-FIXED 4/4/0; Code Quality RESOLVED-FIXED 6/5/0 + 1 withdrawn; DRY RESOLVED-FIXED 2/2/0, no extraction opportunities; Operations RESOLVED-FIXED 2/2/0). Semantic Guard not spawned (no checks.md surface).

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- `docs/TODO.md` §Supply Chain — devloop image npx Playwright install not integrity-pinned
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — stale, slow, unwired base-ref behavior-equivalence test

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: (A), (B) and (C) are independently revertible. (A) reverts as one unit with its ADR-0033 / runbook / SKILL.md text and the `no-languages-registered` reclassification (reverting the rank without the docs leaves docs describing the wrong ladder). (B)'s `--no-renames` may stay if the refusal is reverted (it only tightens the binding); the refusal and its selftest cases revert together. (C): a failed/timed-out image build keeps the previous image (`build_devloop_image` removes it only after a successful, different build); recovery is fix + `devloop.sh --rebuild` (host-side). (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
