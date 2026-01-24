---
name: dev-loop-status
description: Check current state of dev-loops. Read-only utility to see active loops and their progress.
---

# Dev-Loop Status Check

This skill checks for active or incomplete dev-loops and reports their current state. It's read-only and safe to run at any time.

## Instructions

### Step 1: Scan for Dev-Loop Directories

List all directories in `docs/dev-loop-outputs/` (excluding `_template`):

```bash
ls -d docs/dev-loop-outputs/*/ 2>/dev/null | grep -v _template
```

### Step 2: Check Each Directory for Loop State

For each directory found, read `main.md` and extract the Loop State table:

| Field | Value |
|-------|-------|
| Implementing Agent | `{agent_id}` |
| Implementing Specialist | `{specialist-name}` |
| Current Step | `{init|planning|implementation|validation|code_review|reflection|complete}` |
| Iteration | `{1-5}` |
| Security Reviewer | `{agent_id or pending}` |
| Test Reviewer | `{agent_id or pending}` |
| Code Reviewer | `{agent_id or pending}` |
| DRY Reviewer | `{agent_id or pending}` |

### Step 3: Classify Each Loop

- **Active**: Current Step is NOT `complete`
- **Complete**: Current Step is `complete`

### Step 4: Report Results

#### If No Dev-Loops Found

```
**Dev-Loop Status**: No dev-loops found

To start a new dev-loop, run:
  /dev-loop-init "task description"
```

#### If All Loops Are Complete

```
**Dev-Loop Status**: No active dev-loops

Completed loops:
- docs/dev-loop-outputs/2026-01-15-example-task/ (complete)
- ...

To start a new dev-loop, run:
  /dev-loop-init "task description"
```

#### If Active Loop(s) Found

For each active loop, report:

```
**Dev-Loop Status**: Active loop found

**Directory**: docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/
**Task**: {from main.md Task Overview section}
**Current Step**: {current_step}
**Iteration**: {iteration}
**Implementing Specialist**: {specialist-name}

**Checkpoint Files**:
- main.md
- {specialist}.md (if exists)
- {reviewer}.md (if exists for each reviewer)

**Next Action**:
{Based on Current Step, recommend next skill to run}
```

### Step 5: Recommend Next Action

Based on Current Step, recommend:

| Current Step | Next Action |
|--------------|-------------|
| `init` | `Run /dev-loop-implement to spawn the specialist.` |
| `planning` | `Run /dev-loop-plan to continue planning, or /dev-loop-implement if plan is approved.` |
| `implementation` | `Specialist still running or interrupted. Run /dev-loop-restore if interrupted.` |
| `validation` | `Run /dev-loop-validate to verify the implementation.` |
| `code_review` | `Run /dev-loop-review to complete code review.` |
| `reflection` | `Run /dev-loop-reflect to capture learnings.` |

## Auto-Detection Logic

When other dev-loop skills need to find the "current" output directory, they use this logic:

1. List directories in `docs/dev-loop-outputs/` (excluding `_template`)
2. Filter to those with `Current Step != complete` in `main.md`
3. If exactly one active loop: use it
4. If multiple active loops: ask user which one
5. If no active loops: error (no loop in progress)

Skills can accept an explicit path argument to override auto-detection.

### Planning State Detection

When `Current Step = planning`:
- Check if `Implementing Agent` is set (not `pending`)
  - If set: Planning in progress, can resume with `/dev-loop-plan` or proceed with `/dev-loop-implement`
  - If `pending`: Ready to start planning with `/dev-loop-plan`
- Check for `Planning Proposal` section in main.md
  - If present: Plan has been proposed, ready for `/dev-loop-implement`
  - If absent: Planning not yet started or in progress

---

**Next step**: {recommended action based on state}
