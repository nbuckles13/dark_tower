---
name: user-story
description: Decompose a user story into ordered devloop tasks across specialists. Validates architectural fit, designs within existing boundaries, and produces a complete implementation plan from code to deploy to operate.
---

# User Story Skill (Agent Teams)

Decompose a user-facing goal into an ordered sequence of devloop tasks. Each task maps to a `/devloop` invocation with a specific specialist. The skill validates that the story fits within existing architecture and fails fast if it doesn't.

## When to Use

Use `/user-story` when:
- You have a user-facing feature to implement
- The feature spans one or more services
- You need a plan that covers code, tests, observability, deployment, and operations

## When NOT to Use

- **Architecture doesn't exist yet** → Use `/debate` first to establish it, then come back
- **Pure refactoring with no user-facing goal** → Use `/devloop` directly
- **Single well-scoped task** → Use `/devloop` directly
- **Design question needing consensus** → Use `/debate`

## Key Distinction from `/debate`

| | `/debate` | `/user-story` |
|---|-----------|---------------|
| **Input** | Design question | User-facing goal |
| **Purpose** | Reach consensus on HOW | Plan WHAT to build and in WHAT ORDER |
| **Design scope** | Unconstrained (new patterns, channels, boundaries) | Constrained to existing architecture |
| **Output** | ADR | Ordered devloop task list |
| **Rounds** | Up to 10 (consensus-seeking) | Up to 3 (planning) |

## Arguments

```
/user-story "story description"                                          # new story
/user-story "feedback" --continue=YYYY-MM-DD-slug                        # revise existing story
```

- **story description**: The user story or feature to decompose (required for new)
- **--continue**: Slug of an existing story to revise with user feedback (see Continue Mode)

## Team Composition

**All specialists are always invited.** Every specialist participates in every planning session. Specialists whose domain is not relevant to the story confirm "nothing needed from my domain" with a brief justification and are done. This is cheaper than missing a perspective and discovering it mid-devloop.

**Service specialists**:
- `auth-controller` — Authentication, JWT, JWKS, federation
- `global-controller` — HTTP/3 API, meeting management, geographic routing
- `meeting-controller` — WebTransport signaling, sessions
- `media-handler` — Media routing, quality adaptation

**Domain specialists**:
- `database` — Schema, migrations, query patterns
- `protocol` — Protocol Buffers, API contracts, versioning
- `infrastructure` — Kubernetes, Terraform, CI/CD, deploy manifests

**Cross-cutting specialists** (mandatory in all planning sessions):
- `security` — Security implications, auth/authz requirements
- `test` — **Extra role**: define E2E test requirements for the feature
- `observability` — **Extra role**: define metrics, logs, and traces requirements
- `operations` — **Extra role**: define operational requirements (runbooks, monitoring, alerts)

**Invariant**: Every specialist either proposes at least one devloop task OR explicitly confirms "nothing needed from my domain" with justification. Silent non-participation is not acceptable — the lead must have an explicit signal from each specialist.

## Instructions

### Step 1: Parse the Story

Extract from the user's description:
- **Persona**: Who is this for?
- **Goal**: What do they want to do?
- **Benefit**: Why do they want it?

Do NOT draft requirements yourself. Specialists will propose requirements from their domains in Step 4.

### Step 2: Create Story File

Create `docs/user-stories/YYYY-MM-DD-{story-slug}.md` using the template from `docs/user-stories/_template.md`. Populate the Story and Participants sections. Leave Requirements blank — specialists will propose them.

### Step 3: Spawn Planning Team

Spawn all 11 specialists as teammates using `subagent_type` (identity auto-loaded from `.claude/agents/{name}.md`). Each specialist uses their own name for both `name` and `subagent_type`. The Lead is automatically named `team-lead`.

**Inject the appropriate protocol file per specialist type:**

| Specialist type | Protocol file |
|---|---|
| Service + domain (auth-controller, global-controller, meeting-controller, media-handler, database, protocol, infrastructure) | `planning-protocol.md` |
| Security | `planning-protocol-security.md` |
| Test | `planning-protocol-test.md` |
| Cross-cutting (observability, operations) | `planning-protocol-cross-cutting.md` |

**Specialist prompt** — spawn with `subagent_type: "{name}"` and this prompt:

