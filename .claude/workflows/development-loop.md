# Development Loop Workflow

The Development Loop is the primary workflow for implementing features. It combines specialist ownership, context injection, verification, code review, and reflection.

---

## When to Use

| Scenario | Use Loop? | Notes |
|----------|-----------|-------|
| Implement new feature | Yes | Standard flow |
| Bug fix | Yes | Unless trivial one-liner |
| Refactoring | Yes | Tests catch regressions |
| Documentation only | No | No verification needed |
| Exploration/research | No | No code to verify |

When starting, announce:
> *"Starting development loop (specialist-owned verification, max 5 iterations)"*

Record the start time for duration tracking.

---

## Step Announcements

Announce each step transition to keep the user informed of progress.

### Format

> **[Step Name]** | Duration: {elapsed} | {status info}

### Step-Specific Announcements

| Transition | Announcement |
|------------|--------------|
| → Implementation | **Implementation** \| Duration: 0m \| Iteration 1, invoking {specialist} specialist |
| → Validation | **Validation** \| Duration: {elapsed} \| Specialist returned, re-running verification |
| → Code Review | **Code Review** \| Duration: {elapsed} \| Validation passed, invoking reviewers |
| → Reflection | **Reflection** \| Duration: {elapsed} \| Code review approved, capturing learnings |
| → Complete | **Complete** \| Duration: {elapsed} \| All steps passed |

### Iteration Announcements

When returning to implementation after findings:
> **Implementation** | Duration: {elapsed} | Iteration {n}, fixing {count} findings from {reviewer}

### Failure Announcements

| Scenario | Announcement |
|----------|--------------|
| Validation failed | **Validation Failed** \| Duration: {elapsed} \| Layer {n} failed, resuming specialist |
| Code review blocked | **Code Review Blocked** \| Duration: {elapsed} \| {reviewer} raised BLOCKER, returning to implementation |
| Max iterations | **Loop Aborted** \| Duration: {elapsed} \| Max 5 iterations reached, escalating to user |

---

## Loop States

| State | Description | Next State | Step File |
|-------|-------------|------------|-----------|
| `implementation` | Specialist implements + verifies | `validation` | `development-loop/step-implementation.md` |
| `validation` | Orchestrator re-runs verification | `code_review` (pass) or `implementation` (fail) | `development-loop/step-validation.md` |
| `code_review` | 4 specialists review | `reflection` (approved) or `implementation` (findings) | `code-review.md` |
| `reflection` | Update knowledge files | `complete` | `development-loop/step-reflection.md` |
| `complete` | Done | - | - |

---

## Step Reference

| Step | File | When to Read |
|------|------|--------------|
| Implementation | `development-loop/step-implementation.md` | Starting a dev-loop |
| Validation | `development-loop/step-validation.md` | After implementation |
| Code Review | `code-review.md` | After validation passes |
| Reflection | `development-loop/step-reflection.md` | After code review approves |
| Output Format | `development-loop/output-documentation.md` | Creating output files |
| Recovery | `development-loop/session-restore.md` | Resuming interrupted loops |

---

## Before Switching Tasks

If user requests a different task while a dev-loop is in progress:
1. Check Loop State in the current output file
2. If Current Step is NOT `complete`:
   - Ask user: "We have an incomplete dev-loop at step '{step}'. Complete it first or pause?"

---

## Deadlock Handling

