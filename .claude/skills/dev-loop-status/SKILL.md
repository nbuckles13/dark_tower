---
name: dev-loop-status
description: Check current state of dev-loops. Read-only utility to see active loops and their progress.
---

# Dev-Loop Status Check

This skill checks for active or incomplete dev-loops and reports their current state. It's read-only and safe to run at any time.

## Instructions

### Step 1: Run the Status Script

Use the helper script to scan all dev-loop directories and extract their state:

```bash
./scripts/workflow/dev-loop-status.sh
```

**Options**:
- `--active-only` - Only show active (non-complete) loops
- `--complete-only` - Only show completed loops
- `--format tsv` - Tab-separated output for parsing
- `--format json` - JSON output

The script extracts from each `main.md`:
- Current Step (init, planning, implementation, validation, code_review, reflection, complete)
- Implementing Specialist
- Iteration number
- Agent ID
- Task description

### Step 2: Interpret Results and Recommend Next Action

The script will show active and completed loops. Based on the **Current Step** of active loops, recommend:

| Current Step | Next Action |
|--------------|-------------|
| `init` | Run `/dev-loop-implement` to spawn the specialist |
| `planning` | Run `/dev-loop-plan` to continue, or `/dev-loop-implement` if plan approved |
| `implementation` | Specialist running or interrupted. Run `/dev-loop-restore` if interrupted |
| `validation` | Run `/dev-loop-validate` to verify the implementation |
| `code_review` | Run `/dev-loop-review` to complete code review |
| `reflection` | Run `/dev-loop-reflect` to capture learnings |

### Step 3: Check Checkpoint Files (if needed)

For active loops, you can list checkpoint files:

```bash
ls docs/dev-loop-outputs/{directory}/
```

Expected files:
- `main.md` - Primary tracking document
- `{specialist}.md` - Specialist checkpoint (if implementation started)
- `{reviewer}.md` - Reviewer checkpoints (security.md, test.md, code-reviewer.md, dry-reviewer.md)

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

After running the script, report the status to the user and recommend the appropriate next skill based on the current step.
