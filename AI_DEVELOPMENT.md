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

Instead of one AI doing everything, we use **specialist agent teams** - each agent with deep expertise in a specific domain, communicating directly with each other while a Lead coordinates at key gates.

```
┌─────────────────────────────────────────────────────────────┐
│                    HUMAN ORCHESTRATOR                        │
│              (Invokes skills, approves gates)                │
└────────────────────────────┬────────────────────────────────┘
                             │
                  Invokes: /dev-loop or /debate
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   LEAD (Delegate Mode)                       │
│   Spawns team, enforces gates, cannot implement directly    │
└────────────────────────────┬────────────────────────────────┘
                             │
        Spawns teammates with composed prompts
        (specialist identity + dynamic knowledge)
                             │
     ┌───────────────────────┼───────────────────────┐
     │                       │                       │
     ▼                       ▼                       ▼
┌──────────┐  ┌───────────────────┐  ┌─────────────────────┐
│IMPLEMENTER│  │  DOMAIN REVIEWERS │  │ CROSS-CUTTING       │
│           │  │                   │  │ REVIEWERS           │
│ One of:   │  │ code-reviewer     │  │ security (mandatory)│
│ auth-ctrl │  │ dry-reviewer      │  │ test (mandatory)    │
│ global-ctrl│ │                   │  │ operations          │
│ meeting-ctrl│└─────────┬─────────┘  └──────────┬──────────┘
│ media-hndlr│           │                       │
│ database  │            └───────────┬───────────┘
│ protocol  │                        │
│ infra     │          Direct peer-to-peer
│ observ.   │            messaging between
└─────┬─────┘              all teammates
      │                        │
      └────────────────────────┘
```

**Why this structure?**
- **Autonomous teammates**: Specialists drive their own work and communicate directly with each other - the Lead doesn't shuttle messages
- **Independent context**: Each teammate has its own context window, so one specialist's work doesn't crowd out another's
- **Lead stays minimal**: The Lead only spawns the team and intervenes at gates (plan approval, validation, final approval) - minimal context accumulation
- **Focused prompts**: Each specialist gets only their domain knowledge, relevant principles, and the specific task
- **Cross-cutting review**: Security, Test, and Operations specialists catch what domain experts miss

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

**Output**: ADR only - implementation is a separate `/dev-loop` invocation.

---

## Dynamic Knowledge: AI That Learns

Each specialist maintains a **knowledge directory** that compounds over time:

```
docs/specialist-knowledge/{specialist}/
├── patterns.md          # What works well (standard)
├── gotchas.md           # Pitfalls to avoid (standard)
├── integration.md       # How to work with other components (standard)
└── {domain-specific}.md # Specialist-created files for their domain
```

The three standard files are common across all specialists, but specialists can create additional domain-specific files during reflection. For example:

| Specialist | Extra Files | Purpose |
|------------|-------------|---------|
| Security | `approved-crypto.md` | Reviewed/approved cryptographic algorithms |
| Test | `coverage-targets.md` | Per-component coverage thresholds |
| Code Reviewer | `key-adrs.md` | ADRs to check code against |
| DRY Reviewer | `common-patterns.md` | Known shared code in `crates/common/` |

**How it works**:
1. After each implementation, a **reflection step** captures learnings
2. Specialists create or update any `.md` files in their knowledge directory
3. On future invocations, **all files** in the directory are injected into the specialist's context (via `{{inject-all:}}` directive)
4. The specialist gets smarter with each task - and can organize knowledge however makes sense for their domain

**Example from `security/gotchas.md`**:
```markdown
## SecretBox requires owned data
- `SecretBox::new()` takes `Box<T>`, not `&T`
- Clone the data before wrapping: `SecretBox::new(Box::new(data.clone()))`
- This is intentional - prevents accidental exposure of borrowed references
```

This knowledge persists across sessions. A specialist that encountered this issue once won't make the same mistake again. And because specialists can create new files freely, knowledge organization evolves naturally - a Security specialist might start tracking `approved-crypto.md` separately because it has a different review cadence than general patterns.

