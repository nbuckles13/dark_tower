---
name: dev-loop-restore
description: Recover an interrupted dev-loop from checkpoint files. Use when resuming after session ended.
disable-model-invocation: true
---

# Dev-Loop Restore

Restore a dev-loop that was interrupted (session ended, context compressed, process killed). This skill:
1. Finds incomplete dev-loops
2. Shows checkpoint context
3. Allows user to resume from current step

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
2. This returns all loops with `Current Step != complete`
3. Display the list to the user

### Step 2: Show Restore Options

#### If No Incomplete Loops

```
**No incomplete dev-loops found**

All dev-loops in `docs/dev-loop-outputs/` are complete.

To start a new dev-loop, run:
  /dev-loop-init "task description"
```

#### If One Incomplete Loop

```
**Found incomplete dev-loop**

Directory: {output_dir}
Task: {task from main.md}
Current step: {current_step}
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

| # | Directory | Task | Step | Iteration |
|---|-----------|------|------|-----------|
| 1 | {dir1} | {task1} | {step1} | {iter1} |
| 2 | {dir2} | {task2} | {step2} | {iter2} |
...

Which loop would you like to restore? (1-N, or 'none')
```

Wait for user selection.

### Step 3: Read Checkpoint Files

For the selected loop, read all checkpoint files:

- `main.md` - Loop State, task context
- `{specialist}.md` - Implementing specialist's working notes
- `security.md`, `test.md`, `code-reviewer.md`, `dry-reviewer.md` - Reviewer checkpoints (if they exist)

### Step 4: Show Restoration Context

```
**Restoration Context**

**Task**: {task description from main.md}

**Loop State**:
| Field | Value |
|-------|-------|
| Current Step | {step} |
| Iteration | {iter} |
| Implementing Specialist | {specialist} |

**Implementing Specialist Checkpoint**:
{summary of checkpoint content - patterns, gotchas, status}

**Reviewer Checkpoints** (if any):
- Security: {exists/missing} - {verdict if exists}
- Test: {exists/missing} - {verdict if exists}
- Code Reviewer: {exists/missing} - {verdict if exists}
- DRY Reviewer: {exists/missing} - {verdict if exists}

**Recommended action**: Based on current step, run `{recommended skill}`
```

### Step 5: Recommend Next Action

Based on Current Step:

| Current Step | Recommendation |
|--------------|----------------|
| `init` | Run `/dev-loop-implement` |
| `implementation` | Specialist was interrupted. Run `/dev-loop-implement` to re-invoke with checkpoint context |
| `validation` | Run `/dev-loop-validate` |
| `code_review` | Run `/dev-loop-review` |
| `reflection` | Run `/dev-loop-reflect` |

### Step 6: Update for New Session

Since agent IDs from previous session are no longer valid:

1. Note that previous agent IDs won't work (can't resume)
2. Skills will use checkpoint injection for fresh agents

```
**Note**: Previous agent IDs are no longer valid (session ended).

When you run the next skill, specialists will be re-invoked fresh with their checkpoint context injected. This preserves most working knowledge.

Ready to continue?
```

### Step 7: Guide to Next Step

```
**Restoration Ready**

Current step: {step}

**Next step**: Run `{recommended skill}`

Example:
  {skill command}
```

## Checkpoint Injection Pattern

When restoring a specialist (used by other skills):

```markdown
# Context Recovery for {Specialist}

You are continuing a dev-loop that was interrupted. Here's your previous context:

## Your Previous Working Notes

{paste from checkpoint file: Patterns, Gotchas, Decisions, Observations}

## Current Loop State

- Step: {current_step}
- Iteration: {iteration}

## What's Already Complete

{summary from main.md: Task Overview, Implementation Summary if present}

## Your Task

Continue from where you left off. Based on your working notes, {specific instruction for current step}.
```

## Critical Constraints

- **Agent IDs invalid**: Previous session's agent IDs cannot be resumed
- **Checkpoint context**: Use checkpoint files to reconstruct context
- **User confirmation**: Always get user confirmation before restoration
- **Preserve Loop State**: Don't modify Loop State during restore; let the next skill handle it

## Checkpoint File Locations

```
{output_dir}/
├── main.md                       # Loop State, task context
├── {implementing-specialist}.md  # Implementing specialist checkpoint
├── security.md                   # Security reviewer (if review started)
├── test.md                       # Test reviewer (if review started)
├── code-reviewer.md              # Code reviewer (if review started)
└── dry-reviewer.md               # DRY reviewer (if review started)
```

## Limitations

Restored specialists have checkpoint context but not full memory from the original session. The restoration is "good enough" for:
- Continuing implementation work
- Understanding what was done
- Making informed decisions

It may miss nuances from the original session. This is acceptable - the alternative is starting over completely.

---

**Next step**: Run `/{recommended-skill}`
