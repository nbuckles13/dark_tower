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

For each new endpoint, RPC, cross-service flow, or client-driven SDK behavior, list env-test scenarios. Driver is per-scenario detail (Rust HTTP/RPC, headless browser, etc.); all env-tests run against the live Kind cluster in the same Layer 7 slot. See ADR-0028 §7 (amended 2026-06-25) for the unified Env-Test tier framing.

- **Env-Tests** (live-cluster verification, any driver — owned by implementing specialist, paired with test):
  - {Scenario 1}: driver={Rust|Browser}, owned by {specialist}, validates R-{N}
  - {Scenario 2}: ...
- **Integration Tests** (per-service, handler/repo level — NOT against live cluster; against test postgres sidecar or similar in-process fakes):
  - {Key integration points}

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

Human-readable intent only. **No `Status` column and no `Devloop Output`
column** — both live in the manifest below and nowhere else (ADR-0035 §4).
Prose descriptions and machine state have different edit rhythms, and a table
mixing them drifts on the fast-moving column.

| # | Task | Specialist | Dependencies | Covers |
|---|------|-----------|--------------|--------|
| 1 | {task description} | {specialist} | — | {code, migration, etc.} |
| 2 | {task description} | {specialist} | 1 | {code} |
| 3 | {task description} | {specialist} | 1, 2 | {code} |
| 4 | {task description} | {specialist} | 2, 3 | {deploy, operations} |
| 5 | {task description} | {specialist} | 2, 3, 4 | {tests} |

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

## Task Metadata (dt-story manifest v1)

The **only** home for per-task machine state: status, specialist, `env_tests`,
deps, prompt, tag, and the devloop-output slug. `run-story` reads and writes
it; `/close-story` reads status and slug from it. There is no §Devloop
Tracking table — it was a second home for both facts and it demonstrably
drifted (ADR-0035 §4).

**Emit this block with `dt-story add-task`, not by hand** — see
`/user-story` Step 10.4. The write verb refuses to produce a manifest
containing a markdown fence, which is what keeps a prompt carrying a fenced
example from silently truncating the block.

<!-- THREE CONSTRAINTS ON EDITING THIS SECTION. This file is discovered by
     scripts/guards/simple/validate-story-manifest.sh (it greps for the
     marker line, so the template is validated in CI exactly like a real
     story), and breaking any of these reds Layer 3 REPO-WIDE, not just here.

     1. Real values, not {placeholders}. `{specialist}` is YAML flow-mapping
        syntax and deserializes as a map, not a string.
     2. Exactly ONE marker-bearing block in the file. A second example block
        fails with "found 2 manifest blocks".
     3. NO `- id: N` lines anywhere OUTSIDE this block. dt-story treats a
        manifest-shaped line outside the block as the signature of a
        silently-truncated manifest and hard-errors. A template is the file
        most likely to want an illustrative task snippet in prose — put any
        such example INSIDE the block below. -->

```yaml
# task-metadata (dt-story manifest v1)
story: story-slug
tasks:
- id: 1
  status: pending
  specialist: database
  env_tests: false
  prompt: Self-contained devloop prompt. The devloop sees only this text, so
    it must carry every fact needed to do the work. No fenced code blocks —
    a fence closes the manifest block early. Inline `backticks` are fine.
    Record pairing as prose here ("Pair with protocol"), never in the
    specialist field.
  tag: story-story-slug-task-1
- id: 2
  status: pending
  specialist: global-controller
  env_tests: true
  deps:
  - 1
  prompt: Second task. Runs only after task 1 completes.
  tag: story-story-slug-task-2
```

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
