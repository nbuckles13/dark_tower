# AI-Centric Development: How Dark Tower is Built

Dark Tower is built entirely with AI-generated code. But not just "ask an AI to write code" - we've developed a structured methodology that treats AI agents as specialized team members, with defined roles, accumulated knowledge, and rigorous verification.

This document explains how it works.

---

## The Fundamental Constraint: AI Context is Limited

Before explaining the architecture, it's important to understand the constraint that shapes everything:

**AI models have limited context windows, and even within those limits, quality degrades as context grows.**

Ask an AI to "build a secure authentication system" with no structure, and it will:
- Forget security requirements while implementing features
- Lose track of earlier decisions as the conversation grows
- Miss edge cases because there's too much to hold in "working memory"
- Produce inconsistent code across different parts of the system

This isn't a flaw to work around - it's a fundamental constraint to design for. Our entire methodology exists to keep any single AI interaction **focused, bounded, and verifiable**.

---

## The Core Idea: Autonomous AI Specialist Teams

Instead of one AI doing everything, we use **specialist agent teams**. Each agent has deep expertise in one domain, and agents talk to each other directly while a Lead enforces gates.

```
┌─────────────────────────────────────────────────────────────┐
│                    HUMAN ORCHESTRATOR                        │
│     Plans stories, approves designs, runs the manual demo    │
└────────────────────────────┬────────────────────────────────┘
                             │
        /user-story, /debate, /devloop, or run-story.sh
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   LEAD (Delegate Mode)                       │
│   Spawns team, enforces gates, cannot implement directly    │
└────────────────────────────┬────────────────────────────────┘
                             │
        Spawns teammates with composed prompts
        (specialist identity + navigation INDEX + task)
                             │
     ┌───────────────────────┼───────────────────────┐
     ▼                       ▼                       ▼
┌────────────┐  ┌───────────────────┐  ┌──────────────────────┐
│IMPLEMENTER │  │  DOMAIN REVIEWERS │  │ CROSS-CUTTING        │
│ One of:    │  │ code-reviewer     │  │ REVIEWERS (mandatory)│
│ auth-ctrl  │  │ dry-reviewer      │  │ security             │
│ global-ctrl│  │ semantic-guard    │  │ test                 │
│ meeting-ctl│  │   (conditional)   │  │ observability        │
│ media-hndlr│  └─────────┬─────────┘  │ operations           │
│ client     │            │            └──────────┬───────────┘
│ database   │            └───────────┬───────────┘
│ protocol   │                        │
│ infra      │          Direct peer-to-peer
│ observ.    │            messaging between
└─────┬──────┘              all teammates
      └────────────────────────┘
```

**Why this structure?**
- **Autonomous teammates**: Specialists drive their own work and talk to each other directly; the Lead doesn't relay messages
- **Independent context**: Each teammate has its own context window, so one specialist's work doesn't crowd out another's
- **Lead stays minimal**: The Lead spawns the team and acts only at gates (plan confirmation, review verdicts, full validation)
- **Focused prompts**: Each specialist gets its identity, a short navigation index for its domain, and the task, nothing else
- **Cross-cutting review**: Security, Test, Observability, and Operations review every change and catch what domain experts miss

---

## Multi-Agent Debates: Consensus-Driven Design

For features that cross service boundaries, we don't let one specialist decide. Instead, we run a **structured debate** where specialists discuss directly with each other:

1. **Spawn team** - Lead spawns domain + mandatory cross-cutting specialists as teammates
2. **Debate directly** - Specialists message each other with proposals, concerns, and rebuttals
3. **Score satisfaction** - Each specialist periodically reports their satisfaction score (0-100%) to the Lead
4. **Converge** - Continue until all participants reach 90%+ satisfaction
5. **Document** - Lead creates an Architecture Decision Record (ADR)

**Minimum debate size**: 5 agents (domain specialist + Security + Test + Observability + Operations)