---

## Guard Pipeline: Trust but Verify

Even with focused specialists and targeted principles, AI will sometimes forget things. A specialist deep in implementation logic might overlook a logging statement that contains sensitive data. This isn't failure - it's expected.

**Guards exist because we know AI is fallible.** They're the safety net that catches what slips through.

Before any code is committed, it passes through a **validation pipeline** (see ADR-0024 for full specification):

| Layer | Check | Purpose |
|-------|-------|---------|
| 1 | `cargo check` | Basic compilation |
| 2 | `cargo fmt` | Consistent formatting |
| 3 | Simple guards | Pattern-based security checks |
| 4 | Unit tests | Isolated functionality |
| 5 | Integration tests | Cross-component behavior |
| 6 | `cargo clippy` | Linting and best practices |
| 7 | Semantic guards | AI-powered analysis |

### Simple Guards (Pattern Matching)

Fast, deterministic checks that catch common issues:

| Guard | What it catches |
|-------|-----------------|
| `no-hardcoded-secrets` | API keys, passwords in source code |
| `no-secrets-in-logs` | Sensitive data in log statements |
| `no-pii-in-logs` | Email, IP addresses in logs |
| `no-test-removal` | Accidental deletion of tests |
| `api-version-check` | API versioning compliance |
| `test-coverage` | Coverage regression detection |

### Semantic Guards (AI-Powered)

For issues that patterns can't catch, we use Claude to analyze code:

```bash
./scripts/guards/semantic/credential-leak.sh src/auth/token.rs
```

The AI reviews the code against our principles and returns:
- **SAFE** - No credential leak risks detected
- **UNSAFE** - Specific concerns with line numbers and explanations

This catches subtle issues like "this function logs a struct that contains a field that could contain sensitive data."

---

## Specialist Knowledge: Right Knowledge at the Right Time

We can't inject every best practice into every prompt — that would overwhelm the AI and dilute focus. Instead, each specialist accumulates **dynamic knowledge files** that are injected into their prompts at invocation:

```
docs/specialist-knowledge/{specialist}/
├── patterns.md      # What works well (successful approaches)
├── gotchas.md       # What to avoid (pitfalls encountered)
├── integration.md   # How to work with other components
└── {domain}.md      # Domain-specific knowledge (e.g., approved-crypto.md)
```

Knowledge compounds over time — after each implementation, the reflection phase captures new learnings. This ensures specialists follow project-specific standards that evolve with the codebase, not just generic best practices.

> **Historical note**: An earlier approach used static `docs/principles/` files (crypto.md, jwt.md, etc.) with keyword-based injection. This was superseded by specialist knowledge files which are more targeted and evolve organically. See ADR-0017 for the knowledge architecture.

---

## The Development Loop

Putting it all together, here's how a feature gets implemented. The key insight: **autonomous teammates drive the work; the Lead only enforces gates**.

