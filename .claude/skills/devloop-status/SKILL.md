---
name: devloop-status
description: Check current state of devloops. Read-only utility to see active loops and their progress.
---

# Dev-Loop Status Check

This skill checks for active or incomplete devloops and reports their current state. It's read-only and safe to run at any time.

## Instructions

### Step 1: Run the Status Script

Use the helper script to scan all devloop directories and extract their state:

```bash
./scripts/workflow/devloop-status.sh
```

**Options**:
- `--active-only` - Only show active (non-complete) loops
- `--complete-only` - Only show completed loops
- `--format tsv` - Tab-separated output for parsing
- `--format json` - JSON output

The script extracts from each `main.md`:
- Phase (setup, planning, implementation, review, reflection, complete)
- Implementing Specialist
- Iteration number
- Task description

### Step 2: Interpret Results and Recommend Next Action

The script will show active and completed loops. Based on the **Phase** of active loops, recommend:

| Phase | Next Action |
|-------|-------------|
| `setup` | Run `/devloop` again to restart |
| `planning` | Run `/devloop` again to restart (main.md records start commit for rollback) |
| `implementation` | Run `/devloop` again to restart |
| `review` | Run `/devloop` again to restart |
| `reflection` | Run `/devloop` again to restart |

### Step 3: Check Checkpoint Files (if needed)

For active loops, you can list checkpoint files:

```bash
ls docs/devloop-outputs/{directory}/
```

Expected files:
- `main.md` - Primary tracking document
- `{specialist}.md` - Specialist checkpoint (if implementation started)
- `{reviewer}.md` - Reviewer checkpoints (security.md, test.md, code-reviewer.md, dry-reviewer.md, operations.md)

## Auto-Detection Logic

When other devloop skills need to find the "current" output directory, they use this logic:

1. List directories in `docs/devloop-outputs/` (excluding `_template`)
2. Filter to those with `Phase != complete` in `main.md`
3. If exactly one active loop: use it
4. If multiple active loops: ask user which one
5. If no active loops: error (no loop in progress)

Skills can accept an explicit path argument to override auto-detection.

---

After running the script, report the status to the user and recommend the appropriate next action based on the current phase.
