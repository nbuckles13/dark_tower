---
name: dev-loop
description: Overview of the dev-loop workflow and available steps. Use when user asks about dev-loop or needs guidance on which step to run.
---

# Dev-Loop Workflow Overview

The dev-loop is a multi-step workflow for implementing features with specialist ownership, verification, code review, and reflection. Each step is a separate skill that must be explicitly invoked.

## Available Dev-Loop Steps

| Step | Skill | Purpose |
|------|-------|---------|
| 0 | `/dev-loop-init` | Initialize a new dev-loop: create output dir, match principles, preview specialist prompt |
| 1 | `/dev-loop-implement` | Spawn the implementing specialist with injected context |
| 2 | `/dev-loop-validate` | Run 7-layer verification on specialist's work |
| 3 | `/dev-loop-review` | Spawn 4 code reviewers in parallel |
| 4 | `/dev-loop-reflect` | Resume specialists sequentially for reflection |
| 5 | `/dev-loop-complete` | Mark loop complete, summarize results |

## Utility Steps

| Skill | Purpose |
|-------|---------|
| `/dev-loop-status` | Check current state of any dev-loop (read-only) |
| `/dev-loop-fix` | Resume specialist to fix validation or review findings |
| `/dev-loop-restore` | Recover an interrupted dev-loop from checkpoint files |

## Typical Flow

```
/dev-loop-init "task description"
    ↓
/dev-loop-implement
    ↓
/dev-loop-validate
    ↓ (if pass)              ↓ (if fail)
/dev-loop-review         /dev-loop-fix → /dev-loop-validate
    ↓ (if approved)          ↓ (if findings)
/dev-loop-reflect        /dev-loop-fix → /dev-loop-validate
    ↓
/dev-loop-complete
```

## Starting a New Dev-Loop

To start a new dev-loop, run:

```
/dev-loop-init "your task description here"
```

The init step will:
1. Create an output directory at `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/`
2. Match your task to principle categories
3. Show you the specialist prompt that will be used
4. Tell you to run `/dev-loop-implement` next

## Checking Current State

To see the current state of a dev-loop, run:

```
/dev-loop-status
```

This will show you:
- Active dev-loops (if any)
- Current step
- Iteration count
- What to run next

## Reference Documentation

For full workflow details, see:
- `.claude/workflows/development-loop.md` - State machine and step details
- `.claude/workflows/development-loop/*.md` - Step-specific instructions
- `.claude/workflows/code-review.md` - Code review process
- `docs/dev-loop-outputs/_template/main.md` - Output file format

---

**Next step**: Based on what you need:
- Starting new work? → Run `/dev-loop-init "task description"`
- Checking status? → Run `/dev-loop-status`
- Resuming work? → Run `/dev-loop-restore`
