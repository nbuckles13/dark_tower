# User Story: {Title}

**Date**: YYYY-MM-DD
**Status**: Draft | Planning | Ready | In Progress | Complete
**Participants**: {list of specialists}

## Story

As a **{persona}**, I want **{goal}** so that **{benefit}**.

## Requirements

- [ ] {R-1}
- [ ] {R-2}
- [ ] {R-3}

---

## Architecture Validation

**Result**: PASS | FAIL

{If FAIL, list gaps and recommended debates, then stop here.}

---

## Design

### {service-specialist-1}

{What changes in this service. Endpoints, handlers, logic, etc.}

### {service-specialist-2}

{What changes in this service.}

### Database Changes

{Schema changes, migrations, query patterns. Or: N/A — {justification}}

### Protocol Changes

{Message changes, new fields, contract updates. Or: N/A — {justification}}

---

## Cross-Cutting Requirements

### Security

{Auth/authz requirements, threat surface changes, crypto implications.}

### Observability

- **Metrics**: {counters, histograms, gauges}
- **Logs**: {structured log events}
- **Traces**: {spans, propagation}
- **Dashboards**: {panels or dashboards to create/update}

### Test

- **E2E Scenarios**:
  - {Scenario 1}: {steps, validates which requirement}
  - {Scenario 2}: {steps, validates which requirement}
- **Integration Tests**: {key integration points to test}

### Deployment

{Manifest changes, env vars, config. Or: N/A — {justification}}

### Operations

- **Runbook updates**: {what operators need to know. Or: N/A — {justification}}
- **Monitoring/Alerts**: {thresholds, alerts to configure. Or: N/A — {justification}}
- **Rollback**: {how to undo this feature if needed}

---

## Assumptions

| # | Assumption | Made By | Reason Not Blocked |
|---|-----------|---------|-------------------|
| 1 | {what was assumed} | {specialist} | {why a reasonable default} |

## Clarification Questions

| # | Question | Asked By | Status | Answer |
|---|---------|----------|--------|--------|
| 1 | {question} | {specialist} | Pending / Answered | {answer if available} |

---

## Implementation Plan

| # | Task | Specialist | Dependencies | Covers | Status |
|---|------|-----------|--------------|--------|--------|
| 1 | {task description} | {specialist} | — | {code, migration, etc.} | Pending |
| 2 | {task description} | {specialist} | 1 | {code} | Pending |
| 3 | {task description} | {specialist} | 1, 2 | {code} | Pending |
| 4 | {task description} | {specialist} | 2, 3 | {deploy, operations} | Pending |
| 5 | {task description} | {specialist} | 2, 3, 4 | {tests} | Pending |

### Requirements Coverage

| Req | Covered By Tasks |
|-----|-----------------|
| R-1 | 1, 2 |
| R-2 | 2, 3 |
| R-3 | 3, 5 |

### Aspect Coverage

| Aspect | Covered By Tasks | N/A? |
|--------|-----------------|------|
| Code | 1, 2, 3 | |
| Database | 1 | |
| Tests | 5 | |
| Observability | 3 | |
| Deployment | 4 | |
| Operations | 4 | |

---

## Devloop Tracking

{Updated as devloops complete}

| # | Task | Devloop Output | PR | Status |
|---|------|---------------|-----|--------|
| 1 | {task} | | | Pending |
| 2 | {task} | | | Pending |

---

## Revisions

{Added by /user-story --continue when user provides feedback on the plan}

<!--
### Revision 1 — YYYY-MM-DD

**Feedback**: "{user's feedback}"

**Changes**:
- {what changed in the design}
- {what changed in the implementation plan}
-->
