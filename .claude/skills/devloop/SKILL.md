---
name: devloop
description: Single-command implementation workflow using Agent Teams. Autonomous teammates handle planning, implementation, and review with minimal Lead involvement.
---

# Dev-Loop (Agent Teams)

A unified implementation workflow where autonomous teammates drive the process. The Lead only intervenes at gates (plan approval, validation, final approval).

## When to Use

Any implementation work: bug fixes, refactors, new features. For design decisions needing consensus first, use `/debate` to create an ADR, then use this for implementation.

**All specialist implementation work MUST go through `/devloop`**. Never manually spawn a specialist via the Task tool — use `/devloop` (full) or `/devloop --light` to ensure consistent identity and navigation context.

## Arguments

```
/devloop "task description"                                        # new, full, auto-detect specialist
/devloop "task description" --specialist={name}                    # new, full, explicit specialist
/devloop "task description" --light                                # new, light (3 teammates)
/devloop "task description" --paired-with=<specialist>             # overlay: co-implementer collaborator
/devloop "feedback" --continue=YYYY-MM-DD-slug                     # reopen completed loop, full
/devloop "feedback" --continue=YYYY-MM-DD-slug --light             # reopen completed loop, light
```

- **task description**: What to implement (required)
- **--specialist**: Implementing specialist (optional, auto-detected from task)
- **--light**: Lightweight mode — 3 teammates, skip planning gate and reflection (see Lightweight Mode)
- **--paired-with=\<specialist\>**: Overlay flag — the named specialist actively collaborates during implementation and is an explicit reviewer at Gate 2. Composes with `--light`/full; does not replace routing. Recommended for first-of-N exemplar rollouts (N=1); for N≥4 affected services, use one paired exemplar + remaining-services-as-mechanical-sweep. **Does not exempt Guarded Shared Areas from owner-implements routing** (see §Cross-Boundary Edits below and ADR-0024 §6.5).
- **--continue**: Reopen a completed devloop to address human review feedback (see Continue Mode)

## Team Composition

### Full Mode (default)

Every devloop spawns **7 teammates** (Lead + Implementer + 6 reviewers). `name` is the SendMessage recipient; `subagent_type` loads identity from `.claude/agents/{name}.md`.

| Role | `name` | `subagent_type` | Purpose |
|------|--------|-----------------|---------|
| Implementer | `implementer` | `{specialist}` | Does the work |
| Security Reviewer | `security` | `security` | Vulnerabilities, crypto, auth |
| Test Reviewer | `test` | `test` | Coverage, test quality, regression |
| Observability Reviewer | `observability` | `observability` | Metrics, logging, tracing, PII, SLOs |
| Code Quality Reviewer | `code-reviewer` | `code-reviewer` | Rust idioms, ADR compliance |
| DRY Reviewer | `dry-reviewer` | `dry-reviewer` | Cross-service duplication (see DRY exception in review protocol) |
| Operations Reviewer | `operations` | `operations` | Deployment safety, rollback, runbooks |
| Paired Specialist (if `--paired-with=<specialist>`) | `paired-<specialist>` | `{specialist}` | Active collaborator during implementation + Gate 2 reviewer. When `<specialist>` is already a mandatory reviewer (security/test/observability/operations), the paired teammate replaces that slot with the same identity and an expanded role. |

The Lead (orchestrator) is automatically named `team-lead` in the team config.

**Review model**: All findings default to "fix it." Implementer may defer with justification. See review protocol for details.

**Conditional domain reviewer**: When the task touches database patterns (`migration|schema|sql`) but the implementer is NOT the Database specialist, add Database as a conditional 8th reviewer. Same for Protocol when API contracts are affected by a non-Protocol implementer.

### Lightweight Mode (`--light`)

For small, contained changes (typically 10-30 lines):

| Role | Specialist | Purpose |
|------|------------|---------|
| Implementer | Specified or auto-detected | Does the work |
| Security Reviewer | security | Always present |
| Context Reviewer | Lead chooses | One reviewer based on change type |

**Third reviewer selection** (Lead decides):
- Code Quality — for style/idiom changes
- Observability — for metrics/tracing changes
- Test — for test changes
- Operations — for deployment/config changes
- DRY — for shared code changes

**Skips**: Gate 1 (plan approval), reflection phase
**Keeps**: Full validation pipeline (Gate 2), review verdicts