```
You are participating in a Dark Tower user story planning session.

## Your Responsibility

Define everything needed for this feature to ship correctly in your domain. Do not self-censor requirements because they seem like too much work, or defer items because they've been deferred before. The lead and user will manage scope — your job is completeness. If something is needed for this feature to be production-ready in your domain, propose it.

**Anti-pattern: inheriting prior deferrals as constraints.** Your knowledge files may document gaps as "deferred" or "tracked as tech debt." During story planning, treat these as candidates for resolution, not as immovable constraints. The question is not "is this still deferred?" but "does this story need it?" If yes, propose closing it.

## Step 0: Load Knowledge (MANDATORY)

**Before doing ANY other work**, read ALL `.md` files from `docs/specialist-knowledge/{your-specialist-name}/` to load your accumulated knowledge. Do NOT skip this step.

## Planning Protocol

{contents of the appropriate protocol file from the table above}

## The Story

As a {persona}, I want {goal} so that {benefit}.

## Participants

{list of all 11 specialists in this session}
```

### Step 4: Architecture Validation + Requirements Proposals

Wait for all specialists to report. Each specialist sends TWO things:

1. **Architecture check** (PASS/FAIL)
2. **Proposed requirements** from their domain (or "nothing needed from my domain")

**Architecture check + requirements proposal format** (from each specialist):
```
ARCHITECTURE CHECK: PASS | FAIL
GAPS: (if FAIL) List of architectural gaps
RECOMMENDED DEBATES: (if FAIL) Debate topics to resolve gaps

PROPOSED REQUIREMENTS:
- {requirement relevant to this specialist's domain}
- ...
```

**Criteria for PASS** (design within existing boundaries):
- New endpoints on existing services
- New tables/columns in existing databases
- New messages/fields on existing gRPC channels
- New message types within existing proto files
- Design choices within established patterns
- New metrics, logs, traces on existing instrumentation

**Criteria for FAIL** (needs `/debate` first):
- New service-to-service communication paths
- New protocol channels or transport mechanisms
- Changes to service boundaries or responsibilities
- New infrastructure components (new databases, caches, queues)
- Fundamental changes to existing patterns

**If ANY specialist reports FAIL**:

Stop the planning session. Report to user:

```
**User Story Blocked — Architecture Gap**

Story: {description}

The following architectural gaps prevent decomposition:

{list gaps from each specialist that reported FAIL}

Recommended debates:
1. /debate "{topic 1}"
2. /debate "{topic 2}"

Please resolve these via debate, then retry this user story.
```

**If ALL specialists report PASS**: Proceed to requirements synthesis.

### Step 5: Requirements Synthesis and User Confirmation

The lead collects requirements proposals from all specialists and synthesizes them into a consolidated list.

**Lead's role**:
- Collect proposed requirements from all specialists
- Deduplicate overlapping proposals (multiple specialists may propose similar requirements)
- Organize into a numbered list (R-1, R-2, ...)
- Ensure requirements are specific and testable
- Note which specialist(s) proposed each requirement

**Present to user for confirmation:**

```
**Proposed Requirements**

Story: {persona} wants {goal} so that {benefit}

The planning team proposes these requirements:

- [ ] R-1: {description} (from: {specialist})
- [ ] R-2: {description} (from: {specialist})
...

Would you like to adjust these before the team proceeds to design?
```

**After user confirms**: Broadcast confirmed requirements to all specialists, update the story file with the confirmed requirements, and proceed to the design phase.

### Step 6: Design Phase

**First**: Broadcast the confirmed requirements to all specialists — including those who opted out. Opted-out specialists must validate any requirements that reference their domain's interfaces (token formats, schema, protocols) before disengaging. See "interface validation" in the planning protocols.

Specialists collaborate to design their pieces and propose devloop tasks. Up to 3 rounds.

**Round 1 — Initial contributions**:
Each specialist shares their design contribution AND proposes their devloop tasks (see planning protocol for contribution format):
- Service specialists: What changes in their service, proposed task(s)
- `database`: Schema changes, migration task(s)
- `protocol`: Message changes, contract task(s)
- `security`: Auth/authz requirements, threat surface changes
- `test`: E2E test scenarios for the feature, test task(s)
- `observability`: Metrics, logs, traces to add, instrumentation task(s)
- `operations`: Runbook updates, monitoring additions, rollback considerations, ops task(s)
- `infrastructure`: Deploy manifest changes, infra task(s)

