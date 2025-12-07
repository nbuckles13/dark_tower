# Multi-Agent Debate Workflow

This document describes how specialist agents collaborate to reach consensus on cross-cutting designs through structured debate.

## Overview

When a feature or design decision affects multiple subsystems, we use a **multi-agent debate loop** where specialists iterate until consensus is reached or we escalate to human decision.

## Identifying Which Agents Participate

### Decision Matrix

| Feature Type | Agents Involved |
|--------------|----------------|
| API endpoint (GC only) | GC + Test + Security + Observability + Operations |
| Signaling message (MC only) | MC + Protocol + Test + Security + Observability + Operations |
| Media forwarding change | MH + Test + Security + Observability + Operations (maybe Protocol) |
| Cross-service feature | Multiple agents based on impact + Test + Security + Observability + Operations |
| Database schema change | Database + affected service agents + Test + Security + Observability + Operations |
| Performance optimization | Affected service agents + Test + Security + Observability + Operations |
| Infrastructure change | Infrastructure + affected service agents + Test + Security + Observability + Operations |
| Deployment/scaling change | Operations + Infrastructure + Test + Security + Observability |

### Examples

**Feature**: "Add endpoint to update meeting settings"
- **Agents**: Global Controller, Test, Security, Observability, Operations (maybe Database if schema changes)
- **Rationale**: GC implements, Test ensures testability, Security validates auth/authz, Observability defines metrics/traces, Operations ensures deployability

**Feature**: "Implement adaptive bitrate control"
- **Agents**: Meeting Controller, Media Handler, Protocol, Test, Security, Observability, Operations
- **Rationale**: MC sets policy, MH executes, Protocol defines messages, Test ensures E2E testing, Security validates DoS protection, Observability defines SLOs for quality, Operations ensures failure modes are handled

**Feature**: "Add participant mute functionality"
- **Agents**: Meeting Controller, Protocol, Database, Test, Security, Observability, Operations
- **Rationale**: MC enforces, Protocol defines messages, DB stores mute state, Test ensures coverage, Security validates authorization, Observability ensures mute events are auditable, Operations validates runbook for mute-related issues

**Feature**: "Optimize layout computation performance"
- **Agents**: Meeting Controller, Test, Security, Observability, Operations
- **Rationale**: Internal MC optimization, Test validates performance, Security ensures no DoS vulnerability introduced, Observability defines latency SLOs, Operations ensures degradation path exists

**Feature**: "Deploy to new region"
- **Agents**: Infrastructure, Operations, Test, Security, Observability
- **Rationale**: Infrastructure provisions resources, Operations defines deployment strategy, Test validates cross-region behavior, Security ensures data residency compliance, Observability defines cross-region dashboards

**Rule of thumb**: Test, Security, Observability, and Operations are ALWAYS included in debates. If unsure about domain specialists, err on the side of including more agents. Extra perspectives help.

## The N-Agent Debate Loop

### Configuration

```yaml
target_satisfaction: 90  # All agents must reach ≥90%
max_iterations: 10       # Max rounds before escalation
min_progress: 5          # Must improve by 5% to continue
context_window_limit: 50000  # Tokens - start summarizing history
```

### Agent Response Format

Each agent returns:

```markdown
## Proposal
[Detailed design from this agent's perspective]

## Satisfaction Score
[Integer 0-100]

## Rationale
[Why this score? What's good? What's concerning?]

## Blocking Concerns (if < 90%)
1. [Critical issue #1 that prevents satisfaction]
2. [Critical issue #2]

## Suggested Changes
[Specific changes that would increase satisfaction to ≥90%]

## Performance Impact (if applicable)
[Latency/throughput/scalability implications]
```

### Loop Algorithm