**Not eligible** (must use full mode):
- Changes touching auth, crypto, session paths, security-critical code
- Schema/migration changes
- Protocol changes
- Deployment manifests (K8s, Docker)
- `Cargo.toml` dependency changes
- `crates/common/` (affects all services)
- Instrumentation code (`tracing::`, `metrics::`, `#[instrument]`)

**Escalation**: Any reviewer can request upgrade to full devloop.
**Ambiguity rule**: When in doubt, use full mode.

## Cross-Boundary Edits

When an implementer's plan touches files owned by another specialist, **the implementer self-classifies** each such edit in the devloop brief using the scheme below. Reviewers may **upgrade** a classification at Gate 1 or Gate 2 (downgrade disallowed); challenges auto-route to ESCALATE. Route per the Owner Involvement table (§6.3). See the ADR for rationale; this section covers operational triggers.

### Three-Category Classification (§6.2)

- **Mechanical** — Value-neutral *and* structure-preserving (the `sed`-test applies: deterministic find-and-replace that does not change the encoded concept). Requires full guard pipeline coverage for the change-pattern: **Mechanical iff guards catch every partial version** (see `./scripts/guards/run-guards.sh`). Concept substitution (renaming a metric label while changing its semantic meaning) is NOT Mechanical.
- **Minor-judgment** — Small defensive adjustments where a reasonable reader could argue either way but impact is bounded. Examples: widening a numeric threshold, adding a missing structured-log field. **Alert rule changes** (severity, routing labels, `for:` duration) are Minor-judgment when the edit couples to runbook prose (`docs/runbooks/*.md`) or the alert conventions doc (`docs/observability/alert-conventions.md`) — hunk-ACK by operations is required because the runbook narrative must stay coherent with the fired-state semantics.
- **Domain-judgment** — Changes requiring the owner's domain knowledge (threshold tuning, behavior changes, API semantics, new instrumentation affecting SLO shape).

Use ADR-0019 Pattern A/B/C vocabulary for duplication/rename patterns; Pattern B coordinated renames require a **named convention author** (e.g., observability for metric taxonomy) — absent one, Pattern B collapses to owner-implements.

### Owner Involvement (§6.3)

| Category | Owner involvement | Mechanism |
|----------|-------------------|-----------|
| Mechanical | Review-only | Owner sees the change at the standard reviewer gate. No separate approval — **proceed with review**. |
| Minor-judgment | Hunk-ACK required | Owner-specialist must explicitly ACK the specific cross-boundary hunk via a commit trailer (below). PR-level review insufficient. |
| Domain-judgment | Owner-implements | Route to a separate devloop with owner as implementer, or use `--paired-with=<owner>` to keep the owner in the loop during the current devloop. |

**Default-posture flip**: For Mechanical cross-boundary edits the default is "proceed with review," NOT "defer to owner." The older implicit "owner-implements" rule holds only for Domain-judgment and Guarded Shared Areas. **This flip does NOT apply inside Guarded Shared Areas — Mechanical classification is disallowed there.**

### Guarded Shared Areas (§6.4)

Certain surfaces override the category classification: even a sed-clean edit routes to the owner-specialist. **Mechanical is disallowed inside GSA; Minor-judgment requires owner hunk-ACK.**

**Criterion** (names the test, not just the list): wire-format runtime coupling, OR auth-routing policy, OR detection/forensics contract, OR schema evolution. Paths matching the criterion are Guarded whether or not enumerated below.

<!-- Mirror of ADR-0024 §6.4 enumerated list. Update all three locations together when extending via micro-debate. -->

- `proto/**`, `proto-gen/**`, `build.rs` — wire format
- `crates/media-protocol/**` — SFU protocol semantics (protocol + MH co-sign)
- `crates/common/src/jwt.rs`, `meeting_token.rs`, `token_manager.rs`, `secret.rs` — auth/crypto primitives
- `crates/common/src/webtransport/**` — wire-runtime coupling
- `crates/ac-service/src/jwks/**`, `src/token/**`, `src/crypto/**` — crypto primitives
- `crates/ac-service/src/audit/**` — detection/forensics contract
- `db/migrations/**` — schema evolution
- ADR-0027-approved crypto primitives (wherever referenced) — path-independent; guards a concept, not a directory. New call sites of enumerated primitives inherit Guarded status regardless of the containing file.

