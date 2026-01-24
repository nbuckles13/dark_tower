# ADR-0022: Skill-Based Development Loop

**Status**: Accepted

**Date**: 2026-01-23

**Deciders**: Development team

**Supersedes**: Workflow-based approach documented in ADR-0016

---

## Context

The development loop workflow (ADR-0016) was originally implemented as documentation files in `.claude/workflows/` that the orchestrator (Claude) would read and follow. This approach had several limitations:

1. **Documentation drift**: Workflow docs described behavior but weren't executable
2. **Context overhead**: Orchestrator had to read and interpret multiple workflow files
3. **Manual orchestration**: User had to invoke the Task tool and manually coordinate steps
4. **Context loss**: Each Task tool invocation created a new agent, losing planning context
5. **Unreliability**:  Having Claude as the orchestrator led to inconsistent and incorrect execution of the dev-loop steps

### Original Approach (ADR-0016)

```
.claude/workflows/development-loop.md       # Main workflow doc
.claude/workflows/development-loop/
├── specialist-invocation.md               # How to invoke specialists
├── step-implementation.md                 # Implementation step details
├── step-validation.md                     # 7-layer verification
├── step-reflection.md                     # Knowledge capture process
└── session-restore.md                     # Checkpoint recovery
```

The orchestrator would:
1. Read workflow docs to understand the process
2. Use Task tool to spawn specialists
3. Track state manually in output files
4. Coordinate step transitions

### Problems Encountered

1. **Context fragmentation**: Planning done by one agent, implementation by another
2. **Verbose prompts**: Had to inject full workflow instructions into each agent
3. **User friction**: User had to manually run `/dev-loop-init`, coordinate steps
4. **Inconsistent behavior**: Orchestrator interpretation varied session to session

## Decision

**Migrate the development loop from workflow documentation to executable skills.**

Each dev-loop step becomes a skill with:
- **SKILL.md** defining the executable procedure
- Direct invocation via `/dev-loop-*` commands
- Agent preservation across planning and implementation phases

### New Skill Structure

```
.claude/skills/
├── dev-loop/SKILL.md           # Overview and navigation
├── dev-loop-init/SKILL.md      # Step 0: Initialize loop
├── dev-loop-plan/SKILL.md      # Step 0.5: Optional planning
├── dev-loop-implement/SKILL.md # Step 1: Implementation
├── dev-loop-validate/SKILL.md  # Step 2: 7-layer verification
├── dev-loop-review/SKILL.md    # Step 3: Code review
├── dev-loop-reflect/SKILL.md   # Step 4: Knowledge capture
├── dev-loop-fix/SKILL.md       # Utility: Fix findings
├── dev-loop-restore/SKILL.md   # Utility: Recover from checkpoint
└── dev-loop-status/SKILL.md    # Utility: Check loop state
```

### Key Changes

| Aspect | Workflow Approach | Skill Approach |
|--------|-------------------|----------------|
| Invocation | Orchestrator reads docs, uses Task tool | User runs `/dev-loop-*` commands |
| Agent continuity | New agent per step | Same agent for plan → implement |
| Context injection | Manual in orchestrator prompt | Built into skill definitions |
| Step coordination | Orchestrator interprets workflow | Skills guide next step explicitly |
| Error handling | Described in docs | Encoded in skill procedures |

### What Skills Capture

All content from the workflow files has been incorporated into skills:

| Workflow Content | Skill Location |
|------------------|----------------|
| Context injection order | `/dev-loop-implement` Step 3 |
| Task-to-category mapping | `/dev-loop-init` Step 4 |
| Specialist selection | `/dev-loop-init` Step 3 |
| Prompt structure template | `/dev-loop-implement` Step 3 |
| Task tool invocation | `/dev-loop-implement` Step 4 |
| Resume capability | `/dev-loop-implement` Step 4, `/dev-loop-fix` |
| Checkpoint requirements | `/dev-loop-implement` Step 6 |
| 7-layer verification | `/dev-loop-validate` |
| Session restore | `/dev-loop-restore` |
| Reflection process | `/dev-loop-reflect` |

## Consequences

### Positive

1. **Same agent continuity**: Planning → implementation preserves context
2. **Executable procedures**: Skills define exact steps, not prose descriptions
3. **Clear next steps**: Each skill ends with explicit "run X next"
4. **User control**: User explicitly invokes each phase
5. **Easier extension**: Add new skills without modifying core workflow
6. **Self-documenting**: Skills show Claude what to do, not just what the process is

### Negative

1. **Migration overhead**: Existing workflow docs become obsolete
2. **Multiple files**: One SKILL.md per step vs. fewer workflow docs
3. **Skill learning curve**: Users must learn `/dev-loop-*` commands

### Neutral

1. **Output format unchanged**: `docs/dev-loop-outputs/` structure preserved
2. **Checkpoint system unchanged**: ADR-0018 checkpointing still works
3. **Specialist definitions unchanged**: `.claude/agents/*.md` unchanged

## Implementation

### Files Deleted

The following workflow files are now obsolete and should be deleted:
- `.claude/workflows/development-loop.md`
- `.claude/workflows/development-loop/specialist-invocation.md`
- `.claude/workflows/development-loop/step-implementation.md`
- `.claude/workflows/development-loop/step-validation.md`
- `.claude/workflows/development-loop/step-reflection.md`
- `.claude/workflows/development-loop/session-restore.md`
- `.claude/workflows/development-loop/output-documentation.md`

### Files Updated

- `CLAUDE.md` - Dev-loop section now references skills
- `.claude/DEVELOPMENT_WORKFLOW.md` - Restore procedure uses `/dev-loop-restore`
- `docs/PROJECT_STATUS.md` - Notes skill migration
- `.claude/agents/dry-reviewer.md` - Integration section updated

### ADR Updates

- ADR-0016: Added "Superseded By" note pointing to this ADR
- ADR-0018: Added note that restore is now via `/dev-loop-restore` skill

## Alternatives Considered

### Alternative A: Keep Workflow Docs, Add Skills as Wrappers

Skills would be thin wrappers that reference workflow docs.

- Pros: No content migration needed
- Cons: Two sources of truth, potential drift, more files to maintain

### Alternative B: Single Monolithic Skill

One large SKILL.md containing all steps.

- Pros: Single file, easy to find
- Cons: Too large for context window, can't invoke individual steps

### Alternative C: Hybrid Approach

Keep workflow docs for reference, skills for execution.

- Pros: Documentation preserved
- Cons: Confusion about which is authoritative

## References

- ADR-0016: Development Loop with Guard and Code Review Integration
- ADR-0017: Specialist Knowledge Architecture
- ADR-0018: Dev-Loop Checkpointing and Restore
- ADR-0021: Step-Runner Architecture
- Claude Code Skills Documentation: https://docs.anthropic.com/en/docs/claude-code/skills
