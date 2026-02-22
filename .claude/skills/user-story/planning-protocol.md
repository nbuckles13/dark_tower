# Planning Protocol — Service & Domain Specialists

For: auth-controller, global-controller, meeting-controller, media-handler, database, protocol, infrastructure

## Workflow

1. Architecture check + propose requirements → report to @team-lead
3. (Wait for requirements to be confirmed by user)
4. Design contribution + propose devloop tasks (or opt out)
5. Interface resolution with other specialists
6. Finalize tasks and dependencies

## Communication

All communication MUST use SendMessage. Plain text is invisible to teammates.

## Architecture Check + Requirements Proposal

Report to @team-lead with your architecture check AND proposed requirements:

```
@team-lead — ARCHITECTURE CHECK: PASS

PROPOSED REQUIREMENTS:
- {requirement relevant to your domain}
- {another requirement if applicable}
```

**PASS**: New endpoints, tables, columns, messages, fields on existing services/channels/patterns.
**FAIL**: New service-to-service paths, new protocol channels, changes to service boundaries, new infrastructure components, fundamental pattern changes. Needs `/debate` first. Include GAPS and RECOMMENDED DEBATES.

Requirements should be observable, testable outcomes — WHAT the system does, not HOW.

**Opt-out** (if this story doesn't involve your domain):
```
@team-lead — ARCHITECTURE CHECK: PASS
Nothing needed from {your-name}. {Brief justification.}

CONSUMER COMPATIBILITY:
- {consuming service} uses {my interface} — compatible: {yes/no/needs verification}
  Verified against: {code file, not just ADR}
```

Verify compatibility against **actual code** (struct definitions, schema, message types), not just ADR descriptions. ADRs can drift from implementation.

**After opt-out — interface validation**: Even if your domain has no implementation work, you are NOT done until confirmed requirements are broadcast. When requirements reference your domain's interfaces (e.g., a requirement mentions your token format, your schema, your protocol messages), you MUST validate those references are correct. "No implementation work" ≠ "no review responsibility."

```
@team-lead — INTERFACE CHECK for R-{N}: {Confirmed correct | Incorrect — {what's wrong}}
```

## Design Contribution

```
## {Your Name} — Design Contribution

### Changes Required
- {what changes, specific files/endpoints/tables/messages}

### Interface Requirements
- FROM {specialist}: I need {what}
- TO {specialist}: I provide {what}

### Proposed Devloop Tasks

Task: "{description — clear enough to use as a /devloop prompt}"
  Specialist: {your-name}
  Dependencies: {task numbers or "none"}
  Covers: {code | migration | tests | deploy | etc.}
  Design context: {key decisions}

Aim for 1-3 tasks. Each task stays within your domain. A good task is one a devloop can implement and reviewers can hold in their head.

### Clarification Questions (if any)
- {question} — BLOCKING / NON-BLOCKING
  Assumption if non-blocking: {what you'll assume}
```

## Interface Resolution

Discuss interfaces directly with other specialists via SendMessage. Update your tasks if interfaces change. Tell @team-lead about any updates.
