---
name: devloop
description: Single-command implementation workflow using Agent Teams. Autonomous teammates handle planning, implementation, and review with minimal Lead involvement.
---

# Dev-Loop (Agent Teams)

A unified implementation workflow where autonomous teammates drive the process. The Lead only intervenes at gates (plan approval, validation, final approval).

## When to Use

Use `/devloop` for:
- Any implementation task
- Bug fixes
- Refactoring
- Feature additions

For design decisions needing consensus first, use `/debate` to create an ADR, then use this for implementation.

**All specialist implementation work MUST go through `/devloop`**. Never manually spawn a specialist via the Task tool — use `/devloop` (full) or `/devloop --light` to ensure consistent identity and navigation context.

## Arguments

```
/devloop "task description"                                        # new, full, auto-detect specialist
/devloop "task description" --specialist={name}                    # new, full, explicit specialist
/devloop "task description" --light                                # new, light (3 teammates)
/devloop "feedback" --continue=YYYY-MM-DD-slug                     # reopen completed loop, full
/devloop "feedback" --continue=YYYY-MM-DD-slug --light             # reopen completed loop, light
```

- **task description**: What to implement (required)
- **--specialist**: Implementing specialist (optional, auto-detected from task)
- **--light**: Lightweight mode — 3 teammates, skip planning gate and reflection (see Lightweight Mode)
- **--continue**: Reopen a completed devloop to address human review feedback (see Continue Mode)

## Team Composition

### Full Mode (default)

Every devloop spawns **7 teammates** (Lead + Implementer + 6 reviewers):

| Role | Specialist | Purpose |
|------|------------|---------|
| Implementer | Specified or auto-detected | Does the work |
| Security Reviewer | security | Vulnerabilities, crypto, auth |
| Test Reviewer | test | Coverage, test quality, regression |
| Observability Reviewer | observability | Metrics, logging, tracing, PII, SLOs |
| Code Quality Reviewer | code-reviewer | Rust idioms, ADR compliance |
| DRY Reviewer | dry-reviewer | Cross-service duplication (see DRY exception in review protocol) |
| Operations Reviewer | operations | Deployment safety, rollback, runbooks |

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

## Workflow Overview

```
Lead (minimal involvement)
│
├── SETUP
│   ├── Create output directory
│   ├── Record git state: `git rev-parse HEAD` in main.md
│   ├── Spawn teammates via subagent_type (identity auto-loaded)
│   └── Send task to implementer
│
├── PLANNING (Implementer + Reviewers collaborate) [SKIPPED in --light]
│   ├── Implementer drafts approach
│   ├── Reviewers provide input directly
│   └── All reviewers confirm → GATE 1
│
├── GATE 1: PLAN APPROVAL (Lead) [SKIPPED in --light]
│   └── Check all reviewers confirmed (see Plan Confirmation Checklist in review protocol)
│
├── IMPLEMENTATION (Implementer drives)
│   ├── Implementer does the work
│   └── Ready → request validation
│
├── GATE 2: VALIDATION (Lead)
│   └── Run validation pipeline (see below)
│
├── REVIEW (Reviewers + Implementer collaborate)
│   ├── Reviewers examine code, send findings to implementer
│   ├── Implementer fixes findings or defers with justification
│   ├── Reviewers accept deferrals or escalate to Lead
│   └── Send verdicts to Lead (CLEAR / RESOLVED / ESCALATED)
│
├── GATE 3: FINAL APPROVAL (Lead)
│   └── Check all verdicts CLEAR or RESOLVED; resolve any ESCALATED
│
├── REFLECTION (All teammates) [SKIPPED in --light]
│   └── Each captures learnings in knowledge files
│
└── COMPLETE
    └── Lead writes summary, invites human review feedback
```

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

**IMPORTANT**: All teammates are spawned using the `subagent_type` parameter in the Task tool, which auto-loads their identity from `.claude/agents/{name}.md`. Do NOT manually read or inject specialist identity files — the agent system handles this.

**INDEX injection**: Before spawning each teammate, read `docs/specialist-knowledge/{name}/INDEX.md` and include its contents in the teammate's prompt under a `## Navigation` header. This gives each specialist a navigation map to relevant code and ADRs.

**Rule 4**: Give the implementer the big-picture task. Let them decide how to break it down — don't micro-manage subtask decomposition.

**Naming convention**: Use the `name` parameter in the Task tool to set each teammate's SendMessage recipient name. These names MUST match the `@` references used in teammate prompts:

| Role | `name` | `subagent_type` |
|------|--------|-----------------|
| Implementer | `implementer` | `{specialist-name}` (e.g., `global-controller`) |
| Security Reviewer | `security` | `security` |
| Test Reviewer | `test` | `test` |
| Observability Reviewer | `observability` | `observability` |
| Code Quality Reviewer | `code-reviewer` | `code-reviewer` |
| DRY Reviewer | `dry-reviewer` | `dry-reviewer` |
| Operations Reviewer | `operations` | `operations` |

The Lead (orchestrator) is automatically named `team-lead` in the team config.