If specialists deadlock (e.g., code review disagreements that can't be resolved):
- Exit the loop
- Present the situation to the user
- Let the user decide how to proceed

---

## Step-Runner Architecture

The dev-loop uses a three-level agent architecture:

| Level | Role | Context |
|-------|------|---------|
| Orchestrator | State machine, step sequencing | This file only |
| Step-Runner | Execute one step | Step-specific file only |
| Specialist | Domain expertise | Specialist definition + dynamic knowledge |

**Key principle**: Each level only knows what it needs. Orchestrator doesn't know step details. Step-runners don't know overall state. Specialists don't know process.

---

## Step-Runner Invocation

### Standard Format

All step-runners are general-purpose agents invoked with this structure:

```markdown
## Step-Runner: {step_name}

**Your job**: Execute the {step_name} step of the dev-loop.

### Instructions

Read and follow: `.claude/workflows/{step_file}`

### Input

Task: {task_description}
Specialist: {specialist_name}
Output directory: {output_dir}
Action: {Start new specialist | Resume specialist {id}}
Findings to address: {list, if iteration 2+}

### Your Responsibilities

1. Read the step instructions file
2. Execute the action (start new or resume specialist)
3. Ensure specialist creates checkpoint at `{output_dir}/{specialist}.md`
4. Update `{output_dir}/main.md` with your step's section
5. Return structured output (see below)

### Expected Output

Respond with exactly this format:

status: success | failed
specialist_id: {agent ID from specialist you invoked}
files_created: {list of new files}
files_modified: {list of modified files}
checkpoint_exists: true | false
error: {if failed, explanation}
```

### Step-Specific Inputs

| Step | Step File | Specialist | Additional Input |
|------|-----------|------------|------------------|
| Implementation | `development-loop/step-implementation.md` | Domain specialist | Findings (iteration 2+) |
| Validation | `development-loop/step-validation.md` | None (runs commands) | Files to validate |
| Code Review | `code-review.md` | security, test, code-reviewer, dry-reviewer | Files to review |
| Reflection | `development-loop/step-reflection.md` | Same as implementation + reviewers | Agent IDs to resume |

### Validation Step (No Specialist)

Validation doesn't invoke specialists - it runs verification commands directly:

```markdown
## Step-Runner: Validation

**Your job**: Execute the validation step of the dev-loop.

### Instructions

Read and follow: `.claude/workflows/development-loop/step-validation.md`

### Input

Output directory: {output_dir}
Files to validate: {list from implementation step}

### Your Responsibilities

1. Read the step instructions file
2. Run the 7-layer verification
3. Update `{output_dir}/main.md` with Dev-Loop Verification Steps section
4. Return structured output

### Expected Output

status: success | failed
layer_failed: {if failed, which layer: 1-7}
error_details: {if failed, what went wrong}
```

### Code Review Step (Multiple Specialists)

Code review invokes 4 specialists and collects their verdicts:

```markdown
## Step-Runner: Code Review

**Your job**: Execute the code review step of the dev-loop.

### Instructions

Read and follow: `.claude/workflows/code-review.md`

### Input

Output directory: {output_dir}
Files to review: {list}
Iteration: {n}

### Your Responsibilities

1. Read the code review workflow
2. Invoke all 4 reviewers (security, test, code-reviewer, dry-reviewer)
3. Ensure each reviewer creates checkpoint at `{output_dir}/{reviewer}.md`
4. Update `{output_dir}/main.md` with Code Review Results section
5. Return structured output with all verdicts

### Expected Output

status: approved | needs_fixes | blocked
reviewer_ids:
  security: {id}
  test: {id}
  code_reviewer: {id}
  dry_reviewer: {id}
findings: {list of findings if needs_fixes}
blocker_count: {number of blocking findings}
```

### Reflection Step (Resume Specialists)

Reflection resumes previously-invoked specialists:

```markdown
## Step-Runner: Reflection

**Your job**: Execute the reflection step of the dev-loop.

### Instructions

Read and follow: `.claude/workflows/development-loop/step-reflection.md`

### Input

Output directory: {output_dir}
Implementing specialist to resume: {id}
Reviewers to resume:
  security: {id}
  test: {id}
  code_reviewer: {id}
  dry_reviewer: {id}

### Your Responsibilities

1. Read the step instructions file
2. Resume each specialist sequentially (not in parallel)
3. Each specialist updates their knowledge files and checkpoint
4. Update `{output_dir}/main.md` with Reflection section
5. Return structured output

### Expected Output

status: success | failed
knowledge_files_updated: {list of files updated}
error: {if failed}
```

---

## Orchestrator State Management

The orchestrator maintains minimal state between steps:

```
{
  task: "description",
  output_dir: "docs/dev-loop-outputs/YYYY-MM-DD-{task}",
  step: "implementation | validation | code_review | reflection | complete",
  iteration: 1,
  specialist_ids: {
    implementing: "abc123",
    security: "def456",
    test: "ghi789",
    code_reviewer: "jkl012",
    dry_reviewer: "mno345"
  }
}
```

**State transitions based on step-runner output**:

| Current Step | Output | Next Step |
|--------------|--------|-----------|
| implementation | success | validation |
| validation | success | code_review |
| validation | failed | implementation (resume specialist with errors) |
| code_review | approved | reflection |
| code_review | needs_fixes | implementation (resume specialist with findings) |
| reflection | success | complete |

---

## Related Workflows

| Workflow | When to Use | Relationship |
|----------|-------------|--------------|
| `multi-agent-debate.md` | Cross-cutting design | Happens BEFORE loop, produces ADR |
| `code-review.md` | Quality gate | Integrated INTO loop (step 4) |
| `orchestrator-guide.md` | General orchestration | This loop is a key subprocess |
| `process-review-record.md` | Process failures | Use when loop reveals gaps |
