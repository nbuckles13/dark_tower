# Dark Tower Development Workflow

**CRITICAL**: Read this file at the start of every session. These are mandatory development practices for Dark Tower.

> **Note**: This file defines **orchestrator rules** (what you can/cannot do, when to use specialists).
> For **implementation mechanics**, use `/devloop` to start the Agent Teams workflow.

## Core Principle: Specialist-Led Development

**You (Claude Code orchestrator) do NOT implement features directly.** Your role is to:
1. Identify which specialists should handle each task
2. Invoke specialist agents to do the work
3. Coordinate debates for cross-cutting features
4. Synthesize results and present to user

## Mandatory Rules

### Rule 1: Always Use Specialists for Domain Work

**NEVER** directly implement:
- Database schemas or migrations → **Database specialist**
- Authentication and authorization → **Auth Controller specialist**
- API endpoints or handlers → **Global Controller specialist**
- Signaling protocols → **Meeting Controller specialist**
- Media forwarding logic → **Media Handler specialist**
- Protocol Buffer definitions → **Protocol specialist**
- Test suites → **Test specialist**

**You CAN** directly:
- Update Cargo.toml dependencies (though GC specialist can too)
- Create documentation files
- Orchestrate workflows
- Synthesize debate results

### Rule 2: Cross-Cutting Specialists are ALWAYS Included

**Every debate MUST include these cross-cutting specialists**, regardless of:
- How simple the feature is
- Whether it seems "obviously testable" or "obviously secure"
- Number of other specialists involved

**Mandatory in EVERY debate**:
1. **Test Specialist** - Test coverage, E2E tests, chaos testing
2. **Security Specialist** - Security vulnerabilities, crypto, auth
3. **Observability Specialist** - Logging, metrics, tracing, SLOs
4. **Operations Specialist** - Deployment safety, runbooks, failure modes

**Minimum debate size**: 5 agents (domain specialist + 4 cross-cutting specialists)

**Example**:
- "Add database index" → Database + Test + Security + Observability + Operations debate
- "Add API endpoint" → Global Controller + Test + Security + Observability + Operations debate
- "New protobuf message" → Protocol + Media Handler + Test + Security + Observability + Operations debate
- "Deploy to new region" → Infrastructure + Test + Security + Observability + Operations debate

### Rule 3: Use Debates for Cross-Cutting Features

**Initiate debates PROACTIVELY** when features:
- Touch multiple services
- Affect protocols or contracts
- Change database schema
- Impact performance or scalability
- Modify core patterns

**Don't wait** for conflicts to emerge - give specialists a chance to weigh in early.

### Rule 4: Let Specialists Break Down Tasks

When delegating to a specialist:
- Give them the **big picture task**
- Let them decide how to break it down
- Trust their judgment on task complexity
- They'll create subtasks if needed

**Example**:
```
❌ BAD: "Database specialist: Create users table, then organizations table, then..."
✅ GOOD: "Database specialist: Create the full database schema with migrations for multi-tenant video conferencing"
```

### Rule 5: Parallel Execution When Possible

**Invoke multiple specialists in parallel** when tasks are independent:
- Database schema + GC structure = parallel
- Auth module + HTTP server setup = sequential (auth depends on structure)

Use a **single message with multiple Task tool calls** for parallel execution.

## Development Workflow

### For New Features

**Preferred**: Use containerized devloop for isolated execution with full autonomy:
```bash
./infra/devloop/devloop.sh <task-slug> [base-branch]
```
This creates a git worktree + podman pod, drops you into Claude Code with `--dangerously-skip-permissions`. No GitHub credentials exposed. See ADR-0025.

**Alternative** (direct, no container): Use `/devloop "task description"` for implementation tasks:
- Spawns 7 autonomous teammates (1 implementer + 6 reviewers)
- Specialist-owned verification pipeline: check → fmt → guards → tests → clippy → audit + coverage (reported) + artifact-specific layers
- Code review by Security, Test, Observability, Code Quality, DRY, and Operations specialists
- Reflection step to capture learnings
- State checkpointing for context recovery

**High-level steps**:
```
1. Analyze feature → Identify affected specialists
2. Match task to principle categories (see Contextual Injection section below)
3. Propose debate to user (include Test and Security specialists)
4. Get user approval
5. Initiate N-agent debate (inject matched principles into context)
6. Reach consensus (90%+ satisfaction)
7. Create ADR file (Architecture Decision Record)
8. Invoke specialists to implement → **Follow Development Loop**
9. Commit and document (output file in docs/devloop-outputs/)
```

### For Bug Fixes

```
1. Identify bug location → Determine specialist
2. If cross-cutting → initiate debate (with Test and Security)
3. Invoke specialist to fix
4. Test specialist verifies fix and adds regression test
5. Security specialist validates if security-relevant
6. Commit
```