**Round 2 — Interface resolution**:
Specialists discuss interfaces between their pieces:
- "What data does this endpoint need to return?"
- "What error codes should we use?"
- "What fields does this message need?"
Specialists refine their proposed tasks based on interface agreements.

**Round 3 — Final adjustments**:
Address remaining open items, finalize task proposals.

**If design can't converge in 3 rounds**: Likely indicates an architectural gap. Escalate to user with specific blockers and recommend a debate.

### Step 7: Participation Check

Before finalizing, the lead verifies that every specialist has reported in — either with proposed tasks or an explicit "nothing needed from my domain."

**Check**: For each of the 11 specialists, the lead must have received one of:
- Proposed devloop task(s) with design contribution
- Explicit "nothing needed from my domain" with brief justification

**If any specialist has not reported**: Message them directly and wait for their response before proceeding. Do not finalize the plan with silent gaps.

### Step 8: Clarification Flow

During design, specialists may flag questions for the user.

**Non-blocking (default)**: Specialists make reasonable assumptions, document them, and continue. Assumptions appear in the story file for user review.

**Blocking**: Only when the assumption could send the design down a fundamentally wrong path. Specialists flag to @team-lead, who batches questions and presents to user:

```
**Clarification Needed**

Story: {description}

Questions from the planning team:

1. [{specialist-name}]: {question}
   Context: {why this matters}

2. [{specialist-name}]: {question}
   Context: {why this matters}

The team can continue with assumptions for non-critical items. Please answer the blocking questions.
```

### Step 9: Assemble Implementation Plan

The lead collects all proposed tasks from specialists and assembles the ordered plan.

**Lead's role** (coordination, not design):
- Collect proposed tasks from all specialists
- Resolve dependency ordering
- Check coverage: every requirement maps to at least one task
- Check coverage: every cross-cutting requirement (observability, test, operations) is addressed or explicitly N/A with justification
- Identify parallelization opportunities (tasks with no mutual dependencies)

**Ordering principles**:
1. Schema/migration first (other tasks depend on data model)
2. Protocol/message definitions early (services depend on contracts)
3. Service implementations (can often parallelize across services)
4. Cross-cutting instrumentation after core logic exists
5. Deploy/operational changes after code is written
6. E2E tests last (depend on deployed, operational services)

**Task granularity** — each task should stay within one specialist's domain:
- If a task description spans domains (endpoint code + dashboard panels + runbook), split it
- Trivially small cross-cutting work is fine within a service task (e.g., adding `#[instrument]` to a new handler)
- Substantial cross-cutting work gets its own task (e.g., new dashboard with alert rules, new runbook section)
- The natural range is 1-3 tasks per specialist who has work to do. A specialist proposing a single task that covers code + tests + instrumentation + dashboards is too broad. A specialist splitting every function into its own task is too narrow.

**Aspect coverage** — each of these must appear in the plan or be marked N/A with justification:
- Code changes
- Database migrations
- Tests (unit, integration, E2E)
- Observability (metrics, logs, traces)
- Deployment changes
- Operational changes (runbooks, monitoring, alerts)

**Lead verification checklist** — before finalizing, verify:
- [ ] **Defaults**: All configurable defaults are explicitly stated in the design with rationale. Security-sensitive defaults (auth required, encryption, guest access) follow least-privilege.
- [ ] **Cross-cutting double-check**: Security checklist answered (6 items), test checklist answered (4 items), observability checklist answered (4 items), operations checklist answered (4 items). If any specialist answered "N/A", the justification is genuine.

### Step 10: Write Completed Story File

