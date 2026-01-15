# ADR-0018: Dev-Loop Checkpointing and Restore

**Status**: Proposed

**Date**: 2026-01-14

**Deciders**: Development team

---

## Context

The development loop workflow (ADR-0016) uses specialist agents that accumulate context as they work. When a session is interrupted (computer restart, context compression, process kill), agent context is lost and cannot be resumed. This leads to:

1. **Lost work** - Specialists may have made progress that wasn't captured
2. **Lost insights** - Patterns discovered and gotchas encountered are forgotten
3. **Incomplete reflection** - The reflection step requires resuming agents with their original context
4. **Manual recovery** - Currently requires human intervention to reconstruct state

### Current State

- Loop State in output file tracks agent IDs and current step
- Agent IDs become useless after session loss (context is gone)
- No mechanism to reconstruct specialist context for continuation
- Reflection step is particularly vulnerable (requires "memory" of what was done)

### Triggering Incident

During GC Phase 1 implementation, a computer restart occurred during the reflection step. All four specialists (implementing + 3 reviewers) had completed their work but couldn't be resumed for reflection because their context was lost.

## Decision

Implement a **per-specialist checkpoint system** that captures enough context during work that a fresh agent can meaningfully continue after an interruption.

### Key Components

#### 1. Per-Specialist Checkpoint Files

Each specialist writes to their own checkpoint file during work:

```
docs/dev-loop-outputs/{date}-{task}/
├── main.md                    # Main output file (orchestrator owns)
├── global-controller.md       # Implementing specialist's working notes
├── security.md                # Security reviewer's observations
├── test.md                    # Test reviewer's observations
└── code-reviewer.md           # Code reviewer's observations
```

**Naming convention**: Match `.claude/agents/{specialist}.md` filename

#### 2. Checkpoint Content (Written As-You-Work)

Each specialist file contains:

```markdown
# {Specialist} Checkpoint - {Task}

## Working Notes

### Patterns Discovered
<!-- What approaches worked well? What patterns did you follow or establish? -->

### Gotchas Encountered
<!-- What mistakes did you catch? What was tricky? What would you warn others about? -->

### Key Decisions
<!-- What choices did you make and why? -->

### Observations (for reviewers)
<!-- What did you notice during review? What informed your verdict? -->

## Status
- Step completed: {implementation|review|reflection}
- Verdict (if reviewer): {APPROVED|FINDINGS}
- Timestamp: {ISO timestamp}
```

#### 3. Restore Procedure

On session start, orchestrator:

1. Scans `docs/dev-loop-outputs/` for directories with incomplete `main.md` (Loop State step != complete)
2. Reads all checkpoint files in that directory
3. Offers restore: "Found incomplete dev-loop at step '{step}'. Restore?"
4. On restore: Invokes fresh specialists with checkpoint content as context

#### 4. Restore Prompt Template

```markdown
# Context Recovery for {Specialist}

You are continuing a dev-loop that was interrupted. Here's your previous context:

## Your Previous Working Notes
{paste from checkpoint file}

## Current Loop State
- Step: {current_step}
- Iteration: {iteration}

## What's Already Complete
{summary from main.md}

## Your Task
Continue from where you left off. Based on your working notes, {specific instruction for current step}.
```

#### 5. Validation Before Step Transitions

Orchestrator validates checkpoint files exist before advancing:

| Transition | Validation |
|------------|------------|
| Implementation → Validation | Implementing specialist checkpoint has Patterns/Gotchas sections |
| Validation → Code Review | (no checkpoint needed - validation is stateless) |
| Code Review → Reflection | All reviewer checkpoints have Observations sections |
| Reflection → Complete | All specialists have updated Status to reflection complete |

### Testing the Restore Process

#### Method 1: Force Exit
```bash
# During a dev-loop:
Ctrl+C  # or kill process

# Restart Claude Code
# Orchestrator detects incomplete loop, offers restore
```

#### Method 2: Verify Checkpoint Content
Ask orchestrator: "Show me what would be restored from {dev-loop-output-dir}"

Orchestrator reads checkpoint files and reports:
- Which specialists have checkpoints
- How much context is available
- What step would be resumed

#### Method 3: Simulate Interruption
Add instruction to dev-loop: "After code review, simulate an interruption - stop without doing reflection. Next session will test restore."

## Consequences

### Positive

1. **Resilient dev-loops** - Can recover from any interruption
2. **Better documentation** - Working notes capture thinking, not just results
3. **Improved reflection** - Patterns/gotchas written as-you-work are more accurate
4. **Parallel-safe** - Each specialist writes to own file, no conflicts
5. **Debuggable** - Can see what each specialist was thinking

### Negative

1. **More files** - Directory per dev-loop instead of single file
2. **More writing** - Specialists must write notes as they work
3. **Validation overhead** - Orchestrator must check for checkpoints
4. **Slightly degraded restore** - Fresh agent with context < resumed agent with memory

### Neutral

1. **Changes output structure** - From single file to directory
2. **New orchestrator responsibilities** - Checkpoint validation and restore logic

## Alternatives Considered

### Alternative A: Agent Context Serialization

Have Claude Code serialize agent conversation state to a file for later restoration.

- Pros: Perfect restoration, no context loss
- Cons: Would require Claude Code product changes, not implementable at workflow level

### Alternative B: Single Output File with Sections

Keep single output file, have specialists write to designated sections.

- Pros: Simpler structure, single file
- Cons: Parallel specialists can't safely write to same file

### Alternative C: Specialists Return Notes, Orchestrator Writes

Specialists include checkpoint notes in their response, orchestrator captures and writes.

- Pros: Single writer, no file conflicts
- Cons: If orchestrator interrupted before writing, notes lost

### Alternative D: Skip Checkpointing, Accept Loss

Document that session loss means lost context, restart affected steps from scratch.

- Pros: No additional complexity
- Cons: Lost work, poor developer experience, unreliable workflow

## Implementation Notes

### Files to Modify

1. **`docs/dev-loop-outputs/_template.md`**
   - Change from single file to directory structure
   - Add per-specialist template

2. **`.claude/workflows/development-loop.md`**
   - Add checkpoint writing requirements to specialist prompts
   - Add checkpoint validation to orchestrator checklist
   - Add restore procedure documentation

3. **`.claude/DEVELOPMENT_WORKFLOW.md`**
   - Add session start check for incomplete dev-loops
   - Add restore offer behavior

### Migration

- Existing single-file outputs remain valid (read-only historical record)
- New dev-loops use directory structure
- No migration needed for existing files

### Specialist Prompt Updates

Add to all specialist prompts:
```markdown
## Checkpoint Requirements

As you work, write to your checkpoint file at `docs/dev-loop-outputs/{task-dir}/{your-name}.md`:

1. **Patterns Discovered** - What worked well
2. **Gotchas Encountered** - What was tricky
3. **Key Decisions** - Choices you made and why

These notes enable recovery if the session is interrupted.
```

## References

- ADR-0016: Development Loop (the workflow this enhances)
- ADR-0017: Specialist Knowledge (related - knowledge accumulation)
- Claude Code Checkpointing: https://code.claude.com/docs/en/checkpointing (file edit checkpointing - different feature)
- Triggering incident: GC Phase 1 dev-loop interrupted during reflection step