```
function multi_agent_debate(topic, agents):
    # Initialize
    proposals = []
    satisfaction_scores = {agent: 0 for agent in agents}
    round = 0

    # Separate cross-cutting specialists from domain agents
    # Cross-cutting specialists have fixed order at end of each round
    cross_cutting = ['Observability', 'Operations', 'Test', 'Security']
    domain_agents = [a for a in agents if a not in cross_cutting]

    # Randomize domain agents initially (avoid order bias)
    if len(domain_agents) > 1:
        shuffle(domain_agents)

    while round < max_iterations:
        round += 1
        round_proposals = []

        # Re-randomize domain agents every 3 rounds (rounds 4, 7, 10)
        if len(domain_agents) > 1 and round > 1 and (round - 1) % 3 == 0:
            shuffle(domain_agents)

        # Build agent order: domain agents first (randomized), then cross-cutting in fixed order
        # Order: Domain agents → Observability → Operations → Test → Security
        agent_order = domain_agents.copy()
        for cc in ['Observability', 'Operations', 'Test', 'Security']:
            if cc in agents:
                agent_order.append(cc)

        # Each agent responds in sequence
        for agent in agent_order:
            # Build context (with summarization if needed)
            context = build_context(proposals, agent)

            # Invoke agent
            response = invoke_specialist(
                agent=agent,
                topic=topic,
                context=context,
                round=round
            )

            round_proposals.append((agent, response))
            satisfaction_scores[agent] = extract_score(response)

        # Add round to history
        proposals.append({
            'round': round,
            'proposals': round_proposals,
            'scores': copy(satisfaction_scores)
        })

        # Check for consensus
        if all(score >= target_satisfaction for score in satisfaction_scores.values()):
            return {
                'status': 'consensus',
                'rounds': round,
                'final_design': synthesize_design(round_proposals),
                'scores': satisfaction_scores
            }

        # Check for progress
        if round > 3 and not is_progressing(proposals, min_progress):
            return {
                'status': 'stalemate',
                'rounds': round,
                'positions': round_proposals,
                'scores': satisfaction_scores,
                'reason': 'No progress in last 3 rounds'
            }

    # Max iterations reached
    return {
        'status': 'max_iterations',
        'rounds': max_iterations,
        'positions': round_proposals,
        'scores': satisfaction_scores
    }
```

### Agent Ordering Strategy

**Fixed Order for Cross-Cutting Specialists**:
- Observability specialist goes after all domain agents
- Operations specialist goes after Observability
- Test specialist goes after Operations
- Security specialist ALWAYS goes last in each round
- This allows cross-cutting specialists to review all domain proposals before providing feedback
- Order: Domain agents → Observability → Operations → Test → Security

**Domain Agent Randomization**:
When multiple domain agents (not cross-cutting) are debating:
- Randomize domain agents once at start of debate (Round 1)
- Re-randomize domain agents every 3 rounds (Rounds 4, 7, 10)
- This provides structure while preventing order bias

**Example ordering** (7-agent debate: GC, MC, Protocol, Observability, Operations, Test, Security):
- Round 1: [GC, Protocol, MC, Observability, Operations, Test, Security] ← domain agents randomized initially
- Round 2: [GC, Protocol, MC, Observability, Operations, Test, Security] ← same order
- Round 3: [GC, Protocol, MC, Observability, Operations, Test, Security] ← same order
- Round 4: [MC, GC, Protocol, Observability, Operations, Test, Security] ← domain agents re-randomized
- Cross-cutting specialists always go last in fixed order: Observability → Operations → Test → Security

### Context Management

As debate progresses, context grows. Strategy:

**Rounds 1-3**: Full history
- Include all proposals verbatim
- Agents see complete debate

**Rounds 4-7**: Summarize early rounds
- Keep last 2 rounds verbatim
- Summarize rounds 1-N-2 as:
  ```
  Early rounds summary:
  - Round 1: [Agent A] proposed X, [Agent B] concerned about Y (scores: A=90, B=45)
  - Round 2: [Agent A] revised to address Y, [Agent B] satisfied (scores: A=85, B=75)
  ```

**Rounds 8+**: Aggressive summarization
- Keep only last round verbatim
- Summarize convergence trajectory
- Highlight blocking concerns

**Context template**:
```markdown
# Multi-Agent Debate: {topic}

## Participants
{list of agents}

## Current Round: {N}/10

## Recent History
{last 1-2 rounds verbatim}

## Earlier Rounds Summary
{summarized history}

## Current Satisfaction Scores
{agent: score for all agents}

## Your Turn
You are the {agent_name} specialist.
[Agent's full specialist definition]

Please provide your proposal following the standard format.
```

### Progress Detection

Track whether debate is moving toward consensus:

