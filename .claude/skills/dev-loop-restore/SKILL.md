---
name: dev-loop-restore
description: Recover an interrupted dev-loop from checkpoint files. Use when resuming after session ended.
disable-model-invocation: true
---

# Dev-Loop Restore

Restore a dev-loop that was interrupted (session ended, context compressed, process killed). This skill:
1. Finds incomplete dev-loops
2. Shows checkpoint context
3. Guides user to resume from current state

**Key principle**: Agent context is lost, but checkpoint files preserve working notes. Restored specialists have "good enough" context to continue.

## Arguments

```
/dev-loop-restore [output-dir]
```

- **output-dir** (optional): Specific dev-loop to restore

## Instructions

### Step 1: Find Incomplete Dev-Loops

If output-dir not provided:

1. Run `./scripts/workflow/dev-loop-status.sh --active-only`
2. This returns all loops with `Phase != complete`
3. Display the list to the user

### Step 2: Show Restore Options

#### If No Incomplete Loops

```
**No incomplete dev-loops found**

All dev-loops in `docs/dev-loop-outputs/` are complete.

To start a new dev-loop, run:
  /dev-loop "task description"
```

#### If One Incomplete Loop

```
**Found incomplete dev-loop**

Directory: {output_dir}
Task: {task from main.md}
Phase: {phase}
Iteration: {iteration}
Implementing specialist: {specialist}

**Checkpoint files available**:
{list .md files in directory}

**Restore this loop?** (y/n)
```

Wait for user confirmation.

#### If Multiple Incomplete Loops

```
**Found multiple incomplete dev-loops**

| # | Directory | Task | Phase | Iteration |
|---|-----------|------|-------|-----------|
| 1 | {dir1} | {task1} | {phase1} | {iter1} |
| 2 | {dir2} | {task2} | {phase2} | {iter2} |
...

Which loop would you like to restore? (1-N, or 'none')
```

Wait for user selection.

### Step 3: Read Checkpoint Files

For the selected loop, read all checkpoint files:

- `main.md` - Loop State, task context
- `{specialist}.md` - Implementing specialist's working notes
- `security.md`, `test.md`, `code-reviewer.md`, `dry-reviewer.md`, `operations.md` - Reviewer checkpoints (if they exist)

### Step 4: Show Restoration Context

```
**Restoration Context**

**Task**: {task description from main.md}

**Loop State**:
| Field | Value |
|-------|-------|
| Phase | {phase} |
| Iteration | {iter} |
| Implementing Specialist | {specialist} |

**Implementing Specialist Checkpoint**:
{summary of checkpoint content - patterns, gotchas, status}

**Reviewer Checkpoints** (if any):
- Security: {exists/missing} - {verdict if exists}
- Test: {exists/missing} - {verdict if exists}
- Code Reviewer: {exists/missing} - {verdict if exists}
- DRY Reviewer: {exists/missing} - {verdict if exists}
- Operations: {exists/missing} - {verdict if exists}

**Recommended action**: Based on current phase, run `/dev-loop --restore {output-dir}`
```

### Step 5: Recommend Next Action

Based on Phase:

| Phase | Recommendation |
|-------|----------------|
| `setup` | Run `/dev-loop` again with checkpoint injection |
| `planning` | Spawn new team, inject checkpoint, resume planning |
| `implementation` | Spawn new team, inject checkpoint, resume implementation |
| `review` | Spawn new team, inject checkpoint, resume review |
| `reflection` | Spawn new team, complete reflection |

**Restoration** requires re-spawning the full team with checkpoint context. See "Restoration Process" section below.

### Step 6: Update for New Session

Since agent IDs from previous session are no longer valid:

1. Note that previous agent IDs won't work (can't resume)
2. Skills will use checkpoint injection for fresh agents

```
**Note**: Previous agent IDs are no longer valid (session ended).

When you run `/dev-loop`, specialists will be re-invoked fresh with their checkpoint context injected. This preserves most working knowledge.

Ready to continue?
```

### Step 7: Guide to Next Step

```
**Restoration Ready**

Phase: {phase}

**Next step**: Run `/dev-loop --restore {output-dir}`
```

## Checkpoint Injection Pattern

When restoring a specialist (used by other skills):

```markdown
# Context Recovery for {Specialist}

You are continuing a dev-loop that was interrupted. Here's your previous context:

## Your Previous Working Notes

{paste from checkpoint file: Patterns, Gotchas, Decisions, Observations}

## Current Loop State

- Phase: {phase}
- Iteration: {iteration}

## What's Already Complete

{summary from main.md: Task Overview, Implementation Summary if present}

## Your Task

Continue from where you left off. Based on your working notes, {specific instruction for current phase}.
```

## Restoration Process

1. **Read all checkpoint files**:
   - `main.md` - Loop state, task context, decisions
   - `{implementing-specialist}.md` - Implementer notes
   - `security.md`, `test.md`, `code-reviewer.md`, `dry-reviewer.md`, `operations.md` - Reviewer states

2. **Compose restoration prompts** with checkpoint injection:

**For Implementer**:
```
You are resuming an interrupted dev-loop.

## Previous Context

{paste from main.md: Task Overview, Implementation Summary}

## Your Previous Working Notes

{paste from {specialist}.md checkpoint}

## Current State

Phase: {phase}
Iteration: {iteration}
What's been done: {summary}
What's remaining: {based on phase}

## Your Task

Continue from phase "{phase}". Based on your notes:
{specific instructions for current phase}
```

**For Reviewers** (if review phase):
```
You are resuming an interrupted review.

## Your Previous Review

{paste from {reviewer}.md if exists}

## Current State

Your verdict: {pending or previous verdict}
Other reviewers: {status}

## Your Task

{Continue review / Re-send verdict / etc.}
```

3. **Re-spawn team** via `/dev-loop` with `--restore` flag:
   - Lead spawns all 7 teammates with restoration prompts
   - Team resumes from current phase
   - Checkpoints continue accumulating

## Critical Constraints

- **Agent IDs invalid**: Previous session's agent IDs cannot be resumed
- **Checkpoint context**: Use checkpoint files to reconstruct context
- **User confirmation**: Always get user confirmation before restoration
- **Preserve Loop State**: Don't modify Loop State during restore; let the next skill handle it

## Checkpoint File Locations

```
{output_dir}/
├── main.md                       # Loop State with Phase field
├── {implementing-specialist}.md  # Implementer checkpoint
├── security.md                   # Security reviewer
├── test.md                       # Test reviewer
├── code-reviewer.md              # Code Quality reviewer
├── dry-reviewer.md               # DRY reviewer
└── operations.md                 # Operations reviewer
```

## Limitations

Restored specialists have checkpoint context but not full memory from the original session. The restoration is "good enough" for:
- Continuing implementation work
- Understanding what was done
- Making informed decisions

It may miss nuances from the original session. This is acceptable - the alternative is starting over completely.

### Limitations

- Team composition must match original (same 7 roles)
- Reviewer-to-reviewer discussions from original session are lost
- Implementer-reviewer discussions from original session are lost
- Verdicts and confirmations from checkpoint files are preserved

---

**Next step**: Run the recommended action
- `/dev-loop --restore {output-dir}` (re-spawns team with context)
