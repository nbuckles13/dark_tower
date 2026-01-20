# Session Restore

This file covers recovery procedures when a dev-loop session is interrupted.

---

## Purpose

If a session is interrupted (computer restart, context compression, process kill), agent context may be lost. Checkpoint files and agent IDs enable meaningful recovery.

---

## Recovery Mechanisms

### 1. Agent Resume (Preferred)

If the agent ID is known and the session is still valid:

```
Task tool call:
  resume: "{agent_id}"
  prompt: "Continue from where you left off..."
```

**Benefits**: Full context preserved from previous invocation.

**When it works**: Same CLI session, agent hasn't expired.

### 2. Checkpoint Recovery (Fallback)

If agent can't be resumed, use checkpoint files:

1. Read the specialist's checkpoint file
2. Spawn a fresh specialist with checkpoint context injected
3. Use the "Restore Context Template" format below

---

## Checkpoint Files

Each specialist writes to their own checkpoint file during work:

```
docs/dev-loop-outputs/YYYY-MM-DD-{task}/
├── main.md                       # Main output (loop state)
├── {implementing-specialist}.md  # Implementing specialist's working notes
└── {reviewer}.md                 # Each reviewer's observations
```

**Checkpoint content** (written as specialist works):
- **Patterns Discovered** - What approaches worked well
- **Gotchas Encountered** - What was tricky, what to warn others about
- **Key Decisions** - Choices made and why
- **Observations** - What informed the review verdict (reviewers)
- **Status** - Current step, verdict, timestamp

---

## Restore Procedure

When starting a new session, check for incomplete dev-loops:

1. **Scan** `docs/dev-loop-outputs/` for directories
2. **Check** each `main.md` for Loop State with `Current Step != complete`
3. **If found**, offer restore to user

### Restore Prompt

```
Found incomplete dev-loop: {task-slug}
- Current step: {step}
- Iteration: {iteration}
- Implementing specialist: {name}

Restore and continue? (Specialists will be re-invoked with checkpoint context)
```

---

## Restore Context Template

When restoring a specialist, inject their checkpoint:

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

---

## Agent ID Tracking

The Loop State in `main.md` tracks agent IDs:

```markdown
| Implementing Agent ID | `abc123` |
| Security Reviewer ID | `def456` |
| Test Reviewer ID | `ghi789` |
| Code Reviewer ID | `jkl012` |
| DRY Reviewer ID | `mno345` |
```

These IDs enable resuming specialists via the Task tool's `resume` parameter.

---

## Choosing a Strategy

| Scenario | Strategy |
|----------|----------|
| Agent ID known, session recent | Agent resume |
| Orchestrator context compressed | Checkpoint injection |
| New session started | Checkpoint injection |
| Agent resume fails | Fall back to checkpoint injection |

---

## Limitations

Restored specialists have checkpoint context but not full memory. The restore is "good enough" to continue meaningful work, but may miss nuances from the original session. This is acceptable - the alternative is starting over completely.
