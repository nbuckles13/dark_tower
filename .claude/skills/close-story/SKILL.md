---
name: close-story
description: Close a completed user story — verify completeness, run story-scope reflection, commit, push, create or update PR.
---

# Close-Story Skill

Close a user story: verify every devloop is done, run story-scope reflection (specialists update INDEX across the story's devloops), run a DRY ownership-lens retrospective, commit + push, create or update the story's PR.

Moves reflection from per-devloop (cheap, repetitive) to per-story (once, with cross-devloop context). Per-devloop reflection no longer exists — see `.claude/skills/devloop/SKILL.md` Workflow Overview.

## When to Use

After the final devloop in a user story completes. `/user-story` creates a `/close-story` task blocked by all devloop tasks; when those unblock it, run this skill.

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
VERIFY → REFLECTION → DRY RETRO → FINALIZE (commit/push) → PR → COMPLETE
```

## Phase 1: Verify

**Slug validation**. Before any file read or shell invocation, validate the argument:

```
^[a-z0-9-]+$
```

Reject otherwise. This covers the normal `billing-portal` shape *and* a full `2026-04-20-billing-portal` (dashes and digits already allowed). Apply the **same regex** to every devloop-slug value read from the story file's `Devloop Tracking` table before constructing `docs/devloop-outputs/{devloop-slug}/main.md` paths — second injection vector (hand-edited story file).

**Locate the story file** via glob match:

```
matches = glob("docs/user-stories/*{slug}.md")     # suffix match; exact form also matches
```

- 0 matches → abort: "no story file matches slug '{slug}'".
- 1 match → proceed with that path.
- ≥2 matches → escalate to the user (SendMessage): list the matched filenames, ask the user to pick one (they reply with the full `YYYY-MM-DD-slug` or an index).

**Enumerate the story's tasks from the story file** (not from `TaskList`): read the `**Close-Story Task ID**:` header line and the `Task ID` column of the `Devloop Tracking` table. These are written by `/user-story` Step 10.5. For each ID, call `TaskGet`.

Fallback (story predates `/user-story` Step 10.5, so IDs are absent): prompt the user for task IDs rather than guessing via `TaskList` subject-substring matching — that would silently include unrelated tasks or miss renames.

**Completeness check**: every story-task ID other than the close-story task itself MUST have `status == completed`. Phase 1 advances to Phase 2 iff this returns zero non-completed IDs; otherwise escalate:

```
**Story close blocked — incomplete tasks**

Story: {story-title}   Slug: {story-slug}

Incomplete tasks:
- #{id} [{status}] "{subject}" (owner: {owner or unassigned})

Resolve these before closing. If a devloop is stuck: /devloop --continue=<slug>.
```

## Phase 2: Story-scope Reflection

**Identify participating specialists**: from each `docs/devloop-outputs/{devloop-slug}/main.md`, collect the implementing specialist (`**Specialist**:` header) plus any reviewer whose Code Review Results verdict was `RESOLVED` or `ESCALATED`. CLEAR reviewers skip reflection. Deduplicate.

**Spawn each** via Task tool (`name` and `subagent_type` both set to the specialist name). Include `docs/specialist-knowledge/{name}/INDEX.md` under a `## Navigation` header.

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

Spawn **dry-reviewer only**. Send this prompt verbatim:

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

## Phase 3: Finalize

1. `TaskUpdate(taskId=<close-story-task-id>, status="completed")`.

2. Stage — **narrow scope only**:
   ```bash
   git add docs/specialist-knowledge/ docs/devloop-outputs/
   ```
   Never `git add -A`. The Phase 5 story-file update lands post-commit.

   **Unexpected-modification check**: run `git status --short` before committing. If modified files sit outside `docs/specialist-knowledge/` and `docs/devloop-outputs/`, escalate to the user — a specialist may have touched unrelated files, or ambient work belongs elsewhere.

3. Commit (heredoc avoids shell-expansion of anything read from files; trailer order matches /devloop Step 8):

   ```bash
   git commit -m "$(cat <<'EOF'
   Close user story: <story-title>

   Story: docs/user-stories/<story-slug>.md
   Devloops: <comma-separated list of devloop slugs>
   Tasks-closed: <count>

   Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
   EOF
   )"
   ```

   If nothing to commit (reflection produced no INDEX changes), skip silently and note "no commit (no changes)" in the report.

4. `git push`. The harness permission model prompts the user. No in-skill confirmation UI, no dry-run preview, no retry-on-deny — deny is terminal. No `--force`, no `--no-verify`.

## Phase 4: Pull Request

**Enumerate branch commits**:

```bash
git log --oneline "$(git merge-base HEAD main)..HEAD"
```

Categorize each: **story-devloop** (references a `docs/devloop-outputs/{slug}/` path or has a `Devloop:` trailer matching the story's Devloop Tracking table), **story-close** (this skill's own commit), or **adjacent** (anything else).

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
- Missing devloop output dir → hard-fail with the path and the Devloop Tracking row that referenced it.
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

Update the story file: set `**Status**:` to `Complete`; append the PR URL as a new `**PR**: <url>` line under the header.

**Report**:

```
**Story closed**: {story-title}

Devloops: {count} ({comma-separated slugs})
PR: {URL}
TODO additions: {count from Phase 2.5}
Reflection: {count of specialists who updated INDEX}
Ownership-lens retrospective: docs/devloop-outputs/{story-slug}-story-close/ownership-lens-retrospective.md
```

Conditional output: print the retrospective line only if the file exists (substitute `skipped (timeout)` or omit). If Phase 3 was a no-op commit, add `Commit: none (no working-tree changes)`. Flag any other skipped phases inline.

## Limits

| Phase | Limit | Action |
|-------|-------|--------|
| Phase 1 | — | Block immediately on incomplete tasks |
| Phase 2 | 20 min per specialist | Proceed without; note in report |
| Phase 2 INDEX guard | 3 retries | Fail close — do not commit |
| Phase 2.5 | 15 min | Proceed without; note in report |
| Phase 3 (commit/push), Phase 4 (PR) | — | Harness permission prompts; deny is terminal |

## Files

- Story file: `docs/user-stories/{story-slug}.md` (exact filename resolved via glob at Phase 1)
- Devloop outputs: `docs/devloop-outputs/{devloop-slug}/main.md`
- Story-close output: `docs/devloop-outputs/{story-slug}-story-close/ownership-lens-retrospective.md`
- Specialist INDEX: `docs/specialist-knowledge/{name}/INDEX.md`
- Upstream: `.claude/skills/devloop/SKILL.md` (reflection removed), `.claude/skills/user-story/SKILL.md` Step 10.5 (creates this task blocked by devloops)