> **Evolution Note**: The dev-loop went through several iterations before arriving at the current design. See [Evolution of the Dev-Loop](#evolution-of-the-dev-loop) below for why earlier approaches failed.

```
┌─────────────────────────────────────────────────────────────┐
│  SETUP (Lead)                                               │
│     Human: /dev-loop "implement feature X"               │
│     Lead: Spawns 7 teammates (1 implementer + 6 reviewers)  │
│     Lead: Composes prompts with specialist identity +        │
│           dynamic knowledge + task context                   │
│     Lead: Records git state, goes idle - teammates drive     │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  PLANNING (Teammates collaborate directly)                   │
│     Implementer: Drafts approach, messages reviewers         │
│     Reviewers: Provide input, raise concerns                 │
│     Implementer: Revises plan based on feedback              │
│     All reviewers confirm → proceed                          │
│     ► GATE 1: Lead checks all reviewers confirmed            │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  IMPLEMENTATION (Implementer drives)                         │
│     Implementer: Does the work, messages reviewers if needed │
│     Implementer: Signals "Ready for validation"              │
│     ► GATE 2: Lead runs validation pipeline                   │
│       Pass → proceed to review                               │
│       Fail → feedback to implementer, loop back (max 3)      │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  REVIEW (Reviewers + Implementer collaborate)                │
│     Reviewers: Examine code, discuss with each other         │
│     Implementer: Responds to feedback, fixes issues          │
│     Each reviewer: Sends verdict (APPROVED or BLOCKED)       │
│     ► GATE 3: Lead checks all verdicts                       │
│       All approved → proceed                                 │
│       Any blocked → implementer fixes, loop back (max 3)     │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  REFLECTION (All teammates)                                  │
│     Each teammate: Documents learnings to knowledge files    │
│     Lead: Writes summary to output file                      │
│     Team disbanded                                           │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  COMMIT                                                      │
│     With full audit trail in docs/dev-loop-outputs/          │
└─────────────────────────────────────────────────────────────┘
```

**Team composition** (every dev-loop):

| Teammate | Role |
|----------|------|
| Implementing Specialist | Domain expert who does the work |
| Security Reviewer | Vulnerabilities, crypto, zero-trust |
| Test Reviewer | Coverage, edge cases, quality gates |
| Code Quality Reviewer | Rust idioms, ADR compliance |
| DRY Reviewer | Cross-service duplication |
| Operations Reviewer | Deployment safety, runbooks |

**Why autonomous teammates?**
- **Teammates preserve full context**: They stay alive throughout the entire loop with their complete history
- **Minimal Lead context**: Lead is idle during planning, implementation, and review - only wakes at gates
- **Natural collaboration**: Reviewers discuss findings with the implementer directly, not through a coordinator
- **Self-organizing**: Implementer drives pace, gets reviewer buy-in directly, asks questions as needed
- **Graceful recovery**: Checkpoint files in `docs/dev-loop-outputs/` enable restart if session dies

**Code review blocking behavior**:
- Security, Test, Code Quality, Operations: ALL findings must be fixed
- DRY Reviewer: Only BLOCKER-level blocks; non-BLOCKERs documented as tech debt (see ADR-0019)

---

## Evolution of the Dev-Loop

The current Agent Teams dev-loop is the result of several iterations. Each version taught us something about working with AI agents.

### Version 1: Autonomous Orchestrator (Jan 9, 2026)

**Approach**: Claude acted as an autonomous orchestrator, running the entire dev-loop without human intervention between steps.

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

### Version 3: Skill-Based Dev-Loop (Jan 21, 2026)

**Approach**: Each step became an executable skill. User invokes skills directly via `/dev-loop-*` commands. Skills are procedures, not docs to interpret.

```
User: /dev-loop-init "implement feature X"
    ↓
Claude: Executes skill procedure (creates output dir, matches principles)
    ↓
User: /dev-loop-implement
    ↓
Claude: Executes skill procedure (spawns specialist with context)
    ↓
User: /dev-loop-validate
    ↓
... and so on through /dev-loop-review, /dev-loop-reflect, /dev-loop-complete
```

**Available skills**:
| Step | Skill | Purpose |
|------|-------|---------|
| 0 | `/dev-loop-init` | Initialize: create output dir, match principles, preview specialist prompt |
| 0.5 | `/dev-loop-plan` | (Optional) Spawn specialist for exploration before implementation |
| 1 | `/dev-loop-implement` | Spawn implementing specialist with injected context |
| 2 | `/dev-loop-validate` | Run 7-layer verification |
| 3 | `/dev-loop-review` | Spawn 4 code reviewers in parallel |
| 4 | `/dev-loop-reflect` | Resume specialists for reflection |
| 5 | `/dev-loop-complete` | Mark loop complete, summarize results |

**Utilities**: `/dev-loop-status`, `/dev-loop-fix`, `/dev-loop-restore`

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

**Files at this version**: [.claude/skills/dev-loop/](.claude/skills/dev-loop/)

**Relevant ADR**: [ADR-0022: Skill-Based Development Loop](docs/decisions/adr-0022-skill-based-dev-loop.md)

---

### Version 4: Agent Teams Dev-Loop (Feb 2026 - Current)

**Approach**: Replaced subagent-based orchestration with Claude Code Agent Teams. Specialists are spawned as autonomous teammates that communicate directly with each other. The Lead only intervenes at gates.

```
User: /dev-loop "implement feature X" --specialist=meeting-controller
    ↓
Lead: Spawns 7 teammates (1 implementer + 6 reviewers)
Lead: Records git state, goes idle
    ↓
Teammates drive autonomously:
    Planning → Implementation → Review → Reflection
    (Lead wakes only at gates to run validation and check verdicts)
```

**What changed from Version 3**:
- **Subagent invocations → Teammate spawning**: Specialists stay alive with full context throughout the entire loop instead of reporting back and being discarded
- **One-way reporting → Peer-to-peer messaging**: Reviewers discuss findings directly with the implementer, and with each other
- **Coordinator-driven → Teammate-driven**: Implementer drives pace, gets reviewer buy-in directly, asks questions as needed
- **Multi-step skills → Single command**: One `/dev-loop` invocation handles the entire workflow
- **Lead context accumulation → Lead mostly idle**: Lead only wakes at 3 brief gate checks, avoiding the context rot that plagued earlier versions

**Why this works**:
- **Teammates preserve context**: Each specialist keeps their full history for the entire loop
- **Natural collaboration**: Reviewers and implementer have real discussions, not coordinator-mediated exchanges
- **Minimal Lead overhead**: ~4 brief gate checks vs continuous orchestration
- **Graceful recovery**: Checkpoint files enable restart via `/dev-loop-restore` if session dies

**Current files**: [.claude/skills/dev-loop/SKILL.md](.claude/skills/dev-loop/SKILL.md)

**Version 3 skills have been retired.** The Agent Teams workflow (`/dev-loop`) is the current and only implementation.

#### Containerized Execution (Preferred)

For maximum autonomy, dev-loops run inside isolated **podman containers** where Claude Code operates with `--dangerously-skip-permissions` (all permission prompts disabled). The container boundary limits blast radius to the current task's worktree.

```
Host (WSL2)                              Container (podman pod)
┌──────────────────────┐                 ┌──────────────────────────┐
│ GitHub credentials   │                 │ Claude Code (autonomous) │
│ SSH keys             │  bind mount     │ Rust toolchain + tools   │
│ Git worktree ────────┼────────────────►│ /work (worktree only)    │
│ devloop.sh wrapper   │                 │ PostgreSQL sidecar       │
│                      │                 │                          │
│ Push + PR creation ◄─┼── .devloop-pr  │ No GitHub credentials    │
│ (host credentials)   │    .json        │ No SSH keys              │
└──────────────────────┘                 └──────────────────────────┘
```

**Why containerize?**
- `--dangerously-skip-permissions` enables fully autonomous Claude Code execution — no approval prompts interrupting the dev-loop
- Container isolation means Claude can only access the mounted worktree, not SSH keys, GitHub credentials, or other projects
- The only credential exposed is `ANTHROPIC_API_KEY` (unavoidable — Claude needs it to function)
- PR descriptions are written by Claude (which has the full task context) to a `.devloop-pr.json` file, then the host-side wrapper creates the actual PR using the host's GitHub credentials

**Workflow**:
```bash
# One command: creates worktree, starts pod, drops into Claude
./infra/devloop/devloop.sh td-42-rate-limiting

# Inside container:
claude> /dev-loop "implement rate limiting on GC endpoints"
# ... autonomous implementation, review, commit ...
claude> /exit

# Back on host — wrapper offers to push and create PR
# using .devloop-pr.json that Claude wrote
```

**See**: [ADR-0025](docs/decisions/adr-0025-containerized-devloop.md) for the full design and security model.

---

### The Root Cause (Versions 1-3)

The first three versions shared a common failure mode: **putting Claude in charge of process flow**.

When Claude orchestrates:
1. **Context accumulates** - Each step adds output to the conversation
2. **Helpfulness backfires** - Claude tries to optimize the process, skipping "unnecessary" steps
3. **Interpretation varies** - Same documentation, different behavior across sessions
4. **Complex beats simple** - When faced with a choice, Claude picks the "smarter" approach even when the simple one is correct

Version 3 (skill-based) fixed the interpretation and skipping problems by giving the human explicit control of flow. Version 4 (Agent Teams) fixed the remaining context accumulation problem by making the Lead mostly idle - teammates drive themselves, and the Lead's context stays small.

---

## Why This Works

### 1. Respects the Constraint
Every design decision works within AI's context limitations rather than fighting them. Each teammate has its own independent context window - one specialist's deep dive doesn't crowd out another's focus.

### 2. Specialists Stay Focused
Each specialist only sees what they need: their domain knowledge, relevant principles, and the specific task. No cognitive overload.

### 3. Knowledge Compounds Without Bloating
Dynamic knowledge files grow over time, but we inject only what's relevant to each task. The system learns without each prompt getting larger.

### 4. Guards Assume Failure
We don't trust any single AI interaction to be perfect. Seven verification layers catch inevitable oversights before they reach production.

### 5. Explicit Over Implicit
Principles aren't generic best practices the AI "should know" - they're our specific standards, documented and injected. The AI follows *your* rules, not its training defaults.

### 6. Audit Trail
Every implementation produces an output file documenting what was done, what was verified, and what was learned. When things go wrong, we can trace why.

### 7. Natural Collaboration
Agent Teams enables the kind of back-and-forth discussion that produces better outcomes - a Security reviewer can raise a concern, the implementer can propose a fix, and the Test reviewer can suggest how to verify it, all without a coordinator bottleneck.

---

## Results So Far

Using this methodology, Dark Tower has achieved:

- **Authentication Controller**: Production-ready OAuth 2.0 implementation
- **Global Controller**: HTTP/3 API gateway with meeting management and MC/MH registration
- **Meeting Controller**: WebTransport signaling with actor-based session management, Prometheus metrics
- **83% test coverage** (targeting 95%) with 65+ security tests
- **Zero known security vulnerabilities** in implemented components
- **Consistent code quality** across 15,000+ lines of Rust
- **Clear architectural decisions** documented in 23+ ADRs
- **13 specialist knowledge bases** with accumulated patterns and gotchas

The codebase handles authentication, JWT issuance, key rotation, rate limiting, encryption, meeting lifecycle, session binding, and observability - all generated by AI following this structured methodology.

---

## Learn More

| Topic | Document |
|-------|----------|
| Project overview | [README.md](README.md) |
| Full architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Dev-loop overview | [.claude/skills/dev-loop/SKILL.md](.claude/skills/dev-loop/SKILL.md) |
| Containerized dev-loop | [infra/devloop/](infra/devloop/) and [ADR-0025](docs/decisions/adr-0025-containerized-devloop.md) |
| Debate workflow | [.claude/skills/debate/SKILL.md](.claude/skills/debate/SKILL.md) |
| Specialist definitions | [.claude/agents/](.claude/agents/) |
| Specialist knowledge | [docs/specialist-knowledge/](docs/specialist-knowledge/) |
| Architecture decisions | [docs/decisions/](docs/decisions/) |
| Debate records | [docs/debates/](docs/debates/) |
| Dev-loop outputs | [docs/dev-loop-outputs/](docs/dev-loop-outputs/) |
| Current progress | [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md) |

---

*Dark Tower is an experiment in whether AI can build complex, secure, production-quality systems - not by pretending AI has unlimited capacity, but by designing processes that work within its constraints. Four iterations of development methodology have converged on a model where autonomous specialist teams collaborate directly, accumulate knowledge over time, and verify each other's work through structured review - turning AI's limitations into a manageable engineering problem.*