**For Implementer**, spawn with `name: "implementer"`, `subagent_type: "{specialist-name}"` and this prompt:

```
You are implementing a feature for Dark Tower.

## Navigation

{contents of docs/specialist-knowledge/{specialist-name}/INDEX.md}

## Your Task

{task description}

{detailed requirements}

## Your Workflow

1. PLANNING: Draft your approach, use SendMessage to share your plan with reviewers for input
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

**CRITICAL**: Teammates go idle after every turn — this is normal. An idle notification does NOT mean the teammate has finished their task. Teammates may go idle because:
- They sent a message and are waiting for a response
- The user sent them a message, they responded, and their turn ended
- They are waiting for input from another teammate

**Only treat a task as complete when the teammate explicitly signals completion** (e.g., implementer sends "Ready for validation", reviewer sends their verdict). Never advance the workflow based solely on an idle notification.

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
| 2. Format | `cargo fmt --all -- --check` | Style violations |
| 3. Guards | `./scripts/guards/run-guards.sh` | Credential leaks, PII, instrument-skip-all, test-coverage, api-version-check |
| 4. Tests | `./scripts/test.sh --workspace` | Regressions; ensures DB setup + migrations; report P0 security test count |
| 5. Clippy | `cargo clippy --workspace --lib --bins -- -D warnings` | Lint warnings |
| 6. Audit | `cargo audit` | Known dependency vulnerabilities |
| 7. Semantic | Spawn `semantic-guard` agent (see below) | AI-powered diff analysis: credential leaks, actor blocking, error context |

**Layer 7 — Semantic Guard Agent**:

After layers 1-6 pass, spawn the semantic-guard agent to analyze the diff:

```
name: "semantic-guard"
subagent_type: "semantic-guard"
prompt: "Analyze the current diff for semantic issues. Report your verdict to @team-lead."
```

Wait for the agent's verdict message. If UNSAFE, treat as a validation failure (send findings to implementer, increment iteration). If SAFE, proceed.

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
- Message reviewers: "Start Review. Validation passed — please examine the changes and send your verdict."

**If fail**:
- Send failure details to implementer
- Increment iteration count
- Max 3 attempts before escalation

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
- Full mode: Message team: "All approved. Please capture reflections."
- Light mode: Skip to Step 9.

### Step 8: Reflection [FULL MODE ONLY]

Allow 15 minutes for teammates to update their INDEX.md navigation file.

Each specialist maintains ONE file: `docs/specialist-knowledge/{name}/INDEX.md`

**INDEX.md is a navigation map — pointers to code and ADRs, not content.**

Each entry is a pointer: "Topic → `path/to/file.rs:function_name()`" or "Topic → ADR-NNNN (Section X)"

Pointers should be as specific as possible:
- Code: point to the function, not just the file (`jwt.rs:validate_token()` not just `jwt.rs`)
- ADRs: point to the relevant section when the topic is specific ("crypto requirements → ADR-0003 Section 2"), or the whole ADR when the topic is broad ("overall architecture → ADR-0001")

**Add a pointer when**: New ADR created that affects your domain. Significant new functionality added. New integration seam discovered.
**Update a pointer when**: Code moved or renamed. ADR superseded.
**Remove a pointer when**: Referenced code deleted. ADR deprecated.

**INDEX.md MUST NOT contain:**
- Implementation facts (how functions work, what parameters they take, why a pattern was chosen)
- Task status, deferred items, or date-stamped feature sections
- Design decisions or gotchas (document these as code comments at the decision point)
- General Rust/library/database knowledge (e.g., "modulo bias requires rejection sampling")
- Duplication findings (DRY reviewer: add these to `.claude/TODO.md`, not your INDEX)

**No exceptions.** If something feels important but isn't a pointer, it belongs as a code comment, an ADR, or a TODO.md entry — not in INDEX.md.

**Organize by architectural concept** (e.g., "Code Locations", "Integration Seams"), not by feature or date.

**Size cap**: 50 lines maximum.

### Step 9: Complete

Update main.md:
- Phase = complete
- Duration
- Final summary
- Tech debt section (all accepted deferrals with justifications, plus DRY extraction opportunities)

Write `.devloop-pr.json` to the worktree root with PR metadata for the host wrapper script:
```json
{
  "title": "Short PR title (under 70 chars)",
  "body": "## Summary\n- bullet points\n\n## Review\n- reviewer verdicts\n\n## Test plan\n- [ ] verification steps\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)"
}
```

The `title` should summarize the change. The `body` should include the task summary, reviewer verdicts, files changed, and a test plan. This file is read by the host-side `devloop.sh` wrapper to create the GitHub PR with proper context. See ADR-0025.

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

PR metadata written to .devloop-pr.json

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

## Your Workflow

1. PLANNING: Draft your approach, use SendMessage to share your plan with reviewers for input
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

## Limits

| Phase | Limit | Action |
|-------|-------|--------|
| Planning | 30 min / 3 rounds | Escalate |
| Implementation | No limit | Lead monitors progress |
| Validation | 3 attempts | Escalate |
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