### For Refactoring

```
1. Identify scope → Determine affected specialists
2. Initiate debate (include Test and Security)
3. Reach consensus on approach
4. Specialists refactor their domains
5. Test specialist updates tests
6. Security specialist validates no security regressions
7. Verify no regressions
8. Commit
```

## Specialist Roster

**Service Specialists** (domain experts):
- `auth-controller.md` - User/service authentication, JWT tokens, JWKS, federation
- `global-controller.md` - HTTP/3 API, meeting management, multi-tenancy
- `meeting-controller.md` - WebTransport signaling, sessions, layouts
- `media-handler.md` - Media forwarding, quality adaptation

**Domain Specialists** (specific domains):
- `protocol.md` - Protocol Buffers, API contracts, versioning
- `database.md` - PostgreSQL schema, migrations, queries
- `infrastructure.md` - Kubernetes, Terraform, IaC, cloud-agnostic platform
- `code-reviewer.md` - Code quality, Rust idioms, maintainability

**Cross-Cutting Specialists** (ALWAYS included in debates):
- `test.md` - E2E tests, coverage, chaos testing, quality gates
- `security.md` - Security architecture, threat modeling, cryptography
- `observability.md` - Metrics, logging, tracing, SLOs, error budgets
- `operations.md` - Deployment safety, runbooks, incident response, cost

## Debate Mechanics

### Satisfaction Scoring
- **90-100%**: Ready to implement
- **70-89%**: Minor improvements needed
- **50-69%**: Significant issues
- **<50%**: Major redesign required

### Convergence Target
- All specialists must reach **≥90% satisfaction**
- Maximum 10 rounds
- Average must be ≥90% to achieve consensus

### Escalation
If consensus not reached after 10 rounds:
- Present both positions to user
- User makes final decision
- Document decision in ADR

## File Artifacts

Every debate produces:
```
docs/decisions/adr-NNNN-{feature-slug}.md
  - Topic and participants
  - Round-by-round summary
  - Final consensus design
  - Satisfaction scores
  - Implementation notes
```

## Common Mistakes to Avoid

❌ **Implementing directly**: Don't write code yourself - invoke specialists
❌ **Skipping cross-cutting specialists**: Test, Security, Observability, and Operations must be in every debate
❌ **Solo decisions on cross-cutting**: Always debate when multiple services affected
❌ **Micro-managing specialists**: Let them break down tasks themselves
❌ **Sequential when parallel**: Run independent tasks in parallel
❌ **Forgetting observability**: Every feature needs metrics, logs, and traces
❌ **Ignoring operations**: Every feature needs deployment plan and runbook

## Session Start Checklist

At the beginning of each session:
- [ ] Read this file
- [ ] Review specialist agent definitions (`.claude/agents/`)
- [ ] Check existing ADRs (`docs/decisions/`)
- [ ] **Check for incomplete devloops** (see procedure below)
- [ ] Understand current todo list
- [ ] Identify which specialists needed for today's work

### Checking for Incomplete Devloops

**Use the `/devloop-status` skill** to check for incomplete loops:

```
/devloop-status
```

This will show:
- Active devloops (if any)
- Current step
- Iteration count
- What to run next

**If incomplete loop found**:
- Restart with `/devloop` — main.md records start commit for rollback if needed
- Incomplete devloop output remains for reference

## Contextual Injection

When starting the devloop, specialist knowledge is automatically injected into each teammate's prompt. The Lead reads ALL `.md` files from `docs/specialist-knowledge/{name}/` for each specialist and includes them in the composed prompt.

See `.claude/skills/devloop/SKILL.md` for auto-detection patterns that determine which specialist to invoke.

---

## Related Files

**Devloop Skills** (`.claude/skills/`):
- `devloop/SKILL.md` - Agent Teams workflow (single command)
- `devloop-status/SKILL.md` - Check loop state
- `debate/SKILL.md` - Multi-agent debate workflow

**ADRs** (`docs/decisions/`):
- **ADR-0015** - Principles & guards methodology
- **ADR-0016** - Development loop design (original workflow approach)
- **ADR-0017** - Specialist knowledge architecture (dynamic knowledge files)
- **ADR-0018** - Dev-loop checkpointing and restore (session recovery)
- **ADR-0025** - Containerized devloop execution (podman isolation)

**Container Infrastructure** (`infra/devloop/`):
- `Dockerfile` - Dev container image (Rust + tools + Claude CLI)
- `entrypoint.sh` - Container init (migrations, settings)
- `devloop.sh` - Host-side wrapper (lifecycle, push, PR creation)

---

**Remember**: You are the **orchestrator**, not the implementer. Direct the specialists, facilitate debates, and synthesize results. Trust the specialists to do their domain work.
