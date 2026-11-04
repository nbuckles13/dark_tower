---
name: triage-pr
description: Diagnose a GitHub PR's CI state and recommend next actions — merge, review, rerun, or fix (with the exact devloop.sh --pr and /devloop commands, routed to the owning specialist). Read-only; acts only on explicit confirmation.
---

# Triage PR

Point it at a PR (or every open PR by an author) and get a diagnosis plus the exact next step.
Works for any PR; Dependabot PRs get a few extra rules (Step 3).

```
/triage-pr <number>
/triage-pr --author <login>        # e.g. --author app/dependabot: every open PR by that author
```

## Rules

- **Read-only until confirmed.** Never merge, push, comment, rerun, or edit files on your own.
  Recommend; act only on the user's explicit go-ahead, one action at a time.
- **Host-side diagnosis from logs.** This host cannot do full builds. Diagnose from CI logs and
  the PR's diff; when that is not enough, say so (Step 5 fallback) — never guess a cause.
- **Fail loudly.** A check you could not read, a log you could not fetch, or a rule file you
  could not parse is reported as such, never treated as "passed" or "no findings".

## Step 1: Gather

```bash
gh pr view <n> --json number,title,url,author,headRefName,baseRefName,isCrossRepository,mergeable,mergeStateStatus,files,statusCheckRollup
```

- `mergeable: UNKNOWN` is normal right after a push or a base change; re-query once before
  reporting it.
- Note whether the branch is behind its base: `gh api repos/{owner}/{repo}/compare/<base>...<head> --jq .behind_by`.
- For `--author`: `gh pr list --author <login> --state open --json number` and run Steps 1–6 per PR.

If any check is still pending or in progress, report that and stop for that PR (recommend
waiting); do not diagnose a partial run.

## Step 2: Classify the checks

Group `statusCheckRollup` by conclusion. `SKIPPED` checks are expected for some lanes (e.g.
`Layer 7 images (main)` on PRs) — list them only if a required-looking check is skipped.

- All green → Step 3.
- Any failure → Step 4.

## Step 3: Green — check review rules before recommending merge

Green CI is necessary, not sufficient. Check, in order:

1. **Guarded Shared Areas.** Match the PR's changed files against the keys of
   `scripts/guards/simple/cross-boundary-ownership.yaml` (glob patterns; union the specialist
   lists when several match). Read the file — do not copy its list into this skill. Its header
   notes the path-independent GSA it cannot enumerate: ADR-0027 crypto primitives
   (`docs/decisions/adr-0027-approved-crypto.md`). A dependency bump of a crate or package
   ADR-0027 names is a GSA change → `security` + the owner.
2. **Dependabot PRs.** Read `.github/dependabot.yml`: its comments carry per-dependency review
   requirements (e.g. tonic/prost regenerate proto-gen → protocol + security; `@bufbuild/buf` is
   the proto formatter → protocol + security; rust-toolchain needs `devloop.sh --rebuild`
   afterwards). Find the group or package the PR touches and apply what its comment says.
3. **Lockfile-only diffs.** List the packages whose versions change (Cargo.lock /
   pnpm-lock.yaml). Flag any major (or 0.x minor) bump — those can carry behaviour changes
   CI does not exercise.

Recommendation: **merge** (`gh pr merge <n> --squash --delete-branch` — the repo allows squash
and rebase only) or **merge after review by <specialists>**, with the reason.

## Step 4: Red — read the failure

For each failed check, take the job id from its `detailsUrl` (`.../job/<id>`) and fetch the log:

```bash
gh api repos/{owner}/{repo}/actions/jobs/<id>/logs \
  | sed -E 's/^[0-9TZ:.-]+ //; s/\x1b\[[0-9;]*m//g' > <scratchpad>/job-<id>.log
```

`gh run view --job <id> --log-failed` often returns nothing for these jobs — use the API.

Extract, in this order of usefulness:
- Pipeline tokens (ADR-0033): `STATUS=FAIL REASON=...` and `LAYER=<n> RESULT=FAIL`. The first
  non-summary `REASON` names the failing lane.
- Self-test failures: the `- [<case>] ...` lines under `<name>.test.sh: N passed, M failed`.
- Guard violations: `VIOLATION: [...]` lines (with the guard name from `FAILED: <guard>`).
- Rust: `error[E....]` / `error:` lines plus the following `-->` location; nextest `FAIL [`
  lines; for panics, the 2–3 lines after `panicked at` (the message, not just the location).
- Client: vitest/playwright `FAIL` / `✗` lines and their assertion messages.

Ignore noise: `::error::` text inside echoed step scripts (the step header prints the script
source), `CACHE_ON_FAILURE`, and stderr from tests that passed.

**Dedupe**: one root cause usually fails several checks (Pipeline test, Test Suite, Code
Coverage). Group by cause, list the checks each cause explains, and make sure every failed check
is explained by some cause — an unexplained failure is reported as unexplained.

