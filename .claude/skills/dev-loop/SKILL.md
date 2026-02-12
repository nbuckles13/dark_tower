---
name: dev-loop
description: Single-command implementation workflow using Agent Teams. Autonomous teammates handle planning, implementation, and review with minimal Lead involvement.
---

# Dev-Loop (Agent Teams)

A unified implementation workflow where autonomous teammates drive the process. The Lead only intervenes at gates (plan approval, validation, final approval).

## When to Use

Use `/dev-loop` for:
- Any implementation task
- Bug fixes
- Refactoring
- Feature additions

For design decisions needing consensus first, use `/debate` to create an ADR, then use this for implementation.

## Arguments

```
/dev-loop "task description" --specialist={name}
/dev-loop "task description"  # auto-detect specialist
```

- **task description**: What to implement (required)
- **--specialist**: Implementing specialist (optional, auto-detected from task)

## Team Composition

Every dev-loop spawns **7 teammates** (Lead + Implementer + 6 reviewers):

| Role | Specialist | Purpose | Blocking |
|------|------------|---------|----------|
| Implementer | Specified or auto-detected | Does the work | N/A |
| Security Reviewer | security | Vulnerabilities, crypto, auth | Yes (all findings) |
| Test Reviewer | test | Coverage, test quality, regression | Yes (all findings) |
| Observability Reviewer | observability | Metrics, logging, tracing, PII, SLOs | Yes (BLOCKER/HIGH); advisory (MEDIUM/LOW) |
| Code Quality Reviewer | code-reviewer | Rust idioms, ADR compliance | Yes (all findings) |
| DRY Reviewer | dry-reviewer | Cross-service duplication | BLOCKER only; TECH_DEBT documented (per ADR-0019) |
| Operations Reviewer | operations | Deployment safety, rollback, runbooks | Yes (all findings) |

**Conditional domain reviewer**: When the task touches database patterns (`migration|schema|sql`) but the implementer is NOT the Database specialist, add Database as a conditional 8th reviewer. Same for Protocol when API contracts are affected by a non-Protocol implementer.

## Workflow Overview

```
Lead (minimal involvement)
│
├── SETUP
│   ├── Create output directory
│   ├── Record git state: `git rev-parse HEAD` in main.md
│   ├── Spawn 7 teammates with composed prompts
│   └── Send task to implementer
│
├── PLANNING (Implementer + Reviewers collaborate)
│   ├── Implementer drafts approach
│   ├── Reviewers provide input directly
│   └── All reviewers confirm → GATE 1
│
├── GATE 1: PLAN APPROVAL (Lead)
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
│   ├── Reviewers examine code (scoped via git diff)
│   ├── Discuss findings with implementer
│   └── Send verdicts to Lead
│
├── GATE 3: FINAL APPROVAL (Lead)
│   └── Check all verdicts APPROVED
│
├── REFLECTION (All teammates)
│   └── Each captures learnings in knowledge files
│
└── COMPLETE
    └── Lead writes summary, documents rollback procedure
```

## Instructions

### Step 1: Parse Arguments

Extract:
- Task description
- Specialist (if provided, else detect from keywords)

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

### Step 2: Create Output Directory

```bash
mkdir -p docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}
```

Create `main.md` (see `docs/dev-loop-outputs/_template/main.md` for the full template). Key fields to populate at setup:

- **Loop Metadata**: Record `git rev-parse HEAD` as Start Commit and current branch
- **Loop State**: All 7 reviewers (including Observability) set to `pending`
- **Phase**: `setup`

For security-critical implementations, the implementer should maintain a "Security Decisions" table in main.md:

```markdown
| Decision | Choice | Rationale | ADR Reference |
|----------|--------|-----------|---------------|
| RNG source | SystemRandom | CSPRNG required | ADR-0002 |
```

### Step 3: Compose Teammate Prompts

**For Implementer**, compose:
1. Specialist identity: `.claude/agent-teams/specialists/{name}.md`
2. Dynamic knowledge: Read ALL `.md` files from `docs/specialist-knowledge/{name}/` (not just patterns/gotchas/integration - include any domain-specific files)
3. Task context

**Implementer prompt**:

```
You are implementing a feature for Dark Tower.

## Your Identity

{contents of specialists/{name}.md}

## Your Task

{task description}

{detailed requirements}

## Your Workflow

1. PLANNING: Draft your approach, message reviewers for input
2. Once all reviewers confirm, proceed to implementation
3. IMPLEMENTATION: Do the work, message reviewers if questions arise
4. When done, message Lead: "Ready for validation"
5. REVIEW: Respond to reviewer feedback, fix issues
6. REFLECTION: Document learnings when complete

## Communication

- Message reviewers directly with your plan and questions
- CC Lead only for phase transitions ("Ready for validation", etc.)
- Discuss review findings with reviewers directly

## Dynamic Knowledge

{injected knowledge files}
```

