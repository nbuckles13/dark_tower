# Devloop Output: Gate-2 authority gate (tree-bound verdict artifact + commit-hook enforcement)

**Date**: 2026-06-09
**Task**: `layer-all.sh` emits a tree-bound Gate-2 verdict artifact; the existing pre-commit hook enforces it, so a devloop cannot be committed without a fresh, tree-matching, passing local pipeline run. Closes the "authority" skip-vector (vector C) deferred out of task #50.
**Specialist**: infrastructure (paired with operations)
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-49`
**Duration**: ~90m (extended planning given security-critical gate machinery + a thorough review pass)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e6b91d157dd39ff43067a595826db574b1313e4c` |
| Branch | `feature/browser-client-join-task-49` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (Iteration 2 — `--light --continue` done) |
| Implementer | `implementer@gate2-continue` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `security@gate2-authority-gate` |
| Test | `test@gate2-authority-gate` |
| Observability | `observability@gate2-authority-gate` |
| Code Quality | `code-reviewer@gate2-authority-gate` |
| DRY | `dry-reviewer@gate2-authority-gate` |
| Operations | `operations@gate2-authority-gate` (PAIRED — active collaborator + Gate-2 reviewer) |
| Semantic Guard | `semantic-guard@gate2-authority-gate` |

---

## Task Overview

