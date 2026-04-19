# ADR-0024: Agent Teams Development Workflow

## Status

Accepted

## Date

2026-02-10 (amended 2026-04-18 — added §6 Cross-Boundary Ownership Model)

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

### 6. Cross-Boundary Ownership Model

*Added by the 2026-04-18 amendment debate (see Debate Reference).*

#### 6.1 Motivation

The implicit "file owner implements" rule produces correct outcomes when changes require domain judgment but disproportionate ceremony when changes are mechanical, minor defensive adjustments, or naturally span services via convention. The 2026-04 ADR-0031 rollout surfaced multiple instances where cross-boundary friction generated elaborate workarounds (~80 LOC of allowlist infrastructure, Lead-level adjudication thrash, multiple devloops where one would do) that were disproportionate to the underlying work.

This section codifies a three-category classification × tiered owner-involvement model, carves out genuinely high-risk shared surfaces (Guarded Shared Areas), and backs the mechanism with guard infrastructure rather than process ceremony.

#### 6.2 Three-Category Classification

Every cross-boundary edit self-classifies as one of:

**Mechanical** — Value-neutral *and* structure-preserving changes where the `sed`-test applies: the edit is expressible as a deterministic find-and-replace that does not change the concept encoded in the code. Examples: identifier renames that track an approved taxonomy change, path/URL rewrites tracking a deployment move, comment fixes, format conformance.

Mechanical classification requires that the **full guard pipeline covers the change-pattern**. If no guard exists for the file-type × change-pattern combination, the default classification is Minor-judgment. This creates a forcing function for guard expansion: the payoff for adding a guard is that the relaxed rule extends to that area ("mechanical iff guards catch every partial version").

Edits that pass the guard pipeline but change the **concept encoded in a string** (e.g., renaming a metric label without updating its semantic meaning in the taxonomy) are NOT mechanical.

**Minor-judgment** — Small defensive adjustments where a reasonable reader could argue either way but impact is bounded: bumping a `for:` duration to match convention, widening a threshold by a small margin, adding a missing structured-log field.

**Domain-judgment** — Changes requiring the owner's domain knowledge: threshold tuning, behavior changes, API semantics, new instrumentation that affects SLO shape.

**Classification timing and scope.** The implementer lists **every** planned file change in the devloop's `main.md` (plan template) with a per-file classification: **Mine** (in implementing specialist's domain — the trivial label for most rows), or for cross-boundary rows one of **Not mine, Mechanical** / **Not mine, Minor-judgment** / **Not mine, Domain-judgment**. Classification happens during plan authoring, before Gate 1. Reviewers may **upgrade** a classification at Gate 1 or during Gate 3 review; downgrade is disallowed. Classification challenges auto-route to ESCALATE. This monotonic-upgrade rule prevents owner-specialists from being pressured down to review-only when their judgment says owner-implements.

#### 6.3 Owner Involvement by Category

| Category | Owner involvement | Mechanism |
|----------|-------------------|-----------|
| Mechanical | Review-only | Owner sees the change at the standard reviewer gate. No separate approval. |
| Minor-judgment | Hunk-ACK required | Owner-specialist must explicitly ACK the specific cross-boundary hunk via a commit trailer (§6.7). PR-level review is insufficient. |
| Domain-judgment | Owner-implements | Route to a separate devloop with owner as implementer, or use the **Paired flag** (§6.5) to keep the owner in the loop during the current devloop. |

#### 6.4 Guarded Shared Areas

Certain surfaces override the category classification: even a "mechanical-looking" edit routes to the owner-specialist. **Mechanical classification is disallowed inside Guarded Shared Areas; Minor-judgment requires owner hunk-ACK.**

**Criterion** (names the test, not just the list): wire-format runtime coupling, OR auth-routing policy, OR detection/forensics contract, OR schema evolution. Modules matching the criterion are Guarded whether or not they appear in the enumerated list below.

