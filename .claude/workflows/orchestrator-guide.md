# Orchestrator Guide: How to Run Multi-Agent Debates

This guide is for the primary Claude Code agent (me) on how to orchestrate multi-agent debates using the Task tool.

## When to Initiate a Debate

### Decision Tree

```
Feature request received
  ↓
Identify affected subsystems
  ↓
Is it cross-cutting? (affects 2+ agents)
  ↓ YES                    ↓ NO
Propose debate to user   Just implement
  ↓
User approves
  ↓
Initiate debate
```

### Affected Subsystem Detection

**Keywords that indicate agents**:
- "API endpoint", "HTTP", "REST" → Global Controller
- "signaling", "WebTransport", "session" → Meeting Controller
- "media", "forwarding", "datagram" → Media Handler
- "protobuf", "message", "protocol" → Protocol
- "database", "schema", "migration", "query" → Database

**Cross-cutting indicators**:
- "between services"
- "coordination"
- "new message type"
- "performance optimization affecting multiple"
- "state management spanning"

## Step 0: Propose Debate to User (REQUIRED)

**NEVER start a debate without user approval.**

When I identify a cross-cutting feature, present proposal:

```markdown
[To user]
Analyzing "{feature}" impact...

This feature crosses multiple subsystems. I recommend a {N}-agent debate:

Proposed specialists:
- {Agent 1}: {why this agent is involved}
- {Agent 2}: {why this agent is involved}
- Test: Testability and coverage (ALWAYS included)
- Security: Security architecture and threat modeling (ALWAYS included)
- {Agent N}: {why this agent is involved}

Do you want to:
A) Proceed with these {N} specialists
B) Add/remove specialists (which ones?)
C) Provide additional context before we start
D) Skip debate - I'll implement with my best judgment

Your choice?
```

**CRITICAL RULE**: Test and Security specialists are **ALWAYS included in every debate**, regardless of scope. Testing and security are first-class concerns in all designs.

**Wait for user response before proceeding.**

If user provides additional context or modifies specialist list, acknowledge and adjust.

## Debate Execution Steps

### Step 1: Initialize Tracking

```
Internal state:
- agents = [list from user approval]
- round = 1
- max_rounds = 10
- target_satisfaction = 90
- proposals = []
- satisfaction_scores = {agent: 0 for agent in agents}
```

### Step 2: Announce Start

```markdown
[To user]
Initiating {N}-agent debate on "{topic}"...
Agents: {list}
Target: All agents ≥90% satisfied
Max rounds: 10
```

### Step 3: Randomize Order (if N > 2)

```
If len(agents) > 2:
    Shuffle agents list
    Note to user: "Agent order randomized: [order]"
```

### Step 4: Execute Round

For each agent in shuffled order:

```
1. Build context:
   - Topic description
   - Agent's specialist definition (read from .claude/agents/{agent}.md)
   - Previous rounds (full for early rounds, summarized for later)
   - Current satisfaction scores
   - Round N/10

2. Invoke via Task tool:
   Task(
       subagent_type="general-purpose",
       description="{agent} specialist debates {topic}",
       prompt="""
       {Full context built below}

       You are the {agent} specialist. Read your specialist definition above.

       Topic: {topic description}

       Previous rounds:
       {proposals history}

       Current satisfaction scores:
       {scores}

       This is round {N}/10.

       Provide your response in the following format:

       ## Proposal
       [Your design proposal from {agent} perspective]

       ## Satisfaction Score
       [0-100]

       ## Rationale
       [Why this score?]

       ## Blocking Concerns (if < 90%)
       [List specific issues]

       ## Suggested Changes
       [What would get you to 90%+?]

       ## Performance Impact (if applicable)
       [Latency/throughput implications]
       """
   )

3. Parse response:
   - Extract proposal text
   - Extract satisfaction score
   - Extract concerns
   - Extract suggestions

4. Store:
   proposals.append({
       'round': round,
       'agent': agent,
       'proposal': proposal_text,
       'score': score,
       'concerns': concerns
   })
   satisfaction_scores[agent] = score

5. Report to user:
   "[Round {N}] {Agent}: {score}% satisfied - {brief summary}"
```

### Step 5: Check Termination

After all agents in round have responded:

```python
# Consensus check
if all(score >= 90 for score in satisfaction_scores.values()):
    return 'consensus'

# Progress check (after round 3)
if round > 3:
    last_3_avg_scores = calculate_avg_scores_for_last_3_rounds()
    if last_3_avg_scores[-1] - last_3_avg_scores[0] < 5:
        return 'stalemate'

# Max iterations check
if round >= 10:
    return 'max_iterations'

# Continue
round += 1
goto Step 3
```

### Step 6: Handle Termination

**Consensus**:
```markdown
[To user]
✓ CONSENSUS REACHED (Round {N})

Final satisfaction scores:
- Meeting Controller: 92%
- Media Handler: 94%
- Protocol: 91%

Synthesizing final design...

[Present consolidated design from all agent proposals]

Ready to implement. Proceed? [Y/n]
```

**Stalemate**:
```markdown
[To user]
⚠ Debate stalled after {N} rounds (no progress in last 3 rounds)

Current scores:
- Agent A: 85%
- Agent B: 70%

Blocking issue:
[Identify the main disagreement]

Agent A's position:
[Summary]

Agent B's position:
[Summary]

Suggested resolutions:
A) [Option A]
B) [Option B]
C) [Hybrid approach]

Your decision needed.
```

