# ADR-0021: Step-Runner Architecture for Devloop Reliability

**Status**: Accepted

**Date**: 2026-01-17

**Deciders**: Nathan, Claude Code Orchestrator

---

## Context

The Development Loop (ADR-0016) establishes a workflow for implementing features: implementation → validation → code review → reflection. The orchestrator (Claude Code) was responsible for executing all steps directly, tracking state, invoking specialists, and following step-specific rules.

**Problem observed**: Each devloop run surfaced different process errors:
- Not running workspace-wide tests (only package-specific)
- Code review findings not acted upon (misinterpreting blocking rules)
- Missing checkpoint files
- Orchestrator implementing fixes directly instead of delegating to specialists
- Step announcements skipped

**Root cause**: The orchestrator had too much context to manage simultaneously:
- Overall state (which step, which iteration)
- Step-specific rules (how to run each step)
- Specialist management (IDs, when to resume)
- Process rules (who does what, blocking rules)
- Technical understanding of the task

When focused on one aspect (e.g., fixing a technical issue), other aspects slipped. Context compression exacerbated this by losing detailed workflow nuances.

## Decision

**Introduce a three-level agent architecture** where each level has focused context:

| Level | Role | Context | Reads |
|-------|------|---------|-------|
| Orchestrator | State machine, step sequencing | Minimal - just state transitions | `development-loop.md` |
| Step-Runner | Execute one step completely | Step-specific rules only | Step file (e.g., `step-implementation.md`) |
| Specialist | Domain expertise | Domain knowledge only | Specialist definition + dynamic knowledge |

### Key Design Principles

1. **Separation of concerns**: Each level only knows what it needs
   - Orchestrator doesn't know step execution details
   - Step-runners don't know overall state or other steps
   - Specialists don't know process rules

2. **Explicit handoffs**: Orchestrator passes structured input to step-runners, receives structured output
   - No ambiguity about what information flows between levels
   - Step-runners return status, agent IDs, and artifacts

3. **Fresh context per step**: Each step-runner starts with clean context containing only its step's documentation

### Orchestrator Responsibilities

- Track state: current step, iteration, specialist IDs
- Decide step transitions based on step-runner output
- Update Loop State in main.md
- Handle user interaction and task switching

**Does NOT**: Execute step logic, invoke specialists directly, know step-specific rules

### Step-Runner Responsibilities

- Read and follow step-specific documentation
- Invoke or resume specialists as instructed
- Ensure checkpoints are created
- Update main.md with step-specific sections
- Return structured output to orchestrator

### Information Flow

```
Orchestrator State:
{
  task, output_dir, step, iteration,
  specialist_ids: { implementing, security, test, ... }
}
        │
        ▼ (passes relevant subset as prompt)

Step-Runner receives:
  - Task description
  - Action: "Start new specialist" or "Resume specialist {id}"
  - Findings to address (if iteration 2+)
  - Output directory

        │
        ▼ (returns structured result)

Step-Runner returns:
  - status: success | failed
  - specialist_id (for orchestrator to store)
  - files_created, files_modified
  - checkpoint_exists: true | false
  - error (if failed)
```

### State Transitions

| Current Step | Step-Runner Output | Next Step |
|--------------|-------------------|-----------|
| implementation | success | validation |
| validation | success | code_review |
| validation | failed | implementation (resume with errors) |
| code_review | approved | reflection |
| code_review | needs_fixes | implementation (resume with findings) |
| reflection | success | complete |

## Alternatives Considered

### 1. Tool-Based Enforcement

Add tools that enforce workflow rules:
- `StartStep(name)` - validates preconditions
- `EndStep(outputs)` - validates postconditions
- `DelegateToSpecialist(specialist, task)` - forces delegation

**Rejected because**: Requires tool development and still relies on orchestrator to call tools correctly. Doesn't address context overload.

### 2. Simpler Documentation

Reduce workflow documentation to bare essentials.

**Rejected because**: The rules exist for good reasons. Simplifying them would lose important process guarantees. The problem is context management, not documentation complexity.

### 3. Mandatory Pre-Step Reading

Require orchestrator to re-read step documentation before each step.

**Rejected because**: Still puts all context in one agent. Doesn't prevent focus-induced errors.

### 4. Single Persistent Step-Runner

One step-runner agent that handles all steps sequentially.

**Rejected because**: Recreates context accumulation problem at step-runner level.

## Consequences

### Positive

- **Focused context**: Each agent level has only what it needs
- **Reduced errors**: Step-runners follow step rules without distraction
- **Clear contracts**: Structured input/output makes handoffs explicit
- **Easier debugging**: Can identify which level failed
- **Fresh start**: Each step-runner starts clean, no accumulated state

### Negative

- **More API calls**: Three levels means more agent invocations
- **Latency**: Sequential agent calls add overhead
- **Coordination complexity**: Information must flow correctly between levels
- **New failure modes**: Step-runner could fail, requiring orchestrator handling

### Neutral

- **Documentation restructured**: `development-loop.md` now focused on orchestrator concerns
- **Step files unchanged**: Step-runners read existing step documentation

## Implementation

### Files Modified

- `.claude/workflows/development-loop.md` - Added step-runner architecture section, removed orchestrator-executed details
- `.claude/workflows/development-loop/output-documentation.md` - Added Loop State format and Categories Shorthand

### Step-Runner Invocation

Standard format documented in `development-loop.md` under "Step-Runner Invocation". Each step has specific input/output contracts.

## Verification

Success criteria:
1. Dev-loop runs complete without process errors
2. Step-runners follow step documentation correctly
3. Orchestrator maintains minimal state
4. Checkpoint files created by specialists (validated by step-runners)

## References

- ADR-0016: Development Loop with Guard Integration
- ADR-0017: Specialist Self-Improvement via Dynamic Knowledge
- ADR-0018: Dev-Loop Checkpointing for Session Recovery
- `.claude/workflows/development-loop.md` - Updated workflow documentation