**Enumerated Guarded Shared Areas** (current snapshot):

<!-- Source of truth for GSA enumeration. Mirrored in .claude/skills/devloop/SKILL.md and .claude/skills/devloop/review-protocol.md — update all three when extending via micro-debate. -->

- `proto/**`, `proto-gen/**`, `build.rs` — wire format
- `crates/media-protocol/**` — SFU protocol semantics (protocol + MH co-sign)
- `crates/common/src/jwt.rs`, `meeting_token.rs`, `token_manager.rs`, `secret.rs` — auth/crypto primitives
- `crates/common/src/webtransport/**` — wire-runtime coupling
- `crates/ac-service/src/jwks/**`, `src/token/**`, `src/crypto/**` — crypto primitives
- `crates/ac-service/src/audit/**` — detection/forensics contract
- `db/migrations/**` — schema evolution
- ADR-0027-approved crypto primitives (wherever referenced)

**Intersection rule.** When an edit spans two Guarded Shared Areas (e.g., auth-routing fields in `proto/internal.proto` span both wire-format *and* auth-routing-policy), all affected owners co-sign via trailer. Canonical case: `ServiceType` enum, scope enums, and identity fields in `proto/internal.proto` require `Approved-Cross-Boundary: protocol`, `Approved-Cross-Boundary: auth-controller`, and `Approved-Cross-Boundary: security` per ADR-0003 §5.7.

Extending this list requires a **micro-debate** (~3 specialists: the affected owner + security + one cross-cutting), not a new ADR.

**Counter-intuitive property**: the rule is *stricter* inside Guarded Shared Areas, not looser. The category rule relaxes cross-boundary friction in low-risk areas; the Guarded carve-out prevents that relaxation from reaching high-risk surfaces.

**`crates/common/**` outside the Guarded subset** is not owned by a single specialist. Edits require DRY reviewer + code-reviewer approval; affected-specialist involvement is review-only unless call-site semantics change (escalates to Minor-judgment hunk-approval).

#### 6.5 Paired Flag

`/devloop --paired-with=<specialist>` overlays any devloop routing tier (full / light / owner-implements). The paired specialist actively collaborates during implementation and is an explicit reviewer at Gate 2.

- Paired is **a flag, not a mode** — it composes with routing, does not replace it.
- Recommended for **first-of-N exemplar rollouts (N=1)**. For N≥4 affected services, use one paired exemplar + remaining-services-as-mechanical-sweep (the MH ADR-0031 rollout precedent).
- Paired does not exempt Guarded Shared Areas edits from owner-implements routing.

#### 6.6 Classification Workflow

1. Implementer lists every planned file change in `main.md` with per-file classification (§6.2).
2. Reviewers examine the classification table at Gate 1 (plan approval). Primary review question: are cross-boundary rows classified correctly, and does any row need upgrade?
3. Any reviewer may upgrade a classification; challenges auto-route to ESCALATE.
4. **Pattern B** convention-driven coordinated renames require a **named convention author** (e.g., observability for metric taxonomy, operations for alert conventions). Absent a named convention author, Pattern B collapses to owner-implements.
5. Gate 2 validation includes two mechanical consistency guards (no semantic judgment):
   - **Scope-drift guard (Layer A)**: compares the current diff against the plan's file list. Unplanned changes or listed-but-untouched files are flagged for Lead adjudication per §6.3/§6.4 rules.
   - **Classification-sanity guard (Layer B)**: enforces narrow rules a shell script can safely check — (a) Guarded Shared Area paths cannot be classified Mechanical; (b) plan rows for GSA paths must have an Owner field matching the ownership manifest; (c) plan rows marked "Trailer: Yes" must list a specialist that appears as an `Approved-Cross-Boundary:` trailer at commit time.