**Example**: Designing a key rotation system required debate between:
- Auth Controller (implementation)
- Security (cryptographic requirements)
- Test (how to verify correctness)
- Observability (what to log/metric)
- Operations (deployment safety, failure modes)

The specialists debated directly - Security raising concerns about key lifecycle that Operations then addressed with rollback proposals, while Test suggested verification approaches that Observability built monitoring around. The result was a more robust design than any single specialist would have produced.

**Invoke with**: `/debate "design question"`

**Output**: ADR only - implementation is a separate `/devloop` invocation.

---

## Specialist Knowledge: Navigation, Not Memory

Each specialist has one file, `docs/specialist-knowledge/{specialist}/INDEX.md`. It is a short map of where things live in its domain: which ADR governs a topic, which file and function implement it, which guard enforces it.

```
docs/specialist-knowledge/security/INDEX.md
- Service auth (OAuth 2.0 Client Credentials) → ADR-0003 | Key rotation → ADR-0008 | ...
- sender_id allocation, never reused → crates/mc-service/src/media_admission/sender_id.rs:SenderIdAllocator::allocate()
- No key material on the MC→MH contract → crates/dt-guard/src/internal_proto_no_key_material.rs (wrapper scripts/guards/simple/validate-internal-proto-no-key-material.sh)
```

**How it works**:
1. The Lead injects the specialist's INDEX into every spawn prompt as its `## Navigation` section. Injection is deterministic, so no step depends on an agent remembering to read its notes.
2. The INDEX points; it does not restate. Facts live in code and ADRs only, and rules that can be enforced are enforced by tooling (clippy, guards, CI), not by instructions.
3. A guard (`validate-knowledge-index.sh`) checks every INDEX on every validation run, so a pointer to a moved or deleted file fails the pipeline.
4. When a user story closes, `/close-story` runs one reflection pass: specialists update their INDEX for the architecture the story changed, with the whole story's context in hand.

**Why not accumulate lessons?** That was the first design ([ADR-0017](docs/decisions/adr-0017-specialist-knowledge.md)): each specialist kept growing `patterns.md`, `gotchas.md` and `integration.md` files and read them first on every task. Over time those files cached facts that went stale, and a stale "fact" in a knowledge file overrode the ADR it had been copied from. [ADR-0026](docs/decisions/adr-0026-knowledge-restructure.md) replaced them with navigation-only indexes: an agent that is told *where* to look reads the current truth instead of a remembered one.

---

## Validation Pipeline: Trust but Verify

Even with focused specialists, AI will sometimes forget things. A specialist deep in implementation logic might overlook a log statement that contains sensitive data. This isn't failure; it's expected.

**Guards exist because we know AI is fallible.** They're the safety net that catches what slips through.

Every change passes through a **polyglot validation pipeline** (`scripts/layer-all.sh` → `layer1..7.sh`; see [ADR-0033](docs/decisions/adr-0033-polyglot-validation-pipeline.md)). Each layer runs the relevant checks for Rust, TypeScript and protobuf, and the same scripts run locally, in devloops, and in CI:

| Layer | Check | Purpose |
|-------|-------|---------|
| 1 | Compile | `cargo check` + TS typecheck/build + pnpm peer check |
| 2 | Format | `cargo fmt` + prettier + `buf format` |
| 3 | Guards | Pattern and policy checks (`dt-guard`, ADR-0034) + pipeline self-tests |
| 4 | Test | Rust unit/integration + TS unit/component (Nx) |
| 5 | Lint | `cargo clippy` + eslint/svelte-check + `buf lint` |
| 6 | Audit | `cargo audit` + `pnpm audit` + `buf breaking` |
| 7 | Env-tests | Integration and browser E2E tests against a live Kind cluster |

Two rules keep the pipeline honest. **No layer may report success without running**: a skipped, stubbed or unrecognized step is a failure, never a quiet pass. And **no retries** (ADR-0028): a flaky test is fixed, not re-run until green.

