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
- **--light**: Lightweight mode — 3 teammates, skip planning gate (see Lightweight Mode)
- **--paired-with=\<specialist\>**: Overlay flag — the named specialist actively collaborates during implementation and is an explicit reviewer at Gate 2. Composes with `--light`/full; does not replace routing. Recommended for first-of-N exemplar rollouts (N=1); for N≥4 affected services, use one paired exemplar + remaining-services-as-mechanical-sweep. **Does not exempt Guarded Shared Areas from owner-implements routing** (see §Cross-Boundary Edits below and ADR-0024 §6.5).
- **--continue**: Reopen a completed devloop to address human review feedback (see Continue Mode)

## Team Composition

### Full Mode (default)

<!-- Mirror of ADR-0024 §1 Team Composition / SKILL.md §Team Composition.
     Update both locations together when changing teammate count or roster. -->

Every devloop spawns **8 teammates** (Lead + Implementer + 7 reviewers). `name` is the SendMessage recipient; `subagent_type` loads identity from `.claude/agents/{name}.md`.

| Role | `name` | `subagent_type` | Purpose |
|------|--------|-----------------|---------|
| Implementer | `implementer` | `{specialist}` | Does the work |
| Security Reviewer | `security` | `security` | Vulnerabilities, crypto, auth |
| Test Reviewer | `test` | `test` | Coverage, test quality, regression |
| Observability Reviewer | `observability` | `observability` | Metrics, logging, tracing, PII, SLOs |
| Code Quality Reviewer | `code-reviewer` | `code-reviewer` | Rust idioms, ADR compliance |
| DRY Reviewer | `dry-reviewer` | `dry-reviewer` | Cross-service duplication (see DRY exception in review protocol) |
| Operations Reviewer | `operations` | `operations` | Deployment safety, rollback, runbooks |
| Semantic Guard Reviewer | `semantic-guard` | `semantic-guard` | Diff-level anti-pattern checks per `scripts/guards/semantic/checks.md` (credential leak, actor blocking, error-context preservation, metrics path completeness). Distinct from code-reviewer's general lens (Rust idioms, ADR compliance, naming, error handling). Applies to non-test production code per `.claude/agents/semantic-guard.md` §Judgment Calibration. |
| Paired Specialist (if `--paired-with=<specialist>`) | `paired-<specialist>` | `{specialist}` | Active collaborator during implementation + Gate 2 reviewer. When `<specialist>` is already a mandatory reviewer (security/test/observability/operations), the paired teammate replaces that slot with the same identity and an expanded role. |

The Lead (orchestrator) is automatically named `team-lead` in the team config.

**Review model**: All findings default to "fix it." Deferral requires demonstrating that fix-now-cost > fix-later-cost + tracking-overhead — *for this specific finding*. Verdicts split RESOLVED into RESOLVED-FIXED (everything cleared from the diff) vs RESOLVED-DEFERRED (any finding still in tree); even one accepted deferral forces RESOLVED-DEFERRED for that reviewer. See review protocol for the burden-of-proof, the suspicious-deferral check, and the full taxonomy.

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

(Semantic Guard is full-mode-only; not eligible as a `--light` context reviewer. The semantic-guard checks are most valuable on multi-file diffs where pattern guards miss issues; for the small contained changes `--light` targets, the panel stays at 3 teammates. If a `--light` change appears to need semantic-guard coverage, escalate to full mode per the escalation rule.)

**Skips**: Gate 1 (plan approval)
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

When authoring the plan for a devloop, the implementer lists **every** planned file change in `main.md` (plan template) with a per-file classification — **Mine** (in-domain, the trivial label for most rows), or for cross-boundary rows one of **Not mine, Mechanical** / **Not mine, Minor-judgment** / **Not mine, Domain-judgment**. Reviewers may **upgrade** a classification at Gate 1 or during Gate 3 review (downgrade disallowed); challenges auto-route to ESCALATE. Route per the Owner Involvement table (§6.3). See ADR-0024 §6 for rationale; this section covers operational triggers.

### Three-Category Classification (§6.2)