**Intersection rule**: edits spanning two GSA (e.g., auth-routing fields in `proto/internal.proto` crossing wire-format × auth-routing-policy) require all affected owners co-sign: `Approved-Cross-Boundary: protocol`, `Approved-Cross-Boundary: auth-controller`, `Approved-Cross-Boundary: security` (ADR-0003 §5.7).

**`crates/common/**` outside the Guarded subset** is not owned by a single specialist. Edits require DRY reviewer + code-reviewer approval; affected-specialist involvement is review-only unless call-site semantics change (escalates to Minor-judgment hunk-approval).

Extending the enumerated list requires a **micro-debate** (~3 specialists: affected owner + security + one cross-cutting), not a new ADR. Counter-intuitive property: the rule is *stricter* inside GSA, not looser.

### `Approved-Cross-Boundary:` Commit Trailer (§6.7)

Hunk-ACK is recorded as a git commit trailer:

```
Approved-Cross-Boundary: <specialist-name> <reason ≥ 10 chars>
```

RFC-5322 style, parseable by `git interpret-trailers`. Multiple trailers allowed on a single commit (one per approving specialist). Reason-clauses should name the authority (e.g., "label-taxonomy rename matches ADR-0011 canonical"), not just the what.

**Enforcement**: Gate 2 validation will include `validate-cross-boundary-approval.sh` (scans commit trailers to enforce §6.3 hunk-ACK requirements). **Pending implementation** — tracked as ADR-0024 §6.8 follow-up #1 (operations + test + security). Until the guard lands, trailer presence is a manual verification step at Gate 3 via the Ownership-lens verdict field (see review-protocol.md).

## Workflow Overview

```
SETUP → PLANNING [skipped --light] → GATE 1 [skipped --light] →
IMPLEMENTATION → GATE 2 (VALIDATION) → REVIEW → GATE 3 (FINAL APPROVAL) →
REFLECTION [skipped --light] → COMPLETE
```

Lead has minimal involvement — acts only at the three gates. Teammates drive Planning, Implementation, and Review directly. See Instructions below for each step.

## Instructions

### Step 1: Parse Arguments

Extract:
- Task description
- Specialist (if provided, else detect from keywords)
- Mode flags: `--light`, `--continue`

**Auto-detection patterns**:
| Pattern | Specialist |
|---------|------------|
| `auth\|jwt\|token\|oauth\|credential\|key\|rotation\|jwks\|federation\|bcrypt\|password` | auth-controller |
| `meeting\|session\|signaling\|participant\|layout\|roster\|ice\|dtls` | meeting-controller |
| `media\|video\|audio\|stream\|sfu\|simulcast\|bandwidth\|codec\|datagram` | media-handler |
| `api\|endpoint\|route\|http\|gateway\|http3\|webtransport\|tenant\|geographic` | global-controller |
| `database\|migration\|schema\|sql\|index\|query\|sqlx\|postgres\|redis` | database |
| `proto\|protobuf\|contract\|wire\|signaling\|message.format\|grpc` | protocol |
| `test\|coverage\|fuzz` | test |
| `metric\|trace\|log\|observability` | observability |
| `deploy\|k8s\|infra\|terraform\|docker\|kubernetes\|helm\|ci\|cd\|pipeline\|github.actions` | infrastructure |

**Disambiguation**: When a task matches multiple specialist patterns, the more specific match takes precedence. If ambiguity remains, Lead prompts the user to choose. Example: "fix meeting assignment load balancing" matches both `meeting` (MC) and `assignment` (GC) — Lead asks user.

**If `--continue` is specified**: See Continue Mode section below. Skip to that workflow.

**If `--light` is specified**: Validate eligibility against the exclusion list. If not eligible, inform user and run full mode instead.

### Step 2: Create Output Directory

```bash
mkdir -p docs/devloop-outputs/YYYY-MM-DD-{task-slug}
```

Create `main.md` (see `docs/devloop-outputs/_template/main.md` for the full template). Key fields to populate at setup:

- **Loop Metadata**: Record `git rev-parse HEAD` as Start Commit and current branch
- **Loop State**: All reviewers set to `pending`
- **Phase**: `setup`
- **Mode**: `full` or `light`

