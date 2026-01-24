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

## The Core Idea: Human-Orchestrated AI Specialists

Instead of one AI doing everything autonomously, we use a **human-in-the-loop model** with **specialist agents** - each with deep expertise in a specific domain.

```
┌─────────────────────────────────────────────────────────────┐
│                    HUMAN ORCHESTRATOR                       │
│         (Invokes skills, approves results, decides)         │
└────────────────────────────┬────────────────────────────────┘
                             │
            Invokes: /dev-loop-init, /dev-loop-implement, etc.
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                 CLAUDE (SKILL EXECUTOR)                     │
│     (Follows skill procedures, spawns specialists)          │
└────────────────────────────┬────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐  ┌─────────────────┐  ┌─────────────────────┐
│   SERVICE     │  │     DOMAIN      │  │    CROSS-CUTTING    │
│  SPECIALISTS  │  │   SPECIALISTS   │  │     SPECIALISTS     │
├───────────────┤  ├─────────────────┤  ├─────────────────────┤
│ auth-controller│  │ database        │  │ security (mandatory)│
│ global-controller│ │ protocol       │  │ test (mandatory)    │
│ meeting-controller│ │ infrastructure│  │ observability       │
│ media-handler │  │ code-reviewer   │  │ operations          │
└───────────────┘  └─────────────────┘  └─────────────────────┘
```

**Why this structure?**
- **Human controls flow**: User invokes skills explicitly; Claude executes but doesn't decide what's next
- **Bounded context**: Each specialist only needs to understand their domain, not the entire system
- **Focused prompts**: We inject only relevant knowledge, not everything we know
- **Parallel execution**: Independent tasks run simultaneously, each with fresh context
- **Cross-cutting review**: Security and Test specialists catch what domain experts miss

---

## Multi-Agent Debates: Consensus-Driven Design

For features that cross service boundaries, we don't let one specialist decide. Instead, we run a **structured debate**:

1. **Propose** - Orchestrator identifies which specialists should participate
2. **Debate in rounds** - Each specialist proposes, critiques, and refines
3. **Score satisfaction** - Each specialist rates the current proposal (0-100%)
4. **Iterate** - Continue until 90%+ average satisfaction
5. **Document** - Create an Architecture Decision Record (ADR)

**Minimum debate size**: 5 agents (domain specialist + Security + Test + Observability + Operations)

**Example**: Designing a key rotation system required debate between:
- Auth Controller (implementation)
- Security (cryptographic requirements)
- Test (how to verify correctness)
- Observability (what to log/metric)
- Operations (deployment safety, failure modes)

The result was a more robust design than any single specialist would have produced.

---

## Dynamic Knowledge: AI That Learns

Each specialist maintains **knowledge files** that compound over time:

```
docs/specialist-knowledge/{specialist}/
├── patterns.md      # What works well
├── gotchas.md       # Pitfalls to avoid
└── integration.md   # How to work with other components
```

**How it works**:
1. After each implementation, a **reflection step** captures learnings
2. New patterns, gotchas, and integration notes are added to knowledge files
3. On future invocations, knowledge files are injected into the specialist's context
4. The specialist gets smarter with each task

**Example from `security/gotchas.md`**:
```markdown
## SecretBox requires owned data
- `SecretBox::new()` takes `Box<T>`, not `&T`
- Clone the data before wrapping: `SecretBox::new(Box::new(data.clone()))`
- This is intentional - prevents accidental exposure of borrowed references
```

This knowledge persists across sessions. A specialist that encountered this issue once won't make the same mistake again.

---

## Guard Pipeline: Trust but Verify

Even with focused specialists and targeted principles, AI will sometimes forget things. A specialist deep in implementation logic might overlook a logging statement that contains sensitive data. This isn't failure - it's expected.

**Guards exist because we know AI is fallible.** They're the safety net that catches what slips through.

Before any code is committed, it passes through a **7-layer verification pipeline**:

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

## Principles: Right Knowledge at the Right Time

We can't inject every best practice into every prompt - that would overwhelm the AI and dilute focus. Instead, we codify standards in **principles files** and inject only what's relevant:

```
docs/principles/
├── crypto.md    # EdDSA, bcrypt, no hardcoded secrets
├── jwt.md       # Token validation, claims, expiry
├── logging.md   # No PII, no secrets, structured format
├── queries.md   # Parameterized SQL, org_id filtering
├── errors.md    # No panics, Result types, generic messages
└── input.md     # Length limits, type validation
```

When a specialist is invoked, **only matched principles are injected** based on task keywords:

| Task contains | Principles injected |
|--------------|---------------------|
| "password", "encrypt", "key" | crypto, logging |
| "jwt", "token", "auth" | crypto, jwt, logging |
| "query", "database", "sql" | queries, logging |
| "handler", "endpoint", "api" | logging, errors, input |

This ensures specialists follow project-specific security and quality standards, not just generic best practices.

---

## The Development Loop

Putting it all together, here's how a feature gets implemented. The key insight: **humans approve each step before it runs**.