- **Mechanical** — Value-neutral *and* structure-preserving (the `sed`-test applies: deterministic find-and-replace that does not change the encoded concept). Requires full guard pipeline coverage for the change-pattern: **Mechanical iff guards catch every partial version** (see `./scripts/guards/run-guards.sh`). Concept substitution (renaming a metric label while changing its semantic meaning) is NOT Mechanical.
- **Minor-judgment** — Small defensive adjustments where a reasonable reader could argue either way but impact is bounded. Examples: widening a numeric threshold, adding a missing structured-log field. **Alert rule changes** (severity, routing labels, `for:` duration) are Minor-judgment when the edit couples to runbook prose (`docs/runbooks/*.md`) or the alert conventions doc (`docs/observability/alert-conventions.md`) — hunk-ACK by operations is required because the runbook narrative must stay coherent with the fired-state semantics.
- **Domain-judgment** — Changes requiring the owner's domain knowledge (threshold tuning, behavior changes, API semantics, new instrumentation affecting SLO shape).

Use ADR-0019 Pattern A/B/C vocabulary for duplication/rename patterns; Pattern B coordinated renames require a **named convention author** (e.g., observability for metric taxonomy) — absent one, Pattern B collapses to owner-implements.

### Owner Involvement (§6.3)

| Category | Owner involvement | Mechanism |
|----------|-------------------|-----------|
| Mechanical | Review-only | Owner sees the change at the standard reviewer gate. No separate approval — **proceed with review**. |
| Minor-judgment | Owner confirmation required | Owner-specialist must be a reviewer on the devloop and must confirm the cross-boundary hunk at Gate 1 and at Gate 3 (via Ownership Lens verdict). Not satisfied by generic PR approval. Optional `Approved-Cross-Boundary:` trailer (below) available as an audit breadcrumb. |
| Domain-judgment | Owner-implements | Route to a separate devloop with owner as implementer, or use `--paired-with=<owner>` to keep the owner in the loop during the current devloop. |

**Default-posture flip**: For Mechanical cross-boundary edits the default is "proceed with review," NOT "defer to owner." The older implicit "owner-implements" rule holds only for Domain-judgment and Guarded Shared Areas. **This flip does NOT apply inside Guarded Shared Areas — Mechanical classification is disallowed there.**

### Guarded Shared Areas (§6.4)

Certain surfaces override the category classification: even a sed-clean edit routes to the owner-specialist. **Mechanical is disallowed inside GSA; Minor-judgment requires owner confirmation at Gate 1 and Gate 3 (§6.3).**

**Criterion** (names the test, not just the list): wire-format runtime coupling, OR auth-routing policy, OR detection/forensics contract, OR schema evolution. Paths matching the criterion are Guarded whether or not enumerated below.

<!-- Mirror of ADR-0024 §6.4 enumerated list. Update all five locations together
     (ADR-0024 §6.4, this file, .claude/skills/devloop/review-protocol.md Step 0,
      scripts/guards/simple/cross-boundary-ownership.yaml,
      and the CANON array in scripts/guards/simple/validate-gsa-sync.sh)
     when extending via micro-debate. -->

- `proto/**`, `proto-gen/**`, `build.rs` — wire format
- `crates/media-protocol/**` — SFU protocol semantics (protocol + MH co-sign)
- `crates/common/src/jwt.rs`, `meeting_token.rs`, `token_manager.rs`, `secret.rs` — auth/crypto primitives
- `crates/common/src/webtransport/**` — wire-runtime coupling
- `crates/ac-service/src/jwks/**`, `src/token/**`, `src/crypto/**` — crypto primitives
- `crates/ac-service/src/audit/**` — detection/forensics contract
- `db/migrations/**` — schema evolution
- ADR-0027-approved crypto primitives (wherever referenced) — path-independent; guards a concept, not a directory. New call sites of enumerated primitives inherit Guarded status regardless of the containing file.

**Intersection rule**: edits spanning two GSA (e.g., auth-routing fields in `proto/internal.proto` crossing wire-format × auth-routing-policy) require all affected owners present as reviewers and all confirming at Gate 1 / Gate 3. Canonical case: changes to `ServiceType` enum, scope enums, or identity fields in `proto/internal.proto` need protocol + auth-controller + security (ADR-0003 §5.7).

**`crates/common/**` outside the Guarded subset** is not owned by a single specialist. Edits require DRY reviewer + code-reviewer approval; affected-specialist involvement is review-only unless call-site semantics change (escalates to Minor-judgment, §6.3).

Extending the enumerated list requires a **micro-debate** (~3 specialists: affected owner + security + one cross-cutting), not a new ADR. Counter-intuitive property: the rule is *stricter* inside GSA, not looser.

### Optional `Approved-Cross-Boundary:` Commit Trailer (§6.7)

