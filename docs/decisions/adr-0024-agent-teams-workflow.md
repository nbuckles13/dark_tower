# ADR-0024: Agent Teams Development Workflow

## Status

Accepted

## Date

2026-02-10

## Context

Dark Tower's development process evolved through four iterations:

1. **Autonomous orchestrator** (v1) — Claude drives everything. Failed: skipped steps, inconsistent execution.
2. **Step-runner architecture** (v2) — Structured pipeline. Failed: context accumulation in coordinator.
3. **Skill-based multi-step** (v3) — User invokes `/devloop-init`, `/devloop-implement`, etc. Worked but coordinator context still rotted across steps.
4. **Agent Teams** (v4, current) — Single `/devloop` command spawns autonomous teammates. Lead only intervenes at gates. Minimal context accumulation.

The v3 skills have been retired. `/devloop` and `/debate` are now the sole workflows. This ADR documents the v4 design and improvements agreed through a 13-specialist debate (see `docs/debates/2026-02-10-agent-teams-workflow-review/debate.md`).

### Key Design Principles

- **Lead is a coordinator, not implementer** — Lead only acts at gates (plan approval, validation, final approval)
- **Teammates communicate peer-to-peer** — Reviewers message the implementer directly, reducing Lead context load
- **Specialist-owned verification** — Each reviewer owns their domain; findings are blocking unless otherwise specified
- **Simple recovery** — `main.md` records start commit; if interrupted, restart the devloop from the beginning

## Decision

### 1. Dev-Loop Workflow

#### Team Composition: 7 Teammates

Every devloop spawns **7 teammates** (Lead + Implementer + 6 reviewers):

| Role | Specialist | Purpose | Blocking |
|------|------------|---------|----------|
| Implementer | Auto-detected or specified | Does the work | N/A |
| Security Reviewer | security | Vulnerabilities, crypto, auth | MINOR+ blocks; rest TECH_DEBT |
| Test Reviewer | test | Coverage, test quality, regression | MAJOR+ blocks; rest TECH_DEBT |
| Observability Reviewer | observability | Metrics, logging, tracing, PII, SLOs | MINOR+ blocks; rest TECH_DEBT |
| Code Quality Reviewer | code-reviewer | Rust idioms, ADR compliance | MAJOR+ blocks; rest TECH_DEBT |
| DRY Reviewer | dry-reviewer | Cross-service duplication | BLOCKER only; rest TECH_DEBT (per ADR-0019) |
| Operations Reviewer | operations | Deployment safety, rollback, runbooks | MAJOR+ blocks; rest TECH_DEBT |

**Rationale for 7 teammates**: All four mandatory cross-cutting specialists (Security, Test, Observability, Operations) are now included alongside Code Quality and DRY. This resolves a policy inconsistency where CLAUDE.md listed Observability as mandatory but the devloop excluded it. Each reviewer covers a distinct, non-overlapping domain — no natural combination exists without diluting expertise. Reviewers work in parallel, so the added teammate does not significantly increase wall-clock time.

**Conditional domain reviewer**: When the task touches database patterns (`migration|schema|sql`) but the implementer is NOT the Database specialist, add Database as a conditional 8th reviewer for that loop. This prevents schema changes landing without database-aware review. The same principle applies to Protocol when API contracts are affected by a non-Protocol implementer.

**Observability blocking authority** (blocks on MINOR+):
- **BLOCKER**: PII in logs/traces without visibility wrapper, secrets leaked via `#[instrument]` Debug, unbounded metric cardinality, missing `skip_all` on public handlers
- **MAJOR**: Missing instrumentation on critical paths, naming convention violations, no structured logging on error paths
- **MINOR**: Non-critical spans, histogram bucket alignment, verbosity tuning

**Severity definitions** (used across all reviewers):
- **BLOCKER**: Critical issue, cannot merge under any threshold
- **MAJOR**: Significant issue, should fix before merge
- **MINOR**: Should address, lower impact
- Anything not fixed is documented as **TECH_DEBT** in the devloop output

#### Workflow Phases

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
│   └── Check all reviewers confirmed → Lead messages implementer "Plan approved"
│
├── IMPLEMENTATION (Implementer drives — waits for "Plan approved" from Lead)
│   ├── Implementer does the work
│   └── Ready → request validation
│
├── GATE 2: VALIDATION (Lead)
│   └── Run validation pipeline (see below)
│   └── On pass → Lead messages reviewers "Start Review"
│
├── REVIEW (Reviewers examine code — waits for "Start Review" from Lead)
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