> **Evolution Note**: The dev-loop went through several iterations before arriving at the current design. See [Evolution of the Dev-Loop](#evolution-of-the-dev-loop) below for why earlier approaches failed.

```
┌─────────────────────────────────────────────────────────────┐
│  0. INITIATION                                              │
│     Human: "Let's work on task X"                           │
│     Claude: Prepares specialist prompt, shows principles    │
│     Human: Reviews and approves                             │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  1. IMPLEMENTATION                                          │
│     Claude: Spawns specialist with:                         │
│     - Specialist definition (role, responsibilities)        │
│     - Dynamic knowledge (patterns, gotchas, integration)    │
│     - Matched principles (crypto, logging, etc.)            │
│     Human: Reviews results, approves next step              │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  2. VERIFICATION (7 LAYERS)                                 │
│     Claude runs directly:                                   │
│     check → fmt → guards → tests → clippy → semantic        │
│     Human: Reviews failures, decides how to fix             │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  3. CODE REVIEW                                             │
│     Claude: Spawns 4 reviewers in parallel:                 │
│     - Security Specialist (vulnerabilities, crypto)         │
│     - Test Specialist (coverage, edge cases)                │
│     - Code Quality Reviewer (idioms, maintainability)       │
│     - DRY Reviewer (cross-service duplication)              │
│     Human: Reviews findings, decides what to fix            │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  4. REFLECTION                                              │
│     Claude: Captures learnings to dynamic knowledge files:  │
│     - New patterns discovered                               │
│     - Gotchas encountered                                   │
│     - Integration insights                                  │
│     Human: Reviews and completes loop                       │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  5. COMMIT                                                  │
│     With full audit trail in docs/dev-loop-outputs/         │
└─────────────────────────────────────────────────────────────┘
```

**Why human control of flow?**
- **Prevents skipped steps**: When Claude orchestrates, it "helpfully" skips steps that seem unnecessary
- **Prevents runaway loops**: Without human checkpoints, Claude iterates endlessly on minor issues
- **Consistent execution**: Skills define exact procedures; no interpretation variance between sessions
- **Bounded context**: Each skill invocation starts fresh; conversation doesn't accumulate indefinitely
- **Course correction**: Human can fix simple issues directly or adjust approach before expensive operations

See [Evolution of the Dev-Loop](#evolution-of-the-dev-loop) for why earlier autonomous approaches failed.

---

## Evolution of the Dev-Loop

The current skill-based dev-loop is the result of several iterations. Each version taught us something about working with AI agents.

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

### Version 3: Skill-Based Dev-Loop (Jan 21, 2026 - Current)

**Approach**: Each step is an executable skill. User invokes skills directly via `/dev-loop-*` commands. Skills are procedures, not docs to interpret.

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
... and so on
```

**Why this works**:
- **No interpretation needed**: Skills define exact steps, not prose to parse
- **User controls flow**: Each skill explicitly tells the user what to run next
- **Bounded context**: Each skill loads only what it needs
- **Same agent continuity**: Planning and implementation can use the same agent, preserving context
- **Reproducible**: Same skill invocation produces same behavior

**Current files**: [.claude/skills/dev-loop/](https://github.com/nbuckles13/dark_tower/tree/main/.claude/skills)

**Relevant ADR**: [ADR-0022: Skill-Based Development Loop](docs/decisions/adr-0022-skill-based-dev-loop.md)

---

### The Root Cause

All three failed approaches shared a common failure mode: **putting Claude in charge of process flow**.

When Claude orchestrates:
1. **Context accumulates** - Each step adds output to the conversation
2. **Helpfulness backfires** - Claude tries to optimize the process, skipping "unnecessary" steps
3. **Interpretation varies** - Same documentation, different behavior across sessions
4. **Complex beats simple** - When faced with a choice, Claude picks the "smarter" approach even when the simple one is correct

The fix wasn't better documentation or more guardrails - it was **removing Claude from the decision loop entirely**. The human decides what to run; Claude just executes.

---

## Why This Works

### 1. Respects the Constraint
Every design decision works within AI's context limitations rather than fighting them. Small, focused interactions beat sprawling conversations.

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

---

## Results So Far

Using this methodology, Dark Tower has achieved:

- **Authentication Controller**: Production-ready OAuth 2.0 implementation
- **83% test coverage** (targeting 95%) with 65+ security tests
- **Zero known security vulnerabilities** in implemented components
- **Consistent code quality** across 10,000+ lines of Rust
- **Clear architectural decisions** documented in 22 ADRs

The codebase handles authentication, JWT issuance, key rotation, rate limiting, and encryption - all generated by AI following this structured methodology.

---

## Learn More

| Topic | Document |
|-------|----------|
| Project overview | [README.md](README.md) |
| Full architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Development workflow | [.claude/skills/dev-loop/SKILL.md](.claude/skills/dev-loop/SKILL.md) |
| Specialist definitions | [.claude/agents/](.claude/agents/) |
| Principles | [docs/principles/](docs/principles/) |
| Architecture decisions | [docs/decisions/](docs/decisions/) |
| Debate records | [docs/debates/](docs/debates/) |
| Dev-loop outputs | [docs/dev-loop-outputs/](docs/dev-loop-outputs/) |
| Current progress | [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md) |

---

*Dark Tower is an experiment in whether AI can build complex, secure, production-quality systems - not by pretending AI has unlimited capacity, but by designing processes that work within its constraints. Early results are promising: focused specialists, targeted knowledge injection, and layered verification turn AI's limitations into a manageable engineering problem.*
