# Devloop Output: Gate-2 pre-commit hook: skip verdict for byte-identical devloop replays

**Date**: 2026-10-03
**Task**: While git is replaying a cherry-pick or rebase (CHERRY_PICK_HEAD / REBASE_HEAD; merges excluded by user ruling 2026-10-03), skip the Gate-2 verdict requirement for a staged devloop main.md only if byte-identical to the replayed commit's blob and that commit carries exactly one `Devloop:` trailer equal to the slug
**Specialist**: infrastructure (paired: security)
**Mode**: Agent Teams (v2) — full, Gate-1 present (--paired-with=security)
**Branch**: `feature/0a-gate2-replay-hook`
**Duration**: ~14h wall-clock (incl. ~11h stall); ~2.5h active

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `cf0fcd5c2d389af514de98604dab18bf03c9d819` |
| Branch | `feature/0a-gate2-replay-hook` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `infrastructure` |
| Tier | `full` |
| Iteration | `1` |
| Security | `paired-security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |

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
Stop the Gate-2 pre-commit hook from requiring a fresh verdict when a conflict-resolved cherry-pick or rebase faithfully re-stages an already-validated devloop main.md (absorb-devloop, rebase onto main), without opening any other skip path.

### Scope
- **Service(s)**: none — dev tooling only (pre-commit hook library, selftest, absorb tooling, runbook)
- **Schema**: No
- **Cross-cutting**: Yes — affects every devloop commit path that goes through `.githooks/pre-commit`

### Debate Decision
NOT NEEDED - narrow change to an existing control; scope rulings (merge arm dropped, CI backstop) made by the user during Gate 1.

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
| `scripts/lang/_gate2_binding.sh` | Mine | — |
| `scripts/guards/simple/selftest-gate2-verdict.sh` | Mine | — |
| `infra/devloop/absorb-devloop.sh` | Mine | — |
| `.claude/skills/absorb-devloop/SKILL.md` | Mine | — |
| `docs/runbooks/devloop-validation.md` | Not mine, Minor-judgment | operations |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Mine | — |
| `docs/TODO.md` | Mine | — |
| `.githooks/pre-commit` (comment only) | Mine | — |

---

## Planning

> **Scope ruling (user, via Lead, Gate 1):** cherry-pick and rebase only. The merge arm from the
> original brief and security R4/R5's merge rule are dropped. With `MERGE_HEAD` present the hook
> keeps today's behaviour (verdict required) and says why. The runbook says to rebase, not merge,
> branches that carry devloop records.

### Mechanism restatement

The Gate-2 hook is triggered by a *marker* (a staged devloop main.md at Phase=complete), not by
evidence of new work. A replay (conflict-resolved cherry-pick, a manual `git commit` during a
stopped rebase) faithfully re-stages a marker that was already gated when the replayed commit was
made, so the gate re-fires with no new evidence available to satisfy it. The wider class is
"marker-triggered commit-time checks that re-fire on replay". Siblings in `.githooks/pre-commit`:
the TBD/pending main.md check and `dt-guard todo-tracking` are content checks (a byte-identical
main.md passes them again); cargo fmt/clippy are tree-content checks whose failure on a replayed
tree is a real defect. Gate-2 is the only marker-triggered authority check, so the class has no
other same-owner member to fix here.

### What the verdict binds, and what the skip relaxes (precise)

- Trigger conjunct 1: exactly one staged complete main.md (>1 ⇒ ambiguity block). Conjunct 2:
  the staged changeset (index vs HEAD) minus exclusions is non-empty.
- When triggered, the verdict's SIGNATURE binds every staged non-excluded path's blob (and
  deletions). main.md itself is excluded from the binding; it only *triggers*.
- The replay skip acts **only on conjunct 1**: a staged complete main.md that *qualifies* (below)
  is removed from the trigger set. The existing logic then runs unchanged on the remaining set:
  0 left ⇒ no verdict required; 1 left ⇒ verdict required and bound over the *whole* staged
  changeset as today; >1 left ⇒ ambiguity block. A mixed commit therefore still needs a verdict.
- Consequence, stated plainly (the **user's decision**): when every staged complete main.md
  qualifies, **the whole commit** needs no local verdict, including the non-excluded files the
  conflict resolution changed — exactly the bytes nobody validated locally. Accepted because the
  local hook is anti-drift, not a security boundary (runbook §8.5 threat model). CI re-runs
  layer-all.sh from scratch only where `.github/workflows/ci.yml`'s `on:` block triggers, so the
  skip line and the runbook point at that rather than claiming unconditional coverage. The audit
  line counts the bound paths that ride along unvalidated.

### Qualification predicate — every step fail-closed

`gate2_replay_source` (once per commit) then `gate2_replay_qualifies <path>` (per main.md). A
staged complete main.md `<path>` (slug `<s>`) qualifies iff ALL hold; any git error, empty
output, or unexpected shape at any step ⇒ "does not qualify" (today's behaviour), never "skip":

1. **No merge in progress — checked FIRST, positively.** `git rev-parse --verify --quiet
   MERGE_HEAD` resolves ⇒ no skip (`[merge-not-vouched]`), whatever else is present, so a merge
   stop (incl. `rebase -r` stopped on a merge: REBASE_HEAD + MERGE_HEAD) can never fall through to
   a cherry-pick/rebase qualification. No octopus logic (all merges refuse; no `--git-path` read).
   One exclusion comment at `gate2_replay_source` covers both MERGE_HEAD (no single replayed
   commit; §8.5) and REVERT_HEAD (stages the pre-image).
2. **Replay state.** Exactly one of `CHERRY_PICK_HEAD`, `REBASE_HEAD` resolves via
   `git rev-parse --verify --quiet <REF>^{commit}` — through git, so linked worktrees,
   per-worktree pseudorefs and a reftable backend all work; never a hand-built `.git/<REF>`.
   Both present ⇒ no skip (`[multiple-replay-states]`). `^{commit}` rejects garbage and tree
   OIDs (security-verified). **The source commit is `<REF>^{commit}` itself** — no ancestor walk,
   so an ancestor's trailer can never vouch for the commit actually being replayed.
3. **Not a branch spoof.** `git rev-parse --symbolic-full-name <REF>` must print exactly `<REF>`
   (verified on git 2.39: a branch named `CHERRY_PICK_HEAD` otherwise resolves via refs/heads/).
4. **Byte-identical, same path.** staged `git rev-parse --verify --quiet ":<path>"` ==
   `git rev-parse --verify --quiet "<REF>:<path>"` (OID equality ⇒ byte equality). Path absent
   in the replayed commit's tree ⇒ no skip; content at a different slug path never matches.
   Per the user's rule, REF need not itself have *changed* main.md: a trailer-matching replayed
   commit whose tree holds a byte-identical main.md qualifies (pinned by q7). That commit already
   carries the completion record for `<s>`; requiring a touch would be mechanism the rule doesn't
   ask for.
5. **Trailer.** `gate2_commit_devloop_trailer <REF>`: `git log -1 --format=%B <REF>` then
   `git interpret-trailers --parse`, two separately rc-checked steps. Exactly one trailer with
   key exactly `Devloop` (case-sensitive), value whitespace-trimmed, equal to `<s>`. A `Devloop:`
   line in a body paragraph is not a trailer (verified). Header comment notes `interpret-trailers`
   honours local `trailer.*` config — same actor as `--no-verify`, inside the threat model.

Forgery note (@paired-security R7, in the header comment): a hand-written pseudoref can only
replay a main.md byte-identical, at the same path, to one already in a real commit carrying
`Devloop: <slug>` — it cannot mint a new completion record. Strictly weaker than `--no-verify` or a
hand-authored PASS verdict, both of which already exist.

### Outcomes and diagnostics (one prefix: `Gate-2:`)

Both "not eligible" and "git error" fall back to **today's verdict requirement** (never "skip";
security, code-reviewer and observability agreed). They differ in the message, not the rc. Git
errors keep git's own stderr on the failing step (no `2>/dev/null` there) plus step name and rc.
All lines are written with `printf '%s'` arguments — path/slug/trailer text never reaches a format
string; a mismatched trailer value is stripped of control characters and length-capped.

- **Skip fires** (one line per main.md):
  `⚠️  Gate-2: replay skip: <path> blob <oid> == <REF> <ref-full-sha> (Devloop: <slug>); Gate-2 verdict NOT checked for this commit — <N> bound path(s) not validated locally (<M> differ from the replayed commit) — run ./scripts/layer-all.sh to validate, or rely on CI (.github/workflows/ci.yml) where it runs.`
  Terminal-only; the runbook says to keep the replay output if the record matters.
- **Replay detected, skip refused** (nothing new prints when no replay/merge is in progress),
  immediately above the existing ❌ block (cause, then consequence):
  `⚠️  Gate-2: replay skip not applied to <path> (<REF> <short-sha>): [<token>] <reason>`.
  Tokens: `merge-not-vouched`, `multiple-replay-states`, `ref-not-pseudoref`,
  `main-md-modified` (staged <oid>, replayed <oid>), `path-absent-in-replayed-commit`,
  `no-trailer`, `duplicate-trailer`, `trailer-slug-mismatch`, `git-error` (step, rc).
  Then one action line pointing at runbook §8.5: restore main.md from the replayed commit
  (`git checkout <REF> -- <path>`), or `./scripts/layer-all.sh && git add -A` then resume
  (`git cherry-pick --continue` / `git commit` / rerun the absorb); for merges, rebase instead or
  bypass knowingly. `--no-verify` stays the user's call.
- **Merge refusal action** (operations): `merges of devloop records are not vouched for (runbook
  §8.5) — git merge --abort (discards the in-progress resolution), then rebase (or rerun
  absorb-devloop.sh); or git commit --no-verify if you accept CI as the only check (your decision).`

### Code shape

- New fns `gate2_replay_source`, `gate2_replay_qualifies <path>`, `gate2_commit_devloop_trailer
  <sha>` live hook-side next to the trigger fn; the producer never calls them. Signature, records,
  exclusion set and conjunct 2 are untouched (producer/hook symmetry, :283).
- `gate2_staged_complete_slug` is renamed `gate2_staged_trigger_slug` (callers: this file, the
  selftest comment, the runbook). It already holds the staged path and does the one Phase parse;
  it filters each complete candidate through `gate2_replay_qualifies` before the ambiguity count,
  slug via `gate2_mainmd_slug` (@dry-reviewer). Its header documents the replay filter, the new
  rc 3, and that skip/refusal lines go to stderr because it runs inside `$(…)` (stdout = slug).
- Trigger enumeration uses `git diff --cached -z --name-only --diff-filter=d`: a staged deletion
  can't be at Phase=complete, and without it the rc-3 hardening below would hard-block every
  `git rm` of a main.md (`git show :<deleted>` is rc 128) — @paired-security.
- Hardening (fix-don't-defer): today `staged="$(git show ":$f" … || true)"` turns a failed read of
  a PRESENT staged main.md into a silent no-op. Now ⇒ rc 3 ⇒ the caller's existing "error
  evaluating the commit trigger" block.
- Explicit rc checks throughout: `set -e` is inert in an `if`-called predicate, so fail-closed
  comes only from `local x; x="$(git …)" || …` (never `local x="$(…)"`). No strict mode in
  hook-called fns.
- Bash: new code runs only after `gate2_require_bash4`; nothing parse-incompatible with 3.2 at
  file scope.
- Header THREAT MODEL: replay relaxation as the user's decision, R7 forgery note, merge exclusion,
  REVERT_HEAD exclusion.
- Which operations run pre-commit (git 2.39, verified): `cherry-pick --continue` and `git commit`
  during a cherry-pick stop — yes (CHERRY_PICK_HEAD); `git commit` at a rebase stop — yes
  (REBASE_HEAD); `git rebase --continue` — **no** (sequencer passes -n), so Gate-2 is not checked
  locally on that path regardless of this change; `git commit` concluding a conflicted merge —
  yes (MERGE_HEAD ⇒ no skip).

### Tests (`scripts/guards/simple/selftest-gate2-verdict.sh`, real git state, no verdict file)

Harness hardening (@test): `with_temp_repo` exports `GIT_CONFIG_GLOBAL=/dev/null`,
`GIT_CONFIG_NOSYSTEM=1`, `GIT_EDITOR=true`, `GIT_SEQUENCE_EDITOR=true` and sets
`init.defaultBranch`, so a user's hooksPath, rebase config or editor can't leak in or hang.

Fixture (built from `with_temp_repo` / `mk_main_md` / `seed_commit`): a devloop branch whose
commit adds `docs/devloop-outputs/<s>/main.md` (complete) + `src.rs` with trailer `Devloop: <s>`;
a target branch that conflicts on `src.rs`. Every replay case first asserts the op exited non-zero
and the REF really exists (own setup token, so a conflict that never happened can't pass a case
vacuously). Positive cases have NO verdict file and a staged bound file, so they fail if the replay
check is removed. Negative cases assert their specific token and the absence of a skip line, not
just rc≠0 (rc=1 is the no-verdict baseline). Audit-line fields come from the fixture's own
`git rev-parse`, never literals.

| Case | Setup | Expect |
|------|-------|--------|
| q1 cherry-pick, allow despite hand-edited resolution | conflicted `git cherry-pick`, hand-resolve src.rs | allow; audit line has path, blob, CHERRY_PICK_HEAD + full sha == devloop commit, slug; N == bound staged count; M == `git diff --name-only <REF> -- <bound>` count, M ≥ 1 (pins the user's whole-commit decision) |
| q2 rebase | conflicted `git rebase`, resolve, `git commit` at the stop | allow; REBASE_HEAD in audit line |
| q3 linked worktree | q1 inside a `git worktree add` checkout (`.git` is a file) | allow |
| q4 e2e allow | real `GIT_EDITOR=true git cherry-pick --continue` (as absorb-devloop.sh does) through a temp `core.hooksPath` shim sourcing the lib | commit lands |
| q5 e2e refuse | same with main.md edited | `--continue` rc≠0, CHERRY_PICK_HEAD still present, HEAD unchanged |
| q6 trailer whitespace | `Devloop: <s>  ` | allow (value trimmed) |
| q7 trailer commit didn't touch main.md | replayed commit carries `Devloop: <s>`, changes only src.rs; main.md hand-staged identical to its tree | allow (user's rule; see step 4) |
| r1 edited main.md | cherry-pick, edit main.md | block; `[main-md-modified]`; ⚠️ line directly precedes the ❌ block |
| r2 no trailer | replayed commit lacks `Devloop:` | block; `[no-trailer]` |
| r3 wrong trailer | `Devloop: other-slug` | block; `[trailer-slug-mismatch]` |
| r4 body-only trailer | `Devloop:` in a body paragraph | block; `[no-trailer]` |
| r5 no replay state | same staged tree, ordinary commit | block; NO replay line of either kind |
| r6 merge (would otherwise qualify) | conflicted `git merge` of the devloop branch; main.md identical to MERGE_HEAD's tree; MERGE_HEAD tip carries `Devloop: <s>` | block; `[merge-not-vouched]` + `git merge --abort` recovery line, no skip line |
| r6b rebase -r on a merge | conflicted cherry-pick state + MERGE_HEAD written via `git update-ref` (comment says why) | block; `[merge-not-vouched]` wins over everything else |
| r6c multiple replay states | conflicted cherry-pick + REBASE_HEAD written via `git update-ref` | block; `[multiple-replay-states]` |
| r7 branch spoof | branch named `CHERRY_PICK_HEAD`, nothing in progress | block; `[ref-not-pseudoref]`, no skip line |
| r8 forged garbage | `CHERRY_PICK_HEAD` file holds garbage / a tree OID | block; silence (`^{commit}` doesn't resolve ⇒ treated as no replay state), no skip line |
| r9 forged real commit | `CHERRY_PICK_HEAD` = a trailer-bearing commit with a different blob | block; `[main-md-modified]` |
| r10 mixed | replayed qualifying main.md + a second complete main.md staged by hand with edited content | block; one skip line for the first, one refusal for the second |
| r10b one commit, two main.md | cherry-picked commit carries two complete main.md, one `Devloop:` trailer | block; non-matching one refuses `[trailer-slug-mismatch]` |
| r11 other slug path | identical content at a different slug's path | block; `[path-absent-in-replayed-commit]` |
| r12 git error, first + mid-check | shim fails `interpret-trailers`; variant fails `rev-parse <REF>:<path>` after REF resolution succeeded | block; `[git-error]` with step + rc AND the shim's own stderr text |
| r13 read failure | staged present main.md unreadable (`git show` shimmed to fail) | block via the existing trigger-error path (rc 3) |
| r14 duplicate trailer | two `Devloop: <s>` trailers | block; `[duplicate-trailer]` |
| r15 staged deletion | `git rm` a complete main.md + a bound file, no verdict | allow (today's behaviour; no rc-3 block) |
| r16 ancestor trailer only | A (`Devloop: <s>`) adds main.md; B (no trailer) on top touches src.rs; cherry-pick B with conflict, hand-stage main.md identical to B's tree | block; `[no-trailer]` (regression guard: the dropped ancestor-walk would have allowed it) |
| r17 rebase stopped on a later commit | rebase replays devloop commit + later non-trailer commit, stops on the later one; main.md staged identical | block; `[no-trailer]` |
| r18 mismatched slug + whitespace | `Devloop: <s>x  ` | block; `[trailer-slug-mismatch]` (trim can't create a match) |

The `for c in …` loop grows by exactly the number of new cases; the before/after Layer-3 pass count
is quoted in this file.

### Docs / tooling

- `docs/runbooks/devloop-validation.md` §8.5 (doc SSoT, names the functions): exception paragraph
  under the trigger; a "Replays" subsection (predicate, what is relaxed and that it is the user's
  decision, skip/refusal lines, reconstruct recipe `git log -1 --format=%B <sha> | git
  interpret-trailers --parse` + `git rev-parse <sha>:<path>`, which operations run pre-commit incl.
  `rebase --continue` running none, **rebase — don't merge — branches carrying devloop records**
  (stated in full only here; everything else points at it), stated alongside the fact that
  `git rebase --continue` runs no pre-commit hook so Gate-2 on that recommended path is CI-only
  and REBASE_HEAD only matters for a manual `git commit` at a stop; the merge recovery
  (`git merge --abort` discards the in-progress resolution); absorb's `git merge --ff-only` path
  makes no commit and runs no hook, so it is unaffected;
  no new state and no kill switch, `--no-verify` remains the escape hatch, CI coverage per
  ci.yml's `on:` block); failure-table rows per refusal token; matching rows in the §8
  troubleshooting index (~:903).
- `infra/devloop/absorb-devloop.sh` (:187, :193) + `.claude/skills/absorb-devloop/SKILL.md`: point
  at §8.5 instead of restating the rule — an unedited devloop replay is skipped automatically; a
  refusal names its reason; keep the one actionable hint (`--theirs` for `docs/devloop-outputs/**`
  keeps main.md byte-identical); else `./scripts/layer-all.sh && git add -A` and rerun;
  `--no-verify` is the user's call.
- `docs/TODO.md` "absorb-devloop.sh cannot resume, and its own hand-off trips the Gate-2
  pre-commit hook": mark resolved — (a) shipped earlier (`resume_interrupted_absorb`), (b) by this
  devloop; the hooks-off remedy it prescribed is superseded.
- `docs/specialist-knowledge/infrastructure/INDEX.md`: Gate-2 line names
  `gate2_replay_source()`, `gate2_replay_qualifies()`, `gate2_commit_devloop_trailer()`,
  `gate2_staged_trigger_slug()`.

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed (re-confirmed after merge-arm drop) |
| Test | confirmed (re-confirmed) |
| Observability | confirmed (re-confirmed) |
| Code Quality | confirmed (re-confirmed) |
| DRY | confirmed (re-confirmed) |
| Operations | confirmed (re-confirmed) |

Lead rulings: (1) user dropped the merge arm — MERGE_HEAD ⇒ no skip (`[merge-not-vouched]`); source = replayed commit itself. (2) CI "gap" (ci.yml `branches: [main, develop]`) is not a gap — the filter matches the PR base branch; no ci.yml change. (3) q7 (trailer-matching replayed commit that did not itself change main.md) stays allow — matches the user's rule as written. Classification-sanity guard: `STATUS=OK`.

---

## Pre-Work

None. (`dt-guard`/`dt-story` release binaries built so the Gate-1 classification guard could run.)

---

## Implementation Summary

### Replay skip (hook side, `scripts/lang/_gate2_binding.sh`)
| Item | Before | After |
|------|--------|-------|
| Trigger fn | `gate2_staged_complete_slug` | `gate2_staged_trigger_slug`: same contract, filters vouched replays before the ambiguity count; rc 3 on an unreadable present main.md (was `|| true` ⇒ silent no-op); enumerates with `--diff-filter=d` |
| Replay state | — | `gate2_replay_source`: MERGE_HEAD first (file or ref ⇒ `[merge-not-vouched]`), exactly one of CHERRY_PICK_HEAD/REBASE_HEAD via `rev-parse --verify --quiet <REF>^{commit}` (rc 1 = absent, other rc = `[git-error]`), `--symbolic-full-name` spoof check |
| Per-main.md vouch | — | `gate2_replay_qualifies`: staged OID == `<REF>:<path>` OID, exactly one `Devloop:` trailer == slug; skip line with N/M counts |
| Trailer parse | — | `gate2_commit_devloop_trailer`: `git log -1 --format=%B` → `git interpret-trailers --parse`, exact key, trimmed values |
| Output safety | — | all diagnostics via `printf '%s'` args; control bytes in path/slug/trailer shown as `?`; trailer value capped at 80 chars |

### Tests
`scripts/guards/simple/selftest-gate2-verdict.sh`: hermetic git env; 33 new cases (q1–q7 allow,
r1–r22 + r6b/r6c/r10b/r12c refuse/guard), real conflicted cherry-pick/rebase/merge state, one e2e allow +
one e2e refuse through a `core.hooksPath` shim. Layer-3 selftest count **16 → 49** passed.
Mutation check: forcing `gate2_replay_source` to return 1 fails 24 of the original 28 new cases (the
other 4 — r5, r8, r13, r15 — assert today's behaviour by design).

### Additional Changes
- Runbook §8.5: trigger exception, "Replays" subsection (predicate, the user's whole-commit
  decision, rebase-don't-merge, which operations run pre-commit, skip/refusal lines, reconstruct
  recipe, no new state), seven failure-table rows, two troubleshooting-index rows.
- `absorb-devloop.sh` refusal messages + absorb SKILL point at §8.5 (`--theirs` keeps the skip).
- `docs/TODO.md` absorb/Gate-2 entry closed; hooks-off remedy recorded as superseded.
- INDEX pointer updated.
- Gate-3 fixes: every decision-bearing enumeration (trigger, conjunct 2, record streams,
  audit counts) goes through `__gate2_capture_nul` — a `< <(…)` read made a failing
  `git diff --cached` look like an empty changeset (fail-OPEN, pre-existing on the trigger and
  conjunct-2 paths); signature recompute/emit run under `pipefail`; per-token action lines;
  the "verdict NOT checked" line prints only when the whole commit is skipped; every git-error
  reason names its step and rc; CI wording aligned to "PRs into main/develop" at all sites.
  Follow-up (security S2): running the record streams inside `if !` made errexit inert, so
  blob-resolution and `gate2_signature` failures are now explicit `|| return 1`, and the
  producer's changeset group chains with `&&` (r20). Round 2: the emitter's raw slug
  enumeration also goes through `__gate2_capture_nul` (warn + no verdict on failure); the
  trigger fn's reporting moved into `__gate2_replay_report_vouched` / `__gate2_replay_action_lines`;
  per-commit replay state reset at entry (r22); a git error with >1 complete main.md blocks
  via the trigger error, not the misleading ambiguity message (r21).

---

## Files Modified

```
 .claude/skills/absorb-devloop/SKILL.md            |   4 +-
 .githooks/pre-commit                              |   3 +-
 docs/TODO.md                                      |   4 +-
 docs/runbooks/devloop-validation.md               | 104 +++-
 docs/specialist-knowledge/infrastructure/INDEX.md |   2 +-
 infra/devloop/absorb-devloop.sh                   |   4 +-
 scripts/guards/simple/selftest-gate2-verdict.sh   | 648 +++++++++++++++++++++-
 scripts/lang/_gate2_binding.sh                    | 492 ++++++++++++++--
 8 files changed, 1199 insertions(+), 62 deletions(-)
```

### Key Changes by File
| File | Changes |
|------|---------|
| `scripts/lang/_gate2_binding.sh` | Replay-vouch predicate (cherry-pick/rebase only), trailer parser, `__gate2_capture_nul` fail-closed enumeration (fixes pre-existing fail-open), explicit rc checks, per-reason diagnostics |
| `scripts/guards/simple/selftest-gate2-verdict.sh` | Hermetic env; 33 new real-git-state cases incl. e2e hook shim; 16 → 49 |
| `docs/runbooks/devloop-validation.md` | §8.5 Replays subsection, rebase-don't-merge, host-validation note, failure-table/index rows, CI wording |
| `infra/devloop/absorb-devloop.sh` | Refusal message points at §8.5 instead of prescribing a bypass |
| `.claude/skills/absorb-devloop/SKILL.md` | Same; stop and ask the user on non-restorable refusals |
| `.githooks/pre-commit` | Comment-only CI wording |
| `docs/TODO.md` | Absorb/Gate-2 entry closed (hooks-off remedy superseded); renamed-fn reference |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Pointer update |

---

## Devloop Verification Steps

Gate 2 (`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, Lead, final tree): exit 0, `TOTAL_RESULT=N/A` (N/A = intentional gaps only).

| Layer | Result | Duration | Notes |
|-------|--------|----------|-------|
| 1 Compile | OK | 6s | |
| 2 Format | OK | 4s | |
| 3 Guards | OK | 143s | gate2 selftest 49/49 |
| 4 Test | N/A | 286s | rust/ts OK; proto has no test phase (`not-applicable-to-this-lang`) |
| 5 Lint | OK | 3s | |
| 6 Audit | N/A | 4s | `no-dep-changes`; buf breaking OK |
| 7 Env-tests | OK | 910s | dev-cluster + Rust env-tests + browser E2E |

Fast check (Lead, Step 6): exit 0, 546s.

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED-FIXED | 2 | 2 | 0 | S1 fail-open `< <(…)` enumeration (2 pre-existing sites + 1 new) → `__gate2_capture_nul`, r19; S2 errexit inert under `if !` → explicit rc checks, r20. Round-2 re-check: verdict stands. |
| Test | RESOLVED-FIXED | 3 | 3 | 0 | r12c replay-state git errors; q1 expectations from plain git; r8 asserts no-verdict block |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | rc-propagating audit counts; step+rc on every git-error; `unresolved` sha; CI wording "PRs into main/develop". Follow-up: misleading ambiguity msg → rc-3 trigger error (r21) |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | F1 per-reason action lines; F2 skip-line truthfulness; F3 MERGE_HEAD git-error refuses; F4/R1 helper extraction; R2 state reset (r22); R3 rc-3 contract comment |
| DRY | RESOLVED-FIXED | 2 | 2 | 0 | stale renamed-fn refs; emitter SLUG enumeration via `__gate2_capture_nul`. No extraction entries. |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | per-reason recovery line (no `git checkout` for spoof/no-trailer); host-side layer-all needs cluster helper — documented in hook/absorb/SKILL/§8.5/TODO |
| Semantic Guard | — (not spawned) | | | | No check surface in diff (shell hook + docs; checks.md surfaces are Rust/client) |

Selftest: 49/49 (was 16). Review→implementation iterations: 3 (fix round 1, round 2, R3 comment).

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `cf0fcd5c2d389af514de98604dab18bf03c9d819`
2. Review all changes: `git diff cf0fcd5c2d389af514de98604dab18bf03c9d819..HEAD`
3. Soft reset (preserves changes): `git reset --soft cf0fcd5c2d389af514de98604dab18bf03c9d819`
4. Hard reset (clean revert): `git reset --hard cf0fcd5c2d389af514de98604dab18bf03c9d819`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: the whole commit. Do not revert the `__gate2_capture_nul`/explicit-rc hardening without also reverting the replay skip — that hardening closes a pre-existing fail-open; reverting it alone reintroduces commits allowed on a failing `git diff --cached`. Reverting only the replay skip (keeping the hardening) is safe. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Issue 1: Merge arm in brief vs. story-branch reality
**Problem**: A literal "MERGE_HEAD tip carries the trailer" rule would vouch only the last devloop on a branch; security proposed a history walk.
**Resolution**: User dropped the merge arm entirely — merges refuse (`[merge-not-vouched]`), runbook says rebase, don't merge.

### Issue 2: Pre-existing fail-open in the Gate-2 hook
**Problem**: `< <(git diff --cached …)` swallowed git's exit status; a failing enumeration read as an empty changeset and the commit was ALLOWED with no verdict (security repro).
**Resolution**: All decision-bearing enumerations via rc-propagating `__gate2_capture_nul`; regression S2 (errexit inert under `if !`) fixed with explicit rc checks. Tests r19/r20.

### Issue 3: Devloop stalled ~11h after implementation
**Problem**: layer-fast (~9–12 min) exceeds the 600s foreground Bash cap, so the implementer backgrounded it and ended its turn; the completion did not wake the idle teammate and the Lead had no timer — "Ready for review" never came.
**Resolution**: Lead re-ran the fast check; implementer told to block on background runs. Skill fix to be made by the user in a separate session.

---

## Lessons Learned

1. A "CI re-validates" premise must be checked against the workflow triggers, and `pull_request.branches` filters on the BASE branch — get the semantics right before declaring a gap.
2. Process substitution and `if !` both silently neuter bash error handling; any authority-gate enumeration needs an explicit rc capture.
3. Long validation runs in a teammate must be awaited in-turn; an idle teammate is not reliably woken by background completion.

---

## Appendix: Verification Commands

```bash
DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh
DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh
./scripts/guards/simple/selftest-gate2-verdict.sh
./scripts/guards/simple/validate-cross-boundary-classification.sh docs/devloop-outputs/2026-10-03-gate2-replay-skip/main.md
```