Owner confirmation is satisfied by Gate 1 review + Gate 3 Ownership Lens verdict (§6.3). For cases where a durable audit breadcrumb matters (e.g., auth-critical edits that will be referenced during post-incident review), an owner may optionally record confirmation as a commit trailer:

```
Approved-Cross-Boundary: <specialist-name> <reason ≥ 10 chars>
```

RFC-5322 style, parseable by `git interpret-trailers`. Multiple trailers per commit allowed. Not mechanically enforced — use when durability of the audit record matters.

**Enforcement**: two narrow mechanical guards (no semantic judgment):
- **Layer B classification-sanity** (runs at Gate 1 via Lead invocation; also at Gate 2 via `run-guards.sh` as safety net): GSA paths cannot be Mechanical; GSA paths must have the Owner field filled per the ownership manifest.
- **Layer A scope-drift** (runs at Gate 2 via `run-guards.sh`; needs the diff): flags files in the diff that weren't listed in the plan, or plan entries that weren't touched.

**Pending implementation** — tracked as ADR-0024 §6.8 item #1. Until the guards land, the Lead manually examines the plan's Classification table against ADR §6.3 and §6.4 rules at Gate 1 and Gate 3.

## Workflow Overview

```
SETUP → PLANNING [skipped --light] → GATE 1 [skipped --light] →
IMPLEMENTATION → GATE 2 (VALIDATION) → REVIEW → GATE 3 (FINAL APPROVAL) →
COMMIT → COMPLETE
```

Lead has minimal involvement — acts only at the three gates. Teammates drive Planning, Implementation, and Review directly. See Instructions below for each step.

**Story-scope reflection**: per-devloop reflection has moved to the story level. Each user story runs a single reflection pass at story-close time (`/close-story`), where specialists update their `INDEX.md` based on architectural shifts across the story's devloops. The Gate 2 INDEX guard (`validate-knowledge-index.sh`, invoked via `run-guards.sh`) remains the per-devloop safety net for INDEX consistency.

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
| `proto\|protobuf\|buf\|contract\|wire\|signaling\|message.format\|grpc` | protocol |
| `test\|coverage\|fuzz` | test |
| `metric\|trace\|log\|observability` | observability |
| `deploy\|k8s\|infra\|terraform\|docker\|kubernetes\|helm\|ci\|cd\|pipeline\|github.actions` | infrastructure |
| `client\|svelte\|sdk\|tsx?` | client |

**Disambiguation**: When a task matches multiple specialist patterns, the more specific match takes precedence. If ambiguity remains, Lead prompts the user to choose. Example: "fix meeting assignment load balancing" matches both `meeting` (MC) and `assignment` (GC) — Lead asks user. Example: "fix media SDK bandwidth heuristic" matches both `sdk` (client) and `media` (MH) — Lead asks user.

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

**Defensive cleanup**: Before creating a new team, check for and clean up any stale team from a previous devloop. If a team already exists, send shutdown requests to all teammates and call `TeamDelete`. This handles cases where a previous devloop was interrupted before Step 8.5 (Cleanup Team).

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
6. Use SendMessage to tell @team-lead your verdict: "CLEAR", "RESOLVED-FIXED", "RESOLVED-DEFERRED", or "ESCALATED: {reason}". Any accepted deferral or spin-out forces RESOLVED-DEFERRED (not RESOLVED-FIXED), even if other findings were fixed in the same review.

## Communication

All teammate communication MUST use the SendMessage tool. Plain text output is not visible to other teammates.

- Use SendMessage to message @implementer directly with feedback
- Use SendMessage to message other reviewers if you spot issues in their domain
- Use SendMessage to tell @team-lead for confirmations and verdicts (CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED)
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
- Semantic Guard Reviewer: @semantic-guard (full mode only)
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
| Observability | confirmed / pending |
| Code Quality | confirmed / pending |
| DRY | confirmed / pending |
| Operations | confirmed / pending |
| Semantic Guard | confirmed / pending |
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

When all confirmed, before issuing "Plan approved" run the classification-sanity guard:

```bash
./scripts/guards/simple/validate-cross-boundary-classification.sh docs/devloop-outputs/YYYY-MM-DD-{slug}/main.md
```

If it fails (GSA path marked Mechanical, or GSA path missing Owner field), send findings to @implementer and return to planning. Do NOT issue "Plan approved."

When the guard passes, update main.md: Phase = implementation, and send "Plan approved" to @implementer.

### Step 6: Gate 2 - Validation

When implementer signals "Ready for validation", run the validation pipeline:

**ENFORCED** — single command runs all seven layers in order, stops on first failure:

```bash
./scripts/layer-all.sh
```

Each `scripts/layerN.sh` is independently callable for targeted debugging (e.g., `scripts/layer4.sh` to re-run only Layer 4 on a failing diff). See ADR-0033 §4 for the wrapper contract (`STATUS=` lines, `LAYER=N START=… END=… RESULT=…` stderr summary, worst-child STATUS aggregation, 90s p95 wall-clock budget for the always-run subset).

**Pipeline failures**: see `docs/runbooks/devloop-validation.md` for layer-by-layer failure-mode mapping, exit-code / `STATUS=` enum reference, `_get_base_ref.sh` troubleshooting (the `BASE_REF=…` stderr line is the anchor), and per-language wrapper triage.

**Always-Run vs Skip-If-Untouched matrix** (operational subset of ADR-0033 §3; see the ADR for the classifying principle, worked examples like `buf breaking`, and the "when in doubt, always-run" default):

| Layer | Verb     | Always-run                                  | Skip-if-untouched per `lang/<X>/changed.sh` |
|-------|----------|---------------------------------------------|----------------------------------------------|
| 1     | Compile  | —                                           | rust, ts, proto                              |
| 2     | Format   | —                                           | rust, ts, proto                              |
| 3     | Guards   | ALL guards (each self-classifies)           | —                                            |
| 4     | Test     | —                                           | rust, ts (proto has no `test.sh`)            |
| 5     | Lint     | —                                           | rust, ts, proto                              |
| 6     | Audit    | `cargo audit`, `pnpm audit`, `buf breaking` | —                                            |
| 7     | Env-tests| dev-cluster + Rust env-tests + Playwright `@smoke` | —                                     |

**Layer N/A justification template**:

A wrapper script under `scripts/lang/<X>/` may report `STATUS=N/A`, `SKIPPED-NO-DIFF`, or `SKIPPED-NO-VERB` per the wrapper contract in ADR-0033 §6. `scripts/layer-all.sh` records the status + `REASON=…` in its summary table; the implementer does **not** owe Gate 2 a separate explanation in those cases — the wrapper's own `REASON=…` is the justification.

The only case requiring implementer action is an unexpected `STATUS=N/A` outside the documented skip cases — that indicates a wrapper bug. Escalate to operations rather than defer.

**ARTIFACT-SPECIFIC** (mandatory when detected file types are in the changeset):

| Artifact | Verification | Trigger |
|----------|-------------|---------|
| `.proto` files | Proto compilation, freshness check (regenerate + diff `proto-gen/`), backward compat | `git diff --name-only` includes `proto/` |
| `migrations/` | Sequential numbering, `.sqlx/` offline data freshness, reversibility documented | `git diff --name-only` includes `migrations/` |
| K8s manifests | `kubeconform` schema validation | `git diff --name-only` includes `infra/kubernetes/` |
| Dockerfiles | `hadolint` lint | `git diff --name-only` includes `Dockerfile` |
| Shell scripts | `shellcheck` lint | `git diff --name-only` includes `*.sh` |

**Layer 7 — Env-tests (Integration)**:

Layer 7 is the seventh shell-layer in `scripts/layer-all.sh`, executed automatically after layers 1-6. The protocol below describes the work `scripts/layer7.sh` performs against the live Kind cluster. Layer 7 always runs — intentionally broader than ADR-0030's trigger-path list, because business logic changes can break integration tests too.

1. **Cluster readiness**: Run `dev-cluster status`. If not ready, run `dev-cluster setup` (polls `setup_in_progress` first). Setup does NOT consume attempts; escalate on setup failure as infra.
2. **Infra change detection**: If `git diff --name-only ${START_COMMIT}..HEAD -- infra/kind/` shows changes, `dev-cluster teardown` then `setup` (cluster skeleton stale). Does NOT consume attempts; log triggering files.
3. **Rebuild services**: `dev-cluster rebuild-all`. Report wall-clock time.
4. **Run env-tests**: Read `/tmp/devloop/ports.json` to construct `ENV_TEST_{AC,GC,PROMETHEUS,GRAFANA,LOKI}_URL` from `.container_urls.*`. Run `timeout 600 cargo test -p env-tests --features all 2>&1 | tee /tmp/devloop/env-test-output.log`. On failure, forward full output + log path to implementer.
5. **Classify exit**: Exit 0 = pass. Non-zero: if stderr matches infra patterns (`connection refused|timed out|connection reset|broken pipe`), **infrastructure failure** (retry once, do NOT consume attempt, then escalate). Otherwise **test failure** (consume attempt).