### Pattern Guards

Fast, deterministic checks, about fifty of them, most implemented in one Rust binary, `dt-guard` ([ADR-0034](docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md)). Examples:

| Guard | What it catches |
|-------|-----------------|
| `no-hardcoded-secrets` | API keys, passwords in source code |
| `no-secrets-in-logs` / `no-pii-in-logs` | Sensitive data or PII in log statements |
| `instrument-skip-all` | `#[instrument]` using denylist `skip()` instead of allowlist `skip_all` |
| `test-registration` | Test files not registered in an entry point (silently never run) |
| `ts-no-retained-credentials` | Client code that keeps a credential alive past its use |
| `validate-dashboard-panels` | Panels that misuse a metric's type (a raw counter outside `rate()`) or hard-code a datasource |
| `validate-alert-rules` | Alerts whose `runbook_url` is missing or points to no file under `docs/runbooks/` |

See [`scripts/guards/simple/`](scripts/guards/simple/) for the full set.

### Semantic Guards (AI-Powered)

For issues that patterns can't catch, a **semantic-guard agent** reviews the diff during the devloop. It reads its checks from [`scripts/guards/semantic/checks.md`](scripts/guards/semantic/checks.md), the authoritative list (today: credential leaks, client credential lifetime, actor blocking, error context preservation, metrics path completeness). It can read full source files for context, and it reports SAFE or UNSAFE with file, line and explanation.

It is spawned only when a diff touches one of those check surfaces ([ADR-0037](docs/decisions/adr-0037-devloop-process-tuning.md) D4). Running it as a teammate rather than a shell step gives it the whole codebase for context and avoids invoking Claude from inside a shell script.

---

## The Development Loop

Putting it together, here's how one task gets implemented. The key idea: **autonomous teammates drive the work; the Lead only enforces gates**. The authoritative procedure is [`.claude/skills/devloop/SKILL.md`](.claude/skills/devloop/SKILL.md).