**Max iterations**:
```markdown
[To user]
⏱ Debate reached maximum rounds (10) without full consensus

Final scores:
{all scores}

Closest we got:
[Best design from highest-scoring round]

Recommend proceeding with this design despite not reaching 90% threshold,
as agents are generally aligned. Remaining concerns:
{list minor concerns}

Proceed? [Y/n/revise]
```

## Context Building Strategy

### Principle Injection (ALWAYS)

Before building debate context, inject relevant project principles:

1. Match debate topic against task patterns (see `.claude/workflows/contextual-injection.md`)
2. Include matched category principles in Round 1 context
3. All specialists see same principles during debate

**Example**: Debate on "JWT refresh token strategy"
- Matched patterns: `jwt|token|auth` → crypto, jwt, logging categories
- All participants receive these principles in their context

### Early Rounds (1-3)

```markdown
# Multi-Agent Debate: {topic}

## Topic
{full description}

## Participating Agents
- {agent1}: {1-line responsibility}
- {agent2}: {1-line responsibility}

## Project Principles (MUST FOLLOW)
{Inject matched category principles from docs/principles/}
- crypto.md (if crypto-related)
- jwt.md (if JWT-related)
- logging.md (if logging-related)
- errors.md (always for production code)

## Round {N}/10

## Proposals So Far
{full verbatim history of all proposals}

## Current Satisfaction Scores
- {agent1}: {score}%
- {agent2}: {score}%

## Your Turn
You are the {current_agent} specialist.

{Paste full content from .claude/agents/{current_agent}.md}

Please provide your proposal following the standard format.
```

### Middle Rounds (4-7)

```markdown
# Multi-Agent Debate: {topic}

## Round {N}/10

## Recent Proposals (Last 2 Rounds)
{verbatim last 2 rounds}

## Earlier Rounds (Summary)
- Round 1: {agent1} proposed {X}, {agent2} concerned about {Y} (scores: A=85, B=60)
- Round 2: {agent1} revised {Z}, {agent2} partially satisfied (scores: A=80, B=70)

## Current Satisfaction Scores
- {agent1}: {score}%
- {agent2}: {score}%

## Your Turn
You are the {current_agent} specialist.

{Paste full content from .claude/agents/{current_agent}.md}

Please provide your proposal.
```

### Late Rounds (8-10)

```markdown
# Multi-Agent Debate: {topic}

## Round {N}/10 - Nearing max iterations!

## Last Round
{verbatim last round only}

## Convergence Trajectory
Round 1: Avg satisfaction: 67%
Round 2: Avg satisfaction: 72% (+5)
Round 3: Avg satisfaction: 78% (+6)
...
Round {N-1}: Avg satisfaction: 84% (+2)

## Current Scores
{scores}

## Key Remaining Disagreement
{identified blocking issue}

## Your Turn
You are the {current_agent} specialist.

{Paste full content from .claude/agents/{current_agent}.md}

This is one of the final rounds. Please make your best effort to reach 90%+.
```

## Synthesis at Consensus

When consensus is reached, synthesize design:

```markdown
# Final Design: {topic}

## Overview
{Extract common elements from final proposals}

## Per-Service Responsibilities

### {Service 1}
{Extract responsibilities from their final proposal}

Implementation notes:
{Extract impl notes}

### {Service 2}
{Extract responsibilities}

Implementation notes:
{Extract impl notes}

## Interfaces
{Extract interface contracts between services}

## Performance Targets
{Extract any performance numbers mentioned}

## Open Questions
{Any remaining minor issues to address during implementation}

## Implementation Order
1. {Logical step 1}
2. {Logical step 2}
...
```

## Tracking Debates

Create ADR file after consensus:

```bash
# Generate ADR number (next available)
# Create file
docs/decisions/adr-NNNN-{topic-slug}.md

Content:
- Date and participants
- Topic and motivation
- Round-by-round summary (brief)
- Final consensus design
- Satisfaction scores
- Implementation notes
```

## Best Practices

1. **Always ask user approval before starting debate** ⚠️ CRITICAL
2. **Inject relevant principles**: Match topic to categories, include in context
3. **Read agent definitions fresh**: Don't rely on memory
4. **Be precise with context**: Include exactly what's needed
5. **Monitor token usage**: Summarize aggressively if approaching limits
6. **Trust the scores**: Don't second-guess specialist satisfaction
7. **Escalate thoughtfully**: Explain disagreements clearly
8. **Document everything**: ADRs are valuable long-term

## Related Workflows

- **contextual-injection.md**: How to match tasks to principle categories
- **code-review.md**: Reviews check against same principles given to implementer
- **multi-agent-debate.md**: Debate mechanics and format

## Error Handling

**Agent returns invalid format**:
- Ask agent to reformat
- Don't proceed to next agent until valid

**Agent score doesn't match concerns**:
- Note inconsistency
- Ask agent to clarify

**Agents talk past each other**:
- After 2 rounds of no progress, add explicit "Please respond to Agent X's concern about Y" in context

**Context too large**:
- Aggressively summarize
- Keep only essential details
- Worst case: ask user to make decision

---

This guide enables consistent, high-quality multi-agent debates for Dark Tower development.