For security-critical implementations, the implementer should maintain a "Security Decisions" table in main.md:

```markdown
| Decision | Choice | Rationale | ADR Reference |
|----------|--------|-----------|---------------|
| RNG source | SystemRandom | CSPRNG required | ADR-0002 |
```

### Step 3: Spawn Teammates

**Defensive cleanup**: Before creating a new team, check for and clean up any stale team from a previous devloop. If a team already exists, send shutdown requests to all teammates and call `TeamDelete`. This handles cases where a previous devloop was interrupted before Step 8.9.

**IMPORTANT**: All teammates are spawned using the `subagent_type` parameter in the Task tool, which auto-loads their identity from `.claude/agents/{name}.md`. Do NOT manually read or inject specialist identity files — the agent system handles this.

**INDEX injection**: Before spawning each teammate, read `docs/specialist-knowledge/{name}/INDEX.md` and include its contents in the teammate's prompt under a `## Navigation` header. This gives each specialist a navigation map to relevant code and ADRs.

**Rule 4**: Give the implementer the big-picture task. Let them decide how to break it down — don't micro-manage subtask decomposition.

Use `name`/`subagent_type` per the Teammate Roster table in §Team Composition. `name` MUST match the `@` references used in teammate prompts.

**For Implementer**, spawn with `name: "implementer"`, `subagent_type: "{specialist-name}"` and this prompt:

```
You are implementing a feature for Dark Tower.

## Navigation

{contents of docs/specialist-knowledge/{specialist-name}/INDEX.md}

## Your Task

{task description}

{detailed requirements}

## Your Workflow

1. PLANNING: Draft your approach, use SendMessage to share your plan with reviewers for input. If your plan includes cross-boundary edits (files owned by another specialist), self-classify each per §Cross-Boundary Edits (Mechanical / Minor-judgment / Domain-judgment) in the plan.
2. **WAIT for @team-lead to send you "Plan approved" before implementing.** Individual reviewer confirmations are not sufficient — @team-lead is the gatekeeper.
3. IMPLEMENTATION: Do the work, use SendMessage to ask reviewers if questions arise
4. When done, use SendMessage to tell @team-lead: "Ready for validation"
5. REVIEW: Respond to reviewer findings — fix each one or defer with justification (see review protocol for valid/invalid justifications)
6. REFLECTION: Document learnings when complete

## Communication

All teammate communication MUST use the SendMessage tool. Plain text output is not visible to other teammates.

- Use SendMessage to message reviewers directly with your plan and questions
- Use SendMessage to tell @team-lead for phase transitions ("Ready for validation", etc.)
- Use SendMessage to discuss review findings with reviewers directly
- **Do NOT start implementing until @team-lead sends you "Plan approved"**
```

**For Reviewers**, spawn with `name: "{reviewer-name}"`, `subagent_type: "{reviewer-name}"` and this prompt:

```
You are a reviewer in a Dark Tower devloop.

## Navigation

{contents of docs/specialist-knowledge/{reviewer-name}/INDEX.md}

## Review Protocol

{contents of .claude/skills/devloop/review-protocol.md}

## Your Workflow

1. PLANNING: Review implementer's approach, provide input
2. When satisfied with plan, use SendMessage to tell @team-lead: "Plan confirmed"
3. **WAIT for @team-lead to send you "Start Review" before examining code.** Do NOT review code during planning or implementation phases.
4. REVIEW: Examine the code, send findings to @implementer. Each finding defaults to "fix it."
5. TRIAGE: If implementer defers a finding with justification, accept or escalate per review protocol.
6. Use SendMessage to tell @team-lead your verdict: "CLEAR", "RESOLVED", or "ESCALATED: {reason}"
7. REFLECTION: Document learnings when complete

## Communication

All teammate communication MUST use the SendMessage tool. Plain text output is not visible to other teammates.

- Use SendMessage to message @implementer directly with feedback
- Use SendMessage to message other reviewers if you spot issues in their domain
- Use SendMessage to tell @team-lead for confirmations and verdicts (CLEAR / RESOLVED / ESCALATED)
- **Do NOT start reviewing code until @team-lead sends you "Start Review"**
```

**For `--light` mode**: Skip workflow steps 1-2 for the implementer (no planning gate). Implementer starts implementing immediately. Reviewer prompt omits planning steps.