**For Reviewers**, compose:
1. Specialist identity: `.claude/agent-teams/specialists/{name}.md`
2. Review protocol: `.claude/agent-teams/protocols/review.md`
3. Dynamic knowledge: Read ALL `.md` files from `docs/specialist-knowledge/{name}/` (not just patterns/gotchas/integration - include any domain-specific files)

**Reviewer prompt**:

```
You are a reviewer in a Dark Tower dev-loop.

## Your Identity

{contents of specialists/{name}.md}

## Review Protocol

{contents of protocols/review.md}

## Your Workflow

1. PLANNING: Review implementer's approach, provide input
2. When satisfied with plan, message Lead: "Plan confirmed"
3. REVIEW: When validation passes, examine the code
4. Discuss findings with implementer
5. Send verdict to Lead: "APPROVED" or "BLOCKED: {reason}"
6. REFLECTION: Document learnings when complete

## Communication

- Message implementer directly with feedback
- Message other reviewers if you spot issues in their domain
- CC Lead for confirmations and verdicts

## Dynamic Knowledge

{injected knowledge files}
```

### Step 4: Spawn Team

**IMPORTANT**: Requires Agent Teams enabled.

Spawn all 7 teammates (+ conditional domain reviewer if applicable):
- Lead enables delegate mode (cannot implement directly)
- Each teammate gets their composed prompt

Send initial message to implementer:
```
Task: {task description}

Requirements:
{detailed requirements}

Team:
- Security Reviewer: @security
- Test Reviewer: @test
- Observability Reviewer: @observability
- Code Quality: @code-reviewer
- DRY Reviewer: @dry-reviewer
- Operations: @operations

Start by drafting your approach and getting reviewer input.
```

Update main.md: Phase = planning

### Step 5: Gate 1 - Plan Approval

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
| 4. Tests | `cargo test --workspace` | Regressions; report P0 security test count |
| 5. Clippy | `cargo clippy --workspace --lib --bins -- -D warnings` | Lint warnings |
| 6. Audit | `cargo audit` | Known dependency vulnerabilities |

**REPORTED** (tracked in main.md, not blocking):

| Layer | Command | Purpose |
|-------|---------|---------|
| 7. Coverage | `cargo llvm-cov --workspace` | Coverage vs thresholds; flag security-critical files < 95% |

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
- Message reviewers: "Validation passed. Please review the changes."

**If fail**:
- Send failure details to implementer
- Increment iteration count
- Max 3 attempts before escalation

### Step 7: Gate 3 - Final Approval

Wait for all reviewer verdicts.

Track verdicts in main.md:
```
| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | APPROVED / BLOCKED | {reason if blocked} |
| Test | APPROVED / BLOCKED | |
| Observability | APPROVED / BLOCKED | |
| Code Quality | APPROVED / BLOCKED | |
| DRY | APPROVED / BLOCKED | |
| Operations | APPROVED / BLOCKED | |
```

**If any BLOCKED**:
- Route blockers to implementer
- Return to implementation phase
- Max 3 review→implementation iterations

**If all APPROVED**:
- Update main.md: Phase = reflection
- Message team: "All approved. Please capture reflections."

### Step 8: Reflection

Allow 15 minutes for teammates to document learnings.

Each teammate updates their knowledge directory at `docs/specialist-knowledge/{name}/`. Teammates can create or update any `.md` files in their directory - common files include `patterns.md`, `gotchas.md`, and `integration.md`, but specialists may also maintain domain-specific files (e.g., `approved-crypto.md`, `coverage-targets.md`, `common-patterns.md`).

### Step 9: Complete

Update main.md:
- Phase = complete
- Duration
- Final summary
- Tech debt section (from DRY reviewer)

Report to user:
```
**Dev-Loop Complete**

Task: {task description}
Duration: {time}
Iterations: {count}

Results:
- Security: APPROVED
- Test: APPROVED
- Observability: APPROVED
- Code Quality: APPROVED
- DRY: APPROVED ({N} tech debt items documented)
- Operations: APPROVED

Files changed:
{summary}

**Next steps**:
- Review changes: `git diff`
- Commit when ready: `git add . && git commit`
```

## Limits

| Phase | Limit | Action |
|-------|-------|--------|
| Planning | 30 min / 3 rounds | Escalate |
| Implementation | Checkpoint every 30 min | Lead checks progress |
| Validation | 3 attempts | Escalate |
| Review→Impl loop | 3 iterations | Escalate |
| Reflection | 15 min | Proceed without |

## Checkpoints

main.md is updated at each phase for recovery:
- After planning: decisions captured
- After implementation: summary written
- After validation: results recorded
- After review: verdicts documented
- After reflection: learnings captured

If session interrupted, use `/dev-loop-restore` to resume.

## Files

- **Output**: `docs/dev-loop-outputs/YYYY-MM-DD-{slug}/main.md`
- **Review files**: `docs/dev-loop-outputs/YYYY-MM-DD-{slug}/{reviewer}.md`
- **Knowledge updates**: `docs/specialist-knowledge/{name}/*.md`