6. Trailer consistency is verified at **commit time** (pre-commit hook and/or CI), not Gate 2 — commits don't exist at Gate 2. The Layer B rule (c) above is the commit-time check.
7. The code-reviewer verdict at Gate 3 includes an explicit **Ownership Lens** field recording classifications, owner-specialists involved, and any trailer-backed approvals observed.

**Design rationale.** Classification is the human work (semantic — is this really Mechanical?). Guards are mechanical consistency checks (does the diff match the plan? is the Owner field filled? does the commit trailer exist?). This keeps scripts narrow (no ownership-judgment heuristics, no category-classification heuristics) and puts the meaningful decision at Gate 1 where a plan change is cheap, not at commit time where changes are expensive.

#### 6.7 APPROVED-CROSS-BOUNDARY Commit Trailer

Hunk-ACK is recorded as a git commit trailer, not an in-thread string:

```
Approved-Cross-Boundary: <specialist-name> <reason ≥ 10 chars>
```

- RFC-5322 style, parseable by `git interpret-trailers`.
- Matches the ADR-hash-stamping precedent already used elsewhere in the repo.
- Durable across devloop restarts and thread archival — threads are ephemeral, trailers are permanent record.
- Multiple trailers allowed on a single commit (one per approving specialist).
- **Enforcement point**: commit time (pre-commit hook or CI), not Gate 2 — commits don't exist at Gate 2. See §6.6 step 6 and §6.8 item #2.

#### 6.8 Follow-Up Work

Named spin-outs from this amendment (not blocking adoption).

**Active:**

1. **Plan-template extension + scope-drift guard (Layer A) + classification-sanity guard (Layer B) + Gate 1 reviewer-checklist update.** Extend `docs/devloop-outputs/_template/main.md` with a `## Cross-Boundary Classification` section; author the two guards; build an ownership manifest mapping GSA paths to required specialists; add Gate 1 reviewer-checklist item to `.claude/skills/devloop/review-protocol.md`. Owners: operations + test + security + code-reviewer. Estimated: one medium devloop.
2. **Commit-time trailer consistency check.** Pre-commit hook (or CI job) verifying plan rows marked "Trailer: Yes" produce matching `Approved-Cross-Boundary:` trailers on the devloop's commits. Can fold into #1 or run as a small follow-up. Owner: operations.
3. **GSA three-way sync guard** (~15 LOC). Diffs the GSA enumerated list across `docs/decisions/adr-0024-agent-teams-workflow.md` §6.4, `.claude/skills/devloop/SKILL.md` §Cross-Boundary Edits, and `.claude/skills/devloop/review-protocol.md` Step 0. Prevents drift when GSA extends via micro-debate. Owner: dry-reviewer.
4. **DRY reviewer retrospective audit on Ownership Lens verdict field.** Owner: dry-reviewer.

**Reshaped or dropped from earlier drafts (for the record):**