### Step 4: Send Task to Implementer

Send initial message to implementer:
```
Task: {task description}

Requirements:
{detailed requirements}

Team:
- Security Reviewer: @security
- Test Reviewer: @test (full mode only)
- Observability Reviewer: @observability (full mode only)
- Code Quality: @code-reviewer (full mode only)
- DRY Reviewer: @dry-reviewer (full mode only)
- Operations: @operations (full mode only)
{list only the teammates actually spawned}

Start by drafting your approach and getting reviewer input.
```

Update main.md: Phase = planning (full) or implementation (light)

### Gate Management: Idle ≠ Done

**CRITICAL**: Teammates go idle after every turn — this does NOT mean the teammate has finished their task (they may be waiting for a response, or their turn ended after replying). Only treat a task as complete when the teammate explicitly signals completion (e.g., implementer sends "Ready for validation", reviewer sends their verdict). Never advance the workflow based solely on an idle notification.

### Step 5: Gate 1 - Plan Approval [FULL MODE ONLY]

Wait for all reviewers to confirm plan.

Track confirmations in main.md:
```
| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed / pending |
| Test | confirmed / pending |
| ... | ... |
```

**Timeout**: 30 minutes
**Max iterations**: 3 revision rounds

If exceeded, escalate:
```
**Planning Timeout**

Implementer proposed approach, but reviewers haven't all confirmed.

Status:
{who confirmed, who hasn't}

Options:
1. Give more time
2. Proceed with current confirmations
3. Adjust approach

Which would you prefer?
```

When all confirmed, update main.md: Phase = implementation

### Step 6: Gate 2 - Validation

When implementer signals "Ready for validation", run the validation pipeline:

**ENFORCED** (run in order, stop on first failure):

| Layer | Command | What It Catches |
|-------|---------|-----------------|
| 1. Compile | `cargo check --workspace` | Type errors, sqlx compile-time failures |
| 2. Format | `cargo fmt --all` | Auto-fix style violations |
| 3. Guards | `./scripts/guards/run-guards.sh` | Credential leaks, PII, instrument-skip-all, test-coverage, api-version-check |
| 4. Tests | `./scripts/test.sh --workspace` | Regressions; ensures DB setup + migrations; report P0 security test count |
| 5. Clippy | `cargo clippy --workspace -- -D warnings` | Lint warnings |
| 6. Audit | `cargo audit` | Known dependency vulnerabilities |
| 7. Semantic | Spawn `semantic-guard` agent (see below) | AI-powered diff analysis: credential leaks, actor blocking, error context |
| 8. Env-tests | `dev-cluster rebuild-all` + `cargo test -p env-tests --features all` | Integration test failures against live Kind cluster |

**Layer 7 — Semantic Guard Agent**:

After layers 1-6 pass, spawn the semantic-guard agent to analyze the diff:

```
name: "semantic-guard"
subagent_type: "semantic-guard"
prompt: "Analyze the current diff for semantic issues. Report your verdict to @team-lead."
```

Wait for the agent's verdict message. If UNSAFE, treat as a validation failure (send findings to implementer, increment iteration). If SAFE, proceed.

**Layer 8 — Env-tests (Integration)**:

After layers 1-7 pass, run integration tests against the live Kind cluster. Layer 8 always runs — intentionally broader than ADR-0030's trigger-path list, because business logic changes can break integration tests too.

1. **Cluster readiness**: Run `dev-cluster status`. If not ready, run `dev-cluster setup` (polls `setup_in_progress` first). Setup does NOT consume attempts; escalate on setup failure as infra.
2. **Infra change detection**: If `git diff --name-only ${START_COMMIT}..HEAD -- infra/kind/` shows changes, `dev-cluster teardown` then `setup` (cluster skeleton stale). Does NOT consume attempts; log triggering files.
3. **Rebuild services**: `dev-cluster rebuild-all`. Report wall-clock time.
4. **Run env-tests**: Read `/tmp/devloop/ports.json` to construct `ENV_TEST_{AC,GC,PROMETHEUS,GRAFANA,LOKI}_URL` from `.container_urls.*`. Run `timeout 600 cargo test -p env-tests --features all 2>&1 | tee /tmp/devloop/env-test-output.log`. On failure, forward full output + log path to implementer.
5. **Classify exit**: Exit 0 = pass. Non-zero: if stderr matches infra patterns (`connection refused|timed out|connection reset|broken pipe`), **infrastructure failure** (retry once, do NOT consume attempt, then escalate). Otherwise **test failure** (consume attempt).

