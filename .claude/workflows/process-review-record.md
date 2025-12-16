# Process Review Record (PRR) Workflow

This document describes the PRR process for investigating non-incident issues that reveal process gaps between specialists.

## Overview

A **Process Review Record (PRR)** is triggered when we discover a coordination failure between specialists that didn't cause an incident but indicates a systemic process gap. Unlike debates (for design decisions) or code reviews (for code quality), PRRs focus on **how specialists coordinate**.

## When to Trigger a PRR

Trigger a PRR when you observe:

1. **Cross-specialist coordination failure**: Work by one specialist doesn't align with decisions/work by another
2. **ADR violation**: Implementation doesn't match an existing ADR
3. **Missing handoff**: Specialist A completed work that required Specialist B's review but didn't get it
4. **Documentation drift**: Implementation and documentation are out of sync
5. **Repeated mistakes**: Same type of error occurs multiple times

**Examples**:
- Dashboard uses wrong metric names (Infrastructure ↔ Observability gap)
- API endpoint doesn't follow security ADR (Service ↔ Security gap)
- Test coverage doesn't include critical paths identified in debate (Test ↔ Domain gap)

## PRR vs Other Processes

| Process | Purpose | Trigger | Output |
|---------|---------|---------|--------|
| **Debate** | Design decisions | New feature affecting 2+ services | ADR |
| **Code Review** | Code quality | Before commit/merge | Approved/rejected changes |
| **PRR** | Process improvement | Coordination failure discovered | Process updates + immediate fix |

## PRR Structure

### Participants

Every PRR includes:
- **Involved specialists**: Those whose work was affected by or caused the gap
- **Operations specialist**: Process owner, facilitates the PRR
- **Orchestrator**: Documents findings and coordinates fixes

### Rounds

PRRs are lighter weight than debates:
- **Round 1**: Each involved specialist explains what happened from their perspective
- **Round 2**: Root cause analysis and proposed fixes
- **Round 3** (if needed): Consensus on process improvements

Target: 1-2 rounds for most PRRs. Max 3 rounds.

### Satisfaction Target

Unlike debates (90% consensus), PRRs aim for:
- **100% agreement** on root cause
- **80% agreement** on process improvements (some specialists may have no strong opinion)

## PRR Execution

### Step 1: Identify the Gap

Orchestrator documents:
1. What was expected (per ADR, specialist definition, or established process)
2. What actually happened
3. Which specialists' domains are involved

### Step 2: Gather Perspectives (Round 1)

Invoke each involved specialist to answer:
1. What did you understand your responsibility to be?
2. What information did you have when you did your work?
3. What information would have prevented this gap?

### Step 3: Root Cause Analysis (Round 2)

Use the **5 Whys** technique:
1. Why did the gap occur?
2. Why wasn't it caught earlier?
3. Why didn't the process prevent it?
4. Why wasn't there a check for this?
5. What systemic issue does this reveal?

### Step 4: Recommendations

Each specialist proposes:
1. **Immediate fix**: What code/config changes fix the current issue
2. **Process improvement**: What changes to specialist definitions, workflows, or checklists prevent recurrence

### Step 5: Document and Implement

1. Create PRR document in `docs/process-reviews/prr-NNNN-{slug}.md`
2. Update specialist definitions as agreed
3. Update workflows/checklists as agreed
4. Implement immediate fixes via appropriate specialists

## PRR Document Template

```markdown
# PRR-NNNN: {Descriptive Title}

**Status**: Open | Closed
**Date**: YYYY-MM-DD
**Trigger**: {Brief description of what was observed}
**Participants**: {List of specialists involved}

## Summary

{2-3 sentence summary of what happened and the outcome}

## Investigation

### What Happened

{Factual, blameless description of the gap}

### Expected Behavior

{What should have happened per ADRs, processes, or specialist definitions}

### Root Cause Analysis

1. Why did the gap occur?
   → {Answer}
2. Why wasn't it caught earlier?
   → {Answer}
3. Why didn't the process prevent it?
   → {Answer}
4. Why wasn't there a check for this?
   → {Answer}
5. What systemic issue does this reveal?
   → {Answer}

### Process Gap Identified

{Clear statement of which specialist coordination failed and why}

## Specialist Perspectives

### {Specialist A}

- **Understanding**: {What they thought their responsibility was}
- **Information available**: {What they knew when doing the work}
- **Missing information**: {What would have helped}

### {Specialist B}

{Same structure}

## Recommendations

### Immediate Fix

**Owner**: {Specialist responsible}
**Files**: {List of files to change}
**Changes**: {Description of changes}

### Process Improvements

#### {Improvement 1}

**Target**: {Specialist definition / workflow / checklist}
**Change**: {What to add or modify}
**Rationale**: {Why this prevents recurrence}

#### {Improvement 2}

{Same structure}

## Implementation Status

- [ ] Immediate fix implemented
- [ ] Specialist definition(s) updated
- [ ] Workflow(s) updated
- [ ] PRR closed

## Files Changed

| File | Type | Description |
|------|------|-------------|
| {path} | FIX | {description} |
| {path} | PROCESS | {description} |

## Follow-up

{Any additional work needed, or "None"}
```

## Integration with Development Workflow

### When Orchestrator Discovers a Gap

```
1. Recognize coordination failure
2. Pause current work
3. Create PRR document (skeleton)
4. Invoke involved specialists for Round 1
5. Conduct root cause analysis (Round 2)
6. Document recommendations
7. Invoke specialists to implement fixes
8. Update process artifacts
9. Close PRR
10. Resume original work
```

### Linking PRRs to Other Artifacts

- Reference PRRs in commit messages when fixing gaps
- Link PRRs from updated specialist definitions
- Add PRR numbers to workflow changelogs

## Success Metrics

Track over time:
- **PRR frequency**: Should decrease as processes mature
- **Time to close**: Target <1 day for simple gaps
- **Recurrence rate**: Same gap type should not recur
- **Process coverage**: All specialist interactions have defined handoffs

## Best Practices

### For Orchestrator

1. **Be blameless**: Focus on process, not people
2. **Be specific**: Document exact files, lines, decisions
3. **Be constructive**: Every gap is a learning opportunity
4. **Be thorough**: Update all affected artifacts

### For Specialists

1. **Be honest**: Explain what you actually knew/did
2. **Be helpful**: Propose concrete improvements
3. **Be open**: Accept that processes can improve
4. **Be proactive**: Suggest checks you wish existed

### For Process Improvements

1. **Minimal viable fix**: Don't over-engineer the solution
2. **Specific triggers**: When exactly should the check happen?
3. **Clear ownership**: Who is responsible for the check?
4. **Testable**: Can we verify the improvement works?

---

**Remember**: PRRs are about making the system better, not assigning blame. Every gap is an opportunity to improve how specialists work together.