- ~~`validate-cross-boundary-approval.sh` at Gate 2~~ — reshaped. The "Gate 2 scans commit trailers" framing was flawed (no commits exist at Gate 2). The scope-drift + classification-sanity guards in item #1 (plus commit-time trailer check in item #2) replace it.
- ~~APPROVED-CROSS-BOUNDARY classification-failure fixture suite~~ — dropped. The simplified Layer B guard's narrow rules are reviewer-verifiable during authoring; no separate fixture suite needed.
- ~~Scope/claim/session-field rename guard~~ — deferred indefinitely. Compiler catches most partial renames in Rust; reviewers catch string-level renames for auth-critical paths at Gate 1. Residual risk (string renames across non-Rust files the compiler can't see) is narrow; revisit if observed in practice.

#### 6.9 Cross-References

- **ADR-0019 (DRY Reviewer)**: Pattern A/B/C framework. Amendment required: add clause "`proto/**` edits are never classified as Mechanical for DRY purposes; route via ADR-0024 §6."
- **ADR-0031 (Service-Owned Dashboards and Alerts)**: motivating case; the 2026-04-17 rollout friction drove this amendment.
- **`.claude/skills/devloop/SKILL.md`** and **`.claude/skills/devloop/review-protocol.md`**: operational surfaces implementing §6. See Implementation Items §17-30.

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

### 2026-04-18 Amendment: Cross-Boundary Ownership Model (§6)

17. Update `.claude/skills/devloop/SKILL.md` — add §Cross-Boundary Edits summarizing §6.2–6.5, Paired flag argument, Guarded Shared Areas enumeration, default-posture flip ("proceed-with-review" for Mechanical, not "defer-to-owner")
18. Update `.claude/skills/devloop/review-protocol.md` — Step 0 Guarded Shared Areas scoping, spin-out as third fix-or-defer path, Ownership-lens verdict field, sed-test worked example
19. Update `docs/decisions/adr-0019-dry-reviewer.md` — cross-ref §6, Pattern B note, proto-never-mechanical clause
20. Update `.claude/agents/security.md` — cross-boundary posture; Gate 1 block rules for Guarded Shared Areas
21. Update `.claude/agents/auth-controller.md` — cross-boundary posture; auth-adjacent co-sign on proto edits
22. Update `.claude/agents/observability.md` — cross-boundary posture; literal-convention-citation requirement
23. Update `.claude/agents/meeting-controller.md` — cross-boundary posture
24. Update `.claude/agents/global-controller.md` — cross-boundary posture
25. Update `.claude/agents/media-handler.md` — cross-boundary posture
26. Update `.claude/agents/code-reviewer.md` — Ownership-lens review gate; classification-gaming language
27. Update `.claude/agents/protocol.md` — cross-boundary posture; wire-visibility co-sign rule
28. Update `.claude/agents/test.md` — classification-failure fixture commitment
29. Update `.claude/agents/dry-reviewer.md` — Pattern A/B/C framework; common/ non-ownership clarification
30. Update all specialist `docs/specialist-knowledge/*/INDEX.md` — pointer to ADR-0024 §6
31. Follow-up devloop: `scripts/guards/simple/validate-cross-boundary-approval.sh` (operations + test + security)
32. Follow-up devloop: APPROVED-CROSS-BOUNDARY classification-failure fixture suite (test + AC + security)

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

Original ADR: `docs/debates/2026-02-10-agent-teams-workflow-review/debate.md`

§6 amendment: `docs/debates/2026-04-18-devloop-cross-ownership-friction/debate.md`

### §6 Amendment Participants (2026-04-18)

| Specialist | Final Satisfaction | Position summary |
|-----------|--------------------|------------------|
| Protocol | 94 | Wire-visibility carve-out + proto-gen/build.rs explicit; amend-over-new-ADR |
| Global-Controller | 94 | Paired-as-flag reframe, authz-enforcement surface sensitivity |
| Meeting-Controller | 93 | Value-neutrality clause, classification monotonicity, webtransport carve-out |
| Security | 92 | Conditional asks landed: audit path in GSA + commit-trailer relocation |
| Test | 92 | Guard-coverage conditional, hunk-ACK, classification-failure fixture suite |
| Observability | 92 | Mechanical-iff-guards-catch-every-partial-version formulation, Paired as flag |
| Operations | 92 | Co-owns §6; commit-trailer guard; db/migrations GSA; concept-substitution exclusion |
| Auth-Controller | 92 | Catch-all for future auth/crypto files in common/ |
| Media-Handler | 92 | Extending GSA requires micro-debate (not new ADR); rule stricter inside GSA |
| Code-Reviewer | 92 | Co-owns §6; review-protocol.md ownership; sed-test backbone |
| DRY-Reviewer | 90 | Pattern A/B/C + expanded common/ list (token_manager.rs + secret.rs); common/ non-ownership clarification |

Consensus: Round 2, all 11 participants ≥ 90.

## Supersedes

- ADR-0022: Skill-Based Development Loop (which superseded ADR-0016)
- ADR-0021: Step-Runner Architecture