**Layer 8 attempt budget**: 2 attempts (separate from layers 1-7's 3). Infrastructure failures do not consume attempts. First-run cluster setup (~7 min) does not count toward attempts.

**ARTIFACT-SPECIFIC** (mandatory when detected file types are in the changeset):

| Artifact | Verification | Trigger |
|----------|-------------|---------|
| `.proto` files | Proto compilation, freshness check (regenerate + diff `proto-gen/`), backward compat | `git diff --name-only` includes `proto/` |
| `migrations/` | Sequential numbering, `.sqlx/` offline data freshness, reversibility documented | `git diff --name-only` includes `migrations/` |
| K8s manifests | `kubeconform` schema validation | `git diff --name-only` includes `infra/kubernetes/` |
| Dockerfiles | `hadolint` lint | `git diff --name-only` includes `Dockerfile` |
| Shell scripts | `shellcheck` lint | `git diff --name-only` includes `*.sh` |

**If pass**:
- Update main.md: Phase = review
- Message each reviewer individually (unicast, not broadcast): "Start Review. Validation passed — please examine the changes and send your verdict."

**If fail (layers 1-7)**:
- Send failure details to implementer
- Increment iteration count
- Max 3 attempts before escalation

**If fail (layer 8)**:
- Send full env-test output (stdout + stderr) to implementer
- Increment Layer 8 iteration count (separate from layers 1-7)
- Max 2 attempts before escalation
- Infrastructure failures do not consume attempts (retry once, then escalate)

### Step 7: Gate 3 - Final Approval

Wait for all reviewer verdicts.

Track verdicts in main.md:
```
| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR / RESOLVED / ESCALATED | {count} | {count} | {count} | |
| Test | CLEAR / RESOLVED / ESCALATED | | | | |
| Observability | CLEAR / RESOLVED / ESCALATED | | | | |
| Code Quality | CLEAR / RESOLVED / ESCALATED | | | | |
| DRY | CLEAR / RESOLVED / ESCALATED | | | | |
| Operations | CLEAR / RESOLVED / ESCALATED | | | | |
```

**If any ESCALATED**:
- Lead reviews the specific finding and the implementer's deferral justification
- Lead decides: fix it (route back to implementer) or accept the deferral (override)
- If routed back: return to implementation phase, max 3 review→implementation iterations

**If all CLEAR or RESOLVED**:
- Update main.md: Phase = reflection (full) or complete (light)
- Document accepted deferrals as tech debt in main.md (with implementer's justification)
- Full mode: proceed to reflection (Step 8)
- Light mode: Skip to Step 9.

### Step 8: Reflection [FULL MODE ONLY]

Send the reflection instructions to each teammate individually (unicast, not broadcast):

```
Reflection: update your INDEX.md at `docs/specialist-knowledge/{your-name}/INDEX.md`.

INDEX.md is a navigation map — pointers to code and ADRs ONLY.

Format: "Topic → `path/to/file.rs:function_name()`" or "Topic → ADR-NNNN"

- Add pointers for new code locations, new ADRs, new integration seams
- Update pointers for moved/renamed code
- Remove pointers for deleted code

DO NOT add implementation facts, gotchas, patterns, design decisions,
review checklists, task status, or date-stamped sections. If something
feels important but isn't a pointer, put it as a code comment, an ADR,
or a TODO.md entry instead.

DRY reviewer: duplication findings go in `docs/TODO.md`, not INDEX.

Organize by architectural concept (not by feature or date). Max 75 lines.
```

Allow 15 minutes for updates.

After reflection, re-run the INDEX guard to catch any issues introduced:
```bash
./scripts/guards/simple/validate-knowledge-index.sh
```
If it fails, fix the INDEX files before proceeding.

### Step 8.5: Commit

After reflection (full mode) or after review (light mode), stage and commit:

1. `git add -A`
2. Commit with message:
   ```
   {task description}

   Devloop: {YYYY-MM-DD-slug}
   Specialist: {specialist}
   Mode: {full|light}
   Verdicts: Security {verdict}, Test {verdict}, ...

   Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
   ```
3. If nothing to commit, skip silently

### Step 8.9: Cleanup Team

Shut down all teammates and delete the team before completing:

1. Send shutdown requests to all teammates
2. Call `TeamDelete` to remove the team and task list

This prevents stale team context from leaking into subsequent devloops
when chained by story-run.

### Step 9: Complete

Update main.md:
- Phase = complete
- Duration
- Final summary
- Tech debt section (all accepted deferrals with justifications, plus DRY extraction opportunities, plus any spun-out findings with target devloop slug or "to be scheduled" per review-protocol.md)

If this task is part of a user story, update the Devloop Tracking table
in the user story file: set Status to Completed, fill in the Devloop
Output path and commit hash. Include this in the Step 8.5 commit (or
amend it).

Report to user:
```
**Dev-Loop Complete**

Task: {task description}
Duration: {time}
Iterations: {count}

Results:
- Security: CLEAR/RESOLVED ({N} findings, {M} fixed, {K} deferred)
- Test: CLEAR/RESOLVED
- Observability: CLEAR/RESOLVED
- Code Quality: CLEAR/RESOLVED
- DRY: CLEAR/RESOLVED ({N} tech debt observations)
- Operations: CLEAR/RESOLVED

Files changed:
{summary}

**To address review feedback**:
- Small fix: `/devloop --light "description" --continue=YYYY-MM-DD-{slug}`
- Larger change: `/devloop "description" --continue=YYYY-MM-DD-{slug}`
```

## Continue Mode (`--continue`)

Reopens a completed devloop to address human review feedback. All work is tracked in the same `main.md` as additional iterations.

### How It Works

1. **Parse**: Extract feedback description and devloop slug from `--continue=YYYY-MM-DD-slug`
2. **Load context**: Read `docs/devloop-outputs/{slug}/main.md` to get:
   - Original task description
   - Which specialist implemented
   - What was done
   - Previous reviewer verdicts
3. **Record feedback**: Add a "Human Review" section to main.md:
   ```markdown
   ## Human Review (Iteration {N})

   **Feedback**: "{user's feedback}"
   ```
4. **Spawn implementer**: Use the same specialist as the original devloop, via `subagent_type`. Prompt includes:
   - The original task context (from main.md)
   - The human review feedback
   - Reference to the previous implementation
5. **Determine mode**: `--light` or full is controlled by the user's flags, same rules as new devloops
6. **Run workflow**: Same gates as a new devloop (validation + review), tracked as additional iterations in the same main.md
7. **Update main.md**: Record implementation changes, validation results, and reviewer verdicts for this iteration

### Continue Prompt (Implementer)

Spawn with `name: "implementer"`, `subagent_type: "{original-specialist}"`:

```
You are continuing work on a previous devloop implementation.

## Navigation

{contents of docs/specialist-knowledge/{specialist-name}/INDEX.md}

## Original Task

{original task description from main.md}

## What Was Done

{summary of previous implementation from main.md}

## Human Review Feedback

{user's feedback}

## Your Task

Address the feedback above. The previous implementation is already in the codebase.

Follow the standard Implementer workflow + communication rules (see Step 3 Implementer prompt above).
```

## Limits

| Phase | Limit | Action |
|-------|-------|--------|
| Planning | 30 min / 3 rounds | Escalate |
| Implementation | No limit | Lead monitors progress |
| Validation (L1-7) | 3 attempts | Escalate |
| Validation (L8) | 2 attempts | Escalate |
| Infra failures (L8) | Retry once | Escalate (don't consume attempts) |
| First-run setup (L8) | ~7 min | Does not count toward attempts |
| Review→Impl loop | 3 iterations | Escalate |
| Reflection | 15 min | Proceed without |
| Human review rounds | 3 per devloop | Escalate ("is this task well-scoped?") |

## Recovery

If a session is interrupted, restart the devloop from the beginning. The main.md file records the start commit for rollback if needed.

## Files

- **Specialist definitions**: `.claude/agents/{name}.md` (auto-loaded via `subagent_type`)
- **Review protocol**: `.claude/skills/devloop/review-protocol.md`
- **Output**: `docs/devloop-outputs/YYYY-MM-DD-{slug}/main.md`
- **Navigation**: `docs/specialist-knowledge/{name}/INDEX.md`
