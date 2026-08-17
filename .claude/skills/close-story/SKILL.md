---
name: close-story
description: Close a completed user story — verify completeness, run story-scope reflection, commit, push, create or update PR.
---

# Close-Story Skill

Close a user story: verify every devloop is done, run story-scope reflection (specialists update INDEX across the story's devloops), run a DRY ownership-lens retrospective, commit + push, create or update the story's PR.

Moves reflection from per-devloop (cheap, repetitive) to per-story (once, with cross-devloop context). Per-devloop reflection no longer exists — see `.claude/skills/devloop/SKILL.md` Workflow Overview.

## When to Use

After every task in the story's `dt-story` manifest has `status: completed`.

Do NOT use mid-story (Phase 1 refuses), for standalone devloops (Gate 2 INDEX guard suffices), or to recover a stuck devloop (use `/devloop --continue=...` first).

## Arguments

```
/close-story story-slug
```

- **story-slug**: a short slug matching the `YYYY-MM-DD-{slug}` filename under `docs/user-stories/`. The `YYYY-MM-DD-` prefix is stripped; pass the post-date tail only. Example: for `docs/user-stories/2026-04-20-billing-portal.md`, pass `billing-portal`. The full `YYYY-MM-DD-slug` form is also accepted.

## Design Rationale

- **Security gates are all input-boundary**: regex validate the CLI arg + every devloop-slug read from the story file before any path construction.
- **Structured-only extraction**: Phase 4 reads a fixed field list from devloop main.md; no freeform concat (token/log leak risk).
- **Narrow git scope**: Phase 3 stages only INDEX + devloop-outputs dirs, with a `git status --short` safety check for surprise modifications.
- **Harness permission model owns commit/push/gh**: no in-skill confirmation UI, dry-runs, or retry-on-deny. A deny is terminal.
- **Fail-close on INDEX guard**: Phase 2's post-reflection `validate-knowledge-index.sh` must pass before any commit.

## Workflow

```
VERIFY → TEAM CREATE → REFLECTION → DRY RETRO → TEAM DELETE → FINALIZE (commit/push) → PR → COMPLETE
```

## Phase 1: Verify

**Slug validation**. Before any file read or shell invocation, validate the argument:

```
^[a-z0-9-]+$
```

Reject otherwise. This covers the normal `billing-portal` shape *and* a full `2026-04-20-billing-portal` (dashes and digits already allowed). Devloop slugs read from the manifest are validated separately, against the canonical class pinned below — see “Slug extraction and validation”.

**Locate the story file** via glob match:

```
matches = glob("docs/user-stories/*{slug}.md")     # suffix match; exact form also matches
```

- 0 matches → abort: "no story file matches slug '{slug}'".
- 1 match → proceed with that path.
- ≥2 matches → escalate to the user (SendMessage): list the matched filenames, ask the user to pick one (they reply with the full `YYYY-MM-DD-slug` or an index).

**Read task state from the `dt-story` manifest** — never from a markdown table. Per ADR-0035 §4 the manifest is the only home for task status and the devloop-output slug.

```bash
target/release/dt-story list-tasks docs/user-stories/{story-file}.md
```

**Use `list-tasks`, NOT `next`.** `next` mutates on exactly one path — selecting an escalated task reopens it (status back to `pending`, `escalation` cleared, file rewritten) — and that path is reachable **precisely when the completeness check would fail**. A completeness probe built on `next` is therefore safe only when it passes and destructive when it fails, which is the inverse of what an audit gate must be: it would erase the record that a task was escalated at the exact moment someone is auditing the story, and Phase 3 stages `docs/user-stories/`, so the mutation would ride into a commit. `list-tasks` is read-only.

**Exit-code handling — rc 2 aborts, never degrades.** Per the `list-tasks` contract, rc 2 means "this binary cannot answer" and guarantees empty stdout. Do **not** wrap the call so a failure becomes an empty task list: that converts "cannot answer" into "nothing to do", which passes vacuously. If `list-tasks` exits non-zero, or the story file has no manifest block, abort with the message below — there is deliberately **no table-reading fallback**.

```
**Story close blocked — manifest unreadable**

Story: {story-title}   Slug: {story-slug}

`dt-story list-tasks` exited {rc}. This skill reads task state only from the
dt-story manifest. If this story predates the manifest (pre-2026-05), it is
already closed and re-closing is not supported.
```

**Completeness check**: every task's `status` MUST be **positively** `completed`.

```bash
target/release/dt-story list-tasks "$STORY" | jq -e 'length > 0 and all(.[]; .status == "completed")'
```

Assert `completed` positively — do **not** test for the absence of `pending`. `Status` has **three** variants, so `all(.status != "pending")` silently passes a story with an **escalated** task, which is strictly weaker than the check this replaces. Otherwise escalate:

```
**Story close blocked — incomplete tasks**

Story: {story-title}   Slug: {story-slug}

Incomplete tasks (from the dt-story manifest):
- #{id}: status {status}

Resolve these before closing. If a devloop is stuck: /devloop --continue=<slug>.
An escalated task is retried by rerunning the story runner.
```

**Slug extraction and validation.** Take each completed task's `slug` field from the same `list-tasks` JSON — structured extraction, never a hand-parse of the YAML block. Apply this regex before constructing any `docs/devloop-outputs/{slug}/main.md` path:

<!-- slug-class-sync: ^[a-z0-9]+(-[a-z0-9]+)*$ -->

That literal is pinned to `manifest::SLUG_PATTERN` (the canonical class, enforced in `dt-story`'s deserializer) by `scripts/guards/simple/validate-slug-class-sync.sh`. Re-validating here is deliberate defence in depth: a manifest is a hand-editable markdown block regardless of who wrote it, so it is an input boundary. **Edit the marker line above only to match a canonical-class change** — the guard fails on drift.

**Missing slugs — distinguish history from a recording failure.** The discriminator is: *does **any** completed task in this manifest carry a slug?*

- **No completed task has a slug** → a pre-slug-era story (closed before the runner recorded slugs). **Tolerate**: proceed, and state in the Phase 5 report which devloops could not be enumerated and that the story's own file is the historical record. Do not fail — a story cannot become uncloseable for lacking a field it predates.
- **Some do, but this task does not** → a live recording failure. **Hard-fail**, naming the task id, the runner's corresponding `STORY_RUN: NO-SLUG task=N cause=...` line, and the repair:

  ```
  target/release/dt-story complete docs/user-stories/{story}.md {id} --slug {slug}
  ```

  That gesture works on an already-completed task (slug is last-writer-wins), so it repairs the record in place.

**Revalidate at use.** A slug is a stored reference whose referent can be removed by a later commit while the story file stays byte-identical. Before using one, confirm `docs/devloop-outputs/{slug}/main.md` exists; a missing directory is the hard-fail in Phase 4, not a silently shortened list.

## Phase 2: Story-scope Reflection

**Identify participating specialists**: from each `docs/devloop-outputs/{devloop-slug}/main.md`, collect the implementing specialist (`**Specialist**:` header) plus any reviewer whose Code Review Results verdict was `RESOLVED` or `ESCALATED`. CLEAR reviewers skip reflection. Deduplicate. Always include `dry-reviewer` (Phase 2.5 retrospective runs in the same team).

**Spawn the reflection teammates.** The session has a single implicit team — there is no team to create (the `team_name` argument is deprecated and ignored). Custom subagent types from `.claude/agents/{name}.md` are spawnable directly (mirrors `/devloop` Step 3).

Defensive cleanup (best-effort): if this session already ran a close/devloop, shut down survivors first — `TaskList` to find prior teammates (by `owner`) and leftover tasks, send `{type: "shutdown_request"}` via SendMessage + `TaskStop` by name, then `TaskUpdate(status: "deleted")` on stale tasks. Ignore not-found failures.

**Spawn each specialist** via the Agent tool with `name: "{specialist-name}"`, `subagent_type: "{specialist-name}"`. The agent system auto-loads identity from `.claude/agents/{name}.md`. Include `docs/specialist-knowledge/{name}/INDEX.md` under a `## Navigation` header in the prompt.

**Send this prompt** (unicast via SendMessage, verbatim):

```
You are reflecting on a COMPLETED USER STORY spanning multiple devloops, not a single devloop.

Read the Implementation Summary + Code Review Results sections of each devloop's main.md in the story's range (paths below). Find persistent architectural shifts — patterns or code locations that will be load-bearing for future work. Update your `docs/specialist-knowledge/{your-name}/INDEX.md` to add pointers for those shifts. Remove or consolidate redundant pointers that individual devloops added that are now superseded by the story's final state.

INDEX.md is a navigation map — pointers to code and ADRs ONLY.

Format: "Topic → `path/to/file.rs:function_name()`" or "Topic → ADR-NNNN"

- Add pointers for new code locations, new ADRs, new integration seams that the story introduces
- Consolidate pointers where multiple devloops converged on the same seam
- Remove pointers for code that was moved or deleted during the story

DO NOT add implementation facts, gotchas, patterns, design decisions, review checklists, task status, or date-stamped sections. If something feels important but isn't a pointer, put it as a code comment, an ADR, or a TODO.md entry instead.

DRY reviewer: duplication findings go in `docs/TODO.md`, not INDEX.

Organize by architectural concept. Max 75 lines total.

Devloop main.md paths:
- docs/devloop-outputs/{slug-1}/main.md
- ...

When done, reply "Reflection complete" via SendMessage.
```

**Timeout**: 20 min per specialist; proceed without late returners, note in report.

**Post-reflection INDEX guard** (fail-close):

```bash
./scripts/guards/simple/validate-knowledge-index.sh
```

If it fails, forward to the offending specialist, ask for a fix, re-run. Phase 2 fails closed if the guard cannot be cleared — do not proceed to commit. Per ADR-0024 §6.3, cross-boundary INDEX edits follow owner-involvement rules (rare — INDEX files are in each specialist's own domain).

## Phase 2.5: DRY Ownership Lens Retrospective

Send this prompt to `dry-reviewer` (already in the team from Phase 2) via SendMessage:

```
You are running the Ownership Lens retrospective for user story {story-slug}.

Scope: story-level, across the N completed devloops. NOT per-devloop code-duplication review — that's done at Gate 3.

Inputs:
- docs/user-stories/{story-slug}.md
- docs/devloop-outputs/{each-devloop-slug}/main.md

Read the "Code Review Results" section of each devloop main.md, focusing on the Ownership Lens verdict field (ADR-0024 §6.6 step 7). Assess cross-devloop patterns: templated vs specific entries; classification drift; same edit shape with different classifications across devloops; Pattern B without named convention author; GSA accidentally routed as Mechanical; ESCALATE routes; Paired flag use.

Output: Write docs/devloop-outputs/{story-slug}-story-close/ownership-lens-retrospective.md. Create the directory if needed — the `-story-close` suffix keeps it distinct from devloop output dirs. ≤30 lines with this EXACT structure (stable headers for future machine parsing per ADR-0024 §6.8 item #3):

## Summary
{1-2 sentences}

## Ownership Lens Verdict Audit
- Devloop: <slug>   Classification: <Mine|Mechanical|Minor-judgment|Domain-judgment>   Outcome: <clean|upgraded|escalated>
{one bullet per devloop}

## Pattern Observations
{bulleted}

## Follow-Ups
{bulleted or "None"; add TODO.md entries if warranted}

Non-blocking: advisory only, does NOT gate the close. Do NOT perform DRY code-duplication analysis.

When done, reply "Retrospective complete".
```

**Timeout**: 15 min; proceed, note "retrospective skipped (timeout)" if missed.

## Phase 2.6: Teammate Teardown

After all reflections + the retrospective have completed (or timed out):

1. Send `{type: "shutdown_request"}` to every teammate you spawned via SendMessage; `TaskStop` by name any that do not wind down.
2. Clear the session-global task list: call `TaskList`, then `TaskUpdate(status: "completed" | "deleted")` on remaining items.

There is no team object to delete. Mirrors `/devloop` Step 8.5. Prevents stale teammates/tasks from leaking into subsequent close-story or devloop runs in the same session.

## Phase 3: Finalize

1. Update the story file: set `**Status**:` to `Complete`. This is the only metadata recorded on close.

2. Stage — **narrow scope only**:
   ```bash
   git add docs/specialist-knowledge/ docs/devloop-outputs/ docs/user-stories/
   ```

   **Unexpected-modification check**: run `git status --short` before committing. If modified files sit outside `docs/specialist-knowledge/`, `docs/devloop-outputs/`, and `docs/user-stories/`, escalate to the user — a specialist may have touched unrelated files, or ambient work belongs elsewhere.

3. Commit (heredoc avoids shell-expansion of anything read from files; trailer order matches /devloop Step 8):

   ```bash
   git commit -m "$(cat <<'EOF'
   Close user story: <story-title>

   Story: docs/user-stories/<story-slug>.md
   Devloops: <comma-separated list of devloop slugs>
   Tasks-closed: <count>
   EOF
   )"
   ```

   (The standard `Co-Authored-By` trailer is added per the harness commit convention — do not hard-code it here; a hard-coded version has gone stale before.)

   If nothing to commit (reflection produced no INDEX changes), skip silently and note "no commit (no changes)" in the report.

4. `git push`. The harness permission model prompts the user. No in-skill confirmation UI, no dry-run preview, no retry-on-deny — deny is terminal. No `--force`, no `--no-verify`.

## Phase 4: Pull Request

**Enumerate branch commits**:

```bash
git log --oneline "$(git merge-base HEAD main)..HEAD"
```

Categorize each: **story-devloop** (references a `docs/devloop-outputs/{slug}/` path or has a `Devloop:` trailer matching a slug from the manifest), **story-close** (this skill's own commit), or **adjacent** (anything else).

**PR body synthesis — structured fields only**. For each story-devloop main.md, include ONLY:

- `**Task**:` header value
- `Code Review Results` verdict values (CLEAR / RESOLVED / ESCALATED) and finding counts
- `Implementation Summary` category/priority tables (structured, not freeform)
- `Cross-Boundary Classification` table
- `Tech Debt` section

Exclude everything else: `.env` dumps, log tails, teammate transcripts, freeform reflection notes, arbitrary body concatenation, and any `## Reflection` section if an older main.md still has one.

**Missing sections — tolerate**:
- Missing Implementation Summary → omit description beyond Task + verdicts.
- Missing Tech Debt → skip; if all devloops lack it, render "Remaining follow-ups: None."
- Missing devloop output dir → hard-fail with the path and the manifest task id whose `slug` referenced it. This is the revalidate-at-use check that admits `slug` as a stored reference at all — do not soften it into a silently shorter list.
- All-sections-missing across N devloops → "{N} devloops contributed no structured data; see individual main.md files" in the affected section.

**PR body shape** (fixed template, heredoc-safe):

```markdown
## Summary
{1-2 sentences from the story's "As a {persona}, I want {goal} so that {benefit}" line}

## Devloops completed
- **{devloop-slug}** — {Task: header value}. `docs/devloop-outputs/{devloop-slug}/main.md`

## Architectural shifts
{Bulleted pointers from specialists' Phase 2 INDEX updates: `{topic} → {path:function or ADR-NNNN}`}

## Adjacent work
{Non-story commits; or "None".}

## Remaining follow-ups
{Aggregated devloop Tech Debt + Phase 2.5 TODO.md additions; or "None".}

## Test evidence
Rolled-up verdicts:
- Security: {X CLEAR, Y RESOLVED, Z ESCALATED}
- Test: ...
- Observability: ...
- Code Quality: ...
- DRY: ...
- Operations: ...
```

**Create or edit PR** — branch on presence:

```bash
EXISTING_PR=$(gh pr list --head "$(git branch --show-current)" --json number --jq '.[0].number // empty')
if [ -n "$EXISTING_PR" ]; then
  gh pr edit "$EXISTING_PR" --body-file <(cat <<'EOF'
{synthesized body}
EOF
)
else
  gh pr create --title "<story-title>" --body-file <(cat <<'EOF'
{synthesized body}
EOF
)
fi
```

`gh pr edit` overwrites the body — intentional; the close-story synthesis is authoritative. Do not attempt to merge with the existing description. The `--json number --jq '.[0].number // empty'` form is stable across `gh` CLI versions.

**Argument hygiene**: quoted heredoc (`'EOF'`) disables shell expansion inside, so a backtick or `$(...)` in a read-from-main.md value is inert. Never interpolate main.md-derived variables into the `gh` command line.

Harness permission prompts fire on `gh pr create/edit`; deny is terminal per Phase 3's rule.

## Phase 5: Complete

**Report**:

```
**Story closed**: {story-title}

Devloops: {count} ({comma-separated slugs})
Slug coverage: {N}/{M} completed tasks
PR: {URL}
TODO additions: {count from Phase 2.5}
Reflection: {count of specialists who updated INDEX}
Ownership-lens retrospective: docs/devloop-outputs/{story-slug}-story-close/ownership-lens-retrospective.md
```

**`Slug coverage` is ALWAYS printed, never conditional.** The normal case reads `4/4`; a degraded close (the pre-slug-era tolerate path) reads e.g. `0/65` and is visible in the fixed record **by value**. A degraded close must not depend on a model remembering to mention it in prose under budget pressure — same reason the runner emits a `cause=` token rather than a sentence.

Conditional output: print the retrospective line only if the file exists (substitute `skipped (timeout)` or omit). If Phase 3 was a no-op commit, add `Commit: none (no working-tree changes)`. Flag any other skipped phases inline.

## Limits

| Phase | Limit | Action |
|-------|-------|--------|
| Phase 1 | — | Block immediately on incomplete tasks |
| Phase 2 | 20 min per specialist | Proceed without; note in report |
| Phase 2 INDEX guard | 3 retries | Fail close — do not commit |
| Phase 2.5 | 15 min | Proceed without; note in report |
| Phase 2.6 (team teardown) | — | Always runs after Phase 2 + 2.5, even if either timed out |
| Phase 3 (commit/push), Phase 4 (PR) | — | Harness permission prompts; deny is terminal |

## Files

- Story file: `docs/user-stories/{story-slug}.md` (exact filename resolved via glob at Phase 1)
- Devloop outputs: `docs/devloop-outputs/{devloop-slug}/main.md`
- Story-close output: `docs/devloop-outputs/{story-slug}-story-close/ownership-lens-retrospective.md`
- Specialist INDEX: `docs/specialist-knowledge/{name}/INDEX.md`
- Upstream: `.claude/skills/devloop/SKILL.md` (reflection removed), `.claude/skills/user-story/SKILL.md` (emits the `dt-story` manifest this skill reads)
- Slug class pinned to `crates/dt-story/src/manifest.rs::SLUG_PATTERN` by `scripts/guards/simple/validate-slug-class-sync.sh`
