# Session Restore

This file covers recovery procedures when a dev-loop session is interrupted.

---

## Purpose

If a session is interrupted (computer restart, context compression, process kill), agent context is lost. Per-specialist checkpoint files enable meaningful recovery by capturing working notes as specialists work.

---

## Checkpoint Files

Each specialist writes to their own checkpoint file during work:

```
docs/dev-loop-outputs/YYYY-MM-DD-{task}/
├── main.md                       # Main output (orchestrator owns)
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

## Resume Fallback Pattern

When using the Task tool's `resume` parameter fails (API errors, concurrency issues), fall back to checkpoint injection:

1. **Attempt resume first** - Use `resume: "{agent_id}"` parameter
2. **If resume fails** - Don't use `/rewind`; instead:
   - Read the specialist's checkpoint file
   - Invoke a fresh agent with checkpoint context injected
   - Use the "Restore Context Template" format above

**Example fallback prompt**:
```
The resume failed. Using checkpoint recovery instead.

# Context Recovery for {Specialist}

You are continuing work that was interrupted. Here's your previous context:

## Your Previous Working Notes

{paste checkpoint file content}

## Your Task

{original task description}
```

This ensures the fresh agent has meaningful context even without the original agent's full memory.

---

## Validation Before Step Transitions

Orchestrator validates checkpoint files exist before advancing:

| Transition | Validation |
|------------|------------|
| Implementation → Validation | Implementing specialist checkpoint has Patterns/Gotchas sections |
| Code Review → Reflection | All reviewer checkpoints have Observations sections |
| Reflection → Complete | All specialists have updated Status to reflection complete |

---

## Limitations

Restored specialists have checkpoint context but not full memory. The restore is "good enough" to continue meaningful work, but may miss nuances from the original session. This is acceptable - the alternative is starting over completely.
