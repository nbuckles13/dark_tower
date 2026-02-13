---
name: debate
description: Multi-agent design debate using Agent Teams. Use for design questions that affect multiple services or require cross-cutting consensus.
---

# Debate Skill (Agent Teams)

Run a multi-agent design debate to reach consensus on cross-cutting design decisions. Produces an Architecture Decision Record (ADR) when consensus is reached.

## When to Use

Use `/debate` when:
- Design decision affects 2+ services
- Protocol or contract **breaking changes, semantic changes, or new message categories** (NOT simple additive fields — per ADR-0004, safe changes like new optional fields, new enum values, new RPCs use a standard `/dev-loop` without debate)
- Database schema changes with cross-service impact
- Performance/scalability trade-offs need discussion
- Core pattern modifications

Do NOT use for:
- Single-service implementation (use `/dev-loop`)
- Research/exploration (use Task tool with Explore agent)
- Simple questions (just answer directly)

## Arguments

```
/debate "design question"
/debate "design question" --specialists=list
```

- **design question**: The question to resolve (required)
- **--specialists**: Comma-separated additional domain specialists beyond mandatory cross-cutting

## Instructions

### Step 1: Validate the Question

Confirm the question is appropriate for debate:
- Is it a design question (not implementation)?
- Does it affect multiple components?
- Would multiple specialists have opinions?

If not appropriate, suggest alternatives:
- Single-service → `/dev-loop`
- Need exploration → Explore agent
- Simple question → Answer directly

### Step 2: Identify Participants

**Mandatory (always included)**:
- Security
- Test
- Observability
- Operations

**Domain specialists** (based on question):
- `protocol` - API/wire protocol changes
- `database` - Schema changes
- `infrastructure` - Deployment/platform changes
- `auth-controller` - Auth system changes
- `global-controller` - API gateway changes
- `meeting-controller` - Session/signaling changes
- `media-handler` - Media routing changes

Minimum team: 5 specialists (1 domain + 4 mandatory)

### Step 3: Create Debate Directory

```bash
mkdir -p docs/debates/YYYY-MM-DD-{question-slug}
```

Create `debate.md` with initial state:

```markdown
# Debate: {Question}

**Date**: YYYY-MM-DD
**Status**: In Progress
**Participants**: {list}

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

{The design question}

## Context

{Background information relevant to the decision}

## Positions

### Initial Positions

| Specialist | Position | Satisfaction |
|------------|----------|--------------|
| {name} | TBD | TBD |

## Discussion

### Round 1

{Will be populated as debate progresses}

## Consensus

TBD

## Decision

TBD - ADR will be created when consensus reached
```

### Step 4: Compose Specialist Prompts

For each specialist, compose:

1. **Debate protocol**: `.claude/skills/debate/debate-protocol.md`

**NOTE**: Specialist identity is auto-loaded via `subagent_type` parameter. Do NOT manually read or inject `.claude/agents/{name}.md`. Specialists self-load their own knowledge from `docs/specialist-knowledge/{name}/` as their first step.

Spawn with `subagent_type: "{name}"` and this prompt:

```
You are participating in a Dark Tower design debate.

## Step 0: Load Knowledge (MANDATORY)

**Before doing ANY other work**, read ALL `.md` files from `docs/specialist-knowledge/{your-specialist-name}/` to load your accumulated knowledge. This includes patterns, gotchas, integration notes, and any domain-specific files. Do NOT skip this step.

## Debate Protocol

{contents of protocols/debate.md}

## The Question

{design question}

## Context

{background context}

## Your Task

1. State your initial position on this question
2. Engage with other specialists' positions
3. Update your satisfaction score as discussion progresses
4. Work toward consensus (90%+ all participants)

CC the Lead with satisfaction updates after each substantive exchange.
```

### Step 5: Spawn Debate Team

**IMPORTANT**: This step requires Agent Teams to be enabled.