#### Validation Pipeline (Gate 2)

Concrete, tiered verification replacing the aspirational "7-layer" reference:

**ENFORCED** (run in order, stop on first failure):

| Layer | Command | What It Catches |
|-------|---------|-----------------|
| 1. Compile | `cargo check --workspace` | Type errors, sqlx compile-time failures |
| 2. Format | `cargo fmt --all -- --check` | Style violations |
| 3. Guards | `./scripts/guards/run-guards.sh` | Credential leaks, PII, instrument-skip-all, test-coverage, api-version-check, metrics-naming, cardinality bounds |
| 4. Tests | `./scripts/test.sh --workspace` | Regressions; ensures DB setup + migrations; report P0 security test count |
| 5. Clippy | `cargo clippy --workspace --lib --bins -- -D warnings` | Lint warnings |
| 6. Audit | `cargo audit` | Known dependency vulnerabilities |

**REPORTED** (tracked in main.md, not blocking):

| Layer | Command | Purpose |
|-------|---------|---------|
| 7. Coverage | `cargo llvm-cov --workspace` | Coverage vs thresholds; flag security-critical files < 95% |

**ARTIFACT-SPECIFIC** (mandatory when detected file types are in the changeset):

| Artifact | Verification | Trigger |
|----------|-------------|---------|
| `.proto` files | Proto compilation (`prost-build`), proto freshness check (regenerate and diff `proto-gen/`), backward compatibility (no removed fields, no changed field numbers) | `git diff --name-only` includes `proto/` |
| `migrations/` | Sequential numbering validation, `.sqlx/` offline data freshness (`sqlx prepare --check` or test DB run), migration reversibility documented | `git diff --name-only` includes `migrations/` |
| K8s manifests | `kubeconform` schema validation, `kubectl apply --dry-run=server` if cluster available | `git diff --name-only` includes `infra/kubernetes/` |
| Dockerfiles | `hadolint` lint | `git diff --name-only` includes `Dockerfile` |
| Shell scripts | `shellcheck` lint | `git diff --name-only` includes `*.sh` |

Note: If only non-Rust files changed, Rust layers (1-6) may be skipped. If both changed, run both pipelines. The script detects artifact types via `git diff --name-only` and activates the relevant layers.

**FUTURE** (improvement items):
- `verify-all.sh` script wrapping all layers with artifact-type detection
- New observability guards: `metrics-naming.sh` (naming conventions), `cardinality-guard.sh` (unbounded label detection)
- Enhance existing `no-pii-in-logs.sh` guard to also check metric labels for PII (currently covers logs, tracing, `#[instrument]`, error messages)
- Failure tracking across retry attempts (flag different failures each attempt as unstable implementation)
- Per-crate benchmark layer (`cargo bench` with regression detection for performance-critical crates)

#### Git State Tracking and Rollback

Dev-loop records git state at setup for recovery:

```markdown
## Loop Metadata
| Field | Value |
|-------|-------|
| Start commit | {git rev-parse HEAD} |
| Branch | {current branch} |
```

**Rollback procedure** (documented in main.md on abandonment):
1. Verify start commit from Loop Metadata
2. `git diff {start_commit}..HEAD` to review all changes
3. `git reset --soft {start_commit}` to unstage (preserves changes for inspection)
4. Or `git reset --hard {start_commit}` if clean revert confirmed
5. For security-critical changes: verify no partial security state remains (no half-secured endpoints)
6. For schema changes: rollback requires a forward migration (ALTER TABLE DROP, etc.) — `git reset` alone is insufficient if migrations were applied
7. For infrastructure changes: rollback may require `skaffold delete` or `kubectl delete -f` if manifests were applied to a dev cluster

**Inline security decision checkpointing**: For security-critical implementations, the implementer maintains a "Security Decisions" table in main.md, updated in real-time:

```markdown
| Decision | Choice | Rationale | ADR Reference |
|----------|--------|-----------|---------------|
| RNG source | SystemRandom | CSPRNG required | ADR-0002 |
| Algorithm | Ed25519 | ADR-approved | ADR-0008 |
```

This persists reasoning in the checkpoint file for safe restore of security-critical work.