> **Evolution Note**: The devloop went through several iterations before arriving at this design. See [Evolution of the Devloop](#evolution-of-the-devloop) below for why earlier approaches failed.

```
┌─────────────────────────────────────────────────────────────┐
│  SETUP (Lead)                                               │
│     /devloop "implement feature X"                          │
│     Lead: spawns implementer + reviewers, each with its     │
│           identity, navigation INDEX and the task           │
│     Lead: records git state, goes idle - teammates drive    │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  PLANNING (teammates collaborate directly)                  │
│     Implementer drafts the approach; reviewers raise        │
│     concerns; the plan is revised until all confirm         │
│     ► GATE 1: every reviewer has confirmed the plan         │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  IMPLEMENTATION (implementer drives)                        │
│     Does the work, asks reviewers as needed                 │
│     Runs layer-fast.sh (layers 1-6) before asking for review│
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  REVIEW (reviewers + implementer)                           │
│     Every finding defaults to "fix it now"                  │
│     ► GATE 3: every reviewer's verdict is in                │
│       CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED / ESCALATED│
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  FULL VALIDATION                                            │
│     ► GATE 2: layer-all.sh, all 7 layers                    │
│       Fail → back to implementation                         │
│       (3 attempts for layers 1-6, 2 for layer 7)            │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  COMMIT                                                     │
│     Pre-commit hook checks the Gate-2 verdict; full record  │
│     in docs/devloop-outputs/<date>-<slug>/main.md           │
└─────────────────────────────────────────────────────────────┘
```

Gate numbers are names, not order. Review runs before the full pipeline ([ADR-0037](docs/decisions/adr-0037-devloop-process-tuning.md) D10): review almost always changes the tree, so running the full pipeline first meant running it, and bringing up the Layer-7 cluster, twice. Now it runs once, on the reviewed tree. Any failure after implementation returns to implementation and repeats the cycle from there.

**Team composition** (full mode):

| Teammate | Role |
|----------|------|
| Implementing Specialist | Domain expert who does the work |
| Security Reviewer | Vulnerabilities, crypto, auth |
| Test Reviewer | Coverage, test quality, regressions |
| Observability Reviewer | Metrics, logging, tracing, PII, SLOs |
| Code Quality Reviewer | Rust idioms, ADR compliance |
| DRY Reviewer | Cross-service duplication |
| Operations Reviewer | Deployment safety, rollback, runbooks |
| Semantic Guard Reviewer | Only when the diff touches a semantic check surface |

**Variants**:
- `--light`: small, contained changes get a 3-teammate panel (implementer, security, one context reviewer) and skip Gate 1.
- `--tier=light`: set by the story runner for tasks with no plan surface (no wire contract, new seam or multi-service span); skips only the Gate-1 planning round and keeps the full review panel.
- `--paired-with=<specialist>`: a second specialist collaborates during implementation and reviews at Gate 3.
- `--continue=<slug>`: reopens a completed devloop to address human review feedback.

**Review model**: every finding defaults to "fix it." A reviewer can accept a deferral only by showing that fixing later is genuinely cheaper; an accepted deferral is tracked in `docs/TODO.md` and makes that reviewer's verdict RESOLVED-DEFERRED rather than RESOLVED-FIXED. The DRY reviewer's suggestions to extract shared code are different: they go to `docs/TODO.md` as opportunities and never count as deferrals ([ADR-0019](docs/decisions/adr-0019-dry-reviewer.md)).

**Cross-boundary edits**: an implementer may edit any file, but an edit in another specialist's domain pulls that specialist into planning and review. Guarded Shared Areas, such as the wire protocol and auth policy, also pull in security ([ADR-0024](docs/decisions/adr-0024-agent-teams-workflow.md) §6).

**Why autonomous teammates?**
- **Teammates preserve full context**: they stay alive for the whole loop with their complete history
- **Minimal Lead context**: the Lead is idle during planning, implementation and review, and only wakes at gates
- **Natural collaboration**: reviewers discuss findings with the implementer directly, not through a coordinator
- **Graceful recovery**: checkpoint files in `docs/devloop-outputs/` let a loop resume after a session dies

---

## From Goal to Demo: User Stories

A devloop implements one task. A **user story** is how a user-facing goal, such as "participants can see each other", becomes a sequence of devloops.

1. **Plan** (`/user-story`): specialists decompose the goal into tasks, each cut so it delivers something demonstrable and never ships half of an invariant ([ADR-0037](docs/decisions/adr-0037-devloop-process-tuning.md) D3, D9). Requirements are stated as observable behaviour, and every story carries a manual test plan a human runs in a real browser. The plan is a story document in [`docs/user-stories/`](docs/user-stories/) plus a task manifest managed by `dt-story`.
2. **Run** (`scripts/workflow/run-story.sh`, [ADR-0035](docs/decisions/adr-0035-story-runner.md)): a deterministic bash runner, with no LLM in its control flow, walks the manifest in dependency order and starts one headless `/devloop` per task inside a devloop container. **Pass/fail is the runner's own `layer-all.sh` run, never the devloop's self-report**: a gate that says PASS while its work never happened has failed before, so the runner re-executes instead of trusting. An escalation stops the run and waits for a human decision.
3. **Close**: a human runs the story's manual test plan, then `/close-story` checks every task is done, runs the story-scope reflection that updates specialist INDEX files, and opens the story's PR.

This moves human involvement from every task boundary to the story boundary: plan a story, then come back to a finished story or to a short queue of genuine decisions.

---

## Evolution of the Devloop

The current Agent Teams devloop is the result of several iterations. Each version taught us something about working with AI agents.

### Version 1: Autonomous Orchestrator (Jan 9, 2026)

**Approach**: Claude acted as an autonomous orchestrator, running the entire devloop without human intervention between steps.

```
Human: "Implement feature X"
    ↓
Claude (autonomously):
    → Identifies specialist
    → Spawns specialist
    → Runs verification
    → Spawns reviewers
    → Does reflection
    → Reports results
```

**What went wrong**:
- **Skipped steps**: In trying to be helpful, Claude would skip verification or code review when the implementation "looked good"
- **Lost context**: As the conversation grew with verification output and reviewer feedback, Claude lost track of the overall state
- **Over-optimization**: Instead of following the documented process, Claude would improvise "shortcuts" that missed important checks
- **Runaway loops**: Without human checkpoints, Claude would sometimes iterate endlessly on minor issues

**Files at this version**: [.claude/workflows/development-loop.md @ 64d200e](https://github.com/nbuckles13/dark_tower/blob/64d200e/.claude/workflows/development-loop.md)

**Relevant ADR**: [ADR-0016: Development Loop](https://github.com/nbuckles13/dark_tower/blob/64d200e/docs/decisions/adr-0016-development-loop.md)

---

### Version 2: Step-Runner Architecture (Jan 17, 2026)

**Approach**: Introduced a three-tier hierarchy to contain context. Orchestrator reads minimal state machine; step-runners handle details; specialists do domain work.

```
┌─────────────────────────────────────┐
│  ORCHESTRATOR (Claude)              │
│  Reads: development-loop.md only    │
│  Does: State transitions            │
└────────────────┬────────────────────┘
                 ↓
┌─────────────────────────────────────┐
│  STEP-RUNNER (claude --print)       │
│  Reads: step-*.md for current step  │
│  Does: Execute one step completely  │
└────────────────┬────────────────────┘
                 ↓
┌─────────────────────────────────────┐
│  SPECIALIST (Task agent)            │
│  Reads: agent definition + knowledge│
│  Does: Domain expertise             │
└─────────────────────────────────────┘
```

**What went wrong**:
- **Still Claude as orchestrator**: The fundamental problem remained - Claude was deciding when and how to run steps
- **Context still accumulated**: Even with step-runners, the orchestrator conversation grew with each step's output
- **Inconsistent execution**: Claude would interpret the workflow docs differently across sessions, leading to non-reproducible behavior
- **Process drift**: Claude would "helpfully" deviate from the documented process, missing critical steps

**Files at this version**: [.claude/workflows/development-loop/ @ 0759656](https://github.com/nbuckles13/dark_tower/tree/0759656/.claude/workflows/development-loop)

**Relevant ADR**: [ADR-0021: Step-Runner Architecture](https://github.com/nbuckles13/dark_tower/blob/0759656/docs/decisions/adr-0021-step-runner-architecture.md)

---

### Version 2.5: User as Orchestrator (Jan 19, 2026)

**Approach**: Shifted orchestration to the human. User explicitly approves each step; Claude prepares prompts and spawns specialists but doesn't decide what to run next.

**Improvement**: Human approval points prevented skipped steps and runaway loops.

**Remaining problems**:
- **Workflow docs as source of truth**: Claude still had to read and interpret workflow documentation
- **Interpretation variance**: Different sessions interpreted the same docs differently
- **Context overhead**: Workflow docs loaded into context took space away from actual work

**Files at this version**: [.claude/workflows/development-loop.md @ 215f233](https://github.com/nbuckles13/dark_tower/blob/215f233/.claude/workflows/development-loop.md)

---

### Version 3: Skill-Based Devloop (Jan 21, 2026)

**Approach**: Each step became an executable skill. User invokes skills directly via `/devloop-*` commands. Skills are procedures, not docs to interpret.

```
User: /devloop-init "implement feature X"
    ↓
Claude: Executes skill procedure (creates output dir, matches principles)
    ↓
User: /devloop-implement
    ↓
Claude: Executes skill procedure (spawns specialist with context)
    ↓
User: /devloop-validate
    ↓
... and so on through /devloop-review, /devloop-reflect, /devloop-complete
```

**Available skills**:
| Step | Skill | Purpose |
|------|-------|---------|
| 0 | `/devloop-init` | Initialize: create output dir, match principles, preview specialist prompt |
| 0.5 | `/devloop-plan` | (Optional) Spawn specialist for exploration before implementation |
| 1 | `/devloop-implement` | Spawn implementing specialist with injected context |
| 2 | `/devloop-validate` | Run 7-layer verification |
| 3 | `/devloop-review` | Spawn 4 code reviewers in parallel |
| 4 | `/devloop-reflect` | Resume specialists for reflection |
| 5 | `/devloop-complete` | Mark loop complete, summarize results |

**Utilities**: `/devloop-status`, `/devloop-fix`, `/devloop-restore`

**What it solved**:
- **No interpretation needed**: Skills define exact steps, not prose to parse
- **User controls flow**: Each skill explicitly tells the user what to run next
- **Bounded context**: Each skill loads only what it needs
- **Reproducible**: Same skill invocation produces same behavior

**What remained problematic**:
- **Coordinator context rot**: The Lead's context still accumulated across steps as verification output, reviewer feedback, and state updates piled up
- **One-way communication**: Specialists reported back to the coordinator only - reviewers couldn't discuss findings with the implementer directly
- **Manual message passing**: The coordinator had to shuttle messages between specialists during review
- **Sequential bottleneck**: Each step waited for the previous one; no parallel planning/review collaboration

**Files at this version**: [.claude/skills/devloop/](.claude/skills/devloop/)

**Relevant ADR**: [ADR-0022: Skill-Based Development Loop](docs/decisions/adr-0022-skill-based-dev-loop.md)

---

### Version 4: Agent Teams Devloop (Feb 2026 - Current)

**Approach**: Replaced subagent-based orchestration with Claude Code Agent Teams. Specialists are spawned as autonomous teammates that communicate directly with each other. The Lead only acts at gates.

```
User: /devloop "implement feature X" --specialist=meeting-controller
    ↓
Lead: Spawns implementer + reviewers, records git state, goes idle
    ↓
Teammates drive autonomously:
    Planning → Implementation → Review → Full validation → Commit
    (Lead wakes only at gates)
```

**What changed from Version 3**:
- **Subagent invocations → Teammate spawning**: Specialists stay alive with full context for the whole loop instead of reporting back and being discarded
- **One-way reporting → Peer-to-peer messaging**: Reviewers discuss findings directly with the implementer, and with each other
- **Coordinator-driven → Teammate-driven**: Implementer drives pace, gets reviewer buy-in directly, asks questions as needed
- **Multi-step skills → Single command**: One `/devloop` invocation handles the entire workflow
- **Lead context accumulation → Lead mostly idle**: Lead only wakes at gate checks, avoiding the context rot that plagued earlier versions

Since February the loop has been tuned rather than redesigned ([ADR-0037](docs/decisions/adr-0037-devloop-process-tuning.md)): review moved ahead of the full pipeline, semantic-guard became conditional, the Gate-1 planning round became risk-tiered, and reflection moved from every devloop to once per story.

**Version 3 skills have been retired.** `/devloop` is the current and only implementation; `/devloop --continue` resumes a completed loop.

#### Containerized Execution (Preferred)

For maximum autonomy, devloops run inside isolated **podman containers** where Claude Code operates with `--dangerously-skip-permissions` (no permission prompts). The container boundary limits the blast radius to the current task's clone.

```
Host (WSL2)                              Containers (podman, per devloop)
┌──────────────────────┐                 ┌──────────────────────────┐
│ GitHub credentials   │                 │ Claude Code (autonomous) │
│ SSH keys             │  bind mount     │ Rust + Node toolchains   │
│ Git clone ───────────┼────────────────►│ /work (clone only)       │
│ devloop.sh wrapper   │                 │ PostgreSQL (shared net)  │
│ Cluster helper ◄─────┼── socket ───────┤ No GitHub credentials    │
│  └─ per-devloop Kind │                 │ No SSH keys              │
│     cluster          │                 └──────────────────────────┘
│ Push + PR creation   │
└──────────────────────┘
```

**Why containerize?**
- `--dangerously-skip-permissions` enables fully autonomous execution: no approval prompts interrupting the devloop
- The container can only reach the mounted clone, not SSH keys, GitHub credentials or other projects
- Claude's own credentials are mounted read-only; no API keys are needed
- Layer 7 needs a Kubernetes cluster, which the container cannot run itself. A **host-side helper** gives each devloop its own Kind cluster and exposes a narrow set of cluster verbs to the container ([ADR-0030](docs/decisions/adr-0030-host-side-cluster-helper.md))

**Workflow**:
```bash
# One command: creates clone, starts containers, drops into Claude
./infra/devloop/devloop.sh td-42-rate-limiting main

# Inside container:
claude> /devloop "implement rate limiting on GC endpoints"
# ... autonomous implementation, review, validation, commit ...
claude> /exit

# Back on host: devloop.sh offers to push and open a PR,
# or bring the commits into your branch with /absorb-devloop td-42-rate-limiting
```

**See**: [ADR-0025](docs/decisions/adr-0025-containerized-devloop.md) for the full design and security model.

---

### Version 5: Unattended Story Runner (Aug 2026 - Current)

**Approach**: Version 4 still needed a human at every task boundary: start each devloop, watch for derailment, merge the result, start the next. `run-story.sh` takes that job ([ADR-0035](docs/decisions/adr-0035-story-runner.md)). It is a program, not a prompt: a bash runner and the `dt-story` manifest tool decide everything from exit codes and manifest fields, and run one headless devloop per task.

**What it solved**:
- **Human time moves to the story boundary**: plan the story, run it, review the result and the escalation queue
- **Independent pass/fail**: the runner re-runs the full pipeline itself instead of trusting the devloop's verdict, because self-reported success has been wrong before
- **Faithful record**: with no model in the control flow, the runner's event log and cost ledger record what actually happened

**Deliberately not solved**: the runner is serial, one task at a time on one branch. Usage quota, not compute, limits throughput, and parallel branches would need parallel Kind clusters and would let reviewers review against diverging bases.

---

### The Root Cause (Versions 1-3)

The first three versions shared a common failure mode: **putting Claude in charge of process flow**.

When Claude orchestrates:
1. **Context accumulates** - Each step adds output to the conversation
2. **Helpfulness backfires** - Claude tries to optimize the process, skipping "unnecessary" steps
3. **Interpretation varies** - Same documentation, different behavior across sessions
4. **Complex beats simple** - When faced with a choice, Claude picks the "smarter" approach even when the simple one is correct

Version 3 (skill-based) fixed the interpretation and skipping problems by giving the human explicit control of flow. Version 4 (Agent Teams) fixed the remaining context accumulation problem by making the Lead mostly idle. Version 5 finished the arc: the orchestration between tasks stopped being instructions a model follows and became a program.

---

## Why This Works

### 1. Respects the Constraint
Every design decision works within AI's context limitations rather than fighting them. Each teammate has its own context window; one specialist's deep dive doesn't crowd out another's focus.

### 2. Specialists Stay Focused
Each specialist sees only its identity, a short navigation index for its domain, and the task. No cognitive overload.

### 3. Knowledge Points, It Doesn't Remember
Specialist indexes point to code and ADRs instead of copying facts out of them, so they cannot drift into contradicting the source, and a guard fails the pipeline when a pointer goes stale.

### 4. Guards Assume Failure
We don't trust any single AI interaction to be perfect. Seven validation layers, about fifty pattern guards, and a semantic reviewer catch inevitable oversights before they are committed.

### 5. Enforce, Don't Instruct
A rule that tooling can enforce (no panics, no secrets in logs, no unregistered tests) is enforced by tooling, not written into a prompt and hoped for. Instructions are kept for judgment that tooling cannot make.

### 6. Audit Trail
Every devloop produces an output file recording the plan, each reviewer's findings and verdict, and the validation results. When things go wrong, we can trace why.

### 7. Natural Collaboration
Agent Teams enables the back-and-forth that produces better outcomes: a Security reviewer raises a concern, the implementer proposes a fix, and the Test reviewer suggests how to verify it, all without a coordinator bottleneck.

---

## Results So Far

Using this methodology, Dark Tower has:

- **Authentication Controller**: OAuth 2.0 client credentials, user auth, JWT/JWKS, key rotation
- **Global Controller**: HTTP/3 API with meeting management and MC/MH registration
- **Meeting Controller**: WebTransport signaling with actor-based sessions, media routing and key distribution
- **Media Handler**: a keyless SFU that forwards end-to-end encrypted media it cannot read
- **Browser client**: TypeScript SDK (`sdk-core`/`sdk-svelte`) and Svelte web app; participants can join and hear each other, and video is in progress
- **Distributed tracing**: OpenTelemetry with W3C trace context from browser to MC to MH, verified by env-tests
- **90% test coverage** ([Codecov](https://app.codecov.io/gh/nbuckles13/dark_tower)), with dependencies audited on every lockfile change and weekly
- **~250,000 lines of Rust and ~53,000 lines of TypeScript/Svelte**, including tests
- **38 ADRs** and **15 specialists**, each with its own navigation index

All of it was written by AI following this structured methodology.

---

## Learn More

| Topic | Document |
|-------|----------|
| Project overview | [README.md](README.md) |
| Full architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Devloop procedure | [.claude/skills/devloop/SKILL.md](.claude/skills/devloop/SKILL.md) |
| Process tuning | [ADR-0037](docs/decisions/adr-0037-devloop-process-tuning.md) |
| User stories | [.claude/skills/user-story/SKILL.md](.claude/skills/user-story/SKILL.md), [docs/user-stories/](docs/user-stories/) |
| Story runner | [ADR-0035](docs/decisions/adr-0035-story-runner.md), `scripts/workflow/run-story.sh` |
| Validation pipeline | [ADR-0033](docs/decisions/adr-0033-polyglot-validation-pipeline.md), [docs/runbooks/devloop-validation.md](docs/runbooks/devloop-validation.md) |
| Containerized devloop | [infra/devloop/](infra/devloop/), [ADR-0025](docs/decisions/adr-0025-containerized-devloop.md), [ADR-0030](docs/decisions/adr-0030-host-side-cluster-helper.md) |
| Debate workflow | [.claude/skills/debate/SKILL.md](.claude/skills/debate/SKILL.md) |
| Specialist definitions | [.claude/agents/](.claude/agents/) |
| Specialist knowledge | [docs/specialist-knowledge/](docs/specialist-knowledge/), [ADR-0026](docs/decisions/adr-0026-knowledge-restructure.md) |
| Architecture decisions | [docs/decisions/](docs/decisions/) |
| Debate records | [docs/debates/](docs/debates/) |
| Devloop outputs | [docs/devloop-outputs/](docs/devloop-outputs/) |

---

*Dark Tower is an experiment in whether AI can build complex, secure, production-quality systems, not by pretending AI has unlimited capacity, but by designing processes that work within its constraints. Five iterations of development methodology have converged on a model where autonomous specialist teams collaborate directly, navigate the codebase from short indexes instead of accumulated memory, verify each other's work through structured review, and are driven task by task by a deterministic runner. That turns AI's limitations into a manageable engineering problem.*
