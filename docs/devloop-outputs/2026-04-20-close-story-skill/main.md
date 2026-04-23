# Devloop Output: /close-story skill + /devloop & /user-story integration

**Date**: 2026-04-20
**Task**: Create /close-story skill; remove reflection phase from /devloop; add /close-story task to /user-story decomposition
**Specialist**: code-reviewer
**Mode**: Agent Teams — full (6 teammates: code-reviewer is implementer; 5 reviewers + no separate code-reviewer slot)
**Branch**: `feature/dashboard-owner-debate`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d92830af3278fd815d0293f573f4639ed1b34bcc` |
| Branch | `feature/dashboard-owner-debate` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `code-reviewer` (team torn down on WSL crash; not re-spawned) |
| Implementing Specialist | `code-reviewer` |
| Iteration | `2` (teammate review → user review) |
| Duration | 2026-04-20 start → 2026-04-23 close (multi-day; WSL crash mid-iteration-2) |
| Security | RESOLVED — 2 findings fixed |
| Test | CLEAR — 3 iterations, all resolved |
| Observability | CLEAR — tech debt cleared |
| Code Quality | N/A — implementer is code-reviewer |
| DRY | CLEAR — 1 tech-debt observation spun to TODO.md |
| Operations | CLEAR — late catch resolved |

---

## Task Overview

### Objective

Three coordinated skill-level changes to make story-closing a first-class action:

1. Create `.claude/skills/close-story/SKILL.md` — new skill with 5 phases: Verify → Reflection → DRY retrospective → Finalize → PR
2. Remove reflection phase (Step 8) from `.claude/skills/devloop/SKILL.md` — moves to story level
3. Modify `.claude/skills/user-story/SKILL.md` — add final `/close-story` TaskCreate at end of decomposition with `addBlockedBy` on all devloop tasks

### Scope

- **Service(s)**: None (skill files only)
- **Schema**: No
- **Cross-cutting**: Yes — affects all future devloops and user stories

### Debate Decision

NOT NEEDED — design discussion conducted in-session 2026-04-20 with explicit user approval. See session transcript.

---

## Cross-Boundary Classification

<!-- Per ADR-0024 §6. All touched files are in code-reviewer's domain (skill files). -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `.claude/skills/close-story/SKILL.md` | Mine | — |
| `.claude/skills/devloop/SKILL.md` | Mine | — |
| `.claude/skills/user-story/SKILL.md` | Mine | — |
| `docs/devloop-outputs/_template/main.md` | Mine | — |
| `docs/devloop-outputs/2026-04-20-close-story-skill/main.md` | Mine | — |
| `docs/TODO.md` | Mine | — |

Rationale: all three skill files and the devloop output template are code-reviewer-owned (skill/devloop surface). No cross-boundary edits, no GSA paths. The template edit is a mechanical downstream consequence of the /devloop reflection removal (template must match the workflow surface).

---

## Planning

In-session planning with user on 2026-04-20 — no separate planning-phase teammate pass. Three-file coordinated change designed together (new `/close-story` skill + `/devloop` reflection removal + `/user-story` task-list creation), then implemented in this single devloop. See "Debate Decision" above.

---

## Implementation Summary

### File 1: `.claude/skills/devloop/SKILL.md` (reflection removed)

| Change | Before | After |
|--------|--------|-------|
| Step 8 Reflection section | 25 LOC unicast-prompt + INDEX-guard-rerun block | deleted |
| Workflow Overview arrow | `REFLECTION [skipped --light] →` | `COMMIT →` |
| Lightweight Mode Skips line | `Gate 1 (plan approval), reflection phase` | `Gate 1 (plan approval)` |
| Limits table row | `Reflection \| 15 min \| Proceed without` | removed |
| Step 7 Gate 3 branching | `Phase = reflection (full) or complete (light)` + full/light split | unified: `Phase = complete` → Step 8 Commit |
| Step 8.5 → Step 8 (Commit) | "After reflection (full) or after review (light)" | "After review" |
| Commit Co-Authored-By | `Claude Opus 4.6` | `Claude Opus 4.7 (1M context)` |
| Step 8.9 → Step 8.5 (Cleanup Team) | Step 8.9 | Step 8.5 |
| Step 9 cross-reference | "Step 8.5 commit" | "Step 8 commit" |
| `--light` argument help | "skip planning gate and reflection" | "skip planning gate" |
| Defensive-cleanup step-ref | "Step 8.9" | "Step 8.5 (Cleanup Team)" |
| Implementer prompt Workflow | had step 6 REFLECTION | removed (now 5 steps) |
| Reviewer prompt Workflow | had step 7 REFLECTION | removed (now 6 steps) |
| Story-scope reflection pointer | — | added after Workflow Overview + at end of Step 9 |

**Net LOC**: 634 → 600 (−34 lines). Stretch goal (net-smaller) met.

Only remaining "reflection" references: the single paragraph pointer to `/close-story` Phase 2 after the Workflow Overview, and the end-of-Step-9 note. Both intentional — they preserve context for future readers about where reflection now lives.

### File 2: `.claude/skills/user-story/SKILL.md` (task creation + close-story task)

New **Step 10.5: Create Devloop Task List** inserted between existing Step 10 (Write Story File) and Step 11 (Report and Review). Content:

1. Lead calls `TaskCreate` per devloop task (subject: `Run /devloop "{task}" --specialist={name}`).
2. Lead chains dependencies via `TaskUpdate addBlockedBy` per plan's Dependencies column.
3. Lead calls `TaskCreate` for the close-story task (subject: `Run /close-story {story-slug}`).
4. Lead calls `TaskUpdate addBlockedBy` on the close-story task with every devloop task ID.

Step 11 output updated: adds `Task list: N devloop tasks + 1 close-story task` line, adds `/close-story` invocation in Next Step, adds forcing-function explanation.

**Net LOC**: 449 → 472 (+23 lines, ~22 LOC of new Step 10.5 prose + 6 LOC in Step 11 output).

### File 3: `.claude/skills/close-story/SKILL.md` (new skill, 327 lines)

Five phases per task spec:

- **Phase 1 Verify** — slug regex validation (`^\d{4}-\d{2}-\d{2}-[a-z0-9-]+$`) before any file read; load story file; `TaskList`; every non-close-story task must be `completed`; escalate to user with specifics on failure.
- **Phase 2 Story-scope Reflection** — identify participating specialists from each devloop main.md (implementer + non-CLEAR reviewers); spawn as teammates; send reflection prompt (distinct wording from deleted Step 8 — framed as cross-devloop shift consolidation); 20-min timeout; re-run `validate-knowledge-index.sh`; fail phase if guard fails. Cross-refs ADR-0024 §6.3 for cross-boundary INDEX edits.
- **Phase 2.5 DRY Ownership Lens Retrospective** — single-specialist pass (dry-reviewer only); literal prompt text in SKILL.md (pasted verbatim, not "per spec"); output `ownership-lens-retrospective.md` ≤30 lines with 4 fixed sections; non-blocking.
- **Phase 3 Finalize** — `TaskUpdate` close-story task to completed; `git add -A`; commit via heredoc (per CLAUDE.md convention) with Co-Authored-By: Claude Opus 4.7 (1M context); `git push` (harness permission prompt).
- **Phase 4 PR** — `git log` enumerate commits from `merge-base HEAD main`; categorize story-devloop / story-close / adjacent; extract structured fields only from each devloop main.md (Task header, verdicts, Implementation Summary prose, Cross-Boundary Classification, Tech Debt); explicit Do-NOT-include list (env dumps, log tails, teammate transcripts, freeform reflection notes); body via `--body-file <(cat <<'EOF' ... EOF)` to prevent shell expansion of variables read from main.md; `gh pr list` to detect existing, `gh pr edit` or `gh pr create` accordingly.
- **Phase 5 Complete** — update story file (Status: Complete, PR URL); report summary.

Security incorporated per @security feedback:
1. Phase 1 regex slug validation before any path join.
2. Phase 4 structured-field extraction with explicit "Do NOT include" list (no freeform blob concat).
3. Phase 4 heredoc/`--body-file` hygiene in all `gh` invocations.

DRY incorporated per @dry-reviewer feedback:
1. Phase 3 commit message has distinct shape from /devloop Step 8's commit (story title + devloop slugs + story file path vs task desc + single slug + specialist + mode + verdicts) — not Pattern A duplication.
2. Phase 2.5 prompt text pasted verbatim (cold-executable).
3. Phase 2 reflection prompt avoids verbatim-lifting from the deleted Step 8 — reframed for cross-devloop shift consolidation while preserving the pointers-only INDEX discipline (unavoidable overlap, not duplication).

Operations / observability / test: no separate must-fix items; plan approved on their reviews.

---

## Code Review Results

| Reviewer | Verdict | Must-Fix | Deferred | Notes |
|----------|---------|----------|----------|-------|
| Security | RESOLVED | 2 | 0 | Findings fixed: (1) slug regex validation in Phase 1 before any path join; (2) structured-field extraction in Phase 4 with explicit Do-NOT-include list + heredoc/`--body-file` hygiene for `gh` invocations |
| Test | CLEAR | 0 | 0 | 3 iterations to reach clean; all raised findings resolved |
| Observability | CLEAR | 0 | 0 | Tech-debt observation cleared during iteration |
| Code Quality | N/A | — | — | Implementer is code-reviewer; no separate code-quality slot |
| DRY | CLEAR | 0 | 1 | 1 story-level tech-debt observation spun to `docs/TODO.md` (broader file-type ownership map — see Tech Debt below). Pattern-A-distinguishing commit shape in Phase 3; verbatim Phase-2.5 prompt; Phase-2 prompt reframed for cross-devloop scope (not lifted from deleted Step 8) |
| Operations | CLEAR | 0 | 0 | Late catch resolved during iteration |

User-review iteration 2 findings (applied on 2026-04-23 after WSL recovery):
- Slug arg surface relaxed from `^\d{4}-\d{2}-\d{2}-[a-z0-9-]+$` to `^[a-z0-9-]+$`, with glob-based story-file lookup; both short (`billing-portal`) and full (`2026-04-20-billing-portal`) forms accepted. `/user-story` Step 10.5 + Step 11 output updated to match.
- Skill prose tightened throughout (intro, phase headers, Limits table); "When NOT to Use" condensed inline; "Design Rationale" section added.
- `docs/devloop-outputs/_template/main.md` added to Cross-Boundary Classification table (template edit is a mechanical downstream consequence of the /devloop reflection removal).

---

## Tech Debt

- **Cross-boundary ownership coverage beyond GSA — broader file-type ownership map** (DRY observation, surfaced during this devloop): ADR-0024 §6 / `cross-boundary-ownership.yaml` cover only GSAs (Tier 1). Non-GSA cross-boundary work (skills, infra, docs, agent defs, runbooks, CI, observability conventions) has no mechanical ownership map. Fine/coarse granularity tension noted. Captured as `docs/TODO.md:108` with "revisit when bandwidth" framing. Needs a design debate before implementation. Owners: code-reviewer + operations + dry-reviewer.

---

## Rollback Procedure

1. Start commit: `d92830af3278fd815d0293f573f4639ed1b34bcc`
2. `git diff d92830a..HEAD`
3. `git reset --hard d92830a` if needed

---

## Reflection

This devloop ran under the new post-change workflow — per-devloop reflection was skipped (Step 8 no longer exists). Story-scope reflection would normally happen via `/close-story` Phase 2, but this devloop is a standalone skill-surface change, not part of a user story; the Gate 2 INDEX guard (`validate-knowledge-index.sh`) remains the safety net for any INDEX pointers added during implementation.
