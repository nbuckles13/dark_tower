---
name: story-run
description: Chain devloops for a user story. Reads dependency graph, runs tasks sequentially.
---

# Story Run

Orchestrates sequential devloop execution for a user story. Reads the task
dependency graph and chains /devloop invocations automatically.

## Arguments

```
/story-run <user-story.md>               # start from first Pending task
/story-run <user-story.md> --from=N      # start from task N
/story-run <user-story.md> --to=N        # stop after task N
/story-run <user-story.md> --dry-run     # show execution plan only
```

## Instructions

### Step 1: Parse User Story

Read the user story file. Extract the task-metadata YAML block (fenced
code block containing `# task-metadata`). This gives task ID, slug,
description, specialist, and dependencies.

Build the dependency graph. Validate all dep references are valid task IDs.

If no task-metadata block found, fall back to parsing the Implementation
Plan markdown table.

### Step 2: Initialize

Read the Devloop Tracking table to get current task statuses (Pending,
Completed, Failed).

- --from=N: treat tasks before N as Completed for dependency resolution
  without changing their status in the file
- --to=N: stop after completing task N

### Step 3: Main Loop

Repeat until all tasks completed or all remaining tasks blocked:

1. **Select next task**: Find tasks where status=Pending and all deps
   Completed. When multiple are ready, prefer tasks on the critical path
   (most downstream dependents), then lowest ID as tiebreaker.
   If none ready, report blocked tasks and stop.

2. **Show status**: Print progress — completed, current, upcoming, blocked.

3. **Record rollback point**: `git rev-parse HEAD`

4. **Build devloop prompt**: Construct the /devloop invocation with task
   description, --specialist from task-metadata, the relevant ### section
   under ## Design, and requirements covered by this task.

5. **Invoke /devloop**: Spawn a general-purpose subagent as devloop lead
   using the same model as the current session:

   ```
   Agent(
     name: "devloop-lead",
     subagent_type: "general-purpose",
     model: {same model as current session},
     prompt: "{SKILL.md contents}\n\n{task + specialist + design context}"
   )
   ```

   The subagent creates a team, spawns teammates, manages all gates,
   commits the result (Step 8.5), and returns a summary. Each devloop
   runs in its own subprocess context — story-run's context grows by
   ~1 message per devloop.

6. **Handle result**:
   - **Success**: Verify new commit exists, update Devloop Tracking
     table (Status -> Completed, fill Devloop Output path and commit
     hash), tag commit `story-task-{N}`. Continue to next task.
   - **Failure**: `git reset --hard {rollback_point}`, update Devloop
     Tracking table (Status -> Failed), continue with non-dependent
     tasks. If none remain, stop.

### Step 4: Complete

- Push the story branch
- Update .devloop-pr.json with story-level PR metadata
- Print final summary: tasks completed, total duration, any failures

### Error Recovery

- **Re-run**: `/story-run` reads the Devloop Tracking table and skips
  Completed tasks automatically.
- **--from=N**: Skip earlier tasks for dependency purposes.
- **--to=N**: Run up to task N and stop.
- **Rollback on failure**: git reset --hard to pre-task state.

### Interaction with User

Runs autonomously; user monitors via --remote-control. If the user sends
a message during execution, pause the loop and respond. Resume with
user confirmation.