**Layer 7 attempt budget**: 2 attempts (separate from layers 1-6's 3). Infrastructure failures do not consume attempts. First-run cluster setup (~7 min) does not count toward attempts.

**If pass**:
- Update main.md: Phase = review
- Message each reviewer individually (unicast, not broadcast): "Start Review. Validation passed — please examine the changes and send your verdict."

**If fail (layers 1-6)**:
- Send failure details to implementer
- Increment iteration count
- Max 3 attempts before escalation

**If fail (layer 7)**:
- Send full env-test output (stdout + stderr) to implementer
- Increment Layer 7 iteration count (separate from layers 1-6)
- Max 2 attempts before escalation
- Infrastructure failures do not consume attempts (retry once, then escalate)

### Step 7: Gate 3 - Final Approval

Wait for all reviewer verdicts.

Track verdicts in main.md:
```
| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED | {count} | {count} | {count} | |
| Test | CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED | | | | |
| Observability | CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED | | | | |
| Code Quality | CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED | | | | |
| DRY | CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED | | | | |
| Operations | CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED | | | | |
| Semantic Guard | CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED | | | | |
```

A reviewer's verdict is **RESOLVED-DEFERRED** if even one of their findings was deferred or spun-out, regardless of how many others were fixed. RESOLVED-FIXED means zero remaining findings from that reviewer in the diff.

**If any ESCALATED**:
- Lead reviews the specific finding and the implementer's deferral justification
- Lead decides: fix it (route back to implementer) or accept the deferral (override)
- If routed back: return to implementation phase, max 3 review→implementation iterations

**If all CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED**:
- Update main.md: Phase = complete
- If any reviewer landed on RESOLVED-DEFERRED, surface that explicitly in the summary — at least one accepted deferral exists. Document accepted deferrals in main.md's §Accepted Deferrals section (with implementer's justification).
- Proceed to Step 8 (Commit).

### Step 8: Commit

After review, stage and commit:

1. `git add -A`
2. Commit with message:
   ```
   {task description}

   Devloop: {YYYY-MM-DD-slug}
   Specialist: {specialist}
   Mode: {full|light}
   Verdicts: Security {verdict}, Test {verdict}, Observability {verdict}, Code Quality {verdict}, DRY {verdict}, Operations {verdict}, Semantic Guard {verdict}

   Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
   ```
3. If nothing to commit, skip silently

### Step 8.5: Cleanup Team

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

Append any accepted deferrals, DRY extraction opportunities, and
spun-out findings to `docs/TODO.md` under the appropriate section
(Cross-Service Duplication, Observability Debt, Code Quality, etc.),
then add a one-line pointer to each under main.md's §Accepted Deferrals.
The pointer surfaces the cost shift at the devloop level; the body lives
in `docs/TODO.md` as the durable, decay-tracked home for forward-looking
work. Scope decisions that are devloop-local context (e.g., "we did not
do X because that is task Y's scope") belong in main.md's existing
scope/classification sections — not under §Accepted Deferrals, which is
reserved for issues that were findings and remain in the diff.

If this task is part of a user story, update the Devloop Tracking table
in the user story file: set Status to Completed and fill in the Devloop
Output path. Include this update in the Step 8 commit.

Story-scope INDEX updates happen at story-close time, not here — see `/close-story` Phase 2.

Report to user:
```
**Dev-Loop Complete**

Task: {task description}
Duration: {time}
Iterations: {count}

Results:
- Security: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED ({N} findings, {M} fixed, {K} deferred)
- Test: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED
- Observability: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED
- Code Quality: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED
- DRY: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED ({N} tech debt observations)
- Operations: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED
- Semantic Guard: CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED

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
| Human review rounds | 3 per devloop | Escalate ("is this task well-scoped?") |

## Recovery

If a session is interrupted, restart the devloop from the beginning. The main.md file records the start commit for rollback if needed.

## Files

- **Specialist definitions**: `.claude/agents/{name}.md` (auto-loaded via `subagent_type`)
- **Review protocol**: `.claude/skills/devloop/review-protocol.md`
- **Output**: `docs/devloop-outputs/YYYY-MM-DD-{slug}/main.md`
- **Navigation**: `docs/specialist-knowledge/{name}/INDEX.md`
