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
| 0.5 | `/dev-loop-plan` | (Optional) Spawn specialist for exploration and planning before implementation |
| 1 | `/dev-loop-implement` | Spawn (or resume) the implementing specialist with injected context |
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

### Standard Flow (no planning)

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

### Planning Flow (for complex tasks)

```
/dev-loop-init --plan
    ↓
/dev-loop-plan  ←─────────────┐
    ↓                         │
    ├─ (needs clarification) ─┘
    ├─ (recommend escalation) → consider debate workflow
    ↓ (ready)
/dev-loop-implement (resumes same specialist with planning context)
    ↓
... (same as standard flow)
```

The same specialist handles both planning and implementation, preserving context.

## Starting a New Dev-Loop

### Standard Start (direct to implementation)

```
/dev-loop-init "your task description here"
```

### With Planning Phase (for complex tasks)

```
/dev-loop-init "your task description here" --plan
# or just:
/dev-loop-init --plan
```

The init step will:
1. Create an output directory at `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/`
2. Match your task to principle categories
3. Show you the specialist prompt that will be used
4. Tell you to run `/dev-loop-plan` (if `--plan`) or `/dev-loop-implement` next

### When to Use Planning

Use `--plan` when:
- Task scope is unclear and needs exploration
- You want the specialist to propose an approach before implementing
- Task might be too large and need escalation to debate workflow

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
- `.claude/skills/dev-loop-*/SKILL.md` - Individual step instructions
- `.claude/workflows/code-review.md` - Code review process
- `docs/dev-loop-outputs/_template/main.md` - Output file format
- `docs/decisions/adr-0022-skill-based-dev-loop.md` - Design rationale

---

**Next step**: Based on what you need:
- Starting new work? → Run `/dev-loop-init "task description"`
- Complex task needing exploration? → Run `/dev-loop-init --plan`
- Checking status? → Run `/dev-loop-status`
- Resuming work? → Run `/dev-loop-restore`