#### Auto-Detection Patterns

Expanded patterns for specialist routing:

| Pattern | Specialist |
|---------|------------|
| `auth\|jwt\|token\|oauth\|credential\|key\|rotation\|jwks\|federation\|bcrypt\|password` | auth-controller |
| `meeting\|session\|signaling\|participant\|layout\|roster\|ice\|dtls` | meeting-controller |
| `media\|video\|audio\|stream\|sfu\|simulcast\|bandwidth\|codec\|datagram` | media-handler |
| `api\|endpoint\|route\|http\|gateway\|http3\|webtransport\|tenant\|geographic` | global-controller |
| `database\|migration\|schema\|sql\|index\|query\|sqlx\|postgres\|redis` | database |
| `proto\|protobuf\|contract\|wire\|signaling\|message.format\|grpc` | protocol |
| `deploy\|k8s\|infra\|terraform\|docker\|kubernetes\|helm\|ci\|cd\|pipeline\|github.actions` | infrastructure |
| `test\|coverage\|fuzz` | test |
| `metric\|trace\|log\|observability` | observability |

**Disambiguation**: When a task matches multiple specialist patterns, the more specific match takes precedence. If ambiguity remains, the Lead prompts the user to choose. Example: "fix meeting assignment load balancing" matches both `meeting` (MC) and `assignment` (GC) — Lead asks user which specialist should implement.

#### Lightweight Dev-Loop (`--light`)

For small, contained changes (typically 10-30 lines):

- **Team**: Implementer + Security + one context-dependent reviewer (3 teammates)
- **Third reviewer selection** (Lead decides): Code Quality for style, Observability for metrics, Test for test changes, Operations for deployment, DRY for shared code
- **Skips**: Gate 1 (plan approval), reflection phase
- **Keeps**: Full validation pipeline (Gate 2), review verdicts
- **Not eligible**: Changes touching auth, crypto, session paths, security-critical code, schema/migration changes, protocol changes, deployment manifests (K8s, Docker), `Cargo.toml` dependency changes, `crates/common/` (affects all services), or instrumentation code (`tracing::`, `metrics::`, `#[instrument]`)
- **Escalation**: Any reviewer can request upgrade to full devloop
- **Ambiguity rule**: When in doubt, use full mode. Lead errs on the side of full.

#### Cross-Service Implementation Model

For features spanning multiple services, use tiered approach:

**Tier A — New cross-service patterns** (debate required):
1. Debate defines the interface contract (proto, performance budgets, error semantics)
2. Proto/shared-spec devloop implements the shared interface (locks the contract)
3. Per-service devloops run in parallel against the locked contract
4. Integration devloop verifies the full cross-service flow

**Tier B — Extensions of established patterns** (coordination brief, no debate):
1. Lead provides coordination brief referencing existing integration knowledge files
2. Parallel devloops for each affected service
3. Shared review team validates cross-service consistency

**Differentiator**: Does `docs/specialist-knowledge/{service}/integration.md` already document the pattern? If yes → Tier B. If no → Tier A.

**Exception**: Features involving shared crypto context (e.g., connection tokens where MC issues and MH validates) expand the proto devloop to include a crypto spec, co-owned by Protocol + Security.

**Context handoff**: When a devloop implements an ADR, the implementer prompt MUST reference both the ADR and the debate record:
```
## Context
ADR: docs/decisions/adr-NNNN-{topic}.md
Debate record: docs/debates/YYYY-MM-DD-{slug}/debate.md
Read both before starting. The ADR captures the decision; the debate record captures reasoning behind rejected alternatives.
```

### 2. Debate Workflow

#### Participants

- **Mandatory** (always included): Security, Test, Observability, Operations
- **Domain** (based on question): auth-controller, global-controller, meeting-controller, media-handler, database, protocol, infrastructure, code-reviewer, dry-reviewer
- **Minimum**: 5 specialists (1 domain + 4 mandatory)

#### Escalation with Veto Protection

When a debate is escalated (no consensus after 10 rounds or stalled for 3 rounds):

**For domain disagreements** (non-cross-cutting specialists dissenting):
- Accept majority position with dissent noted (current behavior)