Spawn all specialists as teammates using `subagent_type: "{name}"` in the Task tool:
- Enable delegate mode (Lead coordinates, doesn't participate in debate content)
- Identity auto-loaded from `.claude/agents/{name}.md`
- Specialists can message each other directly

### Step 6: Monitor Debate

As Lead, track:
- Satisfaction scores from each specialist (CC'd to you)
- Round count (max 10 rounds)
- Time elapsed (max 2 hours)
- Progress toward consensus

**Consensus check**: All specialists at 90%+ satisfaction

**Escalation triggers**:
- No progress for 3 rounds
- Round 10 reached without consensus
- 2 hour limit reached

On escalation:

**For domain disagreements** (non-cross-cutting specialists dissenting):
- Accept majority position with dissent noted

**For cross-cutting specialist dissent** (Security, Test, Observability, or Operations scoring < 70):
- Escalation message to user **must explicitly highlight** the dissenting specialist's specific objection
- User must provide **explicit risk acceptance**: "I acknowledge [specialist] has unresolved concerns about [X]. I accept this risk."
- This is informed risk acceptance, not implicit majority override

```
**Debate Escalation Required**

Question: {question}
Current state: {summary of positions}
Blockers: {what's preventing consensus}

⚠️ Cross-cutting dissent: {specialist} at {score}% — concern: {specific objection}
Risk acceptance required for: {list specific unresolved concerns}

Options:
1. Continue with modified scope
2. Accept majority position with explicit risk acceptance for cross-cutting dissent
3. Defer decision pending more information
4. Split into smaller questions

Which approach would you like to take?
```

### Step 7: Create ADR

When consensus reached:

1. Draft ADR in `docs/decisions/adr-NNNN-{topic}.md`
2. Send draft to all specialists for review
3. Incorporate any final clarifications
4. Finalize ADR

**ADR template**:

```markdown
# ADR-NNNN: {Title}

## Status

Accepted

## Context

{Background and why this decision was needed}

## Decision

{The decision reached by consensus}

## Implementation Guidance

- Suggested specialist: {name}
- Task breakdown: {if multi-loop, list sequential dev-loops}
- Key files: {primary files to modify}
- Dependencies: {order constraints between implementation steps}

## Consequences

### Positive
{Benefits}

### Negative
{Trade-offs accepted}

### Neutral
{Other implications}

## Participants

- {Specialist}: {final position summary}

## Debate Reference

See: `docs/debates/YYYY-MM-DD-{slug}/debate.md`
```

**Conditional sections** (include when applicable):

Protocol Constraints (when debate touches wire format):
- Field numbers allocated, backward compatibility, rollout order, wire format, actor routing

Migration Plan (when debate involves schema changes):
- Step-by-step migration sequence, backward compatibility window, rollback procedure, data backfill strategy

### Step 8: Reflection

After ADR is created, ask each specialist:
- What patterns emerged that should be documented?
- Any gotchas to add to knowledge files?
- Integration notes with other components?
- Any new domain-specific knowledge worth capturing in a new file?

Each specialist updates their knowledge directory at `docs/specialist-knowledge/{specialist}/`. Specialists can create or update any `.md` files in their directory - common files include `patterns.md`, `gotchas.md`, and `integration.md`, but specialists may also maintain domain-specific files as needed.

### Step 9: Complete

Update `debate.md`:
- Status: Complete
- Link to ADR
- Final satisfaction scores

Report to user:
```
**Debate Complete**

Question: {question}
Consensus reached: Round {N}
ADR created: docs/decisions/adr-NNNN-{topic}.md

Participants:
- {name}: {final satisfaction}% - {one-line position}

**Next step**: If implementation needed, run `/dev-loop "implement ADR-NNNN"`
```

## Limits

| Phase | Limit | Action on Exceeded |
|-------|-------|-------------------|
| Rounds | 10 | Escalate to user |
| Time | 2 hours | Escalate to user |
| Stalled | 3 rounds no progress | Escalate to user |
| ADR revisions | 2 rounds | Finalize as-is |
| Reflection | 15 minutes | Proceed without |

## Output

- **Debate record**: `docs/debates/YYYY-MM-DD-{slug}/debate.md`
- **ADR**: `docs/decisions/adr-NNNN-{topic}.md` (on consensus)
- **Knowledge updates**: Updated specialist knowledge files

## Notes

- Debate produces an ADR only - implementation is a separate `/dev-loop`
- Lead uses delegate mode (coordinates, doesn't implement)
- Specialists message each other directly for natural debate flow
- Satisfaction scoring enables tracking progress toward consensus