**Classify each cause**:
| Class | Signs | Action |
|---|---|---|
| Code | compile error, test failure, guard violation, lint | fix (Step 5) |
| Stale branch | failure in code the PR did not touch, and the branch is behind base or main has since fixed it | update the branch (for Dependabot, before anyone else pushes: `@dependabot rebase`) |
| CI infra / flaky | runner/network/cache/timeout/cancelled, or a test unrelated to the diff that is known flaky | `gh run rerun <run-id> --failed` |
| Decision | the fix requires a choice (e.g. which crypto backend a new major requires) | ask the user; name the options and the owner who should weigh in |

For a **Decision**, check the options against the ADRs that govern the area (e.g. ADR-0027 for
crypto). If every option departs from an ADR, say so with the ADR line, and recommend a
`/debate` (or an ADR amendment) with the owning specialists instead of a direct choice — the fix
devloop waits for that outcome.

For **dependency bumps** with an API-change failure, read the upstream changelog / release
notes for the version range to identify the breaking change and its migration path, and cite it.

## Step 5: Route

Map the failing code to the implementing specialist:
- Path → crate → specialist from the CLAUDE.md service table (`crates/ac-service` →
  auth-controller, `crates/gc-service` → global-controller, `crates/mc-service` →
  meeting-controller, `crates/mh-service` → media-handler). `packages/` → client;
  `proto/` → protocol; `migrations/` → database; `scripts/`, `infra/`, `.github/` →
  infrastructure.
- Failures only in test code (`tests/`, `*-test-utils`, `#[cfg(test)]`) across crates → `test`.
- A dependency with a domain owner overrides the path (e.g. JWT / crypto crates →
  auth-controller, `--paired-with=security`).
- GSA paths (Step 3.1) add the owners and `security` via `--paired-with`.
- If the routing is ambiguous, say which specialists are plausible and why; let the user pick.

**Fallback** — logs do not explain the failure (CI-only behaviour, truncated logs, a cause
spanning many crates): recommend investigating before a devloop. Start the PR-mode container,
run plain `claude` inside it, reproduce with `scripts/layer-all.sh`, then launch `/devloop` with
the specialist the investigation points to. Do not emit a `/devloop` command with a guessed
specialist.

## Step 6: Report

Per PR:

```
PR #<n> <title> — <GREEN|RED|PENDING> (<url>)
Cause:    <one line, with its class; the checks it explains>
Evidence: <≤10 extracted log lines>
Owner:    <specialist> [+ paired]
Action:   <merge | merge after review by X | rerun | update branch | fix | decide>
```

With **several causes**, number them and give each its own complete block — every cause gets
its own `Owner:` and `Action:` line; never merge two causes' owners into one line:

```
PR #<n> <title> — RED (<url>)
Cause 1:  <one line, class; checks explained>
Evidence: <lines>
Owner:    <specialist> [+ paired]
Action:   <action>
Cause 2:  <one line, class; checks explained>
Evidence: <lines>
Owner:    <specialist> [+ paired]
Action:   <action>
PR action: <the overall next step — e.g. "decide (cause 2), then one fix devloop covering causes 1–2">
```

One PR gets **one** fix devloop covering all its code causes; route it to the owner of the
cause that needs the most domain judgment and `--paired-with` the others' owners.

For **fix**, print the commands, filled in:

```
infra/devloop/devloop.sh --pr <n> <slug>        # from a host checkout/worktree; slug: lowercase, <= 41 chars, e.g. pr<n>-<dep>
# inside the container:
/devloop "PR <n> (<author>) <what the PR does>; CI fails: <cause, one line>.
<evidence lines>
<known fix or migration path, if identified — else: find the root cause>
Reproduce with scripts/layer-all.sh and make all layers pass. Keep the PR's intent<, for Dependabot: do not revert or pin back the bump>. <Review rules from Step 3, if any>. If the fix needs a decision, stop and raise it." --specialist=<specialist> [--paired-with=<owner>]
# exit with [p] to push to the PR
```

Add `--light` only when the fix is clearly small and touches none of the `--light` exclusions
(`.claude/skills/devloop/SKILL.md` §Lightweight Mode) — when unsure, leave it off.

For Dependabot PRs that need a fix, remind: once anyone pushes to the branch, Dependabot stops
rebasing it, and `@dependabot rebase` / `@dependabot recreate` would discard the pushed commits.

With `--author`, print a summary table first (PR, status, action), then the per-PR detail.

## Step 7: Act on confirmation

Only after the user confirms, and one PR at a time:
- **Merge**: `gh pr merge <n> --squash --delete-branch`, then confirm with
  `gh pr view <n> --json state,mergedAt`. After each merge, the remaining PRs' bases moved —
  re-check `mergeable` (and, for lockfile PRs, expect conflicts) before merging the next.
- **Rerun**: `gh run rerun <run-id> --failed`.
- **Fix**: the user runs the printed commands; this skill does not start devloops.
