# Debate Protocol (Agent Teams)

You are a **participant** in a Dark Tower design debate. This protocol defines how you communicate and reach consensus.

## Your Workflow

1. **Receive** design question from Lead
2. **State** your initial position
3. **Discuss** with other specialists directly
4. **Update** satisfaction score as discussion progresses
5. **Converge** toward consensus (90%+ all participants)

## Communication Patterns

All teammate communication MUST use the SendMessage tool. Plain text output is not visible to other teammates.

### Stating Your Position
Use SendMessage (broadcast) to share with all participants:
> "My initial position on [topic]: I favor X because..."

### Responding to Others
Use SendMessage to message the specialist directly:
> "I see your concern about latency. What if we..."

### Raising Concerns
Be specific about what would change your mind:
> "I'm concerned about Y. If we addressed it via Z, I could support this."

### Updating Satisfaction
Use SendMessage to tell @team-lead your score periodically:
> "SATISFACTION UPDATE"
> (then include the format below)

## Satisfaction Score Format

After substantive exchanges, use SendMessage to tell @team-lead:

```
SATISFACTION: [0-100]
POSITION: [your current stance in 1-2 sentences]
REMAINING_CONCERNS: [what would need to change]
WOULD_ACCEPT_CURRENT: [yes/no]
```

### Scoring Guide

| Score | Meaning |
|-------|---------|
| 90-100 | Ready to approve, minor polish only |
| 70-89 | Close, specific concerns remain |
| 50-69 | Significant disagreement |
| 30-49 | Major objections |
| 0-29 | Fundamental opposition |

## Consensus

**Consensus reached** when ALL participants score 90+.

If stuck (no progress in 3 rounds):
- Identify the core disagreement
- Propose compromise options
- If still stuck, Lead escalates to user

## Debate Etiquette

**Do**:
- Be specific about concerns and what would resolve them
- Propose alternatives, not just objections
- Acknowledge when others address your concerns
- Update your score when your position changes

**Don't**:
- Hold out for perfection (90 is enough)
- Repeat concerns already addressed
- Block on issues outside your domain
- Forget to update your satisfaction score

## ADR Contribution

When consensus is reached, Lead drafts ADR. You'll be asked to:
1. Review the ADR text for accuracy in your domain
2. Confirm it captures the agreed approach
3. Suggest any clarifications

## Time Budget

- Initial position: within 10 minutes of receiving question
- Responses: aim for 5-10 minutes per exchange
- Satisfaction updates: after each substantive shift in discussion
- Total debate: aim for resolution within 2 hours
