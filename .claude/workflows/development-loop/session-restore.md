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

## Resume Strategies

There are two complementary resume mechanisms:

### 1. Session Resume (Short-term)

Step-runners invoke specialists via `claude --print` which returns a `session_id`. For iteration 2+ (fixing findings), use:

```bash
claude --print --resume "$session_id" --model opus --output-format json ...
```

**Benefits**: Full context preserved, ~100x cost reduction via prompt caching.

**Limitation**: Only works within same CLI session. session_id may expire.

**Session tracking** in `main.md`:
```markdown
## Session Tracking

| Specialist | Session ID | Iteration | Status |
|------------|------------|-----------|--------|
| auth-controller | 9e956e47-... | 2 | fixing-findings |
```

### 2. Checkpoint Recovery (Long-term)

For full session restarts (context compression, computer restart), use checkpoint injection:

1. Read the specialist's checkpoint file
2. Invoke a fresh specialist with checkpoint context injected
3. Use the "Restore Context Template" format above

**Example checkpoint-based recovery prompt**:
```
# Context Recovery for {Specialist}

You are continuing work that was interrupted. Here's your previous context:

## Your Previous Working Notes

{paste checkpoint file content}

## Your Task

{original task description}
```

This ensures the fresh specialist has meaningful context even without the original session.

### Choosing a Strategy

| Scenario | Strategy |
|----------|----------|
| Iteration 2+ within same run | Session resume (`--resume`) |
| Orchestrator context compressed | Checkpoint injection |
| New session started | Checkpoint injection |
| Session resume fails | Fall back to checkpoint injection |

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