**For cross-cutting specialist dissent** (Security, Test, Observability, or Operations scoring < 70):
- Escalation message to user **must explicitly highlight** the dissenting specialist's specific objection
- User must provide **explicit risk acceptance**: "I acknowledge [Security/Test/Observability/Operations] has unresolved concerns about [X]. I accept this risk."
- This is informed risk acceptance, not implicit majority override

#### ADR Template Enhancements

Debate-produced ADRs include new optional sections:

**Implementation Guidance** (always included):
```markdown
## Implementation Guidance
- Suggested specialist: {name}
- Task breakdown: {if multi-loop, list sequential devloops}
- Key files: {primary files to modify}
- Dependencies: {order constraints between implementation steps}
```

**Protocol Constraints** (when debate touches wire format):
```markdown
### Protocol Constraints
- Field numbers: {allocated numbers and rationale}
- Backward compatibility: {safe vs unsafe changes per ADR-0004}
- Rollout order: {server-first or client-first}
- Wire format: {encoding considerations, size budgets}
- Actor routing: {which actor handles this message type}
```

**Migration Plan** (when debate involves schema changes):
```markdown
### Migration Plan
- Step-by-step migration sequence
- Backward compatibility window
- Rollback procedure
- Data backfill strategy (if applicable)
```

#### Debate Trigger Clarification

"Protocol or contract changes" means **breaking changes, semantic changes, or new message categories** — NOT simple additive fields. Per ADR-0004, safe changes (new optional fields, new enum values, new RPCs, new message types) use a standard `/devloop` without debate.

### 3. Review Protocol Improvements

#### Step 0: Scope the Review

Before reviewing code, each reviewer:
1. Run `git diff --name-only` to identify changed files
2. Prioritize by risk: new files, security-sensitive paths, high-churn files
3. Note `Cargo.toml` changes (new dependencies to audit)
4. Flag security-sensitive file patterns: `auth/`, `crypto/`, `middleware/`, key management files

#### Plan Confirmation Checklist (Gate 1)

When reviewers "confirm" the implementer's plan, each must verify:
1. Approach is technically sound for their domain
2. Approach is ADR-compliant (no contradictions with existing decisions)
3. No domain-specific concerns that would require redesign
4. For security reviewer: threat model implications considered

#### Explicit ADR Compliance

Code Quality reviewer MUST check changed code against relevant ADRs:

1. Identify changed files and their component (`crates/{service}/`)
2. Look up applicable ADRs via `docs/specialist-knowledge/code-reviewer/key-adrs.md`
3. Check implementation against ADR MUST/SHOULD/MAY requirements
4. Severity mapping: MUST/REQUIRED = BLOCKER, SHOULD/RECOMMENDED = MAJOR, MAY/OPTIONAL = MINOR
5. "ADR Compliance" is a mandatory section in the Code Quality verdict

#### Blocking Behavior by Reviewer

| Reviewer | Blocks on | Non-blocking → TECH_DEBT |
|----------|-----------|-----------------------------|
| Security | MINOR+ (all findings) | — |
| Observability | MINOR+ (all findings) | — |
| Infrastructure | MINOR+ (all findings) | — |
| Test | MAJOR+ | MINOR → TECH_DEBT |
| Code Quality | MAJOR+ | MINOR → TECH_DEBT |
| Operations | MAJOR+ | MINOR → TECH_DEBT |
| DRY | BLOCKER only | MAJOR, MINOR → TECH_DEBT (per ADR-0019) |

Anything not fixed is documented as TECH_DEBT in the devloop output's Tech Debt section.

#### guard:ignore Justification

Any `guard:ignore` annotation MUST include a reason:
```rust
// guard:ignore(REASON) — e.g., guard:ignore(test-only fixture, not production code)
```
Guards without justification are flagged as findings.

### 4. Recovery Model

If a devloop is interrupted, restart from the beginning with `/devloop`. The `main.md` file records the start commit for rollback if needed. No checkpoint/restore mechanism is required — restarting is simpler and avoids stale context.

### 5. CLAUDE.md Consistency

Update CLAUDE.md to explicitly state: **All 4 cross-cutting specialists (Security, Test, Observability, Operations) are mandatory in both devloops and debates.** This closes the policy inconsistency.

## Consequences

### Positive

