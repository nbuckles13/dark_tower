---
name: dev-loop-plan
description: Spawn specialist for exploration and planning. Creates implementation proposal for user approval before implementation.
disable-model-invocation: true
---

# Dev-Loop Plan

Spawn the implementing specialist in planning/exploration mode. The specialist explores the codebase, understands existing patterns, and proposes an implementation approach for user approval.

**Key insight**: The same agent handles both planning and implementation. This skill captures the agent ID so `/dev-loop-implement` can resume it with full context.

## Arguments

```
/dev-loop-plan [output-dir]
```

- **output-dir** (optional): Override auto-detected output directory

## Instructions

### Step 1: Locate Active Dev-Loop in Planning State

If output-dir not provided, auto-detect:

1. Run `./scripts/workflow/dev-loop-status.sh --active-only`
2. Filter output to loops with `Current Step` = `planning`
3. If exactly one: use it
4. If multiple: ask user which one
5. If none with planning state, check for `init` with `--plan` flag indicator
6. If still none: error - "No dev-loop ready for planning. Run `/dev-loop-init --plan` first."

Read the `main.md` to get:
- Task description (from Task Overview > Objective - may be empty if `--plan` only)
- Implementing Specialist name (from Loop State)
- Matched principles (from Matched Principles section)

### Step 2: Update Loop State

Update `main.md` Loop State:

| Field | Value |
|-------|-------|
| Current Step | `planning` |
| Implementing Agent | `pending` (will update after spawn) |

### Step 3: Build Planning Prompt

Read and concatenate these files in order:

1. **Specialist definition**: `.claude/agents/{specialist}.md`
2. **Principles**: Each file from `docs/principles/{category}.md`
3. **Knowledge files** (if exist): `docs/specialist-knowledge/{specialist}/*.md`

Then add the planning section:

```markdown
{specialist definition content}

---

## Project Principles

{content of each matched principle file, separated by ---}

---

## Accumulated Knowledge

{content of patterns.md, gotchas.md, integration.md if they exist}
{or "No accumulated knowledge yet. You will create knowledge files during reflection."}

---

## Task

{task description - VERBATIM from main.md, or "To be defined during planning" if empty}

---

## Planning Mode

You are in **planning mode**. Your goal is to explore the codebase and propose an implementation approach. Do NOT implement yet.

### Your Responsibilities

1. **Explore** the codebase to understand:
   - Existing patterns and conventions
   - Related code that may need changes
   - Dependencies and integrations
   - Potential complications or edge cases

2. **Assess scope** and determine if this task is appropriate for dev-loop:
   - If task touches multiple services or core patterns → recommend debate + ADR instead
   - If task requires architectural decisions beyond your domain → recommend escalation
   - If task is well-scoped to your specialty → proceed with proposal

3. **Propose approach** with structured output (see below)

4. **Ask questions** if requirements are unclear

### Escalation Criteria

Recommend escalation to debate workflow when:
- Task touches 2+ services or crosses service boundaries
- Changes affect core patterns or shared code
- Architectural decisions need input from multiple specialists
- Changes warrant formal documentation (ADR)

If you recommend escalation, explain why and what the debate should cover.

### Output Format

Return a structured planning proposal:

```yaml
status: ready | needs_clarification | recommend_escalation
objective: "{refined objective based on exploration}"
approach: |
  {Multi-line description of proposed implementation approach}
files_to_modify:
  - path: "path/to/file.rs"
    changes: "Brief description of changes"
files_to_create:
  - path: "path/to/new_file.rs"
    purpose: "Brief description of purpose"
key_decisions:
  - decision: "Decision description"
    rationale: "Why this approach"
questions:
  - "Any clarifying questions for the user"
escalation_reason: "{If recommend_escalation, explain why}"
debate_participants: "{If escalation, which specialists should participate}"
```

---

## Output Directory

Write exploration notes to: `{output_dir}/planning-notes.md` (optional, for your reference)
```

### Step 4: Invoke Specialist in Planning Mode

Use the **Task tool** with `general-purpose` subagent_type and model `Opus`:

```
Task tool parameters:
- subagent_type: "general-purpose"
- description: "Plan: {first 30 chars of task or 'explore task scope'}"
- prompt: {built prompt from Step 3}
```

### Step 5: Capture Agent ID

After Task tool returns, capture the agent ID from the response.

Update `main.md` Loop State:

| Field | Value |
|-------|-------|
| Implementing Agent | `{agent_id}` |
| Current Step | `planning` |

### Step 6: Process Planning Response

Parse the specialist's response:

#### If `status: ready`

1. Update `main.md` with Planning Proposal section:

```markdown
## Planning Proposal

**Status**: Ready for implementation

### Approach
{approach from response}

### Files to Modify
{formatted list from response}

### Files to Create
{formatted list from response}

### Key Decisions
{formatted list from response}

### Questions (if any)
{questions from response}

---
```

2. Report to user:

```
**Planning Complete**

Specialist: {specialist-name}
Agent ID: {agent_id} (will be resumed for implementation)

**Proposed Approach**:
{brief summary}

**Files Affected**: {count} modified, {count} created

**Key Decisions**:
{list decisions}

{If questions, list them}

**Next steps**:
- Review the proposal in: {output_dir}/main.md
- Discuss any questions or concerns
- When ready, run `/dev-loop-implement` to proceed
```

#### If `status: needs_clarification`

1. Update `main.md` Planning Proposal with questions

2. Report to user:

```
**Planning Paused - Clarification Needed**

Specialist: {specialist-name}
Agent ID: {agent_id}

**Questions**:
{list questions}

**Next steps**:
- Answer the questions above
- Resume the planner with `/dev-loop-plan` to continue (will resume same agent)
- Or provide answers and the orchestrator can relay them
```

#### If `status: recommend_escalation`

1. Update `main.md` with escalation recommendation

2. Report to user:

```
**Planning Complete - Escalation Recommended**

Specialist: {specialist-name}
Agent ID: {agent_id}

**Reason for Escalation**:
{escalation_reason}

**Recommended Debate Participants**:
{debate_participants}

**Next steps**:
- Consider initiating a debate for this task
- Or proceed with `/dev-loop-implement` if you want to continue anyway
```

### Step 7: Handle Resume (Follow-up Questions)

If this skill is invoked when `Implementing Agent` is already set (not `pending`):

1. Check if agent ID exists from previous planning
2. Use Task tool with `resume` parameter to continue the same agent
3. Pass any new context or answers from the user

```
Task tool parameters:
- subagent_type: "general-purpose"
- description: "Continue planning: {specialist}"
- prompt: "Continue planning with this additional context:\n\n{user's questions/answers}"
- resume: "{existing_agent_id}"
```

## Critical Constraints

- **Same agent for plan + implement**: The agent ID captured here will be resumed by `/dev-loop-implement`
- **VERBATIM task description**: Never paraphrase, summarize, or modify the task description
- **No implementation**: Specialist explores and proposes but does NOT implement
- **Escalation is valid**: If task is too big, specialist should recommend debate

---

**Next step**: Review proposal, then run `/dev-loop-implement` to proceed