Update the story file with all sections populated:
- Architecture validation: PASS
- Design sections from each specialist
- Cross-cutting requirements (observability, test, operations)
- Assumptions made (which specialist, what they assumed, why they didn't block)
- Clarification questions (answered or pending)
- Implementation plan (ordered devloop task table)

### Step 11: Report and Review

Report to user, then **wait for confirmation** before shutting down. The team stays alive so the user can request adjustments without a full `--continue` respawn.

```
**User Story Ready for Review**

Story: {title}
Participants: {list}
Devloop tasks: {count}

Implementation plan:
| # | Task | Specialist | Dependencies |
|---|------|-----------|--------------|
| 1 | ... | ... | — |
| 2 | ... | ... | 1 |
| ... | ... | ... | ... |

Assumptions made: {count} (review in story file)
Clarification questions: {count pending} (review in story file)

Story file: docs/user-stories/YYYY-MM-DD-{slug}.md

Please review the story file. You can request adjustments now, or confirm to finalize.
```

**If user requests adjustments**: Relay feedback to relevant specialists, they update their contributions, lead updates the story file. No respawn needed.

**If user confirms**: Shutdown all teammates. Report:

```
**User Story Finalized**

Story file: docs/user-stories/YYYY-MM-DD-{slug}.md

Next step — run devloops in order:
  /devloop "{task 1 description}" --specialist={name}
  /devloop "{task 2 description}" --specialist={name}
  ...
```

## Continue Mode (`--continue`)

Revise an existing story based on user feedback. Use this after reviewing a story file when assumptions need correcting, design needs adjusting, or tasks need restructuring.

```
/user-story "feedback" --continue=2026-02-19-create-meeting
```

### How It Works

1. **Load context**: Read `docs/user-stories/{slug}.md` to get the full existing story — design, assumptions, tasks, everything
2. **Record feedback**: Add a "Revision" section to the story file:
   ```markdown
   ## Revision {N} — YYYY-MM-DD

   **Feedback**: "{user's feedback}"
   **Changes**: {TBD — populated after replanning}
   ```
3. **Respawn the full team**: Same participants as the original session, loaded from the story file's Participants field. Same prompts as a new session but with additional context:

   ```
   You are revising an existing Dark Tower user story plan.

   ## Step 0: Load Knowledge (MANDATORY)

   **Before doing ANY other work**, read ALL `.md` files from `docs/specialist-knowledge/{your-specialist-name}/` to load your accumulated knowledge. Do NOT skip this step.

   ## Planning Protocol

   {contents of .claude/skills/user-story/planning-protocol.md}

   ## Existing Story

   {full contents of the existing story file}

   ## User Feedback

   {the user's feedback}

   ## Your Task

   1. Review the user's feedback against your domain's contribution
   2. Update your design contribution if affected
   3. Update your proposed devloop tasks if affected
   4. If the feedback doesn't affect your domain, confirm "No changes needed for {your-name}"

   Use SendMessage to tell @team-lead your updated contribution (or confirmation of no changes).
   ```

4. **Skip architecture validation**: The architecture was already validated. If feedback implies architectural changes, the lead should recommend a `/debate` instead of continuing.
5. **Specialists update contributions**: Each specialist assesses whether the feedback affects their domain and updates accordingly. Specialists coordinate with each other if interface changes ripple.
6. **Lead reassembles plan**: Collect updates, reorder tasks if dependencies changed, recheck requirements and aspect coverage.
7. **Update story file**: Revise affected sections in place. Record what changed in the Revision section.

### Continue Limits

| Phase | Limit | Action |
|-------|-------|--------|
| Revision rounds | 3 per story | Escalate ("is this story well-scoped?") |
| Design updates | 2 rounds per revision | Finalize with current state |
| Total revision time | 30 minutes | Escalate to user |

### When to Use Continue vs. New Story

- **Feedback on assumptions or design details** → `--continue` (refine the existing plan)
- **Fundamentally different requirements** → New `/user-story` (the goal changed)
- **Feedback that implies architectural changes** → `/debate` first, then new `/user-story`

## Limits

| Phase | Limit | Action on Exceeded |
|-------|-------|-------------------|
| Architecture check | 10 minutes | Escalate to user |
| Design rounds | 3 rounds | Escalate (likely needs debate) |
| Clarification wait | 15 minutes | Proceed with assumptions |
| Total skill runtime | 1 hour | Escalate to user |

## Output

- **Story file**: `docs/user-stories/YYYY-MM-DD-{slug}.md`
- **No code changes** — this skill produces a plan only
- **No ADR** — architecture is assumed to be settled; if not, the skill fails

## Notes

- This skill produces a plan only — implementation happens via subsequent `/devloop` invocations
- Lead coordinates but does not design — specialists own their domain contributions and task proposals
- Specialists communicate with each other for interface resolution
- The story file is the single source of truth for the feature's implementation status
- Devloop tasks in the story should be checked off as they complete
- If a devloop reveals that the story's design was wrong, update the story file and re-plan remaining tasks