```python
def is_progressing(proposals, min_progress=5):
    # Get last 3 rounds
    if len(proposals) < 3:
        return True  # Too early to tell

    recent = proposals[-3:]

    # Calculate average satisfaction across all agents
    avg_scores = [
        sum(round['scores'].values()) / len(round['scores'])
        for round in recent
    ]

    # Check if improving
    improvement = avg_scores[-1] - avg_scores[0]

    return improvement >= min_progress
```

## Consensus Synthesis

When consensus is reached, synthesize final design:

```python
def synthesize_design(final_round_proposals):
    design = {
        'overview': extract_common_elements(proposals),
        'per_agent_responsibilities': {},
        'interfaces': extract_interfaces(proposals),
        'performance_targets': extract_targets(proposals),
        'open_questions': []
    }

    for agent, proposal in final_round_proposals:
        design['per_agent_responsibilities'][agent] = {
            'what_this_agent_owns': extract_responsibilities(proposal),
            'implementation_notes': extract_notes(proposal)
        }

    return design
```

## Escalation Scenarios

### 1. Max Iterations Reached

**Action**: Present both/all positions to user

```markdown
Debate reached max iterations (10 rounds) without consensus.

Final satisfaction scores:
- Meeting Controller: 85%
- Media Handler: 78%

MC Position:
[MC's final proposal]

MH Position:
[MH's final proposal]

Key disagreement:
- MC wants centralized control for observability
- MH wants autonomous decisions for latency

Your decision needed: Which approach should we take?
Options:
A) MC's approach (centralized)
B) MH's approach (autonomous)
C) Hybrid (I'll facilitate one more round)
```

### 2. Stalemate (No Progress)

**Action**: Identify blocking issue and ask user

```markdown
Debate has stalled. Satisfaction scores haven't improved in 3 rounds.

Blocking issue:
[Specific technical disagreement]

Current scores:
- Protocol: 92%
- Database: 65%

Database concern:
"This requires a migration with 2-hour downtime on large installations"

Suggested resolutions:
A) Accept the migration (schedule maintenance)
B) Design a zero-downtime migration (more complex)
C) Reconsider the feature (do we really need this?)

Your input needed.
```

### 3. Fundamental Disagreement

**Action**: Recognize architectural tension

```markdown
Agents have identified a fundamental architectural trade-off:

Latency vs. Observability

- MH specialist: "Autonomous decisions add <10ms latency" (95% satisfied)
- MC specialist: "Centralized control enables debugging" (60% satisfied)

This isn't solvable through iteration - it's a system-level trade-off.

Recommendation: Implement MH's low-latency approach with enhanced telemetry
to satisfy MC's observability needs.

Proceed? [Y/n]
```

## Example Workflow

### Simple Case (quick consensus)

```
User: "Add QualityAlert message for immediate quality drops"

Orchestrator: Analyzing impact...
  - Touches signaling protocol (Protocol agent)
  - Touches media handler behavior (MH agent)
  - Cross-cutting specialists always included

Initiating 6-agent debate: Protocol, Media Handler, Observability, Operations, Test, Security

Round 1:
  [Order: Protocol, MH, Observability, Operations, Test, Security]
  Protocol: 85% - "Good idea, but field naming could be clearer"
  MH: 90% - "Perfect, this is exactly what we need"
  Observability: 88% - "Need metric for alert frequency, add trace context"
  Operations: 90% - "Need runbook for high-frequency alerts, otherwise good"
  Test: 92% - "Easy to test, clear contract"
  Security: 91% - "No security concerns, normal signaling"

Round 2:
  [Order: Protocol, MH, Observability, Operations, Test, Security]
  Protocol: 93% - "Better field names, added trace_id field"
  MH: 91% - "Agreed on naming"
  Observability: 92% - "Trace context included, metrics defined"
  Operations: 91% - "Runbook drafted, thresholds clear"
  Test: 93% - "Still good"
  Security: 92% - "Still secure"

CONSENSUS (2 rounds)

Final Design:
[Consolidated QualityAlert message definition with observability requirements]

Implementing...
```

### Complex Case (multiple rounds)