1. **Policy consistency** — Observability now included in devloop, matching CLAUDE.md's mandate
2. **Concrete verification** — Validation pipeline defined with specific commands, not aspirational descriptions
3. **Informed risk acceptance** — Security/Ops dissent in debates requires explicit user acknowledgment
4. **Scalable cross-service model** — Tiered approach (debate → sequential devloops) handles multi-service features
5. **Lightweight option** — `--light` reduces overhead for small, safe changes
6. **Artifact-aware verification** — Pipeline extensible to non-Rust artifacts (proto, K8s, Docker)
7. **Rollback safety** — Git state tracked, rollback procedure documented per devloop

### Negative

1. **Larger review team** — 7 teammates instead of 6 increases resource usage per devloop
2. **More complex verification** — Artifact-specific layers add implementation work
3. **Stricter debate escalation** — Cross-cutting veto may slow consensus on contentious decisions

### Neutral

1. **Knowledge files unchanged** — Specialist knowledge architecture (ADR-0017) unaffected
2. **Output format compatible** — `docs/devloop-outputs/` structure preserved with additions
3. **Simpler recovery** — `/devloop-restore` removed in favor of restart-from-beginning model

## Implementation Items

### Immediate (before next devloop)

1. Update `.claude/skills/devloop/SKILL.md` — Add Observability reviewer, update team to 7, add conditional domain reviewer, add auto-detection disambiguation
2. Update `CLAUDE.md` — Clarify all 4 cross-cutting specialists mandatory in devloops AND debates
3. Update `.claude/agent-teams/protocols/review.md` — Add Step 0 scoping, plan confirmation checklist, ADR compliance procedure, blocking behavior generalized note, guard:ignore(REASON) requirement
4. Update `.claude/skills/debate/SKILL.md` — Veto protection in escalation, debate trigger clarification, Implementation Guidance section in ADR template
5. Add `protocol` row to auto-detection table with expanded patterns for all specialists
6. Add git state tracking + inline security decision checkpointing to devloop setup phase
7. Update devloop output template with Observability status field, rollback procedure section, Loop Metadata

### Follow-Up (subsequent devloops)

8. Create `scripts/workflow/verify-all.sh` with artifact-type detection (mandatory infra/proto/migration layers)
9. Create observability guards: `metrics-naming.sh`, `cardinality-guard.sh`, `no-pii-in-tracing.sh`
10. Add proto freshness check to verification pipeline (regenerate + diff proto-gen/)
11. Add migration safety check to verification pipeline (sequential numbering, sqlx prepare --check)
12. Implement `--light` flag in devloop skill with explicit exclusion criteria
13. Add cross-service implementation model (Tier A/B) documentation to devloop skill
14. Add Protocol Constraints and Migration Plan conditional sections to debate ADR template
15. ~~Add restore pre-flight verification~~ — Removed; restart-from-beginning model adopted instead
16. Add per-crate benchmark layer for performance-critical services (future)

## Participants

| Specialist | Final Position | Satisfaction |
|-----------|---------------|-------------|
| Security | Sound with veto protection, verification layers, inline decision checkpointing | 93 |
| Test | Sound with concrete pipeline, coverage reporting, debate veto | 92 |
| Observability | Sound with inclusion as 7th reviewer, validation checks, guard specs | 95 |
| Operations | Operationally sound with rollback, veto weight, recovery model | 95 |
| Code-Reviewer | Sound with ADR compliance checklist, Step 0 scoping, blocking behavior | 93 |
| DRY-Reviewer | Well-positioned with generalized blocking behavior documentation | 93 |
| Auth-Controller | Practical with observability fix, expanded patterns, security checkpointing | 95 |
| Global-Controller | Improved with cross-service model, lightweight variant, disambiguation rule | 93 |
| Meeting-Controller | Resolved with sequential devloops, Tier A/B model, observability | 95 |
| Media-Handler | Improved with observability, benchmark future item, sequential loops | 94 |
| Database | Improved with migration safety layers, expanded auto-detection, migration plan template | 92 |
| Protocol | Improved with proto freshness check, auto-detection row, Protocol Constraints template | 95 |
| Infrastructure | Improved with mandatory artifact verification, infra-specific layers | 93 |

**Consensus**: Reached at Round 3 with 93.7% average satisfaction (all participants ≥ 92%).

## Debate Reference

See: `docs/debates/2026-02-10-agent-teams-workflow-review/debate.md`

## Supersedes

- ADR-0022: Skill-Based Development Loop (which superseded ADR-0016)
- ADR-0021: Step-Runner Architecture
