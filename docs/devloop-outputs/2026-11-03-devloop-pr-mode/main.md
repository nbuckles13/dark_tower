# Devloop Output: devloop.sh --pr <number> mode

**Date**: 2026-11-03
**Task**: Add `--pr <n>` mode to infra/devloop/devloop.sh to fix an existing same-repo GitHub PR in place and push back to its head branch
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/devloop-pr-mode`
**Duration**: ~3h (03:27–06:00 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a595bd87ca1d3465d4bff5b567c61383d238e20c` |
| Branch | `feature/devloop-pr-mode` |
| Lead Model | `claude-opus-5-5` |

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
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — diff (shell script arg handling, git/gh plumbing) touches no checks.md surface` |

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
Add a `--pr <number>` mode to `infra/devloop/devloop.sh` so a devloop can fix an existing GitHub PR in place (e.g. a red Dependabot PR) and push back to that PR's branch.

**Behaviour**
- `devloop.sh --pr <n> <slug>` looks up PR <n> with gh. The devloop clone is checked out on the PR's head branch at the PR's current head, with the PR's base branch as BASE_BRANCH. The clone, containers and cluster are named by <slug> as today.
- The exit menu detects the PR and offers to push updates to that PR's head branch. It never offers to create a new branch or PR.
- It fails loudly, before creating any clone or container, when: the PR doesn't exist or isn't open; the PR comes from a fork; `--pr` is combined with a positional base-branch argument; a clone for <slug> already exists on a different branch.
- Re-running `devloop.sh --pr <n> <slug>` re-attaches the same way a re-run does today.
- Existing invocations without `--pr` behave exactly as before.

**Design constraint**: reuse the existing PR-iteration paths (ensure_clone's remote-branch fetch, and the exit menu's open-PR detection and push), keyed on BRANCH_NAME=feature/<slug>. Generalise BRANCH_NAME/BASE_BRANCH from the PR instead of adding a parallel code path. A Dependabot head ref contains slashes (dependabot/cargo/jsonwebtoken-11.1.0) — check every place BRANCH_NAME is interpolated.

**Also**: the menu's "commits since base" listing runs `git log ${BASE_BRANCH}..HEAD` inside the clone, and the clone may have no local branch named BASE_BRANCH. Check whether that works in `--pr` mode and today's mode; fix it here if broken.

**Docs**: usage header + examples at top of devloop.sh, and `docs/runbooks/devloop-validation.md` if it documents invocations. Include the Dependabot caveat: once you push to a Dependabot branch it stops rebasing that PR, and `@dependabot rebase` / `@dependabot recreate` would discard your commits.

**Tests**: argument handling + fail-loud cases in a podman-free test following `infra/devloop/absorb-devloop.test.sh`. Keep `scripts/parse-whole-script.test.sh` passing.

**Manual test plan (operator, before PR)**: (1) from a linked worktree run `--pr <open same-repo PR> pr-test` — clone on head branch at head SHA; (2) trivial commit, exit, push via menu, commit appears on PR; (3) re-run re-attaches; (4) closed PR and `--pr 123 slug main` both fail before anything is created; (5) `--destroy pr-test`.

### Scope
- **Service(s)**: none — devloop tooling (`infra/devloop/`)
- **Schema**: No
- **Cross-cutting**: No

### Debate Decision
NOT NEEDED - single-script tooling change with a fully specified behaviour

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
| `infra/devloop/devloop.sh` | Mine | — |
| `infra/devloop/devloop-pr.test.sh` (new) | Mine | — |
| `scripts/layer3.sh` (wire the new self-test) | Mine | — |
| `docs/decisions/adr-0025-containerized-devloop.md` (§7 usage line) | Mine | — |
| `docs/specialist-knowledge/infrastructure/INDEX.md` (pointer to new test) | Mine | — |
| `infra/lib/clone-head.sh` (new; review S-R1: read a clone's HEAD as a file) | Mine | — |
| `infra/devloop/absorb-devloop.sh` (review S-R1: branch via clone-head.sh, not git in the worktree) | Mine | — |
| `infra/devloop/absorb-devloop.test.sh` (HEAD-file read + detached case) | Mine | — |

---

## Planning

### Problem, in mechanism terms
devloop.sh derives the clone's branch as the constant `BRANCH_NAME=feature/<slug>` and the comparison base from the
positional arg (default: host's current branch). Every PR-iteration path is keyed on those two variables:
`ensure_clone` (fetch `BRANCH_NAME` from origin if not local, then `clone --branch`), the exit menu
(`gh pr list --head BRANCH_NAME` → `push origin BRANCH_NAME`), and the `r` re-exec (`exec $0 slug BASE_BRANCH`).
`--pr <n>` therefore only has to (a) set `BRANCH_NAME:=headRefName`, `BASE_BRANCH:=baseRefName` from
`gh pr view`, (b) refuse every state in which those paths would do something other than "clone the PR head /
push to the PR head", and (c) make the re-entry paths re-supply `--pr <n>`.

### Design (rev 2 — folds in Gate-1 input from all six reviewers)
Refusals print one stable line `ERROR: PR_MODE_REFUSED REASON=<token> PR=<n> …` on stderr plus indented remedy
lines (style of `CLUSTER_NAME_TOO_LONG`). Tests assert the token, never gh/git wording.

1. **Args (before anything else).** `--pr <n>` in the leading option loop; `${2:-}` guarded; value must match
   `^[1-9][0-9]*$` (so `--pr -R…`/`--pr --web` can't reach gh as options). Tokens: `bad-pr-number`, `pr-given-twice`,
   `with-destroy`, `with-refresh-creds` (teardown/cred refresh must never depend on gh — O2), `no-slug`
   (incl. `--rebuild --pr N`), `with-base-branch`. `-- <command...>` still works after the slug. The `${1:?Usage…}`
   string and the header usage gain `--pr`.
2. **ONE PR lookup (`resolve_pr`), right after REPO_ROOT/GITHUB_REMOTE — before token load, image build, orphan
   scan, helper build, runtime-dir mkdir, clone, any podman call.** `command -v gh` else `gh-missing`. One call:
   `gh pr view <n> -R "$GITHUB_REMOTE" --json state,isCrossRepository,headRepositoryOwner,headRepository,headRefName,headRefOid,baseRefName,url --jq <tsv>`;
   gh's stderr is shown as-is, rc≠0 → `lookup-failed` (covers not-found AND auth/network: telling them apart would
   mean matching gh's wording; the surfaced gh message says which). `-R "$GITHUB_REMOTE"` (verified: gh accepts the
   ssh/https remote URL) pins the PR to the repo ensure_clone fetches from and the clone pushes to (S4/O1).
   Checks on that one result: `state` must be `OPEN` (`not-open STATE=…`); fork check fails CLOSED (S3):
   `isCrossRepository` must be the literal `false` AND `headRepositoryOwner.login/headRepository.name` must equal the
   owner/repo parsed from the PR's own `url` (null/missing/other → `fork`); `headRefOid` must be 40-hex;
   `headRefName` and `baseRefName` must pass `git check-ref-format --branch` AND the allowlist
   `^[A-Za-z0-9][A-Za-z0-9._/-]*$` AND contain no `..`, end in neither `/` nor `.lock` (`unsafe-ref` — S2: blocks
   `+foo` force-refspecs, `@`, `-x`, `$(…)`). Then `BRANCH_NAME:=headRefName`, `BASE_BRANCH:=baseRefName`
   (default stays `feature/<slug>` / positional / current branch). A resolution banner goes to stdout: PR #, URL,
   head ref + short SHA, base ref, slug, clone dir (+ existing clone's branch/HEAD) — Obs-1.
3. **Clone-state preconditions (still before image build / any container).**
   - Clone exists: `symbolic-ref -q --short HEAD` must equal BRANCH_NAME (detached = mismatch). PR mode → refuse
     `clone-branch-mismatch` naming both branches + remedies. Non-PR mode → unchanged behaviour (spec), but a
     WARNING is printed, suggesting `--pr <n>` if the clone looks like a PR-mode clone; refusing there would change
     today's re-attach and could lock an operator out of their own container. Equal → re-attach exactly as today
     (no fetch, no reset: local commits preserved; no SHA check).
   - No clone, PR mode, host has local `refs/heads/<head>` ≠ headRefOid → refuse `local-branch-stale` (prior run left
     it, Dependabot rebased since — O4) with remedies (`git branch -D <head>` / ff-fetch). Never overwritten.
4. **ensure_clone (the one clone path), two PR-mode guards before its `git clone`:** fetched/local head must equal
   headRefOid (`head-moved` — pushes between lookup and fetch; S5/S3-must), and the "fresh start: clone base +
   `checkout -b`" arm refuses (`head-not-on-origin`) instead of silently inventing a branch. The fetch itself is
   unchanged; it only runs when the local ref is absent, so the worktree-checked-out / non-ff refusals of
   `fetch B:B` cannot arise.
5. **Re-entry carries the mode (one place).** Set once after args: `REENTER_ARGS` = (`--pr n slug`) | (`slug`) and
   `RERUN_ARGS` = (`--pr n slug`) | (`slug BASE_BRANCH`). All `exec "$0" …` sites (menu_reenter_or_cleanup + both
   `[r]` arms) use RERUN_ARGS; every "Re-enter with:"/"Re-enter:" hint and the `--recreate` hint in the token check
   use REENTER_ARGS. (`--refresh-creds`'s own `--recreate` hint and the `--destroy` hints stay slug-only: those
   flags are refused with `--pr`.) `r` after the PR closes → re-run refuses `not-open`; that message prints the
   clone path, the manual push command and `--destroy` so unpushed work isn't stranded (O8).
6. **Exit menu.** Detection stays the one existing `gh pr list --head "$BRANCH_NAME" --state open`, now
   `--json number,url`: rc≠0 is no longer swallowed (WARNING with gh's stderr); non-PR mode takes the first URL as
   today; PR mode takes the row whose number == n exactly (`/pull/12` ≠ `/pull/123`; fork PR with the same head
   name can't be picked — S7). PR mode with no matching open row (closed/merged since launch or gh failed) →
   WARNING + manual push command + r/d/q only; the `[c] create PR` / `[p] push branch` arm is unreachable in PR mode.
   Push to an existing PR (both modes, one site): `git push origin "refs/heads/$B:refs/heads/$B"` (fully qualified,
   never force, no `-u`), destination printed; a rejection (non-ff: Dependabot rebased / someone pushed) prints an
   ERROR with the recovery (in the clone: `git fetch origin <head>` + `git rebase origin/<head>`, re-run, push) and
   returns to the r/d/q menu instead of `set -e`-killing the script. Dependabot WARNING above the push prompt when
   `BRANCH_NAME == dependabot/*` (and in the launch banner).
7. **`git log ${BASE_BRANCH}..HEAD` — broken today, fixed once.** A `clone --local --branch X` has a local branch only
   for X; the host's branches are `refs/remotes/origin/*`. So the base resolves only on the fresh-start path; in PR
   mode and today's PR iteration (`feature/<slug>` already existed) `git log main..HEAD` errors, `2>/dev/null || true`
   masks it, and the menu says "No new commits" with commits present (→ no push offered, operator may `[d]`).
   ONE `commits_since_base` helper serves both sites (menu + `--` exec report): resolve `refs/heads/$BASE_BRANCH`
   then `refs/remotes/origin/$BASE_BRANCH` (fully qualified, after the allowlist — S8) in the clone; neither →
   WARNING naming the base, never "No new commits"; git errors not swallowed. `UNPUSHED` (`origin/<head>..HEAD`)
   gets the same treatment: missing `refs/remotes/origin/<head>` → WARNING, not "nothing unpushed".
8. **Slash audit.** Every BRANCH_NAME use is one quoted argv element or message text; none builds a path, container,
   network or cluster name (all slug-keyed). New sites (refspecs, gh args, banner) likewise quoted. No eval/`bash -c`.
9. **Docs.** devloop.sh header is the ONE home: usage, `--pr` option, examples, and a "PR mode" note — Dependabot
   caveat (after your push Dependabot stops rebasing the PR; `@dependabot rebase`/`recreate` force-pushes and
   discards your commits), return path is the menu push (NOT `/absorb-devloop`, which would cherry-pick the PR's
   commits onto your host branch), non-ff recovery, forks unsupported, host keeps a local `<head>` ref after
   `--destroy` (delete it or the next run refuses `local-branch-stale` once the PR moves). ADR-0025 §7 usage line
   gains `--pr` + a pointer to the header. `docs/runbooks/devloop-validation.md` is the validation-pipeline runbook
   and documents no devloop.sh invocation (only `--rebuild` remedies) → no edit (a section there would be a second
   home that drifts).
10. **Tests — `infra/devloop/devloop-pr.test.sh` (sources `_test_helpers.sh`), wired in `scripts/layer3.sh` next to
    absorb.** Runs the REAL devloop.sh from a throwaway host repo whose `origin` is a bare fixture repo (`main` +
    `dependabot/cargo/x-1.2.3`), so `../worktrees/<slug>` lands in `$WORK`. `GIT_CONFIG_GLOBAL=/dev/null`, temp HOME
    (no token file), `ANTHROPIC_API_KEY=dummy` (passes the auth precondition; only ever forwarded by name in the
    container-create path, which the tests never reach). PATH = stubs + `/usr/bin:/bin`, with a precondition that
    `kind` is NOT resolvable (fails the suite, never skips) — so no helper build, no cargo, suite ≈ seconds.
    Stub `gh` logs argv and serves `pr view`/`pr list` from JSON fixtures through real `jq` (the script's own `--jq`
    is what's tested); a fixture can force rc≠0 with an auth-style stderr. Stub `podman` logs argv; models "image
    exists", "container already running", no-op `exec`/`cp`, `container exists` → absent (harvest no-op).
    - Arg cases (assert token, rc≠0, no clone, podman log empty, gh NOT called): bad/missing/non-numeric value,
      given twice, + base, + --destroy, + --refresh-creds, no slug.
    - Lookup cases (same + gh log has `pr view <n> -R <origin>` — proves it died at the intended check): not found,
      auth failure (gh stderr surfaced), CLOSED, MERGED, fork (`true`), fork (`null`), head-owner mismatch,
      unsafe head refs `+evil` `-x` `@` `a..b` `x$(id)` `x.lock`, unsafe base ref, existing clone on another branch,
      stale local head branch, head not on origin, head moved (fixture headRefOid ≠ origin).
    - Positive control / happy path (podman log non-empty): slash ref → clone on `dependabot/cargo/x-1.2.3` at
      headRefOid, banner + Dependabot warning shown; a commit made in the clone; menu (piped input) detects PR #n,
      lists it as unpushed, `p` pushes and the bare origin's head now has it; no `[c]`; then `q`. Re-run re-attaches
      (no mismatch error, commit kept). `r` re-execs with `--pr n slug` (second invocation's banner seen in output).
      PR closed mid-session (fixture flipped before exit) → WARNING, no push/create arms, manual hint shown.
      Same-head-name fork PR listed first → still matched by number.
    - Item 7 regression, today's mode: host already has `feature/<slug>` with a commit, clone has no local `main` →
      menu lists the commit (not "No new commits").
    - No-`--pr` control: `<slug> main` → clone on `feature/<slug>` from main, gh log has no `pr view`, menu offers `[c]`.
    `scripts/parse-whole-script.test.sh` stays green (body stays inside `{ … exit; }`).

### Rev 3 deltas (supersede rev 2 where they differ)
- **R3-1 Clone-branch check refuses in BOTH modes** (Lead's final ruling — the ONE intentional change to no-`--pr`
  behaviour). Reason: a clone on any branch other than BRANCH_NAME is already broken today — the menu lists HEAD's
  commits but pushes / creates a PR for `feature/<slug>`, a different branch. No lockout: the clone is a host
  directory, fixable from the host. ONE helper `check_clone_branch` (detached HEAD = mismatch), run in pre-flight
  (before image build / podman): `ERROR: CLONE_BRANCH_MISMATCH CLONE=… ON=… EXPECTED=…` (+ `PR_MODE_REFUSED
  REASON=clone-branch-mismatch` in PR mode); remedies `git -C <clone> switch <branch>`, `--pr <n>`, `--destroy`.
  `--destroy` never reaches it. Named in the usage header. Tests: refusal in both modes (+ detached), the switch
  remedy then re-attaches, and a matching non-PR re-attach (rc 0, same clone, commit kept).
- **R3-2 Branch-ref step factored out of ensure_clone, reachable pre-flight.** `ensure_branch_ref` = ensure_clone's
  existing "fetch the branch if only on origin" step (now with the fully-qualified refspec
  `refs/heads/B:refs/heads/B` — same effect in non-PR mode) + in PR mode: absent on origin → `head-not-on-origin`;
  local ref ≠ headRefOid → `local-branch-stale` (was present) / `head-moved` (just fetched). ensure_clone calls it
  (unchanged order in non-PR mode); PR mode ALSO calls it in pre-flight so every PR-mode refusal precedes the first
  podman call (test asserts empty podman log for all of them). Idempotent: second call is a no-op. After the clone,
  PR mode asserts clone `HEAD == headRefOid` (`head-moved`), per Lead.
- **R3-3 commits_since_base prints what it compared against**: "Commits since origin/main (clone-time snapshot of the
  host's main):" vs "Commits since main:"; comment explains no fetch (container/clone has no GitHub creds by design;
  the host repo's branch, not GitHub, is the base the clone was cut from, so a stale host branch can over-list).
- **R3-4 UNPUSHED** goes through the same resolve-or-WARN (`refs/remotes/origin/<head>`), never "nothing unpushed".
- **R3-5 Menu gh outcomes split by exit status**: rc≠0 → "WARNING: could not query open PRs for <head> (gh rc=X)" with
  gh's stderr visible; rc=0 but no row with number==n → "WARNING: PR #n is no longer open (not found for head <head>)".
  Both print the manual push command `git -C <clone> push origin refs/heads/<head>:refs/heads/<head>`, then r/d/q.
- **R3-6 Push rejection**: ERROR + recovery (`git -C <clone> fetch origin <head> && git -C <clone> rebase
  origin/<head>`, re-run, push; never force), back to r/d/q. Test: advance the bare origin's head, then `p`.
- **R3-7 Dependabot caveat**: one predicate+printer `dependabot_caveat` used at the banner and above `[p]`; header prose.
- **R3-8 Usage homes**: header (also gains the `--destroy` and `-- <command…>` forms it was missing), the `${1:?Usage…}`
  string, ADR-0025 §7. Header notes add: return path is the menu push, NOT `/absorb-devloop`; host keeps a local
  `<head>` ref after `--destroy` (`git branch -D <head>` before a later re-run; a stale one is refused).
- **R3-9 Stubs fail loudly**: gh and podman stubs exit 91 with "unmodelled" on any call they don't model.

### Manual test plan (operator, host — not run here)
As in §Objective (1)–(5).

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (rev 3) |
| Test | confirmed (rev 2; rev 3 deltas reviewed at Gate 3) |
| Observability | confirmed (rev 3) |
| Code Quality | confirmed (rev 3) |
| DRY | confirmed (rev 3; neutral on R3-1) |
| Operations | confirmed (rev 3) |

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED (round 3, 3f8d2ddb) | 6 (S-R4 withdrawn) | 5 | 0 | S-R1 container→host escape (pre-existing) fixed in-loop; residuals: bundle mktemp symlink race (negligible), /tmp/devloop mount now unconditional |
| Test | RESOLVED-FIXED (round 3, 3f8d2ddb) | 4 (T1 withdrawn) | 3 | 0 | |
| Observability | pending (reopened for F4) | 4 (F1 withdrawn) | 2 | 0 | F4 podman-inspect misdiagnosis open |
| Code Quality | RESOLVED-FIXED (round 3, 3f8d2ddb) | 3 (F1 withdrawn) | 2 | 0 | F2 ruled: WARNING not refusal in non-PR mode |
| DRY | RESOLVED-FIXED (round 3, 3f8d2ddb) | 5 | 5 | 0 | |
| Operations | RESOLVED-FIXED (round 3, 3f8d2ddb) | 5 (OPS-1 withdrawn) | 4 | 0 | OPS-4 bundle-in-container design |

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### devloop.sh
| Item | Before | After |
|------|--------|-------|
| Options | no PR mode | `--pr <n>` (validated `^[1-9][0-9]*$`); refusals `pr_refuse` → `ERROR: PR_MODE_REFUSED REASON=<token> PR=<n>` + remedies |
| Branch/base | `feature/<slug>` / positional | PR mode: one `gh pr view <n> -R "$GITHUB_REMOTE"` (`resolve_pr`) → headRefName/baseRefName; fork check fails closed; `safe_branch_name` allowlist + check-ref-format on head and base |
| Pre-flight (before image build/podman) | — | `check_clone_branch` (refuses in both modes — the one no-`--pr` behaviour change), `ensure_branch_ref` (PR: head-not-on-origin / local-branch-stale / head-moved), banner + `dependabot_caveat` |
| ensure_clone | inline fetch `B:B` | calls `ensure_branch_ref` (fetch refspec `refs/heads/B:refs/heads/B`); PR mode asserts clone HEAD == headRefOid |
| Re-entry | `exec "$0" slug base`, hints `$0 slug` | `RERUN_ARGS` / `REENTER_ARGS` at every exec + hint (incl. exec-mode and token `--recreate` hints) |
| Commits since base | `git log BASE..HEAD 2>/dev/null \|\| true` (masked → "No new commits" when the clone lacks a local base) | `commits_since_base`: refs/heads/BASE, else refs/remotes/origin/BASE labelled "clone-time snapshot", else WARNING |
| Menu | `gh pr list … 2>/dev/null \|\| true`, `.[0].url`; `origin/B..HEAD` masked; push `origin B` under set -e | gh rc reported; PR mode matches number==n, else WARNING + manual push hint + r/d/q; UNPUSHED resolve-or-WARN; `push_to_pr` (qualified refspec, never force, rejection → recovery + menu) |

### Additional Changes
- `infra/devloop/devloop-pr.test.sh` (new, 284 assertions, ~5s), wired in `scripts/layer3.sh` (`devloop-pr-selftest`).
- ADR-0025 §7 usage + pointer to the script header; INDEX.md pointer.
- Review round 1 (S-R1, Lead fix-now): the host never runs git/gh with the clone as repo/cwd after
  its container exists. Exit menu: `fetch_clone_branch` (host fetch FROM the clone into
  `refs/devloop/<slug>/head`, ref deleted first so it is only ever created) → listings in the host repo
  (`commits_since_base`, `unpushed_commits` via `ls-remote` origin) → every push (`[p]` PR, `[p]` branch,
  `[c]`) via `host_push_cmd` from the host repo to origin's URL; `gh pr list/create -R`, no cd, `[c]`
  title/body built from host commits (`--fill` reads cwd git). Clone HEAD read as a file:
  `infra/lib/clone-head.sh` (also absorb-devloop.sh). Manual-push hints are the host-side two-step.
  Hostile-clone tests (pre-push/pre-commit/post-checkout hooks, pushurl decoy, sshCommand,
  showSignature+gpg.program with a gpgsig commit, fsmonitor) with a positive control that the plants are live.
- Also: S-R3 (post-clone head-moved removes the just-made clone; not testable without a seam — the
  window is closed by the pre-clone SHA check), non-PR ls-remote failure → WARNING (Lead ruling),
  git-log failure WARNs, not-open hint names the real branch + "branch only, not the PR" note,
  `--refresh-creds` token hint mentions `--pr`, tests for unpushed-unknown / base-unknown / origin-unreachable.
- OPS-4 (Lead ruling, security design): push rejected → `print_push_recovery`: host fetches origin's
  branch (host creds) into `refs/devloop/<slug>/remote`, writes a thin `git bundle` into a fresh
  `mktemp -d` dir under the per-slug runtime dir, prints the ONE in-container command
  (`git fetch /tmp/devloop/<dir>/head.bundle +…:refs/remotes/origin/<head> && git rebase origin/<head>`);
  exit → the menu pushes host-side, unforced. No host-side rebase (the repo's pre-commit hook would
  source container-authored scripts), nothing written into or pushed into the clone. To make this work
  with or without Kind, the per-slug runtime mount (host `/tmp/devloop-<slug>` → `/tmp/devloop`) is now
  UNCONDITIONAL on container create (was: only when the helper launched); helper presence in the
  container stays the socket. Older containers without the mount get a `--recreate` hint. Test runs the
  recovery end to end (bundle fetch + rebase in the clone, re-push lands on top of the moved head).
- S-R5: `-c fetch.fsckObjects=true` on both fetches from a clone (devloop.sh, absorb-devloop.sh).
- Obs F4: the recovery's `podman inspect` failure is reported as such (rc + podman stderr), distinct from "no mount".
- DRY G1 + S-R6: ONE `fetch_from_clone` + `CLONE_FETCH_ARGS` in `infra/lib/clone-head.sh` (fsckObjects,
  `--no-tags`, `--no-write-fetch-head`, fully-qualified `refs/heads/<b>` source, delete-then-create dest) used by
  `fetch_clone_branch`, absorb-devloop.sh (no more `+` / tag auto-follow) and the printed `manual_push_hint`.
  Planted-tag test in absorb-devloop.test.sh (killed by the old fetch).
- DRY G2 + OPS-5: CLONE_BRANCH_MISMATCH remedy is in-container (`podman exec -it -w /work <dev> git switch <b>`),
  never host `git -C <clone>`; header + tests updated.
- DRY G3: comment at ensure_clone's clone-side git calls — they run only on the clone just created in that call.
- Obs F5: mismatch remedy adds `podman start <dev> <db>` for stopped containers (pre-flight runs before any start).

## Files Modified

```
 docs/decisions/adr-0025-containerized-devloop.md  |   3 +
 docs/specialist-knowledge/infrastructure/INDEX.md |   2 +-
 infra/devloop/absorb-devloop.sh                   |  11 +-
 infra/devloop/absorb-devloop.test.sh              |  17 +
 infra/devloop/devloop.sh                          | 548 +++++++++++++++++++---
 scripts/layer3.sh                                 |   4 +
 infra/devloop/devloop-pr.test.sh (new)            | 501
 infra/lib/clone-head.sh (new)                     |  53
```

### Key Changes by File
| File | Changes |
|------|---------|
| `infra/devloop/devloop.sh` | `--pr <n>` mode (one pinned `gh pr view -R origin`, fail-closed validation before any clone/container, BRANCH_NAME/BASE_BRANCH generalised, RERUN/REENTER args carry `--pr`); clone-branch mismatch refused in both modes; unmasked commits-since-base / unpushed listings; host-side-only menu (fetch from clone, host push by URL, never forced); bundle-based rejected-push recovery; Dependabot caveat; usage header + examples |
| `infra/devloop/devloop-pr.test.sh` | Hermetic podman-free suite (stub gh/podman fail loudly on unmodelled calls): every refusal asserts no clone + no podman call; happy path on a slash ref; re-attach; push/reject/recovery end to end; hostile-clone traps; no-`--pr` regressions. 365 cases |
| `infra/lib/clone-head.sh` | Shared host-side clone access: `clone_head_branch` (reads `.git/HEAD` as a file), `fetch_from_clone` / `CLONE_FETCH_ARGS` |
| `infra/devloop/absorb-devloop.sh` (+test) | Uses the shared helpers: no tag import, no forced fetch, fsckObjects; planted-tag test |
| `scripts/layer3.sh` | Wires `devloop-pr-selftest` |
| `docs/decisions/adr-0025-containerized-devloop.md` | §7 usage line; points at the script header as the usage SSoT |
| `docs/specialist-knowledge/infrastructure/INDEX.md` | Pointer to the new test |

`docs/runbooks/devloop-validation.md`: not edited — it documents no `devloop.sh` invocations; the script header is the single home (operations agreed).

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` on the frozen tree (devloop.sh sha1 `4e264c38`; all hashes verified before and after) — **TOTAL_RESULT=OK**, 1641s.

| Layer | Result | Duration | Notes |
|-------|--------|----------|-------|
| 1 Compile | OK | 9s | |
| 2 Format | OK | 6s | |
| 3 Guards + self-tests | OK | 62s | incl. devloop-pr-selftest 365/365, absorb 59/59, parse-whole-script 6/6 |
| 4 Test | OK | 185s | |
| 5 Lint | OK | 2s | |
| 6 Audit | OK | 2s | SKIPPED-NO-DIFF no-dep-changes (expected) |
| 7 Env-tests | OK | 1375s | |

Not automated (per artifact table): `shellcheck`/`shfmt` are not installed anywhere; not run.

---

## Code Review Results

All verdicts are on the final tree (devloop.sh `4e264c38`). Full per-finding detail is in the Gate 3 table above.

### Security Specialist
**Verdict**: RESOLVED-FIXED — **Findings**: 6 found (S-R4 withdrawn), 5 fixed, 0 deferred. S-R1 container→host escape via host-side git in the clone (pre-existing); S-R2 `gh -R`; S-R3 remove unvetted clone; S-R5 fsckObjects; S-R6 no tag import in absorb. Residuals noted (not findings): mktemp-dir symlink race under container-writable /tmp/devloop (worst case a stray `head.bundle`); /tmp/devloop mount now unconditional (existing ADR-0030 accepted risk).

### Test Specialist
**Verdict**: RESOLVED-FIXED — 4 found (T1 withdrawn), 3 fixed, 0 deferred. Mutation-verified (T2–T4 plus 7 earlier mutations all killed).

### Observability Specialist
**Verdict**: RESOLVED-FIXED — 5 found (F1 withdrawn), 4 fixed, 0 deferred.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED — 3 found (F1 withdrawn), 2 fixed, 0 deferred.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings**: F1 usage text twice; F2 push command spelled twice; G1 fetch-from-clone three ways; G2 host-side remedy contradicting the rule; G3 fresh-clone comment — all fixed.

**Extraction opportunities**: None

### Operations Reviewer
**Verdict**: RESOLVED-FIXED — 5 found (OPS-1 closed by ruling), 4 fixed, 0 deferred. Untested assumption noted: `podman start <dev> <db>` relies on podman keeping container IPs across stop/start (the `--add-host` DB pin) — check during the manual test plan.

### Semantic Guard Reviewer
Not spawned (diff touches no `checks.md` surface).

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `a595bd87ca1d3465d4bff5b567c61383d238e20c`
2. `git revert <this commit>` (tooling-only; no schema, manifests or deployed artefacts).
3. **Safe-revert unit**: `the whole commit`. Do NOT partially revert: reverting `infra/lib/clone-head.sh` or the menu's host-side push without the rest reconstructs the S-R1 escape; reverting absorb-devloop.sh alone re-opens S-R6.

---

## Issues Encountered & Resolutions

### Issue 1: Fast check — spurious run-story / devloop-pr failures (concurrent tree mutation)
**Problem**: First Lead fast check: Layer 3 `run-story-selftest` STATUS=FAIL, no failing assertion in the captured output.
**Resolution**: `scripts/workflow/run-story.test.sh` run twice in isolation (501/501 both) and a full `layer-fast.sh` re-run → TOTAL_RESULT=OK. Root cause (found by implementer; test reviewer confirmed it was theirs): the test reviewer was mutation-testing `infra/devloop/devloop.sh` in place in /work while `layer-fast.sh` ran, so run-story's containment check saw the tree change mid-run and devloop-pr cases flipped. Lead instructed all reviewers to mutate only a copy or worktree. It recurred once (04:22:40–04:23:40) when the test reviewer's scratch worktree vanished and `cd` failed silently; the file was restored from a snapshot and the test reviewer stopped mutation testing. Lead then adopted a TREE FROZEN + sha1 handshake before any review verification. Not a product flake.

### Issue 2: Gate-1 ruling on non-PR clone-branch mismatch changed during planning
**Problem**: Lead issued conflicting rulings (refuse both modes ↔ warn + menu gate) as messages crossed.
**Resolution**: Final, as implemented and reviewed: a clone-branch mismatch (incl. detached HEAD) is refused in BOTH modes in pre-flight, with remedies `git -C <clone> switch <branch>` / `--pr <n>` / `--destroy`. Recorded in R3-1 as the one intentional change to no-`--pr` behaviour. Lesson: issue one ruling and wait for acks before revising — crossed messages caused two rework passes.

### Issue 3: Container→host escape in the pre-existing exit-menu push (S-R1)
**Problem**: Security reproduced that the host-side `git -C $CLONE_DIR push/log` ran the container-writable clone's hooks and config (pre-push hook executed on host; `remote.origin.pushurl` redirected the push; `log.showSignature`+`gpg.program` executed on `git log`). Pre-existing for today's `[p]`/`[c]`; `--pr` would have widened it to Dependabot branches.
**Resolution**: Lead ruled fix-now. After a container exists the host never runs git/gh in the clone: commits leave via `fetch_from_clone` (fsckObjects, `--no-tags`, fully-qualified source, delete-then-create dest) into `refs/devloop/<slug>/head`; listings and pushes run in the host repo; clone HEAD read as a file (`infra/lib/clone-head.sh`, also adopted by absorb-devloop.sh). Rejected-push recovery uses a host-made bundle + in-container rebase (a host-worktree rebase was rejected: `.githooks/pre-commit` would source container-authored scripts). Hostile-clone tests cover it.

### Issue 4: Freeze discipline
**Problem**: The tree changed during Lead fast checks three times (test reviewer's in-place mutations twice; implementer edits/concurrent layer-fast once), producing spurious Layer-3 failures and a Layer-2 UNKNOWN (shared /tmp/devloop).
**Resolution**: TREE FROZEN + sha1 handshake; reviewers verify on copies; Lead re-checks hashes before and after every run. Gate 2 ran on a tree whose hashes held throughout.

---

## Lessons Learned

1. Lead rulings must be issued once and acknowledged before revising — crossed messages caused two rework passes on R3-1.
2. In a shared working tree, reviewers must never mutate in place; use a TREE FROZEN + sha1 handshake and re-verify hashes around every validation run.
3. Host-side git against a container-writable repo is code execution on the host; the only safe direction is fetching *from* it into a host repo (fsck, no tags).

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