```
User: "Implement participant mute functionality"

Orchestrator: Analyzing impact...
  - Database: Store mute state
  - Protocol: Define mute messages
  - Meeting Controller: Enforce mute
  - Cross-cutting specialists always included

Initiating 7-agent debate: Database, Protocol, Meeting Controller, Observability, Operations, Test, Security

Round 1:
  [Order: Database, Protocol, MC, Observability, Operations, Test, Security]
  Database: 70% - "Where do we store mute state? Redis or PostgreSQL?"
  Protocol: 85% - "Need both MuteParticipant command and ParticipantMuted event"
  MC: 80% - "Who can mute whom? Need permission system"
  Observability: 75% - "Need metrics for mute frequency, trace mute operations"
  Operations: 70% - "What's the failure mode if mute state is lost? Need runbook"
  Test: 88% - "Testable, need clear permission scenarios defined"
  Security: 60% - "CRITICAL: Who can mute whom? Need authorization checks, audit logging"

Round 2:
  [Order: Database, Protocol, MC, Observability, Operations, Test, Security]
  Protocol: 88% - "Added role field to determine permissions"
  MC: 75% - "Better, but need to clarify server-side enforcement"
  Database: 78% - "Suggest Redis for live state, PostgreSQL for history"
  Observability: 85% - "Mute metrics defined, audit events traced"
  Operations: 82% - "Runbook drafted, but need graceful degradation for Redis failure"
  Test: 90% - "Clear test scenarios emerging"
  Security: 75% - "Better with roles, but need explicit permission checks at MC level"

Round 3:
  [Order: Database, Protocol, MC, Observability, Operations, Test, Security]
  MC: 92% - "Server-side enforcement designed, permission checks in place"
  Database: 91% - "Agreed on hybrid storage approach"
  Protocol: 93% - "Messages are clean and complete"
  Observability: 91% - "SLO defined: mute latency <100ms p99, dashboard ready"
  Operations: 90% - "Graceful degradation: fall back to PostgreSQL if Redis down"
  Test: 93% - "Can test all permission combinations, including chaos tests"
  Security: 91% - "Good! Audit log captures mute events, proper authz checks"

CONSENSUS (3 rounds)

Final Design:
  Database: Redis for live mute state, PostgreSQL audit log
  Protocol: MuteParticipant (command) + ParticipantMuted (event)
  MC: Server-side enforcement, role-based permissions
  Observability: Mute latency SLO, audit trace events
  Operations: Runbook for mute issues, Redis failover to PostgreSQL
  Test: E2E tests for all permission scenarios + chaos tests
  Security: Authorization checks, audit logging, rate limiting

Implementing...
```

## Best Practices

### For Orchestrator (Claude Code)

1. **Identify agents early**: Analyze feature, list affected subsystems
2. **Set clear topic**: Be specific about what we're designing
3. **Monitor progress**: Track satisfaction scores each round
4. **Summarize when needed**: Don't let context explode
5. **Escalate thoughtfully**: Explain disagreement clearly to user

### For User

1. **Trust the process**: Let agents iterate
2. **Intervene on stalemate**: Make decisions when needed
3. **Provide context**: If you have preference, state it upfront
4. **Document outcomes**: Debate transcripts become ADRs

### For Specialist Agents

1. **Be specific**: "Add 50ms latency" not "slower"
2. **Propose solutions**: Don't just complain
3. **Update scores honestly**: Don't stick at 89% out of spite
4. **Respect boundaries**: Don't dictate other agents' domains
5. **Converge willingly**: 90% satisfied is good enough

## File Artifacts

Each debate produces:

```
docs/decisions/adr-NNNN-{topic-slug}.md
  - Topic and motivation
  - Participating agents
  - Round-by-round summary
  - Final consensus design
  - Satisfaction scores
  - Implementation notes

Example:
docs/decisions/adr-0015-adaptive-bitrate-control.md
```

## Integration with Development Workflow

```
1. User describes feature
2. Orchestrator identifies affected agents
3. If multiple agents:
   a. Initiate debate
   b. Iterate to consensus (or escalate)
   c. Document in ADR
4. Implement the consensus design
5. Each agent validates their part
6. Integration testing
```

---

This workflow ensures cross-cutting designs are well-thought-out before implementation, preventing costly rewrites and integration issues.