### Objective
Three skip-vectors let unrun/under-run validation masquerade as a pass: (A) test-level
(`#[sqlx::test]` no-ops with no DB — repaired in #46/#49), (B) pipeline-edge
(`SKIPPED-NO-VERB`/early-exit reads as exit 0 — task #50), and **(C) authority** — the
Lead accepts a self-report (or its own assertion) *in lieu of* actually running
`layer-all.sh`, then flips the tracking table to Completed and commits. Vector C has no
mechanism today. This devloop closes it.

### Design (settled with the human Lead — implement as specified)

1. **Producer = `layer-all.sh`** writes `/tmp/devloop/gate2-verdict` as its FINAL
   step via an `EXIT` trap (same pattern as task #50's wrappers), so an early-exit on a
   failing layer still emits `GATE2=FAIL` — a *missing* file and a *failing* run are
   distinct states, never conflated. The agent never writes this file; only a real
   pipeline run produces it.
   - **Format = flat `KEY=VALUE` + repeated prefixed lines (NOT JSON — drop the jq
     dependency so the validator needs only coreutils; consistent with ADR-0033's existing
     `LAYER=`/`STATUS=` line conventions).** Enforced scalars (hook greps these):
     `GATE2`(PASS|FAIL), `SLUG`, `SIGNATURE`. Also `LAYER_ALL_EXIT`, `HEAD`, `BASE_REF`,
     `RUN_AT`, `EXCLUSIONS`. Arrays as prefixed lines: `LAYER <n> <result> <dur> [counts…]`
     (reuse the existing summary's `LAYER=` data; layer-4 `passed/failed/ignored/filtered`
     are RECORDED for humans, NOT hook-enforced). **Two distinct serializations of the
     binding set, and they must not be conflated:**
     1. **On-disk `FILE <path>\t<blob>` lines — HUMAN/DISPLAY ONLY.** TAB-rendered for a
        reader scanning the artifact; the validator reads them back ONLY for the drift
        diagnostic (naming changed paths), NEVER to recompute the signature. Their TAB
        ambiguity therefore cannot affect integrity.
     2. **Signature input — a SEPARATE blob-first, NUL-terminated record stream**
        (`<blob><SP><path>\0` per record): the blob is the fixed-shape hex OID first, the
        path is the trailing field running to the NUL, so a path containing spaces/TAB/
        newline needs NO escaping (NUL is the only byte a git path cannot contain). The
        `signature` is `sha256` over these records, `LC_ALL=C sort -z`-ordered. This is the
        integrity-bearing serialization; the `FILE` lines are not.
     Enumeration uses git's `-z` on BOTH producer and hook so the byte stream is identical
     on both sides (never mix NUL-raw with `core.quotePath`-quoted text). This supersedes the
     earlier single-`FILE`-line-with-TAB sketch, which @security + @code-reviewer flagged as
     delimiter-ambiguous for raw (unquoted) paths.

2. **The binding is the ONLY enforced integrity field.** `files` = the uncommitted
   changeset (`git diff --name-only HEAD` ∪ untracked) MINUS `exclusions`; `blob` =
   `git hash-object` of working-tree content; `signature` = sha256 over the canonical
   blob-first NUL records (per point 1, `LC_ALL=C` sorted). **Proposed 4-entry exclusion set** (operations-owned, mirrors the
   `cross_boundary_scope.rs:70-84` `is_symmetric_exclusion` precedent one-for-one; pending
   security + code-reviewer line-by-line sign-off at Gate 1): `docs/devloop-outputs/**`
   (deliberate widening past precedent — Gate-3 write-back self-invalidates → infinite loop;
   per-entry comment must name this reason), `docs/TODO.md` (literal equality),
   `docs/specialist-knowledge/*/INDEX.md` (single-segment `*`, no `/` crossing),
   `docs/user-stories/*.md` (**depth-2 `*.md` per `is_user_story_path`, NOT `**` — code-reviewer
   caught that `**` over-excludes: a nested non-`.md` file would escape the binding**). All four
   use the narrow anchored semantics of the precedent (`cross_boundary_scope.rs:64-84`
   `is_symmetric_exclusion`) — no `**/INDEX.md`, no substring/`grep -F`. NOT excluded:
   `crates/**`, `scripts/**`, `.githooks/**`, `.github/**` (the validated surface, incl. this
   devloop's own edits), and lockfiles. Single-sourced (`gate2_is_excluded`), shared by emitter +
   hook so trigger-conjunct-2 and the signature use the identical predicate. **Path enumeration
   must be NUL (`-z`) on BOTH sides** (never mix NUL-raw with `core.quotePath` text, or an exotic
   path hashes differently → signature gap). **This is the key judgment call.**

3. **Validator = the EXISTING `.githooks/pre-commit`** (version-controlled, wired via
   `core.hooksPath`; already checks main.md for unfilled `TBD`/pending sections — extend
   it). Do NOT put the check inside `layer-all.sh` (circularity: a check inside the
   pipeline cannot catch "the pipeline never ran").
   - **TRIGGER (two-conjunct, decided with human Lead): require the verdict IFF (1) a
     staged `docs/devloop-outputs/<slug>/main.md` is at Phase=complete, AND (2) the staged
     changeset (`changeset − exclusions`, same universe as the binding) contains ≥1 file.**
     Conjunct 2 reuses the exclusion machinery so historical/typo edits to an already-complete
     main.md (validated-set empty → no-op) cause no friction, AND it closes the `--continue`
     gap (a reopened devloop whose main.md is already `complete` still has validated files →
     enforced). A naive "transition-to-complete" rule was considered and rejected for missing
     `--continue`.
   - Require: file present + `GATE2==PASS` + `SLUG`==staged main.md's slug (rejects a stale
     verdict from a different devloop in the same session) + recomputed `SIGNATURE` over the
     **STAGED index** (`git diff --cached` / `:path` index blobs — NOT working tree) matches
     recorded. On mismatch, diff recorded-vs-recomputed `FILE` lines and NAME the drifted
     path(s) + instruct re-run. Absent / `FAIL` / stale / slug-mismatch → block.
   - **FAIL-CLOSED**: any error while *evaluating the trigger or verdict* → block (do not
     `|| true`-skip). The filter must not itself become a vector-B silent skip.
   - **Accepted gap** (human-Lead ruling): code committed *separately* from main.md (a
     non-standard split commit, outside the `git add -A` devloop flow) no-ops locally; CI's
     from-scratch re-run is the backstop. Closing it locally = "verdict on any validated-file
     commit" = blocks all ordinary commits = too broad.

4. **CI = forgery backstop.** `ci.yml` already re-runs `layer-all.sh` from scratch and
   does NOT read the `/tmp` artifact — confirm/keep that. Document the threat model
   explicitly: locally this is **anti-drift / anti-laziness, NOT anti-forgery** (an agent
   with fs access can hand-write a well-formed PASS). CI's independent re-run is the
   forgery defense — which is also why the artifact is ephemeral in `/tmp` and never
   committed.

5. **main.md stays HUMAN-AUTHORED** in the current format — do NOT render it from the
   JSON (explicitly considered and rejected: main.md is the human summary, not
   load-bearing for the gate; the JSON is the authoritative record).

### Bootstrapping wrinkle (handle explicitly)
This devloop edits `layer-all.sh` (whose own Gate 2 the Lead runs) and `.githooks/pre-commit`
(which will gate this very commit). **Test the hook in isolation first** (temp repo / dummy
staged tree) so a bug cannot wedge the commit, then rely on it for the real commit.

### Scope
- **Service(s)**: none (devloop tooling: new `scripts/lang/_gate2_binding.sh` shared helper,
  `scripts/layer-all.sh`, `.githooks/pre-commit`, `.github/workflows/ci.yml` confirm-only)
- **Schema**: No · **Cross-cutting**: tooling that gates every future devloop (high blast radius)

### Out of scope
- Test-level skip enforcement (`0 ignored/0 filtered` as a hard fail) — belongs in
  Layer 4 / task #50 (vector A/B), not this hook (vector C).
- `gate3-verdicts.json` (separate, later).
- Production service code; any GSA path.

### REQUIRED verification matrix (must appear in §Issues/Verification with evidence)
- (a) clean run → PASS verdict written, signature matches staged tree → commit proceeds.
- (b) edit a validated source file after the run → signature mismatch → commit blocked, naming the drifted file.
- (c) edit ONLY `docs/devloop-outputs/<slug>/main.md` after the run → commit still proceeds (exclusion works).
- (d) a layer fails → `result:FAIL` written via the EXIT trap → commit blocked.
- (e) verdict file absent → commit blocked.
- (f) a NON-devloop commit (no devloop main.md staged) → hook no-ops (does not require a verdict).

### Debate Decision
NOT NEEDED — design settled in the human-Lead design discussion captured above; this is
implementation of an agreed mechanism within existing ADR-0033/ADR-0024 machinery.

---

## Cross-Boundary Classification

<!-- SQUASHED: this devloop was developed across 3 working commits (the gate; the
     severity-record correction; the --light --continue follow-up) and squashed into ONE.
     The table below lists the FULL squashed changeset. Internal references elsewhere in
     this doc to the pre-squash working commits (e370340 / 392cadd / 3a2ed50) are retained
     for the development narrative; those SHAs no longer exist in history post-squash. -->

All changed files are infrastructure-owned (Mine); none are Guarded Shared Areas
(`scripts/**`, `.githooks/**`, `.github/**`, `docs/runbooks/**` meet no ADR-0024 §6.4
criterion). `docs/devloop-outputs/**` and `docs/TODO.md` are binding-excluded (not listed —
the scope-drift guard exempts them).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/_gate2_binding.sh` (new) | infrastructure (mine) — shared single-source helper: exclusion predicate, changeset enumeration (worktree + staged), blob resolution, blob-first NUL `gate2_signature`, slug derivation, `emit_gate2_verdict`, `gate2_validate_commit` + drift; first-class `DELETED` records; source-safe (function-local `local -`; bash≥4.4 guard returns, never exits). | — |
| `scripts/layer-all.sh` | infrastructure (mine) — producer: sources the helper; EXIT trap emits the verdict as the final step (`declare -a` layer arrays). | — |
| `.githooks/pre-commit` | infrastructure (mine) — validator: two-conjunct trigger + verdict enforcement over the staged index; fail-closed. | — |
| `scripts/guards/simple/selftest-gate2-verdict.sh` (new) | infrastructure (mine) — 15-case isolation self-test (matrix a–o incl. deletion integrity `case_l`/`case_o`, source-safety `case_n`); auto-run at Layer 3 + CI. | — |
| `.github/workflows/ci.yml` | infrastructure (mine) — comment-only: CI re-runs from scratch and never reads /tmp (the forgery backstop). | — |
| `docs/runbooks/devloop-validation.md` | infrastructure (mine) — §8.5 threat model + failure catalogue + `--no-verify` note. | — |

---

## Planning

Plan v2: shared single-source helper `scripts/lang/_gate2_binding.sh` (coreutils+git, no jq)
consumed by both producer and hook; EXIT-trap producer; two-conjunct trigger; flat KEY=VALUE
format; 4-entry exclusion set; changeset-derived slug; isolation self-test.

### Gate 1 Plan Confirmations — ALL CONFIRMED → Plan Approved 2026-06-09

Layer B classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (2 constraints: narrow-anchored exclusion semantics + NUL-both-sides; runbook --no-verify/CI note; "go on Lead approval") |
| Test | confirmed (6 Gate-2 conditions: a–f as real runnable self-test wired into a layer, staged-index drift, EXIT-trap real-fail) |
| Observability | confirmed (gate diagnosability + verdict faithfulness lenses) |
| Code Quality | confirmed (full Gate-1: CB table verified 6 rows all Mine/none-GSA; ADR-0033/0024 compliant; exclusion set 3-way locked) |
| DRY | confirmed (single-source `_gate2_binding.sh` makes producer/hook divergence impossible; no third copy) |
| Operations | confirmed/PAIRED (co-designed trap + exclusion set + slug + runbook; pre-empted the bash `case */` over-exclusion hazard) |
| Semantic Guard | confirmed (credential-leak + error-context-preservation lenses; mostly N/A) |

Key design refinements during Gate 1: `docs/user-stories/**` → `*.md` (code-reviewer: `**`
over-excludes); NUL enumeration both sides (security); slug = changeset-derived, never-abort,
diagnostic-only (operations confirmed no harness marker); 4th exclusion `specialist-knowledge/*/INDEX.md`.

---

## Pre-Work

None.

---

## Implementation Summary

Closed skip-vector C (authority) with a tree-bound verdict artifact + commit-hook enforcement,
built entirely from one shared library so the producer and validator cannot diverge.

**Shared library `scripts/lang/_gate2_binding.sh` (new)** — coreutils + git only, self-contained
(own `set -euo pipefail` + bash≥4 guard, no `_common.sh` dependency so the hook stays light).
Provides the single source of truth for:
- `gate2_is_excluded` — the 4-entry exclusion predicate, implemented as explicit anchored
  string-ops (NOT bash `case` globs, which match `/` and would silently over-exclude nested
  paths — the #1 landmine flagged by security + operations). `docs/devloop-outputs/**`
  (prefix-anchored, the one deliberate widening past the precedent), `docs/TODO.md` (exact
  equality), `docs/specialist-knowledge/*/INDEX.md` (single-segment, no `/`), `docs/user-stories/*.md`
  (depth-2, no `/`). Mirrors `cross_boundary_scope.rs::is_symmetric_exclusion`.
- `gate2_changeset_worktree` / `gate2_changeset_staged` — NUL (`-z`) enumeration on BOTH sides
  (`git diff -z HEAD` ∪ `ls-files -z --others` for the producer; `git diff --cached -z` for the
  hook), minus exclusions, `LC_ALL=C sort -z -u`.
- `gate2_blob_worktree` (`git hash-object`) / `gate2_blob_staged` (`git rev-parse :path`) — equal
  OIDs for equal content (documented assumption: no content-altering clean/smudge filters).
- `gate2_signature` — sha256 over `LC_ALL=C`-sorted, NUL-terminated, **blob-first** records
  (`<blob> <path>\0`): blob is fixed-width hex first, path is the trailing field to the NUL, so a
  path with spaces/TAB/newline needs no escaping (NUL is the only byte a git path can't contain).
  The signature is over this record stream, NOT the human-readable `FILE` lines.
- `gate2_derive_slug` — PURE: exactly-one active `docs/devloop-outputs/<slug>/main.md` (per
  `find_active_main_md` rules) → slug; 0 or >1 → empty (never aborts). Callers own policy.
- `emit_gate2_verdict` — producer body. `GATE2` derived from the real exit code (PASS iff 0).
- `gate2_validate_commit` / `gate2_staged_complete_slug` / `gate2_report_drift` — hook body:
  two-conjunct trigger, slug diagnostic, signature check, three-bucket drift naming. FAIL-CLOSED.

**Producer `scripts/layer-all.sh`** — sources the library; declares the per-layer arrays + an
`EXIT` trap *before* the sentinel/precondition checks. The trap captures the real `$?` first,
emits the verdict (errors logged, never mutating the exit code), `trap - EXIT`, `exit $rc`. So a
missing file means exclusively "never invoked", and an early-exit still writes `GATE2=FAIL`.

**Validator `.githooks/pre-commit`** — extends the existing devloop-main.md block: resolves the
repo root, sources the library, runs `gate2_validate_commit || exit 1`. Fail-closed if the library
is missing. No-ops for non-devloop commits.

**Isolation self-test `scripts/guards/simple/selftest-gate2-verdict.sh` (new)** — drives the
producer + validator against synthetic staged trees in throwaway temp git repos. Placed under
`simple/` so `run-guards.sh` auto-discovers it → runs every devloop (Layer 3) + CI; emits the
ADR-0033 `STATUS=` line. Built and proven green BEFORE the new hook was relied on (bootstrapping).

**CI `.github/workflows/ci.yml`** — confirmed it already re-runs `layer-all.sh` from scratch and
never reads the `/tmp` artifact (forgery backstop); added a comment so nobody "optimizes" by
trusting the verdict. **Runbook `docs/runbooks/devloop-validation.md`** — new §8.5 (threat model,
trigger, failure shapes, `--no-verify` escape hatch) + catalogue rows + changelog.

### REQUIRED verification matrix (a–f) — evidence

All run by `scripts/guards/simple/selftest-gate2-verdict.sh` against isolated temp repos
(`14 case(s) passed, 0 failed; STATUS=OK`). Mapping:

| Matrix | Self-test case | Result |
|--------|----------------|--------|
| (a) clean run → PASS verdict matches staged tree → commit proceeds | `case_a` | ✅ allow |
| (b) edit a validated source file after the run → mismatch → blocked, names file | `case_b` (`modified after validation: src.rs`) | ✅ block + name |
| (c) edit ONLY `docs/devloop-outputs/<slug>/main.md` → still proceeds (exclusion) | `case_c` | ✅ allow |
| (d) a layer fails → `GATE2=FAIL` via the EXIT trap → blocked | `case_d` | ✅ block |
| (e) verdict file absent → blocked | `case_e` | ✅ block |
| (f) NON-devloop commit (no complete main.md staged) → hook no-ops | `case_f1`/`case_f2` | ✅ no-op |

Extra regression guards (g–n): `case_g` (stale verdict from a different devloop → slug-mismatch
block), `case_h` (new staged file not in verdict → `staged but not in verdict` block), `case_i`
(producer stamps the correct slug end-to-end — pre-exclusion-vs-raw slug bug, §Issues 1),
`case_j` (exclusion predicate faithful to `is_symmetric_exclusion` — BOTH directions: smuggling
paths stay bound, legit paths excluded), `case_k` (>1 staged complete main.md → fail-closed block,
§Issues 2b), `case_l` (staged deletion → clean `DELETED <path>` record, NOT garbage — RED pre-fix /
GREEN post-fix; §Issues 7), `case_m` (deletion + post-emit tamper → block, names the tampered file),
`case_n` (source-safety is INTRINSIC: a direct, non-subshell call of a strict fn restores `-u` OFF
via `local -` — RED if `local -` is removed; §Issues 6b).

---

## Files Modified

| File | Change |
|------|--------|
| `scripts/lang/_gate2_binding.sh` | NEW — shared producer/validator library (exclusion predicate, NUL changeset enum, blob, blob-first signature, pure slug-derive, `emit_gate2_verdict`, `gate2_validate_commit` + drift diagnostics). |
| `scripts/layer-all.sh` | Source the library; declare per-layer arrays + `EXIT` trap before the early-exit checks; trap captures real `$?`, emits, re-exits unchanged. |
| `.githooks/pre-commit` | Extend the devloop block: source the library, `gate2_validate_commit \|\| exit 1`; fail-closed if the library is missing. |
| `scripts/guards/simple/selftest-gate2-verdict.sh` | NEW — isolation self-test (matrix a–n, 14 cases); auto-discovered by `run-guards.sh` (Layer 3 + CI). |
| `.github/workflows/ci.yml` | Comment-only: document why CI re-runs from scratch and never reads the `/tmp` verdict (forgery backstop). No behavior change. |
| `docs/runbooks/devloop-validation.md` | New §8.5 (Gate-2 threat model / trigger / failure shapes / `--no-verify` / self-test) + §8 catalogue rows + changelog. |

---

## Code Review Results

### Gate 3 — Verdicts (all 7 in; none ESCALATED, none RESOLVED-DEFERRED)

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | RESOLVED-FIXED | **HIGH signature-void integrity bypass** (final, reproduced): a deleted path sorting BEFORE other changed files truncated the pre-fix record stream to the empty-hash on both sides → recorded==recomputed → rc=0 ALLOW (post-validation tamper escapes). Reproduced from the verbatim pre-fix staged-index blob on bash 5.2.15. (Security's mid-review MEDIUM/over-block call was a contaminated-reconstruction error — wrong strict-mode idiom + reused stale verdict — retracted; the HIGH is the evidence-based final, consistent with §Issues 7.) Fixed: first-class `DELETED` records + fail-closed `--verify --quiet` (both orderings block; clean deletions still allow); mechanism pinned by `case_l`. **Severity corrected post-commit** (this devloop committed `e370340` recording MEDIUM during the whiplash; see §Issues / Lessons + the `--continue` follow-up that adds the dedicated adversarial-ordering regression case). |
| Test | RESOLVED-FIXED | Self-test **mutation-tested** (disabling sig-check / over-excluding / producer-FAIL→PASS / dropping staged deletions each turned it RED); stale case-count fixed. |
| Observability | CLEAR | LAYER lines deterministic 1..7 (explicit numeric loop); verdict faithful to LAYER_SUMMARY. |
| Code Quality | RESOLVED-FIXED | bash-4.4 floor, intrinsic `local -` source-safety, DEVLOOP_TMP guard, `declare -A`→`-a`. ADR-0033/0024/0019 compliant; all files infrastructure/Mine, none GSA. |
| DRY | RESOLVED-FIXED | Slug-idiom extracted; single-source binding lib confirmed (producer/hook can't diverge). Bash↔Rust parity TODO-tracked (ADR-0019 exception, not a deferral). |
| Operations (paired) | RESOLVED-FIXED | bash-3 hook-wedge (standout catch) + source-safety hardening + `case_n`; runbook §8.5 + ci.yml forgery-backstop. |
| Semantic Guard | CLEAR (SAFE) | Verdict artifact records only paths/hashes/SHAs/counts (no contents/secrets); EXIT-trap/hook preserve failure context. |

### Gate 2 — Lead independent verification (dogfood, both directions)
Attempt 1 FAILED on `validate-todo-tracking` (the pipeline caught this devloop's OWN main.md format) → the EXIT trap correctly emitted `GATE2=FAIL` with right slug/signature (matrix d, live on the real pipeline). Attempt 2 PASS (`LAYER_ALL_EXIT=0`, guards 33/33, `GATE2=PASS`). Independently verified: self-test 14/14, source-safety (zero file-scope side-effects), `DELETED`-record handling, `rev-parse --verify --quiet`, DEVLOOP_TMP guard, `declare -a` revert. (Final regenerate over the committed tree recorded in §Devloop Verification at commit time.)

### Findings raised during review and their resolutions:

| Reviewer | Finding | Resolution |
|----------|---------|------------|
| operations (paired) | Producer/hook EXIT-trap, exclusion faithfulness (both directions), pure slug, NUL/LC_ALL=C/blob-first signature, atomic write, runbook §8.5, ci.yml comment-only | Verified clean against checklist. |
| operations / team-lead | bash-3 wedge: file-scope `exit 2` version tripwire would kill the hook on every commit on a bash-3 box | FIXED (§Issues 5): version check is now function-local in `gate2_require_bash4`, `return 2` not `exit`; preamble side-effect-free (proven). |
| code-reviewer | Version floor is wrong — `mapfile -d` is bash **4.4**, not 4.0; a 4.0–4.3 box would pass the guard then fail at runtime | FIXED: `gate2_require_bash4` now checks `major<4 OR (major==4 AND minor<4)`; message says ">= 4.4". Verified 4.0/4.3 reject, 4.4+ accept. |
| code-reviewer | `__gate2_strict` was contained only because callers use subshells — fragile if a future edit calls a strict fn directly | FIXED: replaced the helper with inline `local -; set -euo pipefail` (bash 4.4 function-local options, auto-restored) at each strict fn. Verified a DIRECT call leaves the caller's options unchanged. |
| code-reviewer | OID-equality comment claimed "no .gitattributes" but the repo has one | FIXED (comment): clarified the repo's `.gitattributes` declares only `merge=union` (merge driver, not content-altering) on two paths that are excluded from the binding anyway — no content-altering attribute applies to any bound path. |
| security (HIGH, MUST-FIX) | Staged deletions corrupt/truncate the signature stream → integrity bypass | FIXED (§Issues 7): deletions enumerated via `--diff-filter=D` + bound as a first-class `DELETED <path>` record (identical both sides); `gate2_blob_staged` uses `--verify --quiet` (no `:path` echo); non-deletion resolution failures fail closed; FILE-lines + drift derive from the records stream. Guards `case_l`, `case_m`. |
| code-reviewer | `DEVLOOP_TMP` misconfigured inside the work tree would land the verdict in the repo | FIXED (§Issues 8): `emit_gate2_verdict` refuses to write (WARN, return 2) when the verdict dir is inside `git rev-parse --show-toplevel`. Verified. |
| dry-reviewer | devloop-main.md predicate + slug extraction duplicated across two fns | FIXED (§Issues 9): extracted shared `gate2_is_devloop_mainmd` + `gate2_mainmd_slug`. Also added a TODO.md entry tracking the Bash↔Rust exclusion/slug parity (ADR-0019 DRY exception). |
| test | verification-matrix case count stale (said 9) | FIXED: §verification matrix now says 14; the g–n regression guards enumerated. Self-test count is computed dynamically. |
| code-reviewer (Finding 3) | gratuitous `declare -A` on the layer arrays in `layer-all.sh` (unspecified-iteration-order footgun) | FIXED: `layer-all.sh:33` reverted `declare -A`→`declare -a` (indexed). Emitter namerefs + `${arr[$n]}` (n in 1..7) work identically; verified emit + LAYER 4 line correct with indexed arrays. |
| operations / code-reviewer | source-safety: make `local -; set -euo pipefail` containment intrinsic (not subshell-incidental) | FIXED (§Issues 6b): inlined into each strict fn (not a separate helper — the helper form would be a no-op, per code-reviewer's own correction); bash floor raised to ≥4.4 (`local -` is 4.4+). Regression guard `case_n` (direct call restores `-u` OFF; RED if `local -` removed). |

(Updated as further reviewer findings land.)

---

## Accepted Deferrals

- `docs/TODO.md` §Cross-Service Duplication (DRY) — Bash↔Rust exclusion/slug parity CI guard (ADR-0019 extraction opportunity, not a finding-deferral)

---

## Human Review (Iteration 2 — `--light --continue`)

**Feedback**: Post-commit (`e370340`), security reconciled the deletion finding to **HIGH**
(reproduced end-to-end signature-void bypass from the verbatim pre-fix staged-index blob:
a deleted file sorting BEFORE a tampered file truncates the record stream to the empty-hash,
matching both sides → rc=0 ALLOW; bash 5.2.15). The committed fix is correct and unchanged.
Two follow-ups in this iteration: (1) cosmetic — `_gate2_binding.sh:641` WARN says "bash < 4.0"
but the guard floor is 4.4; (2) add a dedicated end-to-end adversarial-ordering regression case
(deleted file sorts before a tampered file → `gate2_validate_commit` MUST BLOCK + name the
tampered file) pinning the HIGH bypass at the validation scope (complements `case_l`'s content
pin). The severity record was already corrected by the Lead in `392cadd`. **Also a deliberate
meta-test: confirm a `--continue` commit is correctly gated by the new hook.**

### Iteration-2 result — RESOLVED-FIXED (both reviewers)

- **Test: RESOLVED-FIXED**, **Security: RESOLVED-FIXED** (`--light`). Both independently flagged
  the same efficacy gap — the first `case_o` was a black-box near-duplicate of `case_m` (the
  post-emit tamper trips the signature on its own, so the deleted-sorts-first *ordering* wasn't
  load-bearing; a faithful `DELETED`-mechanism revert left it GREEN). Fixed (option a): `case_o`
  now asserts the recomputed **record stream** is exactly `{DELETED aaa_del.rs, <staged-tampered-blob> zzz_keep.rs}`
  (set-compare) BEFORE the block check — pinning "a head deletion doesn't truncate the tail," the
  actual bypass shape. Verified mechanism-sensitive: the revert now turns `case_o` RED alongside
  `case_l`; `case_m` stays green. The HIGH bypass is pinned by BOTH `case_l` (record content) and
  `case_o` (deleted-sorts-first ordering).
- Deliverables: (1) `_gate2_binding.sh:641` WARN `< 4.0`→`< 4.4`; (2) `case_o` (self-test 14→15).
- **`--continue` validation finding (the meta-test result)**: the iteration-1 Cross-Boundary table
  over-listed 4 prior-iteration files not in this commit's diff → the Layer-A scope-drift guard
  (`validate-cross-boundary-scope`) correctly flagged them and `GATE2=FAIL` blocked the commit until
  the CB table was scoped to iteration-2's changeset. So a `--continue` IS properly gated — and it
  surfaced that a multi-iteration devloop must scope its CB table per-iteration (or the scope-drift
  guard be made `--continue`-aware). Candidate follow-up; noted here, not actioned this iteration.

---

## Rollback Procedure

1. Start commit: `e6b91d157dd39ff43067a595826db574b1313e4c`
2. Review: `git diff e6b91d1..HEAD`
3. Soft reset: `git reset --soft e6b91d1` · Hard reset: `git reset --hard e6b91d1`
4. Hook rollback note: if the new `.githooks/pre-commit` logic wedges commits, `git commit --no-verify` is the escape hatch while reverting (document this in the runbook touch).

---

## Issues Encountered & Resolutions

1. **Slug derived empty (producer)** — `emit_gate2_verdict` first derived the slug from
   `gate2_changeset_worktree`, which has ALREADY applied exclusions; since the active main.md
   lives under the excluded `docs/devloop-outputs/**`, the slug was always empty. Fixed: derive
   the slug from the RAW worktree changeset (pre-exclusion), mirroring the hook's
   `gate2_staged_complete_slug` (which reads the unfiltered staged name list). Added regression
   guard `case_i` to the self-test (asserts SLUG == staged slug end-to-end). The empty slug was
   non-fatal (validation falls back to signature-only), but it defeated the diagnostic slug-match.

2. **`IFS=$'\n\t'` broke space-splitting** — two helpers (`gate2_signature` stripping the
   sha256sum `<hex>  -` suffix; `gate2_layer4_counts` reading awk output) initially relied on
   `read` space-splitting, which fails because the file pins `IFS` to `$'\n\t'`. Fixed with
   parameter expansion (`${out%% *}`) and newline-delimited `key=value` awk output respectively.

2b. **Hook didn't fail-closed on >1 staged complete main.md** (caught by @operations at review).
   `gate2_staged_complete_slug` originally picked the FIRST slug on a multi-match and proceeded to
   signature validation. Per the design's FAIL-CLOSED rule (and the Rust `bail!` analogue), the
   hook context must BLOCK on ambiguity — exactly-one is the devloop-commit invariant. Fixed:
   the function now returns rc 2 on >1, and `gate2_validate_commit` hard-blocks (rc 2 → block; any
   other nonzero from the trigger eval → block). The PRODUCER stays asymmetric — its pure
   `gate2_derive_slug` still returns empty + carries on (never aborts the pipeline over a
   diagnostic field). Added regression guard `case_k`.

3. **Self-test counters in a subshell** — `ok`/`bad` incremented counters inside the per-case
   subshell, so totals stayed 0. Fixed: each case returns its pass/fail via exit code; the parent
   counts. (The STATUS line was accidentally correct, but the displayed totals were wrong.)

4. **Self-test placement for auto-discovery** — `run-guards.sh` only discovers `simple/**/*.sh`.
   Moved the self-test into `scripts/guards/simple/` (and fixed its repo-root path to 3-levels-up)
   so it runs automatically every devloop + CI per @test, rather than needing a separate wiring.

5. **Library preamble was not source-safe for the hook** (caught by @operations source-hygiene
   review). `_gate2_binding.sh` originally had file-scope `set -euo pipefail` + a hard `exit 2`
   bash-version guard. Sourced into the pre-commit hook (plain `#!/bin/bash`, `set -e`, runs on
   EVERY commit), those would (a) leak `-u`/pipefail into the hook body and (b) `exit 2` the whole
   hook on a bash-3 box — wedging ALL commits, not just devloop ones. Fixed: removed file-scope
   `set`/`IFS`; each subshell-contained git/signature fn sets strict mode locally via
   `__gate2_strict` (contained because they run in `$( )`/`<( )`); `gate2_validate_commit` (called
   directly, not in a subshell) stays correct under ambient `set -e` alone and never mutates the
   hook's options. The bash-4 check is now `gate2_require_bash4` (returns 2, never exits); the hook
   path WARNs + skips on bash<4 (CI still enforces) rather than wedging commits. Verified: sourcing
   into a `set -e`-only shell leaks neither `-u` nor `pipefail`, and a non-devloop commit no-ops.
   (Mechanism further refined in item 6 — the `__gate2_strict` helper was replaced with inline
   `local -` function-local options.)

6. **Review-phase hardening of the source-safety mechanism** (code-reviewer findings):
   (a) **Version floor was wrong — 4.0 vs 4.4.** `mapfile -d` (used by both producer and hook)
   landed in bash 4.4, but the guard checked `>= 4.0`; a 4.0–4.3 box would pass then fail at runtime.
   Fixed: `gate2_require_bash4` now checks `major<4 OR (major==4 AND minor<4)`. Verified 4.0/4.3
   reject, 4.4+ accept.
   (b) **`__gate2_strict` was only contained by caller convention.** Replaced the helper with inline
   `local -; set -euo pipefail` (bash 4.4 function-local options, auto-restored on return) at each
   strict fn, so options are contained regardless of whether the caller uses a subshell. Verified a
   DIRECT (non-subshell) call leaves the caller's options unchanged. Added a scope comment that
   `local -` covers shell options only, NOT IFS (fine — strict fns use command-local `IFS= read`, no
   function-scope IFS change). Regression guard `case_n` (operations): from a `set +u` context, a
   direct strict-fn call must restore `-u` OFF — RED if `local -` is ever removed, so the
   source-safety property is self-enforcing rather than convention-guarded.
   (c) **OID-equality `.gitattributes` comment** clarified: the repo's `.gitattributes` has only
   `merge=union` (a merge driver, not a content-altering clean/smudge filter) on two binding-excluded
   paths, so the `hash-object == rev-parse :path` assumption holds for every bound path.

7. **HIGH integrity bypass — staged deletions VOID the signature** (security finding, MUST-FIX;
   confirmed real by faithful pre-fix tracing). A staged deletion of a tracked file has no blob, and
   in the SHIPPED pre-fix form the failing `blob="$(git rev-parse :path)"` inside the records
   `while read` loop, under `local -; set -euo pipefail`, ABORTS the record subshell mid-stream — so
   `gate2_records_*` emits an EMPTY (truncated) stream and the SIGNATURE becomes the sha256 of empty
   input (`e3b0c442…`) on BOTH producer and hook identically. The PRIMARY integrity field is thus
   voided: any deletion in the changeset zeroes the signature binding on both sides → they match → a
   post-validation tamper on another file is unbound by the signature. (An EARLIER analysis of mine
   wrongly concluded "garbage record, no truncation, spurious over-block not a bypass" — that used a
   non-faithful reconstruction; the faithful shipped form DOES truncate the signature to the empty
   hash. In this env a SEPARATE incidental backstop — the FILE-line set-diff drift check — still
   caught the specific tamper I tried, but relying on that is unacceptable: the load-bearing signature
   was void. Security's end-to-end repro of a clean rc=0 ALLOW stands as the authoritative severity.)
   Fixed, enforcing **a blob-resolution failure must never silently truncate/void the stream**:
   (i) deletions are enumerated separately (`git diff --diff-filter=D`) and bound as a first-class
   `DELETED <path>` record, byte-identical on producer and hook (bind the deletion FACT — no blob),
   so the signature is non-empty and CHANGES when a file is deleted (verified: `5b1cd9e0…` not the
   empty hash); (ii) `gate2_blob_staged` uses `git rev-parse --verify --quiet :path` (nothing +
   nonzero on a missing entry, never the `:path` echo) — note `:path^{blob}` does NOT work (git parses
   it as a pathspec); (iii) any blob-resolution failure on a NON-deleted path aborts the whole record
   stream (fail-closed) rather than emitting a short/void record; (iv) the human FILE lines and the
   drift diagnostic derive from the SAME records stream, single-sourced. Regression guards: `case_l`
   asserts the RECORD CONTENT (a clean `DELETED <path>`, no garbage/empty token) — RED pre-fix
   (`:gone.rs gone.rs`), GREEN post-fix; `case_m` (deletion + post-emit tamper → block, names the
   tampered file) green. (A black-box pass/block check alone can't distinguish here because the
   incidental FILE-line drift backstop also blocks — hence case_l asserts record content directly.)

8. **DEVLOOP_TMP-inside-repo guard** (code-reviewer). If `DEVLOOP_TMP` is misconfigured to a path
   inside the work tree, the verdict would land in the repo (polluting the changeset/binding, or
   getting committed). `emit_gate2_verdict` now refuses to write (loud WARN, returns 2) when the
   verdict dir resolves inside `git rev-parse --show-toplevel`. Verified: inside-repo → no write;
   `/tmp` → writes normally.

9. **Slug-idiom extraction** (dry-reviewer). The devloop-main.md predicate + slug extraction were
   duplicated in `gate2_derive_slug` and `gate2_staged_complete_slug`. Extracted to shared
   `gate2_is_devloop_mainmd` + `gate2_mainmd_slug` so the two callers can't drift.

---

## Lessons Learned

- **Exclusions must be applied at the right layer.** The slug bug came from reusing the
  post-exclusion changeset for a purpose (slug derivation) that needs the pre-exclusion view. When
  a single exclusion set serves multiple consumers, be explicit about which consumers want the
  filtered vs raw list — the binding wants filtered, slug-derivation wants raw.
- **`IFS=$'\n\t'` (a good safety default) silently changes `read` field-splitting.** Any `read`
  that expects space-separated fields under that IFS needs an explicit `IFS=' '` or a different
  parsing approach. Prefer parameter expansion for fixed-shape tokens.
- **bash `case` pathname globs match `/`.** Single-segment exclusions (`*/INDEX.md`, `*.md`) MUST
  be explicit prefix-strip + no-embedded-slash checks; a `case` glob would silently over-exclude
  nested paths — a correctness hole, caught pre-emptively by security/operations at planning.
- **Test the bootstrapping gate in isolation FIRST.** Proving the validator green against
  synthetic temp repos before wiring the real hook meant a logic bug could never wedge this very
  commit — and the self-test (`case_i`) caught the slug regression that the matrix alone missed.
